import assert from 'node:assert/strict';
import { generateKeyPairSync, sign, verify } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { signatureMessage } from './sign-updates.mjs';
const { privateKey, publicKey } = generateKeyPairSync('ed25519');
const asset = { url: 'https://doctorocket.com/downloads/test.zip', sha256: 'a'.repeat(64) };
const message = Buffer.from(signatureMessage('1.2.3', 'windows', asset));
assert.equal(message.toString(), `drrocket-update-v1\n1.2.3\nwindows\n${'a'.repeat(64)}\n${asset.url}\n`);
const signature = sign(null, message, privateKey);
assert(verify(null, message, publicKey, signature));
for (const altered of [signatureMessage('1.2.4', 'windows', asset), signatureMessage('1.2.3', 'linux', asset), signatureMessage('1.2.3', 'windows', { ...asset, sha256: 'b'.repeat(64) })]) {
  assert(!verify(null, Buffer.from(altered), publicKey, signature));
}
console.log('Release signature checks passed');

// The CLI must reject an ephemeral key before reading/signing any production artifacts.
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'drrocket-sign-reject-'));
try {
  const key = path.join(scratch, 'ephemeral.pem');
  fs.writeFileSync(key, privateKey.export({ type: 'pkcs8', format: 'pem' }), { mode: 0o600 });
  assert.throws(() => execFileSync(process.execPath, [fileURLToPath(new URL('./sign-updates.mjs', import.meta.url)), key], { stdio: 'pipe' }), error => /not release-key.pub/.test(error.stderr.toString()));
} finally { fs.rmSync(scratch, { recursive: true, force: true }); }
