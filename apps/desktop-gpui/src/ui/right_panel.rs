//! The web's right panel (`ChatRightPanel.tsx`, `RightPanelTabs.tsx`, `rightPanelStore.ts`) with
//! its Diff surface (`DiffPanel.tsx`). Only that surface is here; files, browser, device, pull
//! requests and side chats have no counterpart yet, so the tab strip lists Diff alone.

use super::*;
use crate::diff::{self, Change, Kind, Preview};
use gpui::{ScrollStrategy, UniformListScrollHandle, uniform_list};
use std::{ops::Range, rc::Rc};

/// `usePreviewPanelInlineSize`: the default, minimum and cap of the docked panel's width, and
/// the width its sibling chat column keeps.
pub(super) const DEFAULT_WIDTH: f32 = 540.;
const MIN_WIDTH: f32 = 360.;
const MAX_WIDTH_FRACTION: f32 = 0.7;
const SIBLING_MIN_WIDTH: f32 = 360.;
/// `RIGHT_PANEL_INLINE_LAYOUT_MEDIA_QUERY`, `(max-width: 980px)`; narrower windows use a sheet.
const INLINE_BREAKPOINT: f32 = 980.;
/// `RightPanelSheet`: `w-[min(42vw,28rem)] min-w-80 max-w-[28rem]`, and below 760px
/// `w-[min(88vw,24rem)] min-w-0`.
const SHEET_MAX: f32 = 448.;
const SHEET_MIN: f32 = 320.;
const SHEET_COMPACT_BREAKPOINT: f32 = 760.;
const SHEET_COMPACT_MAX: f32 = 384.;
const RAIL_WIDTH: f32 = 16.;
/// The tab strip's `h-10`.
const STRIP_HEIGHT: f32 = 40.;
const ROW_HEIGHT: f32 = 24.;
const GUTTER_WIDTH: f32 = 44.;
const ADDITION: u32 = 0x22c55e;
const DELETION: u32 = 0xef4444;

/// The web's default `rightPanel.toggle` binding on macOS.
pub const TOGGLE_SHORTCUT: &str = "\u{2325}\u{2318}B";

/// What the panel can show; the web's `RightPanelKind` minus the surfaces not ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    Diff,
}

impl Surface {
    fn label(self) -> &'static str {
        match self {
            Self::Diff => "Diff",
        }
    }

    fn icon(self) -> Icons {
        match self {
            Self::Diff => Icons::FileDiff,
        }
    }
}

/// The docked width: at least the minimum, at most 70% of the window and what leaves the chat
/// column its own minimum. The cap never drops below the minimum.
fn clamp_width(width: f32, window_width: f32, container_width: f32) -> f32 {
    let cap = (window_width * MAX_WIDTH_FRACTION)
        .floor()
        .min(container_width.floor() - SIBLING_MIN_WIDTH);
    width.clamp(MIN_WIDTH, cap.max(MIN_WIDTH))
}

fn is_sheet(window_width: f32) -> bool {
    window_width <= INLINE_BREAKPOINT
}

fn sheet_width(window_width: f32) -> f32 {
    if window_width <= SHEET_COMPACT_BREAKPOINT {
        return (window_width * 0.88).min(SHEET_COMPACT_MAX);
    }
    (window_width * 0.42).clamp(SHEET_MIN, SHEET_MAX)
}

#[derive(Clone)]
struct RightPanelResize;

impl Render for RightPanelResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// One line of the flattened diff. Every row has the same height so the list can be uniform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    File(usize),
    Hunk(usize, usize),
    Line(usize, usize, usize),
    /// A file with nothing to list: binary, a rename, or a patch the server cut short.
    Empty(usize),
}

fn rows(preview: &Preview) -> Vec<Row> {
    let mut rows = Vec::new();
    for (file_ix, file) in preview.files.iter().enumerate() {
        rows.push(Row::File(file_ix));
        if file.hunks.is_empty() {
            rows.push(Row::Empty(file_ix));
        }
        for (hunk_ix, hunk) in file.hunks.iter().enumerate() {
            rows.push(Row::Hunk(file_ix, hunk_ix));
            rows.extend((0..hunk.lines.len()).map(|line| Row::Line(file_ix, hunk_ix, line)));
        }
    }
    rows
}

/// What the Diff surface has to show for the thread's directory.
#[derive(Clone)]
pub enum Load {
    Done(Rc<Preview>),
    Failed(String),
}

/// The Diff surface's scrolling body: a summary, then the files with their hunks.
pub struct DiffList {
    load: Option<Load>,
    rows: Vec<Row>,
    scroll: UniformListScrollHandle,
}

impl DiffList {
    pub fn new() -> Self {
        Self {
            load: None,
            rows: Vec::new(),
            scroll: UniformListScrollHandle::new(),
        }
    }

    /// Shows `load`, `None` while it is on its way. The same load again keeps the scroll place.
    fn show(&mut self, load: Option<&Load>, cx: &mut Context<Self>) {
        let same = match (&self.load, load) {
            (None, None) => true,
            (Some(Load::Done(a)), Some(Load::Done(b))) => Rc::ptr_eq(a, b),
            (Some(Load::Failed(a)), Some(Load::Failed(b))) => a == b,
            _ => false,
        };
        if same {
            return;
        }
        self.rows = match load {
            Some(Load::Done(preview)) => rows(preview),
            _ => Vec::new(),
        };
        self.load = load.cloned();
        if !self.rows.is_empty() {
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    fn note(text: impl Into<gpui::SharedString>, cx: &App) -> gpui::Div {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .px_6()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(text.into())
    }

    fn summary(preview: &Preview, cx: &App) -> impl IntoElement {
        let (additions, deletions) = preview.totals();
        let count = preview.files.len();
        let theme = cx.theme();
        h_flex()
            .h_8()
            .flex_shrink_0()
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(format!(
                "{count} changed {}",
                if count == 1 { "file" } else { "files" }
            ))
            .child(div().flex_1())
            .child(stat(additions, deletions))
    }

    fn row(&self, row: Row, cx: &App) -> AnyElement {
        let Some(Load::Done(preview)) = &self.load else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let base = h_flex().h(px(ROW_HEIGHT)).w_full().flex_shrink_0();
        match row {
            Row::File(file_ix) => {
                let file = &preview.files[file_ix];
                let (dir, name) = split_path(&file.path);
                let mut label = h_flex().flex_1().min_w_0().overflow_hidden().gap_1();
                if let Some(previous) = &file.previous_path {
                    label = label.child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(format!("{previous} \u{2192}")),
                    );
                }
                base.px_3()
                    .gap_2()
                    .bg(theme.secondary)
                    .border_y_1()
                    .border_color(theme.border)
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .child(
                        label
                            .child(
                                div()
                                    .whitespace_nowrap()
                                    .text_color(theme.muted_foreground)
                                    .child(dir.to_owned()),
                            )
                            .child(div().whitespace_nowrap().child(name.to_owned())),
                    )
                    .child(stat(file.additions, file.deletions))
                    .into_any_element()
            }
            Row::Hunk(file_ix, hunk_ix) => base
                .px_3()
                .bg(theme.muted_foreground.opacity(0.06))
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground)
                .child(
                    div()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(preview.files[file_ix].hunks[hunk_ix].header.clone()),
                )
                .into_any_element(),
            Row::Line(file_ix, hunk_ix, line_ix) => {
                let line = &preview.files[file_ix].hunks[hunk_ix].lines[line_ix];
                let (tint, sign) = match line.kind {
                    Kind::Added => (Some(gpui::rgb(ADDITION)), "+"),
                    Kind::Removed => (Some(gpui::rgb(DELETION)), "-"),
                    Kind::Context | Kind::Note => (None, " "),
                };
                let gutter = |number: Option<u32>| {
                    div()
                        .w(px(GUTTER_WIDTH))
                        .flex_shrink_0()
                        .pr_2()
                        .text_right()
                        .text_color(theme.muted_foreground.opacity(0.7))
                        .child(number.map(|n| n.to_string()).unwrap_or_default())
                };
                base.text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .when_some(tint, |row, color| row.bg(Hsla::from(color).opacity(0.12)))
                    .when(line.kind == Kind::Note, |row| {
                        row.text_color(theme.muted_foreground)
                    })
                    .child(gutter(line.old))
                    .child(gutter(line.new))
                    .child(
                        div()
                            .w_4()
                            .flex_shrink_0()
                            .text_color(tint.map_or(theme.muted_foreground, Hsla::from))
                            .child(sign),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(line.text.replace('\t', "    ")),
                    )
                    .into_any_element()
            }
            Row::Empty(file_ix) => {
                let file = &preview.files[file_ix];
                let text = if file.binary {
                    "Binary file not shown"
                } else if file.additions + file.deletions > 0 {
                    "Too large to show in full; the counts include every change"
                } else if file.change == Change::Renamed {
                    "Renamed without changes"
                } else {
                    "No textual changes"
                };
                base.px_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(text)
                    .into_any_element()
            }
        }
    }
}

/// The web's `DiffStatLabel`: `+N` and `-N`, each tinted while non-zero.
fn stat(additions: u64, deletions: u64) -> impl IntoElement {
    let count = |sign: char, count: u64, color: u32| {
        div()
            .font_family("monospace")
            .opacity(if count == 0 { 0.5 } else { 1. })
            .text_color(gpui::rgb(color))
            .child(format!("{sign}{}", diff::format_count(count)))
    };
    h_flex()
        .flex_shrink_0()
        .gap_1p5()
        .text_xs()
        .child(count('+', additions, ADDITION))
        .child(count('-', deletions, DELETION))
}

/// A path's directory with its trailing slash, and its file name.
fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').map_or(("", path), |(_, name)| {
        (&path[..path.len() - name.len()], name)
    })
}

impl Render for DiffList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.load {
            None => Self::note("Loading diff...", cx).into_any_element(),
            Some(Load::Failed(message)) => {
                Self::note(format!("Could not load the diff: {message}"), cx).into_any_element()
            }
            Some(Load::Done(preview)) if preview.files.is_empty() => {
                Self::note("No changes in the working tree.", cx).into_any_element()
            }
            Some(Load::Done(preview)) => v_flex()
                .flex_1()
                .min_h_0()
                .child(Self::summary(preview, cx))
                .when(preview.truncated, |column| {
                    column.child(
                        div()
                            .px_3()
                            .py_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Partial diff preview. Counts include all changes."),
                    )
                })
                .child(
                    uniform_list(
                        "diff-rows",
                        self.rows.len(),
                        cx.processor(|this, range: Range<usize>, _, cx| {
                            range
                                .map(|ix| this.row(this.rows[ix], cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .flex_1()
                    .min_h_0(),
                )
                .into_any_element(),
        };
        v_flex().size_full().min_h_0().child(body)
    }
}

/// The panel as laid out for one render.
pub struct Layout {
    /// The docked column, inside the row under the top bar.
    pub column: Option<AnyElement>,
    /// Its drag edge, over the window like the sidebar's rail.
    pub rail: Option<AnyElement>,
    /// The sheet over the window when it is too narrow to dock.
    pub sheet: Option<AnyElement>,
    /// The docked width, taken from the main area.
    pub docked_width: f32,
}

impl MainView {
    /// The web's `rightPanelAvailable`: a thread with a workspace to diff.
    pub(super) fn right_panel_available(&self) -> bool {
        self.selected_thread.is_some() && self.details_cwd().is_some()
    }

    fn right_panel_is_open(&self) -> bool {
        self.right_panel_available()
            && self
                .details_key()
                .is_some_and(|key| self.right_panel_open.contains(&key))
    }

    pub(super) fn toggle_right_panel(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.details_key().filter(|_| self.right_panel_available()) else {
            return;
        };
        if !self.right_panel_open.remove(&key) {
            self.right_panel_open.insert(key);
        }
        self.sync_diff();
        cx.notify();
    }

    /// The details card's Changes row: opens the panel on the Diff surface.
    pub(super) fn open_diff(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.details_key().filter(|_| self.right_panel_available()) else {
            return;
        };
        self.right_panel_open.insert(key);
        self.sync_diff();
        cx.notify();
    }

    /// Asks for the diff of the open panel's directory once per connection.
    pub(super) fn sync_diff(&mut self) {
        if !self.right_panel_is_open() {
            return;
        }
        let Some(cwd) = self.details_cwd() else {
            return;
        };
        if self.diffs_requested.insert(cwd.clone()) {
            self.request_diff(cwd);
        }
    }

    fn request_diff(&self, cwd: String) {
        if self.actions.try_send(Action::LoadDiff { cwd }).is_err() {
            eprintln!("[ui] could not ask for the diff");
        }
    }

    fn refresh_diff(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.details_cwd() else {
            return;
        };
        self.diffs.remove(&cwd);
        self.request_diff(cwd);
        cx.notify();
    }

    pub(super) fn set_diff(&mut self, cwd: String, diff: Result<Preview, String>) {
        let load = match diff {
            Ok(preview) => Load::Done(Rc::new(preview)),
            Err(message) => Load::Failed(message),
        };
        self.diffs.insert(cwd, load);
    }

    /// The web's `PanelLayoutControls` right panel toggle, pressed while the panel is open.
    pub(super) fn right_panel_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pressed = self.right_panel_is_open();
        let foreground = cx.theme().foreground;
        header_icon_button(
            "toggle-right-panel",
            Icons::PanelRight,
            format!("Toggle right panel ({TOGGLE_SHORTCUT})").into(),
            true,
            cx,
        )
        .when(pressed, move |button| {
            button.bg(foreground.opacity(0.06)).text_color(foreground)
        })
        .on_click(cx.listener(|this, _, _, cx| this.toggle_right_panel(cx)))
    }

    /// Docked beside the main area, or as a sheet over it in narrow windows.
    pub(super) fn right_panel(
        &mut self,
        window_width: f32,
        container_width: f32,
        cx: &mut Context<Self>,
    ) -> Layout {
        let none = Layout {
            column: None,
            rail: None,
            sheet: None,
            docked_width: 0.,
        };
        if !self.right_panel_is_open() {
            return none;
        }
        let load = self
            .details_cwd()
            .and_then(|cwd| self.diffs.get(&cwd).cloned());
        self.diff_list
            .update(cx, |list, cx| list.show(load.as_ref(), cx));
        if is_sheet(window_width) {
            let width = sheet_width(window_width);
            return Layout {
                sheet: Some(self.right_panel_sheet(width, cx)),
                ..none
            };
        }
        let width = clamp_width(self.right_panel_width, window_width, container_width);
        Layout {
            column: Some(self.right_panel_body(width, false, cx).into_any_element()),
            rail: Some(self.right_panel_rail(width, cx).into_any_element()),
            sheet: None,
            docked_width: width,
        }
    }

    /// The tab strip over the active surface.
    fn right_panel_body(&self, width: f32, sheet: bool, cx: &mut Context<Self>) -> gpui::Div {
        let theme = cx.theme();
        let (border, foreground) = (theme.border, theme.foreground);
        let surface = Surface::Diff;
        let tab = h_flex()
            .h_6()
            .flex_shrink_0()
            .gap_1p5()
            .pl_1p5()
            .pr_2()
            .rounded(px(6.))
            .bg(foreground.opacity(0.06))
            .text_xs()
            .text_color(foreground)
            .child(
                Icon::new(surface.icon())
                    .size(px(12.))
                    .text_color(theme.muted_foreground),
            )
            .child(surface.label());
        let strip = h_flex()
            .h(px(STRIP_HEIGHT))
            .flex_shrink_0()
            .px_2()
            .gap_1()
            .border_b_1()
            .border_color(border)
            .child(tab)
            .child(div().flex_1())
            .child(
                header_icon_button(
                    "refresh-diff",
                    Icons::RefreshCw,
                    "Refresh diff".into(),
                    true,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.refresh_diff(cx))),
            )
            .when(sheet, |strip| {
                strip.child(
                    ghost_icon_button(
                        "close-right-panel",
                        Icon::new(IconName::Close).size_4(),
                        "Close right panel".into(),
                        true,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_right_panel(cx))),
                )
            });
        v_flex()
            .w(px(width))
            .h_full()
            .flex_shrink_0()
            .min_w_0()
            .bg(cx.theme().background)
            .when(!sheet, |panel| panel.border_l_1().border_color(border))
            .child(strip)
            .child(div().flex_1().min_h_0().child(self.diff_list.clone()))
    }

    /// The panel's `DiffPanelShell` as a sheet: a dimmed backdrop and a drawer from the right.
    fn right_panel_sheet(&self, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let drawer = div()
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(width))
            .occlude()
            .shadow_lg()
            .child(self.right_panel_body(width, true, cx))
            .with_animation(
                "right-panel-sheet",
                Animation::new(Duration::from_millis(SHEET_MS)).with_easing(ease_in_out),
                |drawer, delta| drawer.right(px(-SHEET_SLIDE * (1. - delta))).opacity(delta),
            );
        div()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("right-panel-backdrop")
                    .absolute()
                    .inset_0()
                    .cursor_default()
                    .bg(cx.theme().background.opacity(0.6))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_right_panel(cx))),
            )
            .child(drawer)
            .into_any_element()
    }

    /// Drag the panel's left edge to resize, double-click to reset, like the sidebar's rail.
    fn right_panel_rail(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let line = cx.theme().border;
        h_flex()
            .id("right-panel-rail")
            .group("right-panel-rail")
            .absolute()
            .top(px(TOPBAR_HEIGHT))
            .bottom_0()
            .right(px(width - RAIL_WIDTH / 2.))
            .w(px(RAIL_WIDTH))
            .justify_center()
            .occlude()
            .cursor_col_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    if event.click_count == 2 {
                        this.right_panel_width = DEFAULT_WIDTH;
                        cx.notify();
                    }
                }),
            )
            .on_drag(RightPanelResize, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_drag_move(cx.listener(
                |this, event: &DragMoveEvent<RightPanelResize>, window, cx| {
                    let viewport = f32::from(window.viewport_size().width);
                    let wanted = viewport - f32::from(event.event.position.x);
                    let sidebar = if this.sidebar_open {
                        this.rendered_sidebar_width.get()
                    } else {
                        0.
                    };
                    this.right_panel_width = clamp_width(wanted, viewport, viewport - sidebar);
                    cx.notify();
                },
            ))
            .child(
                div()
                    .h_full()
                    .w(px(2.))
                    .group_hover("right-panel-rail", move |style| style.bg(line)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;

    #[test]
    fn the_width_stays_between_the_minimum_and_what_the_chat_column_leaves() {
        assert_eq!(clamp_width(100., 1600., 1344.), MIN_WIDTH);
        assert_eq!(clamp_width(540., 1600., 1344.), 540.);
        assert_eq!(clamp_width(2000., 1600., 1344.), 984.);
        assert_eq!(clamp_width(2000., 2000., 1400.), 1040.);
        // The sibling's minimum wins over the fraction, and the minimum over both.
        assert_eq!(clamp_width(2000., 1000., 700.), MIN_WIDTH);
    }

    #[test]
    fn narrow_windows_use_a_sheet_sized_like_the_webs() {
        assert!(is_sheet(980.) && !is_sheet(981.));
        assert_eq!(sheet_width(900.), 378.);
        assert_eq!(sheet_width(2000.), SHEET_MAX);
        assert_eq!(sheet_width(761.), SHEET_MIN);
        assert_eq!(sheet_width(700.), SHEET_COMPACT_MAX);
        assert_eq!(sheet_width(400.), 352.);
    }

    #[test]
    fn rows_list_each_file_then_its_hunks_and_lines() {
        let files = parse(
            "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-x\n+y\ndiff --git a/b b/b\nBinary files a/b and b/b differ\n",
        );
        let preview = Preview {
            files,
            truncated: false,
        };
        assert_eq!(
            rows(&preview),
            [
                Row::File(0),
                Row::Hunk(0, 0),
                Row::Line(0, 0, 0),
                Row::Line(0, 0, 1),
                Row::File(1),
                Row::Empty(1),
            ]
        );
    }

    #[test]
    fn a_path_splits_into_directory_and_name() {
        assert_eq!(split_path("src/ui/mod.rs"), ("src/ui/", "mod.rs"));
        assert_eq!(split_path("Cargo.toml"), ("", "Cargo.toml"));
    }
}
