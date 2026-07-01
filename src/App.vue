<script setup lang="ts">
import { ref, onMounted, nextTick } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

const searchQuery = ref("");
const inputRef = ref<HTMLInputElement>();
let dragStartX = 0;
let dragStartY = 0;

function onMouseDown(e: MouseEvent) {
  if (e.button !== 0) return;
  dragStartX = e.clientX;
  dragStartY = e.clientY;

  const onMouseMove = (ev: MouseEvent) => {
    if (Math.abs(ev.clientX - dragStartX) > 3 || Math.abs(ev.clientY - dragStartY) > 3) {
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
  await nextTick();
  inputRef.value?.focus();
});
</script>

<template>
  <main class="container" @mousedown="onMouseDown">
    <input
      ref="inputRef"
      v-model="searchQuery"
      type="text"
      class="search-input"
      placeholder="搜索..."
    />
  </main>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

:root {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  font-size: 14px;
  line-height: 1;
  font-weight: 400;
  color: #1a1a1a;
  user-select: none;
}

html, body {
  height: 100%;
  background: transparent;
}

#app {
  height: 100%;
  overflow: hidden;
  background-color: #ffffff;
}

.container {
  height: 100%;
  display: flex;
}

.search-input {
  width: 100%;
  height: 100%;
  padding: 0 14px;
  font-size: 15px;
  font-family: inherit;
  border: none;
  outline: none;
  color: #1a1a1a;
  background-color: #ffffff;
}

.search-input::placeholder {
  color: #999;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #e5e5e7;
  }

  #app {
    background-color: #3a3a3c;
  }

  .search-input {
    color: #e5e5e7;
    background-color: #3a3a3c;
  }

  .search-input::placeholder {
    color: #888;
  }
}
</style>