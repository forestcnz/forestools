//! 应用扫描与启动。

mod launch;
mod model;
mod scan;

pub use model::AppInfo;
pub use scan::scan_cached;
pub use launch::open;
