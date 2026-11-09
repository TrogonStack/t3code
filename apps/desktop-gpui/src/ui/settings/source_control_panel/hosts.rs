//! The expanded panels of the provider rows: `SourceControlTokenSettings` (host and token),
//! `SourceControlAccountSettings` (the `gh` logins) and `SourceControlHostSettings` (the
//! schema-driven rows of Bitbucket and GitCafe). Every save is one `settings.update` patch of
//! `sourceControlHosts[kind]`, followed by a rescan once the server accepts it.

use super::super::components::{KeyHandler, SelectTrigger, popup_shell, row, switch, tab_stop};
use super::super::scope_inheritance::PopTrigger;
use super::super::secret_field::{Check, SecretEvent, SecretField, sm_field};
use super::*;
use crate::secret_source as secret;
use crate::settings_source_control::HostAccountGroup;
use gpui::{Anchor, ElementId};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::popover::Popover;
use serde_json::Map;

/// `credentials` of the GitHub definition (`GITHUB_CREDENTIALS`).
const GITHUB_HOST: &str = "github.com";
const TOKEN_ENV: &str = "GH_TOKEN";
const CLI_NAME: &str = "gh";
const CLI_LABEL: &str = "GitHub CLI";
const TOKEN_ACCESS: &str = "read and write access to pull requests and contents";
const LOGIN_COMMAND: &str = "gh auth login";

/// The inputs of the three expanded panels; the panel subscribes to each.
pub(super) struct HostForms {
    pub github_host: Entity<InputState>,
    pub github_token: Entity<SecretField>,
    /// What the token field was last loaded for: the host and the token saved under it.
    github_key: String,
    pub bitbucket_access: Entity<SecretField>,
    pub bitbucket_api: Entity<SecretField>,
    pub bitbucket_email: Entity<InputState>,
    pub gitcafe_token: Entity<SecretField>,
    /// The saved value each other field was last loaded from.
    synced: HashMap<&'static str, String>,
}

impl HostForms {
    pub(super) fn new(window: &mut Window, cx: &mut Context<SourceControlPanel>) -> Self {
        let github_host = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(GITHUB_HOST)
                .default_value(GITHUB_HOST)
        });
        let bitbucket_email =
            cx.new(|cx| InputState::new(window, cx).placeholder("you@example.com"));
        Self {
            github_host,
            github_token: cx.new(|cx| SecretField::new("github-token", "GitHub token", window, cx)),
            github_key: "\u{0}".to_owned(),
            bitbucket_access: cx
                .new(|cx| SecretField::new("bitbucket-access-token", "Access token", window, cx)),
            bitbucket_api: cx
                .new(|cx| SecretField::new("bitbucket-api-token", "API token", window, cx)),
            bitbucket_email,
            gitcafe_token: cx.new(|cx| SecretField::new("gitcafe-token", "Token", window, cx)),
            synced: HashMap::new(),
        }
    }

    /// What the panel listens to: each secret field, and the text inputs' blur and Enter.
    pub(super) fn subscribe(
        &self,
        window: &mut Window,
        cx: &mut Context<SourceControlPanel>,
    ) -> Vec<gpui::Subscription> {
        let mut subscriptions = Vec::new();
        for (kind, key, field) in self.secrets() {
            subscriptions.push(cx.subscribe(
                &field,
                move |this, field, event: &SecretEvent, cx| {
                    this.secret_event(kind, key, &field, event, cx)
                },
            ));
        }
        subscriptions.push(cx.subscribe_in(
            &self.bitbucket_email,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.commit_host_text("bitbucket", "email", cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &self.github_host,
            window,
            |this, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } => this.save_token(cx),
                _ => {}
            },
        ));
        subscriptions
    }

    /// Every secret field with the provider and settings key it edits.
    pub(super) fn secrets(&self) -> [(&'static str, &'static str, Entity<SecretField>); 4] {
        [
            ("github", "tokens", self.github_token.clone()),
            ("bitbucket", "accessToken", self.bitbucket_access.clone()),
            ("bitbucket", "apiToken", self.bitbucket_api.clone()),
            ("gitcafe", "token", self.gitcafe_token.clone()),
        ]
    }
}

/// A `Part` of a paragraph.
enum Part<'a> {
    Text(&'a str),
    Code(&'a str),
}

/// A paragraph that wraps between words and holds `code` chips, as the web's inline markup does.
fn prose(parts: Vec<Part>, trailing: Option<AnyElement>, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    let mut line = h_flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(4.))
        .max_w(px(672.))
        .text_xs()
        .line_height(px(18.))
        .text_color(theme.muted_foreground);
    for part in parts {
        match part {
            Part::Text(text) => {
                for word in text.split_whitespace() {
                    line = line.child(div().child(word.to_owned()));
                }
            }
            Part::Code(text) => line = line.child(code(text, cx)),
        }
    }
    line.children(trailing)
}

/// `<code className="rounded bg-muted px-1 py-px text-2xs">`.
fn code(text: &str, cx: &App) -> gpui::Div {
    div()
        .px_1()
        .py(px(1.))
        .rounded(px(4.))
        .bg(Tokens::of(cx).muted)
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(10.))
        .child(text.to_owned())
}

/// `Button size="xs"` at the desktop size: `h-6 px-[7px] text-xs`.
fn xs_button(
    id: impl Into<ElementId>,
    variant: Variant,
    enabled: bool,
    label: &'static str,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    button(id, variant, enabled, cx)
        .h(px(24.))
        .px(px(7.))
        .text_size(px(12.))
        .line_height(px(16.))
        .child(label)
}

/// `RedactedLogin`: the same letters for the same login, in mono at 10px. The web blurs them;
/// without a blur the letters are dimmed, and nothing here can be selected.
pub(super) fn redacted_login(account: &str, revealed: bool, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    let (text, color) = if revealed {
        (account.to_owned(), theme.muted_foreground)
    } else {
        (sc::redacted(account), theme.muted_foreground.opacity(0.4))
    };
    div()
        .min_w_0()
        .truncate()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(10.))
        .line_height(px(15.))
        .text_color(color)
        .child(text)
}

/// A secret's patch value as the server stores it: a typed literal reads back as the marker.
fn masked(value: &Value) -> Value {
    match value {
        Value::String(text) if !text.is_empty() => secret::stored_literal(),
        Value::Object(map) if map.get("provider").is_none() => Value::Object(
            map.iter()
                .map(|(key, inner)| (key.clone(), masked(inner)))
                .collect(),
        ),
        other => other.clone(),
    }
}

impl SourceControlPanel {
    /// Whether a host write is in flight (the web locks its fieldset meanwhile).
    fn saving(&self) -> bool {
        !self.host_writes.is_empty()
    }

    /// Sends one `settings.update` of the provider's host settings, showing it at once.
    /// `secrets` names the keys that hold secrets, which show as their marker until the reply.
    fn save_host(
        &mut self,
        kind: &str,
        fields: Map<String, Value>,
        secrets: &[&str],
        cx: &mut Context<Self>,
    ) {
        if fields.is_empty() || !self.ctx(cx).connected {
            return;
        }
        let shown: Map<String, Value> = fields
            .iter()
            .map(|(key, value)| {
                let value = if secrets.contains(&key.as_str()) {
                    masked(value)
                } else {
                    value.clone()
                };
                (key.clone(), value)
            })
            .collect();
        let previous = self.config.clone();
        self.config.apply_host(kind, &shown);
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call: Call::UpdateSettings,
            payload: SourceControlConfig::host_patch(kind, &fields),
        };
        if self.actions.try_send(action).is_ok() {
            self.pending.insert(token, previous);
            self.host_writes.insert(token);
        } else {
            self.config = previous;
        }
        cx.notify();
    }

    /// The first rescan after a host write the server accepted.
    pub(super) fn host_saved(&mut self, token: u64, cx: &mut Context<Self>) {
        if self.host_writes.remove(&token) {
            self.scan(cx);
        }
    }

    /// The outcome of a `server.checkSecretSource` request, reported to the field that asked.
    pub(super) fn checked(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.checks.remove(&token) else {
            return false;
        };
        let state = match event {
            CallEvent::Done(value) if value["_tag"] == "Resolved" => Check::Resolved,
            CallEvent::Done(value) => Check::Failed(
                value["detail"]
                    .as_str()
                    .unwrap_or("The secret could not be resolved.")
                    .to_owned(),
            ),
            CallEvent::Failed(message) => Check::Failed(message),
            CallEvent::Progress(_) => {
                self.checks.insert(token, field);
                return true;
            }
        };
        field.update(cx, |field, cx| field.set_check(state, cx));
        true
    }

    fn check(
        &mut self,
        field: &Entity<SecretField>,
        provider: String,
        reference: String,
        cx: &mut Context<Self>,
    ) {
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call: Call::CheckSecretSource,
            payload: secret::check_payload(&provider, &reference),
        };
        if self.actions.try_send(action).is_ok() {
            self.checks.insert(token, field.clone());
        } else {
            field.update(cx, |field, cx| {
                field.set_check(Check::Failed("Could not reach the server.".to_owned()), cx)
            });
        }
    }

    /// What a secret field reported.
    pub(super) fn secret_event(
        &mut self,
        kind: &'static str,
        key: &'static str,
        field: &Entity<SecretField>,
        event: &SecretEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SecretEvent::Changed => cx.notify(),
            SecretEvent::Check(provider, reference) => {
                self.check(field, provider.clone(), reference.clone(), cx)
            }
            SecretEvent::Commit if kind != "github" => {
                if let Some(value) = field.read(cx).submission(cx) {
                    let mut fields = Map::new();
                    fields.insert(key.to_owned(), value);
                    self.save_host(kind, fields, &[key], cx);
                }
            }
            SecretEvent::Submit if kind == "github" => self.save_token(cx),
            _ => {}
        }
    }

    /// The host input of the token form, as the web normalizes it.
    fn github_host(&self, cx: &App) -> String {
        self.hosts
            .github_host
            .read(cx)
            .value()
            .trim()
            .to_lowercase()
    }

    /// `save(normalizedHost, submission)` of `SourceControlTokenSettings`.
    pub(super) fn save_token(&mut self, cx: &mut Context<Self>) {
        let host = self.github_host(cx);
        let Some(token) = self.hosts.github_token.read(cx).submission(cx) else {
            return;
        };
        if host.is_empty() || self.saving() {
            return;
        }
        self.write_token(host, token, cx);
    }

    fn write_token(&mut self, host: String, token: Value, cx: &mut Context<Self>) {
        // The whole map goes out; the other hosts' markers keep their values.
        let mut tokens: Map<String, Value> = self.config.tokens("github").into_iter().collect();
        tokens.insert(host, token);
        let mut fields = Map::new();
        fields.insert("tokens".to_owned(), Value::Object(tokens));
        self.save_host("github", fields, &["tokens"], cx);
    }

    /// Saves a text field of the host settings when it lost focus with a new value.
    pub(super) fn commit_host_text(
        &mut self,
        kind: &'static str,
        key: &'static str,
        cx: &mut Context<Self>,
    ) {
        let (input, saved) = match (kind, key) {
            ("bitbucket", "email") => (
                self.hosts.bitbucket_email.clone(),
                self.config.host_value(kind, key),
            ),
            _ => return,
        };
        let next = input.read(cx).value().trim().to_owned();
        if next != saved.as_str().unwrap_or_default() {
            let mut fields = Map::new();
            fields.insert(key.to_owned(), Value::String(next));
            self.save_host(kind, fields, &[], cx);
        }
    }

    /// Puts what the server stores into the fields, once it changes; a typed draft goes with it.
    pub(super) fn sync_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saving = self.saving();
        let sources = self.config.secret_sources().to_vec();
        for (kind, key, field) in self.hosts.secrets() {
            field.update(cx, |field, cx| {
                field.set_sources(&sources, cx);
                field.set_disabled(saving, cx);
            });
            if kind == "github" {
                continue;
            }
            let saved = self.config.host_value(kind, key);
            let signature = saved.to_string();
            if self.hosts.synced.get(key) != Some(&signature) {
                self.hosts.synced.insert(key, signature);
                field.update(cx, |field, cx| field.load(&saved, window, cx));
            }
        }
        let host = self.github_host(cx);
        let saved = self
            .config
            .tokens("github")
            .into_iter()
            .find(|(candidate, _)| *candidate == host)
            .map_or(Value::Null, |(_, value)| value);
        let key = format!("{host}|{saved}");
        if key != self.hosts.github_key {
            self.hosts.github_key = key;
            self.hosts
                .github_token
                .update(cx, |field, cx| field.load(&saved, window, cx));
        }
        let email = self.config.host_value("bitbucket", "email");
        let email = email.as_str().unwrap_or_default().to_owned();
        let input = &self.hosts.bitbucket_email;
        if self.hosts.synced.get("email") != Some(&email) {
            let focused = input.read(cx).focus_handle(cx).is_focused(window);
            if !focused || !self.hosts.synced.contains_key("email") {
                self.hosts.synced.insert("email", email.clone());
                input.update(cx, |state, cx| state.set_value(email, window, cx));
            }
        }
    }

    /// The chevron of a row that has details (`aria-label="Toggle {label} details"`).
    pub(super) fn details_toggle(
        &self,
        index: usize,
        item: &Item,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !sc::has_host_settings(&item.kind) || !self.ctx(cx).connected {
            return None;
        }
        let kind = item.kind.clone();
        let open = self.expanded.contains(&kind);
        let view = cx.entity();
        let chevron = Icon::new(Icons::ChevronDown).size(px(14.));
        let chevron = if open {
            chevron.rotate(gpui::radians(std::f32::consts::PI))
        } else {
            chevron
        };
        Some(
            icon_button(
                ("settings-source-control-details", index),
                Variant::GhostMuted,
                true,
                cx,
            )
            .size(px(24.))
            .tooltip(tooltip(format!("Toggle {} details", item.label)))
            .on_click(move |_, _, cx| {
                let kind = kind.clone();
                view.update(cx, |this, cx| {
                    if !this.expanded.remove(&kind) {
                        this.expanded.insert(kind);
                    }
                    cx.notify();
                })
            })
            .child(chevron)
            .into_any_element(),
        )
    }

    /// The panel under an expanded row (`px-3 pb-4 pt-1 sm:px-4`).
    pub(super) fn host_details(
        &self,
        item: &Item,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !sc::has_host_settings(&item.kind)
            || !self.expanded.contains(&item.kind)
            || !self.ctx(cx).connected
        {
            return None;
        }
        let body = match item.kind.as_str() {
            "github" => self.github_panel(item, window, cx),
            "bitbucket" => self.bitbucket_panel(window, cx),
            _ => self.gitcafe_panel(window, cx),
        };
        Some(div().px_4().pb_4().pt_1().child(body).into_any_element())
    }

    fn github_panel(&self, item: &Item, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = v_flex().gap_6().child(self.token_form(window, cx));
        if item.available {
            panel = panel.child(self.accounts(item, window, cx));
        }
        panel.into_any_element()
    }

    /// `SourceControlTokenSettings`.
    fn token_form(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let theme = Tokens::of(cx);
        let saving = self.saving();
        let host = self.github_host(cx);
        let tokens = self.config.tokens("github");
        let saved = tokens
            .iter()
            .find(|(candidate, _)| *candidate == host)
            .map_or(Value::Null, |(_, value)| value.clone());
        let is_saved = secret::is_set(&saved);
        let saved_hosts: Vec<&str> = tokens
            .iter()
            .filter(|(_, value)| secret::is_set(value))
            .map(|(candidate, _)| candidate.as_str())
            .collect();
        let submission = self.hosts.github_token.read(cx).submission(cx);
        let can_save = !host.is_empty() && submission.is_some() && !saving;
        let link_host = if host.is_empty() {
            GITHUB_HOST
        } else {
            host.as_str()
        };
        let url = format!("https://{link_host}/settings/personal-access-tokens/new");
        let view = cx.entity();
        let link = div()
            .id("settings-source-control-create-token")
            .flex()
            .items_center()
            .gap(px(2.))
            .cursor_pointer()
            .text_color(theme.foreground)
            .underline()
            .on_click(move |_, _, cx| {
                let _ = &view;
                cx.open_url(&url)
            })
            .child("Create a token")
            .child(Icon::new(Icons::ExternalLink).size(px(12.)))
            .into_any_element();
        let intro = prose(
            vec![
                Part::Text("A token saved here is used before"),
                Part::Code(TOKEN_ENV),
                Part::Text("and the"),
                Part::Code(CLI_NAME),
                Part::Text("login, so GitHub works without the"),
                Part::Text(CLI_LABEL),
                Part::Text("."),
                Part::Text("Give it"),
                Part::Text(TOKEN_ACCESS),
                Part::Text("."),
            ],
            Some(link),
            cx,
        );
        let label = |text: &'static str| {
            div()
                .text_sm()
                .line_height(px(16.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(text)
        };
        let host_column = v_flex()
            .gap(px(6.))
            .when(self.wide, |column| column.w(px(192.)).flex_shrink_0())
            .child(label("Host"))
            .child(sm_field(
                &self.hosts.github_host,
                false,
                false,
                saving,
                window,
                cx,
            ));
        let token_column = v_flex()
            .gap(px(6.))
            .min_w_0()
            .flex_1()
            .child(label("Token"))
            .child(self.hosts.github_token.clone());
        let columns = if self.wide {
            h_flex()
                .items_start()
                .gap_3()
                .child(host_column)
                .child(token_column)
        } else {
            v_flex().gap_3().child(host_column).child(token_column)
        };
        let status = if saved_hosts.is_empty() {
            format!("No token saved; the server uses {TOKEN_ENV} or the {CLI_NAME} login.")
        } else {
            format!("Saved for {}.", saved_hosts.join(", "))
        };
        let remove = {
            let view = cx.entity();
            let host = host.clone();
            xs_button(
                "settings-source-control-github-remove",
                Variant::Outline,
                !saving,
                "Remove",
                cx,
            )
            .when(!saving, |button| {
                button.on_click(move |_, _, cx| {
                    let host = host.clone();
                    view.update(cx, |this, cx| {
                        this.write_token(host, Value::String(String::new()), cx)
                    })
                })
            })
        };
        let save = {
            let view = cx.entity();
            xs_button(
                "settings-source-control-github-save",
                Variant::Primary,
                can_save,
                "Save",
                cx,
            )
            .when(can_save, |button| {
                button.on_click(move |_, _, cx| view.update(cx, |this, cx| this.save_token(cx)))
            })
        };
        v_flex().gap_3().child(intro).child(columns).child(
            h_flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(status),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_2()
                        .children(is_saved.then_some(remove))
                        .child(save),
                ),
        )
    }

    /// The groups of a provider's logins, from the last scan.
    fn account_groups(&self, kind: &str) -> Vec<HostAccountGroup> {
        self.discovery
            .as_ref()
            .and_then(|discovery| discovery.providers.iter().find(|item| item.kind == kind))
            .and_then(|item| item.auth.as_ref())
            .map(|auth| sc::group_host_accounts(&auth.accounts))
            .unwrap_or_default()
    }

    /// `save(host, change)` of `SourceControlAccountSettings`.
    fn save_account(
        &mut self,
        kind: &'static str,
        host: &str,
        enabled: Option<bool>,
        account: Option<Option<&str>>,
        cx: &mut Context<Self>,
    ) {
        if self.saving() {
            return;
        }
        let next =
            sc::next_host_account_choices(&self.config.host_choices(kind), host, enabled, account);
        let mut fields = Map::new();
        fields.insert("hosts".to_owned(), sc::host_choices_value(&next));
        self.save_host(kind, fields, &[], cx);
    }

    /// `SourceControlAccountSettings`.
    fn accounts(&self, item: &Item, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let kind: &'static str = "github";
        let groups = self.account_groups(&item.kind);
        if groups.is_empty() {
            return prose(
                vec![
                    Part::Text("Sign in with"),
                    Part::Code(LOGIN_COMMAND),
                    Part::Text("on the server host, then rescan to choose accounts here."),
                ],
                None,
                cx,
            )
            .into_any_element();
        }
        let saving = self.saving();
        let revealed = self.reveal_accounts.contains(&item.kind);
        let choices = self.config.host_choices(kind);
        let view = cx.entity();
        let eye = {
            let view = view.clone();
            let name = item.kind.clone();
            icon_button(
                "settings-source-control-github-reveal",
                Variant::GhostMuted,
                true,
                cx,
            )
            .size(px(24.))
            .tooltip(tooltip(if revealed {
                "Hide GitHub accounts"
            } else {
                "Reveal GitHub accounts"
            }))
            .on_click(move |_, _, cx| {
                let name = name.clone();
                view.update(cx, |this, cx| {
                    if !this.reveal_accounts.remove(&name) {
                        this.reveal_accounts.insert(name);
                    }
                    cx.notify();
                })
            })
            .child(Icon::new(if revealed { Icons::EyeOff } else { Icons::Eye }).size(px(14.)))
        };
        let mut panel = v_flex().gap_4().child(
            h_flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(prose(
                    vec![
                        Part::Text("Choose which"),
                        Part::Code(CLI_NAME),
                        Part::Text("login each GitHub host uses, or turn a host off."),
                    ],
                    None,
                    cx,
                ))
                .child(eye),
        );
        let muted = theme.muted_foreground;
        for (index, group) in groups.iter().enumerate() {
            let choice = choices.get(&group.host);
            let enabled = choice.is_none_or(|choice| choice.enabled);
            let stale = choice
                .and_then(|choice| choice.account.as_deref())
                .filter(|account| {
                    !group
                        .selectable
                        .iter()
                        .any(|candidate| candidate == account)
                });
            let pinned = choice
                .and_then(|choice| choice.account.clone())
                .filter(|_| stale.is_none());
            let toggle = {
                let (view, host) = (view.clone(), group.host.clone());
                let handler: Toggle = if saving {
                    Rc::new(|_, _| {})
                } else {
                    Rc::new(move |_, cx| {
                        let host = host.clone();
                        view.update(cx, |this, cx| {
                            this.save_account(kind, &host, Some(!enabled), None, cx)
                        })
                    })
                };
                let control = switch(
                    SharedString::from(format!("settings-source-control-github-host-{index}")),
                    enabled,
                    handler,
                    window,
                    cx,
                );
                div()
                    .flex_shrink_0()
                    .when(saving, |control| control.opacity(0.64))
                    .child(control)
            };
            let mut block = v_flex().gap_2().child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.foreground)
                                    .child(group.host.clone()),
                            )
                            .children(match group.selectable.as_slice() {
                                [only] => Some(
                                    h_flex()
                                        .min_w_0()
                                        .items_center()
                                        .gap_1()
                                        .text_xs()
                                        .text_color(muted)
                                        .child("Signed in as")
                                        .child(redacted_login(only, revealed, cx)),
                                ),
                                _ => None,
                            }),
                    )
                    .child(toggle),
            );
            if group.selectable.len() > 1 {
                block = block.child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(div().text_xs().text_color(muted).child("Account"))
                        .child(self.account_select(
                            index,
                            group,
                            pinned.as_deref(),
                            revealed,
                            !saving && enabled,
                            window,
                            cx,
                        )),
                );
            }
            if stale.is_some() {
                let (view, host) = (view.clone(), group.host.clone());
                block = block.child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .text_xs()
                                .text_color(warning(&theme))
                                .child("The chosen login is no longer signed in, so the active gh login is used."),
                        )
                        .child(
                            xs_button(
                                SharedString::from(format!("settings-source-control-github-active-{index}")),
                                Variant::Outline,
                                !saving,
                                "Use active login",
                                cx,
                            )
                            .when(!saving, |button| {
                                button.on_click(move |_, _, cx| {
                                    let host = host.clone();
                                    view.update(cx, |this, cx| {
                                        this.save_account(kind, &host, None, Some(None), cx)
                                    })
                                })
                            }),
                        ),
                );
            }
            for entry in &group.broken {
                block = block.child(
                    h_flex()
                        .min_w_0()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(muted.opacity(0.7))
                        .child(redacted_login(&entry.account, revealed, cx))
                        .child(format!(
                            "can't be used: {}",
                            entry.error.clone().unwrap_or_else(|| format!(
                                "{CLI_NAME} reports this login as invalid."
                            ))
                        )),
                );
            }
            if let Some(variable) = &group.environment_variable {
                block = block.child(div().text_xs().text_color(warning(&theme)).child(format!(
                    "{variable} is set on the server, so it overrides the account chosen here until it is unset."
                )));
            }
            panel = panel.child(block);
        }
        panel.into_any_element()
    }

    /// The `Account` select of a host with several logins.
    #[allow(clippy::too_many_arguments)]
    fn account_select(
        &self,
        index: usize,
        group: &HostAccountGroup,
        pinned: Option<&str>,
        revealed: bool,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let host = group.host.clone();
        let handle = tab_stop(
            &ElementId::from(SharedString::from(format!(
                "settings-source-control-account-handle-{index}"
            ))),
            window,
            cx,
        );
        let active = |cx: &App| {
            h_flex()
                .min_w_0()
                .items_center()
                .gap_1()
                .child(format!("Active {CLI_NAME} account"))
                .children(group.active_account.as_deref().map(|account| {
                    h_flex()
                        .items_center()
                        .child("(")
                        .child(redacted_login(account, revealed, cx))
                        .child(")")
                }))
        };
        let current = match pinned {
            None => active(cx).into_any_element(),
            Some(account) => redacted_login(account, revealed, cx).into_any_element(),
        };
        let count = group.selectable.len() + 1;
        let selected = pinned
            .and_then(|account| {
                group
                    .selectable
                    .iter()
                    .position(|candidate| candidate == account)
            })
            .map_or(0, |position| position + 1);
        let highlighted = self
            .account_menu
            .as_ref()
            .filter(|(menu_host, _)| *menu_host == host)
            .map(|(_, at)| *at);
        let on_key: KeyHandler = {
            let (view, host) = (view.clone(), host.clone());
            Rc::new(move |event, _, cx| {
                let host = host.clone();
                view.update(cx, |this, cx| {
                    this.account_key(&host, count, selected, event, cx)
                })
            })
        };
        let trigger = SelectTrigger::new(
            SharedString::from(format!("settings-source-control-account-{index}")),
            "",
            256.,
            handle,
            on_key,
        )
        .content(current);
        if !enabled {
            return div()
                .w(px(256.))
                .max_w_full()
                .opacity(0.64)
                .child(trigger)
                .into_any_element();
        }
        let selectable = group.selectable.clone();
        let active_account = group.active_account.clone();
        let (opened, picked) = (view.clone(), view.clone());
        let popup_host = host.clone();
        let hover_host = host.clone();
        Popover::new(SharedString::from(format!(
            "settings-source-control-account-menu-{index}"
        )))
        .anchor(Anchor::TopRight)
        .offset(px(4.))
        .open(highlighted.is_some())
        .p_0()
        .appearance(false)
        .on_open_change(move |next, _, cx| {
            let host = host.clone();
            opened.update(cx, |this, cx| {
                this.account_menu = next.then_some((host, selected));
                cx.notify();
            })
        })
        .trigger(PopTrigger::new(trigger))
        .content(move |_, _, cx| {
            let theme = Tokens::of(cx);
            let mut rows: Vec<AnyElement> = Vec::new();
            for position in 0..count {
                let account = position.checked_sub(1).map(|at| selectable[at].clone());
                let content: AnyElement = match &account {
                    None => h_flex()
                        .min_w_0()
                        .items_center()
                        .gap_1()
                        .child(format!("Active {CLI_NAME} account"))
                        .children(active_account.as_deref().map(|account| {
                            h_flex()
                                .items_center()
                                .child("(")
                                .child(redacted_login(account, revealed, cx))
                                .child(")")
                        }))
                        .into_any_element(),
                    Some(account) => redacted_login(account, revealed, cx).into_any_element(),
                };
                let (view, host) = (picked.clone(), popup_host.clone());
                let hover = (picked.clone(), hover_host.clone());
                rows.push(
                    div()
                        .id(("settings-source-control-account-item", position))
                        .min_h(px(28.))
                        .flex()
                        .items_center()
                        .px_2()
                        .py_1()
                        .rounded(px(4.))
                        .text_sm()
                        .cursor_pointer()
                        .when(position == selected, |item| {
                            item.bg(theme.foreground.opacity(0.08))
                        })
                        .when(highlighted == Some(position), |item| {
                            item.bg(theme.accent).text_color(theme.accent_foreground)
                        })
                        .on_mouse_move(move |_, _, cx| {
                            let host = hover.1.clone();
                            hover.0.update(cx, |this, cx| {
                                if this.account_menu.as_ref() != Some(&(host.clone(), position)) {
                                    this.account_menu = Some((host, position));
                                    cx.notify();
                                }
                            })
                        })
                        .on_click(move |_, _, cx| {
                            let host = host.clone();
                            let account = account.clone();
                            view.update(cx, |this, cx| {
                                this.pick_account(&host, account.as_deref(), cx)
                            })
                        })
                        .child(content)
                        .into_any_element(),
                );
            }
            popup_shell(256., cx).children(rows)
        })
        .into_any_element()
    }

    fn pick_account(&mut self, host: &str, account: Option<&str>, cx: &mut Context<Self>) {
        self.account_menu = None;
        self.save_account("github", host, None, Some(account), cx);
        cx.notify();
    }

    fn account_key(
        &mut self,
        host: &str,
        count: usize,
        selected: usize,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .account_menu
            .as_ref()
            .filter(|(menu_host, _)| menu_host == host)
            .map(|(_, at)| *at);
        let next = match (event.keystroke.key.as_str(), current) {
            ("enter" | "space" | "down" | "up", None) => Some(selected),
            ("down", Some(at)) => Some((at + 1) % count),
            ("up", Some(at)) => Some((at + count - 1) % count),
            ("enter" | "space", Some(at)) => {
                cx.stop_propagation();
                let account = at.checked_sub(1).and_then(|at| {
                    self.account_groups("github")
                        .into_iter()
                        .find(|group| group.host == host)
                        .and_then(|group| group.selectable.get(at).cloned())
                });
                self.pick_account(host, account.as_deref(), cx);
                return;
            }
            _ => return,
        };
        cx.stop_propagation();
        self.account_menu = next.map(|at| (host.to_owned(), at));
        cx.notify();
    }

    /// The control of a secret row: the field at `w-80`, and `Clear {label}` once a value is saved.
    fn secret_control(
        &self,
        kind: &'static str,
        key: &'static str,
        label: &'static str,
        field: &Entity<SecretField>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let saved = self.config.host_value(kind, key);
        let configured = secret::is_set(&saved);
        let view = cx.entity();
        let saving = self.saving();
        let clear = icon_button(
            SharedString::from(format!("settings-source-control-{kind}-{key}-clear")),
            Variant::GhostDestructive,
            !saving,
            cx,
        )
        .tooltip(tooltip(format!("Clear {label}")))
        .when(!saving, |button| {
            button.on_click(move |_, _, cx| {
                view.update(cx, |this, cx| {
                    let mut fields = Map::new();
                    fields.insert(key.to_owned(), Value::String(String::new()));
                    this.save_host(kind, fields, &[], cx);
                })
            })
        })
        .child(Icon::new(Icons::X).size(px(14.)));
        h_flex()
            .min_w_0()
            .items_start()
            .gap_2()
            .when(self.wide, |control| control.w(px(320.)))
            .when(!self.wide, |control| control.w_full())
            .child(div().min_w_0().flex_1().child(field.clone()))
            .children(configured.then_some(clear))
            .into_any_element()
    }

    /// The `Remove` row under the schema rows, while any value is saved.
    fn remove_row(
        &self,
        kind: &'static str,
        keys: &[&'static str],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let saved: Vec<&'static str> = keys
            .iter()
            .copied()
            .filter(|key| {
                let value = self.config.host_value(kind, key);
                value
                    .as_str()
                    .map_or(!value.is_null(), |text| !text.is_empty())
            })
            .collect();
        if saved.is_empty() {
            return None;
        }
        let saving = self.saving();
        let view = cx.entity();
        let keys: Vec<&'static str> = keys.to_vec();
        Some(
            div()
                .flex()
                .justify_end()
                .pt_2()
                .child(
                    xs_button(
                        SharedString::from(format!("settings-source-control-{kind}-remove")),
                        Variant::Outline,
                        !saving,
                        "Remove",
                        cx,
                    )
                    .when(!saving, |button| {
                        button.on_click(move |_, _, cx| {
                            let keys = keys.clone();
                            view.update(cx, |this, cx| {
                                let fields = keys
                                    .iter()
                                    .map(|key| ((*key).to_owned(), Value::String(String::new())))
                                    .collect();
                                this.save_host(kind, fields, &[], cx);
                            })
                        })
                    }),
                )
                .into_any_element(),
        )
    }

    fn bitbucket_panel(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let access = self.secret_control(
            "bitbucket",
            "accessToken",
            "Access token",
            &self.hosts.bitbucket_access,
            cx,
        );
        let api = self.secret_control(
            "bitbucket",
            "apiToken",
            "API token",
            &self.hosts.bitbucket_api,
            cx,
        );
        let saving = self.saving();
        let email = div()
            .min_w_0()
            .when(self.wide, |control| control.w(px(224.)))
            .when(!self.wide, |control| control.w_full())
            .child(sm_field(
                &self.hosts.bitbucket_email,
                false,
                false,
                saving,
                window,
                cx,
            ))
            .into_any_element();
        let rows = v_flex()
            .child(row(
                "Access token",
                "Scoped to one repository, project, or workspace. Create it in that item's Bitbucket settings.",
                None,
                access,
                self.wide,
                cx,
            ))
            .child(row(
                "Atlassian account email",
                "With an API token instead of an access token.",
                None,
                email,
                self.wide,
                cx,
            ))
            .child(row(
                "API token",
                "Reaches every repository your Atlassian account can. Give it read and write access to repositories and pull requests, and read:user:bitbucket.",
                None,
                api,
                self.wide,
                cx,
            ));
        v_flex()
            .child(rows)
            .children(self.remove_row("bitbucket", &["accessToken", "email", "apiToken"], cx))
            .into_any_element()
    }

    fn gitcafe_panel(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let token = self.secret_control("gitcafe", "token", "Token", &self.hosts.gitcafe_token, cx);
        v_flex()
            .child(row(
                "Token",
                "A git.cafe token, used before CAFE_TOKEN and the cafe login, so GitCafe works without the CLI signed in.",
                None,
                token,
                self.wide,
                cx,
            ))
            .children(self.remove_row("gitcafe", &["token"], cx))
            .into_any_element()
    }
}
