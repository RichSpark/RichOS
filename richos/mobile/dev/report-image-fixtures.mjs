// Writes the relay's golden screenshot fixtures (profile v1) into service/report/fixtures/relay/.
//   node richos/mobile/dev/report-image-fixtures.mjs <any baseline JPEG>
// PNGs are built here; the JPEGs are cut from the given JPEG down to the profile's segments, then
// each refused variant adds exactly one forbidden thing. The files are committed; this is only how
// they were made, so they can be remade.
import { writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';

const out = fileURLToPath(new URL('../service/report/fixtures/relay/', import.meta.url));
const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xEDB88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
const crc = bytes => { let c = 0xFFFFFFFF; for (const b of bytes) c = crcTable[(c ^ b) & 255] ^ (c >>> 8); return (c ^ 0xFFFFFFFF) >>> 0; };
const be32 = n => Buffer.from([n >>> 24, (n >>> 16) & 255, (n >>> 8) & 255, n & 255]);
function chunk(type, data = Buffer.alloc(0)) {
  const body = Buffer.concat([Buffer.from(type, 'latin1'), data]);
  return Buffer.concat([be32(data.length), body, be32(crc(body))]);
}
function png(width, height, { color = 2, extra = [], trailing = null, end = true } = {}) {
  const channels = color === 2 ? 3 : 1, rows = [];
  for (let y = 0; y < height; y++) {
    const row = [0];
    for (let x = 0; x < width; x++) row.push(...(color === 2 ? [x * 16 & 255, y * 20 & 255, 128] : [(x + y) % 4]));
    rows.push(...row);
  }
  const ihdr = Buffer.concat([be32(width), be32(height), Buffer.from([8, color, 0, 0, 0])]);
  const parts = [Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr)];
  if (color === 3) parts.push(chunk('PLTE', Buffer.from([0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255])), chunk('tRNS', Buffer.from([0])));
  parts.push(chunk('sRGB', Buffer.from([0])), ...extra, chunk('IDAT', deflateSync(Buffer.from(rows))));
  if (end) parts.push(chunk('IEND'));
  if (trailing) parts.push(trailing);
  void channels;
  return Buffer.concat(parts);
}

// JPEG marker segments: [{ marker, bytes }] where bytes is the whole segment, scan data included.
function segments(jpg) {
  const list = []; let pos = 2;
  while (pos < jpg.length) {
    while (jpg[pos] === 0xFF && jpg[pos + 1] === 0xFF) pos++;
    const marker = jpg[pos + 1];
    if (marker === 0xD9) { list.push({ marker, bytes: jpg.subarray(pos, pos + 2) }); break; }
    const end = pos + 2 + jpg.readUInt16BE(pos + 2);
    let stop = end;
    if (marker === 0xDA) { while (stop < jpg.length && !(jpg[stop] === 0xFF && jpg[stop + 1] !== 0 && !(jpg[stop + 1] >= 0xD0 && jpg[stop + 1] <= 0xD7))) stop++; }
    list.push({ marker, bytes: jpg.subarray(pos, stop) });
    pos = stop;
  }
  return list;
}
const segment = (marker, data) => Buffer.concat([Buffer.from([0xFF, marker]), Buffer.from([(data.length + 2) >> 8, (data.length + 2) & 255]), data]);
const JFIF = segment(0xE0, Buffer.from([0x4A, 0x46, 0x49, 0x46, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0]));
const KEEP = new Set([0xC0, 0xC1, 0xC2, 0xC4, 0xDB, 0xDD, 0xDA, 0xD9]);

// The largest screenshot the profile allows in practice: a valid PNG just under 5 MiB, made on
// demand (never committed) for the slow-upload proof.
export function largePng(width = 1024, height = 1700) {
  const raw = Buffer.alloc(height * (1 + width * 3));
  let x = 12345;
  for (let i = 0; i < raw.length; i++) { if (i % (1 + width * 3) === 0) { raw[i] = 0; continue; } x = (x * 1103515245 + 12345) & 0x7fffffff; raw[i] = x >> 16; }
  const ihdr = Buffer.concat([be32(width), be32(height), Buffer.from([8, 2, 0, 0, 0])]);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw, { level: 0 })), chunk('IEND')]);
}

export function build(sourceJpeg) {
  mkdirSync(out, { recursive: true });
  const files = {
    'accept-rgb.png': png(16, 12),
    'accept-palette.png': png(8, 8, { color: 3 }),
    'reject-text.png': png(16, 12, { extra: [chunk('tEXt', Buffer.from('Author\0Jane Example', 'latin1'))] }),
    'reject-exif.png': png(16, 12, { extra: [chunk('eXIf', Buffer.from('MM\0*\0\0\0\x08\0\0', 'latin1'))] }),
    'reject-iccp.png': png(16, 12, { extra: [chunk('iCCP', Buffer.concat([Buffer.from('icc\0\0', 'latin1'), deflateSync(Buffer.alloc(16))]))] }),
    'reject-trailing.png': png(16, 12, { trailing: Buffer.from('private trailing bytes') }),
    'reject-cut.png': png(16, 12, { end: false }),
  };
  const kept = segments(readFileSync(sourceJpeg)).filter(s => KEEP.has(s.marker)).map(s => s.bytes);
  const soi = Buffer.from([0xFF, 0xD8]), eoi = kept.pop();
  const body = Buffer.concat(kept);
  files['accept-baseline.jpg'] = Buffer.concat([soi, JFIF, body, eoi]);
  files['accept-no-app0.jpg'] = Buffer.concat([soi, body, eoi]);
  files['reject-exif.jpg'] = Buffer.concat([soi, JFIF, segment(0xE1, Buffer.from('Exif\0\0MM\0*\0\0\0\x08\0\0', 'latin1')), body, eoi]);
  files['reject-icc.jpg'] = Buffer.concat([soi, JFIF, segment(0xE2, Buffer.from('ICC_PROFILE\0\x01\x01', 'latin1')), body, eoi]);
  files['reject-comment.jpg'] = Buffer.concat([soi, JFIF, segment(0xFE, Buffer.from('Jane Example')), body, eoi]);
  files['reject-jfxx.jpg'] = Buffer.concat([soi, JFIF, segment(0xE0, Buffer.from('JFXX\0\x10', 'latin1')), body, eoi]);
  files['reject-trailing.jpg'] = Buffer.concat([soi, JFIF, body, eoi, Buffer.from('trailing')]);
  for (const [name, bytes] of Object.entries(files)) writeFileSync(out + name, bytes);
  return Object.keys(files);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  if (!process.argv[2]) { console.error('Usage: node richos/mobile/dev/report-image-fixtures.mjs <baseline JPEG>'); process.exit(2); }
  console.log(build(process.argv[2]).join('\n'));
}
