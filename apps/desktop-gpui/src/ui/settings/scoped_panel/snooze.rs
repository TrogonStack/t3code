//! `SnoozePresetsRow` (SnoozePresetsSettings.tsx): a select for when, then a stepper and a unit
//! select or an `at` time field, with the menu entry each row makes and an X to remove it.

use super::super::components::{Menu, SelectTrigger, select_menu};
use super::super::devices_panel::widgets::{Variant, button, icon_button};
use super::threads::RESET;
use super::*;
use crate::settings_threads as threads;
use gpui::Anchor;
use gpui_component::input::Input;
use gpui_component::popover::Popover;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    When,
    Unit,
}

/// One row's field state; a row keeps its fields while others come and go above it.
struct RowFields {
    amount: Entity<InputState>,
    time: Entity<InputState>,
    when: FocusHandle,
    unit: FocusHandle,
    /// The unit the user chose, kept while stepping so 59 to 60 minutes does not jump to hours.
    unit_choice: Option<usize>,
    /// The other kind's last value, so a misclick on "When" can be undone.
    minutes: u64,
    time_of_day: String,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
pub(super) struct Snooze {
    rows: Vec<RowFields>,
    menu: Option<(usize, Kind)>,
    highlight: usize,
}

impl Snooze {
    pub(super) fn close(&mut self) -> bool {
        self.menu.take().is_some()
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

impl ScopedPanel {
    fn snooze_presets(&self, ctx: &Ctx) -> Vec<Value> {
        self.value(Field::SnoozePresets, ctx)
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn ensure_snooze_rows(&mut self, count: usize, window: &mut Window, cx: &mut Context<Self>) {
        while self.snooze.rows.len() < count {
            let index = self.snooze.rows.len();
            let amount = cx.new(|cx| InputState::new(window, cx));
            let time = cx.new(|cx| InputState::new(window, cx));
            let commit_amount = cx.subscribe_in(
                &amount,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        this.commit_snooze_amount(index, window, cx);
                    }
                },
            );
            let commit_time = cx.subscribe_in(
                &time,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        this.commit_snooze_time(index, window, cx);
                    }
                },
            );
            self.snooze.rows.push(RowFields {
                amount,
                time,
                when: cx.focus_handle().tab_stop(true),
                unit: cx.focus_handle().tab_stop(true),
                unit_choice: None,
                minutes: 60,
                time_of_day: "09:00".into(),
                _subscriptions: vec![commit_amount, commit_time],
            });
        }
    }

    /// The row's amount and unit index: the chosen unit while it divides evenly.
    fn snooze_amount(&self, index: usize, minutes: u64) -> (u64, usize) {
        let chosen = self.snooze.rows[index].unit_choice;
        match chosen {
            Some(unit) if minutes.is_multiple_of(threads::UNIT_MINUTES[unit]) => {
                (minutes / threads::UNIT_MINUTES[unit], unit)
            }
            _ => threads::split_minutes(minutes),
        }
    }

    fn replace_snooze(&mut self, index: usize, next: Value, ctx: &Ctx, cx: &mut Context<Self>) {
        let mut presets = self.snooze_presets(ctx);
        if let Some(slot) = presets.get_mut(index) {
            *slot = next;
            self.write(
                vec![(Field::SnoozePresets, Some(Value::Array(presets)))],
                cx,
            );
        }
    }

    fn commit_snooze_amount(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        let presets = self.snooze_presets(&ctx);
        let Some(minutes) = presets
            .get(index)
            .and_then(|preset| preset.get("minutes"))
            .and_then(Value::as_u64)
        else {
            return;
        };
        let (amount, unit) = self.snooze_amount(index, minutes);
        let per = threads::UNIT_MINUTES[unit];
        let max = threads::MAX_PRESET_MINUTES / per;
        let typed = self.snooze.rows[index].amount.read(cx).value();
        let next = typed
            .trim()
            .parse::<f64>()
            .ok()
            .map(|value| (value.round().max(1.) as u64).min(max));
        let shown = next.unwrap_or(amount);
        self.snooze.rows[index].amount.update(cx, |state, cx| {
            state.set_value(shown.to_string(), window, cx)
        });
        if let Some(next) = next.filter(|next| *next != amount) {
            let mut preset = presets[index].clone();
            preset["minutes"] = json!(next * per);
            self.replace_snooze(index, preset, &ctx, cx);
        }
    }

    fn step_snooze(&mut self, index: usize, direction: i64, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        let presets = self.snooze_presets(&ctx);
        let Some(minutes) = presets
            .get(index)
            .and_then(|preset| preset.get("minutes"))
            .and_then(Value::as_u64)
        else {
            return;
        };
        let (amount, unit) = self.snooze_amount(index, minutes);
        let per = threads::UNIT_MINUTES[unit];
        let max = threads::MAX_PRESET_MINUTES / per;
        let next = (amount as i64 + direction).clamp(1, max as i64) as u64;
        if next != amount {
            let mut preset = presets[index].clone();
            preset["minutes"] = json!(next * per);
            self.replace_snooze(index, preset, &ctx, cx);
        }
    }

    fn commit_snooze_time(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        let presets = self.snooze_presets(&ctx);
        let Some(current) = presets
            .get(index)
            .and_then(|preset| preset.get("time"))
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return;
        };
        let typed = self.snooze.rows[index].time.read(cx).value();
        let next = threads::parse_time(&typed);
        // A field that does not read as a time goes back to what is saved.
        self.snooze.rows[index].time.update(cx, |state, cx| {
            let shown = threads::time_text(next.as_deref().unwrap_or(&current));
            state.set_value(shown, window, cx)
        });
        if let Some(next) = next.filter(|next| *next != current) {
            let mut preset = presets[index].clone();
            preset["time"] = json!(next);
            self.replace_snooze(index, preset, &ctx, cx);
        }
    }

    fn pick_snooze(
        &mut self,
        index: usize,
        kind: Kind,
        choice: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snooze.menu = None;
        let ctx = self.ctx(cx);
        let presets = self.snooze_presets(&ctx);
        let Some(preset) = presets.get(index).cloned() else {
            return;
        };
        match kind {
            Kind::When => {
                let after = preset.get("kind").and_then(Value::as_str) == Some("after");
                let row = &mut self.snooze.rows[index];
                let when = threads::WHEN_VALUES[choice];
                let (kind_changes, was_after) = (after != (when == "after"), after);
                if kind_changes {
                    if was_after {
                        row.minutes = preset["minutes"].as_u64().unwrap_or(row.minutes);
                    } else if let Some(time) = preset["time"].as_str() {
                        row.time_of_day = time.to_owned();
                    }
                }
                let next = threads::with_when(&preset, when, row.minutes, &row.time_of_day);
                if next != preset {
                    self.replace_snooze(index, next, &ctx, cx);
                }
            }
            Kind::Unit => {
                let minutes = preset["minutes"].as_u64().unwrap_or(60);
                let (amount, unit) = self.snooze_amount(index, minutes);
                let next =
                    (amount * threads::UNIT_MINUTES[choice]).min(threads::MAX_PRESET_MINUTES);
                self.snooze.rows[index].unit_choice = Some(choice);
                if next != minutes || unit != choice {
                    let mut next_preset = preset;
                    next_preset["minutes"] = json!(next);
                    self.replace_snooze(index, next_preset, &ctx, cx);
                }
            }
        }
        let handle = match kind {
            Kind::When => self.snooze.rows[index].when.clone(),
            Kind::Unit => self.snooze.rows[index].unit.clone(),
        };
        window.focus(&handle, cx);
        cx.notify();
    }

    fn open_snooze_menu(
        &mut self,
        index: usize,
        kind: Kind,
        selected: usize,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.snooze.menu = Some((index, kind));
        self.snooze.highlight = selected;
        cx.notify();
    }

    /// A `size="sm"` select of the editor: `when` at `w-32`, the unit at `w-28`.
    fn snooze_select(
        &self,
        index: usize,
        kind: Kind,
        labels: Vec<SharedString>,
        selected: usize,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let (id, handle) = match kind {
            Kind::When => ("snooze-when", self.snooze.rows[index].when.clone()),
            Kind::Unit => ("snooze-unit", self.snooze.rows[index].unit.clone()),
        };
        let on_key: super::super::components::KeyHandler = {
            let view = view.clone();
            Rc::new(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| {
                        this.open_snooze_menu(index, kind, selected, cx)
                    });
                }
            })
        };
        let trigger =
            SelectTrigger::new((id, index), labels[selected].clone(), width, handle, on_key);
        let open = self.snooze.menu == Some((index, kind));
        let highlighted = if open {
            self.snooze.highlight
        } else {
            usize::MAX
        };
        let on_pick: super::super::components::PickHandler = {
            let view = view.clone();
            Rc::new(move |choice, window, cx| {
                view.update(cx, |this, cx| {
                    this.pick_snooze(index, kind, choice, window, cx)
                })
            })
        };
        let on_hover: super::super::components::HoverHandler = {
            let view = view.clone();
            Rc::new(move |choice, cx| {
                view.update(cx, |this, cx| {
                    if this.snooze.menu == Some((index, kind)) && this.snooze.highlight != choice {
                        this.snooze.highlight = choice;
                        cx.notify();
                    }
                })
            })
        };
        Popover::new((
            "snooze-popover",
            index * 2 + usize::from(kind == Kind::Unit),
        ))
        .anchor(Anchor::TopLeft)
        .offset(px(4.))
        .open(open)
        .p_0()
        .appearance(false)
        .on_open_change(move |opened, _, cx| {
            view.update(cx, |this, cx| {
                if *opened {
                    this.open_snooze_menu(index, kind, selected, cx);
                } else if this.snooze.menu == Some((index, kind)) {
                    this.snooze.menu = None;
                    cx.notify();
                }
            })
        })
        .trigger(trigger)
        .content(move |_, _, cx| {
            select_menu(
                &Menu {
                    icons: vec![None; labels.len()],
                    headings: Vec::new(),
                    descriptions: Vec::new(),
                    id,
                    labels: labels.clone(),
                    selected,
                    highlighted,
                    width,
                    on_pick: on_pick.clone(),
                    on_hover: on_hover.clone(),
                },
                cx,
            )
        })
        .into_any_element()
    }

    /// `NumberField size="sm" className="w-32"`.
    fn snooze_stepper(
        &self,
        index: usize,
        amount: u64,
        max: u64,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let step =
            |id: &'static str, icon: Icons, direction: i64, off: bool, cx: &mut Context<Self>| {
                div()
                    .id((id, index))
                    .size(px(26.))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .text_color(theme.muted_foreground)
                    .when(off, |step| step.opacity(0.5))
                    .when(!off, |step| {
                        step.cursor_pointer()
                            .hover(move |style| style.bg(theme.accent))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.step_snooze(index, direction, cx)
                            }))
                    })
                    .child(Icon::new(icon).size(px(16.)))
            };
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        h_flex()
            .w(px(128.))
            .h(px(28.))
            .items_center()
            .overflow_hidden()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(background)
            .text_size(px(14.))
            .child(step(
                "snooze-down",
                Icons::Minus,
                -1,
                disabled || amount <= 1,
                cx,
            ))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.snooze.rows[index].amount)
                        .appearance(false)
                        .bordered(false)
                        .disabled(disabled)
                        .h(px(26.))
                        .px_0(),
                ),
            )
            .child(step(
                "snooze-up",
                Icons::Plus,
                1,
                disabled || amount >= max,
                cx,
            ))
            .into_any_element()
    }

    /// `Input type="time" size="sm" className="w-32"` with the clock the native field draws.
    fn snooze_time(&self, index: usize, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        h_flex()
            .w(px(128.))
            .h(px(28.))
            .items_center()
            .pr_2()
            .overflow_hidden()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(background)
            .text_size(px(14.))
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.snooze.rows[index].time)
                        .appearance(false)
                        .bordered(false)
                        .disabled(disabled)
                        .h(px(26.)),
                ),
            )
            .child(
                Icon::new(Icons::Clock)
                    .size(px(14.))
                    .text_color(theme.muted_foreground),
            )
            .into_any_element()
    }

    pub(super) fn snooze(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = Field::SnoozePresets;
        let presets = self.snooze_presets(ctx);
        self.ensure_snooze_rows(presets.len(), window, cx);
        let defaults = threads::default_presets()
            .as_array()
            .cloned()
            .unwrap_or_default();
        let reset = (!threads::same_presets(&presets, &defaults) && self.gate(field, cx).is_none())
            .then(|| {
                self.reset_with(
                    "settings-snooze-reset",
                    RESET,
                    vec![(field, None)],
                    window,
                    cx,
                )
            });
        let inert = self.gate(field, cx).is_some();
        let now = now_secs();
        let previews = threads::previews(&presets, now);
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let mut list = v_flex().gap(px(6.)).pt_3().pb_2();
        for (index, preset) in presets.iter().enumerate() {
            let after = preset.get("kind").and_then(Value::as_str) == Some("after");
            let when = if after {
                0
            } else {
                threads::WHEN_VALUES
                    .iter()
                    .position(|value| Some(*value) == preset["day"].as_str())
                    .unwrap_or(1)
            };
            let when_labels: Vec<SharedString> =
                threads::WHEN_LABELS.iter().map(|l| (*l).into()).collect();
            let mut fields = h_flex()
                .min_w_0()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(self.snooze_select(index, Kind::When, when_labels, when, 128., cx));
            if after {
                let minutes = preset["minutes"].as_u64().unwrap_or(60);
                let (amount, unit) = self.snooze_amount(index, minutes);
                let max = threads::MAX_PRESET_MINUTES / threads::UNIT_MINUTES[unit];
                let row = &self.snooze.rows[index];
                if !row.amount.read(cx).focus_handle(cx).is_focused(window)
                    && row.amount.read(cx).value() != amount.to_string()
                {
                    row.amount.update(cx, |state, cx| {
                        state.set_value(amount.to_string(), window, cx)
                    });
                }
                let names = if amount == 1 {
                    ["minute", "hour", "day"]
                } else {
                    ["minutes", "hours", "days"]
                };
                fields = fields
                    .child(self.snooze_stepper(index, amount, max, inert, cx))
                    .child(self.snooze_select(
                        index,
                        Kind::Unit,
                        names.iter().map(|n| (*n).into()).collect(),
                        unit,
                        112.,
                        cx,
                    ));
            } else {
                let shown = threads::time_text(preset["time"].as_str().unwrap_or("09:00"));
                let row = &self.snooze.rows[index];
                if !row.time.read(cx).focus_handle(cx).is_focused(window)
                    && row.time.read(cx).value() != shown
                {
                    row.time
                        .update(cx, |state, cx| state.set_value(shown, window, cx));
                }
                fields = fields.child(
                    h_flex()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("at"),
                        )
                        .child(self.snooze_time(index, inert, cx)),
                );
            }
            let kept: Vec<Value> = presets
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, preset)| preset.clone())
                .collect();
            let remove_view = view.clone();
            let preview = match &previews[index] {
                threads::Preview::Menu { label, when } => h_flex()
                    .min_w_0()
                    .overflow_hidden()
                    .child(div().text_color(theme.foreground).child(label.clone()))
                    .child(div().pl_2().child(when.clone()))
                    .into_any_element(),
                threads::Preview::Note(note) => div().truncate().child(*note).into_any_element(),
            };
            list = list.child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(fields)
                    .child(
                        h_flex()
                            .ml_auto()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(preview)
                            .child(
                                icon_button(
                                    ("settings-snooze-remove", index),
                                    Variant::GhostMuted,
                                    !inert,
                                    cx,
                                )
                                .tooltip(tooltip(format!("Remove snooze option {}", index + 1)))
                                .when(!inert, |remove| {
                                    remove.on_click(move |_, _, cx| {
                                        let kept = kept.clone();
                                        remove_view.update(cx, |this, cx| {
                                            this.write(vec![(field, Some(Value::Array(kept)))], cx)
                                        })
                                    })
                                })
                                .child(Icon::new(Icons::X).size(px(14.))),
                            ),
                    ),
            );
        }
        let menu = threads::menu_order(&threads::menu_now(&presets, now));
        list = list.child(div().text_xs().text_color(theme.muted_foreground).child(
            if menu.is_empty() {
                "Menus offer only Custom\u{2026}".to_owned()
            } else {
                format!("Menu now: {}", menu.join(" \u{b7} "))
            },
        ));
        let full = presets.len() >= threads::MAX_PRESETS;
        let mut grown = presets.clone();
        grown.push(threads::next_candidate(&presets));
        let add_view = view.clone();
        list = list.child(
            h_flex()
                .items_center()
                .gap_2()
                .child(
                    button(
                        "settings-snooze-add",
                        Variant::GhostMuted,
                        !full && !inert,
                        cx,
                    )
                    .when(!full && !inert, |add| {
                        add.on_click(move |_, _, cx| {
                            let grown = grown.clone();
                            add_view.update(cx, |this, cx| {
                                this.write(vec![(field, Some(Value::Array(grown)))], cx)
                            })
                        })
                    })
                    .child(Icon::new(Icons::Plus).size(px(14.)))
                    .child("Add option"),
                )
                .when(full, |footer| {
                    footer.child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("Up to {} options", threads::MAX_PRESETS)),
                    )
                }),
        );
        let row = self.setting(
            ids!("snooze-options"),
            "Snooze options",
            "Choices in every snooze menu, listed soonest first. Weekdays mean the next one after today.",
            field,
            reset,
            div().into_any_element(),
            Some(list.into_any_element()),
            ctx,
            window,
            cx,
        );
        section("Snooze", group(vec![row], cx)).into_any_element()
    }
}
