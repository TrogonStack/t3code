//! The web's `pull-request` right panel surface (`PullRequestDetailPanel.tsx`): the detail of one
//! pull request, read through the window's connection. A failed read shows
//! `PullRequestsUnavailableState`.

use super::*;
use crate::pull_request::{self, CheckStatus, Detail, Reference};
use crate::shell::timestamp_ms;
use crate::status::CallEvent;
use gpui_component::Disableable as _;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Call tokens, above every other range.
pub const TOKEN_BASE: u64 = 1 << 48;

/// What the detail read has produced for one pull request.
enum Load {
    Loading,
    /// `refreshing` while a retry is on its way, like `refreshing={detailQuery.isPending}`.
    Failed {
        message: String,
        refreshing: bool,
    },
    Loaded(Value),
}

/// The pull request on screen, with the link the host's own page opens at.
struct Shown {
    reference: Reference,
    url: Option<String>,
}

pub struct PullRequestPanel {
    actions: ActionSender,
    loads: HashMap<String, Load>,
    tokens: HashMap<u64, String>,
    next_token: u64,
    shown: Option<Shown>,
}

impl PullRequestPanel {
    pub fn new(actions: ActionSender) -> Self {
        Self {
            actions,
            loads: HashMap::new(),
            tokens: HashMap::new(),
            next_token: TOKEN_BASE,
            shown: None,
        }
    }

    /// Shows `reference`, reading it the first time.
    pub fn show(&mut self, reference: Reference, url: Option<String>, cx: &mut Context<Self>) {
        let key = reference.key();
        let same = self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.reference == reference);
        if !same {
            self.shown = Some(Shown {
                reference: reference.clone(),
                url,
            });
            cx.notify();
        }
        if !self.loads.contains_key(&key) {
            self.request(&reference);
            cx.notify();
        }
    }

    /// A reconnect drops what the old connection answered.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.loads.clear();
        self.tokens.clear();
        if let Some(shown) = &self.shown {
            let reference = shown.reference.clone();
            self.request(&reference);
        }
        cx.notify();
    }

    fn request(&mut self, reference: &Reference) {
        let key = reference.key();
        self.tokens.retain(|_, pending| *pending != key);
        self.next_token += 1;
        let token = self.next_token;
        self.tokens.insert(token, key.clone());
        let previous = self.loads.remove(&key);
        self.loads.insert(
            key.clone(),
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
            call: Call::PullRequestsDetail,
            payload: reference.fresh_payload(),
        };
        if self.actions.try_send(action).is_err() {
            self.tokens.remove(&token);
            self.loads.insert(
                key,
                Load::Failed {
                    message: "Could not send the request".to_owned(),
                    refreshing: false,
                },
            );
            eprintln!("[pull-request] could not send the request");
        }
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        let Some(key) = self.tokens.remove(&token) else {
            return;
        };
        let load = match event {
            CallEvent::Done(value) => Load::Loaded(value),
            CallEvent::Failed(message) => Load::Failed {
                message,
                refreshing: false,
            },
            CallEvent::Progress(_) => return,
        };
        self.loads.insert(key, load);
        cx.notify();
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let Some(reference) = self.shown.as_ref().map(|shown| shown.reference.clone()) else {
            return;
        };
        self.request(&reference);
        cx.notify();
    }

    fn section(title: &'static str, cx: &App) -> gpui::Div {
        div()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground)
            .child(title)
    }

    /// `PullRequestDetailPanel`'s summary: the header, labels, reviewers, checks and description.
    /// The web's Timeline, Code and Guide tabs, its actions and its composers are not here.
    fn detail(detail: &Detail, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, foreground, dark) = (theme.muted_foreground, theme.foreground, theme.is_dark());
        let state = detail.badge_state();
        let tone = pr_color(state, dark).unwrap_or(muted);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis() as i64);
        let updated = timestamp_ms(&detail.updated_at)
            .map(|then| format!("updated {}", pull_request::relative_age(then, now)));
        let author = detail.author.as_ref().map(|author| author.login.clone());
        let meta = [author, updated].into_iter().flatten().collect::<Vec<_>>();
        let url = detail.url.clone();
        let header = v_flex()
            .gap_2()
            .px_4()
            .pt_1()
            .pb_4()
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .child(detail.title.clone()),
                    )
                    .child(
                        ghost_icon_button(
                            "pull-request-open-host",
                            Icon::new(Icons::ExternalLink).size_4(),
                            pull_request::open_label(&detail.url).into(),
                            true,
                            cx,
                        )
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                    ),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_x_2()
                    .text_xs()
                    .text_color(muted)
                    .child(
                        h_flex()
                            .gap_1()
                            .text_color(tone)
                            .child(Icon::new(pr_state_icon(state)).size(px(14.)))
                            .child(detail.state_label()),
                    )
                    .child(meta.join(" \u{b7} ")),
            )
            .children(detail.conflict().map(|text| {
                h_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(gpui::rgb(0xef4444))
                    .child(Icon::new(Icons::CircleAlert).size(px(14.)))
                    .child(text)
            }))
            .child(
                h_flex()
                    .mt_2()
                    .gap_2()
                    .text_xs()
                    .text_color(muted)
                    .child(
                        h_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_1p5()
                            .font_family(theme.mono_font_family.clone())
                            .child(div().truncate().child(detail.base_branch.clone()))
                            .child("\u{2190}")
                            .child(div().truncate().child(detail.head_branch.clone())),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .child(format!("{} files", detail.changed_files))
                            .child(super::right_panel::stat(detail.additions, detail.deletions)),
                    ),
            );
        let labels = (!detail.labels.is_empty()).then(|| {
            v_flex().gap_1().child(Self::section("Labels", cx)).child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(detail.labels.iter().map(|label| {
                        let color = label
                            .color
                            .as_deref()
                            .and_then(|hex| {
                                u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()
                            })
                            .map(|hex| Hsla::from(gpui::rgb(hex)));
                        div()
                            .max_w(px(160.))
                            .truncate()
                            .px_1p5()
                            .rounded_full()
                            .border_1()
                            .border_color(color.map_or(theme.border, |color| color.opacity(0.5)))
                            .bg(color.map_or(foreground.opacity(0.06), |color| color.opacity(0.15)))
                            .text_xs()
                            .child(label.name.clone())
                    })),
            )
        });
        let reviewers = (!detail.reviewers.is_empty()).then(|| {
            v_flex()
                .gap_1()
                .child(Self::section("Reviewers", cx))
                .child(
                    div().text_xs().child(
                        detail
                            .reviewers
                            .iter()
                            .map(|reviewer| reviewer.login.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                )
        });
        let checks = (!detail.checks.is_empty()).then(|| {
            v_flex()
                .gap_1()
                .child(Self::section("Checks", cx))
                .children(detail.checks.iter().enumerate().map(|(index, check)| {
                    let (icon, tone) = match check.status {
                        CheckStatus::Success => (Icons::CircleCheck, gpui::rgb(0x059669).into()),
                        CheckStatus::Failure | CheckStatus::Cancelled => {
                            (Icons::X, gpui::rgb(0xef4444).into())
                        }
                        CheckStatus::Pending | CheckStatus::ActionRequired => {
                            (Icons::LoaderCircle, gpui::rgb(0xf59e0b).into())
                        }
                        CheckStatus::Skipped | CheckStatus::Neutral => (Icons::CircleDashed, muted),
                    };
                    let url = check.url.clone();
                    h_flex()
                        .id(("pull-request-check", index))
                        .gap_2()
                        .text_xs()
                        .when(url.is_some(), |row| row.cursor_pointer())
                        .child(
                            Icon::new(icon)
                                .size(px(14.))
                                .flex_shrink_0()
                                .text_color(tone),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .child(check.name.clone()),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(muted)
                                .child(check.status.label()),
                        )
                        .on_click(move |_, _, cx| {
                            if let Some(url) = &url {
                                cx.open_url(url);
                            }
                        })
                }))
        });
        let description = (!detail.body.trim().is_empty()).then(|| {
            v_flex()
                .gap_1()
                .child(Self::section("Description", cx))
                .child(div().text_sm().child(detail.body.clone()))
        });
        v_flex()
            .id("pull-request-detail")
            .size_full()
            .overflow_y_scroll()
            .pt_3()
            .child(header)
            .child(
                v_flex()
                    .gap_4()
                    .px_4()
                    .pb_4()
                    .children(labels)
                    .children(reviewers)
                    .children(checks)
                    .children(description),
            )
    }

    fn note(text: &'static str, cx: &App) -> gpui::Div {
        div()
            .p_3()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(text)
    }

    fn unavailable(
        &self,
        shown: &Shown,
        message: &str,
        refreshing: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let host = shown
            .reference
            .host
            .as_deref()
            .filter(|host| !host.is_empty())
            .or(shown.url.as_deref())
            .unwrap_or("");
        let failure = pull_request::detail_failure(message, shown.reference.number, host);
        let retry = cx.listener(|this, _, _, cx| this.retry(cx));
        unavailable_view(failure, shown.url.clone(), refreshing, retry, cx)
    }
}

/// `PullRequestsUnavailableState`: an icon card, the title and reason, and the buttons.
pub(super) fn unavailable_view(
    failure: pull_request::Unavailable,
    open: Option<String>,
    refreshing: bool,
    on_retry: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> gpui::Div {
    let theme = cx.theme();
    let label = pull_request::open_label(open.as_deref().unwrap_or(""));
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_6()
        .p_6()
        .text_center()
        .child(
            div()
                .size(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.background)
                .shadow_sm()
                .child(Icon::new(Icons::GitPullRequestArrow).size(px(18.))),
        )
        .child(
            v_flex()
                .max_w(px(384.))
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::MEDIUM)
                        .child(failure.title),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(failure.error),
                ),
        )
        .child(
            h_flex()
                .flex_wrap()
                .justify_center()
                .gap_2()
                .child(
                    Button::new("pull-request-retry")
                        .label("Retry")
                        .icon(Icon::new(Icons::RefreshCw))
                        .outline()
                        .small()
                        .disabled(refreshing)
                        .on_click(on_retry),
                )
                .when_some(open, |row, url| {
                    row.child(
                        Button::new("pull-request-open")
                            .label(label)
                            .icon(Icon::new(Icons::ExternalLink))
                            .outline()
                            .small()
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
                }),
        )
}

impl Render for PullRequestPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(shown) = &self.shown else {
            return div().into_any_element();
        };
        match self.loads.get(&shown.reference.key()) {
            None | Some(Load::Loading) => {
                Self::note("Loading pull request...", cx).into_any_element()
            }
            Some(Load::Failed {
                message,
                refreshing,
            }) => self
                .unavailable(shown, message, *refreshing, cx)
                .into_any_element(),
            Some(Load::Loaded(value)) => match Detail::decode(value) {
                Some(detail) => Self::detail(&detail, cx).into_any_element(),
                None => Self::note("Could not read this pull request.", cx).into_any_element(),
            },
        }
    }
}
