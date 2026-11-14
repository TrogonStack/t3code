//! The task rows' model picker: the composer's `ProviderModelPicker` popover, listing the agent
//! providers that can write text plus the enabled text generators' models
//! (`useTextGenerationTargets`), behind the compact `SETTINGS_PICKER_TRIGGER_CLASSNAME` trigger.

use super::*;
use crate::model_picker::{self, Focus, Origin, State, Tab};
use crate::providers::{Controls, Mark, PickerInstance, PickerModel, Trigger};
use crate::ui::model_popover::{Act, Host, Panel};
use gpui::Keystroke;

/// What the trigger and the popover show for one task.
pub(super) struct Chosen {
    pub mark: Mark,
    pub label: SharedString,
    pub instance: String,
    pub slug: String,
}

/// The model of `(instance, slug)` as the trigger shows it: the picker's own entry, else the slug.
pub(super) fn chosen(instances: &[PickerInstance], instance: &str, slug: &str) -> Chosen {
    let found = instances.iter().find(|i| i.instance_id == instance);
    Chosen {
        mark: found.map_or_else(
            || Mark {
                driver: String::new(),
                display_name: instance.to_owned(),
                badge: None,
            },
            |i| i.mark.clone(),
        ),
        label: found
            .and_then(|i| i.models.iter().find(|m| m.slug == slug))
            .map_or(slug, |m| m.label.as_str())
            .to_owned()
            .into(),
        instance: instance.to_owned(),
        slug: slug.to_owned(),
    }
}

impl Extras {
    pub(in crate::ui::settings::providers_panel) fn set_agents(
        &mut self,
        (agents, favorites): (Vec<PickerInstance>, Vec<(String, String)>),
        cx: &mut Context<ProvidersPanel>,
    ) {
        let starring = self
            .calls
            .values()
            .any(|pending| matches!(pending, Pending::Favorite { .. }));
        let mut changed = self.agents != agents;
        self.agents = agents;
        if !starring && self.favorites != favorites {
            self.favorites = favorites;
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }
}

impl ProvidersPanel {
    /// The agents that write text, then the enabled generators with the models they have loaded.
    /// A generator replaces a provider instance with its id, as the server does.
    pub(super) fn task_instances(&self) -> Vec<PickerInstance> {
        let all = ext::text_generators(&self.settings);
        let mut instances: Vec<PickerInstance> = self
            .extras
            .agents
            .iter()
            .filter(|agent| all.iter().all(|g| g.id != agent.instance_id))
            .cloned()
            .collect();
        for generator in all.iter().filter(|g| g.enabled()) {
            let (loaded, unavailable) = match self.extras.models.get(&generator.id) {
                Some(Models::Loaded(models)) => (models.as_slice(), None),
                Some(Models::Failed(message)) => (&[][..], Some(message.clone())),
                _ => (&[][..], None),
            };
            let mut models: Vec<PickerModel> = loaded
                .iter()
                .map(|model| generator_model(&model.id, &model.name))
                .collect();
            // A selection keeps its model while the list loads.
            for (key, _) in TASKS {
                if let Some((instance, slug)) = ext::task_selection(&self.settings, key)
                    && instance == generator.id
                    && !slug.is_empty()
                    && models.iter().all(|m| m.slug != slug)
                {
                    models.push(generator_model(&slug, &slug));
                }
            }
            let driver = driver_key(ext::brand(&generator.config)).unwrap_or("openaiCompatible");
            let name = generator.name();
            let shared = instances.iter().any(|i| i.mark.driver == driver);
            instances.push(PickerInstance {
                instance_id: generator.id.clone(),
                mark: Mark {
                    driver: driver.to_owned(),
                    badge: shared.then(|| crate::providers::accent_badge(&name, None)),
                    display_name: name.clone(),
                },
                name,
                unavailable,
                models,
                driver: driver.to_owned(),
            });
        }
        for instance in &mut instances {
            for model in &mut instance.models {
                model.favorite = self
                    .extras
                    .favorites
                    .iter()
                    .any(|(i, m)| *i == instance.instance_id && *m == model.slug);
            }
        }
        instances
    }

    /// The task picker's instances, for the Threads tab's text generation row.
    pub fn text_targets(&self) -> Vec<PickerInstance> {
        self.task_instances()
    }

    /// Asks for the models of every enabled generator, as opening the picker does.
    pub fn warm_text_targets(&mut self) -> Vec<PickerInstance> {
        let ids: Vec<String> = ext::text_generators(&self.settings)
            .into_iter()
            .filter(TextGenerator::enabled)
            .map(|generator| generator.id)
            .collect();
        for id in ids {
            self.ensure_models(&id);
        }
        self.task_instances()
    }

    pub(super) fn task_chosen(&self, instances: &[PickerInstance], index: usize) -> Chosen {
        let (instance, slug) =
            ext::task_selection(&self.settings, TASKS[index].0).unwrap_or_default();
        chosen(instances, &instance, &slug)
    }

    /// Opens on the active model's rail entry, or on Favorites (`initialModelPickerProvider`).
    fn open_task_picker(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.set_open(Some(Open::Task(index)), cx);
        let instances = self.task_instances();
        let active = self.task_chosen(&instances, index);
        let visible = instances
            .iter()
            .any(|i| i.instance_id == active.instance && i.unavailable.is_none());
        let origin = self.extras.pick.origin;
        let tab = model_picker::initial_tab(
            &active.instance,
            visible,
            !self.extras.favorites.is_empty(),
            origin,
        );
        let expanded = model_picker::starts_expanded(&instances, &active.instance, &active.slug);
        let highlight = model_picker::listing(&instances, &tab, "", expanded)
            .rows
            .iter()
            .position(|row| {
                row.instance.instance_id == active.instance && row.model.slug == active.slug
            })
            .unwrap_or(0);
        self.extras.pick = State {
            tab: Some(tab),
            expanded,
            highlight,
            origin,
            ..State::default()
        };
        let view = cx.weak_entity();
        self.extras.pick_keys = Some(cx.intercept_keystrokes(move |event, window, cx| {
            let handled = view
                .update(cx, |this, cx| this.task_key(&event.keystroke, window, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        }));
        let search = self.extras.pick_search.clone();
        search.update(cx, |state, cx| state.set_value("", window, cx));
        window.focus(&search.focus_handle(cx), cx);
        cx.notify();
    }

    fn task_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.extras
            .pick_search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.extras.pick.tab = Some(tab);
        self.extras.pick.expanded = false;
        self.extras.pick.highlight = 0;
        self.extras.pick.focus = Focus::List;
        cx.notify();
    }

    fn task_older(&mut self, index: usize, cx: &mut Context<Self>) {
        self.extras.pick.expanded = !self.extras.pick.expanded;
        let instances = self.task_instances();
        let tab = self.picker_tab(&instances, index);
        let query = self.extras.pick_search.read(cx).value();
        self.extras.pick.highlight =
            model_picker::listing(&instances, &tab, &query, self.extras.pick.expanded)
                .rows
                .len();
        cx.notify();
    }

    fn picker_tab(&self, instances: &[PickerInstance], index: usize) -> Tab {
        self.extras
            .pick
            .tab
            .clone()
            .unwrap_or_else(|| Tab::Instance(self.task_chosen(instances, index).instance))
    }

    /// Stars or unstars a model: the local copy now, the server's `setModelFavorites` behind it.
    fn task_favorite(&mut self, instance: &str, slug: &str, cx: &mut Context<Self>) {
        let key = (instance.to_owned(), slug.to_owned());
        let favorite = !self.extras.favorites.contains(&key);
        let before = self.extras.favorites.clone();
        if favorite {
            self.extras.favorites.push(key);
        } else {
            self.extras.favorites.retain(|k| *k != key);
        }
        let sent = self.call(
            Call::UpdateSettings,
            crate::send::favorite_payload(instance, slug, favorite),
            Pending::Favorite {
                before: before.clone(),
            },
        );
        if !sent {
            self.extras.favorites = before;
        }
        cx.notify();
    }

    pub(super) fn pick_task(
        &mut self,
        task: usize,
        instance: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) {
        self.extras.pick.origin = Some(if self.extras.pick.tab == Some(Tab::Favorites) {
            Origin::Favorites
        } else {
            Origin::Instance
        });
        let patch = ext::task_patch(TASKS[task].0, instance, model);
        self.write_patch(patch, cx);
        self.set_open(None, cx);
    }

    /// Cmd+1..9 jump, arrows move within the focused column, Left and Right cross columns at the
    /// search's edges. Returns whether the key was taken.
    fn task_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(Open::Task(index)) = self.extras.open.clone() else {
            return false;
        };
        let instances = self.task_instances();
        let tab = self.picker_tab(&instances, index);
        let search = self.extras.pick_search.read(cx);
        let (query, cursor) = (search.value(), search.cursor());
        let listing = model_picker::listing(&instances, &tab, &query, self.extras.pick.expanded);
        let key = keystroke.key.as_str();
        let modifiers = keystroke.modifiers;
        if modifiers.platform && !modifiers.alt && !modifiers.control {
            let Some(number) = key.parse::<usize>().ok().filter(|n| (1..=9).contains(n)) else {
                return false;
            };
            if let Some(&at) = listing.jump_rows().get(number - 1) {
                let row = listing.rows[at];
                let (instance, slug) = (row.instance.instance_id.clone(), row.model.slug.clone());
                self.pick_task(index, &instance, &slug, cx);
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
        match (self.extras.pick.focus, key) {
            (Focus::List, "down" | "up") => {
                self.extras.pick.highlight =
                    model_picker::step(self.extras.pick.highlight, listing.items(), delta);
            }
            (Focus::List, "enter") => {
                let at = self.extras.pick.highlight;
                if let Some(row) = listing.rows.get(at) {
                    if row.usable() {
                        let (instance, slug) =
                            (row.instance.instance_id.clone(), row.model.slug.clone());
                        self.pick_task(index, &instance, &slug, cx);
                    }
                } else if listing.older.is_some() {
                    self.task_older(index, cx);
                }
            }
            (Focus::List, "left") if cursor == 0 => self.extras.pick.focus = Focus::Sidebar,
            (Focus::Sidebar, "down" | "up") => {
                let next = model_picker::adjacent_tab(&instances, &tab, delta);
                self.task_tab(next, window, cx);
                self.extras.pick.focus = Focus::Sidebar;
            }
            (Focus::Sidebar, "right" | "enter") => self.extras.pick.focus = Focus::List,
            _ => return false,
        }
        cx.notify();
        true
    }

    /// The trigger chip and, once open, the popover below it.
    pub(super) fn task_picker(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let instances = self.task_instances();
        let active = self.task_chosen(&instances, index);
        let open = self.extras.open == Some(Open::Task(index));
        let view = cx.entity();
        let search = self.extras.pick_search.clone();
        let trigger = TaskTrigger {
            id: ("task-picker", index),
            mark: active.mark.clone(),
            label: active.label.clone(),
            open,
        };
        let panel = open.then(|| {
            let host: Host = {
                let view = view.clone();
                std::rc::Rc::new(move |act, window, cx| {
                    view.update(cx, |this, cx| match act {
                        Act::Tab(tab) => this.task_tab(tab, window, cx),
                        Act::Older => this.task_older(index, cx),
                        Act::Favorite(id, slug) => this.task_favorite(&id, &slug, cx),
                        Act::Pick(id, slug) => this.pick_task(index, &id, &slug, cx),
                        Act::Option(..) => {}
                    });
                })
            };
            Panel {
                host,
                search: search.clone(),
                tab: self.picker_tab(&instances, index),
                expanded: self.extras.pick.expanded,
                highlight: self.extras.pick.highlight,
                focus: self.extras.pick.focus,
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
                instances,
            }
        });
        let popup = Tokens::of(cx).popover;
        Popover::new(("task-model-picker", index))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .track_focus(&search.focus_handle(cx))
            .p_0()
            .bg(popup)
            .border_1()
            .border_color(crate::ui::dropdown_glass_edge(cx))
            .on_open_change(move |next, window, cx| {
                view.update(cx, |this, cx| {
                    if *next {
                        this.open_task_picker(index, window, cx);
                    } else if this.extras.open == Some(Open::Task(index)) {
                        this.set_open(None, cx);
                    }
                })
            })
            .trigger(trigger)
            .content(move |_, window, cx| match &panel {
                Some(panel) => panel.render(window, cx),
                None => div().into_any_element(),
            })
            .into_any_element()
    }
}

fn generator_model(slug: &str, name: &str) -> PickerModel {
    PickerModel {
        slug: slug.to_owned(),
        label: name.to_owned(),
        title: name.to_owned(),
        name: name.to_owned(),
        short_name: None,
        sub_provider: None,
        favorite: false,
        legacy: false,
        new: false,
    }
}

/// `SETTINGS_PICKER_TRIGGER_CLASSNAME`: the provider mark, the model, and a chevron, outlined.
#[derive(IntoElement)]
struct TaskTrigger {
    id: (&'static str, usize),
    mark: Mark,
    label: SharedString,
    open: bool,
}

impl Selectable for TaskTrigger {
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

impl RenderOnce for TaskTrigger {
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
            .child(crate::ui::provider_mark(&self.mark, 1., cx))
            .child(
                div()
                    .max_w(px(224.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.label),
            )
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(14.))
                    .opacity(0.7)
                    .text_color(theme.muted_foreground),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(id: &str, slugs: &[&str]) -> PickerInstance {
        PickerInstance {
            instance_id: id.to_owned(),
            mark: Mark {
                driver: "codex".to_owned(),
                display_name: id.to_owned(),
                badge: None,
            },
            name: id.to_owned(),
            unavailable: None,
            models: slugs
                .iter()
                .map(|slug| generator_model(slug, slug))
                .collect(),
            driver: "codex".to_owned(),
        }
    }

    #[test]
    fn the_trigger_names_the_pickers_model_else_the_slug() {
        let instances = [instance("codex", &["gpt-5"])];
        let known = chosen(&instances, "codex", "gpt-5");
        assert_eq!(
            (known.mark.driver.as_str(), known.label.as_ref()),
            ("codex", "gpt-5")
        );
        let gone = chosen(&instances, "other", "m1");
        assert_eq!((gone.mark.driver.as_str(), gone.label.as_ref()), ("", "m1"));
    }
}
