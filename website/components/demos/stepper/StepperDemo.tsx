"use client";
// The algebraic stepper on squares.rkt (BSL). Steps are prepared (stepper-squares.json): they are
// shown, never computed. Editing the file withdraws them until it is restored.
import { useCallback, useMemo, useRef, useState, type ReactNode } from "react";
import DemoRoot, { DisclosureChip } from "../DemoRoot";
import Code from "../Code";
import Notice from "../Notice";
import KeysHelp, { EDITOR_KEYS } from "../KeysHelp";
import { Icon } from "../icons";
import { FileIcon } from "../file-icons";
import { useFile } from "@/lib/demos/useFile";
import { useEntryHint } from "@/lib/demos/hint";
import { styleSpans } from "@/lib/demos/spans";
import type { CodeDecor } from "@/lib/demos/editor/types";
import data from "@/lib/demos/data/stepper-squares.json";
import { pretty } from "./sexpr";
import s from "./Stepper.module.css";

export const LABEL = "The stepper showing a cond expression before and after one reduction, with the matching code highlighted in the editor.";

type Step = { context: string[]; before: string; beforeHl: number[]; after: string; afterHl: number[]; lines: number[]; rest?: boolean };
const STEPS = data.steps as Step[];
export const REST = STEPS.findIndex((x) => x.rest);
const FILE = "cs135/a03/squares.rkt";
const WIDTH = 34;

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

const TREE: [number, string, "dir-open" | "dir" | "file" | "readme", boolean?][] = [
  [0, "a03", "dir-open"],
  [1, "grades.rkt", "file"],
  [1, "squares.rkt", "file", true],
  [0, "a05", "dir"],
  [0, "notes", "dir"],
  [0, "README.md", "readme"],
];

export default function StepperDemo({ className }: { className: string }) {
  const root = useRef<HTMLDivElement>(null);
  const file = useFile(FILE);
  const [at, setAt] = useState(REST);
  const [user, setUser] = useState(false);
  const [cursor, setCursor] = useState({ line: 1, col: 1 });
  const edited = file.status === "edited";
  const step = STEPS[at];

  const go = useCallback(
    (i: number) => {
      if (edited) return;
      setUser(true);
      setAt(Math.max(0, Math.min(STEPS.length - 1, i)));
    },
    [edited],
  );

  useEntryHint(
    root,
    () => [
      { at: 0, run: () => setAt(REST - 2) },
      { at: 700, run: () => setAt(REST - 1) },
      { at: 1400, run: () => setAt(REST) },
    ],
    () => setAt(REST),
  );

  const decor = useMemo<CodeDecor>(() => {
    if (edited) return {};
    const lines = [];
    for (let l = step.lines[0]; l <= step.lines[1]; l++) lines.push({ line: l, cls: s.srcLine });
    return { lines };
  }, [edited, step]);

  const ctl = (label: string, icon: Parameters<typeof Icon>[0]["name"], to: number, extra?: string) => {
    const off = edited || to === at || to < 0 || to >= STEPS.length;
    return (
      <button
        type="button"
        className={`${s.ctl} ${extra ?? ""}`}
        aria-label={label}
        title={edited ? "The steps were prepared for the original file" : label}
        aria-disabled={off || undefined}
        onClick={() => !off && go(to)}
      >
        <Icon name={icon} size={14} />
      </button>
    );
  };

  return (
    <DemoRoot id="stepper" className={className} w={2400} h={1506} label={LABEL} rootProps={{ ref: root, tabIndex: -1 }}>
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
            <Icon name="sidebar" size={14} />
            <Icon name="panelBottom" size={14} />
            <Icon name="panelRight" size={14} />
          </span>
        </div>

        <aside className={s.sidebar} aria-hidden="true">
          <div className={s.sideTools}>
            <span className={s.sideActive}>▣</span>
            <Icon name="search" size={13} />
          </div>
          <div className={s.sideTitle}>CS135</div>
          {TREE.map(([depth, name, kind, sel]) => (
            <div key={name} className={`${s.treeRow} ${sel ? s.treeSel : ""}`} style={{ ["--depth" as string]: depth }}>
              <span className={s.chev}>{kind === "dir-open" ? "⌄" : kind === "dir" ? "›" : ""}</span>
              <FileIcon name={kind === "dir-open" ? "folder-open" : kind === "dir" ? "folder" : kind === "readme" ? "readme" : "racket"} size={13} />
              <span>{name}</span>
            </div>
          ))}
        </aside>

        <section className={s.editor}>
          <div className={s.tabs} aria-hidden="true">
            <span className={s.tab}>
              <FileIcon name="racket" size={12} />
              squares.rkt
              {file.text !== file.original ? <span className={s.dirty}>●</span> : <Icon name="close" size={11} />}
            </span>
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

        <section className={s.panel} aria-label="Stepper">
          <div className={s.panelTabs} aria-hidden="true">
            <span className={s.panelTab}>
              <Icon name="next" size={12} /> Stepper
            </span>
          </div>
          <div
            className={s.controls}
            role="toolbar"
            aria-label="Stepper controls"
            onKeyDown={(e) => {
              const k = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: STEPS.length - 1 }[e.key];
              if (k !== undefined) {
                e.preventDefault();
                go(k);
              }
            }}
          >
            {ctl("First step", "first", 0)}
            {ctl("Previous step", "prev", at - 1)}
            {ctl("Next step", "next", at + 1, s.ctlMain)}
            {ctl("Last step", "last", STEPS.length - 1)}
            <span className={s.counter} aria-live={user ? "polite" : undefined}>
              Step {at + 1} of {STEPS.length}
            </span>
            <span className={s.spacer} />
            {ctl("Restart", "restart", 0)}
          </div>
          {edited ? (
            <Notice onRestore={() => file.set(file.original)} />
          ) : (
            <div className={s.steps}>
              <div className={s.kind}>reduction</div>
              <Card step={step} side="before" />
              <div className={s.arrow} aria-hidden="true">↓</div>
              <Card step={step} side="after" />
            </div>
          )}
        </section>

        <footer className={s.status}>
          <span className={s.crumbs} aria-hidden="true">
            cs135 <span>›</span> a03 <span>›</span> <FileIcon name="racket" size={11} /> squares.rkt
          </span>
          <span className={s.spacer} />
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
