//! 拼音索引构建。

use crate::launcher::AppInfo;
use pinyin::ToPinyin;

/// 一个可搜索名称的索引（主名称或别名都各自索引）。
#[derive(Clone)]
pub struct IndexedName {
    pub lower: String,
    pub pinyin_full: String,
    pub initials: String,
}

/// 预计算每个应用的匹配键（主名称 + 别名，各自建立名称/全拼/首字母索引）。
pub struct IndexedApp {
    /// 基础应用信息（名称、路径、别名）。
    pub info: AppInfo,
    /// names[0] 为主名称，其后为别名。
    pub names: Vec<IndexedName>,
}

/// 为单个名称建立拼音索引。
///
/// 模仿 pinyin-pro 的 nonZh:"consecutive"：连续的非中文字符合并为一整块参与首字母取值，
/// 而非逐字符拆散（例如 "WeChat微信" → 全拼 "wechatweixin"，首字母 "wwx"）。
fn index_name(raw: &str) -> IndexedName {
    let chars: Vec<char> = raw.chars().collect();
    let mut pinyin_parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(p) = c.to_pinyin() {
            // 汉字：取无声调拼音
            pinyin_parts.push(p.plain().to_string());
            i += 1;
        } else {
            // 非汉字：收集连续非汉字字符作为一个块（consecutive）
            let mut block = String::new();
            while i < chars.len() && chars[i].to_pinyin().is_none() {
                block.push(chars[i]);
                i += 1;
            }
            pinyin_parts.push(block);
        }
    }
    let pinyin_full = pinyin_parts.join("").to_lowercase();
    let initials = pinyin_parts
        .iter()
        .filter_map(|s| s.chars().next())
        .collect::<String>()
        .to_lowercase();
    IndexedName {
        lower: raw.to_lowercase(),
        pinyin_full,
        initials,
    }
}

/// 对一批应用建立索引（主名称 + 每个别名各建一条）。
pub fn index_apps(apps: Vec<AppInfo>) -> Vec<IndexedApp> {
    apps.into_iter()
        .map(|a| {
            let mut names = vec![index_name(&a.name)];
            for alias in &a.aliases {
                names.push(index_name(alias));
            }
            IndexedApp { info: a, names }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinyin_full_and_initials() {
        let n = index_name("微信");
        assert_eq!(n.pinyin_full, "weixin");
        assert_eq!(n.initials, "wx");
    }

    #[test]
    fn mixed_cn_ascii_keeps_block() {
        // 连续非汉字作为整块：WeChat微信 → wechatweixin / wwx
        let n = index_name("WeChat微信");
        assert_eq!(n.pinyin_full, "wechatweixin");
        assert_eq!(n.initials, "wwx");
    }
}
