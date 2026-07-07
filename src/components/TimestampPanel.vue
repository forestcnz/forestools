<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";

const emit = defineEmits<{
  back: [];
  drag: [e: MouseEvent];
}>();

type Unit = "s" | "ms";

const tsFirstInput = ref<HTMLInputElement>();

// 全局单位（默认毫秒），控制整页
const unit = ref<Unit>("ms");
const unitLabel = computed(() => (unit.value === "s" ? "秒" : "毫秒"));

function toggleUnit() {
  unit.value = unit.value === "s" ? "ms" : "s";
}

// 双击复制：复制后短暂提示
const toast = ref("");
let toastTimer: number | undefined;
function showToast(msg: string) {
  toast.value = msg;
  if (toastTimer) window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toast.value = ""), 1000);
}
async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    window.getSelection()?.removeAllRanges();
    showToast("已复制");
  } catch {
    showToast("复制失败");
  }
}

// 当前时间戳（实时刷新）
const now = ref(Date.now());
const nowTs = computed(() =>
  unit.value === "s" ? Math.floor(now.value / 1000) : now.value,
);
const nowDate = computed(() => formatLocal(now.value));
let timer: number | undefined;

// 时间戳 → 日期时间（按全局单位解读）
const tsInput = ref("");
const tsToDateResult = computed(() => {
  const ms = tsToMs(tsInput.value, unit.value);
  if (ms == null) return "";
  return formatLocal(ms);
});

// 日期时间 → 时间戳（按全局单位输出）
const dateInput = ref("");
const dateToTsResult = computed(() => {
  const ms = parseLocal(dateInput.value);
  if (ms == null) return "";
  return unit.value === "s" ? String(Math.floor(ms / 1000)) : String(ms);
});

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

function formatLocal(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function parseLocal(s: string): number | null {
  const m = s
    .trim()
    .match(/^(\d{4})-(\d{1,2})-(\d{1,2})(?:[ T](\d{1,2}):(\d{1,2})(?::(\d{1,2}))?)?$/);
  if (!m) return null;
  const d = new Date(
    Number(m[1]),
    Number(m[2]) - 1,
    Number(m[3]),
    m[4] ? Number(m[4]) : 0,
    m[5] ? Number(m[5]) : 0,
    m[6] ? Number(m[6]) : 0,
  );
  const t = d.getTime();
  return isNaN(t) ? null : t;
}

function tsToMs(ts: string, u: Unit): number | null {
  const s = ts.trim();
  if (!/^\d+$/.test(s)) return null;
  const n = Number(s);
  return u === "s" ? n * 1000 : n;
}

onMounted(() => {
  tsInput.value = String(now.value);
  dateInput.value = formatLocal(now.value);
  timer = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
  if (toastTimer) window.clearTimeout(toastTimer);
});

function focusFirst(select = false) {
  tsFirstInput.value?.focus();
  if (select) tsFirstInput.value?.select();
}

defineExpose({ focusFirst });
</script>

<template>
  <div class="ts-page">
    <div class="ts-topbar" @mousedown="emit('drag', $event)">
      <button
        class="ts-icon-btn"
        @click="emit('back')"
        title="返回 (Esc)"
        aria-label="返回"
        tabindex="-1"
      >
        <svg
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="15 18 9 12 15 6" />
        </svg>
      </button>
      <span class="ts-title">时间戳转换</span>
      <span class="ts-topbar-spacer"></span>
    </div>

    <div class="ts-body">
      <!-- 当前时间戳 -->
      <div class="ts-now">
        <div class="ts-now-label">当前时间戳</div>
        <div class="ts-now-value">
          <span class="ts-now-num mono" @dblclick="copyText(String(nowTs))">{{ nowTs }}</span>
          <span
            class="ts-now-unit ts-clickable"
            @click="toggleUnit"
            title="点击切换秒/毫秒"
            >{{ unitLabel }}</span
          >
        </div>
        <div class="ts-now-date mono">{{ nowDate }}</div>
      </div>

      <!-- 时间戳 → 日期时间 -->
      <div class="ts-section">
        <div class="ts-section-label">时间戳转日期时间</div>
        <input
          ref="tsFirstInput"
          class="ts-input mono"
          v-model="tsInput"
          spellcheck="false"
          :placeholder="`输入时间戳（${unitLabel}）`"
        />
        <div class="ts-result">
          <span class="ts-result-label">转换结果</span>
          <span
            class="ts-result-value mono"
            @dblclick="tsToDateResult && copyText(tsToDateResult)"
            >{{ tsToDateResult || "—" }}</span
          >
        </div>
      </div>

      <!-- 日期时间 → 时间戳 -->
      <div class="ts-section">
        <div class="ts-section-label">日期时间转时间戳</div>
        <input
          class="ts-input mono"
          v-model="dateInput"
          spellcheck="false"
          placeholder="YYYY-MM-DD HH:mm:ss"
        />
        <div class="ts-result">
          <span class="ts-result-label">转换结果</span>
          <span
            class="ts-result-value mono"
            @dblclick="dateToTsResult && copyText(dateToTsResult)"
            >{{ dateToTsResult || "—" }}</span
          >
        </div>
      </div>
    </div>

    <Transition name="ts-fade">
      <div v-if="toast" class="ts-toast">{{ toast }}</div>
    </Transition>
  </div>
</template>

<style scoped>
.mono {
  font-family: "SF Mono", "JetBrains Mono", "Consolas", "Menlo", monospace;
}

.ts-page {
  display: flex;
  flex-direction: column;
  position: relative;
}

.ts-topbar {
  flex-shrink: 0;
  height: 48px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 8px;
  cursor: default;
}

.ts-icon-btn {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: none;
  border-radius: 8px;
  color: #666;
  cursor: pointer;
}
.ts-icon-btn:hover {
  background: rgba(0, 0, 0, 0.06);
}

.ts-title {
  flex: 1;
  text-align: center;
  font-size: 15px;
  font-weight: 600;
}

.ts-topbar-spacer {
  width: 32px;
  flex-shrink: 0;
}

.ts-body {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 0 16px 18px;
}

.ts-clickable {
  cursor: pointer;
}
.ts-clickable:hover {
  color: #0071e3;
}

/* 当前时间戳卡片 */
.ts-now {
  padding: 14px;
  background: rgba(0, 113, 227, 0.06);
  border-radius: 10px;
}

.ts-now-label {
  font-size: 12px;
  font-weight: 600;
  color: #555;
  white-space: nowrap;
}

.ts-now-value {
  display: flex;
  align-items: baseline;
  gap: 6px;
  margin-top: 4px;
}

.ts-now-num {
  font-size: 22px;
  font-weight: 600;
  color: #0071e3;
  letter-spacing: 0.5px;
  white-space: nowrap;
  user-select: text;
}

.ts-now-unit {
  font-size: 13px;
  color: #888;
  flex-shrink: 0;
  user-select: none;
}

.ts-now-date {
  font-size: 12px;
  color: #666;
  margin-top: 4px;
  white-space: nowrap;
}

/* 转换区 */
.ts-section {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.ts-section-label {
  font-size: 12px;
  font-weight: 600;
  color: #444;
  white-space: nowrap;
}

.ts-input {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  font-size: 14px;
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 8px;
  outline: none;
  color: inherit;
  background: #fff;
  user-select: text;
  transition: border-color 0.15s;
}
.ts-input:focus {
  border-color: #0071e3;
}

.ts-result {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: 13px;
  padding: 8px 12px;
  background: rgba(0, 0, 0, 0.03);
  border-radius: 8px;
  min-height: 34px;
}

.ts-result-label {
  color: #999;
  font-size: 11px;
  flex-shrink: 0;
  white-space: nowrap;
}

.ts-result-value {
  flex: 1;
  min-width: 0;
  color: #333;
  text-align: right;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  user-select: text;
}

/* 复制提示 toast */
.ts-toast {
  position: absolute;
  bottom: 14px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(0, 0, 0, 0.78);
  color: #fff;
  font-size: 12px;
  padding: 6px 14px;
  border-radius: 16px;
  pointer-events: none;
  z-index: 99;
  white-space: nowrap;
}

.ts-fade-enter-active,
.ts-fade-leave-active {
  transition: opacity 0.18s;
}
.ts-fade-enter-from,
.ts-fade-leave-to {
  opacity: 0;
}

@media (prefers-color-scheme: dark) {
  .ts-icon-btn {
    color: #aaa;
  }
  .ts-icon-btn:hover {
    background: rgba(255, 255, 255, 0.08);
  }
  .ts-title {
    color: #e5e5e7;
  }
  .ts-clickable:hover {
    color: #64a8ff;
  }
  .ts-now {
    background: rgba(100, 168, 255, 0.1);
  }
  .ts-now-label {
    color: #bbb;
  }
  .ts-now-num {
    color: #64a8ff;
  }
  .ts-now-unit {
    color: #888;
  }
  .ts-now-date {
    color: #aaa;
  }
  .ts-section-label {
    color: #ccc;
  }
  .ts-input {
    border-color: rgba(255, 255, 255, 0.12);
    background: rgba(255, 255, 255, 0.05);
  }
  .ts-input:focus {
    border-color: #64a8ff;
  }
  .ts-result {
    background: rgba(255, 255, 255, 0.05);
  }
  .ts-result-label {
    color: #777;
  }
  .ts-result-value {
    color: #ddd;
  }
}
</style>
