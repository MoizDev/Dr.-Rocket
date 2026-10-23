// Dev tool: screenshots each demo placement at 1440 and 375 (evidence for reviews).
// node tests/e2e/evidence-shots.ts <base-url> <out-dir>
import { chromium } from "@playwright/test";
const [base, dir] = process.argv.slice(2);
const sel: [string, string, number?][] = [["hero","heroPanel"],["visualizer","vizStage"],["stepper","cardInset",0],["bindings","cardStack",0],["rich-source","cardInset",1],["projects","cardInset",2],["themes","cardStack",1],["languages","langVisual"]];
const b = await chromium.launch();
for (const w of [1440, 375]) {
  const p = await (await b.newContext({ viewport: { width: w, height: 900 }, reducedMotion: "reduce", deviceScaleFactor: 2 })).newPage();
  await p.goto(base, { waitUntil: "networkidle" });
  await p.evaluate(() => document.querySelectorAll("[data-reveal]").forEach((e) => e.classList.add("is-in")));
  for (const [name, c, i = 0] of sel) {
    const el = p.locator(`main [class*="${c}"]`).nth(i);
    await el.scrollIntoViewIfNeeded(); await p.waitForTimeout(400);
    await el.screenshot({ path: `${dir}/${name}-${w}.png` });
  }
  await p.screenshot({ path: `${dir}/page-${w}.png`, fullPage: true });
}
await b.close();
