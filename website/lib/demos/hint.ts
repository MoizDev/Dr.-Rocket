"use client";
// The entry hint: once per page load, when ≥35% of the demo is visible and the tab is visible.
// It runs ≤ 2.5 s, never under reduced motion, and stops at once on any input inside the demo.
// It must end on the static (rest) state. Afterwards nothing runs: no timers, no rAF.
import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";

export const HINT_MAX_MS = 2500;

export type HintStep = { at: number; run: () => void };

/**
 * `steps` are applied at their times (ms, all < 2500). `finish` jumps to the rest state; it is
 * called when the hint ends or is interrupted, and must be idempotent.
 */
export function useEntryHint(root: RefObject<HTMLElement | null>, start: () => HintStep[], finish: () => void, enabled = true) {
  const fns = useRef({ start, finish });
  useLayoutEffect(() => {
    fns.current = { start, finish };
  });
  useEffect(() => {
    const el = root.current;
    if (!el || !enabled) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    if (document.documentElement.dataset.demoHints === "off") return;
    let timers: number[] = [];
    let state: "waiting" | "running" | "done" = "waiting";
    const stop = () => {
      if (state === "done") return;
      const wasRunning = state === "running";
      state = "done";
      timers.forEach(clearTimeout);
      timers = [];
      io.disconnect();
      for (const t of EVENTS) el.removeEventListener(t, stop, true);
      document.removeEventListener("visibilitychange", onVis);
      if (wasRunning) fns.current.finish();
      el.removeAttribute("data-hint");
    };
    const onVis = () => document.hidden && stop();
    const begin = () => {
      state = "running";
      el.setAttribute("data-hint", "running");
      for (const s of fns.current.start()) timers.push(window.setTimeout(s.run, Math.min(s.at, HINT_MAX_MS - 50)));
      timers.push(window.setTimeout(stop, HINT_MAX_MS));
    };
    const io = new IntersectionObserver(
      (es) => {
        if (state === "waiting" && !document.hidden && es.some((e) => e.intersectionRatio >= 0.35)) begin();
      },
      { threshold: [0.35] },
    );
    io.observe(el);
    for (const t of EVENTS) el.addEventListener(t, stop, true);
    document.addEventListener("visibilitychange", onVis);
    return stop;
  }, [root, enabled]);
}

const EVENTS = ["pointerdown", "keydown", "wheel", "focusin"] as const;

/**
 * After Run demo: lights up the step controls one after another, left to right, so a visitor
 * sees they can be pressed. A tinted fill and ring that fade back within a second, drawn as a
 * box-shadow because no control sets one (their colours are `!important`).
 */
export function flashInTurn(els: Iterable<HTMLElement>) {
  [...els].forEach((el, k) => {
    const css = getComputedStyle(el);
    const ring = css.getPropertyValue("--d-accent").trim();
    const fill = css.getPropertyValue("--d-hl-strong").trim();
    el.animate([{ offset: 0.35, boxShadow: `inset 0 0 0 1px ${ring}, inset 0 0 0 40px ${fill}` }], {
      id: "run-hint",
      duration: 450,
      delay: k * 110,
      easing: "ease-in-out",
    });
  });
}
