//! The breadcrumb pickers of `UsagePage.tsx` (`UsageEnvironmentFilter`, `UsageProviderFilter`)
//! and the `Menu` rows they share with the price dialog's "Apply to" picker.

use super::*;
use crate::usage_data::Hidden;
use gpui::Anchor;
use gpui_component::popover::Popover;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Menu {
    Environments,
    Providers,
    Apply,
}

/// `InlineButton` as a `MenuTrigger`: underlines on hover, chevron fades in.
#[derive(IntoElement)]
pub(super) struct Crumb {
    id: &'static str,
    label: String,
    alert: bool,
    open: bool,
}

impl Selectable for Crumb {
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

impl RenderOnce for Crumb {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let (foreground, muted, warning) =
            (theme.foreground, theme.muted_foreground, theme.warning);
        let open = self.open;
        let icon = if self.alert {
            Icon::new(Icons::CircleAlert)
                .size(px(14.))
                .text_color(warning)
                .into_any_element()
        } else {
            Icon::new(Icons::ChevronDown)
                .size(px(14.))
                .into_any_element()
        };
        let chevron = div()
            .size(px(14.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .when(!self.alert, |chevron| {
                chevron
                    .opacity(if open { 1. } else { 0.6 })
                    .group_hover(self.id, |style| style.opacity(1.))
            })
            .child(icon);
        h_flex()
            .id(self.id)
            .group(self.id)
            .min_w_0()
            .max_w_full()
            .gap(px(2.))
            .items_center()
            .text_color(foreground)
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|style| style.underline())
            .child(div().min_w_0().truncate().child(self.label))
            .child(chevron)
    }
}

/// `MenuCheckboxItem`: a 16px check column, then the content.
pub(super) fn check_row(
    id: impl Into<ElementId>,
    checked: bool,
    content: impl IntoElement,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let hover = theme.accent;
    h_flex()
        .id(id)
        .min_h(px(28.))
        .items_center()
        .gap_2()
        .pl_2()
        .pr_4()
        .rounded(px(4.))
        .text_sm()
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(div().size(px(16.)).flex_shrink_0().when(checked, |slot| {
            slot.child(Icon::new(Icons::Check).size(px(16.)))
        }))
        .child(div().min_w_0().flex_1().child(content))
}

/// `MenuSeparator`.
pub(super) fn separator(cx: &App) -> gpui::Div {
    div().mx_2().my_1().h(px(1.)).bg(cx.theme().border)
}

/// `MenuPopup`: `p-1`, at least 10rem wide.
pub(super) fn menu_popup() -> gpui::Div {
    v_flex().min_w(px(160.)).p_1()
}

pub(super) fn menu_note(text: &'static str, cx: &App) -> gpui::Div {
    div()
        .px_2()
        .py_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// One environment row's right-aligned status text.
pub(super) fn status_text(text: String, destructive: bool, cx: &App) -> gpui::Div {
    let theme = cx.theme();
    div()
        .flex_shrink_0()
        .text_xs()
        .text_color(if destructive {
            theme.danger
        } else {
            theme.muted_foreground
        })
        .child(text)
}

/// `updatingProvidersLabel`.
fn updating_label(providers: &[Provider]) -> String {
    if providers.len() == 1 {
        format!("Updating {}\u{2026}", providers[0].label())
    } else {
        format!("Updating {} providers\u{2026}", providers.len())
    }
}

/// What the environment menu shows for the one connected environment.
#[derive(Clone)]
struct EnvironmentMenu {
    label: String,
    status: String,
    error: bool,
    present: bool,
    selected: bool,
    show_status: bool,
    partial: bool,
}

impl UsageDialog {
    /// The status text in `UsageEnvironmentFilter`, in the web's precedence.
    fn environment_status(&self) -> (String, bool) {
        let mismatch = self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.merged.mismatch.is_some());
        if self.failed {
            return ("Unavailable".to_owned(), true);
        }
        let text = if mismatch {
            "Update required".to_owned()
        } else if self.access != Access::Ready {
            "Connecting\u{2026}".to_owned()
        } else if let Some(shown) = &self.shown {
            if self.manual {
                "Refreshing\u{2026}".to_owned()
            } else if self.stage == Stage::Idle {
                "Ready".to_owned()
            } else {
                let mut providers: Vec<Provider> = Vec::new();
                for source in shown.summary.sources.iter().filter(|s| s.refreshing) {
                    if !providers.contains(&source.provider) {
                        providers.push(source.provider);
                    }
                }
                if providers.is_empty() {
                    "Refreshing\u{2026}".to_owned()
                } else {
                    updating_label(&providers)
                }
            }
        } else {
            "Scanning\u{2026}".to_owned()
        };
        (text, false)
    }

    /// `hasIssue`: the selected environment failed or runs another contract version.
    fn environment_issue(&self) -> bool {
        self.env_selected
            && (self.failed
                || self
                    .shown
                    .as_ref()
                    .is_some_and(|shown| shown.merged.mismatch.is_some()))
    }

    pub(super) fn environment_label(&self) -> String {
        if self.env_selected {
            "All environments".to_owned()
        } else {
            "0 environments".to_owned()
        }
    }

    pub(super) fn set_menu(&mut self, menu: Option<Menu>, cx: &mut Context<Self>) {
        if self.menu != menu {
            self.menu = menu;
            cx.notify();
        }
    }

    /// `onSelectionChange` for the sole environment: every row toggles between all and none.
    pub(super) fn toggle_environment(&mut self, cx: &mut Context<Self>) {
        self.env_selected = !self.env_selected;
        self.hover = None;
        if self.env_selected {
            self.ensure_loaded();
        }
        cx.notify();
    }

    pub(super) fn set_hidden(&mut self, hidden: Hidden, cx: &mut Context<Self>) {
        if self.prefs.hidden == hidden {
            return;
        }
        self.prefs.hidden = hidden;
        self.prefs.save();
        self.hover = None;
        if let Some(shown) = &mut self.shown {
            shown.merged = data::merge(&shown.summary.without(hidden));
        }
        cx.notify();
    }

    pub(super) fn environment_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let limits = self.prefs.metric == Metric::Limits;
        let (status, error) = self.environment_status();
        let menu = EnvironmentMenu {
            label: self.limits.environment.clone(),
            status,
            error,
            present: self.access != Access::Unavailable,
            selected: self.env_selected,
            show_status: !limits,
            partial: !limits && self.dim().partial(),
        };
        let alert = !limits && self.environment_issue();
        Popover::new("usage-environments")
            .anchor(Anchor::TopLeft)
            .open(self.menu == Some(Menu::Environments))
            .p_0()
            .on_open_change({
                let view = view.clone();
                move |open, _, cx| {
                    let menu = open.then_some(Menu::Environments);
                    view.update(cx, |this, cx| this.set_menu(menu, cx));
                }
            })
            .trigger(Crumb {
                id: "usage-crumb-environments",
                label: self.environment_label(),
                alert,
                open: false,
            })
            .content(move |_, _, cx| {
                let theme = cx.theme();
                let muted = theme.muted_foreground;
                let toggle = {
                    let view = view.clone();
                    move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        view.update(cx, |this, cx| this.toggle_environment(cx));
                    }
                };
                let environment = menu.present.then(|| {
                    let label = if menu.label.is_empty() {
                        "Environment".to_owned()
                    } else {
                        menu.label.clone()
                    };
                    check_row(
                        "usage-environment-row",
                        menu.selected,
                        h_flex()
                            .min_w_0()
                            .items_center()
                            .gap_3()
                            .child(div().min_w_0().flex_1().truncate().child(label))
                            .when(menu.show_status, |row| {
                                row.child(status_text(menu.status.clone(), menu.error, cx))
                            }),
                        cx,
                    )
                    .on_click(toggle.clone())
                });
                let open_prices = {
                    let view = view.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        view.update(cx, |this, cx| this.open_prices(window, cx));
                    }
                };
                menu_popup()
                    .child(
                        check_row(
                            "usage-environments-all",
                            menu.selected,
                            "All environments",
                            cx,
                        )
                        .on_click(toggle.clone()),
                    )
                    .child(separator(cx))
                    .children(environment)
                    .when(!menu.present, |list| {
                        list.child(menu_note("No environments connected.", cx))
                    })
                    .when(menu.show_status && menu.partial, |list| {
                        list.child(menu_note(
                            "Totals are partial while selected environments scan.",
                            cx,
                        ))
                    })
                    .child(separator(cx))
                    .child(
                        h_flex()
                            .id("usage-model-prices")
                            .min_h(px(28.))
                            .items_center()
                            .gap_2()
                            .px_2()
                            .rounded(px(4.))
                            .text_sm()
                            .cursor_pointer()
                            .hover({
                                let accent = theme.accent;
                                move |style| style.bg(accent)
                            })
                            .on_click(open_prices)
                            .child(
                                Icon::new(Icons::SlidersHorizontal)
                                    .size(px(16.))
                                    .text_color(muted),
                            )
                            .child("Model prices"),
                    )
            })
            .into_any_element()
    }

    pub(super) fn provider_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let hidden = self.prefs.hidden;
        Popover::new("usage-providers")
            .anchor(Anchor::TopLeft)
            .open(self.menu == Some(Menu::Providers))
            .p_0()
            .on_open_change({
                let view = view.clone();
                move |open, _, cx| {
                    let menu = open.then_some(Menu::Providers);
                    view.update(cx, |this, cx| this.set_menu(menu, cx));
                }
            })
            .trigger(Crumb {
                id: "usage-crumb-providers",
                label: hidden.label(),
                alert: false,
                open: false,
            })
            .content(move |_, _, cx| {
                let all = {
                    let view = view.clone();
                    // `onChange(checked ? [] : PROVIDER_ORDER)`, where `checked` is the new state.
                    move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        view.update(cx, |this, cx| {
                            let next = if this.prefs.hidden.is_empty() {
                                Hidden::all()
                            } else {
                                Hidden::default()
                            };
                            this.set_hidden(next, cx);
                        });
                    }
                };
                let rows = Provider::ORDER.into_iter().map(|provider| {
                    let view = view.clone();
                    let visible = !hidden.contains(provider);
                    check_row(
                        ("usage-provider-row", provider.slot()),
                        visible,
                        h_flex()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(mark(provider, 14., cx))
                            .child(div().truncate().child(provider.label())),
                        cx,
                    )
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            let next = this.prefs.hidden.with(provider, !visible);
                            this.set_hidden(next, cx);
                        });
                    })
                });
                menu_popup()
                    .child(
                        check_row(
                            "usage-providers-all",
                            hidden.is_empty(),
                            "All providers",
                            cx,
                        )
                        .on_click(all),
                    )
                    .child(separator(cx))
                    .children(rows)
            })
            .into_any_element()
    }
}
