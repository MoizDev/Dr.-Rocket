import { createHmac } from "node:crypto";
import { isIP } from "node:net";

/** Trust forwarding headers only on Vercel, which overwrites X-Forwarded-For. */
export function clientHash(headers: Headers, key: string, onVercel: boolean, day: string): string | null {
  if (!onVercel) return null;
  const ip = headers.get("x-forwarded-for")?.trim();
  if (!ip || !isIP(ip)) return null;
  return createHmac("sha256", key).update(`download-counter\n${day}\n${ip}`).digest("hex");
}

export async function recordDownload(os: string, version: string, headers: Headers) {
  const base = process.env.SUPABASE_URL;
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!base || !key) return;
  const hash = clientHash(headers, key, process.env.VERCEL === "1", new Date().toISOString().slice(0, 10));
  if (!hash) return;
  const country = headers.get("x-vercel-ip-country");
  try {
    const res = await fetch(`${base}/rest/v1/rpc/record_download`, {
      method: "POST",
      headers: { apikey: key, Authorization: `Bearer ${key}`, "content-type": "application/json" },
      body: JSON.stringify({ p_os: os, p_version: version, p_country: country && /^[A-Z]{2}$/.test(country) ? country : null, p_client_hash: hash }),
      signal: AbortSignal.timeout(5000),
    });
    if (!res.ok) console.error(`download count failed: ${res.status}`);
  } catch {
    console.error("download count unavailable");
  }
}
