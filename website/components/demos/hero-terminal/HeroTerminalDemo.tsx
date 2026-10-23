"use client";
// The hero's terminal card (terminal-panel.png). Shares cs135's files with the IDE above: `cat`
// shows the file as edited there, while a prepared command (raco test, racket) replays the
// original file's prepared output after one line saying so. Nothing is run.
import { useCallback, useRef, useState, type KeyboardEvent } from "react";
import DemoRoot from "../DemoRoot";
import Terminal from "../Terminal";
import { useEntryHint, type HintStep } from "@/lib/demos/hint";
import { getText, original } from "@/lib/demos/workspace";
import { status } from "@/lib/demos/fixtures";
import { PREPARED, projectFiles, type FileState, type TermLine } from "@/lib/demos/data/terminal-commands";

export const LABEL = "An integrated terminal in Dr. Rocket running raco test on an assignment.";

const PROJECT = "cs135";

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

/** A workspace file's current text and whether it was edited on the page. */
export const files = (id: string) => {
  const text = getText(id);
  return { text, edited: status(text, original(id)) === "edited" };
};

/** What the terminal sees: current texts, and prepared output always available (labelled below). */
const replayFiles: FileState = (id) => ({ text: getText(id), edited: false });

const norm = (cmd: string) => cmd.trim().replace(/\s+/g, " ");

/** The edited file a command's prepared output belongs to, if any. */
function editedSource(cmd: string): string | null {
  const prep = PREPARED[PROJECT]?.[norm(cmd)];
  return prep && files(prep.file).edited ? prep.file.split("/").pop()! : null;
}

const COMMANDS = ["cat", "clear", "help", "ls", "pwd", "raco", "racket"];

function commonPrefix(xs: string[]): string {
  let p = xs[0] ?? "";
  for (const x of xs) while (!x.startsWith(p)) p = p.slice(0, -1);
  return p;
}

/** Candidates for the last word of a command line: a command, `test` after raco, else a path. */
function candidates(line: string): { head: string; word: string; hits: string[] } {
  const i = line.lastIndexOf(" ");
  const head = line.slice(0, i + 1);
  const word = line.slice(i + 1);
  let pool: string[];
  if (!head.trim()) pool = COMMANDS;
  else if (head.trim() === "raco") pool = ["test"];
  else {
    // Paths complete one segment at a time, folders ending in "/", as zsh does.
    const dir = word.slice(0, word.lastIndexOf("/") + 1);
    pool = projectFiles(PROJECT)
      .filter((f) => f.startsWith(dir))
      .map((f) => {
        const rest = f.slice(dir.length);
        const slash = rest.indexOf("/");
        return dir + (slash < 0 ? rest : rest.slice(0, slash + 1));
      });
  }
  const hits = [...new Set(pool)].filter((c) => c.startsWith(word)).sort();
  return { head, word, hits };
}

/** Sets a React-controlled input's value as typing would, so the terminal's own state follows. */
function typeInto(el: HTMLInputElement, value: string) {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.setSelectionRange(value.length, value.length);
}

export default function HeroTerminalDemo({ className }: { className: string }) {
  const root = useRef<HTMLDivElement>(null);
  const [lines, setLinesState] = useState<TermLine[]>(REST_LINES);
  const [typing, setTyping] = useState<string | null>(null);

  // After a prepared command whose file was edited in the IDE, one muted line under the prompt
  // says the output is the original's. The edit itself stays untouched.
  const setLines = useCallback((f: (l: TermLine[]) => TermLine[]) => {
    setLinesState((l) => {
      const next = f(l);
      const cmd = next[l.length];
      if (next.length <= l.length || cmd?.kind !== "prompt") return next;
      const name = editedSource(cmd.cmd);
      if (!name) return next;
      return [...next.slice(0, l.length + 1), { kind: "muted", text: `Edited · replaying the original ${name}` }, ...next.slice(l.length + 1)];
    });
  }, []);

  // Tab completion and Ctrl+L at the prompt, from the prepared command names and the file list.
  const onKeyDownCapture = (e: KeyboardEvent<HTMLDivElement>) => {
    const el = e.target as HTMLElement;
    if (!(el instanceof HTMLInputElement) || !el.closest("form")) return;
    if (e.ctrlKey && !e.metaKey && e.key.toLowerCase() === "l") {
      e.preventDefault();
      setLinesState(() => []);
      return;
    }
    if (e.key !== "Tab" || e.shiftKey || e.altKey || e.metaKey || e.ctrlKey) return;
    if (el.selectionStart !== el.value.length) return;
    const { head, word, hits } = candidates(el.value);
    if (!hits.length) return; // Tab moves focus on as usual.
    e.preventDefault();
    e.stopPropagation();
    const add = hits.length === 1 ? hits[0] + (hits[0].endsWith("/") ? "" : " ") : commonPrefix(hits);
    if (add.length > word.length) typeInto(el, head + add);
    else {
      // Several matches and nothing more in common: list them, as the shell does.
      setLinesState((l) => [...l, { kind: "prompt", cwd: PROJECT, cmd: el.value }, { kind: "out", text: hits.join("  ") }]);
      const body = el.closest("form")?.parentElement;
      requestAnimationFrame(() => body && (body.scrollTop = body.scrollHeight));
    }
  };

  const steps = useCallback((): HintStep[] => {
    const cmd = "racket notes/scratch.rkt";
    const out: HintStep[] = [{ at: 0, run: () => (setLinesState(REST_LINES.slice(0, 5)), setTyping("")) }];
    for (let i = 1; i <= cmd.length; i++) out.push({ at: 250 + i * 55, run: () => setTyping(cmd.slice(0, i)) });
    out.push({ at: 250 + cmd.length * 55 + 300, run: () => (setLinesState(REST_LINES), setTyping(null)) });
    return out;
  }, []);
  useEntryHint(root, steps, () => (setLinesState(REST_LINES), setTyping(null)));

  return (
    <DemoRoot id="hero-terminal" className={className} w={1657} h={937} label={LABEL} rootProps={{ ref: root, tabIndex: -1, onKeyDownCapture }}>
      <Terminal project={PROJECT} lines={lines} setLines={setLines} files={replayFiles} problems={[]} variant="panel" typing={typing} />
    </DemoRoot>
  );
}
