// The release the site offers. The version and file names live in release.json (also read by
// scripts/fetch-downloads.sh and next.config.ts); this module only derives what pages need.
import fs from "node:fs";
import path from "node:path";
import release from "../release.json";

export type Os = "mac" | "windows" | "linux";
export const OSES: Os[] = ["mac", "windows", "linux"];

export type Download = {
  os: Os;
  /** Platform name for labels: "macOS". */
  name: string;
  /** What the file is: "Universal DMG (Apple Silicon + Intel)". */
  kind: string;
  /** Stable URL on this site, e.g. /download/mac (counts, then redirects to the file). */
  href: string;
  /** The release asset's own name, used as the saved file name. */
  filename: string;
  /** "23 MB", when the file is present at build time. */
  size: string | null;
};

export const VERSION: string = release.version;
export const REPO: string = release.repo;

const NAMES: Record<Os, { name: string; kind: string }> = {
  mac: { name: "macOS", kind: "Universal DMG, Apple Silicon and Intel" },
  windows: { name: "Windows", kind: "64-bit zip, no installer" },
  linux: { name: "Linux", kind: "x64 tarball with a desktop entry" },
};

function sizeOf(file: string): string | null {
  try {
    const mb = fs.statSync(path.join(process.cwd(), "public", "downloads", file)).size / 1e6;
    return `${mb < 20 ? Number(mb.toFixed(1)) : Math.round(mb)} MB`;
  } catch {
    return null;
  }
}

export const DOWNLOADS: Record<Os, Download> = Object.fromEntries(
  OSES.map((os) => {
    const d = release.downloads[os];
    return [
      os,
      {
        os,
        ...NAMES[os],
        // Through app/download/[os], which counts the download and redirects to the file.
        href: `/download/${os}`,
        filename: d.asset.replaceAll("{version}", release.version),
        size: sizeOf(d.file),
      },
    ];
  }),
) as Record<Os, Download>;
