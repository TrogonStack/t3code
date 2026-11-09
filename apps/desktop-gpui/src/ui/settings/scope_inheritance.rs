//! `SettingInheritance` (SettingInheritance.tsx:166-330): the Layers marker beside a setting's
//! title, its summary tooltip, and the popover that lists where the value comes from. Any tab
//! can reuse it through `marker_row`, a `SettingsRow` (settingsLayout.tsx) with a marker slot.

use super::components::tab_stop;
use super::tokens::Tokens;
use super::*;
use crate::projects::MachineKind;
use crate::settings_devices::{Layer, State, marker_always_visible};
use gpui::{BoxShadow, ElementId, point};

/// A trigger for `Popover` that draws the element it was given.
#[derive(IntoElement)]
pub(super) struct PopTrigger(AnyElement);

impl PopTrigger {
    pub(super) fn new(element: impl IntoElement) -> Self {
        Self(element.into_any_element())
    }
}

impl Selectable for PopTrigger {
    fn selected(self, _: bool) -> Self {
        self
    }

    fn is_selected(&self) -> bool {
        false
    }
}

impl RenderOnce for PopTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}

/// `dropdown-glass` popup shell (popover.tsx:73-90): a `rounded-lg` box with a `foreground/10`
/// border and the menu shadow.
pub(super) fn popup_shell(cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    let shadow = if theme.dark {
        (18., 44., 0.8)
    } else {
        (16., 40., 0.55)
    };
    v_flex()
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
}

/// What the marker explains.
#[derive(Clone)]
pub(super) struct Marker {
    pub id: &'static str,
    pub state: State,
    pub summary: String,
    pub layers: Vec<Layer>,
    pub machine: MachineKind,
    pub environment: String,
}

/// The popover body (SettingInheritance.tsx:263-329, one environment): a muted header with the
/// machine icon and name, then the layers, the effective one highlighted and checked.
fn chain(marker: &Marker, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    let header = h_flex()
        .items_center()
        .gap(px(6.))
        .pb(px(6.))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.muted_foreground)
        .child(
            div()
                .flex_shrink_0()
                .child(Icon::new(machine_icon(marker.machine)).size(px(14.))),
        )
        .child(div().min_w_0().truncate().child(marker.environment.clone()));
    let layers = v_flex()
        .text_sm()
        .children(marker.layers.iter().map(|layer| {
            let label_color = if layer.effective {
                theme.foreground
            } else {
                theme.muted_foreground
            };
            let value_color = if layer.effective {
                theme.foreground
            } else if layer.set {
                theme.muted_foreground
            } else {
                theme.muted_foreground.opacity(0.6)
            };
            let label = match layer.kind {
                crate::settings_devices::LayerKind::Environment => "Environment".to_owned(),
                _ => layer.label.clone(),
            };
            h_flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .rounded(px(6.))
                .px_2()
                .py_1()
                .when(layer.effective, |row| {
                    row.bg(theme.foreground.opacity(0.06))
                })
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(label_color)
                        .when(layer.effective, |label| {
                            label.font_weight(FontWeight::MEDIUM)
                        })
                        .child(label),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(6.))
                        .text_color(value_color)
                        .child(div().max_w(px(128.)).truncate().child(layer.value.clone()))
                        .child(
                            div()
                                .size(px(14.))
                                .flex_shrink_0()
                                .text_color(theme.primary)
                                .when(layer.effective, |slot| {
                                    slot.child(Icon::new(Icons::Check).size(px(14.)))
                                }),
                        ),
                )
        }));
    v_flex()
        .w(px(320.))
        .max_w_full()
        .px(px(12.))
        .py(px(10.))
        .child(header)
        .child(layers)
}

/// The Layers button: `Button size="icon-micro" variant="ghost-muted"` (`size-5 rounded-sm`)
/// holding a `size-3` icon, `text-primary` while overridden, with the summary as its tooltip.
fn indicator(marker: &Marker, window: &mut Window, cx: &mut App) -> AnyElement {
    let key = ElementId::from(marker.id);
    let handle = tab_stop(&key, window, cx);
    let open = window.use_keyed_state(ElementId::from((key.clone(), "open")), cx, |_, _| false);
    let opened = *open.read(cx);
    let theme = Tokens::of(cx);
    let (muted, foreground, accent, primary) = (
        theme.muted_foreground,
        theme.foreground,
        theme.accent,
        theme.primary,
    );
    let overridden = marker.state == State::Overridden;
    let summary = marker.summary.clone();
    let button = div()
        .id(key)
        .track_focus(&handle)
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .text_color(if overridden { primary } else { muted })
        .cursor_pointer()
        .hover(move |style| {
            style
                .bg(accent)
                .text_color(if overridden { primary } else { foreground })
        })
        .tooltip(tooltip(summary))
        .child(Icon::new(Icons::Layers).size(px(12.)));
    let marker = marker.clone();
    let toggled = open.clone();
    Popover::new(ElementId::from((marker.id, 2usize)))
        .anchor(Anchor::TopLeft)
        .offset(px(4.))
        .open(opened)
        .p_0()
        .appearance(false)
        .on_open_change(move |next, _, cx| toggled.update(cx, |open, _| *open = *next))
        .trigger(PopTrigger::new(button))
        .content(move |_, _, cx| popup_shell(cx).child(chain(&marker, cx)))
        .into_any_element()
}

/// `SettingsRow` with a marker slot between the title and the reset slot (settingsLayout.tsx:
/// 455-459): the marker waits for hover, focus or an open popover unless it explains an
/// inherited or overridden value.
#[allow(clippy::too_many_arguments)]
pub(super) fn marker_row(
    id: &'static str,
    title: &'static str,
    description: impl IntoElement,
    marker: Option<Marker>,
    reset: Option<AnyElement>,
    control: AnyElement,
    wide: bool,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let theme = Tokens::of(cx);
    let slot = marker.map(|marker| {
        let always = marker_always_visible(marker.state);
        let key = ElementId::from(marker.id);
        let handle = tab_stop(&key, window, cx);
        let open = *window
            .use_keyed_state(ElementId::from((key, "open")), cx, |_, _| false)
            .read(cx);
        let visible = always || open || handle.is_focused(window);
        let button = indicator(&marker, window, cx);
        div()
            .size(px(20.))
            .flex_shrink_0()
            .when(!visible, |slot| {
                slot.opacity(0.).group_hover(id, |style| style.opacity(1.))
            })
            .child(button)
    });
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
        .children(slot)
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
    div().id(id).group(id).px_4().py_3().child(inner)
}
