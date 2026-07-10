//! 应用图标提取。
//!
//! 设计参考原 Tauri 版的"路径即图标 + 内存 LRU + 串行提取"方案：
//! - 图标数据不落盘，只驻留主进程内存的 LRU Map（上限 128）。
//! - 提取走串行锁，规避原生 API 在高并发下的潜在问题。
//! - 对外提供 `get_icon_rgba`（直接产出 RGBA 像素，供 egui `ColorImage` 零损耗上传）；
//!   另保留 `get_icon`（PNG 编码，仅供测试校验 PNG 魔数）。

mod cache;
mod platform;

pub use cache::{IconImage, get_icon_rgba};
#[cfg(test)]
pub use cache::get_icon;
