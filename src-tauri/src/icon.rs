//! 应用图标提取与 `appicon://` 自定义协议。
//!
//! 设计参考 ZTools 的"路径即图标 + 懒加载协议流 + 内存 LRU + 串行提取"方案：
//! - 图标 PNG 数据不落盘，只驻留主进程内存的 LRU Map（上限 128）。
//! - 提取走串行锁，规避原生 API 在高并发下的潜在问题。
//! - 前端用普通 `<img src="appicon://icon/<encodeURIComponent(应用路径)>">` 触发提取。

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

/// LRU 容量上限（与 ZTools 一致）。
const MAX_ICON_CACHE: usize = 128;

/// 全局 LRU 缓存：应用原始路径 → PNG 字节。
static CACHE: LazyLock<Mutex<LruCache>> = LazyLock::new(|| Mutex::new(LruCache::new(MAX_ICON_CACHE)));

/// 串行提取锁：保证同一时刻只有一个原生提取任务在执行。
static EXTRACT_LOCK: Mutex<()> = Mutex::new(());

/// 简单的 LRU 缓存。利用 VecDeque 维护访问顺序，HashMap 存放数据。
struct LruCache {
    map: HashMap<String, Vec<u8>>,
    order: VecDeque<String>,
    cap: usize,
}

impl LruCache {
    fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            cap,
        }
    }

    /// 命中时刷新顺序后返回克隆（PNG 体积不大，克隆可接受，避免持有锁跨 await/提取）。
    fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        if let Some(val) = self.map.get(key) {
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.to_string());
            Some(val.clone())
        } else {
            None
        }
    }

    fn put(&mut self, key: String, val: Vec<u8>) {
        if self.map.contains_key(&key) {
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.clone());
            self.map.insert(key, val);
            return;
        }
        if self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.map.insert(key.clone(), val);
        self.order.push_back(key);
    }
}

/// 对外入口：根据应用路径返回 PNG 字节（带缓存）。
pub fn get_icon(path: &str) -> Option<Vec<u8>> {
    // 1. 快速路径：命中缓存直接返回
    if let Some(b) = CACHE.lock().ok().and_then(|mut c| c.get(path)) {
        return Some(b);
    }

    // 2. 串行提取
    let _guard = EXTRACT_LOCK.lock().ok()?;

    // 拿到锁后再次检查缓存（防止并发重复提取）
    if let Some(b) = CACHE.lock().ok().and_then(|mut c| c.get(path)) {
        return Some(b);
    }

    let png = extract_icon_png(path)?;

    if let Ok(mut c) = CACHE.lock() {
        c.put(path.to_string(), png.clone());
    }
    Some(png)
}

// ───────────────────────────── 平台实现 ─────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER,
    };
    use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, ShellLink, SLGP_RAWPATH};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    // SHGetFileInfoW 标志
    const SHGFI_ICON: u32 = 0x0000_0100;
    const SHGFI_LARGEICON: u32 = 0x0000_0000;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
    const DIB_RGB_COLORS: u32 = 0;

    type HIcon = *mut c_void;

    /// 提取应用图标并编码为 PNG。失败返回 None。
    pub fn extract_icon_png(path: &str) -> Option<Vec<u8>> {
        // 对 .lnk 快捷方式：先解析出目标路径，再从目标提取图标，
        // 以避免快捷方式图标上叠加的 Windows 小箭头。
        let effective = if path.to_ascii_lowercase().ends_with(".lnk") {
            resolve_lnk_target(path).unwrap_or_else(|| path.to_string())
        } else {
            path.to_string()
        };
        unsafe {
            let hicon = get_hicon(&effective)?;
            let png = hicon_to_png(hicon);
            DestroyIcon(hicon);
            png
        }
    }

    unsafe fn get_hicon(path: &str) -> Option<HIcon> {
        // 规范化路径分隔符：Win32 Shell API（SHGetFileInfoW）对正斜杠不宽容
        let normalized = path.replace('/', "\\");
        let mut wide: Vec<u16> = normalized.encode_utf16().collect();
        wide.push(0);
        let mut shfi: SHFILEINFOW = zeroed();
        let cb = size_of::<SHFILEINFOW>() as u32;
        let ret = SHGetFileInfoW(
            wide.as_ptr(),
            FILE_ATTRIBUTE_NORMAL,
            &mut shfi,
            cb,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if ret == 0 || shfi.hIcon.is_null() {
            None
        } else {
            Some(shfi.hIcon)
        }
    }

    unsafe fn hicon_to_png(hicon: HIcon) -> Option<Vec<u8>> {
        let mut info: ICONINFO = zeroed();
        if GetIconInfo(hicon, &mut info) == 0 {
            return None;
        }
        let hbm_color = info.hbmColor;
        let hbm_mask = info.hbmMask;
        let color_null = hbm_color.is_null();

        // 从颜色位图（无颜色则为掩码位图）读取尺寸
        let dim_src = if !color_null { hbm_color } else { hbm_mask };
        let mut bmp: BITMAP = zeroed();
        let got_obj = GetObjectW(
            dim_src,
            size_of::<BITMAP>() as i32,
            &mut bmp as *mut _ as *mut c_void,
        );
        if got_obj == 0 {
            cleanup_bitmaps(hbm_color, hbm_mask);
            return None;
        }
        let w = bmp.bmWidth as usize;
        let h = bmp.bmHeight as usize;
        if w == 0 || h == 0 || color_null {
            cleanup_bitmaps(hbm_color, hbm_mask);
            return None;
        }

        let hdc = CreateCompatibleDC(std::ptr::null_mut());

        // 1) 读取 32bpp 颜色位图（BGRA，top-down）
        let mut color = vec![0u8; w * h * 4];
        let mut bi: BITMAPINFO = zeroed();
        bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w as i32;
        bi.bmiHeader.biHeight = -(h as i32); // 负值 = top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let color_got = GetDIBits(
            hdc,
            hbm_color,
            0,
            h as u32,
            color.as_mut_ptr() as *mut c_void,
            &mut bi,
            DIB_RGB_COLORS,
        );

        // 2) 读取 1bpp 掩码位图（用于无 alpha 通道的旧式图标判定透明度）
        let mut mask: Vec<u8> = Vec::new();
        let row_bytes = ((w + 31) / 32) * 4; // 每行字节数（DWORD 对齐）
        let mut mask_ok = false;
        if !hbm_mask.is_null() {
            mask = vec![0u8; row_bytes * h];
            let mut mbi: BITMAPINFO = zeroed();
            mbi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            mbi.bmiHeader.biWidth = w as i32;
            mbi.bmiHeader.biHeight = -(h as i32);
            mbi.bmiHeader.biPlanes = 1;
            mbi.bmiHeader.biBitCount = 1;
            let m = GetDIBits(
                hdc,
                hbm_mask,
                0,
                h as u32,
                mask.as_mut_ptr() as *mut c_void,
                &mut mbi,
                DIB_RGB_COLORS,
            );
            mask_ok = m != 0;
        }

        DeleteDC(hdc);
        cleanup_bitmaps(hbm_color, hbm_mask);

        if color_got == 0 {
            return None;
        }

        // 判定 alpha 来源：
        // - 现代图标（32bpp 且 alpha 通道有非零值）→ 直接信任颜色位图的 alpha；
        // - 旧式图标（alpha 全为 0）→ 从 1bpp 掩码推导透明度；
        // - 都不可用时 → 默认完全不透明。
        // 注意：早期实现把 alpha=0 强制改写为 255，导致透明像素显示成黑色，此处已修复。
        let alpha_sum: u32 = (0..w * h).map(|i| color[i * 4 + 3] as u32).sum();
        let trust_alpha = alpha_sum > 0;
        let use_mask = !trust_alpha && mask_ok;

        // 组装 RGBA
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let mut b = color[i * 4];
                let mut g = color[i * 4 + 1];
                let mut r = color[i * 4 + 2];
                let a = if trust_alpha {
                    color[i * 4 + 3]
                } else if use_mask {
                    // 掩码位 1 = 透明
                    let byte = mask[y * row_bytes + (x >> 3)];
                    let bit = (byte >> (7 - (x & 7))) & 1;
                    if bit == 1 {
                        0
                    } else {
                        255
                    }
                } else {
                    255
                };
                // Win32 图标位图通常为预乘 alpha，PNG 需要 straight alpha，故做反预乘。
                if trust_alpha && a > 0 && a < 255 {
                    let inv = 255.0 / a as f32;
                    r = (r as f32 * inv).min(255.0) as u8;
                    g = (g as f32 * inv).min(255.0) as u8;
                    b = (b as f32 * inv).min(255.0) as u8;
                }
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            }
        }

        encode_png(w as u32, h as u32, &rgba)
    }

    unsafe fn cleanup_bitmaps(color: *mut c_void, mask: *mut c_void) {
        if !color.is_null() {
            DeleteObject(color);
        }
        if !mask.is_null() {
            DeleteObject(mask);
        }
    }

    // ── COM .lnk 目标解析 ──
    // windows-sys 未导出 IShellLinkW / IPersistFile 接口，这里手写最小 vtable。
    use windows_sys::core::{GUID, HRESULT};
    use windows_sys::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };

    const STGM_READ: u32 = 0;
    const CLSCTX_INPROC: u32 = CLSCTX_INPROC_SERVER;
    const COINIT_STA: i32 = COINIT_APARTMENTTHREADED;
    // 解析失败时 Shell 仍可能返回成功但空串；RPC_E_CHANGED_MODE 等
    const SLGP_FLAGS_RAW: i32 = SLGP_RAWPATH;

    // IID_IShellLinkW = {000214F9-0000-0000-C000-000000000046}
    const IID_ISHELL_LINK_W: GUID = GUID::from_u128(0x000214F9_0000_0000_C000_000000000046);
    // IID_IPersistFile = {0000010B-0000-0000-C000-000000000046}
    const IID_IPERSIST_FILE: GUID = GUID::from_u128(0x0000010B_0000_0000_C000_000000000046);

    #[repr(C)]
    #[allow(non_snake_case)]
    struct IUnknownVtbl {
        QueryInterface:
            unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
        AddRef: unsafe extern "system" fn(*mut c_void) -> u32,
        Release: unsafe extern "system" fn(*mut c_void) -> u32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct IShellLinkWVtbl {
        parent: IUnknownVtbl,
        // 第 4 个方法：GetPath(This, pszFile, cch, pfd, fFlags)
        GetPath: unsafe extern "system" fn(*mut c_void, *mut u16, i32, *mut c_void, i32) -> HRESULT,
        // 后续方法不声明（不会被调用）
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct IShellLinkW {
        lpVtbl: *const IShellLinkWVtbl,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct IPersistFileVtbl {
        parent: IUnknownVtbl,
        GetClassID: unsafe extern "system" fn(*mut c_void, *mut GUID) -> HRESULT,
        IsDirty: unsafe extern "system" fn(*mut c_void) -> HRESULT,
        // 第 6 个方法：Load(This, pszFileName, dwMode)
        Load: unsafe extern "system" fn(*mut c_void, *const u16, u32) -> HRESULT,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct IPersistFile {
        lpVtbl: *const IPersistFileVtbl,
    }

    /// 展开 `%SystemRoot%`、`%ProgramFiles%` 等环境变量。
    fn expand_environment_strings(s: &str) -> String {
        use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
        unsafe {
            let mut wide: Vec<u16> = s.encode_utf16().collect();
            wide.push(0);
            let mut buf = vec![0u16; 32768];
            let n = ExpandEnvironmentStringsW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32);
            if n == 0 {
                return s.to_string();
            }
            let len = (n as usize).min(buf.len());
            let actual = buf[..len].iter().position(|&c| c == 0).unwrap_or(len);
            if actual == 0 {
                s.to_string()
            } else {
                String::from_utf16_lossy(&buf[..actual])
            }
        }
    }

    /// 在干净的 STA 线程里解析 .lnk 目标路径，规避宿主线程 COM 套间不确定的问题。
    fn resolve_lnk_target(lnk: &str) -> Option<String> {
        let lnk = lnk.to_string();
        let handle = std::thread::spawn(move || unsafe {
            // 在新线程上以 STA 初始化 COM；返回值忽略（已初始化/套间冲突时解析会自动失败回退）。
            let _ = CoInitializeEx(std::ptr::null(), COINIT_STA as u32);
            let result = resolve_lnk_target_inner(&lnk);
            CoUninitialize();
            result
        });
        handle.join().ok().flatten()
    }

    unsafe fn resolve_lnk_target_inner(lnk: &str) -> Option<String> {
        let mut psl: *mut c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &ShellLink,
            std::ptr::null_mut(),
            CLSCTX_INPROC,
            &IID_ISHELL_LINK_W,
            &mut psl,
        );
        if hr != 0 || psl.is_null() {
            return None;
        }
        let sl = psl as *const IShellLinkW;
        let vt = (*sl).lpVtbl;

        // QueryInterface -> IPersistFile
        let mut ppf: *mut c_void = std::ptr::null_mut();
        let hr = ((*vt).parent.QueryInterface)(psl, &IID_IPERSIST_FILE, &mut ppf);
        if hr != 0 || ppf.is_null() {
            ((*vt).parent.Release)(psl);
            return None;
        }
        let pf = ppf as *const IPersistFile;
        let pvt = (*pf).lpVtbl;

        // Load(.lnk)
        let mut wide: Vec<u16> = lnk.encode_utf16().collect();
        wide.push(0);
        let hr = ((*pvt).Load)(ppf, wide.as_ptr(), STGM_READ);
        let target = if hr == 0 {
            // GetPath
            let mut buf = [0u16; 1040];
            let mut find_data: [u8; 592] = [0; 592]; // WIN32_FIND_DATAW 占位（不读取）
            let hr = ((*vt).GetPath)(
                psl,
                buf.as_mut_ptr(),
                buf.len() as i32,
                find_data.as_mut_ptr() as *mut c_void,
                SLGP_FLAGS_RAW,
            );
            if hr == 0 {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                if len > 0 {
                    Some(String::from_utf16_lossy(&buf[..len]))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        ((*pvt).parent.Release)(ppf);
        ((*vt).parent.Release)(psl);
        // 仅返回存在的真实文件目标（避免拿到环境变量占位路径）
        if let Some(t) = target {
            // 系统快捷方式的目标常含 %SystemRoot% 等环境变量，必须展开后才能
            // 通过存在性校验并供 SHGetFileInfo 正确取图标。
            let expanded = expand_environment_strings(&t);
            let cleaned = expanded.replace('/', "\\");
            if std::path::Path::new(&cleaned).exists() {
                Some(cleaned)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(rgba).ok()?;
        }
        Some(out)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// 验证开始菜单里的系统快捷方式能解析到真实目标文件（含 %SystemRoot% 等环境变量展开），
        /// 这是去除系统应用图标小箭头的关键路径。
        #[test]
        fn resolve_system_shortcut_target() {
            let apps = crate::app_launcher::scan_cached();
            let mut checked = 0;
            let mut resolved = 0;
            for a in apps.iter().take(200) {
                let lower = a.path.to_ascii_lowercase();
                if !lower.ends_with(".lnk") || !lower.contains("start menu") {
                    continue;
                }
                checked += 1;
                if let Some(t) = resolve_lnk_target(&a.path) {
                    assert!(
                        !t.eq_ignore_ascii_case(&a.path),
                        "解析到的目标不应是 .lnk 本身: {}",
                        a.path
                    );
                    assert!(std::path::Path::new(&t).exists(), "目标文件应存在: {}", t);
                    resolved += 1;
                }
                if checked >= 8 {
                    break;
                }
            }
            println!("[icon test] 系统快捷方式目标解析: 检查 {} 个, 成功 {}", checked, resolved);
            assert!(
                resolved > 0 || checked == 0,
                "应至少有一个系统快捷方式能解析到真实目标（验证环境变量展开生效）"
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    /// 非 Windows 平台暂未实现原生图标提取，返回 None 由前端降级为占位图标。
    pub fn extract_icon_png(_path: &str) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(target_os = "windows")]
pub use platform::extract_icon_png;
#[cfg(not(target_os = "windows"))]
pub use platform::extract_icon_png;

use percent_encoding::percent_decode_str;

/// 处理 `appicon://` 请求：解析出应用路径，返回 PNG 响应或 404。
pub fn handle_request(path_part: &str) -> (u16, &'static str, Option<Vec<u8>>) {
    // path_part 形如 "/C%3A%5C...bar.lnk"，去掉前导 '/'
    let raw = path_part.trim_start_matches('/');
    let decoded = percent_decode_str(raw).decode_utf8_lossy();
    match get_icon(&decoded) {
        Some(png) => (200, "image/png", Some(png)),
        None => (404, "text/plain", None),
    }
}
