//! Win32 窗口操作：HWND 缓存、显隐、移动、圆角裁剪。

use std::sync::OnceLock;

use super::position::{PhysicalPos, is_position_on_screen, load_position, save_position};
use super::WINDOW_TITLE;

// ──────────────────────── HWND 缓存 ────────────────────────

/// 缓存的主窗口句柄（以 isize 存储，裸指针不满足 Send/Sync）。
/// 窗口只创建一次，句柄在进程生命周期内不变，首次查找后永久缓存。
static MAIN_HWND: OnceLock<isize> = OnceLock::new();

/// 按标题查找窗口句柄（首次查找后走缓存）。
#[cfg(target_os = "windows")]
fn main_hwnd() -> Option<*mut std::ffi::c_void> {
    if let Some(&h) = MAIN_HWND.get() {
        return Some(h as *mut std::ffi::c_void);
    }
    let h = find_by_title()?;
    // 窗口只创建一次，安全缓存
    let _ = MAIN_HWND.set(h as isize);
    Some(h)
}

#[cfg(target_os = "windows")]
fn find_by_title() -> Option<*mut std::ffi::c_void> {
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;
    let wide: Vec<u16> = WINDOW_TITLE.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = FindWindowW(std::ptr::null(), wide.as_ptr());
        if h.is_null() { None } else { Some(h) }
    }
}

#[cfg(not(target_os = "windows"))]
fn main_hwnd() -> Option<*mut std::ffi::c_void> { None }

// ──────────────────────── 窗口位置读取 ────────────────────────

/// 读取当前窗口位置（通过缓存的 HWND 定位）。隐藏中的窗口也能拿到正确坐标。
#[cfg(target_os = "windows")]
pub fn current_window_position() -> Option<PhysicalPos> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let h = main_hwnd()?;
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(h, &mut rect) != 0 {
            Some(PhysicalPos::new(rect.left, rect.top))
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn current_window_position() -> Option<PhysicalPos> { None }

/// 在隐藏/退出等时机保存当前窗口位置。
pub fn save_current_position() {
    if let Some(pos) = current_window_position() {
        save_position(pos);
    }
}

// ──────────────────────── 窗口显隐 ────────────────────────

/// 显示主窗口并恢复位置、前置聚焦。
#[cfg(target_os = "windows")]
pub fn show_main_window() {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, GetWindowRect, SetForegroundWindow, SetWindowPos, ShowWindow, SM_CXSCREEN,
        SW_SHOW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    };

    let h = match main_hwnd() {
        Some(h) => h,
        None => return,
    };
    unsafe {
        // 恢复上次位置；离屏或无记录则水平居中、距顶 200px
        let restored = if let Some(pos) = load_position() {
            if is_position_on_screen(pos) {
                SetWindowPos(
                    h, std::ptr::null_mut(), pos.x, pos.y, 0, 0,
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
                let x = if screen_w > win_w { (screen_w - win_w) / 2 } else { 0 };
                SetWindowPos(
                    h, std::ptr::null_mut(), x, 200, 0, 0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        let _ = ShowWindow(h, SW_SHOW);
        let _ = SetForegroundWindow(h);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn show_main_window() {}

/// 隐藏主窗口并保存当前位置。
#[cfg(target_os = "windows")]
pub fn hide_main_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    let h = match main_hwnd() {
        Some(h) => h,
        None => return,
    };
    unsafe {
        if let Some(pos) = current_window_position() {
            save_position(pos);
        }
        let _ = ShowWindow(h, SW_HIDE);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn hide_main_window() {}

/// 查询主窗口当前是否可见（以 Win32 实际状态为准）。
#[cfg(target_os = "windows")]
pub fn is_main_window_visible() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    match main_hwnd() {
        Some(h) => unsafe { IsWindowVisible(h) != 0 },
        None => false,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_main_window_visible() -> bool { false }

// ──────────────────────── 窗口移动 ────────────────────────

/// 移动窗口到指定物理坐标（不改变尺寸）。
#[cfg(target_os = "windows")]
pub fn set_window_position(pos: PhysicalPos) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
    const SWP_NOSIZE: u32 = 0x0001;
    if let Some(h) = main_hwnd() {
        unsafe {
            SetWindowPos(
                h, std::ptr::null_mut(), pos.x, pos.y, 0, 0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_window_position(_pos: PhysicalPos) {}

// ──────────────────────── 圆角裁剪 ────────────────────────

/// 设置窗口可见区域为圆角矩形（物理坐标）。不透明窗口下用其实现圆角裁剪，
/// 避免 transparent 带来的 resize 抖动与初始化闪烁。
#[cfg(target_os = "windows")]
pub fn set_window_region(w_logical: f32, h_logical: f32, scale: f32) {
    use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
    if let Some(h) = main_hwnd() {
        let w = (w_logical * scale).round() as i32;
        let hh = (h_logical * scale).round() as i32;
        let r = (12.0 * scale).round().max(1.0) as i32;
        unsafe {
            let rgn = CreateRoundRectRgn(0, 0, w + 1, hh + 1, r, r);
            SetWindowRgn(h, rgn, 1);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_window_region(_w_logical: f32, _h_logical: f32, _scale: f32) {}
