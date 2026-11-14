//! `AddEnvironmentDialog.tsx`: pair another environment over a remote link, or name an SSH host.

use super::{Menu, *};
use crate::add_environment::{self, SshHost};
use crate::ui::settings::devices_panel::widgets::{dialog_of, field};
use gpui::Subscription;
use gpui_component::input::{InputEvent, InputState};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Remote,
    Ssh,
}

pub(super) struct AddDialog {
    mode: Mode,
    host: Entity<InputState>,
    code: Entity<InputState>,
    ssh_host: Entity<InputState>,
    ssh_user: Entity<InputState>,
    ssh_port: Entity<InputState>,
    /// The environment a route is being added to: its id and label.
    target: Option<(String, String)>,
    pub(super) busy: bool,
    error: Option<String>,
    /// `None` while `~/.ssh/config` is still being read.
    hosts: Option<Vec<SshHost>>,
    _subscriptions: Vec<Subscription>,
}

fn input(
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<ConnectionsPanel>,
) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

impl ConnectionsPanel {
    pub(super) fn open_add(
        &mut self,
        target: Option<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = input("backend.example.com", window, cx);
        let code = input("PAIRCODE", window, cx);
        let ssh_host = input("Search hosts or type devbox", window, cx);
        let ssh_user = input("root", window, cx);
        let ssh_port = input("22", window, cx);
        let mut subscriptions = Vec::new();
        // Pasting a full pairing URL into Host fills both fields.
        subscriptions.push(cx.subscribe_in(&host, window, {
            let code = code.clone();
            move |this, host, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let value = host.read(cx).value().to_string();
                    if let Some((base, token)) = add_environment::parse_pairing_url_fields(&value) {
                        host.update(cx, |state, cx| state.set_value(base, window, cx));
                        code.update(cx, |state, cx| state.set_value(token, window, cx));
                    }
                    this.clear_add_error(cx);
                }
                InputEvent::PressEnter { .. } => this.submit_add(cx),
                _ => {}
            }
        }));
        subscriptions.push(
            cx.subscribe(&code, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.clear_add_error(cx),
                InputEvent::PressEnter { .. } => this.submit_add(cx),
                _ => {}
            }),
        );
        subscriptions.push(cx.subscribe(
            &ssh_host,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => this.clear_add_error(cx),
                InputEvent::PressEnter { .. } => this.submit_add(cx),
                _ => {}
            },
        ));
        for state in [&ssh_user, &ssh_port] {
            subscriptions.push(cx.subscribe(
                state,
                |this, _, event: &InputEvent, cx| match event {
                    InputEvent::Change => this.clear_add_error(cx),
                    InputEvent::PressEnter { .. } => this.submit_add(cx),
                    _ => {}
                },
            ));
        }
        window.focus(&host.read(cx).focus_handle(cx), cx);
        self.add = Some(AddDialog {
            mode: Mode::Remote,
            host,
            code,
            ssh_host,
            ssh_user,
            ssh_port,
            target,
            busy: false,
            error: None,
            hosts: None,
            _subscriptions: subscriptions,
        });
        self.menu = None;
        cx.notify();
    }

    fn clear_add_error(&mut self, cx: &mut Context<Self>) {
        if self
            .add
            .as_mut()
            .is_some_and(|add| add.error.take().is_some())
        {
            cx.notify();
        }
    }

    fn set_add_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        let Some(add) = &mut self.add else { return };
        add.mode = mode;
        add.error = None;
        if mode == Mode::Ssh && add.hosts.is_none() {
            let task = cx.background_spawn(async { add_environment::discover_ssh_hosts() });
            cx.spawn(async move |this, cx| {
                let hosts = task.await;
                let _ = this.update(cx, |panel, cx| {
                    if let Some(add) = &mut panel.add {
                        add.hosts = Some(hosts);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn submit_add(&mut self, cx: &mut Context<Self>) {
        let Some(add) = &mut self.add else { return };
        if add.busy {
            return;
        }
        match add.mode {
            Mode::Remote => {
                let host = add.host.read(cx).value().to_string();
                let code = add.code.read(cx).value().to_string();
                let (host, code) = match add_environment::parse_remote_fields(&host, &code) {
                    Ok(fields) => fields,
                    Err(message) => return self.add_failed(message, cx),
                };
                let Some((store, runtime)) = crate::environments::services() else {
                    return self.add_failed("Environments are not running yet.".into(), cx);
                };
                add.busy = true;
                add.error = None;
                let target = add.target.clone();
                let (tx, rx) = tokio::sync::oneshot::channel();
                runtime.spawn(async move {
                    let _ = tx.send(add_environment::pair_remote(store, host, code).await);
                });
                cx.spawn(async move |this, cx| {
                    let result = rx.await.unwrap_or_else(|_| Err("Pairing was interrupted.".into()));
                    let _ = this.update(cx, |panel, cx| match result {
                        Ok(credential) => {
                            let label = credential.label.clone();
                            crate::environments::attach(credential);
                            panel.add = None;
                            match target {
                                Some((_, name)) => panel.notice(
                                    "Route added",
                                    Some(format!("{name} now has another way to connect.")),
                                    false,
                                    cx,
                                ),
                                None => panel.notice(
                                    "Backend added",
                                    Some("The environment is saved and will reconnect on app startup.".into()),
                                    false,
                                    cx,
                                ),
                            }
                            let _ = label;
                            panel.refresh(cx);
                        }
                        Err(message) => {
                            if let Some(add) = &mut panel.add {
                                add.busy = false;
                            }
                            panel.add_failed(message, cx);
                        }
                    });
                })
                .detach();
                cx.notify();
            }
            Mode::Ssh => {
                let host = add.ssh_host.read(cx).value().to_string();
                let user = add.ssh_user.read(cx).value().to_string();
                let port = add.ssh_port.read(cx).value().to_string();
                match add_environment::parse_ssh_target(&host, &user, &port) {
                    Err(message) => self.add_failed(message, cx),
                    // The app has no SSH-managed tunnel yet.
                    Ok(_) => self.add_failed(
                        "SSH-managed tunnels are not available in this app yet.".into(),
                        cx,
                    ),
                }
            }
        }
    }

    fn add_failed(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(add) = &mut self.add {
            add.error = Some(message.clone());
        }
        self.notice("Could not add backend", Some(message), true, cx);
        cx.notify();
    }

    pub(super) fn add_view(
        &mut self,
        view: &Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let Some(add) = &self.add else {
            return div().into_any_element();
        };
        let routing = add.target.clone();
        let title = match &routing {
            Some((_, label)) => format!("Add a route to {label}"),
            None => "Add Environment".to_owned(),
        };
        let description = if routing.is_some() {
            "Pair this machine again over another address, such as its Tailscale name. It joins the existing routes instead of adding a second machine."
        } else {
            "Pair another environment to this client."
        };
        let card =
            |id: &'static str, mode: Mode, icon: Icons, label: &'static str, text: &'static str| {
                let selected = add.mode == mode;
                let panel = view.clone();
                h_flex()
                    .id(id)
                    .min_h(px(96.))
                    .flex_1()
                    .items_start()
                    .gap_3()
                    .p_4()
                    .rounded(px(8.))
                    .border_1()
                    .cursor_pointer()
                    .border_color(if selected {
                        theme.primary.opacity(0.5)
                    } else {
                        theme.border.opacity(0.6)
                    })
                    .when(selected, |card| card.bg(theme.primary.opacity(0.05)))
                    .when(!selected, |card| {
                        let hover = theme.muted.opacity(0.4);
                        card.hover(move |style| style.bg(hover))
                    })
                    .on_click(move |_, _, cx| {
                        panel.update(cx, |this, cx| this.set_add_mode(mode, cx))
                    })
                    .child(
                        div()
                            .size(px(32.))
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(if selected {
                                theme.primary.opacity(0.3)
                            } else {
                                theme.border.opacity(0.7)
                            })
                            .bg(if selected {
                                theme.primary.opacity(0.1)
                            } else {
                                theme.background
                            })
                            .text_color(if selected {
                                theme.primary
                            } else {
                                theme.muted_foreground
                            })
                            .child(Icon::new(icon).size(px(16.))),
                    )
                    .child(
                        v_flex()
                            .gap(px(2.))
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(text),
                            ),
                    )
            };
        let cards = h_flex()
            .gap_3()
            .child(card(
                "add-mode-remote",
                Mode::Remote,
                Icons::ChevronsLeftRightEllipsis,
                "Remote link",
                "Enter a backend host and pairing code.",
            ))
            .child(card(
                "add-mode-ssh",
                Mode::Ssh,
                Icons::Terminal,
                "SSH",
                "Use local SSH config, agent, and tunnels for the backend.",
            ));
        let busy = add.busy;
        let label = if busy {
            "Adding\u{2026}"
        } else if routing.is_some() {
            "Add route"
        } else {
            "Add environment"
        };
        let submit = {
            let panel = view.clone();
            button("add-environment-submit", Variant::Outline, !busy, cx)
                .w_full()
                .child(Icon::new(Icons::Plus).size(px(14.)))
                .child(label)
                .when(!busy, |button| {
                    button
                        .on_click(move |_, _, cx| panel.update(cx, |this, cx| this.submit_add(cx)))
                })
        };
        let error = add.error.clone().map(|message| match add.mode {
            Mode::Remote => problem(message, cx).into_any_element(),
            Mode::Ssh => div()
                .px_3()
                .py_2()
                .rounded(px(8.))
                .border_1()
                .border_color(theme.destructive.opacity(0.3))
                .bg(theme.destructive.opacity(0.05))
                .text_sm()
                .text_color(theme.destructive)
                .child(message)
                .into_any_element(),
        });
        let fields = match add.mode {
            Mode::Remote => {
                let helper = note(
                    "Paste a full pairing URL here to fill both fields automatically.",
                    cx,
                )
                .text_size(px(11.))
                .line_height(px(14.));
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_start()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap(px(6.))
                                    .child(div().text_sm().child("Host"))
                                    .child(field(&add.host, busy, window, cx)),
                            )
                            .child(
                                v_flex()
                                    .w(px(160.))
                                    .gap(px(6.))
                                    .child(div().text_sm().child("Pairing code"))
                                    .child(field(&add.code, busy, window, cx)),
                            ),
                    )
                    .child(helper)
                    .into_any_element()
            }
            Mode::Ssh => {
                let text = add.ssh_host.read(cx).value().to_string();
                let suggestions = self.ssh_suggestions(&text, view, cx);
                v_flex()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap(px(6.))
                            .child(div().text_sm().child("SSH host or alias"))
                            .child(field(&add.ssh_host, busy, window, cx))
                            .child(suggestions),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .items_start()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap(px(6.))
                                    .child(div().text_sm().child("Username"))
                                    .child(field(&add.ssh_user, busy, window, cx)),
                            )
                            .child(
                                v_flex()
                                    .w(px(112.))
                                    .gap(px(6.))
                                    .child(div().text_sm().child("Port"))
                                    .child(field(&add.ssh_port, busy, window, cx)),
                            ),
                    )
                    .into_any_element()
            }
        };
        let body = v_flex()
            .gap_4()
            .pb_4()
            .child(cards)
            .child(fields)
            .children(error)
            .child(submit)
            .into_any_element();
        let dismiss = view.clone();
        dialog_of(
            "add-environment-dialog",
            768.,
            title,
            description,
            vec![body],
            Vec::new(),
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    if !this.add.as_ref().is_some_and(|add| add.busy) {
                        this.add = None;
                        cx.notify();
                    }
                })
            },
            cx,
        )
    }

    /// The host list under the SSH field: loading, matches, or nothing found.
    fn ssh_suggestions(&self, text: &str, view: &Entity<Self>, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let Some(add) = &self.add else {
            return div().into_any_element();
        };
        let Some(hosts) = &add.hosts else {
            return note("Loading hosts\u{2026}", cx).into_any_element();
        };
        let matches = add_environment::filter_ssh_hosts(hosts, text);
        if matches.is_empty() {
            return if text.trim().is_empty() {
                div().into_any_element()
            } else {
                note(format!("No hosts match \"{}\".", text.trim()), cx).into_any_element()
            };
        }
        if matches.iter().any(|host| host.alias == text.trim()) {
            return div().into_any_element();
        }
        let accent = theme.accent;
        v_flex()
            .p_1()
            .max_h(px(160.))
            .overflow_hidden()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.foreground.opacity(0.1))
            .bg(theme.popover)
            .children(
                matches
                    .into_iter()
                    .take(6)
                    .enumerate()
                    .map(|(index, host)| {
                        let panel = view.clone();
                        let alias = host.alias.clone();
                        h_flex()
                            .id(("ssh-host", index))
                            .min_h(px(28.))
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .text_sm()
                            .cursor_pointer()
                            .hover(move |style| style.bg(accent))
                            .on_click(move |_, window, cx| {
                                let alias = alias.clone();
                                panel.update(cx, |this, cx| {
                                    if let Some(add) = &this.add {
                                        add.ssh_host.update(cx, |state, cx| {
                                            state.set_value(alias, window, cx)
                                        });
                                    }
                                })
                            })
                            .child(host.alias)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(host.hostname),
                            )
                    }),
            )
            .into_any_element()
    }
}

#[allow(dead_code)]
fn unused(_: Menu) {}
