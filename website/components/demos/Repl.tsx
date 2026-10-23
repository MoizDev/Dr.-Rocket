"use client";
// The Interactions pane. Input is looked up in a prepared table; nothing is evaluated.
import { useRef, useState, type ReactNode } from "react";
import s from "./Shared.module.css";

export type ReplItem = { key: string; input?: string; out?: ReactNode; muted?: boolean };

export type ReplProps = {
  /** Scrollback (controlled by the demo so Run can replace it). */
  items: ReplItem[];
  setItems: (f: (items: ReplItem[]) => ReplItem[]) => void;
  /** Prepared answer for an input, or null on a miss. */
  answer: (input: string) => ReactNode | null;
  /** Shown on a miss (muted). */
  miss: string;
  /** When the file was edited, every input gets this instead. */
  disabledReply?: string | null;
  langChip?: string;
  placeholder?: string;
  label?: string;
  className?: string;
};

let seq = 0;
export const replKey = () => `r${++seq}`;

export default function Repl(p: ReplProps) {
  const [value, setValue] = useState("");
  const hist = useRef<string[]>([]);
  const at = useRef(-1);
  const log = useRef<HTMLDivElement>(null);
  const submit = () => {
    const input = value.trim();
    if (!input) return;
    hist.current.push(input);
    at.current = -1;
    setValue("");
    const out = p.disabledReply ? null : p.answer(input);
    p.setItems((xs) => [
      ...xs,
      { key: replKey(), input },
      out === null
        ? { key: replKey(), out: p.disabledReply ?? p.miss, muted: true }
        : { key: replKey(), out },
    ]);
    requestAnimationFrame(() => log.current && (log.current.scrollTop = log.current.scrollHeight));
  };
  return (
    <div className={`${s.repl} ${p.className ?? ""}`}>
      <div ref={log} className={s.replLog} role="log" aria-label={p.label ?? "Interactions"}>
        {p.items.map((it) =>
          it.input !== undefined ? (
            <div key={it.key} className={s.replIn}>
              <span className={s.replPrompt} aria-hidden="true">›</span>
              <span className={s.replCode}>{it.input}</span>
            </div>
          ) : (
            <div key={it.key} className={it.muted ? s.replMuted : s.replOut}>
              {it.out}
            </div>
          ),
        )}
      </div>
      <form
        className={s.replForm}
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <input
          className={s.replInput}
          value={value}
          spellCheck={false}
          autoComplete="off"
          aria-label="Evaluate a Racket expression"
          placeholder={p.placeholder ?? "Evaluate a Racket expression..."}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            const h = hist.current;
            if (e.key === "ArrowUp" && h.length) {
              e.preventDefault();
              at.current = at.current < 0 ? h.length - 1 : Math.max(0, at.current - 1);
              setValue(h[at.current]);
            } else if (e.key === "ArrowDown" && at.current >= 0) {
              e.preventDefault();
              at.current = at.current + 1 >= h.length ? -1 : at.current + 1;
              setValue(at.current < 0 ? "" : h[at.current]);
            } else if (e.key === "Escape") {
              (e.currentTarget.closest("[data-demo]") as HTMLElement | null)?.focus();
            }
          }}
        />
        {p.langChip && <span className={s.langChip}>{p.langChip}</span>}
      </form>
    </div>
  );
}
