"use client";

import { useEffect, useState } from "react";
import { RocketMark } from "./icons";
import styles from "./HeroTitle.module.css";

type Phase = "static" | "fly" | "type" | "done";

/**
 * The hero headline's intro, after Kiro's: the rocket flies in and lands at the start of the
 * line beside a blinking caret, then launches away as the headline types itself out.
 * Without scripts, or with reduced motion, the headline is simply there.
 */
export default function HeroTitle({ text, className }: { text: string; className?: string }) {
  const [phase, setPhase] = useState<Phase>("static");
  const [shown, setShown] = useState(text.length);

  useEffect(() => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    let timer = 0;
    let i = 0;
    const tick = () => {
      i += 1;
      setShown(i);
      if (i < text.length) timer = window.setTimeout(tick, text[i - 1] === "\n" ? 260 : 34 + Math.random() * 46);
      else timer = window.setTimeout(() => setPhase("done"), 400);
    };
    // Deferred so React treats these as a response to mounting, not a cascade during it.
    timer = window.setTimeout(() => {
      setShown(0);
      setPhase("fly");
      timer = window.setTimeout(() => {
        setPhase("type");
        timer = window.setTimeout(tick, 260);
      }, 1250);
    }, 0);
    return () => window.clearTimeout(timer);
  }, [text]);

  // The first line is washed out, the second at full strength.
  const nl = text.includes("\n") ? text.indexOf("\n") : text.length;
  const paint = (from: number, to: number) => (
    <>
      {from < nl && <span className={styles.lead}>{text.slice(from, Math.min(to, nl))}</span>}
      {to > nl && text.slice(Math.max(from, nl), to)}
    </>
  );

  return (
    <h1 className={className} aria-label={text.replace("\n", " ")}>
      <span className={styles.line} data-phase={phase} aria-hidden="true">
        {phase !== "static" && (
          <span className={styles.rocket}>
            <RocketMark size={64} />
          </span>
        )}
        <span>{paint(0, shown)}</span>
        {phase !== "static" && <span className={styles.caret} />}
        <span className={styles.rest}>{paint(shown, text.length)}</span>
      </span>
    </h1>
  );
}
