//! Win32 窗口控制：位置持久化、显示器枚举、窗口宽度与默认位置计算、圆角裁剪。
//!
//! 位置数据存于可执行文件同级 `data/position.json`。
//! 由于 egui 不直接暴露窗口外位置读取，改用 Win32 `FindWindowW` + `GetWindowRect`
//! 按窗口标题定位。首次查找后缓存 HWND（窗口只创建一次，不会失效）。

mod ops;
mod position;
mod screen;

/// 主搜索窗口的 Win32 标题（供 FindWindowW 定位），需与 main.rs 中 NativeOptions 标题一致。
pub const WINDOW_TITLE: &str = "神奇的海螺";

pub use ops::{
    current_window_position, hide_main_window, is_main_window_visible, load_position_logical,
    save_current_position, set_window_position, set_window_region, show_main_window,
};
pub use position::PhysicalPos;
pub use screen::compute_window_width;
