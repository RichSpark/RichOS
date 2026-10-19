// The service worker. It does three jobs and refuses a fourth.
//
//  1. IT KEEPS THE SHELL, so the app OPENS where his Mac does not resolve — on a train, on someone
//     else's Wi-Fi. That is not a trick; it is the ordinary reason a PWA has a service worker, and
//     plan §10.7 depends on it: the origin stays `https://<his-mac>.<tailnet>.ts.net:8443`
//     forever and the app must still open there when that name cannot be resolved — which is
//     every moment Tailscale is off on either device.
//
//  2. IT SHOWS RICH'S REPLY WHEN THE APP IS CLOSED, and writes it into storage on the way past. The
//     push payload is encrypted end to end by RFC 8291 — Apple relays a blob it cannot read — and it
//     carries Rich's actual words rather than "you have a message" (§2.4). Writing the row into
//     IndexedDB as the notification is shown is what makes the reply there when he opens the app
//     away from home, instead of behind a connection that is not available.
//
//  3. IT OPENS THE THREAD WHEN HE TAPS THE NOTIFICATION.
//
// AND THE REFUSAL: IT NEVER CACHES ANYTHING UNDER `/api/`. A cached conversation would make this
// phone a second record of what Rich said, which plan §6 forbids in those words: "A phone that can
// disagree with the Mac about what Rich said is worse than a phone that shows nothing."
//
// One more rule, from WebKit and not from us: EVERY push must display a notification. A handler
// that shows none can have the subscription revoked, so every path below ends in one.

importScripts('/lib/storage.js');
// The two values a push payload can put in front of this worker, and the rules that decide whether
// either is usable. Imported rather than reimplemented, because the page validates the same
// `api_base` with the same function and two copies of a rule are two rules.
importScripts('/lib/inbound.js');

const SHELL_CACHE = 'richos-phone-shell-v4';

// The whole app, and nothing that is not the app. Listed rather than discovered, because a service
// worker that caches whatever it happens to see is a service worker that serves yesterday's
// JavaScript to a page that expects today's.
const SHELL = [
	'/lib/connection.js',
	'/lib/follow.js',
    '/lib/notification-target.js',
	'/',
	'/index.html',
	'/styles.css',
	'/app.js',
	'/lib/pcm.js',
	'/lib/voice.js',
	'/lib/recorder-worklet.js',
	'/lib/wordlist.js',
	'/lib/fingerprint.js',
	'/lib/queue.js',
	'/lib/thread.js',
	'/lib/markdown.js',
	'/lib/api.js',
	'/lib/link.js',
	'/lib/storage.js',
	'/lib/inbound.js',
	'/manifest.webmanifest',
	'/icons/apple-touch-icon.png',
	'/icons/icon-192.png',
	'/icons/icon-512.png',
	'/icons/icon-maskable-512.png'
];

self.addEventListener('install', (event) => {
	event.waitUntil((async () => {
		const cache = await caches.open(SHELL_CACHE);
		// `reload` so an install never populates the cache from the cache it is replacing.
		await Promise.all(SHELL.map(async (path) => {
			try {
				const response = await fetch(new Request(path, { cache: 'reload' }));
				if (response.ok) await cache.put(path, response);
			} catch {
				// One missing asset must not abandon the whole install: the app still works online,
				// and the next install will pick it up.
			}
		}));
		await self.skipWaiting();
	})());
});

self.addEventListener('activate', (event) => {
	event.waitUntil((async () => {
		const names = await caches.keys();
		await Promise.all(names.filter((n) => n !== SHELL_CACHE).map((n) => caches.delete(n)));
		await self.clients.claim();
	})());
});

self.addEventListener('fetch', (event) => {
	const request = event.request;
	if (request.method !== 'GET') return;

	const url = new URL(request.url);

	// THE REFUSAL. Conversation, audio and pairing are never cached, never served from a cache, and
	// never touched by this worker at all.
	if (url.pathname.startsWith('/api/')) return;

	// A navigation is the one request that must always answer: opening the app where the Mac is not
	// reachable has to give him the app, not a browser error page.
	if (request.mode === 'navigate') {
		event.respondWith((async () => {
			try {
				return await fetch(request);
			} catch {
				const cache = await caches.open(SHELL_CACHE);
				return (await cache.match('/index.html')) || (await cache.match('/')) || Response.error();
			}
		})());
		return;
	}

	// The shell: from the network when it is there, so a new build reaches him without a dance; from
	// the cache when it is not.
	event.respondWith((async () => {
		const cache = await caches.open(SHELL_CACHE);
		try {
			const fresh = await fetch(request);
			if (fresh && fresh.ok && url.origin === self.location.origin) cache.put(request, fresh.clone());
			return fresh;
		} catch {
			const cached = await cache.match(request);
			if (cached) return cached;
			throw new Error('not available offline');
		}
	})());
});

// ---------------------------------------------------------------------------------------------
// Rich replied while the app was closed
// ---------------------------------------------------------------------------------------------

self.addEventListener('push', (event) => {
	let payload = {};
	try {
		payload = event.data ? event.data.json() : {};
	} catch {
		// An undecryptable or non-JSON body still has to show something, or WebKit may take the
		// subscription away. He gets an honest, contentless notification rather than nothing.
		payload = {};
	}

	const declared = payload.notification || {};
	const title = declared.title || 'Rich';
	let body = declared.body || 'Rich has something for you. Open the app to read it.';

	// THE LINK IS VALIDATED HERE, BEFORE IT REACHES ANYTHING. `target` below is handed to
	// `clients.openWindow()` and posted to the page, and `openWindow` will open a cross-origin
	// address — so this is the last point at which a value that came off the network is still
	// only data. A payload that names no link at all is the ordinary case and is not a refusal;
	// a payload that names one this app does not issue lands him on the app instead, and the
	// reason is written down rather than dropped (plan §2 C).
	const link = declared.navigate === undefined || declared.navigate === null
		? { ok: true, value: RichOSInbound.NAVIGATE_FALLBACK, reason: null }
		: RichOSInbound.validateNavigate(declared.navigate);
	const target = link.ok ? link.value : RichOSInbound.NAVIGATE_FALLBACK;

	event.waitUntil((async () => {
		if (!link.ok) await noteRefusal('navigate', declared.navigate, link.reason);

		// Written BEFORE the notification is shown: if the phone is closed again the moment he
		// swipes it away, the reply is still on the phone for the next time he opens the app.
		await rememberMessage(payload);

        let previews=false;
        try {const db=await RichOSStorage.open();try {previews=await RichOSStorage.settings(db).get('notificationPreviews',true);}finally{db.close();}} catch {}
        body=previews ? [...String(body).trim().replace(/\s+/g,' ')].slice(0,240).join('') : 'Rich has replied.';
        await self.registration.showNotification(title, {
			body,
			tag: payload.message && payload.message.thread_id ? `richos-${payload.message.thread_id}` : 'richos',
			renotify: true,
			requireInteraction: false,
			icon: '/icons/icon-192.png',
			badge: '/icons/icon-192.png',
			data: { url: target, threadId: payload.message && payload.message.thread_id }
		});

		if (self.navigator && self.navigator.setAppBadge) {
			try { await self.navigator.setAppBadge(1); } catch { /* not worth failing a notification over */ }
		}

		const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
		for (const client of windows) {
			client.postMessage({ type: 'push-message', message: payload.message || null, api_base: payload.api_base || null, tier: payload.tier || null });
		}
	})());
});

/// A value that arrived from outside and was refused, written down where it can be found.
///
/// "Refusing anything else with a reason" is the half of plan §2 C that makes the check honest
/// rather than merely safe: a phone that quietly ignores what its Mac tells it is a phone whose
/// misbehavior has no cause anybody can name. The stored shape is deliberately small — what
/// arrived, why it was refused, and when — and nothing reads it on the screen today, which is
/// stated here so nobody looks for a message that is not there.
async function noteRefusal(key, value, reason, existingDb) {
	try {
		const db = existingDb || (await RichOSStorage.open());
		await RichOSStorage.settings(db).set(`${key}Refusal`, {
			value: typeof value === 'string' ? value.slice(0, 200) : String(value),
			reason,
			at: new Date().toISOString()
		});
	} catch {
		// Storage refused, which is the same condition `rememberMessage` survives. The refusal
		// itself already happened; this is only the record of it.
	}
}

async function rememberMessage(payload) {
	if (!payload || !payload.message || !payload.message.id) return;
	try {
		const db = await RichOSStorage.open();
		const row = Object.assign({}, payload.message);
		// The Mac says when its own reply did not fit in a notification; the app renders that as
		// "the rest of this is waiting on your Mac" rather than as a complete answer.
		if (payload.truncated) row.truncated = true;
		await RichOSStorage.messages(db).put(row);
		await RichOSStorage.messages(db).trim();
		// The address the Mac can currently be reached at travels with the push (plan §10.7). It is
		// DATA, and this is one of the three places it is set — the other two are the stream's
		// `hello` and the pair response, both in `lib/api.js`, both through the same function.
		//
		// IT BECOMES THE PREFIX OF EVERY LATER REQUEST, so an unchecked value here redirects the
		// conversation, the send queue and the signed credential at whatever it names.
		if (payload.api_base) {
			const base = RichOSInbound.validateApiBase(payload.api_base, RichOSInbound.pageOrigin());
			if (base.ok) await RichOSStorage.settings(db).set('apiBase', base.value);
			else await noteRefusal('apiBase', payload.api_base, base.reason, db);
		}
	} catch {
		// Storage refused. The notification still shows, and the app will fetch the reply from the
		// Mac when it is next reachable — which is the whole reason the Mac is the record.
	}
}

self.addEventListener('notificationclick', (event) => {
	event.notification.close();
	// Re-validated rather than trusted. What is in `data.url` was built by the handler above and
	// is already safe, but a notification outlives the worker that showed it — it is read back out
	// of the system's own store, possibly by a later build — and the cost of proving that here
	// instead of remembering it is one function call.
	const held = event.notification.data && event.notification.data.url;
	const link = held ? RichOSInbound.validateNavigate(held) : null;
	const target = link && link.ok ? link.value : RichOSInbound.NAVIGATE_FALLBACK;

	event.waitUntil((async () => {
		if (self.navigator && self.navigator.clearAppBadge) {
			try { await self.navigator.clearAppBadge(); } catch { /* nothing depends on this */ }
		}

		const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
		for (const client of windows) {
			if ('focus' in client) {
				await client.focus();
				// `navigate()` is unreliable in an installed iOS web app; a message the page acts on
				// is both simpler and observable.
				client.postMessage({ type: 'notification-clicked', url: target });
				return;
			}
		}
		if (self.clients.openWindow) await self.clients.openWindow(target);
	})());
});
