"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "An integrated terminal in Dr. Rocket running raco test on an assignment.";

export default function HeroTerminalDemo({ className }: { className: string }) {
  return (
    <DemoRoot id="hero-terminal" className={className} w={1657} h={937} label={LABEL}>
      {null}
    </DemoRoot>
  );
}
