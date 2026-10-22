// Run after fetching release downloads: node tools/sign-updates.mjs /outside/repo/release-key.pem
import { createHash, createPrivateKey, createPublicKey, sign } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function signatureMessage(version, os, asset) {
  return `drrocket-update-v1\n${version}\n${os}\n${asset.sha256}\n${asset.url}\n`;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw new Error('Pass the path to an Ed25519 private PEM key outside the repository');
  const key = createPrivateKey(fs.readFileSync(process.argv[2]));
  if (key.asymmetricKeyType !== 'ed25519') throw new Error('An Ed25519 key is required');
  const website = fileURLToPath(new URL('../website/', import.meta.url));
  const release = JSON.parse(fs.readFileSync(path.join(website, 'release.json'), 'utf8'));
  const downloads = {};
  for (const [os, { file }] of Object.entries(release.downloads)) {
    const local = path.join(website, 'public/downloads', file);
    if (!fs.existsSync(local)) continue;
    const asset = {
      url: `https://doctorocket.com/downloads/${file}`,
      sha256: createHash('sha256').update(fs.readFileSync(local)).digest('hex'),
    };
    downloads[os] = { ...asset, signature: sign(null, Buffer.from(signatureMessage(release.version, os, asset)), key).toString('base64') };
  }
  if (Object.keys(downloads).length === 0) throw new Error('No release archives found');
  fs.writeFileSync(path.join(website, 'update-signatures.json'), JSON.stringify({ version: release.version, downloads }, null, 2) + '\n');
  const jwk = createPublicKey(key).export({ format: 'jwk' });
  console.log(`Build with DRROCKET_UPDATE_PUBLIC_KEY=${Buffer.from(jwk.x, 'base64url').toString('base64')}`);
}
