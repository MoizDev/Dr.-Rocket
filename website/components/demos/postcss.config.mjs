// Demo CSS is written in the app's own points, as px. This turns every px into a multiple of
// --u (one app point at the demo's current size), so a demo scales exactly like the screenshot
// it replaced. Only files in this folder use this config; the page's own CSS is untouched.
// Write `1PX` (upper case) for a length that must not scale, such as a hairline.
const demoUnits = () => ({
  postcssPlugin: "demo-units",
  Declaration(decl) {
    if (decl.prop.startsWith("--u") || !/\dpx/.test(decl.value)) return;
    decl.value = decl.value.replace(/(-?\d*\.?\d+)px\b/g, (_, n) => `calc(${n} * var(--u))`);
  },
});
demoUnits.postcss = true;

const config = { plugins: [demoUnits] };
export default config;
