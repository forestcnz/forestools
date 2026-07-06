<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from "vue";
import { getCurrentWindow, LogicalSize, currentMonitor } from "@tauri-apps/api/window";
import {
  scanApps,
  indexApps,
  search,
  openApp,
  iconUrl,
  type IndexedApp,
} from "./lib/apps";

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

/** 根据结果数量调整窗口高度（宽度固定，由启动时按屏幕比例确定）。 */
async function fitWindow() {
  const count = results.value.length;
  const height = count === 0 ? BASE_HEIGHT : BASE_HEIGHT + 8 + Math.min(count, MAX_ITEMS) * ITEM_HEIGHT;
  try {
    await getCurrentWindow().setSize(new LogicalSize(windowWidth, height));
  } catch {
    // 忽略尺寸调整失败
  }
}

watch(results, () => {
  selectedIndex.value = 0;
  fitWindow();
});

async function launch(app: IndexedApp) {
  try {
    await openApp(app.path);
  } catch (e) {
    console.error("启动失败:", e);
  }
  searchQuery.value = "";
  await getCurrentWindow().hide();
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    getCurrentWindow().hide();
    return;
  }
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
  // 窗口获得焦点时聚焦输入框
  const win = getCurrentWindow();
  win.onFocusChanged(({ payload: focused }) => {
    if (focused) inputRef.value?.focus();
  });

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

  // 后台加载应用列表（不阻塞 UI）
  scanApps()
    .then((apps) => {
      allApps.value = indexApps(apps);
    })
    .catch((e) => console.error("扫描应用失败:", e));
});
</script>

<template>
  <main class="container">
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
