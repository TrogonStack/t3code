//! `wizard.tsx`: the `WizardPopup` card with its `WizardSteps` list, shared by the SnapShots setup
//! and the Providers tab's text generator dialog.

use super::components::{Toggle, close_button};
use super::tokens::Tokens;
use super::*;
use gpui::{BoxShadow, ElementId, point};
use std::rc::Rc;

pub(super) type StepHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// `WizardSteps`: `rounded-xl p-1` on `background` (`white/4` dark) with a ring, one cell per
/// step. Steps before the current one are buttons that call `on_step`.
pub(super) fn wizard_steps(
    id: &'static str,
    labels: &[&'static str],
    current: usize,
    on_step: StepHandler,
    cx: &App,
) -> AnyElement {
    let theme = Tokens::of(cx);
    let ring = if theme.dark {
        gpui::white().opacity(0.05)
    } else {
        gpui::black().opacity(0.05)
    };
    let faint = if theme.dark {
        gpui::white().opacity(0.1)
    } else {
        gpui::black().opacity(0.1)
    };
    let cells = labels.iter().enumerate().map(|(index, label)| {
        let is_current = index == current;
        let done = index < current;
        let clickable = done;
        let card = theme.card;
        let (circle_bg, circle_text, circle_ring) = if done {
            (theme.primary, gpui::white(), theme.primary)
        } else if is_current {
            (
                theme.primary.opacity(0.1),
                theme.primary,
                theme.primary.opacity(0.3),
            )
        } else {
            (
                if theme.dark {
                    gpui::white().opacity(0.05)
                } else {
                    card
                },
                theme.muted_foreground,
                faint,
            )
        };
        let circle = div()
            .size(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .rounded_full()
            .border_1()
            .border_color(circle_ring)
            .bg(circle_bg)
            .text_color(circle_text)
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .child(if done {
                Icon::new(IconName::Check).size(px(16.)).into_any_element()
            } else {
                div().child((index + 1).to_string()).into_any_element()
            });
        let on_step = on_step.clone();
        div()
            .id((id, index))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .px(px(10.))
            .py_2()
            .rounded(px(8.))
            .border_1()
            .border_color(if is_current {
                ring
            } else {
                gpui::transparent_black()
            })
            .when(is_current, |cell| {
                cell.bg(card).when(!theme.dark, |cell| {
                    cell.shadow(vec![BoxShadow {
                        color: gpui::black().opacity(0.05),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(2.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                })
            })
            .when(clickable, |cell| {
                cell.cursor_pointer()
                    .hover(move |style| style.bg(card))
                    .on_click(move |_, window, cx| on_step(index, window, cx))
            })
            .child(circle)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if is_current {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    })
                    .child(*label),
            )
    });
    h_flex()
        .gap_1()
        .p_1()
        .rounded(px(12.))
        .border_1()
        .border_color(ring)
        .bg(if theme.dark {
            gpui::white().opacity(0.04)
        } else {
            theme.background
        })
        .children(cells)
        .into_any_element()
}

/// `WizardHeader`: the title, an optional description and the steps.
pub(super) fn wizard_header(
    title: impl IntoElement,
    description: Option<&'static str>,
    steps: AnyElement,
    cx: &App,
) -> AnyElement {
    let theme = Tokens::of(cx);
    v_flex()
        .gap_2()
        .px_6()
        .pt_6()
        .pb_3()
        .child(
            div()
                .text_xl()
                .line_height(px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .children(description.map(|text| {
            div()
                .text_size(px(14.))
                .line_height(px(20.))
                .text_color(theme.muted_foreground)
                .child(text)
        }))
        .child(steps)
        .into_any_element()
}

/// `WizardPanel`: the tinted band between the header and the footer.
pub(super) fn wizard_panel(body: impl IntoElement, cx: &App) -> AnyElement {
    let theme = Tokens::of(cx);
    let ring = if theme.dark {
        gpui::white().opacity(0.05)
    } else {
        gpui::black().opacity(0.05)
    };
    div()
        .min_w_0()
        .px_6()
        .py_5()
        .border_1()
        .border_color(ring)
        .bg(if theme.dark {
            gpui::white().opacity(0.02)
        } else {
            theme.background.opacity(0.8)
        })
        .child(body)
        .into_any_element()
}

/// `WizardFooter`: `leading` at the left, the actions at the right.
pub(super) fn wizard_footer(
    leading: Option<AnyElement>,
    actions: Vec<AnyElement>,
) -> AnyElement {
    h_flex()
        .justify_between()
        .gap_2()
        .px_6()
        .py_4()
        .child(div().children(leading))
        .child(h_flex().gap_2().children(actions))
        .into_any_element()
}

/// `WizardPopup` (`max-w-xl`) over its backdrop, with the close button at the top right.
pub(super) fn wizard_popup(
    id: &'static str,
    parts: [AnyElement; 3],
    dismiss: Toggle,
    cx: &App,
) -> AnyElement {
    let theme = Tokens::of(cx);
    let border = if theme.dark {
        gpui::white().opacity(0.08)
    } else {
        theme.foreground.opacity(0.1)
    };
    let (backdrop_dismiss, close_dismiss) = (dismiss.clone(), dismiss);
    let [header, panel, footer] = parts;
    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .id((ElementId::from(id), "backdrop"))
                .absolute()
                .inset_0()
                .occlude()
                .bg(theme.backdrop())
                .on_click(move |_, window, cx| backdrop_dismiss(window, cx)),
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
                        .relative()
                        .occlude()
                        .w(px(576.))
                        .max_w_full()
                        .max_h_full()
                        .overflow_hidden()
                        .rounded(px(16.))
                        .border_1()
                        .border_color(border)
                        .bg(theme.background)
                        .text_color(theme.foreground)
                        .shadow(vec![BoxShadow {
                            color: gpui::black().opacity(if theme.dark { 0.9 } else { 0.65 }),
                            offset: point(px(0.), px(24.)),
                            blur_radius: px(if theme.dark { 72. } else { 64. }),
                            spread_radius: px(if theme.dark { -20. } else { -24. }),
                            inset: false,
                        }])
                        .child(header)
                        .child(panel)
                        .child(footer)
                        .child(
                            div().absolute().top(px(8.)).right(px(8.)).child(
                                close_button(id, cx)
                                    .on_click(move |_, window, cx| close_dismiss(window, cx)),
                            ),
                        ),
                ),
        )
        .into_any_element()
}
