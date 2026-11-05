mod auth;
mod config;
mod connection;
mod rpc;
mod server;
mod shell;
mod status;
mod wire;

use config::Config;
use gpui::{
    App, Bounds, Context, SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use gpui_platform::application;
use server::Supervisor;
use status::Status;
use std::rc::Rc;

struct StatusView {
    status: SharedString,
}

impl Render for StatusView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .bg(rgb(0x1a1a1a))
            .text_color(rgb(0xe6e6e6))
            .text_xl()
            .child(self.status.clone())
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to start the async runtime");
    let (status_tx, status_rx) = async_channel::unbounded();
    let supervisor = Config::from_env()
        .map(|config| Rc::new(Supervisor::start(runtime.handle(), config, status_tx)));
    let initial = if supervisor.is_some() {
        Status::Starting
    } else {
        Status::NotConnected
    };

    application().run(move |cx: &mut App| {
        if let Some(supervisor) = supervisor {
            // The grace period outlasts gpui's quit timeout, so block here instead of awaiting.
            cx.on_app_quit(move |_| {
                supervisor.stop();
                async {}
            })
            .detach();
        }
        let bounds = Bounds::centered(None, size(px(960.), px(640.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("T3 Code".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx| {
                    cx.spawn(async move |this, cx| {
                        while let Ok(status) = status_rx.recv().await {
                            let updated = this.update(cx, |view: &mut StatusView, cx| {
                                view.status = status.to_string().into();
                                cx.notify();
                            });
                            if updated.is_err() {
                                break;
                            }
                        }
                    })
                    .detach();
                    StatusView {
                        status: initial.to_string().into(),
                    }
                })
            },
        )
        .expect("failed to open the main window");
        cx.activate(true);
    });
    drop(runtime);
}
