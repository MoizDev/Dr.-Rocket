"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "The same program in the One Light theme.";

/** theme-light is the editable front card; theme-dark is an inert mirror (alt="" before). */
export default function ThemeDemo({ className, theme }: { className: string; theme: "dark" | "light" }) {
  return (
    <DemoRoot id={`theme-${theme}`} className={className} w={2400} h={1506} label={LABEL} theme={theme} decorative={theme === "dark"}>
      {null}
    </DemoRoot>
  );
}
