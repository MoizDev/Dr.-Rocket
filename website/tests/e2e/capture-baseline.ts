// Records the page's media boxes at four viewports into footprint-baseline.json. Run it against
// a production build only when a demo's size changes on purpose, and say what changed:
//   npx next start -p 3100 & node tests/e2e/capture-baseline.ts http://localhost:3100 "<change>"
import { writeFileSync } from "node:fs";
import { chromium } from "@playwright/test";
import { VIEWPORTS, measure, downloadAnchors } from "./measure.ts";

const base = process.argv[2] ?? "http://localhost:3100";
const browser = await chromium.launch();
const out: Record<string, unknown> = { capturedFrom: process.argv[3] ?? "unknown", viewports: {} };
for (const vp of VIEWPORTS) {
  const ctx = await browser.newContext({ viewport: vp, reducedMotion: "reduce" });
  const page = await ctx.newPage();
  await page.goto(base, { waitUntil: "networkidle" });
  (out.viewports as Record<string, unknown>)[`${vp.width}x${vp.height}`] = await measure(page);
  if (vp.width === 1440) out.downloads = await downloadAnchors(page);
  await ctx.close();
}
await browser.close();
writeFileSync(new URL("./footprint-baseline.json", import.meta.url), JSON.stringify(out, null, 2) + "\n");
console.log("wrote footprint-baseline.json");
