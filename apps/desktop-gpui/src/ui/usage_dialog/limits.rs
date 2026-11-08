//! The Limits tab: `UsageLimits.tsx` and `UsageLimitsPooled.tsx` over `usage_limits.rs`.

use super::*;
use crate::providers::{accent_badge, driver_label, normalize_accent};
use crate::usage::{Pace, Window as LimitWindow};
use crate::usage_limits::{
    self as limits, Account, CHATGPT_USAGE_URL, Link, Native, Pool, PoolWindow,
    cursor_window_details,
};
use gpui::{Anchor, StyleRefinement, WeakEntity, canvas, fill, pattern_slash};
use gpui_component::hover_card::HoverCard;

/// `CURSOR_KEYCHAIN_COPY`.
pub(super) const CURSOR_KEYCHAIN_COPY: &str =
    "Requires access to your Cursor login in macOS Keychain.";
/// `@2xl/pool`.
const WIDE_BAR: f32 = 672.;

/// The segment whose reset credit the confirm dialog spends.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Target {
    key: String,
    window: String,
    instance: String,
    name: String,
}

#[derive(Default)]
pub(super) struct LimitsState {
    natives: Vec<Native>,
    pub(super) environment: String,
    cursor_ready: bool,
    pub(super) now: i64,
    pending: bool,
    queued: bool,
    refresh_after: i64,
    reveal: bool,
    confirm: Option<Target>,
    busy: Option<(String, String)>,
    status: HashMap<(String, String), String>,
    enabling: bool,
    /// The segment whose popover is open, for the trigger ring.
    open_segment: Option<usize>,
}

impl LimitsState {
    pub(super) fn refreshing(&self) -> bool {
        self.pending
    }
}

fn outcome_text(outcome: &str) -> &'static str {
    match outcome {
        "reset" => "Reset applied. Your windows have cleared.",
        "nothingToReset" => "Nothing to reset right now.",
        "noCredit" => "No reset credit left.",
        "alreadyRedeemed" => "That credit was already redeemed.",
        _ => "Could not use the reset credit.",
    }
}

impl UsageDialog {
    pub fn set_providers(&mut self, catalog: &Catalog, cx: &mut Context<Self>) {
        self.limits.natives = catalog.natives();
        self.limits.cursor_ready = catalog.cursor_ready();
        self.price_store.refresh(catalog.usage_prices());
        if self.open && self.prefs.metric == Metric::Limits {
            self.limits.now = now_ms();
        }
        cx.notify();
    }

    pub fn set_environment(&mut self, label: String) {
        self.limits.environment = label;
    }

    /// `refreshLimits`: automatic ones wait out the five minute throttle, manual ones join a
    /// refresh in flight and run again when it settles.
    pub(super) fn refresh_limits(&mut self, automatic: bool) {
        if !self.can_read() {
            return;
        }
        let state = &mut self.limits;
        if state.pending {
            state.queued |= !automatic;
            return;
        }
        let now = now_ms();
        if automatic && now < state.refresh_after {
            return;
        }
        state.pending = true;
        state.refresh_after = now + 5 * 60_000;
        self.call(Call::RefreshProviders, json!({}), Pending::Limits);
    }

    pub(super) fn limits_settled(&mut self, event: CallEvent) {
        self.limits.pending = false;
        self.limits.now = now_ms();
        if let CallEvent::Done(value) = event
            && let Some(natives) = crate::providers::natives_from_reply(value)
        {
            self.limits.natives = natives;
        }
        if std::mem::take(&mut self.limits.queued) {
            self.refresh_limits(false);
        }
    }

    pub(super) fn redeem_settled(&mut self, key: (String, String), event: CallEvent) {
        self.limits.busy = None;
        let text = match event {
            CallEvent::Done(value) => value
                .get("warning")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    outcome_text(value.get("outcome").and_then(Value::as_str).unwrap_or(""))
                        .to_owned()
                }),
            CallEvent::Failed(message) if !message.is_empty() => message,
            _ => "Could not use the reset credit.".to_owned(),
        };
        self.limits.status.insert(key, text);
    }

    pub(super) fn enable_settled(&mut self, event: CallEvent) {
        self.limits.enabling = false;
        if matches!(event, CallEvent::Done(_)) {
            self.generation += 1;
            self.stage = Stage::Idle;
            self.start_query();
            self.refresh_limits(false);
        }
    }

    fn enable_cursor(&mut self, cx: &mut Context<Self>) {
        if self.limits.enabling {
            return;
        }
        self.limits.enabling = true;
        self.call(
            Call::UpdateSettings,
            json!({"patch": {"cursorKeychainUsageEnabled": true}}),
            Pending::EnableCursor,
        );
        cx.notify();
    }

    fn confirm_redeem(&mut self, target: Target, cx: &mut Context<Self>) {
        self.limits.confirm = Some(target);
        cx.notify();
    }

    fn redeem(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.limits.confirm.take() else {
            return;
        };
        let key = (target.key.clone(), target.window.clone());
        self.limits.busy = Some(key.clone());
        self.limits.status.remove(&key);
        self.call(
            Call::ConsumeResetCredit,
            json!({"instanceId": target.instance}),
            Pending::Redeem { key },
        );
        cx.notify();
    }

    /// `needsCursorKeychainAccess` unless a Cursor account is already read (`usage.ts:18-44`).
    fn cursor_prompt_needed(&self) -> bool {
        let Some(shown) = &self.shown else {
            return false;
        };
        if self.prefs.hidden.contains(Provider::Cursor) {
            return false;
        }
        let sources: Vec<_> = shown
            .summary
            .sources
            .iter()
            .filter(|source| !self.prefs.hidden.contains(source.provider))
            .collect();
        let has_account = sources
            .iter()
            .any(|source| source.provider == Provider::Cursor && source.host_id == "cursor.com");
        !has_account && self.limits.cursor_ready && sources.iter().any(|source| source.has_action)
    }

    /// The natives of the providers the breadcrumb has not hidden.
    fn shown_natives(&self) -> Vec<Native> {
        let hidden: Vec<&str> = Provider::ORDER
            .iter()
            .filter(|provider| self.prefs.hidden.contains(**provider))
            .map(|provider| provider.driver_kind())
            .collect();
        self.limits
            .natives
            .iter()
            .filter(|native| !hidden.contains(&native.driver.as_str()))
            .cloned()
            .collect()
    }

    /// The overview's `showChatGptSharing`.
    pub(super) fn chatgpt_sharing(&self) -> bool {
        self.shown_natives()
            .iter()
            .any(Native::uses_chatgpt_sharing)
    }

    pub(super) fn cursor_enable_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.cursor_prompt_needed() {
            return None;
        }
        let theme = cx.theme();
        let label = self.limits.environment.clone();
        let enabling = self.limits.enabling;
        Some(
            h_flex()
                .min_w_0()
                .items_baseline()
                .justify_between()
                .gap_4()
                .text_size(px(14.))
                .line_height(px(20.))
                .child(
                    h_flex()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .size(px(8.))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(tint(Provider::Cursor, cx)),
                        )
                        .child(mark(Provider::Cursor, 16., cx))
                        .child(div().truncate().child("Cursor")),
                )
                .child(
                    inline_button("usage-cursor-enable-row", "Enable", enabling, theme.primary)
                        .tooltip(tooltip(CURSOR_KEYCHAIN_COPY))
                        .when(!enabling, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.enable_cursor(cx)))
                        })
                        .id(("usage-cursor-enable-row", label.len())),
                )
                .into_any_element(),
        )
    }

    pub(super) fn chatgpt_row(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        h_flex()
            .items_center()
            .justify_between()
            .gap_2()
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(theme.muted_foreground)
            .child("ChatGPT shared usage")
            .child(manage_usage_button(
                "usage-chatgpt-manage",
                CHATGPT_USAGE_URL,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn limits_body(&self, vw: f32, cx: &mut Context<Self>) -> AnyElement {
        let st = &self.limits;
        let now = st.now;
        let natives = self.shown_natives();
        let accounts = limits::collect_accounts(&natives);
        let pools = limits::collect_pools(&accounts, now);
        let notices = limits::collect_notices(&natives);
        let links = limits::collect_external_links(&natives, &st.environment);
        let prompt = self.cursor_prompt_needed();
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let position = |driver: &str| {
            pools
                .iter()
                .position(|pool| pool.driver == driver)
                .map_or(-1, |index| index as isize)
        };
        let prompt_at = (position("codex").max(position("claudeAgent")) + 1) as usize;

        let side = if vw >= 640. { 24. } else { 20. };
        let modal = if vw >= 640. {
            let pad = if vw >= 1024. { 40. } else { 24. };
            (vw - 2. * pad).min(1280.)
        } else {
            vw
        };
        let content = (modal.min(CONTENT_MAX + 2. * side) - 2. * side).max(0.);
        let two_columns = vw >= 768.;
        let bar_width = if two_columns {
            content - 176. - 24. - 34.
        } else {
            content - 34.
        };

        let mut page = v_flex().gap_8();
        if pools.is_empty() && notices.is_empty() && !prompt && links.is_empty() {
            page = page.child(
                div()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .text_color(muted)
                    .child("No provider on the selected environments reports subscription limits."),
            );
        }
        let weak = cx.weak_entity();
        for (index, pool) in pools.iter().enumerate() {
            if prompt && index == prompt_at {
                page = page.child(self.cursor_section(cx));
            }
            page = page.child(self.pool_section(index, pool, &weak, bar_width, two_columns, cx));
        }
        if prompt && prompt_at == pools.len() {
            page = page.child(self.cursor_section(cx));
        }
        for (index, link) in links.iter().enumerate() {
            page = page.child(link_card(index, link, cx));
        }
        if !notices.is_empty() {
            page = page.child(notice_alert(&notices, cx));
        }
        page.into_any_element()
    }

    fn cursor_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let enabling = self.limits.enabling;
        let label = self.limits.environment.clone();
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(provider_mark_at(
                        &Mark {
                            driver: "cursor".to_owned(),
                            display_name: "Cursor".to_owned(),
                            badge: None,
                        },
                        20.,
                        16.,
                        0.8,
                        cx,
                    ))
                    .child("Cursor"),
            )
            .child(
                v_flex()
                    .items_start()
                    .gap_3()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.border.opacity(0.6))
                    .p_4()
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .text_color(theme.muted_foreground)
                            .child(CURSOR_KEYCHAIN_COPY),
                    )
                    .child(
                        div()
                            .id("usage-cursor-enable")
                            .px(px(10.))
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(theme.border)
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .font_weight(FontWeight::MEDIUM)
                            .when(enabling, |button| button.opacity(0.64))
                            .when(!enabling, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme.foreground.opacity(0.06)))
                                    .on_click(cx.listener(|this, _, _, cx| this.enable_cursor(cx)))
                            })
                            .tooltip(tooltip(format!("Enable Cursor usage from {label}")))
                            .child("Enable"),
                    ),
            )
            .into_any_element()
    }

    fn pool_section(
        &self,
        pool_index: usize,
        pool: &Pool,
        weak: &WeakEntity<Self>,
        bar_width: f32,
        two_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = pool_color(&pool.driver, cx);
        let label = driver_label(&pool.driver);
        let heading = h_flex()
            .items_center()
            .gap_2()
            .text_size(px(14.))
            .line_height(px(20.))
            .font_weight(FontWeight::MEDIUM)
            .child(provider_mark_at(
                &Mark {
                    driver: pool.driver.clone(),
                    display_name: label.clone(),
                    badge: None,
                },
                20.,
                16.,
                0.8,
                cx,
            ))
            .child(label);
        let env = Env {
            now: self.limits.now,
            environment: self.limits.environment.clone(),
            reveal: self.limits.reveal,
            bar_width,
            weak: weak.clone(),
        };
        let cards = pool
            .display_windows()
            .into_iter()
            .enumerate()
            .map(|(window_index, window)| {
                self.window_card(
                    pool_index,
                    window_index,
                    pool,
                    window,
                    color,
                    &env,
                    two_columns,
                    cx,
                )
            });
        v_flex()
            .gap_3()
            .child(heading)
            .children(cards)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn window_card(
        &self,
        pool_index: usize,
        window_index: usize,
        pool: &Pool,
        window: &PoolWindow,
        color: Hsla,
        env: &Env,
        two_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let cursor = if pool.driver == "cursor" {
            cursor_window_details(&window.id)
        } else {
            None
        };
        let label = cursor.map_or(window.label.as_str(), |(label, _)| label);
        let description = cursor.map(|(_, description)| description);
        let next_refill = window
            .resets
            .iter()
            .find(|reset| reset.restores_percent > 0)
            .map(|reset| reset.restores_percent);
        let columns = window.columns.len();
        let pace = window.pace.map(|pace| {
            let icon = match pace {
                Pace::Ahead => Icons::TrendingUp,
                Pace::On => Icons::Gauge,
                Pace::Under => Icons::TrendingDown,
            };
            div()
                .id(("usage-pace", pool_index * 1000 + window_index))
                .flex()
                .text_color(muted)
                .child(Icon::new(icon).size(px(14.)))
                .tooltip(tooltip(pace.description()))
        });
        let left = v_flex()
            .gap_1()
            .child(
                div()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(label.to_owned()),
            )
            .child(
                h_flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(30.))
                            .line_height(px(36.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{}%", window.remaining_percent)),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .text_color(muted)
                            .child("left")
                            .children(pace),
                    ),
            )
            .children(next_refill.filter(|_| columns > 1).map(|percent| {
                div()
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(format!("\u{21bb} +{percent}%"))
            }));
        let bar = self.pool_bar(pool_index, window_index, pool, window, color, env, cx);
        v_flex()
            .gap_x(px(24.))
            .gap_y_3()
            .rounded(px(8.))
            .border_1()
            .border_color(border.opacity(0.6))
            .p_4()
            .child(
                div()
                    .flex()
                    .when(two_columns, |row| {
                        row.flex_row().items_center().gap(px(24.))
                    })
                    .when(!two_columns, |row| row.flex_col().gap_3())
                    .child(
                        div()
                            .when(two_columns, |column| column.w(px(176.)).flex_shrink_0())
                            .child(left),
                    )
                    .child(div().flex_1().min_w_0().child(bar)),
            )
            .children(description.map(|text| {
                div()
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .text_color(muted)
                    .child(text)
            }))
            .into_any_element()
    }

    fn redeem_statuses(
        &self,
        pool: &Pool,
        window: &PoolWindow,
        theme: &gpui_component::Theme,
    ) -> Vec<AnyElement> {
        let (foreground, muted) = (theme.foreground, theme.muted_foreground);
        let mut rows = Vec::new();
        for member in &window.members {
            let account = &pool.accounts[member.account];
            let key = (account.key.clone(), member.window.id.clone());
            let busy = self.limits.busy.as_ref() == Some(&key);
            let text = if busy {
                Some("Using reset credit\u{2026}".to_owned())
            } else {
                self.limits.status.get(&key).cloned()
            };
            if let Some(text) = text {
                rows.push(
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(4.))
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .text_color(muted)
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(foreground)
                                .child(account_name_for_status(account, &pool.driver)),
                        )
                        .child(text)
                        .into_any_element(),
                );
            }
        }
        rows
    }

    #[allow(clippy::too_many_arguments)]
    fn pool_bar(
        &self,
        pool_index: usize,
        window_index: usize,
        pool: &Pool,
        window: &PoolWindow,
        color: Hsla,
        env: &Env,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let columns = window.columns.len();
        let wide = env.bar_width >= WIDE_BAR;
        let show_name = columns > 1;
        let segment_width = segment_width(env.bar_width, columns);
        let mut cells: Vec<AnyElement> = Vec::new();
        let mut legend: Vec<AnyElement> = Vec::new();
        for (column, slot) in window.columns.iter().enumerate() {
            let Some(member_index) = *slot else {
                cells.push(div().flex_1().min_w_0().into_any_element());
                if !wide {
                    legend.push(div().h_0().into_any_element());
                }
                continue;
            };
            let part = Segment::new(
                pool_index * 1_000_000 + window_index * 1_000 + column,
                column + 1,
                pool,
                window,
                member_index,
                color,
                self,
                env,
                cx,
            );
            cells.push(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(part.bar(wide, show_name, segment_width, cx))
                    .into_any_element(),
            );
            if !wide {
                legend.push(part.legend_row(cx));
            }
        }
        v_flex()
            .gap_1()
            .child(h_flex().gap_1().children(cells))
            .children(legend)
            .children(self.redeem_statuses(pool, window, cx.theme()))
            .into_any_element()
    }

    /// The confirm dialog, over the page.
    pub(super) fn reset_credit_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.limits.confirm.as_ref()?;
        let theme = cx.theme();
        let (border, background, muted) = (theme.border, theme.background, theme.muted_foreground);
        let backdrop = background.opacity(if theme.is_dark() { 0.64 } else { 0.6 });
        let cancel = div()
            .id("usage-credit-cancel")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .border_1()
            .border_color(border)
            .text_size(px(14.))
            .line_height(px(20.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|style| style.bg(theme.foreground.opacity(0.06)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.limits.confirm = None;
                cx.notify();
            }))
            .child("Cancel");
        let confirm = div()
            .id("usage-credit-confirm")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .bg(theme.primary)
            .text_color(theme.primary_foreground)
            .text_size(px(14.))
            .line_height(px(20.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.redeem(cx)))
            .child("Use credit");
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("usage-credit-backdrop")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .bg(backdrop)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.limits.confirm = None;
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            v_flex()
                                .occlude()
                                .w(px(448.))
                                .max_w_full()
                                .gap_4()
                                .p_6()
                                .rounded(px(16.))
                                .border_1()
                                .border_color(border)
                                .bg(background)
                                .shadow_lg()
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_lg()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child("Use a reset credit?"),
                                        )
                                        .child(div().text_size(px(14.)).line_height(px(20.)).text_color(muted).child(
                                            "This redeems one credit on your account and clears the current rate-limit windows. It cannot be undone.",
                                        )),
                                )
                                .child(
                                    h_flex()
                                        .justify_end()
                                        .gap_2()
                                        .child(cancel)
                                        .child(confirm),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// What a segment and its popover need from the page.
struct Env {
    now: i64,
    environment: String,
    reveal: bool,
    bar_width: f32,
    weak: WeakEntity<UsageDialog>,
}

fn pool_color(driver: &str, cx: &App) -> Hsla {
    Provider::ORDER
        .into_iter()
        .find(|provider| provider.driver_kind() == driver)
        .map_or(cx.theme().foreground, |provider| tint(provider, cx))
}

fn account_name_text(account: &Account, driver: &str) -> String {
    if let Some(name) = &account.display_name {
        return name.clone();
    }
    if let Some(email) = &account.email {
        return limits::account_initials(email);
    }
    driver_label(driver)
}

/// Tracks which segment's popover is open.
fn open_change(
    weak: WeakEntity<UsageDialog>,
    id: usize,
) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    move |open, _, cx| {
        weak.update(cx, |this, cx| {
            let state = &mut this.limits.open_segment;
            if *open {
                *state = Some(id);
            } else if *state == Some(id) {
                *state = None;
            }
            cx.notify();
        })
        .ok();
    }
}

/// `AccountName` in a status line: the name, or the email chip.
fn account_name_for_status(account: &Account, driver: &str) -> AnyElement {
    account_name(account, driver)
}

/// Pixel width of one segment: columns share the bar minus a 4px gap between each.
fn segment_width(bar_width: f32, columns: usize) -> f32 {
    let columns = columns.max(1) as f32;
    (bar_width - 4. * (columns - 1.)) / columns
}

/// Center of the pace tick in px: `clamp(3px, T%, 100% - 3px)` of the segment.
fn pace_mark_center(left_percent: i64, width: f32) -> f32 {
    (left_percent as f32 / 100. * width).clamp(3., (width - 3.).max(3.))
}

/// `repeating-linear-gradient(135deg, c 0 1px, transparent 1px 5px)`: the shader's
/// stripe sizes are device pixels, so scale a 1px stripe on a 5px period by the display scale.
fn hatch_pattern(color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let scale = window.scale_factor();
            let period = 5.0_f32 * scale;
            let thickness = 1.0_f32 * scale;
            let diagonal = std::f32::consts::SQRT_2;
            let width = thickness * diagonal;
            let interval = (period - thickness) * diagonal;
            window.paint_quad(fill(bounds, pattern_slash(color, width, interval)));
        },
    )
    .size_full()
}

/// `oklch(l c h)` as an sRGB color.
fn oklch(lightness: f32, chroma: f32, hue: f32) -> Hsla {
    let (a, b) = (
        chroma * hue.to_radians().cos(),
        chroma * hue.to_radians().sin(),
    );
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let linear = [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ];
    let encode = |value: f32| {
        let value = value.clamp(0., 1.);
        let encoded = if value <= 0.003_130_8 {
            12.92 * value
        } else {
            1.055 * value.powf(1. / 2.4) - 0.055
        };
        (encoded * 255.).round() as u32
    };
    rgb((encode(linear[0]) << 16) | (encode(linear[1]) << 8) | encode(linear[2])).into()
}

/// `AccountChip`.
fn account_chip(email: &str) -> gpui::Div {
    let hue = limits::account_hue(email) as f32;
    div()
        .size(px(16.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(oklch(0.85, 0.08, hue))
        .text_color(oklch(0.35, 0.1, hue))
        .text_size(px(10.))
        .line_height(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(limits::account_initials(email))
}

/// `AccountName`.
fn account_name(account: &Account, driver: &str) -> AnyElement {
    if let Some(name) = &account.display_name {
        return div().truncate().child(name.clone()).into_any_element();
    }
    match &account.email {
        Some(email) => account_chip(email).into_any_element(),
        None => div()
            .truncate()
            .child(driver_label(driver))
            .into_any_element(),
    }
}

/// `AccountAvatar`.
fn account_avatar(account: &Account, driver: &str, cx: &App) -> AnyElement {
    if account.redeem.is_some() {
        let display_name = account
            .display_name
            .clone()
            .unwrap_or_else(|| driver_label(driver));
        let accent = account.accent_color.as_deref().and_then(normalize_accent);
        let badge = account
            .display_name
            .as_ref()
            .map(|name| accent_badge(name, accent));
        return provider_mark_at(
            &Mark {
                driver: driver.to_owned(),
                display_name,
                badge,
            },
            20.,
            16.,
            1.,
            cx,
        )
        .into_any_element();
    }
    match &account.email {
        Some(email) => account_chip(email).into_any_element(),
        None => div().into_any_element(),
    }
}

/// One member's segment: the bar, its legend row and the popover both open.
struct Segment {
    id: usize,
    position: usize,
    account: Account,
    driver: String,
    window: LimitWindow,
    remaining: i64,
    time_left: Option<i64>,
    resets_in: Option<String>,
    credits: i64,
    restores: i64,
    refill: f64,
    has_reset: bool,
    color: Hsla,
    now: i64,
    environment: String,
    reveal: bool,
    key: (String, String),
    busy: bool,
    open: bool,
    weak: WeakEntity<UsageDialog>,
}

impl Segment {
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: usize,
        position: usize,
        pool: &Pool,
        window: &PoolWindow,
        member_index: usize,
        pool_color: Hsla,
        dialog: &UsageDialog,
        env: &Env,
        _cx: &App,
    ) -> Self {
        let member = &window.members[member_index];
        let account = pool.accounts[member.account].clone();
        let reset = window.reset_of(member_index);
        let color = account
            .accent_color
            .as_deref()
            .and_then(normalize_accent)
            .map_or(pool_color, |accent| rgb(accent).into());
        let credits = account
            .limits
            .reset_credits
            .as_ref()
            .map_or(0, |credits| credits.available);
        let key = (account.key.clone(), member.window.id.clone());
        Self {
            id,
            position,
            driver: pool.driver.clone(),
            remaining: member.window.remaining_percent(),
            time_left: member.window.pace_remaining_percent(env.now),
            resets_in: member.window.resets_in(env.now),
            credits,
            restores: reset.map_or(0, |reset| reset.restores_percent),
            refill: member.refill_percent,
            has_reset: reset.is_some(),
            window: member.window.clone(),
            color,
            now: env.now,
            environment: env.environment.clone(),
            reveal: env.reveal,
            busy: dialog.limits.busy.as_ref() == Some(&key),
            open: dialog.limits.open_segment == Some(id),
            key,
            weak: env.weak.clone(),
            account,
        }
    }

    fn label(&self) -> String {
        let who = self.account.display_name.clone().unwrap_or_else(|| {
            self.account
                .email
                .as_deref()
                .map_or_else(|| driver_label(&self.driver), limits::account_initials)
        });
        let mut label = format!("{who}: {}% left", self.remaining);
        if let Some(resets_in) = &self.resets_in {
            label.push_str(&format!(", {resets_in}"));
        }
        if let Some(left) = self.time_left {
            label.push_str(&format!(", {left}% of the window left"));
        }
        if self.credits > 0 {
            let noun = if self.credits == 1 {
                "credit"
            } else {
                "credits"
            };
            label.push_str(&format!(", {} reset {noun} banked", self.credits));
        }
        label
    }

    fn chip_text(&self) -> Option<String> {
        self.resets_in
            .as_ref()
            .map(|text| text.replace("resets in ", "\u{21bb} "))
    }

    fn has_chip(&self) -> bool {
        self.resets_in.is_some() || self.credits > 0
    }

    fn bar(&self, wide: bool, show_name: bool, width: f32, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let (foreground, muted, background) =
            (theme.foreground, theme.muted_foreground, theme.background);
        let height = if wide { 32. } else { 20. };
        let fraction = (self.remaining as f32 / 100.).clamp(0., 1.);
        let mut track = div()
            .id(("usage-limit-segment", self.id))
            .relative()
            .w_full()
            .h(px(height))
            .rounded(px(6.))
            .overflow_hidden()
            .bg(theme.muted)
            .cursor_default();
        if fraction > 0. {
            track = track.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(relative(fraction))
                    .rounded(px(6.))
                    .bg(self.color.opacity(0.35)),
            );
        }
        let hatch = (self.remaining < 100 && self.has_reset && self.refill > 0.)
            .then(|| ((100 - self.remaining) as f64).min(self.refill) as f32 / 100.);
        if let Some(share) = hatch {
            track = track.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(relative(share))
                    .child(hatch_pattern(self.color.opacity(0.2))),
            );
        }
        if let Some(left) = self.time_left.filter(|_| width >= 6.) {
            let at = px(pace_mark_center(left, width));
            track = track
                .child(
                    div().absolute().top_0().bottom_0().left(at).w_0().child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(-3.))
                            .w(px(6.))
                            .bg(background),
                    ),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(2.))
                        .bottom(px(2.))
                        .left(at)
                        .w_0()
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px(-1.))
                                .w(px(2.))
                                .rounded_full()
                                .bg(foreground),
                        ),
                );
        }
        if wide {
            let chip = self.has_chip().then(|| {
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(4.))
                    .bg(background.opacity(0.85))
                    .px(px(6.))
                    .py(px(2.))
                    .text_size(px(11.))
                    .line_height(px(16.))
                    .children(self.chip_text())
                    .when(self.credits > 0, |chip| {
                        chip.children(
                            self.resets_in
                                .is_some()
                                .then(|| div().text_color(muted).child("\u{b7}")),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_0p5()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(Icon::new(Icons::Ticket).size(px(12.)))
                                .child(self.credits.to_string()),
                        )
                    })
            });
            track =
                track.child(
                    h_flex()
                        .absolute()
                        .inset_0()
                        .items_center()
                        .gap(px(6.))
                        .px_2()
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .child(h_flex().min_w_0().items_center().gap(px(6.)).when(
                            show_name,
                            |row| {
                                row.child(
                                    div()
                                        .min_w_0()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(account_name(&self.account, &self.driver)),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format!("{}%", self.remaining)),
                                )
                            },
                        ))
                        .child(div().flex_1())
                        .children(chip),
                );
        } else {
            track = track.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(foreground.opacity(0.8))
                    .child(self.position.to_string()),
            );
        }
        if self.open {
            track = track.child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme.border),
            );
        }
        let popover = self.popover();
        HoverCard::new(("usage-limit-popover", self.id))
            .on_open_change(open_change(self.weak.clone(), self.id))
            .appearance(true)
            .px_4()
            .py_4()
            .anchor(Anchor::BottomCenter)
            .open_delay(Duration::from_millis(150))
            .trigger_style(StyleRefinement::default().w_full())
            .trigger(track.tooltip(tooltip(self.label())))
            .content(move |_, _, cx| popover.render(cx))
            .into_any_element()
    }

    fn legend_row(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let (muted, foreground) = (theme.muted_foreground, theme.foreground);
        let right = self.has_chip().then(|| {
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.))
                .text_size(px(11.))
                .line_height(px(16.))
                .text_color(muted)
                .children(self.chip_text())
                .when(self.credits > 0, |row| {
                    row.children(self.resets_in.is_some().then_some("\u{b7}"))
                        .child(
                            h_flex()
                                .items_center()
                                .gap_0p5()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(foreground)
                                .child(Icon::new(Icons::Ticket).size(px(12.)))
                                .child(self.credits.to_string()),
                        )
                })
        });
        let accent = foreground.opacity(0.06);
        let open = self.open;
        let row = h_flex()
            .id(("usage-limit-legend", self.id))
            .w_full()
            .h(px(28.))
            .items_center()
            .gap_1()
            .px(px(8.))
            .rounded(px(6.))
            .text_size(px(12.))
            .line_height(px(16.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .when(open, |row| row.bg(accent))
            .hover(move |style| style.bg(accent))
            .child(
                div()
                    .size(px(16.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .bg(self.color.opacity(0.35))
                    .text_size(px(10.))
                    .line_height(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(foreground.opacity(0.8))
                    .child(self.position.to_string()),
            )
            .child(
                div()
                    .min_w_0()
                    .font_weight(FontWeight::MEDIUM)
                    .child(account_name(&self.account, &self.driver)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{}%", self.remaining)),
            )
            .child(div().flex_1())
            .children(right);
        let popover = self.popover();
        HoverCard::new(("usage-limit-legend-popover", self.id))
            .on_open_change(open_change(self.weak.clone(), self.id))
            .px_4()
            .py_4()
            .anchor(Anchor::BottomCenter)
            .open_delay(Duration::from_millis(150))
            .trigger_style(StyleRefinement::default().w_full())
            .trigger(row)
            .content(move |_, _, cx| popover.render(cx))
            .into_any_element()
    }

    fn popover(&self) -> Rc<Popover> {
        Rc::new(Popover {
            id: self.id,
            account: self.account.clone(),
            driver: self.driver.clone(),
            window: self.window.clone(),
            remaining: self.remaining,
            time_left: self.time_left,
            resets_in: self.resets_in.clone(),
            restores: self.restores,
            now: self.now,
            environment: self.environment.clone(),
            reveal: self.reveal,
            key: self.key.clone(),
            busy: self.busy,
            weak: self.weak.clone(),
        })
    }
}

/// `SegmentPopoverContent`.
struct Popover {
    id: usize,
    account: Account,
    driver: String,
    window: LimitWindow,
    remaining: i64,
    time_left: Option<i64>,
    resets_in: Option<String>,
    restores: i64,
    now: i64,
    environment: String,
    reveal: bool,
    key: (String, String),
    busy: bool,
    weak: WeakEntity<UsageDialog>,
}

impl Popover {
    fn row(label: &'static str, value: String, cx: &App) -> gpui::Div {
        h_flex()
            .gap_x_3()
            .items_baseline()
            .child(
                div()
                    .w(px(72.))
                    .flex_shrink_0()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(div().min_w_0().child(value))
    }

    fn render(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let (border, muted, foreground) = (
            theme.border.opacity(0.6),
            theme.muted_foreground,
            theme.foreground,
        );
        let account = &self.account;
        let title = account
            .display_name
            .clone()
            .unwrap_or_else(|| driver_label(&self.driver));
        let email = account
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty());
        let email_line = email.map(|email| {
            let shown = if self.reveal {
                email.to_owned()
            } else {
                redacted_placeholder(email)
            };
            let weak = self.weak.clone();
            div()
                .id(("usage-limit-email", self.id))
                .w_auto()
                .cursor_pointer()
                .rounded(px(4.))
                .font_family("monospace")
                .text_size(px(11.))
                .text_color(muted)
                .hover(move |style| style.text_color(foreground))
                .when(!self.reveal, |text| text.opacity(0.6))
                .tooltip(tooltip(if self.reveal {
                    "Click to hide email"
                } else {
                    "Click to reveal email"
                }))
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| {
                        this.limits.reveal = !this.limits.reveal;
                        cx.notify();
                    })
                    .ok();
                })
                .child(shown)
        });
        let header = v_flex()
            .min_w_0()
            .gap_0p5()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(account_avatar(account, &self.driver, cx))
                    .child(div().truncate().child(title)),
            )
            .children(email_line);
        let mut identity = v_flex()
            .gap_1()
            .border_t_1()
            .border_color(border)
            .pt(px(10.));
        if let Some(plan) = account.plan.as_ref().filter(|plan| !plan.is_empty()) {
            identity = identity.child(Self::row("Plan", plan.clone(), cx));
        }
        if !self.environment.is_empty() {
            identity = identity.child(Self::row("Signed in", self.environment.clone(), cx));
        }
        let mut figures = v_flex()
            .gap_1()
            .border_t_1()
            .border_color(border)
            .pt(px(10.))
            .child(Self::row("Left", format!("{}%", self.remaining), cx));
        if let Some(at) = self.window.resets_at {
            let zone = crate::usage_data::Zone::detect();
            let mut text = crate::usage_data::format_upcoming(at, self.now, &zone);
            if let Some(resets_in) = &self.resets_in {
                text.push_str(&format!(
                    " \u{b7} {}",
                    resets_in.replace("resets in ", "in ")
                ));
            }
            figures = figures.child(Self::row("Resets", text, cx));
        }
        if let Some(left) = self.time_left {
            figures = figures.child(Self::row("Even pace", format!("{left}% left by now"), cx));
        }
        if self.restores > 0 {
            figures = figures.child(Self::row(
                "Restores",
                format!("+{}% of pool", self.restores),
                cx,
            ));
        }
        let credits = account
            .redeem
            .as_ref()
            .zip(account.limits.reset_credits.as_ref())
            .filter(|(_, credits)| credits.available > 0);
        let credit_section = credits.map(|(instance, credits)| {
            let weak = self.weak.clone();
            let target = Target {
                key: self.key.0.clone(),
                window: self.key.1.clone(),
                instance: instance.clone(),
                name: account_name_text(account, &self.driver),
            };
            let busy = self.busy;
            h_flex()
                .items_center()
                .gap_3()
                .border_t_1()
                .border_color(border)
                .pt(px(10.))
                .text_color(muted)
                .child(limits::reset_credits_summary(credits, self.now, true))
                .child(
                    div()
                        .id(("usage-limit-use-reset", self.id))
                        .ml_auto()
                        .h(px(24.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme.border)
                        .text_color(foreground)
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .when(busy, |button| button.opacity(0.64))
                        .when(!busy, |button| {
                            button
                                .cursor_pointer()
                                .hover(move |style| style.bg(foreground.opacity(0.06)))
                                .on_click(move |_, _, cx| {
                                    let target = target.clone();
                                    weak.update(cx, |this, cx| this.confirm_redeem(target, cx))
                                        .ok();
                                })
                        })
                        .child(if busy { "Using\u{2026}" } else { "Use reset" }),
                )
        });
        v_flex()
            .w(px(288.))
            .gap(px(10.))
            .text_size(px(12.))
            .line_height(px(16.))
            .child(header)
            .child(identity)
            .child(figures)
            .children(credit_section)
            .into_any_element()
    }
}

/// `redactedPlaceholder`: a stable stand-in of the same shape.
fn redacted_placeholder(value: &str) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut state: u32 = 0x811c_9dc5;
    for unit in value.encode_utf16() {
        state ^= u32::from(unit);
        state = state.wrapping_mul(0x0100_0193);
    }
    let mut next = || {
        state = (state ^ (state >> 13)).wrapping_mul(0x85eb_ca6b);
        state = (state ^ (state >> 16)).wrapping_mul(0xc2b2_ae35);
        let magnitude = (state as i32).unsigned_abs() as usize;
        char::from(ALPHABET[magnitude % ALPHABET.len()])
    };
    value
        .chars()
        .map(|ch| {
            if matches!(ch, '@' | '.' | '-' | '_') {
                ch
            } else {
                next()
            }
        })
        .collect()
}

fn inline_button(
    id: &'static str,
    text: &'static str,
    disabled: bool,
    color: Hsla,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .text_color(color)
        .when(disabled, |button| button.opacity(0.64))
        .when(!disabled, |button| button.cursor_pointer())
        .child(text)
}

/// `Button` `ghost-muted` `xs` with the external-link icon.
fn manage_usage_button(id: &'static str, url: &'static str, cx: &App) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (muted, foreground) = (theme.muted_foreground, theme.foreground);
    let url = url.to_owned();
    h_flex()
        .id(id)
        .flex_shrink_0()
        .h(px(24.))
        .px(px(8.))
        .gap_1()
        .items_center()
        .rounded(px(6.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.bg(foreground.opacity(0.06)).text_color(foreground))
        .on_click(move |_, _, cx| cx.open_url(&url))
        .child("Manage usage")
        .child(Icon::new(Icons::ExternalLink).size(px(14.)))
}

fn link_card(index: usize, link: &Link, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let chatgpt = link.url == CHATGPT_USAGE_URL;
    let url = link.url.clone();
    let open = h_flex()
        .id(("usage-link-open", index))
        .flex_shrink_0()
        .h(px(24.))
        .px(px(8.))
        .gap_1()
        .items_center()
        .rounded(px(6.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(muted)
        .cursor_pointer()
        .hover({
            let foreground = theme.foreground;
            move |style| style.bg(foreground.opacity(0.06)).text_color(foreground)
        })
        .on_click(move |_, _, cx| cx.open_url(&url))
        .child("Manage usage")
        .child(Icon::new(Icons::ExternalLink).size(px(14.)));
    h_flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_3()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border)
        .p_4()
        .child(
            h_flex()
                .min_w_0()
                .flex_1()
                .items_center()
                .gap_3()
                .when(chatgpt, |row| {
                    row.child(provider_mark_at(
                        &Mark {
                            driver: "codex".to_owned(),
                            display_name: "OpenAI".to_owned(),
                            badge: None,
                        },
                        20.,
                        20.,
                        1.,
                        cx,
                    ))
                })
                .child(
                    v_flex()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(14.))
                                .line_height(px(20.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(link.label.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(16.))
                                .text_color(muted)
                                .child(link.accounts.join(", ")),
                        )
                        .when(chatgpt, |column| {
                            column.child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(muted)
                                    .child("View usage in ChatGPT with your connected account."),
                            )
                        })
                        .when(!chatgpt, |column| {
                            column.children(link.message.as_ref().map(|message| {
                                div()
                                    .max_w(px(576.))
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(muted)
                                    .child(message.clone())
                            }))
                        }),
                ),
        )
        .child(open)
        .into_any_element()
}

/// `LimitNotices`: a warning alert with one title per notice.
fn notice_alert(notices: &[String], cx: &App) -> AnyElement {
    let theme = cx.theme();
    let dark = theme.is_dark();
    let text_color: Hsla = rgb(if dark { 0xfbbf24 } else { 0xb45309 }).into();
    h_flex()
        .items_start()
        .gap_2()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.warning.opacity(0.32))
        .bg(theme.warning.opacity(if dark { 0.16 } else { 0.08 }))
        .px(px(14.))
        .py_3()
        .text_size(px(14.))
        .line_height(px(20.))
        .text_color(text_color)
        .child(
            div()
                .h(px(20.))
                .w(px(16.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .child(
                    Icon::new(Icons::TriangleAlert)
                        .size(px(16.))
                        .text_color(theme.warning),
                ),
        )
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .gap_0p5()
                .font_weight(FontWeight::MEDIUM)
                .children(notices.iter().map(|notice| div().child(notice.clone()))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_width_subtracts_gaps() {
        assert_eq!(segment_width(400., 1), 400.);
        assert_eq!(segment_width(404., 2), 200.);
        assert_eq!(segment_width(408., 3), 400. / 3.);
    }

    #[test]
    fn pace_mark_clamps_to_three_px_from_edges() {
        assert_eq!(pace_mark_center(0, 200.), 3.);
        assert_eq!(pace_mark_center(50, 200.), 100.);
        assert_eq!(pace_mark_center(100, 200.), 197.);
        assert_eq!(pace_mark_center(1, 200.), 3.);
    }
}
