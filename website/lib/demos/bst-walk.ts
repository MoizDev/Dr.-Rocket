// A hand-written walker for the one fixed program in bst.rkt (insert, tree-sum, foldl over
// (list 5 3 8 1 4 9)). It is not an interpreter: it knows these three definitions only, and it
// runs at development time (`npm run gen:demos`) to produce the Visualizer trace and the
// Debugger pauses, which are committed as JSON. The page never runs it.
// No imports, so `node scripts/gen-demo-fixtures.mts` can load it directly.

export type Seg = { k: "t" | "fn" | "var" | "code" | "val"; s: string };
export type Hl = { from: number; to: number; kind: "cur" | "done" | "ret"; note?: string };
export type ThenSeg = { code: string } | { val: string; ptr?: number } | { hole: true };
export type VFrame = {
  fn: string | null; // null: Top level
  args: string[];
  status: "running" | "waiting" | "returning" | "done";
  vars: { name: string; value: string; ptr?: number }[];
  then?: ThenSeg[];
  question?: { code: string; answer: "Yes" | "No" };
  returns?: { value: string; ptr?: number };
};
export type VStep = {
  kind: "START" | "CALL" | "QUESTION" | "RETURN" | "DEFINE" | "DONE";
  chip: string;
  hl: Hl[];
  marker?: number; // line of the current expression
  lines: Seg[][];
  work: string[];
  frames: VFrame[];
  nodes: number[]; // keys of nodes that exist so far
  depth: number;
  rest?: true;
  output?: string;
};
export type DPause = {
  kind: "enter" | "call" | "return";
  from: number;
  to: number;
  line: number;
  depth: number;
  expr: string;
  frames: { label: string; vars: { name: string; value: string }[] }[];
  globals: { name: string; value: string }[];
  rest?: true;
};

type Node = { key: number; left: Node | null; right: Node | null } | null;

export function walk(src: string): { trace: VStep[]; pauses: DPause[]; result: number } {
  const lines = src.split("\n");
  const ls = (n: number) => lines.slice(0, n - 1).reduce((a, l) => a + l.length + 1, 0);
  const find = (line: number, needle: string) => {
    const i = lines[line - 1].indexOf(needle);
    if (i < 0) throw new Error(`"${needle}" not on line ${line}`);
    return { from: ls(line) + i, to: ls(line) + i + needle.length };
  };
  const form = (line: number, needle: string) => {
    const { from } = find(line, needle);
    let d = 0;
    for (let i = from; i < src.length; i++) {
      const c = src[i];
      if (c === "(" || c === "[") d++;
      else if (c === ")" || c === "]") if (--d === 0) return { from, to: i + 1 };
    }
    throw new Error("unbalanced");
  };
  const lineOf = (pos: number) => src.slice(0, pos).split("\n").length;
  const flat = (r: { from: number; to: number }) => src.slice(r.from, r.to).replace(/\s+/g, " ");

  const R = {
    foldl: form(25, "(foldl"),
    defTree: form(25, "(define tree"),
    tsTop: find(26, "(tree-sum tree)"),
    insEmptyQ: find(8, "(empty? t)"),
    mkLeaf: find(8, "(make-node k empty empty)"),
    ltQ: find(9, "(< k (node-key t))"),
    insL: find(11, "(insert k (node-left t))"),
    insR: find(16, "(insert k (node-right t))"),
    mkL: form(10, "(make-node"),
    mkR: form(14, "(make-node"),
    tsCond: form(20, "(cond"),
    tsEmptyQ: find(20, "(empty? t)"),
    zero: { from: find(20, "0]").from, to: find(20, "0]").from + 1 },
    plus: form(21, "(+"),
    nodeKey: find(21, "(node-key t)"),
    tsL: find(22, "(tree-sum (node-left t))"),
    tsR: find(23, "(tree-sum (node-right t))"),
  };

  const show = (t: Node) => (t ? `node ${t.key}` : "empty");
  const ptr = (t: Node) => (t ? t.key : undefined);
  const dval = (t: Node): string => (t ? `(make-node ${t.key} ${dval(t.left)} ${dval(t.right)})` : "'()");
  const T = (s: string): Seg => ({ k: "t", s });
  const FN = (s: string): Seg => ({ k: "fn", s });
  const V = (s: string): Seg => ({ k: "var", s });
  const C = (s: string): Seg => ({ k: "code", s });
  const thenText = (th?: ThenSeg[]) =>
    th ? th.map((x) => ("code" in x ? x.code : "val" in x ? x.val : "•••")).join(" ").replace(/\( /g, "(").replace(/ \)/g, ")") : "";

  const trace: VStep[] = [];
  const pauses: DPause[] = [];
  const nodes = new Set<number>();
  const top: VFrame = { fn: null, args: [], status: "running", vars: [] };
  const stack: VFrame[] = [top];
  const snap = (): VFrame[] => JSON.parse(JSON.stringify(stack));
  const step = (s: Omit<VStep, "frames" | "nodes" | "depth" | "work"> & { work?: string[] }) =>
    trace.push({ work: [], ...s, frames: snap(), nodes: [...nodes].sort((a, b) => a - b), depth: stack.length - 1 });

  // Debugger state (tree-sum phase only; the breakpoint is on line 20).
  const dstack: { label: string; t: Node; site: string }[] = [];
  let globalsTree: Node = null;
  const pause = (kind: DPause["kind"], r: { from: number; to: number }) => {
    const frames = dstack
      .slice()
      .reverse()
      .map((f, i) => ({ label: i === 0 ? flat(r) : dstack[dstack.length - i].site, vars: [{ name: "t", value: dval(f.t) }] }));
    pauses.push({
      kind,
      from: r.from,
      to: r.to,
      line: lineOf(r.from),
      depth: dstack.length,
      expr: flat(r),
      frames,
      globals: [
        { name: "insert", value: "insert" },
        { name: "tree-sum", value: "tree-sum" },
        { name: "tree", value: dval(globalsTree) },
      ],
    });
  };

  // ---- (define tree (foldl insert empty (list 5 3 8 1 4 9)))
  step({
    kind: "START",
    chip: flat(R.foldl),
    hl: [{ ...R.foldl, kind: "cur" }],
    marker: 25,
    lines: [[T("The program starts")], [T("To define "), V("tree"), T(", it evaluates "), C(flat(R.foldl))]],
  });

  const insert = (k: number, t: Node, site: { from: number; to: number }, from: "foldl" | "left" | "right", first: boolean, last: boolean): Node => {
    const parent = stack[stack.length - 1];
    parent.status = "waiting";
    const f: VFrame = { fn: "insert", args: [String(k), show(t)], status: "running", vars: [{ name: "k", value: String(k) }, { name: "t", value: show(t), ptr: ptr(t) }] };
    stack.push(f);
    const back = (val: string): Seg[] =>
      parent.fn === null
        ? [T("Back to "), FN("foldl"), T(last ? ", which has used every number" : ", which goes on with the next number")]
        : [T("Back to "), FN("insert"), T(", which was waiting for this result to finish "), C(thenText(parent.then))].concat(val ? [] : []);
    if (from === "foldl")
      step({
        kind: "CALL",
        chip: flat(R.foldl),
        hl: [{ ...R.foldl, kind: "cur", note: `k = ${k}` }],
        marker: 25,
        lines: first
          ? [[T("The program calls "), FN("insert")], [V("k"), T(" is "), C(String(k))], [V("t"), T(" is "), C(show(t))]]
          : [[FN("foldl"), T(" calls "), FN("insert"), T(" with the next number")], [V("k"), T(" is "), C(String(k))], [V("t"), T(" is "), C(show(t)), T(", the tree so far")]],
        work: [],
      });
    else
      step({
        kind: "CALL",
        chip: flat(site),
        hl: [{ ...site, kind: "cur", note: `t = ${show(t)}` }],
        marker: lineOf(site.from),
        lines: [
          [FN("insert"), T(" calls itself")],
          [V("k"), T(" is "), C(String(k))],
          [V("t"), T(" is "), C(show(t)), T(`, which was `), V("t"), T(`'s ${from} in the previous call`)],
          [T("When it returns, "), FN("insert"), T(" goes on with "), C(thenText(parent.then))],
        ],
        work: [`(node-${from} t) → ${show(t)}`],
      });
    const isEmpty = t === null;
    f.question = { code: "(empty? t)", answer: isEmpty ? "Yes" : "No" };
    step({
      kind: "QUESTION",
      chip: "(empty? t)",
      hl: [{ ...R.insEmptyQ, kind: "cur", note: `t is ${show(t)}` }],
      marker: 8,
      lines: [
        [T("Is "), V("t"), T(" empty?  "), T(isEmpty ? "Yes" : "No")],
        [T("Here "), V("t"), T(" is "), C(show(t))],
        [T(isEmpty ? "So cond uses this clause's answer" : "So cond moves on to the next clause")],
      ],
    });
    let res: Node;
    let ret: { from: number; to: number };
    let built: Seg[];
    if (!t) {
      res = { key: k, left: null, right: null };
      nodes.add(k);
      ret = R.mkLeaf;
      built = [T("It builds a new node with key "), C(String(k)), T(", left "), C("empty"), T(" and right "), C("empty")];
    } else {
      const lt = k < t.key;
      f.question = { code: "(< k (node-key t))", answer: lt ? "Yes" : "No" };
      step({
        kind: "QUESTION",
        chip: "(< k (node-key t))",
        hl: [{ ...R.ltQ, kind: "cur", note: `k is ${k} and t's key is ${t.key} → ${lt ? "Yes" : "No"}` }],
        marker: 9,
        lines: [
          [T("Is "), V("k"), T(" less than "), V("t"), T("'s key?  "), T(lt ? "Yes" : "No")],
          [T("Here "), V("k"), T(" is "), C(String(k)), T(" and "), V("t"), T("'s key is "), C(String(t.key))],
          [T(lt ? "So cond uses this clause's answer" : "So cond moves on to the next clause")],
        ],
      });
      delete f.question;
      if (lt) {
        f.then = [{ code: "(make-node" }, { val: String(t.key) }, { hole: true }, { code: "(node-right t))" }];
        const l = insert(k, t.left, R.insL, "left", false, false);
        res = { key: t.key, left: l, right: t.right };
        ret = R.mkL;
      } else {
        f.then = [{ code: "(make-node" }, { val: String(t.key) }, { val: show(t.left), ptr: ptr(t.left) }, { hole: true }, { code: ")" }];
        const r = insert(k, t.right, R.insR, "right", false, false);
        res = { key: t.key, left: t.left, right: r };
        ret = R.mkR;
      }
      delete f.then;
      f.status = "running";
      built = [T("It builds a new node with key "), C(String(t.key)), T(", left "), C(show(res!.left)), T(" and right "), C(show(res!.right))];
    }
    delete f.question;
    f.status = "returning";
    f.returns = { value: show(res), ptr: ptr(res) };
    step({
      kind: "RETURN",
      chip: from === "foldl" ? flat(R.foldl) : flat(site),
      hl: [{ ...ret, kind: "ret", note: `→ returns ${show(res)}` }],
      marker: lineOf(ret.from),
      lines: [[FN("insert"), T(" returns "), C(show(res))], built, back(show(res))],
    });
    stack.pop();
    parent.status = "running";
    return res;
  };

  let tree: Node = null;
  const keys = [5, 3, 8, 1, 4, 9];
  keys.forEach((k, i) => (tree = insert(k, tree, R.foldl, "foldl", i === 0, i === keys.length - 1)));
  globalsTree = tree;
  top.vars = [{ name: "tree", value: show(tree), ptr: ptr(tree) }];
  step({
    kind: "DEFINE",
    chip: "(define tree …)",
    hl: [{ ...R.defTree, kind: "done", note: `tree = ${show(tree)}` }],
    marker: 25,
    lines: [[T("The program defines "), V("tree")], [V("tree"), T(" is "), C(show(tree))]],
  });

  // ---- (tree-sum tree)
  const treeSum = (t: Node, site: { from: number; to: number }, from: "top" | "left" | "right", done: Hl[], work: string[]): number => {
    const parent = stack[stack.length - 1];
    parent.status = "waiting";
    const f: VFrame = { fn: "tree-sum", args: [show(t)], status: "running", vars: [{ name: "t", value: show(t), ptr: ptr(t) }] };
    stack.push(f);
    const dsite = from === "top" ? "(tree-sum tree)" : flat(site);
    step({
      kind: "CALL",
      chip: flat(site),
      hl: [...done, { ...site, kind: "cur", note: `t = ${show(t)}` }],
      marker: lineOf(site.from),
      lines:
        from === "top"
          ? [[T("The program calls "), FN("tree-sum")], [V("t"), T(" is "), C(show(t)), T(", which is "), V("tree")]]
          : [
              [FN("tree-sum"), T(" calls itself")],
              [V("t"), T(" is "), C(show(t)), T(", which was "), V("t"), T(`'s ${from} in the previous call`)],
              [T("When it returns, "), FN("tree-sum"), T(" goes on with "), C(thenText(parent.then))],
            ],
      work,
    });
    dstack.push({ label: "", t, site: dsite });
    pause("enter", R.tsCond);
    const isEmpty = t === null;
    f.question = { code: "(empty? t)", answer: isEmpty ? "Yes" : "No" };
    step({
      kind: "QUESTION",
      chip: "(empty? t)",
      hl: [{ ...R.tsEmptyQ, kind: "cur", note: `t is ${show(t)}` }],
      marker: 20,
      lines: [
        [T("Is "), V("t"), T(" empty?  "), T(isEmpty ? "Yes" : "No")],
        [T("Here "), V("t"), T(" is "), C(show(t))],
        [T(isEmpty ? "So cond uses this clause's answer" : "So cond moves on to the next clause")],
      ],
    });
    delete f.question;
    let v: number;
    let ret: { from: number; to: number };
    let rwork: string[] = [];
    if (!t) {
      v = 0;
      ret = R.zero;
    } else {
      const keyDone: Hl = { ...R.nodeKey, kind: "done", note: `(node-key t) → ${t.key}` };
      f.then = [{ code: "(+" }, { val: String(t.key) }, { hole: true }, { code: "(tree-sum (node-right t)))" }];
      pause("call", R.tsL);
      const a = treeSum(t.left, R.tsL, "left", [keyDone], [`(node-key t) → ${t.key}`, `(node-left t) → ${show(t.left)}`]);
      f.then = [{ code: "(+" }, { val: String(t.key) }, { val: String(a) }, { hole: true }, { code: ")" }];
      pause("call", R.tsR);
      const b = treeSum(t.right, R.tsR, "right", [keyDone, { ...R.tsL, kind: "done", note: `→ ${a}` }], [`(node-right t) → ${show(t.right)}`]);
      delete f.then;
      v = t.key + a + b;
      ret = R.plus;
      rwork = [`(+ ${t.key} ${a} ${b}) → ${v}`];
    }
    f.status = "returning";
    f.returns = { value: String(v) };
    pause("return", ret);
    const back: Seg[] =
      parent.fn === null
        ? [T("Back to the top level, which shows the result")]
        : [T("Back to "), FN("tree-sum"), T(", which was waiting for this result to finish "), C(thenText(parent.then))];
    step({
      kind: "RETURN",
      chip: dsite,
      hl: [{ ...ret, kind: "ret", note: `→ returns ${v}` }],
      marker: lineOf(ret.to),
      lines: [[FN("tree-sum"), T(" returns "), C(String(v))], back],
      work: rwork,
    });
    dstack.pop();
    stack.pop();
    parent.status = "running";
    return v;
  };

  const result = treeSum(tree, R.tsTop, "top", [], []);
  top.status = "done";
  top.vars = [...top.vars, { name: "result", value: String(result) }];
  step({
    kind: "DONE",
    chip: "",
    hl: [],
    lines: [[T("The program finished")], [T("Its result is "), C(String(result))]],
    output: String(result),
  });

  // Rest states: the video's poster (tree-sum calls itself on node 4's left) and the
  // screenshot's pause (the 4th stop on line 20, with t = '()).
  const rest = trace.findIndex((s) => s.kind === "CALL" && s.frames.at(-2)?.args[0] === "node 4" && s.hl.some((h) => h.kind === "cur" && h.from === R.tsL.from));
  trace[rest].rest = true;
  const enters = pauses.filter((p) => p.kind === "enter");
  enters[3].rest = true;
  return { trace, pauses, result };
}
