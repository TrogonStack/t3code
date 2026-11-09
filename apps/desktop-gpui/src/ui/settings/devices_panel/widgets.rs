//! The small controls of the Devices tab: `Button` (button.tsx), `Input` (input.tsx),
//! `Spinner`, the menu rows of a popup and the dialog frame (dialog.tsx).

use super::super::tokens::Tokens;
use super::super::*;
use gpui::{ElementId, Hsla};
use gpui_component::Size;
use gpui_component::input::Input;
use gpui_component::spinner::Spinner;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Variant {
    Primary,
    Outline,
    Ghost,
    GhostMuted,
    GhostDestructive,
}

/// `--warning-foreground` (index.css:1165, 1217): amber-700, amber-400 in dark.
pub(super) fn warning(theme: &Tokens) -> Hsla {
    gpui::rgb(if theme.dark { 0xfbbf24 } else { 0xb45309 }).into()
}

/// `Button size="sm"` (`h-7 gap-1.5 rounded-lg px-[9px] text-sm`): the caller adds the content
/// and `on_click`.
pub(super) fn button(
    id: impl Into<ElementId>,
    variant: Variant,
    enabled: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = Tokens::of(cx);
    let (background, border, text, hover) = match variant {
        Variant::Primary => (
            theme.primary,
            theme.primary,
            gpui::white(),
            theme.primary.opacity(0.9),
        ),
        Variant::Outline if theme.dark => (
            theme.input.opacity(0.32),
            theme.input,
            theme.foreground,
            theme.input.opacity(0.64),
        ),
        Variant::Outline => (
            theme.popover,
            theme.input,
            theme.foreground,
            theme.accent.opacity(0.5),
        ),
        Variant::Ghost => (
            gpui::transparent_black(),
            gpui::transparent_black(),
            theme.foreground,
            theme.accent,
        ),
        Variant::GhostMuted | Variant::GhostDestructive => (
            gpui::transparent_black(),
            gpui::transparent_black(),
            theme.muted_foreground,
            theme.accent,
        ),
    };
    let hover_text = match variant {
        Variant::GhostMuted => Some(theme.foreground),
        Variant::GhostDestructive => Some(theme.destructive),
        _ => None,
    };
    div()
        .id(id)
        .h(px(28.))
        .px(px(9.))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(8.))
        .border_1()
        .border_color(border)
        .bg(background)
        .text_color(text)
        .text_size(px(14.))
        .line_height(px(20.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .when(enabled, |button| {
            button.cursor_pointer().hover(move |style| {
                let style = style.bg(hover);
                match hover_text {
                    Some(color) => style.text_color(color),
                    None => style,
                }
            })
        })
        .when(!enabled, |button| button.opacity(0.64))
}

/// `Button size="icon-sm"` (`size-8 sm:size-7`).
pub(super) fn icon_button(
    id: impl Into<ElementId>,
    variant: Variant,
    enabled: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    button(id, variant, enabled, cx).size(px(28.)).p_0()
}

/// `Spinner` (spinner.tsx): the glyph without the continuous spin the app avoids.
pub(super) fn spinner(size: f32, color: Hsla) -> AnyElement {
    Spinner::new()
        .icon(Icon::new(Icons::LoaderCircle))
        .color(color)
        .with_size(Size::Size(px(size)))
        .into_any_element()
}

/// `Input` in its default size: `h-8 rounded-lg border bg-popover`, `ring` while focused.
pub(super) fn field(
    input: &Entity<InputState>,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> gpui::Div {
    let theme = Tokens::of(cx);
    let focused = input.read(cx).focus_handle(cx).contains_focused(window, cx);
    let background = if theme.dark {
        theme.input.opacity(0.32)
    } else {
        theme.background
    };
    div()
        .w_full()
        .h(px(32.))
        .px(px(11.))
        .rounded(px(8.))
        .border_1()
        .border_color(if focused { theme.ring } else { theme.input })
        .bg(background)
        .text_size(px(14.))
        .when(disabled, |field| field.opacity(0.64))
        .child(
            Input::new(input)
                .appearance(false)
                .bordered(false)
                .disabled(disabled)
                .h(px(30.))
                .px_0(),
        )
}

/// `label.block.space-y-1.5.text-sm`.
pub(super) fn labeled(title: &'static str, body: impl IntoElement) -> gpui::Div {
    v_flex()
        .gap(px(6.))
        .text_size(px(14.))
        .child(div().child(title))
        .child(body)
}

/// A line of `text-xs text-muted-foreground`.
pub(super) fn note(text: impl IntoElement, cx: &App) -> gpui::Div {
    div()
        .text_xs()
        .line_height(px(16.))
        .text_color(Tokens::of(cx).muted_foreground)
        .child(text)
}

/// `p.text-xs.text-destructive[role=alert]`.
pub(super) fn problem(text: impl IntoElement, cx: &App) -> gpui::Div {
    div()
        .text_xs()
        .line_height(px(16.))
        .text_color(Tokens::of(cx).destructive)
        .child(text)
}

/// An item of a popup menu (menu.tsx): `min-h-7 rounded-sm px-2 py-1 text-sm`.
pub(super) fn menu_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    destructive: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = Tokens::of(cx);
    let accent = theme.accent;
    div()
        .id(id)
        .min_h(px(28.))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(px(4.))
        .text_sm()
        .cursor_pointer()
        .text_color(if destructive {
            theme.destructive
        } else {
            theme.foreground
        })
        .hover(move |style| style.bg(accent))
        .child(label.into())
}

/// `DialogPopup` (dialog.tsx:99-165): a `max-w-lg` card over the backdrop with a header, a
/// scrolling panel and a footer on `muted`. `dismiss` runs on a backdrop click.
pub(super) fn dialog(
    id: &'static str,
    title: &'static str,
    description: impl IntoElement,
    body: Vec<AnyElement>,
    footer: Vec<AnyElement>,
    dismiss: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = Tokens::of(cx);
    let border = if theme.dark {
        gpui::white().opacity(0.08)
    } else {
        theme.foreground.opacity(0.1)
    };
    let header = v_flex()
        .gap_2()
        .px_6()
        .pt_6()
        .pb_3()
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_size(px(14.))
                .line_height(px(20.))
                .text_color(theme.muted_foreground)
                .child(description),
        );
    let panel = v_flex()
        .id(ElementId::from((id, 1usize)))
        .min_h_0()
        .overflow_y_scroll()
        .gap_4()
        .px_6()
        .pt(px(4.))
        .pb(px(4.))
        .children(body);
    let footer = h_flex()
        .justify_end()
        .gap_2()
        .px_6()
        .py_4()
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.accent)
        .children(footer);
    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .id(ElementId::from((id, 2usize)))
                .absolute()
                .inset_0()
                .occlude()
                .bg(theme.backdrop())
                .on_click(move |_, window, cx| dismiss(window, cx)),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    v_flex()
                        .occlude()
                        .w(px(512.))
                        .max_w_full()
                        .max_h_full()
                        .overflow_hidden()
                        .rounded(px(16.))
                        .border_1()
                        .border_color(border)
                        .bg(theme.background)
                        .shadow(vec![gpui::BoxShadow {
                            color: gpui::black().opacity(if theme.dark { 0.9 } else { 0.65 }),
                            offset: gpui::point(px(0.), px(24.)),
                            blur_radius: px(if theme.dark { 72. } else { 64. }),
                            spread_radius: px(if theme.dark { -20. } else { -24. }),
                            inset: false,
                        }])
                        .child(header)
                        .child(panel)
                        .child(footer),
                ),
        )
        .into_any_element()
}
