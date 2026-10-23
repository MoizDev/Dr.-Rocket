// Small UI glyphs for the Visualizer and Debugger (Material Symbols shapes, Apache-2.0), drawn
// as inline SVG. Sizes are app points.
const P = {
  menu: "M3 6h18v2H3zm0 5h18v2H3zm0 5h18v2H3z",
  cap: "M12 3 1 9l11 6 9-4.9V17h2V9zM5 13.2v4L12 21l7-3.8v-4L12 17z",
  dots: "M12 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4zm0 2a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm0 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4z",
  terminal: "M4 4h16v16H4zm2 4v10h12V8zm1.5 1.5L11 13l-3.5 3.5-1-1L9 13l-2.5-2.5zM12 15h5v1.5h-5z",
  stepper: "M2 11h3v2H2zm5 0h3v2H7zm6-4 6 5-6 5V13h-1v-2h1z",
  viz: "M4 4h6v6H7v3h4v-2h6v6h-6v-2H5V10H4zm2 2v2h2V6zm7 7v2h2v-2zM14 4h6v6h-6zm2 2v2h2V6z",
  tests: "M9 2h6v2h-1v6l5 8.5A2 2 0 0 1 17.3 21H6.7A2 2 0 0 1 5 18.5L10 10V4H9zm3 9.5L9.5 16h5z",
  macro: "M7.5 5.6 10 7 8.6 4.5 10 2 7.5 3.4 5 2l1.4 2.5L5 7zm12 9.8L17 14l1.4 2.5L17 19l2.5-1.4L22 19l-1.4-2.5L22 14zM22 2l-2.5 1.4L17 2l1.4 2.5L17 7l2.5-1.4L22 7l-1.4-2.5zm-7.6 5.3a1 1 0 0 0-1.4 0L1.3 18.9a1 1 0 0 0 0 1.4l2.3 2.3a1 1 0 0 0 1.4 0L16.7 11a1 1 0 0 0 0-1.4zm-1 5.5-2.1-2.1 2.4-2.4 2.1 2.1z",
  first: "M6 6h2v12H6zm3.5 6 8.5 6V6z",
  prev: "M15.4 7.4 14 6l-6 6 6 6 1.4-1.4-4.6-4.6z",
  play: "M8 5v14l11-7z",
  pause: "M6 5h4v14H6zm8 0h4v14h-4z",
  next: "M8.6 16.6 10 18l6-6-6-6-1.4 1.4 4.6 4.6z",
  last: "M6 18l8.5-6L6 6zM16 6v12h2V6z",
  over: "M12 7a7 7 0 0 0-6.9 6H3l3.5 4 3.5-4H7.2a5 5 0 0 1 9.6-1.5l1.9-.6A7 7 0 0 0 12 7zm-1 10h2v2h-2z",
  into: "M11 3h2v10l3.5-3.5L18 11l-6 6-6-6 1.5-1.5L11 13zm0 16h2v2h-2z",
  out: "M11 21h2V9l3.5 3.5L18 11l-6-6-6 6 1.5 1.5L11 9zM8 3h8v2H8z",
  speed: "M20.4 8.6 19 10a7.9 7.9 0 0 1-.3 8H5.3A8 8 0 0 1 15.9 6.9l1.3-1.4A10 10 0 0 0 3.6 19a2 2 0 0 0 1.7 1h13.4a2 2 0 0 0 1.7-1 10 10 0 0 0 0-10.4zm-9.8 7a2 2 0 0 0 2.8 0L19 7l-8.4 5.7a2 2 0 0 0 0 2.9z",
  center: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zm8.9 3A9 9 0 0 0 13 3.1V1h-2v2.1A9 9 0 0 0 3.1 11H1v2h2.1a9 9 0 0 0 7.9 7.9V23h2v-2.1a9 9 0 0 0 7.9-7.9H23v-2zM12 19a7 7 0 1 1 0-14 7 7 0 0 1 0 14z",
  expand: "M14 3h7v7h-2V6.4l-4.3 4.3-1.4-1.4L17.6 5H14zM3 14h2v3.6l4.3-4.3 1.4 1.4L6.4 19H10v2H3z",
  restart: "M12 5V1L7 6l5 5V7a5 5 0 1 1-5 5H5a7 7 0 1 0 7-7z",
  list: "M3 13h2v-2H3zm0 4h2v-2H3zm0-8h2V7H3zm4 4h14v-2H7zm0 4h14v-2H7zM7 7v2h14V7z",
  bug: "M20 8h-2.8a6 6 0 0 0-1.8-2l1.6-1.6L15.6 3l-2.2 2.2a6 6 0 0 0-2.8 0L8.4 3 7 4.4 8.6 6a6 6 0 0 0-1.8 2H4v2h2.1a6 6 0 0 0 0 1v1H4v2h2v1a6 6 0 0 0 .1 1H4v2h2.8a6 6 0 0 0 10.4 0H20v-2h-2.1a6 6 0 0 0 .1-1v-1h2v-2h-2v-1a6 6 0 0 0-.1-1H20zm-6 8h-4v-2h4zm0-4h-4v-2h4z",
  home: "M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z",
  hourglass: "M6 2v6l4 4-4 4v6h12v-6l-4-4 4-4V2zm10 14.5V20H8v-3.5l4-4zm-4-5-4-4V4h8v3.5z",
} as const;

export type GlyphName = keyof typeof P;

export function Glyph({ name, size = 15 }: { name: GlyphName; size?: number }) {
  return (
    <svg
      viewBox="0 0 24 24"
      style={{ width: `calc(${size} * var(--u))`, height: `calc(${size} * var(--u))`, flex: "none" }}
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      <path d={P[name]} />
    </svg>
  );
}
