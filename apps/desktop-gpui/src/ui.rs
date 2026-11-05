use crate::connection::{Action, ActionSender};
use crate::projects::{self, EnvironmentProjects, MachineKind, ProjectEntry};
use crate::status::{ShellView, Status, ThreadRow};
use crate::wire::generated::ThreadsEntryStatus as ThreadStatus;
use gpui::{
    Anchor, AnyElement, App, Context, Entity, Focusable as _, FontWeight, Hsla, IntoElement,
    Keystroke, ParentElement, Render, RenderOnce, Styled, UniformListScrollHandle, Window, div,
    prelude::*, px, rgb, uniform_list,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Selectable, Sizable, TitleBar,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    kbd::Kbd,
    popover::Popover,
    tag::Tag,
    tooltip::Tooltip,
    v_flex,
};

const SIDEBAR_WIDTH: f32 = 300.;
const ROW_HEIGHT: f32 = 68.;

pub struct MainView {
    status: Status,
    search: Entity<InputState>,
    project_search: Entity<InputState>,
    composer: Entity<TextareaState>,
    list: UniformListScrollHandle,
    selected_project: Option<String>,
    picker_open: bool,
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
            selected_project: None,
            picker_open: false,
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
        let count = self.shell().map_or(0, |shell| shell.rows.len());
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
                        shell.rows[range]
                            .iter()
                            .map(|row| thread_row(row, muted, hover))
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
                    .child(
                        Breadcrumb::new()
                            .child(BreadcrumbItem::new(project.clone()))
                            .child(BreadcrumbItem::new("New thread")),
                    ),
            )
            .child(
                centered(cx)
                    .gap_6()
                    .child(self.hero(
                        &project,
                        selected_id(self, active).map(str::to_owned),
                        projects::with_remote_members(&shell.project_entries, &self.environments),
                        cx,
                    ))
                    .child(self.composer_box(cx)),
            )
            .into_any_element()
    }

    fn hero(
        &self,
        project: &str,
        active_id: Option<String>,
        entries: Vec<ProjectEntry>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let (actions, can_pin) = (self.actions.clone(), self.can_pin());
        let order = self.picker_order.clone();
        let search = self.project_search.clone();
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
            .trigger(ProjectTrigger::new(project))
            .content(move |_, _, cx| {
                let query = search.read(cx).value();
                let mut listed = projects::picker_entries(&entries, active_id.as_deref(), &query);
                if let Some(order) = &order {
                    listed = projects::order_while_open(listed, order);
                }
                let theme = cx.theme();
                let (dark, muted) = (theme.is_dark(), theme.muted_foreground);
                let (hover, selected) = (theme.list_hover, theme.list_active);
                let mono = theme.mono_font_family.clone();
                let border = theme.border;
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
                v_flex()
                    .w(px(400.))
                    .child(
                        h_flex().px_3().border_b_1().border_color(border).child(
                            Input::new(&search)
                                .appearance(false)
                                .prefix(Icon::new(IconName::Search)),
                        ),
                    )
                    .child(
                        v_flex()
                            .id("project-list")
                            .max_h(px(320.))
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
        v_flex()
            .items_center()
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .text_2xl()
                    .child("What should we build in")
                    .child(picker)
                    .child("?"),
            )
            .child(
                div()
                    .id("start-without-project")
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .cursor_pointer()
                    .hover(|style| style.underline())
                    .tooltip(|window, cx| {
                        Tooltip::element(|_, _| {
                            Kbd::new(Keystroke::parse("alt-cmd-n").expect("valid keystroke"))
                        })
                        .build(window, cx)
                    })
                    .child("or start without a project"),
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
                        Button::new("attach")
                            .ghost()
                            .small()
                            .icon(gpui_kit_assets::IconName::Paperclip)
                            .tooltip("Attach files")
                            .on_click(|_, _, _| {}),
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
#[derive(IntoElement)]
struct ProjectTrigger {
    name: String,
    open: bool,
}

impl ProjectTrigger {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            open: false,
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
        let tooltip = self.name.clone();
        div()
            .id("project")
            .ml_2()
            .min_w_0()
            .max_w_full()
            .truncate()
            .text_2xl()
            .cursor_pointer()
            .when(self.open, |name| name.underline())
            .hover(|style| style.underline())
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .child(self.name)
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
            .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
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
    Button::new(("pin", index))
        .ghost()
        .xsmall()
        .tooltip(projects::pin_tooltip(&entry.title, pinned))
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
        })
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
            updated_at: String::new(),
        }
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
