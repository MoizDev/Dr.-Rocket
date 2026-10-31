import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { OSES, SITE, PINNED_KEY, assetName, assetUrl, githubArtifact, validateManifest, verifyProvenance, verifySignature } from '../lib/download-provenance.mjs';
const exec = promisify(execFile);

// Semver precedence, including numeric prerelease identifiers and stable-after-prerelease.
export function compareVersions(a, b) {
  const parts = version => {
    const [core, pre] = version.split('+')[0].split(/-(.*)/s);
    return [core.split('.').map(Number), pre ? pre.split('.') : []];
  };
  const [[ac, ap], [bc, bp]] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) if (ac[i] !== bc[i]) return ac[i] - bc[i];
  if (!ap.length || !bp.length) return bp.length - ap.length;
  for (let i = 0; i < Math.min(ap.length, bp.length); i++) {
    const [x, y] = [ap[i], bp[i]];
    const [xn, yn] = [/^\d+$/.test(x), /^\d+$/.test(y)];
    const cmp = xn && yn ? Number(x) - Number(y) : xn !== yn ? (xn ? -1 : 1) : (x < y ? -1 : x > y ? 1 : 0);
    if (cmp) return cmp;
  }
  return ap.length - bp.length;
}

// TLS authenticates both metadata/digests and downloads; tokens only go to the GitHub API.
export async function httpsDownload(url, destination, token) {
  if (!url.startsWith('https://')) throw Error('HTTPS required');
  const args = ['-fsSL', '--proto', '=https', '--proto-redir', '=https', '--retry', '2', '--connect-timeout', '20', '--max-time', '1800'];
  if (token && new URL(url).origin === 'https://api.github.com') args.push('-H', `Authorization: Bearer ${token}`, '-H', url.includes('/releases/assets/') ? 'Accept: application/octet-stream' : 'Accept: application/vnd.github+json');
  await exec('curl', [...args, '-o', destination, url]);
}

// Injection is an import-only testing interface; the CLI has fixed production origins/key.
export async function fetchDownloads(website, { download = httpsDownload, token = '', publicKey = PINNED_KEY } = {}) {
  const manifest = path.join(website, 'release.json');
  const original = JSON.parse(fs.readFileSync(manifest, 'utf8'));
  validateManifest(original);
  const out = path.join(website, 'public/downloads');
  fs.mkdirSync(path.dirname(out), { recursive: true });
  const stage = fs.mkdtempSync(path.join(path.dirname(out), '.downloads-'));
  const json = async (url, name) => {
    const dest = path.join(stage, name);
    await download(url, dest, token);
    return JSON.parse(fs.readFileSync(dest, 'utf8'));
  };
  try {
    let metadata;
    let release = structuredClone(original);
    try {
      const list = await json(`https://api.github.com/repos/${release.repo}/releases?per_page=20`, 'releases.json');
      const ready = list.filter(x => {
        const candidate = { ...release, version: x.tag_name?.replace(/^v/, '') };
        try { return OSES.every(os => githubArtifact(candidate, x, os)); } catch { return false; }
      }).sort((a, b) => compareVersions(a.tag_name.slice(1), b.tag_name.slice(1)));
      if (!ready.length) throw Error('No complete release with authenticated digests');
      release.version = ready.at(-1).tag_name.slice(1);
    } catch (error) {
      console.warn(`fetch-downloads: release discovery unavailable; retaining ${release.version}: ${error.message}`);
    }
    const metadataUrl = `https://api.github.com/repos/${release.repo}/releases/tags/v${release.version}`;
    try { metadata = await json(metadataUrl, 'release-metadata.json'); } catch { /* signed same-version site fallback below */ }
    let live;
    const receipt = { version: release.version, repo: release.repo, downloads: {} };
    for (const os of OSES) {
      const dest = path.join(stage, release.downloads[os].file);
      let entry = { file: release.downloads[os].file, asset: assetName(release, os), source: 'github', metadataUrl, release: metadata };
      try {
        const asset = githubArtifact(release, metadata, os);
        const url = token ? `https://api.github.com/repos/${release.repo}/releases/assets/${asset.id}` : asset.url;
        await download(url, dest, token);
        verifyProvenance(release, { ...receipt, downloads: { [os]: entry } }, os, fs.readFileSync(dest), publicKey);
      } catch {
        live ??= await json(`${SITE}/version.json`, 'live.json');
        const asset = live.downloads?.[os];
        if (live.version !== release.version || asset?.url !== assetUrl(release, os)) throw Error(`No verified same-version ${os} fallback`);
        verifySignature(release.version, os, asset, publicKey);
        await download(asset.url, dest);
        entry = { file: release.downloads[os].file, asset: assetName(release, os), source: 'signed-site', feed: live };
        verifyProvenance(release, { ...receipt, downloads: { [os]: entry } }, os, fs.readFileSync(dest), publicKey);
      }
      receipt.downloads[os] = entry;
    }
    for (const file of ['releases.json', 'release-metadata.json', 'live.json']) fs.rmSync(path.join(stage, file), { force: true });
    fs.writeFileSync(path.join(stage, '.provenance.json'), JSON.stringify(receipt, null, 2) + '\n');
    fs.rmSync(out, { recursive: true, force: true });
    fs.renameSync(stage, out);
    // The release version advances only after every platform has verified bytes.
    fs.writeFileSync(manifest, JSON.stringify(release, null, 2) + '\n');
    console.log(`Fetched verified mac, windows, linux archives for ${release.version}`);
  } catch (error) {
    // Do not let a later signer/build mistake previous stable-name files for this attempt.
    fs.rmSync(out, { recursive: true, force: true });
    throw error;
  } finally { fs.rmSync(stage, { recursive: true, force: true }); }
}

export async function githubToken() {
  if (process.env.GITHUB_TOKEN) return process.env.GITHUB_TOKEN;
  try { return (await exec('gh', ['auth', 'token'])).stdout.trim(); } catch { return ''; }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await fetchDownloads(fileURLToPath(new URL('../', import.meta.url)), { token: await githubToken() });
}
