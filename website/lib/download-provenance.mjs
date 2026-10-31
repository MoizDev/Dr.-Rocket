// Local receipts are created only after authenticated HTTPS acquisition. They are build inputs,
// not a second signing authority: the site additionally requires the pinned release signature.
import fs from 'node:fs';
import path from 'node:path';
import { createHash, createPublicKey, verify } from 'node:crypto';

export const OSES = ['mac', 'windows', 'linux'];
export const SITE = 'https://doctorocket.com';
// Same unchanged public pin as release-key.pub; parity is enforced by the offline tests.
// Kept inside the website root so Next can bundle the verifier without external file tracing.
export const PINNED_KEY = 'QwG8i6V49rq9xf4ArnewH/ICKtsA/yez7FDoykUA91Q=';
export const hash = bytes => createHash('sha256').update(bytes).digest('hex');
export const signatureMessage = (version, os, asset) => `drrocket-update-v1\n${version}\n${os}\n${asset.sha256}\n${asset.url}\n`;
export const assetUrl = (release, os) => `${SITE}/downloads/${release.downloads[os].file}`;
export const assetName = (release, os) => release.downloads[os].asset.replaceAll('{version}', release.version);

export function validateManifest(release) {
  if (!/^\d+\.\d+\.\d+(?:-[\da-zA-Z.-]+)?$/.test(release.version) || !/^[\w.-]+\/[\w.-]+$/.test(release.repo)) throw Error('Invalid release identity');
  const files = new Set();
  for (const os of OSES) {
    const { file, asset } = release.downloads[os];
    if (!/^[\w.-]+$/.test(file) || !/^[\w.{}-]+$/.test(asset) || !asset.includes('{version}') || files.has(file)) throw Error(`Invalid ${os} artifact name`);
    files.add(file);
  }
}

export function verifySignature(version, os, asset, publicKey = PINNED_KEY) {
  if (!asset || !/^[a-f0-9]{64}$/.test(asset.sha256) || !asset.url?.startsWith('https://') || /[\x00-\x20]/.test(asset.url) || !/^[A-Za-z0-9+/]{86}==$/.test(asset.signature ?? '')) throw Error(`Invalid ${os} signed metadata`);
  const key = createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x: Buffer.from(publicKey, 'base64').toString('base64url') }, format: 'jwk' });
  if (!verify(null, Buffer.from(signatureMessage(version, os, asset)), key, Buffer.from(asset.signature, 'base64'))) throw Error(`Untrusted ${os} signature`);
}

export function githubArtifact(release, metadata, os) {
  validateManifest(release);
  if (metadata.draft !== false || metadata.tag_name !== `v${release.version}` || !Number.isSafeInteger(metadata.id) || metadata.id <= 0) throw Error('Release metadata identity mismatch');
  const matches = metadata.assets.filter(a => a.name === assetName(release, os));
  const asset = matches[0];
  if (matches.length !== 1 || !Number.isSafeInteger(asset.id) || asset.id <= 0 || !Number.isSafeInteger(asset.size) || asset.size <= 0 || !/^sha256:[a-f0-9]{64}$/.test(asset.digest)) throw Error(`Missing authenticated ${os} asset digest/identity`);
  const expected = `https://github.com/${release.repo}/releases/download/v${release.version}/${assetName(release, os)}`;
  if (asset.browser_download_url !== expected) throw Error(`Unexpected ${os} release URL`);
  return { sha256: asset.digest.slice(7), size: asset.size, url: expected, id: asset.id };
}

export function verifyProvenance(release, receipt, os, bytes, publicKey = PINNED_KEY) {
  validateManifest(release);
  if (receipt.version !== release.version || receipt.repo !== release.repo) throw Error('Download provenance version/repo mismatch');
  const entry = receipt.downloads?.[os];
  if (!entry || entry.file !== release.downloads[os].file || entry.asset !== assetName(release, os)) throw Error(`Missing ${os} download provenance`);
  let expected;
  if (entry.source === 'github') {
    if (entry.metadataUrl !== `https://api.github.com/repos/${release.repo}/releases/tags/v${release.version}`) throw Error('Unexpected release metadata origin');
    expected = githubArtifact(release, entry.release, os);
  } else if (entry.source === 'signed-site') {
    if (entry.feed.version !== release.version) throw Error('Fallback release version mismatch');
    expected = entry.feed.downloads[os];
    if (expected.url !== assetUrl(release, os)) throw Error('Unexpected fallback artifact URL');
    verifySignature(release.version, os, expected, publicKey);
  } else throw Error(`Untrusted ${os} provenance source`);
  if (hash(bytes) !== expected.sha256 || (expected.size !== undefined && bytes.length !== expected.size)) throw Error(`${os} bytes do not match release provenance`);
}

export function verifiedDownloads(website, release, signedRelease, publicKey = PINNED_KEY, authenticatedReceipt) {
  validateManifest(release);
  const dir = path.join(website, 'public/downloads');
  const receipt = authenticatedReceipt ?? JSON.parse(fs.readFileSync(path.join(dir, '.provenance.json'), 'utf8'));
  const downloads = {};
  if (signedRelease && signedRelease.version !== release.version) throw Error('Release signatures version mismatch');
  for (const os of OSES) {
    const local = path.join(dir, release.downloads[os].file);
    if (!fs.lstatSync(local).isFile()) throw Error(`Invalid ${os} archive`);
    const bytes = fs.readFileSync(local);
    verifyProvenance(release, receipt, os, bytes, publicKey);
    const asset = { url: assetUrl(release, os), sha256: hash(bytes), size: bytes.length };
    if (signedRelease) {
      const signed = signedRelease.downloads[os];
      if (!signed || signed.url !== asset.url || signed.sha256 !== asset.sha256) throw Error(`${os} signature does not match release bytes`);
      verifySignature(release.version, os, signed, publicKey);
      asset.signature = signed.signature;
    }
    downloads[os] = asset;
  }
  return downloads;
}
