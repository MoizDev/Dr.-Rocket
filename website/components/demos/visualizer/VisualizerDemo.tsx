"use client";
// The Visualizer: bst.rkt on the left, the prepared trace of its run on the right. Every step
// comes from lib/demos/data/visualizer-trace.json (generated at development time); nothing runs.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import DemoRoot, { DisclosureChip, ThemeToggle } from "../DemoRoot";
import Code from "../Code";
import Notice from "../Notice";
import KeysHelp, { EDITOR_KEYS } from "../KeysHelp";
import { Icon } from "../icons";
import { FileIcon } from "../file-icons";
import { useFile } from "@/lib/demos/useFile";
import { remapRange } from "@/lib/demos/fixtures";
import { flashInTurn, useEntryHint } from "@/lib/demos/hint";
import type { CodeDecor } from "@/lib/demos/editor/types";
import type { Seg, VStep } from "@/lib/demos/bst-walk";
import traceJson from "@/lib/demos/data/visualizer-trace.json";
import Stage from "./Stage";
import { Glyph } from "./glyphs";
import s from "./Visualizer.module.css";

export const LABEL =
  "The Visualizer replaying a binary search tree program at 2x: frames for each insert and tree-sum call appear, the tree is drawn node by node, and each step is explained in plain words.";

const FILE = "cs135/a05/bst.rkt";
const TRACE = traceJson as unknown as VStep[];
export const REST = TRACE.findIndex((t) => t.rest);
const SPEEDS = [0.5, 1, 2, 4];
const MAX_DEPTH = Math.max(...TRACE.map((t) => t.depth));

const KEYS: [string, string][] = [
  ["← / →", "Previous / next step (timeline)"],
  ["Home / End", "First / last step"],
  ["Space", "Play or pause (timeline)"],
  ["Click a frame or node", "Jump to the step where it began"],
  ["Drag / ⌘-scroll", "Pan / zoom the drawing"],
  ...EDITOR_KEYS,
];

/** The first step where each tree node exists. */
const NODE_STEP = new Map<number, number>();
TRACE.forEach((t, j) => t.nodes.forEach((k) => NODE_STEP.has(k) || NODE_STEP.set(k, j)));

/** The step where frame `k` of step `i` was pushed: the latest CALL at or before `i` that made the stack k+1 deep. */
export function frameStart(i: number, k: number) {
  if (k === 0) return 0;
  for (let j = i; j >= 0; j--) if (TRACE[j].kind === "CALL" && TRACE[j].frames.length === k + 1) return j;
  return 0;
}

export function lineOfPos(text: string, pos: number) {
  let n = 1;
  for (let i = 0; i < pos && i < text.length; i++) if (text[i] === "\n") n++;
  return n;
}

/** The step's highlights in the editor, moved to the current text (or none if it no longer applies). */
export function stepDecor(step: VStep, original: string, text: string): CodeDecor {
  const marks: NonNullable<CodeDecor["marks"]> = [];
  const after: NonNullable<CodeDecor["after"]> = [];
  const lines: NonNullable<CodeDecor["lines"]> = [];
  for (const h of step.hl) {
    const r = remapRange(h, original, text);
    if (!r) return {};
    marks.push({ ...r, cls: h.kind === "cur" ? s.hlCur : h.kind === "ret" ? s.hlRet : s.hlDone });
    if (h.note) after.push({ line: lineOfPos(text, r.to), text: h.note, cls: s.note });
    if (h.kind !== "done") lines.push({ line: lineOfPos(text, r.from), cls: s.markerLine });
  }
  return { marks, after, lines };
}

function Segs({ segs }: { segs: Seg[] }) {
  return (
    <>
      {segs.map((g, i) =>
        g.k === "code" || g.k === "val" ? (
          <code key={i} className={s.chipCode}>
            {g.s}
          </code>
        ) : (
          <span key={i} className={g.k === "fn" || g.k === "var" ? s.name : undefined}>
            {g.s}
          </span>
        ),
      )}
    </>
  );
}

export default function VisualizerDemo({ className }: { className: string }) {
  const file = useFile(FILE);
  // After an edit the same prepared run still plays; only the editor highlights drop if they no
  // longer fit the text. Nothing the visitor typed is run.
  const edited = file.status === "edited";
  const [i, setI] = useState(REST);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(2);
  const [tab, setTab] = useState(2);
  const [expanded, setExpanded] = useState(false);
  const [work, setWork] = useState(false);
  const [cursor, setCursor] = useState({ line: 1, col: 1 });
  const [announce, setAnnounce] = useState("");
  const [recenter, setRecenter] = useState(0);
  const [peek, setPeek] = useState<number | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const step = TRACE[i];

  const go = useCallback(
    (n: number, user = true) => {
      const j = Math.max(0, Math.min(TRACE.length - 1, n));
      setI(j);
      if (user) {
        const t = TRACE[j];
        setAnnounce(`Step ${j + 1} of ${TRACE.length}. ${t.kind}. ${t.lines[0]?.map((g) => g.s).join("") ?? ""}`);
      }
    },
    [],
  );

  // Play: a timeout chain that stops at the end, on pause, and when the window loses focus or
  // the tab is hidden. Never loops.
  useEffect(() => {
    if (!playing) return;
    if (i >= TRACE.length - 1) return;
    const t = window.setTimeout(() => {
      go(i + 1, false);
      if (i + 1 >= TRACE.length - 1) setPlaying(false);
    }, 900 / speed);
    const stop = () => setPlaying(false);
    const vis = () => document.hidden && stop();
    window.addEventListener("blur", stop);
    document.addEventListener("visibilitychange", vis);
    return () => {
      clearTimeout(t);
      window.removeEventListener("blur", stop);
      document.removeEventListener("visibilitychange", vis);
    };
  }, [playing, i, speed, go]);

  useEntryHint(
    root,
    () => {
      setI(REST - 6);
      return [1, 2, 3, 4, 5, 6].map((k) => ({ at: k * 350, run: () => setI(REST - 6 + k) }));
    },
    () => setI(REST),
  );

  const decor = useMemo(() => stepDecor(step, file.original, file.text), [step, file.original, file.text]);
  const overIdx = (dir: "over" | "out") => {
    const d = step.depth;
    for (let j = i + 1; j < TRACE.length; j++) if (dir === "over" ? TRACE[j].depth <= d : TRACE[j].depth < d) return j;
    return TRACE.length - 1;
  };
  // Roving focus in the step bar: one tool is the Tab stop, arrows move between them.
  const [toolFocus, setToolFocus] = useState(2);
  const tool = (k: number) => ({ tabIndex: k === toolFocus ? 0 : -1, "data-tool": true });
  const btn = (k: number, label: string, icon: Parameters<typeof Glyph>[0]["name"], run: () => void, extra?: object) => (
    <button type="button" className={s.tbtn} aria-label={label} title={label} onClick={run} {...tool(k)} {...extra}>
      <Glyph name={icon} />
    </button>
  );
  const togglePlay = () => {
    if (!playing && i >= TRACE.length - 1) go(0, false);
    setPlaying((p) => !p);
  };
  const runDemo = () => {
    go(0, false);
    setAnnounce(`Running the prepared demo from step 1 of ${TRACE.length}.`);
    setPlaying(true);
    flashInTurn(root.current?.querySelectorAll<HTMLElement>("[data-transport]") ?? []);
  };

  const onToolbarKey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const items = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("[data-tool]"));
    const k = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      items[(k + (e.key === "ArrowRight" ? 1 : items.length - 1)) % items.length]?.focus();
    } else if (e.key === "Home") {
      e.preventDefault();
      go(0);
    } else if (e.key === "End") {
      e.preventDefault();
      go(TRACE.length - 1);
    }
  };
  const restore = () => file.set(file.original);
  const tabs = ["Interactions", "Stepper", "Visualizer", "Tests", "Macro Stepper"] as const;
  const tabIcons = ["terminal", "stepper", "viz", "tests", "macro"] as const;

  return (
    <DemoRoot id="visualizer" className={className} w={1920} h={1362} label={LABEL}>
      <div ref={root} className={s.root} data-expanded={expanded || undefined} data-edited={edited || undefined}>
        <header className={s.title}>
          <span className={s.menu} aria-hidden="true">
            <Glyph name="menu" />
          </span>
          <span className={s.project}>cs135</span>
          <span className={s.plus} aria-hidden="true">
            <Icon name="add" size={15} />
          </span>
          <span className={s.titleRight}>
            <span className={s.langPill} aria-hidden="true">
              <Glyph name="cap" />
              ISL+
              <Icon name="chevronDown" size={13} />
            </span>
            <button
              type="button"
              className={s.runDemo}
              title="Replay the prepared run of bst.rkt from the first step (your edits aren't run)"
              aria-pressed={playing}
              onClick={runDemo}
            >
              <Icon name="play" size={14} />
              Run demo
            </button>
            <span className={s.titleIcons} aria-hidden="true">
              <span className={s.sep} />
              <Icon name="sidebar" size={15} />
              <Icon name="panelBottom" size={15} />
              <Icon name="panelRight" size={15} />
            </span>
          </span>
        </header>

        <section className={s.left} aria-label="Editor">
          <div className={s.tabs}>
            <span className={s.fileTab}>
              <FileIcon name="racket" size={13} />
              bst.rkt
              {file.status !== "pristine" && <span className={s.dirty} aria-label="modified">●</span>}
              <Icon name="close" size={11} />
            </span>
            <span className={s.tabTools}>
              <ThemeToggle />
              <Icon name="chevronDown" size={14} />
              <Glyph name="dots" />
            </span>
          </div>
          <div className={s.codeBox}>
            <div className={s.codeScale}>
              <Code
                text={file.text}
                label="bst.rkt"
                onChange={file.set}
                decor={decor}
                onCursor={(_, line, col) => setCursor({ line, col })}
              />
            </div>
          </div>
        </section>

        <section className={s.right} aria-label="Visualizer panel">
          <div className={s.ptabs} role="tablist" aria-label="Panels">
            {tabs.map((t, k) => (
              <button
                key={t}
                type="button"
                role="tab"
                aria-selected={tab === k}
                tabIndex={tab === k ? 0 : -1}
                className={s.ptab}
                onClick={() => setTab(k)}
                onKeyDown={(e) => {
                  if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
                  e.preventDefault();
                  const n = (k + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length;
                  setTab(n);
                  (e.currentTarget.parentElement!.children[n] as HTMLElement).focus();
                }}
              >
                <Glyph name={tabIcons[k]} />
                {t}
              </button>
            ))}
            <span className={s.ptabPlus} aria-hidden="true">
              <Icon name="add" size={15} />
            </span>
          </div>

          {tab !== 2 ? (
            <div className={s.otherTab} role="tabpanel" aria-label={tabs[tab]}>
              {tab === 0 && (
                <div className={s.otherMono}>
                  <div>Welcome to Racket 9.3</div>
                  <div>Language: Intermediate Student with lambda</div>
                  <div className={s.otherVal}>30</div>
                </div>
              )}
              {tab === 1 && <p>The Stepper shows one reduction at a time. Press ▶ in its panel to step through (tree-sum tree).</p>}
              {tab === 3 && <p>No tests in bst.rkt.</p>}
              {tab === 4 && <p>No macros to expand in bst.rkt.</p>}
              <DisclosureChip />
            </div>
          ) : (
            <div className={s.vizPanel} role="tabpanel" aria-label="Visualizer">
              <div
                className={s.transport}
                role="toolbar"
                aria-label="Steps"
                onKeyDown={onToolbarKey}
                onFocus={(e) => {
                  const items = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("[data-tool]"));
                  const k = items.indexOf(e.target as HTMLElement);
                  if (k >= 0) setToolFocus(k);
                }}
              >
                {btn(0, "First step", "first", () => go(0), { "data-transport": "" })}
                {btn(1, "Previous step", "prev", () => go(i - 1), { "data-transport": "" })}
                {btn(2, playing ? "Pause" : "Play", playing ? "pause" : "play", togglePlay, { "aria-pressed": playing, "data-transport": "" })}
                {btn(3, "Next step", "next", () => go(i + 1), { "data-transport": "" })}
                {btn(4, "Last step", "last", () => go(TRACE.length - 1), { "data-transport": "" })}
                {btn(5, "Step over", "over", () => go(overIdx("over")))}
                {btn(6, "Step out", "out", () => go(overIdx("out")))}
                <span className={s.count}>
                  Step {i + 1} of {TRACE.length}
                </span>
                <span className={s.grow} />
                <button
                  type="button"
                  className={`${s.tbtn} ${s.speed}`}
                  aria-label={`Speed ${speed}×`}
                  title="Speed"
                  onClick={() => setSpeed(SPEEDS[(SPEEDS.indexOf(speed) + 1) % SPEEDS.length])}
                  {...tool(7)}
                >
                  <Glyph name="speed" />
                  {speed}×
                </button>
                <button type="button" className={s.tbtn} aria-label="Recenter" title="Recenter" {...tool(8)} data-recenter onClick={() => setRecenter((r) => r + 1)}>
                  <Glyph name="center" />
                </button>
                <button
                  type="button"
                  className={`${s.tbtn} ${expanded ? s.on : ""}`}
                  aria-label="Focus the drawing"
                  aria-pressed={expanded}
                  title="Focus the drawing"
                  onClick={() => setExpanded((x) => !x)}
                  {...tool(9)}
                >
                  <Glyph name="expand" />
                </button>
                {btn(10, "Restart", "restart", () => (setPlaying(false), go(0)))}
              </div>
              <span className={s.srOnly} aria-live="polite">
                {announce}
              </span>

              <Timeline i={i} peek={peek} onSeek={(n) => go(n)} onToggle={togglePlay} />

              <Stage
                step={step}
                recenter={recenter}
                frameStep={(k) => frameStart(i, k)}
                nodeStep={(k) => NODE_STEP.get(k) ?? 0}
                onJump={(n) => go(n)}
                onPeek={setPeek}
              />

              <div className={s.explain}>
                {edited && <Notice compact onRestore={restore} text="Edited · replaying the original bst.rkt" />}
                <div className={s.label}>
                  <span className={s[`k${step.kind}`]}>{step.kind === "START" || step.kind === "DEFINE" ? "STEP" : step.kind}</span>
                  {step.chip && <code className={s.chipCode}>{step.chip}</code>}
                </div>
                {step.lines.map((l, k) => (
                  <p key={k} className={k === 0 ? s.lead : s.line}>
                    <Segs segs={l} />
                  </p>
                ))}
                {step.work.length > 0 && (
                  <>
                    <button type="button" className={s.work} aria-expanded={work} onClick={() => setWork((w) => !w)}>
                      {work ? "▾" : "▸"} Show the work ({step.work.length})
                    </button>
                    {work && (
                      <ul className={s.workList}>
                        {step.work.map((w, k) => (
                          <li key={k}>
                            <code className={s.chipCode}>{w}</code>
                          </li>
                        ))}
                      </ul>
                    )}
                  </>
                )}
                {step.output && (
                  <div className={s.output}>
                    <div className={s.outLabel}>OUTPUT SO FAR</div>
                    <code>{step.output}</code>
                  </div>
                )}
              </div>
            </div>
          )}
        </section>

        <footer className={s.status}>
          <span className={s.statusIcons} aria-hidden="true">
            <Icon name="terminal" size={13} />
            <Icon name="check" size={13} />
            <Glyph name="list" />
          </span>
          <span className={s.crumbs}>
            cs135 <span>›</span> a05 <span>›</span> <FileIcon name="racket" size={11} /> bst.rkt <span>›</span> <i>ƒx</i> node
          </span>
          <span className={s.grow} />
          <DisclosureChip />
          <span>
            Ln {cursor.line}, Col {cursor.col}
          </span>
          <span>Racket 9.3</span>
          <KeysHelp keys={KEYS} />
        </footer>
      </div>
    </DemoRoot>
  );
}

/** The depth timeline: one bar per step, height by call depth. A slider. */
function Timeline({ i, peek, onSeek, onToggle }: { i: number; peek: number | null; onSeek: (n: number) => void; onToggle: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const n = TRACE.length;
  const seekAt = (x: number) => {
    const r = ref.current!.getBoundingClientRect();
    onSeek(Math.round(((x - r.left) / r.width) * (n - 1)));
  };
  return (
    <div
      ref={ref}
      className={s.timeline}
      role="slider"
      tabIndex={0}
      aria-label="Step"
      aria-valuemin={1}
      aria-valuemax={n}
      aria-valuenow={i + 1}
      aria-valuetext={`Step ${i + 1} of ${n}`}
      onKeyDown={(e) => {
        const m: Record<string, number> = { ArrowRight: 1, ArrowUp: 1, ArrowLeft: -1, ArrowDown: -1, PageUp: 10, PageDown: -10 };
        if (e.key in m) onSeek(i + m[e.key]);
        else if (e.key === "Home") onSeek(0);
        else if (e.key === "End") onSeek(n - 1);
        else if (e.key === " ") onToggle();
        else return;
        e.preventDefault();
      }}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        seekAt(e.clientX);
      }}
      onPointerMove={(e) => {
        if (e.currentTarget.hasPointerCapture(e.pointerId)) seekAt(e.clientX);
      }}
    >
      {TRACE.map((t, k) => (
        <span
          key={k}
          className={k <= i ? s.barOn : s.bar}
          style={{ height: `${18 + (t.depth / MAX_DEPTH) * 82}%` }}
        />
      ))}
      {peek !== null && peek !== i && <span className={s.ghost} style={{ left: `${(peek / (n - 1)) * 100}%` }} aria-hidden="true" />}
      <span className={s.thumb} style={{ left: `${(i / (n - 1)) * 100}%` }} />
    </div>
  );
}
