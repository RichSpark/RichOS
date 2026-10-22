// The relay's own checks of a RichDoctor report (PRD §7): the approved schema, its limits and the
// content rules. The relay accepts the exact words or rejects them for review; it never rewrites
// them. The contract is REPORT.md beside the Worker.
export const SCHEMA = 'richdoctor-report/1';
export const MAX_TITLE_CHARS = 200;
export const MAX_BODY_BYTES = 60000;
// Alphabet 23456789ABCDEFGHJKMNPQRSTVWXYZ: no 0, 1, I, L, O or U.
export const REPORT_ID = /^RD-[2-9A-HJKMNP-TV-Z]{4}-[2-9A-HJKMNP-TV-Z]{4}$/;
const SECRET = /^[A-Za-z0-9_-]{43}$/;
const UUID = /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/;
const APP_VERSION = /^\d{1,4}\.\d{1,4}(\.\d{1,4})?([-+][0-9A-Za-z.-]{1,32})?$/;
const FIELDS = ['schema', 'report_id', 'receipt_secret', 'content_sha256', 'installation_id', 'app_version', 'approved_at', 'title', 'body'];
const utf8 = new TextEncoder();

export const hex = bytes => Array.from(new Uint8Array(bytes), b => b.toString(16).padStart(2, '0')).join('');
export const sha256 = async text => hex(await crypto.subtle.digest('SHA-256', utf8.encode(text)));
export const contentHash = (title, body) => sha256(title + '\n\n' + body);

// The disclosed footer: the body's last line, and the line reconciliation looks for.
const FOOTER_PREFIX = 'Sent with RichDoctor ';
export function footerMatches(line, reportId) {
  const tail = ' · Report ID ' + reportId;
  if (!line.startsWith(FOOTER_PREFIX) || !line.endsWith(tail)) return false;
  return /^\d{1,4}\.\d{1,4}(\.\d{1,4})?$/.test(line.slice(FOOTER_PREFIX.length, line.length - tail.length));
}
export const hasFooterLine = (body, reportId) => String(body || '').replace(/\r\n/g, '\n').split('\n').some(line => footerMatches(line, reportId));

const schemaError = detail => ({ status: 400, state: 'rejected', reason: 'schema', detail });
const sizeError = detail => ({ status: 413, state: 'rejected', reason: 'size', detail });

// Returns { value } or { error }. Never echoes a submitted value back.
export async function checkCreate(data, now) {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return { error: schemaError('The report is not in the expected format.') };
  const keys = Object.keys(data);
  if (keys.length !== FIELDS.length || FIELDS.some(k => !keys.includes(k))) return { error: schemaError('The report has missing or unexpected fields.') };
  const { schema, report_id, receipt_secret, content_sha256, installation_id, app_version, approved_at, title, body } = data;
  if (schema !== SCHEMA) return { error: schemaError('This report format version is not supported.') };
  if (typeof report_id !== 'string' || !REPORT_ID.test(report_id)) return { error: schemaError('The report ID is not valid.') };
  if (typeof receipt_secret !== 'string' || !SECRET.test(receipt_secret)) return { error: schemaError('The report receipt is not valid.') };
  if (typeof content_sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(content_sha256)) return { error: schemaError('The report fingerprint is not valid.') };
  if (typeof installation_id !== 'string' || !UUID.test(installation_id)) return { error: schemaError('The installation ID is not valid.') };
  if (typeof app_version !== 'string' || !APP_VERSION.test(app_version)) return { error: schemaError('The app version is not valid.') };
  if (!Number.isSafeInteger(approved_at) || approved_at < 1_700_000_000_000 || approved_at > now + 86_400_000) return { error: schemaError('The approval time is not valid.') };
  if (typeof title !== 'string' || typeof body !== 'string') return { error: schemaError('The title and body must be text.') };
  if (!title.trim()) return { error: schemaError('The title is empty.') };
  if ([...title].length > MAX_TITLE_CHARS) return { error: sizeError(`The title is longer than ${MAX_TITLE_CHARS} characters.`) };
  if (utf8.encode(body).byteLength > MAX_BODY_BYTES) return { error: sizeError('The report is longer than GitHub allows.') };
  if (await contentHash(title, body) !== content_sha256) return { error: schemaError('The report fingerprint does not match its words.') };
  return { value: { schema, reportId: report_id, secret: receipt_secret, contentHash: content_sha256, installationId: installation_id.toLowerCase(), appVersion: app_version, approvedAt: approved_at, title, body } };
}

// The words GitHub shows: each backslash escape read as its mark, and the zero-width space after
// every @ removed, so the secret, email and path checks see what a reader sees.
const shown = text => text.replace(/\\([\\`*_[\]<>#|~&+=.)!-])/g, '$1').replace(/​/g, '');

const SECRETS = [
  /\bgh[pousr]_[A-Za-z0-9]{20,}/, /\bgithub_pat_[A-Za-z0-9_]{20,}/, /\bxox[abposr]-[A-Za-z0-9-]{10,}/,
  /\b(AKIA|ASIA)[0-9A-Z]{16}\b/, /\bsk-ant-[A-Za-z0-9_-]{10,}/, /\bsk-[A-Za-z0-9_-]{20,}/,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/, /\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\./,
  /\b(authorization|proxy-authorization|cookie|set-cookie)\s*:\s*(?!\[)\S/i, /\bbearer\s+[A-Za-z0-9._~+/-]{16,}/i,
  /\b(api[_-]?key|access[_-]?token|secret|password|passwd)\s*[=:]\s*(?!\[)[^\s]{8,}/i,
];
const EMAIL_DOMAIN = /^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}/;
const LOCAL_CHAR = /[A-Za-z0-9._%+-]/;
// Each @ is examined in a bounded window, so a 60,000-byte line costs linear time.
function* emails(words) {
  for (let at = words.indexOf('@'); at >= 0; at = words.indexOf('@', at + 1)) {
    let start = at;
    while (start > 0 && at - start < 64 && LOCAL_CHAR.test(words[start - 1])) start--;
    const domain = EMAIL_DOMAIN.exec(words.slice(at + 1, at + 254));
    if (start < at && domain) yield words.slice(start, at + 1) + domain[0];
  }
}
const ALLOWED_EMAILS = new Set(['support@richos.ceo']);
const HOME_PATH = /\/Users\/(?!Shared(\/|\b)|\[)[^/\s]+/;

const DETAILS = {
  control: 'It contains an invisible control character.',
  footer: 'Its last line is not the RichDoctor footer with this report ID.',
  markup: 'It contains a link, picture or formatting mark that was not made safe.',
  mention: 'It contains an @ mention.',
  secret: 'It contains something that looks like a password, key or token.',
  email: 'It contains an email address.',
  home_path: 'It contains a folder path that shows a user name.',
};

function privateKind(text, secret) {
  const words = shown(text);
  if (words.includes(secret) || SECRETS.some(pattern => pattern.test(words))) return 'secret';
  for (const email of emails(words)) if (!ALLOWED_EMAILS.has(email.toLowerCase())) return 'email';
  if (HOME_PATH.test(words)) return 'home_path';
  return null;
}

// An unescaped <, [ or ] (an even number of backslashes before it) can start HTML, a link or a
// picture. An @ without the zero-width space after it is a mention.
function markupKind(line) {
  let slashes = 0;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '\\') { slashes++; continue; }
    if ((c === '<' || c === '[' || c === ']') && slashes % 2 === 0) return 'markup';
    if (c === '@' && line[i + 1] !== '​') return 'mention';
    slashes = 0;
  }
  return null;
}

// The first problem, as { kind, item, detail }, or null. `item` names a place, never the words.
export function contentProblem({ title, body, reportId, secret }) {
  const reject = (kind, item) => ({ kind, item, detail: DETAILS[kind] });
  if (/[\u0000-\u001F\u007F]/.test(title)) return reject('control', 'title');
  const titleKind = privateKind(title, secret);
  if (titleKind) return reject(titleKind, 'title');
  const lines = body.split('\n');
  for (let n = 0; n < lines.length; n++) {
    const line = lines[n];
    if (/[\u0000-\u0008\u000B-\u001F\u007F]/.test(line)) return reject('control', `line ${n + 1}`);
  }
  if (!footerMatches(lines[lines.length - 1], reportId)) return reject('footer', `line ${lines.length}`);
  for (let n = 0; n < lines.length; n++) {
    const kind = markupKind(lines[n]) || privateKind(lines[n], secret);
    if (kind) return reject(kind, `line ${n + 1}`);
  }
  // A token split across lines is still a token: the whole body is checked once more.
  const whole = privateKind(body, secret);
  if (whole) return reject(whole, 'body');
  return null;
}
