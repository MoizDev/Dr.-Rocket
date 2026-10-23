// Works out check-syntax bindings for the demo's ISL file, by significant-token index. Used to
// generate (and re-check) bindings-grades.json; it handles only the forms that file uses.
import { sigTokens } from "../fixtures";

export type Bindings = {
  module: string;
  /** Binder token index → what it binds and every bound occurrence. */
  binders: Record<string, { name: string; kind: "parameter" | "definition"; uses: number[] }>;
  /** Token index of a free identifier → its name (imported from `module`). */
  imported: Record<string, string>;
};

type Tree = { i: number; kids?: Tree[] };

export function resolve(text: string, module = "lang/htdp-intermediate"): Bindings {
  const toks = sigTokens(text);
  let i = 0;
  const read = (): Tree => {
    const t = toks[i];
    if (t.kind === "open") {
      const node: Tree = { i, kids: [] };
      i++;
      while (i < toks.length && toks[i].kind !== "close") node.kids!.push(read());
      i++;
      return node;
    }
    i++;
    return { i: i - 1 };
  };
  const forms: Tree[] = [];
  while (i < toks.length) forms.push(read());

  const out: Bindings = { module, binders: {}, imported: {} };
  const name = (t: Tree) => toks[t.i].text;
  const isSym = (t: Tree) => !t.kids && toks[t.i].kind === "symbol";
  const top = new Map<string, number>();
  const bind = (t: Tree, kind: "parameter" | "definition") => (out.binders[t.i] = { name: name(t), kind, uses: [] });

  for (const f of forms)
    if (f.kids && name(f.kids[0]) === "define") {
      const head = f.kids[1];
      const b = head.kids ? head.kids[0] : head;
      bind(b, "definition");
      top.set(name(b), b.i);
    }

  const walk = (t: Tree, scope: Map<string, number>) => {
    if (t.kids) return t.kids.forEach((k) => walk(k, scope));
    if (!isSym(t)) return;
    const b = scope.get(name(t)) ?? top.get(name(t));
    if (b !== undefined) out.binders[b].uses.push(t.i);
    else out.imported[t.i] = name(t);
  };
  for (const f of forms) {
    if (f.kids && name(f.kids[0]) === "define") {
      walk(f.kids[0], new Map());
      const head = f.kids[1];
      const scope = new Map<string, number>();
      if (head.kids)
        for (const p of head.kids.slice(1)) {
          bind(p, "parameter");
          scope.set(name(p), p.i);
        }
      f.kids.slice(2).forEach((k) => walk(k, scope));
    } else walk(f, new Map());
  }
  return out;
}
