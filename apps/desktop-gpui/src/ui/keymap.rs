//! The GPUI key bindings the server's keybindings produce. Each web command id maps to the GPUI
//! action that does the same thing; a command without one binds nothing.

use super::usage_dialog::{
    PastDay, PastMonth, PastQuarter, PastWeek, ShowCost, ShowLimits, ShowTokens,
};
use super::*;
use crate::keybindings::{Identifier, Keybindings, Predicate};
use gpui::{Action, KeyBinding, KeyBindingContextPredicate, KeyBindingMetaIndex};
use std::rc::Rc;

/// Marks the bindings this module owns, so a new set replaces them and nothing else.
const SOURCE: KeyBindingMetaIndex = KeyBindingMetaIndex(0x7e3);

const MAC: bool = cfg!(target_os = "macos");

/// The action of a web command id.
fn action_for(command: &str) -> Option<Box<dyn Action>> {
    Some(match command {
        "sidebar.toggle" => Box::new(ToggleSidebar),
        "navigation.back" => Box::new(GoBack),
        "navigation.forward" => Box::new(GoForward),
        "terminal.toggle" => Box::new(ToggleTerminal),
        "terminal.split" => Box::new(SplitTerminal),
        "terminal.splitVertical" => Box::new(SplitTerminalVertical),
        "terminal.new" => Box::new(NewTerminal),
        "terminal.close" => Box::new(CloseTerminal),
        "rightPanel.toggle" => Box::new(ToggleRightPanel),
        "threadPanel.toggle" => Box::new(ToggleDetails),
        "chat.new" => Box::new(NewThread),
        "chat.newWithoutProject" => Box::new(NewScratchThread),
        "editor.openFavorite" => Box::new(OpenFavoriteEditor),
        "usage.open" => Box::new(OpenUsage),
        "usage.cost" => Box::new(ShowCost),
        "usage.tokens" => Box::new(ShowTokens),
        "usage.limits" => Box::new(ShowLimits),
        "usage.period.day" => Box::new(PastDay),
        "usage.period.week" => Box::new(PastWeek),
        "usage.period.month" => Box::new(PastMonth),
        "usage.period.quarter" => Box::new(PastQuarter),
        "composer.workspace" => Box::new(OpenWorkspace),
        "composer.previousWorktree" => Box::new(UsePreviousWorktree),
        "composer.mode" => Box::new(OpenPermissions),
        _ => return None,
    })
}

/// How this client answers a `when` identifier: the GPUI key context standing in for it, or a
/// constant. Identifiers no view sets a context for are false, like the web's unknown ones.
fn identify(name: &str) -> Identifier {
    match name {
        "true" | "isDesktop" => Identifier::Always,
        "false" | "isWeb" => Identifier::Never,
        "terminalFocus" => Identifier::Context("Terminal".to_owned()),
        "usagePageOpen" => Identifier::Context(usage_dialog::CONTEXT.to_owned()),
        other
            if other
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-') =>
        {
            Identifier::Context(other.to_owned())
        }
        _ => Identifier::Never,
    }
}

/// The bindings of `keybindings`, in the web's order: a later rule outranks an earlier one.
fn build(keybindings: &Keybindings, cx: &App) -> Vec<KeyBinding> {
    let mut mapped = Vec::new();
    for rule in &keybindings.rules {
        let predicate = match rule
            .when
            .as_ref()
            .map_or(Predicate::Always, |when| when.fold(&identify))
        {
            Predicate::Never => continue,
            Predicate::Always => None,
            Predicate::Expr(expr) => match KeyBindingContextPredicate::parse(&expr) {
                Ok(parsed) => Some(Rc::new(parsed)),
                Err(error) => {
                    eprintln!(
                        "[keybindings] unreadable when for {}: {error}",
                        rule.command
                    );
                    continue;
                }
            },
        };
        mapped.push((rule, action_for(&rule.command), predicate));
    }
    // A rule of a command with no action still shadows the earlier rules of its shortcut, so it
    // binds nothing to the key rather than letting an older binding fire.
    let claimed = |shortcut: &crate::keybindings::Shortcut| {
        mapped
            .iter()
            .any(|(rule, action, _)| action.is_some() && rule.shortcut == *shortcut)
    };
    let mut bindings = Vec::new();
    for (rule, action, predicate) in &mapped {
        let action = match action {
            Some(action) => action.boxed_clone(),
            None if claimed(&rule.shortcut) => Box::new(gpui::NoAction),
            None => continue,
        };
        match KeyBinding::load(
            &rule.shortcut.keystroke(MAC),
            action,
            predicate.clone(),
            false,
            None,
            cx,
        ) {
            Ok(binding) => bindings.push(binding.with_meta(SOURCE)),
            Err(error) => eprintln!("[keybindings] cannot bind {}: {error}", rule.command),
        }
    }
    bindings
}

fn install(bindings: Vec<KeyBinding>, cx: &mut App) {
    let keymap = cx.key_bindings();
    let others = keymap
        .borrow()
        .bindings()
        .filter(|binding| binding.meta() != Some(SOURCE))
        .cloned()
        .collect::<Vec<_>>();
    cx.clear_key_bindings();
    cx.bind_keys(others);
    cx.bind_keys(bindings);
}

/// Replaces the keybinding-driven bindings. A server that sends no rules gets the defaults.
pub fn apply(keybindings: &Keybindings, cx: &mut App) {
    if keybindings.rules.is_empty() {
        return apply_defaults(cx);
    }
    let bindings = build(keybindings, cx);
    install(bindings, cx);
}

/// The web's default keybindings for the commands this client has, for before the server answers.
pub fn apply_defaults(cx: &mut App) {
    let bindings = vec![
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-u", OpenUsage, Some("!Terminal")),
        KeyBinding::new("shift-cmd-a", OpenPermissions, Some("!Terminal")),
        KeyBinding::new("shift-cmd-x", OpenWorkspace, Some("!Terminal")),
        KeyBinding::new("shift-cmd-l", UsePreviousWorktree, Some("!Terminal")),
        KeyBinding::new("shift-cmd-b", ToggleDetails, Some("!Terminal")),
        KeyBinding::new("cmd-j", ToggleTerminal, None),
        KeyBinding::new("alt-cmd-b", ToggleRightPanel, None),
        KeyBinding::new("cmd-o", OpenFavoriteEditor, None),
        KeyBinding::new("cmd-n", NewThread, Some("!Terminal")),
        KeyBinding::new("alt-cmd-n", NewScratchThread, Some("!Terminal")),
        KeyBinding::new("cmd-[", GoBack, Some("!Terminal")),
        KeyBinding::new("cmd-]", GoForward, Some("!Terminal")),
        KeyBinding::new("cmd-d", SplitTerminal, Some("Terminal")),
        KeyBinding::new("shift-cmd-d", SplitTerminalVertical, Some("Terminal")),
        KeyBinding::new("cmd-n", NewTerminal, Some("Terminal")),
        KeyBinding::new("cmd-w", CloseTerminal, Some("Terminal")),
        KeyBinding::new("c", ShowCost, Some(usage_dialog::CONTEXT)),
        KeyBinding::new("t", ShowTokens, Some(usage_dialog::CONTEXT)),
        KeyBinding::new("l", ShowLimits, Some(usage_dialog::CONTEXT)),
        KeyBinding::new("shift-cmd-1", PastDay, Some(usage_dialog::CONTEXT)),
        KeyBinding::new("shift-cmd-2", PastWeek, Some(usage_dialog::CONTEXT)),
        KeyBinding::new("shift-cmd-3", PastMonth, Some(usage_dialog::CONTEXT)),
        KeyBinding::new("shift-cmd-4", PastQuarter, Some(usage_dialog::CONTEXT)),
    ];
    install(
        bindings
            .into_iter()
            .map(|binding| binding.with_meta(SOURCE))
            .collect(),
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_predicate_the_fold_makes_parses() {
        for expr in [
            "Terminal",
            "!(Terminal)",
            "(a) && (!(b))",
            "(a) || (UsagePage)",
        ] {
            assert!(KeyBindingContextPredicate::parse(expr).is_ok(), "{expr}");
        }
    }

    #[test]
    fn commands_map_to_actions() {
        assert!(action_for("sidebar.toggle").is_some());
        assert!(action_for("modelPicker.jump.1").is_none());
    }
}
