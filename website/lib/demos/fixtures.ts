// Prepared results and the rule that withdraws them. Nothing here evaluates code: a result is
// shown only while the text it was prepared for still means the same thing.
import { lex, significant, type Token } from "./racket-lexer";

export type Status = "pristine" | "reformatted" | "edited";

const SEP = "\u0001";

/** The file's meaning for our purposes: its tokens without whitespace and comments. */
export function signature(text: string): string {
  return significant(lex(text))
    .map((t) => text.slice(t.start, t.end))
    .join(SEP);
}

export function status(text: string, original: string): Status {
  if (text === original) return "pristine";
  return signature(text) === signature(original) ? "reformatted" : "edited";
}

/** Significant tokens with their text, for remapping and binding lookups. */
export function sigTokens(text: string): (Token & { text: string })[] {
  return significant(lex(text)).map((t) => ({ ...t, text: text.slice(t.start, t.end) }));
}

/**
 * Moves a position in the original text to the same spot in a reformatted text: the same
 * offset inside the same significant token, or just after the previous one. Returns null when
 * the texts don't share a signature (the fixture no longer applies).
 */
export function remap(pos: number, original: string, text: string): number | null {
  if (text === original) return pos;
  const a = sigTokens(original);
  const b = sigTokens(text);
  if (a.length !== b.length || a.some((t, i) => t.text !== b[i].text)) return null;
  for (let i = 0; i < a.length; i++) {
    if (pos < a[i].start) return i === 0 ? Math.min(pos, b[0].start) : b[i - 1].end + Math.min(pos - a[i - 1].end, b[i].start - b[i - 1].end);
    if (pos <= a[i].end) return b[i].start + (pos - a[i].start);
  }
  const last = a.length - 1;
  return last < 0 ? pos : b[last].end + (pos - a[last].end);
}

/** Maps a [from, to) range through `remap`. */
export function remapRange(r: { from: number; to: number }, original: string, text: string) {
  const from = remap(r.from, original, text);
  const to = remap(r.to, original, text);
  return from === null || to === null ? null : { from, to };
}

export type Entry = { input: string; output: string[]; kind?: "value" | "error" };
export type Table = {
  entries: Entry[];
  /** Shown on a miss; the examples come from the table. */
  examples: string[];
};

/** Looks up a REPL input by token signature, so spacing doesn't matter. Never evaluates. */
export function lookup(table: Table, input: string): Entry | null {
  const sig = signature(input);
  if (!sig) return null;
  return table.entries.find((e) => signature(e.input) === sig) ?? null;
}

export function missReply(table: Table): string {
  const ex = table.examples.slice(0, 2);
  return `This demo only has prepared answers for a few expressions. Try ${ex.join(" or ")}.`;
}

export const DISCLOSURE = "Simulated preview";
export const EDITED_NOTICE =
  "This page doesn't run code. These results were prepared for the original file, so they're hidden while it's changed.";
