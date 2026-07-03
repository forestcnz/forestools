import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { pinyin } from "pinyin-pro";

export interface AppInfo {
  name: string;
  path: string;
}

interface ScanResult {
  apps: AppInfo[];
}

/**
 * 应用图标 URL：走自定义协议 `appicon`，由 Rust 端按需提取 PNG。
 * 使用 convertFileSrc 以匹配平台（Windows 上为 `http://appicon.localhost/<path>`，
 * macOS/Linux 为 `appicon://localhost/<path>`）。
 */
export function iconUrl(path: string): string {
  return convertFileSrc(path, "appicon");
}

/** 调用后端扫描已安装应用（带缓存）。 */
export async function scanApps(): Promise<AppInfo[]> {
  const res = await invoke<ScanResult>("scan_apps");
  return res.apps ?? [];
}

/** 启动指定路径的应用。 */
export async function openApp(path: string): Promise<void> {
  await invoke("open_app", { path });
}

/** 预计算每个应用的匹配键（名称、全拼、首字母缩写）。 */
export interface IndexedApp extends AppInfo {
  nameLower: string;
  pinyinFull: string;
  initials: string;
}

const pinyinOpts = { toneType: "none", type: "array", nonZh: "consecutive" } as const;

export function indexApps(apps: AppInfo[]): IndexedApp[] {
  return apps.map((a) => {
    const arr = pinyin(a.name, pinyinOpts) as unknown as string[];
    return {
      ...a,
      nameLower: a.name.toLowerCase(),
      pinyinFull: arr.join("").toLowerCase(),
      initials: arr.map((s) => (s ? s[0] : "")).join("").toLowerCase(),
    };
  });
}

export interface Match {
  app: IndexedApp;
  score: number;
}

/** 相关性下限：低于该分数的匹配不展示，避免无关结果混入。 */
const MIN_SCORE = 500;

/** 按相关性搜索并排序。 */
export function search(indexed: IndexedApp[], query: string): Match[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  // 单字符查询不做宽泛的 includes 匹配，避免一次命中大量无关应用。
  const short = q.length === 1;
  const out: Match[] = [];
  for (const app of indexed) {
    let score = -1;
    if (app.nameLower === q) score = 1000;
    else if (app.nameLower.startsWith(q)) score = 900;
    else if (app.initials === q) score = 880;
    else if (app.initials.startsWith(q)) score = 760;
    else if (!short && app.nameLower.includes(q)) score = 700;
    else if (app.pinyinFull === q) score = 680;
    else if (app.pinyinFull.startsWith(q)) score = 600;
    else if (!short && app.pinyinFull.includes(q)) score = 500;

    if (score >= MIN_SCORE) {
      out.push({ app, score });
    }
  }
  out.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return a.app.nameLower.length - b.app.nameLower.length;
  });
  return out.slice(0, 8);
}
