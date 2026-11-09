//! The Secrets tab: `SecretsSettings.tsx` (the provider rows and the remove confirmation) and
//! `SecretProviderDialog.tsx` (the add and edit dialog). The panel owns its dialog, inputs and
//! requests; settings writes are optimistic and put back when the call fails.

use super::components::{group, section_with_action};
use super::devices_panel::widgets::{Variant, button, dialog_of, field, labeled, note, problem};
use super::scope_inheritance::{PopTrigger, popup_shell};
use super::scope_pickers::ScopeBar;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::settings_labs::{Scope, resolve};
use crate::settings_secrets::{self as secrets, Account, Auth, Draft, Kind, Provider, Target};
use crate::status::CallEvent;
use gpui::{Anchor, ElementId, Subscription};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::popover::Popover;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

const DIALOG_WIDTH: f32 = 448.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Kind,
    Auth,
    Accounts,
}

struct Dialog {
    target: Target,
    kind: Kind,
    auth: Auth,
    /// The token already stored with the provider, kept when none is typed.
    stored_token: String,
    /// Errors show only once a save was tried.
    submitted: bool,
    menu: Option<Menu>,
}

struct Inputs {
    name: Entity<InputState>,
    account: Entity<InputState>,
    address: Entity<InputState>,
    mount: Entity<InputState>,
    namespace: Entity<InputState>,
    token: Entity<InputState>,
    region: Entity<InputState>,
    profile: Entity<InputState>,
}

impl Inputs {
    fn all(&self) -> [&Entity<InputState>; 8] {
        [
            &self.name,
            &self.account,
            &self.address,
            &self.mount,
            &self.namespace,
            &self.token,
            &self.region,
            &self.profile,
        ]
    }
}

/// What the scope bar says about the environment the tab manages.
struct Ctx {
    label: String,
    connected: bool,
    editable: bool,
    in_project: bool,
}

pub struct SecretsPanel {
    actions: ActionSender,
    scope: Entity<ScopeBar>,
    wide: bool,
    config: secrets::Config,
    /// Settings writes in flight, with the settings to fall back to.
    pending: HashMap<u64, Value>,
    next_token: u64,
    dialog: Option<Dialog>,
    /// The provider whose removal the confirmation asks about.
    confirm: Option<String>,
    /// Providers whose account name shows.
    revealed: HashSet<String>,
    accounts: Vec<Account>,
    accounts_token: Option<u64>,
    accounts_requested: bool,
    inputs: Inputs,
    _subscriptions: Vec<Subscription>,
}

fn text_of(input: &Entity<InputState>, cx: &App) -> String {
    input.read(cx).value().to_string()
}

impl SecretsPanel {
    pub fn new(
        actions: ActionSender,
        scope: Entity<ScopeBar>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let make = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let inputs = Inputs {
            name: make("Personal", window, cx),
            account: make("my.1password.com", window, cx),
            address: make("https://bao.example:8200", window, cx),
            mount: make("secret", window, cx),
            namespace: make("admin/platform", window, cx),
            token: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("s.xxxxxxxxxxxxxxxxxxxxxxxx")
                    .masked(true)
            }),
            region: make("us-east-1", window, cx),
            profile: make("production", window, cx),
        };
        let mut subscriptions = vec![cx.observe(&scope, |_, _, cx| cx.notify())];
        for input in inputs.all() {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    _ => {}
                },
            ));
        }
        Self {
            actions,
            scope,
            wide: true,
            config: secrets::Config::default(),
            pending: HashMap::new(),
            next_token: secrets::TOKEN_BASE,
            dialog: None,
            confirm: None,
            revealed: HashSet::new(),
            accounts: Vec::new(),
            accounts_token: None,
            accounts_requested: false,
            inputs,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    /// The attached environment's secret providers; ignored while a write is in flight.
    pub fn set_data(&mut self, config: secrets::Config, cx: &mut Context<Self>) {
        if self.pending.is_empty() && self.config != config {
            self.config = config;
            cx.notify();
        }
    }

    /// Closes the dialogs, as the settings page opens.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        self.confirm = None;
        self.revealed.clear();
        self.accounts_requested = false;
        cx.notify();
    }

    /// Closes a menu, then a dialog; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(dialog) = &mut self.dialog {
            if dialog.menu.take().is_none() {
                self.dialog = None;
            }
            cx.notify();
            return true;
        }
        if self.confirm.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    fn ctx(&self, cx: &App) -> Ctx {
        let scope = self.scope.read(cx);
        let (search, data) = (scope.search(), scope.data());
        let resolved = resolve(search, data);
        let selected = match &resolved {
            Scope::Environment(id) => data.environments.iter().find(|e| &e.id == id),
            _ => None,
        };
        let environment = selected
            .or_else(|| data.environments.iter().find(|e| e.connected && e.writable))
            .or_else(|| data.environments.iter().find(|e| e.connected))
            .or(data.environments.first());
        Ctx {
            label: environment.map_or_else(|| "Environment".to_owned(), |e| e.label.clone()),
            connected: environment.is_some_and(|e| e.connected),
            editable: environment.is_some_and(|e| e.connected && e.writable),
            in_project: matches!(resolved, Scope::Project(_)),
        }
    }

    fn providers(&self) -> Vec<Provider> {
        secrets::providers(&self.config.settings)
    }

    fn send(&mut self, call: Call, payload: Value) -> Option<u64> {
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call,
            payload,
        };
        self.actions.try_send(action).ok().map(|()| token)
    }

    /// Sends one settings patch, showing it at once.
    fn write(&mut self, patch: Value, cx: &mut Context<Self>) {
        let previous = self.config.settings.clone();
        secrets::apply_patch(&mut self.config.settings, &patch);
        match self.send(Call::UpdateSettings, json!({ "patch": patch })) {
            Some(token) => {
                self.pending.insert(token, previous);
            }
            None => self.config.settings = previous,
        }
        cx.notify();
    }

    /// The outcome of a write or of the accounts read.
    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        if self.accounts_token == Some(token) {
            self.accounts_token = None;
            if let CallEvent::Done(value) = event {
                self.accounts = secrets::decode_accounts(&value);
                cx.notify();
            }
            return;
        }
        let Some(previous) = self.pending.remove(&token) else {
            return;
        };
        if matches!(event, CallEvent::Failed(_)) {
            self.config.settings = previous;
            cx.notify();
        }
    }

    fn request_accounts(&mut self) {
        if self.accounts_requested {
            return;
        }
        if let Some(token) = self.send(Call::ListOnePasswordAccounts, json!({})) {
            self.accounts_requested = true;
            self.accounts_token = Some(token);
        }
    }

    fn fill(&mut self, draft: &Draft, window: &mut Window, cx: &mut Context<Self>) {
        let values = [
            (&self.inputs.name, draft.name.as_str()),
            (&self.inputs.account, draft.account.as_str()),
            (&self.inputs.address, draft.address.as_str()),
            (&self.inputs.mount, draft.mount.as_str()),
            (&self.inputs.namespace, draft.namespace.as_str()),
            (&self.inputs.token, draft.token.as_str()),
            (&self.inputs.region, draft.region.as_str()),
            (&self.inputs.profile, draft.profile.as_str()),
        ];
        for (input, value) in values {
            input.update(cx, |state, cx| {
                state.set_value(value.to_owned(), window, cx)
            });
        }
        self.sync_placeholders(draft.kind, !draft.stored_token.is_empty(), window, cx);
    }

    fn sync_placeholders(
        &self,
        kind: Kind,
        stored: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inputs.name.update(cx, |state, cx| {
            state.set_placeholder(kind.name_placeholder(), window, cx)
        });
        self.inputs.token.update(cx, |state, cx| {
            state.set_placeholder(
                if stored {
                    "Stored secret, enter a new value to replace"
                } else {
                    "s.xxxxxxxxxxxxxxxxxxxxxxxx"
                },
                window,
                cx,
            )
        });
    }

    fn open(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        let draft = match &target {
            Target::Add => Draft::empty(),
            Target::Edit(id) => match self.providers().iter().find(|p| &p.id == id) {
                Some(provider) => Draft::from_provider(provider),
                None => return,
            },
        };
        self.fill(&draft, window, cx);
        if draft.kind == Kind::OnePassword {
            self.request_accounts();
        }
        self.dialog = Some(Dialog {
            target,
            kind: draft.kind,
            auth: draft.auth,
            stored_token: draft.stored_token,
            submitted: false,
            menu: None,
        });
        let focus = self.inputs.name.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn draft(&self, dialog: &Dialog, cx: &App) -> Draft {
        Draft {
            kind: dialog.kind,
            name: text_of(&self.inputs.name, cx),
            account: text_of(&self.inputs.account, cx),
            address: text_of(&self.inputs.address, cx),
            mount: text_of(&self.inputs.mount, cx),
            namespace: text_of(&self.inputs.namespace, cx),
            auth: dialog.auth,
            token: text_of(&self.inputs.token, cx),
            stored_token: dialog.stored_token.clone(),
            region: text_of(&self.inputs.region, cx),
            profile: text_of(&self.inputs.profile, cx),
        }
    }

    fn check(&self, dialog: &Dialog, cx: &App) -> Result<(String, Value), Box<secrets::Issues>> {
        let ids: Vec<String> = self.providers().into_iter().map(|p| p.id).collect();
        secrets::from_draft(&self.draft(dialog, cx), &ids, &dialog.target)
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        dialog.submitted = true;
        let dialog = self.dialog.as_ref().expect("checked above");
        match self.check(dialog, cx) {
            Ok((id, entry)) => {
                self.dialog = None;
                self.write(secrets::save_patch(&id, entry), cx);
            }
            Err(_) => cx.notify(),
        }
    }

    fn set_menu(&mut self, menu: Menu, open: bool, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.dialog {
            if open {
                dialog.menu = Some(menu);
            } else if dialog.menu == Some(menu) {
                dialog.menu = None;
            }
            cx.notify();
        }
    }

    fn pick_kind(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let kind = Kind::ALL[index];
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        dialog.kind = kind;
        dialog.menu = None;
        let stored = !dialog.stored_token.is_empty();
        self.sync_placeholders(kind, stored, window, cx);
        if kind == Kind::OnePassword {
            self.request_accounts();
        }
        cx.notify();
    }

    fn pick_auth(&mut self, index: usize, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.dialog {
            dialog.auth = Auth::ALL[index];
            dialog.menu = None;
            cx.notify();
        }
    }

    fn pick_account(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(account) = self.accounts.get(index).map(|a| a.account.clone()) else {
            return;
        };
        self.inputs
            .account
            .update(cx, |state, cx| state.set_value(account, window, cx));
        self.set_menu(Menu::Accounts, false, cx);
    }

    fn remove(&mut self, id: &str, cx: &mut Context<Self>) {
        self.confirm = None;
        self.write(secrets::remove_patch(id), cx);
    }

    fn icon(kind: &str, color: gpui::Hsla, size: f32) -> AnyElement {
        match crate::assets::secret_provider_icon(kind) {
            Some(path) => gpui::svg()
                .path(path)
                .size(px(size))
                .flex_shrink_0()
                .text_color(color)
                .into_any_element(),
            None => Icon::new(Icons::KeyRound)
                .size(px(size))
                .text_color(color)
                .into_any_element(),
        }
    }

    fn small(
        id: impl Into<ElementId>,
        variant: Variant,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        button(id, variant, enabled, cx)
            .h(px(24.))
            .px(px(7.))
            .gap(px(4.))
            .text_size(px(12.))
            .line_height(px(16.))
    }

    fn provider_row(
        &self,
        provider: &Provider,
        index: usize,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let supported = provider.supported();
        let icon =
            supported.then(|| Self::icon(&provider.kind, theme.foreground.opacity(0.8), 18.));
        let description = match (&provider.classified, provider.summary()) {
            (secrets::Classified::OnePassword { account }, _) => {
                let shown = self.revealed.contains(&provider.id);
                let (text, color) = if shown {
                    (account.clone(), theme.muted_foreground)
                } else {
                    (
                        crate::settings_source_control::redacted(account),
                        theme.muted_foreground.opacity(0.4),
                    )
                };
                let id = provider.id.clone();
                h_flex()
                    .min_w_0()
                    .gap_1()
                    .child(div().flex_shrink_0().child("1Password \u{b7}"))
                    .child(
                        div()
                            .id(("secret-account", index))
                            .min_w_0()
                            .truncate()
                            .cursor_pointer()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_size(px(10.))
                            .line_height(px(15.))
                            .text_color(color)
                            .child(text)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.revealed.remove(&id) {
                                    this.revealed.insert(id.clone());
                                }
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            }
            (_, Some(summary)) => div().child(summary).into_any_element(),
            (_, None) => div()
                .child(format!("Unsupported provider ({})", provider.kind))
                .into_any_element(),
        };
        let text = v_flex()
            .min_w_0()
            .flex_1()
            .gap_1()
            .child(
                h_flex()
                    .min_h(px(20.))
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .children(icon)
                    .child(div().min_w_0().child(provider.label.clone())),
            )
            .child(
                div()
                    .max_w(px(672.))
                    .text_xs()
                    .line_height(px(18.))
                    .text_color(theme.muted_foreground)
                    .child(description),
            );
        let (edit_id, remove_id) = (provider.id.clone(), provider.id.clone());
        let control = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .when(supported, |controls| {
                controls.child(
                    Self::small(("secret-edit", index), Variant::Ghost, true, cx)
                        .child("Edit")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open(Target::Edit(edit_id.clone()), window, cx)
                        })),
                )
            })
            .child(
                Self::small(("secret-remove", index), Variant::Ghost, true, cx)
                    .child("Remove")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = Some(remove_id.clone());
                        cx.notify();
                    })),
            );
        let inner = if self.wide {
            h_flex().items_center().gap(px(32.)).child(text).child(
                div()
                    .min_w(px(160.))
                    .flex()
                    .justify_end()
                    .when(editable, |c| c.child(control)),
            )
        } else {
            v_flex()
                .gap_3()
                .child(text)
                .when(editable, |column| column.child(control))
        };
        div().px_4().py_3().child(inner).into_any_element()
    }

    fn empty_row(cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        div()
            .px_4()
            .py_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child("No secret providers configured."),
                    )
                    .child(
                        div()
                            .max_w(px(672.))
                            .text_xs()
                            .line_height(px(18.))
                            .text_color(theme.muted_foreground)
                            .child("Add 1Password, OpenBao, or AWS Secrets Manager, then pick it as the source of a provider variable."),
                    ),
            )
            .into_any_element()
    }

    /// `Select` (select.tsx): a trigger showing the choice and a popup of the options.
    #[allow(clippy::too_many_arguments)]
    fn select(
        view: &Entity<Self>,
        id: &'static str,
        menu: Menu,
        open: bool,
        disabled: bool,
        current: (Option<&'static str>, String),
        options: Vec<(Option<&'static str>, String)>,
        selected: usize,
        pick: fn(&mut Self, usize, &mut Window, &mut Context<Self>),
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let (icon, label) = current;
        let trigger = h_flex()
            .id(id)
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
            .when(disabled, |trigger| trigger.opacity(0.64))
            .when(!disabled, |trigger| trigger.cursor_pointer())
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .children(icon.map(|kind| Self::icon(kind, theme.foreground, 16.)))
                    .child(div().min_w_0().truncate().child(label)),
            )
            .child(
                div()
                    .opacity(0.7)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            );
        let (opened, picked) = (view.clone(), view.clone());
        Popover::new(SharedString::from(format!("{id}-menu")))
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .open(open && !disabled)
            .p_0()
            .appearance(false)
            .on_open_change(move |next, _, cx| {
                opened.update(cx, |this, cx| this.set_menu(menu, *next, cx))
            })
            .trigger(PopTrigger::new(trigger))
            .content(move |_, _, cx| {
                let theme = Tokens::of(cx);
                let accent = theme.accent;
                popup_shell(cx)
                    .w(px(DIALOG_WIDTH - 48.))
                    .max_w_full()
                    .max_h(px(240.))
                    .p_1()
                    .children(options.iter().enumerate().map(|(index, (icon, label))| {
                        let view = picked.clone();
                        h_flex()
                            .id((id, index))
                            .min_h(px(28.))
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .text_sm()
                            .cursor_pointer()
                            .text_color(theme.foreground)
                            .hover(move |style| style.bg(accent))
                            .when(index == selected, |item| {
                                item.font_weight(FontWeight::MEDIUM)
                            })
                            .children(icon.map(|kind| Self::icon(kind, theme.foreground, 16.)))
                            .child(label.clone())
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| pick(this, index, window, cx))
                            })
                    }))
            })
            .into_any_element()
    }

    fn account_row(
        &self,
        view: &Entity<Self>,
        open: bool,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let input = field(&self.inputs.account, false, window, cx);
        if self.accounts.is_empty() {
            return input.into_any_element();
        }
        let chevron = div()
            .id("secret-accounts")
            .size(px(32.))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .cursor_pointer()
            .opacity(0.7)
            .child(Icon::new(Icons::ChevronDown).size(px(14.)));
        let (opened, picked) = (view.clone(), view.clone());
        let accounts = self.accounts.clone();
        let popover = Popover::new("secret-accounts-menu")
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change(move |next, _, cx| {
                opened.update(cx, |this, cx| this.set_menu(Menu::Accounts, *next, cx))
            })
            .trigger(PopTrigger::new(chevron))
            .content(move |_, _, cx| {
                let theme = Tokens::of(cx);
                let accent = theme.accent;
                popup_shell(cx)
                    .w(px(320.))
                    .max_w_full()
                    .max_h(px(240.))
                    .p_1()
                    .children(accounts.iter().enumerate().map(|(index, account)| {
                        let view = picked.clone();
                        h_flex()
                            .id(("secret-account-item", index))
                            .min_h(px(28.))
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .text_sm()
                            .cursor_pointer()
                            .hover(move |style| style.bg(accent))
                            .child(div().min_w_0().truncate().child(account.account.clone()))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(account.email.clone()),
                            )
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| this.pick_account(index, window, cx))
                            })
                    }))
            });
        h_flex()
            .w_full()
            .gap_1()
            .child(div().min_w_0().flex_1().child(input))
            .child(popover)
            .into_any_element()
    }

    fn dialog_view(
        &self,
        view: &Entity<Self>,
        label: &str,
        window: &mut Window,
        cx: &App,
    ) -> AnyElement {
        let Some(dialog) = &self.dialog else {
            return div().into_any_element();
        };
        let editing = matches!(dialog.target, Target::Edit(_));
        let result = self.check(dialog, cx);
        let issues = match (&result, dialog.submitted) {
            (Err(issues), true) => (**issues).clone(),
            _ => secrets::Issues::default(),
        };
        let invalid = result.is_err();
        let existing_label = match &dialog.target {
            Target::Edit(id) => self
                .providers()
                .into_iter()
                .find(|p| &p.id == id)
                .map(|p| p.label),
            Target::Add => None,
        };
        let title = match existing_label {
            Some(label) => format!("Edit {label}"),
            None => "Add a secret provider".to_owned(),
        };
        let description = format!(
            "Provider variables on {label} can read values from it by name. Values are read on that server when a provider starts."
        );
        let with_error = |input: gpui::Div, message: &Option<String>| {
            v_flex()
                .gap(px(6.))
                .child(input)
                .children(message.clone().map(|message| problem(message, cx)))
        };
        let mut body: Vec<AnyElement> = Vec::new();
        let kinds = Kind::ALL
            .iter()
            .map(|kind| (Some(kind.wire()), kind.label().to_owned()))
            .collect();
        let selected = Kind::ALL
            .iter()
            .position(|kind| *kind == dialog.kind)
            .unwrap_or(0);
        body.push(
            labeled(
                "Kind",
                Self::select(
                    view,
                    "secret-provider-kind",
                    Menu::Kind,
                    dialog.menu == Some(Menu::Kind),
                    editing,
                    (Some(dialog.kind.wire()), dialog.kind.label().to_owned()),
                    kinds,
                    selected,
                    Self::pick_kind,
                    cx,
                ),
            )
            .into_any_element(),
        );
        body.push(
            labeled(
                "Name",
                with_error(field(&self.inputs.name, false, window, cx), &issues.name),
            )
            .into_any_element(),
        );
        match dialog.kind {
            Kind::OnePassword => {
                body.push(
                    labeled(
                        "Account",
                        v_flex()
                            .gap(px(6.))
                            .child(self.account_row(view, dialog.menu == Some(Menu::Accounts), window, cx))
                            .children(issues.account.clone().map(|message| problem(message, cx)))
                            .child(note(
                                "Accounts the op CLI is signed into are suggested. Type any other shorthand, sign-in address without https://, or ID.",
                                cx,
                            )),
                    )
                    .into_any_element(),
                );
            }
            Kind::OpenBao => {
                body.push(
                    labeled(
                        "Address",
                        with_error(
                            field(&self.inputs.address, false, window, cx),
                            &issues.address,
                        ),
                    )
                    .into_any_element(),
                );
                body.push(
                    labeled(
                        "Mount",
                        with_error(field(&self.inputs.mount, false, window, cx), &issues.mount),
                    )
                    .into_any_element(),
                );
                body.push(
                    labeled(
                        "Namespace (optional)",
                        with_error(
                            field(&self.inputs.namespace, false, window, cx),
                            &issues.namespace,
                        ),
                    )
                    .into_any_element(),
                );
                let auths = Auth::ALL
                    .iter()
                    .map(|auth| (None, auth.label().to_owned()))
                    .collect();
                let selected = Auth::ALL
                    .iter()
                    .position(|auth| *auth == dialog.auth)
                    .unwrap_or(0);
                body.push(
                    labeled(
                        "Authentication",
                        v_flex()
                            .gap(px(6.))
                            .child(Self::select(
                                view,
                                "secret-provider-auth",
                                Menu::Auth,
                                dialog.menu == Some(Menu::Auth),
                                false,
                                (None, dialog.auth.label().to_owned()),
                                auths,
                                selected,
                                Self::pick_auth,
                                cx,
                            ))
                            .child(note(dialog.auth.hint(), cx)),
                    )
                    .into_any_element(),
                );
                if dialog.auth == Auth::Token {
                    body.push(
                        labeled(
                            "Token",
                            with_error(field(&self.inputs.token, false, window, cx), &issues.token),
                        )
                        .into_any_element(),
                    );
                }
            }
            Kind::Aws => {
                body.push(
                    labeled(
                        "Region",
                        with_error(
                            field(&self.inputs.region, false, window, cx),
                            &issues.region,
                        ),
                    )
                    .into_any_element(),
                );
                body.push(
                    labeled(
                        "Profile (optional)",
                        v_flex()
                            .gap(px(6.))
                            .child(field(&self.inputs.profile, false, window, cx))
                            .children(issues.profile.clone().map(|message| problem(message, cx)))
                            .child(note(
                                "Credentials come from the aws CLI configuration on the server: this profile, or its default chain when empty.",
                                cx,
                            )),
                    )
                    .into_any_element(),
                );
            }
        }
        let enabled = !(dialog.submitted && invalid);
        let (cancel, save, dismiss) = (view.clone(), view.clone(), view.clone());
        let footer = vec![
            button("secret-provider-cancel", Variant::Ghost, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    })
                })
                .into_any_element(),
            button("secret-provider-save", Variant::Primary, enabled, cx)
                .child(if editing {
                    "Save changes"
                } else {
                    "Add provider"
                })
                .when(enabled, |save_button| {
                    save_button.on_click(move |_, window, cx| {
                        save.update(cx, |this, cx| this.submit(window, cx))
                    })
                })
                .into_any_element(),
        ];
        dialog_of(
            "secret-provider-dialog",
            DIALOG_WIDTH,
            title,
            description,
            body,
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.dialog = None;
                    cx.notify();
                })
            },
            cx,
        )
    }

    fn confirm_view(&self, view: &Entity<Self>, provider: &Provider, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let uses = secrets::uses(&self.config.settings, &provider.id);
        let label = provider.label.clone();
        let description = if uses.is_empty() {
            format!("Nothing reads from {label}. The secrets themselves are untouched.")
        } else {
            format!(
                "These read from {label} and will be unset. The secrets themselves are untouched."
            )
        };
        let mut body: Vec<AnyElement> = Vec::new();
        if !uses.is_empty() {
            body.push(
                v_flex()
                    .id("secret-provider-uses")
                    .max_h(px(192.))
                    .overflow_y_scroll()
                    .gap(px(2.))
                    .text_sm()
                    .children(
                        uses.into_iter()
                            .map(|usage| div().child(format!("\u{2022} {usage}"))),
                    )
                    .into_any_element(),
            );
        }
        let id = provider.id.clone();
        let (cancel, confirm, dismiss) = (view.clone(), view.clone(), view.clone());
        let destructive = theme.destructive;
        let footer = vec![
            button("secret-provider-remove-cancel", Variant::Outline, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.confirm = None;
                        cx.notify();
                    })
                })
                .into_any_element(),
            button("secret-provider-remove-confirm", Variant::Ghost, true, cx)
                .bg(destructive)
                .border_color(destructive)
                .text_color(gpui::white())
                .hover(move |style| style.bg(destructive.opacity(0.9)))
                .child("Remove provider")
                .on_click(move |_, _, cx| {
                    let id = id.clone();
                    confirm.update(cx, |this, cx| this.remove(&id, cx))
                })
                .into_any_element(),
        ];
        dialog_of(
            "secret-provider-remove",
            DIALOG_WIDTH,
            format!("Remove {label}?"),
            description,
            body,
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.confirm = None;
                    cx.notify();
                })
            },
            cx,
        )
    }

    /// The dialog or the confirmation, over a backdrop.
    pub fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ctx = self.ctx(cx);
        if !ctx.editable || ctx.in_project || !self.config.manageable {
            self.dialog = None;
            self.confirm = None;
            return None;
        }
        let view = cx.entity();
        if let Some(id) = self.confirm.clone() {
            return match self.providers().into_iter().find(|p| p.id == id) {
                Some(provider) => Some(self.confirm_view(&view, &provider, cx)),
                None => {
                    self.confirm = None;
                    None
                }
            };
        }
        self.dialog.as_ref()?;
        Some(self.dialog_view(&view, &ctx.label, window, cx))
    }
}

impl Render for SecretsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = self.ctx(cx);
        let note_line = |text: String| div().px_4().child(note(text, cx));
        if !ctx.connected {
            return note_line(
                "Connect an environment to manage the secret providers its provider variables read from."
                    .to_owned(),
            );
        }
        if !self.config.manageable {
            return note_line(format!(
                "Update T3 Code on {} to manage its secret providers.",
                ctx.label
            ));
        }
        let read_only = !ctx.editable || ctx.in_project;
        let providers = self.providers();
        let mut rows: Vec<AnyElement> = Vec::new();
        if ctx.in_project {
            rows.push(
                div()
                    .px_4()
                    .pt_3()
                    .child(note(
                        "Environment-wide setting. Select an environment to change it.",
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        if providers.is_empty() {
            rows.push(Self::empty_row(cx));
        } else {
            for (index, provider) in providers.iter().enumerate() {
                rows.push(self.provider_row(provider, index, !read_only, cx));
            }
        }
        let add = (!read_only).then(|| {
            Self::small("secret-provider-add", Variant::Outline, true, cx)
                .child(Icon::new(Icons::Plus).size(px(12.)))
                .child("Add provider")
                .on_click(cx.listener(|this, _, window, cx| this.open(Target::Add, window, cx)))
                .into_any_element()
        });
        section_with_action("Secret providers", add, group(rows, cx))
    }
}
