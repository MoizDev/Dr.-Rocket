// Download links point here: count the download, then send the browser to the file.
// Imports release.json rather than lib/release.ts, which reads public/ from disk.
import { after, type NextRequest } from "next/server";
import release from "@/release.json";

type Os = keyof typeof release.downloads;

// Link unfurlers and crawlers fetch links without downloading anything.
const BOT = /bot|crawl|spider|slurp|preview|facebookexternalhit|embedly|whatsapp|telegram|discord|slack/i;

function target(os: string, request: NextRequest) {
  if (!Object.hasOwn(release.downloads, os)) return null;
  return new URL(`/downloads/${release.downloads[os as Os].file}`, request.url);
}

export async function GET(request: NextRequest, { params }: { params: Promise<{ os: string }> }) {
  const { os } = await params;
  const url = target(os, request);
  if (!url) return new Response("Not found", { status: 404 });
  const agent = request.headers.get("user-agent") ?? "";
  const prefetch = request.headers.get("purpose") === "prefetch" || request.headers.has("next-router-prefetch");
  if (!BOT.test(agent) && !prefetch) {
    const country = request.headers.get("x-vercel-ip-country");
    // After the response: the download never waits on the count.
    after(() => record(os, country));
  }
  return Response.redirect(url, 302);
}

export async function HEAD(request: NextRequest, { params }: { params: Promise<{ os: string }> }) {
  const url = target((await params).os, request);
  return url ? Response.redirect(url, 302) : new Response(null, { status: 404 });
}

async function record(os: string, country: string | null) {
  const base = process.env.SUPABASE_URL;
  const key = process.env.SUPABASE_PUBLISHABLE_KEY;
  if (!base || !key) return;
  try {
    const res = await fetch(`${base}/rest/v1/rpc/record_download`, {
      method: "POST",
      headers: { apikey: key, "content-type": "application/json" },
      body: JSON.stringify({ p_os: os, p_version: release.version, p_country: country }),
      signal: AbortSignal.timeout(5000),
    });
    if (!res.ok) console.error(`download count failed: ${res.status} ${await res.text()}`);
  } catch (e) {
    console.error("download count failed:", e);
  }
}
