import { describe, expect, test } from "vitest";
import json from "@/lib/demos/data/bindings-grades.json";
import { resolve } from "@/lib/demos/data/bindings-resolve";
import { GRADES } from "@/lib/demos/data/files";
import { sigTokens, status } from "@/lib/demos/fixtures";

const tok = (i: number | string) => sigTokens(GRADES)[+i];

describe("bindings-grades.json", () => {
  test("is what the resolver produces for grades.rkt", () => {
    expect(resolve(GRADES)).toEqual(json);
  });

  test("the parameter marks of total has exactly 3 bound occurrences, as the screenshot says", () => {
    const [k, b] = Object.entries(json.binders).find(([, v]) => v.name === "marks" && v.kind === "parameter" && v.uses.length === 3)!;
    expect(tok(k).text).toBe("marks");
    const lines = GRADES.slice(0, tok(k).start).split("\n").length;
    expect(lines).toBe(7);
    expect(b.uses.map((u) => tok(u).text)).toEqual(["marks", "marks", "marks"]);
  });

  test("every binder and use points at a token with the binder's name", () => {
    for (const [k, b] of Object.entries(json.binders)) {
      expect(tok(k).text).toBe(b.name);
      for (const u of b.uses) expect(tok(u).text).toBe(b.name);
    }
    for (const [k, name] of Object.entries(json.imported)) expect(tok(k).text).toBe(name);
  });

  test("the counts follow scope: parameters shadow the top-level marks", () => {
    const by = (name: string, kind: string) => Object.values(json.binders).filter((b) => b.name === name && b.kind === kind);
    expect(by("marks", "definition")[0].uses).toHaveLength(2);
    expect(by("marks", "parameter").map((b) => b.uses.length)).toEqual([2, 3]);
    expect(by("mark", "parameter")[0].uses).toHaveLength(3);
    expect(Object.values(json.imported)).toContain("length");
  });

  test("reformatting keeps the token indexes; editing a name does not", () => {
    const reformatted = GRADES.replace("(define (total marks)\n  (cond", "(define   (total marks)\n      (cond").replace(";; total", ";; total, re-commented");
    expect(status(reformatted, GRADES)).toBe("reformatted");
    expect(resolve(reformatted)).toEqual(json);
    const edited = GRADES.replace("(total (rest marks))", "(total (rest markz))");
    expect(status(edited, GRADES)).toBe("edited");
  });
});
