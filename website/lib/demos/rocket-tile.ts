// The rocket image pasted into poster.rkt: the brand rocket (public/brand/rocket.svg) on a blue
// tile, 140×140 image pixels as Racket sees it. Drawn as inline SVG, never an <img>. No ids, so
// any number of copies can sit in one page.

/** The brand rocket's two paths, in its own 94.53 × 145.79 viewBox. */
export const ROCKET_BODY =
  "M92.15,67.94l-6.89-7.15c-2.12-2.19-3.3-5.12-3.3-8.18v-18.99c0-1.7-.68-3.34-1.89-4.54L53.72,2.68c-3.56-3.57-9.35-3.57-12.91,0L14.46,29.08c-1.21,1.2-1.88,2.84-1.88,4.54v18.99c0,3.05-1.19,5.99-3.31,8.19l-6.89,7.14c-1.53,1.58-2.38,3.7-2.38,5.9v34.41c0,7.43,8.78,11.37,14.33,6.43l23.35-20.81c5.47-4.87,13.71-4.87,19.17,0l23.35,20.8c5.54,4.94,14.33,1.01,14.33-6.42v-34.41c0-2.2-.86-4.32-2.38-5.9ZM67.98,47.09l-7.55,2.22c-4.3,1.26-7.66,4.62-8.92,8.91l-2.22,7.56c-.59,2.03-3.47,2.03-4.06,0l-2.22-7.56c-1.26-4.29-4.62-7.65-8.92-8.91l-7.55-2.22c-2.03-.6-2.03-3.47,0-4.07l7.55-2.21c4.3-1.27,7.66-4.62,8.92-8.92l2.22-7.56c.59-2.03,3.47-2.03,4.06,0l2.22,7.56c1.26,4.3,4.62,7.65,8.92,8.92l7.55,2.21c2.03.6,2.03,3.47,0,4.07Z";
export const ROCKET_FLAME =
  "M64.31,123.89l-12.77,19.58c-2.01,3.09-6.54,3.09-8.55,0l-12.77-19.58c-2.02-3.09-1.55-7.18,1.11-9.74l11.48-11.02c2.49-2.39,6.42-2.39,8.91,0l11.48,11.02c2.66,2.56,3.13,6.64,1.11,9.74Z";

export const TILE = 140;
export const TILE_BG = "linear-gradient(165deg, #3d87f6 0%, #2a5ddc 70%, #2450d6 100%)";
export const BODY_FILL = "#dadde3";
export const FLAME_FILL = "#61afef";
/** The rocket's placement inside the tile: 72% of its height, centred. */
export const ROCKET_TRANSFORM = "translate(37.32 19) scale(0.6914)";

/** Inner SVG markup of a tile (for the editor's inline image widget). */
export function rocketTile(radius = 16): string {
  return (
    `<svg viewBox="0 0 ${TILE} ${TILE}" width="100%" height="100%" aria-hidden="true" focusable="false" ` +
    `style="display:block;background:${TILE_BG};border-radius:${(radius / TILE) * 100}%">` +
    `<g transform="${ROCKET_TRANSFORM}"><path d="${ROCKET_BODY}" fill="${BODY_FILL}"/><path d="${ROCKET_FLAME}" fill="${FLAME_FILL}"/></g>` +
    "</svg>"
  );
}
