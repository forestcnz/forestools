//! 窗口手动拖动逻辑。
//!
//! eframe 的 `ViewportCommand::StartDrag` 不会进入系统 modal 拖动（update 不暂停），
//! 无法检测松手。这里手动实现：pointer 按下搜索栏 → 记录起始坐标 → 移动时
//! `SetWindowPos` 实时跟随 → 松手立即保存位置。

use eframe::egui::{self, Pos2, Rect, Vec2};

use crate::window::{self, PhysicalPos};

/// 拖动状态：鼠标按下时的屏幕坐标 + 窗口起始位置（均为物理像素）。
pub(super) struct DragState {
    mouse_start: PhysicalPos,
    window_start: PhysicalPos,
}

impl super::App {
    /// 处理手动拖动：按下搜索栏 + 移动 → SetWindowPos 实时移动窗口；松手立即保存。
    pub(super) fn handle_drag(&mut self, ctx: &egui::Context) {
        let (pdown, ppos) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
        let scale = ctx.pixels_per_point();

        if pdown {
            if let Some(pos) = ppos {
                let search_rect =
                    Rect::from_min_size(Pos2::ZERO, Vec2::new(self.win_width, super::state::BASE_HEIGHT));

                // 按下：记录鼠标屏幕坐标 + 窗口起始位置（仅在搜索栏区域）
                if self.drag.is_none() && search_rect.contains(pos) {
                    if let Some(win_pos) = window::current_window_position() {
                        let mouse_screen = PhysicalPos::new(
                            win_pos.x + (pos.x * scale) as i32,
                            win_pos.y + (pos.y * scale) as i32,
                        );
                        self.drag = Some(DragState {
                            mouse_start: mouse_screen,
                            window_start: win_pos,
                        });
                    }
                }

                // 移动：窗口跟随鼠标位移
                if let Some(drag) = &self.drag {
                    if let Some(win_pos) = window::current_window_position() {
                        let mouse_screen = PhysicalPos::new(
                            win_pos.x + (pos.x * scale) as i32,
                            win_pos.y + (pos.y * scale) as i32,
                        );
                        let new_pos = PhysicalPos::new(
                            drag.window_start.x + (mouse_screen.x - drag.mouse_start.x),
                            drag.window_start.y + (mouse_screen.y - drag.mouse_start.y),
                        );
                        window::set_window_position(new_pos);
                    }
                }
            }
        } else if self.drag.is_some() {
            // 松手：立即保存位置
            window::save_current_position();
            self.drag = None;
        }
    }
}
