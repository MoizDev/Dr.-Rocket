import type { CSSProperties, ReactNode } from "react";
import { DISCLOSURE } from "@/lib/demos/fixtures";
import d from "./Demo.module.css";

export type DemoProps = {
  /** The placement classes the replaced media had (e.g. `shot heroShot`). */
  className: string;
};

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
  theme?: "dark" | "light";
  /** The old image had alt="": hide the demo from assistive tech and focus. */
  decorative?: boolean;
  children: ReactNode;
  rootProps?: Record<string, unknown>;
}) {
  const style = { "--demo-w": w, "--demo-h": h, "--demo-pt": w / 2 } as CSSProperties;
  const chipId = `demo-${id}-chip`;
  const a11y = decorative
    ? { "aria-hidden": true, inert: true, "data-decorative-contrast": "" }
    : { role: "group", "aria-roledescription": "interactive demo", "aria-label": label, "aria-describedby": chipId };
  return (
    <div {...rootProps} {...a11y} className={`${className} ${d.demo}`} style={style} data-demo={id} data-theme={theme}>
      <div className={d.stage}>{children}</div>
      {!decorative && (
        <span id={chipId} className={d.srOnly}>
          {DISCLOSURE}
        </span>
      )}
    </div>
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
