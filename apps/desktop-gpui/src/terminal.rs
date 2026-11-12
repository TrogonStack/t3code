//! The terminal drawer's logic, ported from `ThreadTerminalDrawer.tsx` (height bounds),
//! `useThreadTerminalActions.ts` (what a launch carries) and `terminalUiStateStore.ts` (which
//! terminal a thread starts with), plus the wire shapes of `packages/contracts/src/terminal.ts`
//! and the keyboard encoding the web gets from its terminal surface.
//! The terminal RPC types are not in the generated wire types; the payloads and the
//! `TerminalAttachStreamEvent` union are written here against that file.

use gpui::Keystroke;
use serde_json::{Value, json};

/// `DEFAULT_THREAD_TERMINAL_ID`: every thread starts with this one terminal.
pub const DEFAULT_TERMINAL_ID: &str = "term-1";
/// The web's default binding for `terminal.toggle` on macOS.
pub const TOGGLE_SHORTCUT: &str = "\u{2318}J";

/// `DEFAULT_THREAD_TERMINAL_HEIGHT`, `MIN_DRAWER_HEIGHT` and `MAX_DRAWER_HEIGHT_RATIO`.
pub const DEFAULT_HEIGHT: f32 = 280.;
pub const MIN_HEIGHT: f32 = 180.;
const MAX_HEIGHT_RATIO: f32 = 0.75;
/// The drawer's grid before it is measured, and the contract's bounds for one.
pub const FALLBACK_GRID: Grid = Grid { cols: 80, rows: 24 };
const MAX_COLS: usize = 1000;
const MAX_ROWS: usize = 500;

/// `resolveTerminalDrawerMaxHeight`: three quarters of the window, and never more than the chat
/// column leaves once the header and the floating composer are taken out.
pub fn max_height(viewport: f32, chat_column: f32) -> f32 {
    let window_cap = MIN_HEIGHT.max((viewport * MAX_HEIGHT_RATIO).floor());
    window_cap.min(chat_column.max(0.))
}

/// `clampDrawerHeight`.
pub fn clamp_height(height: f32, max: f32) -> f32 {
    let safe = if height.is_finite() {
        height
    } else {
        DEFAULT_HEIGHT
    };
    safe.round().max(MIN_HEIGHT).min(max)
}

/// A terminal's character grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
}

impl Grid {
    /// What fits `width` by `height` pixels at the given cell size, within the contract's bounds.
    pub fn fit(width: f32, height: f32, cell_width: f32, line_height: f32) -> Option<Self> {
        if cell_width <= 0. || line_height <= 0. {
            return None;
        }
        let cols = (width / cell_width).floor();
        let rows = (height / line_height).floor();
        (cols >= 1. && rows >= 1.).then(|| Self {
            cols: (cols as usize).min(MAX_COLS),
            rows: (rows as usize).min(MAX_ROWS),
        })
    }

    fn within_contract(cols: u64, rows: u64) -> Option<Self> {
        let valid = (1..=MAX_COLS as u64).contains(&cols) && (1..=MAX_ROWS as u64).contains(&rows);
        valid.then_some(Self {
            cols: cols as usize,
            rows: rows as usize,
        })
    }
}

/// Where a thread's terminal runs: its worktree, else its project's root
/// (`projectScriptCwd`), with the environment `projectScriptRuntimeEnv` adds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub thread_id: String,
    pub terminal_id: String,
    pub project_root: String,
    pub worktree_path: Option<String>,
}

impl Target {
    /// The same place for another terminal of the thread.
    pub fn for_terminal(&self, terminal_id: &str) -> Self {
        Self {
            terminal_id: terminal_id.to_owned(),
            ..self.clone()
        }
    }

    pub fn cwd(&self) -> &str {
        self.worktree_path.as_deref().unwrap_or(&self.project_root)
    }

    /// `terminal.attach`, which opens the session when the server has none and otherwise joins it.
    pub fn attach_payload(&self) -> Value {
        let mut env = serde_json::Map::new();
        env.insert("T3CODE_PROJECT_ROOT".into(), json!(self.project_root));
        if let Some(path) = &self.worktree_path {
            env.insert("T3CODE_WORKTREE_PATH".into(), json!(path));
        }
        let mut payload = json!({
            "threadId": self.thread_id,
            "terminalId": self.terminal_id,
            "cwd": self.cwd(),
            "env": env,
        });
        if let Some(path) = &self.worktree_path {
            payload["worktreePath"] = json!(path);
        }
        payload
    }
}

pub fn write_payload(thread_id: &str, terminal_id: &str, data: &str) -> Value {
    json!({ "threadId": thread_id, "terminalId": terminal_id, "data": data })
}

pub fn resize_payload(thread_id: &str, terminal_id: &str, grid: Grid) -> Value {
    json!({
        "threadId": thread_id,
        "terminalId": terminal_id,
        "cols": grid.cols,
        "rows": grid.rows,
    })
}

/// Ends the session and its history, as `closeTerminalSession` does.
pub fn close_payload(thread_id: &str, terminal_id: &str) -> Value {
    json!({ "threadId": thread_id, "terminalId": terminal_id, "deleteHistory": true })
}

/// One element of the `terminal.attach` stream (`TerminalAttachStreamEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The session as the server holds it: its output so far replaces whatever was drawn.
    Snapshot {
        history: String,
        grid: Option<Grid>,
        exited: bool,
    },
    Output(String),
    Cleared,
    Restarted {
        history: String,
        grid: Option<Grid>,
    },
    /// The shared PTY grid changed.
    Resized(Grid),
    Exited,
    Closed,
    Error(String),
}

fn grid_of(value: &Value) -> Option<Grid> {
    Grid::within_contract(value.get("cols")?.as_u64()?, value.get("rows")?.as_u64()?)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

/// Reads a stream element; `None` for the ones this client has nothing to do for (`activity`
/// without a grid) and for shapes it does not know.
pub fn decode_event(value: &Value) -> Option<Event> {
    match value.get("type")?.as_str()? {
        "snapshot" => {
            let snapshot = value.get("snapshot")?;
            Some(Event::Snapshot {
                history: text(snapshot, "history")?,
                grid: grid_of(snapshot),
                exited: matches!(
                    snapshot.get("status").and_then(Value::as_str),
                    Some("exited" | "error")
                ),
            })
        }
        "restarted" => {
            let snapshot = value.get("snapshot")?;
            Some(Event::Restarted {
                history: text(snapshot, "history")?,
                grid: grid_of(snapshot),
            })
        }
        "output" => Some(Event::Output(text(value, "data")?)),
        "cleared" => Some(Event::Cleared),
        "exited" => Some(Event::Exited),
        "closed" => Some(Event::Closed),
        "error" => Some(Event::Error(text(value, "message")?)),
        "activity" => grid_of(value).map(Event::Resized),
        _ => None,
    }
}

/// What the keyboard sends the shell: the bytes xterm would, for printable text, Enter,
/// Backspace, Tab, Escape, the arrows and navigation keys, function keys and Ctrl chords.
/// Command chords are the app's, so they send nothing.
pub fn encode_key(keystroke: &Keystroke, app_cursor: bool) -> Option<String> {
    let mods = keystroke.modifiers;
    if mods.platform || mods.function {
        return None;
    }
    let key = keystroke.key.as_str();
    // xterm's modifier parameter: 1 + shift + 2 * alt + 4 * ctrl.
    let param = 1 + mods.shift as u8 + 2 * mods.alt as u8 + 4 * mods.control as u8;
    let modified = param > 1;
    let csi = |code: &str, final_byte: char| -> String {
        if modified {
            format!("\x1b[{code};{param}{final_byte}")
        } else {
            format!("\x1b[{}{final_byte}", if code == "1" { "" } else { code })
        }
    };
    let cursor = |letter: char| -> String {
        if modified {
            format!("\x1b[1;{param}{letter}")
        } else if app_cursor {
            format!("\x1bO{letter}")
        } else {
            format!("\x1b[{letter}")
        }
    };
    let named = match key {
        "enter" => Some(if mods.alt { "\x1b\r" } else { "\r" }.to_owned()),
        "backspace" => Some(if mods.alt { "\x1b\x7f" } else { "\x7f" }.to_owned()),
        "tab" if mods.shift => Some("\x1b[Z".to_owned()),
        "tab" => Some("\t".to_owned()),
        "escape" => Some("\x1b".to_owned()),
        "up" => Some(cursor('A')),
        "down" => Some(cursor('B')),
        "right" => Some(cursor('C')),
        "left" => Some(cursor('D')),
        "home" => Some(cursor('H')),
        "end" => Some(cursor('F')),
        "insert" => Some(csi("2", '~')),
        "delete" => Some(csi("3", '~')),
        "pageup" => Some(csi("5", '~')),
        "pagedown" => Some(csi("6", '~')),
        "f1" => Some(function_key(1, param)),
        "f2" => Some(function_key(2, param)),
        "f3" => Some(function_key(3, param)),
        "f4" => Some(function_key(4, param)),
        "f5" => Some(csi("15", '~')),
        "f6" => Some(csi("17", '~')),
        "f7" => Some(csi("18", '~')),
        "f8" => Some(csi("19", '~')),
        "f9" => Some(csi("20", '~')),
        "f10" => Some(csi("21", '~')),
        "f11" => Some(csi("23", '~')),
        "f12" => Some(csi("24", '~')),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if mods.control {
        let byte = control_byte(key)?;
        let mut sent = String::new();
        if mods.alt {
            sent.push('\x1b');
        }
        sent.push(byte as char);
        return Some(sent);
    }
    if key == "space" {
        return Some(if mods.alt { "\x1b " } else { " " }.to_owned());
    }
    // `key_char` is the typed character, composed by Option and the layout.
    keystroke
        .key_char
        .as_deref()
        .filter(|typed| !typed.is_empty() && !typed.chars().any(char::is_control))
        .map(str::to_owned)
}

fn function_key(number: u8, param: u8) -> String {
    let letter = (b'O' + number) as char;
    if param > 1 {
        format!("\x1b[1;{param}{letter}")
    } else {
        format!("\x1bO{letter}")
    }
}

/// The C0 byte a Ctrl chord types, `Ctrl-A` as 0x01 through `Ctrl-_` as 0x1f.
fn control_byte(key: &str) -> Option<u8> {
    let mut chars = key.chars();
    let (first, None) = (chars.next()?, chars.next()) else {
        return (key == "space").then_some(0);
    };
    match first {
        'a'..='z' => Some(first as u8 - b'a' + 1),
        '@' | ' ' | '2' => Some(0),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' => Some(0x1f),
        '?' => Some(0x7f),
        _ => None,
    }
}

/// Pasted text, which the shell takes as one unit when it asked for bracketed paste.
pub fn paste_text(text: &str, bracketed: bool) -> String {
    if bracketed {
        // A pasted escape would end the bracket early.
        format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A parsed keystroke with the `key_char` a real key event carries for a printable key.
    fn typed(text: &'static str) -> Keystroke {
        let mut keystroke = Keystroke::parse(text).expect("keystroke");
        if keystroke.key.chars().count() == 1 {
            keystroke.key_char = Some(if keystroke.modifiers.shift {
                keystroke.key.to_uppercase()
            } else {
                keystroke.key.clone()
            });
        }
        keystroke
    }

    fn keys(text: &'static str) -> Option<String> {
        encode_key(&typed(text), false)
    }

    #[test]
    fn height_follows_the_webs_bounds() {
        assert_eq!(max_height(1000., 900.), 750.);
        assert_eq!(max_height(1000., 400.), 400.);
        assert_eq!(max_height(100., 900.), MIN_HEIGHT);
        assert_eq!(clamp_height(50., 750.), MIN_HEIGHT);
        assert_eq!(clamp_height(900., 750.), 750.);
        assert_eq!(clamp_height(300.4, 750.), 300.);
        assert_eq!(clamp_height(f32::NAN, 750.), DEFAULT_HEIGHT);
        // A chat column too short for the minimum wins, as on the web.
        assert_eq!(clamp_height(280., 120.), 120.);
    }

    #[test]
    fn the_grid_is_what_fits_within_the_contract() {
        assert_eq!(
            Grid::fit(805., 330., 8., 16.),
            Some(Grid {
                cols: 100,
                rows: 20
            })
        );
        assert_eq!(Grid::fit(4., 330., 8., 16.), None);
        assert_eq!(Grid::fit(80., 330., 0., 16.), None);
        assert_eq!(
            Grid::fit(100_000., 100_000., 8., 16.),
            Some(Grid {
                cols: 1000,
                rows: 500
            })
        );
    }

    #[test]
    fn attach_runs_in_the_worktree_else_the_project_root() {
        let mut target = Target {
            thread_id: "t1".into(),
            terminal_id: "term-1".into(),
            project_root: "/work/app".into(),
            worktree_path: None,
        };
        assert_eq!(
            target.attach_payload(),
            json!({
                "threadId": "t1",
                "terminalId": "term-1",
                "cwd": "/work/app",
                "env": { "T3CODE_PROJECT_ROOT": "/work/app" },
            })
        );
        target.worktree_path = Some("/work/app-wt".into());
        assert_eq!(
            target.attach_payload(),
            json!({
                "threadId": "t1",
                "terminalId": "term-1",
                "cwd": "/work/app-wt",
                "worktreePath": "/work/app-wt",
                "env": {
                    "T3CODE_PROJECT_ROOT": "/work/app",
                    "T3CODE_WORKTREE_PATH": "/work/app-wt",
                },
            })
        );
    }

    #[test]
    fn unary_payloads_name_their_terminal() {
        assert_eq!(
            write_payload("t1", "term-1", "ls\r"),
            json!({"threadId": "t1", "terminalId": "term-1", "data": "ls\r"})
        );
        assert_eq!(
            resize_payload("t1", "term-1", Grid { cols: 90, rows: 12 }),
            json!({"threadId": "t1", "terminalId": "term-1", "cols": 90, "rows": 12})
        );
        assert_eq!(
            close_payload("t1", "term-1"),
            json!({"threadId": "t1", "terminalId": "term-1", "deleteHistory": true})
        );
    }

    #[test]
    fn stream_events_decode_from_the_contract() {
        let snapshot = json!({
            "type": "snapshot",
            "snapshot": {
                "threadId": "t1", "terminalId": "term-1", "cwd": "/w", "worktreePath": null,
                "status": "running", "pid": 7, "history": "$ ", "exitCode": null,
                "exitSignal": null, "label": "zsh", "updatedAt": "now",
                "cols": 100, "rows": 30,
            },
        });
        assert_eq!(
            decode_event(&snapshot),
            Some(Event::Snapshot {
                history: "$ ".into(),
                grid: Some(Grid {
                    cols: 100,
                    rows: 30
                }),
                exited: false
            })
        );
        let old_server =
            json!({"type": "snapshot", "snapshot": {"history": "", "status": "exited"}});
        assert_eq!(
            decode_event(&old_server),
            Some(Event::Snapshot {
                history: String::new(),
                grid: None,
                exited: true
            })
        );
        assert_eq!(
            decode_event(&json!({"type": "output", "data": "hi\r\n"})),
            Some(Event::Output("hi\r\n".into()))
        );
        assert_eq!(
            decode_event(&json!({"type": "error", "message": "boom"})),
            Some(Event::Error("boom".into()))
        );
        assert_eq!(
            decode_event(&json!({"type": "activity", "hasRunningSubprocess": true, "label": "x"})),
            None
        );
        assert_eq!(
            decode_event(&json!({"type": "activity", "label": "x", "cols": 50, "rows": 9})),
            Some(Event::Resized(Grid { cols: 50, rows: 9 }))
        );
        assert_eq!(
            decode_event(&json!({"type": "exited", "exitCode": 0})),
            Some(Event::Exited)
        );
        assert_eq!(
            decode_event(&json!({"type": "closed"})),
            Some(Event::Closed)
        );
        assert_eq!(decode_event(&json!({"type": "mystery"})), None);
        assert_eq!(decode_event(&json!({"type": "output"})), None);
    }

    #[test]
    fn printable_text_and_named_keys_encode_as_xterm_does() {
        assert_eq!(keys("a").as_deref(), Some("a"));
        assert_eq!(keys("shift-a").as_deref(), Some("A"));
        assert_eq!(keys("enter").as_deref(), Some("\r"));
        assert_eq!(keys("backspace").as_deref(), Some("\x7f"));
        assert_eq!(keys("tab").as_deref(), Some("\t"));
        assert_eq!(keys("shift-tab").as_deref(), Some("\x1b[Z"));
        assert_eq!(keys("escape").as_deref(), Some("\x1b"));
        assert_eq!(keys("space").as_deref(), Some(" "));
        assert_eq!(keys("up").as_deref(), Some("\x1b[A"));
        assert_eq!(keys("left").as_deref(), Some("\x1b[D"));
        assert_eq!(keys("delete").as_deref(), Some("\x1b[3~"));
        assert_eq!(keys("pageup").as_deref(), Some("\x1b[5~"));
        assert_eq!(keys("f1").as_deref(), Some("\x1bOP"));
        assert_eq!(keys("f5").as_deref(), Some("\x1b[15~"));
        assert_eq!(keys("ctrl-right").as_deref(), Some("\x1b[1;5C"));
        assert_eq!(keys("alt-backspace").as_deref(), Some("\x1b\x7f"));
    }

    #[test]
    fn application_cursor_mode_changes_only_unmodified_arrows() {
        let up = Keystroke::parse("up").expect("keystroke");
        assert_eq!(encode_key(&up, true).as_deref(), Some("\x1bOA"));
        let ctrl_up = Keystroke::parse("ctrl-up").expect("keystroke");
        assert_eq!(encode_key(&ctrl_up, true).as_deref(), Some("\x1b[1;5A"));
    }

    #[test]
    fn control_chords_type_c0_bytes() {
        assert_eq!(keys("ctrl-c").as_deref(), Some("\x03"));
        assert_eq!(keys("ctrl-a").as_deref(), Some("\x01"));
        assert_eq!(keys("ctrl-z").as_deref(), Some("\x1a"));
        assert_eq!(keys("ctrl-[").as_deref(), Some("\x1b"));
        assert_eq!(keys("ctrl-space").as_deref(), Some("\0"));
        assert_eq!(keys("ctrl-alt-d").as_deref(), Some("\x1b\x04"));
    }

    #[test]
    fn command_chords_belong_to_the_app() {
        assert_eq!(keys("cmd-c"), None);
        assert_eq!(keys("cmd-j"), None);
        assert_eq!(keys("shift"), None);
    }

    #[test]
    fn paste_brackets_when_the_shell_asked_for_it() {
        assert_eq!(paste_text("a\nb", false), "a\rb");
        assert_eq!(paste_text("a\r\nb", false), "a\rb");
        assert_eq!(paste_text("a\x1b[201~b", true), "\x1b[200~a[201~b\x1b[201~");
    }
}
