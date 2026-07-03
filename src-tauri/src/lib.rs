use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[derive(Serialize, Deserialize)]
struct WindowPosition {
    x: i32,
    y: i32,
}

fn data_dir(_app: &tauri::AppHandle) -> PathBuf {
    let exe = std::env::current_exe().expect("failed to get executable path");
    let dir = exe.parent().expect("failed to get parent directory");
    let data = dir.join("data");
    fs::create_dir_all(&data).ok();
    data
}

fn save_position(app: &tauri::AppHandle, x: i32, y: i32) {
    let path = data_dir(app).join("position.json");
    let pos = WindowPosition { x, y };
    if let Ok(content) = serde_json::to_string(&pos) {
        let _ = fs::write(&path, content);
    }
}

fn load_position(app: &tauri::AppHandle) -> Option<WindowPosition> {
    let path = data_dir(app).join("position.json");
    if path.exists() {
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

fn set_default_position(window: &tauri::WebviewWindow) {
    if let Some(monitor) = window.current_monitor().ok().flatten() {
        let mon_pos = monitor.position();
        let mon_size = monitor.size();
        if let Ok(win_size) = window.outer_size() {
            let x = mon_pos.x + (mon_size.width as i32 - win_size.width as i32) / 2;
            let y = mon_pos.y + 200;
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                let w = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        if let Ok(pos) = w.outer_position() {
                            save_position(&app_handle, pos.x, pos.y);
                        }
                    }
                });
            }

            if let (Ok(quit), Some(icon)) = (
                MenuItem::with_id(app, "quit", "退出", true, None::<&str>),
                app.default_window_icon(),
            ) {
                if let Ok(menu) = Menu::with_items(app, &[&quit]) {
                    let _ = TrayIconBuilder::new()
                        .icon(icon.clone())
                        .menu(&menu)
                        .on_menu_event(|app, event| {
                            if event.id.as_ref() == "quit" {
                                if let Some(window) = app.get_webview_window("main") {
                                    if let Ok(pos) = window.outer_position() {
                                        save_position(app, pos.x, pos.y);
                                    }
                                    let _ = window.close();
                                }
                                app.exit(0);
                            }
                        })
                        .build(app);
                }
            }

            if let Err(e) =
                app.global_shortcut()
                    .on_shortcut("Alt+Space", |app_handle, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            if let Some(window) = app_handle.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    if let Ok(pos) = window.outer_position() {
                                        save_position(app_handle, pos.x, pos.y);
                                    }
                                    let _ = window.hide();
                                } else {
                                    if let Some(pos) = load_position(app_handle) {
                                        let _ = window.set_position(tauri::PhysicalPosition::new(
                                            pos.x, pos.y,
                                        ));
                                    } else {
                                        set_default_position(&window);
                                    }
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    })
            {
                eprintln!("warning: failed to register Alt+Space: {e}");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
