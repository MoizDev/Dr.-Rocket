// Style spans for a document: token colours (with rainbow brackets by depth) and indent bands.
// The static markup and the CodeMirror view both use these, so they look the same.
import { lex } from "./racket-lexer";

export type Span = { from: number; to: number; cls: string };

/** Class names: `tk-<class>` for tokens, plus `rb-<1..3>` for brackets, `band band-<1..3>` for indents. */
export function styleSpans(text: string): Span[] {
  const out: Span[] = [];
  for (const t of lex(text)) {
    if (t.cls === "bracket") out.push({ from: t.start, to: t.end, cls: `tk-bracket rb-${(t.depth % 3) + 1}` });
    else if (t.cls === "image") continue;
    else out.push({ from: t.start, to: t.end, cls: `tk-${t.cls}` });
  }
  // Indent bands: each 2-column step of leading spaces, in the next rainbow colour.
  let lineStart = 0;
  while (lineStart <= text.length) {
    let nl = text.indexOf("\n", lineStart);
    if (nl < 0) nl = text.length;
    let k = 0;
    while (lineStart + k < nl && text[lineStart + k] === " ") k++;
    if (k < nl - lineStart) {
      for (let c = 0; c + 1 < k + 1 && c < k; c += 2) {
        out.push({ from: lineStart + c, to: lineStart + Math.min(c + 2, k), cls: `band band-${((c / 2) % 3) + 1}` });
      }
    }
    lineStart = nl + 1;
  }
  return out.sort((a, b) => a.from - b.from || a.to - b.to);
}

/** 1-based line and 0-based column of a position. */
export function lineCol(text: string, pos: number) {
  let line = 1;
  let ls = 0;
  for (let i = 0; i < pos && i < text.length; i++) if (text[i] === "\n") {
      line++;
      ls = i + 1;
    }
  return { line, col: pos - ls };
}

/** Position of a 1-based line's start. */
export function lineStart(text: string, line: number) {
  let pos = 0;
  for (let l = 1; l < line; l++) {
    const nl = text.indexOf("\n", pos);
    if (nl < 0) return text.length;
    pos = nl + 1;
  }
  return pos;
}
