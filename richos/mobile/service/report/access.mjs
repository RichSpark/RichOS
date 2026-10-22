// Staff access to screenshots (PRD v6 §7): every image read carries a Cloudflare Access JWT, and
// the Worker itself checks its signature, issuer, audience and expiry against the team's published
// keys, on every hostname, so a direct workers.dev request without Access gets nothing.
// https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/authorization-cookie/validating-json/
const keys = new Map();   // team domain -> { at, byKid }
const KEY_TTL_MS = 600_000;
const b64url = text => Uint8Array.from(atob(text.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));
const json = text => JSON.parse(new TextDecoder().decode(b64url(text)));

async function teamKeys(team, fetchImpl, now, refresh = false) {
  const cached = keys.get(team);
  if (cached && !refresh && now - cached.at < KEY_TTL_MS) return cached.byKid;
  const response = await fetchImpl(`${team}/cdn-cgi/access/certs`, { redirect: 'manual', signal: AbortSignal.timeout(5000) });
  const data = response.status === 200 ? await response.json() : null;
  if (!Array.isArray(data?.keys)) throw Error('access_keys_unavailable');
  const byKid = new Map();
  for (const jwk of data.keys) {
    if (jwk?.kty !== 'RSA' || typeof jwk.kid !== 'string') continue;
    byKid.set(jwk.kid, await crypto.subtle.importKey('jwk', { kty: 'RSA', n: jwk.n, e: jwk.e, alg: 'RS256', ext: true },
      { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['verify']));
  }
  keys.set(team, { at: now, byKid });
  return byKid;
}

// true only for a valid, unexpired Access token for this application. Never throws.
export async function staffAllowed(request, env, { fetch: fetchImpl = fetch, now = Date.now } = {}) {
  try {
    const team = env.ACCESS_TEAM_DOMAIN, audience = env.ACCESS_AUD;
    if (!/^https:\/\/[a-z0-9-]+\.cloudflareaccess\.com$/.test(team || '') || !/^[a-f0-9]{64}$/.test(audience || '')) return false;
    const token = request.headers.get('cf-access-jwt-assertion') || '';
    const [h, p, s] = token.split('.');
    if (!h || !p || !s) return false;
    const header = json(h), claims = json(p), seconds = Math.floor(now() / 1000);
    if (header.alg !== 'RS256' || typeof header.kid !== 'string') return false;
    let byKid = await teamKeys(team, fetchImpl, now());
    if (!byKid.has(header.kid)) byKid = await teamKeys(team, fetchImpl, now(), true);
    const key = byKid.get(header.kid);
    if (!key || !await crypto.subtle.verify('RSASSA-PKCS1-v1_5', key, b64url(s), new TextEncoder().encode(h + '.' + p))) return false;
    const audiences = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
    return claims.iss === team && audiences.includes(audience) && Number.isFinite(claims.exp) && claims.exp > seconds
      && (claims.nbf === undefined || claims.nbf <= seconds + 60);
  } catch { return false; }
}
export const _forgetKeys = () => keys.clear();
