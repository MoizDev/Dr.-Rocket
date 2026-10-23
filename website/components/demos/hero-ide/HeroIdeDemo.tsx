"use client";
import { useRef } from "react";
import DemoRoot from "../DemoRoot";
import GradesIde from "./GradesIde";

export const LABEL = "Dr. Rocket with an Intermediate Student program: the file tree on the left, check-expect results passing inline, and the REPL on the right showing the results of a run.";

export default function HeroIdeDemo({ className }: { className: string }) {
  const root = useRef<HTMLDivElement>(null);
  return (
    <DemoRoot id="hero-ide" className={className} w={2400} h={1506} label={LABEL} rootProps={{ ref: root, tabIndex: -1 }}>
      <GradesIde gradesId="cs135/a03/grades.rkt" hint rootRef={root} />
    </DemoRoot>
  );
}
