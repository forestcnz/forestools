use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

mod app_launcher;
mod icon;

#[derive(Serialize)]
struct ScanResult {
    apps: Vec<app_launcher::AppInfo>,
}

/// 扫描已安装应用（带缓存）。
#[tauri::command]
fn scan_apps() -> ScanResult {
    ScanResult {
        apps: app_launcher::scan_cached(),
    }
}

/// 启动指定路径的应用。
#[tauri::command]
fn open_app(path: String) -> Result<(), String> {
    app_launcher::open(&path)
}

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

/// Windows 在窗口最小化/隐藏时会将其位置报为 (-32000, -32000) 这种哨兵值，
/// 保存它会导致下次唤起窗口跑到屏幕外。这里统一过滤掉明显离屏的坐标。
fn is_valid_position(x: i32, y: i32) -> bool {
    x > -10_000 && y > -10_000
}

fn save_position(app: &tauri::AppHandle, x: i32, y: i32) {
    if !is_valid_position(x, y) {
        return;
    }
    let path = data_dir(app).join("position.json");
    let pos = WindowPosition { x, y };
    if let Ok(content) = serde_json::to_string(&pos) {
        let _ = fs::write(&path, content);
    }
}

/// 检查坐标是否落在某个真实显示器范围内，用于加载时兜底
/// （历史数据可能已存入 -32000 哨兵值，或显示器配置变更后原位置已离屏）。
fn is_position_on_screen(window: &tauri::WebviewWindow, x: i32, y: i32) -> bool {
    if let Ok(monitors) = window.available_monitors() {
        for m in monitors {
            let pos = m.position();
            let size = m.size();
            let x0 = pos.x;
            let y0 = pos.y;
            let x1 = pos.x + size.width as i32;
            let y1 = pos.y + size.height as i32;
            if x >= x0 && x < x1 && y >= y0 && y < y1 {
                return true;
            }
        }
    }
    false
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
        .register_uri_scheme_protocol("appicon", |_ctx, request| {
            let path_part = request.uri().path().to_string();
            let (status, content_type, body) = icon::handle_request(&path_part);
            let mut builder = tauri::http::Response::builder().status(status);
            if let Some(body) = body {
                builder = builder
                    .header("content-type", content_type)
                    .header("access-control-allow-origin", "*");
                builder.body(body).unwrap()
            } else {
                builder.body(Vec::new()).unwrap()
            }
        })
        .invoke_handler(tauri::generate_handler![scan_apps, open_app])
        .setup(|app| {
            let app_handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                let w = window.clone();
                window.on_window_event(move |event| {
                    match event {
                        // 窗口移动即保存最新位置，覆盖所有隐藏途径
                        // （全局快捷键、Esc、启动应用后 hide() 等），避免再唤起时位置回退。
                        tauri::WindowEvent::Moved(pos) => {
                            save_position(&app_handle, pos.x, pos.y);
                        }
                        tauri::WindowEvent::CloseRequested { .. } => {
                            if let Ok(pos) = w.outer_position() {
                                save_position(&app_handle, pos.x, pos.y);
                            }
                        }
                        _ => {}
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
                                    let restored = if let Some(pos) = load_position(app_handle) {
                                        if is_position_on_screen(&window, pos.x, pos.y) {
                                            let _ = window.set_position(tauri::PhysicalPosition::new(
                                                pos.x, pos.y,
                                            ));
                                            true
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };
                                    if !restored {
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
