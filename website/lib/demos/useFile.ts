"use client";
import { useCallback, useSyncExternalStore } from "react";
import { status, type Status } from "./fixtures";
import { getText, original, setText, subscribe } from "./workspace";

/** A workspace file's live text and fixture status. */
export function useFile(id: string): { text: string; original: string; status: Status; set: (t: string) => void } {
  const text = useSyncExternalStore(
    useCallback((l: () => void) => subscribe(id, l), [id]),
    () => getText(id),
    () => original(id),
  );
  const orig = original(id);
  return { text, original: orig, status: status(text, orig), set: useCallback((t: string) => setText(id, t), [id]) };
}
