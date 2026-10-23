// Prepared REPL answers for the projects demo's three #lang racket files. Written out by hand;
// nothing evaluates.
import type { Table } from "../fixtures";

export type ProjectFixture = {
  name: string;
  /** Files open in editor tabs, first is active. Paths are relative to the project. */
  open: string[];
  /** Folders expanded in the tree at rest. */
  expanded: string[];
  table: Table;
  /** What Run prints for the active file. */
  run: Record<string, string[]>;
};

const WORDS = `'(("the" . 2) ("and" . 1) ("hat" . 1) ("cat" . 1))`;
const DEGREES = "'((a . 2) (b . 2) (c . 3) (d . 1))";

export const PROJECTS: ProjectFixture[] = [
  {
    name: "cs136",
    open: ["a02/queue.rkt"],
    expanded: ["a02"],
    table: {
      entries: [
        { input: "q", output: ["(queue '() '(2 1))"] },
        { input: "(queue-pop q)", output: ["(queue '(2) '())"] },
        { input: "(queue-push q 3)", output: ["(queue '() '(3 2 1))"] },
        { input: "(queue-pop (queue-push empty-queue 1))", output: ["(queue '() '())"] },
        { input: "empty-queue", output: ["(queue '() '())"] },
        { input: "(queue-pop empty-queue)", output: ["queue-pop: empty queue"], kind: "error" },
      ],
      examples: ["(queue-pop q)", "(queue-push q 3)"],
    },
    run: { "a02/queue.rkt": [] },
  },
  {
    name: "math239",
    open: ["graphs/degree.rkt"],
    expanded: ["graphs"],
    table: {
      entries: [
        { input: "edges", output: ["'((a b) (a c) (b c) (c d))"] },
        { input: "(degrees edges)", output: ["'#hash((a . 2) (b . 2) (c . 3) (d . 1))"] },
        { input: "(sort (hash->list (degrees edges)) symbol<? #:key car)", output: [DEGREES] },
        { input: "(length edges)", output: ["4"] },
      ],
      examples: ["(degrees edges)", "edges"],
    },
    run: { "graphs/degree.rkt": [DEGREES] },
  },
  {
    name: "cs135",
    open: ["notes/scratch.rkt"],
    expanded: ["notes"],
    table: {
      entries: [
        { input: `(word-counts "the cat and the hat")`, output: [WORDS] },
        { input: `(string-split "the cat and the hat")`, output: [`'("the" "cat" "and" "the" "hat")`] },
        { input: `(word-counts "")`, output: ["'()"] },
        { input: `(string-downcase "The Cat")`, output: [`"the cat"`] },
      ],
      examples: [`(word-counts "the cat and the hat")`, `(string-split "the cat and the hat")`],
    },
    run: { "notes/scratch.rkt": [WORDS] },
  },
];
