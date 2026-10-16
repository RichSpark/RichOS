const utf8 = new TextEncoder();
export const hex = bytes => Array.from(new Uint8Array(bytes), b => b.toString(16).padStart(2, '0')).join('');
export const digest = async value => hex(await crypto.subtle.digest('SHA-256', typeof value === 'string' ? utf8.encode(value) : value));
export function decode(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_-]+$/.test(value)) throw Error('invalid_encoding');
  return Uint8Array.from(atob(value.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));
}
export const encode = bytes => btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
export async function signingInput(time, nonce, method, path, body) {
  return `RICHOS-CONNECT-V1\n${time}\n${nonce}\n${method}\n${path}\n${await digest(body)}`;
}
export async function authenticate(request, body, now = Date.now()) {
  try {
    const key = request.headers.get('x-richos-key');
    const time = request.headers.get('x-richos-time');
    const nonce = request.headers.get('x-richos-nonce');
    if (!/^\d{13}$/.test(time) || Math.abs(now - Number(time)) > 60_000 || !/^[a-f0-9]{32}$/.test(nonce)) return null;
    if (key?.length !== 87) return null;
    const point = decode(key), signature = decode(request.headers.get('x-richos-signature'));
    if (point.length !== 65 || point[0] !== 4 || signature.length !== 64) return null;
    const publicKey = await crypto.subtle.importKey('raw', point, { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify']);
    const url = new URL(request.url);
    const input = await signingInput(time, nonce, request.method, url.pathname + url.search, body);
    if (!await crypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, publicKey, signature, utf8.encode(input))) return null;
    return { id: (await digest(point)).slice(0, 32), key, nonce };
  } catch { return null; }
}

export async function readBody(request) {
  if (Number(request.headers.get('content-length') || 0) > 8192) throw Error('body_too_large');
  if (!request.body) return '';
  const reader = request.body.getReader();
  const parts = []; let size = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 8192) { await reader.cancel(); throw Error('body_too_large'); }
      parts.push(value);
    }
  } finally { reader.releaseLock(); }
  const buffer = new Uint8Array(size); let offset = 0;
  for (const part of parts) { buffer.set(part, offset); offset += part.byteLength; }
  return new TextDecoder('utf-8', { fatal: true }).decode(buffer);
}
