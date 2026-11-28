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
use crate::settings_typography::{
    Slot, TypographySettings, default_family_label, installed_families,
};
use gpui::{
    FocusHandle, FontWeight, HighlightStyle, Hsla, KeyDownEvent, Rgba, StyledText, UnderlineStyle,
    rgb,
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
        let families = installed_families(cx)
            .iter()
            .filter(|name| !name.starts_with('.'))
            .map(|name| Choice {
                value: name.clone(),
                label: name.clone(),
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
        self.settings.apply_to_theme(cx);
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
    fn default_label(&self, slot: Slot, cx: &App) -> String {
        let installed = installed_families(cx);
        match slot {
            Slot::Interface => default_family_label(false, installed),
            Slot::Prompt => {
                let sans = self.settings.family(Slot::Interface);
                if sans.is_empty() {
                    default_family_label(false, installed)
                } else {
                    sans.to_owned()
                }
            }
            Slot::Code | Slot::Terminal => default_family_label(true, installed),
        }
    }

    fn family_picker(&self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let open = self.font_open == Some(slot);
        let family = self.settings.family(slot).to_owned();
        let default = self.default_label(slot, cx);
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
                ..Default::default()
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
                        icons: Vec::new(),
                        headings: Vec::new(),
                        descriptions: Vec::new(),
                        id,
                        labels: labels.iter().map(|l| l.to_string().into()).collect(),
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

/// `color-mix(in lab, a share_of_a%, b)`: CSS Lab (D50) interpolation, as Pierre's diff tokens use.
fn lab_mix(a: Hsla, b: Hsla, share_of_a: f32) -> Hsla {
    const TO_XYZ: [[f32; 3]; 3] = [
        [0.412_390_8, 0.357_584_34, 0.180_480_8],
        [0.212_639, 0.715_168_7, 0.072_192_32],
        [0.019_330_82, 0.119_194_78, 0.950_532_15],
    ];
    const D65_TO_D50: [[f32; 3]; 3] = [
        [1.047_929_8, 0.022_946_794, -0.050_192_23],
        [0.029_627_816, 0.990_434_5, -0.017_073_825],
        [-0.009_243_058, 0.015_055_145, 0.751_874_3],
    ];
    const D50_TO_D65: [[f32; 3]; 3] = [
        [0.955_473_4, -0.023_098_537, 0.063_259_31],
        [-0.028_369_707, 1.009_995_5, 0.021_041_399],
        [0.012_314_002, -0.020_507_697, 1.330_366],
    ];
    const FROM_XYZ: [[f32; 3]; 3] = [
        [3.240_97, -1.537_383_2, -0.498_610_76],
        [-0.969_243_65, 1.875_967_5, 0.041_555_06],
        [0.055_630_08, -0.203_976_96, 1.056_971_5],
    ];
    const WHITE: [f32; 3] = [0.964_295_7, 1., 0.825_104_6];
    const EPSILON: f32 = 216. / 24_389.;
    const KAPPA: f32 = 24_389. / 27.;

    fn apply(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
        m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
    }
    fn to_lab(color: Hsla) -> [f32; 3] {
        let c = Rgba::from(color);
        let linear = [c.r, c.g, c.b].map(|v| {
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        let xyz = apply(&D65_TO_D50, apply(&TO_XYZ, linear));
        let f = [0, 1, 2].map(|i| {
            let t = xyz[i] / WHITE[i];
            if t > EPSILON {
                t.cbrt()
            } else {
                (KAPPA * t + 16.) / 116.
            }
        });
        [
            116. * f[1] - 16.,
            500. * (f[0] - f[1]),
            200. * (f[1] - f[2]),
        ]
    }
    fn from_lab(lab: [f32; 3]) -> Hsla {
        let fy = (lab[0] + 16.) / 116.;
        let f = [fy + lab[1] / 500., fy, fy - lab[2] / 200.];
        let xyz = [0, 1, 2].map(|i| {
            let cubed = f[i].powi(3);
            let t = if cubed > EPSILON {
                cubed
            } else {
                (116. * f[i] - 16.) / KAPPA
            };
            t * WHITE[i]
        });
        let [r, g, b] = apply(&FROM_XYZ, apply(&D50_TO_D65, xyz)).map(|v| {
            let v = v.clamp(0., 1.);
            if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            }
        });
        Rgba { r, g, b, a: 1. }.into()
    }
    let (a, b) = (to_lab(a), to_lab(b));
    from_lab([0, 1, 2].map(|i| a[i] * share_of_a + b[i] * (1. - share_of_a)))
}

/// The tokens of Pierre's `pierre-dark` and `t3-light` themes that the preview's tokens use
/// (`@pierre/highlights` themes, light darkened by `readableSyntaxColor`).
struct Syntax {
    keyword: Hsla,
    function: Hsla,
    punctuation: Hsla,
    parameter: Hsla,
    kind: Hsla,
    property: Hsla,
    string: Hsla,
    comment: Hsla,
}

/// Git colors the theme hands Pierre as `--diffs-{addition,deletion,modified}-color`.
struct GitColors {
    addition: Hsla,
    deletion: Hsla,
    modified: Hsla,
}

/// `CodeFontPreview`: Pierre's unified diff of `src/formatUser.ts`, one 20px row per line
/// (`--diffs-line-height`), a 4ch gutter with a 2px seam, `data-indicators="bars"`.
fn diff_preview(family: SharedString, size: f32, cx: &App) -> AnyElement {
    let theme = Tokens::of(cx);
    let dark = theme.dark;
    let pick = |light: u32, night: u32| -> Hsla { rgb(if dark { night } else { light }).into() };
    let syntax = Syntax {
        keyword: pick(0xb12251, 0xff678d),
        function: pick(0x693acf, 0x9d6afb),
        punctuation: pick(0x4b4b4b, 0x636363),
        parameter: pick(0x4b4b4b, 0xa3a3a3),
        kind: pick(0x912ba7, 0xd568ea),
        property: pick(0x8f4e17, 0xffa359),
        string: pick(0x106a2b, 0x5ecc71),
        comment: pick(0x5c5c5c, 0x737373),
    };
    let git = GitColors {
        addition: pick(0x18a46c, 0x07c480),
        deletion: pick(0xd52c36, 0xff2e3f),
        modified: pick(0x009fff, 0x009fff),
    };
    let bg = code_background(&theme);
    let fg = theme.foreground;
    let number = lab_mix(fg, bg, 0.65);
    // Gutter cells mix 91% (85% dark) of the surface with the base, rows 88% (80%).
    let (gutter_share, line_share, emphasis) = if dark {
        (0.85, 0.8, 0.2)
    } else {
        (0.91, 0.88, 0.15)
    };
    let ch = cx
        .text_system()
        .advance(
            cx.text_system().resolve_font(&gpui::font(family.clone())),
            px(size),
            '0',
        )
        .map_or(size * 0.6, |advance| f32::from(advance.width));
    let row_height = 20.;

    let row = |digit: &'static str, base: Option<Hsla>, dashed: bool, text: StyledText| {
        let (gutter_bg, line_bg, digit_color) = match base {
            Some(base) => (
                lab_mix(bg, base, gutter_share),
                lab_mix(bg, base, line_share),
                base,
            ),
            None => (bg, bg, number),
        };
        let bar = base.map(|base| {
            let mut bar = v_flex()
                .absolute()
                .top_0()
                .left_0()
                .w(px(4.))
                .h(px(row_height));
            if dashed {
                let gap = lab_mix(bg, base, line_share);
                for _ in 0..(row_height / 2.) as usize {
                    bar = bar
                        .child(div().h(px(1.)).bg(base))
                        .child(div().h(px(1.)).bg(gap));
                }
                bar
            } else {
                bar.bg(base)
            }
        });
        h_flex()
            .h(px(row_height))
            .items_center()
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .h_full()
                    .w(px(ch * 4.))
                    .pr(px(ch))
                    .flex()
                    .items_center()
                    .justify_end()
                    .bg(gutter_bg)
                    .text_color(digit_color)
                    .when_some(bar, |cell, bar| cell.child(bar))
                    .child(digit),
            )
            .child(div().flex_shrink_0().w(px(2.)).h_full().bg(bg))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .px(px(ch))
                    .bg(line_bg)
                    .whitespace_nowrap()
                    .child(text),
            )
    };

    let s = &syntax;
    let p = s.punctuation;
    let removed_emphasis = git.deletion.opacity(emphasis);
    let removed_line = lab_mix(bg, git.deletion, line_share);
    let added_emphasis = git.addition.opacity(emphasis);
    let added_line = lab_mix(bg, git.addition, line_share);
    let del_span =
        |text: &'static str, color: Hsla| seg(text, color).on(removed_line.blend(removed_emphasis));
    let add_span =
        |text: &'static str, color: Hsla| seg(text, color).on(added_line.blend(added_emphasis));
    let removed = row(
        "2",
        Some(git.deletion),
        true,
        line(vec![
            seg("  return ", s.keyword),
            seg("user", s.property),
            seg(".", p),
            seg("name", s.property),
            seg(".", p),
            del_span("toUpperCase", s.function),
            del_span("()", p),
            del_span(";", p),
        ]),
    );
    let added = row(
        "2",
        Some(git.addition),
        false,
        line(vec![
            seg("  return ", s.keyword),
            add_span("`", s.string),
            add_span("${", s.keyword),
            seg("user", s.property),
            seg(".", p),
            seg("name", s.property),
            add_span("}", s.keyword),
            add_span(" <", s.string),
            add_span("${", s.keyword),
            add_span("user", s.property),
            add_span(".", p),
            add_span("email", s.property),
            add_span("}", s.keyword),
            add_span(">`", s.string),
            add_span(";", p),
            add_span(" ", p),
            add_span("// 0O 1lI", s.comment),
        ]),
    );
    v_flex()
        .mt_1()
        .mb_2()
        .overflow_hidden()
        .bg(bg)
        .text_size(px(size))
        .line_height(px(row_height))
        .text_color(fg)
        .child(
            h_flex()
                .h(px(row_height + 24.))
                .items_center()
                .gap(px(8.))
                .px(px(16.))
                .child(
                    gpui::svg()
                        .path(crate::assets::DIFF_SYMBOL_MODIFIED)
                        .flex_shrink_0()
                        .size(px(16.))
                        .text_color(git.modified),
                )
                .child("src/formatUser.ts")
                .child(div().flex_1())
                .child(
                    h_flex()
                        .gap(px(ch))
                        .font_family(family.clone())
                        .child(div().text_color(git.deletion).child("-1"))
                        .child(div().text_color(git.addition).child("+1")),
                ),
        )
        .child(
            v_flex()
                .font_family(family)
                .pb(px(8.))
                .child(row(
                    "1",
                    None,
                    false,
                    line(vec![
                        seg("export ", s.keyword),
                        seg("function ", s.keyword),
                        seg("formatUser", s.function),
                        seg("(", p),
                        seg("user", s.parameter),
                        seg(": ", s.keyword),
                        seg("User", s.kind),
                        seg(") {", p),
                    ]),
                ))
                .child(removed)
                .child(added)
                .child(row("3", None, false, line(vec![seg("}", p)]))),
        )
        .into_any_element()
}

/// The ANSI colors the transcript uses: `TERMINAL_PALETTES` (ThreadTerminalDrawer.tsx:239-276),
/// indices 0 (black), 2 (green), 6 (cyan), 4 (blue), 1 (red) and 3 (yellow).
struct Ansi {
    black: Hsla,
    green: Hsla,
    cyan: Hsla,
    blue: Hsla,
    red: Hsla,
    yellow: Hsla,
}

fn ansi(dark: bool) -> Ansi {
    let pick = |light: u32, night: u32| -> Hsla { rgb(if dark { night } else { light }).into() };
    Ansi {
        black: pick(0x000000, 0x000000),
        green: pick(0x107c10, 0x0dbc79),
        cyan: pick(0x0598bc, 0x11a8cd),
        blue: pick(0x0451a5, 0x2472c8),
        red: pick(0xcd3131, 0xcd3131),
        yellow: pick(0x949800, 0xe5e510),
    }
}

/// `TerminalFontPreview`: the transcript on a Ghostty surface (`CONTENT_PADDING` 4, cells
/// `round(size * 1.35)` tall) in a `rounded-lg border` box that fits its rows.
fn terminal_preview(family: SharedString, size: f32, cx: &App) -> AnyElement {
    let theme = Tokens::of(cx);
    let c = ansi(theme.dark);
    let fg = theme.foreground;
    let bg = theme.background;
    // libghostty draws SGR 2 as `(fg * 155 + bg * 100) / 255`.
    let dim: Hsla = {
        let (f, b) = (Rgba::from(fg), Rgba::from(bg));
        let mix = |f: f32, b: f32| (f * 155. + b * 100.) / 255.;
        Rgba {
            r: mix(f.r, b.r),
            g: mix(f.g, b.g),
            b: mix(f.b, b.b),
            a: 1.,
        }
        .into()
    };
    let cursor: Hsla = if theme.dark {
        rgb(0xb4cbff)
    } else {
        rgb(0x26384e)
    }
    .into();
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
            seg(" READY ", c.black).on(c.green),
            seg(" ", fg),
            seg("watching for changes, press", dim),
            seg(" ", fg),
            seg("q", fg).bold(),
            seg(" ", fg),
            seg("to quit", dim),
        ],
        vec![],
        prompt(),
    ];
    debug_assert_eq!(lines.len(), TERMINAL_ROWS);
    let height = (size * 1.35).round().max(1.);
    let cell_width = cx
        .text_system()
        .advance(
            cx.text_system().resolve_font(&gpui::font(family.clone())),
            px(size),
            'M',
        )
        .map_or(size * 0.6, |advance| f32::from(advance.width));
    let last = lines.len() - 1;
    v_flex()
        .mt_1()
        .mb_2()
        .h(px(height * TERMINAL_ROWS as f32 + 8. + 2.))
        .p(px(4.))
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(bg)
        .font_family(family)
        .text_size(px(size))
        .text_color(fg)
        .children(lines.into_iter().enumerate().map(|(index, segments)| {
            h_flex()
                .h(px(height))
                .flex_shrink_0()
                .items_center()
                .whitespace_nowrap()
                .child(line(segments))
                .when(index == last, |row| {
                    row.child(
                        div()
                            .w(px(cell_width))
                            .h(px(height))
                            .border_1()
                            .border_color(cursor),
                    )
                })
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
