"use client";
// A Beginning Student file with a pasted image (poster.rkt), and the Interactions pane with the
// images it produced. The image outputs are prepared pictures, drawn as SVG; nothing is run.
import { useCallback, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import DemoRoot, { DisclosureChip } from "../DemoRoot";
import Code from "../Code";
import Repl, { replKey, type ReplItem } from "../Repl";
import Notice from "../Notice";
import KeysHelp, { EDITOR_KEYS } from "../KeysHelp";
import { Icon } from "../icons";
import { FileIcon, fileIconFor } from "../file-icons";
import RImage from "./RImage";
import { useFile } from "@/lib/demos/useFile";
import { useEntryHint } from "@/lib/demos/hint";
import { answer, MISS, RUN_OUTPUT, WELCOME, type Img } from "@/lib/demos/data/richsource-repl";
import type { CodeDecor } from "@/lib/demos/editor/types";
import s from "./RichSource.module.css";

export const LABEL = "A Beginning Student file saved by DrRacket with a pasted rocket image, and the REPL showing the images it produced.";

const POSTER = "cs135/a03/poster.rkt";

type TreeNode = { id: string; name: string; dir?: boolean; children?: TreeNode[]; file?: string };
const TREE: TreeNode[] = [
  {
    id: "a03",
    name: "a03",
    dir: true,
    children: [
      { id: "a03/grades", name: "grades.rkt", file: "cs135/a03/grades.rkt" },
      { id: "a03/poster", name: "poster.rkt", file: POSTER },
      { id: "a03/squares", name: "squares.rkt", file: "cs135/a03/squares.rkt" },
    ],
  },
  {
    id: "a05",
    name: "a05",
    dir: true,
    children: [
      { id: "a05/bst", name: "bst.rkt", file: "cs135/a05/bst.rkt" },
      { id: "a05/readme", name: "README.md" },
    ],
  },
  { id: "notes", name: "notes", dir: true, children: [{ id: "notes/scratch", name: "scratch.rkt", file: "cs135/notes/scratch.rkt" }] },
  { id: "readme", name: "README.md" },
];

const KEYS: [string, string][] = [["⌘R", "Run poster.rkt (Ctrl+R elsewhere)"], ["↑ / ↓", "REPL history, or move in the file tree"], ["← / →", "Collapse / expand a folder"], ...EDITOR_KEYS];

const fileName = (id: string) => id.split("/").pop()!;

/** The static picture: the matched brackets of line 1 boxed and line 1 active, as in the shot. */
const REST_DECOR: CodeDecor = { lines: [{ line: 1, cls: "active" }], marks: [{ from: 0, to: 1, cls: "rs-match" }, { from: 20, to: 21, cls: "rs-match" }] };
const NO_DECOR: CodeDecor = {};

function welcome(): ReplItem[] {
  return [{ key: "w", out: <div className={s.welcome}><div>{WELCOME[0]}</div><div className={s.welcomeLang}>{WELCOME[1]}</div></div> }];
}
const imgItem = (img: Img, uid: string): ReplItem => ({ key: `i-${uid}`, out: <RImage img={img} uid={uid} className={s.img} /> });
const noTests = (): ReplItem => ({
  key: "t",
  out: (
    <div className={s.tests} role="status">
      <Icon name="check" size={12} className={s.testsIcon} />
      No tests
    </div>
  ),
});
const runItems = () => [...welcome(), ...RUN_OUTPUT.map((img, i) => imgItem(img, `run${i}`)), noTests()];

export default function RichSourceDemo({ className }: { className: string }) {
  const root = useRef<HTMLDivElement>(null);
  const poster = useFile(POSTER);
  const [tabs, setTabs] = useState<string[]>(["cs135/a03/grades.rkt", POSTER]);
  const [active, setActive] = useState(POSTER);
  const [open, setOpen] = useState<Record<string, boolean>>({ a03: true });
  const [focusRow, setFocusRow] = useState("a03/poster");
  const [items, setItems] = useState<ReplItem[]>(runItems);
  const [cursor, setCursor] = useState({ line: 1, col: 1 });
  const [live, setLive] = useState(false);
  const [phase, setPhase] = useState<"rest" | "pre" | "in">("rest");
  const uid = useRef(0);

  const edited = poster.status === "edited";

  useEntryHint(
    root,
    () => [
      { at: 0, run: () => setPhase("pre") },
      { at: 250, run: () => setPhase("in") },
    ],
    () => setPhase("rest"),
  );

  const run = useCallback(() => {
    if (active !== POSTER) {
      setItems((xs) => [...xs, { key: replKey(), out: `This demo only has a prepared run for poster.rkt. Open it to run it.`, muted: true }]);
      return;
    }
    if (edited) return; // the notice is already showing
    setItems(runItems());
  }, [active, edited]);

  const openFile = (file: string) => {
    setTabs((t) => (t.includes(file) ? t : [...t, file]));
    setActive(file);
  };
  const closeTab = (file: string) => {
    setTabs((t) => {
      if (t.length === 1) return t;
      const next = t.filter((x) => x !== file);
      if (active === file) setActive(next[Math.max(0, t.indexOf(file) - 1)]);
      return next;
    });
  };

  // Rows in tree order, for arrow keys.
  const rows = useMemo(() => {
    const out: { node: TreeNode; depth: number; parent?: string }[] = [];
    const walk = (ns: TreeNode[], depth: number, parent?: string) => {
      for (const n of ns) {
        out.push({ node: n, depth, parent });
        if (n.dir && open[n.id]) walk(n.children ?? [], depth + 1, n.id);
      }
    };
    walk(TREE, 0);
    return out;
  }, [open]);

  const treeRef = useRef<HTMLDivElement>(null);
  const focusTo = (id: string) => {
    setFocusRow(id);
    requestAnimationFrame(() => treeRef.current?.querySelector<HTMLElement>(`[data-row="${id}"]`)?.focus());
  };
  const activate = (n: TreeNode) => {
    if (n.dir) setOpen((o) => ({ ...o, [n.id]: !o[n.id] }));
    else if (n.file) openFile(n.file);
    setFocusRow(n.id);
  };
  const onTreeKey = (e: KeyboardEvent) => {
    const i = rows.findIndex((r) => r.node.id === focusRow);
    const r = rows[i];
    if (!r) return;
    let handled = true;
    if (e.key === "ArrowDown") focusTo(rows[Math.min(rows.length - 1, i + 1)].node.id);
    else if (e.key === "ArrowUp") focusTo(rows[Math.max(0, i - 1)].node.id);
    else if (e.key === "Home") focusTo(rows[0].node.id);
    else if (e.key === "End") focusTo(rows[rows.length - 1].node.id);
    else if (e.key === "ArrowRight") {
      if (r.node.dir && !open[r.node.id]) setOpen((o) => ({ ...o, [r.node.id]: true }));
      else if (r.node.dir) focusTo(r.node.children![0].id);
    } else if (e.key === "ArrowLeft") {
      if (r.node.dir && open[r.node.id]) setOpen((o) => ({ ...o, [r.node.id]: false }));
      else if (r.parent) focusTo(r.parent);
    } else if (e.key === "Enter" || e.key === " ") activate(r.node);
    else handled = false;
    if (handled) e.preventDefault();
  };

  const onRootKey = (e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "r") {
      e.preventDefault();
      run();
    }
  };

  const restore = () => poster.set(poster.original);
  const shown: ReplItem[] = edited ? [...welcome(), { key: "notice", out: <Notice onRestore={restore} /> }] : items;
  const answerFor = (input: string): ReactNode | null => {
    const img = answer(input);
    return img ? <RImage img={img} uid={`q${++uid.current}`} className={s.img} /> : null;
  };

  const activeFile = active;
  const crumbs = ["cs135", ...activeFile.split("/").slice(1)];

  return (
    <DemoRoot
      id="rich-source"
      className={`${className} ${s.root}`}
      w={2400}
      h={1506}
      label={LABEL}
      rootProps={{ ref: root, tabIndex: -1, onKeyDown: onRootKey, "data-phase": phase }}
    >
      {/* Title bar */}
      <div className={s.titlebar}>
        <span className={s.menu} aria-hidden="true">
          <svg viewBox="0 0 24 24"><path d="M3 6h18v2H3zm0 5h18v2H3zm0 5h18v2H3z" fill="currentColor" /></svg>
        </span>
        <span className={s.projectTab}>cs135</span>
        <span className={s.plus} aria-hidden="true"><Icon name="add" size={14} /></span>
        <span className={s.titleRight}>
          <span className={s.langPill}>
            <svg viewBox="0 0 24 24" className={s.hat} aria-hidden="true"><path d="M12 3 1 9l11 6 9-4.9V17h2V9zM5 13.2v4L12 21l7-3.8v-4L12 17z" fill="currentColor" /></svg>
            BSL
            <Icon name="chevronDown" size={12} />
          </span>
          <button type="button" className={s.run} onClick={run} aria-label="Run poster.rkt" title="Run (⌘R)">
            <Icon name="play" size={16} />
          </button>
          <span className={s.layoutIcons} aria-hidden="true">
            <Icon name="sidebar" size={14} />
            <Icon name="panelBottom" size={14} />
            <Icon name="panelRight" size={14} />
          </span>
        </span>
      </div>

      {/* Sidebar */}
      <div className={s.sidebar}>
        <div className={s.sideTools} aria-hidden="true">
          <span className={s.sideToolOn}><svg viewBox="0 0 24 24"><path d="M3 6a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zm2 2v10h14V8z" fill="currentColor" /></svg></span>
          <span><svg viewBox="0 0 24 24"><path d="M3 6h12v2H3zm0 5h12v2H3zm0 5h12v2H3zm14-10h4v2h-4zm0 5h4v2h-4zm0 5h4v2h-4z" fill="currentColor" /></svg></span>
          <span><Icon name="search" size={14} /></span>
          <span className={s.gap} />
          <span><svg viewBox="0 0 24 24"><path d="M6 2h8l6 6v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zm7 1.5V9h5.5zM11 12v3H8v2h3v3h2v-3h3v-2h-3v-3z" fill="currentColor" /></svg></span>
          <span><Icon name="restart" size={14} /></span>
          <span><svg viewBox="0 0 24 24"><path d="m7.4 4.6 1.4-1.4L12 6.4l3.2-3.2 1.4 1.4L12 9.2zm0 14.8 1.4 1.4 3.2-3.2 3.2 3.2 1.4-1.4-4.6-4.6z" fill="currentColor" /></svg></span>
        </div>
        <div className={s.project}>CS135</div>
        <div ref={treeRef} role="tree" aria-label="cs135 files" className={s.tree} onKeyDown={onTreeKey}>
          {rows.map(({ node, depth }) => {
            const isOpen = !!open[node.id];
            const selected = !!node.file && node.file === active;
            return (
              <div
                key={node.id}
                role="treeitem"
                aria-level={depth + 1}
                aria-expanded={node.dir ? isOpen : undefined}
                aria-selected={selected}
                tabIndex={focusRow === node.id ? 0 : -1}
                data-row={node.id}
                className={`${s.row} ${selected ? s.rowSel : ""}`}
                style={{ paddingLeft: `calc(${8 + depth * 12} * var(--u))` }}
                onClick={() => activate(node)}
              >
                <span className={s.chev} aria-hidden="true">
                  {node.dir && <Icon name={isOpen ? "chevronDown" : "next"} size={12} />}
                </span>
                <FileIcon name={fileIconFor(node.name, isOpen, node.dir)} size={13} />
                <span className={s.rowName}>{node.name}</span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Editor */}
      <div className={s.editor}>
        <div className={s.tabs}>
          <div role="tablist" aria-label="Open files" className={s.tablist}>
            {tabs.map((f) => (
              <EditorTab key={f} file={f} active={f === active} onSelect={() => setActive(f)} onClose={() => closeTab(f)} />
            ))}
          </div>
          <span className={s.tabTools} aria-hidden="true">
            <Icon name="chevronDown" size={14} />
            <svg viewBox="0 0 24 24"><path d="M12 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4zm0 2a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm0 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4z" fill="currentColor" /></svg>
          </span>
        </div>
        <div className={s.codeArea} role="tabpanel" aria-label={fileName(active)}>
          <FileEditor
            key={active}
            file={active}
            decor={active === POSTER && !live ? REST_DECOR : NO_DECOR}
            onReady={() => setLive(true)}
            onCursor={(line, col) => setCursor({ line, col })}
          />
          <span className={s.scrollbar} aria-hidden="true" />
        </div>
      </div>

      {/* Interactions */}
      <div className={s.right}>
        <div className={s.rightTabs}>
          <span className={s.rightTabOn}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 5h18v14H3zm2 4v8h14V9z" fill="currentColor" /></svg>
            Interactions
          </span>
          <span className={s.rightIcons} aria-hidden="true">
            <Icon name="next" size={13} />
            <svg viewBox="0 0 24 24"><path d="M4 4h6v6H4zm10 0h6v6h-6zM4 14h6v6H4zm10 3h6v2h-6z" fill="currentColor" /></svg>
            <svg viewBox="0 0 24 24"><path d="M9 3h6v2h-1v4.5l5.5 9A2 2 0 0 1 17.8 21H6.2a2 2 0 0 1-1.7-2.5l5.5-9V5H9z" fill="currentColor" /></svg>
            <svg viewBox="0 0 24 24"><path d="m7 14 2-6 2 6 6 2-6 2-2 6-2-6-6-2zm10-12 1 3 3 1-3 1-1 3-1-3-3-1 3-1z" fill="currentColor" /></svg>
          </span>
          <span className={s.rightIconsEnd} aria-hidden="true">
            <svg viewBox="0 0 24 24"><path d="M12 2a3 3 0 0 1 3 3v1h-6V5a3 3 0 0 1 3-3zM5 8h14l-2 13H7z" fill="currentColor" /></svg>
            <Icon name="add" size={14} />
          </span>
        </div>
        <Repl
          className={s.repl}
          items={shown}
          setItems={setItems}
          answer={answerFor}
          miss={MISS}
          disabledReply={edited ? "This page doesn't run code. poster.rkt was edited here, so its prepared answers are hidden. Restore it above." : null}
          langChip="BSL"
          label="Interactions"
        />
      </div>

      {/* Status bar */}
      <div className={s.status}>
        <span className={s.statusIcons} aria-hidden="true">
          <Icon name="sidebar" size={12} />
          <Icon name="check" size={12} />
          <Icon name="terminal" size={12} />
        </span>
        <span className={s.crumbs}>
          {crumbs.map((c, i) => (
            <span key={i} className={s.crumb}>
              {i > 0 && <span className={s.sep} aria-hidden="true">›</span>}
              {i === crumbs.length - 1 && <FileIcon name={fileIconFor(c)} size={11} />}
              {c}
            </span>
          ))}
        </span>
        <span className={s.statusRight}>
          <DisclosureChip />
          <KeysHelp keys={KEYS} />
          <span aria-live="off">
            Ln {cursor.line}, Col {cursor.col}
          </span>
          <span>Racket 9.3</span>
        </span>
      </div>
    </DemoRoot>
  );
}

function EditorTab({ file, active, onSelect, onClose }: { file: string; active: boolean; onSelect: () => void; onClose: () => void }) {
  const f = useFile(file);
  const modified = f.text !== f.original;
  return (
    <span className={`${s.tab} ${active ? s.tabOn : ""}`}>
      <button type="button" role="tab" aria-selected={active} tabIndex={active ? 0 : -1} className={s.tabBtn} onClick={onSelect}>
        <FileIcon name="racket" size={13} />
        {fileName(file)}
        {modified && <span className={s.dirty} aria-label="modified">●</span>}
      </button>
      {active && !modified && (
        <span className={s.close} aria-hidden="true" onClick={onClose}>
          <Icon name="close" size={11} />
        </span>
      )}
    </span>
  );
}

function FileEditor({ file, decor, onReady, onCursor }: { file: string; decor: CodeDecor; onReady: () => void; onCursor: (line: number, col: number) => void }) {
  const f = useFile(file);
  return (
    <Code
      text={f.text}
      label={`${fileName(file)}, editable`}
      onChange={f.set}
      decor={decor}
      onReady={onReady}
      onCursor={(_p, line, col) => onCursor(line, col)}
      className={s.code}
    />
  );
}
