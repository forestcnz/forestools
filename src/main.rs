// 防止 release 模式下弹出控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod app_launcher;
mod icon;
mod search;
mod theme;
mod window_ctl;

use eframe::egui;

fn main() -> eframe::Result {
    // 窗口图标（嵌入 PNG，避免依赖运行时文件路径）
    let icon = load_window_icon();
    let win_width = window_ctl::compute_window_width();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(window_ctl::WINDOW_TITLE) // 供 FindWindowW 定位
        .with_decorations(false) // 无系统标题栏
        .with_transparent(true) // 透明背景（配合 clear_color alpha=0 实现圆角）
        .with_resizable(false) // 禁止用户调整
        .with_visible(true) // egui 始终认为可见（保证 update 持续运行）；实际显隐由 Win32 ShowWindow 控制
        .with_inner_size([win_width, app::MAX_WINDOW_HEIGHT]); // 固定尺寸（消除 resize 抖动）
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let opts = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "forestools",
        opts,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)) as Box<dyn eframe::App>)),
    )
}

/// 从嵌入的 PNG 资源加载窗口图标。
fn load_window_icon() -> Option<egui::IconData> {
    const ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");
    let img = image::load_from_memory(ICON_PNG).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}
