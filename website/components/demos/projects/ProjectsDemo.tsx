"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "Three project tabs, cs136, math239 and cs135, with a #lang racket file open and an integrated terminal showing raco test results.";

export default function ProjectsDemo({ className }: { className: string }) {
  return (
    <DemoRoot id="projects" className={className} w={2400} h={1506} label={LABEL}>
      {null}
    </DemoRoot>
  );
}
