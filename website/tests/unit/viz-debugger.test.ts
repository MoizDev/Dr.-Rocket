import { describe, expect, test } from "vitest";
import { walk, type DPause, type VStep } from "@/lib/demos/bst-walk";
import { BREAK_LINE, breakpointsValid, next, parseVal } from "@/lib/demos/bst-debug";
import { BST } from "@/lib/demos/data/files";
import traceJson from "@/lib/demos/data/visualizer-trace.json";
import pausesJson from "@/lib/demos/data/debugger-pauses.json";
import { stepDecor, REST as VREST } from "@/components/demos/visualizer/VisualizerDemo";
import { pauseDecor, REST as DREST, RESULT } from "@/components/demos/debugger/DebuggerDemo";

const TRACE = traceJson as unknown as VStep[];
const PAUSES = pausesJson as unknown as DPause[];
const BP = new Set([BREAK_LINE]);

describe("generator", () => {
  test("the committed JSON is what the walker produces for bst.rkt", () => {
    const { trace, pauses, result } = walk(BST);
    expect(trace).toEqual(TRACE);
    expect(pauses).toEqual(PAUSES);
    expect(result).toBe(RESULT);
  });

  test("every highlight and pause range points at the code it names", () => {
    for (const p of PAUSES) expect(BST.slice(p.from, p.to).replace(/\s+/g, " ")).toBe(p.expr);
    for (const t of TRACE) for (const h of t.hl) expect(h.to).toBeGreaterThan(h.from);
  });

  test("tree-sum adds the keys 5 3 8 1 4 9", () => {
    expect(walk(BST).result).toBe(5 + 3 + 8 + 1 + 4 + 9);
  });
});

describe("visualizer", () => {
  test("rests on the poster's step: tree-sum calling itself on an empty left", () => {
    const t = TRACE[VREST];
    expect(t.kind).toBe("CALL");
    expect(t.chip).toBe("(tree-sum (node-left t))");
    expect(t.frames.length).toBeGreaterThanOrEqual(5);
  });

  test("highlights follow a reformatted file and vanish on an edit", () => {
    const step = TRACE[VREST];
    const a = stepDecor(step, BST, BST);
    expect(a.marks?.length).toBeGreaterThan(0);
    const spaced = BST.replace("(tree-sum (node-left t))", "(tree-sum   (node-left t))");
    const b = stepDecor(step, BST, spaced);
    const m = b.marks!.find((x) => spaced.slice(x.from, x.to).startsWith("(tree-sum   (node-left"));
    expect(m).toBeTruthy();
    expect(stepDecor(step, BST, BST.replace("(list 5 3 8 1 4 9)", "(list 5 3 8 1 4 10)"))).toEqual({});
  });

  test("depth never jumps by more than one between steps", () => {
    for (let i = 1; i < TRACE.length; i++) expect(Math.abs(TRACE[i].depth - TRACE[i - 1].depth)).toBeLessThanOrEqual(1);
  });
});

describe("debugger", () => {
  test("rests as the screenshot shows: t = '() four frames deep, three recursive calls", () => {
    const p = PAUSES[DREST];
    expect(p.line).toBe(BREAK_LINE);
    expect(p.frames[0].vars).toEqual([{ name: "t", value: "'()" }]);
    expect(p.frames.slice(1).map((f) => f.label)).toEqual(Array(3).fill("(tree-sum (node-left t))"));
    expect(p.globals.map((g) => g.name)).toEqual(["insert", "tree-sum", "tree"]);
  });

  test("continue stops at each entry on the breakpoint, then finishes", () => {
    let i: number | "done" = -1;
    const stops: number[] = [];
    while ((i = next(PAUSES, i as number, "continue", BP)) !== "done") stops.push(i);
    expect(stops.length).toBe(PAUSES.filter((p) => p.kind === "enter").length);
    expect(stops.every((k) => PAUSES[k].line === BREAK_LINE)).toBe(true);
    expect(next(PAUSES, -1, "continue", new Set())).toBe("done");
  });

  test("step into, over and out move by depth", () => {
    expect(next(PAUSES, 0, "into", BP)).toBe(1);
    const over = next(PAUSES, 1, "over", BP) as number;
    expect(PAUSES[over].depth).toBeLessThanOrEqual(PAUSES[1].depth);
    const out = next(PAUSES, DREST, "out", BP) as number;
    expect(PAUSES[out].depth).toBeLessThan(PAUSES[DREST].depth);
    expect(next(PAUSES, PAUSES.length - 1, "into", BP)).toBe("done");
  });

  test("only {20} or no breakpoints match the prepared run", () => {
    expect(breakpointsValid(BP)).toBe(true);
    expect(breakpointsValid(new Set())).toBe(true);
    expect(breakpointsValid(new Set([20, 22]))).toBe(false);
    expect(breakpointsValid(new Set([22]))).toBe(false);
  });

  test("structure values parse for inline expansion", () => {
    expect(parseVal("(make-node 1 '() '())")).toEqual({ key: "1", left: "'()", right: "'()" });
    const v = parseVal(PAUSES[0].frames[0].vars[0].value);
    expect(typeof v).toBe("object");
    expect(parseVal("insert")).toBe("insert");
  });

  test("the paused range follows reformatting and is withdrawn on edits", () => {
    const p = PAUSES[DREST];
    expect(pauseDecor(p, BP, BST, BST).marks).toEqual([expect.objectContaining({ from: p.from, to: p.to })]);
    const edited = BST.replace("0]", "1]");
    const d = pauseDecor(p, BP, BST, edited);
    expect(d.marks).toBeUndefined();
    expect(d.gutter).toHaveLength(1);
  });
});
