// The CodeMirror 6 setup behind every demo editor. Loaded lazily (one shared chunk), only when
// a demo nears the viewport or is touched. It edits text and draws decorations; it never runs
// anything.
import { EditorState, RangeSetBuilder, StateEffect, StateField, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  GutterMarker,
  ViewPlugin,
  WidgetType,
  drawSelection,
  gutter,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, insertNewlineAndIndent } from "@codemirror/commands";
import { bracketMatching, indentService, indentUnit } from "@codemirror/language";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { IMAGE_CHAR } from "../racket-lexer";
import { styleSpans } from "../spans";
import { indentFor } from "../indent";
import { rocketTile } from "../rocket-tile";
import type { CodeDecor, EditorHandle, EditorOptions } from "./types";

/** Token colours, rainbow brackets and indent bands, recomputed from the whole (small) doc. */
const syntax = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view.state.doc.toString());
    }
    update(u: ViewUpdate) {
      if (u.docChanged) this.decorations = build(u.state.doc.toString());
    }
  },
  { decorations: (v) => v.decorations },
);

const markCache = new Map<string, Decoration>();
const mark = (cls: string) => {
  let m = markCache.get(cls);
  if (!m) markCache.set(cls, (m = Decoration.mark({ class: cls })));
  return m;
};

function build(text: string): DecorationSet {
  const b = new RangeSetBuilder<Decoration>();
  for (const s of styleSpans(text)) if (s.to > s.from) b.add(s.from, s.to, mark(s.cls));
  return b.finish();
}

/** A pasted image: one atomic character drawn as the rocket tile. */
class ImageWidget extends WidgetType {
  constructor(private cls: string) {
    super();
  }
  eq(o: ImageWidget) {
    return o.cls === this.cls;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = this.cls;
    span.setAttribute("role", "img");
    span.setAttribute("aria-label", "pasted image: rocket");
    span.innerHTML = rocketTile();
    return span;
  }
}

function images(cls: string): Extension {
  const deco = (text: string) => {
    const b = new RangeSetBuilder<Decoration>();
    for (let i = text.indexOf(IMAGE_CHAR); i >= 0; i = text.indexOf(IMAGE_CHAR, i + 1))
      b.add(i, i + 1, Decoration.replace({ widget: new ImageWidget(cls) }));
    return b.finish();
  };
  const field = StateField.define<DecorationSet>({
    create: (s) => deco(s.doc.toString()),
    update: (v, tr) => (tr.docChanged ? deco(tr.state.doc.toString()) : v),
    provide: (f) => [EditorView.decorations.from(f), EditorView.atomicRanges.of((view) => view.state.field(f))],
  });
  return field;
}

class AfterWidget extends WidgetType {
  constructor(
    private text: string,
    private cls: string,
    private check: boolean,
    private label?: string,
  ) {
    super();
  }
  eq(o: AfterWidget) {
    return o.text === this.text && o.cls === this.cls && o.check === this.check;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = this.cls;
    if (this.label) span.setAttribute("aria-label", this.label);
    span.textContent = (this.check ? "✓ " : "") + this.text;
    return span;
  }
  ignoreEvent() {
    return false;
  }
}

class BreakpointMarker extends GutterMarker {
  constructor(
    private cls: string,
    private label: string,
  ) {
    super();
  }
  eq(o: BreakpointMarker) {
    return o.cls === this.cls;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = this.cls;
    span.title = this.label;
    return span;
  }
}

const setDecor = StateEffect.define<CodeDecor>();

function decorField(): StateField<{ decor: CodeDecor; set: DecorationSet; gutter: Map<number, BreakpointMarker> }> {
  const make = (state: EditorState, decor: CodeDecor) => {
    const doc = state.doc;
    const items: { from: number; to: number; d: Decoration }[] = [];
    const lineOk = (l: number) => l >= 1 && l <= doc.lines;
    for (const l of decor.lines ?? []) if (lineOk(l.line)) items.push({ from: doc.line(l.line).from, to: doc.line(l.line).from, d: Decoration.line({ class: l.cls }) });
    for (const m of decor.marks ?? [])
      if (m.to > m.from && m.to <= doc.length) items.push({ from: m.from, to: m.to, d: Decoration.mark({ class: m.cls, attributes: m.title ? { title: m.title } : undefined }) });
    for (const a of decor.after ?? [])
      if (lineOk(a.line)) {
        const end = doc.line(a.line).to;
        items.push({ from: end, to: end, d: Decoration.widget({ widget: new AfterWidget(a.text, a.cls, !!a.check, a.label), side: 1 }) });
      }
    items.sort((a, b) => a.from - b.from || (a.d.startSide ?? 0) - (b.d.startSide ?? 0));
    const set = Decoration.set(
      items.map((i) => i.d.range(i.from, i.to)),
      true,
    );
    const g = new Map<number, BreakpointMarker>();
    for (const m of decor.gutter ?? []) if (lineOk(m.line)) g.set(m.line, new BreakpointMarker(m.cls, m.label));
    return { decor, set, gutter: g };
  };
  return StateField.define({
    create: (s) => make(s, {}),
    update(v, tr) {
      for (const e of tr.effects) if (e.is(setDecor)) return make(tr.state, e.value);
      return tr.docChanged ? { ...v, set: v.set.map(tr.changes) } : v;
    },
    provide: (f) => EditorView.decorations.from(f, (v) => v.set),
  });
}

export function mountEditor(parent: HTMLElement, o: EditorOptions): EditorHandle {
  const field = decorField();
  const exts: Extension[] = [
    o.breakpointGutter
      ? gutter({
          class: o.classes.bpGutter,
          lineMarker: (view, line) => view.state.field(field).gutter.get(view.state.doc.lineAt(line.from).number) ?? null,
          lineMarkerChange: (u) => u.transactions.some((t) => t.effects.some((e) => e.is(setDecor))),
          initialSpacer: () => new BreakpointMarker("", ""),
          domEventHandlers: {
            mousedown(view, line) {
              o.onGutterClick?.(view.state.doc.lineAt(line.from).number);
              return true;
            },
          },
        })
      : [],
    lineNumbers({
      formatNumber: (n) => String(n + o.firstLine - 1),
      domEventHandlers: o.onGutterClick
        ? {
            mousedown(view, line) {
              o.onGutterClick?.(view.state.doc.lineAt(line.from).number);
              return true;
            },
          }
        : {},
    }),
    highlightActiveLineGutter(),
    highlightActiveLine(),
    drawSelection({ cursorBlinkRate: 1100 }),
    syntax,
    images(o.classes.image),
    field,
    EditorView.contentAttributes.of({ "aria-label": o.label }),
    EditorView.editorAttributes.of({ class: o.classes.cm }),
    EditorState.readOnly.of(o.readOnly),
    EditorView.editable.of(!o.readOnly),
    indentUnit.of("  "),
    EditorState.tabSize.of(8),
    EditorView.updateListener.of((u) => {
      if (u.docChanged) o.onChange(u.state.doc.toString());
      if (u.docChanged || u.selectionSet) {
        const head = u.state.selection.main.head;
        const line = u.state.doc.lineAt(head);
        o.onCursor(head, line.number + o.firstLine - 1, head - line.from + 1);
      }
    }),
  ];
  if (!o.readOnly) {
    exts.push(
      history(),
      closeBrackets(),
      bracketMatching(),
      indentService.of((ctx, pos) => indentFor(ctx.state.doc.toString(), pos)),
      keymap.of([
        { key: "Escape", run: () => (o.onEscape(), true) },
        { key: "Enter", run: insertNewlineAndIndent },
        ...closeBracketsKeymap,
        // Tab and Shift-Tab are left to the browser so focus can always leave the editor.
        ...defaultKeymap.filter((k) => k.key !== "Tab" && k.key !== "Shift-Tab" && k.key !== "Mod-[" && k.key !== "Mod-]"),
        ...historyKeymap,
      ]),
    );
  } else {
    exts.push(keymap.of([{ key: "Escape", run: () => (o.onEscape(), true) }]));
  }

  const view = new EditorView({ parent, state: EditorState.create({ doc: o.doc, extensions: exts }) });
  view.dispatch({ effects: setDecor.of(o.decor) });
  if (o.topLine > 1) {
    const pos = view.state.doc.line(Math.min(o.topLine, view.state.doc.lines)).from;
    view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: "start" }) });
  }

  return {
    setDoc(text, userEvent) {
      const cur = view.state.doc.toString();
      if (cur === text) return;
      view.dispatch({ changes: { from: 0, to: cur.length, insert: text }, userEvent });
    },
    setDecor: (d) => view.dispatch({ effects: setDecor.of(d) }),
    focus: () => view.focus(),
    destroy: () => view.destroy(),
    getDoc: () => view.state.doc.toString(),
    coordsAt: (pos) => view.coordsAtPos(Math.min(pos, view.state.doc.length)),
    posAt: (x, y) => view.posAtCoords({ x, y }),
    scrollTop: () => view.scrollDOM.scrollTop,
    scroller: view.scrollDOM,
    dom: view.dom,
  };
}
