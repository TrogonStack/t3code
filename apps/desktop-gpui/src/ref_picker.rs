//! The branch ref picker's popover (`BranchPicker.tsx`): the search box, the ref rows with their
//! tags, the create row and the paging footer. Shared by the details card and the composer's
//! branch control; each owns its refs and decides what a click does.

use crate::details_card::tip;
use crate::refs::{self, Page, Tag};
use gpui::{
    AnyElement, App, Entity, Focusable as _, InteractiveElement as _, IntoElement, ParentElement,
    ScrollHandle, StatefulInteractiveElement as _, Styled, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, h_flex,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit_assets::IconName as Icons;
use std::rc::Rc;

/// What a ref picker's popover draws, snapshotted at each render of its owner. The owner decides
/// what picking, creating and paging do.
pub struct RefMenu {
    pub query: String,
    pub page: Page,
    pub loading: bool,
    pub error: Option<String>,
    pub cwd: String,
    pub project_cwd: String,
    pub search: Entity<InputState>,
    pub scroll: ScrollHandle,
    /// Whether a typed name offers to create a ref.
    pub can_create: bool,
    /// A row was clicked: its name and whether it is the current ref.
    pub on_pick: Rc<dyn Fn(String, bool, &mut App)>,
    pub on_create: Rc<dyn Fn(String, &mut App)>,
    /// The list scrolled near its end with more refs to load.
    pub on_more: Rc<dyn Fn(&mut App)>,
}

impl RefMenu {
    pub fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = cx.theme();
        let (muted, border, ring) = (theme.muted_foreground, theme.border, theme.ring);
        let (hover, selected) = (theme.list_hover, theme.list_active);
        let searching = self.search.focus_handle(cx).is_focused(window);
        let sanitized = refs::sanitize_new_ref_name(&self.query);
        let creatable = self.can_create
            && !sanitized.is_empty()
            && !self.page.refs.iter().any(|r| r.name == sanitized);
        let rows = self.page.refs.iter().enumerate().map(|(index, entry)| {
            let tag = refs::tag(entry, &self.project_cwd);
            let elsewhere = !entry.current
                && entry
                    .worktree_path
                    .as_deref()
                    .is_some_and(|path| path != self.cwd);
            let (on_pick, name) = (self.on_pick.clone(), entry.name.clone());
            let current = entry.current;
            let label = match refs::split_for_middle_truncate(&entry.name, None) {
                Some((head, tail)) => h_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().min_w_0().truncate().child(head))
                    .child(div().flex_shrink_0().child(tail))
                    .into_any_element(),
                None => div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(entry.name.clone())
                    .into_any_element(),
            };
            h_flex()
                .id(("ref", index))
                .gap_2()
                .px_2()
                .py_1p5()
                .items_center()
                .rounded_md()
                .text_sm()
                .when(current, |row| row.bg(selected))
                .when(elsewhere, |row| {
                    row.opacity(0.5)
                        .tooltip(tip("This ref is checked out in another worktree."))
                })
                .when(!elsewhere, |row| {
                    row.cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(move |_, _, cx| on_pick(name.clone(), current, cx))
                })
                .child(label)
                .children(tag.map(|tag: Tag| {
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(tag.label())
                }))
        });
        let create = creatable.then(|| {
            let (on_create, name) = (self.on_create.clone(), sanitized.clone());
            h_flex()
                .id("ref-create")
                .gap_2()
                .px_2()
                .py_1p5()
                .items_center()
                .rounded_md()
                .text_sm()
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .child(Icon::new(Icons::GitBranchPlus).size_4().text_color(muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(format!("Create new ref \"{sanitized}\"")),
                )
                .on_click(move |_, _, cx| on_create(name.clone(), cx))
        });
        let more = (self.page.has_more && !self.loading).then(|| self.on_more.clone());
        let scroll = self.scroll.clone();
        let footer = self.page.status(self.loading);
        let empty = self.page.refs.is_empty() && !self.loading && create.is_none();
        v_flex()
            .w(px(288.))
            .rounded_lg()
            .overflow_hidden()
            .child(
                div().px_3().pt(px(10.)).child(
                    h_flex()
                        .pb(px(6.))
                        .border_b_1()
                        .border_color(if searching { ring } else { border.opacity(0.7) })
                        .child(
                            Input::new(&self.search)
                                .appearance(false)
                                .h(px(26.))
                                .px_0()
                                .prefix(
                                    Icon::new(IconName::Search)
                                        .size_4()
                                        .text_color(muted.opacity(0.55)),
                                ),
                        ),
                ),
            )
            .child(
                v_flex()
                    .id("ref-list")
                    .max_h(px(280.))
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .on_scroll({
                        let scroll = scroll.clone();
                        move |_, _, cx| {
                            let Some(more) = &more else { return };
                            let offset = scroll.offset().y;
                            let remaining = scroll.max_offset().y + offset;
                            if remaining < px(160.) {
                                more(cx);
                            }
                        }
                    })
                    .p_1()
                    .children(create)
                    .children(rows)
                    .when(empty, |list| {
                        list.child(
                            div().p_2().text_sm().text_color(muted).child(
                                self.error
                                    .clone()
                                    .unwrap_or_else(|| "No matching refs.".to_owned()),
                            ),
                        )
                    }),
            )
            .children(footer.map(|text| {
                div()
                    .px_3()
                    .py_1p5()
                    .border_t_1()
                    .border_color(border)
                    .text_xs()
                    .text_color(muted)
                    .child(text)
            }))
            .into_any_element()
    }
}
