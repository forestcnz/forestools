//! 搜索栏 + 结果列表绘制。

use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

use crate::theme::ThemeColors;

/// 图标显示尺寸（逻辑像素）。
const ICON_SIZE: f32 = 28.0;

impl super::App {
    /// 绘制搜索栏 + 结果列表，返回被点击项的路径（如有）。
    pub(super) fn render_content(
        &mut self,
        ui: &mut egui::Ui,
        items: &[(String, String)],
        colors: &ThemeColors,
        new_height: f32,
    ) -> Option<String> {
        let focus_input = self.focus_frames > 0;
        let selected = self.selected;
        let mut click_launch: Option<String> = None;

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
            Vec2::new(rect.width() - 36.0, super::state::BASE_HEIGHT),
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
        for (i, (path, name)) in items.iter().enumerate().take(super::state::MAX_ITEMS) {
            let item_top = rect.min.y + super::state::BASE_HEIGHT + 4.0 + i as f32 * super::state::ITEM_HEIGHT;
            let item_rect = Rect::from_min_size(
                Pos2::new(list_x, item_top),
                Vec2::new(list_w, super::state::ITEM_HEIGHT - 4.0),
            );
            let resp = ui.interact(item_rect, ui.id().with(("item", i)), Sense::click());
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
            if let Some(super::state::IconState::Ready(Some(handle))) = self.icon_cache.get(path) {
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

        click_launch
    }
}
