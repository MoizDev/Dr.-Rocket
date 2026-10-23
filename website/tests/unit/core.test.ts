import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { KEYWORDS } from "@/lib/demos/keywords";
import { lex, significant, IMAGE_CHAR } from "@/lib/demos/racket-lexer";
import { lookup, missReply, remap, signature, status, type Table } from "@/lib/demos/fixtures";
import { indentFor } from "@/lib/demos/indent";
import { styleSpans } from "@/lib/demos/spans";
import { getText, original, resetWorkspace, setText, subscribe } from "@/lib/demos/workspace";
import { GRADES } from "@/lib/demos/data/files";

const root = join(__dirname, "../..");

describe("keywords", () => {
  const rust = readFileSync(join(root, "../src/editor/keywords.rs"), "utf8");
  const words = [...rust.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1].replace(/\\(.)/g, "$1"));
  it("matches src/editor/keywords.rs exactly", () => {
    expect(KEYWORDS.length).toBe(words.length);
    expect(KEYWORDS[0]).toBe(words[0]);
    expect(KEYWORDS.at(-1)).toBe(words.at(-1));
    expect([...KEYWORDS]).toEqual(words);
  });
  it("is sorted for binary search", () => {
    const sorted = [...KEYWORDS].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
    expect([...KEYWORDS]).toEqual(sorted);
  });
});

describe("lexer", () => {
  const cls = (src: string) => lex(src).filter((t) => t.cls !== "space").map((t) => [src.slice(t.start, t.end), t.cls]);
  it("colours heads by the app's rule", () => {
    expect(cls("(define (f x) (+ x 1))")).toEqual([
      ["(", "bracket"], ["define", "keyword"], ["(", "bracket"], ["f", "function"], ["x", "variable"], [")", "bracket"],
      ["(", "bracket"], ["+", "operator"], ["x", "variable"], ["1", "number"], [")", "bracket"], [")", "bracket"],
    ]);
  });
  it("knows strings, comments, booleans and constants", () => {
    const got = Object.fromEntries(cls(`"hi" ; c\n#t empty '()`));
    expect(got['"hi"']).toBe("string");
    expect(got["; c"]).toBe("comment");
    expect(got["#t"]).toBe("constant");
    expect(got["empty"]).toBe("constant");
  });
  it("treats the pasted image as one token", () => {
    const t = lex(`(define r ${IMAGE_CHAR})`).find((x) => x.cls === "image");
    expect(t && t.end - t.start).toBe(1);
  });
  it("colours brackets by depth", () => {
    const b = styleSpans("((()))").map((s) => s.cls);
    expect(b).toEqual(["tk-bracket rb-1", "tk-bracket rb-2", "tk-bracket rb-3", "tk-bracket rb-3", "tk-bracket rb-2", "tk-bracket rb-1"]);
  });
  it("drops whitespace and comments from significant tokens", () => {
    expect(significant(lex("(a ; x\n b)")).length).toBe(4);
  });
});

describe("fixtures", () => {
  it("tells pristine, reformatted and edited apart", () => {
    expect(status(GRADES, GRADES)).toBe("pristine");
    expect(status(GRADES.replace("(define", "(define   ") + "\n; note", GRADES)).toBe("reformatted");
    expect(status(GRADES.replace("92", "93"), GRADES)).toBe("edited");
  });
  it("remaps positions through reformatting and refuses edits", () => {
    const a = "(f x)";
    const b = "(f\n   x)";
    expect(remap(3, a, b)).toBe(b.indexOf("x"));
    expect(remap(3, a, "(g x)")).toBeNull();
  });
  it("looks up by token signature, never by evaluation", () => {
    const t: Table = { entries: [{ input: "(total marks)", output: ["319"] }], examples: ["(total marks)", "marks"] };
    expect(lookup(t, "(  total   marks )")?.output).toEqual(["319"]);
    expect(lookup(t, "(+ 1 2)")).toBeNull();
    expect(missReply(t)).toContain("(total marks) or marks");
    expect(signature("")).toBe("");
  });
});

describe("workspace", () => {
  it("shares one file object and restores", () => {
    resetWorkspace();
    let n = 0;
    const off = subscribe("cs135/a03/grades.rkt", () => n++);
    setText("cs135/a03/grades.rkt", "x");
    expect(getText("cs135/a03/grades.rkt")).toBe("x");
    expect(getText("themes/grades.rkt")).toBe(original("themes/grades.rkt"));
    setText("cs135/a03/grades.rkt", original("cs135/a03/grades.rkt"));
    expect(n).toBe(2);
    off();
  });
});

describe("indent", () => {
  const dir = join(root, "tests/fixtures/indent");
  for (const f of readdirSync(dir).filter((x) => x.endsWith(".rkt"))) {
    it(`agrees with DrRacket on ${f} wherever it answers`, () => {
      const text = readFileSync(join(dir, f), "utf8");
      let pos = 0;
      let answered = 0;
      const wrong: string[] = [];
      text.split("\n").forEach((line, i) => {
        const start = pos;
        pos += line.length + 1;
        if (i === 0 || !line.trim() || line.trimStart().startsWith(";")) return;
        const want = line.length - line.trimStart().length;
        const got = indentFor(text, start);
        if (got === null) return;
        answered++;
        if (got !== want) wrong.push(`${f}:${i + 1} want ${want} got ${got}: ${line.trim()}`);
      });
      expect(wrong).toEqual([]);
      expect(answered).toBeGreaterThan(5);
    });
  }
});

describe("security", () => {
  const files = (d: string): string[] =>
    readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? files(join(d, e.name)) : /\.(tsx?|mjs)$/.test(e.name) ? [join(d, e.name)] : []));
  it("demo code never evaluates, fetches or spawns anything", () => {
    const bad = /\beval\s*\(|new\s+Function\b|\bFunction\s*\(|new\s+Worker\b|SharedWorker|\bfetch\s*\(|WebAssembly|XMLHttpRequest|localStorage|sessionStorage|importScripts|<img\b/;
    const hits = [...files(join(root, "components/demos")), ...files(join(root, "lib/demos"))].filter((f) => bad.test(readFileSync(f, "utf8")));
    expect(hits).toEqual([]);
  });
});
