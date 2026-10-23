"use client";
// The "Keys" help button and popover, listing a demo's shortcuts.
import { useId, useRef, useState } from "react";
import { Icon } from "./icons";
import s from "./Shared.module.css";

export default function KeysHelp({ keys, placement = "up" }: { keys: [string, string][]; placement?: "up" | "down" }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const btn = useRef<HTMLButtonElement>(null);
  return (
    <span className={s.keys}>
      <button
        ref={btn}
        type="button"
        className={s.keysBtn}
        aria-label="Keys"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
        data-keys-help
      >
        <Icon name="help" size={14} />
      </button>
      {open && (
        <div
          id={id}
          role="dialog"
          aria-label="Keyboard shortcuts"
          className={`${s.keysPop} ${placement === "down" ? s.keysDown : ""}`}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              setOpen(false);
              btn.current?.focus();
            }
          }}
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node) && e.relatedTarget !== btn.current) setOpen(false);
          }}
        >
          <dl>
            {keys.map(([k, v]) => (
              <div key={k} className={s.keysRow}>
                <dt><kbd>{k}</kbd></dt>
                <dd>{v}</dd>
              </div>
            ))}
          </dl>
          <button type="button" className={s.keysClose} onClick={() => (setOpen(false), btn.current?.focus())}>
            Close
          </button>
        </div>
      )}
    </span>
  );
}

export const EDITOR_KEYS: [string, string][] = [
  ["⌘Z / ⇧⌘Z", "Undo / redo (Ctrl elsewhere)"],
  ["Enter", "New line, indented like Dr. Rocket"],
  ["Tab", "Leave the editor"],
  ["Esc", "Back to the demo"],
];
