//! DPI 与屏幕尺寸查询。

/// 查询主屏 DPI（每逻辑英寸像素数，96 = 100% 缩放）。
#[cfg(target_os = "windows")]
fn query_dpi() -> i32 {
    use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC};
    const LOGPIXELSX: i32 = 88;
    unsafe {
        let hdc = GetDC(std::ptr::null_mut());
        let dpi = if hdc.is_null() { 96 } else { GetDeviceCaps(hdc, LOGPIXELSX) };
        ReleaseDC(std::ptr::null_mut(), hdc);
        if dpi > 0 { dpi } else { 96 }
    }
}

#[cfg(not(target_os = "windows"))]
fn query_dpi() -> i32 { 96 }

/// 主屏 DPI 缩放比例（96 = 1.0）。
pub fn dpi_scale() -> f32 {
    query_dpi() as f32 / 96.0
}

/// 主屏物理像素宽度。
#[cfg(target_os = "windows")]
fn screen_width_px() -> i32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};
    unsafe { GetSystemMetrics(SM_CXSCREEN) }
}

#[cfg(not(target_os = "windows"))]
fn screen_width_px() -> i32 { 0 }

/// 主屏逻辑像素宽度（物理像素按 DPI 折算）。
pub fn primary_screen_width_logical() -> f32 {
    let px = screen_width_px();
    if px <= 0 {
        return 480.0;
    }
    px as f32 * 96.0 / query_dpi() as f32
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
