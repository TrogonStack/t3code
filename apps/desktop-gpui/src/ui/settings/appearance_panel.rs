//! The Appearance tab, minus typography: the color scheme tiles and theme cards of
//! `ThemeLibrary` (ThemeSettings.tsx:640-860) and the Interface section of
//! `AppearanceSettingsPanel` (AppearanceSettingsPanel.tsx:164-486). Theme, contrast, glass and
//! animation choices are saved but not yet applied to the running app.

use super::appearance_themes::{orb, wireframe};
use super::components::{
    Change, HoverHandler, KeyHandler, Menu, PickHandler, SelectTrigger, disclosure, group,
    outline_button, popup_shell, reset_button, row, section, select_menu, slider, switch,
};
use super::tokens::Tokens;
use super::*;
use crate::settings_appearance::{
    AppearanceSettings, CONTRAST, ChatWidth, ColorMode, EffortDisplay, EnvironmentIdentification,
    GLASS, PANEL_MS, Slide, THREAD_ITEMS, ThemeChoice, ThemeId,
};
use gpui::{BoxShadow, FocusHandle, KeyDownEvent, MouseButton, point, relative};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Width,
    Details,
    Effort,
    Environment,
}

impl Choice {
    const ALL: [Self; 4] = [Self::Width, Self::Details, Self::Effort, Self::Environment];

    fn id(self) -> &'static str {
        match self {
            Self::Width => "settings-chat-width",
            Self::Details => "settings-thread-details",
            Self::Effort => "settings-effort-display",
            Self::Environment => "settings-environment-identification",
        }
    }

    fn slot(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    fn labels(self) -> Vec<&'static str> {
        match self {
            Self::Width => ChatWidth::ALL.iter().map(|v| v.menu_label()).collect(),
            Self::Details => THREAD_ITEMS.iter().map(|item| item.label).collect(),
            Self::Effort => EffortDisplay::ALL.iter().map(|v| v.menu_label()).collect(),
            Self::Environment => EnvironmentIdentification::ALL
                .iter()
                .map(|v| v.label())
                .collect(),
        }
    }

    fn selected(self, settings: &AppearanceSettings) -> usize {
        let index = match self {
            Self::Width => ChatWidth::ALL
                .iter()
                .position(|v| *v == settings.chat_width),
            Self::Effort => EffortDisplay::ALL
                .iter()
                .position(|v| *v == settings.effort_display),
            Self::Environment => Some(settings.environment_identification.index()),
            Self::Details => None,
        };
        index.unwrap_or(0)
    }

    fn value_label(self, settings: &AppearanceSettings) -> String {
        match self {
            Self::Width => settings.chat_width.label().into(),
            Self::Effort => settings.effort_display.label().into(),
            Self::Environment => settings.environment_identification.label().into(),
            Self::Details => format!("{} of {} on", settings.items_on(), THREAD_ITEMS.len()),
        }
    }

    fn apply(self, settings: &mut AppearanceSettings, index: usize) {
        match self {
            Self::Width => settings.chat_width = ChatWidth::ALL[index],
            Self::Effort => settings.effort_display = EffortDisplay::ALL[index],
            Self::Environment => {
                settings.environment_identification = EnvironmentIdentification::ALL[index]
            }
            Self::Details => {}
        }
    }
}

struct SlideSpec {
    id: &'static str,
    range: Slide,
    value: u32,
    badge: String,
    badge_width: f32,
    write: fn(&mut AppearanceSettings, u32),
}

pub struct AppearancePanel {
    settings: AppearanceSettings,
    theme: ThemeChoice,
    menu: Option<(Choice, usize)>,
    triggers: [FocusHandle; 4],
    advanced_open: bool,
    preview_open: bool,
    wide: bool,
    content_width: f32,
}

impl AppearancePanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            settings: AppearanceSettings::load(),
            theme: ThemeChoice::load(),
            menu: None,
            triggers: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            advanced_open: false,
            preview_open: true,
            wide: true,
            content_width: 896.,
        }
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.settings = AppearanceSettings::load();
        self.theme = ThemeChoice::load();
        self.menu = None;
        cx.notify();
    }

    pub fn set_layout(&mut self, wide: bool, content_width: f32, cx: &mut Context<Self>) {
        if self.wide != wide || self.content_width != content_width {
            self.wide = wide;
            self.content_width = content_width;
            cx.notify();
        }
    }

    pub fn open_advanced(&mut self, cx: &mut Context<Self>) {
        self.advanced_open = true;
        cx.notify();
    }

    pub fn restore_defaults(&mut self, cx: &mut Context<Self>) {
        self.update(|settings| *settings = AppearanceSettings::default(), cx);
    }

    pub fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((choice, _)) = self.menu.take() else {
            return false;
        };
        window.focus(&self.triggers[choice.slot()], cx);
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

    fn toggle_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let item = &THREAD_ITEMS[index];
        self.update(
            |settings| settings.set_item(item, !settings.item_on(item)),
            cx,
        );
    }

    fn update(&mut self, change: impl FnOnce(&mut AppearanceSettings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.settings.save();
        cx.notify();
    }

    fn update_theme(&mut self, change: impl FnOnce(&mut ThemeChoice), cx: &mut Context<Self>) {
        change(&mut self.theme);
        self.theme.save();
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
            "enter" | "space" if choice == Choice::Details => self.toggle_item(highlighted, cx),
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
        restore: fn(&mut AppearanceSettings, &AppearanceSettings),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = cx.entity();
        changed.then(|| {
            reset_button(
                id,
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.update(|s| restore(s, &AppearanceSettings::default()), cx)
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
        toggle: fn(&mut AppearanceSettings),
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

    /// `min-w-12 rounded-md bg-muted px-2 py-1` mono value readout beside a slider.
    fn badge(text: String, min_width: f32, cx: &App) -> gpui::Div {
        let theme = Tokens::of(cx);
        div()
            .min_w(px(min_width))
            .flex_shrink_0()
            .px_2()
            .py_1()
            .rounded(px(6.))
            .bg(theme.muted)
            .text_center()
            .font_family(cx.theme().mono_font_family.clone())
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.foreground)
            .child(text)
    }

    fn slide(&self, spec: SlideSpec, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let SlideSpec {
            id,
            range,
            value,
            badge,
            badge_width,
            write,
        } = spec;
        let view = cx.entity();
        let change: Change = Rc::new(move |value, _, cx| {
            view.update(cx, |this, cx| this.update(|s| write(s, value), cx))
        });
        h_flex()
            .w_full()
            .items_center()
            .gap(px(12.))
            .child(Self::badge(badge, badge_width, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(slider(id, range, value, change, window, cx)),
            )
    }

    fn trigger(&self, choice: Choice, cx: &mut Context<Self>) -> SelectTrigger {
        let view = cx.entity();
        let on_key: KeyHandler = Rc::new(move |event, _, cx| {
            if matches!(
                event.keystroke.key.as_str(),
                "enter" | "space" | "down" | "up"
            ) {
                cx.stop_propagation();
                view.update(cx, |this, cx| this.open_menu(choice, cx));
            }
        });
        SelectTrigger::new(
            choice.id(),
            choice.value_label(&self.settings),
            160.,
            self.triggers[choice.slot()].clone(),
            on_key,
        )
    }

    fn popover(&self, choice: Choice, cx: &mut Context<Self>) -> Popover {
        let view = cx.entity();
        let open = matches!(self.menu, Some((open, _)) if open == choice);
        Popover::new(("settings-appearance-select", choice.slot()))
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
    }

    fn select(&self, choice: Choice, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let labels = choice.labels();
        let selected = choice.selected(&self.settings);
        let highlighted = match self.menu {
            Some((open, index)) if open == choice => index,
            _ => usize::MAX,
        };
        let on_pick: PickHandler = {
            let view = view.clone();
            Rc::new(move |index, window, cx| {
                view.update(cx, |this, cx| this.pick(choice, index, window, cx))
            })
        };
        let on_hover: HoverHandler = Rc::new(move |index, cx| {
            view.update(cx, |this, cx| {
                if this.menu.is_some_and(|(_, current)| current != index) {
                    this.menu = Some((choice, index));
                    cx.notify();
                }
            })
        });
        self.popover(choice, cx)
            .trigger(self.trigger(choice, cx))
            .content(move |_, _, cx| {
                select_menu(
                    &Menu {
                        id: choice.id(),
                        labels: labels.clone(),
                        selected,
                        highlighted,
                        width: 160.,
                        on_pick: on_pick.clone(),
                        on_hover: on_hover.clone(),
                    },
                    cx,
                )
            })
            .into_any_element()
    }

    /// `ThreadDetailsItemsMenu`: one checkbox row per card item; the menu stays open while
    /// items are toggled.
    fn details(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let highlighted = match self.menu {
            Some((Choice::Details, index)) => index,
            _ => usize::MAX,
        };
        let on: Vec<bool> = THREAD_ITEMS
            .iter()
            .map(|item| self.settings.item_on(item))
            .collect();
        self.popover(Choice::Details, cx)
            .trigger(self.trigger(Choice::Details, cx))
            .content(move |_, _, cx| {
                let theme = Tokens::of(cx);
                let mut menu = popup_shell(224., cx);
                for (index, item) in THREAD_ITEMS.iter().enumerate() {
                    let (toggle, hover) = (view.clone(), view.clone());
                    menu = menu.child(
                        h_flex()
                            .id(("settings-thread-item", index))
                            .min_h(px(28.))
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .text_sm()
                            .text_color(theme.foreground)
                            .cursor_pointer()
                            .when(highlighted == index, |row| row.bg(theme.accent))
                            .on_hover(move |entered, _, cx| {
                                if *entered {
                                    hover.update(cx, |this, cx| {
                                        if this.menu.is_some_and(|(_, current)| current != index) {
                                            this.menu = Some((Choice::Details, index));
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .on_click(move |_, _, cx| {
                                toggle.update(cx, |this, cx| this.toggle_item(index, cx))
                            })
                            .child(div().size(px(14.)).flex_shrink_0().when(on[index], |slot| {
                                slot.child(Icon::new(Icons::Check).size(px(14.)))
                            }))
                            .child(item.label),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// `PanelAnimationsPreview`: replay by click; the panels snap, as nothing animates here yet.
    fn preview(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let _ = window;
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let open = self.preview_open;
        let line = |width: f32, alpha: f32| {
            div()
                .h(px(2.))
                .w(relative(width))
                .rounded_full()
                .bg(theme.muted_foreground.opacity(alpha))
        };
        h_flex()
            .id("settings-panel-animations-preview")
            .h(px(40.))
            .w_full()
            .overflow_hidden()
            .p_1()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.preview_open = !this.preview_open;
                    cx.notify();
                })
            })
            .child(
                div()
                    .h_full()
                    .flex_shrink_0()
                    .rounded(px(6.))
                    .bg(theme.sidebar)
                    .w(px(if open { 16. } else { 0. })),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .px_1()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .pt_1()
                            .child(line(1., 0.25))
                            .child(line(0.8, 0.2))
                            .child(line(0.6, 0.15)),
                    )
                    .when(open, |column| {
                        column.child(
                            h_flex()
                                .h(px(8.))
                                .flex_shrink_0()
                                .items_center()
                                .px_2()
                                .border_t_1()
                                .border_color(theme.border.opacity(0.7))
                                .bg(theme.foreground.opacity(0.05))
                                .child(
                                    div()
                                        .h(px(1.))
                                        .w(relative(0.67))
                                        .bg(theme.muted_foreground.opacity(0.25)),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .h_full()
                    .flex_shrink_0()
                    .rounded(px(6.))
                    .bg(theme.muted)
                    .w(px(if open { 20. } else { 0. })),
            )
            .into_any_element()
    }

    fn mode_tiles(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = Tokens::of(cx);
        h_flex()
            .w_full()
            .gap_3()
            .children(ColorMode::ALL.iter().enumerate().map(|(index, &mode)| {
                let active = self.theme.mode == mode;
                let view = cx.entity();
                let tile = v_flex()
                    .id(("settings-color-mode", index))
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .p_2()
                    .rounded(px(12.))
                    .border_1()
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.update_theme(|t| t.mode = mode, cx))
                    });
                let tile = if active {
                    let ring = BoxShadow {
                        color: theme.ring,
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(2.),
                        inset: true,
                    };
                    tile.border_color(gpui::transparent_black())
                        .bg(theme.accent.opacity(0.3))
                        .shadow(vec![ring])
                } else {
                    let hover = theme.accent.opacity(0.1);
                    tile.border_color(theme.border.opacity(0.7))
                        .bg(theme.card.opacity(0.6))
                        .hover(move |style| style.bg(hover))
                };
                tile.child(wireframe(mode, self.theme, cx)).child(
                    div()
                        .flex()
                        .justify_center()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if active {
                            theme.foreground
                        } else {
                            theme.muted_foreground
                        })
                        .child(mode.label()),
                )
            }))
    }

    fn orb_button(
        &self,
        id: ThemeId,
        dark: bool,
        picked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let tip = if dark {
            "Use for dark mode only"
        } else {
            "Use for light mode only"
        };
        let ring_slot =
            (ThemeId::ALL.iter().position(|t| *t == id).unwrap_or(0) * 2) + dark as usize;
        div()
            .id(("settings-theme-orb", ring_slot))
            .relative()
            .size(px(68.))
            .flex_shrink_0()
            .p(px(6.))
            .rounded_full()
            .cursor_pointer()
            .tooltip(tooltip(tip))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                view.update(cx, |this, cx| {
                    this.update_theme(|t| t.pick_half(id, dark), cx)
                })
            })
            .child(orb(id.orb(dark), dark, cx))
            .when(picked, |button| {
                button
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_full()
                            .border_2()
                            .border_color(theme.ring),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom(px(2.))
                            .right(px(2.))
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(theme.border.opacity(0.7))
                            .bg(theme.background)
                            .text_color(theme.foreground)
                            .child(
                                Icon::new(if dark { Icons::Moon } else { Icons::Sun })
                                    .size(px(12.)),
                            ),
                    )
            })
            .into_any_element()
    }

    fn theme_card(
        &self,
        index: usize,
        id: ThemeId,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let (light, dark) = self.theme.picked(id);
        let active = light || dark;
        let hover = theme.accent.opacity(0.1);
        let ring = BoxShadow {
            color: theme.ring,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(1.),
            inset: true,
        };
        let card = v_flex()
            .id(("settings-theme-card", index))
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(12.))
            .border_1()
            .border_color(theme.border.opacity(0.7))
            .cursor_pointer()
            .tooltip(tooltip("Use for both light and dark"))
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| this.update_theme(|t| t.pick_both(id), cx))
            });
        let card = if active {
            card.bg(theme.accent.opacity(0.3)).shadow(vec![ring])
        } else {
            card.bg(theme.card.opacity(0.6))
                .hover(move |style| style.bg(hover))
        };
        card.child(
            h_flex()
                .min_h(px(64.))
                .items_center()
                .justify_center()
                .gap(px(10.))
                .px_3()
                .pt_3()
                .child(self.orb_button(id, false, light, cx))
                .child(self.orb_button(id, true, dark, cx)),
        )
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .px_3()
                .pb_3()
                .pt_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(id.label()),
                )
                .child(
                    div()
                        .id(("settings-theme-duplicate", index))
                        .size(px(24.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .text_color(theme.muted_foreground)
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover.opacity(3.)))
                        .tooltip(tooltip("Duplicate theme"))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(Icon::new(Icons::Copy).size(px(14.))),
                ),
        )
    }

    /// `repeat(auto-fit, minmax(min(100%, 16rem), 1fr))` with `gap-2`.
    fn theme_grid(&self, cx: &mut Context<Self>) -> gpui::Div {
        let inner = self.content_width.min(896.) - 48.;
        let columns = (((inner + 8.) / 264.).floor() as usize).max(1);
        let mut grid = v_flex().w_full().gap_2();
        for (chunk, ids) in ThemeId::ALL.chunks(columns).enumerate() {
            let mut line = h_flex().w_full().items_start().gap_2();
            for (offset, &id) in ids.iter().enumerate() {
                line = line.child(self.theme_card(chunk * columns + offset, id, cx));
            }
            for _ in ids.len()..columns {
                line = line.child(div().flex_1().min_w_0());
            }
            grid = grid.child(line);
        }
        grid
    }

    fn themes(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = Tokens::of(cx);
        let heading = |text: &'static str| {
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(text)
        };
        let inert = |id: &'static str, icon: Icons, label: &'static str, cx: &App| {
            outline_button(id, cx)
                .child(
                    Icon::new(icon)
                        .size(px(14.))
                        .text_color(theme.muted_foreground),
                )
                .child(label)
        };
        let create = inert(
            "settings-create-theme",
            Icons::Paintbrush,
            "Create theme",
            cx,
        );
        let add = inert("settings-add-theme", Icons::Plus, "Add theme", cx);
        v_flex()
            .w_full()
            .gap_3()
            .child(
                h_flex()
                    .min_h(px(28.))
                    .items_center()
                    .px_4()
                    .child(heading("Color scheme")),
            )
            .child(self.mode_tiles(cx))
            .child(
                h_flex()
                    .min_h(px(32.))
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_4()
                    .pt_5()
                    .child(heading("Themes"))
                    .child(
                        h_flex()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(create)
                            .child(add),
                    ),
            )
            .child(self.theme_grid(cx))
    }
}

impl Render for AppearancePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.settings.clone();
        let d = AppearanceSettings::default();
        let wide = self.wide;
        let view = cx.entity();
        let rows = |items: Vec<gpui::Div>| -> Vec<AnyElement> {
            items
                .into_iter()
                .map(IntoElement::into_any_element)
                .collect()
        };

        let contrast = self.slide(
            SlideSpec {
                id: "settings-appearance-contrast",
                range: CONTRAST,
                value: s.contrast,
                badge: format!("{}%", s.contrast),
                badge_width: 48.,
                write: |s, v| s.contrast = v,
            },
            window,
            cx,
        );
        let glass = self.slide(
            SlideSpec {
                id: "settings-appearance-glass",
                range: GLASS,
                value: s.glass_opacity,
                badge: format!("{}%", s.glass_opacity),
                badge_width: 48.,
                write: |s, v| s.glass_opacity = v,
            },
            window,
            cx,
        );
        let animation = self.slide(
            SlideSpec {
                id: "settings-panel-animation-duration",
                range: PANEL_MS,
                value: s.panel_animation_ms,
                badge: format!("{} ms", s.panel_animation_ms),
                badge_width: 64.,
                write: |s, v| s.panel_animation_ms = v,
            },
            window,
            cx,
        );
        let animation_control = if wide {
            h_flex()
                .items_center()
                .gap(px(16.))
                .child(div().w(px(112.)).child(self.preview(window, cx)))
                .child(div().w(px(208.)).child(animation))
        } else {
            h_flex()
                .w_full()
                .items_center()
                .gap_3()
                .child(div().w(px(80.)).child(self.preview(window, cx)))
                .child(div().flex_1().min_w_0().child(animation))
        };

        let advanced = vec![
            row(
                "Panel animations",
                "Set how fast panels open and close.",
                self.reset(
                    "settings-reset-panel-animations",
                    s.panel_animation_ms != d.panel_animation_ms,
                    |s, d| s.panel_animation_ms = d.panel_animation_ms,
                    window,
                    cx,
                ),
                animation_control.into_any_element(),
                wide,
                cx,
            ),
            row(
                "Environment identification",
                "Choose how Dev and Nightly environments are identified.",
                self.reset(
                    "settings-reset-environment-identification",
                    s.environment_identification != d.environment_identification,
                    |s, d| s.environment_identification = d.environment_identification,
                    window,
                    cx,
                ),
                self.select(Choice::Environment, cx),
                wide,
                cx,
            ),
            row(
                "Composer context",
                "Keep branch and worktree controls below the composer after a thread starts.",
                self.reset(
                    "settings-reset-composer-context",
                    s.composer_context != d.composer_context,
                    |s, d| s.composer_context = d.composer_context,
                    window,
                    cx,
                ),
                self.flag(
                    "settings-composer-context",
                    s.composer_context,
                    |s| s.composer_context = !s.composer_context,
                    window,
                    cx,
                ),
                wide,
                cx,
            ),
        ];
        let toggle_advanced = Rc::new(move |_: &mut Window, cx: &mut App| {
            view.update(cx, |this, cx| {
                this.advanced_open = !this.advanced_open;
                cx.notify();
            })
        });
        let advanced = disclosure(
            "settings-appearance-advanced",
            self.advanced_open,
            toggle_advanced,
            advanced
                .into_iter()
                .map(IntoElement::into_any_element)
                .collect(),
            window,
            cx,
        );

        let slider_width = |control: gpui::Div| control.w(px(208.)).into_any_element();
        let interface = group(
            {
                let mut items = rows(vec![
                    row(
                        "Contrast",
                        "Adjust the contrast of colors and borders across the interface.",
                        self.reset(
                            "settings-reset-contrast",
                            s.contrast != d.contrast,
                            |s, d| s.contrast = d.contrast,
                            window,
                            cx,
                        ),
                        if wide {
                            slider_width(contrast)
                        } else {
                            contrast.into_any_element()
                        },
                        wide,
                        cx,
                    ),
                    row(
                        "Glass opacity",
                        "Higher values make menus, dialogs, and the composer more solid.",
                        self.reset(
                            "settings-reset-glass-opacity",
                            s.glass_opacity != d.glass_opacity,
                            |s, d| s.glass_opacity = d.glass_opacity,
                            window,
                            cx,
                        ),
                        if wide {
                            slider_width(glass)
                        } else {
                            glass.into_any_element()
                        },
                        wide,
                        cx,
                    ),
                    row(
                        "Chat width",
                        "Set how wide messages and the composer can grow on large screens.",
                        self.reset(
                            "settings-reset-chat-width",
                            s.chat_width != d.chat_width,
                            |s, d| s.chat_width = d.chat_width,
                            window,
                            cx,
                        ),
                        self.select(Choice::Width, cx),
                        wide,
                        cx,
                    ),
                    row(
                        "Thread details",
                        "Choose what the thread details card shows. You can also right-click the card.",
                        self.reset(
                            "settings-reset-thread-details",
                            s.items_changed(),
                            |s, _| s.reset_items(),
                            window,
                            cx,
                        ),
                        self.details(cx),
                        wide,
                        cx,
                    ),
                    row(
                        "Effort display",
                        "Show reasoning effort in the composer as its name, such as High, or as bars.",
                        self.reset(
                            "settings-reset-effort-display",
                            s.effort_display != d.effort_display,
                            |s, d| s.effort_display = d.effort_display,
                            window,
                            cx,
                        ),
                        self.select(Choice::Effort, cx),
                        wide,
                        cx,
                    ),
                    row(
                        "Render math",
                        "Typeset LaTeX math, such as $x^2$ or \\[x^2\\], in messages and Markdown previews. The composer keeps TeX source editable.",
                        self.reset(
                            "settings-reset-math-rendering",
                            s.math_rendering != d.math_rendering,
                            |s, d| s.math_rendering = d.math_rendering,
                            window,
                            cx,
                        ),
                        self.flag(
                            "settings-math-rendering",
                            s.math_rendering,
                            |s| s.math_rendering = !s.math_rendering,
                            window,
                            cx,
                        ),
                        wide,
                        cx,
                    ),
                ]);
                items.push(advanced);
                items
            },
            cx,
        );
        v_flex()
            .w_full()
            .gap_8()
            .on_key_down(cx.listener(|this, event, window, cx| this.key_down(event, window, cx)))
            .child(self.themes(cx))
            .child(section("Interface", interface))
    }
}
