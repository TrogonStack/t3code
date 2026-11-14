//! The Connections tab's T3 Connect account: sign in through the system browser and sign out.
//! The account lives in `crate::cloud`; this panel only shows its state and starts its flows.

use super::components::{group, row, section};
use super::devices_panel::widgets::{Variant, button, note, problem};
use super::*;
use crate::cloud::{self, CloudState};

pub struct ConnectionsPanel {
    state: CloudState,
    wide: bool,
}

impl ConnectionsPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut changes = cloud::service().subscribe();
        let state = changes.borrow_and_update().clone();
        cx.spawn(async move |this, cx| {
            while changes.changed().await.is_ok() {
                let state = changes.borrow_and_update().clone();
                if this
                    .update(cx, |panel, cx| {
                        panel.state = state;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self { state, wide: true }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }
}

impl Render for ConnectionsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let description = if state.signed_in {
            "Signed in. Environments on your account connect through T3 Connect when they are not reachable directly."
        } else if state.signing_in {
            "Finish signing in in your browser."
        } else {
            "Sign in to reach the environments on your T3 Connect account from this device."
        };
        let control = if state.signed_in {
            button("t3-connect-sign-out", Variant::Outline, true, cx)
                .child("Sign out")
                .on_click(|_, _, _| cloud::spawn_sign_out())
        } else {
            button(
                "t3-connect-sign-in",
                Variant::Primary,
                !state.signing_in,
                cx,
            )
            .child(if state.signing_in {
                "Waiting for browser"
            } else {
                "Sign in"
            })
            .when(!state.signing_in, |sign_in| {
                sign_in.on_click(|_, _, _| cloud::spawn_sign_in())
            })
        };
        section(
            "T3 Connect",
            group(
                vec![
                    row(
                        "Account",
                        description,
                        None,
                        control.into_any_element(),
                        self.wide,
                        cx,
                    )
                    .into_any_element(),
                ],
                cx,
            ),
        )
        .children(
            state
                .error
                .map(|message| div().px_4().child(problem(message, cx))),
        )
        .child(div().px_4().child(note(
            "Signing in opens your browser and returns here when it is done.",
            cx,
        )))
    }
}
