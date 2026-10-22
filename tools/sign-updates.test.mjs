import assert from 'node:assert/strict';
import { generateKeyPairSync, sign, verify } from 'node:crypto';
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
