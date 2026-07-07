<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from "vue";
import { getCurrentWindow, LogicalSize, currentMonitor } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import {
  scanApps,
  indexApps,
  search,
  openApp,
  iconUrl,
  BUILTIN_APPS,
  type IndexedApp,
} from "./lib/apps";
import TimestampPanel from "./components/TimestampPanel.vue";

/** 内置「时间戳转换」应用结果列表中的图标（时钟 SVG，主题色描边）。 */
const TIMESTAMP_ICON =
  "data:image/svg+xml," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#0071e3" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>',
  );

const BASE_HEIGHT = 56; // 搜索栏高度（逻辑像素）
const ITEM_HEIGHT = 44; // 单条结果高度
const MAX_ITEMS = 8;

const searchQuery = ref("");
const inputRef = ref<HTMLInputElement>();
const selectedIndex = ref(0);
let dragStartX = 0;
let dragStartY = 0;
let windowWidth = 480; // 启动时按屏幕 2/5 重算

const allApps = ref<IndexedApp[]>([]);
const matches = computed(() => search(allApps.value, searchQuery.value));
const results = computed(() => matches.value.map((m) => m.app));

const tsPanelRef = ref<InstanceType<typeof TimestampPanel>>();
const tsView = ref<HTMLElement>();
let unlistenShow: (() => void) | undefined;
let unlistenAltSpace: (() => void) | undefined;

type View = "search" | "timestamp";

const EMBEDDED_PARAMS = new URLSearchParams(window.location.search);
const isEmbedded = EMBEDDED_PARAMS.get("view") === "timestamp";
if (isEmbedded) {
  const w = EMBEDDED_PARAMS.get("w");
  if (w) windowWidth = parseInt(w) || windowWidth;
}
const view = ref<View>(isEmbedded ? "timestamp" : "search");

/** 根据结果数量调整窗口高度（宽度固定，由启动时按屏幕比例确定）。 */
async function fitWindow() {
  await nextTick();
  let height = BASE_HEIGHT;
  if (view.value === "timestamp" && tsView.value) {
    height = tsView.value.offsetHeight + 2;
  } else {
    const count = results.value.length;
    if (count > 0) {
      height = BASE_HEIGHT + 8 + Math.min(count, MAX_ITEMS) * ITEM_HEIGHT;
    }
  }
  try {
    await getCurrentWindow().setSize(new LogicalSize(windowWidth, height));
  } catch {
    // 忽略尺寸调整失败
  }
}

watch([view, results], () => {
  selectedIndex.value = 0;
  fitWindow();
});

async function launch(app: IndexedApp) {
  if (app.kind === "timestamp") {
    searchQuery.value = "";
    const win = getCurrentWindow();
    const pos = await win.outerPosition();
    const size = await win.outerSize();
    const scaleFactor = size.width / windowWidth;
    new WebviewWindow(`timestamp-${Date.now()}`, {
      url: `/?view=timestamp&w=${windowWidth}`,
      title: "时间戳转换",
      x: Math.round(pos.x / scaleFactor),
      y: Math.round(pos.y / scaleFactor),
      width: windowWidth,
      height: BASE_HEIGHT,
      decorations: false,
      transparent: true,
      resizable: false,
      visible: false,
    });
    await win.hide();
    return;
  }
  try {
    await openApp(app.path);
  } catch (e) {
    console.error("启动失败:", e);
  }
  searchQuery.value = "";
  await getCurrentWindow().hide();
}

function onKeyDown(e: KeyboardEvent) {
  if (results.value.length === 0) return;

  if (e.key === "ArrowDown") {
    e.preventDefault();
    selectedIndex.value = (selectedIndex.value + 1) % results.value.length;
    scrollIntoView();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    selectedIndex.value =
      (selectedIndex.value - 1 + results.value.length) % results.value.length;
    scrollIntoView();
  } else if (e.key === "Enter") {
    e.preventDefault();
    const app = results.value[selectedIndex.value];
    if (app) launch(app);
  }
}

/** 退出时间戳页面，回到搜索。时间戳独立窗口中则关闭窗口。 */
function backToSearch() {
  if (isEmbedded) {
    getCurrentWindow().close();
    return;
  }
  view.value = "search";
  searchQuery.value = "";
  nextTick(() => inputRef.value?.focus());
  fitWindow();
}

function onEmbeddedKeyDown(e: KeyboardEvent) {
  if (e.key === "Escape") getCurrentWindow().close();
}

/** 全局 Esc：时间戳页面下返回搜索；搜索页面下隐藏窗口。 */
function onGlobalKeyDown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    if (view.value === "timestamp") backToSearch();
    else getCurrentWindow().hide();
  }
}

function scrollIntoView() {
  nextTick(() => {
    const el = document.querySelector<HTMLElement>(`.result-item.selected`);
    el?.scrollIntoView({ block: "nearest" });
  });
}

function onMouseDown(e: MouseEvent) {
  if (e.button !== 0) return;
  dragStartX = e.clientX;
  dragStartY = e.clientY;

  const onMouseMove = (ev: MouseEvent) => {
    if (
      Math.abs(ev.clientX - dragStartX) > 3 ||
      Math.abs(ev.clientY - dragStartY) > 3
    ) {
      cleanup();
      getCurrentWindow().startDragging();
    }
  };
  const onMouseUp = () => cleanup();
  const cleanup = () => {
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
  };
  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
}

onMounted(async () => {
  const win = getCurrentWindow();

  if (isEmbedded) {
    window.addEventListener("keydown", onEmbeddedKeyDown);
    win.onFocusChanged(({ payload: focused }) => {
      if (focused) tsPanelRef.value?.focusFirst();
    });
    await nextTick();
    tsPanelRef.value?.focusFirst(true);
    await fitWindow();
    await win.show();
    await win.setFocus();
    return;
  }

  // ── 以下为主搜索窗口的初始化 ──
  window.addEventListener("keydown", onGlobalKeyDown);

  win.onFocusChanged(({ payload: focused }) => {
    if (!focused) return;
    inputRef.value?.focus();
  });

  // 窗口每次从隐藏状态显示时（比如 Alt+Space 唤起），重置回搜索页面
  listen("window-shown", () => {
    view.value = "search";
    searchQuery.value = "";
  }).then((fn) => { unlistenShow = fn; });

  // 窗口可见时按 Alt+Space：隐藏搜索窗口
  listen("alt-space-pressed", () => {
    getCurrentWindow().hide();
  }).then((fn) => { unlistenAltSpace = fn; });

  await nextTick();
  inputRef.value?.focus();

  // 按当前屏幕宽度的 2/5 设置窗口宽度
  try {
    const monitor = await currentMonitor();
    if (monitor) {
      const screenWidthLogical = monitor.size.width / monitor.scaleFactor;
      windowWidth = Math.round(screenWidthLogical * 2 / 5);
    }
  } catch {
    // 取不到显示器信息则沿用默认宽度
  }
  fitWindow();

  // 后台加载应用列表（不阻塞 UI），内置应用置顶参与索引
  scanApps()
    .then((apps) => {
      allApps.value = indexApps([...BUILTIN_APPS, ...apps]);
    })
    .catch((e) => console.error("扫描应用失败:", e));
});

onUnmounted(() => {
  window.removeEventListener("keydown", onGlobalKeyDown);
  window.removeEventListener("keydown", onEmbeddedKeyDown);
  unlistenShow?.();
  unlistenAltSpace?.();
});
</script>

<template>
  <main class="container">
    <template v-if="view === 'search'">
      <div class="search-bar" @mousedown="onMouseDown">
        <input
          ref="inputRef"
          v-model="searchQuery"
          type="text"
          class="search-input"
          placeholder="神奇的海螺"
          spellcheck="false"
          @keydown="onKeyDown"
        />
      </div>
      <div v-if="results.length" class="result-list">
        <div
          v-for="(app, index) in results.slice(0, MAX_ITEMS)"
          :key="app.path"
          class="result-item"
          :class="{ selected: index === selectedIndex }"
          @mousemove="selectedIndex = index"
          @click="launch(app)"
        >
          <img
            v-if="app.kind === 'timestamp'"
            class="result-icon"
            :src="TIMESTAMP_ICON"
            alt=""
            draggable="false"
          />
          <img
            v-else
            class="result-icon"
            :src="iconUrl(app.path)"
            alt=""
            draggable="false"
            @error="(e) => ((e.target as HTMLImageElement).style.visibility = 'hidden')"
          />
          <div class="result-text">
            <div class="result-name">{{ app.name }}</div>
            <div class="result-path">{{ app.path }}</div>
          </div>
        </div>
      </div>
    </template>
    <div v-else ref="tsView" class="ts-view">
      <TimestampPanel ref="tsPanelRef" @back="backToSearch" @drag="onMouseDown" />
    </div>
  </main>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

:root {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica,
    Arial, sans-serif;
  font-size: 14px;
  line-height: 1.2;
  font-weight: 400;
  color: #1a1a1a;
  user-select: none;
}

html,
body {
  height: 100%;
  background: transparent;
}

#app {
  height: 100%;
  overflow: hidden;
}

.container {
  height: 100%;
  display: flex;
  flex-direction: column;
  background-color: #ffffff;
  border: 1px solid rgba(0, 0, 0, 0.1);
  border-radius: 12px;
  overflow: hidden;
}

.search-bar {
  height: 56px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
}

.search-input {
  width: 100%;
  height: 100%;
  padding: 0 18px;
  font-size: 20px;
  font-family: inherit;
  border: none;
  outline: none;
  color: inherit;
  background: transparent;
}

.search-input::placeholder {
  color: #999;
}

.ts-view {
  flex-shrink: 0;
  overflow-y: auto;
  scrollbar-width: none;
}

.ts-view::-webkit-scrollbar {
  display: none;
}

.result-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 8px 8px;
  scrollbar-width: none;
}

.result-list::-webkit-scrollbar {
  display: none;
}

.result-item {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 44px;
  padding: 0 10px;
  border-radius: 8px;
  cursor: pointer;
}

.result-item.selected {
  background-color: rgba(0, 113, 227, 0.12);
}

.result-icon {
  width: 28px;
  height: 28px;
  object-fit: contain;
  flex-shrink: 0;
}

.result-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.result-name {
  font-size: 14px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.result-path {
  font-size: 11px;
  color: #888;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #e5e5e7;
  }

  .container {
    background-color: rgb(40, 40, 42);
    border-color: rgba(255, 255, 255, 0.12);
  }

  .result-item.selected {
    background-color: rgba(100, 168, 255, 0.2);
  }

  .result-path {
    color: #888;
  }
}
</style>
