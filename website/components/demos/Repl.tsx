"use client";
// The Interactions pane. Input is looked up in a prepared table; nothing is evaluated.
import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
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
  /** When set, every input gets this instead of an answer. */
  disabledReply?: string | null;
  /** Names Tab completes at the prompt (identifiers from the prepared table). */
  completions?: string[];
  /** Ctrl+L at the prompt; empties the scrollback when not given. */
  onClear?: () => void;
  langChip?: string;
  placeholder?: string;
  label?: string;
  className?: string;
};

let seq = 0;
export const replKey = () => `r${++seq}`;

/** Characters that end a Racket identifier, for Tab completion. */
const ID_TAIL = /[^\s()[\]{}",'`;]+$/;

function commonPrefix(xs: string[]): string {
  let p = xs[0] ?? "";
  for (const x of xs) while (!x.startsWith(p)) p = p.slice(0, -1);
  return p;
}

export default function Repl(p: ReplProps) {
  const [value, setValue] = useState("");
  const hist = useRef<string[]>([]);
  const at = useRef(-1);
  const draft = useRef("");
  const log = useRef<HTMLDivElement>(null);

  // Follow new output to the bottom, as the app does (not on first paint: that's the rest state).
  const shown = useRef(p.items.length);
  useLayoutEffect(() => {
    if (p.items.length > shown.current && log.current) log.current.scrollTop = log.current.scrollHeight;
    shown.current = p.items.length;
  }, [p.items.length]);

  const submit = () => {
    const input = value.trim();
    if (!input) return;
    if (hist.current[hist.current.length - 1] !== input) hist.current.push(input);
    at.current = -1;
    draft.current = "";
    setValue("");
    const out = p.disabledReply ? null : p.answer(input);
    p.setItems((xs) => [
      ...xs,
      { key: replKey(), input },
      out === null ? { key: replKey(), out: p.disabledReply ?? p.miss, muted: true } : { key: replKey(), out },
    ]);
  };

  /** Completes the name before the caret. Returns false when there's nothing to complete. */
  const complete = (el: HTMLInputElement): boolean => {
    const caret = el.selectionStart ?? value.length;
    if (!p.completions?.length || caret !== el.selectionEnd) return false;
    const prefix = ID_TAIL.exec(value.slice(0, caret))?.[0];
    if (!prefix) return false;
    const hits = [...new Set(p.completions)].filter((c) => c.startsWith(prefix) && c !== prefix).sort();
    if (!hits.length) return false;
    const add = commonPrefix(hits).slice(prefix.length);
    if (add) {
      setValue(value.slice(0, caret) + add + value.slice(caret));
      requestAnimationFrame(() => el.setSelectionRange(caret + add.length, caret + add.length));
    } else {
      p.setItems((xs) => [...xs, { key: replKey(), out: hits.join("  "), muted: true }]);
    }
    return true;
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
          autoCapitalize="off"
          aria-label="Evaluate a Racket expression"
          placeholder={p.placeholder ?? "Evaluate a Racket expression..."}
          onChange={(e) => {
            setValue(e.target.value);
            at.current = -1;
          }}
          onKeyDown={(e) => {
            const h = hist.current;
            if (e.key === "ArrowUp" && h.length) {
              e.preventDefault();
              if (at.current < 0) draft.current = value;
              at.current = at.current < 0 ? h.length - 1 : Math.max(0, at.current - 1);
              setValue(h[at.current]);
            } else if (e.key === "ArrowDown" && at.current >= 0) {
              e.preventDefault();
              at.current = at.current + 1 >= h.length ? -1 : at.current + 1;
              setValue(at.current < 0 ? draft.current : h[at.current]);
            } else if (e.key === "Tab" && !e.shiftKey && !e.altKey && !e.metaKey && !e.ctrlKey) {
              // Only a real completion keeps Tab; otherwise it moves focus as usual.
              if (complete(e.currentTarget)) e.preventDefault();
            } else if (e.ctrlKey && !e.metaKey && e.key.toLowerCase() === "l") {
              e.preventDefault();
              if (p.onClear) p.onClear();
              else p.setItems(() => []);
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
