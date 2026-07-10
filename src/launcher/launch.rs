//! 应用启动。

use super::model::LauncherError;

/// 启动应用。
#[cfg(target_os = "windows")]
pub fn open(path: &str) -> Result<(), LauncherError> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    unsafe {
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let mut file: Vec<u16> = path.encode_utf16().collect();
        file.push(0);
        // ShellExecuteW 返回 HINSTANCE (>32 视为成功)
        let ret = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL as i32,
        );
        if ret as usize > 32 {
            Ok(())
        } else {
            Err(LauncherError::ShellExecuteFailed(ret as isize))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn open(path: &str) -> Result<(), LauncherError> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| LauncherError::ShellExecuteFailed(0))
}
