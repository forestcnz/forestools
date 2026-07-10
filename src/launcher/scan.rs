//! 应用扫描（带缓存）。Windows 扫描开始菜单与桌面 `.lnk`，macOS 扫描 `/Applications`。

use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use super::model::AppInfo;

/// 扫描结果缓存（首次扫描后驻留，避免每次唤起都重扫）。
static CACHE: LazyLock<Mutex<Option<Vec<AppInfo>>>> = LazyLock::new(|| Mutex::new(None));

/// 需要跳过的快捷方式名称关键词（卸载、帮助、文档等）。
fn should_skip(name: &str) -> bool {
    let lower = name.to_lowercase();
    const PATTERNS: &[&str] = &[
        "uninstall",
        "卸载",
        "website",
        "网站",
        "帮助",
        "help",
        "readme",
        "文档",
        "manual",
        "license",
        "documentation",
    ];
    PATTERNS.iter().any(|p| lower.contains(p))
}

/// 扫描已安装应用（带缓存）。
pub fn scan_cached() -> Vec<AppInfo> {
    {
        if let Ok(guard) = CACHE.lock() {
            if let Some(cached) = guard.clone() {
                return cached;
            }
        }
    }
    let apps = scan_inner();
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(apps.clone());
    }
    apps
}

#[cfg(target_os = "windows")]
fn scan_inner() -> Vec<AppInfo> {
    let mut apps: Vec<AppInfo> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut roots = Vec::new();
    if let Ok(program_data) = std::env::var("ProgramData") {
        roots.push(PathBuf::from(program_data).join("Microsoft/Windows/Start Menu/Programs"));
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        roots.push(PathBuf::from(app_data).join("Microsoft/Windows/Start Menu/Programs"));
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        roots.push(PathBuf::from(home).join("Desktop"));
    }

    for root in &roots {
        walk_lnks(root, &mut apps, &mut seen);
    }

    apps
}

#[cfg(target_os = "windows")]
fn walk_lnks(dir: &PathBuf, apps: &mut Vec<AppInfo>, seen: &mut std::collections::HashSet<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_lnks(&path, apps, seen);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        if ext != "lnk" {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.trim().to_string(),
            None => continue,
        };
        if stem.is_empty() || should_skip(&stem) {
            continue;
        }
        let path_str = path.to_string_lossy().to_string();

        // 优先取 Shell 本地化显示名（中文版 Windows 上系统应用为"控制中心"等），
        // 取不到时回退到 .lnk 文件名。文件名作为搜索别名保留，中英文都能搜到。
        let display = shell_display_name(&path_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let (name, aliases) = match display {
            Some(d) if d != stem => (d, vec![stem.clone()]),
            other => (other.unwrap_or_else(|| stem.clone()), Vec::new()),
        };
        if name.is_empty() || should_skip(&name) {
            continue;
        }
        let key = name.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        apps.push(AppInfo {
            name,
            path: path_str,
            aliases,
        });
    }
}

/// 通过 SHGetFileInfoW(SHGFI_DISPLAYNAME) 获取 Shell 显示名（含本地化解析）。
#[cfg(target_os = "windows")]
fn shell_display_name(path: &str) -> Option<String> {
    use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW};
    const SHGFI_DISPLAYNAME: u32 = 0x0000_0200;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
    unsafe {
        let normalized = path.replace('/', "\\");
        let mut wide: Vec<u16> = normalized.encode_utf16().collect();
        wide.push(0);
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        let cb = std::mem::size_of::<SHFILEINFOW>() as u32;
        let r = SHGetFileInfoW(
            wide.as_ptr(),
            FILE_ATTRIBUTE_NORMAL,
            &mut shfi,
            cb,
            SHGFI_DISPLAYNAME,
        );
        if r == 0 {
            return None;
        }
        let len = shfi
            .szDisplayName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(shfi.szDisplayName.len());
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&shfi.szDisplayName[..len]))
    }
}

#[cfg(target_os = "macos")]
fn scan_inner() -> Vec<AppInfo> {
    let mut apps: Vec<AppInfo> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut roots = vec![PathBuf::from("/Applications"), PathBuf::from("/System/Applications")];
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }

    for root in &roots {
        let entries = match fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if !name.ends_with(".app") {
                continue;
            }
            let stem = name.trim_end_matches(".app").to_string();
            if stem.is_empty() || should_skip(&stem) {
                continue;
            }
            let key = stem.to_lowercase();
            if !seen.insert(key) {
                continue;
            }
            apps.push(AppInfo {
                name: stem,
                path: path.to_string_lossy().to_string(),
                aliases: Vec::new(),
            });
        }
    }

    apps
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn scan_inner() -> Vec<AppInfo> {
    Vec::new()
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    /// 验证：扫描能拿到应用，且首个应用图标提取产生有效 PNG（PNG 魔数头）。
    #[test]
    fn scan_and_extract_icon() {
        let apps = scan_cached();
        println!("[test] 扫描到 {} 个应用", apps.len());
        for a in apps.iter().take(5) {
            println!("  - {}  =>  {}", a.name, a.path);
        }
        if let Some(first) = apps.first() {
            let png = crate::icon::get_icon(&first.path);
            assert!(png.is_some(), "图标提取应成功: {}", first.path);
            let bytes = png.unwrap();
            // PNG 文件魔数：89 50 4E 47 0D 0A 1A 0A
            assert!(bytes.len() > 8, "PNG 数据过短");
            assert_eq!(&bytes[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "非合法 PNG");
            println!("[test] 图标 PNG 字节数: {}", bytes.len());

            // 解码并校验 alpha 通道确实保留了透明像素（修复前透明区域被改成不透明黑色）
            use std::io::Cursor;
            let decoder = png::Decoder::new(Cursor::new(&bytes));
            let mut reader = decoder.read_info().expect("PNG 解码失败");
            let info = reader.info().clone();
            let mut buf = vec![0u8; reader.output_buffer_size()];
            reader.next_frame(&mut buf).expect("PNG 帧解码失败");
            assert_eq!(info.color_type, png::ColorType::Rgba, "颜色类型应为 RGBA");
            let pixels = info.width as usize * info.height as usize;
            let (transparent, opaque) =
                (0..pixels).fold((0u32, 0u32), |(t, o), i| {
                    let a = buf[i * 4 + 3];
                    if a == 0 { (t + 1, o) } else if a == 255 { (t, o + 1) } else { (t, o) }
                });
            println!(
                "[test] {}x{} alpha: 透明像素={} 不透明像素={} 半透明像素={}",
                info.width, info.height, transparent, opaque, pixels as u32 - transparent - opaque
            );
            assert!(transparent > 0, "图标应含透明像素（圆角/边缘），但全为不透明 → alpha 通道被错误地填满");
        } else {
            println!("[test] 未扫描到应用（开始菜单可能为空），跳过图标校验");
        }
    }
}
