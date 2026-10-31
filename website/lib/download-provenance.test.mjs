import assert from 'node:assert/strict';
import { test } from 'node:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { generateKeyPairSync, sign } from 'node:crypto';
import { fetchDownloads, compareVersions } from '../scripts/fetch-downloads.mjs';
import { OSES, SITE, PINNED_KEY, hash, signatureMessage, verifiedDownloads, verifySignature } from './download-provenance.mjs';
import { signRelease } from '../../tools/sign-updates.mjs';

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'drrocket-provenance-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const release = { version: '1.2.3', repo: 'example/rocket', downloads: Object.fromEntries(OSES.map(os => [os, { file: `${os}.zip`, asset: `${os}-{version}.zip` }])) };
  fs.writeFileSync(path.join(root, 'release.json'), JSON.stringify(release));
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const key = Buffer.from(publicKey.export({ format: 'jwk' }).x, 'base64url').toString('base64');
  const bytes = Object.fromEntries(OSES.map(os => [os, Buffer.from(`archive-${os}-1.2.3`)]));
  const metadata = { tag_name: 'v1.2.3', id: 1, draft: false, assets: OSES.map((os, i) => ({ id: i + 10, name: `${os}-1.2.3.zip`, digest: `sha256:${hash(bytes[os])}`, size: bytes[os].length, browser_download_url: `https://github.com/example/rocket/releases/download/v1.2.3/${os}-1.2.3.zip` })) };
  const feed = { version: release.version, downloads: Object.fromEntries(OSES.map(os => {
    const asset = { url: `${SITE}/downloads/${os}.zip`, sha256: hash(bytes[os]), size: bytes[os].length };
    return [os, { ...asset, signature: sign(null, Buffer.from(signatureMessage(release.version, os, asset)), privateKey).toString('base64') }];
  })) };
  const download = async (url, dest) => {
    let value;
    if (url.includes('releases?')) value = JSON.stringify([metadata]);
    else if (url.includes('/tags/')) value = JSON.stringify(metadata);
    else if (url.endsWith('/version.json')) value = JSON.stringify(feed);
    else {
      const platform = OSES.find(os => url.endsWith(`/${os}.zip`) || url.endsWith(`/${os}-1.2.3.zip`));
      assert(platform, `Unexpected fake request ${url}`);
      value = bytes[platform];
    }
    fs.writeFileSync(dest, value);
  };
  return { root, release, privateKey, key, bytes, metadata, feed, download };
}

test('complete authenticated release can be signed and offered; tampered bytes fail both boundaries', async t => {
  const f = fixture(t);
  await fetchDownloads(f.root, { download: f.download });
  const signed = await signRelease(f.root, f.privateKey, f.key, async () => structuredClone(f.metadata));
  assert.deepEqual(Object.keys(verifiedDownloads(f.root, f.release, signed, f.key)), OSES);
  const signaturesBefore = fs.readFileSync(path.join(f.root, 'update-signatures.json'));
  fs.writeFileSync(path.join(f.root, 'public/downloads/linux.zip'), 'previous version');
  await assert.rejects(() => signRelease(f.root, f.privateKey, f.key, async () => structuredClone(f.metadata)), /provenance/);
  assert.deepEqual(fs.readFileSync(path.join(f.root, 'update-signatures.json')), signaturesBefore);
  assert.throws(() => verifiedDownloads(f.root, f.release, signed, f.key), /provenance/);
});

test('failed Linux fetch cannot leave an old file to promote or sign; manifest does not advance', async t => {
  const f = fixture(t);
  await fetchDownloads(f.root, { download: f.download });
  const old = { ...f.release, version: '1.2.2' };
  fs.writeFileSync(path.join(f.root, 'release.json'), JSON.stringify(old));
  const download = async (url, dest) => {
    if (url.includes('linux') || url.endsWith('version.json')) throw Error('offline');
    return f.download(url, dest);
  };
  await assert.rejects(fetchDownloads(f.root, { download }), /offline/);
  assert(!fs.existsSync(path.join(f.root, 'public/downloads')));
  assert.equal(JSON.parse(fs.readFileSync(path.join(f.root, 'release.json'))).version, '1.2.2');
  await assert.rejects(() => signRelease(f.root, f.privateKey, f.key, async () => structuredClone(f.metadata)));
});

test('private repository fallback requires signed metadata, exact version and verified bytes', async t => {
  const f = fixture(t);
  const download = async (url, dest) => {
    if (url.startsWith('https://api.github.com') || url.startsWith('https://github.com')) throw Error('private');
    return f.download(url, dest);
  };
  await fetchDownloads(f.root, { download, publicKey: f.key });
  assert.deepEqual(Object.keys(verifiedDownloads(f.root, f.release, f.feed, f.key)), OSES);
  const resigned = await signRelease(f.root, f.privateKey, f.key, async () => { throw Error('signed-site fallback must not need GitHub'); });
  assert.deepEqual(Object.keys(verifiedDownloads(f.root, f.release, resigned, f.key)), OSES);
  for (const mutate of [
    () => { f.feed.version = '1.2.4'; },
    () => { f.feed.version = '1.2.3'; f.feed.downloads.mac.signature = ''; },
  ]) {
    mutate();
    await assert.rejects(fetchDownloads(f.root, { download, publicKey: f.key }));
    assert(!fs.existsSync(path.join(f.root, 'public/downloads')));
  }
});

test('missing digests, missing platforms, digest mismatch and swapped platform bytes fail closed', async t => {
  for (const mode of ['digest', 'platform', 'mismatch', 'swapped']) {
    const f = fixture(t);
    if (mode === 'digest') delete f.metadata.assets[2].digest;
    if (mode === 'platform') f.metadata.assets.pop();
    if (mode === 'mismatch') f.metadata.assets[2].digest = `sha256:${'0'.repeat(64)}`;
    if (mode === 'swapped') f.bytes.linux = f.bytes.windows;
    const download = async (url, dest) => {
      if (url.endsWith('/version.json')) throw Error('no fallback');
      return f.download(url, dest);
    };
    await assert.rejects(fetchDownloads(f.root, { download }));
    await assert.rejects(() => signRelease(f.root, f.privateKey, f.key, async () => structuredClone(f.metadata)));
  }
});

test('missing files, wrong versions, missing signatures and wrong keys cannot produce an offered feed', async t => {
  const f = fixture(t);
  await fetchDownloads(f.root, { download: f.download });
  assert.throws(() => verifiedDownloads(f.root, { ...f.release, version: '1.2.4' }, f.feed, f.key));
  assert.throws(() => verifiedDownloads(f.root, f.release, f.feed)); // production pin rejects ephemeral key
  const signed = structuredClone(f.feed);
  delete signed.downloads.linux;
  assert.throws(() => verifiedDownloads(f.root, f.release, signed, f.key));
  f.feed.downloads.mac.url = 'file:///tmp/archive';
  assert.throws(() => verifySignature('1.2.3', 'mac', f.feed.downloads.mac, f.key));
  fs.rmSync(path.join(f.root, 'public/downloads/windows.zip'));
  await assert.rejects(() => signRelease(f.root, f.privateKey, f.key, async () => structuredClone(f.metadata)));
});


test('website trust pin equals the unchanged app release public key', () => {
  assert.equal(PINNED_KEY, fs.readFileSync(new URL('../../release-key.pub', import.meta.url), 'utf8').trim());
});


test('release selection preserves semver precedence rather than lexical tag ordering', () => {
  for (const [older, newer] of [['1.0.0-beta.9', '1.0.0-beta.10'], ['1.0.0-beta.10', '1.0.0'], ['1.0.0-2', '1.0.0-1a'], ['1.0.0-alpha', '1.0.0-alpha.1']]) {
    assert(compareVersions(older, newer) < 0);
    assert(compareVersions(newer, older) > 0);
  }
});


test('signing reauthenticates GitHub metadata rather than trusting a forged local receipt', async t => {
  const f = fixture(t);
  await fetchDownloads(f.root, { download: f.download });
  const local = path.join(f.root, 'public/downloads');
  const receiptPath = path.join(local, '.provenance.json');
  const receipt = JSON.parse(fs.readFileSync(receiptPath));
  const stale = Buffer.from('stale Linux archive');
  fs.writeFileSync(path.join(local, 'linux.zip'), stale);
  const asset = receipt.downloads.linux.release.assets.find(a => a.name.startsWith('linux'));
  asset.digest = `sha256:${hash(stale)}`;
  asset.size = stale.length;
  fs.writeFileSync(receiptPath, JSON.stringify(receipt));
  await assert.rejects(signRelease(f.root, f.privateKey, f.key, async () => structuredClone(f.metadata)), /provenance/);
  await assert.rejects(signRelease(f.root, f.privateKey, f.key, async () => { throw Error('metadata unavailable'); }), /metadata unavailable/);
  assert(!fs.existsSync(path.join(f.root, 'update-signatures.json')));
});
