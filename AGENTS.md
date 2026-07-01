# forestools

Tauri v2 + Vue 3 + TypeScript 桌面搜索启动器。

## 关键约束

- **禁止AI 启动/运行程序**（包括 `bun tauri dev`、`bun run dev`、`cargo run` 等）

## 项目结构

```
src/              Vue 前端
  main.ts         入口
  App.vue         单组件视图
src-tauri/        Rust 后端
  src/lib.rs      核心逻辑（窗口管理、全局快捷键）
  src/main.rs     Rust 入口
  tauri.conf.json Tauri 配置
  capabilities/   权限声明
```

## 开发命令

| 用途 | 命令 |
|------|------|
| 类型检查 | `bun run build`（内部跑 `vue-tsc --noEmit && vite build`） |
| Vite 开发服务器 | `bun run dev`（端口 1420） |
| Tauri 开发模式 | `bun tauri dev` |
| Tauri 构建 | `bun tauri build` |

无独立的 lint / 测试命令。

## 架构要点

- **窗口行为**：启动隐藏（`visible: false`），Alt+Space 全局快捷键切换显示/隐藏（Rust 端 `tauri-plugin-global-shortcut` 实现）
- **无系统标题栏**：`decorations: false` + `transparent: true`，CSS `border-radius: 10px` 模拟圆角
- **窗口拖动**：容器监听 `mousedown`，通过手势识别（移动 >3px 触发 `startDragging()`）实现按住拖动，原地点击不影响输入框正常聚焦
- **窗口固定 480×64，不可调整大小**
- **搜索框**：`App.vue` 为唯一视图，挂载后自动聚焦搜索框
- **构建命令链**：`beforeDevCommand` → `bun run dev`，`beforeBuildCommand` → `bun run build`
- **vite 配置**：固定端口 1420，strictPort，忽略 `src-tauri/` 目录变更监听
