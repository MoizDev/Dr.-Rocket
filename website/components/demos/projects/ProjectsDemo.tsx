"use client";
// terminal.png: three project tabs, a #lang racket file, Interactions and the integrated
// terminal. Switching projects swaps the tree, open file, REPL and terminal. ⌘P opens quick open.
import { useCallback, useMemo, useRef, useState } from "react";
import DemoRoot from "../DemoRoot";
import Code from "../Code";
import Repl, { replKey, type ReplItem } from "../Repl";
import Notice from "../Notice";
import Terminal from "../Terminal";
import Val from "../Val";
import { EDITOR_KEYS } from "../KeysHelp";
import { EditorTabs, IdeWindow, InteractionsHeader, Sidebar, StatusBar, TitleBar, Welcome, ideStyles as s } from "../Ide";
import { IdeIcon } from "../IdeIcons";
import { FileIcon, fileIconFor } from "../file-icons";
import { files as wsFiles, REST_LINES } from "../hero-terminal/HeroTerminalDemo";
import { useFile } from "@/lib/demos/useFile";
import { lookup, missReply } from "@/lib/demos/fixtures";
import { useEntryHint, type HintStep } from "@/lib/demos/hint";
import { PROJECTS } from "@/lib/demos/data/projects-tables";
import { projectFiles, type TermLine } from "@/lib/demos/data/terminal-commands";
import p from "./Projects.module.css";

export const LABEL = "Three project tabs, cs136, math239 and cs135, with a #lang racket file open and an integrated terminal showing raco test results.";

const NAMES = PROJECTS.map((x) => x.name);
const KEYS: [string, string][] = [["⌘P", "Quick open (Ctrl+P elsewhere)"], ["⌘R", "Run"], ...EDITOR_KEYS];

type ProjState = { active: string; tabs: string[]; expanded: Set<string>; repl: ReplItem[]; term: TermLine[] };

function runOutput(name: string, path: string): ReplItem[] {
  const out = PROJECTS.find((x) => x.name === name)!.run[path];
  const items: ReplItem[] = [{ key: replKey(), out: <Welcome language="#lang racket" /> }];
  if (!out) items.push({ key: replKey(), out: "There's no prepared run for this file in this demo.", muted: true });
  else out.forEach((v) => items.push({ key: replKey(), out: <Val text={v} /> }));
  return items;
}

function initial(): Record<string, ProjState> {
  const r: Record<string, ProjState> = {};
  for (const x of PROJECTS)
    r[x.name] = {
      active: x.open[0],
      tabs: [...x.open],
      expanded: new Set(x.expanded),
      repl: runOutput(x.name, x.open[0]).map((it, i) => ({ ...it, key: `${x.name}-r${i}` })),
      term: x.name === "cs135" ? REST_LINES : [],
    };
  return r;
}

/** Subsequence match, scored by how tight it is; for quick open. */
export function fuzzy(query: string, items: string[]): string[] {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return items;
  const scored: [number, string][] = [];
  for (const it of items) {
    const s = it.toLowerCase();
    let at = -1, first = -1, gaps = 0;
    let ok = true;
    for (const c of q) {
      const n = s.indexOf(c, at + 1);
      if (n < 0) { ok = false; break; }
      if (first < 0) first = n;
      if (at >= 0) gaps += n - at - 1;
      at = n;
    }
    if (ok) scored.push([gaps * 2 + first + (s.split("/").pop()!.includes(q) ? -50 : 0), it]);
  }
  return scored.sort((a, b) => a[0] - b[0]).map((x) => x[1]);
}

export default function ProjectsDemo({ className }: { className: string }) {
  const root = useRef<HTMLDivElement>(null);
  const [proj, setProj] = useState("cs135");
  const [state, setState] = useState(initial);
  const [cursor, setCursor] = useState({ ln: 1, col: 1 });
  const [palette, setPalette] = useState(false);
  const st = state[proj];
  const update = useCallback((f: (s: ProjState) => ProjState) => setState((all) => ({ ...all, [proj]: f(all[proj]) })), [proj]);
  const fixture = PROJECTS.find((x) => x.name === proj)!;
  const files = useMemo(() => projectFiles(proj), [proj]);
  const id = `${proj}/${st.active}`;
  const file = useFile(id);
  const main = useFile(`${proj}/${fixture.open[0]}`);
  const edited = main.status === "edited";
  const fileEdited = file.status === "edited";

  const open = (path: string) => update((s) => ({ ...s, active: path, tabs: s.tabs.includes(path) ? s.tabs : [...s.tabs, path] }));
  const run = () => {
    if (fileEdited) update((s) => ({ ...s, repl: [{ key: replKey(), out: <Notice onRestore={() => file.set(file.original)} /> }] }));
    else update((s) => ({ ...s, repl: runOutput(proj, s.active) }));
  };

  const steps = useCallback((): HintStep[] => [
    { at: 0, run: () => setProj("cs136") },
    { at: 1100, run: () => setProj("cs135") },
  ], []);
  useEntryHint(root, steps, () => setProj("cs135"));

  const name = st.active.split("/").pop()!;
  const readOnly = !st.active.endsWith(".rkt");

  return (
    <DemoRoot id="projects" className={className} w={2400} h={1506} label={LABEL} rootProps={{ ref: root, tabIndex: -1 }}>
      <div
        className={s.fill}
        onKeyDown={(e) => {
          const k = e.key.toLowerCase();
          if ((e.metaKey || e.ctrlKey) && k === "p") {
            e.preventDefault();
            setPalette(true);
          } else if ((e.metaKey || e.ctrlKey) && k === "r") {
            e.preventDefault();
            run();
          }
        }}
      >
        <IdeWindow
          title={<TitleBar projects={NAMES} active={proj} onProject={setProj} lang={{ label: "#lang racket", icon: "code" }} onRun={run} />}
          sidebar={
            <Sidebar
              key={proj}
              project={proj}
              files={files}
              expanded={st.expanded}
              onToggle={(path) =>
                update((s) => {
                  const n = new Set(s.expanded);
                  if (n.has(path)) n.delete(path);
                  else n.add(path);
                  return { ...s, expanded: n };
                })
              }
              selected={st.active}
              onOpen={open}
              onSearch={() => setPalette(true)}
            />
          }
          editor={
            <>
              <EditorTabs
                tabs={st.tabs.map((t) => ({ path: t, name: t.split("/").pop()!, modified: t === st.active && file.status !== "pristine" }))}
                active={st.active}
                onSelect={(t) => update((s) => ({ ...s, active: t }))}
              />
              <div className={s.codeArea}>
                <Code key={id} text={file.text} onChange={file.set} label={`${name}${readOnly ? "" : ", editable"}`} readOnly={readOnly} onCursor={(_, ln, col) => setCursor({ ln, col })} />
              </div>
            </>
          }
          bottom={
            <Terminal
              key={proj}
              project={proj}
              lines={st.term}
              setLines={(f) => update((s) => ({ ...s, term: f(s.term) }))}
              files={wsFiles}
              problems={fileEdited ? [`Results hidden: ${name} was edited on this page`] : []}
              variant="embedded"
            />
          }
          right={
            <>
              <InteractionsHeader />
              <Repl
                key={proj}
                className={s.repl}
                items={st.repl}
                setItems={(f) => update((s) => ({ ...s, repl: f(s.repl) }))}
                answer={(input) => {
                  const e = lookup(fixture.table, input);
                  return e ? <Val text={e.output.join("\n")} /> : null;
                }}
                miss={missReply(fixture.table)}
                disabledReply={edited ? `This page doesn't run code. ${fixture.open[0].split("/").pop()} was edited, so its prepared answers are hidden. Restore it to try them again.` : null}
              />
              <span className={s.replChip} aria-hidden="true">
                <IdeIcon name="code" size={11} />
                #lang racket
              </span>
              <span className={s.send} aria-hidden="true">
                <IdeIcon name="send" size={11} />
              </span>
            </>
          }
          status={<StatusBar crumbs={[proj, ...st.active.split("/").slice(0, -1)]} file={name} ln={cursor.ln} col={cursor.col} keys={KEYS} />}
          overlay={palette && <QuickOpen files={files} onClose={() => setPalette(false)} onOpen={(f) => (open(f), setPalette(false))} />}
        />
      </div>
    </DemoRoot>
  );
}

function QuickOpen({ files, onOpen, onClose }: { files: string[]; onOpen: (f: string) => void; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const back = useRef<HTMLElement | null>(typeof document !== "undefined" ? (document.activeElement as HTMLElement | null) : null);
  const hits = fuzzy(q, files);
  const close = (f?: string) => {
    if (f) onOpen(f);
    else onClose();
    requestAnimationFrame(() => back.current?.focus());
  };
  return (
    <div className={p.scrim} onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className={p.palette} role="dialog" aria-label="Quick open">
        <div className={p.field}>
          <IdeIcon name="search" size={13} />
          <input
            autoFocus
            role="combobox"
            aria-expanded="true"
            aria-controls="qo-list"
            aria-activedescendant={hits[sel] ? `qo-${sel}` : undefined}
            aria-label="Search files by name"
            placeholder="Search files by name"
            value={q}
            onChange={(e) => (setQ(e.target.value), setSel(0))}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                  close();
                }
              else if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setSel((i) => Math.min(i + 1, hits.length - 1));
                }
              else if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setSel((i) => Math.max(i - 1, 0));
                }
              else if (e.key === "Enter" && hits[sel]) {
                  e.preventDefault();
                  close(hits[sel]);
                }
            }}
          />
        </div>
        <ul id="qo-list" role="listbox" className={p.list}>
          {hits.length === 0 && <li className={p.empty}>No matching files</li>}
          {hits.map((f, i) => (
            <li key={f} id={`qo-${i}`} role="option" aria-selected={i === sel} className={`${p.item} ${i === sel ? p.on : ""}`} onMouseDown={(e) => (e.preventDefault(), close(f))}>
              <FileIcon name={fileIconFor(f)} size={12} />
              <span>{f.split("/").pop()}</span>
              <span className={p.dir}>{f.includes("/") ? f.slice(0, f.lastIndexOf("/")) : ""}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
