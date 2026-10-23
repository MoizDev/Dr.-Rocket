"use client";
import { createContext, useContext, useMemo, useState, type CSSProperties, type ReactNode } from "react";
import { DISCLOSURE } from "@/lib/demos/fixtures";
import { Icon } from "./icons";
import d from "./Demo.module.css";

export type DemoProps = {
  /** The placement classes the replaced media had (e.g. `shot heroShot`). */
  className: string;
};

type Theme = "dark" | "light";

/** The demo's colour theme and how to flip it; null where the theme is fixed. */
const ThemeContext = createContext<{ theme: Theme; toggle: () => void } | null>(null);

/**
 * The box a demo lives in: the same element and classes as the image it replaces, with the
 * image's aspect ratio, so the page's placement CSS sizes it exactly as before.
 */
export default function DemoRoot({
  id,
  className,
  w,
  h,
  label,
  theme = "dark",
  themeToggle = true,
  decorative,
  children,
  rootProps,
}: {
  id: string;
  className: string;
  /** Intrinsic pixel size of the replaced media (@2x). */
  w: number;
  h: number;
  /** The replaced media's alt text, word for word. */
  label: string;
  /** The theme it starts in. */
  theme?: Theme;
  /** Whether a ThemeToggle inside may flip the theme (each demo on its own). */
  themeToggle?: boolean;
  /** The old image had alt="": hide the demo from assistive tech and focus. */
  decorative?: boolean;
  children: ReactNode;
  rootProps?: Record<string, unknown>;
}) {
  const [current, setCurrent] = useState(theme);
  const switcher = useMemo(
    () => (themeToggle && !decorative ? { theme: current, toggle: () => setCurrent((t) => (t === "dark" ? "light" : "dark")) } : null),
    [themeToggle, decorative, current],
  );
  const style = { "--demo-w": w, "--demo-h": h, "--demo-pt": w / 2 } as CSSProperties;
  const chipId = `demo-${id}-chip`;
  const a11y = decorative
    ? { "aria-hidden": true, inert: true, "data-decorative-contrast": "" }
    : { role: "group", "aria-roledescription": "interactive demo", "aria-label": label, "aria-describedby": chipId };
  return (
    <div {...rootProps} {...a11y} className={`${className} ${d.demo}`} style={style} data-demo={id} data-theme={current}>
      <ThemeContext.Provider value={switcher}>
        <div className={d.stage}>{children}</div>
      </ThemeContext.Provider>
      {!decorative && (
        <span id={chipId} className={d.srOnly}>
          {DISCLOSURE}
        </span>
      )}
    </div>
  );
}

/** The sun/moon button in an editor's tab bar: flips its demo between dark and light. */
export function ThemeToggle({ className = "" }: { className?: string }) {
  const t = useContext(ThemeContext);
  if (!t) return null;
  const next = t.theme === "dark" ? "light" : "dark";
  const label = `Switch to the ${next} theme`;
  return (
    <button type="button" className={`${d.themeToggle} ${className}`} aria-label={label} title={label} onClick={t.toggle} data-theme-toggle>
      <Icon name={next === "light" ? "sun" : "moon"} size={14} />
    </button>
  );
}

/** The visible disclosure chip, placed in each demo's status bar or panel header. */
export function DisclosureChip() {
  return (
    <span className={d.chip} data-disclosure aria-hidden="true">
      {DISCLOSURE}
    </span>
  );
}
