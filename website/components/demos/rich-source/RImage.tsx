// Draws a prepared image value as SVG, at the app's scale (an image pixel is 0.736 pt, as in the
// screenshot). Text from a string literal is rendered by React, so it is always escaped.
import { BAR, describe, size, type Img } from "@/lib/demos/data/richsource-repl";
import { BODY_FILL, FLAME_FILL, ROCKET_BODY, ROCKET_FLAME, ROCKET_TRANSFORM, TILE } from "@/lib/demos/rocket-tile";

export const IMAGE_SCALE = 103 / 140;

function Node({ img, x, y, grad }: { img: Img; x: number; y: number; grad: string }) {
  const s = size(img);
  switch (img.kind) {
    case "rocket":
      return (
        <g transform={`translate(${x} ${y})`}>
          <rect width={TILE} height={TILE} fill={`url(#${grad})`} />
          <g transform={ROCKET_TRANSFORM}>
            <path d={ROCKET_BODY} fill={BODY_FILL} />
            <path d={ROCKET_FLAME} fill={FLAME_FILL} />
          </g>
        </g>
      );
    case "bar":
      return (
        <g transform={`translate(${x} ${y})`}>
          <rect width={s.w} height={BAR.h} fill={BAR.fill} />
          <text x={s.w / 2} y={BAR.h / 2} dominantBaseline="central" textAnchor="middle" fill="#fff" fontSize={BAR.font} fontFamily="var(--font-app), sans-serif" fontWeight={500} style={{ whiteSpace: "pre" }}>
            {img.title}
          </text>
        </g>
      );
    case "beside": {
      const xs = img.items.map((_, i) => x + img.items.slice(0, i).reduce((a, it) => a + size(it).w, 0));
      return (
        <>
          {img.items.map((it, i) => (
            <Node key={i} img={it} x={xs[i]} y={y + (s.h - size(it).h) / 2} grad={grad} />
          ))}
        </>
      );
    }
    case "above": {
      const ys = img.items.map((_, i) => y + img.items.slice(0, i).reduce((a, it) => a + size(it).h, 0));
      return (
        <>
          {img.items.map((it, i) => (
            <Node key={i} img={it} x={x + (s.w - size(it).w) / 2} y={ys[i]} grad={grad} />
          ))}
        </>
      );
    }
  }
}

/** `uid` must be unique in the page and stable between server and client (gradient id). */
export default function RImage({ img, uid, className }: { img: Img; uid: string; className?: string }) {
  const s = size(img);
  const grad = `rs-grad-${uid}`;
  return (
    <svg
      className={className}
      role="img"
      aria-label={describe(img)}
      viewBox={`0 0 ${s.w} ${s.h}`}
      style={{ width: `calc(${s.w * IMAGE_SCALE} * var(--u))`, height: `calc(${s.h * IMAGE_SCALE} * var(--u))`, display: "block" }}
    >
      <defs>
        <linearGradient id={grad} x1="0" y1="0" x2="0.27" y2="1">
          <stop offset="0" stopColor="#3d87f6" />
          <stop offset="0.7" stopColor="#2a5ddc" />
          <stop offset="1" stopColor="#2450d6" />
        </linearGradient>
      </defs>
      <Node img={img} x={0} y={0} grad={grad} />
    </svg>
  );
}
