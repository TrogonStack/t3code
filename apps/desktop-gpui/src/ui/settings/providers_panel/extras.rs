//! The sections under the Providers card: "Usage providers" (`UsageProviderSettings.tsx` with
//! `AddUsageLimitSourceDialog.tsx`), "Text generators" (`TextGeneratorsSettings.tsx`, its wizard,
//! model list and task pickers) and "Advanced" (`AdvancedSettings.tsx`, the health check interval
//! and `CompactIdleThreadsRow`). Writes are optimistic and put back by the panel when they fail.

use super::super::components::{
    KeyHandler, SelectTrigger, Toggle, info_tip, popup_shell, reset_button_labeled, tab_stop,
};
use super::super::devices_panel::widgets::{menu_item, spinner};
use super::super::scope_inheritance::PopTrigger;
use super::super::wizard::{
    StepHandler, wizard_footer, wizard_header, wizard_panel, wizard_popup, wizard_steps,
};
use super::*;
use crate::providers::PickerInstance;
use crate::settings_provider_extras::{
    self as ext, Brand, IdleMode, Model, PRESETS, TASKS, TextGenerator,
};
use gpui::{Anchor, ElementId, Stateful, img};
use gpui_component::input::Input;
use gpui_component::popover::Popover;

mod task_picker;

const HUB_WIDTH: f32 = 448.;
const DESCRIPTION: &str = "Writes thread titles, commit messages, and other short text without starting a coding agent. Your API key is stored on this environment.";
const HEALTH_TIP: &str = "This interval is configured here, then the shared Background activity policy decides whether provider probes may run when the timer fires. Custom intervals appear as Advanced in Maintenance settings.";

#[derive(Clone, PartialEq, Eq)]
enum Open {
    Row(String),
    Task(usize),
    Idle,
}

#[derive(Clone, Copy)]
enum Which {
    Hub,
    Wizard,
}

enum Pending {
    Check(Which),
    Connect { id: String, config: Value },
    Models { id: String, refresh: bool },
    Test(usize),
    Favorite { before: Vec<(String, String)> },
}

enum Models {
    Loading,
    Loaded(Vec<Model>),
    Failed(String),
}

struct Wizard {
    existing: Option<TextGenerator>,
    step: usize,
    preset: usize,
    checking: bool,
    error: Option<String>,
    saved_id: Option<String>,
    model_count: usize,
}

struct Tested {
    key: String,
    busy: bool,
    error: Option<String>,
}

pub(super) struct Extras {
    restricted: bool,
    open: Option<Open>,
    highlight: usize,
    hub_open: bool,
    hub_url: Entity<InputState>,
    hub_label: Entity<InputState>,
    hub_key: Entity<SecretField>,
    hub_remove: Option<String>,
    gen_remove: Option<String>,
    wizard: Option<Wizard>,
    wiz_url: Entity<InputState>,
    wiz_name: Entity<InputState>,
    wiz_key: Entity<SecretField>,
    models: HashMap<String, Models>,
    models_for: Option<String>,
    models_refreshing: bool,
    models_error: Option<String>,
    models_search: Entity<InputState>,
    tests: [Option<Tested>; 3],
    health: Entity<InputState>,
    health_shown: i64,
    agents: Vec<PickerInstance>,
    favorites: Vec<(String, String)>,
    pick: crate::model_picker::State,
    pick_search: Entity<InputState>,
    pick_keys: Option<Subscription>,
    calls: HashMap<u64, Pending>,
    name_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl Extras {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ProvidersPanel>) -> Self {
        let input =
            |placeholder: &'static str, window: &mut Window, cx: &mut Context<ProvidersPanel>| {
                cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
            };
        let hub_url = input("https://hub.example.ts.net:8318", window, cx);
        let hub_label = input("Defaults to the hub's host name", window, cx);
        let wiz_url = input("http://localhost:11434/v1", window, cx);
        let wiz_name = input("OpenAI", window, cx);
        let models_search = input("Search models", window, cx);
        let health = input("300", window, cx);
        let pick_search = input("Search models...", window, cx);
        let hub_key = cx.new(|cx| SecretField::new("usage-hub-key", "", window, cx));
        let wiz_key = cx.new(|cx| SecretField::new("text-generator-key", "", window, cx));
        let subscriptions = vec![
            cx.subscribe_in(
                &hub_url,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.submit_hub(cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(&hub_label, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_hub(cx);
                }
            }),
            cx.subscribe_in(
                &hub_key,
                window,
                |this, _, event: &SecretEvent, _, cx| match event {
                    SecretEvent::Check(provider, reference) => {
                        this.check_secret(Which::Hub, provider, reference, cx)
                    }
                    SecretEvent::Submit => this.submit_hub(cx),
                    SecretEvent::Changed | SecretEvent::Commit => cx.notify(),
                },
            ),
            cx.subscribe_in(
                &wiz_url,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => this.wizard_changed(cx),
                    InputEvent::PressEnter { .. } => this.connect(cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &wiz_key,
                window,
                |this, _, event: &SecretEvent, _, cx| match event {
                    SecretEvent::Check(provider, reference) => {
                        this.check_secret(Which::Wizard, provider, reference, cx)
                    }
                    SecretEvent::Submit => this.connect(cx),
                    SecretEvent::Changed => this.wizard_changed(cx),
                    SecretEvent::Commit => cx.notify(),
                },
            ),
            cx.subscribe_in(&pick_search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&models_search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(
                &health,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        this.commit_health(window, cx);
                    }
                },
            ),
        ];
        Self {
            restricted: false,
            open: None,
            highlight: 0,
            hub_open: false,
            hub_url,
            hub_label,
            hub_key,
            hub_remove: None,
            gen_remove: None,
            wizard: None,
            wiz_url,
            wiz_name,
            wiz_key,
            models: HashMap::new(),
            models_for: None,
            models_refreshing: false,
            models_error: None,
            models_search,
            tests: [None, None, None],
            health,
            health_shown: i64::MIN,
            agents: Vec::new(),
            favorites: Vec::new(),
            pick: crate::model_picker::State::default(),
            pick_search,
            pick_keys: None,
            calls: HashMap::new(),
            name_subscription: None,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn set_restricted(&mut self, restricted: bool) {
        self.restricted = restricted;
    }
}

fn xs_button(
    id: impl Into<ElementId>,
    variant: Variant,
    enabled: bool,
    cx: &App,
) -> Stateful<gpui::Div> {
    button(id, variant, enabled, cx)
        .h(px(24.))
        .px(px(7.))
        .gap(px(4.))
        .text_size(px(12.))
        .line_height(px(16.))
}

fn destructive_button(id: impl Into<ElementId>, cx: &App) -> Stateful<gpui::Div> {
    let destructive = Tokens::of(cx).destructive;
    button(id, Variant::Ghost, true, cx)
        .bg(destructive)
        .border_color(destructive)
        .text_color(gpui::white())
        .hover(move |style| style.bg(destructive.opacity(0.9)))
}

fn driver_key(brand: Brand) -> Option<&'static str> {
    match brand {
        Brand::Anthropic => Some("anthropic"),
        Brand::OpenRouter => Some("openrouter"),
        Brand::OpenAi => Some("openai"),
        Brand::Ollama => Some("ollama"),
        Brand::OpenAiCompatible => None,
    }
}

/// `ProviderInstanceIcon` without a badge: the brand's art, or the initials of `name`.
fn brand_icon(brand: Brand, name: &str, size: f32, cx: &App) -> AnyElement {
    let dark = Tokens::of(cx).dark;
    match driver_key(brand).and_then(|key| crate::assets::provider_icon(key, dark)) {
        Some(path) => img(path).size(px(size)).flex_shrink_0().into_any_element(),
        None => div()
            .size(px(size))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .text_size(px(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(crate::providers::initials(name))
            .into_any_element(),
    }
}

fn preset_brand(index: usize) -> Brand {
    let preset = PRESETS[index];
    ext::brand(&json!({"kind": preset.kind, "baseUrl": preset.base_url}))
}

fn valid_url(url: &str) -> bool {
    url.split_once("://")
        .is_some_and(|(scheme, rest)| matches!(scheme, "http" | "https") && !rest.trim().is_empty())
}

impl ProvidersPanel {
    fn write_patch(&mut self, patch: Value, cx: &mut Context<Self>) {
        let previous = self.settings.clone();
        ext::apply_patch(&mut self.settings, &patch);
        match self.send(Call::UpdateSettings, json!({"patch": patch})) {
            Some(token) => {
                self.pending.insert(token, previous);
            }
            None => self.settings = previous,
        }
        cx.notify();
    }

    fn call(&mut self, call: Call, payload: Value, pending: Pending) -> bool {
        match self.send(call, payload) {
            Some(token) => {
                self.extras.calls.insert(token, pending);
                true
            }
            None => false,
        }
    }

    fn check_secret(
        &mut self,
        which: Which,
        provider: &str,
        reference: &str,
        cx: &mut Context<Self>,
    ) {
        let payload = crate::secret_source::check_payload(provider, reference);
        let field = match which {
            Which::Hub => self.extras.hub_key.clone(),
            Which::Wizard => self.extras.wiz_key.clone(),
        };
        let state = if self.call(Call::CheckSecretSource, payload, Pending::Check(which)) {
            Check::Checking
        } else {
            Check::Failed("The check could not be sent.".to_owned())
        };
        field.update(cx, |field, cx| field.set_check(state, cx));
    }

    fn set_open(&mut self, open: Option<Open>, cx: &mut Context<Self>) {
        if self.extras.open != open {
            self.extras.highlight = 0;
            if matches!(open, Some(Open::Task(_))) {
                let ids: Vec<String> = ext::text_generators(&self.settings)
                    .into_iter()
                    .filter(TextGenerator::enabled)
                    .map(|generator| generator.id)
                    .collect();
                for id in ids {
                    self.ensure_models(&id);
                }
            }
            if !matches!(open, Some(Open::Task(_))) {
                self.extras.pick_keys = None;
            }
            self.extras.open = open;
            cx.notify();
        }
    }

    /// Loads the models of a generator once; later views reuse the list.
    fn ensure_models(&mut self, id: &str) {
        if self.extras.models.contains_key(id) {
            return;
        }
        let config = ext::text_generators(&self.settings)
            .into_iter()
            .find(|generator| generator.id == id)
            .map(|generator| generator.config);
        let payload = ext::list_models_input(id, config.as_ref());
        let pending = Pending::Models {
            id: id.to_owned(),
            refresh: false,
        };
        let state = if self.call(Call::ListTextGeneratorModels, payload, pending) {
            Models::Loading
        } else {
            Models::Failed("The request could not be sent.".to_owned())
        };
        self.extras.models.insert(id.to_owned(), state);
    }

    fn refresh_models(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.extras.models_refreshing {
            return;
        }
        let pending = Pending::Models {
            id: id.to_owned(),
            refresh: true,
        };
        if self.call(
            Call::ListTextGeneratorModels,
            ext::refresh_models_input(id),
            pending,
        ) {
            self.extras.models_refreshing = true;
            self.extras.models_error = None;
            cx.notify();
        }
    }

    /// The outcome of a call this section started; false when the token is not one of them.
    pub(super) fn extras_call(
        &mut self,
        token: u64,
        event: &CallEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pending) = self.extras.calls.remove(&token) else {
            return false;
        };
        match pending {
            Pending::Check(which) => {
                let state = match event {
                    CallEvent::Done(value) if value["_tag"] == "Resolved" => Check::Resolved,
                    CallEvent::Done(value) => Check::Failed(
                        value["detail"]
                            .as_str()
                            .unwrap_or("The secret could not be resolved.")
                            .to_owned(),
                    ),
                    CallEvent::Failed(message) => Check::Failed(message.clone()),
                    CallEvent::Progress(_) => return true,
                };
                let field = match which {
                    Which::Hub => self.extras.hub_key.clone(),
                    Which::Wizard => self.extras.wiz_key.clone(),
                };
                field.update(cx, |field, cx| field.set_check(state, cx));
            }
            Pending::Connect { id, config } => match event {
                CallEvent::Done(value) => {
                    let models = ext::parse_models(value);
                    let count = models.len();
                    self.write_patch(ext::text_generator_patch(&id, config), cx);
                    self.extras
                        .models
                        .insert(id.clone(), Models::Loaded(models));
                    if let Some(wizard) = &mut self.extras.wizard {
                        wizard.checking = false;
                        wizard.saved_id = Some(id);
                        wizard.model_count = count;
                        wizard.step = 1;
                    }
                    self.extras
                        .wiz_key
                        .update(cx, |field, cx| field.set_disabled(false, cx));
                }
                CallEvent::Failed(message) => {
                    if let Some(wizard) = &mut self.extras.wizard {
                        wizard.checking = false;
                        wizard.error = Some(message.clone());
                    }
                    self.extras
                        .wiz_key
                        .update(cx, |field, cx| field.set_disabled(false, cx));
                }
                CallEvent::Progress(_) => {}
            },
            Pending::Models { id, refresh } => match event {
                CallEvent::Done(value) => {
                    self.extras
                        .models
                        .insert(id, Models::Loaded(ext::parse_models(value)));
                    if refresh {
                        self.extras.models_refreshing = false;
                    }
                }
                CallEvent::Failed(message) => {
                    if refresh {
                        self.extras.models_refreshing = false;
                        self.extras.models_error = Some(message.clone());
                    } else {
                        self.extras
                            .models
                            .insert(id, Models::Failed(message.clone()));
                    }
                }
                CallEvent::Progress(_) => {}
            },
            Pending::Favorite { before } => {
                if matches!(event, CallEvent::Failed(_)) {
                    self.extras.favorites = before;
                }
            }
            Pending::Test(index) => {
                if let Some(test) = &mut self.extras.tests[index] {
                    test.busy = false;
                    if let CallEvent::Failed(message) = event {
                        test.error = Some(message.clone());
                    }
                }
            }
        }
        cx.notify();
        true
    }

    /// Closes the topmost popup or dialog; returns whether there was one.
    pub(super) fn extras_close(&mut self, cx: &mut Context<Self>) -> bool {
        let closed = self.extras.open.take().is_some()
            || self.extras.wizard.take().is_some()
            || self.extras.models_for.take().is_some()
            || self.extras.gen_remove.take().is_some()
            || std::mem::take(&mut self.extras.hub_open)
            || self.extras.hub_remove.take().is_some()
            || self
                .extras
                .hub_key
                .update(cx, |field, cx| field.close_menu(cx))
            || self
                .extras
                .wiz_key
                .update(cx, |field, cx| field.close_menu(cx));
        if closed {
            self.extras.pick_keys = None;
            cx.notify();
        }
        closed
    }

    pub(super) fn extras_reload(&mut self, cx: &mut Context<Self>) {
        self.extras.open = None;
        self.extras.pick_keys = None;
        self.extras.wizard = None;
        self.extras.models_for = None;
        self.extras.gen_remove = None;
        self.extras.hub_open = false;
        self.extras.hub_remove = None;
        self.extras.models.clear();
        self.extras.tests = [None, None, None];
        cx.notify();
    }

    fn open_hub(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for input in [&self.extras.hub_url, &self.extras.hub_label] {
            input.update(cx, |state, cx| state.set_value(String::new(), window, cx));
        }
        let sources = self.sources.clone();
        self.extras.hub_key.update(cx, |field, cx| {
            field.set_sources(&sources, cx);
            field.clear(window, cx);
        });
        self.extras.hub_open = true;
        let focus = self.extras.hub_url.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn can_add_hub(&self, cx: &App) -> bool {
        !self.extras.hub_url.read(cx).value().trim().is_empty()
            && self.extras.hub_key.read(cx).has_draft(cx)
    }

    fn submit_hub(&mut self, cx: &mut Context<Self>) {
        if !self.extras.hub_open || !self.can_add_hub(cx) {
            return;
        }
        let url = self.extras.hub_url.read(cx).value().trim().to_owned();
        let label = self.extras.hub_label.read(cx).value().to_string();
        let Some(key) = self.extras.hub_key.read(cx).submission(cx) else {
            return;
        };
        let id = ext::source_id_from_url(&url, &ext::usage_sources(&self.settings));
        self.extras.hub_open = false;
        self.write_patch(ext::add_usage_source_patch(&id, &url, &label, key), cx);
    }

    fn open_wizard(
        &mut self,
        editing: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = editing.and_then(|id| {
            ext::text_generators(&self.settings)
                .into_iter()
                .find(|generator| generator.id == id)
        });
        let (url, label, key, placeholder) = match &existing {
            Some(generator) => (
                generator.base_url().to_owned(),
                generator.config["label"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                generator.config["apiKey"].clone(),
                generator.name(),
            ),
            None => (
                PRESETS[0].base_url.to_owned(),
                String::new(),
                Value::Null,
                PRESETS[0].label.to_owned(),
            ),
        };
        self.extras
            .wiz_url
            .update(cx, |state, cx| state.set_value(url, window, cx));
        self.set_name_input(&placeholder, &label, window, cx);
        let sources = self.sources.clone();
        self.extras.wiz_key.update(cx, |field, cx| {
            field.set_sources(&sources, cx);
            field.set_disabled(false, cx);
            field.load(&key, window, cx);
        });
        let saved_id = existing.as_ref().map(|generator| generator.id.clone());
        self.extras.wizard = Some(Wizard {
            existing,
            step: 0,
            preset: 0,
            checking: false,
            error: None,
            saved_id,
            model_count: 0,
        });
        self.extras.open = None;
        cx.notify();
    }

    fn set_name_input(
        &mut self,
        placeholder: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.to_owned();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let text = text.to_owned();
        input.update(cx, |state, cx| state.set_value(text, window, cx));
        self.extras.name_subscription =
            Some(
                cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.connect(cx);
                    }
                }),
            );
        self.extras.wiz_name = input;
    }

    fn wizard_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(wizard) = &mut self.extras.wizard {
            wizard.error = None;
        }
        cx.notify();
    }

    fn pick_preset(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.extras.wizard else {
            return;
        };
        if wizard.checking || wizard.existing.is_some() {
            return;
        }
        wizard.preset = index;
        wizard.error = None;
        let url = PRESETS[index].base_url.to_owned();
        let label = self.extras.wiz_name.read(cx).value().to_string();
        let placeholder = if PRESETS[index].id == "custom" {
            "e.g. My local model"
        } else {
            PRESETS[index].label
        };
        self.extras
            .wiz_url
            .update(cx, |state, cx| state.set_value(url, window, cx));
        self.set_name_input(placeholder, &label, window, cx);
        cx.notify();
    }

    fn shows_url(wizard: &Wizard) -> bool {
        match &wizard.existing {
            None => PRESETS[wizard.preset].id == "custom",
            Some(generator) => !matches!(
                generator.brand(),
                Brand::OpenAi | Brand::Anthropic | Brand::OpenRouter
            ),
        }
    }

    fn key_required(wizard: &Wizard) -> bool {
        wizard.existing.is_none() && PRESETS[wizard.preset].id != "custom"
    }

    /// The config Connect would save, or `None` while the form is incomplete.
    fn wizard_config(&self, cx: &App) -> Option<Value> {
        let wizard = self.extras.wizard.as_ref()?;
        let preset = PRESETS[wizard.preset];
        let url = if Self::shows_url(wizard) {
            self.extras.wiz_url.read(cx).value().to_string()
        } else if let Some(generator) = &wizard.existing {
            generator.base_url().to_owned()
        } else {
            preset.base_url.to_owned()
        };
        if !valid_url(url.trim()) {
            return None;
        }
        let stored = wizard
            .existing
            .as_ref()
            .and_then(|generator| generator.config["apiKey"].as_str())
            .filter(|key| !key.is_empty())
            .map(|key| Value::String(key.to_owned()));
        if !self.extras.wiz_key.read(cx).is_valid() {
            return None;
        }
        let key = self.extras.wiz_key.read(cx).submission(cx).or(stored);
        if Self::key_required(wizard) && key.is_none() {
            return None;
        }
        let kind = wizard
            .existing
            .as_ref()
            .map_or(preset.kind, TextGenerator::kind);
        let enabled = wizard.existing.as_ref().is_none_or(TextGenerator::enabled);
        let label = self.extras.wiz_name.read(cx).value().to_string();
        Some(ext::wizard_config(kind, &url, key, &label, enabled))
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        let Some(config) = self.wizard_config(cx) else {
            return;
        };
        let Some(wizard) = &self.extras.wizard else {
            return;
        };
        if wizard.checking || wizard.step != 0 {
            return;
        }
        let id = wizard.saved_id.clone().unwrap_or_else(|| {
            let ids: Vec<String> = self.instances.iter().map(|row| row.id.clone()).collect();
            ext::next_text_generator_id(PRESETS[wizard.preset].id, &self.settings, &ids)
        });
        let payload = ext::list_models_input(&id, Some(&config));
        let sent = self.call(
            Call::ListTextGeneratorModels,
            payload,
            Pending::Connect { id, config },
        );
        if let Some(wizard) = &mut self.extras.wizard {
            if sent {
                wizard.checking = true;
                wizard.error = None;
            } else {
                wizard.error = Some("The request could not be sent.".to_owned());
            }
        }
        let checking = self.extras.wizard.as_ref().is_some_and(|w| w.checking);
        self.extras
            .wiz_key
            .update(cx, |field, cx| field.set_disabled(checking, cx));
        cx.notify();
    }

    fn test_task(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((instance, model)) = ext::task_selection(&self.settings, TASKS[index].0) else {
            return;
        };
        if self.extras.tests[index]
            .as_ref()
            .is_some_and(|test| test.busy)
        {
            return;
        }
        let key = format!("{instance}:{model}");
        let sent = self.call(
            Call::ListTextGeneratorModels,
            ext::test_model_input(&instance, &model),
            Pending::Test(index),
        );
        self.extras.tests[index] = Some(Tested {
            key,
            busy: sent,
            error: (!sent).then(|| "The request could not be sent.".to_owned()),
        });
        cx.notify();
    }

    fn commit_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = ext::health_seconds(&self.settings);
        let typed = self.extras.health.read(cx).value().trim().parse::<f64>();
        let seconds = typed.map_or(current, |value| ext::normalize_seconds(value, 0));
        self.set_health(seconds, window, cx);
    }

    fn set_health(&mut self, seconds: i64, window: &mut Window, cx: &mut Context<Self>) {
        if seconds != ext::health_seconds(&self.settings) {
            self.write_patch(ext::health_interval_patch(&self.settings, seconds), cx);
        }
        self.extras.health_shown = seconds;
        self.extras.health.update(cx, |state, cx| {
            state.set_value(seconds.to_string(), window, cx)
        });
    }

    fn step_health(&mut self, direction: i64, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.extras.health.read(cx).value().trim().parse::<f64>();
        let base = typed.map_or_else(
            |_| ext::health_seconds(&self.settings),
            |value| ext::normalize_seconds(value, 0),
        );
        let seconds = (base + direction * ext::HEALTH_INTERVAL_STEP_SECONDS).max(0);
        self.set_health(seconds, window, cx);
    }

    /// The Usage providers, Text generators and Advanced sections.
    pub(super) fn sections(
        &mut self,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = ext::health_seconds(&self.settings);
        let focused = self
            .extras
            .health
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        if self.extras.health_shown != current && !focused {
            self.extras.health_shown = current;
            self.extras.health.update(cx, |state, cx| {
                state.set_value(current.to_string(), window, cx)
            });
        }
        let usage = self.usage_section(editable, cx);
        let generators =
            (!self.extras.restricted).then(|| self.generators_section(editable, window, cx));
        let advanced = self.advanced_section(editable, window, cx);
        v_flex()
            .gap_8()
            .child(usage)
            .children(generators)
            .child(advanced)
            .into_any_element()
    }

    fn add_button(
        id: &'static str,
        label: &'static str,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        xs_button(id, Variant::Outline, true, cx)
            .child(Icon::new(Icons::Plus).size(px(12.)))
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
            .into_any_element()
    }

    fn usage_section(&mut self, editable: bool, cx: &mut Context<Self>) -> gpui::Div {
        let wide = self.wide;
        let hubs = ext::usage_sources(&self.settings);
        let mut rows: Vec<AnyElement> = Vec::new();
        if hubs.is_empty() {
            rows.push(
                row(
                    "No hubs configured.",
                    "",
                    None,
                    div().into_any_element(),
                    wide,
                    cx,
                )
                .into_any_element(),
            );
        }
        for (index, hub) in hubs.iter().enumerate() {
            let control = if editable {
                let id = hub.id.clone();
                xs_button(("usage-remove", index), Variant::Ghost, true, cx)
                    .child("Remove")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.extras.hub_remove = Some(id.clone());
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                div().into_any_element()
            };
            rows.push(
                row(hub.title(), hub.description(), None, control, wide, cx).into_any_element(),
            );
        }
        let action = editable.then(|| {
            Self::add_button(
                "usage-add",
                "Add hub",
                |this, window, cx| this.open_hub(window, cx),
                cx,
            )
        });
        section_with_action("Usage providers", action, group(rows, cx))
    }

    fn generators_section(
        &mut self,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let wide = self.wide;
        let generators = ext::text_generators(&self.settings);
        let mut rows: Vec<AnyElement> = Vec::new();
        if generators.is_empty() {
            rows.push(
                row(
                    "No text generators yet",
                    "Add a model API to write thread titles, commit messages, and other short text without starting a coding agent.",
                    None,
                    div().into_any_element(),
                    wide,
                    cx,
                )
                .into_any_element(),
            );
        }
        for (index, generator) in generators.iter().enumerate() {
            let name = generator.name();
            let title = h_flex()
                .items_center()
                .gap(px(10.))
                .child(brand_icon(generator.brand(), &name, 16., cx))
                .child(name.clone());
            let description = div().truncate().pl(px(30.)).child(generator.description());
            let control = if editable {
                self.generator_controls(index, generator, window, cx)
            } else {
                div().into_any_element()
            };
            rows.push(row(title, description, None, control, wide, cx).into_any_element());
        }
        let action = editable.then(|| {
            Self::add_button(
                "generator-add",
                "Add generator",
                |this, window, cx| this.open_wizard(None, window, cx),
                cx,
            )
        });
        section_with_action("Text generators", action, group(rows, cx))
    }

    fn generator_controls(
        &mut self,
        index: usize,
        generator: &TextGenerator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let id = generator.id.clone();
        let enabled = generator.enabled();
        let open = self.extras.open == Some(Open::Row(id.clone()));
        let trigger = icon_button(("generator-more", index), Variant::GhostMuted, true, cx)
            .child(Icon::new(Icons::EllipsisVertical).size(px(16.)));
        let menu = Popover::new(("generator-menu", index))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change({
                let (view, id) = (view.clone(), id.clone());
                move |next, _, cx| {
                    view.update(cx, |this, cx| {
                        if *next {
                            this.set_open(Some(Open::Row(id.clone())), cx);
                        } else if this.extras.open == Some(Open::Row(id.clone())) {
                            this.set_open(None, cx);
                        }
                    })
                }
            })
            .trigger(PopTrigger::new(trigger))
            .content({
                let (view, id) = (view.clone(), id.clone());
                move |_, _, cx| {
                    let muted = Tokens::of(cx).muted_foreground;
                    let (edit, models, remove) = (view.clone(), view.clone(), view.clone());
                    let (edit_id, models_id, remove_id) = (id.clone(), id.clone(), id.clone());
                    let show = menu_item(("generator-models", index), "Show models", false, cx);
                    let show = if enabled {
                        show.on_click(move |_, _, cx| {
                            let id = models_id.clone();
                            models.update(cx, |this, cx| {
                                this.extras.open = None;
                                this.ensure_models(&id);
                                this.extras.models_error = None;
                                this.extras.models_for = Some(id);
                                cx.notify();
                            })
                        })
                    } else {
                        show.cursor_default().opacity(0.64).text_color(muted)
                    };
                    popup_shell(128., cx)
                        .child(
                            menu_item(("generator-edit", index), "Edit", false, cx).on_click(
                                move |_, window, cx| {
                                    let id = edit_id.clone();
                                    edit.update(cx, |this, cx| {
                                        this.open_wizard(Some(id), window, cx)
                                    })
                                },
                            ),
                        )
                        .child(show)
                        .child(
                            menu_item(("generator-remove", index), "Remove", true, cx).on_click(
                                move |_, _, cx| {
                                    let id = remove_id.clone();
                                    remove.update(cx, |this, cx| {
                                        this.extras.open = None;
                                        this.extras.gen_remove = Some(id);
                                        cx.notify();
                                    })
                                },
                            ),
                        )
                }
            });
        let toggle: Toggle = {
            let config = generator.config.clone();
            Rc::new(move |_, cx| {
                let patch = ext::text_generator_patch(&id, ext::with_enabled(&config, !enabled));
                view.update(cx, |this, cx| this.write_patch(patch, cx))
            })
        };
        h_flex()
            .items_center()
            .gap_2()
            .child(menu)
            .child(switch(
                ("generator-switch", index),
                enabled,
                toggle,
                window,
                cx,
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn select_popover(
        &mut self,
        open_as: Open,
        id: (&'static str, usize),
        label: impl Into<SharedString>,
        width: f32,
        auto_width: bool,
        popup: Rc<dyn Fn(&App) -> AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let open = self.extras.open.as_ref() == Some(&open_as);
        let handle = tab_stop(&ElementId::from(id), window, cx);
        let on_key: KeyHandler = {
            let (view, open_as) = (view.clone(), open_as.clone());
            Rc::new(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    let open_as = open_as.clone();
                    view.update(cx, |this, cx| this.set_open(Some(open_as), cx));
                }
            })
        };
        let mut trigger = SelectTrigger::new(id, label, width, handle, on_key);
        if auto_width {
            trigger = trigger.auto_width();
        }
        Popover::new(("extras-select", id.1 + if id.0.len() > 4 { 100 } else { 0 }))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change(move |next, _, cx| {
                view.update(cx, |this, cx| {
                    if *next {
                        this.set_open(Some(open_as.clone()), cx);
                    } else if this.extras.open.as_ref() == Some(&open_as) {
                        this.set_open(None, cx);
                    }
                })
            })
            .trigger(trigger)
            .content(move |_, _, cx| popup(cx))
            .into_any_element()
    }

    fn task_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let (key, title) = TASKS[index];
        let theme = Tokens::of(cx);
        let selection = ext::task_selection(&self.settings, key);
        let ids: Vec<String> = ext::text_generators(&self.settings)
            .into_iter()
            .filter(TextGenerator::enabled)
            .map(|generator| generator.id)
            .collect();
        for id in ids {
            self.ensure_models(&id);
        }
        let picker = self.task_picker(index, cx);
        let testable = selection.as_ref().is_some_and(|(instance, _)| {
            ext::text_generators(&self.settings)
                .iter()
                .any(|generator| &generator.id == instance)
        });
        let test_key = selection
            .as_ref()
            .map(|(instance, model)| format!("{instance}:{model}"));
        let shown = self.extras.tests[index]
            .as_ref()
            .filter(|test| Some(&test.key) == test_key.as_ref());
        let busy = shown.is_some_and(|test| test.busy);
        let status = match shown {
            Some(test) if test.busy => None,
            Some(test) => Some(match &test.error {
                Some(error) => problem(error.clone(), cx),
                None => note(
                    format!(
                        "{} answered. It works.",
                        selection.as_ref().map_or("", |(_, model)| model.as_str())
                    ),
                    cx,
                ),
            }),
            None if ext::task_follows(&self.settings, key) => {
                Some(note("Follows the text generation model", cx))
            }
            None => None,
        };
        let test = testable.then(|| {
            let spin = busy.then(|| {
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(spinner(12., theme.muted_foreground))
            });
            xs_button(("task-test", index), Variant::Outline, !busy, cx)
                .child(
                    div()
                        .relative()
                        .child(div().when(busy, |label| label.invisible()).child("Test"))
                        .children(spin),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.test_task(index, cx)))
        });
        h_flex()
            .items_center()
            .gap_2()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .pr_2()
                    .child(div().text_size(px(14.)).child(title))
                    .children(status),
            )
            .child(picker)
            .child(div().w(px(48.)).flex().justify_end().children(test))
            .into_any_element()
    }

    fn advanced_section(
        &mut self,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let wide = self.wide;
        let view = cx.entity();
        let current = ext::health_seconds(&self.settings);
        let tip = info_tip("health-tip", HEALTH_TIP, window, cx);
        let title = h_flex()
            .items_center()
            .gap(px(6.))
            .child("Health check interval")
            .child(tip);
        let reset = (current != ext::health_preset_seconds(&self.settings)).then(|| {
            let view = view.clone();
            reset_button_labeled(
                "health-reset",
                "Reset provider health check interval to default",
                move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        if !editable {
                            return;
                        }
                        let patch = ext::health_reset_patch(&this.settings);
                        this.write_patch(patch, cx);
                        let seconds = ext::health_seconds(&this.settings);
                        this.extras.health_shown = seconds;
                        this.extras.health.update(cx, |state, cx| {
                            state.set_value(seconds.to_string(), window, cx)
                        });
                    })
                },
                window,
                cx,
            )
        });
        let theme = Tokens::of(cx);
        let step = |id: &'static str, icon: Icons, direction: i64, cx: &mut Context<Self>| {
            div()
                .id(id)
                .size(px(26.))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(theme.muted_foreground)
                .hover(move |style| style.bg(theme.accent))
                .child(Icon::new(icon).size(px(16.)))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.step_health(direction, window, cx)),
                )
        };
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let interval = h_flex()
            .items_center()
            .gap_2()
            .when(!editable, |control| control.opacity(0.5))
            .child(
                h_flex()
                    .w(px(128.))
                    .h(px(28.))
                    .items_center()
                    .overflow_hidden()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.input)
                    .bg(background)
                    .text_size(px(14.))
                    .child(step("health-down", Icons::Minus, -1, cx))
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.extras.health)
                                .appearance(false)
                                .bordered(false)
                                .disabled(!editable)
                                .h(px(26.))
                                .px_0(),
                        ),
                    )
                    .child(step("health-up", Icons::Plus, 1, cx)),
            )
            .child(note("seconds", cx));
        let health = row(
            title,
            "Refresh provider status, versions, and models in the background. Set to 0 to disable.",
            reset,
            if editable {
                interval.into_any_element()
            } else {
                div().child(interval).into_any_element()
            },
            wide,
            cx,
        );
        let idle = self.idle_row(editable, window, cx);
        section_with_action(
            "Advanced",
            None,
            group(vec![health.into_any_element(), idle], cx),
        )
    }

    fn idle_row(
        &mut self,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let wide = self.wide;
        let view = cx.entity();
        let state = ext::idle_state(&[&self.settings]);
        let label: SharedString = if state.mixed {
            "Mixed".into()
        } else {
            state.mode.label().into()
        };
        let highlighted = if self.extras.open == Some(Open::Idle) {
            self.extras.highlight
        } else {
            usize::MAX
        };
        let popup: Rc<dyn Fn(&App) -> AnyElement> = {
            let view = view.clone();
            Rc::new(move |cx| {
                let theme = Tokens::of(cx);
                let (accent, accent_foreground, chosen, muted) = (
                    theme.accent,
                    theme.accent_foreground,
                    theme.foreground.opacity(0.08),
                    theme.muted_foreground,
                );
                let mut shell = popup_shell(224., cx);
                for (index, mode) in IdleMode::ALL.into_iter().enumerate() {
                    let allowed = mode != IdleMode::Off || state.never_available;
                    let item = div()
                        .id(("idle-mode", index))
                        .min_h(px(28.))
                        .flex()
                        .items_center()
                        .px_2()
                        .py_1()
                        .rounded(px(4.))
                        .text_sm()
                        .when(!state.mixed && mode == state.mode, |item| item.bg(chosen))
                        .child(mode.label());
                    let item = if allowed {
                        let (pick, hover) = (view.clone(), view.clone());
                        item.cursor_pointer()
                            .when(index == highlighted, |item| {
                                item.bg(accent).text_color(accent_foreground)
                            })
                            .on_hover(move |entered, _, cx| {
                                if *entered {
                                    hover.update(cx, |this, cx| {
                                        if this.extras.highlight != index {
                                            this.extras.highlight = index;
                                            cx.notify();
                                        }
                                    })
                                }
                            })
                            .on_click(move |_, _, cx| {
                                pick.update(cx, |this, cx| {
                                    let patch = ext::idle_patch(&this.settings, mode);
                                    this.write_patch(patch, cx);
                                    this.set_open(None, cx);
                                })
                            })
                    } else {
                        item.cursor_default().opacity(0.64).text_color(muted)
                    };
                    shell = shell.child(item);
                }
                shell.into_any_element()
            })
        };
        let control =
            div()
                .when(!editable, |control| control.opacity(0.5))
                .child(self.select_popover(
                    Open::Idle,
                    ("idle-select", 0),
                    label,
                    224.,
                    false,
                    popup,
                    window,
                    cx,
                ));
        let reset = (ext::global_idle(&self.settings) != IdleMode::OnReturn).then(|| {
            reset_button_labeled(
                "idle-reset",
                "Reset compact idle threads to default",
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        if editable {
                            let patch = ext::idle_patch(&this.settings, IdleMode::OnReturn);
                            this.write_patch(patch, cx);
                        }
                    })
                },
                window,
                cx,
            )
        });
        row(
            "Compact idle threads",
            "When to compact an idle Claude thread over 200,000 tokens: after you return, just before its prompt cache expires, or never. Each Claude provider can override this.",
            reset,
            control.into_any_element(),
            wide,
            cx,
        )
        .into_any_element()
    }

    /// The wizard, models dialog, confirmations and hub dialog, over a backdrop.
    pub(super) fn extras_overlay(
        &mut self,
        label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.extras.wizard.is_some() {
            return Some(self.wizard_view(window, cx));
        }
        if let Some(id) = self.extras.models_for.clone() {
            return Some(self.models_view(&id, window, cx));
        }
        if let Some(id) = self.extras.gen_remove.clone() {
            let name = ext::text_generators(&self.settings)
                .into_iter()
                .find(|generator| generator.id == id)
                .map(|generator| generator.name())
                .unwrap_or_default();
            let description = format!(
                "This deletes the API key stored on {label}. Anything set to use this generator falls back to the text generation model."
            );
            let go = move |this: &mut Self, cx: &mut Context<Self>| {
                this.extras.gen_remove = None;
                this.write_patch(ext::remove_text_generator_patch(&id), cx);
            };
            return Some(self.confirm_dialog(
                "generator-remove",
                format!("Remove {name}?"),
                description,
                "Remove",
                |this| this.extras.gen_remove = None,
                go,
                cx,
            ));
        }
        if self.extras.hub_open {
            return Some(self.hub_view(label, window, cx));
        }
        if let Some(id) = self.extras.hub_remove.clone() {
            let title = ext::usage_sources(&self.settings)
                .into_iter()
                .find(|source| source.id == id)
                .map(|source| source.title())
                .unwrap_or_default();
            let go = move |this: &mut Self, cx: &mut Context<Self>| {
                this.extras.hub_remove = None;
                this.write_patch(ext::remove_usage_source_patch(&id), cx);
            };
            return Some(self.confirm_dialog(
                "usage-remove-dialog",
                format!("Remove {title}?"),
                "The hub's management key is deleted from this server. Its accounts leave the Limits view; the hub itself is untouched. Add it again with the URL and key to bring them back."
                    .to_owned(),
                "Remove hub",
                |this| this.extras.hub_remove = None,
                go,
                cx,
            ));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn confirm_dialog(
        &self,
        id: &'static str,
        title: String,
        description: String,
        action: &'static str,
        cancel: impl Fn(&mut Self) + Clone + 'static,
        go: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let (cancel_view, go_view, dismiss_view) = (view.clone(), view, cx.entity());
        let (cancel_click, cancel_dismiss) = (cancel.clone(), cancel);
        let footer = vec![
            button((id, 1usize), Variant::Outline, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel_view.update(cx, |this, cx| {
                        cancel_click(this);
                        cx.notify();
                    })
                })
                .into_any_element(),
            destructive_button((id, 2usize), cx)
                .child(action)
                .on_click(move |_, _, cx| go_view.update(cx, |this, cx| go(this, cx)))
                .into_any_element(),
        ];
        dialog_of(
            id,
            DIALOG_WIDTH,
            title,
            description,
            Vec::new(),
            footer,
            move |_, cx| {
                dismiss_view.update(cx, |this, cx| {
                    cancel_dismiss(this);
                    cx.notify();
                })
            },
            cx,
        )
    }

    fn hub_view(&mut self, label: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let can_add = self.can_add_hub(cx);
        let (cancel, add, dismiss) = (view.clone(), view.clone(), view);
        let body = vec![
            v_flex()
                .gap_4()
                .child(labeled(
                    "Hub URL",
                    field(&self.extras.hub_url, false, window, cx),
                ))
                .child(labeled("Management key", self.extras.hub_key.clone()))
                .child(labeled(
                    "Label (optional)",
                    field(&self.extras.hub_label, false, window, cx),
                ))
                .into_any_element(),
        ];
        let footer = vec![
            button("hub-cancel", Variant::Outline, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.extras.hub_open = false;
                        cx.notify();
                    })
                })
                .into_any_element(),
            button("hub-add", Variant::Primary, can_add, cx)
                .child("Add hub")
                .on_click(move |_, _, cx| add.update(cx, |this, cx| this.submit_hub(cx)))
                .into_any_element(),
        ];
        dialog_of(
            "hub-dialog",
            HUB_WIDTH,
            "Add a CLIProxyAPI hub",
            format!(
                "Show the quota of every account the hub pools, next to the providers on {label}. The key stays on that server."
            ),
            body,
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.extras.hub_open = false;
                    cx.notify();
                })
            },
            cx,
        )
    }

    fn models_view(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let name = ext::text_generators(&self.settings)
            .into_iter()
            .find(|generator| generator.id == id)
            .map(|generator| generator.name())
            .unwrap_or_default();
        let refreshing = self.extras.models_refreshing;
        let theme = Tokens::of(cx);
        let (description, body): (String, AnyElement) = match self.extras.models.get(id) {
            Some(Models::Loaded(models)) => {
                let count = models.len();
                (
                    format!(
                        "{count} {} available. Pick one in a text generation model picker.",
                        if count == 1 { "model" } else { "models" }
                    ),
                    self.model_list(models, window, cx),
                )
            }
            Some(Models::Failed(error)) => (
                "The model list couldn't load.".to_owned(),
                problem(error.clone(), cx)
                    .text_size(px(14.))
                    .into_any_element(),
            ),
            _ => (
                "Loading the model list.".to_owned(),
                spinner(16., theme.muted_foreground),
            ),
        };
        let refresh_error = self
            .extras
            .models_error
            .clone()
            .map(|error| problem(error, cx).into_any_element());
        let (refresh, done, dismiss) = (view.clone(), view.clone(), view);
        let refresh_id = id.to_owned();
        let footer = vec![
            button("models-refresh", Variant::Outline, !refreshing, cx)
                .gap(px(6.))
                .child(Icon::new(Icons::RotateCcw).size(px(16.)))
                .child("Refresh")
                .on_click(move |_, _, cx| {
                    let id = refresh_id.clone();
                    refresh.update(cx, |this, cx| this.refresh_models(&id, cx))
                })
                .into_any_element(),
            button("models-done", Variant::Primary, true, cx)
                .child("Done")
                .on_click(move |_, _, cx| {
                    done.update(cx, |this, cx| {
                        this.extras.models_for = None;
                        cx.notify();
                    })
                })
                .into_any_element(),
        ];
        let mut parts = vec![body];
        parts.extend(refresh_error);
        dialog_of(
            "models-dialog",
            448.,
            format!("{name} models"),
            description,
            vec![v_flex().gap_2().children(parts).into_any_element()],
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.extras.models_for = None;
                    cx.notify();
                })
            },
            cx,
        )
    }

    fn model_list(
        &self,
        models: &[Model],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let query = self
            .extras
            .models_search
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let shown: Vec<&Model> = models
            .iter()
            .filter(|model| {
                query.is_empty()
                    || model.id.to_lowercase().contains(&query)
                    || model.name.to_lowercase().contains(&query)
            })
            .collect();
        let search = (models.len() > 8)
            .then(|| field(&self.extras.models_search, false, window, cx).into_any_element());
        let items: Vec<AnyElement> = if shown.is_empty() {
            vec![
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("No models match.")
                    .into_any_element(),
            ]
        } else {
            shown
                .iter()
                .enumerate()
                .map(|(index, model)| {
                    v_flex()
                        .when(index > 0, |item| {
                            item.border_t_1().border_color(theme.border.opacity(0.6))
                        })
                        .px_3()
                        .py_2()
                        .text_sm()
                        .child(div().truncate().child(model.name.clone()))
                        .when(model.name != model.id, |item| {
                            item.child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(theme.muted_foreground)
                                    .child(model.id.clone()),
                            )
                        })
                        .into_any_element()
                })
                .collect()
        };
        v_flex()
            .gap_2()
            .children(search)
            .child(
                div()
                    .id("models-list")
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.border)
                    .child(v_flex().children(items)),
            )
            .into_any_element()
    }

    fn wizard_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let theme = Tokens::of(cx);
        let Some(wizard) = self.extras.wizard.as_ref() else {
            return div().into_any_element();
        };
        let (step, preset, checking, adding) = (
            wizard.step,
            wizard.preset,
            wizard.checking,
            wizard.existing.is_none(),
        );
        let error = wizard.error.clone();
        let count = wizard.model_count;
        let shows_url = Self::shows_url(wizard);
        let key_required = Self::key_required(wizard);
        let title = match &wizard.existing {
            Some(generator) => format!("Edit {}", generator.name()),
            None => "Add a text generator".to_owned(),
        };
        let connected_name = self
            .wizard_config(cx)
            .map(|config| ext::display_name(&config))
            .unwrap_or_else(|| PRESETS[preset].label.to_owned());
        let on_step: StepHandler = {
            let view = view.clone();
            Rc::new(move |index, _, cx| {
                view.update(cx, |this, cx| {
                    if let Some(wizard) = &mut this.extras.wizard
                        && index < wizard.step
                    {
                        wizard.step = index;
                        cx.notify();
                    }
                })
            })
        };
        let steps = wizard_steps("generator-steps", &["Connect", "Use it"], step, on_step, cx);
        let header = wizard_header(title, Some(DESCRIPTION), steps, cx);
        let body: AnyElement = if step == 0 {
            let mut form = v_flex().gap_4();
            if adding {
                let cards = (0..PRESETS.len()).map(|index| {
                    let selected = index == preset;
                    let custom = PRESETS[index].id == "custom";
                    let accent = theme.accent;
                    let icon = if custom {
                        Icon::new(Icons::Server).size(px(20.)).into_any_element()
                    } else {
                        brand_icon(preset_brand(index), PRESETS[index].label, 20., cx)
                    };
                    div()
                        .id(("generator-preset", index))
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_3()
                        .rounded(px(8.))
                        .border_1()
                        .text_sm()
                        .when(checking, |card| card.opacity(0.64))
                        .when(!checking, |card| card.cursor_pointer())
                        .when(selected, |card| {
                            card.border_color(theme.primary)
                                .bg(theme.primary.opacity(0.05))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.foreground)
                        })
                        .when(!selected, |card| {
                            let card = card
                                .border_color(theme.border)
                                .bg(theme.background)
                                .text_color(theme.muted_foreground);
                            if checking {
                                card
                            } else {
                                card.hover(move |style| style.bg(accent.opacity(0.5)))
                            }
                        })
                        .child(icon)
                        .child(if custom {
                            "Custom"
                        } else {
                            PRESETS[index].label
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.pick_preset(index, window, cx)
                        }))
                });
                form = form.child(
                    v_flex()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Service"),
                        )
                        .child(h_flex().gap_2().children(cards)),
                );
            }
            if shows_url {
                form = form.child(labeled(
                    "Base URL",
                    field(&self.extras.wiz_url, checking, window, cx),
                ));
            }
            form = form
                .child(labeled(
                    if key_required {
                        "API key"
                    } else {
                        "API key (optional)"
                    },
                    self.extras.wiz_key.clone(),
                ))
                .child(labeled(
                    "Name (optional)",
                    field(&self.extras.wiz_name, checking, window, cx),
                ))
                .children(error.map(|error| problem(error, cx).text_size(px(14.))));
            form.into_any_element()
        } else {
            let tasks: Vec<AnyElement> = (0..TASKS.len())
                .map(|index| self.task_row(index, cx))
                .collect();
            v_flex()
                .gap_4()
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(theme.muted_foreground)
                        .child(format!(
                            "{connected_name} is connected and lists {count} {}. Switch any of these to it, or keep what they use now.",
                            if count == 1 { "model" } else { "models" }
                        )),
                )
                .child(v_flex().gap_4().children(tasks))
                .into_any_element()
        };
        let panel = wizard_panel(body, cx);
        let can_connect = !checking && self.wizard_config(cx).is_some();
        let existing = !adding;
        let footer = if step == 0 {
            let (cancel, connect) = (view.clone(), view.clone());
            let label = if checking {
                "Connecting"
            } else if existing {
                "Save and continue"
            } else {
                "Connect"
            };
            wizard_footer(
                None,
                vec![
                    button("generator-cancel", Variant::Outline, true, cx)
                        .child("Cancel")
                        .on_click(move |_, _, cx| {
                            cancel.update(cx, |this, cx| {
                                this.extras.wizard = None;
                                cx.notify();
                            })
                        })
                        .into_any_element(),
                    button("generator-connect", Variant::Primary, can_connect, cx)
                        .gap(px(6.))
                        .children(checking.then(|| spinner(14., gpui::white())))
                        .child(label)
                        .on_click(move |_, _, cx| connect.update(cx, |this, cx| this.connect(cx)))
                        .into_any_element(),
                ],
            )
        } else {
            let (back, done) = (view.clone(), view.clone());
            wizard_footer(
                Some(
                    button("generator-back", Variant::Ghost, true, cx)
                        .child("Back")
                        .on_click(move |_, _, cx| {
                            back.update(cx, |this, cx| {
                                if let Some(wizard) = &mut this.extras.wizard {
                                    wizard.step = 0;
                                }
                                cx.notify();
                            })
                        })
                        .into_any_element(),
                ),
                vec![
                    button("generator-done", Variant::Primary, true, cx)
                        .child("Done")
                        .on_click(move |_, _, cx| {
                            done.update(cx, |this, cx| {
                                this.extras.wizard = None;
                                this.extras.open = None;
                                cx.notify();
                            })
                        })
                        .into_any_element(),
                ],
            )
        };
        let dismiss: Toggle = Rc::new(move |_, cx| {
            view.update(cx, |this, cx| {
                this.extras.wizard = None;
                this.extras.open = None;
                cx.notify();
            })
        });
        wizard_popup("generator-wizard", [header, panel, footer], dismiss, cx)
    }
}
