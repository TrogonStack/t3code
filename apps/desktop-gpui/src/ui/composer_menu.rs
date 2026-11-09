//! The look the composer's select menus share: `ComposerSelectControl` at `size="xs"` for the
//! trigger, and `select.tsx` `SelectPopup`, `SelectGroupLabel` and `SelectItem` for the popup.
//! Colors are the web's `:root` tokens, read from the settings table.

use super::settings::tokens::Tokens;
use super::*;
use gpui::{BoxShadow, point, transparent_black};

/// `h-6 px-1.75 gap-1.5 rounded-(--control-radius)`, a transparent border, `text-xs`, a 12px
/// icon and a label that truncates at `max-w-60`.
fn control(
    id: &'static str,
    icon: Icons,
    label: SharedString,
    rest: Hsla,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .h(px(24.))
        .px(px(7.))
        .rounded(px(8.))
        .border_1()
        .border_color(transparent_black())
        .text_xs()
        .text_color(rest)
        .child(Icon::new(icon).size(px(12.)))
        .child(
            div()
                .max_w(px(240.))
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

/// The label the web renders when nothing can be chosen: `text-muted-foreground/70`, no hover.
pub(super) fn locked(
    id: &'static str,
    icon: Icons,
    label: impl Into<SharedString>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    control(
        id,
        icon,
        label.clone(),
        Tokens::of(cx).muted_foreground.opacity(0.7),
    )
    .tooltip(tooltip(label.to_string()))
}

/// A select's trigger: muted at rest (`foreground/70` in light), `bg-accent` and full
/// foreground when hovered or open.
#[derive(IntoElement)]
pub(super) struct Trigger {
    pub id: &'static str,
    pub icon: Icons,
    pub label: SharedString,
    pub tip: String,
    pub open: bool,
}

impl Selectable for Trigger {
    fn selected(self, _: bool) -> Self {
        self
    }

    fn is_selected(&self) -> bool {
        false
    }

    fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    fn is_open(&self) -> bool {
        self.open
    }
}

impl RenderOnce for Trigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::of(cx);
        let rest = if tokens.dark {
            tokens.muted_foreground
        } else {
            tokens.foreground.opacity(0.7)
        };
        let (accent, foreground, open) = (tokens.accent, tokens.foreground, self.open);
        control(self.id, self.icon, self.label, rest)
            .cursor_pointer()
            .hover(move |style| style.bg(accent).text_color(foreground))
            .when(open, move |control| {
                control.bg(accent).text_color(foreground)
            })
            .when(!open, |control| control.tooltip(tooltip(self.tip)))
    }
}

/// How a row reads: `data-highlighted` is the lit row, and a disabled row is dimmed.
#[derive(Clone, Copy, Default)]
pub(super) struct RowState {
    pub selected: bool,
    pub lit: bool,
    pub disabled: bool,
    /// "Add to another machine": quiet until lit (`opacity-55 in-data-highlighted:opacity-100`).
    pub quiet: bool,
}

pub(super) type OnHover = Box<dyn Fn(&mut App)>;
pub(super) type OnClick = Box<dyn Fn(&mut Window, &mut App)>;

pub(super) struct Row {
    pub id: (&'static str, usize),
    pub state: RowState,
    pub body: AnyElement,
    pub on_hover: OnHover,
    pub on_click: OnClick,
}

/// `inline-flex items-center gap-1.5` with a `size-3` icon and a label.
pub(super) fn plain_body(icon: Icons, label: impl IntoElement, cx: &App) -> gpui::Div {
    h_flex()
        .gap(px(6.))
        .items_center()
        .whitespace_nowrap()
        .child(
            div()
                .text_color(Tokens::of(cx).muted_foreground)
                .child(Icon::new(icon).size(px(12.))),
        )
        .child(label)
}

/// The popup: `dropdown-glass` painted opaque on a `rounded-lg` shell, a `p-1` list, the group
/// label, then the rows. `width` is a fixed width; otherwise `min-w-[150px]`.
pub(super) fn popup(
    group: &'static str,
    width: Option<Pixels>,
    rows: Vec<Row>,
    cx: &App,
) -> gpui::Div {
    let tokens = Tokens::of(cx);
    let shadow = if tokens.dark {
        (18., 44., 0.8)
    } else {
        (16., 40., 0.55)
    };
    let rows = rows.into_iter().map(|row| {
        let RowState {
            selected,
            lit,
            disabled,
            quiet,
        } = row.state;
        let (on_hover, on_click) = (row.on_hover, row.on_click);
        div()
            .id(row.id)
            .flex()
            .items_center()
            .min_h(px(28.))
            .px_2()
            .py_1()
            .rounded(px(4.))
            .text_sm()
            .line_height(px(20.))
            .text_color(tokens.foreground)
            .when(selected, |row| row.bg(tokens.foreground.opacity(0.08)))
            .when(lit, |row| {
                row.bg(tokens.accent).text_color(tokens.accent_foreground)
            })
            .when(quiet && !lit, |row| row.opacity(0.55))
            .when(disabled, |row| row.opacity(0.64))
            .when(!disabled, |row| {
                row.cursor_pointer()
                    .on_hover(move |entered, _, cx| {
                        if *entered {
                            on_hover(cx);
                        }
                    })
                    .on_click(move |_, window, cx| on_click(window, cx))
            })
            .child(row.body)
    });
    v_flex()
        .when_some(width, |popup, width| popup.w(width))
        .min_w(px(150.))
        .rounded(px(8.))
        .border_1()
        .border_color(tokens.foreground.opacity(0.1))
        .bg(tokens.popover)
        .text_color(tokens.foreground)
        .shadow(vec![BoxShadow {
            color: gpui::black().opacity(shadow.2),
            offset: point(px(0.), px(shadow.0)),
            blur_radius: px(shadow.1),
            spread_radius: px(-18.),
            inset: false,
        }])
        .child(
            v_flex()
                .p_1()
                .child(
                    div()
                        .px_2()
                        .py(px(6.))
                        .text_xs()
                        .line_height(px(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(tokens.muted_foreground)
                        .child(group),
                )
                .children(rows),
        )
}
