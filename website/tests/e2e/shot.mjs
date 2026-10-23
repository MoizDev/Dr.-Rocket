// Dev helper: screenshot one demo. node tests/e2e/shot.mjs <base> <demo> <out.png> [width]
import { chromium } from "@playwright/test";
const [base, demo, out, w = "1440"] = process.argv.slice(2);
const b = await chromium.launch();
const p = await (await b.newContext({ viewport: { width: +w, height: 900 }, reducedMotion: "reduce", deviceScaleFactor: 2 })).newPage();
p.on("pageerror", (e) => console.log("PAGEERR", e.message));
p.on("console", (m) => m.type() === "error" && console.log("CONSOLE", m.text().slice(0, 300)));
await p.goto(base, { waitUntil: "networkidle" });
await p.evaluate(() => document.querySelectorAll("[data-reveal]").forEach((e) => e.classList.add("is-in")));
const el = p.locator(`[data-demo="${demo}"]`);
await el.scrollIntoViewIfNeeded();
await p.waitForTimeout(1500);
await el.screenshot({ path: out });
await b.close();
