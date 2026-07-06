// 生成 forestools 图标源文件（1024x1024 PNG）——「极夜黑 · 神奇海螺」
// 用法: bun run scripts/gen-icon.mjs
import { Resvg } from "@resvg/resvg-js";
import { writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");

// —— 对数螺旋海螺几何（与 docs/logo-conch-colors.html 一致）——
const cx = 50, cy = 50, a = 7, b = 0.16, t0 = 0, t1 = 9.9, steps = 200, W = 5.4, rot = 112;
const spiral = (off) =>
  Array.from({ length: steps + 1 }, (_, i) => {
    const t = t0 + ((t1 - t0) * i) / steps;
    const r = Math.max(0.2, a * Math.exp(b * t) + off);
    return [cx + r * Math.cos(t), cy + r * Math.sin(t)];
  });
const to = (p) => p.map((q) => q[0].toFixed(2) + " " + q[1].toFixed(2));
const outer = spiral(0);
const inner = spiral(-W).reverse();
const D_SHELL = "M" + to(outer).join(" L") + " L" + to(inner).join(" L") + " Z";
const D_LINE = "M" + to(spiral(-W / 2)).join(" L");

// —— 极夜黑配色 ——
const C1 = "#1F2228"; // 渐变起（深）
const C2 = "#3A3D44"; // 渐变止
const SHELL = "#E8EAEE"; // 壳体
const LINE = "#0F1115"; // 壳内描线

const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="${C1}"/>
      <stop offset="1" stop-color="${C2}"/>
    </linearGradient>
    <radialGradient id="sheen" cx="0.32" cy="0.24" r="0.85">
      <stop offset="0" stop-color="#FFFFFF" stop-opacity="0.10"/>
      <stop offset="0.55" stop-color="#FFFFFF" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="100" height="100" rx="22" fill="url(#bg)"/>
  <rect width="100" height="100" rx="22" fill="url(#sheen)"/>
  <g transform="rotate(${rot} 50 50)">
    <path d="${D_SHELL}" fill="${SHELL}"/>
    <path d="${D_LINE}" fill="none" stroke="${LINE}" stroke-opacity="0.28" stroke-width="1.6" stroke-linecap="round"/>
    <path d="M78 26 L79.5 31 L84.5 32.5 L79.5 34 L78 39 L76.5 34 L71.5 32.5 L76.5 31 Z" fill="${SHELL}" fill-opacity="0.92"/>
    <path d="M30 74 L31 77 L34 78 L31 79 L30 82 L29 79 L26 78 L29 77 Z" fill="${SHELL}" fill-opacity="0.7"/>
  </g>
</svg>`;

const resvg = new Resvg(svg, {
  fitTo: { mode: "width", value: 1024 },
  background: "#00000000",
});
const png = resvg.render().asPng();
const out = resolve(ROOT, "app-icon.png");
writeFileSync(out, png);
console.log("written:", out, png.length, "bytes");
