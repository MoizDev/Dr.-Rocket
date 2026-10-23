"use client";
// Dr. Rocket's window chrome, shared by the hero, projects and theme demos: title bar with
// project tabs, the file tree, editor tabs, the Interactions header and the status bar.
import { useState, type ReactNode } from "react";
import { DisclosureChip } from "./DemoRoot";
import KeysHelp from "./KeysHelp";
import { FileIcon, fileIconFor } from "./file-icons";
import { IdeIcon } from "./IdeIcons";
import s from "./Ide.module.css";
import sh from "./Shared.module.css";

export { s as ideStyles };

export function TitleBar(p: {
  projects: string[];
  active: string;
  onProject?: (name: string) => void;
  lang: { label: string; icon: "cap" | "code" };
  onRun: () => void;
  runDisabled?: string | null;
}) {
  const [focus, setFocus] = useState(p.projects.indexOf(p.active));
  return (
    <div className={s.title}>
      <IdeIcon name="menu" size={16} className={s.menu} />
      <div className={s.projects} role="tablist" aria-label="Projects">
        {p.projects.map((name, i) => (
          <button
            key={name}
            type="button"
            role="tab"
            aria-selected={name === p.active}
            tabIndex={i === focus ? 0 : -1}
            className={`${s.project} ${name === p.active ? s.projectOn : ""}`}
            onClick={() => p.onProject?.(name)}
            onKeyDown={(e) => {
              const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
              if (!d) return;
              e.preventDefault();
              const n = (i + d + p.projects.length) % p.projects.length;
              setFocus(n);
              (e.currentTarget.parentElement!.children[n] as HTMLElement).focus();
              p.onProject?.(p.projects[n]);
            }}
          >
            {name}
          </button>
        ))}
      </div>
      <span className={s.plus} aria-hidden="true">
        <IdeIcon name="add" size={15} />
      </span>
      <span className={s.titleRight}>
        <span className={s.langPill}>
          <IdeIcon name={p.lang.icon} size={14} className={p.lang.icon === "cap" ? s.capIcon : s.codeIcon} />
          {p.lang.label}
          <IdeIcon name="chevronDown" size={13} />
        </span>
        <button
          type="button"
          className={s.run}
          aria-label="Run"
          title={p.runDisabled ?? "Run (⌘R)"}
          aria-disabled={p.runDisabled ? true : undefined}
          onClick={() => !p.runDisabled && p.onRun()}
          data-run
        >
          <IdeIcon name="play" size={18} />
        </button>
        <IdeIcon name="layoutLeft" size={15} className={s.layoutIcon} />
        <IdeIcon name="layoutBottom" size={15} className={s.layoutIcon} />
        <IdeIcon name="layoutRight" size={15} className={s.layoutIcon} />
      </span>
    </div>
  );
}

type Node = { path: string; name: string; dir: boolean; depth: number };

/** Visible rows of a tree built from file paths: folders first, then files, by name. */
export function treeRows(files: string[], expanded: Set<string>): Node[] {
  const rows: Node[] = [];
  const walk = (prefix: string, depth: number) => {
    const dirs = new Set<string>();
    const leaves: string[] = [];
    for (const f of files) {
      if (!f.startsWith(prefix)) continue;
      const rest = f.slice(prefix.length);
      const slash = rest.indexOf("/");
      if (slash < 0) leaves.push(rest);
      else dirs.add(rest.slice(0, slash));
    }
    for (const d of [...dirs].sort()) {
      const path = prefix + d;
      rows.push({ path, name: d, dir: true, depth });
      if (expanded.has(path)) walk(path + "/", depth + 1);
    }
    for (const l of leaves.sort()) rows.push({ path: prefix + l, name: l, dir: false, depth });
  };
  walk("", 0);
  return rows;
}

export function Sidebar(p: {
  project: string;
  files: string[];
  expanded: Set<string>;
  onToggle: (path: string) => void;
  selected: string;
  onOpen: (path: string) => void;
  onSearch?: () => void;
}) {
  const rows = treeRows(p.files, p.expanded);
  const sel = Math.max(0, rows.findIndex((r) => r.path === p.selected));
  const [focus, setFocus] = useState<number | null>(null);
  const at = Math.min(focus ?? sel, rows.length - 1);
  const move = (e: React.KeyboardEvent, i: number) => {
    e.preventDefault();
    setFocus(i);
    const tree = (e.currentTarget as HTMLElement).closest("[role=tree]")!;
    (tree.querySelectorAll("[role=treeitem]")[i] as HTMLElement | undefined)?.focus();
  };
  return (
    <div className={s.sidebar}>
      <div className={s.sideTools}>
        <span className={s.sideToolOn}>
          <IdeIcon name="folder" size={14} />
        </span>
        <IdeIcon name="outline" size={14} />
        {p.onSearch ? (
          <button type="button" className={s.sideBtn} aria-label="Quick open (⌘P)" onClick={p.onSearch}>
            <IdeIcon name="search" size={14} />
          </button>
        ) : (
          <IdeIcon name="search" size={14} />
        )}
        <span className={s.grow} />
        <IdeIcon name="newFile" size={14} />
        <IdeIcon name="refresh" size={14} />
        <IdeIcon name="collapse" size={14} />
      </div>
      <div className={s.sideHead}>{p.project.toUpperCase()}</div>
      <div role="tree" aria-label={`${p.project} files`} className={s.tree}>
        {rows.map((r, i) => {
          const open = r.dir && p.expanded.has(r.path);
          return (
            <div
              key={r.path}
              role="treeitem"
              aria-level={r.depth + 1}
              aria-expanded={r.dir ? open : undefined}
              aria-selected={!r.dir && r.path === p.selected}
              tabIndex={i === at ? 0 : -1}
              className={`${s.row} ${!r.dir && r.path === p.selected ? s.rowOn : ""}`}
              style={{ ["--depth" as string]: r.depth }}
              onClick={() => (setFocus(i), r.dir ? p.onToggle(r.path) : p.onOpen(r.path))}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown" && i + 1 < rows.length) move(e, i + 1);
                else if (e.key === "ArrowUp" && i > 0) move(e, i - 1);
                else if (e.key === "Home") move(e, 0);
                else if (e.key === "End") move(e, rows.length - 1);
                else if (e.key === "ArrowRight" && r.dir && !open) {
                  e.preventDefault();
                  p.onToggle(r.path);
                }
                else if (e.key === "ArrowLeft" && r.dir && open) {
                  e.preventDefault();
                  p.onToggle(r.path);
                }
                else if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  if (r.dir) p.onToggle(r.path);
                  else p.onOpen(r.path);
                }
              }}
            >
              <span className={s.chev}>{r.dir && <IdeIcon name={open ? "chevronDown" : "chevronRight"} size={12} />}</span>
              <FileIcon name={fileIconFor(r.name, open, r.dir)} size={13} />
              <span className={s.rowName}>{r.name}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function EditorTabs(p: {
  tabs: { path: string; name: string; modified: boolean }[];
  active: string;
  onSelect: (path: string) => void;
  onClose?: (path: string) => void;
}) {
  const ai = Math.max(0, p.tabs.findIndex((t) => t.path === p.active));
  return (
    <div className={s.tabBar}>
      <div role="tablist" aria-label="Open files" className={s.tabs}>
        {p.tabs.map((t, i) => {
          const on = t.path === p.active;
          const close = on && !t.modified ? p.onClose : undefined;
          return (
            <div key={t.path} className={`${s.tab} ${on ? s.tabOn : ""}`}>
              <button
                type="button"
                role="tab"
                aria-selected={on}
                aria-keyshortcuts={close ? "Delete" : undefined}
                tabIndex={i === ai ? 0 : -1}
                className={s.tabBtn}
                onClick={() => p.onSelect(t.path)}
                onKeyDown={(e) => {
                  const list = e.currentTarget.closest("[role=tablist]")!;
                  if (e.key === "Delete" && close) {
                    e.preventDefault();
                    close(t.path);
                    requestAnimationFrame(() => list.querySelector<HTMLElement>('[role=tab][aria-selected="true"]')?.focus());
                    return;
                  }
                  const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
                  if (!d) return;
                  e.preventDefault();
                  const n = (i + d + p.tabs.length) % p.tabs.length;
                  p.onSelect(p.tabs[n].path);
                  requestAnimationFrame(() => (list.querySelectorAll("[role=tab]")[n] as HTMLElement | undefined)?.focus());
                }}
              >
                <FileIcon name={fileIconFor(t.name)} size={12} />
                {t.name}
                {t.modified && (
                  <span className={s.dirty} title="Edited">
                    ●<span className={s.sr}> (edited)</span>
                  </span>
                )}
              </button>
              {/* A tablist may only hold tabs, so the ✕ is for the pointer and Delete closes from the keyboard. */}
              {on && !t.modified && (
                <span
                  className={`${s.tabClose} ${close ? s.tabCloser : ""}`}
                  aria-hidden="true"
                  title={close ? "Close" : undefined}
                  onClick={close && (() => close(t.path))}
                >
                  <IdeIcon name="close" size={12} />
                </span>
              )}
            </div>
          );
        })}
      </div>
      <span className={s.grow} />
      <IdeIcon name="chevronDown" size={15} className={s.tabTool} />
      <IdeIcon name="more" size={15} className={s.tabTool} />
    </div>
  );
}

/** `onClear` makes the broom a working "clear interactions" button. */
export function InteractionsHeader({ onClear }: { onClear?: () => void } = {}) {
  return (
    <div className={s.rHead}>
      <span className={s.rTabOn}>
        <IdeIcon name="repl" size={13} />
        Interactions
      </span>
      <IdeIcon name="stepper" size={14} className={s.rTool} />
      <IdeIcon name="tree" size={14} className={s.rTool} />
      <IdeIcon name="flask" size={14} className={s.rTool} />
      <IdeIcon name="wand" size={14} className={s.rTool} />
      <span className={s.grow} />
      {onClear ? (
        <button type="button" className={`${s.rTool} ${sh.toolBtn}`} aria-label="Clear interactions" title="Clear (Ctrl+L)" onClick={onClear}>
          <IdeIcon name="broom" size={14} />
        </button>
      ) : (
        <IdeIcon name="broom" size={14} className={s.rTool} />
      )}
      <IdeIcon name="add" size={15} className={s.rTool} />
    </div>
  );
}

export function Welcome({ language }: { language: string }) {
  return (
    <div className={s.welcome}>
      <div>Welcome to Racket 9.3</div>
      <div className={s.welcomeLang}>Language: {language}</div>
    </div>
  );
}

export function TestCard({ text }: { text: string }) {
  return (
    <div className={s.testCard}>
      <IdeIcon name="checkCircle" size={14} className={s.testIcon} />
      {text}
    </div>
  );
}

export function StatusBar(p: { crumbs: string[]; file: string; ln: number; col: number; keys: [string, string][] }) {
  return (
    <div className={s.status}>
      <IdeIcon name="layoutLeft" size={13} />
      <IdeIcon name="problems" size={13} />
      <IdeIcon name="lines" size={13} />
      <span className={s.crumbs}>
        {p.crumbs.map((c) => (
          <span key={c} className={s.crumb}>
            {c}
            <IdeIcon name="chevronRight" size={11} />
          </span>
        ))}
        <FileIcon name={fileIconFor(p.file)} size={11} />
        <span className={s.crumbFile}>{p.file}</span>
      </span>
      <span className={s.grow} />
      <DisclosureChip />
      <KeysHelp keys={p.keys} />
      <span className={s.lnCol} aria-live="off">
        Ln {p.ln}, Col {p.col}
      </span>
      <span>Racket 9.3</span>
    </div>
  );
}

/** The window: title bar on top, three panels, status bar. `bottom` splits the editor column. */
export function IdeWindow(p: {
  title: ReactNode;
  sidebar: ReactNode;
  editor: ReactNode;
  right: ReactNode;
  bottom?: ReactNode;
  status: ReactNode;
  overlay?: ReactNode;
}) {
  return (
    <div className={s.ide}>
      {p.title}
      <div className={`${s.panel} ${s.side}`}>{p.sidebar}</div>
      <div className={`${s.panel} ${s.editor} ${p.bottom ? s.editorSplit : ""}`}>{p.editor}</div>
      {p.bottom && <div className={`${s.panel} ${s.bottom}`}>{p.bottom}</div>}
      <div className={`${s.panel} ${s.right}`}>{p.right}</div>
      {p.status}
      {p.overlay}
    </div>
  );
}
