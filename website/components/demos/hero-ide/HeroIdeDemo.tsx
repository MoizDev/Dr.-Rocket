"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "Dr. Rocket with an Intermediate Student program: the file tree on the left, check-expect results passing inline, and the REPL on the right showing the results of a run.";

export default function HeroIdeDemo({ className }: { className: string }) {
  return (
    <DemoRoot id="hero-ide" className={className} w={2400} h={1506} label={LABEL}>
      {null}
    </DemoRoot>
  );
}
