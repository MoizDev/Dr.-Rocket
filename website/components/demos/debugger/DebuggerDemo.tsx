"use client";
// The debugger paused on bst.rkt. Every pause (position, variables, call stack) comes from
// lib/demos/data/debugger-pauses.json, generated at development time; nothing runs here. Edits
// and breakpoint changes never block the controls: they always replay the prepared pauses of the
// original file (Continue stops at the prepared line-20 entries while that breakpoint is set).
import { useCallback, useMemo, useRef, useState } from "react";
import DemoRoot, { DisclosureChip } from "../DemoRoot";
import Code, { StaticLines } from "../Code";
import Notice from "../Notice";
import KeysHelp, { EDITOR_KEYS } from "../KeysHelp";
import { Icon } from "../icons";
import { FileIcon } from "../file-icons";
import { Glyph } from "../visualizer/glyphs";
import { useFile } from "@/lib/demos/useFile";
import { remapRange } from "@/lib/demos/fixtures";
import { useEntryHint } from "@/lib/demos/hint";
import { BREAK_LINE, next, parseVal, type Val } from "@/lib/demos/bst-debug";
import type { CodeDecor } from "@/lib/demos/editor/types";
import type { DPause } from "@/lib/demos/bst-walk";
import pausesJson from "@/lib/demos/data/debugger-pauses.json";
import s from "./Debugger.module.css";
import c from "../Code.module.css";

export const LABEL = "The debugger paused at a breakpoint inside tree-sum, with variables and a call stack of recursive calls.";

const FILE = "cs135/a05/bst-debug.rkt";
const PAUSES = pausesJson as unknown as DPause[];
export const REST = PAUSES.findIndex((p) => p.rest);
export const RESULT = 30;
/** The sticky header line (the enclosing define) and the first line shown under it. */
const STICKY = 7;
const TOP = 10;

type Pos = number | "done";
type Cmd = "continue" | "into" | "over" | "out" | "restart";

const KEYS: [string, string][] = [
  ["F5", "Continue"],
  ["F10", "Step over"],
  ["F11", "Step into"],
  ["⇧F11", "Step out"],
  ["⇧⌘F5", "Restart"],
  ["Gutter click", "Toggle a breakpoint"],
  ...EDITOR_KEYS,
];

/** The editor decorations for a pause: the paused expression and the breakpoint dots. */
export function pauseDecor(p: DPause | null, bps: Set<number>, original: string, text: string): CodeDecor {
  const gutter = [...bps].map((line) => ({ line, cls: s.bp, label: `Breakpoint on line ${line}` }));
  if (!p) return { gutter };
  const r = remapRange(p, original, text);
  if (!r) return { gutter };
  return { gutter, marks: [{ ...r, cls: s.paused }] };
}

function ValView({ v, depth = 0, open: shown, onToggle }: { v: Val; depth?: number; open?: boolean; onToggle?: () => void }) {
  const [own, setOwn] = useState(false);
  const open = shown ?? own;
  const toggle = onToggle ?? (() => setOwn((o) => !o));
  if (typeof v === "string") return <span className={s.val}>{v}</span>;
  return (
    <span className={s.struct}>
      <button type="button" className={s.disclose} aria-expanded={open} onClick={toggle} aria-label={open ? "Collapse node" : "Expand node"}>
        {open ? "▾" : "▸"}
      </button>
      {open ? (
        <span className={s.fields} style={{ marginLeft: depth ? undefined : 0 }}>
          <span className={s.val}>(make-node {v.key}</span>
          <span className={s.field}>
            <span className={s.fname}>left</span> <ValView v={v.left} depth={depth + 1} />
          </span>
          <span className={s.field}>
            <span className={s.fname}>right</span> <ValView v={v.right} depth={depth + 1} />
          </span>
        </span>
      ) : (
        <span className={s.val}>{show(v)}</span>
      )}
    </span>
  );
}

const show = (v: Val): string => (typeof v === "string" ? v : `(make-node ${v.key} ${show(v.left)} ${show(v.right)})`);

/** One variable: clicking its name selects the row and opens or closes a structure value. */
function VarRow({ name, value, selected, onSelect }: { name: string; value: string; selected: boolean; onSelect: () => void }) {
  const v = parseVal(value);
  const struct = typeof v !== "string";
  const [open, setOpen] = useState(false);
  return (
    <li className={s.var} data-selected={selected || undefined}>
      <button
        type="button"
        className={s.vbtn}
        aria-pressed={selected}
        aria-label={struct ? `${name}: ${open ? "collapse" : "expand"} the value` : `${name} = ${value}`}
        title={struct ? (open ? "Collapse the value" : "Expand the value") : `${name} = ${value}`}
        onClick={() => {
          onSelect();
          if (struct) setOpen((o) => !o);
        }}
      >
        <span className={s.vname}>{name}</span>
        <span className={s.eq}> = </span>
      </button>
      <ValView v={v} open={open} onToggle={() => setOpen((o) => !o)} />
    </li>
  );
}

export default function DebuggerDemo({ className }: { className: string }) {
  const file = useFile(FILE);
  const root = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<Pos>(REST);
  const [bps, setBps] = useState<Set<number>>(() => new Set([BREAK_LINE]));
  const [frame, setFrame] = useState(0);
  const [raised, setRaised] = useState(false);
  const [announce, setAnnounce] = useState("");
  const [sel, setSel] = useState<string | null>(null);

  // Neither edits nor breakpoint changes block anything: the same prepared pauses replay (the
  // edited text never runs).
  const edited = file.status === "edited";
  const pause = typeof pos === "number" && pos >= 0 ? PAUSES[pos] : null;

  const go = useCallback((p: Pos, say = true) => {
    setPos(p);
    setFrame(0);
    setSel(null);
    if (say) setAnnounce(p === "done" ? `Finished. Result ${RESULT}.` : `Paused on line ${PAUSES[p].line}: ${PAUSES[p].expr}`);
  }, []);

  const run = (cmd: Cmd) => {
    // Restart, and Continue after the run finishes, start the prepared run again.
    if (cmd === "restart" || pos === "done") {
      go(next(PAUSES, -1, cmd === "restart" ? "continue" : cmd, bps));
      return;
    }
    go(next(PAUSES, pos, cmd, bps));
  };

  useEntryHint(
    root,
    () => {
      setPos(REST - 2);
      return [
        { at: 500, run: () => setPos(REST - 1) },
        { at: 1000, run: () => setPos(REST) },
      ];
    },
    () => setPos(REST),
  );

  const decor = useMemo(() => pauseDecor(pause, bps, file.original, file.text), [pause, bps, file.original, file.text]);
  const toggle = (line: number) =>
    setBps((b) => {
      const n = new Set(b);
      if (n.has(line)) n.delete(line);
      else n.add(line);
      return n;
    });

  const btn = (label: string, icon: Parameters<typeof Glyph>[0]["name"], cmd: Cmd | null, keys?: string) => (
    <button
      type="button"
      className={s.tbtn}
      aria-label={label}
      title={keys ? `${label} (${keys})` : label}
      aria-disabled={cmd === null || undefined}
      onClick={() => cmd && run(cmd)}
      data-tool
    >
      <Glyph name={icon} size={14} />
    </button>
  );

  const onKey = (e: React.KeyboardEvent) => {
    const m: Record<string, Cmd> = {
      F5: e.shiftKey && (e.metaKey || e.ctrlKey) ? "restart" : "continue",
      F10: "over",
      F11: e.shiftKey ? "out" : "into",
    };
    const cmd = m[e.key];
    if (!cmd) return;
    e.preventDefault();
    run(cmd);
  };

  const shown = pause;
  const fr = shown ? shown.frames[Math.min(frame, shown.frames.length - 1)] : null;
  const vars = fr && shown ? [...fr.vars, ...shown.globals] : [];
  const stickyText = file.text.split("\n")[STICKY - 1] ?? "";

  return (
    <DemoRoot
      id="debugger"
      className={`${className} ${raised ? s.raised : ""}`}
      w={2000}
      h={887}
      label={LABEL}
      rootProps={{
        ref: root,
        tabIndex: -1,
        onFocus: () => setRaised(true),
        onBlur: (e: React.FocusEvent<HTMLElement>) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setRaised(false);
        },
        onKeyDown: onKey,
      }}
    >
      <div className={s.app}>
        <section className={s.editor} aria-label="Editor">
          <div className={s.tabs}>
            <span className={s.tab} aria-hidden="true">
              <FileIcon name="racket" size={12} />
              bst.rkt
              {file.text !== file.original ? <span className={s.dirty}>●</span> : <Icon name="close" size={10} />}
            </span>
            <span className={s.tabTools} aria-hidden="true">
              <Icon name="chevronDown" size={13} />
              <Glyph name="dots" size={13} />
            </span>
          </div>
          <div className={s.codeBox}>
            <Code
              text={file.text}
              label="bst.rkt, editable. Click the line numbers to toggle breakpoints."
              onChange={file.set}
              decor={decor}
              topLine={TOP}
              breakpointGutter
              onGutterClick={toggle}
            />
            <div className={`${c.code} ${s.sticky}`} aria-hidden="true">
              <StaticLines text={stickyText} firstLine={STICKY} breakpointGutter />
            </div>
          </div>
        </section>

        <section className={s.panel} aria-label="Debugger panel">
          <div className={s.ptabs} aria-hidden="true">
            <Glyph name="terminal" size={12} />
            <Glyph name="stepper" size={12} />
            <Glyph name="viz" size={12} />
            <Glyph name="tests" size={12} />
            <Glyph name="macro" size={12} />
            <span className={s.ptab}>
              <Glyph name="bug" size={12} />
              Debugger
            </span>
            <span className={s.ptabPlus}>
              <Icon name="add" size={13} />
            </span>
          </div>
          <div className={s.toolbar} role="toolbar" aria-label="Debugger controls">
            {btn(pos === "done" ? "Run again" : "Continue", "play", "continue", "F5")}
            {btn("Pause", "pause", null)}
            {btn("Step over", "over", "over", "F10")}
            {btn("Step into", "into", "into", "F11")}
            {btn("Step out", "out", "out", "⇧F11")}
            {btn("Restart", "restart", "restart", "⇧⌘F5")}
            <span className={pos === "done" ? s.finished : s.state} data-debug-state>
              {pos === "done" ? `Finished · result ${RESULT}` : "Paused"}
            </span>
          </div>
          <span className={s.srOnly} aria-live="polite">
            {announce}
          </span>

          <div className={s.body}>
            {edited && <Notice compact onRestore={() => file.set(file.original)} text="Edited · replaying the original bst.rkt" />}
            <div className={s.head}>VARIABLES</div>
            <ul className={s.vars} aria-label="Variables">
              {vars.map((v) => (
                <VarRow
                  key={`${pos}-${frame}-${v.name}`}
                  name={v.name}
                  value={v.value}
                  selected={sel === v.name}
                  onSelect={() => setSel(v.name)}
                />
              ))}
              {!shown && <li className={s.empty}>{pos === "done" ? `Not paused · result ${RESULT}` : "Not paused"}</li>}
            </ul>
            <div className={s.head}>CALL STACK</div>
            <ul className={s.stack} aria-label="Call stack">
              {shown?.frames.map((f, k) => (
                <li key={k}>
                  <button
                    type="button"
                    className={s.frame}
                    aria-current={k === frame || undefined}
                    onClick={() => {
                      setFrame(k);
                      setSel(null);
                    }}
                    title={`${f.label}: show its variables`}
                  >
                    {f.label}
                  </button>
                </li>
              ))}
            </ul>
          </div>
          <div className={s.foot}>
            <DisclosureChip />
            <KeysHelp keys={KEYS} />
          </div>
        </section>
      </div>
    </DemoRoot>
  );
}
