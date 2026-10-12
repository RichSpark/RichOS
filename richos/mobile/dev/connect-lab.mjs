import { validateStorage } from '../cli/machine-settings.mjs';
// Operator-only isolated test host. Private keys and scoped tokens stay in SSD scratch.
import { webcrypto, createHash } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync, mkdirSync, realpathSync, rmSync } from 'node:fs';
import { join } from 'node:path';
const origin = 'https://connect.richos.ceo';
export async function lab(action, directory, transport = fetch) {
  if (!directory) throw Error('Connect lab requires an explicit directory');
  directory = validateStorage(directory);
  mkdirSync(directory, { recursive: true, mode: 0o700 });

  const file = join(directory, 'host-identity.json');
  if (action === 'lab-identity' && !existsSync(file)) {
    const pair = await webcrypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, ['sign','verify']);
    const privateKey = await webcrypto.subtle.exportKey('jwk', pair.privateKey);
    const raw = Buffer.from(await webcrypto.subtle.exportKey('raw', pair.publicKey));
    writeFileSync(file, JSON.stringify({ privateKey, key: raw.toString('base64url'), id: createHash('sha256').update(raw).digest('hex').slice(0,32) }), { mode: 0o600, flag: 'wx' });
  }
  const identity = JSON.parse(readFileSync(file));
  if (action === 'lab-identity') return { id: identity.id, publicKey: identity.key };
  const key = await webcrypto.subtle.importKey('jwk', identity.privateKey, { name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign']);
  async function call(method, path, body = '') {
    const time = Date.now(), nonce = Buffer.from(webcrypto.getRandomValues(new Uint8Array(16))).toString('hex');
    const canonical = `RICHOS-CONNECT-V1\n${time}\n${nonce}\n${method}\n${path}\n${createHash('sha256').update(body).digest('hex')}`;
    const signature = Buffer.from(await webcrypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, key, Buffer.from(canonical))).toString('base64url');
    const response = await transport(origin + path, { method, redirect: 'error', signal: AbortSignal.timeout(30000), headers: {
      'x-richos-key': identity.key, 'x-richos-time': String(time), 'x-richos-nonce': nonce, 'x-richos-signature': signature,
      'Content-Type': 'application/json',
    }, ...(method === 'GET' ? {} : { body }) });
    const value = await response.json();
    if (!response.ok) throw Error(`Connect ${method} ${path}: ${response.status} ${value.error || 'unavailable'}`);
    return value;
  }
  if (action === 'lab-status') return call('GET','/v1/host');
  if (action === 'lab-disable') {
    const allocation = await call('DELETE','/v1/host','{}');
    writeFileSync(join(directory,'allocation.json'),JSON.stringify(allocation),{mode:0o600});
    rmSync(join(directory,'tunnel-token'),{force:true});
    return allocation;
  }
  if (action !== 'lab-enable') throw Error('Expected lab-identity, lab-enable, lab-status or lab-disable');
  const allocation = await call('POST','/v1/hosts','{}');
  const reply = await call('POST','/v1/host/token','{}');
  if (reply.id !== identity.id || reply.generation !== allocation.generation || typeof reply.token !== 'string') throw Error('Unexpected Connect allocation');
  writeFileSync(join(directory,'tunnel-token'), reply.token, { mode: 0o600 });
  writeFileSync(join(directory,'allocation.json'), JSON.stringify(allocation), { mode: 0o600 });
  return allocation;
}
