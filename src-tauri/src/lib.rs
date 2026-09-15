pub mod bt;
pub mod config;
mod manager;

use manager::{ConnectionManager, ConnState};
use tauri::{
    AppHandle, Emitter, Manager, State,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use bt::protocol::AncMode;

pub struct AppState {
    pub manager: ConnectionManager,
}

#[tauri::command]
fn scan_buds(state: State<AppState>) -> Result<Vec<manager::DiscoveredBuds>, String> {
    state.manager.scan()
}

#[tauri::command]
fn connect_buds(
    app: AppHandle,
    state: State<AppState>,
    address: u64,
    name: String,
) -> Result<(), String> {
    let event_app = app.clone();
    let name_for_save = name.clone();
    match state.manager.connect(address, &name, move |event| {
        // Remember the device once authentication succeeds (for auto-reconnect).
        if matches!(event, bt::session::SessionEvent::Authenticated) {
            config::save_last_device(&config::LastDevice {
                address,
                name: name_for_save.clone(),
            });
        }
        let _ = event_app.emit("buds-event", event);
    }) {
        Ok(true) => Ok(()),
        Ok(false) => {
            // Already connected to this device: re-emit the current state so
            // the UI settles back to the connected view.
            let s = state.manager.state();
            let _ = app.emit(
                "buds-event",
                bt::session::SessionEvent::StateUpdated { state: s.state },
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
fn disconnect_buds(state: State<AppState>) -> Result<(), String> {
    state.manager.disconnect()
}

#[tauri::command]
fn set_anc(state: State<AppState>, mode: u8) -> Result<(), String> {
    let mode = AncMode::from_u8(mode).ok_or("invalid ANC mode")?;
    state.manager.set_anc(mode)
}

#[tauri::command]
fn connection_state(state: State<AppState>) -> ConnState {
    state.manager.state()
}

/// Try to reconnect to the last device in the background; emits normal
/// buds-events so the UI reflects progress.
fn auto_reconnect(app: AppHandle) {
    if let Some(dev) = config::load_last_device() {
        let event_app = app.clone();
        let _ = app
            .state::<AppState>()
            .manager
            .connect(dev.address, &dev.name, move |event| {
                let _ = event_app.emit("buds-event", event);
            });
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Xiaomi Earbuds", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Xiaomi Earbuds Desktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            manager: ConnectionManager::new(),
        })
        .setup(|app| {
            setup_tray(app.handle())?;
            // Auto-reconnect to the last buds, in the background.
            let handle = app.handle().clone();
            std::thread::spawn(move || auto_reconnect(handle));
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close-to-tray: closing the window keeps the app running.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan_buds,
            connect_buds,
            disconnect_buds,
            set_anc,
            connection_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
