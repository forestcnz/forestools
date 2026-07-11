//! eframe::App trait 实现：每帧编排。

use std::sync::atomic::Ordering;

use eframe::egui::{self, Key, Stroke};

use crate::launcher;
use crate::search;
use crate::theme::ThemeColors;
use crate::window;

impl eframe::App for super::App {
    /// 窗口背景色（不透明，配合 SetWindowRgn 圆角裁剪）。
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if visuals.dark_mode {
            [40.0 / 255.0, 40.0 / 255.0, 42.0 / 255.0, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 启动隐藏：eframe 首帧 post_rendering 会强制 set_visible(true)，
        // 无法阻止。首帧将窗口移到屏幕外，post_rendering 显示时用户不可见；
        // 第二帧再用 Win32 SW_HIDE 隐藏（此后 post_rendering 不再重复 show）。
        if !self.position_restored {
            self.position_restored = true;
            window::set_window_position(window::PhysicalPos::new(-32000, -32000));
        } else if self.need_initial_hide {
            self.need_initial_hide = false;
            window::hide_main_window();
        }

        // ── 1. 应用列表加载 ──
        if let Ok(indexed) = self.apps_rx.try_recv() {
            self.apps = indexed;
        }

        // ── 2. 窗口刚显示则重置查询/聚焦（由后台线程置位）──
        if self.just_shown.swap(false, Ordering::SeqCst) {
            self.query.clear();
            self.selected = 0;
            self.focus_frames = 15;
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
        if window::is_main_window_visible() {
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
            if let Err(e) = launcher::open(&path) {
                eprintln!("启动失败: {e}");
            }
            self.query.clear();
            should_hide = true;
        }

        // ── 7. 内容高度变化时更新圆角裁剪区域（物理裁剪，无需 transparent）──
        let new_height = self.content_height(result_count);
        if (new_height - self.last_region_height).abs() > 0.5 {
            let scale = ctx.pixels_per_point();
            window::set_window_region(self.win_width, new_height, scale);
            self.last_region_height = new_height;
        }

        // ── 8. 手动拖动 ──
        self.handle_drag(ctx);

        // ── 9. 绘制 ──
        let click_launch = egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(colors.bg)
                    .stroke(Stroke::NONE)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| self.render_content(ui, &items, &colors, new_height))
            .inner;

        // 点击启动
        if let Some(path) = click_launch {
            if let Err(e) = launcher::open(&path) {
                eprintln!("启动失败: {e}");
            }
            self.query.clear();
            should_hide = true;
        }

        if should_hide {
            window::hide_main_window();
        }

        if self.focus_frames > 0 {
            self.focus_frames -= 1;
        }

        // ── 10. 保活：及时消费后台事件（隐藏状态下亦依赖 request_repaint 唤醒）──
        ctx.request_repaint_after(super::state::POLL_INTERVAL);
    }
}
