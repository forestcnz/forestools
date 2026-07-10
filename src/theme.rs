//! 颜色主题（对应前端 CSS，区分浅色/暗色两套）。

use eframe::egui::Color32;

/// 一组界面颜色，按系统深浅色选择。
pub struct ThemeColors {
    /// 窗口圆角背景填充。
    pub bg: Color32,
    /// 窗口描边。
    pub border: Color32,
    /// 选中项背景。
    pub selected: Color32,
    /// 主文本（应用名）。
    pub text: Color32,
    /// 次要文本（应用路径）。
    pub text_secondary: Color32,
}

impl ThemeColors {
    /// 浅色（对应 :root 非暗色样式）。
    pub fn light() -> Self {
        Self {
            bg: Color32::from_rgb(255, 255, 255),
            border: Color32::from_rgba_unmultiplied(0, 0, 0, 26), // rgba(0,0,0,0.1)
            selected: Color32::from_rgba_unmultiplied(0, 113, 227, 31), // ≈0.12
            text: Color32::from_rgb(0x1a, 0x1a, 0x1a),
            text_secondary: Color32::from_rgb(0x88, 0x88, 0x88),
        }
    }

    /// 暗色（对应 @media prefers-color-scheme: dark）。
    pub fn dark() -> Self {
        Self {
            bg: Color32::from_rgb(40, 40, 42),
            border: Color32::from_rgba_unmultiplied(255, 255, 255, 31), // ≈0.12
            selected: Color32::from_rgba_unmultiplied(100, 168, 255, 51), // ≈0.2
            text: Color32::from_rgb(0xe5, 0xe5, 0xe7),
            text_secondary: Color32::from_rgb(0x88, 0x88, 0x88),
        }
    }

    /// 按当前 visuals 是否暗色选取。
    pub fn pick(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}
