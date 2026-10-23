// Page-wide checks for the demos: all present, only Check Syntax's screenshot left, downloads
// unchanged, accessible, idle after the hint, and no layout shift from the lazy editors.
import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { gzipSync } from "node:zlib";
import baseline from "./footprint-baseline.json" with { type: "json" };
import { downloadAnchors } from "./measure";

const IDS = ["hero-ide", "hero-terminal", "visualizer", "stepper", "debugger", "rich-source", "projects", "theme-dark", "theme-light", "languages"];

test("all 10 demos replace the media, in order; Check Syntax keeps its screenshot", async ({ page }) => {
  await page.goto("/");
  expect(await page.locator("main [data-demo]").evaluateAll((els) => els.map((e) => e.getAttribute("data-demo")))).toEqual(IDS);
  // CodeMirror's empty cursor-buffer <img>s around widgets aren't media.
  const media = page.locator("main img:not(.cm-widgetBuffer), main video, main picture");
  expect(await media.evaluateAll((els) => els.map((e) => e.getAttribute("src") ?? e.tagName))).toEqual([expect.stringContaining("checksyntax")]);
  for (const id of IDS.filter((i) => i !== "theme-dark")) {
    const root = page.locator(`[data-demo="${id}"]`);
    await expect(root).toHaveAttribute("role", "group");
    await expect(root).toHaveAttribute("aria-roledescription", "interactive demo");
    const desc = await root.getAttribute("aria-describedby");
    await expect(page.locator(`#${desc}`)).toHaveText("Simulated preview");
    await expect(root.locator("[data-disclosure]"), id).toHaveCount(1);
  }
  const dark = page.locator('[data-demo="theme-dark"]');
  await expect(dark).toHaveAttribute("aria-hidden", "true");
  expect(await dark.evaluate((e) => (e as HTMLElement).inert)).toBe(true);
});

test("renders complete without JavaScript", async ({ browser }) => {
  const ctx = await browser.newContext({ javaScriptEnabled: false });
  const page = await ctx.newPage();
  await page.goto("/");
  await expect(page.locator("main [data-demo]")).toHaveCount(IDS.length);
  for (const id of ["hero-ide", "rich-source", "stepper"]) {
    const text = await page.locator(`[data-demo="${id}"]`).innerText();
    expect(text.length, id).toBeGreaterThan(80);
  }
  await ctx.close();
});

test("each editor has a sun/moon toggle that flips only its own demo", async ({ page }) => {
  await page.goto("/");
  const owners = await page.locator("[data-theme-toggle]").evaluateAll((els) => els.map((e) => e.closest("[data-demo]")!.getAttribute("data-demo")));
  expect(owners).toEqual(["hero-ide", "visualizer", "stepper", "debugger", "rich-source", "projects"]);
  const viz = page.locator('[data-demo="visualizer"]');
  await viz.scrollIntoViewIfNeeded();
  await expect(viz.locator(".cm-content")).toBeAttached(); // hydrated
  await viz.getByRole("button", { name: "Switch to the light theme" }).click();
  await expect(viz).toHaveAttribute("data-theme", "light");
  await expect(page.locator('[data-demo="hero-ide"]')).toHaveAttribute("data-theme", "dark");
  await viz.getByRole("button", { name: "Switch to the dark theme" }).click();
  await expect(viz).toHaveAttribute("data-theme", "dark");
});

test("download links are unchanged", async ({ page }) => {
  await page.goto("/");
  expect(await downloadAnchors(page)).toEqual(baseline.downloads);
});

test("no serious or critical accessibility violations", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await page.evaluate(() => document.querySelectorAll("[data-reveal]").forEach((el) => el.classList.add("is-in")));
  const scan = (b: AxeBuilder) => b.include("main [data-demo]");
  const all = await scan(new AxeBuilder({ page }).disableRules(["color-contrast"])).analyze();
  // Contrast is checked everywhere except what reproduces the app's own themes: syntax colours
  // (tk-*), inactive line numbers, muted dialog chrome, test badges and the dimmed back card.
  const contrast = await scan(new AxeBuilder({ page }).withRules(["color-contrast"]).exclude("[data-decorative-contrast]").exclude('[class*="tk-"]')).analyze();
  const bad = [...all.violations, ...contrast.violations].filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(bad.map((v) => `${v.id}: ${v.nodes.slice(0, 3).map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
});

test("demos go idle after their entry hint", async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as unknown as { __intervals: number; __raf: number };
    w.__intervals = 0;
    w.__raf = 0;
    const si = window.setInterval;
    window.setInterval = ((...a: Parameters<typeof si>) => (w.__intervals++, si(...a))) as typeof si;
    const raf = window.requestAnimationFrame;
    window.requestAnimationFrame = ((cb: FrameRequestCallback) => (w.__raf++, raf(cb))) as typeof raf;
  });
  await page.goto("/");
  for (const id of IDS) {
    await page.locator(`[data-demo="${id}"]`).scrollIntoViewIfNeeded();
    await page.waitForTimeout(150);
  }
  await page.waitForTimeout(3000);
  const quiet = async () =>
    page.evaluate(() => {
      const w = window as unknown as { __raf: number };
      const running = document
        .getAnimations()
        .filter((a) => a.playState === "running" && (a.effect as KeyframeEffect | null)?.target instanceof Element && ((a.effect as KeyframeEffect).target as Element).closest("[data-demo]"));
      return { running: running.length, raf: w.__raf, hints: document.querySelectorAll("[data-demo][data-hint]").length };
    });
  const a = await quiet();
  await page.waitForTimeout(1500);
  const b = await quiet();
  expect(a.running).toBe(0);
  expect(a.hints).toBe(0);
  expect(b.raf - a.raf, "requestAnimationFrame calls while idle").toBe(0);
});

test("reduced motion: no entry hints run", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  let seen = 0;
  for (const id of IDS) {
    await page.locator(`[data-demo="${id}"]`).scrollIntoViewIfNeeded();
    await page.waitForTimeout(100);
    seen += await page.locator("[data-demo][data-hint]").count();
  }
  expect(seen).toBe(0);
});

test("lazy editors cause no layout shift", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "layout-shift entries are Chromium-only");
  await page.goto("/");
  await page.evaluate(() => {
    (window as unknown as { __cls: number }).__cls = 0;
    new PerformanceObserver((l) => {
      for (const e of l.getEntries() as unknown as { value: number; hadRecentInput: boolean }[]) if (!e.hadRecentInput) (window as unknown as { __cls: number }).__cls += e.value;
    }).observe({ type: "layout-shift", buffered: true });
  });
  for (const id of IDS) {
    await page.locator(`[data-demo="${id}"]`).scrollIntoViewIfNeeded();
    await page.waitForTimeout(250);
  }
  await expect(page.locator('[data-demo="hero-ide"] [data-live]').first()).toBeAttached();
  expect(await page.evaluate(() => (window as unknown as { __cls: number }).__cls)).toBeLessThan(0.02);
});

test("CodeMirror is not in the initial bundle, and stays within budget", async ({ page, request, baseURL }) => {
  // The initial bundle is the scripts the HTML names. They load async, so when they arrive can't
  // tell them apart from the lazy chunks.
  const html = await (await request.get("/")).text();
  const initial = [...html.matchAll(/<script[^>]+src="([^"]+)"/g)].map((m) => new URL(m[1], baseURL).href);
  let initialHasCm = false;
  for (const u of initial) if ((await (await request.get(u)).text()).includes("cm-scroller")) initialHasCm = true;
  expect(initialHasCm).toBe(false);
  const scripts: string[] = [];
  page.on("response", (r) => r.request().resourceType() === "script" && scripts.push(r.url()));
  await page.goto("/");
  await page.locator('[data-demo="hero-ide"] [data-live]').first().waitFor({ state: "attached" });
  for (const id of IDS) await page.locator(`[data-demo="${id}"]`).scrollIntoViewIfNeeded();
  await page.waitForTimeout(1000);
  const lazy = [...new Set(scripts)].filter((u) => !initial.includes(u));
  expect(lazy.length, "lazy demo chunks").toBeGreaterThan(0);
  let bytes = 0;
  for (const u of lazy) bytes += gzipSync(await (await request.get(u)).body()).length;
  expect(bytes, "lazy demo chunks, gzip").toBeLessThanOrEqual(180 * 1024);
});
