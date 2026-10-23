// Regenerates the Visualizer trace and Debugger pauses from bst.rkt's text. Run with
// `npm run gen:demos` after changing the BST file in lib/demos/data/files.ts, and commit the JSON.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { walk } from "../lib/demos/bst-walk.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
const files = readFileSync(`${root}lib/demos/data/files.ts`, "utf8");
const m = /export const BST = `([^`]*)`;/.exec(files);
if (!m) throw new Error("BST not found in files.ts");
const { trace, pauses, result } = walk(m[1]);
writeFileSync(`${root}lib/demos/data/visualizer-trace.json`, JSON.stringify(trace) + "\n");
writeFileSync(`${root}lib/demos/data/debugger-pauses.json`, JSON.stringify(pauses) + "\n");
console.log(`${trace.length} steps, ${pauses.length} pauses, result ${result}`);
