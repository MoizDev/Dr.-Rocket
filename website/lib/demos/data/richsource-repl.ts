// Prepared Interactions answers for poster.rkt. Inputs are matched by their tokens; nothing is
// evaluated. `(poster "<any string>")` is the one template: the string literal is shown as
// text in the same picture.
import { lex, significant } from "../racket-lexer";
import { signature, missReply } from "../fixtures";
import { TILE } from "../rocket-tile";

export type Img =
  | { kind: "rocket" }
  | { kind: "bar"; title: string }
  | { kind: "beside"; items: Img[] }
  | { kind: "above"; items: Img[] };

export const ROCKET: Img = { kind: "rocket" };
export const BAR = { w: 140, h: 34, font: 16, fill: "#24248c" };

export const poster = (title: string): Img => ({ kind: "above", items: [ROCKET, { kind: "bar", title }] });

/** Rough width of `(text s 16 "white")`: Racket grows the overlay when the text is wider. */
export const textWidth = (s: string) => Math.ceil([...s].length * BAR.font * 0.55);

export function size(img: Img): { w: number; h: number } {
  switch (img.kind) {
    case "rocket":
      return { w: TILE, h: TILE };
    case "bar":
      return { w: Math.max(BAR.w, textWidth(img.title)), h: BAR.h };
    case "beside": {
      const s = img.items.map(size);
      return { w: s.reduce((a, b) => a + b.w, 0), h: Math.max(...s.map((x) => x.h)) };
    }
    case "above": {
      const s = img.items.map(size);
      return { w: Math.max(...s.map((x) => x.w)), h: s.reduce((a, b) => a + b.h, 0) };
    }
  }
}

/** Words for screen readers. */
export function describe(img: Img): string {
  switch (img.kind) {
    case "rocket":
      return "the rocket image";
    case "bar":
      return `a navy bar reading “${img.title}”`;
    case "beside":
      return img.items.every((i) => i.kind === "rocket") ? `${img.items.length} rockets side by side` : `${img.items.map(describe).join(", ")}, side by side`;
    case "above":
      return img.items.length === 2 && img.items[1].kind === "bar" ? `a poster: the rocket above a navy bar reading “${(img.items[1] as { title: string }).title}”` : `${img.items.map(describe).join(" above ")}`;
  }
}

const FIXED: { input: string; img: Img }[] = [
  { input: "rocket", img: ROCKET },
  { input: '(beside rocket rocket rocket)', img: { kind: "beside", items: [ROCKET, ROCKET, ROCKET] } },
  { input: "(above rocket rocket)", img: { kind: "above", items: [ROCKET, ROCKET] } },
];

export const EXAMPLES = ['(poster "Hello")', "(beside rocket rocket rocket)"];
export const MISS = missReply({ entries: [], examples: EXAMPLES });

/** A Racket string literal's contents (the common escapes only). */
export function unquote(lit: string): string {
  return lit.slice(1, -1).replace(/\\(.)/g, (_, c: string) => (c === "n" ? "\n" : c === "t" ? "\t" : c));
}

/** The prepared answer for an input, or null on a miss. Never evaluates. */
export function answer(input: string): Img | null {
  const sig = signature(input);
  for (const f of FIXED) if (signature(f.input) === sig) return f.img;
  const toks = significant(lex(input)).map((t) => ({ ...t, text: input.slice(t.start, t.end) }));
  // (poster "...") — the string may lex as several pieces around escapes.
  if (toks.length >= 4 && toks[0].text === "(" && toks[1].text === "poster" && toks[toks.length - 1].text === ")") {
    const mid = toks.slice(2, -1);
    const lit = input.slice(mid[0].start, mid[mid.length - 1].end);
    if (mid.every((t) => t.kind === "string" || t.kind === "escape") && /^"(?:[^"\\]|\\[\s\S])*"$/.test(lit)) return poster(unquote(lit));
  }
  return null;
}

/** What Run prints for the pristine file: the two image expressions, then the test summary. */
export const RUN_OUTPUT: Img[] = [poster("Launch day"), { kind: "beside", items: [ROCKET, ROCKET, ROCKET] }];
export const WELCOME = ["Welcome to Racket 9.3", "Language: Beginning Student"];
