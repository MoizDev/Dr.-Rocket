// Extra Material Symbols (Apache-2.0) glyphs for the IDE chrome, as inline SVG paths.
import type { SVGProps } from "react";

const P = {
  menu: "M3 6h18v2H3zm0 5h18v2H3zm0 5h18v2H3z",
  folder: "M10 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-8z",
  outline: "M3 5h4v2H3zm6 0h12v2H9zM3 11h4v2H3zm6 0h12v2H9zm-6 6h4v2H3zm6 0h12v2H9z",
  search: "M15.5 14h-.8l-.3-.3A6.5 6.5 0 1 0 14 15.5l.3.3v.8l5 5 1.5-1.5zm-6 0a4.5 4.5 0 1 1 0-9 4.5 4.5 0 0 1 0 9z",
  newFile: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zm-1 9h-2v3H8v2h3v3h2v-3h3v-2h-3zM13 3.5 18.5 9H13z",
  refresh: "M17.6 6.4A8 8 0 1 0 19.7 14h-2.1A6 6 0 1 1 12 6a5.9 5.9 0 0 1 4.2 1.8L13 11h7V4z",
  collapse: "M7.4 18.6 8.8 20l3.2-3.2 3.2 3.2 1.4-1.4L12 14zM16.6 5.4 15.2 4 12 7.2 8.8 4 7.4 5.4 12 10z",
  chevronRight: "M8.6 16.6 13.2 12 8.6 7.4 10 6l6 6-6 6z",
  chevronDown: "M7.4 8.6 12 13.2l4.6-4.6L18 10l-6 6-6-6z",
  more: "M12 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4zm0 2a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm0 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4z",
  play: "M8 5v14l11-7z",
  close: "M19 6.4 17.6 5 12 10.6 6.4 5 5 6.4 10.6 12 5 17.6 6.4 19 12 13.4 17.6 19 19 17.6 13.4 12z",
  add: "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6z",
  layoutLeft: "M3 4h18v16H3zm2 2v12h4V6zm6 0v12h8V6z",
  layoutBottom: "M3 4h18v16H3zm2 2v8h14V6zm0 10v2h14v-2z",
  layoutRight: "M3 4h18v16H3zm2 2v12h9V6zm11 0v12h3V6z",
  repl: "M4 4h16v16H4zm2 2v12h12V6zm1.5 2.5L11 12l-3.5 3.5-1-1L9 12 6.5 9.5zM12 15h5v1.5h-5z",
  stepper: "M3 11h11.2l-3.6-3.6L12 6l6 6-6 6-1.4-1.4 3.6-3.6H3zm17-5h2v12h-2z",
  tree: "M22 11V3h-7v3H9V3H2v8h7V8h2v10h4v3h7v-8h-7v3h-2V8h2v3z",
  flask: "M19.8 18.4 14 10.7V6.5l1.4-1.7c.3-.3 0-.8-.4-.8H9c-.4 0-.7.5-.4.8L10 6.5v4.2l-5.8 7.7A1 1 0 0 0 5 20h14a1 1 0 0 0 .8-1.6z",
  wand: "M7.5 5.6 10 7 8.6 4.5 10 2 7.5 3.4 5 2l1.4 2.5L5 7zm12 9.8L17 14l1.4 2.5L17 19l2.5-1.4L22 19l-1.4-2.5L22 14zM22 2l-2.5 1.4L17 2l1.4 2.5L17 7l2.5-1.4L22 7l-1.4-2.5zm-7.6 5.3a1 1 0 0 0-1.4 0L1.3 18.9a1 1 0 0 0 0 1.4l2.4 2.4a1 1 0 0 0 1.4 0L16.7 11a1 1 0 0 0 0-1.4zm-1 5.5-2.2-2.2 2.4-2.4 2.2 2.2z",
  broom: "M19.4 3.4 18 2l-5.6 5.6-1-1a2 2 0 0 0-2.8 0l-.7.7 8.4 8.4.7-.7a2 2 0 0 0 0-2.8l-1-1zM6.5 8.6 3 12.1V21h8.9l3.5-3.5z",
  terminal: "M20 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2zm0 14H4V8h16zM6 10l4 4-4 4-1.4-1.4L7.2 14l-2.6-2.6zm5 6h6v2h-6z",
  problems: "M11 15h2v2h-2zm0-8h2v6h-2zm1-5a10 10 0 1 0 .1 20A10 10 0 0 0 12 2zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16z",
  check: "M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4z",
  checkCircle: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm-2 15-5-5 1.4-1.4 3.6 3.6 7.6-7.6L19 8z",
  send: "M4 12l1.4 1.4L11 7.8V20h2V7.8l5.6 5.6L20 12l-8-8z",
  code: "M9.4 16.6 4.8 12l4.6-4.6L8 6l-6 6 6 6zm5.2 0 4.6-4.6-4.6-4.6L16 6l6 6-6 6z",
  cap: "M5 13.2v4L12 21l7-3.8v-4L12 17zM12 3 1 9l11 6 9-4.9V17h2V9z",
  info: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm1 15h-2v-6h2zm0-8h-2V7h2z",
  lines: "M3 5h18v2H3zm0 6h12v2H3zm0 6h18v2H3z",
  help: "M11 18h2v-2h-2zm1-16a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16zm0-14a4 4 0 0 0-4 4h2a2 2 0 1 1 4 0c0 2-3 1.8-3 5h2c0-2.3 3-2.5 3-5a4 4 0 0 0-4-4z",
} as const;

export type IdeIconName = keyof typeof P;

export function IdeIcon({ name, size = 16, ...rest }: { name: IdeIconName; size?: number } & SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      style={{ width: `calc(${size} * var(--u))`, height: `calc(${size} * var(--u))`, flex: "none" }}
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      <path d={P[name]} />
    </svg>
  );
}
