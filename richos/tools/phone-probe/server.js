'use strict';

// The phone probe's whole back end: two listeners, five static files, one push subscription in
// memory, and one encrypted push on a delay.
//
// THE TWO LISTENERS, and why there have to be two:
//
//   HTTPS on 8443 — the probe itself, at `https://mm1.local:8443`. A PWA cannot exist without a
//   secure origin: service workers, push and Add to Home Screen all require one. This is the origin
//   the CEO's phone opens, and the port is the one the plan pins (§2.1) because THE PORT IS PART OF
//   THE ORIGIN: :8443 and :9443 are two different origins with two different service workers and two
//   different push subscriptions. 443 is refused because binding it needs root.
//
//   HTTP on 8442 — the trust step, and it is plain HTTP for a reason that cannot be designed away.
//   The certificate the HTTPS listener presents is signed by an authority that exists only on this
//   Mac, so before the phone has installed and trusted that authority it CANNOT open the HTTPS
//   origin without a warning. The page that hands over the certificate therefore has to be served
//   over something the phone already trusts, and over a LAN with no public name, that is HTTP.
//   Nothing sensitive crosses it: a root certificate is a public key, and the profile is a wrapper
//   around one.
//
// This replaced a Railway deployment on 2026-09-18 by CEO ruling (`wiki/ceo-decisions.md` §57):
// *"the user is not expected to have something like Railway and I shouldn't provide that for a free
// and open-source app. Instead, the RichOS app should have 'something' running on the user's device
// (Mac in this case)."* The hosting step is now part of what the probe tests, because the real phone
// client will be served exactly this way.
//
// It is deliberately not a relay. The relay described in the plan (§2) is the NEXT brief; this
// process exists to answer §3.2's questions and then be deleted. Nothing here is a foundation for
// anything, and reading it as one would be a mistake.
//
// WHAT IT STORES, exhaustively, because the page makes a promise about this:
//   - one push subscription per device that presses "turn on notifications": the push-service
//     endpoint URL, its public key and its auth secret. All three are required to send a push
//     at all, on every platform including native, and they identify a MAILBOX, not a person.
//   - nothing else. No audio (it never leaves the phone), no check results (localStorage on the
//     phone), no identity, no analytics, no request log of his traffic.
// In memory only — a restart forgets everything, and there is no database and no disk write.
// The ONE thing written to disk is the certificate authority, under the probe's own state directory
// and never in the repository (lib/state.js).

const http = require('node:http');
const https = require('node:https');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { generateVapidKeys, sendNotification } = require('./lib/webpush.js');
const { ensureCertificates } = require('./lib/tls-setup.js');
const state = require('./lib/state.js');
const localnames = require('./lib/localnames.js');
// SIX WORDS INSTEAD OF A HASH. The CEO's own finding from the 2026-09-18 walk: "On the second page
// ... an ugly horizontal scroll bar appeared. I suspect that was caused by the long SHA256 which was
// all in one line on the second page." He was right about the cause, and the fix is not to wrap the
// hash — it is to stop showing him one. The product's own six-word rendering is used rather than a
// second copy of it, so the probe teaches exactly what the app does.
const fingerprintWords = require('../../web/web-app/lib/fingerprint.js');
const qr = require('./lib/qr.js');

// PORT is still honored so an older invocation keeps working, but the two ports have their own
// names now because there are two of them.
const HTTPS_PORT = Number(process.env.PROBE_HTTPS_PORT || process.env.PORT || 8443);
const TRUST_PORT = Number(process.env.PROBE_TRUST_PORT || 8442);
const BIND = process.env.PROBE_BIND || '0.0.0.0'; // the phone is not on this machine
const PUBLIC_DIR = path.join(__dirname, 'public');
const TRUST_DIR = path.join(__dirname, 'trust');
// The VAPID subject: WHO is sending this push, as told to Apple and Google.
//
// It is the PUBLIC PROJECT PAGE, not an email address. RFC 8292 §2.1 allows a `mailto:` or an
// `https:` URL and Apple's push service accepts either. The URL is the better default for exactly one
// reason: a personal address is never baked into a build, a config default or a record, and a default
// is the one place a value ends up in all three. The app's own push sender will use the same URL.
//
// The placeholder it replaced (`mailto:probe@richos.invalid`) was worse than useless: Google's FCM
// accepts a fabricated subject, Apple returns BadJwtToken for one, so check 4 would have failed on his
// iPhone for a reason that has nothing to do with the phone.
const VAPID_SUBJECT_DEFAULT = 'https://github.com/RichSpark/RichOS';
const VAPID_SUBJECT = process.env.VAPID_SUBJECT || VAPID_SUBJECT_DEFAULT;

// An unguessable code, if one is set. The plan's own doctrine (§2, "one device set, everything
// else 404") says an unpaired caller gets a flat 404 rather than an error that tells it what
// exists — so that is what this does. Unset means open, which is fine for a home LAN with no port
// forwarding and is warned about at boot.
const ACCESS_CODE = process.env.PROBE_ACCESS_CODE || '';

function log(msg) {
	process.stdout.write(`[phone-probe] ${msg}\n`);
}

// ---------------------------------------------------------------------------
// TLS — the certificate this Mac presents
// ---------------------------------------------------------------------------

const tls = ensureCertificates({ log: (line) => log(line) });
const HOST_NAME = localnames.primaryName();
const HTTPS_ORIGIN = `https://${HOST_NAME}${HTTPS_PORT === 443 ? '' : `:${HTTPS_PORT}`}`;
const TRUST_ORIGIN = `http://${HOST_NAME}${TRUST_PORT === 80 ? '' : `:${TRUST_PORT}`}`;

// ---------------------------------------------------------------------------
// VAPID keys
// ---------------------------------------------------------------------------

// A push subscription is taken against ONE public key. If the key changes, every subscription made
// against the old one is dead, and check 4 fails for a reason that has nothing to do with the phone.
//
// When this probe was a hosted service there was nowhere to keep a pair, so it generated one per
// process and warned loudly. Hosting it on the Mac removes that whole failure mode: there is a state
// directory now, so the pair is generated once and kept beside the certificate, mode 0600. Restarting
// the server — or the Mac — no longer costs him the subscription.
let vapid = {
	publicKey: process.env.VAPID_PUBLIC_KEY || '',
	privateKey: process.env.VAPID_PRIVATE_KEY || ''
};
let vapidSource = 'the environment';
if (!vapid.publicKey || !vapid.privateKey) {
	const file = state.paths().vapid;
	let stored = null;
	try { stored = JSON.parse(fs.readFileSync(file, 'utf8')); } catch { /* first run, or unreadable */ }
	if (stored && stored.publicKey && stored.privateKey) {
		vapid = { publicKey: stored.publicKey, privateKey: stored.privateKey };
		vapidSource = file;
	} else {
		vapid = generateVapidKeys();
		state.writePrivate(file, `${JSON.stringify({ ...vapid, createdAt: new Date().toISOString() }, null, 2)}\n`);
		vapidSource = `${file} (created now)`;
	}
}
// Kept for the page's own reporting: there is no longer any path that produces a pair which dies with
// the process, so this is now always false. The field stays because `/api/config` publishes it and the
// page prints it — removing it would silently change what the results panel says.
const vapidEphemeral = false;

// ---------------------------------------------------------------------------
// State — one Map, no disk
// ---------------------------------------------------------------------------

/** @type {Map<string, {id: string, subscription: object, createdAt: string, lastSend: object|null}>} */
const devices = new Map();
const MAX_DEVICES = 8; // one CEO, one phone, plus room for a desktop test browser or two
/** @type {Map<string, NodeJS.Timeout>} */
const pending = new Map();

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

function sendJson(res, status, body) {
	const text = JSON.stringify(body);
	res.writeHead(status, {
		'Content-Type': 'application/json; charset=utf-8',
		'Content-Length': Buffer.byteLength(text),
		'Cache-Control': 'no-store'
	});
	res.end(text);
}

function notFound(res) {
	// Flat and uninformative, on purpose.
	res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8', 'Cache-Control': 'no-store' });
	res.end('Not found\n');
}

function readBody(req, limitBytes = 64 * 1024) {
	return new Promise((resolve, reject) => {
		const chunks = [];
		let total = 0;
		req.on('data', (c) => {
			total += c.length;
			if (total > limitBytes) {
				reject(new Error('body too large'));
				req.destroy();
				return;
			}
			chunks.push(c);
		});
		req.on('end', () => resolve(Buffer.concat(chunks)));
		req.on('error', reject);
	});
}

function codeOk(req, url) {
	if (!ACCESS_CODE) return true;
	const supplied = req.headers['x-probe-code'] || url.searchParams.get('k') || '';
	// Timing-safe, because it costs one line.
	const a = Buffer.from(String(supplied));
	const b = Buffer.from(ACCESS_CODE);
	return a.length === b.length && crypto.timingSafeEqual(a, b);
}

const MIME = {
	'.html': 'text/html; charset=utf-8',
	'.js': 'text/javascript; charset=utf-8',
	'.css': 'text/css; charset=utf-8',
	'.json': 'application/json; charset=utf-8',
	'.webmanifest': 'application/manifest+json; charset=utf-8',
	'.png': 'image/png',
	'.svg': 'image/svg+xml',
	'.ico': 'image/x-icon'
};

function serveStatic(res, urlPath) {
	const rel = urlPath === '/' ? 'index.html' : urlPath.replace(/^\/+/, '');
	const full = path.join(PUBLIC_DIR, rel);
	// Never serve outside public/.
	if (!full.startsWith(PUBLIC_DIR + path.sep)) return notFound(res);

	fs.readFile(full, (err, data) => {
		if (err) return notFound(res);
		const headers = {
			'Content-Type': MIME[path.extname(full)] || 'application/octet-stream',
			'Content-Length': data.length,
			// A probe must never show a stale build; every answer it gives would be about the
			// wrong page. The service worker is the file where this matters most.
			'Cache-Control': 'no-store'
		};
		res.writeHead(200, headers);
		res.end(data);
	});
}

// The manifest is generated rather than static for one reason: `start_url` must carry the access
// code, or Add to Home Screen produces an installed app that 404s on its own first launch. An
// installed iOS web app may also be storage-partitioned away from Safari, so localStorage cannot
// be relied on to carry the code across the install.
function serveManifest(res, code) {
	const q = code ? `?k=${encodeURIComponent(code)}` : '';
	const manifest = {
		name: 'RichOS phone probe',
		short_name: 'Probe',
		description: 'Five checks that say whether a RichOS phone app can be a web app.',
		// `standalone` is not cosmetic: WebKit gives an installed web app no push at all unless
		// display is standalone or fullscreen.
		display: 'standalone',
		start_url: `/${q}`,
		scope: '/',
		orientation: 'portrait',
		// Matches --ground in both themes (richos/app/ui/style.css), so the status bar and the
		// splash agree with the page instead of flashing white.
		background_color: '#0c1322',
		theme_color: '#0c1322',
		icons: [
			{ src: `/icons/icon-192.png${q}`, sizes: '192x192', type: 'image/png', purpose: 'any' },
			{ src: `/icons/icon-512.png${q}`, sizes: '512x512', type: 'image/png', purpose: 'any' },
			{ src: `/icons/icon-maskable-512.png${q}`, sizes: '512x512', type: 'image/png', purpose: 'maskable' }
		]
	};
	const text = JSON.stringify(manifest, null, 2);
	res.writeHead(200, {
		'Content-Type': 'application/manifest+json; charset=utf-8',
		'Content-Length': Buffer.byteLength(text),
		'Cache-Control': 'no-store'
	});
	res.end(text);
}

// ---------------------------------------------------------------------------
// The push the probe sends
// ---------------------------------------------------------------------------

// The payload is BOTH a plain object our service worker understands AND a valid Declarative Web
// Push document (`web_push: 8030`, WebKit's "Meet Declarative Web Push"). On iOS 18.4+ Safari may
// render the declared notification itself without running our JavaScript; on iOS 16.4–18.3 the
// service worker's `push` handler reads the same fields. Either way a notification appears, which
// is the only thing check 4 is asking. Costs one extra key and removes a whole failure mode.
function pushPayload(code) {
	const q = code ? `?k=${encodeURIComponent(code)}&from=push` : '?from=push';
	return {
		web_push: 8030,
		notification: {
			title: 'Rich',
			body: 'Check 4 passed — this push arrived with the app closed. Tap to finish check 5.',
			navigate: `/${q}`,
			// An app-icon badge is documented as working in the foreground and while handling a
			// push in the background, so it is a second, independent signal that the push landed.
			app_badge: 1
		},
		// Read by the service worker on the iOS 16.4–18.3 path.
		url: `/${q}`,
		sentAt: new Date().toISOString()
	};
}

async function deliver(id, code) {
	const device = devices.get(id);
	if (!device) return;
	try {
		const result = await sendNotification(device.subscription, pushPayload(code), {
			vapidPublicKey: vapid.publicKey,
			vapidPrivateKey: vapid.privateKey,
			vapidSubject: VAPID_SUBJECT,
			ttl: 300,
			urgency: 'high'
		});
		device.lastSend = {
			at: new Date().toISOString(),
			statusCode: result.statusCode,
			ok: result.statusCode >= 200 && result.statusCode < 300,
			gone: result.gone,
			// A push service's error body is the most useful diagnostic there is when this fails,
			// and it says nothing about him, so it is kept.
			detail: result.statusCode >= 300 ? String(result.body).slice(0, 400) : ''
		};
		// Plan risk 2: a lapsed subscription is a real event, not something to swallow. A silent
		// dead channel is the failure that matters.
		if (result.gone) devices.delete(id);
		log(`push ${id.slice(0, 8)} -> ${result.statusCode}${result.gone ? ' (subscription gone, forgotten)' : ''}`);
	} catch (err) {
		device.lastSend = { at: new Date().toISOString(), statusCode: 0, ok: false, gone: false, detail: String(err.message).slice(0, 400) };
		log(`push ${id.slice(0, 8)} -> failed: ${err.message}`);
	} finally {
		pending.delete(id);
	}
}

// ---------------------------------------------------------------------------
// Routes — the HTTPS listener, which is the probe
// ---------------------------------------------------------------------------

const handler = async (req, res) => {
	const url = new URL(req.url, `https://${req.headers.host || HOST_NAME}`);
	const route = url.pathname;

	if (route === '/healthz') {
		return sendJson(res, 200, { ok: true, devices: devices.size, vapidEphemeral, tls: true });
	}

	// The manifest and the icons are fetched by the OS, which does not send our header, so they
	// authenticate by query string only.
	if (route === '/manifest.webmanifest') {
		if (ACCESS_CODE && url.searchParams.get('k') !== ACCESS_CODE) return notFound(res);
		return serveManifest(res, ACCESS_CODE);
	}

	if (route.startsWith('/api/')) {
		if (!codeOk(req, url)) return notFound(res);

		if (route === '/api/config' && req.method === 'GET') {
			return sendJson(res, 200, {
				vapidPublicKey: vapid.publicKey,
				vapidEphemeral,
				// So the page can prove which build answered it, rather than assuming.
				buildSha: process.env.PROBE_BUILD_SHA || 'unset',
				serverTime: new Date().toISOString(),
				// Check 0's machine-checkable half. The page shows these so what the phone reached
				// can be compared with what the profile said it was installing, rather than trusted.
				tls: {
					// `req.socket.encrypted` is the only honest answer to "did this request arrive
					// over TLS" — a header could be anything.
					secure: Boolean(req.socket && req.socket.encrypted),
					hostReached: url.host,
					names: tls.meta.leafSubjectAltName,
					// The hash stays in the API because a machine may want it; the PAGE renders the
					// words. Nothing he reads is a hash.
					caFingerprintSha256: tls.meta.caFingerprintSha256,
					caWords: fingerprintWords.phraseFromHex(tls.meta.caFingerprintSha256),
					caNotAfter: tls.meta.caNotAfter,
					leafNotAfter: tls.meta.leafNotAfter
				},
				trustUrl: TRUST_ORIGIN
			});
		}

		if (route === '/api/subscribe' && req.method === 'POST') {
			let body;
			try {
				body = JSON.parse((await readBody(req)).toString('utf8'));
			} catch {
				return sendJson(res, 400, { error: 'expected JSON' });
			}
			const sub = body && body.subscription;
			if (!sub || !sub.endpoint || !sub.keys || !sub.keys.p256dh || !sub.keys.auth) {
				return sendJson(res, 400, { error: 'subscription must have endpoint and keys.p256dh and keys.auth' });
			}
			// Re-subscribing on every launch is plan risk 2's mitigation, so the same endpoint
			// arriving twice must update in place rather than pile up.
			let id = null;
			for (const [key, d] of devices) {
				if (d.subscription.endpoint === sub.endpoint) { id = key; break; }
			}
			if (!id) {
				if (devices.size >= MAX_DEVICES) {
					// Drop the oldest rather than refuse: a probe that stops working because a
					// test browser filled a slot is a probe that answers the wrong question.
					const oldest = [...devices.entries()].sort((a, b) => a[1].createdAt.localeCompare(b[1].createdAt))[0];
					if (oldest) devices.delete(oldest[0]);
				}
				id = crypto.randomUUID();
			}
			devices.set(id, {
				id,
				subscription: sub,
				createdAt: devices.get(id) ? devices.get(id).createdAt : new Date().toISOString(),
				lastSend: devices.get(id) ? devices.get(id).lastSend : null
			});
			log(`subscribed ${id.slice(0, 8)} via ${new URL(sub.endpoint).host}`);
			return sendJson(res, 200, { id, pushService: new URL(sub.endpoint).host });
		}

		if (route === '/api/test-push' && req.method === 'POST') {
			let body;
			try {
				body = JSON.parse((await readBody(req)).toString('utf8'));
			} catch {
				return sendJson(res, 400, { error: 'expected JSON' });
			}
			const id = body && body.id;
			if (!id || !devices.has(id)) return notFound(res);
			const delay = Math.min(Math.max(Number(body.delaySeconds) || 60, 0), 600);
			if (pending.has(id)) clearTimeout(pending.get(id));
			const timer = setTimeout(() => { deliver(id, ACCESS_CODE); }, delay * 1000);
			// A stray timer must never hold the process open past a shutdown.
			if (timer.unref) timer.unref();
			pending.set(id, timer);
			devices.get(id).lastSend = null;
			log(`scheduled push for ${id.slice(0, 8)} in ${delay}s`);
			return sendJson(res, 200, { scheduled: true, delaySeconds: delay, dueAt: new Date(Date.now() + delay * 1000).toISOString() });
		}

		if (route === '/api/status' && req.method === 'GET') {
			const id = url.searchParams.get('id');
			if (!id || !devices.has(id)) return notFound(res);
			return sendJson(res, 200, { id, pending: pending.has(id), lastSend: devices.get(id).lastSend });
		}

		if (route === '/api/forget' && req.method === 'POST') {
			let body = {};
			try { body = JSON.parse((await readBody(req)).toString('utf8')); } catch { /* an empty body is fine */ }
			const id = body && body.id;
			if (id && pending.has(id)) { clearTimeout(pending.get(id)); pending.delete(id); }
			const had = id ? devices.delete(id) : false;
			log(`forget ${String(id).slice(0, 8)} -> ${had ? 'deleted' : 'nothing to delete'}`);
			return sendJson(res, 200, { forgotten: had });
		}

		return notFound(res);
	}

	if (req.method !== 'GET' && req.method !== 'HEAD') return notFound(res);

	// The access code gates the PAGE, the manifest and the API — the three things that either
	// identify the probe or act on his behalf. It deliberately does NOT gate the stylesheet, the
	// scripts or the icons.
	//
	// Not laziness: a `<script src="/pcm.js">` in the markup is fetched by the browser without our
	// query string, and an `audioWorklet.addModule` inside a service-worker-controlled page is worse
	// again. Gating those would mean threading the code through every asset reference and having the
	// page die on whichever one was missed — a failure that would present as "the microphone does not
	// work", which is the one wrong answer this probe must not give. The assets are a stylesheet and
	// some arithmetic; they reveal nothing and do nothing.
	const gated = route === '/' || route === '/index.html';
	if (ACCESS_CODE && gated && !codeOk(req, url)) return notFound(res);
	return serveStatic(res, route);
};

const server = https.createServer({ cert: tls.leafCertPem, key: tls.leafKeyPem }, handler);

// ---------------------------------------------------------------------------
// Routes — the HTTP listener, which is check 0 and nothing else
// ---------------------------------------------------------------------------
// Five paths, and no route to the probe itself. Anyone who finds this port gets a certificate they
// can install and a page explaining what it is for; there is nothing else here to find.

function trustPage() {
	const template = fs.readFileSync(path.join(TRUST_DIR, 'index.html'), 'utf8');
	const code = ACCESS_CODE ? `?k=${encodeURIComponent(ACCESS_CODE)}` : '';
	const addresses = localnames.lanAddresses();
	const filled = template
		.replace(/\{\{HTTPS_URL\}\}/g, `${HTTPS_ORIGIN}/${code}`)
		.replace(/\{\{HTTPS_URL_PLAIN\}\}/g, `${HTTPS_ORIGIN}/`)
		.replace(/\{\{HOST\}\}/g, HOST_NAME)
		.replace(/\{\{LAN_ADDRESS\}\}/g, addresses.length ? addresses[0].address : 'this Mac\'s address')
		.replace(/\{\{TRUST_PORT\}\}/g, String(TRUST_PORT))
		.replace(/\{\{CA_FINGERPRINT\}\}/g, tls.meta.caFingerprintSha256)
		.replace(/\{\{CA_WORDS\}\}/g, fingerprintWords.phraseFromHex(tls.meta.caFingerprintSha256))
		.replace(/\{\{CA_NAME\}\}/g, tls.meta.caCommonName)
		.replace(/\{\{CERT_NAMES\}\}/g, tls.meta.leafSubjectAltName)
		.replace(/\{\{CA_PATH\}\}/g, tls.paths.caCert);

	// A page that still carries a placeholder is a page telling him to look for a switch whose label
	// reads "{{CA_NAME}}". Better to fail at boot, here, than to be found on his phone.
	const leftover = /\{\{[A-Z_]+\}\}/.exec(filled);
	if (leftover) throw new Error(`the trust page still contains ${leftover[0]} — every placeholder must be filled`);
	return filled;
}

const trustHandler = (req, res) => {
	const url = new URL(req.url, `http://${req.headers.host || HOST_NAME}`);
	const route = url.pathname;

	if (route === '/healthz') return sendJson(res, 200, { ok: true, trustStep: true });

	// Gated exactly like the probe's own page, for the same reason: if a code is set, an unpaired
	// caller learns nothing. The download links on the page carry the code themselves.
	if (ACCESS_CODE && !codeOk(req, url)) return notFound(res);
	if (req.method !== 'GET' && req.method !== 'HEAD') return notFound(res);

	if (route === '/' || route === '/index.html') {
		const html = trustPage();
		res.writeHead(200, {
			'Content-Type': 'text/html; charset=utf-8',
			'Content-Length': Buffer.byteLength(html),
			'Cache-Control': 'no-store'
		});
		return res.end(html);
	}

	// The profile. `application/x-apple-aspen-config` is what makes iOS treat it as something to
	// install rather than something to display, and the filename is what he sees in Settings.
	if (route === '/ca.mobileconfig') {
		const body = Buffer.from(tls.profile, 'utf8');
		res.writeHead(200, {
			'Content-Type': 'application/x-apple-aspen-config',
			'Content-Disposition': 'attachment; filename="richos-local-ca.mobileconfig"',
			'Content-Length': body.length,
			'Cache-Control': 'no-store'
		});
		return res.end(body);
	}

	// The bare certificate, for a browser or a desktop that wants the certificate without a profile.
	if (route === '/ca.crt') {
		const body = Buffer.from(tls.caCertPem, 'utf8');
		res.writeHead(200, {
			'Content-Type': 'application/x-x509-ca-cert',
			'Content-Disposition': 'attachment; filename="richos-local-ca.crt"',
			'Content-Length': body.length,
			'Cache-Control': 'no-store'
		});
		return res.end(body);
	}

	// The code that sends the phone to the HTTPS origin once the root is trusted.
	if (route === '/qr.png') {
		const code = ACCESS_CODE ? `?k=${encodeURIComponent(ACCESS_CODE)}` : '';
		const rendered = qr.toPng(`${HTTPS_ORIGIN}/${code}`, { scale: 8 });
		res.writeHead(200, {
			'Content-Type': 'image/png',
			'Content-Length': rendered.png.length,
			'Cache-Control': 'no-store'
		});
		return res.end(rendered.png);
	}

	// The trust page shares the probe's stylesheet, so the two screens are one thing rather than
	// two, and so the contrast test covers both.
	if (route === '/styles.css') return serveStatic(res, '/styles.css');

	return notFound(res);
};

const trustServer = http.createServer(trustHandler);

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

let listening = 0;
const announce = () => {
	if (++listening < 2) return;
	const addresses = localnames.lanAddresses();
	log('');
	log(`  CHECK 0, the trust step — open this on the iPhone FIRST, in Safari:`);
	log(`    ${TRUST_ORIGIN}/${ACCESS_CODE ? `?k=${ACCESS_CODE}` : ''}`);
	if (addresses.length) log(`    or by address: http://${addresses[0].address}:${TRUST_PORT}/${ACCESS_CODE ? `?k=${ACCESS_CODE}` : ''}`);
	log('');
	log(`  CHECKS 1-5, the probe — this only opens after the root is trusted:`);
	log(`    ${HTTPS_ORIGIN}/${ACCESS_CODE ? `?k=${ACCESS_CODE}` : ''}`);
	log('');
	log(`  certificate names: ${tls.meta.leafSubjectAltName}`);
	// The terminal is a place for a hash: it is not his phone, it wraps, and an operator may need to
	// compare it with something else. The six words are printed beside it because they are what the
	// two SCREENS show.
	log(`  root SHA-256:      ${tls.meta.caFingerprintSha256}`);
	log(`  root in six words: ${fingerprintWords.phraseFromHex(tls.meta.caFingerprintSha256)}`);
	log(`  root on disk:      ${tls.paths.caCert}`);
	log('');
	if (process.env.PROBE_PRINT_QR !== '0') {
		const trustUrl = `${TRUST_ORIGIN}/${ACCESS_CODE ? `?k=${ACCESS_CODE}` : ''}`;
		log('  Point the iPhone camera at this to start the trust step:');
		process.stdout.write(`\n${qr.toAnsi(trustUrl).text}\n\n`);
	}
	log(`VAPID public key: ${vapid.publicKey}`);
	log(`  from ${vapidSource} — it survives a restart, so a subscription taken today still works tomorrow.`);
	log(`VAPID subject:    ${VAPID_SUBJECT}`);
	if (!process.env.VAPID_SUBJECT) {
		log('  VAPID_SUBJECT is not set, so the public project page is being used —');
		log('  https://github.com/RichSpark/RichOS. RFC 8292 §2.1 allows a mailto: or an https:');
		log('  URL and Apple accepts either; a personal address is never baked into a build, a config');
		log('  default or a record, so the project URL is the default on purpose. Nothing to fix.');
		log('');
	}
	if (/\.invalid\b/.test(VAPID_SUBJECT)) {
		log('  WARNING: VAPID_SUBJECT names a .invalid domain. Google accepts a fabricated subject;');
		log('  Apple returns BadJwtToken for one, so check 4 would fail on his iPhone for a reason');
		log('  that has nothing to do with the phone. Unset it to use the project URL.');
		log('');
	}
	if (!ACCESS_CODE) {
		log('  NOTE: PROBE_ACCESS_CODE is unset, so both origins are open to anything on this');
		log('  Wi-Fi. Fine behind a home router with no port forwarding; set one otherwise.');
	}
};

server.listen(HTTPS_PORT, BIND, () => {
	log(`HTTPS on ${HTTPS_ORIGIN} (bound to ${BIND})`);
	announce();
});
trustServer.listen(TRUST_PORT, BIND, () => {
	log(`HTTP trust step on ${TRUST_ORIGIN} (bound to ${BIND})`);
	announce();
});

for (const [name, listener] of [['https', server], ['trust', trustServer]]) {
	listener.on('error', (err) => {
		if (err.code === 'EADDRINUSE') {
			log(`FATAL: the ${name} port is already in use. Another probe is probably running — stop it, or`);
			log(`set PROBE_${name === 'https' ? 'HTTPS' : 'TRUST'}_PORT. Refusing to start half a probe.`);
			process.exit(1);
		}
		log(`FATAL: the ${name} listener failed: ${err.message}`);
		process.exit(1);
	});
}

// Anything still pending at shutdown is dropped on purpose — a probe that fires a scheduled push
// after a restart is a probe that lies about which build sent it.
for (const signal of ['SIGTERM', 'SIGINT']) {
	process.on(signal, () => {
		log(`${signal} — closing`);
		for (const timer of pending.values()) clearTimeout(timer);
		pending.clear();
		devices.clear();
		let closed = 0;
		const done = () => { if (++closed >= 2) process.exit(0); };
		server.close(done);
		trustServer.close(done);
		setTimeout(() => process.exit(0), 3000).unref();
	});
}

module.exports = { server, trustServer };
