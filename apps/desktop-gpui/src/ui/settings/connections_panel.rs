//! Settings > Environments (`ConnectionsSettings.tsx`, `view="environments"`): this app's local
//! environment, every other environment with its routes, permissions and diagnosis, the folded
//! Load balancing and Pull request sharing cards, and the T3 Connect account.
//!
//! The state of each environment comes from `environment_state`, which `environments::follow`
//! writes; what this device remembers per environment lives in `environment_prefs`.

mod add;

use super::components::{group, section, section_with_action, switch};
use super::confirm_dialog::{ConfirmSpec, confirm_dialog};
use super::devices_panel::widgets::{
    Variant, button, icon_button, menu_item, note, problem, xs_button,
};
use super::scope_inheritance::{PopTrigger, popup_shell};
use super::tokens::Tokens;
use super::*;
use crate::cloud::{self, CloudState};
use crate::connection_diagnosis::{self as diagnosis, Diagnosis, HopResult, Input, Via};
use crate::environment_prefs::{self, Prefs, Sharing};
use crate::environment_presentation::{self as presentation, Tone};
use crate::environment_state::{self, EnvState, Phase};
use crate::projects::MachineKind;
use crate::settings_load_balancing::{LoadBalancing, Preference, summarize};
use crate::ui::add_to_machine::Notice;
use gpui::{Anchor, ClipboardItem, ElementId};
use gpui_component::popover::Popover;
use std::collections::HashMap;
use std::time::Duration;

const ICON_SIZE: f32 = 16.;

/// `EnvironmentMachineKind` choices of the Icon submenu, in the web's order.
const MACHINES: [(MachineKind, &str); 7] = [
    (MachineKind::Server, "Server"),
    (MachineKind::Cloud, "Cloud VM"),
    (MachineKind::Linux, "Linux/WSL"),
    (MachineKind::Desktop, "Desktop"),
    (MachineKind::Laptop, "Laptop"),
    (MachineKind::MacMini, "Mini PC"),
    (MachineKind::MacStudio, "Workstation"),
];

pub(super) fn machine_icon(kind: MachineKind) -> Icons {
    match kind {
        MachineKind::Server => Icons::Server,
        MachineKind::Cloud => Icons::Cloud,
        MachineKind::Linux => Icons::Terminal,
        MachineKind::Desktop => Icons::Monitor,
        MachineKind::Laptop => Icons::Laptop,
        MachineKind::MacMini => Icons::HardDrive,
        MachineKind::MacStudio => Icons::Computer,
    }
}

/// Which of an environment's expandable areas are open.
#[derive(Clone, Copy, Default)]
struct Expanded {
    details: bool,
    routes: bool,
    permissions: bool,
}

#[derive(Clone, PartialEq, Eq)]
enum Menu {
    Row(String),
    Preference(String),
    Sharing(String),
}

#[derive(Clone, PartialEq, Eq)]
enum Confirm {
    Local,
    Remove(String),
}

pub struct ConnectionsPanel {
    cloud: CloudState,
    wide: bool,
    envs: Vec<EnvState>,
    prefs: Prefs,
    balancing: LoadBalancing,
    expanded: HashMap<String, Expanded>,
    menu: Option<Menu>,
    icon_menu: bool,
    confirm: Option<Confirm>,
    removing: Option<String>,
    local_busy: bool,
    local_error: Option<String>,
    balancing_open: bool,
    sharing_open: bool,
    add: Option<add::AddDialog>,
}

impl gpui::EventEmitter<Notice> for ConnectionsPanel {}

impl ConnectionsPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut changes = cloud::service().subscribe();
        let state = changes.borrow_and_update().clone();
        cx.spawn(async move |this, cx| {
            while changes.changed().await.is_ok() {
                let state = changes.borrow_and_update().clone();
                if this
                    .update(cx, |panel, cx| {
                        panel.cloud = state;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let mut envs = environment_state::subscribe();
        cx.spawn(async move |this, cx| {
            while envs.changed().await.is_ok() {
                if this.update(cx, |panel, cx| panel.refresh(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        // The "retrying in Ns" clause counts down; nothing else here moves on its own.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |panel, cx| {
                    if panel.envs.iter().any(|env| env.retry_at_ms.is_some()) {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            cloud: state,
            wide: true,
            envs: environment_state::snapshot(),
            prefs: environment_prefs::load(),
            balancing: LoadBalancing::load(),
            expanded: HashMap::new(),
            menu: None,
            icon_menu: false,
            confirm: None,
            removing: None,
            local_busy: false,
            local_error: None,
            balancing_open: false,
            sharing_open: false,
            add: None,
        }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.envs = environment_state::snapshot();
        self.prefs = environment_prefs::load();
        cx.notify();
    }

    fn notice(
        &mut self,
        title: impl Into<String>,
        description: Option<String>,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        cx.emit(Notice {
            title: title.into(),
            description,
            error,
        });
    }

    fn open_menu(&mut self, menu: Option<Menu>, cx: &mut Context<Self>) {
        self.menu = menu;
        self.icon_menu = false;
        cx.notify();
    }

    /// Escape: a menu, then the add dialog or a confirmation. Returns whether it closed one.
    pub fn close_menu(&mut self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.menu.take().is_some() {
            self.icon_menu = false;
            cx.notify();
            return true;
        }
        if self.add.is_some() {
            if !self.add.as_ref().is_some_and(|add| add.busy) {
                self.add = None;
                cx.notify();
            }
            return true;
        }
        if self.confirm.is_some() && !self.local_busy && self.removing.is_none() {
            self.confirm = None;
            self.local_error = None;
            cx.notify();
            return true;
        }
        false
    }

    fn toggle(&mut self, id: &str, edit: impl FnOnce(&mut Expanded), cx: &mut Context<Self>) {
        edit(self.expanded.entry(id.to_owned()).or_default());
        cx.notify();
    }

    fn set_enabled(&mut self, id: &str, enabled: bool, cx: &mut Context<Self>) {
        environment_prefs::set_enabled(id, enabled);
        self.refresh(cx);
    }

    fn remove(&mut self, id: String, cx: &mut Context<Self>) {
        let Some((store, runtime)) = crate::environments::services() else {
            self.confirm = None;
            cx.notify();
            return;
        };
        self.removing = Some(id.clone());
        cx.notify();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let target = id.clone();
        runtime.spawn(async move {
            let _ = tx.send(crate::pairing::forget(&store, &target).await);
        });
        cx.spawn(async move |this, cx| {
            let result = rx
                .await
                .unwrap_or_else(|_| Err("Removal was interrupted.".into()));
            let _ = this.update(cx, |panel, cx| {
                panel.removing = None;
                panel.confirm = None;
                match result {
                    Ok(()) => crate::environments::detach(&id),
                    Err(message) => {
                        panel.notice("Could not remove environment", Some(message), true, cx)
                    }
                }
                panel.refresh(cx);
            });
        })
        .detach();
    }

    fn confirm_local(&mut self, cx: &mut Context<Self>) {
        // This app bundles no server, so there is nothing to restart.
        self.local_busy = false;
        self.local_error =
            Some("Turning the local environment on or off needs the desktop shell.".into());
        cx.notify();
    }

    fn copy(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// `Popover` anchored under `trigger`, open while `menu` is the panel's open menu.
    fn popup(
        &self,
        id: impl Into<SharedString>,
        menu: Menu,
        anchor: Anchor,
        trigger: impl IntoElement,
        content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        view: &Entity<Self>,
    ) -> AnyElement {
        let open = self.menu.as_ref() == Some(&menu);
        let (opened, target) = (view.clone(), menu);
        Popover::new(id.into())
            .anchor(anchor)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change(move |next, _, cx| {
                let menu = target.clone();
                opened.update(cx, |this, cx| {
                    this.open_menu(next.then_some(menu), cx);
                })
            })
            .trigger(PopTrigger::new(trigger))
            .content(move |_, window, cx| content(window, cx))
            .into_any_element()
    }
}

fn eid(prefix: &str, id: &str) -> ElementId {
    ElementId::Name(format!("{prefix}-{id}").into())
}

fn small_link(
    id: ElementId,
    label: impl Into<SharedString>,
    open: Option<bool>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = Tokens::of(cx);
    let (muted, foreground) = (theme.muted_foreground, theme.foreground);
    let chevron = open.map(|open| {
        let icon = Icon::new(Icons::ChevronRight).size(px(12.));
        if open {
            icon.rotate(gpui::percentage(0.25))
        } else {
            icon
        }
    });
    h_flex()
        .id(id)
        .items_center()
        .gap(px(2.))
        .text_xs()
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.text_color(foreground))
        .child(label.into())
        .children(chevron)
}

impl Render for ConnectionsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let envs = self.envs.clone();
        let cloud = self.cloud.clone();
        let wide = self.wide;

        let local_enabled = true;
        let local = self.local_row(local_enabled, &view, window, cx);
        let this_app = section("This app", group(vec![local], cx));

        let mut rows: Vec<AnyElement> = Vec::new();
        for env in &envs {
            rows.push(self.env_row(env, &view, window, cx));
        }
        if rows.is_empty() {
            rows.push(self.empty_state(cloud.signed_in, cx));
        }
        let add_button = {
            let view = view.clone();
            xs_button("add-environment", Variant::GhostMuted, true, cx)
                .child(Icon::new(Icons::Plus).size(px(12.)))
                .child("Add environment")
                .on_click(move |_, window, cx| {
                    view.update(cx, |this, cx| this.open_add(None, window, cx))
                })
                .into_any_element()
        };
        let environments = section_with_action("Environments", Some(add_button), group(rows, cx));

        let balancing = self.balancing_card(&envs, &view, window, cx);
        let sharing = self.sharing_card(&envs, &view, window, cx);

        v_flex()
            .gap(px(24.))
            .child(this_app)
            .child(environments)
            .children(balancing)
            .children(sharing)
            .child(self.t3_connect(&cloud, wide, cx))
    }
}

impl ConnectionsPanel {
    /// `LocalEnvironmentSetting`: a description and the switch.
    fn local_row(
        &self,
        enabled: bool,
        view: &Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let description = if enabled {
            "Run agents on this computer. Turn off to use T3 Code only with remote environments."
        } else {
            "Turned off. Agents only run in remote environments."
        };
        let toggle = {
            let view = view.clone();
            switch(
                "local-environment",
                enabled,
                Rc::new(move |_, cx| {
                    view.update(cx, |this, cx| {
                        this.local_error = None;
                        this.confirm = Some(Confirm::Local);
                        cx.notify();
                    })
                }),
                window,
                cx,
            )
        };
        h_flex()
            .items_center()
            .justify_between()
            .gap_4()
            .px_4()
            .py(px(12.))
            .child(note(description, cx).flex_1().min_w_0())
            .child(toggle)
            .into_any_element()
    }

    fn empty_state(&self, cloud: bool, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let description = if cloud {
            "Click \u{201c}Add environment\u{201d} to pair another environment, or connect one from T3 Connect."
        } else {
            "Click \u{201c}Add environment\u{201d} to pair another environment."
        };
        v_flex()
            .items_center()
            .gap(px(8.))
            .px_4()
            .py(px(32.))
            .text_center()
            .child(
                div()
                    .size(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(theme.muted)
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(Icons::ChevronsLeftRightEllipsis).size(px(20.))),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("No saved remote environments"),
            )
            .child(note(description, cx))
            .into_any_element()
    }

    /// `SavedBackendListRow` over `EnvironmentRow`.
    fn env_row(
        &self,
        env: &EnvState,
        view: &Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let id = env.id.clone();
        let enabled_pref = self.prefs.enabled(&id);
        let unsupported = env.phase == Phase::Unsupported;
        let blocked = unsupported && !env.saved;
        let enabled = enabled_pref && !unsupported;
        let expanded = self.expanded.get(&id).copied().unwrap_or_default();
        let removing = self.removing.as_deref() == Some(id.as_str());
        let host = presentation::display_url(&env.url).unwrap_or_else(|| env.label.clone());
        let diag: Option<Diagnosis> = if enabled {
            diagnosis::present(&Input {
                host: host.clone(),
                offline: env.phase == Phase::Offline,
                failure: env.failure.clone(),
                attempt: env.attempt,
                retry_at_ms: env.retry_at_ms,
                via: env.via.unwrap_or(Via::Direct),
                client_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                server_version: env.server_version.clone(),
            })
        } else {
            None
        };
        let (status, tone) = presentation::status(env, enabled_pref);
        let routes = presentation::routes(env);
        let subtitle = presentation::subtitle(env, enabled_pref, diag.is_some());

        let title = h_flex().items_center().gap_2().min_w_0().child(
            div()
                .text_sm()
                .line_height(px(20.))
                .font_weight(FontWeight::MEDIUM)
                .truncate()
                .child(env.label.clone()),
        );

        let mut line = h_flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(4.))
            .text_xs()
            .line_height(px(16.))
            .text_color(if tone == Tone::Error && diag.is_none() {
                theme.destructive
            } else {
                theme.muted_foreground
            })
            .child(div().min_w_0().truncate().child(subtitle));
        if !routes.is_empty() {
            let click = {
                let (view, id) = (view.clone(), id.clone());
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    view.update(cx, |this, cx| {
                        this.toggle(&id, |e| e.routes = !e.routes, cx)
                    })
                }
            };
            line = line.child("\u{b7}").child(
                small_link(
                    eid("routes", &id),
                    presentation::routes_button_label(routes.len()),
                    Some(expanded.routes),
                    cx,
                )
                .on_click(click),
            );
        }
        let click = {
            let (view, id) = (view.clone(), id.clone());
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                view.update(cx, |this, cx| {
                    this.toggle(&id, |e| e.permissions = !e.permissions, cx)
                })
            }
        };
        line = line.child("\u{b7}").child(
            small_link(
                eid("permissions", &id),
                "Permissions",
                Some(expanded.permissions),
                cx,
            )
            .on_click(click),
        );

        let summary = diag
            .as_ref()
            .map(|diag| self.diagnosis_summary(env, diag, expanded.details, view, cx));

        let mut detail = v_flex().gap_0();
        if let (true, Some(diag)) = (expanded.details, &diag) {
            detail = detail.child(self.diagnosis_panel(env, &host, diag, view, cx));
        }
        if expanded.routes && !routes.is_empty() {
            detail = detail.child(self.routes_list(env, &routes, view, cx));
        }
        if expanded.permissions {
            detail = detail.child(self.permissions(env, routes.len() > 1, cx));
        }

        let toggle = {
            let (view, target) = (view.clone(), id.clone());
            let on = enabled;
            let control = switch(
                eid("env-switch", &id),
                enabled,
                Rc::new(move |_, cx| {
                    if !blocked && !removing {
                        view.update(cx, |this, cx| this.set_enabled(&target, !on, cx))
                    }
                }),
                window,
                cx,
            );
            div()
                .when(blocked || removing, |control| control.opacity(0.5))
                .child(control)
        };

        let menu_trigger = icon_button(eid("env-menu", &id), Variant::GhostMuted, !removing, cx)
            .size(px(24.))
            .child(Icon::new(Icons::Ellipsis).size(px(16.)));
        let menu = self.row_menu(env, menu_trigger, view);

        let top = h_flex()
            .items_start()
            .gap(px(12.))
            .child(
                div()
                    .mt(px(2.))
                    .flex_shrink_0()
                    .text_color(theme.muted_foreground)
                    .when(!enabled, |icon| icon.opacity(0.6))
                    .child(Icon::new(machine_icon(env.machine)).size(px(ICON_SIZE))),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(title)
                    .child(line)
                    .children(summary),
            )
            .child(h_flex().items_center().gap_1().child(toggle).child(menu));
        let _ = status;
        v_flex()
            .px_3()
            .py(px(10.))
            .child(top)
            .when(
                expanded.details || expanded.routes || expanded.permissions,
                |row| row.child(div().ml(px(ICON_SIZE + 12.)).child(detail)),
            )
            .into_any_element()
    }

    /// `ConnectionDiagnosisSummary`.
    fn diagnosis_summary(
        &self,
        env: &EnvState,
        diag: &Diagnosis,
        open: bool,
        view: &Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let retry = diagnosis::retry_text(env.retry_at_ms, crate::auth::now_ms());
        let click = {
            let (view, id) = (view.clone(), env.id.clone());
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                view.update(cx, |this, cx| {
                    this.toggle(&id, |e| e.details = !e.details, cx)
                })
            }
        };
        let mut second = h_flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(4.))
            .text_xs()
            .line_height(px(16.))
            .text_color(theme.muted_foreground);
        if let Some(retry) = retry {
            second = second.child(retry).child("\u{b7}");
        }
        if env.attempt > 0 {
            second = second
                .child(format!("attempt {}", env.attempt))
                .child("\u{b7}");
        }
        second = second
            .child(small_link(eid("details", &env.id), "Details", Some(open), cx).on_click(click));
        v_flex()
            .child(
                div()
                    .text_xs()
                    .line_height(px(16.))
                    .text_color(theme.destructive)
                    .child(diag.headline.clone()),
            )
            .child(second)
            .into_any_element()
    }

    /// The Details area: hops, what to try, the key/value facts and the buttons.
    fn diagnosis_panel(
        &self,
        env: &EnvState,
        host: &str,
        diag: &Diagnosis,
        view: &Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let hops = diag.hops.iter().map(|hop| {
            let (mark, color, note_text) = match hop.result {
                HopResult::Ok => ("\u{2713}", theme.foreground, None),
                HopResult::Failed => ("\u{2715}", theme.destructive, Some("Failed")),
                HopResult::Unchecked => ("\u{2013}", theme.muted_foreground, Some("Not checked")),
            };
            h_flex()
                .items_start()
                .gap_2()
                .child(div().w(px(12.)).text_color(color).child(mark))
                .child(div().text_color(color).child(hop.label.clone()))
                .children(
                    note_text.map(|text| div().text_color(theme.muted_foreground).child(text)),
                )
        });
        let steps = v_flex()
            .gap(px(4.))
            .child(div().font_weight(FontWeight::MEDIUM).child("What to try"))
            .children(diag.steps.iter().enumerate().map(|(index, step)| {
                h_flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .w(px(16.))
                            .text_color(theme.muted_foreground)
                            .child(format!("{}.", index + 1)),
                    )
                    .child(div().flex_1().child(step.clone()))
            }))
            .children(diag.more_info_url.clone().map(|url| {
                div()
                    .id(eid("more-info", &env.id))
                    .cursor_pointer()
                    .underline()
                    .child("More info")
                    .on_click(move |_, _, _| cloud::open_in_browser(&url))
            }));
        let facts = v_flex()
            .gap(px(2.))
            .font_family("monospace")
            .children(diag.facts.iter().map(|(key, value)| {
                h_flex()
                    .gap_3()
                    .child(
                        div()
                            .w(px(112.))
                            .flex_shrink_0()
                            .text_color(theme.muted_foreground)
                            .child(key.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(theme.foreground)
                            .child(value.clone()),
                    )
            }));
        let copy = {
            let (view, text) = (view.clone(), diagnosis::text(host, diag));
            xs_button(eid("copy-diagnostics", &env.id), Variant::Outline, true, cx)
                .child("Copy diagnostics")
                .on_click(move |_, _, cx| {
                    let text = text.clone();
                    view.update(cx, |this, cx| this.copy(text, cx))
                })
        };
        let relay_only = env.via == Some(Via::Relay)
            && !presentation::routes(env).iter().any(|route| !route.relay);
        let add_direct = relay_only.then(|| {
            let (view, id, label) = (view.clone(), env.id.clone(), env.label.clone());
            xs_button(eid("add-direct", &env.id), Variant::Outline, true, cx)
                .child("Add direct route")
                .on_click(move |_, window, cx| {
                    let target = (id.clone(), label.clone());
                    view.update(cx, |this, cx| this.open_add(Some(target), window, cx))
                })
        });
        v_flex()
            .mt_2()
            .gap_3()
            .border_t_1()
            .border_color(theme.border.opacity(0.5))
            .pt_2()
            .text_xs()
            .line_height(px(16.))
            .child(v_flex().gap(px(4.)).children(hops))
            .child(steps)
            .child(facts)
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    // "Retry now" needs a wake-up the follower does not have yet.
                    .child(
                        xs_button(eid("retry-now", &env.id), Variant::Outline, false, cx)
                            .child("Retry now"),
                    )
                    .child(copy)
                    .children(add_direct),
            )
            .into_any_element()
    }

    /// `EnvironmentRoutesList`.
    fn routes_list(
        &self,
        env: &EnvState,
        routes: &[presentation::RouteRow],
        view: &Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let items = routes.iter().enumerate().map(|(index, route)| {
            let badge = route.in_use.then(|| {
                div()
                    .px(px(6.))
                    .rounded(px(4.))
                    .text_size(px(10.))
                    .line_height(px(16.))
                    .bg(gpui::Hsla::from(gpui::rgb(0x22c55e)).opacity(0.12))
                    .text_color(gpui::rgb(if theme.dark { 0x4ade80 } else { 0x15803d }))
                    .child("In use")
            });
            h_flex()
                .items_center()
                .gap_2()
                .py(px(4.))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(route.label.clone()),
                                )
                                .children(badge),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .line_height(px(14.))
                                .text_color(theme.muted_foreground)
                                .truncate()
                                .child(route.address.clone()),
                        ),
                )
                .child(
                    div()
                        .text_color(theme.muted_foreground.opacity(0.4))
                        .child(Icon::new(Icons::GripVertical).size(px(14.))),
                )
                .id(("route", index))
        });
        let add_route = {
            let (view, id, label) = (view.clone(), env.id.clone(), env.label.clone());
            button(eid("add-route", &env.id), Variant::GhostMuted, true, cx)
                .h(px(24.))
                .px(px(7.))
                .text_size(px(12.))
                .child(Icon::new(Icons::Plus).size(px(12.)))
                .child("Add route")
                .on_click(move |_, window, cx| {
                    let target = (id.clone(), label.clone());
                    view.update(cx, |this, cx| this.open_add(Some(target), window, cx))
                })
        };
        v_flex()
            .mt_2()
            .border_t_1()
            .border_color(theme.border.opacity(0.5))
            .py_2()
            .child(v_flex().children(items))
            .child(div().child(add_route))
            .into_any_element()
    }

    /// `SessionPermissions`.
    fn permissions(&self, env: &EnvState, route_context: bool, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let body = if env.phase == Phase::Connected {
            let intro = if route_context {
                "These permissions apply to your active route. Other routes may have different permissions."
            } else {
                "These permissions apply to this client\u{2019}s current connection."
            };
            v_flex()
                .gap_3()
                .child(div().text_color(theme.muted_foreground).child(intro))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(24.))
                        .gap_y(px(8.))
                        .children(SCOPES.iter().map(|title| {
                            h_flex()
                                .w(px(260.))
                                .items_start()
                                .justify_between()
                                .gap_3()
                                .child(div().child(*title))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_color(theme.muted_foreground)
                                        .child("Unknown"),
                                )
                        })),
                )
                .into_any_element()
        } else {
            div()
                .text_color(theme.muted_foreground)
                .child("Connect to this environment to view this session\u{2019}s permissions.")
                .into_any_element()
        };
        div()
            .mt_2()
            .border_t_1()
            .border_color(theme.border.opacity(0.5))
            .py_3()
            .text_xs()
            .line_height(px(16.))
            .child(body)
            .into_any_element()
    }

    /// The row's "..." menu.
    fn row_menu(
        &self,
        env: &EnvState,
        trigger: impl IntoElement,
        view: &Entity<Self>,
    ) -> AnyElement {
        let env = env.clone();
        let menu = Menu::Row(env.id.clone());
        let icon_open = self.icon_menu;
        let expanded = self.expanded.get(&env.id).copied().unwrap_or_default();
        let routes = presentation::routes(&env);
        let mcp = presentation::mcp_url(&env);
        let trace = env
            .failure
            .as_ref()
            .and_then(|failure| failure.trace_id.clone());
        let connected = env.phase == Phase::Connected;
        let removable = env.saved;
        let panel = view.clone();
        self.popup(
            eid("env-menu-popup", &env.id).to_string(),
            menu,
            Anchor::TopRight,
            trigger,
            move |_, cx| {
                let theme = Tokens::of(cx);
                let mut items = popup_shell(cx).w(px(224.)).p_1();
                let toggle_icon = {
                    let panel = panel.clone();
                    menu_item(eid("menu-icon", &env.id), "Icon", false, cx)
                        .justify_between()
                        .child(Icon::new(Icons::ChevronRight).size(px(14.)))
                        .on_click(move |_, _, cx| {
                            panel.update(cx, |this, cx| {
                                this.icon_menu = !this.icon_menu;
                                cx.notify();
                            })
                        })
                };
                items = items.child(toggle_icon);
                if icon_open {
                    let lock = if connected {
                        "Your session on this environment cannot change its settings."
                    } else {
                        "Connect to this environment to change its icon."
                    };
                    items = items
                        .child(div().px_2().py_1().text_xs().text_color(theme.muted_foreground).child(lock))
                        .child(div().h(px(1.)).my_1().bg(theme.border.opacity(0.5)));
                    for (kind, label) in MACHINES {
                        let detected = kind == env.machine;
                        items = items.child(
                            h_flex()
                                .min_h(px(28.))
                                .items_center()
                                .gap_2()
                                .px_2()
                                .py_1()
                                .pl(px(16.))
                                .text_sm()
                                .opacity(0.64)
                                .child(Icon::new(machine_icon(kind)).size(px(14.)))
                                .child(label)
                                .when(detected, |row| {
                                    row.child(
                                        div().text_xs().text_color(theme.muted_foreground).child("detected"),
                                    )
                                }),
                        );
                    }
                    items = items.child(div().h(px(1.)).my_1().bg(theme.border.opacity(0.5)));
                }
                if !routes.is_empty() {
                    let panel = panel.clone();
                    let id = env.id.clone();
                    let label = if expanded.routes { "Hide routes" } else { "Routes" };
                    items = items.child(
                        menu_item(eid("menu-routes", &env.id), label, false, cx)
                            .child(Icon::new(Icons::Route).size(px(14.)))
                            .on_click(move |_, _, cx| {
                                panel.update(cx, |this, cx| {
                                    this.toggle(&id, |e| e.routes = !e.routes, cx);
                                    this.open_menu(None, cx);
                                })
                            }),
                    );
                }
                if let Some(url) = mcp.clone() {
                    let panel = panel.clone();
                    items = items.child(menu_item(eid("menu-mcp", &env.id), "Copy MCP URL", false, cx).on_click(
                        move |_, _, cx| {
                            let url = url.clone();
                            panel.update(cx, |this, cx| {
                                this.copy(url.clone(), cx);
                                this.notice(
                                    "MCP URL copied",
                                    Some(format!("Add it to an agent, e.g. claude mcp add --transport http t3 {url}")),
                                    false,
                                    cx,
                                );
                                this.open_menu(None, cx);
                            })
                        },
                    ));
                }
                if let Some(trace) = trace.clone() {
                    let panel = panel.clone();
                    items = items.child(menu_item(eid("menu-trace", &env.id), "Copy trace ID", false, cx).on_click(
                        move |_, _, cx| {
                            let trace = trace.clone();
                            panel.update(cx, |this, cx| {
                                this.copy(trace, cx);
                                this.open_menu(None, cx);
                            })
                        },
                    ));
                }
                if removable {
                    let panel = panel.clone();
                    let id = env.id.clone();
                    items = items
                        .child(div().h(px(1.)).my_1().bg(theme.border.opacity(0.5)))
                        .child(
                            menu_item(eid("menu-remove", &env.id), "Remove from this device\u{2026}", true, cx)
                                .on_click(move |_, _, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.confirm = Some(Confirm::Remove(id.clone()));
                                        this.open_menu(None, cx);
                                    })
                                }),
                        );
                }
                items.into_any_element()
            },
            view,
        )
    }

    /// `FoldedSettingsSection`: a closed-by-default card with a title, a summary and an optional
    /// control at the right of the header.
    #[allow(clippy::too_many_arguments)]
    fn folded(
        &self,
        id: &'static str,
        title: &'static str,
        summary: String,
        control: Option<AnyElement>,
        open: bool,
        on_toggle: impl Fn(&mut Window, &mut App) + 'static,
        rows: Vec<AnyElement>,
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let divider = theme.border.opacity(0.5);
        let chevron = Icon::new(Icons::ChevronRight)
            .size(px(16.))
            .text_color(theme.muted_foreground);
        let chevron = if open {
            chevron.rotate(gpui::percentage(0.25))
        } else {
            chevron
        };
        let trigger = h_flex()
            .id(id)
            .flex_1()
            .min_w_0()
            .min_h(px(44.))
            .items_center()
            .gap_2()
            .cursor_pointer()
            .on_click(move |_, window, cx| on_toggle(window, cx))
            .child(chevron)
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(summary),
            );
        let header = h_flex()
            .flex_wrap()
            .items_center()
            .gap_x_4()
            .px_3()
            .child(trigger)
            .children(control.map(|control| div().py(px(6.)).child(control)));
        let mut panel = v_flex().border_t_1().border_color(divider);
        for (index, row) in rows.into_iter().enumerate() {
            if index > 0 {
                panel = panel.child(div().h(px(1.)).w_full().bg(divider));
            }
            panel = panel.child(row);
        }
        super::components::card(cx)
            .child(header)
            .when(open, |card| card.child(panel))
            .into_any_element()
    }

    /// An environment row inside a folded card: the kind icon, the label, the transport.
    fn card_row(&self, env: &EnvState, control: AnyElement, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        h_flex()
            .items_center()
            .gap(px(12.))
            .px_3()
            .py(px(10.))
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(machine_icon(env.machine)).size(px(ICON_SIZE))),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .line_height(px(20.))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(env.label.clone()),
                    )
                    .child(note(presentation::transport_label(env), cx).truncate()),
            )
            .child(control)
            .into_any_element()
    }

    /// A `Select size="xs"` (`w-32`) of `labels`, the chosen one first.
    #[allow(clippy::too_many_arguments)]
    fn select(
        &self,
        id: SharedString,
        menu: Menu,
        current: &'static str,
        labels: Vec<&'static str>,
        enabled: bool,
        on_pick: impl Fn(&mut Self, usize, &mut Context<Self>) + Clone + 'static,
        view: &Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let trigger = h_flex()
            .id(id.clone())
            .w(px(128.))
            .h(px(24.))
            .items_center()
            .gap(px(6.))
            .px(px(7.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(if theme.dark {
                theme.input.opacity(0.32)
            } else {
                theme.popover
            })
            .text_xs()
            .when(enabled, |trigger| trigger.cursor_pointer())
            .when(!enabled, |trigger| trigger.opacity(0.64))
            .child(div().flex_1().truncate().child(current))
            .child(
                div()
                    .opacity(0.7)
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            );
        if !enabled {
            return trigger.into_any_element();
        }
        let panel = view.clone();
        self.popup(
            format!("{id}-popup"),
            menu,
            Anchor::TopRight,
            trigger,
            move |_, cx| {
                let theme = Tokens::of(cx);
                let accent = theme.accent;
                popup_shell(cx)
                    .w(px(128.))
                    .p_1()
                    .children(labels.iter().enumerate().map(|(index, label)| {
                        let (panel, on_pick) = (panel.clone(), on_pick.clone());
                        div()
                            .id((SharedString::from(format!("{id}-item")), index))
                            .min_h(px(28.))
                            .flex()
                            .items_center()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .text_sm()
                            .cursor_pointer()
                            .when(*label == current, |item| {
                                item.bg(theme.foreground.opacity(0.08))
                            })
                            .hover(move |style| style.bg(accent))
                            .child(*label)
                            .on_click(move |_, _, cx| {
                                let on_pick = on_pick.clone();
                                panel.update(cx, |this, cx| {
                                    on_pick(this, index, cx);
                                    this.open_menu(None, cx);
                                })
                            })
                    }))
                    .into_any_element()
            },
            view,
        )
    }

    /// `LoadBalancingSettings`: hidden with fewer than two environments.
    fn balancing_card(
        &self,
        envs: &[EnvState],
        view: &Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let usable: Vec<&EnvState> = envs
            .iter()
            .filter(|env| self.prefs.enabled(&env.id) && env.phase != Phase::Unsupported)
            .collect();
        if usable.len() + 1 < 2 {
            return None;
        }
        let enabled = self.balancing.enabled;
        let named: Vec<(String, String)> = usable
            .iter()
            .map(|env| (env.id.clone(), env.label.clone()))
            .collect();
        let summary = if enabled {
            summarize(&self.balancing, &named).unwrap_or_default()
        } else {
            "Off".to_owned()
        };
        let toggle = {
            let panel = view.clone();
            switch(
                "load-balancing-switch",
                enabled,
                Rc::new(move |_, cx| {
                    panel.update(cx, |this, cx| {
                        this.balancing.enabled = !this.balancing.enabled;
                        this.balancing.save();
                        cx.notify();
                    })
                }),
                window,
                cx,
            )
        };
        let mut rows: Vec<AnyElement> = vec![
            note(
                "New threads in shared projects start on the machine with the most free CPU and memory, weighted by each machine's preference.",
                cx,
            )
            .px_3()
            .py(px(10.))
            .into_any_element(),
        ];
        for env in usable {
            let current = self.balancing.preference(&env.id);
            let id = env.id.clone();
            let select = self.select(
                format!("lb-{}", env.id).into(),
                Menu::Preference(env.id.clone()),
                current.label(),
                Preference::ALL.iter().map(|p| p.label()).collect(),
                enabled,
                move |this, index, _| {
                    let preference = Preference::ALL[index];
                    this.balancing
                        .weights
                        .insert(id.clone(), preference.weight());
                    this.balancing.save();
                },
                view,
                cx,
            );
            rows.push(self.card_row(env, select, cx));
        }
        let panel = view.clone();
        Some(self.folded(
            "load-balancing",
            "Load balancing",
            summary,
            Some(toggle),
            self.balancing_open,
            move |_, cx| {
                panel.update(cx, |this, cx| {
                    this.balancing_open = !this.balancing_open;
                    cx.notify();
                })
            },
            rows,
            cx,
        ))
    }

    /// `PullRequestRoutingSettings`: hidden with fewer than two environments.
    fn sharing_card(
        &self,
        envs: &[EnvState],
        view: &Entity<Self>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let usable: Vec<&EnvState> = envs
            .iter()
            .filter(|env| self.prefs.enabled(&env.id) && env.phase != Phase::Unsupported)
            .collect();
        if usable.len() + 1 < 2 {
            return None;
        }
        let entries: Vec<(String, Sharing)> = usable
            .iter()
            .map(|env| (env.label.clone(), self.prefs.sharing_of(&env.id)))
            .collect();
        let summary = presentation::summarize_sharing(&entries).unwrap_or_else(|| "Off".into());
        let mut rows: Vec<AnyElement> = vec![
            note(
                "Machines you trust here can read PR data through each other's source control access. Enable both machines. Read and act may use broader permissions than the machine that owns them. This applies only to this device.",
                cx,
            )
            .px_3()
            .py(px(10.))
            .into_any_element(),
        ];
        for env in usable {
            let id = env.id.clone();
            let select = self.select(
                format!("pr-{}", env.id).into(),
                Menu::Sharing(env.id.clone()),
                self.prefs.sharing_of(&env.id).label(),
                Sharing::ALL.iter().map(|s| s.label()).collect(),
                true,
                move |this, index, cx| {
                    environment_prefs::set_sharing(&id, Sharing::ALL[index]);
                    this.refresh(cx);
                },
                view,
                cx,
            );
            rows.push(self.card_row(env, select, cx));
        }
        let panel = view.clone();
        Some(self.folded(
            "pull-request-sharing",
            "Pull request sharing",
            summary,
            None,
            self.sharing_open,
            move |_, cx| {
                panel.update(cx, |this, cx| {
                    this.sharing_open = !this.sharing_open;
                    cx.notify();
                })
            },
            rows,
            cx,
        ))
    }

    /// The T3 Connect account: the GPUI app signs in through the system browser.
    fn t3_connect(&self, state: &CloudState, wide: bool, cx: &App) -> gpui::Div {
        let description = if state.signed_in {
            "Signed in. Environments on your account connect through T3 Connect when they are not reachable directly."
        } else if state.signing_in {
            "Finish signing in in your browser."
        } else {
            "Sign in to reach the environments on your T3 Connect account from this device."
        };
        let control = if state.signed_in {
            button("t3-connect-sign-out", Variant::Outline, true, cx)
                .child("Sign out")
                .on_click(|_, _, _| cloud::spawn_sign_out())
        } else {
            button(
                "t3-connect-sign-in",
                Variant::Primary,
                !state.signing_in,
                cx,
            )
            .child(if state.signing_in {
                "Waiting for browser"
            } else {
                "Sign in"
            })
            .when(!state.signing_in, |sign_in| {
                sign_in.on_click(|_, _, _| cloud::spawn_sign_in())
            })
        };
        section(
            "T3 Connect",
            group(
                vec![
                    super::components::row(
                        "Account",
                        description,
                        None,
                        control.into_any_element(),
                        wide,
                        cx,
                    )
                    .into_any_element(),
                ],
                cx,
            ),
        )
        .children(
            state
                .error
                .clone()
                .map(|message| div().px_4().child(problem(message, cx))),
        )
        .child(div().px_4().child(note(
            "Signing in opens your browser and returns here when it is done.",
            cx,
        )))
    }

    /// The add dialog or a confirmation, over a backdrop.
    pub fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let view = cx.entity();
        if let Some(confirm) = self.confirm.clone() {
            return Some(match confirm {
                Confirm::Local => self.local_confirm(&view, window, cx),
                Confirm::Remove(id) => {
                    let label = self
                        .envs
                        .iter()
                        .find(|env| env.id == id)
                        .map_or_else(|| id.clone(), |env| env.label.clone());
                    let busy = self.removing.is_some();
                    let (accept, dismiss) = (view.clone(), view.clone());
                    confirm_dialog(
                        ConfirmSpec::from_message(
                            "remove-environment",
                            &format!(
                                "Remove {label} from this device?\nThis forgets its pairing, credentials, and cached threads here. Switch it off instead to keep it saved."
                            ),
                        )
                        .destructive()
                        .confirm_label("Remove")
                        .busy(busy),
                        move |_, cx| accept.update(cx, |this, cx| this.remove(id.clone(), cx)),
                        move |_, cx| {
                            dismiss.update(cx, |this, cx| {
                                this.confirm = None;
                                cx.notify();
                            })
                        },
                        window,
                        cx,
                    )
                }
            });
        }
        self.add.as_ref()?;
        Some(self.add_view(&view, window, cx))
    }

    fn local_confirm(
        &mut self,
        view: &Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = true;
        let busy = self.local_busy;
        let (title, body, label) = if enabled {
            (
                "Turn off local environment?",
                "T3 Code will restart without running a server on this computer. Any agents and terminals running here will stop, and other devices will no longer be able to connect to this computer. Your projects, history, and remote environments are unaffected.",
                if busy {
                    "Restarting\u{2026}"
                } else {
                    "Restart and turn off"
                },
            )
        } else {
            (
                "Turn on local environment?",
                "T3 Code will restart and start running a server on this computer again.",
                if busy {
                    "Restarting\u{2026}"
                } else {
                    "Restart and turn on"
                },
            )
        };
        let error = self.local_error.clone().map(|message| {
            div()
                .px_6()
                .pb_4()
                .text_sm()
                .text_color(Tokens::of(cx).destructive)
                .child(if message.is_empty() {
                    "Couldn't change this setting.".to_owned()
                } else {
                    message
                })
                .into_any_element()
        });
        let mut spec = ConfirmSpec::titled("local-environment-confirm", title, body)
            .confirm_label(label)
            .busy(busy)
            .extra(error.into_iter().collect());
        if enabled {
            spec = spec.destructive();
        }
        let (accept, dismiss) = (view.clone(), view.clone());
        confirm_dialog(
            spec,
            move |_, cx| accept.update(cx, |this, cx| this.confirm_local(cx)),
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.confirm = None;
                    this.local_error = None;
                    cx.notify();
                })
            },
            window,
            cx,
        )
    }
}

/// `AUTH_SCOPE_OPTIONS` titles (packages/shared authScopeOptions).
const SCOPES: [&str; 16] = [
    "View environment",
    "Operate tasks",
    "Change environment settings",
    "Manage providers",
    "Maintain environment",
    "Control previews",
    "View diagnostics and usage",
    "View terminals",
    "Use terminals",
    "Change source control",
    "Read files",
    "Write files",
    "View access",
    "Manage access",
    "View relay",
    "Manage relay",
];
