"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "A Beginning Student file saved by DrRacket with a pasted rocket image, and the REPL showing the images it produced.";

export default function RichSourceDemo({ className }: { className: string }) {
  return (
    <DemoRoot id="rich-source" className={className} w={2400} h={1506} label={LABEL}>
      {null}
    </DemoRoot>
  );
}
