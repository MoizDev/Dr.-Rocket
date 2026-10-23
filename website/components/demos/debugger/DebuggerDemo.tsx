"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "The debugger paused at a breakpoint inside tree-sum, with variables and a call stack of recursive calls.";

export default function DebuggerDemo({ className }: { className: string }) {
  return (
    <DemoRoot id="debugger" className={className} w={2000} h={887} label={LABEL}>
      {null}
    </DemoRoot>
  );
}
