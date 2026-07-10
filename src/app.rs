//! eframe::App 实现：搜索框 + 结果列表 + 键盘导航 + 窗口拖动 + 动态高度 + 图标懒加载。
//!
//! 事件流：后台线程接收全局快捷键/托盘菜单事件 → channel + `ctx.request_repaint()`
//! 唤醒主线程 `update`。窗口隐藏时也依赖此机制被唤醒以重新显示。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, Key, Pos2, Rect, Rounding, Sense, Stroke, TextureHandle, Vec2};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::app_launcher;
use crate::icon;
use crate::search::{self, IndexedApp};
use crate::theme::ThemeColors;
use crate::window_ctl;

/// 搜索栏高度（逻辑像素）。
const BASE_HEIGHT: f32 = 56.0;
/// 单条结果高度。
const ITEM_HEIGHT: f32 = 44.0;
/// 最多展示结果数。
const MAX_ITEMS: usize = 8;
/// 窗口固定高度（搜索栏 + 间距 + 最大结果区），避免动态 resize 导致 GL surface 重建抖动。
pub const MAX_WINDOW_HEIGHT: f32 = BASE_HEIGHT + 8.0 + MAX_ITEMS as f32 * ITEM_HEIGHT;
/// 图标显示尺寸。
const ICON_SIZE: f32 = 28.0;
/// 后台事件轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 图标纹理缓存状态。
enum IconState {
    /// 已请求提取，尚未返回。
    Pending,
    /// 提取完成（None 表示提取失败，绘制时留空）。
    Ready(Option<TextureHandle>),
}

pub struct App {
    /// 搜索框文本。
    query: String,
    /// 已索引的应用列表（后台线程加载后填充）。
    apps: Vec<IndexedApp>,
    /// 当前选中结果索引。
    selected: usize,
    /// 窗口宽度（启动时按屏幕 2/5 计算）。
    win_width: f32,
    /// 上次设置圆角裁剪区域的高度（仅在变化时下发 SetWindowRgn）。
    last_region_height: f32,
    /// 是否已恢复启动位置（首个 update 执行一次）。
    position_restored: bool,
    /// 是否需要聚焦输入框（每次显示后置 true，绘制后清零）。
    focus_input: bool,
    /// 手动拖动状态：(鼠标按下屏幕坐标, 窗口起始位置)，均为物理像素。
    drag: Option<((i32, i32), (i32, i32))>,

    /// 图标纹理缓存：应用路径 → 状态。
    icon_cache: HashMap<String, IconState>,
    /// 图标提取请求通道（发给后台提取线程）。
    icon_req_tx: std::sync::mpsc::Sender<String>,
    /// 图标提取结果通道（后台线程回传）。
    icon_resp_rx: std::sync::mpsc::Receiver<(String, Option<(u32, u32, Vec<u8>)>)>,

    /// 后台线程通知主线程"窗口刚显示"（需重置查询/聚焦）。
    just_shown: Arc<AtomicBool>,
    /// 应用列表加载结果通道。
    apps_rx: std::sync::mpsc::Receiver<Vec<IndexedApp>>,

    /// 全局快捷键管理器（保活，drop 即注销）。
    #[allow(dead_code)]
    hk_manager: GlobalHotKeyManager,
    /// 系统托盘（保活，drop 即销毁）。
    #[allow(dead_code)]
    tray: Option<tray_icon::TrayIcon>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 跟随系统深浅色
        let ctx = &cc.egui_ctx;
        ctx.set_theme(egui::ThemePreference::System);
        configure_fonts(ctx);

        let win_width = window_ctl::compute_window_width();

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
        let tray = build_tray(show_item, quit_item);

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
                    if window_ctl::is_main_window_visible() {
                        window_ctl::hide_main_window();
                    } else {
                        window_ctl::show_main_window();
                        just_shown2.store(true, Ordering::SeqCst);
                        ctx2.request_repaint();
                    }
                }
            }
            // 托盘菜单事件
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if ev.id == show_id2 {
                    // "显示"：唤起窗口（与快捷键一致）
                    window_ctl::show_main_window();
                    just_shown2.store(true, Ordering::SeqCst);
                    ctx2.request_repaint();
                }
                if ev.id == quit_id2 {
                    // "退出"：保存位置并退出进程
                    window_ctl::save_current_position();
                    std::process::exit(0);
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        });

        // ── 图标提取后台线程：串行提取，回传 RGBA ──
        let (icon_req_tx, icon_req_rx) = std::sync::mpsc::channel::<String>();
        let (icon_resp_tx, icon_resp_rx) = std::sync::mpsc::channel::<(String, Option<(u32, u32, Vec<u8>)>)>();
        std::thread::spawn(move || {
            while let Ok(path) = icon_req_rx.recv() {
                let result = icon::get_icon_rgba(&path);
                let _ = icon_resp_tx.send((path, result));
            }
        });

        // ── 应用扫描/索引后台线程 ──
        let (apps_tx, apps_rx) = std::sync::mpsc::channel::<Vec<IndexedApp>>();
        std::thread::spawn(move || {
            let apps = app_launcher::scan_cached();
            let _ = apps_tx.send(search::index_apps(apps));
        });

        // 固定窗口尺寸（宽度按屏幕 2/5，高度固定为最大结果区，避免 resize 抖动）
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
            win_width,
            MAX_WINDOW_HEIGHT,
        )));
        // 启动即隐藏（用 Win32 而非 egui Visible，确保 egui update 持续运行）
        window_ctl::hide_main_window();

        Self {
            query: String::new(),
            apps: Vec::new(),
            selected: 0,
            win_width,
            last_region_height: -1.0,
            position_restored: false,
            focus_input: false,
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

    /// 收取图标提取结果，转成纹理存入缓存。
    fn poll_icons(&mut self, ctx: &egui::Context) {
        while let Ok((path, pixels)) = self.icon_resp_rx.try_recv() {
            let handle = pixels.and_then(|(w, h, rgba)| {
                let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                Some(ctx.load_texture(&path, image, egui::TextureOptions::LINEAR))
            });
            self.icon_cache.insert(path, IconState::Ready(handle));
        }
    }

    /// 对结果中尚未缓存的图标发起提取请求。
    fn request_icons(&mut self, paths: &[&str]) {
        for path in paths {
            if !self.icon_cache.contains_key(*path) {
                self.icon_cache
                    .insert(path.to_string(), IconState::Pending);
                let _ = self.icon_req_tx.send(path.to_string());
            }
        }
    }
}

impl eframe::App for App {
    /// 窗口背景色（不透明，配合 SetWindowRgn 圆角裁剪）。
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if visuals.dark_mode {
            [40.0 / 255.0, 40.0 / 255.0, 42.0 / 255.0, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 启动恢复位置（首个 update，窗口已完全创建，FindWindow 可靠）
        if !self.position_restored {
            self.position_restored = true;
            window_ctl::restore_position();
        }

        // ── 1. 应用列表加载 ──
        if let Ok(indexed) = self.apps_rx.try_recv() {
            self.apps = indexed;
        }

        // ── 2. 窗口刚显示则重置查询/聚焦（由后台线程置位）──
        if self.just_shown.swap(false, Ordering::SeqCst) {
            self.query.clear();
            self.selected = 0;
            self.focus_input = true;
        }

        // ── 3. 主题颜色 ──
        let dark = ctx.style().visuals.dark_mode;
        let colors = ThemeColors::pick(dark);

        // ── 4. 搜索结果：克隆出 (path, name)，随即释放对 self.apps 的借用 ──
        let items: Vec<(String, String)> = search::search(&self.apps, &self.query)
            .into_iter()
            .map(|m| (m.app.info.path.clone(), m.app.info.name.clone()))
            .collect();
        let result_count = items.len();

        // ── 5. 图标提取回传 / 请求 ──
        self.poll_icons(ctx);
        let pending_paths: Vec<&str> = items.iter().map(|(p, _)| p.as_str()).collect();
        self.request_icons(&pending_paths);

        // ── 6. 键盘交互（仅在可见时）──
        let mut launch_path: Option<String> = None;
        let mut should_hide = false;
        if window_ctl::is_main_window_visible() {
            let (down, up, enter, esc) = ctx.input(|i| {
                (
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::Enter),
                    i.key_pressed(Key::Escape),
                )
            });
            if result_count > 0 {
                if down {
                    self.selected = (self.selected + 1) % result_count;
                }
                if up {
                    self.selected = (self.selected + result_count - 1) % result_count;
                }
                if enter {
                    if let Some((p, _)) = items.get(self.selected) {
                        launch_path = Some(p.clone());
                    }
                }
            }
            if esc {
                should_hide = true;
            }
        }

        // 启动选中应用
        if let Some(path) = launch_path {
            if let Err(e) = app_launcher::open(&path) {
                eprintln!("启动失败: {e}");
            }
            self.query.clear();
            should_hide = true;
        }

        // 内容高度（窗口物理高度固定为 MAX_WINDOW_HEIGHT，用 SetWindowRgn 裁剪可见区域到内容高度）
        let new_height = if result_count > 0 {
            BASE_HEIGHT + 8.0 + result_count.min(MAX_ITEMS) as f32 * ITEM_HEIGHT
        } else {
            BASE_HEIGHT
        };
        // 内容高度变化时更新圆角裁剪区域（物理裁剪，无需 transparent）
        if (new_height - self.last_region_height).abs() > 0.5 {
            let scale = ctx.pixels_per_point();
            window_ctl::set_window_region(self.win_width, new_height, scale);
            self.last_region_height = new_height;
        }

        // ── 手动拖动：按下搜索栏 + 移动 → SetWindowPos 实时移动窗口；松手立即保存 ──
        let (pdown, ppos) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
        let scale = ctx.pixels_per_point();
        if pdown {
            if let Some(pos) = ppos {
                let search_rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(self.win_width, BASE_HEIGHT));
                // 按下：记录鼠标屏幕坐标 + 窗口起始位置（仅在搜索栏区域）
                if self.drag.is_none() && search_rect.contains(pos) {
                    if let Some(win_pos) = window_ctl::current_window_position() {
                        let mouse_screen = (
                            win_pos.0 + (pos.x * scale) as i32,
                            win_pos.1 + (pos.y * scale) as i32,
                        );
                        self.drag = Some((mouse_screen, win_pos));
                    }
                }
                // 移动：窗口跟随鼠标位移
                if let Some((mouse_start, win_start)) = self.drag {
                    if let Some(win_pos) = window_ctl::current_window_position() {
                        let mouse_screen = (
                            win_pos.0 + (pos.x * scale) as i32,
                            win_pos.1 + (pos.y * scale) as i32,
                        );
                        let new_x = win_start.0 + (mouse_screen.0 - mouse_start.0);
                        let new_y = win_start.1 + (mouse_screen.1 - mouse_start.1);
                        window_ctl::set_window_position(new_x, new_y);
                    }
                }
            }
        } else if self.drag.is_some() {
            // 松手：立即保存位置
            window_ctl::save_current_position();
            self.drag = None;
        }

        // ── 8. 绘制 ──
        let focus_input = self.focus_input;
        let selected = self.selected;
        // 用于点击启动：收集 (index, path)
        let mut click_launch: Option<String> = None;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(colors.bg)
                    .stroke(Stroke::NONE)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                // 用预期高度（new_height）而非窗口实际 max_rect 绘制：
                // 发 InnerSize 后窗口尺寸已变，但本帧若仍按旧 max_rect 渲染会错位抖动。
                let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(self.win_width, new_height));

                // 圆角背景 + 描边
                let rounding = Rounding::same(12.0);
                ui.painter().rect_filled(rect, rounding, colors.bg);
                ui.painter().rect_stroke(rect, rounding, Stroke::new(1.0, colors.border));

                // 输入框
                let input_rect = Rect::from_min_size(
                    Pos2::new(rect.min.x + 18.0, rect.min.y),
                    Vec2::new(rect.width() - 36.0, BASE_HEIGHT),
                );
                #[allow(deprecated)]
                ui.allocate_ui_at_rect(input_rect, |ui| {
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(18.0);
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .font(egui::FontId::proportional(20.0))
                                    .desired_width(input_rect.width() - 36.0)
                                    .frame(false)
                                    .text_color(colors.text)
                                    .hint_text("神奇的海螺"),
                            );
                            if focus_input {
                                resp.request_focus();
                            }
                            if resp.changed() {
                                self.selected = 0;
                            }
                        },
                    );
                });

                // ── 结果列表 ──
                let list_x = rect.min.x + 8.0;
                let list_w = rect.width() - 16.0;
                for (i, (path, name)) in items.iter().enumerate().take(MAX_ITEMS) {
                    let item_top = rect.min.y + BASE_HEIGHT + 4.0 + i as f32 * ITEM_HEIGHT;
                    let item_rect = Rect::from_min_size(
                        Pos2::new(list_x, item_top),
                        Vec2::new(list_w, ITEM_HEIGHT - 4.0),
                    );
                    let resp = ui.interact(
                        item_rect,
                        ui.id().with(("item", i)),
                        Sense::click(),
                    );
                    let painter = ui.painter();

                    // 选中项背景
                    if i == selected {
                        painter.rect_filled(item_rect, Rounding::same(8.0), colors.selected);
                    }

                    // 图标
                    let icon_rect = Rect::from_center_size(
                        Pos2::new(item_rect.min.x + 10.0 + ICON_SIZE / 2.0, item_rect.center().y),
                        Vec2::splat(ICON_SIZE),
                    );
                    if let Some(IconState::Ready(Some(handle))) = self.icon_cache.get(path) {
                        painter.image(
                            handle.id(),
                            icon_rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }

                    // 名称 + 路径
                    let text_x = icon_rect.max.x + 12.0;
                    let name_pos = Pos2::new(text_x, item_rect.center().y - 8.0);
                    let path_pos = Pos2::new(text_x, item_rect.center().y + 8.0);
                    painter.text(
                        name_pos,
                        egui::Align2::LEFT_CENTER,
                        name,
                        egui::FontId::proportional(14.0),
                        colors.text,
                    );
                    painter.text(
                        path_pos,
                        egui::Align2::LEFT_CENTER,
                        path,
                        egui::FontId::proportional(11.0),
                        colors.text_secondary,
                    );

                    // hover 选中 / 点击启动
                    if resp.hovered() {
                        self.selected = i;
                    }
                    if resp.clicked() {
                        click_launch = Some(path.clone());
                    }
                }
            });

        // 点击启动
        if let Some(path) = click_launch {
            if let Err(e) = app_launcher::open(&path) {
                eprintln!("启动失败: {e}");
            }
            self.query.clear();
            should_hide = true;
        }

        if should_hide {
            window_ctl::hide_main_window();
        }

        self.focus_input = false;

        // ── 9. 保活：及时消费后台事件（隐藏状态下亦依赖 request_repaint 唤醒）──
        ctx.request_repaint_after(POLL_INTERVAL);
    }
}

/// 配置中文字体（egui 默认不含 CJK）。尝试加载 Windows 系统微软雅黑，失败回退默认。
fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in &[
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msyh.ttf",
        "C:\\Windows\\Fonts\\simhei.ttf",
    ] {
        if let Ok(data) = std::fs::read(path) {
            fonts
                .font_data
                .insert("system_cjk".to_owned(), egui::FontData::from_owned(data));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "system_cjk".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("system_cjk".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

/// 构建系统托盘（"显示" + "退出"菜单项）。
fn build_tray(show_item: MenuItem, quit_item: MenuItem) -> Option<TrayIcon> {
    let menu = Menu::new();
    let _ = menu.append(&show_item);
    let _ = menu.append(&quit_item);
    let icon = load_tray_icon();
    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(window_ctl::WINDOW_TITLE)
        .with_icon(icon)
        .build()
        .ok()
}

/// 从嵌入的 PNG 资源加载托盘图标。
fn load_tray_icon() -> Icon {
    const TRAY_PNG: &[u8] = include_bytes!("../icons/32x32.png");
    let fallback = Icon::from_rgba(vec![0u8; 4], 1, 1).unwrap_or_else(|_| {
        // 极端情况下构造一个 1x1 透明像素
        Icon::from_rgba(vec![0, 0, 0, 0], 1, 1).expect("1x1 icon")
    });
    let img = match image::load_from_memory(TRAY_PNG) {
        Ok(img) => img.into_rgba8(),
        Err(_) => return fallback,
    };
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).unwrap_or(fallback)
}
