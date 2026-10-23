import { describe, expect, it } from "vitest";
import { GRADES_TABLE, RUNS, letterOf, letterThresholds } from "@/lib/demos/data/hero-tables";
import { GRADES } from "@/lib/demos/data/files";
import { NOT_KNOWN, PREPARED, projectFiles, runCommand } from "@/lib/demos/data/terminal-commands";
import { ORIGINALS } from "@/lib/demos/workspace";
import { fuzzy } from "@/components/demos/projects/ProjectsDemo";
import { checkLines } from "@/components/demos/hero-ide/GradesIde";

// A hand mirror of grades.rkt's cond, so the table can't drift from the source.
const mirror = (m: number) => (m >= 80 ? "A" : m >= 70 ? "B" : m >= 60 ? "C" : "F");

describe("hero REPL tables", () => {
  it("reads the thresholds from grades.rkt", () => {
    expect(letterThresholds(GRADES)).toEqual({ steps: [[80, "A"], [70, "B"], [60, "C"]], otherwise: "F" });
  });
  it("letterOf matches the cond for every mark", () => {
    for (let m = 0; m <= 100; m++) expect(letterOf(m)).toBe(mirror(m));
  });
  it("prepared answers agree with the source", () => {
    const get = (i: string) => GRADES_TABLE.entries.find((e) => e.input === i)?.output[0];
    expect(get("(total marks)")).toBe(String(92 + 78 + 85 + 64));
    expect(get("(average marks)")).toBe(String((92 + 78 + 85 + 64) / 4));
    expect(get("(letter 71)")).toBe('"B"');
    expect(RUNS.grades.values[1]).toBe('(list "A" "B" "A" "C")');
  });
  it("finds check-expect lines", () => {
    expect(checkLines("(a)\n(check-expect 1 1)\n  (check-expect 2 2)")).toEqual([2, 3]);
  });
});

describe("terminal commands", () => {
  const files = (id: string) => ({ text: ORIGINALS[id] ?? "", edited: false });
  it("every prepared command names a real file", () => {
    for (const [p, cmds] of Object.entries(PREPARED)) for (const c of Object.values(cmds)) expect(ORIGINALS[c.file], `${p}: ${c.file}`).toBeDefined();
  });
  it("prepared output, edited withdrawal, unknown", () => {
    const r = runCommand("cs135", "  raco   test a03/grades.rkt ", files);
    expect(r).toContainEqual({ kind: "out", text: "All 4 tests passed!" });
    const e = runCommand("cs135", "raco test a03/grades.rkt", () => ({ text: "", edited: true }));
    expect(e).toHaveLength(1);
    expect((e as { text: string }[])[0].text).toMatch(/doesn't run code/);
    expect(runCommand("cs135", "rm -rf /", files)).toEqual([{ kind: "muted", text: NOT_KNOWN }]);
    expect(runCommand("cs135", "clear", files)).toBe("clear");
  });
  it("ls, pwd and cat read the workspace", () => {
    expect(runCommand("cs135", "pwd", files)).toEqual([{ kind: "out", text: "/Users/student/cs135" }]);
    expect(projectFiles("cs135").some((f) => f.includes("-debug"))).toBe(false);
    const ls = runCommand("cs135", "ls", files) as { text: string }[];
    expect(ls[0].text.split("  ")).toContain("a03");
    expect(runCommand("cs135", "ls nope", files)).toEqual([{ kind: "out", text: "ls: nope: No such file or directory" }]);
    const cat = runCommand("cs135", "cat a03/grades.rkt", files) as { text: string }[];
    expect(cat.map((l) => l.text).join("\n")).toBe(ORIGINALS["cs135/a03/grades.rkt"].replace(/\n$/, ""));
  });
});

describe("quick open", () => {
  it("ranks a file-name hit first and drops non-matches", () => {
    const items = ["a03/grades.rkt", "a05/bst.rkt", "README.md"];
    expect(fuzzy("bst", items)[0]).toBe("a05/bst.rkt");
    expect(fuzzy("zzz", items)).toEqual([]);
    expect(fuzzy("", items)).toEqual(items);
  });
});
