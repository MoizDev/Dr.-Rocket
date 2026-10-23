"use client";
// The algebraic stepper on squares.rkt (BSL). Steps are prepared (stepper-squares.json) for the
// original file: they are shown, never computed. Edits stay in the editor and never stop the
// stepper; only the source highlights drop when they no longer map onto the edited text.
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import DemoRoot, { DisclosureChip, ThemeToggle } from "../DemoRoot";
import Code from "../Code";
import KeysHelp, { EDITOR_KEYS } from "../KeysHelp";
import { Icon, type IconName } from "../icons";
import { FileIcon } from "../file-icons";
import { useFile } from "@/lib/demos/useFile";
import { useEntryHint } from "@/lib/demos/hint";
import { styleSpans } from "@/lib/demos/spans";
import { remapRange } from "@/lib/demos/fixtures";
import type { CodeDecor } from "@/lib/demos/editor/types";
import data from "@/lib/demos/data/stepper-squares.json";
import { STEP_SOURCE } from "@/lib/demos/data/stepper-source";
import { pretty } from "./sexpr";
import s from "./Stepper.module.css";

export const LABEL = "The stepper showing a cond expression before and after one reduction, with the matching code highlighted in the editor.";

type Step = { context: string[]; before: string; beforeHl: number[]; after: string; afterHl: number[]; lines: number[]; rest?: boolean };
const STEPS = data.steps as Step[];
const LAST = STEPS.length - 1;
export const REST = STEPS.findIndex((x) => x.rest);
const FILE = "cs135/a03/squares.rkt";
/** Columns BEFORE is laid out to, as the stepper does for its pane width. */
const WIDTH = 46;
const SPEEDS = [0.5, 1, 2] as const;
const STEP_MS = 1100;

/** Highlighted expression text: syntax colours plus the reducing range. */
function Expr({ text, hl, hlCls }: { text: string; hl?: [number, number]; hlCls?: string }) {
  const spans = styleSpans(text).filter((x) => !x.cls.startsWith("band"));
  const cuts = new Set([0, text.length]);
  for (const x of spans) cuts.add(x.from).add(x.to);
  if (hl) cuts.add(hl[0]).add(hl[1]);
  const pts = [...cuts].sort((a, b) => a - b);
  const out: ReactNode[] = [];
  for (let i = 0; i + 1 < pts.length; i++) {
    const [a, b] = [pts[i], pts[i + 1]];
    const tok = spans.find((x) => x.from <= a && x.to >= b);
    const inHl = hl && a >= hl[0] && b <= hl[1];
    out.push(
      <span key={a} className={[tok?.cls, inHl ? hlCls : ""].filter(Boolean).join(" ") || undefined}>
        {text.slice(a, b)}
      </span>,
    );
  }
  return <>{out}</>;
}

function Card({ step, side }: { step: Step; side: "before" | "after" }) {
  const src = side === "before" ? step.before : step.after;
  const hl = (side === "before" ? step.beforeHl : step.afterHl) as [number, number];
  // BEFORE is laid out like the stepper; AFTER stays on one (wrapping) line, as in the app.
  const p = side === "before" ? pretty(src, WIDTH, hl) : { text: src, hl };
  return (
    <div className={s.card}>
      <div className={s.cardHead}>
        <span className={side === "before" ? s.dotBefore : s.dotAfter} aria-hidden="true" />
        {side === "before" ? "BEFORE" : "AFTER"}
      </div>
      {step.context.map((c, i) => (
        <pre key={i} className={s.context}>
          <Expr text={c} />
        </pre>
      ))}
      <pre className={s.redex} data-step-text={side}>
        <Expr text={p.text} hl={p.hl} hlCls={side === "before" ? s.hlBefore : s.hlAfter} />
      </pre>
    </div>
  );
}

const lineOf = (text: string, pos: number) => text.slice(0, pos).split("\n").length;

/** The step's source ranges in the current text; a range that no longer maps is dropped. */
function stepDecor(at: number, original: string, text: string, edited: boolean): CodeDecor {
  const src = STEP_SOURCE[at];
  if (!src || edited) return {};
  const marks: NonNullable<CodeDecor["marks"]> = [];
  const lines: NonNullable<CodeDecor["lines"]> = [];
  const after = src.after && remapRange(src.after, original, text);
  if (after) marks.push({ ...after, cls: s.srcAfter, title: "Where AFTER comes from" });
  const before = src.before && remapRange(src.before, original, text);
  if (before) {
    marks.push({ ...before, cls: s.srcBefore, title: "Being reduced" });
    for (let l = lineOf(text, before.from); l <= lineOf(text, before.to); l++) lines.push({ line: l, cls: s.srcLine });
  }
  return { marks, lines };
}

export default function StepperDemo({ className }: { className: string }) {
  const root = useRef<HTMLDivElement>(null);
  const file = useFile(FILE);
  const [at, setAt] = useState(REST);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState<(typeof SPEEDS)[number]>(1);
  const [user, setUser] = useState(false);
  const [cursor, setCursor] = useState({ line: 1, col: 1 });
  const changed = file.text !== file.original;
  const edited = file.status === "edited";
  const step = STEPS[at];

  /** A manual move: pauses playback. */
  const go = useCallback((i: number) => {
    setUser(true);
    setPlaying(false);
    setAt(Math.max(0, Math.min(LAST, i)));
  }, []);

  const play = useCallback(() => {
    setUser(true);
    if (at >= LAST) setAt(0);
    setPlaying(true);
  }, [at]);

  const runDemo = useCallback(() => {
    setUser(true);
    setAt(0);
    setPlaying(true);
  }, []);

  // Play: a timeout chain that stops at the last step, on pause or any manual move, and when the
  // window loses focus or the tab is hidden. Never loops; nothing runs while paused.
  useEffect(() => {
    if (!playing || at >= LAST) return;
    const t = window.setTimeout(() => {
      setAt(at + 1);
      if (at + 1 >= LAST) setPlaying(false);
    }, STEP_MS / speed);
    const stop = () => setPlaying(false);
    const vis = () => document.hidden && stop();
    window.addEventListener("blur", stop);
    document.addEventListener("visibilitychange", vis);
    return () => {
      clearTimeout(t);
      window.removeEventListener("blur", stop);
      document.removeEventListener("visibilitychange", vis);
    };
  }, [playing, at, speed]);

  useEntryHint(
    root,
    () => [
      { at: 0, run: () => setAt(REST - 2) },
      { at: 700, run: () => setAt(REST - 1) },
      { at: 1400, run: () => setAt(REST) },
    ],
    () => setAt(REST),
  );

  const decor = useMemo(() => stepDecor(at, file.original, file.text, edited), [at, file.original, file.text, edited]);

  /** ← / → / Home / End step, except where a field or the slider has its own keys. */
  const onKey = (e: KeyboardEvent<HTMLElement>) => {
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, [contenteditable], [role='textbox'], [role='dialog']")) return;
    const k = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: LAST }[e.key];
    if (k === undefined || e.altKey || e.ctrlKey || e.metaKey) return;
    e.preventDefault();
    go(k);
  };

  const ctl = (label: string, icon: IconName, run: () => void, off: boolean, extra?: string) => (
    <button
      type="button"
      className={`${s.ctl} ${extra ?? ""}`}
      aria-label={label}
      title={label}
      aria-disabled={off || undefined}
      onClick={() => !off && run()}
    >
      <Icon name={icon} size={14} />
    </button>
  );

  const counter = `Step ${at + 1} of ${STEPS.length}`;

  return (
    <DemoRoot
      id="stepper"
      className={className}
      w={2160}
      h={1440}
      label={LABEL}
      rootProps={{ ref: root, tabIndex: -1, onKeyDown: (e: KeyboardEvent<HTMLElement>) => e.target === e.currentTarget && onKey(e) }}
    >
      <div className={s.app}>
        <div className={s.titlebar} aria-hidden="true">
          <span className={s.tbIcon}>≡</span>
          <span className={s.projTab}>cs135</span>
          <span className={s.tbPlus}>+</span>
          <span className={s.tbRight}>
            <span className={s.langPill}>
              <span className={s.langDot} />
              BSL
              <Icon name="chevronDown" size={12} />
            </span>
            <Icon name="play" size={18} className={s.run} />
            <Icon name="panelBottom" size={14} />
            <Icon name="panelRight" size={14} />
          </span>
        </div>

        <section className={s.editor}>
          <div className={s.tabs}>
            <span className={s.tab} aria-hidden="true">
              <FileIcon name="racket" size={12} />
              squares.rkt
              {changed ? <span className={s.dirty}>●</span> : <Icon name="close" size={11} />}
            </span>
            <ThemeToggle className={s.tabEnd} />
          </div>
          <div className={s.codeBox}>
            <Code
              text={file.text}
              label="squares.rkt, editable"
              onChange={file.set}
              decor={decor}
              onCursor={(_, line, col) => setCursor({ line, col })}
            />
          </div>
        </section>

        <section className={s.panel} aria-label="Stepper" onKeyDown={onKey}>
          <div className={s.panelTabs} aria-hidden="true">
            <span className={s.panelTab}>
              <Icon name="next" size={12} /> Stepper
            </span>
          </div>
          <div className={s.controls} role="toolbar" aria-label="Stepper controls">
            <button
              type="button"
              className={s.runDemo}
              title="Replay the prepared steps of squares.rkt from the first one"
              onClick={runDemo}
            >
              <Icon name="play" size={12} />
              Run demo
            </button>
            {ctl("First step", "first", () => go(0), at === 0)}
            {ctl("Previous step", "prev", () => go(at - 1), at === 0)}
            {ctl(playing ? "Pause" : "Play", playing ? "pause" : "play", playing ? () => setPlaying(false) : play, false, s.ctlMain)}
            {ctl("Next step", "next", () => go(at + 1), at === LAST)}
            {ctl("Last step", "last", () => go(LAST), at === LAST)}
            <span className={s.counter} aria-live={user && !playing ? "polite" : undefined}>
              {counter}
            </span>
            <span className={s.spacer} />
            <button
              type="button"
              className={s.speed}
              aria-label={`Speed ${speed}×`}
              title="Play speed"
              onClick={() => setSpeed(SPEEDS[(SPEEDS.indexOf(speed) + 1) % SPEEDS.length])}
            >
              <Icon name="speed" size={12} />
              {speed}×
            </button>
            {ctl("Restart", "restart", () => go(0), at === 0 && !playing)}
          </div>
          <div className={s.scrub}>
            <input
              type="range"
              className={s.slider}
              min={1}
              max={STEPS.length}
              step={1}
              value={at + 1}
              aria-label="Step"
              aria-valuetext={counter}
              style={{ ["--fill" as string]: `${(at / LAST) * 100}%` }}
              onChange={(e) => go(Number(e.target.value) - 1)}
            />
          </div>
          {edited && <div className={s.editedNote}>Steps of the original squares.rkt; your edits aren&rsquo;t run.</div>}
          <div className={s.steps} tabIndex={0} role="group" aria-label="Before and after">
            <div className={s.kind}>reduction</div>
            <Card step={step} side="before" />
            <div className={s.arrow} aria-hidden="true">↓</div>
            <Card step={step} side="after" />
          </div>
        </section>

        <footer className={s.status}>
          <span className={s.crumbs} aria-hidden="true">
            cs135 <span>›</span> a03 <span>›</span> <FileIcon name="racket" size={11} /> squares.rkt
          </span>
          <span className={s.spacer} />
          {changed && (
            <button type="button" className={s.restore} title="Restore the original squares.rkt" onClick={() => file.set(file.original)} data-restore>
              Restore
            </button>
          )}
          <DisclosureChip />
          <span>
            Ln {cursor.line}, Col {cursor.col}
          </span>
          <span>Racket 9.3</span>
          <KeysHelp keys={[["← / →", "Previous / next step (in the stepper)"], ["Home / End", "First / last step"], ...EDITOR_KEYS]} />
        </footer>
      </div>
    </DemoRoot>
  );
}
