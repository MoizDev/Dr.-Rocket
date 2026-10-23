// Material Symbols (Apache-2.0) glyphs used by the demos, as inline SVG paths (24×24 viewBox).
import type { SVGProps } from "react";

const P = {
  play: "M8 5v14l11-7z",
  pause: "M6 19h4V5H6zm8-14v14h4V5z",
  stop: "M6 6h12v12H6z",
  first: "M6 6h2v12H6zm3.5 6 8.5 6V6z",
  last: "M6 18l8.5-6L6 6zM16 6v12h2V6z",
  prev: "M15.4 7.4 14 6l-6 6 6 6 1.4-1.4-4.6-4.6z",
  next: "M8.6 16.6 10 18l6-6-6-6-1.4 1.4 4.6 4.6z",
  chevronDown: "M7.4 8.6 12 13.2l4.6-4.6L18 10l-6 6-6-6z",
  restart: "M12 5V1L7 6l5 5V7a5 5 0 1 1-5 5H5a7 7 0 1 0 7-7z",
  close: "M19 6.4 17.6 5 12 10.6 6.4 5 5 6.4 10.6 12 5 17.6 6.4 19 12 13.4 17.6 19 19 17.6 13.4 12z",
  add: "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6z",
  search: "M15.5 14h-.8l-.3-.3A6.5 6.5 0 1 0 14 15.5l.3.3v.8l5 5 1.5-1.5zm-6 0a4.5 4.5 0 1 1 0-9 4.5 4.5 0 0 1 0 9z",
  help: "M11 18h2v-2h-2zm1-16a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16zm0-14a4 4 0 0 0-4 4h2a2 2 0 1 1 4 0c0 2-3 1.8-3 5h2c0-2.3 3-2.5 3-5a4 4 0 0 0-4-4z",
  stepOver: "M12 6a7 7 0 0 0-6.9 6H3l3.5 4L10 12H7.2a5 5 0 0 1 9.6 0l1.9-.6A7 7 0 0 0 12 6zm-1 10h2v2h-2z",
  stepInto: "M11 3h2v10l3.5-3.5L18 11l-6 6-6-6 1.5-1.5L11 13zm0 16h2v2h-2z",
  stepOut: "M11 21h2V11l3.5 3.5L18 13l-6-6-6 6 1.5 1.5L11 11zM11 3h2v2h-2z",
  sidebar: "M3 4h18v16H3zm2 2v12h4V6zm6 0v12h8V6z",
  panelRight: "M3 4h18v16H3zm2 2v12h9V6zm11 0v12h3V6z",
  panelBottom: "M3 4h18v16H3zm2 2v8h14V6zm0 10v2h14v-2z",
  zoomIn: "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6z",
  zoomOut: "M19 13H5v-2h14z",
  center: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM3 11h2v2H3zm16 0h2v2h-2zM11 3h2v2h-2zm0 16h2v2h-2z",
  expand: "M5 5h6v2H7v4H5zm14 0v6h-2V7h-4V5zM5 19v-6h2v4h4v2zm14 0h-6v-2h4v-4h2z",
  check: "M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4z",
  terminal: "M4 4h16v16H4zm2 2v12h12V6zm1.5 2.5L11 12l-3.5 3.5-1-1L9 12 6.5 9.5zM12 15h5v1.5h-5z",
  error: "M11 15h2v2h-2zm0-8h2v6h-2zm1-5a10 10 0 1 0 0 20 10 10 0 0 0 0-20z",
  speed: "M20.4 8.6 19 10a7.9 7.9 0 0 1-.3 8H5.3A8 8 0 0 1 15.9 6.9l1.3-1.4A10 10 0 0 0 3.6 19a2 2 0 0 0 1.7 1h13.4a2 2 0 0 0 1.7-1 10 10 0 0 0 0-10.4zm-9.8 7a2 2 0 0 0 2.8 0L19 7l-8.4 5.7a2 2 0 0 0 0 2.9z",
} as const;

export type IconName = keyof typeof P;

export function Icon({ name, size = 16, ...rest }: { name: IconName; size?: number } & SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" style={{ width: `calc(${size} * var(--u))`, height: `calc(${size} * var(--u))`, flex: "none" }} fill="currentColor" aria-hidden="true" focusable="false" {...rest}>
      <path d={P[name]} />
    </svg>
  );
}
