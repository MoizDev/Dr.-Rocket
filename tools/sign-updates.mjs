// Run after fetching release downloads: node tools/sign-updates.mjs /outside/repo/release-key.pem
import { createPrivateKey, createPublicKey, sign } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { signatureMessage, verifiedDownloads, validateManifest, PINNED_KEY } from '../website/lib/download-provenance.mjs';
import { httpsDownload, githubToken } from '../website/scripts/fetch-downloads.mjs';
export { signatureMessage };

async function releaseMetadata(url) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'drrocket-sign-metadata-'));
  try {
    const destination = path.join(dir, 'release.json');
    await httpsDownload(url, destination, await githubToken());
    return JSON.parse(fs.readFileSync(destination, 'utf8'));
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
}

// Imported by fixture tests with ephemeral keys. The CLI below always checks the production pin.
export async function signRelease(website, key, publicKey = PINNED_KEY, metadata = releaseMetadata) {
  const actual = Buffer.from(createPublicKey(key).export({ format: 'jwk' }).x, 'base64url').toString('base64');
  if (actual !== publicKey) throw new Error('Signing key does not match the trusted public key');
  const release = JSON.parse(fs.readFileSync(path.join(website, 'release.json'), 'utf8'));
  validateManifest(release);
  const receipt = JSON.parse(fs.readFileSync(path.join(website, 'public/downloads/.provenance.json'), 'utf8'));
  const entries = Object.values(receipt.downloads ?? {});
  if (entries.some(entry => entry.source === 'github')) {
    // Reauthenticate at signing time: a locally edited receipt cannot bless arbitrary bytes.
    const authenticated = await metadata(`https://api.github.com/repos/${release.repo}/releases/tags/v${release.version}`);
    for (const entry of entries) if (entry.source === 'github') entry.release = authenticated;
  }
  const assets = verifiedDownloads(website, release, null, publicKey, receipt);
  const downloads = Object.fromEntries(Object.entries(assets).map(([os, asset]) => {
    const metadata = { url: asset.url, sha256: asset.sha256 };
    return [os, { ...metadata, signature: sign(null, Buffer.from(signatureMessage(release.version, os, metadata)), key).toString('base64') }];
  }));
  const signed = { version: release.version, downloads };
  const destination = path.join(website, 'update-signatures.json');
  fs.writeFileSync(destination + '.tmp', JSON.stringify(signed, null, 2) + '\n');
  fs.renameSync(destination + '.tmp', destination);
  return signed;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw new Error('Pass the path to an Ed25519 private PEM key outside the repository');
  const key = createPrivateKey(fs.readFileSync(process.argv[2]));
  if (key.asymmetricKeyType !== 'ed25519') throw new Error('An Ed25519 key is required');
  const jwk = createPublicKey(key).export({ format: 'jwk' });
  const pub = Buffer.from(jwk.x, 'base64url').toString('base64');
  const pinned = fs.readFileSync(fileURLToPath(new URL('../release-key.pub', import.meta.url)), 'utf8').trim();
  if (pub !== pinned) throw new Error(`This key's public half (${pub}) is not release-key.pub (${pinned}); the app would reject these signatures`);
  const website = fileURLToPath(new URL('../website/', import.meta.url));
  const signed = await signRelease(website, key);
  console.log(`Signed ${Object.keys(signed.downloads).join(', ')} for ${signed.version}`);
}
