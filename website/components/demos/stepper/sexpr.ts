// A tiny reader and pretty-printer for the stepper's one-line expressions. Display only.
export type Node = { from: number; to: number; kids?: Node[] };

/** Reads one s-expression; atoms are runs of non-space, non-paren characters (and `'()`). */
export function read(src: string): Node {
  let i = 0;
  const ws = () => {
    while (src[i] === " ") i++;
  };
  const node = (): Node => {
    ws();
    const from = i;
    if (src.startsWith("'()", i)) return (i += 3), { from, to: i };
    if (src[i] === "(") {
      i++;
      const kids: Node[] = [];
      ws();
      while (i < src.length && src[i] !== ")") {
        kids.push(node());
        ws();
      }
      if (src[i] !== ")") throw new Error(`unclosed at ${from} in ${src}`);
      i++;
      return { from, to: i, kids };
    }
    if (src[i] === ")" || i >= src.length) throw new Error(`unexpected at ${i} in ${src}`);
    while (i < src.length && !" ()".includes(src[i])) i++;
    return { from, to: i };
  };
  const n = node();
  ws();
  if (i !== src.length) throw new Error(`trailing text in ${src}`);
  return n;
}

/** Every node, depth first. */
export function nodes(n: Node, out: Node[] = []): Node[] {
  out.push(n);
  n.kids?.forEach((k) => nodes(k, out));
  return out;
}

/** True when [from, to) is exactly one subexpression of src. */
export function isSubexpr(src: string, from: number, to: number): boolean {
  return nodes(read(src)).some((n) => n.from === from && n.to === to);
}

/**
 * Lays an expression out like the stepper: a form that fits in `width` stays on one line,
 * otherwise its head stays and each argument goes on its own line, one space deeper.
 * Returns the text and where the [from, to) range of the source landed.
 */
export function pretty(src: string, width: number, hl: [number, number]): { text: string; hl: [number, number] } {
  let out = "";
  let hlOut: [number, number] = [0, 0];
  const emit = (n: Node, col: number) => {
    const flat = src.slice(n.from, n.to);
    const start = out.length;
    if (!n.kids || n.kids.length < 2 || col + flat.length <= width) out += flat;
    else {
      out += "(";
      emit(n.kids[0], col + 1);
      for (const k of n.kids.slice(1)) {
        out += "\n" + " ".repeat(col + 1);
        emit(k, col + 1);
      }
      out += ")";
    }
    if (n.from === hl[0] && n.to === hl[1]) hlOut = [start, out.length];
  };
  emit(read(src), 0);
  return { text: out, hl: hlOut };
}
