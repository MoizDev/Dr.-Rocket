// Language levels as Dr. Rocket's Choose Language dialog shows them. Copied from
// src/racket/lang.rs (name, summary, GROUPS, default_settings, printing_styles, stepper_level,
// TEACHPACKS) and src/ui/language_dialog.rs; tests/unit/languages.test.ts snapshots them so a
// later sync shows up as a diff.

export type LangId =
  | "module"
  | "beginner"
  | "beginner-abbr"
  | "intermediate"
  | "intermediate-lambda"
  | "advanced"
  | "sdp-beginner"
  | "sdp"
  | "sdp-advanced"
  | "r5rs"
  | "pretty-big";

export type Family = "module" | "htdp" | "deinprogramm" | "legacy";
export type PrintingStyle = "constructor" | "quasiquote" | "write" | "print";
export type FractionStyle = "mixed-fraction" | "repeating-decimal";

export type Level = { id: LangId; name: string; short: string; summary: string; family: Family };

export const LEVELS: Level[] = [
  { id: "module", name: "The Racket Language", short: "Racket", summary: "Start your program with #lang to choose a dialect, for example #lang racket", family: "module" },
  { id: "beginner", name: "Beginning Student", short: "BSL", summary: "define, cond, structs, constants, and primitives", family: "htdp" },
  { id: "beginner-abbr", name: "Beginning Student with List Abbreviations", short: "BSL+", summary: "Beginner, with list style printing in the REPL", family: "htdp" },
  { id: "intermediate", name: "Intermediate Student", short: "ISL", summary: "Beginner plus lexical scope", family: "htdp" },
  { id: "intermediate-lambda", name: "Intermediate Student with lambda", short: "ISL+", summary: "Intermediate plus higher-order functions", family: "htdp" },
  { id: "advanced", name: "Advanced Student", short: "ASL", summary: "Intermediate plus lambda and mutation", family: "htdp" },
  { id: "sdp-beginner", name: "Schreibe Dein Programm! - Anfänger", short: "SDP Anfänger", summary: "Definitions, conditionals, records, and signatures", family: "deinprogramm" },
  { id: "sdp", name: "Schreibe Dein Programm!", short: "SDP", summary: "Anfänger plus higher-order functions and lists", family: "deinprogramm" },
  { id: "sdp-advanced", name: "Schreibe Dein Programm! - fortgeschritten", short: "SDP fortgeschritten", summary: "Plus quote, symbols, and mutation", family: "deinprogramm" },
  { id: "r5rs", name: "R5RS", short: "R5RS", summary: "Scheme as described by the Revised5 Report", family: "legacy" },
  { id: "pretty-big", name: "Pretty Big", short: "Pretty Big", summary: "Racket plus the HtDP teachpacks (legacy)", family: "legacy" },
];

export const GROUPS: { heading: string; title: string; languages: LangId[] }[] = [
  { heading: "The Racket Language", title: "", languages: ["module"] },
  { heading: "Teaching Languages", title: "How to Design Programs", languages: ["beginner", "beginner-abbr", "intermediate", "intermediate-lambda", "advanced"] },
  { heading: "Teaching Languages", title: "DeinProgramm", languages: ["sdp-beginner", "sdp", "sdp-advanced"] },
  { heading: "Other Languages", title: "Legacy Languages", languages: ["r5rs", "pretty-big"] },
];

export const level = (id: LangId) => LEVELS.find((l) => l.id === id)!;
export const isTeaching = (id: LangId) => ["htdp", "deinprogramm"].includes(level(id).family);

export type Settings = {
  caseSensitive: boolean;
  printingStyle: PrintingStyle;
  fractionStyle: FractionStyle;
  showSharing: boolean;
  insertNewlines: boolean;
  tracing: boolean;
  /** Teachpack labels, in the order added. */
  teachpacks: string[];
  /** HtDP only. */
  trueFalseEmpty: boolean;
};

/** `Language::default_settings`. */
export function defaultSettings(id: LangId): Settings {
  if (level(id).family === "deinprogramm")
    return { caseSensitive: false, printingStyle: "write", fractionStyle: "repeating-decimal", showSharing: false, insertNewlines: true, tracing: false, teachpacks: [], trueFalseEmpty: false };
  return {
    caseSensitive: true,
    printingStyle: "constructor",
    fractionStyle: "repeating-decimal",
    // Only Advanced Student shows sharing by default.
    showSharing: id === "advanced",
    insertNewlines: true,
    tracing: false,
    teachpacks: [],
    trueFalseEmpty: false,
  };
}

/** `Language::printing_styles`. */
export function printingStyles(id: LangId): PrintingStyle[] {
  if (level(id).family === "deinprogramm") return ["constructor", "quasiquote", "write"];
  // Beginning Student has no quote, so no quasiquote output.
  if (id === "beginner") return ["constructor", "write", "print"];
  return ["constructor", "quasiquote", "write", "print"];
}

export const PRINTING_LABEL: Record<PrintingStyle, string> = { constructor: "Constructor", quasiquote: "Quasiquote", write: "write", print: "print" };
export const FRACTION_LABEL: Record<FractionStyle, string> = { "mixed-fraction": "Mixed fractions", "repeating-decimal": "Repeating decimals" };

/** `Language::stepper_level("")`: the stepper doesn't support the mutation-heavy levels. */
export function stepperLevel(id: LangId): string | null {
  const m: Partial<Record<LangId, string>> = {
    beginner: "beginner",
    "beginner-abbr": "beginner-abbr",
    intermediate: "intermediate",
    "intermediate-lambda": "intermediate-lambda",
    "sdp-beginner": "sdp-beginner",
    sdp: "sdp-vanilla",
  };
  return m[id] ?? null;
}

/** "Show sharing in values" is hidden for the two Beginner levels. */
export const showsSharingOption = (id: LangId) => id !== "beginner" && id !== "beginner-abbr";
/** The true/false/empty and tracing toggles are HtDP only. */
export const showsHtdpOptions = (id: LangId) => level(id).family === "htdp";

export const TEACHPACKS = [
  "2htdp/image",
  "2htdp/universe",
  "2htdp/batch-io",
  "2htdp/abstraction",
  "2htdp/itunes",
  "2htdp/planetcute",
  "2htdp/web-io",
  "htdp/dir",
  "htdp/draw",
  "htdp/guess",
  "htdp/hangman",
  "htdp/matrix",
];

export const DIALOG_TITLE = "Choose Language";
export const DIALOG_SUBTITLE = "Every DrRacket language level. Teaching-language files stay compatible with DrRacket.";
export const MODULE_TEXT = [
  "The first line of the file selects the language, e.g. #lang racket, #lang racket/base, #lang typed/racket, #lang htdp/bsl, #lang scribble/manual.",
  "Runs with errortrace debugging, runs the main and test submodules, and supports the debugger, profiler and coverage.",
];
export const LEGACY_TEXT = "Programs run at the top level, one form at a time, as in DrRacket's legacy languages.";
export const STEPPER_CARD = "Supports the algebraic stepper (⇧⌘E)";

/** The screenshot's state: Beginning Student, with true/false/empty printing turned on. */
export const REST: { id: LangId; settings: Settings } = {
  id: "beginner",
  settings: { ...defaultSettings("beginner"), trueFalseEmpty: true },
};
