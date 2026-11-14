//! The editor's three pickers: the composer's model popover behind a pill (`ProviderModelPicker`),
//! the base branch refs popover (`BranchPicker`) and the thread combobox of the destination.

use super::*;
use crate::model_picker::{self, Focus, Origin, State, Tab};
use crate::providers::{Controls, Mark, PickerInstance, Trigger};
use crate::refs::Page;
use crate::ui::model_popover::{Act, Host, Panel};
use crate::ui::settings::components::Toggle;
use gpui::{Keystroke, ScrollHandle};
use gpui_component::Selectable;
use gpui_component::input::InputEvent;

const BASE_REMOTE_TIP: &str = "Uses the latest commit of the base branch\u{2019}s tracked remote branch, even if its name differs. Otherwise uses the same-named branch on origin. Uses your local branch if that remote branch is missing.";

/// The pickers' open state and loaded data.
pub(crate) struct Pickers {
    pub model_search: Entity<InputState>,
    pub ref_search: Entity<InputState>,
    pub thread_search: Entity<InputState>,
    pub project_search: Entity<InputState>,
    pub pick: State,
    pub keys: Option<Subscription>,
    /// The stars as the user left them, until the catalog says the same.
    favorites: Option<Vec<Model>>,
    pub refs: Page,
    pub ref_query: String,
    pub ref_loading: bool,
    pub ref_error: Option<String>,
    pub ref_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl Pickers {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<ScheduledTasksPanel>) -> Self {
        let search = |placeholder: &'static str, window: &mut Window, cx: &mut Context<_>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let model_search = search("Search models...", window, cx);
        let ref_search = search("Search refs...", window, cx);
        let thread_search = search("Search all threads...", window, cx);
        let project_search = search("Search projects", window, cx);
        let subscriptions = vec![
            cx.subscribe(&ref_search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.ref_query_changed(cx);
                }
            }),
            cx.subscribe(&thread_search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&project_search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&model_search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            model_search,
            ref_search,
            thread_search,
            project_search,
            pick: State::default(),
            keys: None,
            favorites: None,
            refs: Page::default(),
            ref_query: String::new(),
            ref_loading: false,
            ref_error: None,
            ref_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Forgets what an editor session loaded.
    pub(crate) fn reset(&mut self) {
        self.keys = None;
        self.refs = Page::default();
        self.ref_query.clear();
        self.ref_loading = false;
        self.ref_error = None;
        self.pick = State::default();
    }

    /// Drops the local stars once the catalog agrees with them and none is in flight.
    pub(crate) fn settle_favorites(&mut self, data: &[Model], in_flight: bool) {
        if !in_flight && self.favorites.as_deref() == Some(data) {
            self.favorites = None;
        }
    }
}

/// What the pill shows for the chosen model.
struct Active {
    mark: Mark,
    label: String,
    instance: String,
    slug: String,
}

impl ScheduledTasksPanel {
    fn favorites(&self) -> Vec<Model> {
        self.pickers
            .favorites
            .clone()
            .unwrap_or_else(|| self.data.favorites.clone())
    }

    fn model_instances(&self) -> Vec<PickerInstance> {
        let mut instances = self.data.picker.clone();
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

    fn model_active(
        &self,
        draft: &Draft,
        environment: &str,
        instances: &[PickerInstance],
    ) -> Option<Active> {
        let Some((instance, slug)) = self.selection(draft, environment) else {
            let first = instances.first()?;
            return Some(Active {
                mark: first.mark.clone(),
                label: "Select a model".to_owned(),
                instance: String::new(),
                slug: String::new(),
            });
        };
        let found = instances.iter().find(|i| i.instance_id == instance);
        let model = found.and_then(|i| i.models.iter().find(|m| m.slug == slug));
        Some(Active {
            mark: found.map_or_else(
                || Mark {
                    driver: String::new(),
                    display_name: instance.clone(),
                    badge: None,
                },
                |i| i.mark.clone(),
            ),
            label: model.map_or_else(|| slug.clone(), |m| m.label.clone()),
            instance,
            slug,
        })
    }

    fn model_tab(&self, active: &Option<Active>) -> Tab {
        self.pickers.pick.tab.clone().unwrap_or_else(|| {
            Tab::Instance(
                active
                    .as_ref()
                    .map(|active| active.instance.clone())
                    .unwrap_or_default(),
            )
        })
    }

    fn editor_state(&self) -> Option<(Draft, String)> {
        let editor = self.editor.as_ref()?;
        Some((editor.draft.clone(), editor.environment.clone()))
    }

    /// Opens on the active model's rail entry, or on Favorites (`initialModelPickerProvider`).
    fn open_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((draft, environment)) = self.editor_state() else {
            return;
        };
        let instances = self.model_instances();
        let (instance, model) = self
            .model_active(&draft, &environment, &instances)
            .map(|active| (active.instance, active.slug))
            .unwrap_or_default();
        let visible = instances
            .iter()
            .any(|i| i.instance_id == instance && i.unavailable.is_none());
        let origin = self.pickers.pick.origin;
        let has_favorites = !self.favorites().is_empty();
        let tab = model_picker::initial_tab(&instance, visible, has_favorites, origin);
        let expanded = model_picker::starts_expanded(&instances, &instance, &model);
        let highlight = model_picker::listing(&instances, &tab, "", expanded)
            .rows
            .iter()
            .position(|row| row.instance.instance_id == instance && row.model.slug == model)
            .unwrap_or(0);
        self.pickers.pick = State {
            tab: Some(tab),
            expanded,
            highlight,
            origin,
            ..State::default()
        };
        self.set_menu(Menu::Model, true, cx);
        let view = cx.weak_entity();
        self.pickers.keys = Some(cx.intercept_keystrokes(move |event, window, cx| {
            let handled = view
                .update(cx, |this, cx| this.model_key(&event.keystroke, window, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        }));
        let search = self.pickers.model_search.clone();
        search.update(cx, |state, cx| state.set_value("", window, cx));
        window.focus(&search.focus_handle(cx), cx);
    }

    fn model_switch_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.pickers
            .model_search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.pickers.pick.tab = Some(tab);
        self.pickers.pick.expanded = false;
        self.pickers.pick.highlight = 0;
        self.pickers.pick.focus = Focus::List;
        cx.notify();
    }

    fn model_older(&mut self, cx: &mut Context<Self>) {
        let Some((draft, environment)) = self.editor_state() else {
            return;
        };
        self.pickers.pick.expanded = !self.pickers.pick.expanded;
        let instances = self.model_instances();
        let active = self.model_active(&draft, &environment, &instances);
        let tab = self.model_tab(&active);
        let query = self.pickers.model_search.read(cx).value();
        self.pickers.pick.highlight =
            model_picker::listing(&instances, &tab, &query, self.pickers.pick.expanded)
                .rows
                .len();
        cx.notify();
    }

    /// Stars or unstars a model: the local list now, the server's `setModelFavorites` behind it.
    fn model_favorite(&mut self, instance: &str, slug: &str, cx: &mut Context<Self>) {
        let key = (instance.to_owned(), slug.to_owned());
        let before = self.favorites();
        let favorite = !before.contains(&key);
        let mut next = before.clone();
        if favorite {
            next.push(key);
        } else {
            next.retain(|k| *k != key);
        }
        let sent = self.send(
            PRIMARY,
            Call::UpdateSettings,
            crate::send::favorite_payload(instance, slug, favorite),
            Pending::Favorite(before),
        );
        if sent {
            self.pickers.favorites = Some(next);
        }
        cx.notify();
    }

    fn model_pick(&mut self, instance: &str, slug: &str, cx: &mut Context<Self>) {
        self.pickers.pick.origin = Some(if self.pickers.pick.tab == Some(Tab::Favorites) {
            Origin::Favorites
        } else {
            Origin::Instance
        });
        let model = (instance.to_owned(), slug.to_owned());
        self.edit(cx, |editor| editor.draft.model = Some(model));
    }

    /// Cmd+1..9 jump, arrows move within the focused column, Left and Right cross columns at the
    /// search's edges. Returns whether the key was taken.
    fn model_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((draft, environment)) = self.editor_state() else {
            return false;
        };
        if self.editor.as_ref().and_then(|editor| editor.menu) != Some(Menu::Model) {
            return false;
        }
        let instances = self.model_instances();
        let active = self.model_active(&draft, &environment, &instances);
        let tab = self.model_tab(&active);
        let search = self.pickers.model_search.read(cx);
        let (query, cursor) = (search.value(), search.cursor());
        let listing = model_picker::listing(&instances, &tab, &query, self.pickers.pick.expanded);
        let key = keystroke.key.as_str();
        let modifiers = keystroke.modifiers;
        if modifiers.platform && !modifiers.alt && !modifiers.control {
            let Some(number) = key.parse::<usize>().ok().filter(|n| (1..=9).contains(n)) else {
                return false;
            };
            if let Some(&at) = listing.jump_rows().get(number - 1) {
                let row = listing.rows[at];
                let (instance, model) = (row.instance.instance_id.clone(), row.model.slug.clone());
                self.model_pick(&instance, &model, cx);
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
        match (self.pickers.pick.focus, key) {
            (Focus::List, "down" | "up") => {
                self.pickers.pick.highlight =
                    model_picker::step(self.pickers.pick.highlight, listing.items(), delta);
            }
            (Focus::List, "enter") => {
                let at = self.pickers.pick.highlight;
                if let Some(row) = listing.rows.get(at) {
                    if row.usable() {
                        let (instance, model) =
                            (row.instance.instance_id.clone(), row.model.slug.clone());
                        self.model_pick(&instance, &model, cx);
                    }
                } else if listing.older.is_some() {
                    self.model_older(cx);
                }
            }
            (Focus::List, "left") if cursor == 0 => self.pickers.pick.focus = Focus::Sidebar,
            (Focus::Sidebar, "down" | "up") => {
                let next = model_picker::adjacent_tab(&instances, &tab, delta);
                self.model_switch_tab(next, window, cx);
                self.pickers.pick.focus = Focus::Sidebar;
            }
            (Focus::Sidebar, "right" | "enter") => self.pickers.pick.focus = Focus::List,
            _ => return false,
        }
        cx.notify();
        true
    }

    /// `ProviderModelPicker` in a form field: the pill and the composer's model popover. Other
    /// environments have no catalog here, so they keep a text field.
    pub(super) fn model_field(
        &mut self,
        view: &Entity<Self>,
        draft: &Draft,
        environment: &str,
        menu: Option<Menu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        if environment != PRIMARY {
            return field_of(
                "Model",
                None,
                field(&self.inputs.model, false, window, cx),
                cx,
            )
            .into_any_element();
        }
        let instances = self.model_instances();
        let active = self.model_active(draft, environment, &instances);
        let Some(shown) = &active else {
            return field_of(
                "Model",
                None,
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("No providers available"),
                cx,
            )
            .into_any_element();
        };
        let open = menu == Some(Menu::Model);
        let search = self.pickers.model_search.clone();
        let trigger = Pill {
            id: ("scheduled-task-model-pill", 0),
            mark: shown.mark.clone(),
            label: shown.label.clone().into(),
            open,
        };
        let panel = open.then(|| {
            let host: Host = {
                let view = view.clone();
                Rc::new(move |act, window, cx| {
                    view.update(cx, |this, cx| match act {
                        Act::Tab(tab) => this.model_switch_tab(tab, window, cx),
                        Act::Older => this.model_older(cx),
                        Act::Favorite(id, model) => this.model_favorite(&id, &model, cx),
                        Act::Pick(id, model) => this.model_pick(&id, &model, cx),
                        Act::Option(..) => {}
                    });
                })
            };
            Panel {
                host,
                search: search.clone(),
                tab: self.model_tab(&active),
                expanded: self.pickers.pick.expanded,
                highlight: self.pickers.pick.highlight,
                focus: self.pickers.pick.focus,
                effort: 0,
                footer: None,
                trigger: Trigger {
                    instance_id: shown.instance.clone(),
                    slug: shown.slug.clone(),
                    mark: shown.mark.clone(),
                    model: shown.label.clone(),
                    effort: None,
                    fast: false,
                    controls: Controls::default(),
                    options: Vec::new(),
                    title: shown.label.clone(),
                },
                instances: instances.clone(),
            }
        });
        let picker = Popover::new("scheduled-task-model-picker")
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .open(open)
            .track_focus(&search.focus_handle(cx))
            .p_0()
            .bg(theme.popover)
            .border_1()
            .border_color(crate::ui::dropdown_glass_edge(cx))
            .on_open_change({
                let view = view.clone();
                move |next, window, cx| {
                    view.update(cx, |this, cx| {
                        if *next {
                            this.open_model(window, cx);
                        } else {
                            this.set_menu(Menu::Model, false, cx);
                        }
                    })
                }
            })
            .trigger(trigger)
            .content(move |_, window, cx| match &panel {
                Some(panel) => panel.render(window, cx),
                None => div().into_any_element(),
            });
        field_of("Model", None, h_flex().justify_start().child(picker), cx).into_any_element()
    }

    /// Looks the draft's base ref up, so the button can tell a remote ref from a local one.
    pub(super) fn lookup_base(&mut self, cx: &mut Context<Self>) {
        let Some((draft, environment)) = self.editor_state() else {
            return;
        };
        let cwd = self.project_root(&environment, &draft.project_id);
        let name = draft.base_ref.trim().to_owned();
        if cwd.is_empty() || name.is_empty() {
            return;
        }
        let payload = crate::branch_picker::list_payload(&cwd, &name, None);
        self.send(
            &environment,
            Call::ListRefs,
            payload,
            Pending::BaseLookup { name },
        );
        cx.notify();
    }

    fn list_refs(&mut self, append: bool) {
        let Some((draft, environment)) = self.editor_state() else {
            return;
        };
        let cwd = self.project_root(&environment, &draft.project_id);
        if cwd.is_empty() {
            return;
        }
        let cursor = if append {
            let Some(cursor) = self.pickers.refs.next_cursor else {
                return;
            };
            Some(cursor)
        } else {
            None
        };
        let query = self.pickers.ref_query.clone();
        let payload = crate::branch_picker::list_payload(&cwd, &query, cursor);
        if self.send(
            &environment,
            Call::ListRefs,
            payload,
            Pending::Refs { query, append },
        ) {
            self.pickers.ref_loading = true;
            self.pickers.ref_error = None;
        } else {
            self.pickers.ref_error = Some("The environment is not connected.".to_owned());
        }
    }

    fn ref_query_changed(&mut self, cx: &mut Context<Self>) {
        if self.editor.as_ref().and_then(|editor| editor.menu) != Some(Menu::BaseRef) {
            return;
        }
        let query = self.pickers.ref_search.read(cx).value().to_string();
        if query == self.pickers.ref_query {
            return;
        }
        self.pickers.ref_query = query;
        self.pickers.refs = Page::default();
        self.list_refs(false);
        cx.notify();
    }

    fn open_base(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pickers.ref_query.clear();
        self.pickers.refs = Page::default();
        self.pickers.ref_error = None;
        self.set_menu(Menu::BaseRef, true, cx);
        let search = self.pickers.ref_search.clone();
        search.update(cx, |state, cx| state.set_value("", window, cx));
        window.focus(&search.focus_handle(cx), cx);
        self.list_refs(false);
        cx.notify();
    }

    fn pick_base(&mut self, name: String, cx: &mut Context<Self>) {
        let remote = self
            .pickers
            .refs
            .refs
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.is_remote);
        self.edit(cx, |editor| {
            editor.draft.base_ref = name;
            editor.base_remote = remote;
        });
    }

    /// The base branch field: a `GitBranch` button over the refs popover with its "Start from
    /// remote" switch.
    pub(super) fn base_branch(
        &mut self,
        view: &Entity<Self>,
        draft: &Draft,
        environment: &str,
        base_remote: bool,
        menu: Option<Menu>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cwd = self.project_root(environment, &draft.project_id);
        let enabled = !cwd.is_empty();
        let open = enabled && menu == Some(Menu::BaseRef);
        let label = ed::branch_trigger_label(&draft.base_ref, draft.start_from_origin, base_remote);
        let theme = Tokens::of(cx);
        let trigger = button("scheduled-task-base-ref", Variant::Outline, enabled, cx)
            .w_full()
            .justify_start()
            .child(
                Icon::new(Icons::GitBranch)
                    .size(px(14.))
                    .text_color(theme.muted_foreground),
            )
            .child(div().min_w_0().truncate().child(label));
        let footer: crate::ref_picker::Footer = {
            let view = view.clone();
            let checked = draft.start_from_origin;
            Rc::new(move |window, cx| {
                let theme = Tokens::of(cx);
                let toggled = view.clone();
                let toggle: Toggle = Rc::new(move |_, cx| {
                    toggled.update(cx, |this, cx| {
                        if let Some(editor) = this.editor.as_mut() {
                            editor.draft.start_from_origin = !editor.draft.start_from_origin;
                        }
                        cx.notify();
                    })
                });
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .border_t_1()
                    .border_color(theme.border.opacity(0.6))
                    .px_3()
                    .py_2()
                    .text_xs()
                    .child(
                        h_flex()
                            .id("scheduled-task-base-remote-label")
                            .items_center()
                            .gap(px(6.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .tooltip(crate::details_card::tip(BASE_REMOTE_TIP))
                            .child(Icon::new(Icons::RefreshCw).size(px(12.)))
                            .child("Start from remote"),
                    )
                    .child(switch(
                        "scheduled-task-base-remote",
                        checked,
                        toggle,
                        window,
                        cx,
                    ))
                    .into_any_element()
            })
        };
        let (on_pick, on_more) = (view.clone(), view.clone());
        let refs = crate::ref_picker::RefMenu {
            query: self.pickers.ref_query.clone(),
            page: self.pickers.refs.clone(),
            loading: self.pickers.ref_loading,
            error: self.pickers.ref_error.clone(),
            cwd: cwd.clone(),
            project_cwd: cwd,
            search: self.pickers.ref_search.clone(),
            scroll: self.pickers.ref_scroll.clone(),
            can_create: false,
            on_pick: Rc::new(move |name, _, cx| {
                on_pick.update(cx, |this, cx| this.pick_base(name, cx));
            }),
            on_create: Rc::new(|_, _| {}),
            on_more: Rc::new(move |cx| {
                on_more.update(cx, |this, cx| {
                    if !this.pickers.ref_loading {
                        this.list_refs(true);
                        cx.notify();
                    }
                });
            }),
            footer: Some(footer),
        };
        let opened = view.clone();
        Popover::new("scheduled-task-base-picker")
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .open(open)
            .p_0()
            .on_open_change(move |next, window, cx| {
                opened.update(cx, |this, cx| {
                    if *next {
                        this.open_base(window, cx);
                    } else {
                        this.set_menu(Menu::BaseRef, false, cx);
                    }
                })
            })
            .trigger(PopTrigger::new(trigger))
            .content(move |_, window, cx| refs.render(window, cx))
            .into_any_element()
    }

    /// The Destination field: the thread mode select, the thread combobox and the help text.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn destination_field(
        &mut self,
        view: &Entity<Self>,
        draft: &Draft,
        environment: &str,
        offers_modes: bool,
        menu: Option<Menu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let mode = ed::thread_mode(draft);
        let bound = !draft.thread_id.is_empty();
        let mut body = v_flex().gap_2();
        if offers_modes {
            let selected = ed::THREAD_MODES.iter().position(|m| *m == mode);
            body = body.child(Self::select(
                view,
                Sel {
                    id: "scheduled-task-destination".into(),
                    menu: Menu::Destination,
                    highlight: self.highlight(),
                    open: menu == Some(Menu::Destination),
                    disabled: false,
                    lead: None,
                    current: ed::thread_mode_label(&mode),
                    placeholder: false,
                    options: ed::THREAD_MODES
                        .iter()
                        .map(|m| Opt::plain(ed::thread_mode_label(m)))
                        .collect(),
                    selected,
                    width: super::FULL_WIDTH,
                },
                Rc::new(|this, index, _, cx| {
                    if let Some(mode) = ed::THREAD_MODES.get(index) {
                        this.edit(cx, |editor| ed::set_thread_mode(&mut editor.draft, mode));
                    }
                }),
                window,
                cx,
            ));
        }
        if !offers_modes || mode != ed::NEW_THREAD {
            body = body.child(self.thread_combobox(view, draft, environment, &mode, menu, cx));
        }
        if offers_modes {
            let mut detail = ed::destination_detail(&mode, bound).to_owned();
            if mode != ed::NEW_THREAD {
                detail.push_str(" Deleting the thread also deletes this task.");
            }
            body = body.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(detail),
            );
        }
        field_of("Destination", None, body, cx).into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn thread_combobox(
        &mut self,
        view: &Entity<Self>,
        draft: &Draft,
        _environment: &str,
        mode: &str,
        menu: Option<Menu>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let open = menu == Some(Menu::Thread);
        let title_of = |id: &str| {
            self.data
                .threads
                .iter()
                .find(|thread| thread.id == id)
                .map_or_else(|| format!("Thread {id}"), |thread| thread.title.clone())
        };
        let label = if draft.thread_id.is_empty() {
            if mode == ed::NEW_THREAD {
                "New thread each run".to_owned()
            } else {
                "Started by the first run".to_owned()
            }
        } else {
            title_of(&draft.thread_id)
        };
        let query = self.pickers.thread_search.read(cx).value().to_string();
        let searching = !query.trim().is_empty();
        // None is "New thread each run"; Some is a thread id and its title.
        let mut rows: Vec<(Option<String>, String, bool)> = Vec::new();
        if !searching {
            rows.push((
                None,
                "New thread each run".to_owned(),
                draft.thread_id.is_empty(),
            ));
        }
        if !draft.thread_id.is_empty() {
            let title = title_of(&draft.thread_id);
            if !searching || title.to_lowercase().contains(&query.trim().to_lowercase()) {
                rows.push((Some(draft.thread_id.clone()), title, true));
            }
        }
        for thread in ed::destination_threads(
            &self.data.threads,
            &draft.project_id,
            &draft.thread_id,
            &query,
        ) {
            rows.push((Some(thread.id.clone()), thread.title.clone(), false));
        }
        let project_id = draft.project_id.clone();
        let search = self.pickers.thread_search.clone();
        let trigger = button("scheduled-task-thread", Variant::Outline, true, cx)
            .w_full()
            .justify_between()
            .child(
                h_flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        Icon::new(Icons::MessageSquareText)
                            .size(px(14.))
                            .text_color(theme.muted_foreground),
                    )
                    .child(div().min_w_0().truncate().child(label)),
            )
            .child(
                div()
                    .opacity(0.7)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            );
        let picked = view.clone();
        let opened = view.clone();
        let focus = search.clone();
        let search_for_pick = search.clone();
        let search_input = search.clone();
        Popover::new("scheduled-task-thread-picker")
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .open(open)
            .track_focus(&search.focus_handle(cx))
            .p_0()
            .appearance(false)
            .on_open_change(move |next, window, cx| {
                opened.update(cx, |this, cx| {
                    this.set_menu(Menu::Thread, *next, cx);
                });
                if *next {
                    window.focus(&focus.focus_handle(cx), cx);
                }
            })
            .trigger(PopTrigger::new(trigger))
            .content(move |_, window, cx| {
                let theme = Tokens::of(cx);
                let accent = theme.accent;
                let list =
                    if rows.is_empty() {
                        div()
                            .px_2()
                            .py_3()
                            .text_sm()
                            .text_center()
                            .text_color(theme.muted_foreground)
                            .child("No threads found.")
                            .into_any_element()
                    } else {
                        v_flex()
                            .id("scheduled-task-thread-list")
                            .max_h(px(240.))
                            .overflow_y_scroll()
                            .children(rows.iter().enumerate().map(
                                |(index, (id, title, current))| {
                                    let (view, id, project_id, search) = (
                                        picked.clone(),
                                        id.clone(),
                                        project_id.clone(),
                                        search_for_pick.clone(),
                                    );
                                    h_flex()
                                        .id(("scheduled-task-thread-row", index))
                                        .min_h(px(28.))
                                        .items_center()
                                        .px_2()
                                        .py_1()
                                        .rounded(px(4.))
                                        .text_sm()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(accent))
                                        .when(*current, |row| {
                                            row.bg(theme.foreground.opacity(0.08))
                                        })
                                        .child(div().min_w_0().truncate().child(title.clone()))
                                        .on_click(move |_, window, cx| {
                                            search.update(cx, |state, cx| {
                                                state.set_value("", window, cx)
                                            });
                                            let (id, project_id) = (id.clone(), project_id.clone());
                                            view.update(cx, |this, cx| {
                                                this.edit(cx, |editor| match id {
                                                    None => {
                                                        ed::unbind_from_thread(&mut editor.draft);
                                                        editor.draft.thread_mode =
                                                            Some(json!(ed::NEW_THREAD));
                                                    }
                                                    Some(id) => ed::move_to_thread(
                                                        &mut editor.draft,
                                                        &id,
                                                        &project_id,
                                                    ),
                                                })
                                            })
                                        })
                                },
                            ))
                            .into_any_element()
                    };
                popup_shell(cx)
                    .w(px(320.))
                    .p_1()
                    .gap_1()
                    .child(field(&search_input, false, window, cx))
                    .child(list)
            })
            .into_any_element()
    }

    /// Answers of the pickers' calls: ref pages, the base ref lookup and the star.
    pub(in crate::ui::settings::scheduled_tasks_panel) fn on_picker_call(
        &mut self,
        token: u64,
        event: CallEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        let Some(pending) = self.pending.remove(&token) else {
            return;
        };
        match (pending, event) {
            (Pending::Refs { query, append }, CallEvent::Done(value))
                if query == self.pickers.ref_query =>
            {
                let page = Page::decode(&value);
                if append {
                    self.pickers.refs.refs.extend(page.refs);
                    self.pickers.refs.total = page.total;
                    self.pickers.refs.has_more = page.has_more;
                    self.pickers.refs.next_cursor = page.next_cursor;
                } else {
                    self.pickers.refs = page;
                }
                self.pickers.ref_loading = false;
            }
            (Pending::Refs { query, .. }, CallEvent::Failed(message))
                if query == self.pickers.ref_query =>
            {
                self.pickers.ref_loading = false;
                self.pickers.ref_error = Some(message);
            }
            (Pending::BaseLookup { name }, CallEvent::Done(value)) => {
                let page = Page::decode(&value);
                if let Some(editor) = self.editor.as_mut()
                    && editor.draft.base_ref.trim() == name
                {
                    editor.base_remote = page
                        .refs
                        .iter()
                        .find(|entry| entry.name == name)
                        .map(|entry| entry.is_remote);
                }
            }
            (Pending::Favorite(before), CallEvent::Failed(_)) => {
                self.pickers.favorites = Some(before);
            }
            _ => {}
        }
        cx.notify();
    }
}

/// `SETTINGS_PICKER_TRIGGER_CLASSNAME`: an outlined pill with the provider mark and a chevron.
#[derive(IntoElement)]
struct Pill {
    id: (&'static str, usize),
    mark: Mark,
    label: SharedString,
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
                Icon::new(Icons::ChevronDown)
                    .size(px(14.))
                    .opacity(0.7)
                    .text_color(theme.muted_foreground),
            )
    }
}
