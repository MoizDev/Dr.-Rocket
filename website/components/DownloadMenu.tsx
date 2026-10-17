"use client";

import { useEffect, useId, useRef, useState } from "react";
import type { Download, Os } from "@/lib/release";
import { ArrowUpRight, ChevronDown, OsIcon } from "./icons";
import styles from "./Download.module.css";

type Props = { downloads: Record<Os, Download>; placement: "down" | "up" };

/** The chevron half of the download button: every platform, and the Racket requirement. */
export default function DownloadMenu({ downloads, placement }: Props) {
  const [open, setOpen] = useState(false);
  const [detected, setDetected] = useState<string | undefined>();
  const root = useRef<HTMLDivElement>(null);
  const id = useId();

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        root.current?.querySelector("button")?.focus();
      }
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className={styles.more} ref={root}>
      <button
        type="button"
        className={styles.chevron}
        aria-expanded={open}
        aria-controls={id}
        aria-label="Other platforms"
        onClick={() => {
          setDetected(document.documentElement.dataset.os);
          setOpen((o) => !o);
        }}
      >
        <ChevronDown size={18} />
      </button>
      <div id={id} className={styles.menu} data-open={open} data-placement={placement} role="menu" hidden={!open}>
        {Object.values(downloads).map((d) => (
          <a
            key={d.os}
            role="menuitem"
            className={styles.item}
            href={d.href}
            download={d.filename}
            onClick={() => setOpen(false)}
          >
            <span className={styles.itemIcon}>
              <OsIcon os={d.os} size={20} />
            </span>
            <span className={styles.itemText}>
              <span className={styles.itemName}>
                {d.name}
                {detected === d.os && <span className={styles.badge}>Your system</span>}
              </span>
              <span className={styles.itemKind}>
                {d.kind}
                {d.size && ` · ${d.size}`}
              </span>
            </span>
          </a>
        ))}
        <p className={styles.menuNote}>
          Requires Racket:{" "}
          <a href="https://racket-lang.org" target="_blank" rel="noopener">
            get it at racket-lang.org <ArrowUpRight size={12} className={styles.inlineIcon} />
          </a>
        </p>
      </div>
    </div>
  );
}
