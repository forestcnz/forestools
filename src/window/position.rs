//! 位置类型与持久化。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 物理像素坐标（Win32 API 使用）。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhysicalPos {
    pub x: i32,
    pub y: i32,
}

impl PhysicalPos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// 位置数据目录：可执行文件同级 `data/`。
pub fn data_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("failed to get executable path");
    let dir = exe.parent().expect("failed to get parent directory");
    let data = dir.join("data");
    fs::create_dir_all(&data).ok();
    data
}

/// Windows 在窗口最小化/隐藏时会将其位置报为 (-32000, -32000) 这种哨兵值，
/// 保存它会导致下次唤起窗口跑到屏幕外。这里统一过滤掉明显离屏的坐标。
fn is_valid_position(pos: PhysicalPos) -> bool {
    pos.x > -10_000 && pos.y > -10_000
}

pub fn save_position(pos: PhysicalPos) {
    if !is_valid_position(pos) {
        return;
    }
    let path = data_dir().join("position.json");
    if let Ok(content) = serde_json::to_string(&pos) {
        let _ = fs::write(&path, content);
    }
}

pub fn load_position() -> Option<PhysicalPos> {
    let path = data_dir().join("position.json");
    if path.exists() {
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

// ──────────────────────── 显示器枚举 ────────────────────────

/// 枚举所有显示器的物理像素矩形 (left, top, right, bottom)。
#[cfg(target_os = "windows")]
fn enumerate_monitors() -> Vec<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    let mut monitors: Vec<(i32, i32, i32, i32)> = Vec::new();
    let ptr = &mut monitors as *mut Vec<_>;
    unsafe extern "system" fn callback(
        _hmon: HMONITOR, _hdc: HDC, lprc: *mut RECT, data: LPARAM,
    ) -> i32 {
        let rects = data as *mut Vec<(i32, i32, i32, i32)>;
        if !lprc.is_null() && !rects.is_null() {
            let r = &*lprc;
            (*rects).push((r.left, r.top, r.right, r.bottom));
        }
        1 // TRUE
    }
    unsafe {
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(callback), ptr as LPARAM);
    }
    monitors
}

#[cfg(not(target_os = "windows"))]
fn enumerate_monitors() -> Vec<(i32, i32, i32, i32)> { Vec::new() }

/// 检查坐标是否落在某个真实显示器范围内（兜底：历史数据/显示器配置变更后离屏）。
pub fn is_position_on_screen(pos: PhysicalPos) -> bool {
    for (x0, y0, x1, y1) in enumerate_monitors() {
        if pos.x >= x0 && pos.x < x1 && pos.y >= y0 && pos.y < y1 {
            return true;
        }
    }
    false
}
