//! The Typography section of Settings > Appearance: `TypographySection`
//! (TypographySettings.tsx:317-349) with its font rows (`FontFamilySettingsRow`, 351-560), the
//! `FontFamilyPicker` (FontFamilyPicker.tsx) and the three previews (SettingsFontPreviews.tsx).
//!
//! The values are device settings (`settings_typography`); nothing here applies them to the rest
//! of the app yet, so the previews are where a choice shows.

use super::components::{
    HoverHandler, KeyHandler, Menu, PickHandler, SelectTrigger, Toggle, disclosure, group,
    reset_button, row, row_with_preview, section, select_menu, switch,
};
use super::scope_pickers::{Picker, PickerStyle, picker};
use super::tokens::Tokens;
use super::*;
use crate::settings_labs::{Choice, ChoiceIcon};
use crate::settings_typography::{Slot, TypographySettings, default_family_label};
use gpui::{
    FocusHandle, FontWeight, HighlightStyle, Hsla, KeyDownEvent, StyledText, UnderlineStyle, rgb,
};
use std::rc::Rc;

/// The whole px range of the four selects, as labels (`{px} px`).
const PX: [&str; 13] = [
    "8 px", "9 px", "10 px", "11 px", "12 px", "13 px", "14 px", "15 px", "16 px", "17 px",
    "18 px", "19 px", "20 px",
];
const PX_FROM: u32 = 8;

fn px_label(px: u32) -> &'static str {
    PX[(px.clamp(PX_FROM, PX_FROM + PX.len() as u32 - 1) - PX_FROM) as usize]
}

fn px_labels(slot: Slot) -> Vec<&'static str> {
    slot.range().options().map(px_label).collect()
}

fn slot_index(slot: Slot) -> usize {
    Slot::ALL.iter().position(|s| *s == slot).unwrap_or(0)
}

/// `w-22` on the size select, `sm:w-44` on the family picker.
const SIZE_WIDTH: f32 = 88.;
const FAMILY_WIDTH: f32 = 176.;

/// The transcript's height in rows (`TERMINAL_PREVIEW_ROWS`).
const TERMINAL_ROWS: usize = 12;

struct Ids {
    picker: &'static str,
    size: &'static str,
    reset: &'static str,
}

fn ids(slot: Slot) -> Ids {
    match slot {
        Slot::Interface => Ids {
            picker: "settings-font-family-interface",
            size: "settings-font-size-interface",
            reset: "settings-reset-interface-font",
        },
        Slot::Prompt => Ids {
            picker: "settings-font-family-prompt",
            size: "settings-font-size-prompt",
            reset: "settings-reset-prompt-font",
        },
        Slot::Code => Ids {
            picker: "settings-font-family-code",
            size: "settings-font-size-code",
            reset: "settings-reset-code-font",
        },
        Slot::Terminal => Ids {
            picker: "settings-font-family-terminal",
            size: "settings-font-size-terminal",
            reset: "settings-reset-terminal-font",
        },
    }
}

fn description(slot: Slot, separate: bool) -> &'static str {
    match slot {
        Slot::Interface => "Everything outside code blocks and the terminal.",
        Slot::Prompt => "Only the box you write prompts in. Mono works well here.",
        Slot::Code if separate => "Code blocks, diffs, and file previews.",
        Slot::Code => "Code blocks, diffs, file previews, and the terminal.",
        Slot::Terminal => "Terminal output, independent from code blocks and diffs.",
    }
}

fn title(slot: Slot, separate: bool) -> &'static str {
    match slot {
        Slot::Interface => "Interface font",
        Slot::Prompt => "Prompt font",
        Slot::Code if separate => "Code font",
        Slot::Code => "Monospace font",
        Slot::Terminal => "Terminal font",
    }
}

/// `isMonospaceFamily`: every glyph of a fixed-width face advances the same.
fn is_monospace(family: &str, cx: &App) -> bool {
    let text = cx.text_system();
    let id = text.resolve_font(&gpui::font(family.to_owned()));
    let size = px(16.);
    match (text.advance(id, size, 'i'), text.advance(id, size, 'W')) {
        (Ok(narrow), Ok(wide)) => (f32::from(narrow.width) - f32::from(wide.width)).abs() < 0.01,
        _ => false,
    }
}

pub struct AppearanceTypography {
    settings: TypographySettings,
    families: Vec<Choice>,
    font_open: Option<Slot>,
    size_menu: Option<(Slot, usize)>,
    search: Entity<InputState>,
    size_triggers: [FocusHandle; 4],
    advanced_open: bool,
    notice: Option<(Slot, String)>,
    wide: bool,
    _subscription: Subscription,
}

impl AppearanceTypography {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search fonts\u{2026}"));
        let subscription = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let families = cx
            .text_system()
            .all_font_names()
            .into_iter()
            .filter(|name| !name.starts_with('.'))
            .map(|name| Choice {
                value: name.clone(),
                label: name,
                secondary: String::new(),
                offline: false,
                icon: ChoiceIcon::None,
            })
            .collect();
        Self {
            settings: TypographySettings::load(),
            families,
            font_open: None,
            size_menu: None,
            search,
            size_triggers: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            advanced_open: false,
            notice: None,
            wide: true,
            _subscription: subscription,
        }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    /// Re-reads the file and closes any menu, as the dialog does when it opens.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.settings = TypographySettings::load();
        self.font_open = None;
        self.size_menu = None;
        self.notice = None;
        cx.notify();
    }

    /// A search result for a row of the Advanced disclosure opens it. A prompt or terminal font
    /// result also turns separate fonts on, since those rows only exist then
    /// (`isAdvancedTypographyTarget`, `revealSettingsSearchTarget`).
    pub fn open_advanced(&mut self, id: &str, cx: &mut Context<Self>) {
        match id {
            "separate-fonts" | "font-smoothing" => self.advanced_open = true,
            "prompt-font" | "terminal-font" if !self.settings.separate => {
                self.update(|s| s.separate = true, cx);
            }
            _ => return,
        }
        cx.notify();
    }

    /// "Restore defaults" resets the font settings; the separate-fonts flag is a view preference
    /// the web keeps in `localStorage` outside the settings, so it stays.
    pub fn restore_defaults(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        self.font_open = None;
        self.size_menu = None;
        let separate = self.settings.separate;
        self.update(
            |s| {
                *s = TypographySettings::default();
                s.separate = separate;
            },
            cx,
        );
    }

    /// Closes an open picker or menu; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some((slot, _)) = self.size_menu.take() {
            window.focus(&self.size_triggers[slot_index(slot)], cx);
            cx.notify();
            return true;
        }
        let was = self.font_open.take().is_some();
        if was {
            cx.notify();
        }
        was
    }

    fn update(&mut self, change: impl FnOnce(&mut TypographySettings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.settings.save();
        cx.notify();
    }

    fn set_font_open(
        &mut self,
        slot: Slot,
        opened: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if opened {
            self.font_open = Some(slot);
            self.size_menu = None;
            self.notice = None;
            self.search
                .update(cx, |input, cx| input.set_value("", window, cx));
            let handle = self.search.focus_handle(cx);
            window.focus(&handle, cx);
        } else if self.font_open == Some(slot) {
            self.font_open = None;
        }
        cx.notify();
    }

    /// `handlePick`: a non-monospace family is refused for code and terminal.
    fn pick_family(&mut self, slot: Slot, family: &str, cx: &mut Context<Self>) {
        self.font_open = None;
        if !family.is_empty() && slot.requires_monospace() && !is_monospace(family, cx) {
            self.notice = Some((
                slot,
                format!(
                    "\"{family}\" isn't monospace. Code and terminal need a fixed-width font, so the current font was kept."
                ),
            ));
            cx.notify();
            return;
        }
        self.notice = None;
        self.update(|s| s.set_family(slot, family), cx);
    }

    fn open_size(&mut self, slot: Slot, cx: &mut Context<Self>) {
        let current = self.settings.size(slot);
        let index = slot
            .range()
            .options()
            .position(|px| px == current)
            .unwrap_or(0);
        self.size_menu = Some((slot, index));
        self.font_open = None;
        cx.notify();
    }

    fn pick_size(&mut self, slot: Slot, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let px = slot.range().min + index as u32;
        self.update(|s| s.set_size(slot, px), cx);
        self.close_menu(window, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some((slot, highlighted)) = self.size_menu else {
            return;
        };
        let count = slot.range().options().count();
        match event.keystroke.key.as_str() {
            "down" => self.size_menu = Some((slot, (highlighted + 1) % count)),
            "up" => self.size_menu = Some((slot, (highlighted + count - 1) % count)),
            "home" => self.size_menu = Some((slot, 0)),
            "end" => self.size_menu = Some((slot, count - 1)),
            "enter" | "space" => self.pick_size(slot, highlighted, window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// The family a slot's picker offers as its default.
    fn default_label(&self, slot: Slot) -> String {
        match slot {
            Slot::Interface => default_family_label(false).to_owned(),
            Slot::Prompt => {
                let sans = self.settings.family(Slot::Interface);
                if sans.is_empty() {
                    default_family_label(false).to_owned()
                } else {
                    sans.to_owned()
                }
            }
            Slot::Code | Slot::Terminal => default_family_label(true).to_owned(),
        }
    }

    fn family_picker(&self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let open = self.font_open == Some(slot);
        let family = self.settings.family(slot).to_owned();
        let default = self.default_label(slot);
        let label = if family.is_empty() {
            default.clone()
        } else {
            family.clone()
        };
        let spec = Picker {
            id: ids(slot).picker,
            label,
            icon: None,
            all: Some(Choice {
                value: String::new(),
                label: default,
                secondary: String::new(),
                offline: false,
                icon: ChoiceIcon::None,
            }),
            // The list is only needed while the popup is open.
            choices: if open {
                self.families.clone()
            } else {
                Vec::new()
            },
            selected: family,
            open,
            search: self.search.clone(),
            query: if open {
                self.search.read(cx).value().to_string()
            } else {
                String::new()
            },
            on_open: {
                let view = view.clone();
                Rc::new(move |opened, window, cx| {
                    view.update(cx, |this, cx| this.set_font_open(slot, opened, window, cx))
                })
            },
            on_pick: Rc::new(move |value, _, cx| {
                view.update(cx, |this, cx| this.pick_family(slot, &value, cx))
            }),
            style: PickerStyle {
                trigger_width: Some(FAMILY_WIDTH),
                fonts: true,
            },
        };
        picker(spec, window, cx)
    }

    /// The size `Select` (`w-22`, `{px} px` items without an indicator).
    fn size_select(&self, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let id = ids(slot).size;
        let index = slot_index(slot);
        let current = self.settings.size(slot);
        let labels = px_labels(slot);
        let selected = (current - slot.range().min) as usize;
        let open = matches!(self.size_menu, Some((open, _)) if open == slot);
        let highlighted = match self.size_menu {
            Some((open, at)) if open == slot => at,
            _ => usize::MAX,
        };
        let on_key: KeyHandler = {
            let view = view.clone();
            Rc::new(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.open_size(slot, cx));
                }
            })
        };
        let trigger = SelectTrigger::new(
            id,
            px_label(current),
            SIZE_WIDTH,
            self.size_triggers[index].clone(),
            on_key,
        );
        let on_pick: PickHandler = {
            let view = view.clone();
            Rc::new(move |at, window, cx| {
                view.update(cx, |this, cx| this.pick_size(slot, at, window, cx))
            })
        };
        let on_hover: HoverHandler = {
            let view = view.clone();
            Rc::new(move |at, cx| {
                view.update(cx, |this, cx| {
                    if this.size_menu.is_some_and(|(_, current)| current != at) {
                        this.size_menu = Some((slot, at));
                        cx.notify();
                    }
                })
            })
        };
        Popover::new(("settings-font-size-select", index))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change(move |opened, window, cx| {
                view.update(cx, |this, cx| {
                    if *opened {
                        this.open_size(slot, cx);
                    } else if this.size_menu.is_some_and(|(current, _)| current == slot) {
                        this.close_menu(window, cx);
                    }
                })
            })
            .trigger(trigger)
            .content(move |_, _, cx| {
                select_menu(
                    &Menu {
                        id,
                        labels: labels.clone(),
                        selected,
                        highlighted,
                        width: SIZE_WIDTH,
                        on_pick: on_pick.clone(),
                        on_hover: on_hover.clone(),
                    },
                    cx,
                )
            })
            .into_any_element()
    }

    fn font_row(
        &self,
        slot: Slot,
        preview: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let view = cx.entity();
        let reset = (!self.settings.is_default(slot)).then(|| {
            reset_button(
                ids(slot).reset,
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.notice = None;
                        this.update(|s| s.reset(slot), cx)
                    })
                },
                window,
                cx,
            )
        });
        let control = h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.family_picker(slot, window, cx))
            .child(self.size_select(slot, cx))
            .into_any_element();
        let separate = self.settings.separate;
        let (title, description) = (title(slot, separate), description(slot, separate));
        let notice = self
            .notice
            .as_ref()
            .filter(|(at, _)| *at == slot)
            .map(|(_, text)| {
                let theme = Tokens::of(cx);
                div()
                    .pt_1()
                    .text_xs()
                    .text_color(theme.destructive)
                    .child(text.clone())
                    .into_any_element()
            });
        if preview.is_none() && notice.is_none() {
            return row(title, description, reset, control, self.wide, cx);
        }
        let body = v_flex()
            .children(notice)
            .children(preview)
            .into_any_element();
        row_with_preview(title, description, reset, control, body, self.wide, cx)
    }

    fn flag(
        &self,
        id: &'static str,
        checked: bool,
        toggle: fn(&mut TypographySettings),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        switch(
            id,
            checked,
            Rc::new(move |_, cx| view.update(cx, |this, cx| this.update(toggle, cx))),
            window,
            cx,
        )
    }

    fn flag_reset(
        &self,
        id: &'static str,
        changed: bool,
        restore: fn(&mut TypographySettings, &TypographySettings),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = cx.entity();
        changed.then(|| {
            reset_button(
                id,
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.update(|s| restore(s, &TypographySettings::default()), cx)
                    })
                },
                window,
                cx,
            )
        })
    }

    /// The family a preview is set in: the stored one, else the theme's.
    fn family_of(&self, slot: Slot, cx: &App) -> SharedString {
        let stored = self.settings.family(slot).trim();
        if !stored.is_empty() {
            return SharedString::from(stored.to_owned());
        }
        match slot {
            Slot::Interface | Slot::Prompt => cx.theme().font_family.clone(),
            Slot::Code | Slot::Terminal => cx.theme().mono_font_family.clone(),
        }
    }

    fn prompt_preview(&self, cx: &App) -> AnyElement {
        let (slot, size) = self.settings.prompt_font();
        let family = if slot == Slot::Prompt && self.settings.family(Slot::Prompt).trim().is_empty()
        {
            self.family_of(Slot::Interface, cx)
        } else {
            self.family_of(slot, cx)
        };
        prompt_preview(family, size, cx)
    }

    fn code_preview(&self, cx: &App) -> AnyElement {
        let size = self.settings.size(Slot::Code) as f32;
        diff_preview(self.family_of(Slot::Code, cx), size, cx)
    }

    fn terminal_preview(&self, cx: &App) -> AnyElement {
        let slot = self.settings.terminal_slot();
        let size = self.settings.size(slot) as f32;
        terminal_preview(self.family_of(slot, cx), size, cx)
    }
}

impl Render for AppearanceTypography {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.settings.clone();
        let d = TypographySettings::default();
        let wide = self.wide;
        let mut rows: Vec<AnyElement> = Vec::new();
        if s.separate {
            let prompt = self.prompt_preview(cx);
            let code = self.code_preview(cx);
            let terminal = self.terminal_preview(cx);
            rows.push(
                self.font_row(Slot::Interface, None, window, cx)
                    .into_any_element(),
            );
            rows.push(
                self.font_row(Slot::Prompt, Some(prompt), window, cx)
                    .into_any_element(),
            );
            rows.push(
                self.font_row(Slot::Code, Some(code), window, cx)
                    .into_any_element(),
            );
            rows.push(
                self.font_row(Slot::Terminal, Some(terminal), window, cx)
                    .into_any_element(),
            );
        } else {
            let prompt = self.prompt_preview(cx);
            let both = v_flex()
                .child(self.code_preview(cx))
                .child(self.terminal_preview(cx))
                .into_any_element();
            rows.push(
                self.font_row(Slot::Interface, Some(prompt), window, cx)
                    .into_any_element(),
            );
            rows.push(
                self.font_row(Slot::Code, Some(both), window, cx)
                    .into_any_element(),
            );
        }
        rows.push(
            row(
                "Word wrap",
                "Wrap long lines in code blocks, diffs, and file previews by default.",
                self.flag_reset(
                    "settings-reset-word-wrap",
                    s.word_wrap != d.word_wrap,
                    |s, d| s.word_wrap = d.word_wrap,
                    window,
                    cx,
                ),
                self.flag(
                    "settings-word-wrap",
                    s.word_wrap,
                    |s| s.word_wrap = !s.word_wrap,
                    window,
                    cx,
                ),
                wide,
                cx,
            )
            .into_any_element(),
        );
        let mut advanced = vec![
            row(
                "Separate fonts",
                "Set the prompt, code, and terminal fonts separately instead of one monospace font.",
                None,
                self.flag(
                    "settings-separate-fonts",
                    s.separate,
                    |s| s.separate = !s.separate,
                    window,
                    cx,
                ),
                wide,
                cx,
            )
            .into_any_element(),
        ];
        if cfg!(target_os = "macos") {
            advanced.push(
                row(
                    "Font smoothing",
                    "Use thinner grayscale text smoothing instead of the macOS default.",
                    self.flag_reset(
                        "settings-reset-font-smoothing",
                        s.smoothing != d.smoothing,
                        |s, d| s.smoothing = d.smoothing,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-font-smoothing",
                        s.smoothing,
                        |s| s.smoothing = !s.smoothing,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                )
                .into_any_element(),
            );
        }
        let view = cx.entity();
        let toggle: Toggle = Rc::new(move |_, cx| {
            view.update(cx, |this, cx| {
                this.advanced_open = !this.advanced_open;
                cx.notify();
            })
        });
        rows.push(disclosure(
            "settings-typography-advanced",
            self.advanced_open,
            toggle,
            advanced,
            window,
            cx,
        ));
        v_flex()
            .w_full()
            .on_key_down(cx.listener(|this, event, window, cx| this.key_down(event, window, cx)))
            .child(section("Typography", group(rows, cx)))
    }
}

/// One styled stretch of a preview line.
struct Seg {
    text: &'static str,
    color: Option<Hsla>,
    bold: bool,
    underline: bool,
    background: Option<Hsla>,
}

fn seg(text: &'static str, color: impl Into<Option<Hsla>>) -> Seg {
    Seg {
        text,
        color: color.into(),
        bold: false,
        underline: false,
        background: None,
    }
}

impl Seg {
    fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    fn on(mut self, background: Hsla) -> Self {
        self.background = Some(background);
        self
    }
}

fn line(segments: Vec<Seg>) -> StyledText {
    let mut text = String::new();
    let mut runs = Vec::new();
    for segment in &segments {
        let start = text.len();
        text.push_str(segment.text);
        runs.push((
            start..text.len(),
            HighlightStyle {
                color: segment.color,
                font_weight: segment.bold.then_some(FontWeight::BOLD),
                background_color: segment.background,
                underline: segment.underline.then(|| UnderlineStyle {
                    thickness: px(1.),
                    color: segment.color,
                    wavy: false,
                }),
                ..Default::default()
            },
        ));
    }
    StyledText::new(text).with_highlights(runs)
}

/// The chip accents of the composer: `ts-file` for a file, the skill color for a skill. They are
/// the OKLCH tokens of index.css converted to sRGB.
fn skill_accent(dark: bool) -> Hsla {
    rgb(if dark { 0xbb6ac8 } else { 0x93439f }).into()
}

fn file_accent(dark: bool) -> Hsla {
    rgb(if dark { 0x0f96d3 } else { 0x006a99 }).into()
}

/// An inline composer chip: `inline-flex gap-[0.2em] font-medium`, icon `1em`.
fn chip(label: &'static str, icon: Icons, color: Hsla, size: f32) -> gpui::Div {
    h_flex()
        .gap(px(size * 0.2))
        .items_center()
        .rounded(px(2.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .child(Icon::new(icon).size(px(size)))
        .child(label)
}

/// `PromptFontPreview`: the sentence in a `rounded-lg border bg-background px-3 py-2` box.
fn prompt_preview(family: SharedString, size: f32, cx: &App) -> AnyElement {
    let theme = Tokens::of(cx);
    let words = |text: &'static str| {
        text.split_inclusive(' ')
            .map(|word| div().child(word))
            .collect::<Vec<_>>()
    };
    h_flex()
        .flex_wrap()
        .items_center()
        .mt_1()
        .mb_2()
        .px_3()
        .py_2()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .font_family(family)
        .text_size(px(size))
        .line_height(px(size * 1.5))
        .text_color(theme.foreground)
        .children(words("Use "))
        .child(chip(
            "frontend-design",
            Icons::Box,
            skill_accent(theme.dark),
            size,
        ))
        .children(words(" to fix the flaky test in "))
        .child(chip(
            "surface.test.ts",
            Icons::File,
            file_accent(theme.dark),
            size,
        ))
        .children(words(" and align the header with "))
        .child(chip(
            "SettingsPanels.tsx",
            Icons::File,
            file_accent(theme.dark),
            size,
        ))
        .children(words(" before shipping."))
        .into_any_element()
}

/// The code surface (`--code-background`): `card` at 90% over `background`.
fn code_background(theme: &Tokens) -> Hsla {
    theme.background.blend(theme.card.opacity(0.9))
}

/// `CodeFontPreview`: the unified diff of `src/formatUser.ts`.
fn diff_preview(family: SharedString, size: f32, cx: &App) -> AnyElement {
    let theme = Tokens::of(cx);
    let dark = theme.dark;
    let pick = |light: u32, night: u32| -> Hsla { rgb(if dark { night } else { light }).into() };
    let keyword = pick(0xd52c36, 0xff678d);
    let function = pick(0x7b43f8, 0x9d6afb);
    let kind = pick(0x1ca1c7, 0x08c0ef);
    let string = pick(0x199f43, 0x5ecc71);
    let comment = pick(0x84848a, 0x84848a);
    let add = pick(0x199f43, 0x5ecc71);
    let del = pick(0xd52c36, 0xff6762);
    let fg = theme.foreground;
    let height = size * 1.5;
    let code_row = |number: &'static str, background: Option<Hsla>, text: StyledText| {
        h_flex()
            .h(px(height))
            .items_center()
            .when_some(background, |row, color| row.bg(color))
            .child(
                div()
                    .w(px(size * 3.))
                    .flex_shrink_0()
                    .pr_2()
                    .text_right()
                    .text_color(theme.muted_foreground)
                    .child(number),
            )
            .child(div().whitespace_nowrap().child(text))
    };
    let removed = code_row(
        "2",
        Some(del.opacity(0.12)),
        line(vec![
            seg("  ", fg),
            seg("return", keyword),
            seg(" user.name.", fg),
            seg("toUpperCase", function),
            seg("();", fg),
        ]),
    );
    let added = code_row(
        "2",
        Some(add.opacity(0.12)),
        line(vec![
            seg("  ", fg),
            seg("return", keyword),
            seg(" ", fg),
            seg("`${user.name} <${user.email}>`", string),
            seg("; ", fg),
            seg("// 0O 1lI", comment),
        ]),
    );
    v_flex()
        .mt_1()
        .mb_2()
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(code_background(&theme))
        .font_family(family)
        .text_size(px(size))
        .text_color(fg)
        .child(
            h_flex()
                .h(px(size * 2.4))
                .items_center()
                .gap_2()
                .px_3()
                .font_weight(FontWeight::MEDIUM)
                .child(Icon::new(Icons::File).size(px(size)))
                .child("src/formatUser.ts")
                .child(div().flex_1())
                .child(div().text_color(del).child("-1"))
                .child(div().text_color(add).child("+1")),
        )
        .child(code_row(
            "1",
            None,
            line(vec![
                seg("export", keyword),
                seg(" ", fg),
                seg("function", keyword),
                seg(" ", fg),
                seg("formatUser", function),
                seg("(", fg),
                seg("user", fg),
                seg(": ", fg),
                seg("User", kind),
                seg(") {", fg),
            ]),
        ))
        .child(removed)
        .child(added)
        .child(code_row("3", None, line(vec![seg("}", fg)])))
        .into_any_element()
}

/// The ANSI colors the transcript uses, as a terminal theme paints them.
struct Ansi {
    green: Hsla,
    cyan: Hsla,
    blue: Hsla,
    red: Hsla,
    yellow: Hsla,
}

fn ansi(dark: bool) -> Ansi {
    let pick = |light: u32, night: u32| -> Hsla { rgb(if dark { night } else { light }).into() };
    Ansi {
        green: pick(0x2f7d32, 0x98c379),
        cyan: pick(0x0e7490, 0x56b6c2),
        blue: pick(0x2457c5, 0x61afef),
        red: pick(0xc0392b, 0xe06c75),
        yellow: pick(0x9a6700, 0xe5c07b),
    }
}

/// `TerminalFontPreview`: the transcript in a `rounded-lg border` box that fits its rows.
fn terminal_preview(family: SharedString, size: f32, cx: &App) -> AnyElement {
    let theme = Tokens::of(cx);
    let c = ansi(theme.dark);
    let fg = theme.foreground;
    let dim: Hsla = fg.opacity(0.6);
    let prompt = || {
        vec![
            seg("\u{2192}", c.green).bold(),
            seg(" ", fg),
            seg("t3code", c.cyan).bold(),
            seg(" ", fg),
            seg("git:(", c.blue).bold(),
            seg("main", c.red).bold(),
            seg(")", c.blue).bold(),
            seg(" ", fg),
            seg("\u{2717}", c.yellow).bold(),
            seg(" ", fg),
        ]
    };
    let mut first = prompt();
    first.push(seg("vpr dev", fg));
    let mut last = prompt();
    last.push(seg(" ", fg).on(fg.opacity(0.8)));
    let lines: Vec<Vec<Seg>> = vec![
        first,
        vec![],
        vec![
            seg("  ", fg),
            seg("VITE", c.green).bold(),
            seg(" ", fg),
            seg("v7.1.1", c.green),
            seg("  ", fg),
            seg("ready in", dim),
            seg(" ", fg),
            seg("1.24s", fg).bold(),
        ],
        vec![],
        vec![
            seg("  ", fg),
            seg("\u{2192}", c.green),
            seg("  ", fg),
            seg("Local:", dim),
            seg("    ", fg),
            seg("http://127.0.0.1:5173/", c.cyan).underline(),
        ],
        vec![
            seg("  ", fg),
            seg("\u{2192}", c.green),
            seg("  ", fg),
            seg("Network:", dim),
            seg("  ", fg),
            seg("http://192.168.1.24:5173/", c.cyan).underline(),
        ],
        vec![],
        vec![
            seg("  ", fg),
            seg("\u{2713} 85 passed", c.green),
            seg("   ", fg),
            seg("\u{25b3} 2 warnings", c.yellow),
            seg("   ", fg),
            seg("\u{2717} 0 failed", c.red),
        ],
        vec![],
        vec![
            seg("  ", fg),
            seg(" READY ", gpui::black()).on(c.green),
            seg(" ", fg),
            seg("watching for changes, press", dim),
            seg(" ", fg),
            seg("q", fg).bold(),
            seg(" ", fg),
            seg("to quit", dim),
        ],
        vec![],
        last,
    ];
    debug_assert_eq!(lines.len(), TERMINAL_ROWS);
    let height = (size * 1.2).ceil();
    v_flex()
        .mt_1()
        .mb_2()
        .h(px(height * TERMINAL_ROWS as f32 + 2.))
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .font_family(family)
        .text_size(px(size))
        .text_color(fg)
        .children(lines.into_iter().map(|segments| {
            div()
                .h(px(height))
                .whitespace_nowrap()
                .child(line(segments))
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_labels_cover_every_range() {
        for slot in Slot::ALL {
            let range = slot.range();
            assert_eq!(px_labels(slot).len(), range.options().count());
            assert_eq!(px_label(range.min), format!("{} px", range.min));
            assert_eq!(px_label(range.max), format!("{} px", range.max));
        }
    }

    #[test]
    fn titles_follow_the_separate_flag() {
        assert_eq!(title(Slot::Code, false), "Monospace font");
        assert_eq!(title(Slot::Code, true), "Code font");
        assert_eq!(
            description(Slot::Code, false),
            "Code blocks, diffs, file previews, and the terminal."
        );
    }

    #[test]
    fn transcript_fills_the_box() {
        assert_eq!(TERMINAL_ROWS, 12);
    }
}
