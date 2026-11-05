use crate::status::{ShellView, Status, ThreadRow};
use crate::wire::generated::ThreadsEntryStatus as ThreadStatus;
use gpui::{
    AnyElement, Context, Entity, Hsla, IntoElement, ParentElement, Render, Styled,
    UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, TitleBar,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    tag::Tag,
    v_flex,
};

const SIDEBAR_WIDTH: f32 = 300.;
const ROW_HEIGHT: f32 = 68.;

pub struct MainView {
    status: Status,
    search: Entity<InputState>,
    composer: Entity<TextareaState>,
    list: UniformListScrollHandle,
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
        let project = shell
            .first_project
            .clone()
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
                    .child(
                        div()
                            .text_2xl()
                            .child(format!("What should we build in {project}?")),
                    )
                    .child(self.composer_box(cx)),
            )
            .into_any_element()
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
