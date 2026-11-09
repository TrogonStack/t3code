//! The web's `BranchToolbarEnvModeSelector`: the Workspace select, in the shared composer menu
//! look (`composer_menu`), with `PreviousWorktreeItemContent.tsx` for the Previous worktree row.

use super::composer_menu::{self, Row, RowState};
use super::settings::tokens::Tokens;
use super::*;
use crate::workspace::{Choice, Draft, Glyph, Seed};

pub(super) fn icon(glyph: Glyph) -> Icons {
    match glyph {
        Glyph::Folder => Icons::Folder,
        Glyph::FolderGit => Icons::FolderGit,
        Glyph::FolderGit2 => Icons::FolderGit2,
        Glyph::History => Icons::RotateCcwClock,
    }
}

/// What the menu needs from the draft it edits.
pub(super) struct Menu {
    pub view: Entity<MainView>,
    pub draft: Draft,
    pub previous: Option<Seed>,
    pub highlight: usize,
}

pub(super) fn popover(menu: Menu, open: bool, shortcut: &str) -> AnyElement {
    let (glyph, label) = menu.draft.trigger();
    let view = menu.view.clone();
    Popover::new("workspace")
        .anchor(Anchor::BottomLeft)
        .offset(px(4.))
        .open(open)
        .p_0()
        .appearance(false)
        .on_open_change(move |open, window, cx| {
            view.update(cx, |this, cx| this.set_workspace_open(*open, window, cx));
        })
        .trigger(composer_menu::Trigger {
            id: "workspace",
            icon: icon(glyph),
            label: label.into(),
            tip: strip::tooltip_text(label, Some(shortcut)),
            open,
        })
        .content(move |_, _, cx| content(&menu, cx))
        .into_any_element()
}

/// `w-[min(21rem,...)]` once Previous worktree is offered.
fn content(menu: &Menu, cx: &App) -> gpui::Div {
    let selected = menu.draft.selected();
    let rows = Draft::rows(menu.previous.is_some())
        .into_iter()
        .enumerate()
        .map(|(index, choice)| {
            let (click_view, hover_view) = (menu.view.clone(), menu.view.clone());
            let body = match (choice, &menu.previous) {
                (Choice::Previous, Some(seed)) => previous_body(seed, cx).into_any_element(),
                _ => composer_menu::plain_body(
                    icon(menu.draft.glyph(choice)),
                    menu.draft.label(choice),
                    cx,
                )
                .into_any_element(),
            };
            Row {
                id: ("workspace-row", index),
                state: RowState {
                    selected: choice == selected,
                    lit: index == menu.highlight,
                    ..RowState::default()
                },
                body,
                on_hover: Box::new(move |cx| {
                    hover_view.update(cx, |this, cx| this.highlight_workspace(index, cx));
                }),
                on_click: Box::new(move |window, cx| {
                    click_view.update(cx, |this, cx| this.pick_workspace(choice, window, cx));
                }),
            }
        })
        .collect();
    composer_menu::popup(
        "Workspace",
        menu.previous.is_some().then(|| px(338.)),
        rows,
        cx,
    )
}

/// `PreviousWorktreeItemContent`: the history icon at `mt-1`, the label, and the branch beneath
/// it in `text-xs text-muted-foreground`.
fn previous_body(seed: &Seed, cx: &App) -> gpui::Div {
    let muted = Tokens::of(cx).muted_foreground;
    h_flex()
        .min_w_0()
        .items_start()
        .gap(px(6.))
        .child(
            div()
                .mt(px(4.))
                .text_color(muted)
                .child(Icon::new(icon(Glyph::History)).size(px(12.))),
        )
        .child(
            v_flex()
                .min_w_0()
                .child(
                    div()
                        .whitespace_nowrap()
                        .child(Draft::default().label(Choice::Previous)),
                )
                .children(seed.branch.as_deref().map(|branch| {
                    div()
                        .min_w_0()
                        .text_xs()
                        .line_height(px(16.))
                        .text_color(muted)
                        .whitespace_nowrap()
                        .child(middle_truncate(branch, BRANCH_BUDGET))
                })),
        )
}

/// The characters of a branch that fit the `w-[21rem]` row at `text-xs`.
const BRANCH_BUDGET: usize = 44;
