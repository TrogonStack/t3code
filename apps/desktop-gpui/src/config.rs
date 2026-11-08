//! Launch settings handed over by the dev runner (or, later, a packaged launcher).

pub const ENV_PORT: &str = "T3CODE_GPUI_SERVER_PORT";
pub const ENV_HOME: &str = "T3CODE_GPUI_HOME";
pub const ENV_COMMAND: &str = "T3CODE_GPUI_SERVER_COMMAND";
pub const ENV_ENTRY: &str = "T3CODE_GPUI_SERVER_ENTRY";
pub const ENV_ATTACH_URL: &str = "T3CODE_GPUI_SERVER_URL";
pub const ENV_PAIRING_TOKEN: &str = "T3CODE_GPUI_PAIRING_TOKEN";

/// A server someone else runs (a dev server, `npx t3`, a remote host), reached
/// with the session saved from an earlier pairing, or a pairing token for the first time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attach {
    pub http_base: String,
    /// A bare token or the `/pair#token=...` link; absent once the app is paired.
    pub pairing_token: Option<String>,
}

impl Attach {
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let non_empty = |name| {
            get(name)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        Some(Self {
            http_base: non_empty(ENV_ATTACH_URL)?.trim_end_matches('/').to_owned(),
            pairing_token: non_empty(ENV_PAIRING_TOKEN)
                .map(|input| crate::auth::pairing_token(&input)),
        })
    }
}

/// How to start the server. The command is data: dev runs `node <entry>`, a
/// packaged build runs `t3` from PATH with no entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub t3_home: String,
    pub command: ServerCommand,
}

impl Config {
    pub fn http_base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// `None` when the launcher did not ask this app to manage a server.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let non_empty = |name| get(name).filter(|value| !value.trim().is_empty());
        let port = non_empty(ENV_PORT)?.trim().parse().ok()?;
        let t3_home = non_empty(ENV_HOME)?;
        let program = non_empty(ENV_COMMAND)?;
        let args = non_empty(ENV_ENTRY).into_iter().collect();
        Some(Self {
            port,
            t3_home,
            command: ServerCommand { program, args },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn reads_all_settings() {
        let config = Config::from_lookup(lookup(&[
            (ENV_PORT, "13773"),
            (ENV_HOME, "/work/.t3"),
            (ENV_COMMAND, "node"),
            (ENV_ENTRY, "/work/apps/server/src/bin.ts"),
        ]))
        .expect("config");
        assert_eq!(config.port, 13773);
        assert_eq!(config.command.args, vec!["/work/apps/server/src/bin.ts"]);
        assert_eq!(config.http_base(), "http://127.0.0.1:13773");
    }

    #[test]
    fn entry_is_optional_and_missing_values_disable_the_server() {
        let config = Config::from_lookup(lookup(&[
            (ENV_PORT, "1"),
            (ENV_HOME, "/h"),
            (ENV_COMMAND, "t3"),
        ]))
        .expect("config");
        assert!(config.command.args.is_empty());
        assert_eq!(Config::from_lookup(lookup(&[(ENV_PORT, "1")])), None);
        assert_eq!(Config::from_lookup(lookup(&[])), None);
    }

    #[test]
    fn attach_needs_a_url_and_the_token_is_optional() {
        let attach = Attach::from_lookup(lookup(&[
            (ENV_ATTACH_URL, "http://127.0.0.1:13773/"),
            (ENV_PAIRING_TOKEN, "tok"),
        ]))
        .expect("attach");
        assert_eq!(attach.http_base, "http://127.0.0.1:13773");
        assert_eq!(attach.pairing_token.as_deref(), Some("tok"));
        let paired = Attach::from_lookup(lookup(&[(ENV_ATTACH_URL, "http://h")])).expect("attach");
        assert_eq!(paired.pairing_token, None);
        assert_eq!(
            Attach::from_lookup(lookup(&[(ENV_PAIRING_TOKEN, "tok")])),
            None
        );
        let link = Attach::from_lookup(lookup(&[
            (ENV_ATTACH_URL, "http://h"),
            (ENV_PAIRING_TOKEN, "http://h/pair#token=abc"),
        ]))
        .expect("attach");
        assert_eq!(link.pairing_token.as_deref(), Some("abc"));
    }
}
