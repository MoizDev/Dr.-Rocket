"use client";
import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import DemoRoot from "../DemoRoot";
import GradesIde from "../hero-ide/GradesIde";
import { IdeIcon } from "../IdeIcons";
import t from "./Themes.module.css";

export const LABEL = "The same program in the One Light theme.";

type Base = "dark" | "light";
type Theme = { name: string; base: Base; vars?: Record<string, string> };

/** Colour overrides on top of the base palette (lib/demos/theme.css), as `--d-*` tokens. */
const v = (o: Record<string, string>) => Object.fromEntries(Object.entries(o).map(([k, x]) => [`--d-${k}`, x]));

/** The built-in themes the page names. The two bases are the app's own; the rest are close copies. */
export const THEMES: Theme[] = [
  { name: "Moiz One Dark", base: "dark" },
  { name: "One Light", base: "light" },
  {
    name: "GitHub Light",
    base: "light",
    vars: v({
      window: "#f6f8fa", panel: "#f6f8fa", editor: "#ffffff", elevated: "#ffffff", "active-line": "#f6f8fa", "line-no": "#8c959f", "line-no-active": "#1f2328",
      text: "#1f2328", muted: "#59636e", fg: "#1f2328", border: "#d1d9e0", accent: "#0969da", selection: "#e6ebf0",
      success: "#1a7f37", "success-bg": "#dafbe1", "success-border": "#aceebb", error: "#cf222e", warn: "#9a6700",
      "c-keyword": "#cf222e", "c-function": "#8250df", "c-variable": "#953800", "c-number": "#0550ae", "c-string": "#0a3069", "c-comment": "#59636e",
      "c-operator": "#0550ae", "c-punct": "#1f2328", "c-type": "#953800", "c-hint": "#8c959f", r1: "#9a6700", r2: "#8250df", r3: "#0969da",
      "ansi-blue": "#0969da", "ansi-magenta": "#8250df", hl: "rgb(9 105 218 / 0.12)", "hl-strong": "rgb(9 105 218 / 0.22)",
    }),
  },
  {
    name: "Catppuccin Mocha",
    base: "dark",
    vars: v({
      window: "#11111b", panel: "#181825", editor: "#1e1e2e", elevated: "#313244", "active-line": "#26273a", "line-no": "#6c7086", "line-no-active": "#b4befe",
      text: "#cdd6f4", muted: "#a6adc8", fg: "#cdd6f4", border: "#313244", accent: "#89b4fa", selection: "#45475a",
      success: "#a6e3a1", "success-bg": "#24332a", "success-border": "#3b5240", error: "#f38ba8", warn: "#f9e2af",
      "c-keyword": "#cba6f7", "c-function": "#89b4fa", "c-variable": "#eba0ac", "c-number": "#fab387", "c-string": "#a6e3a1", "c-comment": "#9399b2",
      "c-operator": "#89dceb", "c-punct": "#9399b2", "c-type": "#f9e2af", "c-hint": "#7f849c", r1: "#f9e2af", r2: "#cba6f7", r3: "#89b4fa",
      "ansi-blue": "#89b4fa", "ansi-magenta": "#f5c2e7", hl: "rgb(137 180 250 / 0.16)", "hl-strong": "rgb(137 180 250 / 0.28)",
    }),
  },
  {
    name: "Gruvbox Dark",
    base: "dark",
    vars: v({
      window: "#1d2021", panel: "#282828", editor: "#282828", elevated: "#3c3836", "active-line": "#32302f", "line-no": "#7c6f64", "line-no-active": "#d5c4a1",
      text: "#ebdbb2", muted: "#bdae93", fg: "#ebdbb2", border: "#3c3836", accent: "#83a598", selection: "#504945",
      success: "#b8bb26", "success-bg": "#32361a", "success-border": "#4f5424", error: "#fb4934", warn: "#fabd2f",
      "c-keyword": "#fb4934", "c-function": "#8ec07c", "c-variable": "#83a598", "c-number": "#d3869b", "c-string": "#b8bb26", "c-comment": "#928374",
      "c-operator": "#fe8019", "c-punct": "#a89984", "c-type": "#fabd2f", "c-hint": "#7c6f64", r1: "#fabd2f", r2: "#d3869b", r3: "#83a598",
      "ansi-blue": "#83a598", "ansi-magenta": "#d3869b", hl: "rgb(131 165 152 / 0.16)", "hl-strong": "rgb(131 165 152 / 0.28)",
    }),
  },
  {
    name: "Tokyo Night",
    base: "dark",
    vars: v({
      window: "#16161e", panel: "#1a1b26", editor: "#1a1b26", elevated: "#24283b", "active-line": "#1e2030", "line-no": "#3b4261", "line-no-active": "#737aa2",
      text: "#c0caf5", muted: "#a9b1d6", fg: "#c0caf5", border: "#24283b", accent: "#7aa2f7", selection: "#283457",
      success: "#9ece6a", "success-bg": "#1f2a1f", "success-border": "#3a4d2c", error: "#f7768e", warn: "#e0af68",
      "c-keyword": "#bb9af7", "c-function": "#7aa2f7", "c-variable": "#f7768e", "c-number": "#ff9e64", "c-string": "#9ece6a", "c-comment": "#565f89",
      "c-operator": "#89ddff", "c-punct": "#a9b1d6", "c-type": "#2ac3de", "c-hint": "#565f89", r1: "#e0af68", r2: "#bb9af7", r3: "#7aa2f7",
      "ansi-blue": "#7aa2f7", "ansi-magenta": "#bb9af7", hl: "rgb(122 162 247 / 0.16)", "hl-strong": "rgb(122 162 247 / 0.28)",
    }),
  },
  {
    name: "DrRacket",
    base: "light",
    vars: v({
      window: "#ececec", panel: "#ececec", editor: "#ffffff", elevated: "#ffffff", "active-line": "#f4f4f4", "line-no": "#a0a0a0", "line-no-active": "#333333",
      text: "#1a1a1a", muted: "#5c5c5c", fg: "#1a1a1a", border: "#d0d0d0", accent: "#3b6fd4", selection: "#e0e0e0",
      success: "#228b22", "success-bg": "#e6f2e6", "success-border": "#bfd9bf", error: "#cc0000", warn: "#b07000",
      "c-keyword": "#262680", "c-function": "#262680", "c-variable": "#262680", "c-number": "#228b22", "c-string": "#228b22", "c-comment": "#c2741f",
      "c-operator": "#262680", "c-punct": "#845a1e", "c-type": "#262680", "c-hint": "#a0a0a0", r1: "#845a1e", r2: "#845a1e", r3: "#845a1e",
      "ansi-blue": "#262680", "ansi-magenta": "#8b008b", hl: "rgb(59 111 212 / 0.12)", "hl-strong": "rgb(59 111 212 / 0.22)",
    }),
  },
];

/**
 * theme-light is the editable front card; theme-dark (alt="" before) is an inert, read-only
 * mirror of the same file. Both use the theme section's own copy of grades.rkt. The front card
 * has the theme selector: moving through it previews each theme live, Esc goes back.
 */
export default function ThemeDemo({ className, theme }: { className: string; theme: "dark" | "light" }) {
  const root = useRef<HTMLDivElement>(null);
  const dark = theme === "dark";
  const [name, setName] = useState(dark ? "Moiz One Dark" : "One Light");
  const cur = THEMES.find((x) => x.name === name)!;
  return (
    <DemoRoot
      id={`theme-${theme}`}
      className={className}
      w={2400}
      h={1506}
      label={LABEL}
      theme={theme}
      decorative={dark}
      rootProps={dark ? undefined : { ref: root, tabIndex: -1 }}
    >
      <div className={t.skin} data-theme={cur.base} style={cur.vars as CSSProperties}>
        <GradesIde gradesId="themes/grades.rkt" readOnlyMirror={dark} rootRef={root} />
        {!dark && <ThemePicker value={name} onChange={setName} />}
      </div>
    </DemoRoot>
  );
}

/** The title bar's theme selector: a button and a list that applies each theme as you move. */
function ThemePicker({ value, onChange }: { value: string; onChange: (name: string) => void }) {
  const [open, setOpen] = useState(false);
  const [sel, setSel] = useState(0);
  const before = useRef(value);
  const closing = useRef(false);
  const btn = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLUListElement>(null);

  const show = () => {
    before.current = value;
    closing.current = false;
    setSel(THEMES.findIndex((x) => x.name === value));
    setOpen(true);
  };
  const close = (keep: boolean) => {
    closing.current = true;
    if (!keep) onChange(before.current);
    setOpen(false);
    requestAnimationFrame(() => btn.current?.focus());
  };
  const move = (i: number) => {
    setSel(i);
    onChange(THEMES[i].name);
  };

  useEffect(() => {
    if (open) list.current?.focus();
  }, [open]);

  const onKey = (e: KeyboardEvent) => {
    const n = THEMES.length;
    if (e.key === "ArrowDown") move((sel + 1) % n);
    else if (e.key === "ArrowUp") move((sel - 1 + n) % n);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(n - 1);
    else if (e.key === "Enter" || e.key === " " || e.key === "Tab") close(true);
    else if (e.key === "Escape") close(false);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div className={t.picker}>
      <button
        ref={btn}
        type="button"
        className={t.button}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`Theme: ${value}`}
        title="Select theme"
        onClick={() => (open ? close(true) : show())}
      >
        <svg viewBox="0 0 24 24" className={t.swatch} aria-hidden="true">
          <path d="M12 2a10 10 0 1 0 0 20zm0 2v16a8 8 0 0 1 0-16z" fill="currentColor" />
        </svg>
        {value}
        <IdeIcon name="chevronDown" size={12} />
      </button>
      {open && (
        <ul
          ref={list}
          className={t.list}
          role="listbox"
          aria-label="Themes"
          tabIndex={-1}
          aria-activedescendant={`theme-opt-${sel}`}
          onKeyDown={onKey}
          onBlur={(e) => {
            // Clicking away is a cancel, as Esc is; the button toggles on its own.
            if (closing.current || e.currentTarget.parentElement?.contains(e.relatedTarget as Node)) return;
            onChange(before.current);
            setOpen(false);
          }}
        >
          {THEMES.map((x, i) => (
            <li
              key={x.name}
              id={`theme-opt-${i}`}
              role="option"
              aria-selected={i === sel}
              className={`${t.item} ${i === sel ? t.on : ""}`}
              onMouseEnter={() => move(i)}
              onMouseDown={(e) => (e.preventDefault(), move(i), close(true))}
            >
              <span>{x.name}</span>
              <span className={t.base}>{x.base}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
