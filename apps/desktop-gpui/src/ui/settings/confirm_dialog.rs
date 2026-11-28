//! The shared confirmation dialog: `ConfirmDialogHost.tsx` over `alert-dialog.tsx`. Every
//! confirmation of the app goes through `confirm_dialog`, so they share the web's copy split,
//! layout, buttons and keys.
//!
//! Like the web's `AlertDialog`, a click on the backdrop does not dismiss it; Escape cancels.
//! Focus starts on Cancel, so Enter cancels until the user tabs to Confirm.

use super::components::{focus_ring, focus_visible, tab_stop};
use super::devices_panel::widgets::{Variant, button};
use super::tokens::Tokens;
use super::*;
use crate::confirm::split_message;
use gpui::{ElementId, KeyDownEvent};
use std::rc::Rc;

/// What the dialog says and how its buttons behave.
pub(crate) struct ConfirmSpec {
    id: &'static str,
    title: SharedString,
    description: Option<SharedString>,
    confirm_label: SharedString,
    destructive: bool,
    busy: bool,
    confirm_enabled: bool,
    extra: Vec<AnyElement>,
}

impl ConfirmSpec {
    /// A `dialogs.confirm(message)` call: split like `resolveConfirmDialogCopy`, "Confirm".
    pub(crate) fn from_message(id: &'static str, message: &str) -> Self {
        let copy = split_message(message);
        Self {
            id,
            title: copy.title.into(),
            description: copy.description.map(Into::into),
            confirm_label: "Confirm".into(),
            destructive: false,
            busy: false,
            confirm_enabled: true,
            extra: Vec::new(),
        }
    }

    /// A bespoke `AlertDialog` with its own title and description.
    pub(crate) fn titled(
        id: &'static str,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            title: title.into(),
            description: Some(description.into()),
            ..Self::from_message(id, "")
        }
    }

    /// `{ variant: "destructive" }`.
    pub(crate) fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// The action label of a bespoke `AlertDialog` ("Remove profile").
    pub(crate) fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    /// A request is running: both buttons are disabled.
    pub(crate) fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    pub(crate) fn confirm_enabled(mut self, enabled: bool) -> Self {
        self.confirm_enabled = enabled;
        self
    }

    /// Notes or an error under the description.
    pub(crate) fn extra(mut self, extra: Vec<AnyElement>) -> Self {
        self.extra = extra;
        self
    }
}

pub(crate) type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

/// `AlertDialogPopup` with its header and footer, over the backdrop.
pub(crate) fn confirm_dialog(
    spec: ConfirmSpec,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let ConfirmSpec {
        id,
        title,
        description,
        confirm_label,
        destructive,
        busy,
        confirm_enabled,
        extra,
    } = spec;
    let (on_confirm, on_cancel): (Handler, Handler) = (Rc::new(on_confirm), Rc::new(on_cancel));
    let theme = Tokens::of(cx);
    let cancel_focus = tab_stop(&ElementId::from((id, 0usize)), window, cx);
    let confirm_focus = tab_stop(&ElementId::from((id, 1usize)), window, cx);
    let root_focus = window
        .use_keyed_state(ElementId::from((id, 2usize)), cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    if !root_focus.contains_focused(window, cx) {
        window.focus(&cancel_focus, cx);
    }
    let border = if theme.dark {
        gpui::white().opacity(0.08)
    } else {
        theme.foreground.opacity(0.1)
    };
    let cancel_enabled = !busy;
    let confirm_enabled = !busy && confirm_enabled;
    let activate = |enabled: bool, run: &Handler| {
        let run = run.clone();
        move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
            if enabled && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
                run(window, cx);
            }
        }
    };
    let cancel_key = activate(cancel_enabled, &on_cancel);
    let confirm_key = activate(confirm_enabled, &on_confirm);
    let (cancel_click, confirm_click) = (on_cancel.clone(), on_confirm.clone());
    let cancel_ring = focus_visible(&cancel_focus, window);
    let confirm_ring = focus_visible(&confirm_focus, window);
    let (cancel, confirm) = (
        button((id, 3usize), Variant::Outline, cancel_enabled, cx)
            .track_focus(&cancel_focus)
            .relative()
            .h(px(32.))
            .gap(px(8.))
            .on_click(move |_, window, cx| {
                if cancel_enabled {
                    cancel_click(window, cx)
                }
            })
            .on_key_down(cancel_key)
            .child("Cancel")
            .when(cancel_ring, |button| button.child(focus_ring(8., cx))),
        button(
            (id, 4usize),
            if destructive {
                Variant::Destructive
            } else {
                Variant::Primary
            },
            confirm_enabled,
            cx,
        )
        .track_focus(&confirm_focus)
        .relative()
        .h(px(32.))
        .gap(px(8.))
        .on_click(move |_, window, cx| {
            if confirm_enabled {
                confirm_click(window, cx)
            }
        })
        .on_key_down(confirm_key)
        .child(confirm_label)
        .when(confirm_ring, |button| button.child(focus_ring(8., cx))),
    );
    let escape = on_cancel;
    let header = v_flex()
        .gap_2()
        .p_6()
        .child(
            div()
                .text_xl()
                .line_height(px(27.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .when_some(description, |header, description| {
            header.child(
                div()
                    .id(ElementId::from((id, 5usize)))
                    .min_h_0()
                    .max_h(px(384.))
                    .overflow_y_scroll()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .text_color(theme.muted_foreground)
                    .child(description),
            )
        })
        .children(extra);
    let footer = h_flex()
        .justify_end()
        .gap_2()
        .px_6()
        .py_4()
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.72))
        .child(cancel)
        .child(confirm);
    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .id(ElementId::from((id, 6usize)))
                .absolute()
                .inset_0()
                .occlude()
                .bg(theme.backdrop()),
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
                        .track_focus(&root_focus)
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
                        .on_key_down(move |event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" && cancel_enabled {
                                cx.stop_propagation();
                                escape(window, cx);
                            }
                        })
                        .child(header)
                        .child(footer),
                ),
        )
        .into_any_element()
}
