# forestools

纯 Rust（egui）桌面应用启动器（Alt+Space 唤起，拼音搜索并启动已安装应用）。专注 Windows。

## 关键约束

- **禁止 AI 启动/运行程序**（包括 `cargo run`、直接运行 exe 等）。改完代码做类型检查（`cargo check`）或测试（`cargo test`）即可，运行 GUI 交由用户。
- 无前端工具链；仓库是单一 Cargo 包（根目录 `Cargo.toml`）。

## 仓库与 Git

- 远程：`https://github.com/forestcnz/forestools.git`（`origin/master`）。
- 本仓库 `.git/config` 里配置了 **local** 代理 `http.proxy` / `https.proxy` → `http://127.0.0.1:7890`（Clash）。开发者本地开了 VPN，但 git 需显式走代理才能连通 GitHub；**仅 local，非全局**。
  - 端口变更：`git config --local http.proxy http://127.0.0.1:<端口>`（`https.proxy` 同步）。
  - 取消代理：`git config --local --unset http.proxy` 和 `--unset https.proxy`。
  - 若 `git pull/push` 报 `Failed to connect to github.com port 443`，先检查 Clash 是否在跑、端口是否一致。
- **cargo 连 crates.io 也需走代理**：git 的 local 代理对 cargo 无效，须设环境变量：
  - PowerShell：`$env:HTTP_PROXY="http://127.0.0.1:7890"; $env:HTTPS_PROXY="http://127.0.0.1:7890"`
  - 首次 `cargo build/check` 下载依赖前务必设置，否则报 `Failed to connect to index.crates.io port 443`。

## 开发命令

| 用途 | 命令 | 说明 |
|------|------|------|
| 类型检查 | `cargo check` | 首选的快速验证入口（无 warning/error 即过） |
| 编译 | `cargo build` | debug 编译 |
| 测试 | `cargo test` | 含图标回归测试，依赖真实开始菜单/应用存在 |
| 运行（用户手动） | `cargo run` | 启动 GUI，**AI 禁止执行** |

无独立 lint 命令；`cargo check` 即类型检查入口。

## 项目结构

```
src/                      Rust 源码（纯 Rust，无前端）
  main.rs                 入口（eframe::run_native + 视口配置 + 嵌入窗口图标）
  app.rs                  eframe::App：UI 绘制、事件循环、状态、图标纹理缓存、
                          后台线程（快捷键/托盘事件 + 图标提取 + 应用索引）
  search.rs               拼音索引 + 相关性打分（pinyin crate，移植自原 apps.ts）
  app_launcher.rs         应用扫描（带缓存）+ 启动；Windows .lnk / macOS .app
  icon.rs                 图标提取（COM vtable）+ get_icon_rgba（LRU 缓存 + 串行锁）
  window_ctl.rs           位置持久化、Win32 显示器枚举、窗口宽度/默认位置计算
  theme.rs                浅色/暗色颜色常量（对应原 CSS）
icons/                    应用图标资源（include_bytes! 嵌入托盘/窗口图标）
Cargo.toml                依赖：eframe/egui、global-hotkey、tray-icon、pinyin、image、windows-sys
```

## 架构要点

**窗口与交互**
- 启动隐藏（`ViewportBuilder::with_visible(false)`），**Alt+Space** 全局快捷键切换显示/隐藏（`global-hotkey` crate，底层 Win32 `RegisterHotKey`）。
- 无系统标题栏：`with_decorations(false)` + `with_transparent(true)`，`clear_color` 返回 `[0,0,0,0]`；圆角由 `Painter::rect_filled` + `Rounding::same(12)` 自绘。
- 宽度按主屏宽度 2/5 计算（`GetSystemMetrics(SM_CXSCREEN)` + DPI 折算），**高度随结果数量动态变化**（`ViewportCommand::InnerSize`，单位逻辑像素）。别把高度/宽度当常量。
- 窗口拖动：搜索栏区域 `ui.interact(Sense::drag)`，`drag_started()` 触发 `ViewportCommand::StartDrag`（egui 的 StartDrag 要求鼠标刚按下，语义等同原 >3px 阈值）。
- Esc 隐藏窗口；选中应用回车/点击启动后清空查询并隐藏。

**事件循环（重点，egui 与 Tauri 最大差异）**
- egui 隐藏窗口时 `update()` 不会被自动调用。靠**后台线程**接收 `global-hotkey`/`tray-icon` 事件 → 经 `mpsc::channel` 传主线程 + `ctx.request_repaint()` 唤醒。
- 后台线程每 50ms 轮询 `GlobalHotKeyEvent::receiver()` / `MenuEvent::receiver()`；托盘"退出"直接 `save_current_position()` + `process::exit(0)`（隐藏态也能退出）。
- `update()` 末尾 `ctx.request_repaint_after(50ms)` 保活，确保后台事件及时消费。
- 改快捷键/托盘逻辑注意：manager/tray 必须与 eframe 事件循环同线程创建（放 `App::new`），且对象需保活（存 `App` 字段，drop 即注销）。

**位置持久化（易踩坑）**
- 位置存在**可执行文件同级 `data/position.json`**，不是 OS 标准数据目录——开发时即 `target/<profile>/data/`。
- egui **不暴露窗口外位置读取**，改用 Win32 `FindWindowW(标题)` + `GetWindowRect`（`current_window_position`）。窗口标题常量 `WINDOW_TITLE`（`window_ctl.rs`）必须与 `main.rs` 的 `ViewportBuilder::with_title` 一致，否则 FindWindow 找不到。
- 隐藏/托盘退出时保存。Windows 最小化/隐藏会把位置报成 `(-32000, -32000)` 哨兵值，`is_valid_position` 过滤；加载时 `is_position_on_screen` 用 `EnumDisplayMonitors` 兜底校验显示器范围。

**应用扫描**
- Windows：扫描开始菜单（`ProgramData`/`APPDATA` 下）+ 桌面 `.lnk`；`SHGetFileInfoW(SHGFI_DISPLAYNAME)` 取 Shell 本地化显示名，文件名作为搜索别名。
- 结果缓存在 `static LazyLock<Mutex<Option<Vec<AppInfo>>>>`，首次扫描后驻留。`App::new` 起后台线程 scan + `index_apps`，通过 channel 回传。

**搜索（Rust 移植自原 apps.ts）**
- `search.rs` 用 `pinyin` crate（`ToPinyin`，`plain()` 取无声调拼音）替代 `pinyin-pro`；**连续非汉字按整块保留**（对应 `nonZh: "consecutive"`），首字母取每块首字符。
- `score_name` 打分与原 `apps.ts` 完全一致（精确 1000 > 前缀 900 > 首字母等值 880 > 首字母前缀 760 > 子串 700 > 全拼等值 680 > 全拼前缀 600 > 首字母子串 560 > 全拼子串 500），`MIN_SCORE=500`，单字符不做宽泛 `contains`。改排序逻辑务必同步两边。

**图标（重点）**
- `get_icon_rgba` **直出 RGBA 像素**（跳过 PNG 编码），供 egui `ColorImage::from_rgba_unmultiplied` 零损耗上传纹理。`get_icon`（PNG 版）仅 `#[cfg(test)]` 供测试校验魔数。
- **主线程不提取图标**（COM/GDI 耗时会卡 UI）：`icon_req_tx`/`icon_resp_rx` 双 channel + 后台线程串行提取（`EXTRACT_LOCK`）。`update()` 里 poll 回传建 `TextureHandle`，缺失的路径发请求。
- 像素 LRU 缓存上限 128（`icon.rs`），egui 纹理缓存在 `app.rs` 的 `HashMap<path, IconState>`。
- **图标提取仅 Windows 实现**（手写 COM vtable 解析 `.lnk` → IconLocation/GetPath/PIDL 三级回退，避免小箭头；含 alpha 通道/掩码/反预乘处理）；macOS `extract_icon_rgba` 返回 `None`，UI 留空。
- 改 Windows 图标逻辑务必跑 `cargo test`（含 `resolve_file_explorer_icon` 等针对 PIDL 快捷方式的回归）。

**系统托盘**
- `tray-icon` crate，菜单"退出"。图标从 `icons/32x32.png` 经 `include_bytes!` 嵌入（不依赖运行时文件路径）。

**字体（易踩坑）**
- egui 默认字体**不含中文**，不处理会显示豆腐块。`configure_fonts`（`app.rs`）启动时加载 `C:\Windows\Fonts\msyh.ttc`（微软雅黑），失败回退 `msyh.ttf`/`simhei.ttf`。

**release profile**
- `lto=true`、`codegen-units=1`、`panic=abort`、`strip=true`（激进体积优化，沿用）。
