//! The editor's building blocks: `Field` with its hint, the shared `Select`, the segmented
//! schedule toggle, the weekday toggle group and the switch row.

use super::super::*;
use crate::ui::settings::components::{
    HoverHandler, KeyHandler, Menu as MenuSpec, SelectTrigger, Toggle, select_menu, tab_stop,
};
use gpui::ElementId;

/// One select item: a label with an optional second line and a leading icon.
pub(super) struct Opt {
    pub label: String,
    pub secondary: Option<String>,
    pub lead: Option<Icons>,
}

impl Opt {
    pub fn plain(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            secondary: None,
            lead: None,
        }
    }
}

/// What a select shows and offers.
pub(super) struct Sel {
    pub id: SharedString,
    pub menu: super::Menu,
    pub open: bool,
    pub disabled: bool,
    pub lead: Option<Icons>,
    pub current: String,
    /// `current` is the placeholder, so it reads muted.
    pub placeholder: bool,
    pub options: Vec<Opt>,
    pub selected: Option<usize>,
    /// The item under the pointer while the popup is open.
    pub highlight: Option<usize>,
    pub width: f32,
}

pub(super) type PickItem = Rc<dyn Fn(usize, &mut Window, &mut App)>;
pub(super) type OnOpen = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// The shared `SelectTrigger` and `select_menu` of the other tabs, with the popup capped at
/// `max-h-60` so a long project list scrolls.
pub(super) fn select(
    spec: Sel,
    on_open: OnOpen,
    on_hover: HoverHandler,
    pick: PickItem,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let Sel {
        id,
        open,
        disabled,
        lead,
        current,
        placeholder,
        options,
        selected,
        highlight,
        width,
        ..
    } = spec;
    let muted = Tokens::of(cx).muted_foreground;
    let handle = tab_stop(&ElementId::from(id.clone()), window, cx);
    let on_key: KeyHandler = {
        let on_open = on_open.clone();
        Rc::new(move |event, window, cx| {
            if matches!(
                event.keystroke.key.as_str(),
                "enter" | "space" | "down" | "up"
            ) {
                cx.stop_propagation();
                on_open(true, window, cx);
            }
        })
    };
    let mut trigger = SelectTrigger::new(id.clone(), current.clone(), width, handle, on_key);
    if let Some(icon) = lead {
        trigger = trigger.leading(
            Icon::new(icon)
                .size(px(14.))
                .text_color(muted)
                .into_any_element(),
        );
    }
    if placeholder {
        trigger = trigger.content(div().text_color(muted).child(current).into_any_element());
    }
    if disabled {
        return div().opacity(0.64).child(trigger).into_any_element();
    }
    let labels: Vec<SharedString> = options.iter().map(|o| o.label.clone().into()).collect();
    let icons: Vec<Option<Icons>> = options.iter().map(|o| o.lead).collect();
    let descriptions: Vec<Option<SharedString>> = options
        .iter()
        .map(|o| o.secondary.clone().map(Into::into))
        .collect();
    let highlighted = if open {
        highlight.or(selected).unwrap_or(usize::MAX)
    } else {
        usize::MAX
    };
    let menu_id = id.clone();
    Popover::new(SharedString::from(format!("{id}-menu")))
        .anchor(Anchor::TopLeft)
        .offset(px(4.))
        .open(open)
        .p_0()
        .appearance(false)
        .on_open_change(move |next, window, cx| on_open(*next, window, cx))
        .trigger(trigger)
        .content(move |_, _, cx| {
            select_menu(
                &MenuSpec {
                    id: "scheduled-task-select-item",
                    labels: labels.clone(),
                    icons: icons.clone(),
                    headings: Vec::new(),
                    descriptions: descriptions.clone(),
                    selected: selected.unwrap_or(usize::MAX),
                    highlighted,
                    width,
                    on_pick: pick.clone(),
                    on_hover: on_hover.clone(),
                },
                cx,
            )
            .max_h(px(240.))
            .id(SharedString::from(format!("{menu_id}-list")))
            .overflow_y_scroll()
        })
        .into_any_element()
}

/// `Field` with `FieldLabel`: the title, and an optional right-aligned `text-[10px]` hint.
pub(super) fn field_of(
    title: &'static str,
    hint: Option<&'static str>,
    body: impl IntoElement,
    cx: &App,
) -> gpui::Div {
    let theme = Tokens::of(cx);
    v_flex()
        .w_full()
        .gap(px(6.))
        .child(
            h_flex()
                .items_baseline()
                .justify_between()
                .gap_2()
                .child(div().text_size(px(14.)).child(title))
                .children(hint.map(|hint| {
                    div()
                        .text_size(px(10.))
                        .text_color(theme.muted_foreground.opacity(0.8))
                        .child(hint)
                })),
        )
        .child(body)
}

/// A label and description on the left, the switch on the right.
pub(super) fn switch_row_of(
    title: &'static str,
    description: &'static str,
    id: &'static str,
    checked: bool,
    toggle: Toggle,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = Tokens::of(cx);
    h_flex()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            v_flex()
                .min_w_0()
                .gap_1()
                .child(div().text_size(px(14.)).child(title))
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(theme.muted_foreground)
                        .child(description),
                ),
        )
        .child(switch(id, checked, toggle, window, cx))
        .into_any_element()
}

/// The group around the schedule's mode toggle.
pub(super) fn segments(cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    h_flex()
        .flex_wrap()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(8.))
        .bg(theme.input.opacity(0.4))
}

pub(super) fn segment(
    id: impl Into<ElementId>,
    label: &'static str,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = Tokens::of(cx);
    let pressed = if theme.dark {
        theme.input.opacity(0.72)
    } else {
        theme.background
    };
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
        .text_color(if selected {
            theme.foreground
        } else {
            theme.muted_foreground
        })
        .when(selected, |item| item.bg(pressed))
        .child(label)
        .on_click(on_click)
        .into_any_element()
}

pub(super) type Day = (
    (&'static str, usize),
    &'static str,
    bool,
    Rc<dyn Fn(&mut Window, &mut App)>,
);

/// The weekday `ToggleGroup` (outline, sm): joined buttons with a divider between them.
pub(super) fn weekday_group(days: Vec<Day>, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    let count = days.len();
    h_flex()
        .h(px(28.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.input)
        .overflow_hidden()
        .children(
            days.into_iter()
                .enumerate()
                .map(|(index, (id, label, on, toggle))| {
                    div()
                        .id(id)
                        .h_full()
                        .min_w(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.))
                        .cursor_pointer()
                        .when(index + 1 < count, |item| {
                            item.border_r_1().border_color(theme.input)
                        })
                        .text_color(if on {
                            theme.foreground
                        } else {
                            theme.muted_foreground
                        })
                        .when(on, |item| item.bg(theme.foreground.opacity(0.08)))
                        .child(label)
                        .on_click(move |_, window, cx| toggle(window, cx))
                }),
        )
}
