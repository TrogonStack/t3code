//! The "Custom model prices" dialog of `UsagePriceOverrides.tsx`.

use super::pickers::{Menu, check_row, menu_popup, separator, status_text};
use super::*;
use crate::usage_prices::{
    self as price, Change, Draft, Field, Snapshot, Target as PriceTarget, Write,
};
use gpui::{Anchor, FocusHandle, ScrollHandle};
use gpui_component::FocusTrapElement as _;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::popover::Popover;
use std::collections::{BTreeSet, HashMap, HashSet};

/// `Dialog`'s `max-w-4xl`.
const DIALOG_MAX: f32 = 896.;
/// The table's `min-w-180`.
const TABLE_MIN: f32 = 720.;
/// The last column's `w-10`.
const ACTION_WIDTH: f32 = 40.;
const SAVE_FAILED: &str = "Could not save. Try again.";

/// The settings the dialog edits. `local` follows `seen` whenever the catalog reports new
/// settings, and takes a saved change at once so the rows do not wait for a settings read.
#[derive(Default)]
pub(super) struct PriceStore {
    seen: Snapshot,
    local: Snapshot,
}

impl PriceStore {
    pub(super) fn refresh(&mut self, seen: Snapshot) {
        if seen != self.seen {
            self.local = seen.clone();
            self.seen = seen;
        }
    }

    fn apply(&mut self, changes: &[Change]) {
        let mut target = PriceTarget {
            label: String::new(),
            prices: self.local.0.take(),
            aliases: self.local.1.take(),
            unavailable: None,
        };
        price::apply_changes(&mut target, changes);
        self.local = (target.prices, target.aliases);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Model,
    Alias,
    Rate(Field),
}

type Key = (String, Kind);

struct Slot {
    input: Entity<InputState>,
    placeholder: &'static str,
}

#[derive(Clone, PartialEq)]
enum Outcome {
    Saving,
    Saved,
    Failed(String),
}

/// `SaveAttempt`: the drafts a save sent and how the environment answered.
struct Attempt {
    drafts: Vec<Draft>,
    outcome: Outcome,
}

/// The dialog's state (`UsagePriceOverrides`' `useState` hooks).
pub(super) struct Prices {
    /// `selectedIds === null`: every environment. Otherwise none, with one environment.
    all: bool,
    drafts: Vec<Draft>,
    attempt: Option<Attempt>,
    pending: bool,
    next_row: u32,
    focus_row: Option<String>,
    inputs: HashMap<Key, Slot>,
}

impl Prices {
    fn new(all: bool) -> Self {
        Self {
            all,
            drafts: Vec::new(),
            attempt: None,
            pending: false,
            next_row: 0,
            focus_row: None,
            inputs: HashMap::new(),
        }
    }

    fn failed(&self) -> bool {
        self.attempt
            .as_ref()
            .is_some_and(|attempt| matches!(attempt.outcome, Outcome::Failed(_)))
    }

    fn locked(&self) -> bool {
        self.pending || self.failed()
    }

    fn staged(&self) -> Vec<Draft> {
        self.drafts
            .iter()
            .filter(|draft| !price::is_empty_draft(draft))
            .cloned()
            .collect()
    }

    /// `updateDraft`.
    fn update_draft(&mut self, draft: Draft) {
        match self.drafts.iter_mut().find(|entry| entry.id == draft.id) {
            Some(entry) => *entry = draft,
            None => self.drafts.push(draft),
        }
        self.attempt = None;
    }

    /// `updateOrDropDraft`: a saved row left with no edits is no draft at all.
    fn update_or_drop(&mut self, draft: Draft) {
        if !draft.is_new && draft.values.is_empty() && draft.alias.is_none() {
            self.drafts.retain(|entry| entry.id != draft.id);
        } else {
            self.update_draft(draft);
        }
    }

    /// `editCell`.
    fn edit_cell(&mut self, targets: &[PriceTarget], row: &Draft, field: Field, value: String) {
        let mut row = row.clone();
        row.values.set(field, value.clone());
        let original = price::price_cell(targets, &row.model, field);
        if matches_saved(&row, &original, &value) {
            row.values.remove(field);
        }
        self.update_or_drop(row);
    }

    /// `editAlias`.
    fn edit_alias(&mut self, targets: &[PriceTarget], row: &Draft, value: String) {
        let mut next = row.clone();
        next.alias = None;
        let original = price::alias_cell(targets, &row.model);
        if !matches_saved(row, &original, &value) {
            next.alias = Some(value);
        }
        self.update_or_drop(next);
    }
}

/// `matchesSaved`: a mixed or unavailable cell never matches.
fn matches_saved(row: &Draft, original: &price::Cell, value: &str) -> bool {
    !row.is_new && original.is_comparable() && value == original.value
}

/// The models that have a custom price or mapping on any selected environment, sorted.
fn custom_models(targets: &[PriceTarget]) -> Vec<String> {
    let mut models = BTreeSet::new();
    for target in targets {
        models.extend(
            target
                .prices
                .iter()
                .flat_map(|prices| prices.keys().cloned()),
        );
        models.extend(
            target
                .aliases
                .iter()
                .flat_map(|aliases| aliases.keys().cloned()),
        );
    }
    models.into_iter().collect()
}

/// `rows`: the saved models, with their drafts applied, then the new rows. Once a save was
/// attempted a saved model that a new row now names is shown by that row alone.
fn rows(targets: &[PriceTarget], drafts: &[Draft], attempted: bool) -> Vec<Draft> {
    let new_models: HashSet<&str> = drafts
        .iter()
        .filter(|draft| draft.is_new)
        .map(|draft| draft.model.trim())
        .collect();
    let mut rows: Vec<Draft> = custom_models(targets)
        .into_iter()
        .filter(|model| !attempted || !new_models.contains(model.as_str()))
        .map(|model| {
            let id = format!("model:{model}");
            drafts
                .iter()
                .find(|draft| draft.id == id)
                .cloned()
                .unwrap_or_else(|| Draft::new(id, &model, false))
        })
        .collect();
    rows.extend(drafts.iter().filter(|draft| draft.is_new).cloned());
    rows
}

/// `errors`: the table's errors plus the ones for new rows.
fn errors(targets: &[PriceTarget], drafts: &[Draft], attempted: bool) -> HashMap<String, String> {
    let staged: Vec<Draft> = drafts
        .iter()
        .filter(|draft| !price::is_empty_draft(draft))
        .cloned()
        .collect();
    let mut errors = price::table_errors(targets, &staged);
    let custom = custom_models(targets);
    for draft in staged.iter().filter(|draft| draft.is_new) {
        let model = draft.model.trim();
        if model.is_empty() {
            errors.insert(draft.id.clone(), "Enter a model ID.".to_owned());
        } else if !attempted
            && (custom.iter().any(|saved| saved == model)
                || drafts
                    .iter()
                    .any(|other| other.id != draft.id && other.model.trim() == model))
        {
            errors.insert(
                draft.id.clone(),
                "This model already has a row. Edit its prices there.".to_owned(),
            );
        }
    }
    errors
}

/// Button variants the dialog uses (`ui/button.tsx`).
#[derive(Clone, Copy, PartialEq)]
enum Variant {
    Primary,
    Ghost,
}

/// The tab stops of the dialog, in the web's DOM order (`dialog.tsx:117-157`).
pub(super) struct Focus {
    scope: FocusHandle,
    panel: FocusHandle,
    apply: FocusHandle,
    add: FocusHandle,
    discard: FocusHandle,
    save: FocusHandle,
    close: FocusHandle,
    panel_scroll: ScrollHandle,
    rows: HashMap<String, FocusHandle>,
}

impl Focus {
    pub(super) fn new(cx: &mut App) -> Self {
        let stop = |cx: &mut App| cx.focus_handle().tab_stop(true);
        Self {
            scope: cx.focus_handle(),
            panel: stop(cx),
            apply: stop(cx),
            add: stop(cx),
            discard: stop(cx),
            save: stop(cx),
            close: stop(cx),
            panel_scroll: ScrollHandle::new(),
            rows: HashMap::new(),
        }
    }
}

/// `focus-visible`: focused, and the last input was the keyboard.
fn focus_visible(handle: &FocusHandle, window: &Window) -> bool {
    handle.is_focused(window) && window.last_input_was_keyboard()
}

/// `focus-visible:ring-2 ring-inset ring-ring ring-offset-1 ring-offset-background`
/// (`button.tsx:9`, `scroll-area.tsx:54`): an inset ring of 2px, under an offset line of 1px
/// in the background color. `inset` is the border the ring sits inside of.
fn focus_ring(radius: f32, inset: f32, cx: &App) -> gpui::Div {
    let theme = cx.theme();
    div()
        .absolute()
        .top(px(inset))
        .left(px(inset))
        .right(px(inset))
        .bottom(px(inset))
        .rounded(px(radius))
        .border_1()
        .border_color(theme.background)
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .rounded(px((radius - 1.).max(0.)))
                .border_2()
                .border_color(theme.ring),
        )
}

/// A `size="default"` button: `h-8`, `text-sm`, `font-medium`.
fn button(
    id: &'static str,
    label: &'static str,
    variant: Variant,
    enabled: bool,
    handle: &FocusHandle,
    window: &Window,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (primary, on_primary, accent, foreground) = (
        theme.primary,
        theme.primary_foreground,
        theme.accent,
        theme.foreground,
    );
    let handle = handle.clone().tab_stop(enabled);
    let ring = enabled && focus_visible(&handle, window);
    div()
        .id(id)
        .relative()
        .track_focus(&handle)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .border_1()
        .text_size(px(14.))
        .line_height(px(20.))
        .font_weight(FontWeight::MEDIUM)
        .map(|button| match variant {
            Variant::Primary => button
                .border_color(primary)
                .bg(primary)
                .text_color(on_primary)
                .when(enabled, |button| {
                    button.hover(move |style| style.bg(primary.opacity(0.9)))
                }),
            Variant::Ghost => button
                .border_color(gpui::transparent_black())
                .text_color(foreground)
                .when(enabled, |button| {
                    button.hover(move |style| style.bg(accent))
                }),
        })
        .when(enabled, |button| button.cursor_pointer())
        .when(!enabled, |button| button.opacity(0.64))
        .child(label)
        .when(ring, |button| button.child(focus_ring(7., 1., cx)))
}

/// A ghost icon button (`button.tsx:21-34`): `icon-xs` is `size-6`, `icon` is `size-8`.
#[allow(clippy::too_many_arguments)]
fn icon_button(
    id: impl Into<ElementId>,
    icon: impl IntoElement,
    size: f32,
    label: Option<&'static str>,
    enabled: bool,
    handle: &FocusHandle,
    window: &Window,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (accent, foreground) = (theme.accent, theme.foreground);
    let handle = handle.clone().tab_stop(enabled);
    let ring = enabled && focus_visible(&handle, window);
    div()
        .id(id)
        .relative()
        .track_focus(&handle)
        .flex_shrink_0()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_color(foreground)
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(accent))
                .when_some(label, |button, label| button.tooltip(tooltip(label)))
        })
        .when(!enabled, |button| button.opacity(0.64))
        .child(icon)
        .when(ring, |button| button.child(focus_ring(7., 1., cx)))
}

/// `Button variant="outline" size="sm"` as the "Apply to" menu trigger.
#[derive(IntoElement)]
struct ApplyTrigger {
    label: String,
    enabled: bool,
    open: bool,
    ring: bool,
}

impl Selectable for ApplyTrigger {
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

impl RenderOnce for ApplyTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let dark = theme.is_dark();
        let (border, foreground, hover) = (
            theme.input,
            theme.foreground,
            if dark {
                theme.input.opacity(0.64)
            } else {
                theme.accent.opacity(0.5)
            },
        );
        let background = if dark {
            theme.input.opacity(0.32)
        } else {
            theme.popover
        };
        let enabled = self.enabled;
        let ring = self.ring;
        h_flex()
            .id("usage-prices-apply")
            .relative()
            .min_w_0()
            .h(px(28.))
            .px(px(10.))
            .gap(px(6.))
            .items_center()
            .rounded(px(8.))
            .border_1()
            .border_color(border)
            .bg(if self.open { hover } else { background })
            .text_color(foreground)
            .text_size(px(14.))
            .line_height(px(20.))
            .font_weight(FontWeight::MEDIUM)
            .when(enabled, |button| {
                button.cursor_pointer().hover(move |style| style.bg(hover))
            })
            .when(!enabled, |button| button.opacity(0.64))
            .child(div().min_w_0().truncate().child(self.label))
            .child(Icon::new(Icons::ChevronDown).size(px(14.)).flex_shrink_0())
            .when(ring, |button| button.child(focus_ring(7., 1., cx)))
    }
}

fn cell(width: f32) -> gpui::Div {
    div().w(px(width)).flex_shrink_0().min_w_0().p_2()
}

impl UsageDialog {
    /// `priceTargetsAtom` for the one environment this app is paired with.
    pub(super) fn price_target(&self) -> PriceTarget {
        let label = if self.limits.environment.is_empty() {
            "Environment".to_owned()
        } else {
            self.limits.environment.clone()
        };
        let (prices, aliases) = self.price_store.local.clone();
        let unavailable = if self.access != Access::Ready {
            Some("Offline".to_owned())
        } else if prices.is_none() {
            Some("Prices not loaded".to_owned())
        } else {
            None
        };
        PriceTarget {
            label,
            prices,
            aliases,
            unavailable,
        }
    }

    fn price_selected(&self) -> Vec<PriceTarget> {
        match &self.prices {
            Some(prices) if prices.all && self.access != Access::Unavailable => {
                vec![self.price_target()]
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn open_prices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.prices = Some(Prices::new(self.env_selected));
        window.focus(&self.price_focus.scope, cx);
        cx.notify();
    }

    /// Closing waits for a save in flight (`onOpenChange` guard).
    pub(super) fn close_prices(&mut self, cx: &mut Context<Self>) {
        if self.prices.as_ref().is_some_and(|prices| !prices.pending) {
            self.prices = None;
            self.menu = None;
            cx.notify();
        }
    }

    fn set_apply(&mut self, all: bool, cx: &mut Context<Self>) {
        if let Some(prices) = &mut self.prices {
            prices.all = all;
            prices.attempt = None;
            cx.notify();
        }
    }

    fn discard_prices(&mut self, cx: &mut Context<Self>) {
        if let Some(prices) = &mut self.prices {
            prices.drafts.clear();
            prices.attempt = None;
            cx.notify();
        }
    }

    fn add_price_row(&mut self, cx: &mut Context<Self>) {
        if let Some(prices) = &mut self.prices {
            let id = format!("new:{}", prices.next_row);
            prices.next_row += 1;
            prices.focus_row = Some(id.clone());
            prices.update_draft(Draft::new(id, "", true));
            cx.notify();
        }
    }

    fn price_row_action(&mut self, id: &str, cx: &mut Context<Self>) {
        let targets = self.price_selected();
        let Some(prices) = &mut self.prices else {
            return;
        };
        let Some(row) = rows(&targets, &prices.drafts, prices.attempt.is_some())
            .into_iter()
            .find(|row| row.id == id)
        else {
            return;
        };
        if row.is_new {
            prices.drafts.retain(|entry| entry.id != row.id);
        } else if row.removed {
            let mut row = row;
            row.removed = false;
            prices.update_or_drop(row);
        } else {
            let mut row = row;
            row.removed = true;
            prices.update_draft(row);
        }
        cx.notify();
    }

    /// An edit in one of the table's inputs.
    fn price_input_event(
        &mut self,
        key: &Key,
        input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Focus | InputEvent::Blur => {
                cx.notify();
                return;
            }
            InputEvent::Change => {}
            _ => return,
        }
        let text = input.read(cx).value().to_string();
        let targets = self.price_selected();
        let Some(prices) = &mut self.prices else {
            return;
        };
        if prices.locked() {
            return;
        }
        let Some(row) = rows(&targets, &prices.drafts, prices.attempt.is_some())
            .into_iter()
            .find(|row| row.id == key.0)
        else {
            return;
        };
        match key.1 {
            Kind::Model => {
                let mut row = row;
                row.model = text;
                prices.update_draft(row);
            }
            Kind::Alias => prices.edit_alias(&targets, &row, text),
            Kind::Rate(field) => {
                let current = row.values.get(field).map(str::to_owned).unwrap_or_else(|| {
                    if row.is_new {
                        String::new()
                    } else {
                        price::price_cell(&targets, &row.model, field).value
                    }
                });
                if price::is_price_edit(&current, &text) {
                    prices.edit_cell(&targets, &row, field, text);
                } else {
                    input.update(cx, |state, cx| state.set_value(current, window, cx));
                }
            }
        }
        cx.notify();
    }

    /// `save`: one environment, so one write.
    fn save_prices(&mut self, retry: bool, cx: &mut Context<Self>) {
        let selected = self.price_selected();
        let mut target = self.price_target();
        let Some(prices) = &mut self.prices else {
            return;
        };
        if prices.pending {
            return;
        }
        let edits = match (&prices.attempt, retry) {
            (Some(attempt), true) => attempt.drafts.clone(),
            _ => prices.staged(),
        };
        let destination = if retry {
            prices.failed()
        } else {
            !selected.is_empty()
        };
        if !destination || edits.is_empty() {
            return;
        }
        let (changes, errors) = price::table_changes(&target, &edits);
        if target.unavailable.is_none() {
            target.unavailable = edits
                .iter()
                .find_map(|draft| errors.get(&draft.id).cloned());
        }
        let plan = price::plan_write(&target, &changes);
        prices.pending = true;
        prices.attempt = Some(Attempt {
            drafts: edits,
            outcome: Outcome::Saving,
        });
        match plan {
            Write::Failed(reason) => self.prices_finished(Outcome::Failed(reason), &[]),
            Write::Saved => self.prices_finished(Outcome::Saved, &[]),
            Write::Patch(payload) => {
                self.call(Call::UpdateSettings, payload, Pending::Prices { changes })
            }
        }
        cx.notify();
    }

    fn prices_finished(&mut self, outcome: Outcome, changes: &[Change]) {
        let saved = outcome == Outcome::Saved;
        if let Some(prices) = &mut self.prices {
            prices.pending = false;
            if let Some(attempt) = &mut prices.attempt {
                attempt.outcome = outcome;
            }
            if saved {
                prices.drafts.clear();
            }
        }
        if saved && !changes.is_empty() {
            self.price_store.apply(changes);
            // The web's usage summary rebuilds when the settings change.
            if self.can_read() {
                self.start_query();
            }
        }
    }

    pub(super) fn prices_settled(&mut self, changes: Vec<Change>, event: CallEvent) {
        let outcome = match event {
            CallEvent::Done(_) => Outcome::Saved,
            _ => Outcome::Failed(SAVE_FAILED.to_owned()),
        };
        self.prices_finished(outcome, &changes);
    }

    /// `input_for`: the state behind one input, set to the text and placeholder it shows.
    fn price_input(
        &mut self,
        prices: &mut Prices,
        key: Key,
        text: &str,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let slot = prices.inputs.entry(key.clone()).or_insert_with(|| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            let key = key.clone();
            cx.subscribe_in(
                &input,
                window,
                move |this, input, event: &InputEvent, window, cx| {
                    this.price_input_event(&key, input, event, window, cx);
                },
            )
            .detach();
            Slot { input, placeholder }
        });
        if slot.placeholder != placeholder {
            slot.placeholder = placeholder;
            slot.input.update(cx, |state, cx| {
                state.set_placeholder(placeholder, window, cx)
            });
        }
        if slot.input.read(cx).value().as_ref() != text {
            let text = text.to_owned();
            slot.input
                .update(cx, |state, cx| state.set_value(text, window, cx));
        }
        slot.input.clone()
    }

    /// `Input size="compact"`.
    #[allow(clippy::too_many_arguments)]
    fn field(
        &self,
        input: &Entity<InputState>,
        invalid: bool,
        disabled: bool,
        mono: bool,
        window: &Window,
        cx: &App,
    ) -> gpui::Div {
        let theme = cx.theme();
        let focused = input.read(cx).focus_handle(cx).contains_focused(window, cx);
        let border = if invalid {
            theme.danger.opacity(0.36)
        } else if focused {
            theme.ring
        } else {
            theme.input
        };
        let background = if theme.is_dark() {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        div()
            .w_full()
            .h(px(30.))
            .px(px(9.))
            .rounded(px(6.))
            .border_1()
            .border_color(border)
            .bg(background)
            .text_size(px(12.))
            .when(mono, |field| {
                field.font_family(theme.mono_font_family.clone())
            })
            .when(disabled, |field| field.opacity(0.64))
            .child(
                Input::new(input)
                    .appearance(false)
                    .bordered(false)
                    .disabled(disabled)
                    .h(px(28.))
                    .px_0(),
            )
    }

    pub(super) fn prices_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut prices = self.prices.take()?;
        let element = self.prices_overlay(&mut prices, window, cx);
        self.prices = Some(prices);
        Some(element)
    }

    fn prices_overlay(
        &mut self,
        prices: &mut Prices,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
        let dialog_width = (vw - 32.).min(DIALOG_MAX);
        let table_width = (dialog_width - 48. - 2.).max(TABLE_MIN);

        let present = self.access != Access::Unavailable;
        let environment = self.price_target();
        let selected = self.price_selected();
        let attempted = prices.attempt.is_some();
        let all_rows = rows(&selected, &prices.drafts, attempted);
        let row_errors = errors(&selected, &prices.drafts, attempted);
        let has_changes = !prices.staged().is_empty();
        let failed = prices.failed();
        let locked = prices.locked();
        let pending = prices.pending;
        let destination = if selected.len() == 1 {
            selected[0].label.clone()
        } else {
            format!("{} environments", selected.len())
        };
        let selection_label = if prices.all {
            "All environments".to_owned()
        } else {
            destination.clone()
        };

        let apply_focus = self.price_focus.apply.clone();
        let add_focus = self.price_focus.add.clone();
        let discard_focus = self.price_focus.discard.clone();
        let save_focus = self.price_focus.save.clone();
        let close_focus = self.price_focus.close.clone();
        let panel_focus = self.price_focus.panel.clone();
        let scope_focus = self.price_focus.scope.clone();
        let panel_scroll = self.price_focus.panel_scroll.clone();
        let overflows = f32::from(panel_scroll.max_offset().y) > 0.;
        let panel_focus = panel_focus.tab_stop(overflows);
        let panel_ring = overflows && focus_visible(&panel_focus, window);
        let row_focus: Vec<FocusHandle> = all_rows
            .iter()
            .map(|row| {
                self.price_focus
                    .rows
                    .entry(row.id.clone())
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone()
            })
            .collect();

        let theme = cx.theme();
        let (border, background, muted, foreground, danger) = (
            theme.border,
            theme.background,
            theme.muted_foreground,
            theme.foreground,
            theme.danger,
        );
        let hover_row = theme.muted.opacity(0.5);
        let backdrop = background.opacity(if theme.is_dark() { 0.64 } else { 0.6 });

        // Column widths: 22%, 18%, four equal, then the action column.
        let model_width = table_width * 0.22;
        let alias_width = table_width * 0.18;
        let rate_width = (table_width - model_width - alias_width - ACTION_WIDTH) / 4.;

        let apply = {
            let view = cx.entity();
            let all = prices.all;
            let unavailable = environment.unavailable.clone();
            let label = environment.label.clone();
            let enabled = !(pending || has_changes);
            Popover::new("usage-prices-apply-menu")
                .track_focus(&apply_focus)
                .anchor(Anchor::TopLeft)
                .open(self.menu == Some(Menu::Apply) && enabled)
                .p_0()
                .on_open_change({
                    let view = view.clone();
                    move |open, _, cx| {
                        let menu = (*open && enabled).then_some(Menu::Apply);
                        view.update(cx, |this, cx| this.set_menu(menu, cx));
                    }
                })
                .trigger(ApplyTrigger {
                    label: selection_label,
                    enabled,
                    open: false,
                    ring: enabled && focus_visible(&apply_focus, window),
                })
                .content(move |_, _, cx| {
                    let toggle = {
                        let view = view.clone();
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            view.update(cx, |this, cx| {
                                let next = !this.prices.as_ref().is_some_and(|p| p.all);
                                this.set_apply(next, cx);
                            });
                        }
                    };
                    let environment = present.then(|| {
                        check_row(
                            "usage-prices-environment",
                            all,
                            h_flex()
                                .min_w_0()
                                .items_center()
                                .gap_3()
                                .child(div().min_w_0().flex_1().truncate().child(label.clone()))
                                .children(
                                    unavailable.clone().map(|text| status_text(text, false, cx)),
                                ),
                            cx,
                        )
                        .on_click(toggle.clone())
                    });
                    menu_popup()
                        .child(
                            check_row("usage-prices-all", all, "All environments", cx)
                                .on_click(toggle.clone()),
                        )
                        .child(separator(cx))
                        .children(environment)
                })
        };

        let top = h_flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                h_flex()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(14.))
                            .line_height(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Apply to"),
                    )
                    .child(apply),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("USD / million tokens"),
            );

        let table = if selected.is_empty() {
            div()
                .text_size(px(14.))
                .line_height(px(20.))
                .text_color(muted)
                .child(if present {
                    "Select an environment to see and change its model prices."
                } else {
                    "Connect an environment to set model prices."
                })
                .into_any_element()
        } else {
            let head = |label: &'static str, width: f32| {
                div().w(px(width)).flex_shrink_0().px_2().child(label)
            };
            let header = h_flex()
                .h(px(40.))
                .items_center()
                .border_b_1()
                .border_color(border)
                .font_weight(FontWeight::MEDIUM)
                .child(head("Model ID", model_width))
                .child(head("Map to", alias_width))
                .children(
                    Field::ALL
                        .into_iter()
                        .map(|field| head(field.label(), rate_width)),
                )
                .child(
                    cell(ACTION_WIDTH).child(
                        icon_button(
                            "usage-prices-add",
                            Icon::new(Icons::Plus).size(px(14.)),
                            24.,
                            None,
                            !locked,
                            &add_focus,
                            window,
                            cx,
                        )
                        .when(!locked, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.add_price_row(cx)))
                        }),
                    ),
                );

            let mut body = v_flex();
            if all_rows.is_empty() {
                body = body.child(div().w_full().p_2().text_center().child(
                    div().py_6().text_color(muted).child(
                        if selected.iter().any(|target| target.prices.is_none()) {
                            "Some environment prices are unavailable."
                        } else {
                            "No custom prices or mappings. Add a row to set one."
                        },
                    ),
                ));
            }
            let live: HashSet<String> = all_rows.iter().map(|row| row.id.clone()).collect();
            prices.inputs.retain(|(id, _), _| live.contains(id));
            for (index, row) in all_rows.iter().enumerate() {
                let alias_cell = price::alias_cell(&selected, &row.model);
                let alias_text = row
                    .alias
                    .clone()
                    .unwrap_or_else(|| alias_cell.value.clone());
                let mapped = !alias_text.trim().is_empty();
                let error = row_errors.get(&row.id).filter(|_| row.has_text());
                let invalid = row_errors.contains_key(&row.id) && !row.model.trim().is_empty();

                let model_cell = {
                    let content = if row.is_new {
                        let input = self.price_input(
                            prices,
                            (row.id.clone(), Kind::Model),
                            &row.model,
                            "Model ID",
                            window,
                            cx,
                        );
                        if prices.focus_row.as_deref() == Some(row.id.as_str()) {
                            prices.focus_row = None;
                            let input = input.clone();
                            window.defer(cx, move |window, cx| {
                                input.update(cx, |state, cx| state.focus(window, cx));
                            });
                        }
                        self.field(&input, invalid, locked, false, window, cx)
                            .into_any_element()
                    } else {
                        div()
                            .when(row.removed, |text| text.text_color(muted).line_through())
                            .child(row.model.clone())
                            .into_any_element()
                    };
                    cell(model_width)
                        .child(content)
                        .children(error.map(|error| {
                            div()
                                .mt_1()
                                .text_xs()
                                .text_color(danger)
                                .child(error.clone())
                        }))
                };

                let mut line = h_flex()
                    .id(("usage-price-row", index))
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .when(index + 1 == all_rows.len(), |line| line.border_b_0())
                    .hover(move |style| style.bg(hover_row))
                    .child(model_cell);
                if row.removed {
                    line = line.child(
                        cell(alias_width + 4. * rate_width).child(
                            div()
                                .text_color(muted)
                                .child("Automatic pricing after saving"),
                        ),
                    );
                } else {
                    let placeholder = if row.is_new {
                        "Optional"
                    } else {
                        alias_cell.placeholder
                    };
                    let input = self.price_input(
                        prices,
                        (row.id.clone(), Kind::Alias),
                        &alias_text,
                        placeholder,
                        window,
                        cx,
                    );
                    line = line.child(
                        cell(alias_width)
                            .child(self.field(&input, false, locked, false, window, cx)),
                    );
                    if mapped {
                        line = line.child(
                            cell(4. * rate_width).child(
                                div()
                                    .text_color(muted)
                                    .child(format!("Counted as {}", alias_text.trim())),
                            ),
                        );
                    } else {
                        for field in Field::ALL {
                            let saved = price::price_cell(&selected, &row.model, field);
                            let text =
                                row.values.get(field).map(str::to_owned).unwrap_or_else(|| {
                                    if row.is_new {
                                        String::new()
                                    } else {
                                        saved.value.clone()
                                    }
                                });
                            let placeholder = if row.is_new {
                                if field.optional() {
                                    "Input rate"
                                } else {
                                    "0.00"
                                }
                            } else {
                                saved.placeholder
                            };
                            let input = self.price_input(
                                prices,
                                (row.id.clone(), Kind::Rate(field)),
                                &text,
                                placeholder,
                                window,
                                cx,
                            );
                            line = line.child(
                                cell(rate_width)
                                    .child(self.field(&input, false, locked, true, window, cx)),
                            );
                        }
                    }
                }
                let (icon, label) = if row.removed {
                    (Icon::new(Icons::RotateCcw), "Undo reset")
                } else if row.is_new {
                    (Icon::new(IconName::Close), "Remove row")
                } else {
                    (Icon::new(Icons::RotateCcw), "Reset to automatic")
                };
                let id = row.id.clone();
                line = line.child(
                    cell(ACTION_WIDTH).child(
                        icon_button(
                            ("usage-price-action", index),
                            icon.size(px(14.)),
                            24.,
                            Some(label),
                            !locked,
                            &row_focus[index],
                            window,
                            cx,
                        )
                        .when(!locked, |button| {
                            button.on_click(
                                cx.listener(move |this, _, _, cx| this.price_row_action(&id, cx)),
                            )
                        }),
                    ),
                );
                body = body.child(line);
            }

            let help = format!(
                "Blank cache rates use the input price. Enter 0 for free tokens. A mapped model\u{2019}s usage counts as the model it maps to.{}",
                if selected.len() > 1 {
                    " Mixed cells keep each environment\u{2019}s rate until you edit them."
                } else {
                    ""
                }
            );
            v_flex()
                .gap_4()
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(border)
                        .child(
                            div()
                                .id("usage-prices-table")
                                .overflow_x_scroll()
                                .text_xs()
                                .child(v_flex().w(px(table_width)).child(header).child(body)),
                        ),
                )
                .child(div().text_xs().text_color(muted).child(help))
                .into_any_element()
        };

        let status = prices.attempt.as_ref().map(|attempt| {
            let (text, bad) = match &attempt.outcome {
                Outcome::Failed(error) => (format!("Not saved \u{b7} {error}"), true),
                Outcome::Saved => ("Saved".to_owned(), false),
                Outcome::Saving => ("Saving\u{2026}".to_owned(), false),
            };
            h_flex()
                .text_xs()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(muted)
                        .child(environment.label.clone()),
                )
                .child(
                    div()
                        .when(bad, |text| text.text_right())
                        .text_color(if bad { danger } else { muted })
                        .child(text),
                )
        });

        let save_disabled = pending
            || (!has_changes && !failed)
            || (!failed && (!row_errors.is_empty() || selected.is_empty()));
        let save_label = if pending {
            "Saving\u{2026}"
        } else if failed {
            "Retry failed saves"
        } else {
            "Save changes"
        };
        let discard_label = if failed {
            "Discard pending changes"
        } else {
            "Discard changes"
        };

        let header = v_flex()
            .flex_shrink_0()
            .gap_2()
            .p_6()
            .pb_3()
            .child(
                div()
                    .text_size(px(20.))
                    .line_height(px(27.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Custom model prices"),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .text_color(muted)
                    .child(
                        "Prices and mappings apply to all past and future usage on the environments you select.",
                    ),
            );
        let panel = v_flex()
            .id("usage-prices-panel")
            .track_focus(&panel_focus)
            .track_scroll(&panel_scroll)
            .size_full()
            .overflow_y_scroll()
            .gap_4()
            .px_6()
            .py_1()
            .child(top)
            .child(table)
            .children(status);
        let panel = div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(panel)
            .when(panel_ring, |region| region.child(focus_ring(16., 0., cx)));
        let footer = h_flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .gap_2()
            .px_6()
            .py_4()
            .child(div().text_xs().text_color(muted).child(if has_changes {
                format!("Changes apply to {destination}")
            } else {
                String::new()
            }))
            .child(
                h_flex()
                    .gap_2()
                    .when(has_changes, |row| {
                        row.child(
                            button(
                                "usage-prices-discard",
                                discard_label,
                                Variant::Ghost,
                                !pending,
                                &discard_focus,
                                window,
                                cx,
                            )
                            .when(!pending, |button| {
                                button
                                    .on_click(cx.listener(|this, _, _, cx| this.discard_prices(cx)))
                            }),
                        )
                    })
                    .child(
                        button(
                            "usage-prices-save",
                            save_label,
                            Variant::Primary,
                            !save_disabled,
                            &save_focus,
                            window,
                            cx,
                        )
                        .when(!save_disabled, |button| {
                            button.on_click(
                                cx.listener(move |this, _, _, cx| this.save_prices(failed, cx)),
                            )
                        }),
                    ),
            );
        let close = icon_button(
            "usage-prices-close",
            Icon::new(IconName::Close).size(px(16.)),
            32.,
            None,
            !pending,
            &close_focus,
            window,
            cx,
        )
        .absolute()
        .right(px(8.))
        .top(px(8.))
        .on_click(cx.listener(|this, _, _, cx| this.close_prices(cx)));

        div()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("usage-prices-backdrop")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(backdrop)
                    .on_click(cx.listener(|this, _, _, cx| this.close_prices(cx))),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_4()
                    .child(
                        v_flex()
                            .relative()
                            .occlude()
                            .w(px(dialog_width))
                            .max_h(px(vh - 32.))
                            .rounded(px(16.))
                            .border_1()
                            .border_color(border)
                            .bg(background)
                            .text_color(foreground)
                            .shadow_lg()
                            .child(header)
                            .child(panel)
                            .child(footer)
                            .child(close)
                            .focus_trap("usage-prices-trap", &scope_focus),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage_prices::Price;
    use std::collections::BTreeMap;

    fn target(models: &[(&str, &str)]) -> PriceTarget {
        let prices = models
            .iter()
            .map(|(model, _)| {
                (
                    (*model).to_owned(),
                    Price {
                        input: 1.0,
                        output: 2.0,
                        cache_read: None,
                        cache_write: None,
                    },
                )
            })
            .collect();
        let aliases = models
            .iter()
            .filter(|(_, alias)| !alias.is_empty())
            .map(|(model, alias)| ((*model).to_owned(), (*alias).to_owned()))
            .collect();
        PriceTarget {
            label: "Local".to_owned(),
            prices: Some(prices),
            aliases: Some(aliases),
            unavailable: None,
        }
    }

    fn new_row(id: &str, model: &str) -> Draft {
        Draft::new(id.to_owned(), model, true)
    }

    #[test]
    fn rows_list_saved_models_sorted_then_new_rows() {
        let targets = [target(&[("b", ""), ("a", "")])];
        let drafts = [new_row("new:0", "c")];
        let ids: Vec<_> = rows(&targets, &drafts, false)
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, ["model:a", "model:b", "new:0"]);
    }

    #[test]
    fn an_attempt_shows_a_saved_model_by_its_new_row_alone() {
        let targets = [target(&[("a", "")])];
        let drafts = [new_row("new:0", "a")];
        assert_eq!(rows(&targets, &drafts, false).len(), 2);
        let after = rows(&targets, &drafts, true);
        assert_eq!(after.len(), 1);
        assert!(after[0].is_new);
    }

    #[test]
    fn new_rows_need_a_model_and_must_not_repeat_one() {
        let targets = [target(&[("a", "")])];
        let mut empty = new_row("new:0", "");
        empty.values.set(Field::Input, "1".to_owned());
        let mut repeat = new_row("new:1", "a");
        repeat.values.set(Field::Input, "1".to_owned());
        let drafts = [empty, repeat];
        let found = errors(&targets, &drafts, false);
        assert_eq!(found["new:0"], "Enter a model ID.");
        assert_eq!(
            found["new:1"],
            "This model already has a row. Edit its prices there."
        );
        assert!(!errors(&targets, &drafts, true).contains_key("new:1"));
    }

    #[test]
    fn editing_back_to_the_saved_value_drops_the_draft() {
        let targets = [target(&[("a", "")])];
        let mut state = Prices::new(true);
        let row = Draft::new("model:a".to_owned(), "a", false);
        state.edit_cell(&targets, &row, Field::Input, "5".to_owned());
        assert_eq!(state.staged().len(), 1);
        let row = state.drafts[0].clone();
        state.edit_cell(&targets, &row, Field::Input, "1".to_owned());
        assert!(state.drafts.is_empty());
    }

    #[test]
    fn editing_clears_the_attempt_but_dropping_a_row_keeps_it() {
        let mut state = Prices::new(true);
        state.attempt = Some(Attempt {
            drafts: Vec::new(),
            outcome: Outcome::Saved,
        });
        let row = Draft::new("model:a".to_owned(), "a", false);
        state.update_or_drop(row.clone());
        assert!(state.attempt.is_some());
        state.update_draft(row);
        assert!(state.attempt.is_none());
    }

    #[test]
    fn the_store_keeps_a_saved_change_until_the_settings_differ() {
        let mut store = PriceStore::default();
        store.refresh((Some(BTreeMap::new()), Some(BTreeMap::new())));
        store.apply(&[Change::Alias("a".to_owned(), Some("b".to_owned()))]);
        assert_eq!(store.local.1.as_ref().unwrap()["a"], "b");
        store.refresh((Some(BTreeMap::new()), Some(BTreeMap::new())));
        assert_eq!(store.local.1.as_ref().unwrap()["a"], "b");
        let mut next = BTreeMap::new();
        next.insert("z".to_owned(), "y".to_owned());
        store.refresh((Some(BTreeMap::new()), Some(next)));
        assert!(!store.local.1.as_ref().unwrap().contains_key("a"));
    }
}
