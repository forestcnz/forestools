//! 中文字体配置（egui 默认不含 CJK）。
//!
//! 尝试加载 Windows 系统微软雅黑，失败回退默认字体。

use eframe::egui;

/// 加载系统中文字体并注入 egui 字体表。
pub(crate) fn configure(ctx: &egui::Context) {
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
