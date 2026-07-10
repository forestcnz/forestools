//! App 结构体定义、初始化、图标缓存。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Vec2};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use tray_icon::menu::{MenuEvent, MenuItem};

use crate::icon::{self, IconImage};
use crate::launcher;
use crate::search::{self, IndexedApp};
use crate::window;

use super::drag;
use super::fonts;
use super::tray;

/// 搜索栏高度（逻辑像素）。
pub(super) const BASE_HEIGHT: f32 = 56.0;
/// 单条结果高度。
pub(super) const ITEM_HEIGHT: f32 = 44.0;
/// 最多展示结果数。
pub(super) const MAX_ITEMS: usize = 8;
/// 窗口固定高度（搜索栏 + 间距 + 最大结果区），避免动态 resize 导致 GL surface 重建抖动。
pub const MAX_WINDOW_HEIGHT: f32 = BASE_HEIGHT + 8.0 + MAX_ITEMS as f32 * ITEM_HEIGHT;
/// 后台事件轮询间隔。
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 图标纹理缓存状态。
pub(super) enum IconState {
    /// 已请求提取，尚未返回。
    Pending,
    /// 提取完成（None 表示提取失败，绘制时留空）。
    Ready(Option<egui::TextureHandle>),
}

pub struct App {
    /// 搜索框文本。
    pub(super) query: String,
    /// 已索引的应用列表（后台线程加载后填充）。
    pub(super) apps: Vec<IndexedApp>,
    /// 当前选中结果索引。
    pub(super) selected: usize,
    /// 窗口宽度（启动时按屏幕 2/5 计算）。
    pub(super) win_width: f32,
    /// 上次设置圆角裁剪区域的高度（仅在变化时下发 SetWindowRgn）。
    pub(super) last_region_height: f32,
    /// 是否已恢复启动位置（首个 update 执行一次）。
    pub(super) position_restored: bool,
    /// 剩余需要请求输入框聚焦的帧数（窗口刚显示后多帧重试，避免 OS 焦点未到位）。
    pub(super) focus_frames: u8,
    /// 手动拖动状态。
    pub(super) drag: Option<drag::DragState>,

    /// 图标纹理缓存：应用路径 → 状态。
    pub(super) icon_cache: HashMap<String, IconState>,
    /// 图标提取请求通道（发给后台提取线程）。
    pub(super) icon_req_tx: std::sync::mpsc::Sender<String>,
    /// 图标提取结果通道（后台线程回传）。
    pub(super) icon_resp_rx: std::sync::mpsc::Receiver<(String, Option<IconImage>)>,

    /// 后台线程通知主线程"窗口刚显示"（需重置查询/聚焦）。
    pub(super) just_shown: Arc<AtomicBool>,
    /// 应用列表加载结果通道。
    pub(super) apps_rx: std::sync::mpsc::Receiver<Vec<IndexedApp>>,

    /// 全局快捷键管理器（保活，drop 即注销）。
    #[allow(dead_code)]
    pub(super) hk_manager: GlobalHotKeyManager,
    /// 系统托盘（保活，drop 即销毁）。
    #[allow(dead_code)]
    pub(super) tray: Option<tray_icon::TrayIcon>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 跟随系统深浅色
        let ctx = &cc.egui_ctx;
        ctx.set_theme(egui::ThemePreference::System);
        fonts::configure(ctx);

        let win_width = window::compute_window_width();

        // ── 全局快捷键（Alt+Space）──
        let hk_manager = GlobalHotKeyManager::new().expect("failed to create hotkey manager");
        let hotkey = HotKey::new(Some(Modifiers::ALT), Code::Space);
        if let Err(e) = hk_manager.register(hotkey) {
            eprintln!("warning: failed to register Alt+Space: {e}");
        }

        // ── 系统托盘（"显示" + "退出"菜单）──
        let show_item = MenuItem::new("显示", true, None);
        let quit_item = MenuItem::new("退出", true, None);
        let show_id = show_item.id().clone();
        let quit_id = quit_item.id().clone();
        let tray = tray::build(show_item, quit_item);

        // ── 后台事件线程：接收快捷键/托盘事件，直接用 Win32 控制窗口显隐 ──
        // 不走 egui ViewportCommand::Visible：隐藏窗口后 egui update 会停止，
        // 导致再次唤起时无法处理命令。改用 ShowWindow 保证 update 持续运行。
        let just_shown = Arc::new(AtomicBool::new(false));
        let just_shown2 = just_shown.clone();
        let ctx2 = ctx.clone();
        let quit_id2 = quit_id.clone();
        let show_id2 = show_id.clone();
        std::thread::spawn(move || loop {
            while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                if ev.state == HotKeyState::Pressed {
                    if window::is_main_window_visible() {
                        window::hide_main_window();
                    } else {
                        window::show_main_window();
                        just_shown2.store(true, Ordering::SeqCst);
                        ctx2.request_repaint();
                    }
                }
            }
            // 托盘菜单事件
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if ev.id == show_id2 {
                    // "显示"：唤起窗口（与快捷键一致）
                    window::show_main_window();
                    just_shown2.store(true, Ordering::SeqCst);
                    ctx2.request_repaint();
                }
                if ev.id == quit_id2 {
                    // "退出"：保存位置并退出进程
                    window::save_current_position();
                    std::process::exit(0);
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        });

        // ── 图标提取后台线程：串行提取，回传 RGBA ──
        let (icon_req_tx, icon_req_rx) = std::sync::mpsc::channel::<String>();
        let (icon_resp_tx, icon_resp_rx) =
            std::sync::mpsc::channel::<(String, Option<IconImage>)>();
        std::thread::spawn(move || {
            while let Ok(path) = icon_req_rx.recv() {
                let result = icon::get_icon_rgba(&path);
                let _ = icon_resp_tx.send((path, result));
            }
        });

        // ── 应用扫描/索引后台线程 ──
        let (apps_tx, apps_rx) = std::sync::mpsc::channel::<Vec<IndexedApp>>();
        std::thread::spawn(move || {
            let apps = launcher::scan_cached();
            let _ = apps_tx.send(search::index_apps(apps));
        });

        // 固定窗口尺寸（宽度按屏幕 2/5，高度固定为最大结果区，避免 resize 抖动）
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
            win_width,
            MAX_WINDOW_HEIGHT,
        )));
        // 启动即隐藏（用 Win32 而非 egui Visible，确保 egui update 持续运行）
        window::hide_main_window();

        Self {
            query: String::new(),
            apps: Vec::new(),
            selected: 0,
            win_width,
            last_region_height: -1.0,
            position_restored: false,
            focus_frames: 0,
            drag: None,
            icon_cache: HashMap::new(),
            icon_req_tx,
            icon_resp_rx,
            just_shown,
            apps_rx,
            hk_manager,
            tray,
        }
    }

    /// 计算内容区域高度（搜索栏 + 结果列表）。
    pub(super) fn content_height(&self, result_count: usize) -> f32 {
        if result_count > 0 {
            BASE_HEIGHT + 8.0 + result_count.min(MAX_ITEMS) as f32 * ITEM_HEIGHT
        } else {
            BASE_HEIGHT
        }
    }

    /// 收取图标提取结果，转成纹理存入缓存。
    pub(super) fn poll_icons(&mut self, ctx: &egui::Context) {
        while let Ok((path, img)) = self.icon_resp_rx.try_recv() {
            let handle = img.and_then(|icon| {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                );
                Some(ctx.load_texture(&path, image, egui::TextureOptions::LINEAR))
            });
            self.icon_cache.insert(path, IconState::Ready(handle));
        }
    }

    /// 对结果中尚未缓存的图标发起提取请求。
    pub(super) fn request_icons(&mut self, paths: &[&str]) {
        for path in paths {
            if !self.icon_cache.contains_key(*path) {
                self.icon_cache
                    .insert(path.to_string(), IconState::Pending);
                let _ = self.icon_req_tx.send(path.to_string());
            }
        }
    }
}
