import type { NextConfig } from "next";
import release from "./release.json";

// Downloads are served from public/downloads under stable names (see scripts/fetch-downloads.sh).
// They save under the release asset's own versioned name, and browsers recheck them hourly
// since the same URL gets a new build with each release.
const downloadHeaders = Object.values(release.downloads).map(({ file, asset }) => ({
  source: `/downloads/${file}`,
  headers: [
    {
      key: "Content-Disposition",
      value: `attachment; filename="${asset.replaceAll("{version}", release.version)}"`,
    },
    { key: "Cache-Control", value: "public, max-age=3600, must-revalidate" },
    { key: "X-Content-Type-Options", value: "nosniff" },
  ],
}));

const nextConfig: NextConfig = {
  poweredByHeader: false,
  images: {
    formats: ["image/avif", "image/webp"],
    qualities: [85],
    deviceSizes: [640, 828, 1080, 1280, 1600, 2048, 2400],
  },
  async headers() {
    return downloadHeaders;
  },
};

export default nextConfig;
