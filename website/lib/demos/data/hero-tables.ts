// Prepared REPL answers for the hero and theme demos. Nothing here evaluates code: every answer
// is written out, and a lookup only matches inputs by their tokens.
import type { Table } from "../fixtures";
import { GRADES, SQUARES } from "./files";

/** The `letter` cond clauses read from grades.rkt: [threshold, grade] pairs, then the else grade. */
export function letterThresholds(text: string): { steps: [number, string][]; otherwise: string } {
  const def = text.slice(text.indexOf("(define (letter"));
  const steps = [...def.matchAll(/\[\(>=\s+mark\s+(\d+)\)\s+"([A-Z])"\]/g)].map((m) => [Number(m[1]), m[2]] as [number, string]);
  const otherwise = /\[else\s+"([A-Z])"\]/.exec(def)?.[1] ?? "F";
  return { steps, otherwise };
}

/** What grades.rkt's `letter` gives for a mark, by its own thresholds (a table, not a run). */
export function letterOf(mark: number, text = GRADES): string {
  const { steps, otherwise } = letterThresholds(text);
  for (const [t, g] of steps) if (mark >= t) return g;
  return otherwise;
}

const MARKS = [92, 78, 85, 64];
const q = (s: string) => `"${s}"`;

export const GRADES_TABLE: Table = {
  entries: [
    { input: "marks", output: ["(list 92 78 85 64)"] },
    { input: "(total marks)", output: ["319"] },
    { input: "(average marks)", output: ["79.75"] },
    { input: "(map letter marks)", output: [`(list ${MARKS.map((m) => q(letterOf(m))).join(" ")})`] },
    ...[92, 78, 71, 64, 50].map((n) => ({ input: `(letter ${n})`, output: [q(letterOf(n))] })),
    { input: "(length marks)", output: ["4"] },
    { input: "(first marks)", output: ["92"] },
    { input: "(rest marks)", output: ["(list 78 85 64)"] },
    { input: "(average (list 50 100))", output: ["75"] },
    { input: "(total (list 80 90 70))", output: ["240"] },
    { input: "(average (list 80 90 70))", output: ["80"] },
  ],
  examples: ["(letter 71)", "(total marks)"],
};

export const SQUARES_TABLE: Table = {
  entries: [
    { input: "(sum-squares (cons 3 (cons 4 empty)))", output: ["25"] },
    { input: "(sum-squares (cons 1 (cons 2 empty)))", output: ["5"] },
    { input: "(sum-squares empty)", output: ["0"] },
    { input: "(sqr 4)", output: ["16"] },
  ],
  examples: ["(sum-squares (cons 1 (cons 2 empty)))", "(sqr 4)"],
};

/** What Run prints for each runnable file, and its test summary. */
export type RunFixture = { language: string; values: string[]; tests: string };

export const RUNS: Record<string, RunFixture> = {
  grades: {
    language: "Intermediate Student",
    values: ["79.75", `(list ${MARKS.map((m) => q(letterOf(m))).join(" ")})`],
    tests: "All 4 tests passed",
  },
  squares: { language: "Beginning Student", values: ["25"], tests: "The test passed" },
};

export const SOURCES = { grades: GRADES, squares: SQUARES };
