"use client";
// The integrated terminal: Problems and zsh tabs, scrollback and a one-line prompt. Commands
// are looked up in a prepared table (lib/demos/data/terminal-commands.ts); nothing runs.
import { useRef, useState } from "react";
import {
  runCommand,
  type FileState,
  type TermLine,
} from "@/lib/demos/data/terminal-commands";
import { IdeIcon } from "./IdeIcons";
import { DisclosureChip } from "./DemoRoot";
import KeysHelp from "./KeysHelp";

const TERM_KEYS: [string, string][] = [
  ["Enter", "Run a prepared command (type help)"],
  ["↑ / ↓", "Command history"],
  ["Esc", "Back to the demo"],
];
import s from "./Terminal.module.css";

export type TerminalProps = {
  project: string;
  lines: TermLine[];
  setLines: (f: (l: TermLine[]) => TermLine[]) => void;
  files: FileState;
  problems: string[];
  /** "panel": the standalone hero card (larger, as terminal-panel.png). "embedded": in the IDE. */
  variant: "panel" | "embedded";
  /** Text shown in the prompt by the entry hint. */
  typing?: string | null;
};

export default function Terminal(p: TerminalProps) {
  const [tab, setTab] = useState<"problems" | "zsh">("zsh");
  const [value, setValue] = useState("");
  const hist = useRef<string[]>([]);
  const at = useRef(-1);
  const body = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);

  const submit = () => {
    const cmd = value;
    setValue("");
    at.current = -1;
    if (cmd.trim()) hist.current.push(cmd.trim());
    const reply = runCommand(p.project, cmd, p.files);
    if (reply === "clear") return p.setLines(() => []);
    p.setLines((l) => [
      ...l,
      { kind: "prompt", cwd: p.project, cmd },
      ...reply,
    ]);
    requestAnimationFrame(
      () =>
        body.current && (body.current.scrollTop = body.current.scrollHeight),
    );
  };

  const tabs = ["problems", "zsh"] as const;
  return (
    <div
      className={`${s.term} ${p.variant === "panel" ? s.panel : s.embedded}`}
    >
      <div className={s.tabs}>
        <div role="tablist" aria-label="Bottom panel" className={s.tablist}>
          {tabs.map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={tab === t}
              tabIndex={tab === t ? 0 : -1}
              className={`${s.tab} ${tab === t ? s.tabOn : ""}`}
              onClick={() => setTab(t)}
              onKeyDown={(e) => {
                if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                  e.preventDefault();
                  const n = t === "zsh" ? "problems" : "zsh";
                  setTab(n);
                  (
                    e.currentTarget.parentElement!.querySelector(
                      `[data-tab=${n}]`,
                    ) as HTMLElement | null
                  )?.focus();
                }
              }}
              data-tab={t}
            >
              {t === "problems" ? (
                <>
                  <IdeIcon name="problems" size={13} />
                  Problems
                  {p.problems.length > 0 && (
                    <span className={s.badge}>{p.problems.length}</span>
                  )}
                </>
              ) : (
                <>
                  <IdeIcon name="terminal" size={13} className={s.zshIcon} />
                  zsh
                  <IdeIcon name="close" size={11} className={s.x} />
                </>
              )}
            </button>
          ))}
        </div>
        <span className={s.grow} />
        {p.variant === "panel" && <DisclosureChip />}
        <KeysHelp keys={TERM_KEYS} placement="down" />
        <IdeIcon name="add" size={13} className={s.plus} />
      </div>
      {tab === "problems" ? (
        <div className={s.problems} role="list" aria-label="Problems">
          {p.problems.length === 0 ? (
            <div className={s.none}>No problems</div>
          ) : (
            p.problems.map((t) => (
              <div key={t} role="listitem" className={s.problem}>
                <IdeIcon name="info" size={12} />
                {t}
              </div>
            ))
          )}
        </div>
      ) : (
        <div
          ref={body}
          className={s.body}
          onClick={() =>
            window.getSelection()?.isCollapsed && input.current?.focus()
          }
        >
          <div role="log" aria-label="Terminal output">
            {p.lines.map((l, i) =>
              l.kind === "prompt" ? (
                <div key={i} className={s.line}>
                  <Prompt cwd={l.cwd} />
                  {l.cmd}
                </div>
              ) : (
                <div key={i} className={l.kind === "muted" ? s.muted : s.line}>
                  {l.text || "​"}
                </div>
              ),
            )}
          </div>
          <form
            className={s.line}
            onSubmit={(e) => {
              e.preventDefault();
              submit();
            }}
          >
            <Prompt cwd={p.project} />
            {p.typing != null ? (
              <span className={s.typing}>{p.typing}</span>
            ) : (
              <input
                ref={input}
                className={s.input}
                value={value}
                onChange={(e) => setValue(e.target.value)}
                spellCheck={false}
                autoComplete="off"
                autoCapitalize="off"
                aria-label={`Terminal command in ${p.project}`}
                onKeyDown={(e) => {
                  const h = hist.current;
                  if (e.key === "ArrowUp" && h.length) {
                    e.preventDefault();
                    at.current =
                      at.current < 0
                        ? h.length - 1
                        : Math.max(0, at.current - 1);
                    setValue(h[at.current]);
                  } else if (e.key === "ArrowDown" && at.current >= 0) {
                    e.preventDefault();
                    at.current =
                      at.current + 1 >= h.length ? -1 : at.current + 1;
                    setValue(at.current < 0 ? "" : h[at.current]);
                  } else if (e.key === "Escape") {
                    (
                      e.currentTarget.closest(
                        "[data-demo]",
                      ) as HTMLElement | null
                    )?.focus();
                  }
                }}
              />
            )}
          </form>
        </div>
      )}
    </div>
  );
}

function Prompt({ cwd }: { cwd: string }) {
  return (
    <span className={s.prompt} aria-hidden="true">
      <span className={s.cwd}>{cwd}</span>{" "}
      <span className={s.gt}>&gt;</span>{" "}
    </span>
  );
}
