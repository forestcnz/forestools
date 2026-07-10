//! 窗口控制辅助：位置持久化、显示器枚举、窗口宽度与默认位置计算。
//!
//! 位置数据存于可执行文件同级 `data/position.json`（与原 Tauri 版一致）。
//! 由于 egui 不直接暴露窗口外位置读取，改用 Win32 `FindWindowW` + `GetWindowRect`
//! 按窗口标题定位，保证在隐藏/移动/退出等各时机都能拿到真实坐标。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 主搜索窗口的 Win32 标题（供 FindWindowW 定位），需与 main.rs 中 NativeOptions 标题一致。
pub const WINDOW_TITLE: &str = "forestools-launcher";

#[derive(Serialize, Deserialize)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
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
fn is_valid_position(x: i32, y: i32) -> bool {
    x > -10_000 && y > -10_000
}

pub fn save_position(x: i32, y: i32) {
    if !is_valid_position(x, y) {
        return;
    }
    let path = data_dir().join("position.json");
    let pos = WindowPosition { x, y };
    if let Ok(content) = serde_json::to_string(&pos) {
        let _ = fs::write(&path, content);
    }
}

/// 读取当前窗口位置（通过 Win32 标题定位）。隐藏中的窗口也能拿到正确坐标。
#[cfg(target_os = "windows")]
pub fn current_window_position() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect};

    let wide: Vec<u16> = WINDOW_TITLE.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd.is_null() {
            return None;
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) != 0 {
            Some((rect.left, rect.top))
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn current_window_position() -> Option<(i32, i32)> {
    None
}

/// 按标题查找主窗口句柄。
#[cfg(target_os = "windows")]
fn main_hwnd() -> Option<*mut std::ffi::c_void> {
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;
    let wide: Vec<u16> = WINDOW_TITLE.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = FindWindowW(std::ptr::null(), wide.as_ptr());
        if h.is_null() {
            None
        } else {
            Some(h)
        }
    }
}

/// 显示主窗口并恢复位置、前置聚焦。
#[cfg(target_os = "windows")]
pub fn show_main_window() {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, GetWindowRect, SetForegroundWindow, SetWindowPos, ShowWindow, SM_CXSCREEN,
        SW_SHOW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    };
    if let Some(h) = main_hwnd() {
        unsafe {
            // 恢复上次位置；离屏或无记录则水平居中、距顶 200px
            let restored = if let Some(pos) = load_position() {
                if is_position_on_screen(pos.x, pos.y) {
                    SetWindowPos(
                        h,
                        std::ptr::null_mut(),
                        pos.x,
                        pos.y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !restored {
                let mut rect: RECT = std::mem::zeroed();
                if GetWindowRect(h, &mut rect) != 0 {
                    let win_w = (rect.right - rect.left).max(1);
                    let screen_w = GetSystemMetrics(SM_CXSCREEN);
                    let x = if screen_w > win_w {
                        (screen_w - win_w) / 2
                    } else {
                        0
                    };
                    SetWindowPos(
                        h,
                        std::ptr::null_mut(),
                        x,
                        200,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            let _ = ShowWindow(h, SW_SHOW);
            let _ = SetForegroundWindow(h);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn show_main_window() {}

/// 隐藏主窗口并保存当前位置。
#[cfg(target_os = "windows")]
pub fn hide_main_window() {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, ShowWindow, SW_HIDE};
    if let Some(h) = main_hwnd() {
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(h, &mut rect) != 0 {
                save_position(rect.left, rect.top);
            }
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn hide_main_window() {}

/// 查询主窗口当前是否可见（以 Win32 实际状态为准）。
#[cfg(target_os = "windows")]
pub fn is_main_window_visible() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    if let Some(h) = main_hwnd() {
        unsafe { IsWindowVisible(h) != 0 }
    } else {
        false
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_main_window_visible() -> bool {
    false
}

/// 在隐藏/退出等时机保存当前窗口位置。
pub fn save_current_position() {
    if let Some((x, y)) = current_window_position() {
        save_position(x, y);
    }
}

pub fn load_position() -> Option<WindowPosition> {
    let path = data_dir().join("position.json");
    if path.exists() {
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

/// 枚举所有显示器的物理像素矩形 (left, top, right, bottom)。
#[cfg(target_os = "windows")]
fn enumerate_monitors() -> Vec<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    let mut monitors: Vec<(i32, i32, i32, i32)> = Vec::new();
    let ptr = &mut monitors as *mut Vec<_>;
    unsafe extern "system" fn callback(
        _hmon: HMONITOR,
        _hdc: HDC,
        lprc: *mut RECT,
        data: LPARAM,
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
fn enumerate_monitors() -> Vec<(i32, i32, i32, i32)> {
    Vec::new()
}

/// 检查坐标是否落在某个真实显示器范围内（兜底：历史数据/显示器配置变更后离屏）。
pub fn is_position_on_screen(x: i32, y: i32) -> bool {
    for (x0, y0, x1, y1) in enumerate_monitors() {
        if x >= x0 && x < x1 && y >= y0 && y < y1 {
            return true;
        }
    }
    false
}

/// 主屏逻辑像素宽度（物理像素按 DPI 折算）。
#[cfg(target_os = "windows")]
pub fn primary_screen_width_logical() -> f32 {
    use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};
    const LOGPIXELSX: i32 = 88;

    let screen_w_px = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    if screen_w_px <= 0 {
        return 480.0;
    }
    let dpi = unsafe {
        let hdc = GetDC(std::ptr::null_mut());
        let dpi = if hdc.is_null() {
            96
        } else {
            GetDeviceCaps(hdc, LOGPIXELSX)
        };
        ReleaseDC(std::ptr::null_mut(), hdc);
        dpi
    };
    if dpi <= 0 {
        screen_w_px as f32
    } else {
        screen_w_px as f32 * 96.0 / dpi as f32
    }
}

#[cfg(not(target_os = "windows"))]
pub fn primary_screen_width_logical() -> f32 {
    480.0
}

/// 按当前主屏宽度的 2/5 计算窗口宽度（与原 Tauri 版一致）。
pub fn compute_window_width() -> f32 {
    let sw = primary_screen_width_logical();
    if sw > 0.0 {
        (sw * 2.0 / 5.0).round()
    } else {
        480.0
    }
}
