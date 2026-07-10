//! eframe::App 实现：搜索框 + 结果列表 + 键盘导航 + 窗口拖动 + 动态高度 + 图标懒加载。
//!
//! 事件流：后台线程接收全局快捷键/托盘菜单事件 → channel + `ctx.request_repaint()`
//! 唤醒主线程 `update`。窗口隐藏时也依赖此机制被唤醒以重新显示。

mod drag;
mod fonts;
mod state;
mod tray;
mod ui;
mod update;

pub use state::{App, MAX_WINDOW_HEIGHT};
