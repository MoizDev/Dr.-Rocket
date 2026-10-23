"use client";
// Check Syntax on grades.rkt: binding arrows and occurrence tooltips from prepared bindings
// (bindings-grades.json, by significant-token index, so reformatting keeps them). An edit that
// changes the tokens withdraws them until the file is restored.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import DemoRoot, { DisclosureChip } from "../DemoRoot";
import Code from "../Code";
import Notice from "../Notice";
import KeysHelp, { EDITOR_KEYS } from "../KeysHelp";
import { Icon } from "../icons";
import { FileIcon } from "../file-icons";
import { useFile } from "@/lib/demos/useFile";
import { useEntryHint } from "@/lib/demos/hint";
import { sigTokens, status } from "@/lib/demos/fixtures";
import { lineCol, lineStart } from "@/lib/demos/spans";
import type { CodeDecor, EditorHandle } from "@/lib/demos/editor/types";
import raw from "@/lib/demos/data/bindings-grades.json";
import type { Bindings } from "@/lib/demos/data/bindings-resolve";
import s from "./CheckSyntax.module.css";

export const LABEL = "Check syntax arrows from the parameter marks to its three uses, with a tooltip reading 3 bound occurrences.";

const B = raw as Bindings;
const FILE = "checksyntax/grades.rkt";
/** The token the screenshot shows selected: the parameter `marks` of `total`. */
export const REST_TOKEN = Number(Object.keys(B.binders).find((k) => B.binders[k].name === "marks" && B.binders[k].kind === "parameter" && B.binders[k].uses.length === 3));

/** Binder of a token: itself if it is one, else the binder it refers to. */
const BINDER_OF = new Map<number, number>();
for (const [k, v] of Object.entries(B.binders)) {
  BINDER_OF.set(+k, +k);
  for (const u of v.uses) BINDER_OF.set(u, +k);
}

export function tooltipFor(idx: number): string | null {
  if (B.imported[idx] !== undefined) return `imported from ${B.module}`;
  const b = BINDER_OF.get(idx);
  if (b === undefined) return null;
  const binder = B.binders[b];
  if (b === idx) return `${binder.uses.length} bound occurrence${binder.uses.length === 1 ? "" : "s"}`;
  return binder.kind === "parameter" ? `bound by the parameter ${binder.name}` : `bound by the definition of ${binder.name}`;
}

// Code-point grid (Code.module.css): gutter 54, 9 per character, 23.5 per line, 4 top padding.
// The code is drawn at ZOOM× to match the screenshot's large editor.
const ZOOM = 1.356;
const VBW = 828.5 / ZOOM;
const VBH = (470 - 40) / ZOOM;
const GX = 54;
const ADV = 9;
const PITCH = 23.5;
const TOP = 4;

type Box = { x: number; y: number; w: number; h: number };

export default function CheckSyntaxDemo({ className }: { className: string }) {
  const root = useRef<HTMLDivElement>(null);
  const wrap = useRef<HTMLDivElement>(null);
  const handle = useRef<EditorHandle | null>(null);
  const file = useFile(FILE);
  const edited = file.status === "edited";
  const toks = useMemo(() => sigTokens(file.text), [file.text]);
  const [caret, setCaret] = useState<number | null>(REST_TOKEN);
  const [hover, setHover] = useState<number | null>(null);
  const [boxes, setBoxes] = useState<(Box | null)[] | null>(null);
  const [live, setLive] = useState(false);
  const [drawing, setDrawing] = useState(false);

  const tokenAt = useCallback(
    (pos: number | null) => {
      if (pos === null) return null;
      const i = toks.findIndex((t) => t.start <= pos && pos <= t.end && t.kind === "symbol");
      return i >= 0 && (BINDER_OF.has(i) || B.imported[i] !== undefined) ? i : null;
    },
    [toks],
  );

  const active = edited ? null : (hover ?? caret);
  const binder = active === null ? undefined : BINDER_OF.get(active);
  const group = binder === undefined ? (active === null ? [] : [active]) : [binder, ...B.binders[binder].uses];

  /** A token's box in code points: measured from CodeMirror once live, else from the static grid. */
  const boxOf = (i: number): Box | null => {
    const t = toks[i];
    if (!t) return null;
    if (boxes) return boxes[i] ?? null;
    const { line, col } = lineCol(file.text, t.start);
    return { x: GX + col * ADV, y: TOP + (line - 1) * PITCH, w: (t.end - t.start) * ADV, h: PITCH };
  };

  const groupKey = group.join(",");
  const decor = useMemo<CodeDecor>(() => {
    if (edited) return {};
    const marks = group.filter((i) => toks[i]).map((i) => ({ from: toks[i].start, to: toks[i].end, cls: s.occ }));
    const lines = caret !== null && toks[caret] ? [{ line: lineCol(file.text, toks[caret].start).line, cls: "active" }] : [];
    return { marks, lines };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [edited, toks, groupKey, caret, file.text]);

  // Re-measure once CodeMirror has laid out, and follow its scrolling and resizing.
  useEffect(() => {
    if (!live || !handle.current || !wrap.current) return;
    const sc = handle.current.scroller;
    const on = () => {
      const h = handle.current;
      const svg = wrap.current;
      if (!h || !svg) return;
      const r = svg.getBoundingClientRect();
      const kx = VBW / r.width;
      const ky = VBH / r.height;
      const doc = h.getDoc();
      setBoxes(
        sigTokens(doc).map((t) => {
          const a = h.coordsAt(t.start);
          const b = h.coordsAt(t.end);
          if (!a || !b) return null;
          return { x: (a.left - r.left) * kx, y: (a.top - r.top) * ky, w: (b.left - a.left) * kx, h: (a.bottom - a.top) * ky };
        }),
      );
    };
    const raf = requestAnimationFrame(() => requestAnimationFrame(on));
    const ro = new ResizeObserver(on);
    ro.observe(wrap.current);
    sc.addEventListener("scroll", on, { passive: true });
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      sc.removeEventListener("scroll", on);
    };
  }, [live, file.text]);

  // The tooltip describes the focused identifier.
  const tipId = "check-syntax-tip";
  const tip = active === null ? null : tooltipFor(active);
  useEffect(() => {
    const el = wrap.current?.querySelector<HTMLElement>(".cm-content, [role='textbox']");
    if (!el) return;
    if (tip) el.setAttribute("aria-describedby", tipId);
    else el.removeAttribute("aria-describedby");
  }, [tip, live]);

  useEntryHint(
    root,
    () => [{ at: 0, run: () => setDrawing(true) }],
    () => setDrawing(false),
  );

  const pointer = (e: React.PointerEvent) => {
    if (edited) return;
    let pos: number | null = null;
    if (handle.current) pos = handle.current.posAt(e.clientX, e.clientY);
    else if (wrap.current) {
      const r = wrap.current.getBoundingClientRect();
      const gx = ((e.clientX - r.left) * VBW) / r.width;
      const gy = ((e.clientY - r.top) * VBH) / r.height;
      const line = Math.floor((gy - TOP) / PITCH) + 1;
      const col = Math.floor((gx - GX) / ADV);
      if (line >= 1 && col >= 0) pos = lineStart(file.text, line) + col;
    }
    setHover(tokenAt(pos));
  };

  const jump = async (dir: 1 | -1) => {
    const h = handle.current;
    if (!h || caret === null || group.length < 2) return;
    const order = [...group].sort((a, b) => a - b);
    const next = order[(order.indexOf(caret) + dir + order.length) % order.length];
    const { EditorView } = await import("@codemirror/view");
    EditorView.findFromDOM(h.dom)?.dispatch({ selection: { anchor: toks[next].start }, scrollIntoView: true });
  };

  const from = binder !== undefined ? boxOf(binder) : null;
  const arrows =
    binder === undefined || !from
      ? []
      : B.binders[binder].uses.flatMap((u) => {
          const to = boxOf(u);
          if (!to) return [];
          const x1 = from.x + from.w / 2;
          const y1 = from.y + from.h / 2;
          const x2 = to.x + to.w / 2;
          const y2 = to.y + to.h / 2;
          const dx = x2 - x1;
          const lift = 14 + Math.abs(dx) * 0.08;
          const top = Math.min(y1, y2) - from.h / 2 - lift;
          return [`M${x1} ${y1} C${x1 + dx * 0.25} ${top} ${x2 - dx * 0.25} ${top} ${x2} ${y2}`];
        });
  const anchor = active !== null ? boxOf(active) : null;

  return (
    <DemoRoot id="check-syntax" className={className} w={1657} h={940} label={LABEL} rootProps={{ ref: root, tabIndex: -1 }}>
      <div className={s.app}>
        <div className={s.tabs}>
          <span className={s.tab} aria-hidden="true">
            <FileIcon name="racket" size={15} />
            grades.rkt
            {file.text !== file.original ? <span className={s.dirty}>●</span> : <Icon name="close" size={12} />}
          </span>
          <span className={s.spacer} />
          <DisclosureChip />
          <KeysHelp
            placement="down"
            keys={[["⌥↓ / ⌥↑", "Next / previous occurrence"], ["Hover or caret", "Show the binding arrows"], ...EDITOR_KEYS]}
          />
          <span className={s.tabIcons} aria-hidden="true">
            <Icon name="chevronDown" size={14} />
            <span>⋮</span>
          </span>
        </div>
        <div
          ref={wrap}
          className={s.zoom}
          data-drawing={drawing || undefined}
          onPointerMove={pointer}
          onPointerLeave={() => setHover(null)}
          onKeyDownCapture={(e) => {
            if (e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
              e.preventDefault();
              e.stopPropagation();
              jump(e.key === "ArrowDown" ? 1 : -1);
            }
          }}
        >
          <Code
            text={file.text}
            label="grades.rkt, editable"
            onChange={file.set}
            decor={decor}
            onCursor={(pos) => setCaret(tokenAt(pos))}
            onReady={(h) => {
              handle.current = h;
              setLive(true);
              // Rest state: the caret sits in the parameter marks, as in the screenshot.
              const t = sigTokens(h.getDoc())[REST_TOKEN];
              if (t && status(h.getDoc(), file.original) !== "edited")
                import("@codemirror/view").then(({ EditorView }) => EditorView.findFromDOM(h.dom)?.dispatch({ selection: { anchor: t.start + 2 } }));
            }}
          />
          <svg className={s.overlay} viewBox={`0 0 ${VBW} ${VBH}`} preserveAspectRatio="none" aria-hidden="true">
            <defs>
              <marker id="cs-head" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="5" markerHeight="5" orient="auto">
                <path d="M0 0 8 4 0 8z" className={s.head} />
              </marker>
            </defs>
            {arrows.map((d, i) => (
              <path key={i} d={d} className={s.arrow} pathLength={1} markerEnd="url(#cs-head)" />
            ))}
          </svg>
          {tip && anchor && (
            <div
              id={tipId}
              role="tooltip"
              className={s.tip}
              style={{ left: `${(anchor.x / VBW) * 100}%`, top: `${((anchor.y + anchor.h + 6) / VBH) * 100}%` }}
            >
              {tip}
            </div>
          )}
        </div>
        {edited && (
          <div className={s.notice}>
            <Notice compact onRestore={() => file.set(file.original)} text="Edited: the prepared arrows are hidden." />
          </div>
        )}
      </div>
    </DemoRoot>
  );
}
