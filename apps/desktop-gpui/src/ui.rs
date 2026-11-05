use crate::assets::MESSAGE_SQUARE_DASHED;
use crate::connection::{Action, ActionSender};
use crate::projects::{self, EnvironmentProjects, MachineKind, ProjectEntry};
use crate::status::{Section, ShellView, Status, ThreadRow};
use crate::wire::generated::ThreadsEntryStatus as ThreadStatus;
use gpui::{
    Anchor, AnyElement, App, Bounds, Context, Entity, Focusable as _, FontWeight, Hsla,
    IntoElement, Keystroke, ParentElement, Pixels, Render, RenderOnce, Styled,
    UniformListScrollHandle, Window, canvas, div, prelude::*, px, rgb, uniform_list,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Selectable, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    kbd::Kbd,
    popover::Popover,
    tag::Tag,
    tooltip::Tooltip,
    v_flex,
};
use std::{cell::Cell, rc::Rc};

const SIDEBAR_WIDTH: f32 = 300.;
const ROW_HEIGHT: f32 = 68.;
const NO_PROJECT: &str = "No project";
const HEADLINE_SCRATCH: &str = "What should we work on?";
/// The web's `ComboboxPopup` cap, `max-h-[min(var(--available-height),23rem)]`.
const PICKER_MAX_HEIGHT: f32 = 368.;
/// `ComboboxSearchInput`: `pt-2.5`, a 26px input, `pb-1.5` and its 1px border.
const PICKER_SEARCH_HEIGHT: f32 = 43.;
/// The popover's 4px offset from the trigger plus the positioner's 4px viewport margin.
const PICKER_GAP: f32 = 8.;

pub struct MainView {
    status: Status,
    search: Entity<InputState>,
    project_search: Entity<InputState>,
    composer: Entity<TextareaState>,
    list: UniformListScrollHandle,
    /// Working, Snoozed and Settled shelves; the web starts all of them collapsed.
    shelves: [bool; 3],
    selected_project: Option<String>,
    picker_open: bool,
    /// Last painted trigger bounds, so the popup can shrink to fit below it as on the web.
    picker_trigger: Rc<Cell<Bounds<Pixels>>>,
    picker_order: Option<Vec<String>>,
    /// The other saved environments, by id.
    environments: Vec<EnvironmentProjects>,
    actions: ActionSender,
}

impl MainView {
    pub fn new(
        status: Status,
        actions: ActionSender,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search projects..."));
        cx.subscribe(&project_search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        Self {
            status,
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search threads")),
            project_search,
            composer: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Ask anything. @ files, $ skills, / commands")
                    .auto_grow(3, 10)
            }),
            list: UniformListScrollHandle::new(),
            shelves: [false; 3],
            selected_project: None,
            picker_open: false,
            picker_trigger: Rc::default(),
            picker_order: None,
            environments: Vec::new(),
            actions,
        }
    }

    pub fn set_status(&mut self, status: Status, cx: &mut Context<Self>) {
        if let Status::Environment(other) = status {
            match self.environments.iter_mut().find(|e| e.id == other.id) {
                Some(known) => *known = other,
                None => self.environments.push(other),
            }
            return cx.notify();
        }
        self.status = status;
        // Thread activity reorders recency on every delta, so the default must not follow it.
        if self.selected_project.is_none() {
            self.selected_project = self
                .shell()
                .and_then(|shell| active_project(None, shell))
                .map(|entry| entry.id.clone());
        }
        cx.notify();
    }

    fn shell(&self) -> Option<&ShellView> {
        match &self.status {
            Status::Connected { shell, .. } => shell.as_ref(),
            _ => None,
        }
    }

    fn can_pin(&self) -> bool {
        matches!(&self.status, Status::Connected { can_pin: true, .. })
    }

    fn picker_ids(&self) -> Option<Vec<String>> {
        let shell = self.shell()?;
        let active = active_project(self.selected_project.as_deref(), shell);
        let entries = &shell.project_entries;
        let listed = projects::picker_entries(entries, selected_id(self, active), "");
        Some(listed.into_iter().map(|entry| entry.key.clone()).collect())
    }

    fn environment(&self) -> String {
        match &self.status {
            Status::Connected { label, .. } => label.clone(),
            other => other.to_string(),
        }
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self
            .shell()
            .map_or(0, |shell| sidebar_items(&shell.rows, self.shelves).len());
        v_flex()
            .w(px(SIDEBAR_WIDTH))
            .flex_shrink_0()
            .h_full()
            .gap_2()
            .p_2()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(Input::new(&self.search).prefix(Icon::new(IconName::Search)))
            .child(
                uniform_list(
                    "threads",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let Some(shell) = this.shell() else {
                            return Vec::new();
                        };
                        let (muted, hover) = (cx.theme().muted_foreground, cx.theme().list_hover);
                        let items = sidebar_items(&shell.rows, this.shelves);
                        items[range]
                            .iter()
                            .map(|item| match *item {
                                SidebarItem::Row(index) => {
                                    thread_row(&shell.rows[index], muted, hover).into_any_element()
                                }
                                SidebarItem::Header {
                                    section,
                                    count,
                                    expanded,
                                } => {
                                    let shelf = shelf_index(section);
                                    shelf_header(section, count, expanded, muted, hover)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.shelves[shelf] = !this.shelves[shelf];
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                }
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.list)
                .flex_1()
                .min_h_0(),
            )
    }

    fn main_area(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(shell) = self.shell() else {
            return centered(cx)
                .text_xl()
                .child(self.status.to_string())
                .into_any_element();
        };
        let active = active_project(self.selected_project.as_deref(), shell);
        let scratch = active.is_some_and(|entry| entry.scratch);
        let project = active.map_or_else(|| "your project".to_owned(), |entry| entry.title.clone());
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .h(px(40.))
                    .px_4()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.breadcrumb(active, cx)),
            )
            .child(
                centered(cx)
                    .gap_6()
                    .child(self.hero(
                        &project,
                        scratch,
                        selected_id(self, active).map(str::to_owned),
                        projects::with_remote_members(&shell.project_entries, &self.environments),
                        cx,
                    ))
                    .child(self.composer_box(cx)),
            )
            .into_any_element()
    }

    fn breadcrumb(
        &self,
        active: Option<&ProjectEntry>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (foreground, muted, dark) = (theme.foreground, theme.muted_foreground, theme.is_dark());
        let mono = theme.mono_font_family.clone();
        let project = active.map(|entry| {
            let color: Hsla = rgb(entry.color_hex(dark)).into();
            let icon = if entry.scratch {
                Icon::empty()
                    .path(MESSAGE_SQUARE_DASHED)
                    .size(px(14.))
                    .text_color(color)
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(14.))
                    .rounded(px(3.5))
                    .bg(color.opacity(0.14))
                    .text_color(color)
                    .text_size(px(7.21875))
                    .font_weight(FontWeight::BOLD)
                    .font_family(mono)
                    .child(entry.initials.clone())
                    .into_any_element()
            };
            h_flex()
                .gap_3()
                .child(
                    h_flex()
                        .id("breadcrumb-project")
                        .gap(px(6.))
                        .min_w_0()
                        .text_color(muted)
                        .hover(move |style| style.text_color(foreground))
                        .child(icon)
                        .child(
                            div()
                                .max_w(px(160.))
                                .truncate()
                                .child(project_label(Some(entry))),
                        ),
                )
                .child(div().flex_shrink_0().text_color(muted).child("/"))
        });
        h_flex()
            .gap_3()
            .items_center()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .children(project)
            .child(div().text_color(foreground).child("New thread"))
    }

    fn hero(
        &self,
        project: &str,
        scratch: bool,
        active_id: Option<String>,
        entries: Vec<ProjectEntry>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let link_view = view.clone();
        let foreground = cx.theme().foreground;
        let (actions, can_pin) = (self.actions.clone(), self.can_pin());
        let scratch_id = self
            .shell()
            .and_then(|shell| scratch_entry(&shell.project_entries))
            .map(|entry| entry.id.clone());
        let order = self.picker_order.clone();
        let search = self.project_search.clone();
        let trigger_bounds = self.picker_trigger.clone();
        let picker = Popover::new("project-picker")
            .anchor(Anchor::TopCenter)
            .open(self.picker_open)
            .track_focus(&search.focus_handle(cx))
            .p_0()
            .on_open_change({
                let (view, search) = (view.clone(), search.clone());
                move |open, window, cx| {
                    search.update(cx, |state, cx| state.set_value("", window, cx));
                    view.update(cx, |this, cx| {
                        this.picker_open = *open;
                        this.picker_order = open.then(|| this.picker_ids()).flatten();
                        cx.notify();
                    });
                }
            })
            .trigger(ProjectTrigger::new(
                project,
                scratch,
                trigger_bounds.clone(),
            ))
            .content(move |_, window, cx| {
                let query = search.read(cx).value();
                let searching = search.focus_handle(cx).is_focused(window);
                // Shrinking keeps the positioner from flipping the popup over the headline.
                let below = window.viewport_size().height - trigger_bounds.get().bottom();
                let list_height =
                    (below - px(PICKER_GAP)).min(px(PICKER_MAX_HEIGHT)) - px(PICKER_SEARCH_HEIGHT);
                let mut listed = projects::picker_entries(&entries, active_id.as_deref(), &query);
                if let Some(order) = &order {
                    listed = projects::order_while_open(listed, order);
                }
                let theme = cx.theme();
                let (dark, muted) = (theme.is_dark(), theme.muted_foreground);
                let (hover, selected) = (theme.list_hover, theme.list_active);
                let mono = theme.mono_font_family.clone();
                let (border, ring) = (theme.border, theme.ring);
                let rows = listed.iter().enumerate().map(|(index, entry)| {
                    let (view, id) = (view.clone(), entry.target(active_id.as_deref()).to_owned());
                    let (actions, pin_ids) = (actions.clone(), entry.member_ids.clone());
                    let pinned = entry.pinned;
                    let color: Hsla = rgb(entry.color_hex(dark)).into();
                    let active = active_id.as_deref().is_some_and(|id| entry.contains(id));
                    h_flex()
                        .id(("project", index))
                        .group(PICKER_ROW_GROUP)
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .items_center()
                        .rounded_md()
                        .cursor_pointer()
                        .when(active, |row| row.bg(selected))
                        .hover(move |style| style.bg(hover))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .justify_center()
                                .size(px(20.))
                                .rounded(px(5.))
                                .bg(color.opacity(0.14))
                                .text_color(color)
                                .text_size(px(10.))
                                .font_weight(FontWeight::BOLD)
                                .font_family(mono.clone())
                                .child(entry.initials.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .truncate()
                                .child(entry.title.clone()),
                        )
                        .when(can_pin, |row| {
                            row.child(pin_button(index, entry, actions, pin_ids))
                        })
                        .when(!can_pin && pinned, |row| {
                            row.child(
                                Icon::new(gpui_kit_assets::IconName::Pin)
                                    .small()
                                    .text_color(muted),
                            )
                        })
                        .children(environment_badge(index, entry, muted))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.selected_project = Some(id.clone());
                                this.picker_open = false;
                                cx.notify();
                            });
                        })
                });
                // The web's `ComboboxPopup` (`w-72`, at most 23rem tall) and `ComboboxSearchInput`.
                v_flex()
                    .w(px(288.))
                    .rounded_lg()
                    .overflow_hidden()
                    .child(
                        div().px_3().pt(px(10.)).child(
                            h_flex()
                                .pb(px(6.))
                                .border_b_1()
                                .border_color(if searching { ring } else { border.opacity(0.7) })
                                .child(
                                    Input::new(&search)
                                        .appearance(false)
                                        .h(px(26.))
                                        .px_0()
                                        .prefix(
                                            Icon::new(IconName::Search)
                                                .size_4()
                                                .text_color(muted.opacity(0.55)),
                                        ),
                                ),
                        ),
                    )
                    .child(
                        v_flex()
                            .id("project-list")
                            .max_h(list_height.max(px(0.)))
                            .overflow_y_scroll()
                            .p_1()
                            .children(rows)
                            .when(listed.is_empty(), |list| {
                                list.child(
                                    div()
                                        .p_2()
                                        .text_sm()
                                        .text_color(muted)
                                        .child("No matching projects."),
                                )
                            }),
                    )
            });
        let (heading, below) = if scratch {
            let heading = h_flex()
                .items_center()
                .text_3xl()
                .text_color(foreground)
                .child(HEADLINE_SCRATCH)
                .into_any_element();
            (heading, picker.into_any_element())
        } else {
            let heading = h_flex()
                .items_center()
                .text_2xl()
                .child("What should we build in")
                .child(picker)
                .child("?")
                .into_any_element();
            let link = div()
                .id("start-without-project")
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().muted_foreground)
                .cursor_pointer()
                .hover(|style| style.underline().text_color(foreground))
                .tooltip(tooltip(Kbd::format(
                    &Keystroke::parse("alt-cmd-n").expect("valid keystroke"),
                )))
                .on_click(move |_, _, cx| {
                    let Some(id) = scratch_id.clone() else { return };
                    link_view.update(cx, |this, cx| {
                        this.selected_project = Some(id);
                        this.picker_open = false;
                        cx.notify();
                    });
                })
                .child("or start without a project")
                .into_any_element();
            (heading, link)
        };
        v_flex().items_center().child(heading).child(
            h_flex()
                .mt_2()
                .h_6()
                .items_center()
                .justify_center()
                .text_sm()
                .child(below),
        )
    }

    fn composer_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w(px(720.))
            .max_w_full()
            .p_3()
            .gap_2()
            .rounded_xl()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .child(
                Textarea::new(&self.composer)
                    .appearance(false)
                    .bordered(false),
            )
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .id("attach-tooltip")
                            .tooltip(tooltip("Attach files"))
                            .child(
                                Button::new("attach")
                                    .ghost()
                                    .small()
                                    .icon(gpui_kit_assets::IconName::Paperclip)
                                    .on_click(|_, _, _| {}),
                            ),
                    )
                    .child(
                        Button::new("send")
                            .primary()
                            .rounded_full()
                            .icon(IconName::ArrowUp)
                            .on_click(|_, _, _| {}),
                    ),
            )
    }
}

/// The project name as plain heading text that underlines on hover and while its picker is open.
/// Under a scratch draft it is the `InlineButton` `picker` tone at `text-sm`, `font-medium`.
#[derive(IntoElement)]
struct ProjectTrigger {
    name: String,
    scratch: bool,
    open: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl ProjectTrigger {
    fn new(name: &str, scratch: bool, bounds: Rc<Cell<Bounds<Pixels>>>) -> Self {
        Self {
            name: if scratch { NO_PROJECT } else { name }.to_owned(),
            scratch,
            open: false,
            bounds,
        }
    }
}

impl Selectable for ProjectTrigger {
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

impl RenderOnce for ProjectTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let name = self.name.clone();
        let bounds = self.bounds;
        let trigger = div()
            .id("project")
            .relative()
            .child(
                canvas(move |rect, _, _| bounds.set(rect), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .min_w_0()
            .max_w_full()
            .truncate()
            .cursor_pointer()
            .when(self.open, |name| name.underline())
            .hover(|style| style.underline());
        if self.scratch {
            trigger.font_weight(FontWeight::MEDIUM).child(self.name)
        } else {
            trigger
                .ml_2()
                .text_2xl()
                .tooltip(tooltip(name))
                .child(self.name)
        }
    }
}

fn project_label(entry: Option<&ProjectEntry>) -> String {
    match entry {
        Some(entry) if entry.scratch => NO_PROJECT.to_owned(),
        Some(entry) => entry.title.clone(),
        None => String::new(),
    }
}

fn scratch_entry(entries: &[ProjectEntry]) -> Option<&ProjectEntry> {
    entries.iter().find(|entry| entry.scratch)
}

/// The web's `TooltipPopup` sizing: `text-xs`, `px-2 py-1`, `rounded-md`.
fn tooltip(
    text: impl Into<gpui::SharedString>,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |window, cx| {
        Tooltip::new(text.clone())
            .text_xs()
            .py_1()
            .rounded(px(6.))
            .build(window, cx)
    }
}

fn machine_icon(kind: MachineKind) -> gpui_kit_assets::IconName {
    use gpui_kit_assets::IconName as Icons;
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

/// The web's `ProjectEnvironmentBadge`: a machine icon with "Also on <labels>", right of the pin.
fn environment_badge(
    index: usize,
    entry: &ProjectEntry,
    muted: Hsla,
) -> Option<impl IntoElement + use<>> {
    let text = projects::badge_text(&entry.remote)?;
    let machine = projects::badge_machine(&entry.remote)?;
    Some(
        div()
            .id(("environment", index))
            .flex_shrink_0()
            .tooltip(tooltip(text))
            .child(Icon::new(machine_icon(machine)).small().text_color(muted)),
    )
}

const PICKER_ROW_GROUP: &str = "project-row";

/// Shown on row hover, and always for a pinned project. A click toggles the pin and, because
/// `Button` stops propagation, does not select the row.
fn pin_button(
    index: usize,
    entry: &ProjectEntry,
    actions: ActionSender,
    project_ids: Vec<String>,
) -> impl IntoElement + use<> {
    let pinned = entry.pinned;
    let icon = |name| Icon::new(name).small();
    let hint = tooltip(projects::pin_tooltip(&entry.title, pinned));
    let button = Button::new(("pin", index))
        .ghost()
        .xsmall()
        .when(!pinned, |button| {
            button
                .invisible()
                .group_hover(PICKER_ROW_GROUP, |style| style.visible())
        })
        .child(if pinned {
            div()
                .group("pin-icon")
                .child(
                    div()
                        .group_hover("pin-icon", |style| style.invisible())
                        .child(icon(gpui_kit_assets::IconName::Pin)),
                )
                .child(
                    div()
                        .absolute()
                        .invisible()
                        .group_hover("pin-icon", |style| style.visible())
                        .child(icon(gpui_kit_assets::IconName::PinOff)),
                )
        } else {
            div().child(icon(gpui_kit_assets::IconName::Pin))
        })
        .on_click(move |_, _, _| {
            // Clients write every member of the group together.
            for project_id in &project_ids {
                let action = Action::SetProjectPinned {
                    project_id: project_id.clone(),
                    pinned: !pinned,
                };
                if actions.try_send(action).is_err() {
                    eprintln!("[ui] could not queue the pin request");
                }
            }
        });
    div().id(("pin-tooltip", index)).tooltip(hint).child(button)
}

/// The picked project id while its row still exists, else the active row's target.
fn selected_id<'a>(view: &'a MainView, active: Option<&'a ProjectEntry>) -> Option<&'a str> {
    view.selected_project
        .as_deref()
        .filter(|id| active.is_some_and(|entry| entry.contains(id)))
        .or(active.map(|entry| entry.id.as_str()))
}

/// The picked project while it still exists, else the newest thread's project, else the first listed.
fn active_project<'a>(selected: Option<&str>, shell: &'a ShellView) -> Option<&'a ProjectEntry> {
    let by_id = |id: &str| {
        shell
            .project_entries
            .iter()
            .find(|entry| entry.contains(id))
    };
    selected
        .and_then(by_id)
        .or_else(|| {
            shell
                .rows
                .iter()
                .filter_map(|row| by_id(&row.project_id))
                .find(|entry| !entry.scratch)
        })
        .or_else(|| shell.project_entries.iter().find(|entry| !entry.scratch))
}

fn centered(cx: &Context<MainView>) -> gpui::Div {
    v_flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .px_6()
        .items_center()
        .justify_center()
        .text_color(cx.theme().foreground)
}

fn status_tag(status: ThreadStatus) -> Option<Tag> {
    let (tag, label) = match status {
        ThreadStatus::Running | ThreadStatus::Starting => (Tag::info(), "Running"),
        ThreadStatus::Waiting => (Tag::warning(), "Waiting"),
        ThreadStatus::Queued | ThreadStatus::Preparing => (Tag::secondary(), "Queued"),
        ThreadStatus::Failed => (Tag::danger(), "Failed"),
        _ => return None,
    };
    Some(tag.small().child(label))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidebarItem {
    Row(usize),
    Header {
        section: Section,
        count: usize,
        expanded: bool,
    },
}

fn shelf_index(section: Section) -> usize {
    match section {
        Section::Working => 0,
        Section::Snoozed => 1,
        _ => 2,
    }
}

/// The web's default list: pinned and active rows, then a header per shelf (Working and
/// Snoozed only when non-empty, Settled always) with its rows only while expanded.
fn sidebar_items(rows: &[ThreadRow], expanded: [bool; 3]) -> Vec<SidebarItem> {
    if rows.is_empty() {
        return Vec::new();
    }
    let mut items = Vec::with_capacity(rows.len() + 3);
    for (index, row) in rows.iter().enumerate() {
        if matches!(row.section, Section::Pinned | Section::Active) {
            items.push(SidebarItem::Row(index));
        }
    }
    for section in [Section::Working, Section::Snoozed, Section::Settled] {
        let count = rows.iter().filter(|row| row.section == section).count();
        if count == 0 && section != Section::Settled {
            continue;
        }
        let expanded = expanded[shelf_index(section)];
        items.push(SidebarItem::Header {
            section,
            count,
            expanded,
        });
        if expanded {
            let shelf = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.section == section);
            items.extend(shelf.map(|(index, _)| SidebarItem::Row(index)));
        }
    }
    items
}

fn shelf_header(
    section: Section,
    count: usize,
    expanded: bool,
    muted: Hsla,
    hover: Hsla,
) -> gpui::Stateful<gpui::Div> {
    let label = match section {
        Section::Working => "Working",
        Section::Snoozed => "Snoozed",
        _ => "Settled",
    };
    let chevron = if expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    h_flex()
        .id(label)
        .h(px(ROW_HEIGHT))
        .w_full()
        .px_2()
        .gap_2()
        .items_end()
        .pb_2()
        .rounded_md()
        .text_xs()
        .text_color(muted)
        .hover(|style| style.bg(hover))
        .child(Icon::new(chevron).small())
        .child(div().flex_1().child(label))
        .child(count.to_string())
}

fn thread_row(row: &ThreadRow, muted: Hsla, hover: Hsla) -> impl IntoElement + use<> {
    v_flex()
        .id(row.id.clone())
        .h(px(ROW_HEIGHT))
        .w_full()
        .overflow_hidden()
        .px_2()
        .py_1()
        .justify_center()
        .rounded_md()
        .hover(|style| style.bg(hover))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .truncate()
                .child(row.project.clone()),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .truncate()
                        .child(row.title.clone()),
                )
                .children(status_tag(row.status).map(|tag| div().flex_shrink_0().child(tag))),
        )
        .children(row.branch.as_ref().map(|branch| {
            div()
                .text_xs()
                .text_color(muted)
                .truncate()
                .child(branch.clone())
        }))
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child("T3 Code").child(
                    div()
                        .ml_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.environment()),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(self.main_area(cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, scratch: bool) -> ProjectEntry {
        ProjectEntry {
            key: id.to_owned(),
            id: id.to_owned(),
            member_ids: vec![id.to_owned()],
            title: id.to_owned(),
            initials: "XX".to_owned(),
            color: 0,
            pinned: false,
            scratch,
            logical_key: None,
            remote: Vec::new(),
        }
    }

    fn row(project_id: &str) -> ThreadRow {
        ThreadRow {
            id: project_id.to_owned(),
            project_id: project_id.to_owned(),
            project: project_id.to_owned(),
            title: String::new(),
            branch: None,
            status: ThreadStatus::Idle,
            section: Section::Active,
        }
    }

    #[test]
    fn sidebar_items_keep_shelves_collapsed_until_expanded() {
        let in_section = |id: &str, section| ThreadRow { section, ..row(id) };
        let rows = vec![
            in_section("p", Section::Pinned),
            in_section("a", Section::Active),
            in_section("w", Section::Working),
            in_section("s", Section::Settled),
        ];
        let header = |section, count, expanded| SidebarItem::Header {
            section,
            count,
            expanded,
        };
        assert_eq!(
            sidebar_items(&rows, [false; 3]),
            [
                SidebarItem::Row(0),
                SidebarItem::Row(1),
                header(Section::Working, 1, false),
                header(Section::Settled, 1, false),
            ]
        );
        assert_eq!(
            sidebar_items(&rows, [true, false, true]),
            [
                SidebarItem::Row(0),
                SidebarItem::Row(1),
                header(Section::Working, 1, true),
                SidebarItem::Row(2),
                header(Section::Settled, 1, true),
                SidebarItem::Row(3),
            ]
        );
        assert!(sidebar_items(&[], [true; 3]).is_empty());
    }

    #[test]
    fn scratch_reads_as_no_project_and_is_found_in_the_entries() {
        let mut none = entry("none", true);
        none.title = "Whatever the server titled it".to_owned();
        assert_eq!(project_label(Some(&none)), "No project");
        assert_eq!(project_label(Some(&entry("alpha", false))), "alpha");
        assert_eq!(project_label(None), "");
        let entries = [entry("alpha", false), none];
        assert_eq!(scratch_entry(&entries).map(|e| e.id.as_str()), Some("none"));
        assert!(scratch_entry(&entries[..1]).is_none());
        assert_eq!(
            ProjectTrigger::new("alpha", true, Rc::default()).name,
            "No project"
        );
        assert_eq!(
            ProjectTrigger::new("alpha", false, Rc::default()).name,
            "alpha"
        );
    }

    #[test]
    fn active_project_prefers_a_live_pick_then_the_newest_real_project() {
        let shell = ShellView {
            project_entries: vec![
                entry("alpha", false),
                entry("none", true),
                entry("beta", false),
            ],
            rows: vec![row("none"), row("gone"), row("beta")],
            ..ShellView::default()
        };
        let id = |selected| active_project(selected, &shell).map(|e| e.id.as_str());
        assert_eq!(id(Some("alpha")), Some("alpha"));
        assert_eq!(id(Some("gone")), Some("beta"));
        assert_eq!(id(None), Some("beta"));
        let empty = ShellView {
            project_entries: vec![entry("none", true), entry("alpha", false)],
            ..ShellView::default()
        };
        assert_eq!(
            active_project(None, &empty).map(|e| e.id.as_str()),
            Some("alpha")
        );
    }
}
