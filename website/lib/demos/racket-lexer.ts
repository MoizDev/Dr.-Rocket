// A TypeScript port of Dr. Rocket's Racket lexer (src/editor/lexer.rs), used for the static
// demo markup and for the live CodeMirror editors, so both colour code identically.
// Positions are UTF-16 offsets (every character in the demo files is in the BMP).
import { KEYWORDS } from "./keywords";

export type Kind =
  | "lineComment"
  | "blockComment"
  | "datumComment"
  | "string"
  | "escape"
  | "byteString"
  | "regex"
  | "hereString"
  | "char"
  | "number"
  | "boolean"
  | "hashKeyword"
  | "open"
  | "close"
  | "quote"
  | "symbol"
  | "dot"
  | "lang"
  | "langName"
  | "directive"
  | "image"
  | "error";

export type Class =
  | "plain"
  | "comment"
  | "string"
  | "escape"
  | "regex"
  | "number"
  | "constant"
  | "hashKeyword"
  | "symbol"
  | "keyword"
  | "function"
  | "operator"
  | "variable"
  | "bracket"
  | "punctuation"
  | "lang"
  | "langName"
  | "image"
  | "error";

export type Token = {
  start: number;
  end: number;
  kind: Kind;
  cls: Class;
  /** Nesting depth of the list a bracket opens or closes. */
  depth: number;
  /** Index of the matching bracket, if any. */
  partner: number;
  mismatched: boolean;
};

/** U+FFFC stands for a pasted image in a demo document (one atomic character). */
export const IMAGE_CHAR = "￼";

const DELIMS = new Set(["(", ")", "[", "]", "{", "}", '"', ",", "'", "`", ";"]);
const isWs = (c: string) => /\s/.test(c);
export const isDelimiter = (c: string) => isWs(c) || DELIMS.has(c) || c === IMAGE_CHAR;

export function isKeyword(s: string): boolean {
  let lo = 0;
  let hi = KEYWORDS.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    const k = KEYWORDS[mid];
    if (k === s) return true;
    if (k < s) lo = mid + 1;
    else hi = mid;
  }
  return false;
}

const OPERATORS = new Set(["+", "-", "*", "/", "=", ">", "<", ">=", "<="]);
export const isOperator = (s: string) => OPERATORS.has(s);

export const closerFor = (open: string) => (open === "[" ? "]" : open === "{" ? "}" : ")");

function isUint(s: string, radix: number) {
  if (!s) return false;
  const digits = "0123456789abcdef".slice(0, radix);
  return [...s.toLowerCase()].every((c) => c === "#" || digits.includes(c));
}

function isReal(s: string, radix: number): boolean {
  const lower = s.toLowerCase();
  if (/^[+-](inf|nan)\.[0ft]$/.test(lower)) return true;
  s = s.replace(/^[+-]/, "");
  if (!s) return false;
  const slash = s.indexOf("/");
  if (slash >= 0) return isUint(s.slice(0, slash), radix) && isUint(s.slice(slash + 1), radix);
  if (radix !== 10) return isUint(s, radix);
  const k = s.search(/[eEfFdDsSlLtT]/);
  const mantissa = k >= 0 ? s.slice(0, k) : s;
  const exp = k >= 0 ? s.slice(k + 1) : null;
  const digits = mantissa.replaceAll("#", "0");
  const dot = digits.indexOf(".");
  const okM =
    dot >= 0
      ? (dot > 0 || dot < digits.length - 1) && /^\d*$/.test(digits.slice(0, dot)) && /^\d*$/.test(digits.slice(dot + 1))
      : /^\d+$/.test(digits);
  const okE = exp === null || /^[+-]?\d+$/.test(exp);
  return okM && okE;
}

/** Racket number syntax, as in lexer.rs `is_number`. */
export function isNumber(text: string): boolean {
  let s = text;
  let radix = 10;
  while (s.length >= 2 && s[0] === "#") {
    const p = s[1].toLowerCase();
    if (p === "x") radix = 16;
    else if (p === "b") radix = 2;
    else if (p === "o") radix = 8;
    else if (p === "d") radix = 10;
    else if (p !== "e" && p !== "i") return false;
    s = s.slice(2);
  }
  if (!s) return false;
  const at = s.indexOf("@");
  if (at >= 0) return isReal(s.slice(0, at), radix) && isReal(s.slice(at + 1), radix);
  if (/[iI]$/.test(s) && radix !== 16) {
    const body = s.slice(0, -1);
    if (body === "+" || body === "-") return true;
    for (let k = body.length - 1; k >= 1; k--) {
      if ((body[k] === "+" || body[k] === "-") && !/[eE]/.test(body[k - 1])) {
        const im = body.slice(k);
        return isReal(body.slice(0, k), radix) && (im === "+" || im === "-" || isReal(im, radix));
      }
    }
    return isReal(body, radix);
  }
  return isReal(s, radix);
}

function couldBeNumber(s: string) {
  const c = s[0];
  if (c === undefined) return false;
  if (/[0-9.]/.test(c)) return true;
  if (c === "+" || c === "-") return s.length > 1;
  return c === "#";
}

class Lexer {
  i = 0;
  out: Token[] = [];
  constructor(private s: string) {}

  private push(start: number, kind: Kind, end = this.i) {
    this.out.push({ start, end, kind, cls: "plain", depth: 0, partner: -1, mismatched: false });
  }
  private startsWith(p: string) {
    return this.s.startsWith(p, this.i);
  }
  private atomEnd(j: number) {
    const s = this.s;
    while (j < s.length) {
      const c = s[j];
      if (c === "|") {
        j++;
        while (j < s.length && s[j++] !== "|");
        continue;
      }
      if (c === "\\") {
        j += 2;
        continue;
      }
      if (isDelimiter(c)) break;
      j++;
    }
    return Math.min(j, s.length);
  }
  private lineRest() {
    while (this.i < this.s.length && this.s[this.i] !== "\n") this.i++;
  }
  private stringBody(start: number, kind: Kind) {
    const s = this.s;
    let seg = start;
    while (this.i < s.length) {
      const c = s[this.i];
      if (c === '"') {
        this.i++;
        this.push(seg, kind);
        return;
      }
      if (c === "\\" && kind === "string") {
        if (this.i > seg) this.push(seg, kind);
        const es = this.i++;
        const n = s[this.i];
        if (n === "x") {
          this.i++;
          for (let k = 0; k < 2 && /[0-9a-fA-F]/.test(s[this.i] ?? ""); k++) this.i++;
        } else if (n === "u" || n === "U") {
          const max = n === "u" ? 4 : 8;
          this.i++;
          for (let k = 0; k < max && /[0-9a-fA-F]/.test(s[this.i] ?? ""); k++) this.i++;
        } else if (n !== undefined && /[0-7]/.test(n)) {
          for (let k = 0; k < 3 && /[0-7]/.test(s[this.i] ?? ""); k++) this.i++;
        } else if (n !== undefined) this.i++;
        this.push(es, "escape");
        seg = this.i;
        continue;
      }
      if (c === "\\") {
        this.i += 2;
        continue;
      }
      this.i++;
    }
    this.i = s.length;
    if (this.i > seg) this.push(seg, kind);
  }
  private blockComment(start: number) {
    this.i += 2;
    let depth = 1;
    while (this.i < this.s.length) {
      if (this.startsWith("|#")) {
        this.i += 2;
        if (--depth === 0) break;
      } else if (this.startsWith("#|")) {
        this.i += 2;
        depth++;
      } else this.i++;
    }
    this.push(start, "blockComment");
  }
  private hash(start: number) {
    const s = this.s;
    const next = s[this.i + 1];
    if (next === "|") return this.blockComment(start);
    if (next === ";") {
      this.i += 2;
      return this.push(start, "datumComment");
    }
    if (next === "\\") {
      this.i += 2;
      if (this.i < s.length) {
        const first = s[this.i++];
        if (/\p{L}/u.test(first)) while (this.i < s.length && /[\p{L}\p{N}]/u.test(s[this.i])) this.i++;
      }
      return this.push(start, "char");
    }
    if (next === ":") {
      this.i = this.atomEnd(this.i + 2);
      return this.push(start, "hashKeyword");
    }
    if (next === "(" || next === "[" || next === "{") {
      this.i += 2;
      return this.push(start, "open");
    }
    if (next === "'" || next === "`" || next === "&") {
      this.i += 2;
      return this.push(start, "quote");
    }
    if (next === ",") {
      this.i += s[this.i + 2] === "@" ? 3 : 2;
      return this.push(start, "quote");
    }
    if (next === '"') {
      this.i += 2;
      return this.stringBody(start, "byteString");
    }
    if (next === "!") {
      this.lineRest();
      return this.push(start, "lang");
    }
    if (this.startsWith("#lang") && (s[this.i + 5] === undefined || isWs(s[this.i + 5]))) {
      this.i += 5;
      this.push(start, "lang");
      while (s[this.i] === " " || s[this.i] === "\t") this.i++;
      const ns = this.i;
      this.lineRest();
      if (this.i > ns) this.push(ns, "langName");
      return;
    }
    for (const rx of ['#rx#"', '#px#"', '#rx"', '#px"']) {
      if (this.startsWith(rx)) {
        this.i += rx.length;
        return this.stringBody(start, "regex");
      }
    }
    if (this.startsWith("#reader")) {
      this.i += 7;
      return this.push(start, "directive");
    }
    for (const p of ["#hasheqv(", "#hasheq(", "#hashalw(", "#hash(", "#hasheqv[", "#hasheq[", "#hash[", "#s(", "#s["]) {
      if (this.startsWith(p)) {
        this.i += p.length;
        return this.push(start, "open");
      }
    }
    const end = this.atomEnd(this.i + 1);
    const text = s.slice(start, end);
    this.i = Math.max(end, start + 1);
    const lower = text.toLowerCase();
    const kind: Kind = ["#t", "#f", "#true", "#false"].includes(lower)
      ? "boolean"
      : lower === "#ci" || lower === "#cs"
        ? "directive"
        : text.startsWith("#%")
          ? "symbol"
          : isNumber(text)
            ? "number"
            : "error";
    this.push(start, kind);
  }

  step(): boolean {
    const s = this.s;
    while (this.i < s.length && isWs(s[this.i])) this.i++;
    if (this.i >= s.length) return false;
    const c = s[this.i];
    const start = this.i;
    if (c === ";") {
      this.lineRest();
      this.push(start, "lineComment");
    } else if (c === "(" || c === "[" || c === "{") {
      this.i++;
      this.push(start, "open");
    } else if (c === ")" || c === "]" || c === "}") {
      this.i++;
      this.push(start, "close");
    } else if (c === '"') {
      this.i++;
      this.stringBody(start, "string");
    } else if (c === "'" || c === "`") {
      this.i++;
      this.push(start, "quote");
    } else if (c === ",") {
      this.i += s[this.i + 1] === "@" ? 2 : 1;
      this.push(start, "quote");
    } else if (c === IMAGE_CHAR) {
      this.i++;
      this.push(start, "image");
    } else if (c === "#") this.hash(start);
    else {
      const end = this.atomEnd(this.i);
      this.i = Math.max(end, start + 1);
      const atom = s.slice(start, this.i);
      this.push(start, atom === "." ? "dot" : couldBeNumber(atom) && isNumber(atom) ? "number" : "symbol");
    }
    return true;
  }
}

const isTrivia = (t: Token) => t.kind === "lineComment" || t.kind === "blockComment";

/** Index just past the datum that starts at or after token `i`, skipping trivia. */
function datumEnd(toks: Token[], i: number): number | null {
  while (i < toks.length && isTrivia(toks[i])) i++;
  if (i >= toks.length) return null;
  const t = toks[i];
  if (t.kind === "quote" || t.kind === "datumComment") return datumEnd(toks, i + 1);
  if (t.kind === "open") return t.partner >= 0 ? t.partner + 1 : toks.length;
  if (t.kind === "close") return null;
  // A string is a run of adjacent pieces.
  let j = i + 1;
  if (t.kind === "string" || t.kind === "escape") {
    while (j < toks.length && (toks[j].kind === "string" || toks[j].kind === "escape") && toks[j].start === toks[j - 1].end) j++;
  }
  return j;
}

/** Lexes `text` into classified tokens, as lexer.rs `lex_chars` does. */
export function lex(text: string): Token[] {
  const lx = new Lexer(text);
  while (lx.step());
  const toks = lx.out;

  // Brackets: depth, partners, mismatches.
  const stack: number[] = [];
  toks.forEach((t, i) => {
    if (t.kind === "open") {
      t.depth = stack.length;
      stack.push(i);
    } else if (t.kind === "close") {
      const o = stack.pop();
      if (o === undefined) t.mismatched = true;
      else {
        t.depth = stack.length;
        t.partner = o;
        toks[o].partner = i;
        if (closerFor(text[toks[o].end - 1]) !== text[t.start]) t.mismatched = toks[o].mismatched = true;
      }
    }
  });
  for (const o of stack) toks[o].mismatched = true;

  // Base classes.
  for (const t of toks) {
    const word = text.slice(t.start, t.end);
    t.cls = (
      {
        lineComment: "comment",
        blockComment: "comment",
        datumComment: "comment",
        string: "string",
        byteString: "string",
        hereString: "string",
        escape: "escape",
        regex: "regex",
        char: "constant",
        boolean: "constant",
        number: "number",
        hashKeyword: "hashKeyword",
        open: t.mismatched ? "error" : "bracket",
        close: t.mismatched ? "error" : "bracket",
        quote: "punctuation",
        dot: "punctuation",
        lang: "lang",
        directive: "lang",
        langName: "langName",
        image: "image",
        error: "error",
        symbol: isOperator(word) ? "operator" : word === "empty" ? "constant" : "variable",
      } as const
    )[t.kind];
  }

  // Head position: the first significant token after an open bracket.
  toks.forEach((t, i) => {
    if (t.kind !== "open") return;
    let j = i + 1;
    while (j < toks.length && isTrivia(toks[j])) j++;
    const h = toks[j];
    if (h?.kind === "symbol") {
      const sym = text.slice(h.start, h.end);
      h.cls = isKeyword(sym) ? "keyword" : isOperator(sym) ? "operator" : "function";
    }
  });

  // Quoted data and datum comments.
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    const q = text.slice(t.start, t.end);
    if (t.kind === "quote" && (q === "'" || q === "`")) {
      const e = datumEnd(toks, i + 1);
      if (e !== null)
        for (let k = i + 1; k < e; k++) {
          const u = toks[k];
          if (u.kind === "symbol") u.cls = "symbol";
          else if (u.kind === "open" || u.kind === "close") u.cls = u.mismatched ? "error" : "bracket";
        }
      // '() is the empty list constant.
      if (toks[i + 1]?.kind === "open" && toks[i + 1].partner === i + 2) {
        toks[i].cls = toks[i + 1].cls = toks[i + 2].cls = "constant";
      }
    } else if (t.kind === "datumComment") {
      const e = datumEnd(toks, i + 1);
      if (e !== null) {
        for (let k = i + 1; k < e; k++) toks[k].cls = "comment";
        i = e - 1;
      }
    }
  }
  return toks;
}

/** Tokens that carry meaning: everything except whitespace and comments. */
export function significant(toks: Token[]): Token[] {
  const out: Token[] = [];
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    if (isTrivia(t)) continue;
    if (t.kind === "datumComment") {
      const e = datumEnd(toks, i + 1);
      i = (e ?? i + 1) - 1;
      continue;
    }
    out.push(t);
  }
  return out;
}

/** Splits text into styled runs for the static markup: [text, class, depth][] per line. */
export type Run = { text: string; cls: Class | "space"; depth: number };
export function highlightLines(text: string): Run[][] {
  const toks = lex(text);
  const lines: Run[][] = [[]];
  let pos = 0;
  const emit = (s: string, cls: Run["cls"], depth: number) => {
    const parts = s.split("\n");
    parts.forEach((p, k) => {
      if (k > 0) lines.push([]);
      if (p) lines[lines.length - 1].push({ text: p, cls, depth });
    });
  };
  for (const t of toks) {
    if (t.start > pos) emit(text.slice(pos, t.start), "space", 0);
    emit(text.slice(t.start, t.end), t.cls, t.depth);
    pos = t.end;
  }
  if (pos < text.length) emit(text.slice(pos), "space", 0);
  return lines;
}
