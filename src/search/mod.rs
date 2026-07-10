//! 应用搜索：拼音索引 + 相关性打分。
//!
//! 移植自前端 `src/lib/apps.ts`，打分阈值与排序逻辑保持一致。
//! 用 `pinyin` crate 替代 `pinyin-pro`：逐字取无声调拼音，连续非汉字按原样保留
//! （对应 pinyin-pro 的 `{ toneType: "none", type: "array", nonZh: "consecutive" }`）。

mod index;
mod query;

pub use index::{IndexedApp, index_apps};
pub use query::search;
