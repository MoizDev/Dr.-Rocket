// Every demo must take exactly the box its image or video had (±1px), at four widths, and the
// page around it must not move.
import { expect, test } from "@playwright/test";
import baseline from "./footprint-baseline.json" with { type: "json" };
import { VIEWPORTS, measure, MEDIA, type Box } from "./measure";

const near = (a: Box, b: Box, what: string) => {
  for (const k of ["x", "y", "w", "h"] as const) expect(Math.abs(a[k] - b[k]), `${what}.${k}: ${a[k]} vs ${b[k]}`).toBeLessThanOrEqual(1);
};

for (const vp of VIEWPORTS) {
  test(`footprint ${vp.width}x${vp.height}`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.setViewportSize(vp);
    await page.goto("/");
    await page.evaluate(() => document.fonts.ready);
    const got = await measure(page);
    const want = (baseline.viewports as Record<string, typeof got>)[`${vp.width}x${vp.height}`];
    got.media.forEach((b, i) => near(b, want.media[i], `${MEDIA[i]}#${i}`));
    for (const [c, boxes] of Object.entries(want.wrappers)) {
      expect(got.wrappers[c].length, c).toBe(boxes.length);
      boxes.forEach((b, i) => near(got.wrappers[c][i], b, `${c}#${i}`));
    }
    expect(Math.abs(got.mainHeight - want.mainHeight)).toBeLessThanOrEqual(1);
  });
}
