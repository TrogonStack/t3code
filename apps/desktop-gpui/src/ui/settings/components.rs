//! Reusable pieces of the Settings route: `settingsLayout.tsx`, `SettingsGroup.tsx`, and the
//! `Switch`, `Select` and `Button` primitives they hold.

use super::tokens::Tokens;
use super::*;
use gpui::{BoxShadow, ClickEvent, ElementId, FocusHandle, KeyDownEvent, point};
use std::rc::Rc;

pub(super) type Toggle = Rc<dyn Fn(&mut Window, &mut App)>;
pub(super) type KeyHandler = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
pub(super) type PickHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;
pub(super) type HoverHandler = Rc<dyn Fn(usize, &mut App)>;

/// A tab stop kept across frames, keyed by the control's id.
pub(super) fn tab_stop(id: &ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    window
        .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true))
        .read(cx)
        .clone()
}

fn focus_visible(handle: &FocusHandle, window: &Window) -> bool {
    handle.is_focused(window) && window.last_input_was_keyboard()
}

/// `focus-visible:ring-2 ring-inset ring-ring ring-offset-1 ring-offset-background`
/// (switch.tsx, button.tsx): a 2px ring inside the control, over a 1px line in the background.
fn focus_ring(radius: f32, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    div()
        .absolute()
        .inset_0()
        .rounded(px(radius))
        .border_1()
        .border_color(theme.background)
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px((radius - 1.).max(0.)))
                .border_2()
                .border_color(theme.ring),
        )
}

/// `SettingsSection` (settingsLayout.tsx): `space-y-2.5`, the title row `min-h-7 px-4`, the title
/// `text-base font-semibold`.
pub(super) fn section(title: &'static str, group: impl IntoElement) -> gpui::Div {
    v_flex()
        .gap(px(10.))
        .child(
            h_flex().min_h(px(28.)).items_start().px_4().child(
                h_flex()
                    .min_h(px(28.))
                    .items_center()
                    .gap_2()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            ),
        )
        .child(group)
}

/// `SettingsGroup` (SettingsGroup.tsx): `rounded-xl border border-border/60 bg-card/40 shadow-xs/5`
/// with `border-border/50` between rows.
pub(super) fn group(rows: Vec<AnyElement>, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    let divider = theme.border.opacity(0.5);
    let mut card = v_flex()
        .relative()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border.opacity(0.6))
        .bg(theme.card.opacity(0.4))
        .shadow(vec![BoxShadow {
            color: gpui::black().opacity(0.05),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        }]);
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            card = card.child(div().h(px(1.)).w_full().bg(divider));
        }
        card = card.child(row);
    }
    card
}

/// `SettingsRow` (settingsLayout.tsx): `px-4 py-3`, the columns `minmax(0,1fr) minmax(10rem,auto)`
/// with `gap-8` from a 32rem container, stacked with `gap-3` below it.
pub(super) fn row(
    title: &'static str,
    description: impl IntoElement,
    reset: Option<AnyElement>,
    control: AnyElement,
    wide: bool,
    cx: &App,
) -> gpui::Div {
    let theme = Tokens::of(cx);
    let heading = h_flex()
        .min_h(px(20.))
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(title),
        )
        .child(div().size(px(20.)).flex_shrink_0().children(reset));
    let text = v_flex().min_w_0().flex_1().gap_1().child(heading).child(
        div()
            .max_w(px(672.))
            .text_xs()
            .line_height(px(18.))
            .text_color(theme.muted_foreground)
            .child(description),
    );
    let control = h_flex()
        .min_w_0()
        .flex_shrink_0()
        .items_center()
        .gap_2()
        .justify_end()
        .child(control);
    let inner = if wide {
        h_flex()
            .items_center()
            .gap(px(32.))
            .child(text)
            .child(div().min_w(px(160.)).child(control))
    } else {
        v_flex().gap_3().child(text).child(control)
    };
    div().px_4().py_3().child(inner)
}

/// Hover state of a `TooltipPopup side="top"`, shared by the control that opens it.
#[derive(Default)]
struct TipState {
    shown: bool,
    epoch: u64,
}

/// Base UI's default `Tooltip.Root` open delay.
const TIP_DELAY: std::time::Duration = std::time::Duration::from_millis(600);

/// `TooltipPopup` (tooltip.tsx:120-157): above the trigger, `sideOffset` 4, centered, a
/// `rounded-md border bg-popover shadow-md/5 text-xs` box with `px-2 py-1` and `leading-snug`.
fn tip_popup(text: &'static str, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    div()
        .absolute()
        .bottom_full()
        .mb(px(4.))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .px_2()
                .py_1()
                .rounded(px(6.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_xs()
                .line_height(px(15.))
                .text_color(theme.foreground)
                .shadow(vec![BoxShadow {
                    color: gpui::black().opacity(0.05),
                    offset: point(px(0.), px(4.)),
                    blur_radius: px(6.),
                    spread_radius: px(-1.),
                    inset: false,
                }])
                .child(text),
        )
}

/// `SettingResetButton` (settingsLayout.tsx:548-580): `Button size="icon-micro" variant="ghost-muted"`
/// (`size-5 rounded-sm`, muted, `bg-accent` and foreground on hover) holding a `size-3` Undo2,
/// with a "Reset to default" tooltip above it that opens after 600ms of hover or on keyboard focus.
pub(super) fn reset_button(
    id: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let key = ElementId::from(id);
    let handle = tab_stop(&key, window, cx);
    let visible = focus_visible(&handle, window);
    let tip = window.use_keyed_state(ElementId::from((key.clone(), "tip")), cx, |_, _| {
        TipState::default()
    });
    let shown = tip.read(cx).shown || visible;
    let theme = Tokens::of(cx);
    let (muted, hover, foreground) = (theme.muted_foreground, theme.accent, theme.foreground);
    let ring = focus_ring(4., cx);
    let popup = shown.then(|| gpui::deferred(tip_popup("Reset to default", cx)));
    let (hovered_tip, clicked_tip) = (tip.clone(), tip.clone());
    div()
        .id(key)
        .track_focus(&handle)
        .relative()
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.bg(hover).text_color(foreground))
        .on_hover(move |entered, window, cx| {
            let epoch = hovered_tip.update(cx, |state, _| {
                state.epoch += 1;
                state.shown = false;
                state.epoch
            });
            window.refresh();
            if !*entered {
                return;
            }
            let tip = hovered_tip.clone();
            window
                .spawn(cx, async move |cx| {
                    cx.background_executor().timer(TIP_DELAY).await;
                    cx.update(|window, cx| {
                        tip.update(cx, |state, _| state.shown = state.epoch == epoch);
                        window.refresh();
                    })
                    .ok();
                })
                .detach();
        })
        .on_click(move |event, window, cx| {
            clicked_tip.update(cx, |state, _| {
                state.epoch += 1;
                state.shown = false;
            });
            on_click(event, window, cx);
        })
        .child(Icon::new(Icons::Undo2).size(px(12.)))
        .children(popup)
        .when(visible, |button| button.child(ring))
        .into_any_element()
}

/// `Switch` (switch.tsx) at the desktop size: a 30x18 track with 2px padding and a 14px thumb
/// that travels 12px. Space and Enter toggle it.
pub(super) fn switch(
    id: &'static str,
    checked: bool,
    on_toggle: Toggle,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let key = ElementId::from(id);
    let handle = tab_stop(&key, window, cx);
    let visible = focus_visible(&handle, window);
    let theme = Tokens::of(cx);
    let track = if checked { theme.primary } else { theme.input };
    let thumb = if checked || !theme.dark {
        theme.background
    } else {
        theme.muted_foreground
    };
    let ring = focus_ring(9., cx);
    let click = on_toggle.clone();
    div()
        .id(key)
        .track_focus(&handle)
        .relative()
        .w(px(30.))
        .h(px(18.))
        .flex_shrink_0()
        .rounded_full()
        .p(px(2.))
        .bg(track)
        .cursor_pointer()
        .on_click(move |_, window, cx| click(window, cx))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                cx.stop_propagation();
                on_toggle(window, cx);
            }
        })
        .child(
            div()
                .size(px(14.))
                .rounded_full()
                .bg(thumb)
                .ml(px(if checked { 12. } else { 0. }))
                .shadow(vec![BoxShadow {
                    color: gpui::black().opacity(0.05),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(2.),
                    spread_radius: px(0.),
                    inset: false,
                }]),
        )
        .when(visible, |track| track.child(ring))
        .into_any_element()
}

/// The `size="sm"` select trigger (select.tsx): `min-h-7 rounded-lg border`, `text-sm`, a
/// 14px chevron at 70% opacity.
#[derive(IntoElement)]
pub(super) struct SelectTrigger {
    id: ElementId,
    label: &'static str,
    width: f32,
    handle: FocusHandle,
    open: bool,
    on_key: KeyHandler,
    leading: Option<AnyElement>,
    auto_width: bool,
}

impl SelectTrigger {
    /// `w-auto`: the trigger is as wide as its label instead of `width`.
    pub(super) fn auto_width(mut self) -> Self {
        self.auto_width = true;
        self
    }

    /// Content before the label, as the diff colors select puts its two dots.
    pub(super) fn leading(mut self, leading: AnyElement) -> Self {
        self.leading = Some(leading);
        self
    }

    pub(super) fn new(
        id: &'static str,
        label: &'static str,
        width: f32,
        handle: FocusHandle,
        on_key: KeyHandler,
    ) -> Self {
        let id = ElementId::from(id);
        Self {
            id,
            label,
            width,
            handle,
            open: false,
            on_key,
            leading: None,
            auto_width: false,
        }
    }
}

impl Selectable for SelectTrigger {
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

impl RenderOnce for SelectTrigger {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = Tokens::of(cx);
        // `dark:bg-input/32`; light is `bg-background`.
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let on_key = self.on_key;
        // `focus-visible:border-ring focus-visible:ring-[3px] ring-ring/24 ring-inset`, and
        // `shadow-none` while focused or pressed.
        let focus_visible = !self.open && focus_visible(&self.handle, window);
        let shadow = if focus_visible {
            vec![BoxShadow {
                color: theme.ring.opacity(0.24),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(3.),
                inset: true,
            }]
        } else if self.open {
            Vec::new()
        } else {
            vec![BoxShadow {
                color: gpui::black().opacity(0.05),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
                inset: false,
            }]
        };
        h_flex()
            .id(self.id)
            .track_focus(&self.handle)
            .when(!self.auto_width, |trigger| trigger.w(px(self.width)))
            .max_w_full()
            .min_h(px(28.))
            .items_center()
            .gap(px(6.))
            .px(px(9.))
            .rounded(px(8.))
            .border_1()
            .border_color(if focus_visible {
                theme.ring
            } else {
                theme.input
            })
            .bg(background)
            .text_sm()
            .text_color(theme.foreground)
            .cursor_pointer()
            .shadow(shadow)
            .on_key_down(move |event, window, cx| on_key(event, window, cx))
            .children(self.leading)
            .child(
                div()
                    .min_w_0()
                    .when(!self.auto_width, |label| label.flex_1())
                    .truncate()
                    .child(self.label),
            )
            .child(
                div()
                    .mr(px(-4.))
                    .flex_shrink_0()
                    .opacity(0.7)
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            )
    }
}

/// The select popup (select.tsx:94-118, 144-167): `dropdown-glass` (index.css:454-465) on a
/// `rounded-lg` shell with `p-1`, items `min-h-7 rounded-sm px-2 py-1 text-sm`, the chosen item at
/// `foreground/[0.08]` and the highlighted one on `accent`. Painted opaque, as the web does where
/// backdrop blur is unsupported.
pub(super) struct Menu {
    pub id: &'static str,
    pub labels: Vec<&'static str>,
    pub selected: usize,
    pub highlighted: usize,
    pub width: f32,
    pub on_pick: PickHandler,
    pub on_hover: HoverHandler,
}

pub(super) fn select_menu(menu: &Menu, cx: &App) -> gpui::Div {
    menu_items(menu, false, cx)
}

/// `select_menu` with a trailing check on the chosen item (the send shortcut items,
/// SettingsPanels.tsx Chat tab).
pub(super) fn select_menu_checked(menu: &Menu, cx: &App) -> gpui::Div {
    menu_items(menu, true, cx)
}

fn menu_items(menu: &Menu, check: bool, cx: &App) -> gpui::Div {
    let Menu {
        id,
        labels,
        selected,
        highlighted,
        width,
        on_pick,
        on_hover,
    } = menu;
    let (selected, highlighted, id) = (*selected, *highlighted, *id);
    let theme = Tokens::of(cx);
    let (accent, accent_foreground, chosen) = (
        theme.accent,
        theme.accent_foreground,
        theme.foreground.opacity(0.08),
    );
    let shadow = if theme.dark {
        (18., 44., 0.8)
    } else {
        (16., 40., 0.55)
    };
    v_flex()
        .min_w(px(*width))
        .p_1()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.foreground.opacity(0.1))
        .bg(theme.popover)
        .shadow(vec![BoxShadow {
            color: gpui::black().opacity(shadow.2),
            offset: point(px(0.), px(shadow.0)),
            blur_radius: px(shadow.1),
            spread_radius: px(-18.),
            inset: false,
        }])
        .children(labels.iter().enumerate().map(|(index, label)| {
            let pick = on_pick.clone();
            let hover = on_hover.clone();
            div()
                .id((id, index))
                .min_h(px(28.))
                .flex()
                .items_center()
                .px_2()
                .py_1()
                .rounded(px(4.))
                .text_sm()
                .cursor_pointer()
                .when(index == selected, |item| item.bg(chosen))
                .when(index == highlighted, |item| {
                    item.bg(accent).text_color(accent_foreground)
                })
                .on_hover(move |entered, _, cx| {
                    if *entered {
                        hover(index, cx);
                    }
                })
                .on_click(move |_, window, cx| pick(index, window, cx))
                .when(check, |item| item.justify_between().gap_4())
                .child(*label)
                .when(check && index == selected, |item| {
                    item.child(Icon::new(IconName::Check).size(px(16.)))
                })
        }))
}

/// `Button variant="ghost"` (button.tsx:59-60): `rounded-[--control-radius]` (8px), a transparent
/// border, `text-foreground`, `bg-accent` on hover, and the icon in `--contrast-muted-foreground`.
pub(super) fn ghost_button(id: &'static str, enabled: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    let theme = Tokens::of(cx);
    let accent = theme.accent;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_color(theme.foreground)
        .when(enabled, |button| {
            button.cursor_pointer().hover(move |style| style.bg(accent))
        })
        .when(!enabled, |button| button.opacity(0.64))
}

/// `AppRouteModalClose`: `Button size="icon-sm" variant="ghost"` with a 14px X (`sm:size-7`).
pub(super) fn close_button(id: &'static str, cx: &App) -> gpui::Stateful<gpui::Div> {
    let muted = Tokens::of(cx).muted_foreground;
    ghost_button(id, true, cx)
        .size(px(28.))
        .flex_shrink_0()
        .tooltip(tooltip("Close (Esc)"))
        .child(
            div()
                .text_color(muted)
                .child(Icon::new(IconName::Close).size(px(14.))),
        )
}

/// `AdvancedSettings` (AdvancedSettings.tsx:11-42): a `min-h-10 px-4` trigger with a 14px chevron
/// that turns 90 degrees when open, then the rows under a `border-border/50` rule and dividers.
pub(super) fn disclosure(
    id: &'static str,
    open: bool,
    on_toggle: Toggle,
    rows: Vec<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let key = ElementId::from(id);
    let handle = tab_stop(&key, window, cx);
    let visible = focus_visible(&handle, window);
    let theme = Tokens::of(cx);
    let (muted, foreground, ring) = (theme.muted_foreground, theme.foreground, theme.ring);
    let divider = theme.border.opacity(0.5);
    let click = on_toggle.clone();
    let chevron = Icon::new(IconName::ChevronRight).size(px(14.));
    let chevron = if open {
        chevron.rotate(gpui::radians(std::f32::consts::FRAC_PI_2))
    } else {
        chevron
    };
    let trigger = h_flex()
        .id(key)
        .track_focus(&handle)
        .relative()
        .min_h(px(40.))
        .w_full()
        .items_center()
        .gap(px(6.))
        .px_4()
        .when(!open, |trigger| trigger.rounded_b(px(12.)))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.text_color(foreground))
        .on_click(move |_, window, cx| click(window, cx))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                cx.stop_propagation();
                on_toggle(window, cx);
            }
        })
        .child(div().flex_shrink_0().child(chevron))
        .child("Advanced")
        .when(visible, |trigger| {
            trigger.child(
                div()
                    .absolute()
                    .inset_0()
                    .when(!open, |ring_box| ring_box.rounded_b(px(12.)))
                    .border_2()
                    .border_color(ring),
            )
        });
    let mut panel = v_flex().border_t_1().border_color(divider);
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            panel = panel.child(div().h(px(1.)).w_full().bg(divider));
        }
        panel = panel.child(row);
    }
    v_flex()
        .child(trigger)
        .when(open, |disclosure| disclosure.child(panel))
        .into_any_element()
}
