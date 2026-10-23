"use client";
// The edited-file notice: one quiet line in the app's style, with an explicit restore.
import { EDITED_NOTICE } from "@/lib/demos/fixtures";
import s from "./Shared.module.css";

export default function Notice({
  onRestore,
  text = EDITED_NOTICE,
  title = "Edited",
  restoreLabel = "Restore",
  compact,
}: {
  onRestore: () => void;
  text?: string;
  /** A short bold lead-in; left out when `compact`. */
  title?: string;
  restoreLabel?: string;
  /** No lead-in and no outer margin, for tight spots such as a REPL scrollback. */
  compact?: boolean;
}) {
  return (
    <div className={`${s.notice} ${compact ? s.noticeCompact : ""}`} role="status" data-notice>
      <span className={s.noticeDot} aria-hidden="true">●</span>
      {!compact && title && <span className={s.noticeTitle}>{title}</span>}
      <span className={s.noticeText}>{text}</span>
      <button type="button" className={s.restore} onClick={onRestore} data-restore>
        {restoreLabel}
      </button>
    </div>
  );
}
