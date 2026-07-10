//! 应用搜索：拼音索引 + 相关性打分。
//!
//! 移植自前端 `src/lib/apps.ts`，打分阈值与排序逻辑保持一致。
//! 用 `pinyin` crate 替代 `pinyin-pro`：逐字取无声调拼音，连续非汉字按原样保留
//! （对应 pinyin-pro 的 `{ toneType: "none", type: "array", nonZh: "consecutive" }`）。

use crate::app_launcher::AppInfo;
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

pub struct Match<'a> {
    pub app: &'a IndexedApp,
    pub score: i64,
}

/// 相关性下限：低于该分数的匹配不展示，避免无关结果混入。
const MIN_SCORE: i64 = 500;

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

/// 对单个名称计算相关性分数（-1 表示不匹配）。
fn score_name(n: &IndexedName, q: &str, short: bool) -> i64 {
    if n.lower == q {
        return 1000;
    }
    if n.lower.starts_with(q) {
        return 900;
    }
    if n.initials == q {
        return 880;
    }
    if n.initials.starts_with(q) {
        return 760;
    }
    if !short && n.lower.contains(q) {
        return 700;
    }
    if n.pinyin_full == q {
        return 680;
    }
    if n.pinyin_full.starts_with(q) {
        return 600;
    }
    // 首字母子串匹配：支持「yc -> 远程」「kz -> 控制中心」这类
    // 只输入目标词首字母即命中，即便目标词不在名称开头。
    if !short && n.initials.contains(q) {
        return 560;
    }
    if !short && n.pinyin_full.contains(q) {
        return 500;
    }
    -1
}

/// 按相关性搜索并排序。每个应用的主名称与别名都参与匹配，取最高分。
pub fn search<'a>(indexed: &'a [IndexedApp], query: &str) -> Vec<Match<'a>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    // 单字符查询不做宽泛的 includes 匹配，避免一次命中大量无关应用。
    let short = q.chars().count() == 1;
    let mut out: Vec<Match> = Vec::new();
    for app in indexed.iter() {
        let mut best: i64 = -1;
        for n in &app.names {
            let s = score_name(n, &q, short);
            if s > best {
                best = s;
            }
        }
        if best >= MIN_SCORE {
            out.push(Match { app, score: best });
        }
    }
    out.sort_by(|a, b| match b.score.cmp(&a.score) {
        std::cmp::Ordering::Equal => a
            .app
            .names[0]
            .lower
            .len()
            .cmp(&b.app.names[0].lower.len()),
        other => other,
    });
    out.truncate(8);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(name: &str, path: &str, aliases: &[&str]) -> AppInfo {
        AppInfo {
            name: name.to_string(),
            path: path.to_string(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
        }
    }

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

    #[test]
    fn exact_match_beats_prefix() {
        let apps = index_apps(vec![mk("控制中心", "p1", &[])]);
        let r = search(&apps, "控制中心");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].score, 1000);
    }

    #[test]
    fn initials_substring_match() {
        // 首字母子串：kz -> 控制中心（首字母 k z x，子串 kz 命中 560 分）
        let apps = index_apps(vec![mk("控制中心", "p1", &[])]);
        let r = search(&apps, "kz");
        assert!(r.iter().any(|m| m.app.info.name == "控制中心"));
    }

    #[test]
    fn pinyin_prefix_match() {
        let apps = index_apps(vec![mk("远程桌面", "p1", &[])]);
        let r = search(&apps, "yuanch");
        assert_eq!(r.len(), 1);
        assert!(r[0].score >= 600);
    }

    #[test]
    fn single_char_no_fuzzy() {
        // 单字符不做宽泛 includes，但精确/前缀/首字母仍可命中
        let apps = index_apps(vec![mk("计算器", "p1", &[])]);
        let r = search(&apps, "计");
        assert_eq!(r.len(), 1); // 原文前缀命中 900
    }

    #[test]
    fn empty_query_returns_empty() {
        let apps = index_apps(vec![mk("计算器", "p1", &[])]);
        assert!(search(&apps, "").is_empty());
        assert!(search(&apps, "   ").is_empty());
    }

    #[test]
    fn alias_participates() {
        let apps = index_apps(vec![mk("控制中心", "p1", &["Control Center"])]);
        let r = search(&apps, "control");
        assert_eq!(r.len(), 1);
    }
}
