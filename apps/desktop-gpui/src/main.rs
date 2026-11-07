mod assets;
mod auth;
mod branch_picker;
mod catalog;
mod config;
mod connection;
mod details;
mod details_card;
mod diff;
mod electron_dpop;
mod environments;
mod git_actions;
mod history;
mod minimap;
mod projects;
mod providers;
mod ref_picker;
mod refs;
mod rpc;
mod server;
mod shell;
mod slow_requests;
mod status;
mod strip;
mod terminal;
mod terminal_screen;
mod terminal_state;
mod terminal_view;
mod thread;
mod thread_card;
mod toast;
mod ui;
mod usage;
mod wire;

use assets::AppAssets;
use config::{Attach, Config};
use gpui::{
    App, AppContext as _, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, point,
    px, size,
};
use gpui_component::{Root, Theme, ThemeMode, TitleBar};
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
    let (action_tx, action_rx) = async_channel::unbounded();
    let attach = Attach::from_env();
    let supervisor = if attach.is_some() {
        None
    } else {
        Config::from_env().map(|config| {
            Rc::new(Supervisor::start(
                runtime.handle(),
                config,
                action_rx.clone(),
                status_tx.clone(),
            ))
        })
    };
    let initial = if let Some(attach) = attach {
        runtime.spawn(async move {
            let client = auth::http_client();
            match server::wait_until_ready(&client, &attach.http_base).await {
                Ok(environment) => {
                    // Only an attached server shares the Electron app's home, hence its catalog.
                    if let Some(path) = config::connection_catalog_path() {
                        tokio::spawn(environments::run(
                            environment.id.clone(),
                            path,
                            status_tx.clone(),
                        ));
                    }
                    let credential = auth::Credential::Pairing(attach.pairing_token);
                    connection::run(
                        &attach.http_base,
                        &credential,
                        &environment,
                        &action_rx,
                        &status_tx,
                    )
                    .await;
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

    application()
        .with_assets(AppAssets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            // The web's default `sidebar.toggle` and `navigation.*` bindings.
            cx.bind_keys([
                KeyBinding::new("cmd-b", ui::ToggleSidebar, None),
                KeyBinding::new("escape", ui::DismissSidebar, Some(ui::OVERLAY_CONTEXT)),
                KeyBinding::new("shift-cmd-a", ui::OpenPermissions, None),
                KeyBinding::new("shift-cmd-b", ui::ToggleDetails, None),
                KeyBinding::new("cmd-j", ui::ToggleTerminal, None),
                KeyBinding::new("alt-cmd-b", ui::ToggleRightPanel, None),
                KeyBinding::new("cmd-o", ui::OpenFavoriteEditor, None),
                // `chat.new` and `chat.newWithoutProject`; `terminal.new` wins while a terminal has focus.
                KeyBinding::new("cmd-n", ui::NewThread, None),
                KeyBinding::new("alt-cmd-n", ui::NewScratchThread, None),
                KeyBinding::new("cmd-[", ui::GoBack, None),
                KeyBinding::new("cmd-]", ui::GoForward, None),
                // The web's `terminal.*` defaults, which apply while the terminal has focus.
                KeyBinding::new("cmd-d", ui::SplitTerminal, Some("Terminal")),
                KeyBinding::new("shift-cmd-d", ui::SplitTerminalVertical, Some("Terminal")),
                KeyBinding::new("cmd-n", ui::NewTerminal, Some("Terminal")),
                KeyBinding::new("cmd-w", ui::CloseTerminal, Some("Terminal")),
            ]);
            if let Some(supervisor) = supervisor {
                // The grace period outlasts gpui's quit timeout, so block here instead of awaiting.
                cx.on_app_quit(move |_| {
                    supervisor.stop();
                    async {}
                })
                .detach();
            }
            // Electron's `DEFAULT_MAIN_WINDOW_SIZE`; the minimum width is the shipped app's, below
            // the sidebar's overlay breakpoint.
            let bounds = Bounds::centered(None, size(px(1100.), px(780.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(583.), px(620.))),
                    // Electron's `trafficLightPosition`: centered in the 52px workspace top bar.
                    titlebar: Some(TitlebarOptions {
                        traffic_light_position: Some(point(px(16.), px(19.))),
                        ..TitleBar::title_bar_options()
                    }),
                    ..TitleBar::window_options()
                },
                |window, cx| {
                    let view = cx.new(|cx| {
                        cx.spawn(async move |this, cx| {
                            while let Ok(status) = status_rx.recv().await {
                                let updated = this.update(cx, |view: &mut MainView, cx| {
                                    view.set_status(status, cx)
                                });
                                if updated.is_err() {
                                    break;
                                }
                            }
                        })
                        .detach();
                        MainView::new(initial, action_tx, window, cx)
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open the main window");
            cx.activate(true);
        });
    drop(runtime);
}
