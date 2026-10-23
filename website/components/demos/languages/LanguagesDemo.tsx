"use client";
import DemoRoot from "../DemoRoot";

export const LABEL = "The Choose Language dialog listing the Racket language, the How to Design Programs teaching languages and DeinProgramm, with output and fraction settings for Beginning Student.";

export default function LanguagesDemo({ className }: { className: string }) {
  return (
    <DemoRoot id="languages" className={className} w={1762} h={1281} label={LABEL}>
      {null}
    </DemoRoot>
  );
}
