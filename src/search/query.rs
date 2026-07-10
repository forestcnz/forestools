//! 相关性打分与搜索。

use super::index::{IndexedApp, IndexedName};

/// 搜索命中。
pub struct Match<'a> {
    pub app: &'a IndexedApp,
    pub score: i64,
}

/// 相关性下限：低于该分数的匹配不展示，避免无关结果混入。
const MIN_SCORE: i64 = 500;

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
    use super::super::index::index_apps;
    use crate::launcher::AppInfo;

    fn mk(name: &str, path: &str, aliases: &[&str]) -> AppInfo {
        AppInfo {
            name: name.to_string(),
            path: path.to_string(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
        }
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
