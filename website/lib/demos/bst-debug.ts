// Navigation over the debugger's prepared pauses (lib/demos/data/debugger-pauses.json). Pure, so
// it is tested without the component.
import type { DPause } from "./bst-walk";

export const BREAK_LINE = 20;

/** The pause index after a command, or "done" when the run finishes. `i` = -1 means not started. */
export function next(pauses: DPause[], i: number, cmd: "continue" | "into" | "over" | "out", breakpoints: Set<number>): number | "done" {
  const d = i >= 0 ? pauses[i].depth : Infinity;
  for (let j = i + 1; j < pauses.length; j++) {
    const p = pauses[j];
    if (cmd === "into") return j;
    if (cmd === "continue" && p.kind === "enter" && breakpoints.has(p.line)) return j;
    if (cmd === "over" && p.depth <= d) return j;
    if (cmd === "out" && p.depth < d) return j;
  }
  return "done";
}

/** The breakpoints the pauses were prepared for: line 20, or none (then Continue just finishes). */
export function breakpointsValid(bp: Set<number>) {
  return bp.size === 0 || (bp.size === 1 && bp.has(BREAK_LINE));
}

export type Val = string | { key: string; left: Val; right: Val };

/** Parses "(make-node k L R)" values for inline expansion; anything else stays a string. */
export function parseVal(src: string): Val {
  let p = 0;
  const ws = () => {
    while (src[p] === " ") p++;
  };
  const atom = (): Val => {
    ws();
    if (src.startsWith("(make-node ", p)) {
      p += 11;
      ws();
      const s = p;
      while (src[p] !== " ") p++;
      const key = src.slice(s, p);
      const left = atom();
      const right = atom();
      ws();
      p++; // ")"
      return { key, left, right };
    }
    if (src.startsWith("'()", p)) return (p += 3), "'()";
    const s = p;
    while (p < src.length && src[p] !== " " && src[p] !== ")") p++;
    return src.slice(s, p);
  };
  const v = atom();
  return p >= src.length ? v : src;
}
