'use strict';

// THE RECONNECT, DRIVEN AGAINST A REAL SIGNER AND A STUB `EventSource`. `node --test "test/*.test.js"`.
//
// What is being held still here is the thing a phone cannot survive without, under the first of
// the CEO's three base assumptions (§60 — the Mac is on 24/7 and is the server): the Mac stays and
// the PHONE comes and goes, so reconnecting is the ordinary case for this app.
//
// The defect this file exists for, measured on the base of this branch (richos `48104748`): an
// error on the stream closed nothing, re-signed nothing and attempted nothing. The browser's own
// `EventSource` loop reopened the URL IT WAS GIVEN — whose credential is in the query string,
// because `EventSource` cannot send a header — and that signature is over a challenge that is dead
// ten minutes later (`app/src-tauri/src/phone/device.rs:62`, CHALLENGE_LIFETIME_MS = 600_000 ms =
// 600 s = 10 minutes). A refused reconnect cannot even be seen from the phone, because an
// `EventSource` cannot read a response header either. So the phone hammered a Mac that refused it,
// forever, and said "Your Mac isn't reachable from here" the whole time.
//
// The signatures below are REAL ECDSA P-256 signatures over the real signing string, made by
// `lib/api.js` through `openEvents`, so "the next attempt carries a different `auth`" is asserted
// about the bytes that would go on the wire and not about a counter.

const test = require('node:test');
const assert = require('node:assert');
const crypto = require('node:crypto');

const { createLink, FIRST_RETRY_MS, MAX_RETRY_MS } = require('../lib/link.js');
const { createApi } = require('../lib/api.js');

// --------------------------------------------------------------------------------------------
// The harness: a hand-driven clock, a stub `EventSource`, and a Mac that can be made to expire a
// challenge the way the real one does.
// --------------------------------------------------------------------------------------------

/// Timers the test advances by hand. Nothing here sleeps, so a 30-second backoff costs nothing.
function makeClock() {
	let now = 0;
	let next = 1;
	const pending = new Map();
	return {
		setTimeoutImpl(fn, ms) { const id = next++; pending.set(id, { at: now + ms, fn }); return id; },
		clearTimeoutImpl(id) { pending.delete(id); },
		/// Everything owed at or before `now + ms`, in order, one pass.
		async advance(ms) {
			now += ms;
			const due = [...pending.entries()].filter(([, t]) => t.at <= now).sort((a, b) => a[1].at - b[1].at);
			for (const [id, t] of due) { pending.delete(id); t.fn(); await settle(); }
		},
		waits() { return [...pending.values()].map((t) => t.at - now); },
		pendingCount() { return pending.size; }
	};
}

/// Let every already-resolved promise in the chain run. `attempt` is three `.then`s deep.
function settle() { return new Promise((resolve) => setImmediate(resolve)); }

function makeEventSource() {
	const made = [];
	class Stub {
		constructor(url) {
			this.url = url;
			this.handlers = {};
			this.readyState = 0; // CONNECTING
			made.push(this);
		}
		addEventListener(name, fn) { this.handlers[name] = fn; }
		close() { this.closed = true; this.readyState = 2; }
		/// The Mac accepts it: a 200 with `text/event-stream`, which is the only way `open` fires.
		accept() { this.readyState = 1; this.onopen(); }
		deliver(name, value) { this.handlers[name]({ data: JSON.stringify(value) }); }
		/// The Mac refuses it, or the network drops. The browser reports, and would then reopen
		/// THIS URL by itself — which is the loop the link has to stop by closing the source.
		fail() { this.readyState = 0; this.onerror(); }
	}
	Stub.made = made;
	Stub.alive = () => made.filter((s) => !s.closed);
	return Stub;
}

/// A Mac whose challenge ages out. `expire()` is `device.rs:418-424` from the phone's side: what
/// this phone is holding is no longer one the Mac will accept, and only an ordinary response —
/// never the stream — can hand it a live one.
function makeMac() {
	const mac = { issued: 0, challenge: 'challenge-one', probes: 0 };
	mac.expire = () => { mac.issued += 1; mac.challenge = `challenge-live-${mac.issued}`; };
	mac.fetchImpl = async (url) => {
		if (!new URL(url).pathname.endsWith('/api/challenge')) {
			return { status: mac.revoked ? 403 : 200, ok: !mac.revoked,
				headers: { get: () => null }, json: async () => ({ revoked: !!mac.revoked }),
				clone() { return this; } };
		}
		mac.probes += 1;
		return {
			status: 404,
			ok: false,
			headers: { get: (k) => (k === 'X-RichOS-Challenge' ? mac.challenge : null) },
			text: async () => '',
			json: async () => ({}),
			clone() { return this; }
		};
	};
	return mac;
}

function makeSigner() {
	const { privateKey } = crypto.generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
	return {
		deviceId: 'device-1',
		signed: [],
		async sign(input) {
			this.signed.push(input);
			return crypto.sign('sha256', Buffer.from(input, 'utf8'), { key: privateKey, dsaEncoding: 'ieee-p1363' }).toString('base64url');
		},
		async sha256Hex(payload) { return crypto.createHash('sha256').update(Buffer.from(payload)).digest('hex'); }
	};
}

/// The whole thing wired the way `app.js` wires it: one link, over the real api module, over a
/// stub `EventSource`.
function makePhone(overrides) {
	const Stub = makeEventSource();
	const clock = makeClock();
	const mac = makeMac();
	const signer = makeSigner();
	const state = { apiBase: 'https://mm1.tail9a3b2.ts.net:8443', challenge: 'challenge-one', deviceId: 'device-1' };
	const api = createApi({ state, signer, fetchImpl: mac.fetchImpl, eventSourceImpl: Stub });
	const states = [];
	const frames = [];
	const link = createLink(Object.assign({
		open: (handlers) => api.openEvents('t-1', 0, handlers),
		refresh: () => api.refreshChallenge(),
		handlers: {
			hello: (d) => frames.push(['hello', d]),
			message: (d) => frames.push(['message', d]),
			error: () => frames.push(['error'])
		},
		onState: (which) => states.push(which),
		setTimeoutImpl: clock.setTimeoutImpl,
		clearTimeoutImpl: clock.clearTimeoutImpl
	}, overrides || {}));
	return { Stub, clock, mac, signer, state, api, link, states, frames };
}

const authOf = (url) => new URL(url).searchParams.get('auth');

// --------------------------------------------------------------------------------------------
// THE COMPLETION CRITERION
// --------------------------------------------------------------------------------------------

test('after an error past the challenge\'s ten minutes, the next attempt is NEWLY SIGNED over a challenge the Mac will still accept', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();

	assert.strictEqual(p.Stub.made.length, 1);
	const first = p.Stub.made[0];
	first.accept();
	first.deliver('hello', { challenge: 'challenge-one' });
	assert.strictEqual(p.link.isOpen(), true);

	// He walks out of range. More than CHALLENGE_LIFETIME_MS passes — 600_000 ms, which is
	// 600 s, which is ten minutes — so what this phone is holding is no longer signable.
	p.mac.expire();
	first.fail();
	await settle();

	// THE SOURCE IS CLOSED BY US. This is what stops the browser reopening the dead URL.
	assert.strictEqual(first.closed, true, 'the browser is still reopening a URL the Mac refuses');
	assert.strictEqual(p.Stub.alive().length, 0, 'a closed-over source is still alive');
	assert.strictEqual(p.link.isPending(), true, 'nothing was scheduled, so nothing ever reconnects');

	await p.clock.advance(FIRST_RETRY_MS);

	assert.strictEqual(p.Stub.made.length, 2, 'no second attempt was made');
	const second = p.Stub.made[1];

	// DIFFERENT `auth`, and it is different for the reason that matters: a live challenge, not
	// just a fresh ECDSA nonce over the same dead one.
	assert.notStrictEqual(authOf(second.url), authOf(first.url), 'the reconnect reused the old signature');
	assert.strictEqual(p.mac.probes, 1, 'the retry did not ask the Mac for a live challenge');
	assert.strictEqual(p.state.challenge, 'challenge-live-1');
	assert.ok(p.signer.signed[0].startsWith('challenge-one\n'), p.signer.signed[0]);
	assert.ok(p.signer.signed.at(-1).startsWith('challenge-live-1\n'), p.signer.signed.at(-1));

	// And exactly one source is alive through all of it.
	assert.strictEqual(p.Stub.alive().length, 1);
});

test('the backoff is 1, 2, 4, 8, 16, 30, 30 seconds — and it resets only when the Mac accepts one', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	p.Stub.made[0].accept();

	const waits = [];
	for (let i = 0; i < 7; i += 1) {
		p.Stub.made[p.Stub.made.length - 1].fail();
		await settle();
		waits.push(p.clock.waits()[0]);
		await p.clock.advance(p.clock.waits()[0]);
	}
	assert.deepStrictEqual(waits, [1000, 2000, 4000, 8000, 16000, 30000, 30000]);
	assert.strictEqual(MAX_RETRY_MS, 30000);

	// t = 1 + 2 + 4 + 8 + 16 = 31 s to reach the cap, across 5 attempts; 2 a minute after that.
	assert.strictEqual(waits.slice(0, 5).reduce((a, b) => a + b, 0), 31000);

	// Every one of those was a real attempt, and never two sources at once.
	assert.strictEqual(p.Stub.made.length, 8);
	assert.strictEqual(p.Stub.alive().length, 1);

	// The Mac takes one, and the wait goes back to where it started.
	p.Stub.made[7].accept();
	assert.strictEqual(p.link.nextDelayMs(), FIRST_RETRY_MS);
});

test('the backoff resets on `open` and not only on `hello`, because this Mac replays a tail with no `hello` in it', async () => {
	// `stream.rs:168-191`: a reconnect presenting a cursor the hub still holds is answered with
	// `Replay::Tail`, and `stream.rs:189` answers the quiet case with an EMPTY tail. So the
	// commonest successful reconnect sends no `hello` and, with nothing having happened, no frame
	// at all. A backoff that waited for `hello` would climb to thirty seconds across a normal day.
	const p = makePhone();
	p.link.connect();
	await settle();
	p.Stub.made[0].accept();
	p.Stub.made[0].fail();
	await settle();
	await p.clock.advance(1000);

	const second = p.Stub.made[1];
	second.accept();                         // 200 text/event-stream, and then silence
	assert.strictEqual(p.link.isOpen(), true, 'a stream the Mac accepted was not counted as accepted');
	assert.strictEqual(p.link.nextDelayMs(), FIRST_RETRY_MS, 'the wait kept growing through a healthy reconnect');
	assert.deepStrictEqual(p.frames.filter((f) => f[0] === 'hello'), [], 'this test is not testing what it says it is');
});

test('a heartbeat is proof the Mac accepted this credential, even with no `open` and no `hello`', async () => {
	// `app.js` already read a heartbeat this way before any of this landed, and it was right to:
	// a frame only arrives down a stream the Mac verified. It is the case that matters after the
	// quiet reconnect above, where the first thing that ever arrives is a heartbeat.
	const p = makePhone();
	p.link.connect();
	await settle();
	p.Stub.made[0].fail();
	await settle();
	await p.clock.advance(1000);
	assert.strictEqual(p.link.isOpen(), false);

	p.Stub.made[1].deliver('heartbeat', {});
	assert.strictEqual(p.link.isOpen(), true, 'a live stream was still counted as away');
	assert.strictEqual(p.link.nextDelayMs(), FIRST_RETRY_MS);
	assert.deepStrictEqual(p.states, ['opening', 'away', 'opening', 'open']);
});

test('coming back on screen during a pending retry produces ONE open, not two', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	p.Stub.made[0].accept();

	p.Stub.made[0].fail();
	await settle();
	assert.strictEqual(p.link.isPending(), true);
	assert.strictEqual(p.clock.pendingCount(), 1);

	// `visibilitychange`: he is looking at the screen, so the wait that is owed is owed NOW.
	p.link.wake();
	await settle();

	assert.strictEqual(p.Stub.made.length, 2, `${p.Stub.made.length} sources were opened, not one`);
	assert.strictEqual(p.Stub.alive().length, 1);
	assert.strictEqual(p.clock.pendingCount(), 0, 'the scheduled retry is still owed beside the one that just ran');

	// And the timer that was dropped does not fire a second source when its time arrives.
	await p.clock.advance(60000);
	assert.strictEqual(p.Stub.made.length, 2, 'the retry that was dropped opened a source anyway');
	assert.strictEqual(p.Stub.alive().length, 1);
});

test('waking twenty times while the Mac is off does not become twenty attempts a second apart', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	p.Stub.made[0].fail();
	await settle();

	for (let i = 0; i < 20; i += 1) {
		p.link.wake();
		await settle();
		// The invariant holds at the one moment it could break: a wake that fired a retry.
		assert.ok(p.Stub.alive().length <= 1, `${p.Stub.alive().length} sources alive after wake ${i}`);
		p.Stub.made[p.Stub.made.length - 1].fail();
		await settle();
		assert.strictEqual(p.Stub.alive().length, 0, 'a refused source was left open');
	}
	// Each wake spends the wait it was owed; it never resets it. So the wait has grown to the cap
	// rather than staying at one second, which is what twenty foregrounds would otherwise cost the
	// Mac while it is off.
	assert.strictEqual(p.link.nextDelayMs(), MAX_RETRY_MS);
	assert.strictEqual(p.Stub.made.length, 21);
});

test('waking on a working stream leaves it exactly where it is', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	p.Stub.made[0].accept();

	p.link.wake();
	await settle();
	assert.strictEqual(p.Stub.made.length, 1, 'a second stream was opened beside a working one');
	assert.strictEqual(p.Stub.made[0].closed, undefined);
});

test('waking on a source that is still shaking hands leaves it alone — CONNECTING is not evidence of death', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	assert.strictEqual(p.Stub.made[0].readyState, 0);

	p.link.wake();
	await settle();
	assert.strictEqual(p.Stub.made.length, 1, 'a handshake in progress was thrown away on a hunch');

	// A source the BROWSER has given up on — readyState 2, CLOSED — is a positive signal, and
	// that one is replaced.
	p.Stub.made[0].readyState = 2;
	p.link.wake();
	await settle();
	assert.strictEqual(p.Stub.made.length, 2);
	assert.strictEqual(p.Stub.alive().length, 1);
});

// --------------------------------------------------------------------------------------------
// ONE SOURCE, UNDER EVERY RACE THE OLD CODE LOST
// --------------------------------------------------------------------------------------------

test('a source that finishes opening AFTER it was superseded closes itself and is never held', async () => {
	// THE RACE THE OLD `if (stream) stream.close()` COULD NOT WIN, and it is worth being exact
	// about where it lives. Opening is asynchronous because it SIGNS, so between the call and the
	// source existing there is a window in which `stream` is still null and the old guard had
	// nothing to close. A second connect inside that window left the first source unreferenced,
	// unclosable, and feeding the app forever.
	//
	// The generation is checked twice for that reason — once before `open` is even called, which
	// is why three connects in a row only ever construct one source, and once when the promise
	// comes back, which is the case below: the attempt was ALREADY INSIDE `open` when it lost.
	const opened = [];
	let release = null;
	const p = makePhone({
		open: () => new Promise((resolve) => {
			release = () => {
				const source = { closed: false, readyState: 0, addEventListener() {}, close() { this.closed = true; } };
				opened.push(source);
				resolve(source);
			};
		})
	});

	p.link.connect();                 // attempt 1 is now inside `open`, awaiting the signature
	await settle();
	const releaseFirst = release;
	assert.ok(releaseFirst, 'the harness did not reach `open`');

	p.link.connect();                 // he switched threads; attempt 1 has just lost
	await settle();
	const releaseSecond = release;

	releaseFirst();                   // the signature finally comes back for the attempt that lost
	await settle();
	assert.strictEqual(opened.length, 1);
	assert.strictEqual(opened[0].closed, true, 'the source that lost the race is alive and unreachable');
	assert.strictEqual(p.link.hasSource(), false, 'the link is holding the source that lost');

	releaseSecond();
	await settle();
	assert.strictEqual(opened.length, 2);
	assert.strictEqual(opened[1].closed, false);
	assert.strictEqual(p.link.hasSource(), true, 'the winner was dropped along with the loser');
});

test('three connects in a row construct ONE source, because the generation is checked before `open` is called', async () => {
	const p = makePhone();
	p.link.connect();
	p.link.connect();
	p.link.connect();
	await settle();
	await settle();

	assert.strictEqual(p.Stub.made.length, 1, `${p.Stub.made.length} sources were constructed for three connects`);
	assert.strictEqual(p.Stub.alive().length, 1);
});

test('a superseded source delivers nothing to the app, even if the Mac is still talking to it', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	const first = p.Stub.made[0];
	first.accept();

	p.link.connect();          // he switched threads
	await settle();

	first.deliver('message', { cursor: 9, text: 'from the stream that lost' });
	assert.deepStrictEqual(p.frames.filter((f) => f[0] === 'message'), [], 'a superseded stream moved the app\'s state');

	p.Stub.made[1].deliver('message', { cursor: 10, text: 'from the one that owns it' });
	assert.strictEqual(p.frames.filter((f) => f[0] === 'message').length, 1);
});

test('an error on a superseded source schedules nothing — one loop, not two', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	const first = p.Stub.made[0];
	first.accept();

	p.link.connect();
	await settle();
	assert.strictEqual(p.clock.pendingCount(), 0);

	first.fail();              // the corpse twitches
	await settle();
	assert.strictEqual(p.clock.pendingCount(), 0, 'a dead source started a second retry loop');
	assert.strictEqual(p.Stub.made.length, 2);
});

test('a Mac that has forgotten this phone is final: close stops the loop and nothing reopens', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	p.Stub.made[0].accept();
	p.Stub.made[0].fail();
	await settle();
	assert.strictEqual(p.link.isPending(), true);

	p.link.close();
	assert.strictEqual(p.link.isPending(), false);

	await p.clock.advance(120000);
	p.link.wake();
	await settle();
	assert.strictEqual(p.Stub.made.length, 1, 'a phone the Mac revoked went on knocking');
	assert.strictEqual(p.Stub.alive().length, 0);
});

test('a probe that fails does not abort the attempt — the stream is the real test of reachability', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	p.Stub.made[0].accept();
	p.mac.fetchImpl = async () => { throw new TypeError('Load failed'); };
	p.Stub.made[0].fail();
	await settle();

	await p.clock.advance(1000);
	assert.strictEqual(p.Stub.made.length, 2, 'a failed challenge probe swallowed the reconnect');
	assert.strictEqual(p.state.challenge, 'challenge-one', 'a failed probe changed the stored challenge');
});

test('the first connect does not spend a request on a challenge that is almost always fine', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	assert.strictEqual(p.mac.probes, 0, 'every launch now costs an extra request');
	assert.strictEqual(p.Stub.made.length, 1);
});

test('the screen is told opening, then open, then away — and never open on a stream that was refused', async () => {
	const p = makePhone();
	p.link.connect();
	await settle();
	assert.deepStrictEqual(p.states, ['opening']);

	p.Stub.made[0].accept();
	assert.deepStrictEqual(p.states, ['opening', 'open']);

	p.Stub.made[0].fail();
	await settle();
	assert.deepStrictEqual(p.states, ['opening', 'open', 'away']);

	await p.clock.advance(1000);
	assert.deepStrictEqual(p.states, ['opening', 'open', 'away', 'opening']);
	// The re-opened stream is refused too: it says away again and never open.
	p.Stub.made[1].fail();
	await settle();
	assert.deepStrictEqual(p.states, ['opening', 'open', 'away', 'opening', 'away']);
});

test('an `open` that throws is an outage like any other, and the loop survives it', async () => {
	const failures = [];
	const p = makePhone({
		open: () => Promise.reject(new Error('this browser has no event stream')),
		onFailure: (err) => failures.push(err.message)
	});
	p.link.connect();
	await settle();
	await settle();
	assert.strictEqual(p.link.isPending(), true, 'a throwing open left the phone with no way back');
	assert.deepStrictEqual(p.states, ['opening', 'away']);
	assert.deepStrictEqual(failures, ['this browser has no event stream']);
});


test('a revoked event stream stops retries and reports the explicit refusal without caller intervention', async () => {
	const errors = [];
	const p = makePhone({ onFailure: (err) => errors.push(err) });
	p.link.connect();
	await settle();
	p.mac.revoked = true;
	p.Stub.made[0].fail();
	await settle();
	assert.strictEqual(errors.length, 1);
	assert.strictEqual(errors[0].reason, 'revoked');
	assert.strictEqual(p.link.isPending(), false);
	p.link.wake();
	await p.clock.advance(60000);
	assert.strictEqual(p.Stub.made.length, 1);
	assert.strictEqual(p.Stub.alive().length, 0);
});
