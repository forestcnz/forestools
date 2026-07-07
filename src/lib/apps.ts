import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { pinyin } from "pinyin-pro";

export interface AppInfo {
  name: string;
  path: string;
  aliases?: string[];
}

/** 内置应用类型（非系统扫描出来的真实可执行文件）。 */
export type BuiltinKind = "timestamp";

/** 内置应用清单：参与正常搜索索引，选中后由前端展开对应功能面板。 */
export const BUILTIN_APPS: (AppInfo & { kind: BuiltinKind })[] = [
  {
    kind: "timestamp",
    name: "时间戳转换",
    path: "builtin:timestamp",
    aliases: ["时间戳", "timestamp", "ts", "sjc"],
  },
];

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

/** 一个可搜索名称的索引（主名称或别名都各自索引）。 */
interface IndexedName {
  raw: string;
  lower: string;
  pinyinFull: string;
  initials: string;
}

/** 预计算每个应用的匹配键（主名称 + 别名，各自建立名称/全拼/首字母索引）。 */
export interface IndexedApp extends AppInfo {
  /** names[0] 为主名称，其后为别名。 */
  names: IndexedName[];
  /** 内置应用类型；undefined 表示外部扫描到的真实应用。 */
  kind?: BuiltinKind;
}

const pinyinOpts = { toneType: "none", type: "array", nonZh: "consecutive" } as const;

function indexName(raw: string): IndexedName {
  const arr = pinyin(raw, pinyinOpts) as unknown as string[];
  return {
    raw,
    lower: raw.toLowerCase(),
    pinyinFull: arr.join("").toLowerCase(),
    initials: arr.map((s) => (s ? s[0] : "")).join("").toLowerCase(),
  };
}

export function indexApps(apps: AppInfo[]): IndexedApp[] {
  return apps.map((a) => {
    const names = [indexName(a.name), ...(a.aliases ?? []).map(indexName)];
    return { ...a, names };
  });
}

export interface Match {
  app: IndexedApp;
  score: number;
}

/** 相关性下限：低于该分数的匹配不展示，避免无关结果混入。 */
const MIN_SCORE = 500;

/** 对单个名称计算相关性分数（-1 表示不匹配）。 */
function scoreName(n: { lower: string; pinyinFull: string; initials: string }, q: string, short: boolean): number {
  if (n.lower === q) return 1000;
  if (n.lower.startsWith(q)) return 900;
  if (n.initials === q) return 880;
  if (n.initials.startsWith(q)) return 760;
  if (!short && n.lower.includes(q)) return 700;
  if (n.pinyinFull === q) return 680;
  if (n.pinyinFull.startsWith(q)) return 600;
  // 首字母子串匹配：支持「yc -> 远程」「kz -> 控制中心」这类
  // 只输入目标词首字母即命中，即便目标词不在名称开头。
  if (!short && n.initials.includes(q)) return 560;
  if (!short && n.pinyinFull.includes(q)) return 500;
  return -1;
}

/** 按相关性搜索并排序。每个应用的主名称与别名都参与匹配，取最高分。 */
export function search(indexed: IndexedApp[], query: string): Match[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  // 单字符查询不做宽泛的 includes 匹配，避免一次命中大量无关应用。
  const short = q.length === 1;
  const out: Match[] = [];
  for (const app of indexed) {
    let best = -1;
    for (const n of app.names) {
      const s = scoreName(n, q, short);
      if (s > best) best = s;
    }
    if (best >= MIN_SCORE) {
      out.push({ app, score: best });
    }
  }
  out.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return a.app.names[0].lower.length - b.app.names[0].lower.length;
  });
  return out.slice(0, 8);
}
