//! The web's Usage page (`UsagePage.tsx`) as the `/usage` route modal (`AppRouteModal.tsx`).

mod limits;
mod pickers;
mod prices;

use super::*;
use crate::usage_data::{
    self as data, Merged, Metric, Prefs, Provider, Summary, Tint, Window as Span, Zone,
};
use crate::usage_prices::Change;
use gpui::{
    ClickEvent, ElementId, FocusHandle, KeyBinding, PathBuilder, ScrollHandle, Size, fill, point,
    radians, relative, size,
};
use limits::LimitsState;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

/// Call tokens, above the attachment range.
pub const TOKEN_BASE: u64 = 1 << 43;
/// `usagePageOpen` in the web's keybinding `when` clauses.
pub(super) const CONTEXT: &str = "UsagePage";
/// A summary older than this is fetched again when the page opens.
const STALE_MS: i64 = 60_000;
/// `figureClass`, `opacity-40`.
const DIMMED: f32 = 0.4;
/// The chart's `h-56`.
const PLOT_HEIGHT: f32 = 224.;
/// `WorkspacePageContainer` `max-w-5xl`.
const CONTENT_MAX: f32 = 1024.;

actions!(
    usage,
    [
        CloseUsage,
        ShowCost,
        ShowTokens,
        ShowLimits,
        PastDay,
        PastWeek,
        PastMonth,
        PastQuarter
    ]
);

/// Escape for the modal; the `usage.*` keys come from the server's keybindings.
pub fn bindings() -> Vec<KeyBinding> {
    let context = Some(CONTEXT);
    vec![KeyBinding::new("escape", CloseUsage, context)]
}

/// What the connection says about reading usage (`resolveUsageAccess`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Unavailable,
    Pending,
    Offline,
    Ready,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    First,
    Awaiting,
}

enum Pending {
    Query {
        generation: u64,
        awaiting: bool,
        span: Span,
    },
    Rates {
        generation: u64,
    },
    Limits,
    Redeem {
        key: (String, String),
    },
    EnableCursor,
    Prices {
        changes: Vec<Change>,
    },
}

struct Shown {
    span: Span,
    summary: Summary,
    merged: Merged,
    fetched_at: i64,
}

/// `usageLoading`: which providers' figures are still settling.
enum Dim {
    None,
    All,
    Providers(Vec<Provider>),
}

impl Dim {
    fn provider(&self, provider: Provider) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Providers(list) => list.contains(&provider),
        }
    }

    fn partial(&self) -> bool {
        !matches!(self, Self::None)
    }
}

pub struct UsageDialog {
    actions: ActionSender,
    focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    open: bool,
    prefs: Prefs,
    zone: Zone,
    span: Span,
    access: Access,
    shown: Option<Shown>,
    failed: bool,
    manual: bool,
    stage: Stage,
    generation: u64,
    breakdown_time: bool,
    hover: Option<(usize, gpui::Point<Pixels>)>,
    plot: Rc<Cell<Bounds<Pixels>>>,
    tip: Rc<Cell<Size<Pixels>>>,
    scroll: ScrollHandle,
    next_token: u64,
    pending: HashMap<u64, Pending>,
    limits: LimitsState,
    /// `selectedIds`: false once the one paired environment is unchecked.
    env_selected: bool,
    menu: Option<pickers::Menu>,
    prices: Option<prices::Prices>,
    price_store: prices::PriceStore,
    price_focus: prices::Focus,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

impl UsageDialog {
    pub fn new(actions: ActionSender, cx: &mut Context<Self>) -> Self {
        let prefs = Prefs::load();
        let zone = Zone::detect();
        let span = data::make_window(prefs.window_days, now_ms(), prefs.window_days == 1, &zone);
        Self {
            actions,
            focus: cx.focus_handle(),
            previous_focus: None,
            open: false,
            prefs,
            zone,
            span,
            access: Access::Unavailable,
            shown: None,
            failed: false,
            manual: false,
            stage: Stage::Idle,
            generation: 0,
            breakdown_time: false,
            hover: None,
            plot: Rc::default(),
            tip: Rc::default(),
            scroll: ScrollHandle::new(),
            next_token: TOKEN_BASE,
            pending: HashMap::new(),
            limits: LimitsState::default(),
            env_selected: true,
            menu: None,
            prices: None,
            price_store: prices::PriceStore::default(),
            price_focus: prices::Focus::new(cx),
        }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        self.open = true;
        self.previous_focus = window.focused(cx);
        self.span = self.make_span();
        self.hover = None;
        self.breakdown_time = false;
        self.limits.now = now_ms();
        window.focus(&self.focus, cx);
        self.ensure_loaded();
        cx.notify();
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        if let Some(previous) = self.previous_focus.take() {
            window.focus(&previous, cx);
        }
        cx.notify();
    }

    pub fn set_access(&mut self, access: Access, cx: &mut Context<Self>) {
        if self.access == access {
            return;
        }
        self.access = access;
        if self.open {
            self.ensure_loaded();
        }
        cx.notify();
    }

    fn make_span(&self) -> Span {
        data::make_window(
            self.prefs.window_days,
            now_ms(),
            self.prefs.window_days == 1,
            &self.zone,
        )
    }

    fn can_read(&self) -> bool {
        self.access == Access::Ready
    }

    fn call(&mut self, call: Call, payload: Value, pending: Pending) {
        self.next_token += 1;
        let token = self.next_token;
        self.pending.insert(token, pending);
        let action = Action::Call {
            token,
            call,
            payload,
        };
        if self.actions.try_send(action).is_err() {
            self.pending.remove(&token);
            eprintln!("[usage] could not send the request");
        }
    }

    /// Fetches when nothing is shown, the shown window is another one, or the data is old.
    fn ensure_loaded(&mut self) {
        if self.prefs.metric == Metric::Limits {
            self.refresh_limits(true);
            return;
        }
        if !self.can_read() || self.stage != Stage::Idle {
            return;
        }
        let fresh = self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.span == self.span && now_ms() - shown.fetched_at < STALE_MS);
        if !fresh && !self.manual {
            self.start_query();
        }
    }

    fn start_query(&mut self) {
        self.generation += 1;
        self.stage = Stage::First;
        self.failed = false;
        self.query(false);
    }

    fn query(&mut self, awaiting: bool) {
        let pending = Pending::Query {
            generation: self.generation,
            awaiting,
            span: self.span.clone(),
        };
        self.call(Call::GetUsageSummary, self.span.payload(awaiting), pending);
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        let Some(pending) = self.pending.remove(&token) else {
            return;
        };
        match pending {
            Pending::Limits => self.limits_settled(event),
            Pending::Redeem { key } => self.redeem_settled(key, event),
            Pending::EnableCursor => self.enable_settled(event),
            Pending::Prices { changes } => self.prices_settled(changes, event),
            Pending::Rates { generation } => {
                if generation == self.generation {
                    self.start_query();
                }
            }
            Pending::Query {
                generation,
                awaiting,
                span,
            } => {
                if generation != self.generation {
                    return;
                }
                let summary = match event {
                    CallEvent::Done(value) => Summary::decode(&value),
                    _ => None,
                };
                match summary {
                    Some(summary) => {
                        let merged = data::merge(&summary.without(self.prefs.hidden));
                        let again = !awaiting && summary.refreshing();
                        self.shown = Some(Shown {
                            span,
                            merged,
                            summary,
                            fetched_at: now_ms(),
                        });
                        self.failed = false;
                        if again {
                            self.stage = Stage::Awaiting;
                            self.query(true);
                        } else {
                            self.stage = Stage::Idle;
                            self.manual = false;
                        }
                    }
                    None => {
                        if !awaiting {
                            self.failed = true;
                        }
                        self.stage = Stage::Idle;
                        self.manual = false;
                    }
                }
            }
        }
        cx.notify();
    }

    fn close_or_prices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.prices.is_some() {
            self.close_prices(cx);
        } else if self.menu.is_some() {
            self.menu = None;
            cx.notify();
        } else {
            self.close(window, cx);
        }
    }

    fn select_window(&mut self, days: u32, cx: &mut Context<Self>) {
        self.prefs.window_days = days;
        self.prefs.save();
        self.span = self.make_span();
        self.hover = None;
        self.stage = Stage::Idle;
        self.ensure_loaded();
        cx.notify();
    }

    fn select_metric(&mut self, metric: Metric, cx: &mut Context<Self>) {
        self.prefs.metric = metric;
        self.prefs.save();
        self.hover = None;
        if metric == Metric::Limits {
            self.limits.now = now_ms();
        }
        self.ensure_loaded();
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.prefs.metric == Metric::Limits {
            self.refresh_limits(false);
            cx.notify();
            return;
        }
        if self.manual || !self.can_read() {
            return;
        }
        self.manual = true;
        self.span = self.make_span();
        self.generation += 1;
        self.stage = Stage::Idle;
        let pending = Pending::Rates {
            generation: self.generation,
        };
        self.call(Call::RefreshUsageRates, json!({}), pending);
        cx.notify();
    }

    /// `onUsageKeyDown`: the period shortcuts do nothing on the limits tab.
    fn shortcut_window(&mut self, days: u32, cx: &mut Context<Self>) {
        if self.prefs.metric != Metric::Limits {
            self.select_window(days, cx);
        }
    }

    fn dim(&self) -> Dim {
        let Some(shown) = &self.shown else {
            return Dim::None;
        };
        if self.manual || shown.span != self.span {
            return Dim::All;
        }
        match self.stage {
            Stage::Idle => Dim::None,
            Stage::First => Dim::All,
            Stage::Awaiting => {
                let mut providers: Vec<Provider> = Vec::new();
                let mut any = false;
                for source in shown.summary.sources.iter().filter(|s| s.refreshing) {
                    any = true;
                    if !self.prefs.hidden.contains(source.provider)
                        && !providers.contains(&source.provider)
                    {
                        providers.push(source.provider);
                    }
                }
                if !any {
                    Dim::All
                } else if providers.is_empty() {
                    Dim::None
                } else {
                    Dim::Providers(providers)
                }
            }
        }
    }
}

fn tint(provider: Provider, cx: &App) -> Hsla {
    let theme = cx.theme();
    match provider.tint() {
        Tint::Fixed(hex) => rgb(hex).into(),
        Tint::Contrast => theme.foreground,
        Tint::ContrastSoft => theme.background.blend(theme.foreground.opacity(0.72)),
    }
}

fn mark(provider: Provider, size: f32, cx: &App) -> gpui::Div {
    let mark = Mark {
        driver: provider.driver_kind().to_owned(),
        display_name: provider.label().to_owned(),
        badge: None,
    };
    provider_mark_at(&mark, size, size, 1., cx)
}

fn dimmed(loading: bool) -> f32 {
    if loading { DIMMED } else { 1. }
}

/// `ui/toggle.tsx` segmented item.
fn toggle(
    id: impl Into<ElementId>,
    label: &'static str,
    title: String,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let dark = theme.is_dark();
    let (hover_bg, pressed_bg) = if dark {
        (theme.input.opacity(0.32), theme.input.opacity(0.72))
    } else {
        (theme.background.opacity(0.55), theme.background)
    };
    let (muted, foreground) = (theme.muted_foreground, theme.foreground);
    div()
        .id(id)
        .h(px(24.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .tooltip(tooltip(title))
        .when(selected, |item| item.bg(pressed_bg).text_color(foreground))
        .when(!selected, |item| {
            item.text_color(muted)
                .hover(move |style| style.bg(hover_bg).text_color(foreground))
        })
        .on_click(on_click)
        .child(label)
}

fn toggle_group(cx: &App) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(8.))
        .bg(cx.theme().input.opacity(0.4))
}

/// `Button` `ghost` `icon-sm` without a tooltip, which the web's refresh button does not have.
fn plain_icon_button(
    id: &'static str,
    icon: impl IntoElement,
    enabled: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (idle, hover_bg, hover_fg) = (
        theme.muted_foreground,
        theme.foreground.opacity(0.06),
        theme.foreground,
    );
    div()
        .id(id)
        .flex_shrink_0()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .text_color(idle)
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
        })
        .when(!enabled, |button| button.opacity(0.64))
        .child(icon)
}

fn skeleton_block(width: Option<f32>, height: f32, pill: bool, cx: &App) -> gpui::Div {
    div()
        .h(px(height))
        .map(|block| match width {
            Some(width) => block.w(px(width)).flex_shrink_0(),
            None => block.w_full(),
        })
        .bg(cx.theme().muted_foreground.opacity(0.15))
        .rounded(px(if pill { 999. } else { 4. }))
}

impl Render for UsageDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return gpui::Empty.into_any_element();
        }
        let theme = cx.theme();
        let viewport = window.viewport_size();
        let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
        let pad = if vw >= 1024. {
            40.
        } else if vw >= 640. {
            24.
        } else {
            0.
        };
        let framed = vw >= 640.;
        let (width, height) = if framed {
            ((vw - 2. * pad).min(1280.), (vh - 2. * pad).min(960.))
        } else {
            (vw, vh)
        };
        let backdrop = theme
            .background
            .opacity(if theme.is_dark() { 0.64 } else { 0.6 });
        let (border, background) = (theme.border, theme.background);
        let prices = self.prices_dialog(window, cx);
        let header = self.header(width, cx);
        let body = self.body(vw, cx);
        let confirm = self.reset_credit_dialog(cx);
        div()
            .absolute()
            .inset_0()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &CloseUsage, window, cx| this.close_or_prices(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ShowCost, _, cx| this.select_metric(Metric::Cost, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ShowTokens, _, cx| this.select_metric(Metric::Tokens, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ShowLimits, _, cx| this.select_metric(Metric::Limits, cx)),
            )
            .on_action(cx.listener(|this, _: &PastDay, _, cx| this.shortcut_window(1, cx)))
            .on_action(cx.listener(|this, _: &PastWeek, _, cx| this.shortcut_window(7, cx)))
            .on_action(cx.listener(|this, _: &PastMonth, _, cx| this.shortcut_window(30, cx)))
            .on_action(cx.listener(|this, _: &PastQuarter, _, cx| this.shortcut_window(90, cx)))
            .child(
                div()
                    .id("usage-backdrop")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(backdrop)
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .child(
                v_flex()
                    .absolute()
                    .left(px((vw - width) / 2.))
                    .top(px((vh - height) / 2.))
                    .w(px(width))
                    .h(px(height))
                    .overflow_hidden()
                    .occlude()
                    .bg(background)
                    .text_color(cx.theme().foreground)
                    .when(framed, |popup| {
                        popup
                            .rounded(px(16.))
                            .border_1()
                            .border_color(border)
                            .shadow_2xl()
                    })
                    .child(header)
                    .child(
                        div()
                            .id("usage-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(body),
                    ),
            )
            .children(confirm)
            .children(prices)
            .into_any_element()
    }
}

impl UsageDialog {
    fn header(&self, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let limits = self.prefs.metric == Metric::Limits;
        let windows: [(u32, &'static str, &'static str); 4] = [
            (1, "Past 24h", "\u{21e7}\u{2318}1"),
            (7, "7 days", "\u{21e7}\u{2318}2"),
            (30, "30 days", "\u{21e7}\u{2318}3"),
            (90, "90 days", "\u{21e7}\u{2318}4"),
        ];
        let metrics: [(Metric, &'static str, &'static str); 3] = [
            (Metric::Cost, "Cost", "c"),
            (Metric::Tokens, "Tokens", "t"),
            (Metric::Limits, "Limits", "l"),
        ];
        let period = toggle_group(cx).children(windows.map(|(days, label, shortcut)| {
            toggle(
                ("usage-window", days as usize),
                label,
                format!("{label} ({shortcut})"),
                self.prefs.window_days == days,
                cx.listener(move |this, _, _, cx| this.select_window(days, cx)),
                cx,
            )
        }));
        let metric = toggle_group(cx).children(metrics.map(|(value, label, key)| {
            toggle(
                ("usage-metric", value as usize),
                label,
                format!("{label} ({key})"),
                self.prefs.metric == value,
                cx.listener(move |this, _, _, cx| this.select_metric(value, cx)),
                cx,
            )
        }));
        let refreshing = if limits {
            self.limits.refreshing()
        } else {
            self.manual
        };
        let can_refresh = !refreshing && self.can_read();
        let icon = Icon::new(Icons::RefreshCw).size(px(14.));
        let spinner = if refreshing {
            icon.with_animation(
                "usage-refresh-spin",
                Animation::new(Duration::from_secs(1)).repeat(),
                |icon, delta| icon.rotate(radians(delta * std::f32::consts::TAU)),
            )
            .into_any_element()
        } else {
            icon.into_any_element()
        };
        let refresh = plain_icon_button("usage-refresh", spinner, can_refresh, cx)
            .when(can_refresh, |button| {
                button.on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))
            });
        let close = ghost_icon_button(
            "usage-close",
            Icon::new(IconName::Close).size(px(14.)),
            "Close (Esc)".into(),
            true,
            cx,
        )
        .on_click(cx.listener(|this, _, window, cx| this.close(window, cx)));
        let window_label = if limits || width < 1280. {
            None
        } else if self.span.hourly {
            match (&self.span.since_time, &self.span.until_time) {
                (Some(since), Some(until)) => Some(format!(
                    "{} to {}",
                    data::format_date_time_short(since, &self.zone),
                    data::format_date_time_short(until, &self.zone)
                )),
                _ => None,
            }
        } else {
            Some(format!(
                "{} to {}",
                data::format_day_short(&self.span.since_day),
                data::format_day_short(&self.span.until_day)
            ))
        };
        h_flex()
            .flex_shrink_0()
            .min_h(px(48.))
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(border)
            .pl(px(20.))
            .pr(px(8.))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_x_3()
                    .py_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(div().text_color(muted).child("Usage"))
                            .child(div().text_color(muted).child("/"))
                            .child(self.environment_picker(cx))
                            .child(div().text_color(muted).child("/"))
                            .child(self.provider_picker(cx)),
                    )
                    .children(window_label.map(|label| {
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(label)
                    })),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .when(!limits, |row| row.child(period))
                    .child(metric)
                    .child(refresh),
            )
            .child(close)
            .into_any_element()
    }

    fn body(&self, vw: f32, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let limits = self.prefs.metric == Metric::Limits;
        let noun = if limits { "limits" } else { "usage" };
        let content = if self.access == Access::Unavailable {
            div()
                .text_sm()
                .text_color(muted)
                .child(format!("Connect an environment to see {noun}."))
                .into_any_element()
        } else if !self.env_selected {
            div()
                .text_sm()
                .text_color(muted)
                .child(format!("Select an environment to see {noun}."))
                .into_any_element()
        } else if limits {
            self.limits_body(vw, cx)
        } else if self.access == Access::Offline {
            div()
                .py_12()
                .text_center()
                .text_sm()
                .text_color(muted)
                .child("This environment is not connected.")
                .into_any_element()
        } else if self.shown.is_none() && !self.failed {
            self.skeleton(vw, cx)
        } else {
            self.overview(vw, cx)
        };
        let side = if vw >= 640. { 24. } else { 20. };
        v_flex()
            .mx_auto()
            .w_full()
            .max_w(px(CONTENT_MAX))
            .gap_6()
            .px(px(side))
            .pt(px(24.))
            .pb(px(48.))
            .child(content)
            .into_any_element()
    }

    fn skeleton(&self, vw: f32, cx: &App) -> AnyElement {
        let block = |w: Option<f32>, h: f32| skeleton_block(w, h, false, cx);
        let wide = vw >= 1024.;
        let rows = (0..8).map(|_| {
            h_flex()
                .items_center()
                .justify_between()
                .gap_4()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(skeleton_block(Some(8.), 8., true, cx))
                        .child(skeleton_block(Some(16.), 16., true, cx))
                        .child(block(Some(80.), 14.))
                        .child(block(Some(56.), 14.)),
                )
                .child(block(Some(144.), 16.))
        });
        let left = v_flex()
            .gap(px(20.))
            .child(
                v_flex()
                    .gap_1()
                    .child(block(Some(144.), 40.))
                    .child(block(Some(128.), 16.)),
            )
            .children(rows);
        let right = v_flex()
            .gap_3()
            .child(block(Some(96.), 20.))
            .child(div().ml(px(64.)).child(block(None, PLOT_HEIGHT)))
            .child(div().ml(px(64.)).child(block(None, 16.)));
        let totals = v_flex().gap_2().child(block(Some(64.), 20.)).child(
            div()
                .grid()
                .grid_cols(if vw >= 768. { 5 } else { 2 })
                .gap_x_6()
                .gap_y_4()
                .py_1()
                .children((0..5).map(|_| {
                    v_flex()
                        .gap(px(2.))
                        .child(block(Some(80.), 16.))
                        .child(block(Some(64.), 24.))
                })),
        );
        let shares = div()
            .grid()
            .grid_cols(if wide { 2 } else { 1 })
            .gap_x(px(48.))
            .gap_y_8()
            .children((0..2).map(|_| {
                v_flex()
                    .gap(px(10.))
                    .child(block(Some(112.), 20.))
                    .child(block(None, 8.))
                    .child(block(Some(288.), 16.))
            }));
        let breakdown = v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .child(block(Some(80.), 20.))
                    .child(block(Some(112.), 28.)),
            )
            .child(block(None, 176.));
        v_flex()
            .gap_6()
            .child(
                div()
                    .grid()
                    .grid_cols(if wide { 2 } else { 1 })
                    .gap_6()
                    .child(left)
                    .child(right),
            )
            .child(totals)
            .child(shares)
            .child(breakdown)
            .into_any_element()
    }
}

struct ChartPaint {
    grid: Vec<f64>,
    series: Vec<(Vec<data::Curve>, Hsla, bool)>,
    hover_x: Option<f64>,
    grid_color: Hsla,
    hover_color: Hsla,
}

fn curve_path(
    curves: &[data::Curve],
    map: &impl Fn(f64, f64) -> gpui::Point<Pixels>,
    mut builder: PathBuilder,
    close_to: Option<f64>,
) -> Option<gpui::Path<Pixels>> {
    let first = curves.first()?;
    let last = curves.last()?;
    builder.move_to(map(first.from.x, first.from.y));
    for curve in curves {
        builder.cubic_bezier_to(
            map(curve.to.x, curve.to.y),
            map(curve.c1.x, curve.c1.y),
            map(curve.c2.x, curve.c2.y),
        );
    }
    if let Some(bottom) = close_to {
        builder.line_to(map(last.to.x, bottom));
        builder.line_to(map(first.from.x, bottom));
        builder.close();
    }
    builder.build().ok()
}

impl ChartPaint {
    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let x_of = |x: f64| ox + x as f32 / data::VIEW_WIDTH as f32 * w;
        let y_of = |y: f64| oy + y as f32 / data::VIEW_HEIGHT as f32 * h;
        let map = |x: f64, y: f64| point(px(x_of(x)), px(y_of(y)));
        for y in &self.grid {
            let bounds = Bounds::new(point(px(ox), px(y_of(*y) - 0.5)), size(px(w), px(1.)));
            window.paint_quad(fill(bounds, self.grid_color));
        }
        for (curves, color, loading) in &self.series {
            let alpha = dimmed(*loading);
            if let Some(path) =
                curve_path(curves, &map, PathBuilder::fill(), Some(data::VIEW_HEIGHT))
            {
                window.paint_path(path, color.opacity(0.12 * alpha));
            }
        }
        for (curves, color, loading) in &self.series {
            if let Some(path) = curve_path(curves, &map, PathBuilder::stroke(px(2.)), None) {
                window.paint_path(path, color.opacity(dimmed(*loading)));
            }
        }
        if let Some(x) = self.hover_x {
            let top = y_of(data::PLOT_TOP);
            let line = Bounds::new(
                point(px(x_of(x) - 0.5), px(top)),
                size(px(1.), px(oy + h - top)),
            );
            window.paint_quad(fill(line, self.hover_color));
        }
    }
}

fn metric_format(metric: Metric) -> fn(f64) -> String {
    if metric == Metric::Tokens {
        data::format_tokens
    } else {
        data::format_usd
    }
}

fn section_title(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .child(text.into())
}

impl UsageDialog {
    fn overview(&self, vw: f32, cx: &mut Context<Self>) -> AnyElement {
        let empty = Merged::default();
        let (merged, span, sources) = match &self.shown {
            Some(shown) => (&shown.merged, &shown.span, &shown.summary.sources[..]),
            None => (&empty, &self.span, &[][..]),
        };
        let metric = if self.prefs.metric == Metric::Tokens {
            Metric::Tokens
        } else {
            Metric::Cost
        };
        let dim = self.dim();
        let partial = dim.partial();
        let with_data = data::active_providers(merged);
        let active: Vec<Provider> = Provider::ORDER
            .into_iter()
            .filter(|p| {
                with_data.contains(p) || matches!(&dim, Dim::Providers(list) if list.contains(p))
            })
            .collect();
        let theme = cx.theme();
        let (muted, foreground) = (theme.muted_foreground, theme.foreground);
        let wide = vw >= 1024.;

        let visible: Vec<_> = sources
            .iter()
            .filter(|source| !self.prefs.hidden.contains(source.provider))
            .cloned()
            .collect();
        let mut notes = data::source_messages(&visible);
        if let Some(mismatch) = merged.mismatch {
            notes.push(data::contract_mismatch_message(
                "This environment",
                mismatch,
            ));
        }
        let notes = notes
            .into_iter()
            .map(|message| div().mb_4().text_sm().text_color(muted).child(message));

        let big = if metric == Metric::Cost {
            data::format_usd(merged.cost)
        } else {
            data::format_tokens(merged.total_tokens)
        };
        let sessions = format!("{} sessions", data::format_count(merged.sessions));
        let unpriced = (metric == Metric::Cost && merged.unpriced_share > 0.0).then(|| {
            format!(
                "API estimate excludes {} unpriced records.",
                data::format_percent(merged.unpriced_share, 1)
            )
        });
        let mut rows: Vec<AnyElement> = active
            .iter()
            .map(|provider| {
                let provider = *provider;
                let totals = merged.providers.iter().find(|t| t.provider == provider);
                let share = totals.map_or(0.0, |t| {
                    if metric == Metric::Cost {
                        t.cost_share
                    } else {
                        t.token_share
                    }
                });
                let count = totals.map_or(0.0, |t| t.sessions);
                let loading = dim.provider(provider);
                let awaiting = loading && !with_data.contains(&provider);
                let (cost, tokens) = totals.map_or((0.0, 0.0), |t| (t.cost, t.tokens));
                let value = if awaiting {
                    "-".to_owned()
                } else if metric == Metric::Cost {
                    data::format_usd(cost)
                } else {
                    data::format_tokens(tokens)
                };
                let line = if metric == Metric::Cost {
                    format!(
                        "{} of cost \u{b7} {} tokens",
                        data::format_percent(share, 1),
                        data::format_tokens(tokens)
                    )
                } else {
                    format!(
                        "{} of tokens \u{b7} {}",
                        data::format_percent(share, 1),
                        data::format_usd(cost)
                    )
                };
                let noun = if count == 1.0 { "session" } else { "sessions" };
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .items_baseline()
                            .justify_between()
                            .gap_4()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .child(
                                        div()
                                            .size(px(8.))
                                            .flex_shrink_0()
                                            .rounded_full()
                                            .bg(tint(provider, cx)),
                                    )
                                    .child(mark(provider, 16., cx))
                                    .child(
                                        h_flex()
                                            .min_w_0()
                                            .items_baseline()
                                            .gap(px(6.))
                                            .child(div().truncate().child(provider.label()))
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .whitespace_nowrap()
                                                    .text_size(px(11.))
                                                    .text_color(muted)
                                                    .opacity(dimmed(loading))
                                                    .when(awaiting, |d| d.invisible())
                                                    .child(format!(
                                                        "{} {noun}",
                                                        data::format_count(count)
                                                    )),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .opacity(dimmed(loading))
                                    .child(value),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .opacity(dimmed(loading))
                            .when(awaiting, |d| d.invisible())
                            .child(line),
                    )
                    .into_any_element()
            })
            .collect();
        if let Some(row) = self.cursor_enable_row(cx) {
            let at = |provider: Provider| {
                active
                    .iter()
                    .position(|p| *p == provider)
                    .map_or(-1, |i| i as isize)
            };
            let index = (at(Provider::Codex).max(at(Provider::Claude)) + 1) as usize;
            rows.insert(index.min(rows.len()), row);
        }
        let chatgpt = self.chatgpt_sharing().then(|| self.chatgpt_row(cx));
        let left = v_flex()
            .min_w_0()
            .gap(px(20.))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(36.))
                            .line_height(px(40.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .opacity(dimmed(partial))
                            .child(big),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .text_color(muted)
                            .child(
                                h_flex()
                                    .child(div().opacity(dimmed(partial)).child(sessions))
                                    .when(metric == Metric::Cost, |row| {
                                        row.child(" \u{b7} API estimate")
                                    }),
                            )
                            .children(unpriced.map(|text| {
                                div()
                                    .id("usage-unpriced")
                                    .child(Icon::new(Icons::Info).size(px(12.)))
                                    .tooltip(tooltip(text))
                            })),
                    ),
            )
            .children(chatgpt)
            .children(rows);

        let hourly = span.hourly;
        let heading = format!(
            "{} {}",
            if hourly { "Hourly" } else { "Daily" },
            if metric == Metric::Tokens {
                "processed tokens"
            } else {
                "cost"
            }
        );
        let loading_providers: Vec<Provider> = active
            .iter()
            .copied()
            .filter(|p| dim.provider(*p))
            .collect();
        let right = v_flex()
            .min_w_0()
            .gap_3()
            .child(section_title(heading))
            .child(self.chart(merged, span, metric, &active, &loading_providers, cx));

        let totals_cell = |label: &'static str, value: String| {
            v_flex()
                .gap(px(2.))
                .opacity(dimmed(partial))
                .child(div().text_xs().text_color(muted).child(label))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::MEDIUM)
                        .child(value),
                )
        };
        let totals = v_flex().gap_2().child(section_title("Totals")).child(
            div()
                .grid()
                .grid_cols(if vw >= 768. { 5 } else { 2 })
                .gap_x_6()
                .gap_y_4()
                .py_1()
                .child(totals_cell(
                    "Processed tokens",
                    data::format_tokens(merged.total_tokens),
                ))
                .child(totals_cell(
                    "Cached input",
                    data::format_tokens(merged.tokens.cached),
                ))
                .child(totals_cell(
                    "Uncached input",
                    data::format_tokens(merged.tokens.uncached),
                ))
                .child(totals_cell(
                    "Output",
                    data::format_tokens(merged.tokens.output),
                ))
                .child(totals_cell(
                    "Cache savings",
                    data::format_usd(merged.cache_savings),
                )),
        );

        let shares = (merged.total_tokens > 0.0).then(|| {
            let mut bars: Vec<Option<gpui::Div>> = Vec::new();
            if metric == Metric::Tokens {
                bars.push(share_bar(
                    "Tokens by type",
                    None,
                    &data::token_type_segments(&merged.tokens),
                    data::format_tokens,
                    cx,
                ));
            } else {
                bars.push(share_bar(
                    "Cost by type",
                    None,
                    &data::cost_type_segments(&merged.category_cost),
                    data::format_usd,
                    cx,
                ));
                if merged.speed_cost.fast + merged.speed_cost.ultrafast > 0.0 {
                    let aside = h_flex()
                        .gap_1()
                        .text_xs()
                        .text_color(muted)
                        .child("Premium")
                        .child(
                            div()
                                .text_color(foreground)
                                .child(data::format_usd(merged.speed_cost.premium)),
                        );
                    bars.push(share_bar(
                        "Cost by speed",
                        Some(aside),
                        &data::speed_cost_segments(&merged.speed_cost),
                        data::format_usd,
                        cx,
                    ));
                }
            }
            div()
                .grid()
                .grid_cols(if wide { 2 } else { 1 })
                .gap_x(px(48.))
                .gap_y_8()
                .opacity(dimmed(partial))
                .children(bars.into_iter().flatten())
        });

        let breakdown = self.breakdown(vw, merged, span, metric, &active, &dim, cx);

        v_flex()
            .gap_6()
            .children(notes)
            .child(
                div()
                    .grid()
                    .grid_cols(if wide { 2 } else { 1 })
                    .gap_6()
                    .child(left)
                    .child(right),
            )
            .child(totals)
            .children(shares)
            .child(breakdown)
            .into_any_element()
    }

    fn chart(
        &self,
        merged: &Merged,
        span: &Span,
        metric: Metric,
        active: &[Provider],
        loading: &[Provider],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hourly = span.hourly;
        let periods: Vec<String> = match (&span.since_time, &span.until_time) {
            (Some(since), Some(until)) if hourly => data::enumerate_hour_starts(since, until),
            _ => data::enumerate_days(&span.since_day, &span.until_day),
        };
        let totals = if hourly {
            &merged.hourly
        } else {
            &merged.daily
        };
        let chart = data::build_chart(&periods, totals, metric, active);
        let peak = chart
            .columns
            .iter()
            .flat_map(|c| c.bands.iter().map(|(_, v)| *v))
            .fold(0.0, f64::max);
        let placeholder = peak == 0.0 && !loading.is_empty();
        let (max, ticks) = if placeholder {
            (4.0, vec![0.0, 1.0, 2.0, 3.0, 4.0])
        } else {
            (chart.max, chart.ticks.clone())
        };
        let format = metric_format(metric);
        let zone = self.zone.clone();
        let label_of = {
            let zone = zone.clone();
            move |period: &str| {
                if hourly {
                    data::format_hour_short(period, &zone)
                } else {
                    data::format_day_short(period)
                }
            }
        };
        let theme = cx.theme();
        let (muted, border, popover) = (theme.muted_foreground, theme.border, theme.popover);
        let ylabels = (!placeholder).then(|| {
            ticks.iter().map(|tick| {
                let y = data::value_to_y(*tick, max) / data::VIEW_HEIGHT * f64::from(PLOT_HEIGHT);
                div()
                    .absolute()
                    .right_0()
                    .top(px(y as f32 - 7.))
                    .h(px(14.))
                    .text_size(px(10.))
                    .line_height(px(14.))
                    .text_color(muted)
                    .child(if *tick == 0.0 {
                        "0".to_owned()
                    } else {
                        format(*tick)
                    })
            })
        });
        let hover = self.hover.filter(|(index, _)| *index < periods.len());
        let paint = ChartPaint {
            grid: ticks.iter().map(|t| data::value_to_y(*t, max)).collect(),
            series: chart
                .series
                .iter()
                .map(|s| {
                    (
                        s.curve.clone(),
                        tint(s.provider, cx),
                        loading.contains(&s.provider),
                    )
                })
                .collect(),
            hover_x: hover.map(|(index, _)| index as f64 * chart.step_x),
            grid_color: border,
            hover_color: muted,
        };
        let count = periods.len();
        let plot_cell = self.plot.clone();
        let tip_cell = self.tip.clone();
        let tip_card = hover.and_then(|(index, at)| {
            let column = chart.columns.get(index)?;
            let period = periods.get(index)?;
            let label = match (&span.until_time, hourly) {
                (Some(until), true) => data::format_relative_hour_short(period, until, &zone),
                _ => label_of(period),
            };
            let plot = self.plot.get();
            let (pw, ph) = (f32::from(plot.size.width), f32::from(plot.size.height));
            let tip = self.tip.get();
            let (tw, th) = (f32::from(tip.width), f32::from(tip.height));
            let (x, y) = (f32::from(at.x), f32::from(at.y));
            let left = if x + 12. + tw <= pw {
                x + 12.
            } else {
                x - 12. - tw
            };
            let top = if y + 12. + th <= ph {
                y + 12.
            } else {
                y - 12. - th
            };
            let left = left.max(0.).min((pw - tw).max(0.));
            let top = top.max(0.).min((ph - th).max(0.));
            let partial = active.iter().any(|p| loading.contains(p));
            let rows = active.iter().map(|provider| {
                let value = column
                    .bands
                    .iter()
                    .find(|(p, _)| p == provider)
                    .map_or(0.0, |(_, v)| *v);
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(muted)
                            .child(mark(*provider, 12., cx))
                            .child(provider.label()),
                    )
                    .child(
                        div()
                            .text_color(if loading.contains(provider) {
                                muted
                            } else {
                                cx.theme().foreground
                            })
                            .child(format(value)),
                    )
            });
            Some(
                v_flex()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .min_w(px(144.))
                    .max_w_full()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(border.opacity(0.5))
                    .bg(popover)
                    .px(px(10.))
                    .py_2()
                    .text_xs()
                    .shadow_lg()
                    .child(div().mb_1().text_color(muted).child(label))
                    .children(rows)
                    .child(
                        h_flex()
                            .mt_1()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .border_t_1()
                            .border_color(border)
                            .pt_1()
                            .child(div().text_color(muted).child("Total"))
                            .child(
                                div()
                                    .text_color(if partial {
                                        muted
                                    } else {
                                        cx.theme().foreground
                                    })
                                    .child(format(column.total)),
                            ),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| tip_cell.set(bounds.size),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
        });
        let plot = div()
            .id("usage-plot")
            .relative()
            .h(px(PLOT_HEIGHT))
            .flex_1()
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                let bounds = this.plot.get();
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                if w <= 0. || count == 0 {
                    return;
                }
                let x = (f32::from(event.position.x - bounds.origin.x)).clamp(0., w);
                let y = (f32::from(event.position.y - bounds.origin.y)).clamp(0., h);
                let index = ((x / w) * (count - 1) as f32).round() as usize;
                this.hover = Some((index.min(count - 1), point(px(x), px(y))));
                cx.notify();
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered && this.hover.take().is_some() {
                    cx.notify();
                }
            }))
            .child(
                canvas(
                    move |bounds, _, _| plot_cell.set(bounds),
                    move |bounds, _, window, _| paint.paint(bounds, window),
                )
                .absolute()
                .inset_0(),
            )
            .children(tip_card);
        let first = periods.first().map(|p| label_of(p)).unwrap_or_default();
        let middle = periods
            .get(periods.len() / 2)
            .map(|p| label_of(p))
            .unwrap_or_default();
        let last = periods.last().map(|p| label_of(p)).unwrap_or_default();
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .relative()
                            .h(px(PLOT_HEIGHT))
                            .w(px(56.))
                            .flex_shrink_0()
                            .children(ylabels.into_iter().flatten()),
                    )
                    .child(plot),
            )
            .child(
                h_flex()
                    .justify_between()
                    .pl(px(64.))
                    .text_size(px(10.))
                    .text_color(muted)
                    .child(first.to_uppercase())
                    .child(middle.to_uppercase())
                    .child(last.to_uppercase()),
            )
            .into_any_element()
    }
}

/// `UsageShareBar`; nothing when no segment has a value.
fn share_bar(
    label: &'static str,
    aside: Option<gpui::Div>,
    segments: &[data::Segment],
    format: fn(f64) -> String,
    cx: &App,
) -> Option<gpui::Div> {
    let visible: Vec<&data::Segment> = segments.iter().filter(|s| s.value > 0.0).collect();
    let total: f64 = visible.iter().map(|s| s.value).sum();
    if total <= 0.0 {
        return None;
    }
    let theme = cx.theme();
    let (muted, foreground, background) =
        (theme.muted_foreground, theme.foreground, theme.background);
    let color = |segment: &data::Segment| {
        background.blend(foreground.opacity(f32::from(segment.ink) / 100.))
    };
    let last = visible.len() - 1;
    let bar = h_flex()
        .h(px(8.))
        .gap(px(2.))
        .children(visible.iter().enumerate().map(|(index, segment)| {
            let tip = format!(
                "{} \u{b7} {} \u{b7} {}",
                segment.label,
                format(segment.value),
                data::format_percent(segment.value / total, 1)
            );
            div()
                .id(("usage-share", index))
                .h_full()
                .min_w(px(4.))
                .w(relative((segment.value / total) as f32))
                .rounded(px(2.))
                .when(index == 0, |d| d.rounded_tl(px(4.)).rounded_bl(px(4.)))
                .when(index == last, |d| d.rounded_tr(px(4.)).rounded_br(px(4.)))
                .bg(color(segment))
                .tooltip(tooltip(tip))
        }));
    let legend = h_flex()
        .flex_wrap()
        .gap_x_4()
        .gap_y_1()
        .text_xs()
        .children(visible.iter().map(|segment| {
            h_flex()
                .items_center()
                .gap(px(6.))
                .child(div().size(px(8.)).rounded(px(2.)).bg(color(segment)))
                .child(div().text_color(muted).child(segment.label))
                .child(div().text_color(foreground).child(format(segment.value)))
        }));
    Some(
        v_flex()
            .min_w_0()
            .gap(px(10.))
            .child(
                h_flex()
                    .items_baseline()
                    .justify_between()
                    .gap_3()
                    .child(section_title(label))
                    .children(aside),
            )
            .child(bar)
            .child(legend),
    )
}

impl UsageDialog {
    #[allow(clippy::too_many_arguments)]
    fn breakdown(
        &self,
        vw: f32,
        merged: &Merged,
        span: &Span,
        metric: Metric,
        active: &[Provider],
        dim: &Dim,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let time = self.breakdown_time;
        let time_label = if span.hourly { "Hour" } else { "Day" };
        let switch = toggle_group(cx)
            .child(toggle(
                "usage-breakdown-model",
                "Model",
                "Model".to_owned(),
                !time,
                cx.listener(|this, _, _, cx| {
                    this.breakdown_time = false;
                    cx.notify();
                }),
                cx,
            ))
            .child(toggle(
                "usage-breakdown-time",
                time_label,
                time_label.to_owned(),
                time,
                cx.listener(|this, _, _, cx| {
                    this.breakdown_time = true;
                    cx.notify();
                }),
                cx,
            ));
        let table = if time {
            self.time_table(merged, span, active, dim, cx)
        } else {
            self.model_table(vw, merged, metric, dim, cx)
        };
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_title("Breakdown"))
                    .child(switch),
            )
            .child(table)
            .into_any_element()
    }

    fn model_table(
        &self,
        vw: f32,
        merged: &Merged,
        metric: Metric,
        dim: &Dim,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = cx.theme();
        let (muted, border, foreground, hover) = (
            theme.muted_foreground,
            theme.border,
            theme.foreground,
            theme.muted.opacity(0.5),
        );
        let wide = vw >= 640.;
        let by_tokens = metric == Metric::Tokens;
        let models = if by_tokens {
            data::sort_models_by_tokens(&merged.models)
        } else {
            merged.models.clone()
        };
        let value_of = |m: &data::ModelTotals| if by_tokens { m.tokens } else { m.cost };
        let peak = models.iter().map(value_of).fold(0.0, f64::max);
        let head = |label: &'static str, width: Option<f32>| {
            div()
                .when_some(width, |d, w| d.w(px(w)).flex_shrink_0())
                .when(width.is_none(), |d| d.flex_1())
                .text_right()
                .child(label)
        };
        let header = h_flex()
            .py_2()
            .px_2()
            .gap_x(px(0.))
            .border_b_1()
            .border_color(border)
            .text_xs()
            .text_color(muted)
            .child(div().w(px(36.)).flex_shrink_0().text_left().child("#"))
            .child(head("Model", None).text_left())
            .child(head("Cost", Some(96.)))
            .when(wide, |row| row.child(head("Share", Some(80.))))
            .child(head("Tokens", Some(96.)));
        let rows = models.iter().enumerate().map(|(index, model)| {
            let loading = dim.provider(model.provider);
            let value = value_of(model);
            let bar = value > 0.0 && peak > 0.0;
            let cost = if model.cost_unknown() {
                div().text_color(muted).child("Unpriced")
            } else {
                div()
                    .text_color(foreground)
                    .child(data::format_usd(model.cost))
            };
            let share = data::model_share(model, metric)
                .map(|s| data::format_percent(s, 1))
                .unwrap_or_default();
            let tint = tint(model.provider, cx);
            h_flex()
                .id(("usage-model", index))
                .px_2()
                .gap_x(px(0.))
                .border_b_1()
                .border_color(border.opacity(0.5))
                .text_sm()
                .text_color(muted)
                .opacity(dimmed(loading))
                .hover(move |style| style.bg(hover))
                .child(
                    div()
                        .w(px(36.))
                        .flex_shrink_0()
                        .py(px(10.))
                        .text_xs()
                        .child((index + 1).to_string()),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .py(px(if bar { 10. } else { 14. }))
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .text_color(foreground)
                                .child(mark(model.provider, 14., cx))
                                .child(div().truncate().child(model.model.clone())),
                        )
                        .when(bar, |col| {
                            col.child(
                                div().mt(px(6.)).h(px(2.)).max_w(px(192.)).child(
                                    div()
                                        .h_full()
                                        .min_w(px(8.))
                                        .w(relative((value / peak) as f32))
                                        .rounded_full()
                                        .bg(tint),
                                ),
                            )
                        }),
                )
                .child(
                    div()
                        .w(px(96.))
                        .flex_shrink_0()
                        .text_right()
                        .py(px(10.))
                        .child(cost),
                )
                .when(wide, |row| {
                    row.child(
                        div()
                            .w(px(80.))
                            .flex_shrink_0()
                            .text_right()
                            .py(px(10.))
                            .child(share),
                    )
                })
                .child(
                    div()
                        .w(px(96.))
                        .flex_shrink_0()
                        .text_right()
                        .py(px(10.))
                        .child(data::format_tokens(model.tokens)),
                )
        });
        v_flex()
            .child(header)
            .when(models.is_empty(), |t| {
                t.child(
                    div()
                        .py_6()
                        .text_center()
                        .text_sm()
                        .text_color(muted)
                        .child("No activity in this window."),
                )
            })
            .children(rows)
    }

    fn time_table(
        &self,
        merged: &Merged,
        span: &Span,
        active: &[Provider],
        dim: &Dim,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = cx.theme();
        let (muted, border, foreground, hover) = (
            theme.muted_foreground,
            theme.border,
            theme.foreground,
            theme.muted.opacity(0.5),
        );
        let hourly = span.hourly;
        let periods: Vec<&data::PeriodTotals> = if hourly {
            merged.hourly.iter().rev().collect()
        } else {
            merged.daily.iter().rev().collect()
        };
        let zone = self.zone.clone();
        let partial = dim.partial();
        let cell = |width: f32| div().w(px(width)).flex_shrink_0().text_right();
        let header = h_flex()
            .py_2()
            .px_2()
            .border_b_1()
            .border_color(border)
            .text_xs()
            .text_color(muted)
            .child(
                div()
                    .flex_1()
                    .min_w(px(120.))
                    .child(if hourly { "Hour" } else { "Day" }),
            )
            .children(active.iter().map(|p| cell(96.).child(p.label())))
            .child(cell(96.).child("Total"))
            .child(cell(96.).child("Tokens"));
        let rows = periods.iter().enumerate().map(|(index, period)| {
            let label = if hourly {
                data::format_hour_short(&period.key, &zone)
            } else {
                data::format_day_short(&period.key)
            };
            h_flex()
                .id(("usage-period", index))
                .px_2()
                .py_2()
                .border_b_1()
                .border_color(border.opacity(0.5))
                .text_sm()
                .whitespace_nowrap()
                .hover(move |style| style.bg(hover))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(120.))
                        .text_color(foreground)
                        .child(label),
                )
                .children(active.iter().map(|provider| {
                    let cost = period
                        .by_provider
                        .iter()
                        .find(|(p, _)| p == provider)
                        .map_or(0.0, |(_, share)| share.cost);
                    cell(96.)
                        .text_color(muted)
                        .opacity(dimmed(dim.provider(*provider)))
                        .child(data::format_usd(cost))
                }))
                .child(
                    cell(96.)
                        .text_color(foreground)
                        .opacity(dimmed(partial))
                        .child(data::format_usd(period.cost)),
                )
                .child(
                    cell(96.)
                        .text_color(muted)
                        .opacity(dimmed(partial))
                        .child(data::format_tokens(period.tokens)),
                )
        });
        v_flex()
            .overflow_x_hidden()
            .child(header)
            .when(periods.is_empty(), |t| {
                t.child(
                    div()
                        .py_6()
                        .text_center()
                        .text_sm()
                        .text_color(muted)
                        .child("No activity in this window."),
                )
            })
            .children(rows)
    }
}
