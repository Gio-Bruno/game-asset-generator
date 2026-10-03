#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod ui;
use forge_core::{Service, contract::Event, default_data_dir, error::Result};
use gpui::*;
use gpui_component::{Root, Theme, ThemeMode};
use serde_json::Value;
use std::{borrow::Cow, path::PathBuf, sync::mpsc};

pub struct Command {
    pub method: String,
    pub params: Value,
}
pub enum Response {
    Reply(String, Value, Result<Value>),
    Event(Event),
}
actions!(asset_forge, [Quit]);

fn main() {
    let root = std::env::var_os("ASSET_FORGE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(default_data_dir);
    let (commands, mut inbox) = tokio::sync::mpsc::unbounded_channel::<Command>();
    let (outbox, responses) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let service = match Service::open(root) {
                Ok(s) => s,
                Err(e) => {
                    let _ = outbox.send(Response::Reply(
                        "startup".into(),
                        serde_json::json!({}),
                        Err(e),
                    ));
                    return;
                }
            };
            let mut events = service.subscribe();
            let event_out = outbox.clone();
            tokio::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(e) => {
                            if event_out.send(Response::Event(e)).is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            let _ = event_out.send(Response::Event(Event {
                                kind: "RESYNC_REQUIRED".into(),
                                job_id: None,
                                session_id: None,
                                message: "Refreshing the library…".into(),
                            }));
                        }
                        Err(_) => break,
                    }
                }
            });
            let mut installing_update = false;
            while let Some(command) = inbox.recv().await {
                if command.method == "system/shutdown" {
                    break;
                }
                if command.method == "system/update/cancel" {
                    installing_update = false;
                    continue;
                }
                if installing_update {
                    let _ = outbox.send(Response::Reply(
                        command.method,
                        command.params,
                        Err(forge_core::error::ApiError::new(
                            "APP_UPDATING",
                            "Asset Forge is restarting to install an update.",
                        )),
                    ));
                    continue;
                }
                let result = service
                    .dispatch(&command.method, command.params.clone())
                    .await;
                if command.method == "system/update/ready" && result.is_ok() {
                    // Freeze subsequent UI commands until quit, or resume if launching fails.
                    installing_update = true;
                }
                let _ = outbox.send(Response::Reply(command.method, command.params, result));
            }
        });
    });
    let shutdown = commands.clone();
    Application::new().run(move |cx| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("secondary-q", Quit, None)]);
        cx.set_menus(vec![Menu {
            name: "Asset Forge".into(),
            items: vec![MenuItem::action("Quit Asset Forge", Quit)],
        }]);
        gpui_component::init(cx);
        Theme::change(ThemeMode::Light, None, cx);
        let theme = Theme::global_mut(cx);
        theme.font_family = "IBM Plex Sans".into();
        theme.font_size = px(14.);
        theme.radius = px(5.);
        theme.colors.background = rgb(0xf8f6f0).into();
        theme.colors.foreground = rgb(0x272d29).into();
        theme.colors.border = rgb(0xdadbd2).into();
        theme.colors.primary = rgb(0x2f5548).into();
        theme.colors.primary_foreground = rgb(0xffffff).into();
        theme.colors.primary_hover = rgb(0x3e6958).into();
        theme.colors.input = rgb(0xdadbd2).into();
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../../../assets/fonts/IBMPlexSans.ttf")),
                Cow::Borrowed(include_bytes!("../../../assets/fonts/Lora.ttf")),
            ])
            .expect("Bundled fonts must load");
        let bounds = Bounds::centered(None, size(px(1320.), px(860.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1050.), px(720.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Asset Forge".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let studio = cx.new(|cx| ui::Studio::new(commands, responses, window, cx));
                cx.new(|cx| Root::new(studio, window, cx))
            },
        )
        .expect("The desktop window must open");
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
    let _ = shutdown.send(Command {
        method: "system/shutdown".into(),
        params: serde_json::json!({}),
    });
    let _ = worker.join();
}
