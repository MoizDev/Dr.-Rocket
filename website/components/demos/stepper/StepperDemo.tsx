"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "The stepper showing a cond expression before and after one reduction, with the matching code highlighted in the editor.";

export default function StepperDemo({ className }: { className: string }) {
  return (
    <DemoRoot id="stepper" className={className} w={2400} h={1506} label={LABEL}>
      {null}
    </DemoRoot>
  );
}
