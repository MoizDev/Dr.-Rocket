"use client";
// The cs135 window from editor.png / light.png: grades.rkt open, its tests passing inline and the
// Interactions pane after a run. Used by the hero (dark) and the theme cards (light, and a
// read-only dark mirror). Run and the REPL replay prepared results; nothing is evaluated.
// Edits stay in the editor and are never run or overwritten: Run and the REPL keep answering
// for the original file, saying so in one line, and only Restore brings the original back.
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import Code from "../Code";
import Repl, { replKey, type ReplItem } from "../Repl";
import Notice from "../Notice";
import sh from "../Shared.module.css";
import Val from "../Val";
import { EDITOR_KEYS } from "../KeysHelp";
import { EditorTabs, IdeWindow, InteractionsHeader, Sidebar, StatusBar, TestCard, TitleBar, Welcome, ideStyles as s } from "../Ide";
import { IdeIcon } from "../IdeIcons";
import { useFile } from "@/lib/demos/useFile";
import { lookup, missReply } from "@/lib/demos/fixtures";
import { useEntryHint, type HintStep } from "@/lib/demos/hint";
import { GRADES_TABLE, RUNS, SQUARES_TABLE } from "@/lib/demos/data/hero-tables";
import type { CodeDecor } from "@/lib/demos/editor/types";

/** Files in the cs135 tree (relative paths) and the workspace ids behind them. */
export const TREE = ["a03/grades.rkt", "a03/squares.rkt", "a05/bst.rkt", "a05/README.md", "notes/scratch.rkt", "README.md"];
const EDITABLE = new Set(["a03/grades.rkt", "a03/squares.rkt"]);
const RUNNABLE: Record<string, "grades" | "squares"> = { "a03/grades.rkt": "grades", "a03/squares.rkt": "squares" };
type Kind = "grades" | "squares";
const TABLES = { grades: GRADES_TABLE, squares: SQUARES_TABLE };
const LANG = { grades: "ISL", squares: "BSL" } as const;
const FILE_NAME = { grades: "grades.rkt", squares: "squares.rkt" } as const;

/** Names Tab completes at the prompt: the identifiers in each prepared table's inputs. */
const COMPLETIONS = Object.fromEntries(
  Object.entries(TABLES).map(([k, t]) => [k, [...new Set(t.entries.flatMap((e) => e.input.match(/[a-z][a-z0-9?!*-]*/g) ?? []))]]),
) as Record<Kind, string[]>;

export const HERO_KEYS: [string, string][] = [
  ["⌘R", "Run (Ctrl+R elsewhere)"],
  ["↑ / ↓", "REPL history"],
  ["Tab", "Complete a name at the prompt"],
  ["Ctrl+L", "Clear the Interactions"],
  ...EDITOR_KEYS,
];

/** The lines holding a check-expect, which get a "passed" badge while the fixture holds. */
export function checkLines(text: string): number[] {
  return text.split("\n").flatMap((l, i) => (/^\s*\(check-expect\b/.test(l) ? [i + 1] : []));
}

function runItems(kind: Kind): ReplItem[] {
  const r = RUNS[kind];
  return [
    { key: `w-${kind}`, out: <Welcome language={r.language} /> },
    ...r.values.map((v, i) => ({ key: `v-${kind}-${i}`, out: <Val text={v} /> })),
    { key: `t-${kind}`, out: <TestCard text={r.tests} /> },
  ];
}

/** The Interactions scrollback at rest, as in the screenshot. */
export function restItems(): ReplItem[] {
  return [
    ...runItems("grades"),
    { key: "h1", input: "(letter 71)" },
    { key: "h1o", out: <Val text={'"B"'} /> },
    { key: "h2", input: "(total marks)" },
    { key: "h2o", out: <Val text="319" /> },
  ];
}

export default function GradesIde({
  gradesId,
  readOnlyMirror,
  hint,
  rootRef,
}: {
  /** Workspace id of grades.rkt: the hero's copy, or the theme section's own. */
  gradesId: string;
  /** Static, read-only mirror (the dark theme card). */
  readOnlyMirror?: boolean;
  /** Play the hero's entry hint. */
  hint?: boolean;
  rootRef: React.RefObject<HTMLDivElement | null>;
}) {
  const idOf = useCallback((rel: string) => (rel === "a03/grades.rkt" ? gradesId : `cs135/${rel}`), [gradesId]);
  const [tabs, setTabs] = useState(["a03/squares.rkt", "a03/grades.rkt"]);
  const [active, setActive] = useState("a03/grades.rkt");
  const [expanded, setExpanded] = useState(() => new Set(["a03"]));
  const [cursor, setCursor] = useState({ ln: 1, col: 1 });
  const [items, setItemsState] = useState<ReplItem[]>(restItems);
  /** The program the Interactions belong to: the last file run (grades.rkt at rest). */
  const [ran, setRan] = useState<Kind>("grades");
  const timers = useRef<number[]>([]);
  useEffect(() => () => timers.current.forEach(clearTimeout), []);
  const stopRun = () => {
    timers.current.forEach(clearTimeout);
    timers.current = [];
  };
  const setItems = useCallback((f: (x: ReplItem[]) => ReplItem[]) => setItemsState(f), []);

  const grades = useFile(gradesId);
  const squares = useFile("cs135/a03/squares.rkt");
  const file = useFile(idOf(active));
  const kind = RUNNABLE[active];
  const fileOf = (k: Kind) => (k === "squares" ? squares : grades);
  const ranFile = fileOf(ran);
  const ranEdited = ranFile.status === "edited";

  const decor = useMemo<CodeDecor>(() => {
    if (!kind || file.status === "edited") return {};
    return { after: checkLines(file.text).map((line) => ({ line, text: "passed", cls: s.passed, check: true, label: "test passed" })) };
  }, [kind, file.text, file.status]);

  const run = () => {
    stopRun();
    if (!kind) {
      // Only grades.rkt and squares.rkt have a prepared run; the Interactions keep their program.
      const name = active.split("/").pop()!;
      setItemsState((xs) => [...xs, { key: replKey(), out: `No prepared run for ${name}. Open grades.rkt or squares.rkt.`, muted: true }]);
      return;
    }
    // The prepared run of the original file. After an edit it plays all the same, after one line
    // saying so; the edited text stays in the editor and is never run.
    const f = fileOf(kind);
    const note: ReplItem[] =
      f.status === "edited"
        ? [{ key: replKey(), out: <Notice compact onRestore={() => f.set(f.original)} text={`Edited · replaying the original ${FILE_NAME[kind]}`} /> }]
        : [];
    const all = [...note, ...runItems(kind).map((it) => ({ ...it, key: replKey() }))];
    setRan(kind);
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return setItemsState(all);
    setItemsState([]);
    all.forEach((it, i) => timers.current.push(window.setTimeout(() => setItemsState((xs) => [...xs, it]), 80 + i * 100)));
  };

  const open = (rel: string) => {
    setTabs((t) => (t.includes(rel) ? t : [...t, rel]));
    setActive(rel);
  };
  const close = (rel: string) => {
    if (tabs.length < 2) return;
    const i = tabs.indexOf(rel);
    const next = tabs.filter((t) => t !== rel);
    setTabs(next);
    if (rel === active) setActive(next[Math.min(i, next.length - 1)]);
  };
  const clear = () => {
    stopRun();
    setItemsState([]);
  };

  // Entry hint: the prompt types (total marks) and prints 319, ending on the rest state.
  const hintSteps = useCallback((): HintStep[] => {
    const rest = restItems();
    const before = rest.slice(0, -2);
    const text = "(total marks)";
    const steps: HintStep[] = [{ at: 0, run: () => setItemsState(before) }];
    for (let i = 1; i <= text.length; i++)
      steps.push({ at: 300 + i * 75, run: () => setItemsState([...before, { key: "h2", input: text.slice(0, i) }]) });
    steps.push({ at: 300 + text.length * 75 + 350, run: () => setItemsState(rest) });
    return steps;
  }, []);
  useEntryHint(rootRef, hintSteps, () => setItemsState(restItems()), !!hint && !readOnlyMirror);

  const table = TABLES[ran];
  const right: ReactNode = (
    <>
      <InteractionsHeader onClear={readOnlyMirror ? undefined : clear} />
      <Repl
        className={s.repl}
        items={items}
        setItems={setItems}
        answer={(input) => {
          // A lookup in the prepared table for the last-run program. While its file is edited the
          // answer is still the original's, tagged as such.
          const e = lookup(table, input);
          if (!e) {
            const other: Kind = ran === "grades" ? "squares" : "grades";
            return lookup(TABLES[other], input) ? <span className={sh.replHint}>Run {FILE_NAME[other]} first to use that.</span> : null;
          }
          const text = e.output.join("\n");
          return (
            <>
              {e.kind === "error" ? <span className={sh.replError}>{text}</span> : <Val text={text} />}
              {ranEdited && (
                <span className={sh.sampleTag} title={`Prepared answer for the original ${FILE_NAME[ran]}`}>
                  original {FILE_NAME[ran]}
                </span>
              )}
            </>
          );
        }}
        onClear={readOnlyMirror ? undefined : clear}
        miss={missReply(table)}
        completions={COMPLETIONS[ran]}
        label="Interactions"
      />
      <span className={s.replChip} aria-hidden="true" data-decorative-contrast>
        <IdeIcon name="cap" size={11} />
        {LANG[ran]}
      </span>
      <span className={s.send} aria-hidden="true">
        <IdeIcon name="send" size={11} />
      </span>
    </>
  );

  const name = active.split("/").pop()!;
  const summary = kind && file.status !== "edited" ? checkLines(file.text).length : 0;

  return (
    <div
      className={s.fill}
      onKeyDown={(e) => {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r") {
          e.preventDefault();
          run();
        }
      }}
    >
      <IdeWindow
        title={
          <TitleBar
            projects={["cs135"]}
            active="cs135"
            lang={{ label: LANG[kind ?? ran], icon: "cap" }}
            onRun={run}
            runDisabled={null}
          />
        }
        sidebar={
          <Sidebar
            project="cs135"
            files={TREE}
            expanded={expanded}
            onToggle={(p) =>
              setExpanded((x) => {
                const n = new Set(x);
                if (n.has(p)) n.delete(p);
                else n.add(p);
                return n;
              })
            }
            selected={active}
            onOpen={open}
          />
        }
        editor={
          <>
            <EditorTabs
              tabs={tabs.map((t) => ({ path: t, name: t.split("/").pop()!, modified: t === "a03/grades.rkt" ? grades.status !== "pristine" : t === "a03/squares.rkt" ? squares.status !== "pristine" : false }))}
              active={active}
              onSelect={setActive}
              onClose={readOnlyMirror || tabs.length < 2 ? undefined : close}
            />
            <div className={s.codeArea}>
              {kind && file.status === "edited" && !readOnlyMirror && (
                <button type="button" className={sh.editRestore} onClick={() => file.set(file.original)} title={`Restore the original ${name}`} data-restore>
                  <IdeIcon name="refresh" size={10} />
                  Restore
                </button>
              )}
              {summary > 0 && (
                <span className={s.summary} aria-label={`${summary} tests passed`}>
                  <IdeIcon name="check" size={10} />
                  {summary}
                </span>
              )}
              <Code
                key={active}
                text={file.text}
                onChange={file.set}
                label={`${name}, editable`}
                readOnly={readOnlyMirror || !EDITABLE.has(active)}
                decor={decor}
                staticOnly={readOnlyMirror}
                onCursor={(_, ln, col) => setCursor({ ln, col })}
              />
            </div>
          </>
        }
        right={right}
        status={<StatusBar crumbs={["cs135", ...active.split("/").slice(0, -1)]} file={name} ln={cursor.ln} col={cursor.col} keys={HERO_KEYS} />}
      />
    </div>
  );
}
