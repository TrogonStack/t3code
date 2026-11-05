//! Spawns and supervises the T3 server the way apps/desktop does
//! (DesktopBackendConfiguration.ts and DesktopBackendManager.ts): a JSON
//! bootstrap line on fd 3, readiness on /.well-known/t3/environment, SIGTERM
//! then SIGKILL on stop.

use crate::auth;
use crate::config::{Config, ServerCommand};
use crate::connection::{self, Backoff};
use crate::status::{Status, StatusSender, publish};
use serde::{Deserialize, Serialize};
use std::io;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::time::{Instant, sleep, timeout};

const READINESS_PATH: &str = "/.well-known/t3/environment";
// Large state databases can take over a minute to become ready.
const READINESS_TIMEOUT: Duration = Duration::from_secs(3 * 60);
const READINESS_INTERVAL: Duration = Duration::from_millis(100);
const READINESS_REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
// Matches the service launcher's grace so the server can stop provider sessions.
const TERMINATE_GRACE: Duration = Duration::from_secs(5);
// A run this long counts as healthy, so the next failure restarts quickly again.
const HEALTHY_UPTIME: Duration = Duration::from_secs(30);
const BOOTSTRAP_FD: i32 = 3;
// packages/contracts/src/desktopBootstrap.ts: refusals a restart cannot fix.
const EXIT_MANAGED_POLICY: i32 = 77;
const EXIT_STATE_DIR_OWNED: i32 = 78;

// The server takes these from the bootstrap payload, never from the launcher's shell.
const ENV_REMOVED_FROM_SERVER: &[&str] = &[
    "T3CODE_PORT",
    "T3CODE_MODE",
    "T3CODE_NO_BROWSER",
    "T3CODE_HOST",
    "T3CODE_DESKTOP_WS_URL",
    "T3CODE_DESKTOP_LAN_HOST",
    "T3CODE_DESKTOP_HTTPS_ENDPOINTS",
    "T3CODE_TAILSCALE_SERVE",
    "T3CODE_TAILSCALE_SERVE_PORT",
];

/// Shape of packages/contracts/src/desktopBootstrap.ts. Optional telemetry and
/// browser descriptors are omitted: this client provides neither.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Bootstrap<'a> {
    mode: &'static str,
    no_browser: bool,
    port: u16,
    t3_home: &'a str,
    host: &'static str,
    desktop_bootstrap_token: String,
    desktop_bootstrap_secret: &'a str,
    // The schema requires both even with Tailscale Serve off.
    tailscale_serve_enabled: bool,
    tailscale_serve_port: u16,
}

fn bootstrap_line(config: &Config, secret: &str) -> String {
    let bootstrap = Bootstrap {
        mode: "desktop",
        no_browser: true,
        port: config.port,
        t3_home: &config.t3_home,
        host: "127.0.0.1",
        desktop_bootstrap_token: auth::bootstrap_token(secret, auth::now_ms()),
        desktop_bootstrap_secret: secret,
        tailscale_serve_enabled: false,
        tailscale_serve_port: 443,
    };
    let mut line = serde_json::to_string(&bootstrap).expect("bootstrap serializes");
    line.push('\n');
    line
}

/// Pipe pre-filled with `payload`, write end already closed so the server sees EOF.
#[cfg(unix)]
fn bootstrap_pipe(payload: &[u8]) -> io::Result<std::os::fd::OwnedFd> {
    use std::io::Write;
    use std::os::fd::{FromRawFd, OwnedFd};

    let mut fds = [0i32; 2];
    // SAFETY: `fds` holds two ints, as pipe(2) requires.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both descriptors were just created and are owned by nobody else.
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    for fd in [&read, &write] {
        use std::os::fd::AsRawFd;
        // Only the descriptor dup'd onto fd 3 may survive exec.
        // SAFETY: valid, owned descriptor.
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    // The payload is far below the pipe capacity, so this never blocks.
    std::fs::File::from(write).write_all(payload)?;
    Ok(read)
}

#[cfg(unix)]
fn spawn_server(config: &Config, secret: &str) -> io::Result<Child> {
    use std::os::fd::AsRawFd;

    let ServerCommand { program, args } = &config.command;
    let pipe = bootstrap_pipe(bootstrap_line(config, secret).as_bytes())?;
    let source = pipe.as_raw_fd();
    let mut command = Command::new(program);
    command
        .args(args)
        .args(["--bootstrap-fd", &BOOTSTRAP_FD.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Last resort if this task is dropped; the explicit stop path is `terminate`.
        .kill_on_drop(true);
    for name in ENV_REMOVED_FROM_SERVER {
        command.env_remove(name);
    }
    // SAFETY: the closure only calls async-signal-safe fcntl/dup2.
    unsafe {
        command.pre_exec(move || {
            if source == BOOTSTRAP_FD {
                let flags = libc::fcntl(source, libc::F_GETFD);
                if flags < 0 || libc::fcntl(source, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(io::Error::last_os_error());
                }
            } else if libc::dup2(source, BOOTSTRAP_FD) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn();
    drop(pipe);
    child
}

#[cfg(not(unix))]
fn spawn_server(_config: &Config, _secret: &str) -> io::Result<Child> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "starting the server needs a unix fd 3 pipe",
    ))
}

fn forward_output(stream: impl AsyncRead + Unpin + Send + 'static) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            eprintln!("[server] {line}");
        }
    });
}

#[derive(Deserialize)]
struct Descriptor {
    label: String,
}

/// Polls until the descriptor answers, returning the environment label.
async fn wait_until_ready(client: &reqwest::Client, http_base: &str) -> Result<String, String> {
    let url = format!("{http_base}{READINESS_PATH}");
    let deadline = Instant::now() + READINESS_TIMEOUT;
    loop {
        let attempt = client
            .get(&url)
            .timeout(READINESS_REQUEST_TIMEOUT)
            .send()
            .await;
        if let Ok(response) = attempt
            && response.status().is_success()
            && let Ok(descriptor) = response.json::<Descriptor>().await
        {
            return Ok(descriptor.label);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Server did not become ready within {}s",
                READINESS_TIMEOUT.as_secs()
            ));
        }
        sleep(READINESS_INTERVAL).await;
    }
}

/// SIGTERM the child this process spawned, then SIGKILL after the grace period.
async fn terminate(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: `pid` is the id of the child we spawned and have not yet reaped.
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    }
    if timeout(TERMINATE_GRACE, child.wait()).await.is_err() {
        let _ = child.kill().await;
    }
}

async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|requested| *requested).await;
}

/// Waits out `delay`; true when shutdown arrived first.
async fn sleep_or_shutdown(delay: Duration, shutdown: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = sleep(delay) => false,
        _ = shutdown_requested(shutdown) => true,
    }
}

async fn session(config: &Config, secret: &str, status: &StatusSender) -> String {
    let client = auth::http_client();
    match wait_until_ready(&client, &config.http_base()).await {
        Ok(label) => {
            connection::run(config, secret, &label, status).await;
            unreachable_session()
        }
        Err(message) => message,
    }
}

// `connection::run` only ends by cancellation.
fn unreachable_session() -> String {
    "connection ended".to_owned()
}

/// Runs the server and its connection until shutdown, restarting after crashes.
pub async fn supervise(config: Config, status: StatusSender, mut shutdown: watch::Receiver<bool>) {
    let mut backoff = Backoff::new();
    loop {
        publish(&status, Status::Starting);
        let secret = match auth::generate_secret() {
            Ok(secret) => secret,
            Err(message) => return publish(&status, Status::Failed(message)),
        };
        let started = Instant::now();
        let mut child = match spawn_server(&config, &secret) {
            Ok(child) => child,
            Err(error) => {
                publish(
                    &status,
                    Status::Failed(format!("Could not start the server: {error}")),
                );
                if sleep_or_shutdown(backoff.next_delay(), &mut shutdown).await {
                    return;
                }
                continue;
            }
        };
        if let Some(stdout) = child.stdout.take() {
            forward_output(stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            forward_output(stderr);
        }

        let failure = tokio::select! {
            _ = shutdown_requested(&mut shutdown) => {
                terminate(&mut child).await;
                return;
            }
            exited = child.wait() => {
                let code = exited.ok().and_then(|exit| exit.code());
                match code {
                    Some(EXIT_STATE_DIR_OWNED) => {
                        publish(&status, Status::Failed(
                            "Another T3 Code server already owns this T3 home. Stop it or use a different home.".to_owned(),
                        ));
                        shutdown_requested(&mut shutdown).await;
                        return;
                    }
                    Some(EXIT_MANAGED_POLICY) => {
                        publish(&status, Status::Failed("The server was refused by managed policy.".to_owned()));
                        shutdown_requested(&mut shutdown).await;
                        return;
                    }
                    Some(code) => format!("Server exited with code {code}"),
                    None => "Server exited unexpectedly".to_owned(),
                }
            }
            message = session(&config, &secret, &status) => {
                terminate(&mut child).await;
                message
            }
        };
        if started.elapsed() >= HEALTHY_UPTIME {
            backoff.reset();
        }
        publish(&status, Status::Failed(format!("{failure}. Restarting...")));
        if sleep_or_shutdown(backoff.next_delay(), &mut shutdown).await {
            return;
        }
    }
}

/// Handle for stopping the supervisor from the UI thread at quit.
pub struct Supervisor {
    shutdown: watch::Sender<bool>,
    done: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl Supervisor {
    pub fn start(runtime: &tokio::runtime::Handle, config: Config, status: StatusSender) -> Self {
        let (shutdown, shutdown_rx) = watch::channel(false);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        runtime.spawn(async move {
            supervise(config, status, shutdown_rx).await;
            let _ = done_tx.send(());
        });
        Self {
            shutdown,
            done: Mutex::new(Some(done_rx)),
        }
    }

    /// Blocks until the server we spawned is gone (bounded by the grace period).
    pub fn stop(&self) {
        let _ = self.shutdown.send(true);
        let done = self.done.lock().ok().and_then(|mut done| done.take());
        if let Some(done) = done {
            let _ = done.recv_timeout(TERMINATE_GRACE + Duration::from_secs(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            port: 13773,
            t3_home: "/work/.t3".to_owned(),
            command: ServerCommand {
                program: "node".to_owned(),
                args: vec!["/work/apps/server/src/bin.ts".to_owned()],
            },
        }
    }

    #[test]
    fn bootstrap_line_matches_the_contract() {
        let line = bootstrap_line(&config(), "secret");
        assert!(line.ends_with('\n'));
        let value: serde_json::Value = serde_json::from_str(line.trim_end()).expect("json");
        assert_eq!(value["mode"], "desktop");
        assert_eq!(value["noBrowser"], true);
        assert_eq!(value["port"], 13773);
        assert_eq!(value["host"], "127.0.0.1");
        assert_eq!(value["t3Home"], "/work/.t3");
        assert_eq!(value["desktopBootstrapSecret"], "secret");
        assert_eq!(value["tailscaleServeEnabled"], false);
        assert_eq!(value["tailscaleServePort"], 443);
        assert_eq!(
            value["desktopBootstrapToken"].as_str().map(str::len),
            Some(64)
        );
        for omitted in [
            "desktopTelemetryFd",
            "desktopBrowserFd",
            "shellEnvironmentPrepared",
        ] {
            assert!(value.get(omitted).is_none(), "{omitted} must be omitted");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_reads_the_bootstrap_line_from_fd_3() {
        let mut config = config();
        config.command = ServerCommand {
            program: "/bin/sh".to_owned(),
            // `--bootstrap-fd 3` becomes $1 and $2 of the script.
            args: vec![
                "-c".to_owned(),
                "head -n 1 <&3; exit 7".to_owned(),
                "sh".to_owned(),
            ],
        };
        let mut child = spawn_server(&config, "secret").expect("spawn");
        let output = {
            use tokio::io::AsyncReadExt;
            let mut text = String::new();
            child
                .stdout
                .take()
                .expect("stdout")
                .read_to_string(&mut text)
                .await
                .expect("read");
            text
        };
        let exit = child.wait().await.expect("wait");
        assert_eq!(exit.code(), Some(7));
        let value: serde_json::Value = serde_json::from_str(output.trim()).expect("json line");
        assert_eq!(value["desktopBootstrapSecret"], "secret");
    }
}
