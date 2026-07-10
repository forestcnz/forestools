//! 数据模型与错误类型。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 启动器错误。
#[derive(Error, Debug)]
pub enum LauncherError {
    #[error("ShellExecuteW 失败 (返回码 {0})")]
    ShellExecuteFailed(isize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub path: String,
    /// 搜索别名（如英文 .lnk 文件名），让中英文名称都可被搜索。
    #[serde(default)]
    pub aliases: Vec<String>,
}
