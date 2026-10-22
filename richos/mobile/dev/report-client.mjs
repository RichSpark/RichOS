// A minimal protocol-2 client for the relay's own tests (workerd and live): it builds the approved
// payload exactly as RICHDOCTOR-RELAY.md specifies and uploads screenshots, optionally throttled.
// It is a test client, not RichDoctor.
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { attachmentLines, canonicalManifest } from '../service/report/checks.mjs';
import { checkImage } from '../service/report/images.mjs';

const ALPHABET = '23456789ABCDEFGHJKMNPQRSTVWXYZ';
export const reportId = () => 'RD-' + [4, 4].map(n => Array.from(randomBytes(n), b => ALPHABET[b % 30]).join('')).join('-');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');

// files: [{ bytes, type }]. Returns the register body plus `files` (kept apart from the request).
export function screenshotReport({ origin, files, id = reportId(), title = 'RichDoctor relay test: harmless', lines = ['### What happened', 'An automated relay test\\. Nothing is broken\\.'] }) {
  const manifest = files.map(({ bytes, type }, i) => {
    const size = checkImage(bytes, type);
    if (size.error) throw Error(`fixture ${i + 1}: ${size.error}`);
    return { attachment_id: String(i + 1), order: i + 1, filename: `screenshot-${i + 1}.${type === 'image/png' ? 'png' : 'jpg'}`, media_type: type,
      width: size.width, height: size.height, bytes: bytes.length, sha256: sha(bytes) };
  });
  const body = [...lines, '', ...attachmentLines(origin, id, files.length), `Sent with RichDoctor 1.0 · Report ID ${id}`].join('\n');
  return { schema: 'richdoctor-report/2', report_id: id, receipt_secret: randomBytes(32).toString('base64url'),
    content_sha256: sha(Buffer.from(title + '\n\n' + body + '\n\n' + canonicalManifest(manifest))), installation_id: randomUUID(),
    app_version: '1.0.0', approved_at: Date.now(), title, body, manifest, files: files.map(f => f.bytes) };
}
export const registerBody = payload => { const { files, ...rest } = payload; return JSON.stringify(rest); };

// A body that arrives at `bitsPerSecond`, in 100 ms slices, and can be cut off after `stopAfter` bytes.
export function throttled(bytes, bitsPerSecond, { stopAfter = Infinity } = {}) {
  const slice = Math.max(1, Math.round(bitsPerSecond / 8 / 10));
  let sent = 0;
  return new ReadableStream({
    async pull(controller) {
      if (sent >= bytes.length) { controller.close(); return; }
      if (sent >= stopAfter) { controller.error(new Error('connection lost')); return; }
      await new Promise(resolve => setTimeout(resolve, 100));
      const part = bytes.subarray(sent, Math.min(sent + slice, bytes.length, stopAfter));
      sent += part.length; controller.enqueue(new Uint8Array(part));
    },
  });
}
export function uploadInit(payload, n, body = payload.files[n - 1], init = {}) {
  const item = payload.manifest[n - 1];
  return { method: 'PUT', body, ...(body instanceof ReadableStream ? { duplex: 'half' } : {}), ...init,
    headers: { 'Content-Type': item.media_type, 'X-Receipt-Secret': payload.receipt_secret, 'X-Content-SHA256': item.sha256, ...(init.headers || {}) } };
}
