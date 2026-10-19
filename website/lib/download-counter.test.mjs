import assert from "node:assert/strict";
import { clientHash, recordDownload } from "./download-counter.ts";
const headers = new Headers({ "x-forwarded-for": "203.0.113.7" });
const hash = clientHash(headers, "server-secret", true, "2026-10-19");
assert.match(hash, /^[a-f0-9]{64}$/);
assert.equal(hash, clientHash(headers, "server-secret", true, "2026-10-19"));
assert.notEqual(hash, clientHash(headers, "server-secret", true, "2026-10-20"));
assert.notEqual(hash, clientHash(headers, "another-secret", true, "2026-10-19"));
assert.equal(clientHash(headers, "server-secret", false, "2026-10-19"), null);
for (const ip of ["", "forged", "203.0.113.7, 198.51.100.8"]) {
  assert.equal(clientHash(new Headers({ "x-forwarded-for": ip }), "key", true, "2026-10-19"), null);
}
delete process.env.SUPABASE_SERVICE_ROLE_KEY;
let fetched = false;
globalThis.fetch = async () => { fetched = true; return new Response(); };
await recordDownload("mac", "1.0.0", headers);
assert.equal(fetched, false, "Missing server credentials must not use an anonymous fallback");
console.log("Download counter boundary checks passed");
