# 神奇的海螺

纯 Rust（egui）桌面应用启动器。Alt+Space 唤起，拼音搜索并启动已安装应用。专注 Windows。

## 功能

- **Alt+Space** 全局快捷键唤起/隐藏
- 拼音搜索（全拼 + 首字母缩写），如 `kz` → 控制中心、`wx` → 微信
- 自动扫描开始菜单 `.lnk` 快捷方式（含本地化显示名解析）
- 图标提取去小箭头（COM vtable 三级回退：IconLocation → GetPath → PIDL）
- 无边框圆角窗口，手动拖动，位置自动记忆
- 系统托盘（显示 / 退出）
- 跟随系统深浅色主题

## 开发

```powershell
# 类型检查
cargo check

# 编译
cargo build

# 测试（含图标回归测试，依赖真实开始菜单）
cargo test

# 运行
cargo run
```

如果下载依赖报 `Failed to connect to index.crates.io port 443`，需设代理：

```powershell
$env:HTTP_PROXY="http://127.0.0.1:7890"; $env:HTTPS_PROXY="http://127.0.0.1:7890"
```

## 依赖

- Rust 2021 edition
- [eframe](https://github.com/emilk/egui) 0.29 — GUI 框架
- [global-hotkey](https://github.com/tauri-apps/global-hotkey) 0.8 — 全局快捷键
- [tray-icon](https://github.com/tauri-apps/tray-icon) 0.24 — 系统托盘
- [pinyin](https://github.com/mozillazg/pinyin) 0.11 — 汉字转拼音
- [windows-sys](https://github.com/microsoft/windows-rs) 0.59 — Win32 API

## 技术要点

- **窗口显隐用 Win32 `ShowWindow`**，不用 egui `Visible`（后者会永久停止 `update()` 导致无法唤醒）
- **不透明窗口 + `SetWindowRgn` 圆角裁剪**，不用 `with_transparent`（透明窗口 resize 抖动）
- **窗口高度固定**（搜索栏 + 8 条结果），永不 resize，避免 GL surface 重建抖动
- **手动拖动 `SetWindowPos`**，不用 eframe `StartDrag`（不进入系统 modal 拖动）
- **图标提取在后台线程串行执行**，主线程零阻塞
- `mod.rs` 只含模块声明，业务逻辑在子文件中
