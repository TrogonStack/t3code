//! The toast widget (`ui/toast.tsx`, `ui/toast.logic.ts`) on Base UI's toast store semantics.
//!
//! `ToastHost` is an entity the window renders last: it owns the stack, the timers and the
//! hover state, and draws the cards at the web's top-right corner. Everything else goes through
//! `add`, `update` and `dismiss` by `ToastId`.
//!
//! ```ignore
//! let id = host.update(cx, |host, cx| {
//!     host.add(
//!         Toast::new("Some requests are slow")
//!             .kind(Kind::Warning)
//!             .description("2 requests waiting longer than 15s.")
//!             .timeout(Timeout::Never)
//!             .collapse(Collapse::new("Show requests", "Hide requests").on_description())
//!             .body(|_, _| div().child("any element").into_any_element()),
//!         cx,
//!     )
//! });
//! host.update(cx, |host, cx| host.dismiss(id, cx));
//! ```

// The widget is the web's whole surface; the first caller (slow requests) uses part of it.
#![allow(dead_code)]

use crate::details_card::tip;
use gpui::{
    AnyElement, App, BoxShadow, ClipboardItem, Context, Div, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window, div, point, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    h_flex,
    spinner::Spinner,
    v_flex,
};
use gpui_kit_assets::IconName as Icons;
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

/// Base UI's provider `timeout` default.
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(5000);
/// Base UI's provider `limit` default: older toasts are hidden.
const LIMIT: usize = 3;
/// `max-w-90`.
const MAX_WIDTH: f32 = 360.;
/// `--toast-inset`: `--spacing(4)`, `--spacing(8)` from the `sm` breakpoint.
const INSET: f32 = 16.;
const INSET_WIDE: f32 = 32.;
const SM_BREAKPOINT: f32 = 640.;
/// `--toast-gap` between expanded cards and `--toast-peek` between collapsed ones.
const GAP: f32 = 12.;
const PEEK: f32 = 12.;
/// Each collapsed card behind the front one loses `.1` of scale.
const SCALE_STEP: f32 = 0.1;
/// `useCopyToClipboard`'s default.
const COPIED_FOR: Duration = Duration::from_millis(2000);
/// `ERROR_DESCRIPTION_CLAMP_MIN_CHARS`, clamped to `line-clamp-4`.
const ERROR_CLAMP_MIN_CHARS: usize = 180;
const ERROR_CLAMP_LINES: usize = 4;
/// `toastExpandablePanelClassName`'s `max-h-40`.
const PANEL_MAX_HEIGHT: f32 = 160.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToastId(u64);

/// The web's toast `type`, which picks the icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Success,
    Error,
    Warning,
    Info,
    Loading,
}

/// The web's `timeout`: unset, `0`, or milliseconds. A loading toast never times out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Timeout {
    #[default]
    Default,
    Never,
    After(Duration),
}

impl Timeout {
    fn resolve(self, kind: Option<Kind>) -> Option<Duration> {
        if kind == Some(Kind::Loading) {
            return None;
        }
        match self {
            Self::Default => Some(DEFAULT_TIMEOUT),
            Self::Never => None,
            Self::After(duration) => (!duration.is_zero()).then_some(duration),
        }
    }
}

/// The web's `actionVariant` and `secondaryActionVariant` defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    #[default]
    Primary,
    Outline,
}

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;
type BodyBuilder = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type CloseCallback = Rc<dyn Fn(&mut App)>;

#[derive(Clone)]
pub struct Action {
    label: SharedString,
    variant: Variant,
    on_click: Callback,
}

impl Action {
    pub fn new(
        label: impl Into<SharedString>,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            variant: Variant::default(),
            on_click: Rc::new(on_click),
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }
}

/// The web's `expandableLabels` and `expandableDescriptionTrigger`.
#[derive(Clone)]
pub struct Collapse {
    expand: SharedString,
    collapse: SharedString,
    on_description: bool,
}

impl Collapse {
    pub fn new(expand: impl Into<SharedString>, collapse: impl Into<SharedString>) -> Self {
        Self {
            expand: expand.into(),
            collapse: collapse.into(),
            on_description: false,
        }
    }

    /// Makes the description row the toggle, with the chevron at its end.
    pub fn on_description(mut self) -> Self {
        self.on_description = true;
        self
    }
}

impl Default for Collapse {
    fn default() -> Self {
        Self::new("Show details", "Hide details")
    }
}

#[derive(Clone)]
pub struct Toast {
    kind: Option<Kind>,
    title: SharedString,
    description: Option<SharedString>,
    actions: Vec<Action>,
    stacked_actions: bool,
    hide_copy: bool,
    timeout: Timeout,
    body: Option<BodyBuilder>,
    collapse: Option<Collapse>,
    on_close: Option<CloseCallback>,
}

impl Toast {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            kind: None,
            title: title.into(),
            description: None,
            actions: Vec::new(),
            stacked_actions: false,
            hide_copy: false,
            timeout: Timeout::default(),
            body: None,
            collapse: None,
            on_close: None,
        }
    }

    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn action(mut self, action: Action) -> Self {
        self.actions.push(action);
        self
    }

    /// The web's `actionLayout: "stacked-end"`: actions under the text, right-aligned.
    pub fn stacked_actions(mut self) -> Self {
        self.stacked_actions = true;
        self
    }

    /// The web's `hideCopyButton`; error toasts with a description otherwise offer to copy it.
    pub fn hide_copy(mut self) -> Self {
        self.hide_copy = true;
        self
    }

    pub fn timeout(mut self, timeout: Timeout) -> Self {
        self.timeout = timeout;
        self
    }

    /// Any element under the description, built on each render so it can read live state. It
    /// scrolls past `max-h-40`; with `collapse` it starts hidden.
    pub fn body(mut self, body: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.body = Some(Rc::new(body));
        self
    }

    pub fn collapse(mut self, collapse: Collapse) -> Self {
        self.collapse = Some(collapse);
        self
    }

    /// Runs after the toast leaves, whether the user, a timeout or `dismiss` closed it.
    pub fn on_close(mut self, on_close: impl Fn(&mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(on_close));
        self
    }

    fn copy_text(&self) -> Option<&SharedString> {
        self.description
            .as_ref()
            .filter(|_| self.kind == Some(Kind::Error) && !self.hide_copy)
    }

    fn has_trailing_controls(&self) -> bool {
        self.copy_text().is_some() || !self.actions.is_empty()
    }
}

/// Base UI's pausable timer: `remaining` counts down only while `started` is set.
struct Timer {
    delay: Duration,
    remaining: Duration,
    started: Option<Instant>,
}

impl Timer {
    fn elapsed(&self, now: Instant) -> Duration {
        self.started.map_or(Duration::ZERO, |started| {
            now.saturating_duration_since(started)
        })
    }

    fn left(&self, now: Instant) -> Option<Duration> {
        self.started
            .map(|_| self.remaining.saturating_sub(self.elapsed(now)))
    }
}

struct Entry {
    id: ToastId,
    toast: Toast,
    timer: Option<Timer>,
    open: bool,
    copied: bool,
}

/// The toasts newest first, which is also the stacking order, and their timers.
#[derive(Default)]
struct Stack {
    entries: Vec<Entry>,
    next_id: u64,
    paused: bool,
}

impl Stack {
    fn timer(&self, toast: &Toast, now: Instant) -> Option<Timer> {
        let delay = toast.timeout.resolve(toast.kind)?;
        Some(Timer {
            delay,
            remaining: delay,
            started: (!self.paused).then_some(now),
        })
    }

    fn add(&mut self, toast: Toast, now: Instant) -> ToastId {
        let id = ToastId(self.next_id);
        self.next_id += 1;
        let timer = self.timer(&toast, now);
        self.entries.insert(
            0,
            Entry {
                id,
                toast,
                timer,
                open: false,
                copied: false,
            },
        );
        id
    }

    /// Replaces the toast's content; the timer restarts only when its length changed.
    fn update(&mut self, id: ToastId, toast: Toast, now: Instant) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let delay = toast.timeout.resolve(toast.kind);
        if self.entries[index].timer.as_ref().map(|timer| timer.delay) != delay {
            self.entries[index].timer = self.timer(&toast, now);
        }
        self.entries[index].toast = toast;
        true
    }

    fn dismiss(&mut self, id: ToastId) -> Option<Toast> {
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        Some(self.entries.remove(index).toast)
    }

    fn set_paused(&mut self, paused: bool, now: Instant) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        for timer in self
            .entries
            .iter_mut()
            .filter_map(|entry| entry.timer.as_mut())
        {
            if paused {
                if let Some(left) = timer.left(now) {
                    timer.remaining = left;
                    timer.started = None;
                }
            } else {
                if timer.remaining.is_zero() {
                    timer.remaining = timer.delay;
                }
                timer.started = Some(now);
            }
        }
    }

    /// How long until the next running timer fires.
    fn next_expiry(&self, now: Instant) -> Option<Duration> {
        self.entries
            .iter()
            .filter_map(|entry| entry.timer.as_ref()?.left(now))
            .min()
    }

    fn expired(&self, now: Instant) -> Vec<ToastId> {
        self.entries
            .iter()
            .filter(|entry| {
                entry
                    .timer
                    .as_ref()
                    .and_then(|timer| timer.left(now))
                    .is_some_and(|left| left.is_zero())
            })
            .map(|entry| entry.id)
            .collect()
    }

    fn entry_mut(&mut self, id: ToastId) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.id == id)
    }
}

/// The scale of the card `index` places behind the front one while the stack is collapsed.
fn collapsed_scale(index: usize) -> f32 {
    (1. - index as f32 * SCALE_STEP).max(0.)
}

/// The web's `--toast-inset` for a window width.
fn inset(window_width: f32) -> f32 {
    if window_width >= SM_BREAKPOINT {
        INSET_WIDE
    } else {
        INSET
    }
}

/// `w-[calc(100%-var(--toast-inset)*2)] max-w-90`.
fn toast_width(window_width: f32) -> f32 {
    (window_width - inset(window_width) * 2.).clamp(0., MAX_WIDTH)
}

fn clamp_lines(kind: Option<Kind>, description: &str) -> Option<usize> {
    (kind == Some(Kind::Error) && description.chars().count() >= ERROR_CLAMP_MIN_CHARS)
        .then_some(ERROR_CLAMP_LINES)
}

/// Renders the stack at the window's top-right corner, below the top bar.
pub struct ToastHost {
    stack: Stack,
    top_offset: f32,
    hovered: bool,
    window_active: bool,
    /// Bumped on every reschedule so a stale timer task does nothing.
    epoch: u64,
    _activation: Subscription,
}

impl ToastHost {
    /// `top_offset` is the web's `--toast-header-offset`, the top bar's height.
    pub fn new(top_offset: f32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _activation = cx.observe_window_activation(window, |this, window, cx| {
            this.window_active = window.is_window_active();
            this.sync_pause(cx);
        });
        Self {
            stack: Stack::default(),
            top_offset,
            hovered: false,
            window_active: window.is_window_active(),
            epoch: 0,
            _activation,
        }
    }

    pub fn add(&mut self, toast: Toast, cx: &mut Context<Self>) -> ToastId {
        let id = self.stack.add(toast, Instant::now());
        self.sync_pause(cx);
        id
    }

    /// Replaces a toast's content; does nothing once it is gone, as the web's `update` does.
    pub fn update(&mut self, id: ToastId, toast: Toast, cx: &mut Context<Self>) {
        if self.stack.update(id, toast, Instant::now()) {
            self.sync_pause(cx);
        }
    }

    pub fn dismiss(&mut self, id: ToastId, cx: &mut Context<Self>) {
        let Some(toast) = self.stack.dismiss(id) else {
            return;
        };
        if let Some(on_close) = toast.on_close {
            // The callback may reach back into this entity, which is borrowed right now.
            cx.defer(move |cx| on_close(cx));
        }
        self.reschedule(cx);
    }

    /// Timers stand still while the stack is hovered (Base UI's `expanded`) or the window is not
    /// the active one.
    fn sync_pause(&mut self, cx: &mut Context<Self>) {
        self.stack
            .set_paused(self.hovered || !self.window_active, Instant::now());
        self.reschedule(cx);
    }

    fn reschedule(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        let epoch = self.epoch;
        cx.notify();
        let Some(wait) = self.stack.next_expiry(Instant::now()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |this, cx| {
                if this.epoch != epoch {
                    return;
                }
                for id in this.stack.expired(Instant::now()) {
                    this.dismiss(id, cx);
                }
                this.reschedule(cx);
            })
            .ok();
        })
        .detach();
    }

    fn copy(&mut self, id: ToastId, text: SharedString, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        if let Some(entry) = self.stack.entry_mut(id) {
            entry.copied = true;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            this.update(cx, |this, cx| {
                if let Some(entry) = this.stack.entry_mut(id) {
                    entry.copied = false;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn toggle(&mut self, id: ToastId, cx: &mut Context<Self>) {
        if let Some(entry) = self.stack.entry_mut(id) {
            entry.open = !entry.open;
            cx.notify();
        }
    }
}

struct Palette {
    dark: bool,
    foreground: Hsla,
    muted: Hsla,
    popover: Hsla,
    /// `border-border/60`.
    border: Hsla,
    /// `dropdown-glass`'s `border-contrast-foreground/10`.
    glass_border: Hsla,
}

impl Palette {
    fn new(cx: &App) -> Self {
        let theme = cx.theme();
        let dark = theme.is_dark();
        Self {
            dark,
            foreground: theme.foreground,
            muted: theme.muted_foreground,
            popover: theme.popover,
            border: theme.border.opacity(0.6),
            glass_border: if dark { gpui::white() } else { gpui::black() }.opacity(0.1),
        }
    }

    /// `text-destructive`, `text-info`, `text-success` and `text-warning`.
    fn kind(&self, kind: Kind) -> Hsla {
        match kind {
            Kind::Error if self.dark => gpui::rgb(0xf15757),
            Kind::Error => gpui::rgb(0xef4444),
            Kind::Info => gpui::rgb(0x3b82f6),
            Kind::Success => gpui::rgb(0x10b981),
            Kind::Warning => gpui::rgb(0xf59e0b),
            Kind::Loading => return self.foreground.opacity(0.8),
        }
        .into()
    }
}

/// `shadow-xl shadow-black/25`.
fn shadow() -> Vec<BoxShadow> {
    let color = gpui::black().opacity(0.25);
    vec![
        BoxShadow {
            color,
            offset: point(px(0.), px(20.)),
            blur_radius: px(25.),
            spread_radius: px(-5.),
            inset: false,
        },
        BoxShadow {
            color,
            offset: point(px(0.), px(8.)),
            blur_radius: px(10.),
            spread_radius: px(-6.),
            inset: false,
        },
    ]
}

/// The card's own box: `dropdown-glass rounded-lg`, painted opaque because GPUI has no backdrop
/// blur to make the glass's 84% fill legible over the content.
fn surface(palette: &Palette) -> Div {
    div()
        .rounded(px(8.))
        .border_1()
        .border_color(palette.glass_border)
        .bg(palette.popover)
}

fn type_icon(kind: Kind, palette: &Palette) -> AnyElement {
    let color = palette.kind(kind);
    let glyph: AnyElement = match kind {
        Kind::Loading => Spinner::new()
            .icon(Icon::new(Icons::LoaderCircle))
            .color(color)
            .with_size(Size::Size(px(16.)))
            .into_any_element(),
        _ => Icon::new(match kind {
            Kind::Error => Icons::CircleAlert,
            Kind::Info => Icons::Info,
            Kind::Success => Icons::CircleCheck,
            _ => Icons::TriangleAlert,
        })
        .size(px(16.))
        .text_color(color)
        .into_any_element(),
    };
    // `[&>svg]:h-lh [&>svg]:w-4`: a 16px glyph centered in the 20px line.
    div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(px(16.))
        .h(px(20.))
        .child(glyph)
        .into_any_element()
}

fn chevron(open: bool, palette: &Palette) -> Icon {
    Icon::new(if open {
        Icons::ChevronUp
    } else {
        Icons::ChevronDown
    })
    .size(px(14.))
    .flex_shrink_0()
    .text_color(palette.muted.opacity(0.8))
}

impl ToastHost {
    fn card(
        &self,
        entry: &Entry,
        palette: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = entry.id;
        let toast = &entry.toast;
        let stacked = toast.stacked_actions && !toast.actions.is_empty();
        let trailing = toast.has_trailing_controls();

        let title = h_flex()
            .min_w_0()
            .gap(px(8.))
            .when(!stacked && !trailing, |row| row.pr(px(24.)))
            .children(toast.kind.map(|kind| type_icon(kind, palette)))
            .child(
                div()
                    .min_w_0()
                    .font_weight(FontWeight::MEDIUM)
                    .child(toast.title.clone()),
            );

        let description = |palette: &Palette| {
            toast.description.as_ref().map(|text| {
                div()
                    .min_w_0()
                    .text_color(palette.muted)
                    .when_some(clamp_lines(toast.kind, text), |div, lines| {
                        div.line_clamp(lines)
                    })
                    .child(text.clone())
            })
        };

        let body = toast.body.as_ref().map(|build| build(window, cx));
        let panel = |body: AnyElement, scrolls: bool| {
            div()
                .id(("toast-body", id.0))
                .mt(px(8.))
                .max_h(px(PANEL_MAX_HEIGHT))
                .min_h_0()
                .pr(px(2.))
                .overflow_y_scroll()
                .when(!scrolls, |panel| panel.mt_0())
                .child(body)
        };

        let mut text = v_flex()
            .min_w_0()
            .gap(px(2.))
            .when(stacked, |column| column.pr(px(20.)))
            .when(!stacked, |column| column.flex_1())
            .child(title);
        match (&toast.collapse, body) {
            (None, body) => {
                text = text
                    .children(description(palette))
                    .children(body.map(|body| panel(body, true)));
            }
            (Some(_), None) => text = text.children(description(palette)),
            (Some(collapse), Some(body)) => {
                let label = if entry.open {
                    collapse.collapse.clone()
                } else {
                    collapse.expand.clone()
                };
                let toggle = cx.listener(move |this, _, _, cx| this.toggle(id, cx));
                if collapse.on_description {
                    text = text.child(
                        h_flex()
                            .id(("toast-toggle", id.0))
                            .w_full()
                            .min_w_0()
                            .items_start()
                            .gap(px(6.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .hover(|row| row.bg(palette.muted.opacity(0.4 * 0.1)))
                            .tooltip(tip(label))
                            .on_click(toggle)
                            .child(div().min_w_0().flex_1().children(description(palette)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h(px(20.))
                                    .child(chevron(entry.open, palette)),
                            ),
                    );
                } else {
                    text = text.children(description(palette)).child(
                        h_flex()
                            .id(("toast-toggle", id.0))
                            .gap(px(4.))
                            .py(px(2.))
                            .items_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.muted)
                            .hover(|row| row.text_color(palette.foreground))
                            .on_click(toggle)
                            .child(chevron(entry.open, palette))
                            .child(label),
                    );
                }
                if entry.open {
                    text = text.child(panel(body, true));
                }
            }
        }

        let mut controls = h_flex()
            .items_center()
            .gap(px(6.))
            .when(stacked, |row| row.w_full().justify_end())
            .when(!stacked, |row| row.flex_shrink_0());
        if let Some(copy) = toast.copy_text() {
            let copy = copy.clone();
            let (label, glyph, color) = if entry.copied {
                ("Copied error", Icons::Check, palette.kind(Kind::Success))
            } else {
                ("Copy error", Icons::Copy, palette.muted.opacity(0.8))
            };
            controls = controls.child(
                div()
                    .id(("toast-copy", id.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(20.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_color(color)
                    .when(!entry.copied, |button| {
                        button.hover(|button| button.text_color(palette.muted))
                    })
                    .tooltip(tip(label))
                    .on_click(cx.listener(move |this, _, _, cx| this.copy(id, copy.clone(), cx)))
                    .child(Icon::new(glyph).size(px(12.))),
            );
        }
        for (index, action) in toast.actions.iter().enumerate() {
            let on_click = action.on_click.clone();
            let button = Button::new(("toast-action", id.0 * 64 + index as u64))
                .label(action.label.clone())
                .xsmall()
                .flex_shrink_0()
                .on_click(move |_, window, cx| on_click(window, cx));
            controls = controls.child(match action.variant {
                Variant::Primary => button.primary(),
                Variant::Outline => button.outline(),
            });
        }

        // `Toast.Content`: `pl-3.5`, then `py-3` and `pr-3.5` (`pr-6` beside controls) inline, or
        // `py-2.5 pr-3.5` with the controls on their own row when stacked.
        let content = if stacked {
            v_flex()
                .gap(px(8.))
                .pl(px(14.))
                .pr(px(14.))
                .py(px(10.))
                .child(text)
                .child(controls)
        } else {
            h_flex()
                .items_center()
                .justify_between()
                .gap(px(6.))
                .pl(px(14.))
                .py(px(12.))
                .pr(px(if trailing { 24. } else { 14. }))
                .child(text)
                .children(trailing.then_some(controls))
        };

        // `toastCornerOrbClass` at `-top-1.5 -right-1.5`.
        let close = div()
            .id(("toast-close", id.0))
            .absolute()
            .top(px(-6.))
            .right(px(-6.))
            .flex()
            .items_center()
            .justify_center()
            .size(px(24.))
            .rounded_full()
            .border_1()
            .border_color(palette.border)
            .bg(palette.popover.opacity(0.92))
            .text_color(palette.muted)
            .cursor_pointer()
            .shadow_sm()
            .hover(|close| close.bg(palette.popover).text_color(palette.foreground))
            .on_click(cx.listener(move |this, _, _, cx| this.dismiss(id, cx)))
            .child(Icon::new(Icons::X).size(px(12.)));

        surface(palette)
            .id(("toast", id.0))
            .relative()
            .w_full()
            .shadow(shadow())
            .text_sm()
            .line_height(px(20.))
            .text_color(palette.foreground)
            .occlude()
            .child(close)
            .child(content)
            .into_any_element()
    }
}

impl Render for ToastHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.stack.entries.len().min(LIMIT);
        if visible == 0 {
            return div().into_any_element();
        }
        let palette = Palette::new(cx);
        let window_width = f32::from(window.viewport_size().width);
        let width = toast_width(window_width);
        let expanded = self.hovered;
        let peeks = if expanded { 0 } else { visible - 1 };

        let mut stack = v_flex()
            .id("toasts")
            .absolute()
            .top(px(inset(window_width) + self.top_offset))
            .right(px(inset(window_width)))
            .w(px(width))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.hovered = *hovered;
                this.sync_pause(cx);
            }));
        if expanded {
            stack = stack.gap(px(GAP));
            for index in 0..visible {
                let entry = &self.stack.entries[index];
                stack = stack.child(self.card(entry, &palette, window, cx));
            }
        } else {
            // Collapsed, the cards behind the front one are its height, scaled about the top
            // center and pushed down by `--toast-peek`: only a strip of each shows under it.
            let front = self.card(&self.stack.entries[0], &palette, window, cx);
            let mut front_slot = div().relative().w_full().pb(px(PEEK * peeks as f32));
            for index in (1..visible).rev() {
                let inset = width * (1. - collapsed_scale(index)) / 2.;
                front_slot = front_slot.child(
                    surface(&palette)
                        .absolute()
                        .left(px(inset))
                        .right(px(inset))
                        .bottom(px(PEEK * (peeks - index) as f32 - 0.))
                        .h(px(PEEK + 16.))
                        .shadow(shadow()),
                );
            }
            stack = stack.child(front_slot.child(front));
        }
        stack.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(kind: Option<Kind>, timeout: Timeout) -> Toast {
        let toast = Toast::new("t").timeout(timeout);
        match kind {
            Some(kind) => toast.kind(kind),
            None => toast,
        }
    }

    #[test]
    fn timeouts_follow_base_ui() {
        let default = Some(Duration::from_secs(5));
        assert_eq!(Timeout::Default.resolve(None), default);
        assert_eq!(Timeout::Never.resolve(None), None);
        assert_eq!(Timeout::After(Duration::ZERO).resolve(None), None);
        assert_eq!(
            Timeout::After(Duration::from_secs(9)).resolve(Some(Kind::Info)),
            Some(Duration::from_secs(9))
        );
        assert_eq!(Timeout::Default.resolve(Some(Kind::Loading)), None);
    }

    #[test]
    fn newest_toast_is_in_front() {
        let now = Instant::now();
        let mut stack = Stack::default();
        let first = stack.add(toast(None, Timeout::Never), now);
        let second = stack.add(toast(None, Timeout::Never), now);
        assert_eq!(stack.entries[0].id, second);
        assert_eq!(stack.entries[1].id, first);
        assert!(stack.dismiss(second).is_some());
        assert!(stack.dismiss(second).is_none());
        assert!(!stack.update(second, toast(None, Timeout::Never), now));
    }

    #[test]
    fn timers_run_expire_and_pause_with_the_remaining_time() {
        let start = Instant::now();
        let mut stack = Stack::default();
        let id = stack.add(toast(None, Timeout::Default), start);
        let later = |seconds| start + Duration::from_secs(seconds);
        assert_eq!(stack.next_expiry(later(2)), Some(Duration::from_secs(3)));
        assert!(stack.expired(later(4)).is_empty());

        // Two seconds run, then a hover holds the toast for as long as it lasts.
        stack.set_paused(true, later(2));
        assert_eq!(stack.next_expiry(later(60)), None);
        assert!(stack.expired(later(60)).is_empty());
        stack.set_paused(false, later(60));
        assert_eq!(stack.next_expiry(later(61)), Some(Duration::from_secs(2)));
        assert_eq!(stack.expired(later(63)), vec![id]);
    }

    #[test]
    fn a_toast_added_while_paused_waits_for_the_resume() {
        let now = Instant::now();
        let mut stack = Stack::default();
        stack.set_paused(true, now);
        let id = stack.add(toast(None, Timeout::Default), now);
        let later = now + Duration::from_secs(30);
        assert!(stack.expired(later).is_empty());
        stack.set_paused(false, later);
        assert_eq!(stack.next_expiry(later), Some(Duration::from_secs(5)));
        assert_eq!(stack.expired(later + Duration::from_secs(5)), vec![id]);
    }

    #[test]
    fn update_restarts_the_timer_only_when_its_length_changes() {
        let start = Instant::now();
        let later = start + Duration::from_secs(3);
        let mut stack = Stack::default();
        let id = stack.add(toast(None, Timeout::Default), start);
        assert!(stack.update(id, toast(None, Timeout::Default), later));
        assert_eq!(stack.next_expiry(later), Some(Duration::from_secs(2)));
        stack.update(
            id,
            toast(None, Timeout::After(Duration::from_secs(10))),
            later,
        );
        assert_eq!(stack.next_expiry(later), Some(Duration::from_secs(10)));
        stack.update(id, toast(None, Timeout::Never), later);
        assert_eq!(stack.next_expiry(later), None);
    }

    #[test]
    fn the_stack_scales_and_insets_like_the_web() {
        assert_eq!(collapsed_scale(0), 1.);
        assert!((collapsed_scale(2) - 0.8).abs() < 1e-6);
        assert_eq!(collapsed_scale(12), 0.);
        assert_eq!((inset(639.), inset(640.)), (16., 32.));
        assert_eq!(toast_width(1100.), 360.);
        assert_eq!(toast_width(500.), 360.);
        assert_eq!(toast_width(300.), 268.);
    }

    #[test]
    fn only_long_error_descriptions_clamp_and_only_errors_copy() {
        let long = "x".repeat(ERROR_CLAMP_MIN_CHARS);
        assert_eq!(clamp_lines(Some(Kind::Error), &long), Some(4));
        assert_eq!(clamp_lines(Some(Kind::Error), &long[1..]), None);
        assert_eq!(clamp_lines(Some(Kind::Info), &long), None);
        let error = Toast::new("t").kind(Kind::Error).description("boom");
        assert!(error.has_trailing_controls());
        assert!(!error.clone().hide_copy().has_trailing_controls());
        assert!(
            !Toast::new("t")
                .kind(Kind::Info)
                .description("x")
                .has_trailing_controls()
        );
    }
}
