// The demo terminals' prepared commands. A command is matched exactly (after trimming) against
// this table and always prints what was prepared for the original file; `ls`, `cat` and `pwd`
// read the in-memory workspace. Nothing is executed.
import { ORIGINALS } from "../workspace";

export type FileState = (id: string) => { text: string; edited: boolean };

export type TermLine = { kind: "prompt"; cwd: string; cmd: string } | { kind: "out"; text: string } | { kind: "muted"; text: string };

type Prepared = { file: string; output: string[] };

/** Prepared commands per project, keyed by the exact command. */
export const PREPARED: Record<string, Record<string, Prepared>> = {
  cs135: {
    "raco test a03/grades.rkt": {
      file: "cs135/a03/grades.rkt",
      output: ['raco test: (submod (file "a03/grades.rkt") test)', "79.75", '(list "A" "B" "A" "C")', "All 4 tests passed!"],
    },
    "racket notes/scratch.rkt": {
      file: "cs135/notes/scratch.rkt",
      output: [`'(("the" . 2) ("and" . 1) ("hat" . 1) ("cat" . 1))`],
    },
    "raco test notes/scratch.rkt": {
      file: "cs135/notes/scratch.rkt",
      output: ['raco test: "notes/scratch.rkt"', `'(("the" . 2) ("and" . 1) ("hat" . 1) ("cat" . 1))`],
    },
  },
  cs136: {
    "raco test a02/queue.rkt": { file: "cs136/a02/queue.rkt", output: ['raco test: "a02/queue.rkt"'] },
    "racket a02/queue.rkt": { file: "cs136/a02/queue.rkt", output: [] },
  },
  math239: {
    "raco test graphs/degree.rkt": {
      file: "math239/graphs/degree.rkt",
      output: ['raco test: "graphs/degree.rkt"', "'((a . 2) (b . 2) (c . 3) (d . 1))"],
    },
    "racket graphs/degree.rkt": { file: "math239/graphs/degree.rkt", output: ["'((a . 2) (b . 2) (c . 3) (d . 1))"] },
  },
};

/** Workspace paths shown in a project (internal copies used by other demos are left out). */
export function projectFiles(project: string): string[] {
  return Object.keys(ORIGINALS)
    .filter((k) => k.startsWith(project + "/") && !k.includes("-debug"))
    .map((k) => k.slice(project.length + 1));
}

function listDir(project: string, dir: string): string[] | null {
  const pre = dir ? dir.replace(/\/$/, "") + "/" : "";
  const names = new Set<string>();
  for (const f of projectFiles(project)) if (f.startsWith(pre)) names.add(f.slice(pre.length).split("/")[0]);
  // ls sorts by byte order, so capitals come first.
  return names.size ? [...names].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)) : null;
}

export const NOT_KNOWN = "zsh: this demo terminal only knows a few prepared commands. Type help.";

/** The reply to one command line, or "clear". */
export function runCommand(project: string, line: string, files: FileState): TermLine[] | "clear" {
  const cmd = line.trim().replace(/\s+/g, " ");
  if (!cmd) return [];
  if (cmd === "clear") return "clear";
  const out = (text: string): TermLine => ({ kind: "out", text });
  const muted = (text: string): TermLine => ({ kind: "muted", text });
  const prepared = PREPARED[project]?.[cmd];
  if (prepared) {
    // Always the output prepared for the original file. After an edit it plays all the same,
    // after one line saying so; the edited text is never run.
    const { edited } = files(prepared.file);
    const name = prepared.file.split("/").pop();
    return [...(edited ? [muted(`Edited · replaying the original ${name}`)] : []), ...prepared.output.map(out)];
  }
  if (cmd === "raco test" || cmd === "raco test .") {
    // Every prepared raco test in the project, in order, each replayed as above.
    const tests = Object.entries(PREPARED[project] ?? {}).filter(([c]) => c.startsWith("raco test "));
    if (tests.length)
      return tests.flatMap(([, t]) => {
        const name = t.file.split("/").pop();
        return [...(files(t.file).edited ? [muted(`Edited · replaying the original ${name}`)] : []), ...t.output.map(out)];
      });
  }
  if (cmd === "help")
    return [
      out("Prepared commands in this demo terminal:"),
      ...Object.keys(PREPARED[project] ?? {}).map((c) => out(`  ${c}`)),
      out("  raco test ., ls [folder], pwd, cat <file>, clear, help"),
    ];
  if (cmd === "pwd") return [out(`/Users/student/${project}`)];
  if (cmd === "ls" || cmd.startsWith("ls ")) {
    const arg = cmd.slice(2).trim();
    const names = listDir(project, arg);
    if (!names) return [out(`ls: ${arg}: No such file or directory`)];
    return [out(names.join("  "))];
  }
  if (cmd.startsWith("cat ")) {
    const arg = cmd.slice(4).trim();
    if (!projectFiles(project).includes(arg)) return [out(`cat: ${arg}: No such file or directory`)];
    return files(`${project}/${arg}`).text.replace(/\n$/, "").split("\n").map(out);
  }
  return [muted(NOT_KNOWN)];
}
