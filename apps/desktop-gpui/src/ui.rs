use crate::status::{ShellView, Status, ThreadRow};
use crate::wire::generated::ThreadsEntryStatus as ThreadStatus;
use gpui::{
    Anchor, AnyElement, Context, Entity, Hsla, IntoElement, Keystroke, ParentElement, Render,
    Styled, UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, TitleBar,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
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
    composer: Entity<TextareaState>,
    list: UniformListScrollHandle,
    selected_project: Option<String>,
    picker_open: bool,
}

impl MainView {
    pub fn new(status: Status, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            status,
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search threads")),
            composer: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Ask anything. @ files, $ skills, / commands")
                    .auto_grow(3, 10)
            }),
            list: UniformListScrollHandle::new(),
            selected_project: None,
            picker_open: false,
        }
    }

    pub fn set_status(&mut self, status: Status, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    fn shell(&self) -> Option<&ShellView> {
        match &self.status {
            Status::Connected { shell, .. } => shell.as_ref(),
            _ => None,
        }
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
        let project = active_project(self.selected_project.as_deref(), shell)
            .unwrap_or_else(|| "your project".to_owned());
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
                    .child(self.hero(&project, shell.project_titles.clone(), cx))
                    .child(self.composer_box(cx)),
            )
            .into_any_element()
    }

    fn hero(&self, project: &str, titles: Vec<String>, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let picker = Popover::new("project-picker")
            .anchor(Anchor::TopCenter)
            .open(self.picker_open)
            .on_open_change({
                let view = view.clone();
                move |open, _, cx| {
                    view.update(cx, |this, cx| {
                        this.picker_open = *open;
                        cx.notify();
                    });
                }
            })
            .trigger(
                Button::new("project")
                    .ghost()
                    .tooltip(project.to_owned())
                    .child(div().text_2xl().child(project.to_owned())),
            )
            .content(move |_, _, cx| {
                let hover = cx.theme().list_hover;
                v_flex()
                    .id("project-list")
                    .w(px(240.))
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .children(titles.iter().enumerate().map(|(index, title)| {
                        let (view, title) = (view.clone(), title.clone());
                        div()
                            .id(("project", index))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_sm()
                            .truncate()
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover))
                            .child(title.clone())
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.selected_project = Some(title.clone());
                                    this.picker_open = false;
                                    cx.notify();
                                });
                            })
                    }))
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
                h_flex().justify_end().child(
                    Button::new("send")
                        .primary()
                        .rounded_full()
                        .icon(IconName::ArrowUp)
                        .on_click(|_, _, _| {}),
                ),
            )
    }
}

/// The picked project while it still exists, else the newest thread's project.
fn active_project(selected: Option<&str>, shell: &ShellView) -> Option<String> {
    selected
        .filter(|title| shell.project_titles.iter().any(|known| known == title))
        .map(str::to_owned)
        .or_else(|| {
            shell
                .rows
                .iter()
                .map(|row| &row.project)
                .find(|project| !project.is_empty())
                .cloned()
        })
        .or_else(|| shell.project_titles.first().cloned())
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

    fn row(project: &str) -> ThreadRow {
        ThreadRow {
            id: project.to_owned(),
            project: project.to_owned(),
            title: String::new(),
            branch: None,
            status: ThreadStatus::Idle,
            updated_at: String::new(),
        }
    }

    #[test]
    fn active_project_prefers_a_live_pick_then_the_newest_thread() {
        let shell = ShellView {
            project_titles: vec!["alpha".into(), "beta".into()],
            rows: vec![row(""), row("beta")],
            ..ShellView::default()
        };
        assert_eq!(
            active_project(Some("alpha"), &shell).as_deref(),
            Some("alpha")
        );
        assert_eq!(
            active_project(Some("gone"), &shell).as_deref(),
            Some("beta")
        );
        let empty = ShellView {
            project_titles: vec!["alpha".into()],
            ..ShellView::default()
        };
        assert_eq!(active_project(None, &empty).as_deref(), Some("alpha"));
    }
}
