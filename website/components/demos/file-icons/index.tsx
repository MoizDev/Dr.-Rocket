// File icons from material-icon-theme (MIT), copied from assets/icons/files/.
import type { CSSProperties } from "react";

const SVG: Record<string, string> = {
  "racket": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 511.875 511.824\"><path fill=\"#0288d1\" d=\"M416.17 381.6c27.189-34.614 43.405-78.256 43.405-125.685 0-112.466-91.168-203.634-203.634-203.634-24.464 0-47.92 4.319-69.65 12.228 82.671 43.366 192.023 184.854 229.88 317.097z\"/><path fill=\"#e53935\" d=\"M226.77 182.174c-31.766-34.221-67.34-61.4-105.015-79.425-42.569 37.324-69.453 92.102-69.453 153.162 0 51.344 19.009 98.24 50.365 134.06 27.641-83.049 79.607-163.09 124.108-207.8zm37.526 46.176c-44.085 47.512-88.014 130.681-103.913 207.415 28.498 15.172 61.02 23.783 95.561 23.783 35.508 0 68.888-9.097 97.951-25.074-16.752-77.405-48.375-148.294-89.591-206.127z\"/></svg>",
  "folder": "<svg viewBox=\"0 0 16 16\" xmlns=\"http://www.w3.org/2000/svg\"><path d=\"m6.922 3.768-.644-.536A1 1 0 0 0 5.638 3H2a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V5a1 1 0 0 0-1-1H7.562a1 1 0 0 1-.64-.232\" fill=\"#90a4ae\" /></svg>",
  "folder-open": "<svg viewBox=\"0 0 16 16\" xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M14.483 6H4.721a1 1 0 0 0-.949.684L2 12V5h12a1 1 0 0 0-1-1H7.562a1 1 0 0 1-.64-.232l-.644-.536A1 1 0 0 0 5.638 3H2a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h11l2.403-5.606A1 1 0 0 0 14.483 6\" fill=\"#90a4ae\" /></svg>",
  "readme": "<svg xmlns=\"http://www.w3.org/2000/svg\" fill=\"none\" viewBox=\"0 0 16 16\"><path d=\"M0 0h24v24H0z\"/><path fill=\"#42a5f5\" d=\"M8 1C4.136 1 1 4.136 1 8s3.136 7 7 7 7-3.136 7-7-3.136-7-7-7m1 11H7V7.5h2zm0-6H7V4h2z\"/></svg>",
  "markdown": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><path fill=\"#42a5f5\" d=\"m14 10-4 3.5L6 10H4v12h4v-6l2 2 2-2v6h4V10zm12 6v-6h-4v6h-4l6 8 6-8z\"/></svg>",
};

export type FileIconName = "racket" | "folder" | "folder-open" | "readme" | "markdown";

export function fileIconFor(name: string, open?: boolean, dir?: boolean): FileIconName {
  if (dir) return open ? "folder-open" : "folder";
  if (/^readme/i.test(name)) return "readme";
  if (name.endsWith(".md")) return "markdown";
  return "racket";
}

export function FileIcon({ name, size = 16 }: { name: FileIconName; size?: number }) {
  const style = { width: `calc(${size} * var(--u))`, height: `calc(${size} * var(--u))`, display: "inline-flex", flex: "none" } as CSSProperties;
  return <span aria-hidden="true" style={style} className="file-icon" dangerouslySetInnerHTML={{ __html: SVG[name] }} />;
}
