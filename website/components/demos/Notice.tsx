"use client";
// The edited-file notice, in the app's style, with an explicit restore.
import { EDITED_NOTICE } from "@/lib/demos/fixtures";
import s from "./Shared.module.css";

export default function Notice({
  onRestore,
  text = EDITED_NOTICE,
  title = "Edited",
  restoreLabel = "Restore original",
  compact,
}: {
  onRestore: () => void;
  text?: string;
  title?: string;
  restoreLabel?: string;
  compact?: boolean;
}) {
  return (
    <div className={compact ? s.noticeCompact : s.notice} role="status" data-notice>
      <span className={s.noticeDot} aria-hidden="true">●</span>
      <div className={s.noticeBody}>
        {!compact && <div className={s.noticeTitle}>{title}</div>}
        <div className={s.noticeText}>{text}</div>
        <div className={s.noticeActions}>
          <button type="button" className={s.restore} onClick={onRestore} data-restore>
            {restoreLabel}
          </button>
          {!compact && <span className={s.noticeHint}>or undo with ⌘Z</span>}
        </div>
      </div>
    </div>
  );
}
