// The released version as JSON, for the README's badge (shields.io reads it).
import { VERSION } from "@/lib/release";

export const dynamic = "force-static";

export function GET() {
  return Response.json({ version: VERSION }, { headers: { "Access-Control-Allow-Origin": "*" } });
}
