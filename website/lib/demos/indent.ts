// A port of the common cases of Dr. Rocket's indenter (src/editor/sexp.rs `indent_for`), which
// is itself a port of DrRacket's. Used by the demo editors when Enter starts a new line.
import { lex, type Token } from "./racket-lexer";

type HeadType = "begin" | "define" | "lambda" | "forFold";

const FOR_FOLD = ["for/fold", "for/fold:", "for*/fold", "for*/fold:", "for/lists", "for/lists:", "for*/lists", "for*/lists:"];
const DEFINE = ["struct", "local", "struct:", "pdefine:", "match-define", "match-define-values", "pattern"];
const BEGIN = ["case-lambda", "case-lambda:", "pcase-lambda:", "match-lambda", "match-lambda*", "syntax-parser", "cond", "delay", "unit", "compound-unit", "public", "private", "override", "require", "inherit", "with-output-to-string", "with-output-to-bytes", "with-module-reading-parameterization"];
const LAMBDA = ["cases", "instantiate", "super-instantiate", "syntax/loc", "quasisyntax/loc", "datum-case", "match", "match*", "match-let", "match-let*", "match-letrec", "λ", "lambda", "let", "let*", "letrec", "recur", "lambda/kw", "letrec-values", "with-syntax", "with-syntax*", "with-continuation-mark", "module", "module*", "module+", "let/cc", "let/ec", "let-syntax", "letrec-syntax", "letrec-syntaxes+values", "let:", "letrec:", "let*:", "let-values:", "letrec-values:", "let*-values:", "let/cc:", "let/ec:", "lambda:", "λ:", "plambda:", "opt-lambda:", "popt-lambda:", "splicing-let", "splicing-letrec", "splicing-let-values", "splicing-letrec-values", "splicing-let-syntax", "splicing-letrec-syntax", "splicing-let-syntaxes", "splicing-letrec-syntaxes", "splicing-letrec-syntaxes+values", "splicing-local", "splicing-parameterize", "splicing-syntax-parameterize", "do:", "kernel-syntax-case", "syntax-case", "syntax-case*", "syntax-rules", "syntax-id-rules", "syntax-parse", "fluid-let", "let-struct", "let-values", "let*-values", "case", "when", "unless", "class", "class*", "rec", "make-object", "mixin", "do", "opt-lambda", "send*", "with-method", "define-record", "shared", "with-handlers", "interface", "parameterize", "parameterize*", "syntax-parameterize", "call-with-input-file", "call-with-input-file*", "with-input-from-file", "with-input-from-string", "call-with-output-file", "with-output-to-file", "for-all", "big-bang", "type-case"];

export function headType(h: string): HeadType | null {
  if (FOR_FOLD.includes(h)) return "forFold";
  if (DEFINE.includes(h)) return "define";
  if (BEGIN.includes(h)) return "begin";
  if (LAMBDA.includes(h)) return "lambda";
  if (h.startsWith("begin")) return "begin";
  if (h.startsWith("def")) return "define";
  if (h === "for" || h === "for*" || h.startsWith("for/") || h.startsWith("for*/") || h.startsWith("with-")) return "lambda";
  return null;
}

const trivia = (t: Token) => t.kind === "lineComment" || t.kind === "blockComment";
const lineOf = (text: string, pos: number) => text.slice(0, pos).split("\n").length;
const col = (text: string, pos: number) => pos - (text.lastIndexOf("\n", pos - 1) + 1);

function datumEnd(toks: Token[], i: number): number {
  const t = toks[i];
  if (!t) return i + 1;
  if (t.kind === "quote" || t.kind === "datumComment") {
    let j = i + 1;
    while (j < toks.length && trivia(toks[j])) j++;
    return datumEnd(toks, j);
  }
  if (t.kind === "open") return t.partner >= 0 ? t.partner + 1 : toks.length;
  let j = i + 1;
  if (t.kind === "string" || t.kind === "escape")
    while (j < toks.length && (toks[j].kind === "string" || toks[j].kind === "escape") && toks[j].start === toks[j - 1].end) j++;
  return j;
}

/** Desired indentation (columns) for the line that starts at `lineStart`, or null to leave it. */
export function indentFor(text: string, lineStart: number): number | null {
  const toks = lex(text);
  const inside = toks.find((t) => t.start < lineStart && t.end > lineStart);
  if (inside && ["string", "escape", "blockComment", "hereString", "byteString", "regex"].includes(inside.kind)) return null;
  // Innermost open bracket before the line that isn't closed before it.
  let o = -1;
  const stack: number[] = [];
  for (let i = 0; i < toks.length && toks[i].start < lineStart; i++) {
    if (toks[i].kind === "open") stack.push(i);
    else if (toks[i].kind === "close") stack.pop();
  }
  if (stack.length) o = stack[stack.length - 1];
  if (o < 0) return 0;

  const kids: number[] = [];
  let k = o + 1;
  for (;;) {
    while (k < toks.length && trivia(toks[k])) k++;
    const t = toks[k];
    if (!t || t.kind === "close" || t.start >= lineStart) break;
    kids.push(k);
    k = datumEnd(toks, k);
  }
  if (!kids.length) return col(text, toks[o].start) + 1;
  const head = toks[kids[0]];
  const c = col(text, head.start);
  const ht = head.kind === "symbol" ? headType(text.slice(head.start, head.end)) : null;
  if (kids.length === 1) return c + (ht === "begin" || ht === "define" ? 1 : ht === "lambda" ? 3 : 0);
  if (ht === "forFold" && kids.length === 2) return col(text, toks[kids[1]].start);
  if (ht === "define" || ht === "lambda" || ht === "forFold") return c + 1;
  const last = kids[kids.length - 1];
  if (lineOf(text, head.start) === lineOf(text, toks[last].start)) {
    if (head.kind === "hashKeyword") return c;
    return col(text, toks[kids[1]].start);
  }
  const lastLine = lineOf(text, toks[last].start);
  let row = last;
  for (let j = kids.length - 1; j >= 0 && lineOf(text, toks[kids[j]].start) === lastLine; j--) row = kids[j];
  return col(text, toks[row].start);
}
