//! The Labs tab: `LabsSettingsPanel` (LabsSettingsPanel.tsx) and `LegacyFeaturesSection`
//! (SettingsPanels.tsx:749-783) under the scope sentence (SettingsScopeSentence.tsx:45-91).
//! The panel owns its select menu, pickers and focus handles, so the other tabs stay untouched.

use super::components::{group, row, section, switch};
use super::scope_pickers::ScopeBar;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::settings_labs::{
    self as labs, GUEST_LINKS, LINKED_ENVIRONMENTS, LabsSettings, Link, Piece, Reach,
    SIMLOCK_HOSTS, ServerFlags, ServerKey,
};
use gpui::{EventEmitter, HighlightStyle, InteractiveText, StyledText, UnderlineStyle};
use std::collections::HashMap;
use std::rc::Rc;

/// What the panel asks of the dialog.
pub enum LabsEvent {
    /// An inline link to another section.
    OpenSection(Section),
}

pub struct LabsPanel {
    actions: ActionSender,
    settings: LabsSettings,
    flags: ServerFlags,
    scope: Entity<ScopeBar>,
    wide: bool,
    /// Server writes in flight: the key and the value to fall back to.
    pending: HashMap<u64, (ServerKey, bool)>,
    next_token: u64,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<LabsEvent> for LabsPanel {}

impl LabsPanel {
    pub fn new(actions: ActionSender, scope: Entity<ScopeBar>, cx: &mut Context<Self>) -> Self {
        let observed = cx.observe(&scope, |_, _, cx| cx.notify());
        Self {
            actions,
            settings: LabsSettings::load(),
            flags: ServerFlags::default(),
            scope,
            wide: true,
            pending: HashMap::new(),
            next_token: labs::TOKEN_BASE,
            _subscriptions: vec![observed],
        }
    }

    /// Re-reads the file and closes any menu, as the dialog does when it opens.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.settings = LabsSettings::load();
        cx.notify();
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    /// The attached environment's flags, as the status carries them.
    pub fn set_flags(&mut self, flags: ServerFlags, cx: &mut Context<Self>) {
        if self.pending.is_empty() && self.flags != flags {
            self.flags = flags;
            cx.notify();
        }
    }

    /// The outcome of a server write: a failure puts the switch back.
    pub fn on_call(&mut self, token: u64, event: crate::status::CallEvent, cx: &mut Context<Self>) {
        use crate::status::CallEvent;
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        let Some((key, previous)) = self.pending.remove(&token) else {
            return;
        };
        if matches!(event, CallEvent::Failed(_)) {
            self.flags.set(key, previous);
            cx.notify();
        }
    }

    fn update(&mut self, change: impl FnOnce(&mut LabsSettings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.settings.save();
        cx.notify();
    }

    fn link(&mut self, link: Link, cx: &mut Context<Self>) {
        match link {
            Link::Section(section) => cx.emit(LabsEvent::OpenSection(section)),
            Link::External(url) => cx.open_url(url),
        }
    }

    /// A server switch: the write goes to the environments in scope that this app can write.
    fn toggle_server(&mut self, key: ServerKey, cx: &mut Context<Self>) {
        let Reach::Targets(_) = labs::reach(
            key,
            self.scope.read(cx).search(),
            self.scope.read(cx).data(),
        ) else {
            return;
        };
        let previous = self.flags.get(key);
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call: Call::UpdateSettings,
            payload: key.patch(!previous),
        };
        if self.actions.try_send(action).is_err() {
            return;
        }
        self.flags.set(key, !previous);
        self.pending.insert(token, (key, previous));
        cx.notify();
    }

    fn flag(
        &self,
        id: &'static str,
        checked: bool,
        toggle: fn(&mut LabsSettings),
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

    /// `ScopedSwitch` on an environment-wide key: inert, at half opacity under a tooltip, when
    /// the scope cannot write it (settingsLayout.tsx:405-448).
    fn server_switch(
        &self,
        id: &'static str,
        key: ServerKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let checked = self.flags.get(key);
        match labs::reach(
            key,
            self.scope.read(cx).search(),
            self.scope.read(cx).data(),
        ) {
            Reach::Targets(_) => switch(
                id,
                checked,
                Rc::new(move |_, cx| view.update(cx, |this, cx| this.toggle_server(key, cx))),
                window,
                cx,
            ),
            Reach::Inert(message) => div()
                .id((id, 1usize))
                .opacity(0.5)
                .tooltip(tooltip(message))
                .child(switch(id, checked, Rc::new(|_, _| {}), window, cx))
                .into_any_element(),
        }
    }

    /// A description with inline links: `underline underline-offset-2`.
    fn linked(
        &self,
        id: &'static str,
        pieces: &'static [Piece],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = Tokens::of(cx).muted_foreground;
        let mut text = String::new();
        let mut ranges = Vec::new();
        let mut targets = Vec::new();
        for piece in pieces {
            match piece {
                Piece::Text(plain) => text.push_str(plain),
                Piece::Link(label, link) => {
                    let start = text.len();
                    text.push_str(label);
                    ranges.push(start..text.len());
                    targets.push(*link);
                }
            }
        }
        let underline = HighlightStyle {
            underline: Some(UnderlineStyle {
                thickness: px(1.),
                color: Some(muted),
                wavy: false,
            }),
            ..HighlightStyle::default()
        };
        let styled = StyledText::new(text).with_highlights(
            ranges
                .iter()
                .cloned()
                .map(|range| (range, underline))
                .collect::<Vec<_>>(),
        );
        let view = cx.entity();
        InteractiveText::new(id, styled)
            .on_click(ranges, move |index, _, cx| {
                let link = targets[index];
                view.update(cx, |this, cx| this.link(link, cx));
            })
            .into_any_element()
    }
}

impl Render for LabsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.settings;
        let wide = self.wide;
        let rows = |items: Vec<gpui::Div>| -> Vec<AnyElement> {
            items
                .into_iter()
                .map(IntoElement::into_any_element)
                .collect()
        };
        let beta = group(
            rows(vec![
                row(
                    "Boring sidebar (group by project)",
                    "Group sidebar threads into project folders. Pinned threads stay on top, and Snoozed and Settled stay below the folders.",
                    None,
                    switch(
                        "settings-boring-sidebar",
                        s.boring_checked(),
                        {
                            let view = cx.entity();
                            Rc::new(move |_, cx| {
                                view.update(cx, |this, cx| {
                                    let checked = !this.settings.boring_checked();
                                    this.update(|s| s.set_boring(checked), cx)
                                })
                            })
                        },
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Two-line folder rows",
                    "In the boring sidebar, show folder threads on two lines: the title, then the branch, pull request, and agent.",
                    None,
                    self.flag(
                        "settings-boring-multiline",
                        s.boring_multiline,
                        |s| s.boring_multiline = !s.boring_multiline,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Guest links",
                    self.linked("settings-labs-guest-links", GUEST_LINKS, cx),
                    None,
                    self.server_switch("settings-guest-links", ServerKey::GuestSharing, window, cx),
                    wide,
                    cx,
                ),
                row(
                    "Linked environments",
                    self.linked("settings-labs-linked", LINKED_ENVIRONMENTS, cx),
                    None,
                    self.server_switch(
                        "settings-linked-environments",
                        ServerKey::LinkedEnvironments,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Simlock device hosts (alpha)",
                    self.linked("settings-labs-simlock", SIMLOCK_HOSTS, cx),
                    None,
                    self.server_switch(
                        "settings-simlock-hosts",
                        ServerKey::SimlockHosts,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
            ]),
            cx,
        );
        let legacy = group(
            rows(vec![
                row(
                    "Plan mode (legacy)",
                    "Restore Build/Plan, /plan, /default, and Shift+Tab. Off uses build mode.",
                    None,
                    self.flag(
                        "settings-plan-mode",
                        s.plan_mode,
                        |s| s.plan_mode = !s.plan_mode,
                        window,
                        cx,
                    ),
                    wide,
                    cx,
                ),
                row(
                    "Context window indicator (legacy)",
                    "Shows context window usage as a circular indicator in the composer.",
                    None,
                    self.flag(
                        "settings-context-meter",
                        s.context_meter,
                        |s| s.context_meter = !s.context_meter,
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
            .child(section("Beta", beta))
            .child(section("Legacy", legacy))
    }
}
