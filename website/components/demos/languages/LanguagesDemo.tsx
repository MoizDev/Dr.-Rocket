"use client";
// The Choose Language dialog, in place (a group, not a modal). Settings follow the app's
// per-level rules (lib/demos/data/languages.ts). Nothing is run. Picking a language updates the
// settings, the language pill and the Interactions banner at once; Cancel goes back to the last OK.
import { useCallback, useRef, useState, type KeyboardEvent } from "react";
import DemoRoot, { DisclosureChip } from "../DemoRoot";
import KeysHelp from "../KeysHelp";
import { IdeIcon } from "../IdeIcons";
import { useEntryHint } from "@/lib/demos/hint";
import {
  DIALOG_SUBTITLE,
  DIALOG_TITLE,
  FRACTION_LABEL,
  GROUPS,
  LEGACY_TEXT,
  MODULE_TEXT,
  PRINTING_LABEL,
  REST,
  STEPPER_CARD,
  TEACHPACKS,
  defaultSettings,
  isTeaching,
  level,
  printingStyles,
  showsHtdpOptions,
  showsSharingOption,
  stepperLevel,
  type FractionStyle,
  type LangId,
  type Settings,
} from "@/lib/demos/data/languages";
import s from "./Languages.module.css";

export const LABEL =
  "The Choose Language dialog listing the Racket language, the How to Design Programs teaching languages and DeinProgramm, with output and fraction settings for Beginning Student.";

const ORDER: LangId[] = GROUPS.flatMap((g) => g.languages);

const KEYS: [string, string][] = [
  ["↑ / ↓", "Choose a language"],
  ["Home / End", "First / last language"],
  ["Enter", "OK (in the list)"],
  ["Space", "Toggle or pick a setting"],
  ["Esc", "Back to the demo"],
];

type State = {
  id: LangId;
  settings: Settings;
  /** Settings per language while browsing, so switching back keeps edits (as in the app). */
  edited: Partial<Record<LangId, Settings>>;
};

const initial = (c: { id: LangId; settings: Settings }): State => ({ id: c.id, settings: c.settings, edited: { [c.id]: c.settings } });

export default function LanguagesDemo({ className }: { className: string }) {
  const [confirmed, setConfirmed] = useState(REST);
  const [st, setSt] = useState<State>(() => initial(REST));
  const root = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);

  const select = useCallback((l: LangId) => {
    setSt((cur) => {
      if (l === cur.id) return cur;
      const edited = { ...cur.edited, [cur.id]: cur.settings };
      return { id: l, settings: edited[l] ?? defaultSettings(l), edited };
    });
  }, []);
  const set = (patch: Partial<Settings>) => setSt((cur) => ({ ...cur, settings: { ...cur.settings, ...patch } }));

  // Entry hint: the selection moves from the Racket language to Beginning Student.
  useEntryHint(
    root,
    () => [
      { at: 0, run: () => select("module") },
      { at: 1100, run: () => select(REST.id) },
    ],
    () => setSt(initial(REST)),
  );

  const onListKey = (e: KeyboardEvent) => {
    const i = ORDER.indexOf(st.id);
    const to =
      e.key === "ArrowDown" ? ORDER[Math.min(ORDER.length - 1, i + 1)] : e.key === "ArrowUp" ? ORDER[Math.max(0, i - 1)] : e.key === "Home" ? ORDER[0] : e.key === "End" ? ORDER[ORDER.length - 1] : null;
    if (to) {
      e.preventDefault();
      select(to);
      requestAnimationFrame(() => list.current?.querySelector<HTMLElement>(`[data-lang="${to}"]`)?.focus());
    } else if (e.key === "Enter") {
      e.preventDefault();
      ok();
    } else if (e.key === "Escape") {
      root.current?.focus();
    }
  };

  const ok = () => setConfirmed({ id: st.id, settings: st.settings });
  const cancel = () => setSt(initial(confirmed));

  const lv = level(st.id);
  const cfg = st.settings;

  return (
    <DemoRoot id="languages" className={className} w={1762} h={1281} label={LABEL} rootProps={{ ref: root, tabIndex: -1 }}>
      <div className={s.dialog}>
        <div className={s.title}>{DIALOG_TITLE}</div>
        <div className={s.subtitle} data-decorative-contrast>{DIALOG_SUBTITLE}</div>
        <div className={s.headerTools}>
          <DisclosureChip />
          <KeysHelp keys={KEYS} placement="down" />
        </div>

        <div ref={list} className={s.list} role="listbox" aria-label="Language" onKeyDown={onListKey}>
          {GROUPS.map((g, gi) => {
            const heading = gi === 0 || GROUPS[gi - 1].heading !== g.heading ? g.heading : null;
            return (
              <div key={gi} role="group" aria-label={g.title || g.heading} className={s.group}>
                {heading && <div className={s.heading} data-decorative-contrast aria-hidden="true">{heading}</div>}
                {g.title && <div className={s.groupTitle} aria-hidden="true">{g.title}</div>}
                {g.languages.map((id) => {
                  const l = level(id);
                  const sel = id === st.id;
                  return (
                    <div
                      key={id}
                      role="option"
                      aria-selected={sel}
                      tabIndex={sel ? 0 : -1}
                      data-lang={id}
                      className={`${s.item} ${sel ? s.sel : ""}`}
                      onClick={() => select(id)}
                      onDoubleClick={ok}
                    >
                      <span className={s.ring} aria-hidden="true" />
                      <span className={s.name}>{l.name}</span>
                      <span className={s.summary}>{l.summary}</span>
                    </div>
                  );
                })}
              </div>
            );
          })}
        </div>
        <div className={s.vline} aria-hidden="true" />

        <div className={s.details} role="group" aria-label={`Settings for ${lv.name}`}>
          <div className={s.dName}>{lv.name}</div>
          <div className={s.dSummary}>{lv.summary}</div>
          {isTeaching(st.id) ? (
            <>
              <div className={`${s.section} ${s.first}`} data-decorative-contrast id="lang-in">Input Syntax</div>
              <Toggle label="Case sensitive" on={cfg.caseSensitive} set={(v) => set({ caseSensitive: v })} />

              <div className={s.section} data-decorative-contrast id="lang-out">Output Syntax</div>
              <div role="radiogroup" aria-labelledby="lang-out">
                {printingStyles(st.id).map((p) => (
                  <Radio key={p} name={`out-${st.id}`} label={PRINTING_LABEL[p]} on={cfg.printingStyle === p} pick={() => set({ printingStyle: p })} />
                ))}
              </div>
              <div className={s.fracLabel} id="lang-frac">Fraction style</div>
              <div role="radiogroup" aria-labelledby="lang-frac">
                {(["mixed-fraction", "repeating-decimal"] as FractionStyle[]).map((f) => (
                  <Radio key={f} name={`frac-${st.id}`} label={FRACTION_LABEL[f]} on={cfg.fractionStyle === f} pick={() => set({ fractionStyle: f })} />
                ))}
              </div>
              <div className={s.toggles}>
                {showsSharingOption(st.id) && <Toggle label="Show sharing in values" on={cfg.showSharing} set={(v) => set({ showSharing: v })} />}
                <Toggle label="Insert newlines in printed values" on={cfg.insertNewlines} set={(v) => set({ insertNewlines: v })} />
                {showsHtdpOptions(st.id) && (
                  <>
                    <Toggle label="Print true/false/empty instead of #true/#false/'()" on={cfg.trueFalseEmpty} set={(v) => set({ trueFalseEmpty: v })} />
                    <Toggle label="Tracing (show function calls)" on={cfg.tracing} set={(v) => set({ tracing: v })} />
                  </>
                )}
              </div>

              <div className={s.section} data-decorative-contrast>Teachpacks</div>
              {cfg.teachpacks.map((tp) => (
                <div key={tp} className={s.tp}>
                  <svg viewBox="0 0 24 24" className={s.tpIcon} aria-hidden="true">
                    <path d="M20.5 11H19V7a2 2 0 0 0-2-2h-4V3.5a2.5 2.5 0 0 0-5 0V5H4a2 2 0 0 0-2 2v3.8h1.5a2.7 2.7 0 0 1 0 5.4H2V20a2 2 0 0 0 2 2h3.8v-1.5a2.7 2.7 0 0 1 5.4 0V22H17a2 2 0 0 0 2-2v-4h1.5a2.5 2.5 0 0 0 0-5z" fill="currentColor" />
                  </svg>
                  <span className={s.tpName}>{tp}</span>
                  <button type="button" className={s.small} onClick={() => set({ teachpacks: cfg.teachpacks.filter((x) => x !== tp) })} aria-label={`Remove ${tp}`}>
                    Remove
                  </button>
                </div>
              ))}
              {TEACHPACKS.some((t) => !cfg.teachpacks.includes(t)) && (
                <span className={s.comboWrap}>
                  <select
                    className={s.combo}
                    aria-label="Add teachpack"
                    value=""
                    onChange={(e) => e.target.value && set({ teachpacks: [...cfg.teachpacks, e.target.value] })}
                  >
                    <option value="">Add teachpack…</option>
                    {TEACHPACKS.filter((t) => !cfg.teachpacks.includes(t)).map((t) => (
                      <option key={t} value={t}>
                        {t}
                      </option>
                    ))}
                  </select>
                  <span className={s.caret} aria-hidden="true">▼</span>
                </span>
              )}
              {stepperLevel(st.id) && (
                <div className={s.stepper} data-stepper-card>
                  <svg viewBox="0 0 24 24" className={s.stepIcon} aria-hidden="true">
                    <path d="M4 18 12.5 12 4 6zm9 0 8.5-6L13 6z" fill="currentColor" />
                  </svg>
                  <span>{STEPPER_CARD}</span>
                </div>
              )}
            </>
          ) : st.id === "module" ? (
            <>
              <p className={s.para}>{MODULE_TEXT[0]}</p>
              <p className={s.paraHint}>{MODULE_TEXT[1]}</p>
            </>
          ) : (
            <p className={s.para}>{LEGACY_TEXT}</p>
          )}
        </div>

        <div className={s.footer}>
          {isTeaching(st.id) && (
            <button type="button" className={s.plain} onClick={() => set(defaultSettings(st.id))}>
              Revert to Language Defaults
            </button>
          )}
          <span className={s.spacer} />
          <button
            type="button"
            className={s.pill}
            title="Choose a language"
            aria-label={`Language: ${lv.short}`}
            onClick={() => list.current?.querySelector<HTMLElement>(`[data-lang="${st.id}"]`)?.focus()}
          >
            <IdeIcon name={isTeaching(st.id) ? "cap" : "code"} size={13} className={s.pillIcon} />
            {lv.short}
            <IdeIcon name="chevronDown" size={12} />
          </button>
          <span className={s.banner} data-confirmed={confirmed.id}>
            <span>Welcome to Racket 9.3</span>
            <span className={s.bannerLang} data-decorative-contrast>Language: {lv.name}</span>
          </span>
          <button type="button" className={s.plain} onClick={cancel}>
            Cancel
          </button>
          <button type="button" className={s.primary} data-decorative-contrast onClick={ok}>
            OK
          </button>
        </div>
      </div>
    </DemoRoot>
  );
}

function Toggle({ label, on, set }: { label: string; on: boolean; set: (v: boolean) => void }) {
  return (
    <label className={s.toggle}>
      <input type="checkbox" role="switch" checked={on} onChange={(e) => set(e.target.checked)} className={s.toggleInput} />
      <span className={s.track} aria-hidden="true" />
      <span>{label}</span>
    </label>
  );
}

function Radio({ name, label, on, pick }: { name: string; label: string; on: boolean; pick: () => void }) {
  return (
    <label className={s.radio}>
      <input type="radio" name={name} checked={on} onChange={pick} className={s.radioInput} />
      <span className={s.dot} aria-hidden="true" />
      <span>{label}</span>
    </label>
  );
}
