import { describe, expect, test } from "vitest";
import data from "@/lib/demos/data/stepper-squares.json";
import { isSubexpr, pretty, read } from "@/components/demos/stepper/sexpr";
import { SQUARES } from "@/lib/demos/data/files";

type Step = { context: string[]; before: string; beforeHl: number[]; after: string; afterHl: number[]; lines: number[]; rest?: boolean };
const steps = data.steps as Step[];
const ws = (t: string) => t.replace(/\s+/g, " ").trim();
const isValue = (t: string) => /^(-?\d+|#true|#false|'\(\)|\(list( -?\d+)*\))$/.test(t);

describe("stepper-squares.json", () => {
  test("each step's AFTER is the next step's BEFORE, within each expression", () => {
    for (let i = 0; i + 1 < steps.length; i++) {
      if (isValue(steps[i].after) || /^\(check-expect (\S+) \1\)$/.test(steps[i].after)) continue; // expression finished
      expect(steps[i + 1].before, `step ${i + 1}`).toBe(steps[i].after);
    }
  });

  test("there are exactly two stepped expressions, the main one first and the check-expect last", () => {
    const starts = steps.filter((x, i) => i === 0 || steps[i - 1].after !== x.before).map((x) => x.before);
    expect(starts).toEqual(["(sum-squares (cons 3 (cons 4 empty)))", "(check-expect (sum-squares (cons 1 (cons 2 empty))) 5)"]);
  });

  test("every highlight is a real subexpression and AFTER is BEFORE with the redex replaced", () => {
    for (const [i, x] of steps.entries()) {
      expect(isSubexpr(x.before, x.beforeHl[0], x.beforeHl[1]), `before ${i + 1}`).toBe(true);
      expect(isSubexpr(x.after, x.afterHl[0], x.afterHl[1]), `after ${i + 1}`).toBe(true);
      expect(x.before.slice(0, x.beforeHl[0]) + x.after.slice(x.afterHl[0], x.afterHl[1]) + x.before.slice(x.beforeHl[1])).toBe(x.after);
    }
  });

  test("the main expression ends in a value, and the check-expect agrees with it", () => {
    const end = steps.findIndex((x, i) => i + 1 < steps.length && steps[i + 1].before !== x.after);
    expect(steps[end].after).toBe("25");
    expect(isValue(steps[end].after)).toBe(true);
    expect(steps.at(-1)!.after).toBe("(check-expect 5 5)");
  });

  test("exactly one rest step, and it is the screenshot's cond-else step", () => {
    const rest = steps.filter((x) => x.rest);
    expect(rest).toHaveLength(1);
    const r = rest[0];
    expect(steps.indexOf(r) + 1).toBe(6);
    const before = pretty(r.before, 34, r.beforeHl as [number, number]).text;
    expect(before).toBe("(cond\n (else\n  (+\n   (sqr (first (list 3 4)))\n   (sum-squares (rest (list 3 4))))))");
    expect(ws(r.after)).toBe("(+ (sqr (first (list 3 4))) (sum-squares (rest (list 3 4))))");
    expect(r.lines).toEqual([4, 6]);
  });

  test("source lines point inside squares.rkt and everything parses", () => {
    const n = SQUARES.split("\n").length;
    for (const x of steps) {
      read(x.before);
      read(x.after);
      expect(x.lines[0]).toBeGreaterThanOrEqual(1);
      expect(x.lines[1]).toBeLessThanOrEqual(n);
    }
  });
});
