# T3 Code desktop (GPUI)

Native desktop client written in Rust with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). It is meant to replace `apps/desktop` (Electron) over time and never embeds Node or Chromium. It starts the T3 server as a child process, authenticates without pairing, and opens the RPC WebSocket.

Status: connection shell. The window shows the connection state ("Starting server...", "Connecting...", "Connected to <environment>", or the error). There are no RPC methods or screens yet.

## Run

From the repo root:

```sh
vp run dev:desktop-gpui
```

This works like `vp run dev:desktop`: one command. The dev runner derives the server port and the per-worktree `.t3` home (never `~/.t3`), then runs `cargo run` here. No Vite or web server starts. Pass `--home-dir` to use another T3 home.

The app reads these variables, which the dev runner sets. Without them it shows "Not connected" and starts nothing.

| Variable                     | Meaning                                                             |
| ---------------------------- | ------------------------------------------------------------------- |
| `T3CODE_GPUI_SERVER_PORT`    | Loopback port for the server.                                       |
| `T3CODE_GPUI_HOME`           | T3 home passed to the server.                                       |
| `T3CODE_GPUI_SERVER_COMMAND` | Program that starts the server (`node` in dev, `t3` once packaged). |
| `T3CODE_GPUI_SERVER_ENTRY`   | Optional first argument, the server entry script in dev.            |

Other scripts:

```sh
vp run --filter @t3tools/desktop-gpui build  # cargo build --release
vp run --filter @t3tools/desktop-gpui test   # cargo test
```

## How it connects

1. Spawns `<command> [entry] --bootstrap-fd 3` and writes one JSON bootstrap line (`packages/contracts/src/desktopBootstrap.ts`) to a pipe dup'd onto fd 3, as Electron does. The secret is random per launch.
2. Polls `GET /.well-known/t3/environment` until the server is ready.
3. Exchanges the HMAC bootstrap token for a bearer session, mints a WebSocket ticket, and opens `/ws`.
4. Sends an RPC `Ping` every 5 seconds and reconnects with backoff after 15 seconds of silence.

On quit the app sends SIGTERM to the server it spawned and SIGKILL after 5 seconds. If the app is killed, the server notices its parent changed within about 2 seconds and shuts itself down.

## Notes

- `gpui` and `gpui_platform` are pinned to one `zed-industries/zed` commit in `Cargo.toml`. Bump both together.
- macOS needs the Xcode Metal toolchain (`xcodebuild -downloadComponent MetalToolchain`) and the `font-kit` feature, which is enabled.
- Server spawning uses a unix pipe for fd 3, so it is macOS and Linux only for now.
- CI does not build this crate yet. GPUI needs a macOS runner or Linux system libraries (wayland, x11, xkbcommon, vulkan).
