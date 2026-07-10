//! 系统托盘构建（"显示" + "退出"菜单项）。

use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::window;

/// 构建系统托盘：菜单含"显示"与"退出"，tooltip 为窗口标题。
pub(crate) fn build(show_item: MenuItem, quit_item: MenuItem) -> Option<TrayIcon> {
    let menu = Menu::new();
    let _ = menu.append(&show_item);
    let _ = menu.append(&quit_item);
    let icon = load_icon();
    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(window::WINDOW_TITLE)
        .with_icon(icon)
        .build()
        .ok()
}

/// 从嵌入的 PNG 资源加载托盘图标。
fn load_icon() -> Icon {
    const TRAY_PNG: &[u8] = include_bytes!("../../icons/32x32.png");
    let fallback = Icon::from_rgba(vec![0u8; 4], 1, 1).unwrap_or_else(|_| {
        // 极端情况下构造一个 1x1 透明像素
        Icon::from_rgba(vec![0, 0, 0, 0], 1, 1).expect("1x1 icon")
    });
    let img = match image::load_from_memory(TRAY_PNG) {
        Ok(img) => img.into_rgba8(),
        Err(_) => return fallback,
    };
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).unwrap_or(fallback)
}
