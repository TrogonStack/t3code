//! The web's `BranchToolbarEnvironmentSelector`: the "Run on" control left of the workspace
//! control. With one environment it is the web's locked label; with several it is a select whose
//! rows name the environments that have the project, plus "Add to another machine".
//!
//! A GPUI draft can only run on the environment the window is attached to, and the app has no
//! add-project-to-machine flow, so the other environments and "Add to another machine" render as
//! the web draws them but do nothing (the web disables a row without `onEnvironmentChange`).

use super::composer_menu::{self, OnClick, OnHover, Row, RowState};
use super::*;
use crate::projects::RemoteMember;

/// The rows of the menu: this environment first, then the others that have the project.
pub(super) struct Env {
    pub label: String,
    pub machine: MachineKind,
}

pub(super) struct Menu {
    pub view: Entity<MainView>,
    pub current: Env,
    pub others: Vec<RemoteMember>,
    pub highlight: usize,
}

/// `hasMultipleEnvironments`: the select exists only when another environment has the project.
pub(super) fn is_select(others: &[RemoteMember]) -> bool {
    !others.is_empty()
}

/// Rows before "Add to another machine": the environments, current first.
fn environments(menu: &Menu) -> Vec<(String, MachineKind)> {
    std::iter::once((menu.current.label.clone(), menu.current.machine))
        .chain(
            menu.others
                .iter()
                .map(|other| (other.label.clone(), other.machine)),
        )
        .collect()
}

/// The index `highlight` takes for "Add to another machine".
fn add_index(menu: &Menu) -> usize {
    1 + menu.others.len()
}

pub(super) fn popover(menu: Menu, open: bool, shortcut: &str) -> AnyElement {
    let label = menu.current.label.clone();
    let machine = menu.current.machine;
    let view = menu.view.clone();
    Popover::new("host")
        .anchor(Anchor::BottomLeft)
        .offset(px(4.))
        .open(open)
        .p_0()
        .appearance(false)
        .on_open_change(move |open, window, cx| {
            view.update(cx, |this, cx| this.set_host_open(*open, window, cx));
        })
        .trigger(composer_menu::Trigger {
            id: "host",
            icon: machine_icon(machine),
            tip: strip::tooltip_text(&label, Some(shortcut)),
            label: label.into(),
            open,
        })
        .content(move |_, _, cx| content(&menu, cx))
        .into_any_element()
}

fn content(menu: &Menu, cx: &App) -> gpui::Div {
    let close = |view: &Entity<MainView>| {
        let view = view.clone();
        Box::new(move |window: &mut Window, cx: &mut App| {
            view.update(cx, |this, cx| this.set_host_open(false, window, cx));
        }) as OnClick
    };
    let hover = |view: &Entity<MainView>, index: usize| {
        let view = view.clone();
        Box::new(move |cx: &mut App| {
            view.update(cx, |this, cx| this.highlight_host(index, cx));
        }) as OnHover
    };
    let mut rows: Vec<Row> = environments(menu)
        .into_iter()
        .enumerate()
        .map(|(index, (label, machine))| Row {
            id: ("host-row", index),
            state: RowState {
                selected: index == 0,
                lit: index == menu.highlight,
                // Retargeting a draft is not supported here, as without `onEnvironmentChange`.
                disabled: index != 0,
                quiet: false,
            },
            body: composer_menu::plain_body(machine_icon(machine), label, cx).into_any_element(),
            on_hover: hover(&menu.view, index),
            on_click: close(&menu.view),
        })
        .collect();
    let add = add_index(menu);
    rows.push(Row {
        id: ("host-row", add),
        state: RowState {
            selected: false,
            lit: add == menu.highlight,
            // The add-project-to-machine dialog is not ported, so the row only closes the menu.
            disabled: false,
            quiet: true,
        },
        body: composer_menu::plain_body(Icons::Plus, "Add to another machine", cx)
            .into_any_element(),
        on_hover: hover(&menu.view, add),
        on_click: close(&menu.view),
    });
    composer_menu::popup("Run on", None, rows, cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_select_needs_another_environment() {
        assert!(!is_select(&[]));
        assert!(is_select(&[RemoteMember {
            label: "Other".to_owned(),
            machine: MachineKind::Cloud,
        }]));
    }
}
