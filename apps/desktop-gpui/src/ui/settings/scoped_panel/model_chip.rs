//! The model rows of the Threads tab: `ProviderModelPicker` and `TraitsPicker` behind the compact
//! `SETTINGS_PICKER_TRIGGER_CLASSNAME` pills (ProjectDefaultsSettings.tsx, SettingsPanels.tsx
//! "Text generation model"). The model pill opens the composer's model popover; the effort pill
//! lists each option of the model as radio rows.

use super::super::components::popup_shell;
use super::*;
use crate::model_picker::{self, Focus, Origin, State, Tab};
use crate::providers::{
    Catalog, Control, Controls, Mark, Pick as Choice, PickerInstance, Traits, Trigger, with_option,
};
use crate::ui::model_popover::{Act, Host, Panel};
use gpui::{Anchor, Keystroke};
use gpui_component::popover::Popover;

/// The three rows that pick a model.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    Default,
    Fallback,
    Text,
}

impl Slot {
    fn field(self) -> Field {
        match self {
            Self::Default => Field::DefaultModel,
            Self::Fallback => Field::FallbackModel,
            Self::Text => Field::TextModel,
        }
    }

    fn id(self) -> usize {
        match self {
            Self::Default => 0,
            Self::Fallback => 1,
            Self::Text => 2,
        }
    }
}

/// The pickers' data and open state.
pub(super) struct Chips {
    catalog: Option<Catalog>,
    text: Vec<PickerInstance>,
    open: Option<Slot>,
    traits_open: Option<Slot>,
    pick: State,
    search: Entity<InputState>,
    keys: Option<Subscription>,
    /// The stars as the user left them, until the catalog says the same.
    favorites: Option<Vec<(String, String)>>,
    stars: HashMap<u64, Vec<(String, String)>>,
}

impl Chips {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<ScopedPanel>,
    ) -> (Self, Entity<InputState>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search models..."));
        (
            Self {
                catalog: None,
                text: Vec::new(),
                open: None,
                traits_open: None,
                pick: State::default(),
                search: search.clone(),
                keys: None,
                favorites: None,
                stars: HashMap::new(),
            },
            search,
        )
    }

    /// Closes the pickers; returns whether one was open.
    pub(super) fn close(&mut self) -> bool {
        let was = self.open.is_some() || self.traits_open.is_some();
        self.open = None;
        self.traits_open = None;
        self.keys = None;
        was
    }

    /// Whether `token` was a star; a failed star puts the previous list back.
    pub(super) fn settled(&mut self, token: u64, failed: bool) -> bool {
        let Some(before) = self.stars.remove(&token) else {
            return false;
        };
        if failed {
            self.favorites = Some(before);
        }
        true
    }
}

/// What a pill shows for the chosen model.
struct Active {
    mark: Mark,
    label: SharedString,
    instance: String,
    slug: String,
    traits: Traits,
    options: Vec<serde_json::Value>,
    /// No selection is saved: the row shows what a new thread resolves to.
    unset: bool,
}

fn project(ctx: &Ctx) -> Option<String> {
    ctx.in_project
        .then(|| ctx.members.first().cloned())
        .flatten()
}

impl ScopedPanel {
    pub fn set_catalog(&mut self, catalog: Option<Catalog>, cx: &mut Context<Self>) {
        if self.chips.catalog == catalog {
            return;
        }
        if self.chips.stars.is_empty()
            && let (Some(local), Some(catalog)) = (&self.chips.favorites, &catalog)
            && *local == catalog.favorites()
        {
            self.chips.favorites = None;
        }
        self.chips.catalog = catalog;
        cx.notify();
    }

    pub fn set_text_targets(&mut self, targets: Vec<PickerInstance>, cx: &mut Context<Self>) {
        if self.chips.text != targets {
            self.chips.text = targets;
            cx.notify();
        }
    }

    fn favorites(&self) -> Vec<(String, String)> {
        self.chips.favorites.clone().unwrap_or_else(|| {
            self.chips
                .catalog
                .as_ref()
                .map(Catalog::favorites)
                .unwrap_or_default()
        })
    }

    fn chip_instances(&self, slot: Slot, ctx: &Ctx) -> Vec<PickerInstance> {
        let mut instances = match (slot, &self.chips.catalog) {
            (Slot::Text, _) => self.chips.text.clone(),
            (_, Some(catalog)) => catalog.picker(project(ctx).as_deref()),
            (_, None) => Vec::new(),
        };
        let favorites = self.favorites();
        for instance in &mut instances {
            for model in &mut instance.models {
                model.favorite = favorites
                    .iter()
                    .any(|(i, m)| *i == instance.instance_id && *m == model.slug);
            }
        }
        instances
    }

    fn chip_selection(&self, slot: Slot, ctx: &Ctx) -> Option<Choice> {
        let value = self.value(slot.field(), ctx);
        let instance = value.get("instanceId").or_else(|| value.get("provider"))?;
        Some(Choice {
            instance_id: instance.as_str()?.to_owned(),
            model: value.get("model")?.as_str()?.to_owned(),
            options: value
                .get("options")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
        })
    }

    fn chip_active(&self, slot: Slot, ctx: &Ctx, instances: &[PickerInstance]) -> Option<Active> {
        let picked = self.chip_selection(slot, ctx);
        let project = project(ctx);
        let catalog = self.chips.catalog.as_ref();
        if slot == Slot::Text {
            let picked = picked?;
            let found = instances
                .iter()
                .find(|instance| instance.instance_id == picked.instance_id);
            let (traits, options) = catalog
                .and_then(|catalog| catalog.trigger_for(None, Some(&picked)))
                .filter(|trigger| trigger.instance_id == picked.instance_id)
                .map(|trigger| (trigger.controls.traits, trigger.options))
                .unwrap_or_default();
            return Some(Active {
                mark: found.map_or_else(
                    || Mark {
                        driver: String::new(),
                        display_name: picked.instance_id.clone(),
                        badge: None,
                    },
                    |instance| instance.mark.clone(),
                ),
                label: found
                    .and_then(|instance| {
                        instance
                            .models
                            .iter()
                            .find(|model| model.slug == picked.model)
                    })
                    .map_or(picked.model.as_str(), |model| model.label.as_str())
                    .to_owned()
                    .into(),
                instance: picked.instance_id,
                slug: picked.model,
                traits,
                options,
                unset: false,
            });
        }
        let trigger = catalog?.trigger_for(project.as_deref(), picked.as_ref())?;
        let unset = picked.is_none();
        Some(Active {
            label: if unset && slot == Slot::Fallback {
                "None".into()
            } else {
                trigger.model.into()
            },
            mark: trigger.mark,
            instance: trigger.instance_id,
            slug: trigger.slug,
            traits: trigger.controls.traits,
            options: trigger.options,
            unset,
        })
    }

    fn chip_tab(&self, slot: Slot, ctx: &Ctx, instances: &[PickerInstance]) -> Tab {
        self.chips.pick.tab.clone().unwrap_or_else(|| {
            Tab::Instance(
                self.chip_active(slot, ctx, instances)
                    .map(|active| active.instance)
                    .unwrap_or_default(),
            )
        })
    }

    fn set_chips(&mut self, open: Option<Slot>, traits: Option<Slot>, cx: &mut Context<Self>) {
        if open.is_some() || traits.is_some() {
            self.menu = None;
        }
        if open.is_none() {
            self.chips.keys = None;
        }
        self.chips.open = open;
        self.chips.traits_open = traits;
        cx.notify();
    }

    /// Opens on the active model's rail entry, or on Favorites (`initialModelPickerProvider`).
    fn open_chip(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        let instances = self.chip_instances(slot, &ctx);
        let (instance, model) = self
            .chip_active(slot, &ctx, &instances)
            .map(|active| (active.instance, active.slug))
            .unwrap_or_default();
        let visible = instances
            .iter()
            .any(|i| i.instance_id == instance && i.unavailable.is_none());
        let origin = self.chips.pick.origin;
        let has_favorites = !self.favorites().is_empty();
        let tab = model_picker::initial_tab(&instance, visible, has_favorites, origin);
        let expanded = model_picker::starts_expanded(&instances, &instance, &model);
        let highlight = model_picker::listing(&instances, &tab, "", expanded)
            .rows
            .iter()
            .position(|row| row.instance.instance_id == instance && row.model.slug == model)
            .unwrap_or(0);
        self.chips.pick = State {
            tab: Some(tab),
            expanded,
            highlight,
            origin,
            ..State::default()
        };
        self.set_chips(Some(slot), None, cx);
        let view = cx.weak_entity();
        self.chips.keys = Some(cx.intercept_keystrokes(move |event, window, cx| {
            let handled = view
                .update(cx, |this, cx| this.chip_key(&event.keystroke, window, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        }));
        let search = self.chips.search.clone();
        search.update(cx, |state, cx| state.set_value("", window, cx));
        window.focus(&search.focus_handle(cx), cx);
    }

    fn chip_switch_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.chips
            .search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.chips.pick.tab = Some(tab);
        self.chips.pick.expanded = false;
        self.chips.pick.highlight = 0;
        self.chips.pick.focus = Focus::List;
        cx.notify();
    }

    fn chip_older(&mut self, slot: Slot, cx: &mut Context<Self>) {
        self.chips.pick.expanded = !self.chips.pick.expanded;
        let ctx = self.ctx(cx);
        let instances = self.chip_instances(slot, &ctx);
        let tab = self.chip_tab(slot, &ctx, &instances);
        let query = self.chips.search.read(cx).value();
        self.chips.pick.highlight =
            model_picker::listing(&instances, &tab, &query, self.chips.pick.expanded)
                .rows
                .len();
        cx.notify();
    }

    /// Stars or unstars a model: the local list now, the server's `setModelFavorites` behind it.
    fn chip_favorite(&mut self, instance: &str, slug: &str, cx: &mut Context<Self>) {
        let key = (instance.to_owned(), slug.to_owned());
        let before = self.favorites();
        let favorite = !before.contains(&key);
        let mut next = before.clone();
        if favorite {
            next.push(key);
        } else {
            next.retain(|k| *k != key);
        }
        let token = self.next_token;
        self.next_token += 1;
        let sent = self
            .actions
            .try_send(Action::Call {
                token,
                call: Call::UpdateSettings,
                payload: crate::send::favorite_payload(instance, slug, favorite),
            })
            .is_ok();
        if sent {
            self.chips.stars.insert(token, before);
            self.chips.favorites = Some(next);
        }
        cx.notify();
    }

    fn chip_pick(&mut self, slot: Slot, instance: &str, slug: &str, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        self.chips.pick.origin = Some(if self.chips.pick.tab == Some(Tab::Favorites) {
            Origin::Favorites
        } else {
            Origin::Instance
        });
        let same = self
            .chip_selection(slot, &ctx)
            .is_some_and(|current| current.instance_id == instance && current.model == slug);
        if !same {
            let value = json!({"instanceId": instance, "model": slug});
            self.write(vec![(slot.field(), Some(value))], cx);
        }
        self.set_chips(None, None, cx);
    }

    /// An effort pill row: the model's options with `id` set to `value`.
    fn chip_option(
        &mut self,
        slot: Slot,
        id: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let ctx = self.ctx(cx);
        let instances = self.chip_instances(slot, &ctx);
        if let Some(active) = self.chip_active(slot, &ctx, &instances) {
            let options = with_option(&active.options, id, value);
            let selection = json!({
                "instanceId": active.instance,
                "model": active.slug,
                "options": options,
            });
            self.write(vec![(slot.field(), Some(selection))], cx);
        }
        self.set_chips(None, None, cx);
    }

    /// Cmd+1..9 jump, arrows move within the focused column, Left and Right cross columns at the
    /// search's edges. Returns whether the key was taken.
    fn chip_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(slot) = self.chips.open else {
            return false;
        };
        let ctx = self.ctx(cx);
        let instances = self.chip_instances(slot, &ctx);
        let tab = self.chip_tab(slot, &ctx, &instances);
        let search = self.chips.search.read(cx);
        let (query, cursor) = (search.value(), search.cursor());
        let listing = model_picker::listing(&instances, &tab, &query, self.chips.pick.expanded);
        let key = keystroke.key.as_str();
        let modifiers = keystroke.modifiers;
        if modifiers.platform && !modifiers.alt && !modifiers.control {
            let Some(number) = key.parse::<usize>().ok().filter(|n| (1..=9).contains(n)) else {
                return false;
            };
            if let Some(&at) = listing.jump_rows().get(number - 1) {
                let row = listing.rows[at];
                let (instance, model) = (row.instance.instance_id.clone(), row.model.slug.clone());
                self.chip_pick(slot, &instance, &model, cx);
            }
            return true;
        }
        if modifiers.platform || modifiers.control || modifiers.alt || modifiers.shift {
            return false;
        }
        let delta = match key {
            "down" => 1,
            "up" => -1,
            _ => 0,
        };
        match (self.chips.pick.focus, key) {
            (Focus::List, "down" | "up") => {
                self.chips.pick.highlight =
                    model_picker::step(self.chips.pick.highlight, listing.items(), delta);
            }
            (Focus::List, "enter") => {
                let at = self.chips.pick.highlight;
                if let Some(row) = listing.rows.get(at) {
                    if row.usable() {
                        let (instance, model) =
                            (row.instance.instance_id.clone(), row.model.slug.clone());
                        self.chip_pick(slot, &instance, &model, cx);
                    }
                } else if listing.older.is_some() {
                    self.chip_older(slot, cx);
                }
            }
            (Focus::List, "left") if cursor == 0 => self.chips.pick.focus = Focus::Sidebar,
            (Focus::Sidebar, "down" | "up") => {
                let next = model_picker::adjacent_tab(&instances, &tab, delta);
                self.chip_switch_tab(next, window, cx);
                self.chips.pick.focus = Focus::Sidebar;
            }
            (Focus::Sidebar, "right" | "enter") => self.chips.pick.focus = Focus::List,
            _ => return false,
        }
        cx.notify();
        true
    }

    /// The model pill and, when the model has options, the effort pill beside it.
    pub(super) fn model_chips(
        &mut self,
        slot: Slot,
        ctx: &Ctx,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let instances = self.chip_instances(slot, ctx);
        let theme = Tokens::of(cx);
        let muted = |text: &'static str| {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(text)
                .into_any_element()
        };
        if slot == Slot::Text {
            if !ctx.connected {
                return muted("Connect an environment to choose its text generation model.");
            }
            if instances.is_empty() {
                return muted("No text generation providers available.");
            }
        }
        let Some(active) = self.chip_active(slot, ctx, &instances) else {
            return muted("No providers available");
        };
        let view = cx.entity();
        let open = self.chips.open == Some(slot);
        let search = self.chips.search.clone();
        let trigger = Pill {
            id: ("settings-model-pill", slot.id()),
            mark: Some(active.mark.clone()),
            label: active.label.clone(),
            chevron: true,
            open,
        };
        let panel = open.then(|| {
            let host: Host = {
                let view = view.clone();
                Rc::new(move |act, window, cx| {
                    view.update(cx, |this, cx| match act {
                        Act::Tab(tab) => this.chip_switch_tab(tab, window, cx),
                        Act::Older => this.chip_older(slot, cx),
                        Act::Favorite(id, model) => this.chip_favorite(&id, &model, cx),
                        Act::Pick(id, model) => this.chip_pick(slot, &id, &model, cx),
                        Act::Option(..) => {}
                    });
                })
            };
            Panel {
                host,
                search: search.clone(),
                tab: self.chip_tab(slot, ctx, &instances),
                expanded: self.chips.pick.expanded,
                highlight: self.chips.pick.highlight,
                focus: self.chips.pick.focus,
                effort: 0,
                footer: None,
                trigger: Trigger {
                    instance_id: active.instance.clone(),
                    slug: active.slug.clone(),
                    mark: active.mark.clone(),
                    model: active.label.to_string(),
                    effort: None,
                    fast: false,
                    controls: Controls::default(),
                    options: Vec::new(),
                    title: active.label.to_string(),
                },
                instances: instances.clone(),
            }
        });
        let popup = theme.popover;
        let picker = Popover::new(("settings-model-picker", slot.id()))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .track_focus(&search.focus_handle(cx))
            .p_0()
            .bg(popup)
            .border_1()
            .border_color(crate::ui::dropdown_glass_edge(cx))
            .on_open_change({
                let view = view.clone();
                move |next, window, cx| {
                    view.update(cx, |this, cx| {
                        if *next {
                            this.open_chip(slot, window, cx);
                        } else if this.chips.open == Some(slot) {
                            this.set_chips(None, None, cx);
                        }
                    })
                }
            })
            .trigger(trigger)
            .content(move |_, window, cx| match &panel {
                Some(panel) => panel.render(window, cx),
                None => div().into_any_element(),
            });
        let shows_traits =
            !active.traits.groups.is_empty() && !(slot == Slot::Fallback && active.unset);
        let traits = shows_traits.then(|| {
            let open = self.chips.traits_open == Some(slot);
            let groups = active.traits.groups.clone();
            let menu_view = view.clone();
            Popover::new(("settings-traits-picker", slot.id()))
                .anchor(Anchor::TopLeft)
                .offset(px(4.))
                .open(open)
                .p_0()
                .bg(popup)
                .border_0()
                .on_open_change({
                    let view = view.clone();
                    move |next, _, cx| {
                        view.update(cx, |this, cx| {
                            let next = next.then_some(slot);
                            if next.is_some() || this.chips.traits_open == Some(slot) {
                                this.set_chips(None, next, cx);
                            }
                        })
                    }
                })
                .trigger(Pill {
                    id: ("settings-traits-pill", slot.id()),
                    mark: None,
                    label: active.traits.label.clone().into(),
                    chevron: false,
                    open,
                })
                .content(move |_, _, cx| traits_menu(&menu_view, slot, &groups, cx))
        });
        h_flex()
            .min_w_0()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .child(picker)
            .children(traits)
            .into_any_element()
    }
}

/// `TraitsMenuContent`: a header per option, then its choices (a flag as On and Off).
fn traits_menu(view: &Entity<ScopedPanel>, slot: Slot, groups: &[Control], cx: &App) -> AnyElement {
    let theme = Tokens::of(cx);
    let muted = theme.muted_foreground;
    let mut shell = popup_shell(160., cx);
    for (group_at, group) in groups.iter().enumerate() {
        if group_at > 0 {
            shell = shell.child(
                div()
                    .h(px(1.))
                    .mx(px(-4.))
                    .my(px(4.))
                    .bg(theme.foreground.opacity(0.1)),
            );
        }
        shell = shell.child(
            div()
                .px_2()
                .pt(px(6.))
                .pb_1()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted)
                .child(group.label.clone()),
        );
        for (at, level) in group.levels.iter().enumerate() {
            let (view, id, value) = (view.clone(), group.id.clone(), level.value.clone());
            let accent = theme.accent;
            shell = shell.child(
                v_flex()
                    .id(("settings-traits-item", group_at * 64 + at))
                    .min_h(px(28.))
                    .justify_center()
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .text_sm()
                    .cursor_pointer()
                    .when(level.selected, |item| {
                        item.bg(theme.foreground.opacity(0.08))
                    })
                    .hover(move |item| item.bg(accent))
                    .on_click(move |_, _, cx| {
                        let (id, value) = (id.clone(), value.clone());
                        view.update(cx, |this, cx| this.chip_option(slot, &id, value, cx))
                    })
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(div().min_w_0().truncate().child(level.label.clone()))
                            .when(level.is_default, |row| {
                                row.child(
                                    div()
                                        .px_1()
                                        .rounded(px(4.))
                                        .border_1()
                                        .border_color(theme.input)
                                        .text_xs()
                                        .text_color(theme.foreground)
                                        .child("Default"),
                                )
                            }),
                    )
                    .children(level.description.clone().map(|text| {
                        div()
                            .max_w(px(224.))
                            .text_xs()
                            .text_color(muted.opacity(0.8))
                            .child(text)
                    })),
            );
        }
    }
    shell.into_any_element()
}

/// `SETTINGS_PICKER_TRIGGER_CLASSNAME`: an outlined pill, with the provider mark and a chevron on
/// the model's, or the gauge on the effort's.
#[derive(IntoElement)]
struct Pill {
    id: (&'static str, usize),
    mark: Option<Mark>,
    label: SharedString,
    chevron: bool,
    open: bool,
}

impl Selectable for Pill {
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

impl RenderOnce for Pill {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = Tokens::of(cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let hover = theme.accent.opacity(0.5);
        h_flex()
            .id(self.id)
            .h(px(28.))
            .px_2()
            .gap(px(6.))
            .flex_shrink_0()
            .items_center()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(background)
            .text_sm()
            .text_color(theme.foreground)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(self.open, move |chip| chip.bg(hover))
            .child(match &self.mark {
                Some(mark) => crate::ui::provider_mark(mark, 1., cx).into_any_element(),
                None => Icon::new(Icons::Gauge)
                    .size(px(14.))
                    .text_color(theme.muted_foreground)
                    .into_any_element(),
            })
            .child(
                div()
                    .max_w(px(224.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.label),
            )
            .when(self.chevron, |chip| {
                chip.child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(14.))
                        .opacity(0.7)
                        .text_color(theme.muted_foreground),
                )
            })
    }
}
