//! The "Diffs & pull requests" tab: `DiffsSettingsPanel` (SettingsPanels.tsx:1289-1430) with
//! `DiffColorsSettingsRow` (1236-1287) and `PullRequestLinksOpenInAppSetting`
//! (IntegrationsSettings.tsx:775-802). It owns its select menu and focus handles, so the
//! General tab's state stays untouched.

use super::components::{
    HoverHandler, KeyHandler, Menu, PickHandler, SelectTrigger, group, reset_button, row, section,
    select_menu, switch,
};
use super::tokens::Tokens;
use super::*;
use crate::settings_diffs::{DiffColors, DiffLayout, DiffSettings, FileState};
use gpui::{FocusHandle, KeyDownEvent, rgb};
use std::rc::Rc;

/// The three selects of the tab.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Layout,
    Files,
    Colors,
}

impl Choice {
    const ALL: [Self; 3] = [Self::Layout, Self::Files, Self::Colors];

    fn id(self) -> &'static str {
        match self {
            Self::Layout => "settings-diff-layout",
            Self::Files => "settings-diff-file-state",
            Self::Colors => "settings-diff-colors",
        }
    }

    /// `w-full sm:w-40` on all three.
    fn width(self) -> f32 {
        160.
    }

    /// The menu's labels; the colors menu marks its default (SettingsPanels.tsx:1281).
    fn labels(self) -> Vec<&'static str> {
        match self {
            Self::Layout => DiffLayout::ALL.iter().map(|v| v.label()).collect(),
            Self::Files => FileState::ALL.iter().map(|v| v.label()).collect(),
            Self::Colors => DiffColors::ALL.iter().map(|v| v.menu_label()).collect(),
        }
    }

    /// What the trigger shows for the current value.
    fn value_label(self, settings: &DiffSettings) -> &'static str {
        match self {
            Self::Layout => settings.layout.label(),
            Self::Files => FileState::of(settings.files_collapsed).label(),
            Self::Colors => settings.colors.label(),
        }
    }

    fn selected(self, settings: &DiffSettings) -> usize {
        let index = match self {
            Self::Layout => DiffLayout::ALL.iter().position(|v| *v == settings.layout),
            Self::Files => FileState::ALL
                .iter()
                .position(|v| *v == FileState::of(settings.files_collapsed)),
            Self::Colors => DiffColors::ALL.iter().position(|v| *v == settings.colors),
        };
        index.unwrap_or(0)
    }

    fn apply(self, settings: &mut DiffSettings, index: usize) {
        match self {
            Self::Layout => settings.layout = DiffLayout::ALL[index],
            Self::Files => settings.files_collapsed = FileState::ALL[index].collapsed(),
            Self::Colors => settings.colors = DiffColors::ALL[index],
        }
    }
}

pub struct DiffsPanel {
    settings: DiffSettings,
    menu: Option<(Choice, usize)>,
    triggers: [FocusHandle; 3],
    wide: bool,
}

impl DiffsPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            settings: DiffSettings::load(),
            menu: None,
            triggers: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            wide: true,
        }
    }

    pub fn settings(&self) -> DiffSettings {
        self.settings
    }

    /// Re-reads the file and closes any menu, as the dialog does when it opens.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.settings = DiffSettings::load();
        self.menu = None;
        cx.notify();
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    pub fn restore_defaults(&mut self, cx: &mut Context<Self>) {
        self.update(|settings| *settings = DiffSettings::default(), cx);
    }

    /// Closes an open menu; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((choice, _)) = self.menu.take() else {
            return false;
        };
        let slot = Choice::ALL.iter().position(|c| *c == choice).unwrap_or(0);
        window.focus(&self.triggers[slot], cx);
        cx.notify();
        true
    }

    fn open_menu(&mut self, choice: Choice, cx: &mut Context<Self>) {
        self.menu = Some((choice, choice.selected(&self.settings)));
        cx.notify();
    }

    fn pick(&mut self, choice: Choice, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.update(|settings| choice.apply(settings, index), cx);
        self.close_menu(window, cx);
    }

    fn update(&mut self, change: impl FnOnce(&mut DiffSettings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.settings.save();
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some((choice, highlighted)) = self.menu else {
            return;
        };
        let count = choice.labels().len();
        match event.keystroke.key.as_str() {
            "down" => self.menu = Some((choice, (highlighted + 1) % count)),
            "up" => self.menu = Some((choice, (highlighted + count - 1) % count)),
            "home" => self.menu = Some((choice, 0)),
            "end" => self.menu = Some((choice, count - 1)),
            "enter" | "space" => self.pick(choice, highlighted, window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn reset(
        &self,
        id: &'static str,
        changed: bool,
        restore: fn(&mut DiffSettings, &DiffSettings),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = cx.entity();
        changed.then(|| {
            reset_button(
                id,
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.update(|s| restore(s, &DiffSettings::default()), cx)
                    })
                },
                window,
                cx,
            )
        })
    }

    fn flag(
        &self,
        id: &'static str,
        checked: bool,
        toggle: fn(&mut DiffSettings),
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

    /// Two 8px dots with a 4px gap (`size-2 rounded-full`, `gap-1`).
    fn dots(&self, dark: bool) -> AnyElement {
        h_flex()
            .flex_shrink_0()
            .gap_1()
            .children(
                self.settings
                    .colors
                    .dots(dark)
                    .map(|hex| div().size(px(8.)).rounded_full().bg(rgb(hex))),
            )
            .into_any_element()
    }

    /// `Select` (select.tsx) over a `Popover`, as the General tab builds it.
    fn select(&self, choice: Choice, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let labels = choice.labels();
        let selected = choice.selected(&self.settings);
        let open = matches!(self.menu, Some((open, _)) if open == choice);
        let highlighted = match self.menu {
            Some((open, index)) if open == choice => index,
            _ => usize::MAX,
        };
        let slot = Choice::ALL.iter().position(|c| *c == choice).unwrap_or(0);
        let on_key: KeyHandler = {
            let view = view.clone();
            Rc::new(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.open_menu(choice, cx));
                }
            })
        };
        let mut trigger = SelectTrigger::new(
            choice.id(),
            choice.value_label(&self.settings),
            choice.width(),
            self.triggers[slot].clone(),
            on_key,
        );
        if choice == Choice::Colors {
            trigger = trigger.leading(self.dots(Tokens::of(cx).dark));
        }
        let on_pick: PickHandler = {
            let view = view.clone();
            Rc::new(move |index, window, cx| {
                view.update(cx, |this, cx| this.pick(choice, index, window, cx))
            })
        };
        let on_hover: HoverHandler = {
            let view = view.clone();
            Rc::new(move |index, cx| {
                view.update(cx, |this, cx| {
                    if this.menu.is_some_and(|(_, current)| current != index) {
                        this.menu = Some((choice, index));
                        cx.notify();
                    }
                })
            })
        };
        Popover::new(("settings-diff-select", slot))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change(move |opened, window, cx| {
                view.update(cx, |this, cx| {
                    if *opened {
                        this.open_menu(choice, cx);
                    } else if this.menu.is_some_and(|(current, _)| current == choice) {
                        this.close_menu(window, cx);
                    }
                })
            })
            .trigger(trigger)
            .content(move |_, _, cx| {
                select_menu(
                    &Menu {
                        id: choice.id(),
                        labels: labels.clone(),
                        selected,
                        highlighted,
                        width: choice.width(),
                        on_pick: on_pick.clone(),
                        on_hover: on_hover.clone(),
                    },
                    cx,
                )
            })
            .into_any_element()
    }
}

impl Render for DiffsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.settings;
        let d = DiffSettings::default();
        let wide = self.wide;
        let rows = |items: Vec<gpui::Div>| -> Vec<AnyElement> {
            items
                .into_iter()
                .map(IntoElement::into_any_element)
                .collect()
        };

        let diffs = group(
            rows(vec![
                row(
                    "Diff layout",
                    "Show diffs stacked or side by side. The toggle in the diff toolbar changes this too.",
                    self.reset(
                        "settings-reset-diff-layout",
                        s.layout != d.layout,
                        |s, d| s.layout = d.layout,
                        window,
                        cx,
                    ),
                    self.select(Choice::Layout, cx),
                    wide,
                    cx,
                ),
                row(
                    "Hide whitespace changes",
                    "Set whether the diff panel ignores whitespace-only edits by default.",
                    self.reset(
                        "settings-reset-diff-whitespace",
                        s.ignore_whitespace != d.ignore_whitespace,
                        |s, d| s.ignore_whitespace = d.ignore_whitespace,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-diff-whitespace",
                        s.ignore_whitespace,
                        |s| s.ignore_whitespace = !s.ignore_whitespace,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Default diff file state",
                    "Start with files expanded or collapsed when opening diffs or a pull request's Code tab.",
                    self.reset(
                        "settings-reset-diff-file-state",
                        s.files_collapsed != d.files_collapsed,
                        |s, d| s.files_collapsed = d.files_collapsed,
                        window,
                        cx,
                    ),
                    self.select(Choice::Files, cx),
                    wide,
                    cx,
                ),
                row(
                    "Diff colors",
                    "Choose colors for additions and deletions, including change counts.",
                    self.reset(
                        "settings-reset-diff-colors",
                        s.colors != d.colors,
                        |s, d| s.colors = d.colors,
                        window,
                        cx,
                    ),
                    self.select(Choice::Colors, cx),
                    wide,
                    cx,
                ),
            ]),
            cx,
        );
        let pull_requests = group(
            rows(vec![
                row(
                    "Open pull requests and changes panels proactively",
                    "Open linked pull requests first. Otherwise, open Changes for edits to at least 3 files or 50 lines.",
                    self.reset(
                        "settings-reset-proactive-panels",
                        s.proactive_panels != d.proactive_panels,
                        |s, d| s.proactive_panels = d.proactive_panels,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-proactive-panels",
                        s.proactive_panels,
                        |s| s.proactive_panels = !s.proactive_panels,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Open pull requests in T3 Code",
                    "Clicking a pull request link opens it in T3 Code. Turn off to open pull request links like any other link, on your git host.",
                    self.reset(
                        "settings-reset-pr-links-in-app",
                        s.pr_links_in_app != d.pr_links_in_app,
                        |s, d| s.pr_links_in_app = d.pr_links_in_app,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-pr-links-in-app",
                        s.pr_links_in_app,
                        |s| s.pr_links_in_app = !s.pr_links_in_app,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
            ]),
            cx,
        );
        v_flex()
            .w_full()
            .gap_8()
            .on_key_down(cx.listener(|this, event, window, cx| this.key_down(event, window, cx)))
            .child(section("Diffs", diffs))
            .child(section("Pull requests", pull_requests))
    }
}
