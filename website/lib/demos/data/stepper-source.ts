// Where each prepared stepper step sits in the ORIGINAL squares.rkt: the redex's source (shown
// with BEFORE's colour) and, when the step pulls code out of the program, the source of what
// replaces it (AFTER's colour). Ranges are offsets into SQUARES, found by anchor text once at
// load. Display data only: nothing here reads or runs the visitor's text.
import { SQUARES } from "./files";

export type Range = { from: number; to: number };
export type StepSource = { before: Range | null; after?: Range | null };

/** The (unique) occurrence of `text`, or null if the file no longer contains it. */
function find(text: string): Range | null {
  const at = SQUARES.indexOf(text);
  return at < 0 ? null : { from: at, to: at + text.length };
}

/** From the start of `start` to the end of `end` (searched after it). */
function span(start: string, end: string): Range | null {
  const a = SQUARES.indexOf(start);
  const b = a < 0 ? -1 : SQUARES.indexOf(end, a);
  return a < 0 || b < 0 ? null : { from: a, to: b + end.length };
}

const MAIN_CALL = find("(sum-squares (cons 3 (cons 4 empty)))");
const CONS4 = find("(cons 4 empty)");
const CONS3 = find("(cons 3 (cons 4 empty))");
const CHECK_CALL = find("(sum-squares (cons 1 (cons 2 empty)))");
const CONS2 = find("(cons 2 empty)");
const CONS1 = find("(cons 1 (cons 2 empty))");
/** The function body: `(cond … )`, closed by the `)` after the else clause's `]`. */
const BODY = span("(cond [", "(rest lon)))])");
const EMPTYQ = find("(empty? lon)");
const CLAUSE1 = find("[(empty? lon) 0]");
const ZERO = CLAUSE1 && { from: CLAUSE1.to - 2, to: CLAUSE1.to - 1 };
const ELSE = span("[else", "(rest lon)))]");
const PLUS = span("(+ (sqr", "(rest lon)))");
const FIRST = find("(first lon)");
const SQR = find("(sqr (first lon))");
const REST = find("(rest lon)");
const REC = find("(sum-squares (rest lon))");

/** One recursive pass through the body for a non-empty list, from the call to the `+`. */
const pass = (call: Range | null): StepSource[] => [
  { before: call, after: BODY }, // apply sum-squares: the call becomes the body
  { before: EMPTYQ }, // (empty? (list …)) → #false
  { before: CLAUSE1, after: ELSE }, // the #false clause is dropped
  { before: ELSE, after: PLUS }, // cond else → its answer
  { before: FIRST }, // (first …) → n
  { before: SQR }, // (sqr n) → n²
  { before: REST }, // (rest …) → the shorter list
];

/** The last pass, on '(): the call, the true test, and the cond picking 0. */
const base = (): StepSource[] => [
  { before: REC, after: BODY },
  { before: EMPTYQ },
  { before: CLAUSE1, after: ZERO },
];

/** One entry per step of stepper-squares.json, in order (42). */
export const STEP_SOURCE: StepSource[] = [
  // (sum-squares (cons 3 (cons 4 empty)))
  { before: CONS4 },
  { before: CONS3 },
  ...pass(MAIN_CALL),
  ...pass(REC),
  ...base(),
  { before: PLUS }, // (+ 16 0)
  { before: PLUS }, // (+ 9 16) → 25
  // (check-expect (sum-squares (cons 1 (cons 2 empty))) 5)
  { before: CONS2 },
  { before: CONS1 },
  ...pass(CHECK_CALL),
  ...pass(REC),
  ...base(),
  { before: PLUS }, // (+ 4 0)
  { before: PLUS }, // (+ 1 4) → 5
];
