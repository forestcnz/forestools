//! 平台特定的图标提取实现。
//!
//! Windows：通过 SHGetFileInfoW + COM vtable（IShellLinkW/IPersistFile）提取 .lnk 图标，
//! 三级回退（IconLocation → GetPath → PIDL）避免小箭头。
//! 非 Windows：暂未实现，返回 None。

#[cfg(target_os = "windows")]
mod win {
    use crate::icon::IconImage;
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
    const SHGFI_PIDL: u32 = 0x0000_0008;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
    const DIB_RGB_COLORS: u32 = 0;

    type HIcon = *mut c_void;

    /// 提取应用图标并返回 RGBA 像素。失败返回 None。
    pub fn extract_icon_rgba(path: &str) -> Option<IconImage> {
        // 对 .lnk 快捷方式：在干净 STA 线程上按 IconLocation / 目标 / PIDL 解析并提取图标，
        // 均失败时回退到 .lnk 本身。前三者都不带 Windows 快捷方式小箭头。
        let is_lnk = path.to_ascii_lowercase().ends_with(".lnk");
        unsafe {
            let hicon: Option<HIcon> = if is_lnk {
                resolve_lnk(path).or_else(|| get_hicon(path))
            } else {
                get_hicon(path)
            };
            let hicon = hicon?;
            let pixels = hicon_to_rgba(hicon);
            DestroyIcon(hicon);
            pixels
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

    unsafe fn hicon_to_rgba(hicon: HIcon) -> Option<IconImage> {
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
        let row_bytes = w.div_ceil(32) * 4; // 每行字节数（DWORD 对齐）
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
                // Win32 图标位图通常为预乘 alpha，PNG/RGBA 需要 straight alpha，故做反预乘。
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

        Some(IconImage { width: w as u32, height: h as u32, rgba })
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
        // 第 5 个方法：GetIDList(This, ppidl) —— 返回 PIDL（文件资源管理器等快捷方式用 PIDL 而非路径）
        GetIDList: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
        SetIDList: *mut c_void,
        GetDescription: *mut c_void,
        SetDescription: *mut c_void,
        GetWorkingDirectory: *mut c_void,
        SetWorkingDirectory: *mut c_void,
        GetArguments: *mut c_void,
        SetArguments: *mut c_void,
        // 第 12 个方法：GetIconLocation(This, pszIconPath, cch, piIcon)
        GetIconLocation: unsafe extern "system" fn(*mut c_void, *mut u16, i32, *mut i32) -> HRESULT,
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

    /// 在干净的 STA 线程里解析 .lnk 并直接提取图标，规避宿主线程 COM 套间不确定的问题。
    /// 返回的 HIcon 跨线程可用（GDI 全局句柄），由调用方负责 DestroyIcon。
    fn resolve_lnk(lnk: &str) -> Option<HIcon> {
        let lnk = lnk.to_string();
        let handle = std::thread::spawn(move || unsafe {
            // 在新线程上以 STA 初始化 COM；返回值忽略（已初始化/套间冲突时解析会自动失败回退）。
            let _ = CoInitializeEx(std::ptr::null(), COINIT_STA as u32);
            let result = resolve_lnk_inner(&lnk);
            CoUninitialize();
            // HIcon（裸指针）非 Send，经 usize 中转跨线程。
            result.map(|h| h as usize)
        });
        handle.join().ok().flatten().map(|u| u as *mut c_void)
    }

    /// 校验路径（展开环境变量、规范分隔符），存在则返回规范化后的绝对路径。
    unsafe fn validate_path(raw: &str) -> Option<String> {
        let expanded = expand_environment_strings(raw);
        let cleaned = expanded.replace('/', "\\");
        if cleaned.is_empty() {
            return None;
        }
        // 去掉可能的 ",index" 后缀（IconLocation 偶尔带），保留纯路径
        let path_only = cleaned.split(',').next().unwrap_or("").trim().to_string();
        if std::path::Path::new(&path_only).exists() {
            Some(path_only)
        } else {
            None
        }
    }

    /// 通过 SHGetFileInfoW(SHGFI_PIDL) 从 PIDL 取图标（用于文件资源管理器这类
    /// 以 PIDL 而非文件路径为目标的快捷方式）。
    unsafe fn icon_from_pidl(pidl: *mut c_void) -> Option<HIcon> {
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        let cb = std::mem::size_of::<SHFILEINFOW>() as u32;
        let flags = SHGFI_PIDL | SHGFI_ICON | SHGFI_LARGEICON;
        let r = SHGetFileInfoW(pidl as *const u16, 0, &mut shfi, cb, flags);
        if r == 0 || shfi.hIcon.is_null() {
            None
        } else {
            Some(shfi.hIcon)
        }
    }

    unsafe fn resolve_lnk_inner(lnk: &str) -> Option<HIcon> {
        use windows_sys::Win32::System::Com::CoTaskMemFree;
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

        let hicon = if hr == 0 {
            // 1) 显式图标资源（IconLocation）：文件资源管理器等特殊快捷方式可能
            //    没有常规目标，但设置图标来源；从该资源直接取图标可去掉小箭头。
            let mut iconbuf = [0u16; 1040];
            let mut icon_index: i32 = 0;
            let hr = ((*vt).GetIconLocation)(
                psl,
                iconbuf.as_mut_ptr(),
                iconbuf.len() as i32,
                &mut icon_index,
            );
            let mut found = None;
            if hr == 0 {
                let len = iconbuf.iter().position(|&c| c == 0).unwrap_or(iconbuf.len());
                if len > 0 {
                    let raw = String::from_utf16_lossy(&iconbuf[..len]);
                    if let Some(p) = validate_path(&raw) {
                        found = extract_icon_resource(&p, icon_index);
                    }
                }
            }
            // 2) 目标路径（GetPath）：系统快捷方式目标常含 %SystemRoot% 等
            //    环境变量，validate_path 会展开并校验存在性。
            if found.is_none() {
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
                        let raw = String::from_utf16_lossy(&buf[..len]);
                        if let Some(p) = validate_path(&raw) {
                            found = get_hicon(&p);
                        }
                    }
                }
            }
            // 3) PIDL（GetIDList）：文件资源管理器等用 PIDL 指向 Shell 项，无文件路径；
            //    用 SHGetFileInfoW(SHGFI_PIDL) 取该 Shell 项图标，同样无小箭头。
            if found.is_none() {
                let mut pidl: *mut c_void = std::ptr::null_mut();
                let hr = ((*vt).GetIDList)(psl, &mut pidl);
                if hr == 0 && !pidl.is_null() {
                    found = icon_from_pidl(pidl);
                    CoTaskMemFree(pidl);
                }
            }
            found
        } else {
            None
        };

        ((*pvt).parent.Release)(ppf);
        ((*vt).parent.Release)(psl);
        hicon
    }

    /// 解析单个 .lnk 的目标路径（展开环境变量、校验存在性）。只调用 GetPath，
    /// 不提取图标；用于扫描阶段获取可执行文件名作为搜索别名。
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

        let mut ppf: *mut c_void = std::ptr::null_mut();
        let hr = ((*vt).parent.QueryInterface)(psl, &IID_IPERSIST_FILE, &mut ppf);
        if hr != 0 || ppf.is_null() {
            ((*vt).parent.Release)(psl);
            return None;
        }
        let pf = ppf as *const IPersistFile;
        let pvt = (*pf).lpVtbl;

        let mut wide: Vec<u16> = lnk.encode_utf16().collect();
        wide.push(0);
        let hr = ((*pvt).Load)(ppf, wide.as_ptr(), STGM_READ);

        let target = if hr == 0 {
            let mut buf = [0u16; 1040];
            let mut find_data: [u8; 592] = [0; 592];
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
                    let raw = String::from_utf16_lossy(&buf[..len]);
                    validate_path(&raw)
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
        target
    }

    /// 批量解析多个 .lnk 的目标路径。在单个干净 STA 线程内完成（只做一次 COM 初始化），
    /// 适合扫描阶段一次性处理开始菜单的所有快捷方式，避免逐个 spawn 线程的开销。
    pub(crate) fn resolve_lnk_targets_batch(lnks: &[String]) -> Vec<Option<String>> {
        let lnks: Vec<String> = lnks.to_vec();
        let handle = std::thread::spawn(move || unsafe {
            let _ = CoInitializeEx(std::ptr::null(), COINIT_STA as u32);
            let results: Vec<Option<String>> =
                lnks.iter().map(|l| resolve_lnk_target_inner(l)).collect();
            CoUninitialize();
            results
        });
        handle.join().unwrap_or_default()
    }

    /// 从 exe/dll/ico 文件按索引/资源 ID 提取图标（支持负数资源 ID，如 imageres.dll,-186）。
    fn extract_icon_resource(file: &str, index: i32) -> Option<HIcon> {
        use windows_sys::Win32::UI::WindowsAndMessaging::PrivateExtractIconsW;
        unsafe {
            let mut wide: Vec<u16> = file.encode_utf16().collect();
            wide.push(0);
            let mut hicon: HIcon = std::ptr::null_mut();
            let mut icon_id: u32 = 0;
            let n = PrivateExtractIconsW(
                wide.as_ptr(),
                index,
                32,
                32,
                &mut hicon,
                &mut icon_id,
                1,
                0,
            );
            if n == 0 || hicon.is_null() {
                None
            } else {
                Some(hicon)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// 验证开始菜单里的系统快捷方式能解析并提取出图标（含 %SystemRoot% 等环境变量
        /// 展开、IconLocation、PIDL 三条路径），这是去除系统应用图标小箭头的关键。
        #[test]
        fn resolve_system_shortcut_icon() {
            let apps = crate::launcher::scan_cached();
            let mut checked = 0;
            let mut resolved = 0;
            for a in apps.iter().take(200) {
                let lower = a.path.to_ascii_lowercase();
                if !lower.ends_with(".lnk") || !lower.contains("start menu") {
                    continue;
                }
                checked += 1;
                if let Some(hicon) = resolve_lnk(&a.path) {
                    assert!(!hicon.is_null(), "解析得到的 HIcon 不应为空");
                    unsafe {
                        DestroyIcon(hicon);
                    }
                    resolved += 1;
                }
                if checked >= 8 {
                    break;
                }
            }
            println!("[icon test] 系统快捷方式图标解析: 检查 {} 个, 成功 {}", checked, resolved);
            assert!(
                resolved > 0 || checked == 0,
                "应至少有一个系统快捷方式能解析出图标（验证去箭头路径生效）"
            );
        }

        /// 文件资源管理器（PIDL 快捷方式）必须能解析出图标——这是本次修复的核心。
        #[test]
        fn resolve_file_explorer_icon() {
            let apps = crate::launcher::scan_cached();
            let mut found = false;
            for a in apps.iter() {
                let blob = format!("{} {}", a.name, a.path).to_ascii_lowercase();
                if blob.contains("explor") || blob.contains("资源管理器") {
                    found = true;
                    let hicon = resolve_lnk(&a.path);
                    assert!(
                        hicon.is_some(),
                        "文件资源管理器应能解析出图标: {}",
                        a.path
                    );
                    if let Some(h) = hicon {
                        unsafe {
                            DestroyIcon(h);
                        }
                    }
                }
            }
            if !found {
                println!("[icon test] 未找到文件资源管理器快捷方式，跳过（非中文/英文系统开始菜单可能不含）");
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod fallback {
    use crate::icon::IconImage;

    /// 非 Windows 平台暂未实现原生图标提取，返回 None 由 UI 降级为占位图标。
    pub fn extract_icon_rgba(_path: &str) -> Option<IconImage> {
        None
    }
}

#[cfg(target_os = "windows")]
pub use win::extract_icon_rgba;
#[cfg(not(target_os = "windows"))]
pub use fallback::extract_icon_rgba;

#[cfg(target_os = "windows")]
pub(crate) use win::resolve_lnk_targets_batch;
