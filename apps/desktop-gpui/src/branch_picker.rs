//! The composer strip's branch control (`BranchToolbarBranchSelector.tsx`): lists the project's
//! refs, and on a pick either checks the ref out (a draft in the current checkout) or records it
//! as the base of the new worktree. The owner applies what it emits.

use crate::connection::{Action, ActionSender, Call};
use crate::ref_picker::RefMenu;
use crate::refs::Page;
use crate::status::CallEvent;
use crate::strip::{Branch, EnvMode};
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, ScrollHandle, Window,
};
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};
use std::{collections::HashMap, rc::Rc};

/// Tokens of this picker's calls start here, apart from the details card's.
pub const TOKEN_BASE: u64 = 1 << 40;
const REF_PAGE: u64 = 100;

/// What the owner learns from the picker.
pub enum Event {
    /// A new worktree will start from this ref.
    Base(Branch),
    /// The checkout of `cwd` moved to another ref.
    Switched,
    Failed { title: &'static str, message: String },
}

impl EventEmitter<Event> for BranchPicker {}

/// What picking a row does, by workspace mode.
#[derive(Debug, PartialEq, Eq)]
pub enum Pick {
    /// The ref is already checked out.
    Keep,
    /// A new worktree starts from the ref.
    Base,
    Checkout,
}

pub fn plan_pick(mode: EnvMode, current: bool) -> Pick {
    match (mode, current) {
        (EnvMode::Worktree, _) => Pick::Base,
        (EnvMode::Local, true) => Pick::Keep,
        (EnvMode::Local, false) => Pick::Checkout,
    }
}

pub fn switch_payload(cwd: &str, name: &str) -> Value {
    json!({"cwd": cwd, "refName": name})
}

pub fn create_payload(cwd: &str, name: &str) -> Value {
    json!({"cwd": cwd, "refName": name, "switchRef": true})
}

pub fn list_payload(cwd: &str, query: &str, cursor: Option<u64>) -> Value {
    let mut payload = json!({"cwd": cwd, "limit": REF_PAGE});
    if !query.is_empty() {
        payload["query"] = json!(query);
    }
    if let Some(cursor) = cursor {
        payload["cursor"] = json!(cursor);
    }
    payload
}

enum Pending {
    Refs { query: String, append: bool },
    Switch,
    Create,
}

pub struct BranchPicker {
    actions: ActionSender,
    cwd: String,
    mode: EnvMode,
    open: bool,
    search: Entity<InputState>,
    scroll: ScrollHandle,
    refs: Page,
    query: String,
    loading: bool,
    error: Option<String>,
    next_token: u64,
    pending: HashMap<u64, Pending>,
}

impl BranchPicker {
    pub fn new(actions: ActionSender, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search branches..."));
        cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.query_changed(cx);
            }
        })
        .detach();
        Self {
            actions,
            cwd: String::new(),
            mode: EnvMode::Local,
            open: false,
            search,
            scroll: ScrollHandle::new(),
            refs: Page::default(),
            query: String::new(),
            loading: false,
            error: None,
            next_token: TOKEN_BASE,
            pending: HashMap::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Takes the draft's directory and workspace mode; a new directory drops the loaded refs.
    pub fn sync(&mut self, cwd: &str, mode: EnvMode) {
        if self.cwd != cwd {
            self.cwd = cwd.to_owned();
            self.refs = Page::default();
            self.query.clear();
            self.error = None;
            self.open = false;
        }
        self.mode = mode;
    }

    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open = open;
        if open {
            self.query.clear();
            self.refs = Page::default();
            self.search
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.list_refs(false);
        }
        cx.notify();
    }

    /// What the popover draws; `create` only makes sense when the checkout moves.
    pub fn menu(&self, this: &Entity<Self>) -> RefMenu {
        let (on_pick, on_create, on_more) = (this.clone(), this.clone(), this.clone());
        RefMenu {
            query: self.query.clone(),
            page: self.refs.clone(),
            loading: self.loading,
            error: self.error.clone(),
            cwd: self.cwd.clone(),
            project_cwd: self.cwd.clone(),
            search: self.search.clone(),
            scroll: self.scroll.clone(),
            can_create: self.mode == EnvMode::Local,
            on_pick: Rc::new(move |name, current, cx| {
                on_pick.update(cx, |this, cx| this.pick(name, current, cx));
            }),
            on_create: Rc::new(move |name, cx| {
                on_create.update(cx, |this, cx| this.create(name, cx));
            }),
            on_more: Rc::new(move |cx| {
                on_more.update(cx, |this, cx| {
                    if !this.loading {
                        this.list_refs(true);
                        cx.notify();
                    }
                });
            }),
        }
    }

    fn call(&mut self, call: Call, payload: Value, pending: Pending) {
        self.next_token += 1;
        let token = self.next_token;
        self.pending.insert(token, pending);
        let action = Action::Call {
            token,
            call,
            payload,
        };
        if self.actions.try_send(action).is_err() {
            self.pending.remove(&token);
            eprintln!("[branch] could not send the request");
        }
    }

    fn list_refs(&mut self, append: bool) {
        let cursor = if append {
            let Some(cursor) = self.refs.next_cursor else {
                return;
            };
            Some(cursor)
        } else {
            None
        };
        self.loading = true;
        self.error = None;
        let query = self.query.clone();
        let payload = list_payload(&self.cwd, &query, cursor);
        self.call(Call::ListRefs, payload, Pending::Refs { query, append });
    }

    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_string();
        if query == self.query {
            return;
        }
        self.query = query;
        self.refs = Page::default();
        self.list_refs(false);
        cx.notify();
    }

    fn pick(&mut self, name: String, current: bool, cx: &mut Context<Self>) {
        self.open = false;
        match plan_pick(self.mode, current) {
            Pick::Keep => {}
            Pick::Base => {
                let is_remote = self
                    .refs
                    .refs
                    .iter()
                    .find(|entry| entry.name == name)
                    .map(|entry| entry.is_remote);
                cx.emit(Event::Base(Branch {
                    name: Some(name),
                    is_remote,
                }));
            }
            Pick::Checkout => {
                let payload = switch_payload(&self.cwd, &name);
                self.call(Call::SwitchRef, payload, Pending::Switch);
            }
        }
        cx.notify();
    }

    fn create(&mut self, name: String, cx: &mut Context<Self>) {
        self.open = false;
        let payload = create_payload(&self.cwd, &name);
        self.call(Call::CreateRef, payload, Pending::Create);
        cx.notify();
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        match event {
            CallEvent::Progress(_) => return,
            CallEvent::Done(value) => match self.pending.remove(&token) {
                Some(Pending::Refs { query, append }) if query == self.query => {
                    let page = Page::decode(&value);
                    if append {
                        self.refs.refs.extend(page.refs);
                        self.refs.total = page.total;
                        self.refs.has_more = page.has_more;
                        self.refs.next_cursor = page.next_cursor;
                    } else {
                        self.refs = page;
                    }
                    self.loading = false;
                }
                Some(Pending::Switch | Pending::Create) => cx.emit(Event::Switched),
                _ => {}
            },
            CallEvent::Failed(message) => match self.pending.remove(&token) {
                Some(Pending::Refs { query, .. }) if query == self.query => {
                    self.loading = false;
                    self.error = Some(message);
                }
                Some(Pending::Switch) => cx.emit(Event::Failed {
                    title: "Failed to switch ref.",
                    message,
                }),
                Some(Pending::Create) => cx.emit(Event::Failed {
                    title: "Failed to create and switch ref.",
                    message,
                }),
                _ => {}
            },
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worktree_draft_only_records_the_base() {
        assert_eq!(plan_pick(EnvMode::Worktree, false), Pick::Base);
        assert_eq!(plan_pick(EnvMode::Worktree, true), Pick::Base);
        assert_eq!(plan_pick(EnvMode::Local, true), Pick::Keep);
        assert_eq!(plan_pick(EnvMode::Local, false), Pick::Checkout);
    }

    #[test]
    fn payloads_match_the_vcs_requests() {
        assert_eq!(
            switch_payload("/p", "dev"),
            json!({"cwd": "/p", "refName": "dev"})
        );
        assert_eq!(
            create_payload("/p", "feat"),
            json!({"cwd": "/p", "refName": "feat", "switchRef": true})
        );
        assert_eq!(
            list_payload("/p", "", None),
            json!({"cwd": "/p", "limit": 100})
        );
        assert_eq!(
            list_payload("/p", "de", Some(7)),
            json!({"cwd": "/p", "limit": 100, "query": "de", "cursor": 7})
        );
    }
}
