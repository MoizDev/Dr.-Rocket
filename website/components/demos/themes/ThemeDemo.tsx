"use client";
import { useRef } from "react";
import DemoRoot from "../DemoRoot";
import GradesIde from "../hero-ide/GradesIde";

export const LABEL = "The same program in the One Light theme.";

/**
 * theme-light is the editable front card; theme-dark (alt="" before) is an inert, read-only
 * mirror of the same file. Both use the theme section's own copy of grades.rkt.
 */
export default function ThemeDemo({ className, theme }: { className: string; theme: "dark" | "light" }) {
  const root = useRef<HTMLDivElement>(null);
  const dark = theme === "dark";
  return (
    <DemoRoot
      id={`theme-${theme}`}
      className={className}
      w={2400}
      h={1506}
      label={LABEL}
      theme={theme}
      decorative={dark}
      rootProps={dark ? undefined : { ref: root, tabIndex: -1 }}
    >
      <GradesIde gradesId="themes/grades.rkt" readOnlyMirror={dark} rootRef={root} />
    </DemoRoot>
  );
}
