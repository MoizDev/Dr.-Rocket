"use client";
// The hero's terminal card (terminal-panel.png). Shares cs135's files with the IDE above, so
// editing grades.rkt there withdraws the raco test result here.
import { useCallback, useRef, useState } from "react";
import DemoRoot from "../DemoRoot";
import Terminal from "../Terminal";
import { useFile } from "@/lib/demos/useFile";
import { useEntryHint, type HintStep } from "@/lib/demos/hint";
import { getText, original } from "@/lib/demos/workspace";
import { status } from "@/lib/demos/fixtures";
import type { TermLine } from "@/lib/demos/data/terminal-commands";

export const LABEL = "An integrated terminal in Dr. Rocket running raco test on an assignment.";

/** The screenshot's scrollback. */
export const REST_LINES: TermLine[] = [
  { kind: "prompt", cwd: "cs135", cmd: "raco test a03/grades.rkt" },
  { kind: "out", text: 'raco test: (submod (file "a03/grades.rkt") test)' },
  { kind: "out", text: "79.75" },
  { kind: "out", text: '(list "A" "B" "A" "C")' },
  { kind: "out", text: "All 4 tests passed!" },
  { kind: "prompt", cwd: "cs135", cmd: "racket notes/scratch.rkt" },
  { kind: "out", text: `'(("the" . 2) ("and" . 1) ("hat" . 1) ("cat" . 1))` },
];

export const files = (id: string) => {
  const text = getText(id);
  return { text, edited: status(text, original(id)) === "edited" };
};

export default function HeroTerminalDemo({ className }: { className: string }) {
  const root = useRef<HTMLDivElement>(null);
  const [lines, setLines] = useState<TermLine[]>(REST_LINES);
  const [typing, setTyping] = useState<string | null>(null);
  const grades = useFile("cs135/a03/grades.rkt");
  const problems = grades.status === "edited" ? ["Results hidden: grades.rkt was edited on this page"] : [];

  const steps = useCallback((): HintStep[] => {
    const cmd = "racket notes/scratch.rkt";
    const out: HintStep[] = [{ at: 0, run: () => (setLines(REST_LINES.slice(0, 5)), setTyping("")) }];
    for (let i = 1; i <= cmd.length; i++) out.push({ at: 250 + i * 55, run: () => setTyping(cmd.slice(0, i)) });
    out.push({ at: 250 + cmd.length * 55 + 300, run: () => (setLines(REST_LINES), setTyping(null)) });
    return out;
  }, []);
  useEntryHint(root, steps, () => (setLines(REST_LINES), setTyping(null)));

  return (
    <DemoRoot id="hero-terminal" className={className} w={1657} h={937} label={LABEL} rootProps={{ ref: root, tabIndex: -1 }}>
      <Terminal project="cs135" lines={lines} setLines={setLines} files={files} problems={problems} variant="panel" typing={typing} />
    </DemoRoot>
  );
}
