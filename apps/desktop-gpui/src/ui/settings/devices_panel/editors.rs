//! The three host editors (DeviceHostEditor.tsx, EasDeviceHostEditor.tsx,
//! SimlockDeviceHostEditor.tsx) as one dialog: their drafts, checks and the host-list writes.

use super::super::scope_inheritance::{PopTrigger, popup_shell};
use super::super::tokens::Tokens;
use super::super::*;
use super::DevicesPanel;
use super::widgets::{
    Variant, button, dialog, field, icon_button, labeled, menu_item, note, problem, spinner,
    warning,
};
use crate::settings_devices::{
    self as devices, AddKind, Availability, DeviceConfig, EasHost, SimlockHost, Source, SshHost,
};
use crate::status::CallEvent;
use gpui::{ElementId, Subscription};
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};

type ConnectionKey = (String, Option<u16>, Option<String>);

enum Outcome {
    Local,
    Connected(Vec<Availability>),
}

/// `useDeviceHostConnectionCheck`: the check belongs to the destination it ran against.
enum Check {
    Idle,
    Running(ConnectionKey),
    Done(ConnectionKey, Result<Outcome, String>),
}

enum Account {
    Idle,
    Checking,
    Signed {
        who: String,
        source: String,
        apps: Vec<(String, String)>,
    },
    Failed(String),
}

enum Token {
    Keep,
    Set(String),
    Clear,
}

enum Lists {
    Ssh(Vec<SshHost>),
    Eas(Vec<EasHost>),
    Simlock(Vec<SimlockHost>),
}

/// A host-list write: what it sets and what a failure is called.
pub(super) struct Save {
    pub(super) title: &'static str,
    lists: Lists,
}

impl Save {
    pub(super) fn apply(&self, config: &mut DeviceConfig) {
        match &self.lists {
            Lists::Ssh(hosts) => config.device_hosts = hosts.clone(),
            Lists::Eas(hosts) => config.eas_device_hosts = hosts.clone(),
            Lists::Simlock(hosts) => config.simlock_device_hosts = hosts.clone(),
        }
    }

    pub(super) fn patch(&self) -> Value {
        match &self.lists {
            Lists::Ssh(hosts) => devices::hosts_patch("deviceHosts", hosts),
            Lists::Eas(hosts) => devices::hosts_patch("easDeviceHosts", hosts),
            Lists::Simlock(hosts) => devices::hosts_patch("simlockDeviceHosts", hosts),
        }
    }
}

/// The write that removes a host.
pub(super) fn removal(source: &Source, config: &DeviceConfig) -> Option<Save> {
    Some(match source {
        Source::Local => return None,
        Source::Ssh(host) => Save {
            title: "Device host not removed everywhere",
            lists: Lists::Ssh(
                devices::update_ssh_hosts(&config.device_hosts, host, true, host).ok()?,
            ),
        },
        Source::Eas(host) => Save {
            title: "EAS host not removed everywhere",
            lists: Lists::Eas(devices::update_eas_hosts(
                &config.eas_device_hosts,
                host,
                true,
                false,
            )),
        },
        Source::Simlock(host) => Save {
            title: "Simlock host not removed everywhere",
            lists: Lists::Simlock(devices::update_simlock_hosts(
                &config.simlock_device_hosts,
                host,
                true,
            )),
        },
    })
}

fn has_token(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.is_empty(),
        Value::Null => false,
        _ => true,
    }
}

pub(super) struct Editor {
    kind: AddKind,
    existing: Option<Source>,
    id: String,
    fields: Vec<Entity<InputState>>,
    options_open: bool,
    pub(super) seq: u64,
    check: Check,
    account: Account,
    app_id: Option<String>,
    saved_token: bool,
    cleared: bool,
    pub(super) app_menu: bool,
    pub(super) saving: bool,
    pub(super) error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl Editor {
    pub(super) fn open(
        kind: AddKind,
        existing: Option<&Source>,
        _config: &DeviceConfig,
        window: &mut Window,
        cx: &mut Context<DevicesPanel>,
    ) -> Self {
        // placeholder, value, masked
        let mut specs: Vec<(&'static str, String, bool)> = Vec::new();
        let mut app_id = None;
        let mut saved_token = false;
        let mut options_open = false;
        let id = match (kind, existing) {
            (AddKind::Ssh, existing) => {
                let host = match existing {
                    Some(Source::Ssh(host)) => host.clone(),
                    _ => SshHost {
                        kind: "ssh".to_owned(),
                        id: devices::new_host_id(),
                        label: String::new(),
                        target: String::new(),
                        identity_file: None,
                        port: None,
                    },
                };
                options_open = host.port.is_some() || host.identity_file.is_some();
                specs.push(("Mac mini", host.label, false));
                specs.push(("user@host or SSH alias", host.target, false));
                specs.push((
                    "SSH config default",
                    host.identity_file.unwrap_or_default(),
                    false,
                ));
                specs.push((
                    "Default",
                    host.port.map(|p| p.to_string()).unwrap_or_default(),
                    false,
                ));
                host.id
            }
            (AddKind::Eas, existing) => {
                let host = match existing {
                    Some(Source::Eas(host)) => host.clone(),
                    _ => devices::new_eas_host(devices::new_host_id()),
                };
                saved_token = has_token(&host.access_token);
                app_id = host.app_id.clone();
                specs.push(("EAS Simulator", host.label, false));
                specs.push(("Optional", String::new(), true));
                specs.push(("", host.max_concurrent_sessions.to_string(), false));
                specs.push(("", host.max_session_minutes.to_string(), false));
                specs.push(("", host.idle_minutes.to_string(), false));
                host.id
            }
            (AddKind::Simlock, existing) => {
                let host = match existing {
                    Some(Source::Simlock(host)) => host.clone(),
                    _ => devices::new_simlock_host(devices::new_host_id()),
                };
                saved_token = has_token(&host.agent_token);
                specs.push(("Simlock", host.label, false));
                specs.push((
                    "https://gateway.example.ts.net:4700",
                    host.gateway_url,
                    false,
                ));
                specs.push(("Optional", String::new(), true));
                specs.push(("", host.max_leases.to_string(), false));
                specs.push(("", host.max_lease_minutes.to_string(), false));
                host.id
            }
        };
        let mut subscriptions = Vec::new();
        let fields: Vec<Entity<InputState>> = specs
            .into_iter()
            .map(|(placeholder, value, masked)| {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(value)
                        .masked(masked)
                });
                subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    |_: &mut DevicesPanel, _, event: &InputEvent, _, cx| {
                        if matches!(event, InputEvent::Change) {
                            cx.notify();
                        }
                    },
                ));
                input
            })
            .collect();
        let first = fields[0].read(cx).focus_handle(cx).clone();
        window.focus(&first, cx);
        Self {
            kind,
            existing: existing.cloned(),
            id,
            fields,
            options_open,
            seq: 0,
            check: Check::Idle,
            account: Account::Idle,
            app_id,
            saved_token,
            cleared: false,
            app_menu: false,
            saving: false,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn wants_account(&self) -> bool {
        self.kind == AddKind::Eas
    }

    fn text(&self, index: usize, cx: &App) -> String {
        self.fields[index].read(cx).value().to_string()
    }

    fn number(&self, index: usize, cx: &App) -> u32 {
        self.text(index, cx).trim().parse().unwrap_or(0)
    }

    fn ssh_draft(&self, cx: &App) -> SshHost {
        let port = self.text(3, cx);
        let port = port.trim();
        let identity = self.text(2, cx);
        SshHost {
            kind: "ssh".to_owned(),
            id: self.id.clone(),
            label: self.text(0, cx),
            target: self.text(1, cx),
            identity_file: (!identity.trim().is_empty()).then_some(identity),
            port: (!port.is_empty()).then(|| port.parse::<u16>().unwrap_or(0)),
        }
    }

    /// `deviceHostConnectionKey`.
    fn key_of(host: &SshHost) -> ConnectionKey {
        (
            host.target.trim().to_owned(),
            host.port,
            host.identity_file
                .as_deref()
                .map(str::trim)
                .filter(|file| !file.is_empty())
                .map(str::to_owned),
        )
    }

    fn eas_draft(&self, cx: &App) -> EasHost {
        EasHost {
            kind: "eas".to_owned(),
            id: self.id.clone(),
            label: self.text(0, cx),
            app_id: self.app_id.clone(),
            access_token: match self.token(cx) {
                Token::Set(token) => json!(token),
                _ => json!(""),
            },
            max_concurrent_sessions: self.number(2, cx),
            max_session_minutes: self.number(3, cx),
            idle_minutes: self.number(4, cx),
        }
    }

    fn simlock_draft(&self, cx: &App) -> SimlockHost {
        SimlockHost {
            kind: "simlock".to_owned(),
            id: self.id.clone(),
            label: self.text(0, cx),
            gateway_url: self.text(1, cx),
            agent_token: match (self.token(cx), &self.existing) {
                (Token::Set(token), _) => json!(token),
                (Token::Keep, Some(Source::Simlock(host))) => host.agent_token.clone(),
                _ => json!(""),
            },
            max_leases: self.number(3, cx),
            max_lease_minutes: self.number(4, cx),
        }
    }

    /// Saved (kept), new, or cleared, as the secret field resolves them.
    fn token(&self, cx: &App) -> Token {
        let typed = self.text(2, cx);
        let typed = if self.kind == AddKind::Eas {
            self.text(1, cx)
        } else {
            typed
        };
        let typed = typed.trim().to_owned();
        if !typed.is_empty() {
            Token::Set(typed)
        } else if self.cleared || !self.saved_token {
            Token::Clear
        } else {
            Token::Keep
        }
    }

    fn token_index(&self) -> usize {
        if self.kind == AddKind::Eas { 1 } else { 2 }
    }

    pub(super) fn start_test(&mut self, cx: &App) -> Option<Value> {
        let host = devices::ssh_test_host(&self.ssh_draft(cx))?;
        self.seq += 1;
        self.check = Check::Running(Self::key_of(&host));
        Some(devices::test_payload(&host))
    }

    pub(super) fn finish_test(&mut self, event: CallEvent) {
        let Check::Running(key) = std::mem::replace(&mut self.check, Check::Idle) else {
            return;
        };
        let result = match event {
            CallEvent::Done(value) => Ok(match devices::host_summary(&value) {
                Some(summary) if summary.kind != "local" => Outcome::Connected(summary.platforms),
                Some(_) => Outcome::Local,
                None => Outcome::Connected(Vec::new()),
            }),
            CallEvent::Failed(message) => Err(message),
            CallEvent::Progress(_) => return,
        };
        self.check = Check::Done(key, result);
    }

    pub(super) fn start_account(&mut self, cx: &App) -> Value {
        self.seq += 1;
        self.account = Account::Checking;
        match self.token(cx) {
            Token::Set(token) => json!({"accessToken": token}),
            Token::Clear if self.saved_token => json!({"accessToken": ""}),
            Token::Keep => match &self.existing {
                Some(Source::Eas(host)) => json!({"hostId": host.id}),
                _ => json!({}),
            },
            Token::Clear => json!({}),
        }
    }

    pub(super) fn finish_account(&mut self, event: CallEvent) {
        self.account = match event {
            CallEvent::Done(value) => Account::Signed {
                who: value["signedInAs"].as_str().unwrap_or_default().to_owned(),
                source: match value["credentialSource"].as_str() {
                    Some("settings") => "the access token saved here",
                    Some("env") => "EXPO_TOKEN on the server",
                    _ => "the server's eas login",
                }
                .to_owned(),
                apps: value["apps"]
                    .as_array()
                    .map(|apps| {
                        apps.iter()
                            .filter_map(|app| {
                                Some((
                                    app["id"].as_str()?.to_owned(),
                                    app["fullName"].as_str()?.to_owned(),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            CallEvent::Failed(message) => Account::Failed(message),
            CallEvent::Progress(_) => return,
        };
    }

    /// What Save writes, `None` while the draft cannot be saved.
    pub(super) fn save(&mut self, config: &DeviceConfig, cx: &App) -> Option<Result<Save, String>> {
        if self.saving || self.checking() {
            return None;
        }
        match self.kind {
            AddKind::Ssh => {
                let host = devices::parse_ssh_draft(&self.ssh_draft(cx))?;
                let original = match &self.existing {
                    Some(Source::Ssh(original)) => original.clone(),
                    _ => host.clone(),
                };
                Some(
                    devices::update_ssh_hosts(&config.device_hosts, &host, false, &original).map(
                        |hosts| Save {
                            title: "Device host not saved everywhere",
                            lists: Lists::Ssh(hosts),
                        },
                    ),
                )
            }
            AddKind::Eas => {
                let host = self.eas_draft(cx);
                if devices::eas_problem(&host).is_some() {
                    return None;
                }
                let keep = matches!(self.token(cx), Token::Keep);
                Some(Ok(Save {
                    title: "EAS host not saved everywhere",
                    lists: Lists::Eas(devices::update_eas_hosts(
                        &config.eas_device_hosts,
                        &host,
                        false,
                        keep,
                    )),
                }))
            }
            AddKind::Simlock => {
                let host = self.simlock_draft(cx);
                if devices::simlock_problem(&host).is_some() {
                    return None;
                }
                Some(Ok(Save {
                    title: "Simlock host not saved everywhere",
                    lists: Lists::Simlock(devices::update_simlock_hosts(
                        &config.simlock_device_hosts,
                        &host,
                        false,
                    )),
                }))
            }
        }
    }

    fn checking(&self) -> bool {
        matches!(self.check, Check::Running(_))
    }

    fn can_save(&self, cx: &App) -> bool {
        if self.saving || self.checking() {
            return false;
        }
        match self.kind {
            AddKind::Ssh => devices::parse_ssh_draft(&self.ssh_draft(cx)).is_some(),
            AddKind::Eas => devices::eas_problem(&self.eas_draft(cx)).is_none(),
            AddKind::Simlock => devices::simlock_problem(&self.simlock_draft(cx)).is_none(),
        }
    }

    fn secret(&self, view: &Entity<DevicesPanel>, window: &Window, cx: &App) -> AnyElement {
        let index = self.token_index();
        let visible = !self.cleared && (self.saved_token || !self.text(index, cx).is_empty());
        let input = self.fields[index].clone();
        let clear = visible.then(|| {
            let view = view.clone();
            icon_button(
                "device-token-clear",
                Variant::GhostDestructive,
                !self.saving,
                cx,
            )
            .tooltip(tooltip("Clear access token"))
            .on_click(move |_, window, cx| {
                let input = input.clone();
                view.update(cx, |this, cx| {
                    if let Some(editor) = &mut this.editor {
                        editor.cleared = true;
                    }
                    input.update(cx, |state, cx| state.set_value("", window, cx));
                    cx.notify();
                })
            })
            .child(Icon::new(Icons::X).size(px(14.)))
        });
        h_flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .child(field(&self.fields[index], self.saving, window, cx)),
            )
            .children(clear)
            .into_any_element()
    }

    fn limits(
        &self,
        labels: &[&'static str],
        first: usize,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        h_flex()
            .gap_3()
            .children(labels.iter().enumerate().map(|(i, title)| {
                div().flex_1().child(labeled(
                    title,
                    field(&self.fields[first + i], self.saving, window, cx),
                ))
            }))
            .into_any_element()
    }

    fn test_box(&self, label: &str, view: &Entity<DevicesPanel>, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let draft = self.ssh_draft(cx);
        let key = Self::key_of(&draft);
        let valid = devices::ssh_test_host(&draft).is_some();
        let (status, row): (String, Option<AnyElement>) = match &self.check {
            Check::Running(running) if *running == key => (
                "Checking environments\u{2026}".to_owned(),
                Some(result_row(label, None, cx)),
            ),
            Check::Done(done, result) if *done == key => (
                if result.is_ok() {
                    "Connection checks passed".to_owned()
                } else {
                    "1 of 1 failed".to_owned()
                },
                Some(result_row(label, Some(result), cx)),
            ),
            _ => ("Check access before saving".to_owned(), None),
        };
        let enabled = valid && !self.saving && !self.checking();
        let view = view.clone();
        v_flex()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border.opacity(0.6))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .py(px(10.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(status),
                    )
                    .child(
                        button("device-ssh-test", Variant::Outline, enabled, cx)
                            .child("Test connection")
                            .when(enabled, |b| {
                                b.on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| this.test_connection(cx))
                                })
                            }),
                    ),
            )
            .children(row)
            .into_any_element()
    }

    fn account_box(&self, view: &Entity<DevicesPanel>, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let (text, color) = match &self.account {
            Account::Signed { who, source, .. } => (
                format!("Signed in as {who} with {source}."),
                theme.muted_foreground,
            ),
            Account::Failed(message) => (message.clone(), theme.destructive),
            Account::Checking => (
                "Checking the Expo account\u{2026}".to_owned(),
                theme.muted_foreground,
            ),
            Account::Idle => (
                "Check the Expo account before choosing a project.".to_owned(),
                theme.muted_foreground,
            ),
        };
        let enabled = !matches!(self.account, Account::Checking) && !self.saving;
        let view = view.clone();
        h_flex()
            .items_center()
            .justify_between()
            .gap_3()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border.opacity(0.6))
            .px_3()
            .py(px(10.))
            .child(
                h_flex()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(color)
                    .when(matches!(self.account, Account::Checking), |line| {
                        line.child(spinner(14., color))
                    })
                    .child(div().min_w_0().child(text)),
            )
            .child(
                button("device-eas-check", Variant::Outline, enabled, cx)
                    .child("Check account")
                    .when(enabled, |b| {
                        b.on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| this.check_account(cx))
                        })
                    }),
            )
            .into_any_element()
    }

    fn project_select(&self, view: &Entity<DevicesPanel>, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let mut options: Vec<(Option<String>, String)> =
            vec![(None, "From the thread's project".to_owned())];
        let apps = match &self.account {
            Account::Signed { apps, .. } => apps.clone(),
            _ => Vec::new(),
        };
        if let Some(saved) = &self.app_id
            && !apps.iter().any(|(id, _)| id == saved)
        {
            options.push((Some(saved.clone()), saved.clone()));
        }
        options.extend(apps.into_iter().map(|(id, name)| (Some(id), name)));
        let current = options
            .iter()
            .find(|(id, _)| *id == self.app_id)
            .map_or_else(
                || "From the thread's project".to_owned(),
                |(_, name)| name.clone(),
            );
        let selected = self.app_id.clone();
        let trigger = h_flex()
            .id("device-eas-app")
            .h(px(32.))
            .w_full()
            .items_center()
            .justify_between()
            .px(px(11.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(if theme.dark {
                theme.input.opacity(0.32)
            } else {
                theme.background
            })
            .text_size(px(14.))
            .cursor_pointer()
            .child(div().min_w_0().truncate().child(current))
            .child(
                div()
                    .opacity(0.7)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            );
        let (opened, picked) = (view.clone(), view.clone());
        Popover::new("device-eas-app-menu")
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .open(self.app_menu && !self.saving)
            .p_0()
            .appearance(false)
            .on_open_change(move |next, _, cx| {
                opened.update(cx, |this, cx| {
                    if let Some(editor) = &mut this.editor {
                        editor.app_menu = *next;
                    }
                    cx.notify();
                })
            })
            .trigger(PopTrigger::new(trigger))
            .content(move |_, _, cx| {
                popup_shell(cx)
                    .w(px(448.))
                    .max_w_full()
                    .max_h(px(240.))
                    .p_1()
                    .children(options.iter().enumerate().map(|(index, (id, name))| {
                        let (view, id) = (picked.clone(), id.clone());
                        menu_item(("device-eas-app-item", index), name.clone(), false, cx)
                            .when(id == selected, |item| item.font_weight(FontWeight::MEDIUM))
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                view.update(cx, |this, cx| {
                                    if let Some(editor) = &mut this.editor {
                                        editor.app_id = id;
                                        editor.app_menu = false;
                                    }
                                    cx.notify();
                                })
                            })
                    }))
            })
            .into_any_element()
    }

    pub(super) fn view(
        &self,
        label: &str,
        view: &Entity<DevicesPanel>,
        window: &mut Window,
        cx: &mut Context<DevicesPanel>,
    ) -> AnyElement {
        let editing = self.existing.is_some();
        let mut body: Vec<AnyElement> = Vec::new();
        let (id, title, description): (&'static str, &'static str, String) = match self.kind {
            AddKind::Ssh => (
                "device-host-dialog",
                if editing { "Edit device host" } else { "Add device host" },
                format!("Connect from {label}. Hosts on the same machine are skipped."),
            ),
            AddKind::Eas => (
                "eas-host-dialog",
                if editing { "Edit EAS host" } else { "Add EAS host" },
                "The environment's server signs in to Expo and starts the sessions. Without a token here it uses EXPO_TOKEN, then its eas login.".to_owned(),
            ),
            AddKind::Simlock => (
                "simlock-host-dialog",
                if editing { "Edit Simlock host" } else { "Add Simlock host" },
                "The environment's server leases devices from your Simlock gateway and releases them when they close.".to_owned(),
            ),
        };
        body.push(
            labeled("Name", field(&self.fields[0], self.saving, window, cx)).into_any_element(),
        );
        let mut problem_text: Option<String> = self.error.clone();
        match self.kind {
            AddKind::Ssh => {
                body.push(
                    labeled(
                        "SSH target",
                        field(&self.fields[1], self.saving, window, cx),
                    )
                    .into_any_element(),
                );
                body.push(self.options(view, window, cx));
                body.push(self.test_box(label, view, cx));
            }
            AddKind::Eas => {
                body.push(
                    labeled("Expo access token", self.secret(view, window, cx)).into_any_element(),
                );
                body.push(self.account_box(view, cx));
                body.push(labeled("EAS project", self.project_select(view, cx)).into_any_element());
                body.push(
                    note(
                        "Sessions bill the project's Expo account. \"From the thread's project\" reads extra.eas.projectId from the project's app config.",
                        cx,
                    )
                    .into_any_element(),
                );
                body.push(self.limits(
                    &["Sessions at once", "Session minutes", "Idle minutes"],
                    2,
                    window,
                    cx,
                ));
                body.push(
                    note(
                        format!(
                            "Expo allows sessions of up to 40 minutes on free accounts and 115 on paid ones, and ends one after the idle minutes without activity, so idle minutes must be smaller than session minutes.{}",
                            if editing { " Saving restarts this host, which stops its running sessions." } else { "" }
                        ),
                        cx,
                    )
                    .into_any_element(),
                );
                problem_text = problem_text
                    .or_else(|| devices::eas_problem(&self.eas_draft(cx)).map(str::to_owned));
            }
            AddKind::Simlock => {
                body.push(
                    labeled(
                        "Gateway URL",
                        field(&self.fields[1], self.saving, window, cx),
                    )
                    .into_any_element(),
                );
                body.push(labeled("Agent token", self.secret(view, window, cx)).into_any_element());
                body.push(
                    note(
                        "Leases, drives, and streams devices. Create one with `simlock --gateway token create --role agent`, or leave it blank to add it later. A blank token uses the machine's `AGENT_DEVICE_DAEMON_AUTH_TOKEN` when its `AGENT_DEVICE_DAEMON_BASE_URL` is this gateway.",
                        cx,
                    )
                    .into_any_element(),
                );
                body.push(self.limits(&["Devices at once", "Lease minutes"], 3, window, cx));
                body.push(
                    note(
                        format!(
                            "A lease ends after the lease minutes even while in use.{}",
                            if editing {
                                " Saving restarts this host, which releases its devices."
                            } else {
                                ""
                            }
                        ),
                        cx,
                    )
                    .into_any_element(),
                );
                problem_text = problem_text.or_else(|| {
                    devices::simlock_problem(&self.simlock_draft(cx)).map(str::to_owned)
                });
            }
        }
        if let Some(text) = problem_text {
            body.push(problem(text, cx).into_any_element());
        }
        let can_save = self.can_save(cx);
        let saving = self.saving;
        let (cancel, save, dismiss) = (view.clone(), view.clone(), view.clone());
        let footer = vec![
            button("device-editor-cancel", Variant::Ghost, !saving, cx)
                .child("Cancel")
                .when(!saving, |b| {
                    b.on_click(move |_, window, cx| {
                        cancel.update(cx, |this, cx| {
                            this.close_menu(window, cx);
                        })
                    })
                })
                .into_any_element(),
            button("device-editor-save", Variant::Primary, can_save, cx)
                .child("Save host")
                .when(can_save, |b| {
                    b.on_click(move |_, _, cx| save.update(cx, |this, cx| this.save_editor(cx)))
                })
                .into_any_element(),
        ];
        dialog(
            id,
            title,
            description,
            body,
            footer,
            move |window, cx| {
                dismiss.update(cx, |this, cx| {
                    this.close_menu(window, cx);
                })
            },
            cx,
        )
    }

    /// The collapsible "SSH options" (DeviceHostEditor.tsx).
    fn options(&self, view: &Entity<DevicesPanel>, window: &Window, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let open = self.options_open;
        let chevron = Icon::new(Icons::ChevronRight).size(px(14.));
        let chevron = if open {
            chevron.rotate(gpui::radians(std::f32::consts::FRAC_PI_2))
        } else {
            chevron
        };
        let toggle = view.clone();
        let header = h_flex()
            .id(ElementId::from("device-ssh-options"))
            .items_center()
            .gap_1()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.muted_foreground)
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                toggle.update(cx, |this, cx| {
                    if let Some(editor) = &mut this.editor {
                        editor.options_open = !editor.options_open;
                    }
                    cx.notify();
                })
            })
            .child(chevron)
            .child("SSH options");
        v_flex()
            .gap_3()
            .child(header)
            .when(open, |options| {
                options
                    .child(
                        h_flex()
                            .gap_3()
                            .child(div().flex_1().child(labeled(
                                "Identity file",
                                field(&self.fields[2], self.saving, window, cx),
                            )))
                            .child(div().w(px(112.)).child(labeled(
                                "Port",
                                field(&self.fields[3], self.saving, window, cx),
                            ))),
                    )
                    .child(note(
                        "Optional. Resolved separately on each environment.",
                        cx,
                    ))
            })
            .into_any_element()
    }
}

/// A result of the connection check (`px-3 py-2.5`, `text-xs`).
fn result_row(label: &str, result: Option<&Result<Outcome, String>>, cx: &App) -> AnyElement {
    let theme = Tokens::of(cx);
    let (status, color, detail): (String, _, Option<String>) = match result {
        None => ("Checking\u{2026}".to_owned(), theme.muted_foreground, None),
        Some(Ok(Outcome::Local)) => (
            "Already available locally".to_owned(),
            theme.muted_foreground,
            None,
        ),
        Some(Ok(Outcome::Connected(platforms))) => (
            "Connected".to_owned(),
            warning_free(&theme),
            (!platforms.is_empty()).then(|| {
                platforms
                    .iter()
                    .map(|p| {
                        let name = match p.platform {
                            devices::Platform::Ios => "iOS",
                            devices::Platform::Android => "Android",
                        };
                        format!(
                            "{name} {}",
                            if p.available {
                                "available"
                            } else {
                                "not available"
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }),
        ),
        Some(Err(message)) => (
            "Failed".to_owned(),
            theme.destructive,
            Some(message.clone()),
        ),
    };
    v_flex()
        .gap_1()
        .px_3()
        .py(px(10.))
        .border_t_1()
        .border_color(theme.border.opacity(0.6))
        .text_xs()
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_owned()),
                )
                .child(div().text_color(color).child(status)),
        )
        .children(detail.map(|detail| {
            div()
                .text_color(if result.is_some_and(Result::is_err) {
                    theme.destructive
                } else {
                    theme.muted_foreground
                })
                .child(detail)
        }))
        .into_any_element()
}

/// The text colour of a passing check: the foreground, since the web has no success tone here.
fn warning_free(theme: &Tokens) -> gpui::Hsla {
    let _ = warning;
    theme.foreground
}
