// Decorations a demo puts on its code, in current-document positions. The static markup and
// the CodeMirror view render the same set.
export type CodeDecor = {
  /** Classes for whole lines (1-based). */
  lines?: { line: number; cls: string }[];
  /** Classes for ranges. */
  marks?: { from: number; to: number; cls: string; title?: string }[];
  /** A small element after a line's text: test badges, inline annotations. */
  after?: { line: number; text: string; cls: string; check?: boolean; label?: string }[];
  /** Markers in the breakpoint gutter. */
  gutter?: { line: number; cls: string; label: string }[];
};

export type EditorOptions = {
  doc: string;
  label: string;
  readOnly: boolean;
  firstLine: number;
  topLine: number;
  decor: CodeDecor;
  /** Class names from Code.module.css that CodeMirror's own nodes need. */
  classes: Record<string, string>;
  breakpointGutter: boolean;
  onChange: (text: string) => void;
  onCursor: (pos: number, line: number, col: number) => void;
  onGutterClick?: (line: number) => void;
  onEscape: () => void;
};

export type EditorHandle = {
  setDoc: (text: string, userEvent: string) => void;
  setDecor: (d: CodeDecor) => void;
  focus: () => void;
  destroy: () => void;
  getDoc: () => string;
  /** Viewport-relative rectangle of a document position, or null if off-screen. */
  coordsAt: (pos: number) => { left: number; top: number; bottom: number } | null;
  posAt: (x: number, y: number) => number | null;
  scrollTop: () => number;
  scroller: HTMLElement;
  dom: HTMLElement;
};
