// RichDoctor screenshot profile, version 1 (PRD v6 §7). The client encoder and this verifier share
// it; golden files are in fixtures/. Only structure is read, never the pixel data: PNG chunk
// types and IHDR, JPEG marker segments and SOF. Anything outside the profile is refused, never
// repaired. Linear in the image's size.
export const PROFILE_VERSION = 1;
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
export const MAX_TOTAL_BYTES = 25 * 1024 * 1024;
export const MAX_IMAGES = 10;
export const MAX_PIXELS = 40_000_000;
const PNG_CHUNKS = new Set(['IHDR', 'PLTE', 'IDAT', 'IEND', 'tRNS', 'sRGB', 'gAMA', 'pHYs']);
const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];
// SOF0-3, 5-7, 9-11, 13-15; DHT; DQT; DRI; DNL. SOS, APP0 (JFIF), RST and EOI are handled apart.
const JPEG_SEGMENTS = new Set([0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF, 0xC4, 0xDB, 0xDD, 0xDC]);
const SOF = new Set([0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF]);

const u16 = (b, i) => (b[i] << 8) | b[i + 1];
const u32 = (b, i) => ((b[i] << 24) >>> 0) + (b[i + 1] << 16) + (b[i + 2] << 8) + b[i + 3];
const refuse = detail => ({ error: detail });

function png(b) {
  if (b.length < 8 || PNG_SIGNATURE.some((v, i) => b[i] !== v)) return refuse('It is not a PNG image.');
  let pos = 8, size = null, idat = false, palette = false;
  while (pos + 12 <= b.length) {
    const length = u32(b, pos), type = String.fromCharCode(b[pos + 4], b[pos + 5], b[pos + 6], b[pos + 7]);
    if (length > b.length - pos - 12) return refuse('The PNG image is cut short.');
    if (!size && type !== 'IHDR') return refuse('The PNG image does not start with its header.');
    if (!PNG_CHUNKS.has(type)) return refuse(`The PNG image carries extra data (${/^[A-Za-z]{4}$/.test(type) ? type : 'unknown'}) the screenshot format does not allow.`);
    const data = pos + 8;
    if (type === 'IHDR') {
      if (size || length !== 13) return refuse('The PNG header is not valid.');
      const [depth, color, compression, filter, interlace] = [b[data + 8], b[data + 9], b[data + 10], b[data + 11], b[data + 12]];
      const depths = { 0: [1, 2, 4, 8, 16], 2: [8, 16], 3: [1, 2, 4, 8], 4: [8, 16], 6: [8, 16] }[color];
      if (!depths?.includes(depth) || compression || filter || interlace > 1) return refuse('The PNG header is not valid.');
      size = { width: u32(b, data), height: u32(b, data + 4) };
    }
    if (type === 'PLTE') { if (idat) return refuse('The PNG palette comes after the image data.'); palette = true; }
    if (type === 'IDAT') idat = true;
    pos = data + length + 4;
    if (type === 'IEND') {
      if (length !== 0 || !idat) return refuse('The PNG image is not complete.');
      if (pos !== b.length) return refuse('The PNG image has extra bytes after its end.');
      return { width: size.width, height: size.height, palette };
    }
  }
  return refuse('The PNG image is cut short.');
}

function jpeg(b) {
  if (b.length < 4 || b[0] !== 0xFF || b[1] !== 0xD8) return refuse('It is not a JPEG image.');
  let pos = 2, size = null, scans = 0, app0 = false;
  while (pos < b.length) {
    if (b[pos] !== 0xFF) return refuse('The JPEG image is not valid.');
    while (b[pos] === 0xFF && pos < b.length) pos++;
    const marker = b[pos]; pos++;
    if (marker === 0xD9) {
      if (!size || !scans) return refuse('The JPEG image is not complete.');
      if (pos !== b.length) return refuse('The JPEG image has extra bytes after its end.');
      return { width: size.width, height: size.height };
    }
    if (pos + 2 > b.length) return refuse('The JPEG image is cut short.');
    const length = u16(b, pos), start = pos + 2, end = pos + length;
    if (length < 2 || end > b.length) return refuse('The JPEG image is cut short.');
    if (marker === 0xE0) {
      const jfif = length >= 7 && b[start] === 0x4A && b[start + 1] === 0x46 && b[start + 2] === 0x49 && b[start + 3] === 0x46 && b[start + 4] === 0;
      if (!jfif || app0) return refuse('The JPEG image carries extra data the screenshot format does not allow.');
      app0 = true;
    } else if (marker === 0xDA) {
      if (!size) return refuse('The JPEG image is not valid.');
      scans++;
      // Entropy-coded data runs to the next marker that is not a stuffed 0x00 or a restart marker.
      let i = end;
      while (i < b.length) {
        if (b[i] !== 0xFF) { i++; continue; }
        const next = b[i + 1];
        if (next === 0x00 || (next >= 0xD0 && next <= 0xD7)) { i += 2; continue; }
        if (next === 0xFF) { i++; continue; }
        break;
      }
      pos = i; continue;
    } else if (!JPEG_SEGMENTS.has(marker)) {
      return refuse('The JPEG image carries extra data the screenshot format does not allow.');
    } else if (SOF.has(marker)) {
      if (size || length < 8) return refuse('The JPEG image is not valid.');
      size = { height: u16(b, start + 1), width: u16(b, start + 3) };
      if (!size.width || !size.height) return refuse('The JPEG image does not state its size.');
    }
    pos = end;
  }
  return refuse('The JPEG image is cut short.');
}

// { width, height } when `bytes` is a profile-v1 image of `mediaType`, or { error } with a plain
// sentence that names no private data.
export function checkImage(bytes, mediaType) {
  const b = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const result = mediaType === 'image/png' ? png(b) : mediaType === 'image/jpeg' ? jpeg(b) : refuse('Only PNG and JPEG screenshots are accepted.');
  if (result.error) return result;
  if (!result.width || !result.height || result.width * result.height > MAX_PIXELS) return refuse('The screenshot is larger than 40 megapixels.');
  return { width: result.width, height: result.height };
}
