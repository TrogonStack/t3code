//! The two pickers of the settings scope sentence: `ScopeMenu` (SettingsScopeSentence.tsx:118-226)
//! with `ProjectScopeMenu` (306-344) and `EnvironmentScopeMenu` (228-304) feeding it. A list of
//! more than six options is a combobox with a search field (`w-80`); a shorter one is a menu
//! (`w-72`) with a separator under the "all" entry.

use super::components::tab_stop;
use super::tokens::Tokens;
use super::*;
use crate::settings_labs::{self as labs, ALL, Choice, ChoiceIcon, ScopeData, ScopeSearch};
use gpui::{BoxShadow, ElementId, FocusHandle, KeyDownEvent, point};
use std::rc::Rc;

pub(super) type OpenHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;
pub(super) type ChoiceHandler = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// What one picker draws.
pub(super) struct Picker {
    pub id: &'static str,
    pub label: String,
    pub icon: Option<ChoiceIcon>,
    pub all: Option<Choice>,
    pub choices: Vec<Choice>,
    pub selected: String,
    pub open: bool,
    pub search: Entity<InputState>,
    pub query: String,
    pub on_open: OpenHandler,
    pub on_pick: ChoiceHandler,
}

/// `Button variant="outline" size="sm"` with `min-w-0 max-w-72`.
#[derive(IntoElement)]
struct Trigger {
    id: ElementId,
    label: String,
    icon: Option<AnyElement>,
    handle: FocusHandle,
    open: bool,
    on_open: OpenHandler,
}

impl Selectable for Trigger {
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

impl RenderOnce for Trigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = Tokens::of(cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let on_open = self.on_open;
        let hover = if theme.dark {
            theme.input.opacity(0.64)
        } else {
            theme.accent.opacity(0.5)
        };
        h_flex()
            .id(self.id)
            .track_focus(&self.handle)
            .min_w_0()
            .max_w(px(288.))
            .min_h(px(28.))
            .items_center()
            .gap(px(6.))
            .px(px(9.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(background)
            .text_sm()
            .text_color(theme.foreground)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    on_open(true, window, cx);
                }
            })
            .children(self.icon)
            .child(div().min_w_0().truncate().child(self.label))
            .child(
                div()
                    .mr(px(-2.))
                    .flex_shrink_0()
                    .opacity(0.7)
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            )
    }
}

/// The icon of a choice: `size-3.5`.
fn icon(kind: &ChoiceIcon, cx: &App) -> Option<AnyElement> {
    let theme = Tokens::of(cx);
    match kind {
        ChoiceIcon::None => None,
        ChoiceIcon::Layers => Some(
            div()
                .flex_shrink_0()
                .child(Icon::new(Icons::Layers).size(px(14.)))
                .into_any_element(),
        ),
        ChoiceIcon::Machine(machine) => Some(
            div()
                .flex_shrink_0()
                .child(Icon::new(machine_icon(*machine)).size(px(14.)))
                .into_any_element(),
        ),
        ChoiceIcon::Project { initials, color } => {
            let color: gpui::Hsla = gpui::rgb(labs::avatar_color(*color, theme.dark)).into();
            let mono = cx.theme().mono_font_family.clone();
            Some(
                monogram(initials, color, 14., &mono)
                    .text_size(px(14. * 8.25 / 16.))
                    .into_any_element(),
            )
        }
    }
}

/// `ScopeOptionLabel` plus the selected check (`CheckIcon size-3.5`).
fn row(
    index: usize,
    id: &'static str,
    choice: &Choice,
    picked: bool,
    on_pick: &ChoiceHandler,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = Tokens::of(cx);
    let (accent, accent_foreground, muted) = (
        theme.accent,
        theme.accent_foreground,
        theme.muted_foreground,
    );
    let value = choice.value.clone();
    let on_pick = on_pick.clone();
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
        .hover(move |style| style.bg(accent).text_color(accent_foreground))
        .on_click(move |_, window, cx| on_pick(value.clone(), window, cx))
        .children(icon(&choice.icon, cx))
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .child(div().truncate().child(choice.label.clone()))
                .when(!choice.secondary.is_empty(), |text| {
                    text.child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(choice.secondary.clone()),
                    )
                }),
        )
        .when(choice.offline, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child("Offline"),
            )
        })
        .when(picked, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .child(Icon::new(Icons::Check).size(px(14.))),
            )
        })
}

/// The popup shell, as the select menu's: `dropdown-glass` on a `rounded-lg` `p-1` box.
fn shell(width: f32, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    let shadow = if theme.dark {
        (18., 44., 0.8)
    } else {
        (16., 40., 0.55)
    };
    v_flex()
        .w(px(width))
        .max_w_full()
        .p_1()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.foreground.opacity(0.1))
        .bg(theme.popover)
        .shadow(vec![BoxShadow {
            color: gpui::black().opacity(shadow.2),
            offset: point(px(0.), px(shadow.0)),
            blur_radius: px(shadow.1),
            spread_radius: px(-18.),
            inset: false,
        }])
}

pub(super) fn picker(spec: Picker, window: &mut Window, cx: &mut App) -> AnyElement {
    let id = ElementId::from(spec.id);
    let handle = tab_stop(&id, window, cx);
    let trigger = Trigger {
        id: id.clone(),
        label: spec.label.clone(),
        icon: spec.icon.as_ref().and_then(|kind| icon(kind, cx)),
        handle,
        open: spec.open,
        on_open: spec.on_open.clone(),
    };
    let on_change = spec.on_open.clone();
    let combobox = labs::searchable(spec.choices.len());
    let popover_id = ElementId::from((spec.id, 1usize));
    Popover::new(popover_id)
        .anchor(Anchor::TopLeft)
        .offset(px(4.))
        .open(spec.open)
        .p_0()
        .appearance(false)
        .on_open_change(move |opened, window, cx| on_change(*opened, window, cx))
        .trigger(trigger)
        .content(move |_, _, cx| {
            let mut all: Vec<Choice> = spec.all.iter().cloned().collect();
            all.extend(spec.choices.iter().cloned());
            let theme = Tokens::of(cx);
            let width = if combobox { 320. } else { 288. };
            let rows = |items: Vec<&Choice>| -> Vec<AnyElement> {
                let mut out = Vec::new();
                let has_all = spec.all.is_some();
                for (index, choice) in items.iter().enumerate() {
                    out.push(
                        row(
                            index,
                            spec.id,
                            choice,
                            choice.value == spec.selected,
                            &spec.on_pick,
                            cx,
                        )
                        .into_any_element(),
                    );
                    // The menu draws a separator under its "all" entry (`MenuSeparator`).
                    if !combobox && has_all && index == 0 {
                        out.push(
                            div()
                                .my_1()
                                .mx(px(-4.))
                                .h(px(1.))
                                .bg(theme.border)
                                .into_any_element(),
                        );
                    }
                }
                out
            };
            if !combobox {
                return shell(width, cx).children(rows(all.iter().collect()));
            }
            let shown = labs::filter(&all, &spec.query);
            let empty = shown.is_empty();
            shell(width, cx)
                .child(
                    div()
                        .px_2()
                        .pb_1()
                        .border_b_1()
                        .border_color(theme.border)
                        .child(
                            Input::new(&spec.search)
                                .appearance(false)
                                .small()
                                .prefix(Icon::new(IconName::Search).size(px(14.))),
                        ),
                )
                .child(
                    v_flex()
                        .id((spec.id, 2usize))
                        .max_h(px(288.))
                        .overflow_y_scroll()
                        .pt_1()
                        .children(rows(shown))
                        .when(empty, |list| {
                            list.child(
                                div()
                                    .py_2()
                                    .text_center()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("No matches found."),
                            )
                        }),
                )
        })
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Project,
    Environment,
}

/// The settings scope sentence and its state, shared by every tab that applies a scope
/// (`SettingsScopeContext`). The dialog owns one and draws it above the panel.
pub(super) struct ScopeBar {
    search: ScopeSearch,
    data: ScopeData,
    open: Option<Axis>,
    project_search: Entity<InputState>,
    environment_search: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl ScopeBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search projects"));
        let environment_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search environments"));
        let subscriptions = [&project_search, &environment_search]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            search: ScopeSearch::default(),
            data: ScopeData::default(),
            open: None,
            project_search,
            environment_search,
            _subscriptions: subscriptions,
        }
    }

    pub fn search(&self) -> &ScopeSearch {
        &self.search
    }

    pub fn data(&self) -> &ScopeData {
        &self.data
    }

    pub fn set_data(&mut self, data: ScopeData, cx: &mut Context<Self>) {
        if self.data != data {
            self.data = data;
            cx.notify();
        }
    }

    /// Closes an open picker; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let was = self.open.take().is_some();
        if was {
            cx.notify();
        }
        was
    }

    fn input(&self, axis: Axis) -> &Entity<InputState> {
        match axis {
            Axis::Project => &self.project_search,
            Axis::Environment => &self.environment_search,
        }
    }

    fn set_open(&mut self, axis: Axis, opened: bool, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.input(axis).clone();
        if opened {
            self.open = Some(axis);
            input.update(cx, |input, cx| input.set_value("", window, cx));
            let handle = input.focus_handle(cx);
            window.focus(&handle, cx);
        } else if self.open == Some(axis) {
            self.open = None;
        }
        cx.notify();
    }

    fn pick(&mut self, axis: Axis, value: &str, cx: &mut Context<Self>) {
        self.search = match axis {
            Axis::Project => self.search.select_project(value),
            Axis::Environment => self.search.select_environment(value),
        };
        self.open = None;
        cx.notify();
    }

    fn project_picker(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let selected = self
            .search
            .project
            .as_ref()
            .and_then(|key| self.data.projects.iter().find(|p| &p.key == key));
        let spec = Picker {
            id: "settings-scope-project",
            label: labs::project_label(&self.search, &self.data),
            icon: selected.map(|p| ChoiceIcon::Project {
                initials: p.initials.clone(),
                color: p.color,
            }),
            all: Some(Choice {
                value: ALL.to_owned(),
                label: "All projects".to_owned(),
                secondary: String::new(),
                offline: false,
                icon: ChoiceIcon::None,
            }),
            choices: labs::project_choices(&self.search, &self.data),
            selected: self
                .search
                .project
                .clone()
                .unwrap_or_else(|| ALL.to_owned()),
            open: self.open == Some(Axis::Project),
            search: self.project_search.clone(),
            query: self.project_search.read(cx).value().to_string(),
            on_open: {
                let view = view.clone();
                Rc::new(move |opened, window, cx| {
                    view.update(cx, |this, cx| {
                        this.set_open(Axis::Project, opened, window, cx)
                    })
                })
            },
            on_pick: Rc::new(move |value, _, cx| {
                view.update(cx, |this, cx| this.pick(Axis::Project, &value, cx))
            }),
        };
        picker(spec, window, cx)
    }

    fn environment_picker(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let selected = self
            .search
            .machine
            .as_ref()
            .and_then(|id| self.data.environments.iter().find(|e| &e.id == id));
        let spec = Picker {
            id: "settings-scope-environment",
            label: labs::environment_trigger_label(&self.search, &self.data),
            icon: selected.map(|e| ChoiceIcon::Machine(e.machine)),
            all: Some(Choice {
                value: ALL.to_owned(),
                label: "All environments".to_owned(),
                secondary: String::new(),
                offline: false,
                icon: ChoiceIcon::Layers,
            }),
            choices: labs::environment_choices(&self.search, &self.data),
            selected: self
                .search
                .machine
                .clone()
                .unwrap_or_else(|| ALL.to_owned()),
            open: self.open == Some(Axis::Environment),
            search: self.environment_search.clone(),
            query: self.environment_search.read(cx).value().to_string(),
            on_open: {
                let view = view.clone();
                Rc::new(move |opened, window, cx| {
                    view.update(cx, |this, cx| {
                        this.set_open(Axis::Environment, opened, window, cx)
                    })
                })
            },
            on_pick: Rc::new(move |value, _, cx| {
                view.update(cx, |this, cx| this.pick(Axis::Environment, &value, cx))
            }),
        };
        picker(spec, window, cx)
    }
}

impl Render for ScopeBar {
    /// `SettingsScopeSentence`: `px-3 text-sm text-muted-foreground`, the two halves `gap-1.5`.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = Tokens::of(cx).muted_foreground;
        let project = self.project_picker(window, cx);
        let environment = self.environment_picker(window, cx);
        h_flex()
            .min_w_0()
            .flex_wrap()
            .items_center()
            .gap_x(px(6.))
            .gap_y(px(4.))
            .px_3()
            .text_sm()
            .text_color(muted)
            .child(
                h_flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.))
                    .child(div().flex_shrink_0().child("Applying settings for"))
                    .child(project),
            )
            .child(
                h_flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.))
                    .child(div().flex_shrink_0().child(self.search.joiner()))
                    .child(environment),
            )
    }
}
