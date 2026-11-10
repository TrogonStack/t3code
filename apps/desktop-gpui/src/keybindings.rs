//! The server's resolved keybindings (`ResolvedKeybindingRule`, `ServerConfig.keybindings` and the
//! `keybindingsUpdated` config stream event), decoded by hand. The server has already merged the
//! defaults with the user's rules, so the client only reads them.

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: String,
    pub meta: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Command on macOS, Control elsewhere.
    pub modifier: bool,
}

/// The `whenAst` of a rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum When {
    Identifier(String),
    Not(Box<When>),
    And(Box<When>, Box<When>),
    Or(Box<When>, Box<When>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub command: String,
    pub shortcut: Shortcut,
    pub when: Option<When>,
}

/// `kind` is `keybindings.malformed-config` or `keybindings.invalid-entry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub kind: String,
    pub message: String,
    pub index: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keybindings {
    pub rules: Vec<Rule>,
    pub issues: Vec<Issue>,
    pub path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireShortcut {
    key: String,
    #[serde(default)]
    meta_key: bool,
    #[serde(default)]
    ctrl_key: bool,
    #[serde(default)]
    shift_key: bool,
    #[serde(default)]
    alt_key: bool,
    #[serde(default)]
    mod_key: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRule {
    command: String,
    shortcut: WireShortcut,
    when_ast: Option<Value>,
}

#[derive(Deserialize)]
struct WireIssue {
    kind: String,
    message: String,
    index: Option<usize>,
}

fn decode_when(value: &Value) -> Option<When> {
    let node = |name: &str| decode_when(&value[name]).map(Box::new);
    Some(match value["type"].as_str()? {
        "identifier" => When::Identifier(value["name"].as_str()?.to_owned()),
        "not" => When::Not(node("node")?),
        "and" => When::And(node("left")?, node("right")?),
        "or" => When::Or(node("left")?, node("right")?),
        _ => return None,
    })
}

fn decode_rules(value: &Value) -> Vec<Rule> {
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    // A rule this client cannot read is dropped, as the web's forward-compatible array does.
    entries
        .iter()
        .filter_map(|entry| {
            let wire = serde_json::from_value::<WireRule>(entry.clone()).ok()?;
            let when = match &wire.when_ast {
                None | Some(Value::Null) => None,
                Some(ast) => Some(decode_when(ast)?),
            };
            let shortcut = wire.shortcut;
            Some(Rule {
                command: wire.command,
                shortcut: Shortcut {
                    key: shortcut.key,
                    meta: shortcut.meta_key,
                    ctrl: shortcut.ctrl_key,
                    shift: shortcut.shift_key,
                    alt: shortcut.alt_key,
                    modifier: shortcut.mod_key,
                },
                when,
            })
        })
        .collect()
}

fn decode_issues(value: &Value) -> Vec<Issue> {
    value
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| serde_json::from_value::<WireIssue>(entry.clone()).ok())
                .map(|issue| Issue {
                    kind: issue.kind,
                    message: issue.message,
                    index: issue.index,
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Keybindings {
    /// What one `subscribeServerConfig` element says about keybindings: the snapshot's config or a
    /// `keybindingsUpdated` payload. `None` for every other event.
    pub fn from_event(event: &Value, previous_path: Option<&str>) -> Option<Self> {
        match event["type"].as_str()? {
            "snapshot" => {
                let config = &event["config"];
                Some(Self {
                    rules: decode_rules(&config["keybindings"]),
                    issues: decode_issues(&config["issues"]),
                    path: config["keybindingsConfigPath"].as_str().map(str::to_owned),
                })
            }
            "keybindingsUpdated" => Some(Self {
                rules: decode_rules(&event["payload"]["keybindings"]),
                issues: decode_issues(&event["payload"]["issues"]),
                path: previous_path.map(str::to_owned),
            }),
            _ => None,
        }
    }
}

/// What a `when` expression comes to once the identifiers this client can answer are known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Predicate {
    Always,
    Never,
    /// A GPUI key context predicate.
    Expr(String),
}

/// One identifier of a `when` expression, as this client answers it.
pub enum Identifier {
    Always,
    Never,
    Context(String),
}

impl When {
    pub fn fold(&self, identify: &impl Fn(&str) -> Identifier) -> Predicate {
        match self {
            Self::Identifier(name) => match identify(name) {
                Identifier::Always => Predicate::Always,
                Identifier::Never => Predicate::Never,
                Identifier::Context(context) => Predicate::Expr(context),
            },
            Self::Not(inner) => match inner.fold(identify) {
                Predicate::Always => Predicate::Never,
                Predicate::Never => Predicate::Always,
                Predicate::Expr(expr) => Predicate::Expr(format!("!({expr})")),
            },
            Self::And(left, right) => match (left.fold(identify), right.fold(identify)) {
                (Predicate::Never, _) | (_, Predicate::Never) => Predicate::Never,
                (Predicate::Always, other) | (other, Predicate::Always) => other,
                (Predicate::Expr(left), Predicate::Expr(right)) => {
                    Predicate::Expr(format!("({left}) && ({right})"))
                }
            },
            Self::Or(left, right) => match (left.fold(identify), right.fold(identify)) {
                (Predicate::Always, _) | (_, Predicate::Always) => Predicate::Always,
                (Predicate::Never, other) | (other, Predicate::Never) => other,
                (Predicate::Expr(left), Predicate::Expr(right)) => {
                    Predicate::Expr(format!("({left}) || ({right})"))
                }
            },
        }
    }
}

impl Shortcut {
    /// The GPUI keystroke: `mod` is Command on macOS and Control elsewhere.
    pub fn keystroke(&self, mac: bool) -> String {
        let mut text = String::new();
        if self.ctrl || (self.modifier && !mac) {
            text.push_str("ctrl-");
        }
        if self.alt {
            text.push_str("alt-");
        }
        if self.shift {
            text.push_str("shift-");
        }
        if self.meta || (self.modifier && mac) {
            text.push_str("cmd-");
        }
        text.push_str(match self.key.as_str() {
            "arrowup" => "up",
            "arrowdown" => "down",
            "arrowleft" => "left",
            "arrowright" => "right",
            " " => "space",
            other => other,
        });
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> Value {
        json!({
            "version": 1,
            "type": "snapshot",
            "config": {
                "environment": {},
                "keybindingsConfigPath": "/home/user/.t3/keybindings.json",
                "keybindings": [
                    {
                        "command": "sidebar.toggle",
                        "shortcut": {
                            "key": "b", "metaKey": false, "ctrlKey": false,
                            "shiftKey": false, "altKey": false, "modKey": true
                        }
                    },
                    {
                        "command": "terminal.split",
                        "shortcut": {
                            "key": "d", "metaKey": false, "ctrlKey": false,
                            "shiftKey": false, "altKey": false, "modKey": true
                        },
                        "whenAst": { "type": "identifier", "name": "terminalFocus" }
                    },
                    {
                        "command": "navigation.back",
                        "shortcut": {
                            "key": "[", "metaKey": false, "ctrlKey": false,
                            "shiftKey": false, "altKey": false, "modKey": true
                        },
                        "whenAst": {
                            "type": "and",
                            "left": { "type": "not", "node": { "type": "identifier", "name": "terminalFocus" } },
                            "right": { "type": "identifier", "name": "isDesktop" }
                        }
                    },
                    { "command": "broken", "shortcut": { "nope": 1 } },
                    {
                        "command": "usage.cost",
                        "shortcut": {
                            "key": "c", "metaKey": false, "ctrlKey": false,
                            "shiftKey": false, "altKey": false, "modKey": false
                        },
                        "whenAst": { "type": "xor" }
                    }
                ],
                "issues": [
                    { "kind": "keybindings.invalid-entry", "message": "Bad entry", "index": 2 },
                    { "kind": "keybindings.malformed-config", "message": "Not JSON" }
                ]
            }
        })
    }

    #[test]
    fn decodes_the_snapshot_and_drops_unreadable_rules() {
        let decoded = Keybindings::from_event(&snapshot(), None).unwrap();
        let commands = decoded
            .rules
            .iter()
            .map(|rule| rule.command.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            commands,
            ["sidebar.toggle", "terminal.split", "navigation.back"]
        );
        assert_eq!(
            decoded.path.as_deref(),
            Some("/home/user/.t3/keybindings.json")
        );
        assert_eq!(decoded.issues.len(), 2);
        assert_eq!(decoded.issues[0].index, Some(2));
        assert_eq!(decoded.issues[1].index, None);
        assert!(matches!(decoded.rules[2].when, Some(When::And(..))));
    }

    #[test]
    fn update_keeps_the_known_path_and_other_events_are_ignored() {
        let update = json!({
            "version": 1,
            "type": "keybindingsUpdated",
            "payload": { "keybindings": [], "issues": [] }
        });
        let decoded = Keybindings::from_event(&update, Some("/p")).unwrap();
        assert!(decoded.rules.is_empty());
        assert_eq!(decoded.path.as_deref(), Some("/p"));
        let other = json!({ "version": 1, "type": "providerStatuses", "payload": {} });
        assert!(Keybindings::from_event(&other, None).is_none());
    }

    #[test]
    fn keystrokes_follow_the_platform() {
        let shortcut = Shortcut {
            key: "arrowup".into(),
            meta: false,
            ctrl: false,
            shift: true,
            alt: false,
            modifier: true,
        };
        assert_eq!(shortcut.keystroke(true), "shift-cmd-up");
        assert_eq!(shortcut.keystroke(false), "ctrl-shift-up");
    }

    #[test]
    fn when_folds_known_identifiers() {
        let identify = |name: &str| match name {
            "terminalFocus" => Identifier::Context("Terminal".into()),
            "isDesktop" => Identifier::Always,
            "isWeb" => Identifier::Never,
            other => Identifier::Context(other.into()),
        };
        let ident = |name: &str| Box::new(When::Identifier(name.into()));
        let not_terminal = When::And(
            Box::new(When::Not(ident("terminalFocus"))),
            ident("isDesktop"),
        );
        assert_eq!(
            not_terminal.fold(&identify),
            Predicate::Expr("!(Terminal)".into())
        );
        let web = When::And(ident("terminalFocus"), ident("isWeb"));
        assert_eq!(web.fold(&identify), Predicate::Never);
        let either = When::Or(ident("isWeb"), ident("a"));
        assert_eq!(either.fold(&identify), Predicate::Expr("a".into()));
    }
}
