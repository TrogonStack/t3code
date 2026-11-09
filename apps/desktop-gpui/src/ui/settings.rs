//! The web's Settings route (`_chat.settings.tsx`) as an `AppRouteModal`, with its section
//! navigation (`SettingsSidebarNav.tsx`) and the General tab (`GeneralSettingsPanel`).

mod appearance_panel;
mod appearance_themes;
mod appearance_typography;
mod components;
mod devices_panel;
mod diffs_panel;
mod labs_panel;
mod scope_inheritance;
mod scope_pickers;
mod secret_field;
mod secrets_panel;
mod source_control_panel;
pub(crate) mod tokens;

use super::*;
use crate::assets::SNAP_SHOT;
use crate::settings::{
    self as model, DeviceSettings, FollowUp, GroupingMode, NAV_GROUPS, NotificationMode, QuitMode,
    SearchItem, Section, SendShortcut, SortOrder, TimeFormat,
};
use appearance_panel::AppearancePanel;
use appearance_typography::AppearanceTypography;
use components::{
    HoverHandler, KeyHandler, Menu, PickHandler, SelectTrigger, close_button, disclosure,
    ghost_button, group, reset_button, row, section, select_menu, select_menu_checked, switch,
};
use devices_panel::DevicesPanel;
use diffs_panel::DiffsPanel;
use gpui::{ClickEvent, FocusHandle, KeyBinding, KeyDownEvent, ScrollHandle, Subscription};
use labs_panel::{LabsEvent, LabsPanel};
use scope_pickers::ScopeBar;
use secrets_panel::SecretsPanel;
use source_control_panel::SourceControlPanel;
use std::rc::Rc;
use tokens::Tokens;

const CONTEXT: &str = "SettingsPage";
/// `SettingsSidebarNav`'s `w-60`.
const NAV_WIDTH: f32 = 240.;
/// `SettingsRow` switches to two columns from a `32rem` container.
const ROW_WIDE: f32 = 512.;
/// `WorkspacePageContainer` `max-w-4xl`.
const CONTENT_MAX: f32 = 896.;

actions!(settings, [CloseSettings, FocusSettingsSearch]);

/// Escape closes the modal; `cmd-f` is the nav's "focus search" shortcut.
pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", CloseSettings, Some(CONTEXT)),
        KeyBinding::new("cmd-f", FocusSettingsSearch, Some(CONTEXT)),
    ]
}

/// Whether the primary modifier prints as the Command symbol (`isMacPlatform`).
const MAC: bool = cfg!(target_os = "macos");

/// The selects of the General and Chat tabs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pick {
    Notifications,
    ProjectOrder,
    Quit,
    Time,
    SendShortcut,
    FollowUp,
}

impl Pick {
    fn id(self) -> &'static str {
        match self {
            Self::Notifications => "settings-thread-notifications",
            Self::ProjectOrder => "settings-project-order",
            Self::Quit => "settings-quit-shortcut",
            Self::Time => "settings-time-format",
            Self::SendShortcut => "settings-send-shortcut",
            Self::FollowUp => "settings-follow-up-behavior",
        }
    }

    /// `w-full sm:w-56`, `sm:w-44`, `sm:w-40`, `sm:w-40`.
    fn width(self) -> f32 {
        match self {
            Self::Notifications => 224.,
            Self::ProjectOrder => 176.,
            Self::Quit | Self::Time => 160.,
            Self::SendShortcut | Self::FollowUp => 0.,
        }
    }

    /// `w-auto min-w-0` (SettingsPanels.tsx Chat tab): sized by the label.
    fn auto_width(self) -> bool {
        matches!(self, Self::SendShortcut | Self::FollowUp)
    }

    fn labels(self) -> Vec<&'static str> {
        match self {
            Self::Notifications => NotificationMode::ALL.iter().map(|m| m.label()).collect(),
            Self::ProjectOrder => SortOrder::ALL.iter().map(|m| m.label()).collect(),
            Self::Quit => QuitMode::ALL.iter().map(|m| m.label()).collect(),
            Self::Time => TimeFormat::ALL.iter().map(|m| m.label()).collect(),
            Self::SendShortcut => SendShortcut::ALL
                .iter()
                .map(|m| m.menu_label(MAC))
                .collect(),
            Self::FollowUp => FollowUp::ALL.iter().map(|m| m.label()).collect(),
        }
    }

    fn selected(self, settings: &DeviceSettings) -> usize {
        let index = match self {
            Self::Notifications => NotificationMode::ALL
                .iter()
                .position(|m| *m == settings.notification_mode),
            Self::ProjectOrder => SortOrder::ALL
                .iter()
                .position(|m| *m == settings.project_order),
            Self::Quit => QuitMode::ALL
                .iter()
                .position(|m| *m == settings.confirm_quit),
            Self::Time => TimeFormat::ALL
                .iter()
                .position(|m| *m == settings.time_format),
            Self::SendShortcut => SendShortcut::ALL
                .iter()
                .position(|m| *m == settings.send_shortcut),
            Self::FollowUp => FollowUp::ALL.iter().position(|m| *m == settings.follow_up),
        };
        index.unwrap_or(0)
    }

    fn apply(self, settings: &mut DeviceSettings, index: usize) {
        match self {
            Self::Notifications => settings.notification_mode = NotificationMode::ALL[index],
            Self::ProjectOrder => settings.project_order = SortOrder::ALL[index],
            Self::Quit => settings.confirm_quit = QuitMode::ALL[index],
            Self::Time => settings.time_format = TimeFormat::ALL[index],
            Self::SendShortcut => settings.send_shortcut = SendShortcut::ALL[index],
            Self::FollowUp => settings.follow_up = FollowUp::ALL[index],
        }
    }

    const ALL: [Self; 6] = [
        Self::Notifications,
        Self::ProjectOrder,
        Self::Quit,
        Self::Time,
        Self::SendShortcut,
        Self::FollowUp,
    ];
}

pub struct SettingsDialog {
    focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    open: bool,
    settings: DeviceSettings,
    section: Section,
    last_grouping: GroupingMode,
    search: Entity<InputState>,
    active_result: usize,
    /// "/" moved focus to the search; its character must not land in the field.
    swallow_slash: bool,
    menu: Option<(Pick, usize)>,
    triggers: [FocusHandle; 6],
    /// `AdvancedSettings`' local `open` state: not persisted, closed again on leaving the tab.
    advanced_open: bool,
    diffs: Entity<DiffsPanel>,
    appearance: Entity<AppearancePanel>,
    typography: Entity<AppearanceTypography>,
    labs: Entity<LabsPanel>,
    devices: Entity<DevicesPanel>,
    source_control: Entity<SourceControlPanel>,
    secrets: Entity<SecretsPanel>,
    scope: Entity<ScopeBar>,
    confirm_restore: bool,
    scroll: ScrollHandle,
    _search_events: Subscription,
    _labs_events: Subscription,
}

impl SettingsDialog {
    pub fn new(
        actions: crate::connection::ActionSender,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scope = cx.new(|cx| ScopeBar::new(window, cx));
        let devices = cx.new(|cx| DevicesPanel::new(actions.clone(), scope.clone(), cx));
        let source_control =
            cx.new(|cx| SourceControlPanel::new(actions.clone(), scope.clone(), window, cx));
        let secrets = cx.new(|cx| SecretsPanel::new(actions.clone(), scope.clone(), window, cx));
        let labs = cx.new(|cx| LabsPanel::new(actions, scope.clone(), cx));
        let labs_events = cx.subscribe_in(&labs, window, |this, _, event: &LabsEvent, _, cx| {
            let LabsEvent::OpenSection(section) = event;
            this.select_section(*section, cx);
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let events = cx.subscribe_in(
            &search,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => this.search_changed(window, cx),
                InputEvent::PressEnter { .. } => this.activate_result(window, cx),
                _ => {}
            },
        );
        Self {
            focus: cx.focus_handle(),
            previous_focus: None,
            open: false,
            settings: DeviceSettings::load(),
            section: Section::General,
            last_grouping: GroupingMode::Repository,
            search,
            active_result: 0,
            swallow_slash: false,
            menu: None,
            triggers: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            advanced_open: false,
            diffs: cx.new(DiffsPanel::new),
            appearance: cx.new(AppearancePanel::new),
            typography: cx.new(|cx| AppearanceTypography::new(window, cx)),
            labs,
            devices,
            source_control,
            secrets,
            scope,
            confirm_restore: false,
            scroll: ScrollHandle::new(),
            _search_events: events,
            _labs_events: labs_events,
        }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        self.open = true;
        self.previous_focus = window.focused(cx);
        self.settings = DeviceSettings::load();
        self.diffs.update(cx, |panel, cx| panel.reload(cx));
        self.appearance.update(cx, |panel, cx| panel.reload(cx));
        self.typography.update(cx, |panel, cx| panel.reload(cx));
        self.labs.update(cx, |panel, cx| panel.reload(cx));
        self.devices.update(cx, |panel, cx| panel.reload(cx));
        self.source_control.update(cx, |panel, cx| panel.reload(cx));
        self.secrets.update(cx, |panel, cx| panel.reload(cx));
        self.menu = None;
        self.advanced_open = false;
        self.confirm_restore = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.clear_search(window, cx);
        if let Some(previous) = self.previous_focus.take() {
            window.focus(&previous, cx);
        }
        cx.notify();
    }

    /// The scope bar's data and the attached environment's Labs flags.
    pub fn set_labs_data(
        &mut self,
        data: crate::settings_labs::ScopeData,
        flags: Option<crate::settings_labs::ServerFlags>,
        cx: &mut Context<Self>,
    ) {
        self.scope.update(cx, |bar, cx| bar.set_data(data, cx));
        if let Some(flags) = flags {
            self.labs.update(cx, |panel, cx| panel.set_flags(flags, cx));
        }
    }

    pub fn on_labs_call(
        &mut self,
        token: u64,
        event: crate::status::CallEvent,
        cx: &mut Context<Self>,
    ) {
        self.labs
            .update(cx, |panel, cx| panel.on_call(token, event, cx));
    }

    pub fn set_devices_data(
        &mut self,
        config: crate::settings_devices::DeviceConfig,
        projects: crate::settings_devices::ProjectIds,
        secret_sources: Vec<crate::secret_source::SecretSource>,
        cx: &mut Context<Self>,
    ) {
        self.devices.update(cx, |panel, cx| {
            panel.set_data(config, projects, secret_sources, cx)
        });
    }

    pub fn set_source_control_data(
        &mut self,
        config: crate::settings_source_control::SourceControlConfig,
        projects: crate::settings_devices::ProjectIds,
        cx: &mut Context<Self>,
    ) {
        self.source_control
            .update(cx, |panel, cx| panel.set_data(config, projects, cx));
    }

    pub fn on_source_control_call(
        &mut self,
        token: u64,
        event: crate::status::CallEvent,
        cx: &mut Context<Self>,
    ) {
        self.source_control
            .update(cx, |panel, cx| panel.on_call(token, event, cx));
    }

    pub fn set_secrets_data(
        &mut self,
        config: crate::settings_secrets::Config,
        cx: &mut Context<Self>,
    ) {
        self.secrets
            .update(cx, |panel, cx| panel.set_data(config, cx));
    }

    pub fn on_secrets_call(
        &mut self,
        token: u64,
        event: crate::status::CallEvent,
        cx: &mut Context<Self>,
    ) {
        self.secrets
            .update(cx, |panel, cx| panel.on_call(token, event, cx));
    }

    pub fn on_devices_call(
        &mut self,
        token: u64,
        event: crate::status::CallEvent,
        cx: &mut Context<Self>,
    ) {
        self.devices
            .update(cx, |panel, cx| panel.on_call(token, event, cx));
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().to_string()
    }

    fn results(&self, cx: &App) -> Vec<&'static SearchItem> {
        let desktop = true;
        let query = self.query(cx);
        model::search(&query, model::searchable())
            .into_iter()
            .filter(|item| desktop || !item.desktop_only)
            .collect()
    }

    fn search_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.swallow_slash) && self.query(cx) == "/" {
            self.search
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.active_result = 0;
        cx.notify();
    }

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.active_result = 0;
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.search.focus_handle(cx);
        window.focus(&handle, cx);
    }

    fn activate_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let results = self.results(cx);
        if let Some(item) = results.get(self.active_result) {
            if item.section != self.section {
                self.advanced_open = false;
            }
            self.advanced_open |= item.id == "skills-in-slash-menu";
            if crate::settings_appearance::is_advanced(item.id) {
                self.appearance
                    .update(cx, |panel, cx| panel.open_advanced(cx));
            }
            if matches!(
                item.id,
                "separate-fonts" | "font-smoothing" | "prompt-font" | "terminal-font"
            ) {
                self.typography
                    .update(cx, |panel, cx| panel.open_advanced(item.id, cx));
            }
            self.section = item.section;
            self.clear_search(window, cx);
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    fn select_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if section != self.section {
            self.advanced_open = false;
        }
        self.section = section;
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.scope.update(cx, |bar, cx| bar.close_menu(cx)) {
            return;
        }
        if self
            .secrets
            .update(cx, |panel, cx| panel.close_menu(window, cx))
        {
            return;
        }
        if self
            .labs
            .update(cx, |panel, cx| panel.close_menu(window, cx))
        {
            return;
        }
        if self
            .devices
            .update(cx, |panel, cx| panel.close_menu(window, cx))
        {
            return;
        }
        if self
            .source_control
            .update(cx, |panel, cx| panel.close_menu(window, cx))
        {
            return;
        }
        if self
            .diffs
            .update(cx, |panel, cx| panel.close_menu(window, cx))
        {
            return;
        }
        if self
            .appearance
            .update(cx, |panel, cx| panel.close_menu(window, cx))
        {
            return;
        }
        if self
            .typography
            .update(cx, |panel, cx| panel.close_menu(window, cx))
        {
            return;
        }
        if self.confirm_restore {
            self.confirm_restore = false;
        } else if self.menu.is_some() {
            self.close_menu(window, cx);
            return;
        } else if self.search.focus_handle(cx).is_focused(window) && !self.query(cx).is_empty() {
            self.clear_search(window, cx);
        } else {
            self.close(window, cx);
            return;
        }
        cx.notify();
    }

    fn open_menu(&mut self, pick: Pick, cx: &mut Context<Self>) {
        self.menu = Some((pick, pick.selected(&self.settings)));
        cx.notify();
    }

    fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((pick, _)) = self.menu.take() {
            let index = Pick::ALL.iter().position(|p| *p == pick).unwrap_or(0);
            window.focus(&self.triggers[index], cx);
            cx.notify();
        }
    }

    fn pick(&mut self, pick: Pick, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.update(|settings| pick.apply(settings, index), cx);
        self.close_menu(window, cx);
    }

    fn update(&mut self, change: impl FnOnce(&mut DeviceSettings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.settings.save();
        cx.notify();
    }

    fn restore_defaults(&mut self, cx: &mut Context<Self>) {
        self.confirm_restore = false;
        self.last_grouping = GroupingMode::Repository;
        self.diffs
            .update(cx, |panel, cx| panel.restore_defaults(cx));
        self.appearance
            .update(cx, |panel, cx| panel.restore_defaults(cx));
        self.typography
            .update(cx, |panel, cx| panel.restore_defaults(cx));
        self.update(|settings| *settings = DeviceSettings::default(), cx);
    }

    fn toggle_grouping(&mut self, cx: &mut Context<Self>) {
        let current = self.settings.project_grouping;
        if current != GroupingMode::Separate {
            self.last_grouping = current;
        }
        let next = model::toggle_grouping(current, self.last_grouping);
        self.update(|settings| settings.project_grouping = next, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if let Some((pick, highlighted)) = self.menu {
            let count = pick.labels().len();
            match key {
                "down" => self.menu = Some((pick, (highlighted + 1) % count)),
                "up" => self.menu = Some((pick, (highlighted + count - 1) % count)),
                "home" => self.menu = Some((pick, 0)),
                "end" => self.menu = Some((pick, count - 1)),
                "enter" | "space" => self.pick(pick, highlighted, window, cx),
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let searching = !self.query(cx).is_empty();
        let search_focused = self.search.focus_handle(cx).is_focused(window);
        if key == "/" && !event.keystroke.modifiers.modified() && !search_focused {
            self.swallow_slash = true;
            self.focus_search(window, cx);
            cx.stop_propagation();
            return;
        }
        if !searching {
            return;
        }
        let count = self.results(cx).len();
        if count == 0 {
            return;
        }
        match key {
            "down" => self.active_result = (self.active_result + 1) % count,
            "up" => self.active_result = (self.active_result + count - 1) % count,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for SettingsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return gpui::Empty.into_any_element();
        }
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
        let tokens = Tokens::of(cx);
        let (backdrop, border, background) = (tokens.backdrop(), tokens.border, tokens.background);
        let nav = self.nav(cx);
        let header = self.header(cx);
        let body = self.body(width - NAV_WIDTH, window, cx);
        let confirm = self.restore_dialog(cx);
        let devices = self
            .devices
            .update(cx, |panel, cx| panel.overlay(window, cx));
        let secrets = self
            .secrets
            .update(cx, |panel, cx| panel.overlay(window, cx));
        div()
            .absolute()
            .inset_0()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CloseSettings, window, cx| this.escape(window, cx)))
            .on_action(cx.listener(|this, _: &FocusSettingsSearch, window, cx| {
                this.focus_search(window, cx)
            }))
            .on_key_down(cx.listener(|this, event, window, cx| this.key_down(event, window, cx)))
            .child(
                div()
                    .id("settings-backdrop")
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
                    .text_color(tokens.foreground)
                    .when(framed, |popup| {
                        popup
                            .rounded(px(16.))
                            .border_1()
                            .border_color(border)
                            .shadow(vec![gpui::BoxShadow {
                                color: gpui::black().opacity(0.4),
                                offset: gpui::point(px(0.), px(25.)),
                                blur_radius: px(50.),
                                spread_radius: px(-12.),
                                inset: false,
                            }])
                    })
                    .child(
                        h_flex().relative().flex_1().min_h_0().child(nav).child(
                            v_flex().flex_1().min_w_0().h_full().child(header).child(
                                div()
                                    .id("settings-scroll")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll)
                                    .child(body),
                            ),
                        ),
                    ),
            )
            .children(confirm)
            .children(devices)
            .children(secrets)
            .into_any_element()
    }
}

impl SettingsDialog {
    /// `SettingsContentLayout`'s header: `h-12 gap-3 border-b ps-5 pe-2`.
    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let (muted, border, foreground) = (theme.muted_foreground, theme.border, theme.foreground);
        let general = self.section == Section::General;
        let disabled = self
            .diffs
            .read(cx)
            .settings()
            .merge_labels(self.settings.changed_labels())
            .is_empty();
        let crumbs = h_flex()
            .min_w_0()
            .items_center()
            .gap_3()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .child(div().flex_shrink_0().text_color(muted).child("Settings"))
            .child(div().flex_shrink_0().text_color(muted).child("/"))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(foreground)
                    .child(self.section.label()),
            );
        // `size="xs" variant="ghost"` (_chat.settings.tsx:33): `sm:h-6 gap-1 px-[7px] sm:text-xs`,
        // `disabled:opacity-64 disabled:pointer-events-none`, icon `size-3.5` at `-mx-0.5`.
        let restore = ghost_button("settings-restore", !disabled, cx)
            .h(px(24.))
            .gap_1()
            .px(px(7.))
            .text_xs()
            .when(!disabled, |button| {
                button.on_click(cx.listener(|this, _, _, cx| {
                    this.confirm_restore = true;
                    cx.notify();
                }))
            })
            .child(
                div()
                    .mx(px(-2.))
                    .text_color(muted)
                    .child(Icon::new(Icons::RotateCcw).size(px(14.))),
            )
            .child("Restore device defaults");
        let close = close_button("settings-close", cx)
            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx)));
        h_flex()
            .h(px(48.))
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .pl_5()
            .pr_2()
            .border_b_1()
            .border_color(border)
            .child(crumbs)
            .child(
                h_flex()
                    .ml_auto()
                    .gap_1()
                    .when(general, |actions| actions.child(restore))
                    .child(close),
            )
            .into_any_element()
    }

    /// `AlertDialog` of `requestConfirmDialog` ("Restore default settings?", destructive).
    fn restore_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.confirm_restore {
            return None;
        }
        let theme = Tokens::of(cx);
        let (background, muted, backdrop) =
            (theme.background, theme.muted_foreground, theme.backdrop());
        // `dialog-glass` border (index.css:425-437).
        let border = if theme.dark {
            gpui::white().opacity(0.08)
        } else {
            theme.foreground.opacity(0.1)
        };
        let (outline_bg, outline_hover) = if theme.dark {
            (theme.input.opacity(0.32), theme.input.opacity(0.64))
        } else {
            (theme.popover, theme.accent.opacity(0.5))
        };
        let labels = self
            .diffs
            .read(cx)
            .settings()
            .merge_labels(self.settings.changed_labels())
            .join(", ");
        let cancel = div()
            .id("settings-restore-cancel")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(outline_bg)
            .text_color(theme.foreground)
            .text_size(px(14.))
            .line_height(px(20.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(move |style| style.bg(outline_hover))
            .on_click(cx.listener(|this, _, _, cx| {
                this.confirm_restore = false;
                cx.notify();
            }))
            .child("Cancel");
        let confirm = div()
            .id("settings-restore-confirm")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.destructive)
            .bg(theme.destructive)
            .text_color(gpui::white())
            .text_size(px(14.))
            .line_height(px(20.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.restore_defaults(cx)))
            .child("Confirm");
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("settings-restore-backdrop")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .bg(backdrop)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_restore = false;
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
                                .shadow(vec![gpui::BoxShadow {
                                    color: gpui::black().opacity(if theme.dark {
                                        0.9
                                    } else {
                                        0.65
                                    }),
                                    offset: gpui::point(px(0.), px(24.)),
                                    blur_radius: px(if theme.dark { 72. } else { 64. }),
                                    spread_radius: px(if theme.dark { -20. } else { -24. }),
                                    inset: false,
                                }])
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_xl()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child("Restore default settings?"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(14.))
                                                .line_height(px(20.))
                                                .text_color(muted)
                                                .child(format!("This will reset: {labels}.")),
                                        ),
                                )
                                .child(h_flex().justify_end().gap_2().child(cancel).child(confirm)),
                        ),
                )
                .into_any_element(),
        )
    }

    /// `SettingsSidebarNav`: `w-60 border-e bg-sidebar`, content inset `p-2`, rows `gap-2`.
    fn nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let (muted, sidebar_foreground, hover, border) = (
            theme.muted_foreground,
            theme.foreground,
            theme.sidebar_row_hover,
            theme.border,
        );
        let (sidebar, foreground) = (theme.sidebar, theme.foreground);
        let query = self.query(cx);
        let searching = !query.is_empty();
        let kbd = div()
            .h(px(20.))
            .min_w(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .px_1()
            .bg(foreground.opacity(0.07))
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(muted)
            .child("/");
        let clear = div()
            .id("settings-search-clear")
            .size(px(20.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .text_color(muted)
            .cursor_pointer()
            .hover(move |style| style.bg(hover).text_color(foreground))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.clear_search(window, cx);
                this.focus_search(window, cx);
                cx.notify();
            }))
            .child(Icon::new(IconName::Close).size(px(12.)));
        // `h-8 rounded-md px-2 gap-2 text-sm font-medium`; only hover paints the row.
        let search_row = h_flex()
            .h_8()
            .items_center()
            .gap_2()
            .px_2()
            .rounded(px(6.))
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(muted)
            .hover(move |style| style.bg(hover).text_color(foreground))
            .child(
                div()
                    .flex_shrink_0()
                    .opacity(0.8)
                    .child(Icon::new(IconName::Search).size_4()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(sidebar_foreground)
                    .child(Input::new(&self.search).appearance(false).small()),
            )
            .child(if searching {
                clear.into_any_element()
            } else {
                kbd.into_any_element()
            });
        let body = if searching {
            self.search_results(cx)
        } else {
            self.groups(cx)
        };
        v_flex()
            .w(px(NAV_WIDTH))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(border)
            .bg(sidebar)
            .text_color(sidebar_foreground)
            .child(
                div()
                    .id("settings-nav-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_2()
                    .child(v_flex().gap_2().child(search_row).child(body)),
            )
            .into_any_element()
    }

    fn section_icon(section: Section, active: bool, cx: &App) -> AnyElement {
        let icon = match section {
            Section::General => Icon::new(Icons::Settings2),
            Section::Appearance => Icon::new(Icons::Palette),
            Section::Chat => Icon::new(Icons::MessageSquareText),
            Section::Diffs => Icon::new(Icons::FileDiff),
            Section::Keybindings => Icon::new(Icons::Keyboard),
            Section::SnapShot => Icon::empty().path(SNAP_SHOT),
            Section::Providers => Icon::new(Icons::Bot),
            Section::Threads => Icon::new(Icons::MessagesSquare),
            Section::Browser => Icon::new(Icons::Globe),
            Section::Devices => Icon::new(Icons::Smartphone),
            Section::ScheduledTasks => Icon::new(Icons::CalendarClock),
            Section::Projects => Icon::new(Icons::PanelsTopLeft),
            Section::SourceControl => Icon::new(Icons::GitBranch),
            Section::Worktrees => Icon::new(Icons::FolderTree),
            Section::Connections => Icon::new(Icons::Server),
            Section::Access => Icon::new(Icons::ShieldCheck),
            Section::Secrets => Icon::new(Icons::KeyRound),
            Section::Maintenance => Icon::new(Icons::Wrench),
            Section::Labs => Icon::new(Icons::FlaskConical),
            Section::About => Icon::new(Icons::Info),
            Section::Archived => Icon::new(Icons::Archive),
        };
        div()
            .flex_shrink_0()
            .text_color(if active {
                Tokens::of(cx).foreground
            } else {
                Tokens::of(cx).sidebar_icon
            })
            .child(icon.size_4())
            .into_any_element()
    }

    /// `SidebarMenuButton`: `h-8 rounded-lg px-2.5 gap-2 text-sm`, muted until hovered or active.
    fn groups(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let (muted, foreground, hover, selected) = (
            theme.muted_foreground,
            theme.foreground,
            theme.sidebar_row_hover,
            theme.sidebar_row_selected,
        );
        let mut groups = v_flex().gap_4();
        for (index, nav_group) in NAV_GROUPS.iter().enumerate() {
            let mut rows = v_flex().gap_1();
            for section in nav_group.sections {
                let target = *section;
                let active = target == self.section;
                let icon = Self::section_icon(target, active, cx);
                rows = rows.child(
                    h_flex()
                        .id(("settings-section", index * 10 + target as usize))
                        .h_8()
                        .items_center()
                        .gap_2()
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .text_color(if active {
                            foreground
                        } else {
                            muted.opacity(0.8)
                        })
                        .when(active, |row| row.bg(selected))
                        .when(!active, |row| {
                            row.hover(move |style| style.bg(hover).text_color(foreground))
                        })
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_section(target, cx)),
                        )
                        .child(icon)
                        .child(div().min_w_0().truncate().child(target.label())),
                );
            }
            groups = groups.child(
                v_flex()
                    .gap_1()
                    .children(nav_group.heading.map(|heading| {
                        div()
                            .px_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(muted.opacity(0.8))
                            .child(heading)
                    }))
                    .child(rows),
            );
        }
        groups.into_any_element()
    }

    /// Results rows: `min-h-10 items-start`, title `text-sm font-medium`, section `text-2xs`.
    fn search_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let results = self.results(cx);
        if results.is_empty() {
            return div()
                .px_2()
                .py_6()
                .text_center()
                .text_xs()
                .text_color(Tokens::of(cx).muted_foreground)
                .child("No settings found")
                .into_any_element();
        }
        let theme = Tokens::of(cx);
        let (muted, foreground, selected) = (
            theme.muted_foreground,
            theme.foreground,
            theme.sidebar_row_selected,
        );
        v_flex()
            .gap_1()
            .children(results.into_iter().enumerate().map(|(index, item)| {
                h_flex()
                    .id(("settings-result", index))
                    .min_h(px(40.))
                    .items_start()
                    .gap_2()
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(index == self.active_result, |row| row.bg(selected))
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this.active_result != index {
                            this.active_result = index;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.active_result = index;
                        this.activate_result(window, cx);
                    }))
                    .child(Self::section_icon(
                        item.section,
                        index == self.active_result,
                        cx,
                    ))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(foreground)
                                    .child(item.title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(muted.opacity(0.75))
                                    .child(item.section.label()),
                            ),
                    )
            }))
            .into_any_element()
    }

    /// `SettingsPageContainer`: `max-w-4xl px-6 pt-6 pb-12 gap-8`.
    fn body(&self, content_width: f32, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let wide = content_width >= ROW_WIDE + 48.;
        let panel = if self.section == Section::General {
            self.general(wide, window, cx)
        } else if self.section == Section::Chat {
            self.chat(wide, window, cx)
        } else if self.section == Section::Diffs {
            self.diffs.update(cx, |panel, cx| panel.set_wide(wide, cx));
            vec![self.diffs.clone().into_any_element()]
        } else if self.section == Section::Appearance {
            self.appearance
                .update(cx, |panel, cx| panel.set_layout(wide, content_width, cx));
            self.typography
                .update(cx, |panel, cx| panel.set_wide(wide, cx));
            vec![
                self.appearance.clone().into_any_element(),
                self.typography.clone().into_any_element(),
            ]
        } else if self.section == Section::Labs {
            self.labs.update(cx, |panel, cx| panel.set_wide(wide, cx));
            vec![self.labs.clone().into_any_element()]
        } else if self.section == Section::Devices {
            self.devices
                .update(cx, |panel, cx| panel.set_wide(wide, cx));
            vec![self.devices.clone().into_any_element()]
        } else if self.section == Section::SourceControl {
            self.source_control
                .update(cx, |panel, cx| panel.set_wide(wide, cx));
            vec![self.source_control.clone().into_any_element()]
        } else if self.section == Section::Secrets {
            self.secrets
                .update(cx, |panel, cx| panel.set_wide(wide, cx));
            vec![self.secrets.clone().into_any_element()]
        } else {
            Vec::new()
        };
        // The web hides the scope sentence on device-only tabs; the tabs that apply a scope list here.
        let scoped = matches!(
            self.section,
            Section::Labs | Section::Devices | Section::SourceControl | Section::Secrets
        );
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(CONTENT_MAX))
                    .gap_8()
                    .px_6()
                    .pt_6()
                    .pb_12()
                    .children(scoped.then(|| self.scope.clone().into_any_element()))
                    .children(panel),
            )
            .into_any_element()
    }

    fn reset(
        &self,
        id: &'static str,
        changed: bool,
        restore: fn(&mut DeviceSettings, &DeviceSettings),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = cx.entity();
        changed.then(|| {
            reset_button(
                id,
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.update(|s| restore(s, &DeviceSettings::default()), cx)
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
        toggle: fn(&mut DeviceSettings),
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

    /// `Select` (select.tsx) over a `Popover`; Enter, Space and the arrows open it.
    fn select(&self, pick: Pick, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let labels = pick.labels();
        let selected = pick.selected(&self.settings);
        let open = matches!(self.menu, Some((open, _)) if open == pick);
        let highlighted = match self.menu {
            Some((open, index)) if open == pick => index,
            _ => usize::MAX,
        };
        let slot = Pick::ALL.iter().position(|p| *p == pick).unwrap_or(0);
        let on_key: KeyHandler = {
            let view = view.clone();
            Rc::new(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.open_menu(pick, cx));
                }
            })
        };
        let trigger = SelectTrigger::new(
            pick.id(),
            labels[selected],
            pick.width(),
            self.triggers[slot].clone(),
            on_key,
        );
        let trigger = if pick.auto_width() {
            trigger.auto_width()
        } else {
            trigger
        };
        let on_pick: PickHandler = {
            let view = view.clone();
            Rc::new(move |index, window, cx| {
                view.update(cx, |this, cx| this.pick(pick, index, window, cx))
            })
        };
        let on_hover: HoverHandler = {
            let view = view.clone();
            Rc::new(move |index, cx| {
                view.update(cx, |this, cx| {
                    if this.menu.is_some_and(|(_, current)| current != index) {
                        this.menu = Some((pick, index));
                        cx.notify();
                    }
                })
            })
        };
        let check = pick == Pick::SendShortcut;
        Popover::new(("settings-select", slot))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change(move |opened, window, cx| {
                view.update(cx, |this, cx| {
                    if *opened {
                        this.open_menu(pick, cx);
                    } else if this.menu.is_some_and(|(current, _)| current == pick) {
                        this.close_menu(window, cx);
                    }
                })
            })
            .trigger(trigger)
            .content(move |_, _, cx| {
                let menu = Menu {
                    id: pick.id(),
                    labels: labels.clone(),
                    selected,
                    highlighted,
                    width: pick.width(),
                    on_pick: on_pick.clone(),
                    on_hover: on_hover.clone(),
                };
                if check {
                    select_menu_checked(&menu, cx)
                } else {
                    select_menu(&menu, cx)
                }
            })
            .into_any_element()
    }

    /// `ChatSettingsPanel` (SettingsPanels.tsx:1075-1287): Composer and Follow-ups.
    fn chat(&self, wide: bool, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let s = self.settings;
        let d = DeviceSettings::default();
        let view = cx.entity();
        let skills = row(
            "Show skills in slash menu",
            "Also include skills in the / command menu. Skills always appear when you type $.",
            self.reset(
                "settings-reset-skills-in-slash-menu",
                s.skills_in_slash_menu != d.skills_in_slash_menu,
                |s, d| s.skills_in_slash_menu = d.skills_in_slash_menu,
                window,
                cx,
            ),
            self.flag(
                "settings-skills-in-slash-menu",
                s.skills_in_slash_menu,
                |s| s.skills_in_slash_menu = !s.skills_in_slash_menu,
                window,
                cx,
            ),
            wide,
            cx,
        );
        let advanced = disclosure(
            "settings-advanced",
            self.advanced_open,
            Rc::new(move |_, cx| {
                view.update(cx, |this, cx| {
                    this.advanced_open = !this.advanced_open;
                    cx.notify();
                })
            }),
            vec![skills.into_any_element()],
            window,
            cx,
        );
        let composer = group(
            vec![
                row(
                    "Rich text composer",
                    "Show formatted Markdown as you type.",
                    self.reset(
                        "settings-reset-rich-text-composer",
                        s.composer_rich_text != d.composer_rich_text,
                        |s, d| s.composer_rich_text = d.composer_rich_text,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-rich-text-composer",
                        s.composer_rich_text,
                        |s| s.composer_rich_text = !s.composer_rich_text,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                )
                .into_any_element(),
                row(
                    "Send shortcut",
                    "Choose when Enter sends a prompt or inserts a new line",
                    self.reset(
                        "settings-reset-send-shortcut",
                        s.send_shortcut != d.send_shortcut,
                        |s, d| s.send_shortcut = d.send_shortcut,
                        window,
                        cx,
                    ),
                    self.select(Pick::SendShortcut, cx),
                    wide,
                    cx,
                )
                .into_any_element(),
                advanced,
            ],
            cx,
        );
        let follow_ups = group(
            vec![
                row(
                    "Follow-up behavior",
                    model::follow_up_description(s.send_shortcut, MAC),
                    self.reset(
                        "settings-reset-follow-up-behavior",
                        s.follow_up != d.follow_up,
                        |s, d| s.follow_up = d.follow_up,
                        window,
                        cx,
                    ),
                    self.select(Pick::FollowUp, cx),
                    wide,
                    cx,
                )
                .into_any_element(),
            ],
            cx,
        );
        vec![
            section("Composer", composer).into_any_element(),
            section("Follow-ups", follow_ups).into_any_element(),
        ]
    }

    /// `GeneralSettingsPanel` (SettingsPanels.tsx:781-1067): Notifications, Sidebar,
    /// Confirmations, Display.
    fn general(&self, wide: bool, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let s = self.settings;
        let d = DeviceSettings::default();
        let view = cx.entity();
        let grouping = {
            let view = view.clone();
            switch(
                "settings-project-grouping",
                s.project_grouping != GroupingMode::Separate,
                Rc::new(move |_, cx| view.update(cx, |this, cx| this.toggle_grouping(cx))),
                window,
                cx,
            )
        };
        let rows = |items: Vec<gpui::Div>| {
            items
                .into_iter()
                .map(IntoElement::into_any_element)
                .collect()
        };

        let notifications = group(
            rows(vec![
                row(
                    "Thread notifications",
                    "System alerts when a thread finishes, fails, or needs input or approval. Applies to this device while T3 Code is open.",
                    self.reset(
                        "settings-reset-thread-notifications",
                        s.notification_mode != d.notification_mode,
                        |s, d| s.notification_mode = d.notification_mode,
                        window,
                        cx,
                    ),
                    self.select(Pick::Notifications, cx),
                    wide,
                    cx,
                ),
                row(
                    "In-app notifications",
                    "Show a toast when another thread finishes, fails, or needs input or approval while this app has focus.",
                    self.reset(
                        "settings-reset-in-app-notifications",
                        s.in_app_notifications != d.in_app_notifications,
                        |s, d| s.in_app_notifications = d.in_app_notifications,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-in-app-notifications",
                        s.in_app_notifications,
                        |s| s.in_app_notifications = !s.in_app_notifications,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
            ]),
            cx,
        );
        let sidebar = group(
            rows(vec![
                row(
                    "Project grouping",
                    "Combine matching repositories across environments.",
                    self.reset(
                        "settings-reset-project-grouping",
                        s.project_grouping != d.project_grouping,
                        |s, d| s.project_grouping = d.project_grouping,
                        window,
                        cx,
                    ),
                    grouping,
                    wide,
                    cx,
                ),
                row(
                    "Project order",
                    "Order of projects in the sidebar project picker and command palette.",
                    self.reset(
                        "settings-reset-project-order",
                        s.project_order != d.project_order,
                        |s, d| s.project_order = d.project_order,
                        window,
                        cx,
                    ),
                    self.select(Pick::ProjectOrder, cx),
                    wide,
                    cx,
                ),
                row(
                    "Working section",
                    "Fold working and monitoring threads into a Working section. They return to the top of the inbox when they need you. The boring sidebar in Labs has no Working section.",
                    self.reset(
                        "settings-reset-working-section",
                        s.working_shelf != d.working_shelf,
                        |s, d| s.working_shelf = d.working_shelf,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-working-section",
                        s.working_shelf,
                        |s| s.working_shelf = !s.working_shelf,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
            ]),
            cx,
        );
        let confirmations = group(
            rows(vec![
                row(
                    "Unpin confirmation",
                    "Ask before unpinning a thread from the pinned section.",
                    self.reset(
                        "settings-reset-unpin-confirmation",
                        s.confirm_unpin != d.confirm_unpin,
                        |s, d| s.confirm_unpin = d.confirm_unpin,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-unpin-confirmation",
                        s.confirm_unpin,
                        |s| s.confirm_unpin = !s.confirm_unpin,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Archive confirmation",
                    "Require a second click on the inline archive action before a thread is archived.",
                    self.reset(
                        "settings-reset-archive-confirmation",
                        s.confirm_archive != d.confirm_archive,
                        |s, d| s.confirm_archive = d.confirm_archive,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-archive-confirmation",
                        s.confirm_archive,
                        |s| s.confirm_archive = !s.confirm_archive,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Delete confirmation",
                    "Ask before deleting a thread and its chat history.",
                    self.reset(
                        "settings-reset-delete-confirmation",
                        s.confirm_delete != d.confirm_delete,
                        |s, d| s.confirm_delete = d.confirm_delete,
                        window,
                        cx,
                    ),
                    self.flag(
                        "settings-delete-confirmation",
                        s.confirm_delete,
                        |s| s.confirm_delete = !s.confirm_delete,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Quit shortcut",
                    "Hold mode also quits on two quick presses.",
                    self.reset(
                        "settings-reset-quit-shortcut",
                        s.confirm_quit != d.confirm_quit,
                        |s, d| s.confirm_quit = d.confirm_quit,
                        window,
                        cx,
                    ),
                    self.select(Pick::Quit, cx),
                    wide,
                    cx,
                ),
            ]),
            cx,
        );
        let display = group(
            rows(vec![row(
                "Time format",
                "System default follows your browser or OS clock preference.",
                self.reset(
                    "settings-reset-time-format",
                    s.time_format != d.time_format,
                    |s, d| s.time_format = d.time_format,
                    window,
                    cx,
                ),
                self.select(Pick::Time, cx),
                wide,
                cx,
            )]),
            cx,
        );
        vec![
            section("Notifications", notifications).into_any_element(),
            section("Sidebar", sidebar).into_any_element(),
            section("Confirmations", confirmations).into_any_element(),
            section("Display", display).into_any_element(),
        ]
    }
}
