mod auth;
mod config;
mod connection;
mod rpc;
mod server;
mod shell;
mod status;
mod ui;
mod wire;

use config::{Attach, Config};
use gpui::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_component::{Root, Theme, ThemeMode, TitleBar};
use gpui_kit_assets::Assets;
use gpui_platform::application;
use server::Supervisor;
use status::Status;
use std::rc::Rc;
use ui::MainView;

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to start the async runtime");
    let (status_tx, status_rx) = async_channel::unbounded();
    let attach = Attach::from_env();
    let supervisor = if attach.is_some() {
        None
    } else {
        Config::from_env().map(|config| {
            Rc::new(Supervisor::start(
                runtime.handle(),
                config,
                status_tx.clone(),
            ))
        })
    };
    let initial = if let Some(attach) = attach {
        runtime.spawn(async move {
            let client = auth::http_client();
            match server::wait_until_ready(&client, &attach.http_base).await {
                Ok(label) => {
                    let credential = auth::Credential::Pairing(attach.pairing_token);
                    connection::run(&attach.http_base, &credential, &label, &status_tx).await;
                }
                Err(message) => status::publish(&status_tx, Status::Failed(message)),
            }
        });
        Status::Connecting
    } else if supervisor.is_some() {
        Status::Starting
    } else {
        Status::NotConnected
    };

    application().with_assets(Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
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
                ..TitleBar::window_options()
            },
            |window, cx| {
                let view = cx.new(|cx| {
                    cx.spawn(async move |this, cx| {
                        while let Ok(status) = status_rx.recv().await {
                            let updated = this
                                .update(cx, |view: &mut MainView, cx| view.set_status(status, cx));
                            if updated.is_err() {
                                break;
                            }
                        }
                    })
                    .detach();
                    MainView::new(initial, window, cx)
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("failed to open the main window");
        cx.activate(true);
    });
    drop(runtime);
}
