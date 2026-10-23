"use client";
// The Visualizer's drawing: call frames on the left, the tree on the right, pointer arrows
// between them. Zoom (buttons, ⌘/Ctrl-wheel) and drag-to-pan are CSS transforms only.
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ThenSeg, VFrame, VStep } from "@/lib/demos/bst-walk";
import { Glyph } from "./glyphs";
import s from "./Visualizer.module.css";

// Stage units are app points.
const VIEW_W = 420;
const VIEW_H = 412;
const TREE_X = 300;
const FRAME_W = 214;
const TOP_W = 128;
const NODE_R = 11;
const POS: Record<number, [number, number]> = { 5: [110, 14], 3: [58, 60], 8: [162, 60], 1: [30, 106], 4: [86, 106], 9: [190, 106] };
const KIDS: Record<number, [number | null, number | null]> = { 5: [3, 8], 3: [1, 4], 8: [null, 9], 1: [null, null], 4: [null, null], 9: [null, null] };

type Placed = { f: VFrame; x: number; y: number; w: number; h: number; rows: number[] };

function layout(frames: VFrame[]): Placed[] {
  let y = 20;
  return frames.map((f, i) => {
    const w = f.fn === null ? TOP_W : FRAME_W;
    const x = f.fn === null ? 44 : i % 2 ? 0 : 36;
    const rows: number[] = [];
    let h = 26;
    const row = () => {
      rows.push(h + 11);
      h += 22;
    };
    f.vars.forEach(row);
    if (f.returns) row();
    if (f.question) h += 24;
    if (f.then) h += 26;
    h += 6;
    const p = { f, x, y, w, h, rows };
    y += h + 30;
    return p;
  });
}

export function summary(step: VStep): string {
  const has = new Set(step.nodes);
  const sub = (k: number | null): string => {
    if (k === null || !has.has(k)) return "·";
    const [l, r] = KIDS[k];
    const a = sub(l);
    const b = sub(r);
    return a === "·" && b === "·" ? String(k) : `${k} (${a}, ${b})`;
  };
  const tree = has.size ? `Tree ${sub(5)}` : "No tree yet";
  const calls = step.frames.filter((f) => f.fn).map((f) => `${f.fn} on ${f.args.at(-1)}`);
  return `${tree}; calls: ${calls.length ? calls.join(", ") : "none"}`;
}

const Then = ({ segs }: { segs: ThenSeg[] }) => (
  <>
    {segs.map((g, k) =>
      "hole" in g ? (
        <span key={k} className={s.hole}>•••</span>
      ) : "val" in g ? (
        <span key={k} className={s.thenVal}>{g.val}</span>
      ) : (
        <span key={k} className={s.thenCode}>{g.code}</span>
      ),
    )}
  </>
);

export default function Stage({ step, dim }: { step: VStep; dim: boolean }) {
  const placed = layout(step.frames);
  const last = placed.at(-1)!;
  const cw = TREE_X + 220;
  const ch = Math.max(last.y + last.h + 20, 170);
  const fit = Math.min(1, VIEW_W / cw, VIEW_H / ch);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  const sc = fit * zoom;
  const pct = Math.round(sc * 100);
  const clampZoom = (z: number) => Math.max(0.5, Math.min(3, z));

  useEffect(() => {
    const el = box.current!;
    const wheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      setZoom((z) => clampZoom(z * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  }, []);

  const has = new Set(step.nodes);
  const pointed = new Set<number>();
  for (const f of step.frames) {
    for (const v of f.vars) if (v.ptr !== undefined) pointed.add(v.ptr);
    if (f.returns?.ptr !== undefined) pointed.add(f.returns.ptr);
  }
  const node = (k: number) => [TREE_X + POS[k][0], POS[k][1]] as const;

  const svg: ReactNode[] = [];
  // tree edges and empty leaves
  for (const k of step.nodes) {
    const [x, y] = node(k);
    KIDS[k].forEach((c, side) => {
      const lbl = side ? "right" : "left";
      if (c !== null && has.has(c)) {
        const [cx, cy] = node(c);
        svg.push(<path key={`e${k}${c}`} d={`M${x} ${y + NODE_R} C ${x} ${y + 30}, ${cx} ${cy - 30}, ${cx} ${cy - NODE_R - 2}`} className={s.edge} markerEnd="url(#vz-a)" />);
        svg.push(<text key={`l${k}${c}`} x={(x + cx) / 2 + (side ? 10 : -10)} y={(y + cy) / 2 - 2} className={s.edgeLabel} textAnchor="middle">{lbl}</text>);
      } else {
        const lx = x + (side ? 14 : -14);
        svg.push(<line key={`n${k}${side}`} x1={x} y1={y + NODE_R} x2={lx} y2={y + 32} className={s.leafEdge} />);
        svg.push(<text key={`z${k}${side}`} x={lx} y={y + 42} className={s.leaf} textAnchor="middle">∅</text>);
      }
    });
  }
  // frame-to-frame arrows
  placed.slice(1).forEach((p, k) => {
    const a = placed[k];
    const x1 = a.x + a.w / 2;
    const y1 = a.y + a.h;
    const x2 = p.x + p.w / 2;
    svg.push(<path key={`f${k}`} d={`M${x1} ${y1} C ${x1} ${y1 + 20}, ${x2} ${p.y - 20}, ${x2} ${p.y - 3}`} className={s.frameArrow} markerEnd="url(#vz-a)" />);
  });
  // pointer arrows from variables to nodes
  placed.forEach((p, fi) => {
    const items = [...p.f.vars.map((v) => v.ptr), ...(p.f.returns ? [p.f.returns.ptr] : [])];
    items.forEach((ptr, r) => {
      if (ptr === undefined || !has.has(ptr)) return;
      const x1 = p.x + p.w - 8;
      const y1 = p.y + p.rows[r];
      const [nx, ny] = node(ptr);
      const x2 = nx - NODE_R - 3;
      svg.push(
        <path
          key={`p${fi}-${r}`}
          d={`M${x1} ${y1} C ${x1 + 60} ${y1}, ${x2 - 60} ${ny}, ${x2} ${ny}`}
          className={fi === placed.length - 1 ? s.ptrHot : s.ptr}
          markerEnd="url(#vz-a)"
        />,
      );
    });
  });

  return (
    <div className={s.stage} data-dim={dim || undefined}>
      <div
        ref={box}
        className={s.stageView}
        role="img"
        aria-label={summary(step)}
        onPointerDown={(e) => {
          if ((e.target as HTMLElement).closest("button")) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          drag.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d) return;
          const r = e.currentTarget.getBoundingClientRect();
          const k = VIEW_W / r.width; // screen px → stage points
          setPan({ x: d.px + (e.clientX - d.x) * k, y: d.py + (e.clientY - d.y) * k });
        }}
        onPointerUp={() => (drag.current = null)}
        onPointerCancel={() => (drag.current = null)}
      >
        <div
          className={s.world}
          style={{
            width: `calc(${cw} * var(--u))`,
            height: `calc(${ch} * var(--u))`,
            transform: `translate(calc(${(VIEW_W - cw * sc) / 2 + pan.x} * var(--u)), calc(${pan.y} * var(--u))) scale(${sc})`,
          }}
        >
          <svg className={s.svg} viewBox={`0 0 ${cw} ${ch}`} aria-hidden="true">
            <defs>
              <marker id="vz-a" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                <path d="M0 0 8 4 0 8z" fill="context-stroke" />
              </marker>
            </defs>
            {svg}
            {step.nodes.map((k) => {
              const [x, y] = node(k);
              return (
                <g key={k} className={pointed.has(k) ? s.nodeHot : s.node}>
                  <circle cx={x} cy={y} r={NODE_R} />
                  <text x={x} y={y + 4} textAnchor="middle">{k}</text>
                </g>
              );
            })}
          </svg>
          {placed.map((p, k) => (
            <div
              key={k}
              className={`${s.frame} ${s[`st_${p.f.status}`]}`}
              style={{ left: `calc(${p.x} * var(--u))`, top: `calc(${p.y} * var(--u))`, width: `calc(${p.w} * var(--u))` }}
            >
              <div className={s.fhead}>
                {p.f.fn === null ? (
                  <span className={s.ftop}>
                    <Glyph name="home" size={10} /> Top level
                  </span>
                ) : (
                  <code>
                    (<span className={s.ffn}>{p.f.fn}</span> {p.f.args.join(" ")})
                  </code>
                )}
                <span className={s.badge}>
                  {p.f.status === "waiting" && <Glyph name="hourglass" size={9} />}
                  {p.f.status === "waiting" ? "waiting" : p.f.status.toUpperCase()}
                </span>
              </div>
              {p.f.vars.map((v) => (
                <div key={v.name} className={s.fvar}>
                  <span className={s.vname}>{v.name}</span>
                  <span className={s.leader} />
                  <span className={s.vval}>{v.value}</span>
                  {v.ptr !== undefined && <span className={s.dot} />}
                </div>
              ))}
              {p.f.returns && (
                <div className={s.fvar}>
                  <span className={s.vret}>returns</span>
                  <span className={s.leader} />
                  <span className={s.vval}>{p.f.returns.value}</span>
                  {p.f.returns.ptr !== undefined && <span className={s.dot} />}
                </div>
              )}
              {p.f.question && (
                <div className={s.fq}>
                  <span className={s.qmark}>◇</span>
                  <code>{p.f.question.code}</code>
                  <span className={p.f.question.answer === "Yes" ? s.yes : s.no}>{p.f.question.answer}</span>
                </div>
              )}
              {p.f.then && (
                <div className={s.fthen}>
                  <span className={s.thenLabel}>THEN</span>
                  <code>
                    <Then segs={p.f.then} />
                  </code>
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
      <div className={s.zoom}>
        <button type="button" aria-label="Zoom out" onClick={() => setZoom((z) => clampZoom(z / 1.2))}>
          −
        </button>
        <span aria-live="off">{pct}%</span>
        <button type="button" aria-label="Zoom in" onClick={() => setZoom((z) => clampZoom(z * 1.2))}>
          +
        </button>
      </div>
    </div>
  );
}
