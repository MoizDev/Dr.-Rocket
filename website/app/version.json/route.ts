// The released version and its files as JSON: the README's badge reads the version (through
// shields.io), and the app's updater (src/update.rs) reads the files and their checksums.
import { updateFeed } from "@/lib/release";

export const dynamic = "force-static";

export function GET() {
  return Response.json(updateFeed(), { headers: { "Access-Control-Allow-Origin": "*" } });
}
