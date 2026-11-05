# T3 Code desktop (GPUI)

Native desktop client written in Rust with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). It is meant to replace `apps/desktop` (Electron) over time and never embeds Node or Chromium. It starts the T3 server as a child process, authenticates without pairing, and opens the RPC WebSocket.

Status: connection shell. The window shows the connection state ("Starting server...", "Connecting...", "Connected to <environment> · N projects · M threads", or the error). The counts come from a live `orchestration.subscribeShell` subscription. Once connected, it draws a static skeleton of the main screen with [gpui-component](https://github.com/longbridge/gpui-component): a thread list (newest first) and an empty composer. Nothing is interactive yet.

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

To see real data, attach to a server that is already running (for example your `vp run dev:desktop`) instead of starting one. That server keeps sole ownership of its database. Mint a one-time pairing token with `node apps/server/src/bin.ts pair --base-dir <its T3 home>`, then run the app with:

| Variable                    | Meaning                                                   |
| --------------------------- | --------------------------------------------------------- |
| `T3CODE_GPUI_SERVER_URL`    | HTTP origin of the running server. Takes precedence.      |
| `T3CODE_GPUI_PAIRING_TOKEN` | The `Token:` value printed by `pair`. It works only once. |

Other scripts:

```sh
vp run --filter @t3tools/desktop-gpui build  # cargo build --release
vp run --filter @t3tools/desktop-gpui test   # cargo test
```

## How it connects

1. Spawns `<command> [entry] --bootstrap-fd 3` and writes one JSON bootstrap line (`packages/contracts/src/desktopBootstrap.ts`) to a pipe dup'd onto fd 3, as Electron does. The secret is random per launch.
2. Polls `GET /.well-known/t3/environment` until the server is ready.
3. Exchanges the HMAC bootstrap token for a bearer session, mints a WebSocket ticket, and opens `/ws`.
4. Subscribes to `orchestration.subscribeShell`, acks each stream chunk, and reduces snapshot and delta items into project and thread counts (`src/rpc.rs`, `src/shell.rs`).
5. Sends an RPC `Ping` every 5 seconds and reconnects with backoff after 15 seconds of silence or when the stream ends.

On quit the app sends SIGTERM to the server it spawned and SIGKILL after 5 seconds. If the app is killed, the server notices its parent changed within about 2 seconds and shuts itself down.

## Wire types

`packages/contracts/schema/rpc.schema.json` is generated from the Effect schemas (`vp run --filter @t3tools/contracts generate`, or `generate --check` to verify). It holds the encoded side of every `WsRpcGroup` method as shared `$defs` plus a `methods` index (payload, success, error refs, stream flag).

`src/wire/generated.rs` is generated from the methods this client calls (`METHODS` in `tests/wire_types.rs`) with [typify](https://github.com/oxidecomputer/typify) and committed, so normal builds need no codegen and diffs stay reviewable. To add a method, add its tag to `METHODS`, then regenerate:

```sh
T3_UPDATE_GENERATED=1 cargo test --test wire_types
```

Without the variable the same test fails when the committed file is stale. Drift in the schema itself is caught by `generate --check` in CI and `vp run check:changed`.

Known gaps: typify cannot flatten the `projectIcon` union (an `anyOf` with a `{kind: string}` catch-all), so `ProjectsEntry.projectIcon` is kept opaque (`serde_json::Value`) through the `OPAQUE` list. Unions without a discriminator generate as `#[serde(untagged)]` enums.

## Notes

- `gpui` and `gpui_platform` are the crates.io packages `gpui-pre` and `gpui-pre-platform`, pinned to the exact version `gpui-component` requires. Bump them together with `gpui-component` and `gpui-kit-assets`.
- macOS needs the Xcode Metal toolchain (`xcodebuild -downloadComponent MetalToolchain`) and the `font-kit` feature, which is enabled.
- Server spawning uses a unix pipe for fd 3, so it is macOS and Linux only for now.
- CI does not build this crate yet. GPUI needs a macOS runner or Linux system libraries (wayland, x11, xkbcommon, vulkan).
