# forestools

纯 Rust（egui）桌面应用启动器（Alt+Space 唤起，拼音搜索并启动已安装应用）。专注 Windows。

## 关键约束

- **禁止 AI 启动/运行程序**（包括 `cargo run`、直接运行 exe 等）。改完代码做类型检查（`cargo check`）或测试（`cargo test`）即可，运行 GUI 交由用户。
- 无前端工具链；仓库是单一 Cargo 包（根目录 `Cargo.toml`）。
- **`cargo build` 若报 `failed to remove forestools.exe (os error 5)`**：旧进程还在跑，先 `taskkill /F /IM forestools.exe` 再编译。

## 网络 / 代理

- git local 代理已配（`.git/config` → `http://127.0.0.1:7890`，Clash）。
- **cargo 不走 git 代理**，须单独设环境变量，否则下载依赖报 `Failed to connect to index.crates.io port 443`：
  ```powershell
  $env:HTTP_PROXY="http://127.0.0.1:7890"; $env:HTTPS_PROXY="http://127.0.0.1:7890"
  ```

## 开发命令

| 用途 | 命令 | 说明 |
|------|------|------|
| 类型检查 | `cargo check` | 首选验证入口（无 warning/error 即过） |
| 编译 | `cargo build` | debug 编译 |
| 测试 | `cargo test` | 含图标回归测试，依赖真实开始菜单/应用存在 |
| 运行 | `cargo run` | 启动 GUI，**AI 禁止执行** |

## 架构要点（大量踩坑经验，改前必读）

### 窗口显隐：Win32 ShowWindow，绝不用 egui Visible

**这是最关键的架构决策。** egui `ViewportCommand::Visible(false)` 隐藏窗口后 `update()` 会永久停止，无法再被唤醒——全局快捷键事件无法处理，程序"死"掉。

正确做法（当前实现）：
- `ViewportBuilder::with_visible(true)` —— egui 始终认为窗口可见，`update()` 持续运行。
- `App::new` 里调 `window_ctl::hide_main_window()`（Win32 `ShowWindow(SW_HIDE)`）实际隐藏。
- Alt+Space 切换走后台线程 → `show_main_window()`/`hide_main_window()`（Win32 `SW_SHOW`/`SW_HIDE`）。
- 后台线程每 50ms 轮询 `GlobalHotKeyEvent::receiver()`；收到事件后 `ctx.request_repaint()` 唤醒 `update()`。

### 圆角与窗口高度：不透明 + SetWindowRgn 裁剪

- **不使用 `with_transparent`**：透明窗口 resize 时 GL surface 重建会疯狂抖动，初始化还会闪现系统标题栏。
- 窗口物理高度固定为 `MAX_WINDOW_HEIGHT`（搜索栏 + 8 条结果），**永不 resize**（resize swapchain = 抖动）。
- 实际可见高度由 `SetWindowRgn`（`CreateRoundRectRgn`）动态裁剪到内容高度（`new_height`），同时实现物理圆角。
- `clear_color` 返回不透明背景色（按系统深浅色）。

### 窗口拖动：手动 SetWindowPos，绝不用 StartDrag

- eframe 的 `ViewportCommand::StartDrag` **不会进入系统 modal 拖动**（`update()` 不暂停），无法检测拖动结束。
- 当前实现：pointer 按下搜索栏 → 记录鼠标屏幕坐标 + 窗口起始位置；移动时 `SetWindowPos` 实时跟随；**松手（`primary_down` 变 false）立即 `save_current_position()` 保存**。

### 位置持久化

- 存于**可执行文件同级** `data/position.json`（开发时即 `target/<profile>/data/`）。
- 保存时机：拖动松手、`hide_main_window`、托盘退出。
- 恢复时机：`main.rs` 的 `with_position`（创建时定位）+ 首个 `update()` 的 `restore_position()`（SetWindowPos 兜底）。
- egui 不暴露窗口位置读取，用 Win32 `FindWindowW(WINDOW_TITLE)` + `GetWindowRect`。
- `WINDOW_TITLE`（`window_ctl.rs`）必须与 `main.rs` 的 `with_title` 一致，否则所有 Win32 窗口操作失效。
- `(-32000, -32000)` 哨兵值由 `is_valid_position` 过滤；`is_position_on_screen` 用 `EnumDisplayMonitors` 兜底。

### 图标提取（主线程禁止）

- COM/GDI 提取耗时会卡 UI。双 mpsc channel（`icon_req_tx`/`icon_resp_rx`）+ 后台线程串行提取（`EXTRACT_LOCK`）。
- `get_icon_rgba` 直出 RGBA 像素（跳过 PNG 编码），egui `ColorImage::from_rgba_unmultiplied` 零损耗上传纹理。
- 像素 LRU 缓存 128；egui 纹理缓存在 `app.rs` 的 `HashMap<path, IconState>`。
- `.lnk` 解析：手写 COM vtable（IShellLinkW/IPersistFile）→ IconLocation/GetPath/PIDL 三级回退，避免小箭头。改图标逻辑务必跑 `cargo test`（含 `resolve_file_explorer_icon` 回归）。

### 搜索（Rust 移植自原 apps.ts）

- `pinyin` crate 替代 `pinyin-pro`；连续非汉字按整块保留（对应 `nonZh: "consecutive"`）。
- 打分阈值**不可随意改**：精确 1000 > 前缀 900 > 首字母等值 880 > 首字母前缀 760 > 子串 700 > 全拼等值 680 > 全拼前缀 600 > 首字母子串 560 > 全拼子串 500，`MIN_SCORE=500`，单字符不做宽泛 `contains`。

### 字体

egui 默认字体不含中文（显示豆腐块）。`configure_fonts`（`app.rs`）启动时加载 `C:\Windows\Fonts\msyh.ttc`。

### 三个后台线程

1. 事件轮询（快捷键 + 托盘菜单），50ms 间隔。
2. 图标提取（串行，按需）。
3. 应用扫描 + `index_apps`（启动一次）。

manager/tray 必须与 eframe 事件循环同线程创建（放 `App::new`），存 `App` 字段保活（drop 即注销）。

## 项目结构

```
src/
  main.rs           入口：eframe::run_native + ViewportBuilder + with_position
  app.rs            eframe::App：UI、事件循环、手动拖动、图标纹理缓存、3 个后台线程
  search.rs         拼音索引 + 打分（pinyin crate）
  app_launcher.rs   应用扫描（带缓存）+ 启动
  icon.rs           图标提取（COM vtable）+ get_icon_rgba（LRU + 串行锁）
  window_ctl.rs     Win32 窗口控制（ShowWindow/SetWindowPos/SetWindowRgn/FindWindowW）+ 位置持久化
  theme.rs          浅色/暗色颜色常量
icons/              图标资源（include_bytes! 嵌入）
```

## 已知遗留

- `README.md` 仍为 Tauri+Vue 模板内容，已过时。
- release profile：`lto=true`、`codegen-units=1`、`panic=abort`、`strip=true`。
