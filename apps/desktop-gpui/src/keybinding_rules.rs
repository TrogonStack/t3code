//! The Keybindings tab's model: `KeybindingsSettings.logic.ts` and the parts of
//! `packages/shared/src/keybindings.ts` it uses (the default rules, the shortcut and `when`
//! parsers). The server merges and resolves rules; this file only reads them back into rows.

use crate::keybindings::{Rule, Shortcut, When};
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// Call tokens of the Keybindings tab, above every other range.
pub const TOKEN_BASE: u64 = 1 << 53;
pub const MAX_WHEN_DEPTH: usize = 64;
#[cfg(test)]
const MAX_KEYBINDINGS: usize = 256;

/// `DEFAULT_KEYBINDINGS` as `(key, command, when)`.
const DEFAULTS: &[(&str, &str, &str)] = &[
    ("mod+b", "sidebar.toggle", ""),
    ("f6", "workspace.focusNext", ""),
    ("shift+f6", "workspace.focusPrevious", ""),
    ("mod+f6", "workspace.focusNext", ""),
    ("mod+shift+f6", "workspace.focusPrevious", ""),
    ("mod+[", "navigation.back", "!terminalFocus"),
    ("mod+]", "navigation.forward", "!terminalFocus"),
    ("mod+j", "terminal.toggle", ""),
    ("mod+alt+b", "rightPanel.toggle", ""),
    ("mod+shift+b", "threadPanel.toggle", "!terminalFocus"),
    (
        "mod+t",
        "rightPanel.new",
        "rightPanelOpen && !terminalFocus",
    ),
    ("mod+d", "terminal.split", "terminalFocus"),
    ("mod+shift+d", "terminal.splitVertical", "terminalFocus"),
    ("mod+shift+d", "composer.dictation", "!terminalFocus"),
    ("mod+n", "terminal.new", "terminalFocus"),
    ("mod+w", "terminal.close", "terminalFocus"),
    ("mod+w", "rightPanel.close", "!terminalFocus"),
    ("mod+shift+t", "view.reopenClosed", ""),
    ("mod+d", "diff.toggle", "!terminalFocus"),
    ("mod+shift+j", "preview.toggle", ""),
    ("mod+r", "preview.refresh", "previewFocus"),
    ("mod+l", "preview.focusUrl", "previewFocus"),
    ("mod+=", "preview.zoomIn", "previewFocus"),
    ("mod++", "preview.zoomIn", "previewFocus"),
    ("mod+-", "preview.zoomOut", "previewFocus"),
    ("mod+0", "preview.resetZoom", "previewFocus"),
    ("mod+k", "commandPalette.toggle", "!terminalFocus"),
    ("mod+p", "filePicker.toggle", "!terminalFocus"),
    ("mod+shift+f", "projectSearch.toggle", "!terminalFocus"),
    ("mod+u", "usage.open", "!terminalFocus"),
    ("mod+alt+a", "theme.select", "!terminalFocus"),
    ("mod+alt+shift+a", "appearance.cycle", "!terminalFocus"),
    ("mod+alt+shift+t", "themeEditor.toggle", ""),
    ("mod+s", "composer.stash", "!terminalFocus"),
    (
        "mod+shift+enter",
        "thread.steerQueuedMessage",
        "!terminalFocus",
    ),
    ("alt+arrowup", "thread.editQueuedMessage", "composerFocus"),
    (
        "mod+enter",
        "composer.sendAlternate",
        "composerFocus && turnRunning",
    ),
    (
        "mod+enter",
        "composer.sendBackground",
        "composerFocus && draftThreadRoute",
    ),
    (
        "mod+alt+enter",
        "composer.sendBackground",
        "composerFocus && draftThreadRoute",
    ),
    (
        "mod+alt+enter",
        "composer.sendAndNewThread",
        "composerFocus && !draftThreadRoute",
    ),
    ("mod+n", "chat.new", "!terminalFocus"),
    ("mod+shift+o", "chat.new", "!terminalFocus"),
    ("mod+shift+n", "chat.newLocal", "!terminalFocus"),
    ("mod+alt+n", "chat.newWithoutProject", "!terminalFocus"),
    ("mod+f", "chat.find", "!terminalFocus && !previewFocus"),
    ("mod+shift+m", "modelPicker.toggle", "!terminalFocus"),
    ("mod+shift+h", "composer.host", "!terminalFocus"),
    ("mod+shift+e", "composer.effort", "!terminalFocus"),
    ("mod+shift+a", "composer.mode", "!terminalFocus"),
    ("mod+shift+x", "composer.workspace", "!terminalFocus"),
    ("mod+shift+g", "composer.branch", "!terminalFocus"),
    ("mod+shift+l", "composer.previousWorktree", "!terminalFocus"),
    ("mod+shift+k", "pullRequest.copyNumber", "!terminalFocus"),
    (
        "mod+shift+arrowup",
        "modelPicker.previousProvider",
        "modelPickerOpen",
    ),
    (
        "mod+shift+arrowdown",
        "modelPicker.nextProvider",
        "modelPickerOpen",
    ),
    ("mod+o", "editor.openFavorite", ""),
    ("mod+shift+[", "thread.previous", ""),
    ("mod+shift+]", "thread.next", ""),
    ("mod+shift+c", "thread.copyReference", "!terminalFocus"),
    ("mod+shift+s", "thread.settle", "!terminalFocus"),
    ("mod+shift+p", "thread.pin", "!terminalFocus"),
    ("mod+z", "thread.undo", "!terminalFocus && !editableFocus"),
];

const USAGE_DEFAULTS: &[(&str, &str, &str)] = &[
    ("c", "usage.cost", "usagePageOpen"),
    ("t", "usage.tokens", "usagePageOpen"),
    ("l", "usage.limits", "usagePageOpen"),
    ("mod+shift+1", "usage.period.day", "usagePageOpen"),
    ("mod+shift+2", "usage.period.week", "usagePageOpen"),
    ("mod+shift+3", "usage.period.month", "usagePageOpen"),
    ("mod+shift+4", "usage.period.quarter", "usagePageOpen"),
];

/// Commands the contract lists besides the jump commands.
const STATIC_COMMANDS: &[&str] = &[
    "workspace.focusNext",
    "workspace.focusPrevious",
    "sidebar.toggle",
    "navigation.back",
    "navigation.forward",
    "terminal.toggle",
    "terminal.split",
    "terminal.splitVertical",
    "terminal.new",
    "terminal.close",
    "rightPanel.toggle",
    "rightPanel.new",
    "threadPanel.toggle",
    "rightPanel.toggleMaximized",
    "rightPanel.close",
    "view.reopenClosed",
    "pullRequest.copyNumber",
    "diff.toggle",
    "preview.toggle",
    "preview.refresh",
    "preview.focusUrl",
    "preview.zoomIn",
    "preview.zoomOut",
    "preview.resetZoom",
    "commandPalette.toggle",
    "filePicker.toggle",
    "projectSearch.toggle",
    "usage.open",
    "theme.select",
    "appearance.cycle",
    "themeEditor.toggle",
    "composer.stash",
    "composer.sendAlternate",
    "composer.sendBackground",
    "composer.sendAndNewThread",
    "composer.dictation",
    "composer.host",
    "composer.cycleHost",
    "composer.effort",
    "composer.increaseEffort",
    "composer.decreaseEffort",
    "composer.mode",
    "composer.workspace",
    "composer.previousWorktree",
    "composer.branch",
    "chat.new",
    "chat.newLocal",
    "chat.newWithoutProject",
    "chat.find",
    "editor.openFavorite",
    "usage.cost",
    "usage.tokens",
    "usage.limits",
    "usage.period.day",
    "usage.period.week",
    "usage.period.month",
    "usage.period.quarter",
    "modelPicker.toggle",
    "modelPicker.previousProvider",
    "modelPicker.nextProvider",
    "thread.stop",
    "thread.steerQueuedMessage",
    "thread.editQueuedMessage",
    "thread.previous",
    "thread.next",
    "thread.copyReference",
    "thread.settle",
    "thread.pin",
    "thread.undo",
];

/// Variables the `when` field always knows, ahead of those the default rules use.
const CORE_WHEN_VARIABLES: [&str; 6] = [
    "terminalFocus",
    "terminalOpen",
    "isWeb",
    "isDesktop",
    "true",
    "false",
];

const USAGE_ORDER: [&str; 8] = [
    "usage.open",
    "usage.cost",
    "usage.tokens",
    "usage.limits",
    "usage.period.day",
    "usage.period.week",
    "usage.period.month",
    "usage.period.quarter",
];

pub fn is_mac() -> bool {
    cfg!(target_os = "macos")
}

fn normalize_key(token: &str) -> String {
    match token {
        "space" => " ".to_owned(),
        "esc" => "escape".to_owned(),
        other => other.to_owned(),
    }
}

/// `parseKeybindingShortcut`.
pub fn parse_shortcut(value: &str) -> Option<Shortcut> {
    let lowered = value.to_lowercase();
    let mut tokens: Vec<String> = lowered
        .split('+')
        .map(|token| token.trim().to_owned())
        .collect();
    let mut trailing = 0;
    while tokens.last().is_some_and(|token| token.is_empty()) {
        tokens.pop();
        trailing += 1;
    }
    if trailing > 0 {
        tokens.push("+".to_owned());
    }
    if tokens.is_empty() || tokens.iter().any(|token| token.is_empty()) {
        return None;
    }
    let mut shortcut = Shortcut {
        key: String::new(),
        meta: false,
        ctrl: false,
        shift: false,
        alt: false,
        modifier: false,
    };
    let mut key: Option<String> = None;
    for token in &tokens {
        match token.as_str() {
            "cmd" | "meta" => shortcut.meta = true,
            "ctrl" | "control" => shortcut.ctrl = true,
            "shift" => shortcut.shift = true,
            "alt" | "option" => shortcut.alt = true,
            "mod" => shortcut.modifier = true,
            other => {
                if key.is_some() {
                    return None;
                }
                key = Some(normalize_key(other));
            }
        }
    }
    shortcut.key = key?;
    Some(shortcut)
}

/// `shortcutToKeybindingInput`: mod, meta, ctrl, alt, shift, key.
pub fn shortcut_input(shortcut: &Shortcut) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if shortcut.modifier {
        parts.push("mod");
    }
    if shortcut.meta {
        parts.push("meta");
    }
    if shortcut.ctrl {
        parts.push("ctrl");
    }
    if shortcut.alt {
        parts.push("alt");
    }
    if shortcut.shift {
        parts.push("shift");
    }
    parts.push(match shortcut.key.as_str() {
        " " => "space",
        "escape" => "esc",
        other => other,
    });
    parts.join("+")
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Ident(String),
    Not,
    And,
    Or,
    Open,
    Close,
}

fn tokenize(expression: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = expression.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let current = chars[index];
        if current.is_whitespace() {
            index += 1;
        } else if current == '&' && chars.get(index + 1) == Some(&'&') {
            tokens.push(Token::And);
            index += 2;
        } else if current == '|' && chars.get(index + 1) == Some(&'|') {
            tokens.push(Token::Or);
            index += 2;
        } else if current == '!' {
            tokens.push(Token::Not);
            index += 1;
        } else if current == '(' {
            tokens.push(Token::Open);
            index += 1;
        } else if current == ')' {
            tokens.push(Token::Close);
            index += 1;
        } else if current.is_ascii_alphabetic() || current == '_' {
            let start = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '.' | '-'))
            {
                index += 1;
            }
            tokens.push(Token::Ident(chars[start..index].iter().collect()));
        } else {
            return None;
        }
    }
    Some(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn primary(&mut self, depth: usize) -> Option<When> {
        if depth > MAX_WHEN_DEPTH {
            return None;
        }
        match self.tokens.get(self.index)?.clone() {
            Token::Ident(name) => {
                self.index += 1;
                Some(When::Identifier(name))
            }
            Token::Open => {
                self.index += 1;
                let inner = self.or(depth + 1)?;
                if self.tokens.get(self.index) != Some(&Token::Close) {
                    return None;
                }
                self.index += 1;
                Some(inner)
            }
            _ => None,
        }
    }

    fn unary(&mut self, depth: usize) -> Option<When> {
        let mut nots = 0;
        while self.tokens.get(self.index) == Some(&Token::Not) {
            self.index += 1;
            nots += 1;
            if nots > MAX_WHEN_DEPTH {
                return None;
            }
        }
        let mut node = self.primary(depth)?;
        for _ in 0..nots {
            node = When::Not(Box::new(node));
        }
        Some(node)
    }

    fn and(&mut self, depth: usize) -> Option<When> {
        let mut left = self.unary(depth)?;
        while self.tokens.get(self.index) == Some(&Token::And) {
            self.index += 1;
            let right = self.unary(depth)?;
            left = When::And(Box::new(left), Box::new(right));
        }
        Some(left)
    }

    fn or(&mut self, depth: usize) -> Option<When> {
        let mut left = self.and(depth)?;
        while self.tokens.get(self.index) == Some(&Token::Or) {
            self.index += 1;
            let right = self.and(depth)?;
            left = When::Or(Box::new(left), Box::new(right));
        }
        Some(left)
    }
}

/// `parseKeybindingWhenExpression`.
pub fn parse_when(expression: &str) -> Option<When> {
    let tokens = tokenize(expression)?;
    if tokens.is_empty() {
        return None;
    }
    let mut parser = Parser { tokens, index: 0 };
    let node = parser.or(0)?;
    (parser.index == parser.tokens.len()).then_some(node)
}

/// `parseWhenExpressionDraft`: an empty draft is no condition, anything else must parse.
pub fn parse_when_draft(expression: &str) -> Result<Option<When>, &'static str> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    parse_when(trimmed)
        .map(Some)
        .ok_or("Use variables with !, &&, ||, and parentheses.")
}

fn wrap(node: &When) -> String {
    match node {
        When::Identifier(_) | When::Not(_) => when_expression(Some(node)),
        _ => format!("({})", when_expression(Some(node))),
    }
}

/// `whenAstToExpression`.
pub fn when_expression(node: Option<&When>) -> String {
    match node {
        None => String::new(),
        Some(When::Identifier(name)) => name.clone(),
        Some(When::Not(inner)) => format!("!{}", wrap(inner)),
        Some(When::And(left, right)) => format!("{} && {}", wrap(left), wrap(right)),
        Some(When::Or(left, right)) => format!("{} || {}", wrap(left), wrap(right)),
    }
}

fn identifiers(node: Option<&When>, into: &mut BTreeSet<String>) {
    match node {
        None => {}
        Some(When::Identifier(name)) => {
            into.insert(name.clone());
        }
        Some(When::Not(inner)) => identifiers(Some(inner), into),
        Some(When::And(left, right)) | Some(When::Or(left, right)) => {
            identifiers(Some(left), into);
            identifiers(Some(right), into);
        }
    }
}

fn known_variables() -> &'static BTreeSet<String> {
    static KNOWN: OnceLock<BTreeSet<String>> = OnceLock::new();
    KNOWN.get_or_init(|| {
        let mut known: BTreeSet<String> = CORE_WHEN_VARIABLES
            .iter()
            .map(|v| (*v).to_owned())
            .collect();
        for rule in default_rules() {
            identifiers(rule.when.as_ref(), &mut known);
        }
        known
    })
}

/// `unknownWhenVariables`, sorted.
pub fn unknown_variables(node: Option<&When>) -> Vec<String> {
    let mut used = BTreeSet::new();
    identifiers(node, &mut used);
    used.into_iter()
        .filter(|name| !known_variables().contains(name))
        .collect()
}

fn jump(prefix: &str, when: &str) -> Vec<(String, String, String)> {
    (1..=9)
        .map(|n| {
            (
                format!("mod+{n}"),
                format!("{prefix}.jump.{n}"),
                when.to_owned(),
            )
        })
        .collect()
}

fn compile(key: &str, command: &str, when: &str) -> Option<Rule> {
    Some(Rule {
        command: command.to_owned(),
        shortcut: parse_shortcut(key)?,
        when: if when.is_empty() {
            None
        } else {
            Some(parse_when(when)?)
        },
    })
}

/// `DEFAULT_RESOLVED_KEYBINDINGS`.
pub fn default_rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let mut rules: Vec<Rule> = DEFAULTS
            .iter()
            .filter_map(|(key, command, when)| compile(key, command, when))
            .collect();
        for (key, command, when) in jump("thread", "!modelPickerOpen && isDesktop")
            .into_iter()
            .chain(jump("modelPicker", "modelPickerOpen && isDesktop"))
        {
            rules.extend(compile(&key, &command, &when));
        }
        rules.extend(
            USAGE_DEFAULTS
                .iter()
                .filter_map(|(key, command, when)| compile(key, command, when)),
        );
        rules
    })
}

/// `mergeWithDefaultKeybindings`; the config stream already carries the merged rules, so only
/// the tests build them.
#[cfg(test)]
fn merge_with_defaults(custom: &[Rule]) -> Vec<Rule> {
    if custom.is_empty() {
        return default_rules().to_vec();
    }
    let overridden: BTreeSet<&str> = custom.iter().map(|rule| rule.command.as_str()).collect();
    let mut merged: Vec<Rule> = default_rules()
        .iter()
        .filter(|rule| !overridden.contains(rule.command.as_str()))
        .cloned()
        .collect();
    merged.extend(custom.iter().cloned());
    if merged.len() > MAX_KEYBINDINGS {
        merged.drain(..merged.len() - MAX_KEYBINDINGS);
    }
    merged
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Default,
    Custom,
    Project,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Custom => "Custom",
            Self::Project => "Project",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub command: String,
    pub key: String,
    pub when: String,
    pub source: Source,
    pub default_key: Option<String>,
    pub default_when: String,
    pub shortcut: Shortcut,
    pub conflicts: Vec<String>,
}

fn source_of(rule: &Rule) -> Source {
    if rule.command.starts_with("script.") {
        return Source::Project;
    }
    let key = shortcut_input(&rule.shortcut);
    let when = when_expression(rule.when.as_ref());
    let is_default = default_rules().iter().any(|entry| {
        entry.command == rule.command
            && shortcut_input(&entry.shortcut) == key
            && when_expression(entry.when.as_ref()) == when
    });
    if is_default {
        Source::Default
    } else {
        Source::Custom
    }
}

fn default_for(rule: &Rule) -> Option<&'static Rule> {
    let key = shortcut_input(&rule.shortcut);
    let when = when_expression(rule.when.as_ref());
    let defaults = default_rules();
    defaults
        .iter()
        .find(|entry| {
            entry.command == rule.command
                && shortcut_input(&entry.shortcut) == key
                && when_expression(entry.when.as_ref()) == when
        })
        .or_else(|| {
            defaults.iter().find(|entry| {
                entry.command == rule.command && when_expression(entry.when.as_ref()) == when
            })
        })
        .or_else(|| defaults.iter().find(|entry| entry.command == rule.command))
}

/// `shortcutConflictKey`.
fn conflict_key(shortcut: &Shortcut, mac: bool) -> String {
    let meta = shortcut.meta || (shortcut.modifier && mac);
    let ctrl = shortcut.ctrl || (shortcut.modifier && !mac);
    format!(
        "{}|{}|{}|{}|{}",
        shortcut.key,
        if meta { "meta" } else { "" },
        if ctrl { "ctrl" } else { "" },
        if shortcut.shift { "shift" } else { "" },
        if shortcut.alt { "alt" } else { "" },
    )
}

fn when_conflicts(left: &str, right: &str) -> bool {
    left.is_empty() || right.is_empty() || left == right
}

fn row_id(command: &str, key: &str, when: &str) -> String {
    format!("{command}\0{key}\0{when}")
}

/// `keybindingConflictLabels`.
pub fn conflict_labels(
    rows: &[Row],
    row_id: &str,
    key: &str,
    when: &str,
    mac: bool,
) -> Vec<String> {
    if key.trim().is_empty() {
        return Vec::new();
    }
    let Some(shortcut) = parse_shortcut(key) else {
        return Vec::new();
    };
    let wanted = conflict_key(&shortcut, mac);
    let labels: BTreeSet<String> = rows
        .iter()
        .filter(|candidate| {
            candidate.id != row_id
                && conflict_key(&candidate.shortcut, mac) == wanted
                && when_conflicts(&candidate.when, when)
        })
        .map(|candidate| command_label(&candidate.command))
        .collect();
    labels.into_iter().collect()
}

fn usage_rank(command: &str) -> Option<usize> {
    USAGE_ORDER.iter().position(|ranked| *ranked == command)
}

/// `localeCompare` without the ICU tables: case-insensitive first, then case.
fn locale_compare(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| right.cmp(left))
}

fn compare_commands(left: &str, right: &str, key: impl Fn(&str) -> String) -> std::cmp::Ordering {
    match (usage_rank(left), usage_rank(right)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (a, b) => locale_compare(
            &key(if a.is_none() { left } else { "usage.cost" }),
            &key(if b.is_none() { right } else { "usage.cost" }),
        ),
    }
}

/// `buildKeybindingRows`.
pub fn build_rows(rules: &[Rule], mac: bool) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for rule in rules {
        let default = default_for(rule);
        let key = shortcut_input(&rule.shortcut);
        let when = when_expression(rule.when.as_ref());
        let id = row_id(&rule.command, &key, &when);
        let row = Row {
            id: id.clone(),
            command: rule.command.clone(),
            key,
            when,
            source: source_of(rule),
            default_key: default.map(|entry| shortcut_input(&entry.shortcut)),
            default_when: when_expression(default.and_then(|entry| entry.when.as_ref())),
            shortcut: rule.shortcut.clone(),
            conflicts: Vec::new(),
        };
        // A later rule with the same id replaces the earlier one in place.
        match rows.iter_mut().find(|existing| existing.id == id) {
            Some(existing) => *existing = row,
            None => rows.push(row),
        }
    }
    let snapshot = rows.clone();
    for row in &mut rows {
        row.conflicts = conflict_labels(&snapshot, &row.id, &row.key, &row.when, mac);
    }
    rows.sort_by(|left, right| {
        compare_commands(&left.command, &right.command, str::to_owned)
            .then_with(|| locale_compare(&left.key, &right.key))
    });
    rows
}

/// `filterKeybindingRows`.
pub fn filter_rows<'a>(rows: &'a [Row], query: &str) -> Vec<&'a Row> {
    let query = query.trim().to_lowercase();
    rows.iter()
        .filter(|row| {
            query.is_empty()
                || row.command.to_lowercase().contains(&query)
                || command_label(&row.command).to_lowercase().contains(&query)
                || row.key.to_lowercase().contains(&query)
                || row.when.to_lowercase().contains(&query)
                || row.source.label().to_lowercase().contains(&query)
        })
        .collect()
}

const GROUPS: [(&str, &[&str]); 7] = [
    (
        "Navigation",
        &[
            "sidebar",
            "rightPanel",
            "commandPalette",
            "filePicker",
            "projectSearch",
            "editor",
        ],
    ),
    ("Threads", &["thread", "chat", "pullRequest"]),
    ("Composer", &["composer", "modelPicker"]),
    ("Terminal", &["terminal"]),
    ("Preview & diff", &["preview", "diff"]),
    ("Appearance", &["theme", "appearance", "themeEditor"]),
    ("Project scripts", &["script"]),
];

/// `groupKeybindingRows`: the groups that have rows, in the web's order.
pub fn group_rows<'a>(rows: &[&'a Row]) -> Vec<(&'static str, Vec<&'a Row>)> {
    let mut groups: Vec<(&'static str, Vec<&'a Row>)> = GROUPS
        .iter()
        .map(|(title, _)| (*title, Vec::new()))
        .chain([("Other", Vec::new())])
        .collect();
    for row in rows {
        let prefix = row.command.split('.').next().unwrap_or("");
        let index = GROUPS
            .iter()
            .position(|(_, prefixes)| prefixes.contains(&prefix))
            .unwrap_or(GROUPS.len());
        groups[index].1.push(row);
    }
    groups.retain(|(_, rows)| !rows.is_empty());
    groups
}

fn title_case(segment: &str) -> String {
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for c in segment.chars() {
        if c.is_ascii_uppercase()
            && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
        {
            spaced.push(' ');
        }
        spaced.push(c);
        previous = Some(c);
    }
    spaced
        .split(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `commandLabel`.
pub fn command_label(command: &str) -> String {
    let special = match command {
        "workspace.focusNext" => "Focus Next Workspace Area",
        "workspace.focusPrevious" => "Focus Previous Workspace Area",
        "composer.sendAlternate" => "Composer: Opposite Queue or Steer Action",
        "composer.sendBackground" => "Composer: Start in Background",
        "composer.sendAndNewThread" => "Composer: Send and Start New Thread",
        "composer.dictation" => "Composer: Start or Finish Dictation",
        "thread.steerQueuedMessage" => "Queue: Send First Queued Message",
        "thread.editQueuedMessage" => "Queue: Edit Last Queued Message",
        "thread.copyReference" => "Pull Request: Copy Link or Thread",
        "usage.cost" => "Usage: Cost",
        "usage.tokens" => "Usage: Tokens",
        "usage.limits" => "Usage: Limits",
        "usage.period.day" => "Usage: Period: Past 24h",
        "usage.period.week" => "Usage: Period: Past 7 days",
        "usage.period.month" => "Usage: Period: Past 30 days",
        "usage.period.quarter" => "Usage: Period: Past 90 days",
        "view.reopenClosed" => "Reopen Closed Tab",
        "preview.focusUrl" => "Preview: Focus URL",
        _ => "",
    };
    if !special.is_empty() {
        return special.to_owned();
    }
    if let Some(id) = command
        .strip_prefix("script.")
        .and_then(|rest| rest.strip_suffix(".run"))
    {
        return format!("Run Script: {}", title_case(id));
    }
    command
        .split('.')
        .map(title_case)
        .collect::<Vec<_>>()
        .join(": ")
}

/// `buildKeybindingCommandOptions`: the contract's commands, the jump commands and every command
/// in the config, by label.
pub fn command_options(rules: &[Rule]) -> Vec<String> {
    let mut commands: Vec<String> = STATIC_COMMANDS.iter().map(|c| (*c).to_owned()).collect();
    for prefix in ["thread", "modelPicker"] {
        commands.extend((1..=9).map(|n| format!("{prefix}.jump.{n}")));
    }
    for rule in rules {
        if !commands.contains(&rule.command) {
            commands.push(rule.command.clone());
        }
    }
    commands.sort_by(|left, right| compare_commands(left, right, command_label));
    commands
}

/// `keybindingValueForCommand`: the last rule of `command`, as input text.
pub fn value_for_command(rules: &[Rule], command: &str) -> Option<String> {
    rules
        .iter()
        .rev()
        .find(|rule| rule.command == command)
        .map(|rule| {
            let s = &rule.shortcut;
            let mut parts: Vec<&str> = Vec::new();
            if s.modifier {
                parts.push("mod");
            }
            if s.ctrl {
                parts.push("ctrl");
            }
            if s.meta {
                parts.push("meta");
            }
            if s.alt {
                parts.push("alt");
            }
            if s.shift {
                parts.push("shift");
            }
            parts.push(match s.key.as_str() {
                " " => "space",
                "escape" => "esc",
                other => other,
            });
            parts.join("+")
        })
}

/// One piece of a shortcut as the pill shows it (`KeybindingPill`).
pub fn pill_parts(input: &str, mac: bool) -> Vec<String> {
    let mut tokens: Vec<&str> = input.split('+').collect();
    let mut trailing = 0;
    while tokens.last() == Some(&"") {
        tokens.pop();
        trailing += 1;
    }
    if trailing > 0 {
        tokens.push("+");
    }
    tokens
        .into_iter()
        .map(|token| match token.to_lowercase().as_str() {
            "mod" if mac => "\u{2318}".to_owned(),
            "mod" => "Ctrl".to_owned(),
            "meta" | "cmd" if mac => "\u{2318}".to_owned(),
            "meta" | "cmd" => "Meta".to_owned(),
            "shift" => "\u{21e7}".to_owned(),
            "alt" | "option" if mac => "\u{2325}".to_owned(),
            "alt" | "option" => "Alt".to_owned(),
            "ctrl" | "control" if mac => "\u{2303}".to_owned(),
            "ctrl" | "control" => "Ctrl".to_owned(),
            "arrowup" => "\u{2191}".to_owned(),
            "arrowdown" => "\u{2193}".to_owned(),
            "arrowleft" => "\u{2190}".to_owned(),
            "arrowright" => "\u{2192}".to_owned(),
            "esc" | "escape" => "Esc".to_owned(),
            other if other.chars().count() == 1 => other.to_uppercase(),
            other => title_case(other),
        })
        .collect()
}

/// The key token of a pressed key, as the web's `keybindingFromKeyboardEvent` names it; `None`
/// for a modifier on its own.
fn key_token(key: &str) -> Option<String> {
    let token = match key {
        "shift" | "control" | "alt" | "platform" | "function" | "" => return None,
        "up" => "arrowup",
        "down" => "arrowdown",
        "left" => "arrowleft",
        "right" => "arrowright",
        other => other,
    };
    Some(token.to_lowercase())
}

/// A pressed key as the input text a rule stores (`mod+k`), for the shortcut recorder.
pub fn record_input(
    key: &str,
    (control, alt, shift, platform): (bool, bool, bool, bool),
    mac: bool,
) -> Option<String> {
    let token = key_token(key)?;
    let (modifier, meta, ctrl) = if mac {
        (platform, false, control)
    } else {
        (control, platform, false)
    };
    let mut parts: Vec<&str> = Vec::new();
    if modifier {
        parts.push("mod");
    }
    if meta {
        parts.push("meta");
    }
    if ctrl {
        parts.push("ctrl");
    }
    if alt {
        parts.push("alt");
    }
    if shift {
        parts.push("shift");
    }
    parts.push(&token);
    Some(parts.join("+"))
}

/// The `command`/`key`/`when` target of a rule on the wire (`ServerRemoveKeybindingInput`).
pub fn target_payload(command: &str, key: &str, when: &str) -> serde_json::Value {
    let mut target = serde_json::json!({ "command": command, "key": key });
    if !when.trim().is_empty() {
        target["when"] = serde_json::Value::String(when.to_owned());
    }
    target
}

/// `saveKeybinding`'s payload: the key and `when` trimmed, `replace` as the row had them.
pub fn upsert_payload(
    command: &str,
    key: &str,
    when: &str,
    replace: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut payload = target_payload(command, key.trim(), when.trim());
    if let Some(replace) = replace {
        payload["replace"] = replace;
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_round_trip() {
        for input in ["mod+b", "mod+shift+f6", "mod++", "alt+arrowup", "c"] {
            let shortcut = parse_shortcut(input).unwrap();
            assert_eq!(shortcut_input(&shortcut), input);
        }
        assert!(parse_shortcut("a+b").is_none());
        assert_eq!(parse_shortcut("cmd+space").unwrap().key, " ");
    }

    #[test]
    fn when_expressions_parse_and_print() {
        let node = parse_when("!terminalFocus && (a || b)").unwrap();
        assert_eq!(when_expression(Some(&node)), "!terminalFocus && (a || b)");
        assert!(parse_when("a &&").is_none());
        assert!(parse_when("(a").is_none());
        assert_eq!(parse_when_draft("  "), Ok(None));
        assert!(parse_when_draft("a b").is_err());
    }

    #[test]
    fn defaults_all_compile() {
        let compiled = default_rules().len();
        let listed = DEFAULTS.len() + USAGE_DEFAULTS.len() + 18;
        assert_eq!(compiled, listed);
    }

    #[test]
    fn rows_mark_source_default_and_conflicts() {
        let rows = build_rows(&merge_with_defaults(&[]), true);
        assert!(rows.iter().all(|row| row.source == Source::Default));
        let toggle = rows.iter().find(|r| r.command == "sidebar.toggle").unwrap();
        assert!(toggle.conflicts.is_empty());
        // mod+shift+d is bound twice with when clauses that cannot both hold, but the web only
        // compares equality or emptiness, so they differ and do not conflict.
        let split = rows
            .iter()
            .find(|r| r.command == "terminal.splitVertical")
            .unwrap();
        assert!(split.conflicts.is_empty());
        // mod+d: terminal.split has `terminalFocus`, diff.toggle `!terminalFocus`: no conflict.
        // mod+n: chat.new and terminal.new likewise.
        let custom = Rule {
            command: "sidebar.toggle".to_owned(),
            shortcut: parse_shortcut("mod+k").unwrap(),
            when: None,
        };
        let rows = build_rows(&merge_with_defaults(&[custom]), true);
        let toggle = rows.iter().find(|r| r.command == "sidebar.toggle").unwrap();
        assert_eq!(toggle.source, Source::Custom);
        assert_eq!(toggle.default_key.as_deref(), Some("mod+b"));
        assert_eq!(toggle.conflicts, vec!["Command Palette: Toggle".to_owned()]);
    }

    #[test]
    fn labels_follow_the_web() {
        assert_eq!(command_label("sidebar.toggle"), "Sidebar: Toggle");
        assert_eq!(command_label("thread.jump.3"), "Thread: Jump: 3");
        assert_eq!(
            command_label("script.run-tests.run"),
            "Run Script: Run Tests"
        );
        assert_eq!(
            command_label("usage.period.week"),
            "Usage: Period: Past 7 days"
        );
        assert_eq!(command_label("composer.cycleHost"), "Composer: Cycle Host");
    }

    #[test]
    fn groups_keep_the_web_order() {
        let rows = build_rows(&merge_with_defaults(&[]), true);
        let refs: Vec<&Row> = rows.iter().collect();
        let titles: Vec<_> = group_rows(&refs).into_iter().map(|g| g.0).collect();
        assert_eq!(titles.first(), Some(&"Navigation"));
        assert_eq!(titles.last(), Some(&"Other"));
    }

    #[test]
    fn pills_show_symbols_on_mac() {
        assert_eq!(
            pill_parts("mod+shift+k", true),
            ["\u{2318}", "\u{21e7}", "K"]
        );
        assert_eq!(pill_parts("mod++", false), ["Ctrl", "+"]);
    }

    #[test]
    fn unknown_variables_are_listed() {
        let node = parse_when("terminalFocus && mystery").unwrap();
        assert_eq!(unknown_variables(Some(&node)), vec!["mystery".to_owned()]);
    }

    #[test]
    fn records_pressed_keys_like_the_web() {
        assert_eq!(
            record_input("k", (false, false, false, true), true).as_deref(),
            Some("mod+k")
        );
        assert_eq!(
            record_input("k", (true, false, true, false), true).as_deref(),
            Some("ctrl+shift+k")
        );
        assert_eq!(
            record_input("k", (true, false, false, false), false).as_deref(),
            Some("mod+k")
        );
        assert_eq!(
            record_input("up", (false, true, false, false), false).as_deref(),
            Some("alt+arrowup")
        );
        assert_eq!(
            record_input("shift", (false, false, true, false), true),
            None
        );
    }

    #[test]
    fn payloads_trim_and_omit_empty_when() {
        let replace = target_payload("terminal.toggle", "mod+j", "");
        assert_eq!(
            upsert_payload("terminal.toggle", " mod+k ", "  ", Some(replace.clone())),
            serde_json::json!({
                "command": "terminal.toggle",
                "key": "mod+k",
                "replace": { "command": "terminal.toggle", "key": "mod+j" },
            })
        );
        assert_eq!(
            target_payload("a", "mod+k", "terminalFocus"),
            serde_json::json!({ "command": "a", "key": "mod+k", "when": "terminalFocus" })
        );
    }
}
