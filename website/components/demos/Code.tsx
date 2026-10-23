"use client";
// A code view. The server renders static highlighted lines; CodeMirror is loaded only when the
// demo nears the viewport or is touched, and then takes over with the same metrics.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { styleSpans } from "@/lib/demos/spans";
import { IMAGE_CHAR } from "@/lib/demos/racket-lexer";
import { rocketTile } from "@/lib/demos/rocket-tile";
import type { CodeDecor, EditorHandle } from "@/lib/demos/editor/types";
import s from "./Code.module.css";

export type CodeProps = {
  text: string;
  label: string;
  onChange?: (text: string) => void;
  readOnly?: boolean;
  firstLine?: number;
  topLine?: number;
  decor?: CodeDecor;
  breakpointGutter?: boolean;
  onGutterClick?: (line: number) => void;
  onCursor?: (pos: number, line: number, col: number) => void;
  /** Called with the handle once CodeMirror is mounted. */
  onReady?: (h: EditorHandle) => void;
  className?: string;
  /** Never upgrade to CodeMirror (decorative mirrors). */
  staticOnly?: boolean;
};

const EMPTY: CodeDecor = {};

/** Splits [from,to) of the text into segments, each with every class covering it. */
function segments(text: string, from: number, to: number, ranges: { from: number; to: number; cls: string; title?: string }[]) {
  const cuts = new Set([from, to]);
  for (const r of ranges) {
    if (r.to <= from || r.from >= to) continue;
    cuts.add(Math.max(r.from, from));
    cuts.add(Math.min(r.to, to));
  }
  for (let i = text.indexOf(IMAGE_CHAR, from); i >= 0 && i < to; i = text.indexOf(IMAGE_CHAR, i + 1)) {
    cuts.add(i);
    cuts.add(i + 1);
  }
  const pts = [...cuts].sort((a, b) => a - b);
  const out: { from: number; to: number; cls: string; title?: string }[] = [];
  for (let i = 0; i + 1 < pts.length; i++) {
    const a = pts[i], b = pts[i + 1];
    const cover = ranges.filter((r) => r.from <= a && r.to >= b);
    out.push({ from: a, to: b, cls: cover.map((r) => r.cls).join(" "), title: cover.find((r) => r.title)?.title });
  }
  return out;
}

export function StaticLines({ text, decor = EMPTY, firstLine = 1, breakpointGutter }: Pick<CodeProps, "text" | "decor" | "firstLine" | "breakpointGutter">) {
  const ranges = [...styleSpans(text), ...(decor.marks ?? [])];
  const lines = text.split("\n");
  const starts: number[] = [];
  lines.reduce((at, line) => (starts.push(at), at + line.length + 1), 0);
  const rows: ReactNode[] = lines.map((line, i) => {
    const n = i + 1;
    const start = starts[i];
    const lineCls = (decor.lines ?? []).filter((l) => l.line === n).map((l) => l.cls).join(" ");
    const bp = (decor.gutter ?? []).find((g) => g.line === n);
    const after = (decor.after ?? []).filter((a) => a.line === n);
    return (
      <div className={s.row} key={n}>
        {breakpointGutter && <span className={s.bpCell}>{bp && <span className={bp.cls} title={bp.label} />}</span>}
        <span className={s.num} data-decorative-contrast>{n + firstLine - 1}</span>
        <span className={`${s.line} ${lineCls}`}>
          {segments(text, start, start + line.length, ranges).map((g) =>
            text[g.from] === IMAGE_CHAR && g.to === g.from + 1 ? (
              <span key={g.from} className={s.image} role="img" aria-label="pasted image: rocket" dangerouslySetInnerHTML={{ __html: rocketTile() }} />
            ) : (
              <span key={g.from} className={g.cls || undefined} title={g.title}>
                {text.slice(g.from, g.to)}
              </span>
            ),
          )}
          {after.map((a, k) => (
            <span key={`a${k}`} className={a.cls} aria-label={a.label} data-decorative-contrast>
              {(a.check ? "✓ " : "") + a.text}
            </span>
          ))}
          {line.length === 0 && "​"}
        </span>
      </div>
    );
  });
  return <div className={s.lines}>{rows}</div>;
}

export default function Code(p: CodeProps) {
  const { text, label, readOnly = false, firstLine = 1, topLine = 1, decor = EMPTY, breakpointGutter = false } = p;
  const host = useRef<HTMLDivElement>(null);
  const handle = useRef<EditorHandle | null>(null);
  const cb = useRef(p);
  useLayoutEffect(() => {
    cb.current = p;
  });
  const [live, setLive] = useState(false);

  // Upgrade when near the viewport, or at once on first touch/focus.
  useEffect(() => {
    if (p.staticOnly) return;
    const el = host.current!;
    let done = false;
    let cancelled = false;
    const go = () => {
      if (done) return;
      done = true;
      io.disconnect();
      import("@/lib/demos/editor").then(({ mountEditor }) => {
        if (cancelled || !host.current) return;
        const mount = el.querySelector<HTMLElement>(`.${s.mount}`)!;
        handle.current = mountEditor(mount, {
          doc: cb.current.text,
          label,
          readOnly,
          firstLine,
          topLine,
          decor: cb.current.decor ?? EMPTY,
          classes: { cm: s.cm, bpGutter: s.bpGutter, image: s.image },
          breakpointGutter,
          onChange: (t) => cb.current.onChange?.(t),
          onCursor: (a, b, c) => cb.current.onCursor?.(a, b, c),
          onGutterClick: cb.current.onGutterClick ? (l) => cb.current.onGutterClick?.(l) : undefined,
          onEscape: () => el.closest<HTMLElement>("[data-demo]")?.focus(),
        });
        // Inactive line numbers are dim on purpose, as in the app.
        mount.querySelector(".cm-gutters")?.setAttribute("data-decorative-contrast", "");
        setLive(true);
        cb.current.onReady?.(handle.current);
      });
    };
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && go(), { rootMargin: "600px" });
    io.observe(el);
    el.addEventListener("pointerdown", go, { once: true });
    el.addEventListener("focusin", go, { once: true });
    return () => {
      cancelled = true;
      io.disconnect();
      handle.current?.destroy();
      handle.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Outside changes (restore, another view of the same file) arrive as one undoable edit.
  useEffect(() => {
    handle.current?.setDoc(text, "input.restore");
  }, [text]);
  useEffect(() => {
    handle.current?.setDecor(decor);
  }, [decor]);

  return (
    <div ref={host} className={`${s.code} ${p.className ?? ""}`} data-live={live || undefined}>
      {!live && (
        <div
          className={s.static}
          tabIndex={p.staticOnly ? undefined : 0}
          role={p.staticOnly ? undefined : "textbox"}
          aria-multiline={p.staticOnly ? undefined : true}
          aria-readonly={readOnly || undefined}
          aria-label={p.staticOnly ? undefined : label}
          style={{ ["--top" as string]: topLine - 1 }}
        >
          <StaticLines text={text} decor={decor} firstLine={firstLine} breakpointGutter={breakpointGutter} />
        </div>
      )}
      <div className={s.mount} />
    </div>
  );
}
