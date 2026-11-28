//! The web's `PullRequestsPage`: the pull requests of every project on this connection, a state
//! filter, and the detail of the selected one beside the list. The web's search, involvement and
//! label filters, grouping, stack layers, bulk actions and paging are not here.

use super::pull_request_panel::{PullRequestPanel, unavailable_view};
use super::*;
use crate::pull_request::{self, List, ListEntry};
use crate::shell::timestamp_ms;
use crate::status::CallEvent;
use gpui::{UniformListScrollHandle, uniform_list};
use std::{
    ops::Range,
    time::{SystemTime, UNIX_EPOCH},
};

/// Call tokens, above every other range.
pub const TOKEN_BASE: u64 = 1 << 49;
/// One page of the first read; the web pages further with the server's cursors.
const LIMIT: u32 = 100;
const ROW_HEIGHT: f32 = 56.;
const DETAIL_WIDTH: f32 = 480.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Filter {
    Open,
    Closed,
    Merged,
    All,
}

impl Filter {
    const ALL: [Self; 4] = [Self::All, Self::Open, Self::Closed, Self::Merged];

    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Closed => "Closed",
            Self::Merged => "Merged",
            Self::All => "All",
        }
    }

    fn wire(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
            Self::All => "all",
        }
    }
}

enum Load {
    Loading,
    Failed { message: String, refreshing: bool },
    Loaded(List),
}

pub struct PullRequestsPage {
    actions: ActionSender,
    detail: Entity<PullRequestPanel>,
    filter: Filter,
    loads: HashMap<Filter, Load>,
    tokens: HashMap<u64, Filter>,
    next_token: u64,
    selected: Option<ListEntry>,
    scroll: UniformListScrollHandle,
}

impl PullRequestsPage {
    pub fn new(actions: ActionSender, detail: Entity<PullRequestPanel>) -> Self {
        Self {
            actions,
            detail,
            filter: Filter::Open,
            loads: HashMap::new(),
            tokens: HashMap::new(),
            next_token: TOKEN_BASE,
            selected: None,
            scroll: UniformListScrollHandle::new(),
        }
    }

    /// Reads the current filter the first time the page opens.
    pub fn open(&mut self, cx: &mut Context<Self>) {
        if !self.loads.contains_key(&self.filter) {
            self.request(self.filter);
        }
        cx.notify();
    }

    /// A reconnect drops what the old connection answered.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.loads.clear();
        self.tokens.clear();
        self.request(self.filter);
        cx.notify();
    }

    /// Drops answers without asking again while the page is closed.
    pub fn forget(&mut self) {
        self.loads.clear();
        self.tokens.clear();
    }

    fn request(&mut self, filter: Filter) {
        self.tokens.retain(|_, pending| *pending != filter);
        self.next_token += 1;
        let token = self.next_token;
        self.tokens.insert(token, filter);
        let previous = self.loads.remove(&filter);
        self.loads.insert(
            filter,
            match previous {
                Some(Load::Failed { message, .. }) => Load::Failed {
                    message,
                    refreshing: true,
                },
                Some(loaded @ Load::Loaded(_)) => loaded,
                _ => Load::Loading,
            },
        );
        let action = Action::Call {
            token,
            call: Call::PullRequestsList,
            payload: pull_request::list_payload(filter.wire(), &[], LIMIT),
        };
        if self.actions.try_send(action).is_err() {
            self.tokens.remove(&token);
            self.loads.insert(
                filter,
                Load::Failed {
                    message: "Could not send the request".to_owned(),
                    refreshing: false,
                },
            );
            eprintln!("[pull-requests] could not send the request");
        }
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        let Some(filter) = self.tokens.get(&token).copied() else {
            return;
        };
        let load = match event {
            CallEvent::Progress(_) => return,
            CallEvent::Done(value) => match List::decode(&value) {
                Some(list) => Load::Loaded(list),
                None => Load::Failed {
                    message: "The server sent a list this app cannot read.".to_owned(),
                    refreshing: false,
                },
            },
            CallEvent::Failed(message) => Load::Failed {
                message,
                refreshing: false,
            },
        };
        self.tokens.remove(&token);
        self.loads.insert(filter, load);
        cx.notify();
    }

    fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        self.filter = filter;
        if !self.loads.contains_key(&filter) {
            self.request(filter);
        }
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.request(self.filter);
        cx.notify();
    }

    fn select(&mut self, entry: ListEntry, cx: &mut Context<Self>) {
        let (reference, url) = (entry.reference(), Some(entry.url.clone()));
        self.selected = Some(entry);
        self.detail
            .update(cx, |detail, cx| detail.show(reference, url, cx));
        cx.notify();
    }

    fn row(
        &self,
        entry: &ListEntry,
        index: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = cx.theme();
        let (muted, foreground, accent, dark) = (
            theme.muted_foreground,
            theme.foreground,
            theme.accent,
            theme.is_dark(),
        );
        let state = pull_request::badge_state(entry.state, entry.is_draft);
        let tone = pr_color(state, dark).unwrap_or(muted);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis() as i64);
        let selected = self
            .selected
            .as_ref()
            .is_some_and(|chosen| chosen.reference() == entry.reference());
        let meta = [
            Some(entry.repository.clone()),
            entry.author.as_ref().map(|author| author.login.clone()),
            timestamp_ms(&entry.updated_at)
                .map(|then| format!("updated {}", pull_request::relative_age(then, now))),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
        let chosen = entry.clone();
        v_flex()
            .id(("pull-request-row", index))
            .h(px(ROW_HEIGHT))
            .px_4()
            .gap_1()
            .justify_center()
            .cursor_pointer()
            .when(selected, |row| row.bg(accent))
            .hover(move |style| style.bg(accent))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Icon::new(pr_state_icon(state))
                            .size(px(14.))
                            .flex_shrink_0()
                            .text_color(tone),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .text_color(foreground)
                            .child(entry.title.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("#{}", entry.number)),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .pl(px(22.))
                    .text_xs()
                    .text_color(muted)
                    .child(div().min_w_0().flex_1().truncate().child(meta))
                    .child(super::right_panel::stat(entry.additions, entry.deletions)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select(chosen.clone(), cx)))
    }

    fn note(text: impl Into<SharedString>, cx: &App) -> gpui::Div {
        div()
            .p_4()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(text.into())
    }

    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.loads.get(&self.filter) {
            None | Some(Load::Loading) => {
                Self::note("Loading pull requests...", cx).into_any_element()
            }
            Some(Load::Failed {
                message,
                refreshing,
            }) => {
                let failure = pull_request::Unavailable {
                    title: pull_request::UNAVAILABLE_TITLE.to_owned(),
                    error: message.clone(),
                };
                let retry = cx.listener(|this, _, _, cx| this.refresh(cx));
                unavailable_view(failure, None, *refreshing, retry, cx).into_any_element()
            }
            Some(Load::Loaded(list)) => {
                let muted = cx.theme().muted_foreground;
                let errors = list.errors.iter().map(|error| {
                    div()
                        .px_4()
                        .py_1()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{}: {}", error.project_title, error.message))
                });
                let body = if list.entries.is_empty() {
                    Self::note("No pull requests.", cx).into_any_element()
                } else {
                    let entries = list.entries.clone();
                    uniform_list(
                        "pull-request-rows",
                        entries.len(),
                        cx.processor(move |this, range: Range<usize>, _, cx| {
                            range
                                .map(|ix| this.row(&entries[ix], ix, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .flex_1()
                    .min_h_0()
                    .into_any_element()
                };
                v_flex()
                    .size_full()
                    .min_h_0()
                    .children(errors)
                    .child(body)
                    .when(list.truncated, |column| {
                        column.child(Self::note(
                            "Narrow your search to find more pull requests.".to_owned(),
                            cx,
                        ))
                    })
                    .into_any_element()
            }
        }
    }
}

impl Render for PullRequestsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, foreground, muted) = (theme.border, theme.foreground, theme.muted_foreground);
        let filters = Filter::ALL.into_iter().enumerate().map(|(index, filter)| {
            let selected = filter == self.filter;
            h_flex()
                .id(("pull-request-filter", index))
                .h_6()
                .px_2()
                .rounded(px(6.))
                .cursor_pointer()
                .text_xs()
                .text_color(if selected { foreground } else { muted })
                .when(selected, |tab| tab.bg(foreground.opacity(0.06)))
                .hover(move |style| style.bg(foreground.opacity(0.06)))
                .child(filter.label())
                .on_click(cx.listener(move |this, _, _, cx| this.set_filter(filter, cx)))
        });
        let strip = h_flex()
            .h(px(40.))
            .flex_shrink_0()
            .px_4()
            .gap_1()
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .mr_2()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Pull requests"),
            )
            .children(filters)
            .child(div().flex_1())
            .child(
                header_icon_button(
                    "refresh-pull-requests",
                    Icons::RefreshCw,
                    "Refresh pull requests".into(),
                    true,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            );
        let detail = self.selected.is_some().then(|| {
            v_flex()
                .w(px(DETAIL_WIDTH))
                .h_full()
                .flex_shrink_0()
                .border_l_1()
                .border_color(border)
                .child(
                    h_flex().h_8().flex_shrink_0().px_2().justify_end().child(
                        ghost_icon_button(
                            "close-pull-request-detail",
                            Icon::new(IconName::Close).size_4(),
                            "Close".into(),
                            true,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.selected = None;
                            cx.notify();
                        })),
                    ),
                )
                .child(div().flex_1().min_h_0().child(self.detail.clone()))
        });
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(cx.theme().background)
            .child(strip)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex_1().min_w_0().h_full().child(self.list(cx)))
                    .children(detail),
            )
    }
}
