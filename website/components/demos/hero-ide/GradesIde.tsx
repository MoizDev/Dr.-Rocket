"use client";
// The cs135 window from editor.png / light.png: grades.rkt open, its tests passing inline and the
// Interactions pane after a run. Used by the hero (dark) and the theme cards (light, and a
// read-only dark mirror). Run and the REPL replay prepared results; nothing is evaluated.
import { useCallback, useMemo, useRef, useState, type ReactNode } from "react";
import Code from "../Code";
import Repl, { replKey, type ReplItem } from "../Repl";
import Notice from "../Notice";
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
const TABLES = { grades: GRADES_TABLE, squares: SQUARES_TABLE };
const LANG = { grades: "ISL", squares: "BSL" } as const;

export const HERO_KEYS: [string, string][] = [["⌘R", "Run (Ctrl+R elsewhere)"], ["↑ / ↓", "REPL history"], ...EDITOR_KEYS];

/** The lines holding a check-expect, which get a "passed" badge while the fixture holds. */
export function checkLines(text: string): number[] {
  return text.split("\n").flatMap((l, i) => (/^\s*\(check-expect\b/.test(l) ? [i + 1] : []));
}

function runItems(kind: "grades" | "squares"): ReplItem[] {
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
  const timers = useRef<number[]>([]);
  const setItems = useCallback((f: (x: ReplItem[]) => ReplItem[]) => setItemsState(f), []);

  const grades = useFile(gradesId);
  const squares = useFile("cs135/a03/squares.rkt");
  const file = useFile(idOf(active));
  const kind = RUNNABLE[active];
  const runFile = kind === "squares" ? squares : grades;
  const runKind = kind ?? "grades";
  const edited = runFile.status === "edited";

  const decor = useMemo<CodeDecor>(() => {
    if (!kind || file.status === "edited") return {};
    return { after: checkLines(file.text).map((line) => ({ line, text: "passed", cls: s.passed, check: true, label: "test passed" })) };
  }, [kind, file.text, file.status]);

  const run = useCallback(() => {
    timers.current.forEach(clearTimeout);
    timers.current = [];
    if (edited) {
      setItemsState([{ key: replKey(), out: <Notice onRestore={() => runFile.set(runFile.original)} /> }]);
      return;
    }
    const all = runItems(runKind).map((it) => ({ ...it, key: replKey() }));
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return setItemsState(all);
    setItemsState([]);
    all.forEach((it, i) => timers.current.push(window.setTimeout(() => setItemsState((xs) => [...xs, it]), 80 + i * 100)));
  }, [edited, runFile, runKind]);

  const open = (rel: string) => {
    setTabs((t) => (t.includes(rel) ? t : [...t, rel]));
    setActive(rel);
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

  const table = TABLES[runKind];
  const right: ReactNode = (
    <>
      <InteractionsHeader />
      <Repl
        className={s.repl}
        items={items}
        setItems={setItems}
        answer={(input) => {
          const e = lookup(table, input);
          return e ? <Val text={e.output.join("\n")} /> : null;
        }}
        miss={missReply(table)}
        disabledReply={edited ? `This page doesn't run code. ${kind === "squares" ? "squares" : "grades"}.rkt was edited, so its prepared answers are hidden. Restore it to try them again.` : null}
        langChip={undefined}
        label="Interactions"
      />
      <span className={s.replChip} aria-hidden="true" data-decorative-contrast>
        <IdeIcon name="cap" size={11} />
        {LANG[runKind]}
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
            lang={{ label: LANG[runKind], icon: "cap" }}
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
            />
            <div className={s.codeArea}>
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
