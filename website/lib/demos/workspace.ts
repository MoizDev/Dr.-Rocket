// One in-memory store of demo files per page load. A file shown in several demos is one entry,
// so an edit shows everywhere it appears. Nothing is persisted: a reload restores everything.
import * as F from "./data/files";

export const ORIGINALS: Record<string, string> = {
  "cs135/a03/grades.rkt": F.GRADES,
  "cs135/a03/squares.rkt": F.SQUARES,
  "cs135/a03/poster.rkt": F.POSTER,
  "cs135/a05/bst.rkt": F.BST,
  "cs135/a05/bst-debug.rkt": F.BST,
  "cs135/notes/scratch.rkt": F.SCRATCH,
  "cs135/README.md": F.README,
  "cs135/a05/README.md": F.A05_STUB,
  "cs136/a02/queue.rkt": F.QUEUE,
  "cs136/README.md": "# CS136\n\nQueues, stacks and friends, in #lang racket.\n",
  "math239/graphs/degree.rkt": F.DEGREE,
  "math239/README.md": "# MATH239\n\nGraph theory notes, checked with Racket.\n",
  "themes/grades.rkt": F.GRADES,
  "checksyntax/grades.rkt": F.GRADES,
};

type Listener = () => void;
const texts = new Map<string, string>();
const listeners = new Map<string, Set<Listener>>();

export function getText(id: string): string {
  return texts.get(id) ?? ORIGINALS[id] ?? "";
}

export function original(id: string): string {
  return ORIGINALS[id] ?? "";
}

export function setText(id: string, text: string) {
  if (getText(id) === text) return;
  texts.set(id, text);
  listeners.get(id)?.forEach((l) => l());
}

export function subscribe(id: string, l: Listener): () => void {
  let set = listeners.get(id);
  if (!set) listeners.set(id, (set = new Set()));
  set.add(l);
  return () => set.delete(l);
}

/** Files under a folder prefix, for trees, `ls` and quick open. */
export function filesUnder(prefix: string): string[] {
  return Object.keys(ORIGINALS).filter((k) => k.startsWith(prefix));
}

/** Test hook: forget every edit. */
export function resetWorkspace() {
  const ids = [...texts.keys()];
  texts.clear();
  ids.forEach((id) => listeners.get(id)?.forEach((l) => l()));
}
