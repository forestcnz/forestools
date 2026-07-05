# forestools

Tauri v2 + Vue 3 + TypeScript 桌面应用启动器（Alt+Space 唤起，搜索并启动已安装应用）。

## 关键约束

- **禁止 AI 启动/运行程序**（包括 `bun tauri dev`、`bun run dev`、`cargo run` 等）。改完代码做类型检查即可，运行交由用户。
- 包管理器/运行时统一用 **bun**（`tauri.conf.json` 的 `beforeDevCommand` / `beforeBuildCommand` 都写死 `bun run ...`）。

## 仓库与 Git

- 远程：`https://github.com/forestcnz/forestools.git`（`origin/master`）。
- 本仓库 `.git/config` 里配置了 **local** 代理 `http.proxy` / `https.proxy` → `http://127.0.0.1:7890`（Clash）。开发者本地开了 VPN，但 git 需显式走代理才能连通 GitHub；**仅 local，非全局**。
  - 端口变更：`git config --local http.proxy http://127.0.0.1:<端口>`（`https.proxy` 同步）。
  - 取消代理：`git config --local --unset http.proxy` 和 `--unset https.proxy`。
  - 若 `git pull/push` 报 `Failed to connect to github.com port 443`，先检查 Clash 是否在跑、端口是否一致。

## 开发命令

| 用途 | 命令 | 说明 |
|------|------|------|
| 前端类型检查 + 构建 | `bun run build` | 内部跑 `vue-tsc --noEmit && vite build`，作为类型检查入口 |
| Vite 开发服务器 | `bun run dev` | 固定端口 1420，`strictPort` |
| Tauri 开发模式 | `bun tauri dev` | 会先自动跑 `bun run dev` |
| Tauri 构建 | `bun tauri build` | 会先自动跑 `bun run build` |
| Rust 测试 | `cargo test`（在 `src-tauri/`） | 仅 Windows 有集成测试，依赖真实开始菜单/应用存在 |

无独立 lint 命令；前端类型检查靠 `bun run build`。

## 项目结构

```
src/                      Vue 前端
  main.ts                 入口
  App.vue                 唯一视图（搜索框 + 结果列表 + 拖动）
  lib/apps.ts             扫描/索引/搜索/图标 URL（拼音索引 + 相关性打分）
src-tauri/                Rust 后端
  src/main.rs             入口（release 下 windows_subsystem，禁用控制台窗口）
  src/lib.rs              插件注册、命令、窗口/快捷键/托盘、位置持久化、appicon 协议
  src/app_launcher.rs     应用扫描（带缓存）+ 启动；Windows .lnk / macOS .app
  src/icon.rs             图标提取 + appicon:// 协议处理（LRU 缓存 + 串行锁）
  tauri.conf.json         Tauri 配置
  capabilities/default.json  权限声明
```

## 架构要点

**窗口与交互**
- 启动隐藏（`visible: false`），**Alt+Space** 全局快捷键切换显示/隐藏（`tauri-plugin-global-shortcut`，Rust 端注册）。
- 无系统标题栏：`decorations: false` + `transparent: true`，CSS `border-radius` 模拟圆角。
- 宽度固定 480，但**高度随结果数量动态变化**：前端 `fitWindow()` 调用 `getCurrentWindow().setSize()`（`resizable:false` 不阻止代码改尺寸）。别把高度当常量。
- 窗口拖动：容器 `mousedown` 监听，移动 >3px 才触发 `startDragging()`，原地点击仍能聚焦输入框。改交互时别破坏这个阈值。
- Esc 隐藏窗口；选中应用回车启动后清空查询并隐藏。

**位置持久化（易踩坑）**
- 位置存在 **可执行文件同级 `data/position.json`**，不是 OS 标准数据目录——开发时即 `src-tauri/target/<profile>/data/`。
- `WindowEvent::Moved`、关闭、快捷键隐藏、托盘退出四处都会保存。
- Windows 最小化/隐藏会把窗口位置报成 `(-32000, -32000)` 哨兵值，`is_valid_position` 会过滤；加载时还用 `is_position_on_screen` 兜底校验显示器范围。

**应用扫描**
- Windows：扫描开始菜单（`ProgramData`/`APPDATA` 下）+ 桌面 `.lnk`；用 `SHGetFileInfoW(SHGFI_DISPLAYNAME)` 取 Shell 本地化显示名，文件名作为搜索别名。
- macOS：扫描 `/Applications`、`/System/Applications`、`~/Applications` 的 `.app`。
- 结果缓存在 `static LazyLock<Mutex<Option<Vec<AppInfo>>>>`，首次扫描后驻留，每次唤起不重扫。
- `should_skip` 过滤卸载/帮助/文档等快捷方式。

**搜索（前端）**
- `src/lib/apps.ts` 用 `pinyin-pro` 对每个名称（主名称 + 别名）预计算三套索引：原文小写、全拼、首字母。
- `scoreName` 按命中类型打分（精确 > 前缀 > 首字母 > 全拼 > 子串），低于 `MIN_SCORE=500` 不展示；单字符查询不做宽泛 `includes`。
- 单字符/短查询行为有特判，改动排序逻辑时注意。

**图标（重点）**
- 自定义 URI scheme **`appicon`**：前端 `iconUrl(path)` = `convertFileSrc(path, "appicon")`（Windows 为 `http://appicon.localhost/<path>`）。Rust 端 `register_uri_scheme_protocol("appicon", ...)` 处理请求，路径经 `percent_decode`。
- 图标 PNG **只驻留内存**（LRU 上限 128 + 串行提取锁），不落盘。
- **图标提取仅 Windows 实现**（手写 COM vtable 解析 `.lnk` → IconLocation/GetPath/PIDL 三级回退，避免小箭头；含 alpha 通道/掩码/反预乘处理）；macOS/Linux `extract_icon_png` 返回 `None`，前端降级。
- 改 Windows 图标逻辑务必跑 `cargo test`（含 `resolve_file_explorer_icon` 等针对 PIDL 快捷方式的回归）。

**系统托盘**
- `lib.rs` setup 里建托盘菜单（"退出"），退出前保存窗口位置。

**Vite / 构建**
- `vite.config.ts` 固定端口 1420、`strictPort`、忽略 `**/src-tauri/**` 监听。
- Rust release profile 激进体积优化（`lto=true`、`codegen-units=1`、`panic=abort`、`strip=true`）。
