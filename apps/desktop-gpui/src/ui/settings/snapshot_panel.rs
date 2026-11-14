//! The SnapShots tab: `SnapShotSettings` (SnapShotSettings.tsx) and its "Set up snapshots" dialog
//! (SnapShotSetupDialog.tsx, wizard.tsx, PermissionChecklist.tsx), macOS direct mode only.

use super::components::{
    HoverHandler, KeyHandler, SelectTrigger, Toggle, close_button, ghost_button, group, kbd,
    outline_button, popup_shell, primary_button, row_with_status, secondary_button, section,
    switch, tab_stop,
};
use super::keybindings_panel::capture;
use super::tokens::Tokens;
use super::*;
use crate::assets::{MAC_ACCESSIBILITY, MAC_SCREEN_RECORDING};
use crate::keybinding_rules::{command_label, is_mac, parse_shortcut};
use crate::keybindings::{Keybindings, Rule};
use crate::mac_permissions::{self, MacPermissions};
use crate::settings_snapshot::{
    self as snap, Requested, Shortcut, SnapShotSettings, Sound, SoundSelection, State, Step,
};
use gpui::{
    ClickEvent, FocusHandle, KeyDownEvent, Keystroke, Subscription, linear_color_stop,
    linear_gradient, rgb, svg,
};
use std::rc::Rc;
use std::time::Duration;

/// `usePermissionStatus`' poll interval.
const POLL: Duration = Duration::from_millis(1500);
const SOUND_MENU: &str = "snap-shot-sound-menu";

struct Wizard {
    step: Step,
    was_enabled: bool,
    error: bool,
    advanced: bool,
}

pub struct SnapShotPanel {
    settings: SnapShotSettings,
    permissions: Option<MacPermissions>,
    rules: Vec<Rule>,
    wide: bool,
    /// The recorded shortcut that is not saved yet (`candidate`).
    candidate: Option<Shortcut>,
    recording: bool,
    _keys: Option<Subscription>,
    menu: Option<usize>,
    sound_trigger: FocusHandle,
    wizard: Option<Wizard>,
    epoch: u64,
}

impl SnapShotPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            settings: SnapShotSettings::load(),
            permissions: mac_permissions::current(),
            rules: Vec::new(),
            wide: true,
            candidate: None,
            recording: false,
            _keys: None,
            menu: None,
            sound_trigger: cx.focus_handle().tab_stop(true),
            wizard: None,
            epoch: 0,
        }
    }

    /// Re-reads the file and closes everything open, as the dialog does when it opens.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.settings = SnapShotSettings::load();
        self.permissions = mac_permissions::current();
        self.candidate = None;
        self.recording = false;
        self._keys = None;
        self.menu = None;
        self.wizard = None;
        cx.notify();
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    pub fn set_keybindings(&mut self, keybindings: &Keybindings, cx: &mut Context<Self>) {
        if self.rules != keybindings.rules {
            self.rules = keybindings.rules.clone();
            cx.notify();
        }
    }

    /// Escape closes the innermost thing first; returns whether there was one.
    pub fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.recording {
            self.stop_recording(cx);
            return true;
        }
        if self.menu.take().is_some() {
            window.focus(&self.sound_trigger, cx);
            cx.notify();
            return true;
        }
        if self.wizard.is_some() {
            self.close_setup(false, cx);
            return true;
        }
        false
    }

    fn state(&self) -> State {
        if self.wizard.is_some() && !self.settings.enabled {
            let mut settings = self.settings.clone();
            settings.enabled = true;
            return State::compute(&settings, self.permissions);
        }
        State::compute(&self.settings, self.permissions)
    }

    fn save(&mut self, change: impl FnOnce(&mut SnapShotSettings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.settings.save();
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let permissions = mac_permissions::current();
        if permissions != self.permissions {
            self.permissions = permissions;
            cx.notify();
        }
    }

    fn display_shortcut(&self) -> Shortcut {
        self.candidate
            .clone()
            .unwrap_or_else(|| self.settings.shortcut.clone())
    }

    fn shortcut_changed(&self) -> bool {
        self.candidate.as_ref().is_some_and(|candidate| {
            !snap::same_shortcut(candidate, &self.settings.shortcut, is_mac())
        })
    }

    fn conflict(&self) -> Option<String> {
        snap::keybinding_conflict(&self.display_shortcut(), &self.rules, is_mac())
            .map(command_label)
    }

    fn can_save_shortcut(&self) -> bool {
        self.shortcut_changed() && self.conflict().is_none()
    }

    /// `shortcutStatus`: recording, then a conflict, then the check result, then the bridge.
    fn shortcut_status(&self, state: &State) -> Option<String> {
        if self.recording {
            Some("Press your shortcut. Esc cancels.".to_owned())
        } else if let Some(command) = self.conflict() {
            Some(format!("T3 Code already uses this for \"{command}\"."))
        } else if self.shortcut_changed() {
            Some("Ready to save.".to_owned())
        } else {
            snap::shortcut_status(Some(state))
        }
    }

    fn start_recording(&mut self, cx: &mut Context<Self>) {
        if self.recording {
            self.stop_recording(cx);
            return;
        }
        let view = cx.weak_entity();
        self.recording = true;
        self._keys = Some(cx.intercept_keystrokes(move |event, _, cx| {
            let keystroke = event.keystroke.clone();
            let _ = view.update(cx, |this, cx| this.record(&keystroke, cx));
            cx.stop_propagation();
        }));
        cx.notify();
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        if self.recording {
            self.recording = false;
            self._keys = None;
            cx.notify();
        }
    }

    /// A global shortcut without a modifier would take that key from every app, so one is required.
    fn record(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        if keystroke.key == "escape" {
            self.stop_recording(cx);
            return;
        }
        let m = keystroke.modifiers;
        if !(m.control || m.alt || m.shift || m.platform) {
            return;
        }
        let chord = capture(keystroke).and_then(|input| parse_shortcut(&input));
        if let Some(chord) = chord {
            self.candidate = Some(Shortcut::Chord(chord));
            self.stop_recording(cx);
        }
    }

    fn cancel_shortcut(&mut self, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        self.candidate = None;
        cx.notify();
    }

    fn save_shortcut(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(candidate) = self.candidate.clone().filter(|_| self.can_save_shortcut()) else {
            return false;
        };
        self.save(|settings| settings.shortcut = candidate, cx);
        self.candidate = None;
        self.state().shortcut_registered
    }

    fn toggle_enabled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.enabled {
            self.save(|settings| settings.enabled = false, cx);
        } else {
            self.open_setup(Requested::Step(Step::Access), window, cx);
        }
    }

    fn set_include_accessibility(&mut self, on: bool, cx: &mut Context<Self>) {
        if on && self.permissions.is_some_and(|p| !p.accessibility) {
            cx.open_url(mac_permissions::ACCESSIBILITY_URL);
        }
        self.save(|settings| settings.include_accessibility = on, cx);
    }

    fn pick_sound(
        &mut self,
        selection: SoundSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save(|settings| settings.select_sound(selection), cx);
        self.close_menu(window, cx);
    }

    fn open_setup(&mut self, requested: Requested, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
        let step = snap::initial_step(&self.state(), requested);
        self.wizard = Some(Wizard {
            step,
            was_enabled: self.settings.enabled,
            error: false,
            advanced: false,
        });
        self.poll(step, window, cx);
        cx.notify();
    }

    fn close_setup(&mut self, completed: bool, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        self.epoch += 1;
        if let Some(wizard) = self.wizard.take()
            && snap::should_disable_on_close(wizard.was_enabled, completed)
        {
            self.save(|settings| settings.enabled = false, cx);
        }
        cx.notify();
    }

    fn change_step(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        if let Some(wizard) = &mut self.wizard {
            wizard.step = step;
            wizard.error = false;
        }
        self.poll(step, window, cx);
        cx.notify();
    }

    /// Re-reads the permissions every 1.5s while the access step is showing.
    fn poll(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        self.epoch += 1;
        if step != Step::Access || self.permissions.is_none() {
            return;
        }
        let (epoch, view) = (self.epoch, cx.weak_entity());
        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor().timer(POLL).await;
                    let live = cx
                        .update(|_, cx| {
                            view.update(cx, |this, cx| {
                                let live = this.epoch == epoch && this.wizard.is_some();
                                if live {
                                    this.refresh(cx);
                                }
                                live
                            })
                            .unwrap_or(false)
                        })
                        .unwrap_or(false);
                    if !live {
                        break;
                    }
                }
            })
            .detach();
    }

    fn allow_screen_recording(&mut self, cx: &mut Context<Self>) {
        if !mac_permissions::request_screen_recording() {
            cx.open_url(mac_permissions::SCREEN_RECORDING_URL);
        }
        self.refresh(cx);
    }

    fn allow_accessibility(&mut self, cx: &mut Context<Self>) {
        cx.open_url(mac_permissions::ACCESSIBILITY_URL);
        self.refresh(cx);
    }

    /// `enableForSetup`: the capture test, then turning snapshots on.
    fn enable_for_setup(&mut self, cx: &mut Context<Self>) -> bool {
        self.refresh(cx);
        if self.permissions.is_some_and(|p| !p.screen_recording) {
            mac_permissions::request_screen_recording();
            self.refresh(cx);
        }
        let ready = snap::access_ready(&self.state());
        if let Some(wizard) = &mut self.wizard {
            wizard.error = !ready;
        }
        ready
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        if self.shortcut_changed() && !self.save_shortcut(cx) {
            if let Some(wizard) = &mut self.wizard {
                wizard.error = true;
            }
            cx.notify();
            return;
        }
        self.save(|settings| settings.enabled = true, cx);
        self.close_setup(true, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(highlighted) = self.menu else {
            return;
        };
        let count = SoundSelection::ALL.len();
        match event.keystroke.key.as_str() {
            "down" => self.menu = Some((highlighted + 1) % count),
            "up" => self.menu = Some((highlighted + count - 1) % count),
            "home" => self.menu = Some(0),
            "end" => self.menu = Some(count - 1),
            "enter" | "space" => self.pick_sound(SoundSelection::ALL[highlighted], window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn open_sound_menu(&mut self, cx: &mut Context<Self>) {
        let selected = SoundSelection::ALL
            .iter()
            .position(|s| *s == self.settings.sound_selection())
            .unwrap_or(0);
        self.menu = Some(selected);
        cx.notify();
    }

    fn flag(
        &self,
        id: &'static str,
        checked: bool,
        enabled: bool,
        change: fn(&mut SnapShotSettings, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let toggle: Toggle = Rc::new(move |_, cx| {
            if enabled {
                view.update(cx, |this, cx| this.save(|s| change(s, !checked), cx))
            }
        });
        disabled_dim(enabled, switch(id, checked, toggle, window, cx))
    }

    /// `SnapShotShortcutKeys` in a `KbdGroup` (`gap-1`).
    fn keys(&self, shortcut: &Shortcut, cx: &App) -> AnyElement {
        h_flex()
            .gap_1()
            .children(
                snap::key_labels(shortcut, is_mac())
                    .into_iter()
                    .map(|label| kbd(label, cx)),
            )
            .into_any_element()
    }

    /// `useSnapShotShortcutRecorder`' button: `outline`, or `secondary` while recording.
    fn recorder(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = "snap-shot-recorder";
        let handle = tab_stop(&id.into(), window, cx);
        let content = if self.recording {
            div().child("Press shortcut\u{2026}").into_any_element()
        } else {
            self.keys(&self.display_shortcut(), cx)
        };
        let view = cx.entity();
        let button = if self.recording {
            secondary_button(id, cx)
        } else {
            outline_button(id, cx)
        };
        button
            .track_focus(&handle)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_recording(cx)))
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.start_recording(cx));
                }
            })
            .child(content)
            .into_any_element()
    }

    fn sound_label(&self, cx: &App) -> AnyElement {
        let muted = Tokens::of(cx).muted_foreground;
        match self.settings.sound_selection() {
            SoundSelection::Off => div().child("Off").into_any_element(),
            SoundSelection::Sound(Sound::SoftPop) => h_flex()
                .gap_1()
                .child("Whoosh")
                .child(div().text_color(muted).child("(Default)"))
                .into_any_element(),
            SoundSelection::Sound(Sound::CameraShutter) => div().child("Click").into_any_element(),
        }
    }

    /// `Menu` with a `SelectButton` trigger and radio rows; the sound rows are
    /// `grid-cols-[1fr_auto]` with a play item.
    fn sound_select(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let on_key: KeyHandler = {
            let view = view.clone();
            Rc::new(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.open_sound_menu(cx));
                }
            })
        };
        let trigger = SelectTrigger::new(
            "snap-shot-sound",
            "",
            0.,
            self.sound_trigger.clone(),
            on_key,
        )
        .auto_width()
        .content(self.sound_label(cx));
        let open = self.menu.is_some() && enabled;
        let highlighted = self.menu.unwrap_or(usize::MAX);
        let selected = self.settings.sound_selection();
        let on_hover: HoverHandler = {
            let view = view.clone();
            Rc::new(move |index, cx| {
                view.update(cx, |this, cx| {
                    if this.menu.is_some_and(|current| current != index) {
                        this.menu = Some(index);
                        cx.notify();
                    }
                })
            })
        };
        let popover = Popover::new(SOUND_MENU)
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change({
                let view = view.clone();
                move |opened, window, cx| {
                    view.update(cx, |this, cx| {
                        if *opened {
                            this.open_sound_menu(cx);
                        } else {
                            this.close_menu(window, cx);
                        }
                    })
                }
            })
            .trigger(trigger)
            .content(move |_, _, cx| {
                let theme = Tokens::of(cx);
                let (accent, accent_foreground, chosen, muted) = (
                    theme.accent,
                    theme.accent_foreground,
                    theme.foreground.opacity(0.08),
                    theme.muted_foreground,
                );
                let rows = SoundSelection::ALL
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        let (label, play): (AnyElement, bool) = match option {
                            SoundSelection::Off => (div().child("Off").into_any_element(), false),
                            SoundSelection::Sound(Sound::SoftPop) => (
                                h_flex()
                                    .gap_1()
                                    .child("Whoosh")
                                    .child(div().text_color(muted).child("(Default)"))
                                    .into_any_element(),
                                true,
                            ),
                            SoundSelection::Sound(Sound::CameraShutter) => {
                                (div().child("Click").into_any_element(), true)
                            }
                        };
                        let (pick, hover) = (view.clone(), on_hover.clone());
                        let option = *option;
                        let item = div()
                            .id((SOUND_MENU, index))
                            .flex_1()
                            .min_h(px(28.))
                            .flex()
                            .items_center()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .text_sm()
                            .cursor_pointer()
                            .when(option == selected, |item| item.bg(chosen))
                            .when(index == highlighted, |item| {
                                item.bg(accent).text_color(accent_foreground)
                            })
                            .on_hover(move |entered, _, cx| {
                                if *entered {
                                    hover(index, cx);
                                }
                            })
                            .on_click(move |_, window, cx| {
                                pick.update(cx, |this, cx| this.pick_sound(option, window, cx))
                            })
                            .child(label);
                        h_flex()
                            .child(item)
                            .when(play, |row| {
                                row.child(
                                    div()
                                        .id((SOUND_MENU, index + 10))
                                        .min_h(px(28.))
                                        .flex()
                                        .items_center()
                                        .px_2()
                                        .py_1()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .text_color(muted.opacity(0.8))
                                        .hover(move |style| style.bg(accent))
                                        .child(Icon::new(Icons::Play).size(px(16.))),
                                )
                            })
                            .into_any_element()
                    });
                popup_shell(160., cx).children(rows)
            });
        disabled_dim(enabled, popover.into_any_element())
    }

    /// The permission icons of Icons.tsx: a 32px tile on a vertical gradient with a 25% white
    /// border and `drop-shadow-sm`, holding a white glyph.
    fn mac_icon(top: u32, bottom: u32, glyph: &'static str) -> AnyElement {
        div()
            .size(px(32.))
            .flex_shrink_0()
            .rounded(px(7.))
            .border_1()
            .border_color(gpui::white().opacity(0.25))
            .bg(linear_gradient(
                180.,
                linear_color_stop(rgb(top), 0.),
                linear_color_stop(rgb(bottom), 1.),
            ))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::black().opacity(0.1),
                offset: gpui::point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
                inset: false,
            }])
            .child(svg().path(glyph).size(px(30.)).text_color(gpui::white()))
            .into_any_element()
    }
}

/// `disabled:opacity-64`.
fn disabled_dim(enabled: bool, element: AnyElement) -> AnyElement {
    if enabled {
        element
    } else {
        div().opacity(0.64).child(element).into_any_element()
    }
}

impl Render for SnapShotPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state();
        let available = state.mode != snap::Mode::Unavailable;
        let wide = self.wide;
        let enabled = self.settings.enabled;
        let view = cx.entity();

        let setup =
            (enabled && !snap::setup_complete(Some(&state), self.settings.include_accessibility))
                .then(|| {
                    outline_button("snap-shot-setup", cx)
                        .child(snap::setup_button_label(Some(&state)))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_setup(Requested::Resume, window, cx)
                        }))
                        .into_any_element()
                });
        let toggle: Toggle = {
            let view = view.clone();
            Rc::new(move |window, cx| {
                if available {
                    view.update(cx, |this, cx| this.toggle_enabled(window, cx))
                }
            })
        };
        let checked = enabled;
        let main_switch = disabled_dim(
            available,
            switch("snap-shot-enabled", checked, toggle, window, cx),
        );
        let mut rows = vec![
            row_with_status(
                "SnapShots",
                snap::DESCRIPTION,
                Some(snap::status(Some(&state), enabled).into()),
                h_flex()
                    .items_center()
                    .gap_2()
                    .children(setup)
                    .child(main_switch)
                    .into_any_element(),
                wide,
                cx,
            )
            .into_any_element(),
        ];
        if enabled && available {
            let accessibility = {
                let view = view.clone();
                let on = self.settings.include_accessibility;
                let toggle: Toggle = Rc::new(move |_, cx| {
                    view.update(cx, |this, cx| this.set_include_accessibility(!on, cx))
                });
                switch("snap-shot-accessibility", on, toggle, window, cx)
            };
            let changed = self.shortcut_changed();
            let can_save = self.can_save_shortcut();
            let shortcut_control = h_flex()
                .items_center()
                .gap_2()
                .child(self.recorder(window, cx))
                .when(changed, |controls| {
                    controls
                        .child(
                            primary_button("snap-shot-save-shortcut", can_save, true, cx)
                                .child("Save")
                                .when(can_save, |button| {
                                    button.on_click(cx.listener(|this, _, _, cx| {
                                        this.save_shortcut(cx);
                                    }))
                                }),
                        )
                        .child(
                            ghost_button("snap-shot-cancel-shortcut", true, cx)
                                .h(px(24.))
                                .px(px(7.))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_shortcut(cx))),
                        )
                })
                .into_any_element();
            let sound = self.sound_select(available, cx);
            let flash = self.flag(
                "snap-shot-flash-switch",
                self.settings.flash,
                available,
                |s, on| s.flash = on,
                window,
                cx,
            );
            let animations = self.flag(
                "snap-shot-animations-switch",
                self.settings.animations,
                available,
                |s, on| s.animations = on,
                window,
                cx,
            );
            rows.extend([
                row_with_status(
                    "Include app text",
                    "Include text and controls when the app makes them available.",
                    None,
                    accessibility,
                    wide,
                    cx,
                )
                .into_any_element(),
                row_with_status(
                    "Capture shortcut",
                    "Capture the window you're using without switching apps.",
                    self.shortcut_status(&state).map(Into::into),
                    shortcut_control,
                    wide,
                    cx,
                )
                .into_any_element(),
                row_with_status(
                    "Capture sound",
                    "Choose the sound played when capture starts.",
                    None,
                    sound,
                    wide,
                    cx,
                )
                .into_any_element(),
                row_with_status(
                    "Capture flash",
                    "Show a gentle cue on the captured window.",
                    None,
                    flash,
                    wide,
                    cx,
                )
                .into_any_element(),
                row_with_status(
                    "Capture animations",
                    "Animate captured windows into your draft.",
                    None,
                    animations,
                    wide,
                    cx,
                )
                .into_any_element(),
            ]);
        }
        v_flex()
            .w_full()
            .gap_8()
            .on_key_down(cx.listener(|this, event, window, cx| this.key_down(event, window, cx)))
            .child(section("SnapShots", group(rows, cx)))
    }
}

/// The setup dialog: `WizardPopup` (`max-w-xl`) with `WizardHeader`, `WizardPanel` and a bare
/// `WizardFooter`.
impl SnapShotPanel {
    pub fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let wizard = self.wizard.as_ref()?;
        let (step, was_enabled, failed, advanced) = (
            wizard.step,
            wizard.was_enabled,
            wizard.error,
            wizard.advanced,
        );
        let theme = Tokens::of(cx);
        let state = self.state();
        let include = self.settings.include_accessibility;
        let permissions_ready = snap::mac_permissions_ready(&state, include);
        let access_ready = snap::access_ready(&state);
        let changed = self.shortcut_changed();

        let (title, description) = match step {
            Step::Access if state.message.is_some() && self.permissions.is_none() => (
                "Let's try that again",
                "Couldn't check snapshots. Try again to continue.",
            ),
            Step::Access if permissions_ready => (
                "Allow snapshots",
                "Test a snapshot of the current window. If macOS asks to bypass its window picker, choose Allow. The test image is discarded.",
            ),
            Step::Access => ("Allow snapshots", "Allow each permission, then continue."),
            Step::Shortcut => (
                "Choose your shortcut",
                "Use both Shift keys, or record a different shortcut.",
            ),
        };

        let border = if theme.dark {
            gpui::white().opacity(0.08)
        } else {
            theme.foreground.opacity(0.1)
        };
        let ring = if theme.dark {
            gpui::white().opacity(0.05)
        } else {
            gpui::black().opacity(0.05)
        };
        let header = v_flex()
            .gap_2()
            .px_6()
            .pt_6()
            .pb_3()
            .child(
                div()
                    .text_xl()
                    .line_height(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Set up snapshots"),
            )
            .child(self.steps(step, cx));

        let mut body = v_flex().gap_4().text_sm().child(
            v_flex()
                .gap_2()
                .child(div().font_weight(FontWeight::MEDIUM).child(title))
                .child(div().text_color(theme.muted_foreground).child(description)),
        );
        match step {
            Step::Access => {
                if let Some(permissions) = self.permissions {
                    body = body.child(self.checklist(permissions, include, cx));
                }
            }
            Step::Shortcut => {
                let retry = !changed
                    && !state.shortcut_registered
                    && !matches!(
                        self.settings.shortcut,
                        Shortcut::BothShift | Shortcut::Pair(_)
                    );
                body = body.child(
                    v_flex()
                        .gap_3()
                        .child(h_flex().child(self.recorder(window, cx)))
                        .children(self.shortcut_status(&state).map(|status| {
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(status)
                        }))
                        .children(retry.then(|| {
                            ghost_button("snap-shot-retry", true, cx)
                                .h(px(28.))
                                .px(px(9.))
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Try again")
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))
                        })),
                );
            }
        }
        if step == Step::Shortcut && !access_ready {
            body = body.child(
                div()
                    .text_color(theme.destructive)
                    .child("Capture needs attention. Go back to check access."),
            );
        }
        if failed {
            body = body.child(
                div()
                    .text_color(theme.destructive)
                    .child("Couldn't finish this step. Try again or check Advanced for help."),
            );
        }
        let details: Vec<String> = if step == Step::Access {
            state.message.clone().into_iter().collect()
        } else {
            Vec::new()
        };
        if !details.is_empty() || failed {
            body = body.child(self.advanced(&details, advanced, cx));
        }
        let panel = div()
            .min_w_0()
            .px_6()
            .py_5()
            .border_1()
            .border_color(ring)
            .bg(if theme.dark {
                gpui::white().opacity(0.02)
            } else {
                theme.background.opacity(0.8)
            })
            .child(body);

        let footer = h_flex()
            .justify_end()
            .gap_2()
            .px_6()
            .py_4()
            .children((step == Step::Shortcut).then(|| {
                wizard_ghost("snap-shot-back", cx).child("Back").on_click(
                    cx.listener(|this, _, window, cx| this.change_step(Step::Access, window, cx)),
                )
            }))
            .child(
                wizard_ghost("snap-shot-close-setup", cx)
                    .child(if was_enabled { "Close" } else { "Finish later" })
                    .on_click(cx.listener(|this, _, _, cx| this.close_setup(false, cx))),
            )
            .child(match step {
                Step::Access => primary_button("snap-shot-continue", permissions_ready, false, cx)
                    .child("Test capture and continue")
                    .when(permissions_ready, |button| {
                        button.on_click(cx.listener(|this, _, window, cx| {
                            if this.enable_for_setup(cx) {
                                this.change_step(Step::Shortcut, window, cx);
                            }
                        }))
                    }),
                Step::Shortcut => {
                    let ready = access_ready
                        && if changed {
                            self.can_save_shortcut()
                        } else {
                            snap::shortcut_ready(&state, changed)
                        };
                    primary_button("snap-shot-done", ready, false, cx)
                        .child(if changed { "Save and finish" } else { "Done" })
                        .when(ready, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.finish(cx)))
                        })
                }
            });

        let dismiss = cx.entity();
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("snap-shot-backdrop")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .bg(theme.backdrop())
                        .on_click(move |_, _, cx| {
                            dismiss.update(cx, |this, cx| this.close_setup(false, cx))
                        }),
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
                                .w(px(576.))
                                .max_w_full()
                                .max_h_full()
                                .overflow_hidden()
                                .rounded(px(16.))
                                .border_1()
                                .border_color(border)
                                .bg(theme.background)
                                .text_color(theme.foreground)
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
                                .child(header)
                                .child(panel)
                                .child(footer)
                                .child(div().absolute().top(px(8.)).right(px(8.)).child(
                                    close_button("snap-shot-close-x", cx).on_click(
                                        cx.listener(|this, _, _, cx| this.close_setup(false, cx)),
                                    ),
                                )),
                        ),
                )
                .into_any_element(),
        )
    }

    /// `WizardSteps`: `rounded-xl p-1` on `background` (`white/4` dark) with a ring, one cell per
    /// step. Steps up to the current one are buttons.
    fn steps(&self, current: Step, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let index_of = |step: Step| usize::from(step == Step::Shortcut);
        let current_index = index_of(current);
        let ring = if theme.dark {
            gpui::white().opacity(0.05)
        } else {
            gpui::black().opacity(0.05)
        };
        let faint = if theme.dark {
            gpui::white().opacity(0.1)
        } else {
            gpui::black().opacity(0.1)
        };
        let cells =
            [("Access", Step::Access), ("Shortcut", Step::Shortcut)].map(|(label, step)| {
                let index = index_of(step);
                let is_current = index == current_index;
                let done = index < current_index;
                let clickable = index <= current_index && !is_current;
                let card = theme.card;
                let (circle_bg, circle_text, circle_ring) = if done {
                    (theme.primary, gpui::white(), theme.primary)
                } else if is_current {
                    (
                        theme.primary.opacity(0.1),
                        theme.primary,
                        theme.primary.opacity(0.3),
                    )
                } else {
                    (
                        if theme.dark {
                            gpui::white().opacity(0.05)
                        } else {
                            card
                        },
                        theme.muted_foreground,
                        faint,
                    )
                };
                let circle = div()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .rounded_full()
                    .border_1()
                    .border_color(circle_ring)
                    .bg(circle_bg)
                    .text_color(circle_text)
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(if done {
                        Icon::new(IconName::Check).size(px(16.)).into_any_element()
                    } else {
                        div().child((index + 1).to_string()).into_any_element()
                    });
                div()
                    .id(("snap-shot-step", index))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px(px(10.))
                    .py_2()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if is_current {
                        ring
                    } else {
                        gpui::transparent_black()
                    })
                    .when(is_current, |cell| {
                        cell.bg(card).when(!theme.dark, |cell| {
                            cell.shadow(vec![gpui::BoxShadow {
                                color: gpui::black().opacity(0.05),
                                offset: gpui::point(px(0.), px(1.)),
                                blur_radius: px(2.),
                                spread_radius: px(0.),
                                inset: false,
                            }])
                        })
                    })
                    .when(clickable, |cell| {
                        cell.cursor_pointer()
                            .hover(move |style| style.bg(card))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.change_step(step, window, cx)
                            }))
                    })
                    .child(circle)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if is_current {
                                theme.foreground
                            } else {
                                theme.muted_foreground
                            })
                            .child(label),
                    )
            });
        h_flex()
            .gap_1()
            .p_1()
            .rounded(px(12.))
            .border_1()
            .border_color(ring)
            .bg(if theme.dark {
                gpui::white().opacity(0.04)
            } else {
                theme.background
            })
            .children(cells)
            .into_any_element()
    }

    /// `PermissionChecklist`: a `rounded-lg border px-3 py-2` row per permission.
    fn checklist(
        &self,
        permissions: MacPermissions,
        include: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let screen = self.permission(
            "snap-shot-allow-screen",
            Self::mac_icon(0xff6972, 0xff2938, MAC_SCREEN_RECORDING),
            "Screen Recording",
            "Capture the window you're using.",
            permissions.screen_recording,
            |this, cx| this.allow_screen_recording(cx),
            cx,
        );
        let accessibility = self.permission(
            "snap-shot-allow-accessibility",
            Self::mac_icon(0x48b6ff, 0x0085ff, MAC_ACCESSIBILITY),
            "Accessibility",
            if include {
                "Include text and controls from the captured app."
            } else {
                "Optional. Include text and controls from the captured app."
            },
            permissions.accessibility,
            |this, cx| this.allow_accessibility(cx),
            cx,
        );
        let _ = theme;
        v_flex()
            .gap_2()
            .child(screen)
            .child(accessibility)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn permission(
        &self,
        id: &'static str,
        icon: AnyElement,
        title: &'static str,
        description: &'static str,
        granted: bool,
        allow: fn(&mut Self, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let status = if granted {
            h_flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(rgb(0x10b981))
                .child(Icon::new(Icons::CircleCheck).size(px(16.)))
                .child("Allowed")
                .into_any_element()
        } else {
            outline_button(id, cx)
                .child("Allow")
                .on_click(cx.listener(move |this, _, _, cx| allow(this, cx)))
                .into_any_element()
        };
        h_flex()
            .items_center()
            .gap_3()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .px_3()
            .py_2()
            .child(icon)
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .child(div().font_weight(FontWeight::MEDIUM).child(title))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(description),
                    ),
            )
            .child(status)
            .into_any_element()
    }

    /// The `<details>` "Advanced" block.
    fn advanced(&self, details: &[String], open: bool, cx: &mut Context<Self>) -> AnyElement {
        let muted = Tokens::of(cx).muted_foreground;
        let summary = h_flex()
            .id("snap-shot-advanced")
            .gap_1()
            .items_center()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(wizard) = &mut this.wizard {
                    wizard.advanced = !wizard.advanced;
                }
                cx.notify();
            }))
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(12.))
                    .when(open, |icon| {
                        icon.rotate(gpui::radians(std::f32::consts::FRAC_PI_2))
                    }),
            )
            .child("Advanced");
        v_flex()
            .text_xs()
            .text_color(muted)
            .child(summary)
            .when(open, |block| {
                block.child(
                    v_flex()
                        .mt_3()
                        .gap_3()
                        .children(details.iter().map(|detail| div().child(detail.clone()))),
                )
            })
            .into_any_element()
    }
}

/// `Button variant="ghost"` at the default size: `h-8 px-[11px] text-sm`.
fn wizard_ghost(id: &'static str, cx: &App) -> gpui::Stateful<gpui::Div> {
    ghost_button(id, true, cx)
        .h(px(32.))
        .px(px(11.))
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
}
