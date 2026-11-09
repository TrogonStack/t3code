//! The web's `pull-request` right panel surface (`PullRequestDetailPanel.tsx`): the detail of one
//! pull request, read through the window's connection. A failed read shows
//! `PullRequestsUnavailableState`.

use super::*;
use crate::pull_request::{self, Reference};
use crate::status::CallEvent;
use gpui_component::Disableable as _;
use serde_json::Value;

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

    fn note(text: &'static str, cx: &App) -> gpui::Div {
        div()
            .p_3()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(text)
    }

    /// `PullRequestsUnavailableState`: an icon card, the title and reason, and the buttons.
    fn unavailable(
        &self,
        shown: &Shown,
        message: &str,
        refreshing: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = cx.theme();
        let failure = pull_request::detail_failure(message, shown.reference.number);
        let open = shown.url.clone();
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
                            .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
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
            Some(Load::Loaded(detail)) => {
                let title = detail["title"].as_str().unwrap_or_default().to_owned();
                v_flex()
                    .p_3()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("#{}", shown.reference.number)),
                    )
                    .child(div().text_sm().child(title))
                    .into_any_element()
            }
        }
    }
}
