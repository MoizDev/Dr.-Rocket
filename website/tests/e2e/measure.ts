// Shared by capture-baseline.ts and footprint.spec.ts: the boxes the demos must keep.
import type { Page } from "@playwright/test";

export const VIEWPORTS = [
  { width: 375, height: 812 },
  { width: 768, height: 1024 },
  { width: 1024, height: 768 },
  { width: 1440, height: 900 },
] as const;

/** Placement classes of the 11 media elements, in page order. */
export const MEDIA = [
  "heroShot",
  "heroTerm",
  "vizShot",
  "cardShot",
  "stackBack",
  "stackFront",
  "cardShot",
  "cardShot",
  "themeBack",
  "themeFront",
  "langShot",
] as const;

export const WRAPPERS = ["heroPanel", "cardInset", "cardStack", "langVisual", "vizStage", "featureCard"] as const;

export type Box = { x: number; y: number; w: number; h: number };
export type Footprint = {
  media: Box[];
  wrappers: Record<string, Box[]>;
  mainHeight: number;
};

/** Settles the page (reveals shown, no hover, no transitions) and measures it. */
export async function measure(page: Page): Promise<Footprint> {
  await page.mouse.move(0, 0);
  await page.evaluate(() => {
    document.querySelectorAll("[data-reveal]").forEach((el) => el.classList.add("is-in"));
    const st = document.createElement("style");
    st.textContent = "*,*::before,*::after{transition:none!important;animation:none!important}";
    document.head.appendChild(st);
  });
  await page.waitForTimeout(100);
  return page.evaluate(
    ({ media, wrappers }) => {
      const box = (el: Element) => {
        const r = el.getBoundingClientRect();
        const r2 = (n: number) => Math.round(n * 100) / 100;
        return { x: r2(r.left + scrollX), y: r2(r.top + scrollY), w: r2(r.width), h: r2(r.height) };
      };
      const cls = (c: string) =>
        Array.from(document.querySelectorAll(`main [class*="${c}"]`)).filter((el) =>
          Array.from(el.classList).some((k) => k === c || k.endsWith(`__${c}`) || k.endsWith(`_${c}`)),
        );
      const used = new Map<string, number>();
      const mediaBoxes = media.map((c) => {
        const i = used.get(c) ?? 0;
        used.set(c, i + 1);
        const el = cls(c)[i];
        if (!el) throw new Error(`missing media .${c} #${i}`);
        return box(el);
      });
      const wrap: Record<string, { x: number; y: number; w: number; h: number }[]> = {};
      for (const c of wrappers) wrap[c] = cls(c).map(box);
      return { media: mediaBoxes, wrappers: wrap, mainHeight: document.querySelector("main")!.scrollHeight };
    },
    { media: [...MEDIA], wrappers: [...WRAPPERS] },
  );
}

/** Every download anchor on the page, for downloads.spec. */
export async function downloadAnchors(page: Page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll<HTMLAnchorElement>("a[href*='download']")).map((a) => ({
      href: a.getAttribute("href"),
      download: a.getAttribute("download"),
      text: (a.textContent ?? "").replace(/\s+/g, " ").trim(),
    })),
  );
}
