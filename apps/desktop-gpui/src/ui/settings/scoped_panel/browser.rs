//! The Browser tab (`BrowserSettingsPanel`, IntegrationsSettings.tsx). Agent browser access is a
//! server setting that resolves per scope; every other key is a device setting persisted in
//! `client-settings.json` (`settings_browser::BrowserSettings`), which a project scope cannot
//! override, so a write there is refused with the web's "Setting not saved" message. The web
//! shows that and the clear results as toasts; the toast host is not reachable from a panel, so
//! they appear in a notice at the top of the tab, as on the Devices tab.

use super::super::components::{disclosure, row, row_with_preview, row_with_status};
use super::super::devices_panel::widgets::{
    Variant, button, dialog_of, icon_button, menu_item, note, problem,
};
use super::super::scope_inheritance::{PopTrigger, popup_shell};
use super::*;
use crate::settings_browser::{self as web, BrowserSettings, ImportSource, Profile};
use gpui::ElementId;

const RESET: &str = "Reset to default";
const REFUSED: &str = "This setting is environment-wide and cannot be overridden by a project.";
const NO_ENVIRONMENT: &str = "Connect to an environment to clear profile data";

/// A rename field of a profile the user made.
struct Name {
    id: String,
    input: Entity<InputState>,
    _subscription: Subscription,
}

/// A clear or remove the user is asked to confirm, or that is on its way to the server.
#[derive(Clone)]
struct Confirm {
    id: String,
    name: String,
    remove: bool,
    error: Option<String>,
}

struct Job {
    token: u64,
    confirm: Confirm,
}

struct Notice {
    title: String,
    description: Option<String>,
    error: bool,
}

pub(super) struct State {
    pub settings: BrowserSettings,
    server_browser: bool,
    names: Vec<Name>,
    add_menu: bool,
    row_menu: Option<String>,
    confirm: Option<Confirm>,
    job: Option<Job>,
    notice: Option<Notice>,
    sources: Vec<&'static ImportSource>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            settings: BrowserSettings::load(),
            server_browser: false,
            names: Vec::new(),
            add_menu: false,
            row_menu: None,
            confirm: None,
            job: None,
            notice: None,
            sources: web::installed_sources(),
        }
    }
}

impl ScopedPanel {
    /// The attached environment can run browser tabs: it can clear a profile's data.
    pub fn set_browser_data(&mut self, server_browser: bool, cx: &mut Context<Self>) {
        if self.browser.server_browser != server_browser {
            self.browser.server_browser = server_browser;
            cx.notify();
        }
    }

    pub(super) fn reload_browser(&mut self) {
        let browser = &mut self.browser;
        browser.settings = BrowserSettings::load();
        browser.sources = web::installed_sources();
        browser.add_menu = false;
        browser.row_menu = None;
        browser.notice = None;
        if browser.job.is_none() {
            browser.confirm = None;
        }
    }

    /// Closes an open menu or an idle confirmation; returns whether there was one.
    pub(super) fn close_browser_menus(&mut self) -> bool {
        let browser = &mut self.browser;
        if browser.add_menu || browser.row_menu.is_some() {
            browser.add_menu = false;
            browser.row_menu = None;
            return true;
        }
        if browser.job.is_none() && browser.confirm.take().is_some() {
            return true;
        }
        false
    }

    pub(super) fn browser_labels(pick: Pick) -> Vec<String> {
        match pick {
            Pick::LinkTarget => web::LINK_TARGETS
                .iter()
                .map(|(_, l)| (*l).to_owned())
                .collect(),
            Pick::SearchEngine => web::SEARCH_ENGINES
                .iter()
                .map(|(_, l)| (*l).to_owned())
                .collect(),
            Pick::Viewport => {
                let mut labels = vec!["Fill panel".to_owned(), "Responsive".to_owned()];
                labels.extend(
                    web::PRESETS
                        .iter()
                        .map(|preset| format!("{}\t{}", preset.label, preset.detail)),
                );
                labels
            }
            Pick::Zoom => web::ZOOM_LEVELS
                .iter()
                .map(|z| web::zoom_label(*z))
                .collect(),
            Pick::Appearance => web::APPEARANCES
                .iter()
                .map(|(_, l)| (*l).to_owned())
                .collect(),
            _ => web::FRAME_RATES
                .iter()
                .map(|r| format!("{r} fps"))
                .collect(),
        }
    }

    fn browser_selected(&self, pick: Pick) -> usize {
        let settings = &self.browser.settings;
        match pick {
            Pick::LinkTarget => web::index_of(&web::LINK_TARGETS, &settings.link_target),
            Pick::SearchEngine => web::index_of(&web::SEARCH_ENGINES, &settings.search_engine),
            Pick::Viewport => settings.viewport.index(),
            Pick::Zoom => web::zoom_index(settings.zoom),
            Pick::Appearance => web::index_of(&web::APPEARANCES, &settings.appearance),
            _ => web::frame_rate_index(settings.frame_rate),
        }
    }

    /// Writes a device setting, or says why a project scope cannot.
    fn browser_write(&mut self, change: impl FnOnce(&mut BrowserSettings), cx: &mut Context<Self>) {
        if self.ctx(cx).in_project {
            self.browser.notice = Some(Notice {
                title: "Setting not saved".to_owned(),
                description: Some(REFUSED.to_owned()),
                error: true,
            });
        } else {
            change(&mut self.browser.settings);
            self.browser.settings.save();
        }
        cx.notify();
    }

    pub(super) fn pick_browser(
        &mut self,
        pick: Pick,
        index: usize,
        _ctx: &Ctx,
        cx: &mut Context<Self>,
    ) {
        self.browser_write(
            |settings| match pick {
                Pick::LinkTarget => {
                    if let Some((wire, _)) = web::LINK_TARGETS.get(index) {
                        settings.link_target = (*wire).to_owned();
                    }
                }
                Pick::SearchEngine => {
                    if let Some((wire, _)) = web::SEARCH_ENGINES.get(index) {
                        settings.search_engine = (*wire).to_owned();
                    }
                }
                Pick::Viewport => settings.viewport = settings.viewport.pick(index),
                Pick::Zoom => {
                    if let Some(zoom) = web::ZOOM_LEVELS.get(index) {
                        settings.zoom = *zoom;
                    }
                }
                Pick::Appearance => {
                    if let Some((wire, _)) = web::APPEARANCES.get(index) {
                        settings.appearance = (*wire).to_owned();
                    }
                }
                _ => {
                    if let Some(rate) = web::FRAME_RATES.get(index) {
                        settings.frame_rate = *rate;
                    }
                }
            },
            cx,
        );
    }

    pub(super) fn cell_text(&self, cell: Cell) -> String {
        let settings = &self.browser.settings;
        let (width, height) = settings.viewport.size().unwrap_or(web::RESPONSIVE_SEED);
        match cell {
            Cell::CustomUrl => settings.custom_url.clone(),
            Cell::Width => width.to_string(),
            Cell::Height => height.to_string(),
        }
    }

    /// Saves a Browser text field as the web commits it: on blur or Enter, when valid and changed.
    pub(super) fn commit_cell(
        &mut self,
        cell: Cell,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match cell {
            Cell::CustomUrl => {
                let url: String = text.chars().take(web::CUSTOM_URL_MAX).collect();
                if url != self.browser.settings.custom_url {
                    self.browser_write(|settings| settings.custom_url = url, cx);
                }
            }
            Cell::Width | Cell::Height => {
                let axis = if cell == Cell::Width {
                    web::Axis::Width
                } else {
                    web::Axis::Height
                };
                if let Some(next) = self.browser.settings.viewport.resized(axis, text) {
                    self.browser_write(|settings| settings.viewport = next, cx);
                }
            }
        }
        let ctx = self.ctx(cx);
        self.sync_lines(&ctx, window, cx);
    }

    /// Keeps one rename field per profile the user made, showing the saved name when unfocused.
    pub(super) fn sync_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let custom: Vec<Profile> = self
            .browser
            .settings
            .listed()
            .into_iter()
            .filter(|profile| !profile.built_in())
            .collect();
        self.browser
            .names
            .retain(|name| custom.iter().any(|profile| profile.id == name.id));
        for profile in &custom {
            if !self.browser.names.iter().any(|name| name.id == profile.id) {
                let input = cx.new(|cx| InputState::new(window, cx));
                let id = profile.id.clone();
                let subscription =
                    cx.subscribe_in(&input, window, move |this, _, event: &InputEvent, _, cx| {
                        if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                            this.rename_profile(&id, cx);
                        }
                    });
                self.browser.names.push(Name {
                    id: profile.id.clone(),
                    input,
                    _subscription: subscription,
                });
            }
        }
        for name in &self.browser.names {
            let Some(profile) = custom.iter().find(|profile| profile.id == name.id) else {
                continue;
            };
            let focused = name.input.read(cx).focus_handle(cx).is_focused(window);
            if !focused && name.input.read(cx).value().as_ref() != profile.name.as_str() {
                let want = profile.name.clone();
                name.input
                    .update(cx, |state, cx| state.set_value(want, window, cx));
            }
        }
    }

    fn rename_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(name) = self.browser.names.iter().find(|name| name.id == id) else {
            return;
        };
        let text = name.input.read(cx).value().to_string();
        let current = self
            .browser
            .settings
            .profiles
            .iter()
            .find(|profile| profile.id == id)
            .map(|profile| profile.name.clone());
        let next: String = text.trim().chars().take(web::PROFILE_NAME_MAX).collect();
        if next.is_empty() || current.as_deref() == Some(next.as_str()) {
            return;
        }
        self.browser_write(
            |settings| {
                settings.rename_profile(id, &next);
            },
            cx,
        );
    }

    fn removal_available(&self, ctx: &Ctx) -> bool {
        self.browser.server_browser && ctx.connected
    }

    fn ask_clear(&mut self, profile: &Profile, remove: bool, cx: &mut Context<Self>) {
        self.browser.row_menu = None;
        self.browser.confirm = Some(Confirm {
            id: profile.id.clone(),
            name: profile.name.clone(),
            remove,
            error: None,
        });
        cx.notify();
    }

    /// `preview.clearProfile` for the confirmed profile; removing the profile follows its data.
    fn run_clear(&mut self, cx: &mut Context<Self>) {
        if self.browser.job.is_some() {
            return;
        }
        let Some(mut confirm) = self.browser.confirm.clone() else {
            return;
        };
        let ctx = self.ctx(cx);
        let token = self.next_token;
        let sent = self.removal_available(&ctx) && {
            self.next_token += 1;
            self.actions
                .try_send(Action::Call {
                    token,
                    call: Call::ClearBrowserProfile,
                    payload: json!({"profileId": confirm.id}),
                })
                .is_ok()
        };
        if sent {
            self.browser.job = Some(Job { token, confirm });
        } else if confirm.remove {
            confirm.error = Some("Connect to an environment before removing this profile.".into());
            self.browser.confirm = Some(confirm);
        } else {
            self.browser.confirm = None;
            self.browser.notice = Some(Notice {
                title: format!("Could not clear {}'s data", confirm.name),
                description: Some("You're not connected to a server yet.".to_owned()),
                error: true,
            });
        }
        cx.notify();
    }

    /// The outcome of `run_clear`; returns whether the token was its own.
    pub(super) fn browser_settled(&mut self, token: u64, event: &CallEvent) -> bool {
        if matches!(event, CallEvent::Progress(_)) {
            return self.browser.job.as_ref().is_some_and(|j| j.token == token);
        }
        if !self.browser.job.as_ref().is_some_and(|j| j.token == token) {
            return false;
        }
        let Some(Job { mut confirm, .. }) = self.browser.job.take() else {
            return false;
        };
        match event {
            CallEvent::Failed(message) if confirm.remove => {
                confirm.error = Some("Profile data could not be deleted. Try again.".into());
                self.browser.confirm = Some(confirm);
            }
            CallEvent::Failed(message) => {
                self.browser.confirm = None;
                self.browser.notice = Some(Notice {
                    title: format!("Could not clear {}'s data", confirm.name),
                    description: Some(message.clone()),
                    error: true,
                });
            }
            _ if confirm.remove => {
                self.browser.settings.remove_profile(&confirm.id);
                self.browser.settings.save();
                self.browser.confirm = None;
            }
            _ => {
                self.browser.confirm = None;
                self.browser.notice = Some(Notice {
                    title: format!("Cleared {}'s cookies and cache", confirm.name),
                    description: None,
                    error: false,
                });
            }
        }
        true
    }

    pub(super) fn browser(
        &mut self,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut sections: Vec<AnyElement> = self.browser_notice(cx).into_iter().collect();
        sections.push(self.agent_access(ctx, window, cx));
        sections.push(self.browsing(ctx, window, cx));
        sections.push(self.recordings(ctx, window, cx));
        sections
    }

    fn browser_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notice = self.browser.notice.as_ref()?;
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let tone = if notice.error {
            theme.destructive
        } else {
            theme.foreground
        };
        Some(
            h_flex()
                .items_start()
                .gap_3()
                .rounded(px(8.))
                .border_1()
                .border_color(if notice.error {
                    theme.destructive.opacity(0.5)
                } else {
                    theme.border
                })
                .px_3()
                .py_2()
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(tone)
                                .child(notice.title.clone()),
                        )
                        .children(notice.description.clone().map(|text| {
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(text)
                        })),
                )
                .child(
                    icon_button(
                        "settings-browser-notice-close",
                        Variant::GhostMuted,
                        true,
                        cx,
                    )
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.browser.notice = None;
                            cx.notify();
                        })
                    })
                    .child(Icon::new(Icons::X).size(px(14.))),
                )
                .into_any_element(),
        )
    }

    fn agent_access(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let field = Field::AgentBrowserAccess;
        let checked = self.value(field, ctx).as_bool().unwrap_or(true);
        let setting = self.setting(
            ids!("agent-browser-access"),
            "Agent browser access",
            if ctx.in_project {
                "Allow agents in this project to use the shared browser. Applies when the agent session next starts."
            } else {
                "Allow agents to use the shared browser. Projects can override it."
            },
            field,
            self.reset(
                "settings-agent-browser-access-reset",
                RESET,
                &[field],
                ctx,
                window,
                cx,
            ),
            self.toggle(
                "settings-agent-browser-access",
                field,
                checked,
                json!(true),
                json!(false),
                window,
                cx,
            ),
            None,
            ctx,
            window,
            cx,
        );
        section("Agent access", group(vec![setting], cx)).into_any_element()
    }

    /// A device-setting reset button, shown while the value differs from its default.
    fn device_reset(
        &self,
        id: &'static str,
        shown: bool,
        change: impl Fn(&mut BrowserSettings) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = cx.entity();
        shown.then(|| {
            reset_button_labeled(
                id,
                RESET,
                move |_, _, cx| view.update(cx, |this, cx| this.browser_write(&change, cx)),
                window,
                cx,
            )
        })
    }

    fn device_select(&self, pick: Pick, width: f32, cx: &mut Context<Self>) -> AnyElement {
        self.select(pick, self.browser_selected(pick), width, None, cx)
    }

    fn device_switch(
        &self,
        id: &'static str,
        checked: bool,
        change: impl Fn(&mut BrowserSettings) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        switch(
            id,
            checked,
            Rc::new(move |_, cx| view.update(cx, |this, cx| this.browser_write(&change, cx))),
            window,
            cx,
        )
    }

    fn browsing(&mut self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.browser.settings.clone();
        let mut rows = vec![self.profiles(ctx, window, cx)];
        rows.push(
            row(
                "Open links in",
                "Where links in the chat and terminal open. Hold \u{2318} or Ctrl while clicking a link to open it in your default browser either way.",
                self.device_reset(
                    "settings-browser-link-target-reset",
                    settings.link_target != "system",
                    |s| s.link_target = "system".to_owned(),
                    window,
                    cx,
                ),
                self.device_select(Pick::LinkTarget, 160., cx),
                self.wide,
                cx,
            )
            .into_any_element(),
        );
        rows.push(
            row(
                "Auto-show floating preview",
                "Show the floating preview when an agent opens a browser or device unless the agent says otherwise.",
                self.device_reset(
                    "settings-browser-auto-show-reset",
                    !settings.auto_show,
                    |s| s.auto_show = true,
                    window,
                    cx,
                ),
                self.device_switch(
                    "settings-browser-auto-show",
                    settings.auto_show,
                    {
                        let next = !settings.auto_show;
                        move |s| s.auto_show = next
                    },
                    window,
                    cx,
                ),
                self.wide,
                cx,
            )
            .into_any_element(),
        );
        rows.push(
            row(
                "Search engine",
                "What the address bar searches when you type something that isn't a URL.",
                self.device_reset(
                    "settings-browser-search-engine-reset",
                    settings.search_dirty(),
                    |s| {
                        s.search_engine = web::DEFAULT_SEARCH_ENGINE.to_owned();
                        s.custom_url = String::new();
                    },
                    window,
                    cx,
                ),
                self.device_select(Pick::SearchEngine, 160., cx),
                self.wide,
                cx,
            )
            .into_any_element(),
        );
        if settings.search_engine == "custom" {
            let status = (!web::valid_search_url(&settings.custom_url)).then(|| {
                SharedString::from(
                    "Enter an http or https URL containing %s. Searches use DuckDuckGo until then.",
                )
            });
            rows.push(
                row_with_status(
                    "Custom search URL",
                    "Use %s where the search terms go.",
                    status,
                    self.line(Key::Browser(Cell::CustomUrl), 256., false, window, cx),
                    self.wide,
                    cx,
                )
                .into_any_element(),
            );
        }
        let advanced = self.advanced(&settings, window, cx);
        let view = cx.entity();
        rows.push(disclosure(
            "settings-browser-advanced",
            self.advanced_open,
            Rc::new(move |_, cx| {
                view.update(cx, |this, cx| {
                    this.advanced_open = !this.advanced_open;
                    cx.notify();
                })
            }),
            advanced,
            window,
            cx,
        ));
        section("Browsing", group(rows, cx)).into_any_element()
    }

    fn advanced(
        &mut self,
        settings: &BrowserSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let view = cx.entity();
        let sized = settings.viewport.size().is_some();
        let theme = Tokens::of(cx);
        let rotate = {
            let view = view.clone();
            icon_button("settings-browser-rotate", Variant::GhostMuted, true, cx)
                .tooltip(tooltip("Rotate"))
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.browser_write(|s| s.viewport = s.viewport.rotated(), cx)
                    })
                })
                .child(Icon::new(Icons::RotateCcw).size(px(16.)))
        };
        let size = h_flex()
            .min_w_0()
            .items_center()
            .gap_1()
            .child(self.line(Key::Browser(Cell::Width), 80., false, window, cx))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("\u{d7}"),
            )
            .child(self.line(Key::Browser(Cell::Height), 80., false, window, cx))
            .child(rotate);
        let viewport = h_flex()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap_2()
            .child(self.device_select(Pick::Viewport, 176., cx))
            .when(sized, |control| control.child(size))
            .into_any_element();
        vec![
            row(
                "Default browser viewport",
                "Tab size for you and agents. Fill fits the panel; other sizes show the device toolbar.",
                self.device_reset(
                    "settings-browser-viewport-reset",
                    settings.viewport != web::Viewport::Fill,
                    |s| s.viewport = web::Viewport::Fill,
                    window,
                    cx,
                ),
                viewport,
                self.wide,
                cx,
            )
            .into_any_element(),
            row(
                "Default browser zoom",
                "Page zoom applied to new browser tabs.",
                self.device_reset(
                    "settings-browser-zoom-reset",
                    (settings.zoom - web::DEFAULT_ZOOM).abs() > 1e-9,
                    |s| s.zoom = web::DEFAULT_ZOOM,
                    window,
                    cx,
                ),
                self.device_select(Pick::Zoom, 160., cx),
                self.wide,
                cx,
            )
            .into_any_element(),
            row(
                "Default browser appearance",
                "The color scheme pages are told to prefer. System follows your OS setting.",
                self.device_reset(
                    "settings-browser-appearance-reset",
                    settings.appearance != "system",
                    |s| s.appearance = "system".to_owned(),
                    window,
                    cx,
                ),
                self.device_select(Pick::Appearance, 160., cx),
                self.wide,
                cx,
            )
            .into_any_element(),
        ]
    }

    fn recordings(
        &mut self,
        _ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.browser.settings.clone();
        let rows = vec![
            row(
                "Browser recording frame rate",
                "Maximum recording rate. 30 fps saves CPU and storage; 60 fps is smoother.",
                self.device_reset(
                    "settings-browser-frame-rate-reset",
                    settings.frame_rate != web::DEFAULT_FRAME_RATE,
                    |s| s.frame_rate = web::DEFAULT_FRAME_RATE,
                    window,
                    cx,
                ),
                self.device_select(Pick::FrameRate, 160., cx),
                self.wide,
                cx,
            )
            .into_any_element(),
            row(
                "Show key presses in recordings",
                "Show pressed keys and shortcuts in new recordings. Password fields are excluded.",
                None,
                self.device_switch(
                    "settings-browser-key-presses",
                    settings.key_presses,
                    {
                        let next = !settings.key_presses;
                        move |s| s.key_presses = next
                    },
                    window,
                    cx,
                ),
                self.wide,
                cx,
            )
            .into_any_element(),
            row(
                "Show mouse presses in recordings",
                "Highlight mouse presses and held buttons in new recordings.",
                None,
                self.device_switch(
                    "settings-browser-mouse-presses",
                    settings.mouse_presses,
                    {
                        let next = !settings.mouse_presses;
                        move |s| s.mouse_presses = next
                    },
                    window,
                    cx,
                ),
                self.wide,
                cx,
            )
            .into_any_element(),
        ];
        section("Recordings", group(rows, cx)).into_any_element()
    }

    /// A menu item that cannot be chosen: `data-disabled:opacity-64`.
    fn dead_item(id: impl Into<ElementId>, label: &str, cx: &App) -> AnyElement {
        div()
            .id(id)
            .min_h(px(28.))
            .flex()
            .items_center()
            .px_2()
            .py_1()
            .rounded(px(4.))
            .text_sm()
            .opacity(0.64)
            .text_color(Tokens::of(cx).foreground)
            .child(label.to_owned())
            .into_any_element()
    }

    fn separator(cx: &App) -> AnyElement {
        div()
            .h(px(1.))
            .mx(px(-4.))
            .my(px(4.))
            .bg(Tokens::of(cx).border)
            .into_any_element()
    }

    fn profiles(&mut self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let disabled = ctx.in_project;
        let listed = self.browser.settings.listed();
        let default_id = self.browser.settings.default_id();
        let removal = self.removal_available(ctx);
        let at_limit = self.browser.settings.at_profile_limit();
        let connected = ctx.connected;
        let sources = self.browser.sources.clone();
        let view = cx.entity();

        let trigger = button(
            "settings-browser-add-profile",
            Variant::Outline,
            !disabled,
            cx,
        )
        .child(Icon::new(Icons::Plus).size(px(14.)))
        .child("Add profile");
        let add = if disabled {
            trigger.into_any_element()
        } else {
            let opened = view.clone();
            let content_view = view.clone();
            Popover::new("settings-browser-add-menu")
                .anchor(Anchor::TopRight)
                .offset(px(4.))
                .open(self.browser.add_menu)
                .p_0()
                .appearance(false)
                .on_open_change(move |next, _, cx| {
                    opened.update(cx, |this, cx| {
                        this.browser.add_menu = *next;
                        cx.notify();
                    })
                })
                .trigger(PopTrigger::new(trigger))
                .content(move |_, _, cx| {
                    let theme = Tokens::of(cx);
                    let mut menu = popup_shell(cx).min_w(px(192.)).p_1();
                    menu = menu.child(if at_limit {
                        Self::dead_item("settings-browser-blank-profile", "Blank profile", cx)
                    } else {
                        let view = content_view.clone();
                        menu_item("settings-browser-blank-profile", "Blank profile", false, cx)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.browser.add_menu = false;
                                    this.browser_write(
                                        |s| {
                                            s.add_profile("New profile");
                                        },
                                        cx,
                                    )
                                })
                            })
                            .into_any_element()
                    });
                    if at_limit {
                        menu = menu.child(Self::dead_item(
                            "settings-browser-profile-limit",
                            "You\u{2019}ve reached the profile limit",
                            cx,
                        ));
                    }
                    menu = menu.child(Self::separator(cx)).child(
                        div()
                            .px_2()
                            .py(px(6.))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child("Import from"),
                    );
                    if sources.is_empty() {
                        menu = menu.child(Self::dead_item(
                            "settings-browser-no-sources",
                            "No supported browsers found",
                            cx,
                        ));
                    } else {
                        for (index, source) in sources.iter().enumerate() {
                            let id = ("settings-browser-import", index);
                            menu = menu.child(if connected {
                                let (view, name) = (content_view.clone(), source.name);
                                menu_item(id, name, false, cx)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| this.import_cookies(name, cx))
                                    })
                                    .into_any_element()
                            } else {
                                Self::dead_item(id, source.name, cx)
                            });
                        }
                        if !connected {
                            menu = menu.child(Self::dead_item(
                                "settings-browser-import-connect",
                                "Connect to an environment to import cookies",
                                cx,
                            ));
                        }
                    }
                    menu
                })
                .into_any_element()
        };

        let mut list = v_flex()
            .overflow_hidden()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border.opacity(0.6));
        for (index, profile) in listed.iter().enumerate() {
            let built_in = profile.built_in();
            let is_default = profile.id == default_id;
            let name_element = if built_in {
                div()
                    .truncate()
                    .text_sm()
                    .when(disabled, |name| name.opacity(0.64))
                    .child(profile.name.clone())
                    .into_any_element()
            } else {
                match self.browser.names.iter().find(|name| name.id == profile.id) {
                    Some(name) => div()
                        .w_full()
                        .max_w(px(224.))
                        .child(sm_field(&name.input, false, false, disabled, window, cx))
                        .into_any_element(),
                    None => div().into_any_element(),
                }
            };
            let badge = is_default.then(|| {
                div()
                    .when(disabled, |badge| badge.opacity(0.64))
                    .rounded(px(4.))
                    .bg(theme.primary)
                    .px(px(6.))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(gpui::white())
                    .child("Default")
            });
            let more = icon_button(
                ("settings-browser-profile-more", index),
                Variant::GhostMuted,
                !disabled,
                cx,
            )
            .size(px(24.))
            .child(Icon::new(Icons::EllipsisVertical).size(px(16.)));
            let more = if disabled {
                more.into_any_element()
            } else {
                let opened = view.clone();
                let content_view = view.clone();
                let (key, profile) = (profile.id.clone(), profile.clone());
                let open = self.browser.row_menu.as_deref() == Some(profile.id.as_str());
                Popover::new(("settings-browser-profile-menu", index))
                    .anchor(Anchor::TopRight)
                    .offset(px(4.))
                    .open(open)
                    .p_0()
                    .appearance(false)
                    .on_open_change(move |next, _, cx| {
                        let key = key.clone();
                        opened.update(cx, |this, cx| {
                            this.browser.row_menu = next.then_some(key);
                            cx.notify();
                        })
                    })
                    .trigger(PopTrigger::new(more))
                    .content(move |_, _, cx| {
                        Self::profile_menu(&content_view, &profile, index, is_default, removal, cx)
                    })
                    .into_any_element()
            };
            list = list.child(
                h_flex()
                    .id(("settings-browser-profile", index))
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .when(index > 0, |item| {
                        item.border_t_1().border_color(theme.border.opacity(0.6))
                    })
                    .child(
                        h_flex()
                            .min_w_0()
                            .flex_1()
                            .items_center()
                            .gap_2()
                            .child(name_element)
                            .children(badge),
                    )
                    .child(more),
            );
        }
        row_with_preview(
            "Browser profiles",
            "Profiles separate cookies and logins. Incognito data is cleared when the app closes.",
            None,
            add,
            div().mt_2().mb_2().child(list).into_any_element(),
            self.wide,
            cx,
        )
        .into_any_element()
    }

    fn profile_menu(
        view: &Entity<Self>,
        profile: &Profile,
        index: usize,
        is_default: bool,
        removal: bool,
        cx: &App,
    ) -> gpui::Div {
        let mut menu = popup_shell(cx).min_w(px(160.)).p_1();
        let id = |name: &'static str| (name, index);
        menu = menu.child(if is_default {
            Self::dead_item(id("settings-browser-default"), "Set as default", cx)
        } else {
            let (view, profile) = (view.clone(), profile.id.clone());
            menu_item(id("settings-browser-default"), "Set as default", false, cx)
                .on_click(move |_, _, cx| {
                    let profile = profile.clone();
                    view.update(cx, |this, cx| {
                        this.browser.row_menu = None;
                        this.browser_write(|s| s.default_profile = profile, cx)
                    })
                })
                .into_any_element()
        });
        menu = menu.child(if removal {
            let (view, profile) = (view.clone(), profile.clone());
            menu_item(
                id("settings-browser-clear"),
                "Clear cookies and cache",
                false,
                cx,
            )
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| this.ask_clear(&profile, false, cx))
            })
            .into_any_element()
        } else {
            Self::dead_item(id("settings-browser-clear"), "Clear cookies and cache", cx)
        });
        if !profile.built_in() {
            menu = menu.child(if removal {
                let (view, profile) = (view.clone(), profile.clone());
                menu_item(
                    id("settings-browser-remove"),
                    "Remove profile and data",
                    true,
                    cx,
                )
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| this.ask_clear(&profile, true, cx))
                })
                .into_any_element()
            } else {
                div()
                    .id(id("settings-browser-remove"))
                    .min_h(px(28.))
                    .flex()
                    .items_center()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .opacity(0.64)
                    .text_color(Tokens::of(cx).destructive)
                    .child("Remove profile and data")
                    .into_any_element()
            });
        }
        if !removal {
            menu = menu.child(Self::separator(cx)).child(Self::dead_item(
                id("settings-browser-no-environment"),
                NO_ENVIRONMENT,
                cx,
            ));
        }
        menu
    }

    /// The cookie import wizard needs the desktop bridge's keychain and browser profile access,
    /// which is Electron-only, so the menu lists the installed browsers and says so.
    fn import_cookies(&mut self, source: &str, cx: &mut Context<Self>) {
        self.browser.add_menu = false;
        self.browser.notice = Some(Notice {
            title: format!("Could not import from {source}"),
            description: Some(
                "Importing cookies is only available in the Electron desktop app.".to_owned(),
            ),
            error: false,
        });
        cx.notify();
    }

    /// The clear and remove confirmations (`AlertDialog`s of `BrowserProfilesSetting`).
    pub fn overlay(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.page != Page::Browser {
            return None;
        }
        let confirm = self.browser.confirm.clone()?;
        let theme = Tokens::of(cx);
        let ctx = self.ctx(cx);
        let busy = self.browser.job.is_some();
        let removal = self.removal_available(&ctx);
        let (cancel, run, dismiss) = (cx.entity(), cx.entity(), cx.entity());
        let (title, description, action) = if confirm.remove {
            (
                format!("Remove \u{201c}{}\u{201d}?", confirm.name),
                "Its cookies and logins are deleted. Desktop tabs already open in this profile stay open until you close them; server browser tabs close now.",
                if busy {
                    "Removing\u{2026}"
                } else {
                    "Remove profile"
                },
            )
        } else {
            (
                format!(
                    "Clear \u{201c}{}\u{201d}'s cookies and cache?",
                    confirm.name
                ),
                "You are signed out of its sites. Server browser tabs open in this profile close now.",
                if busy {
                    "Clearing\u{2026}"
                } else {
                    "Clear data"
                },
            )
        };
        let mut body: Vec<AnyElement> = Vec::new();
        if confirm.remove && !removal {
            body.push(
                note(
                    "Connect to an environment to remove this profile and its data.",
                    cx,
                )
                .into_any_element(),
            );
        }
        if let Some(error) = confirm.error.clone() {
            body.push(problem(error, cx).into_any_element());
        }
        let destructive = theme.destructive;
        let footer = vec![
            button(
                "settings-browser-confirm-cancel",
                Variant::Outline,
                !busy,
                cx,
            )
            .child("Cancel")
            .on_click(move |_, _, cx| {
                cancel.update(cx, |this, cx| {
                    if this.browser.job.is_none() {
                        this.browser.confirm = None;
                        cx.notify();
                    }
                })
            })
            .into_any_element(),
            button(
                "settings-browser-confirm-run",
                Variant::Ghost,
                !busy && (removal || !confirm.remove),
                cx,
            )
            .bg(destructive)
            .border_color(destructive)
            .text_color(gpui::white())
            .hover(move |style| style.bg(destructive.opacity(0.9)))
            .child(action)
            .on_click(move |_, _, cx| run.update(cx, |this, cx| this.run_clear(cx)))
            .into_any_element(),
        ];
        Some(dialog_of(
            "settings-browser-confirm",
            512.,
            title,
            description,
            body,
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    if this.browser.job.is_none() {
                        this.browser.confirm = None;
                        cx.notify();
                    }
                })
            },
            cx,
        ))
    }
}
