// THE ONE THING THAT OWNS THE EVENT STREAM. Exactly one source is alive, ever.
//
// Under the first of the three base assumptions this whole channel is built on (CEO, §60 — the
// Mac is on 24/7 and is the server), the Mac is the side that stays and the PHONE is the side that
// comes and goes: asleep in a pocket, off Wi-Fi, on cellular, out of range on a walk. So a
// reconnect is the ordinary case for this app, not an edge, and everything below is about the
// ordinary case.
//
// FOUR THINGS IN HERE ARE THE DECISION, not implementation detail:
//
//   1. THE BROWSER'S OWN RECONNECT IS THE DEFECT, SO IT IS TURNED OFF. An `EventSource` reconnects
//      by itself, to THE URL IT WAS GIVEN — and this app's stream URL carries its credential in
//      the query string, because `EventSource` cannot send a header. That signature is over a
//      challenge that is dead ten minutes later (`app/src-tauri/src/phone/device.rs:62`:
//      CHALLENGE_LIFETIME_MS = 600_000 ms = 600 s = 10 minutes). So the browser's loop reopens a
//      refused URL forever and can never learn why, because an `EventSource` cannot read a
//      response header either. Calling `close()` on the source is what STOPS that loop, and it is
//      the first thing this file does on an error. One loop, and it is this one.
//
//   2. EVERY RE-OPEN IS NEWLY SIGNED, AND ASKS FOR A LIVE CHALLENGE FIRST. `open` is called again
//      rather than a URL being reused, and a retry — never the first connect — runs `refresh`
//      before it, which is the plain `fetch` in `lib/api.js` whose header this app CAN read. A
//      re-signature over a challenge the Mac has already forgotten is refused exactly as fast as
//      the original was, which is why re-signing ALONE does not recover a phone that was away for
//      more than ten minutes.
//
//   3. A GENERATION, NOT A BOOLEAN. Opening is asynchronous — it signs, and signing awaits. Two
//      overlapping calls therefore both reach `new EventSource` before either can be recorded, and
//      the old code's `if (stream) stream.close()` cannot see a source that does not exist yet.
//      That is how a second stream survives beside the first, unreferenced and unclosable, feeding
//      frames into the app forever. Every attempt here carries a generation; an attempt whose
//      generation has been superseded closes the source it just opened and delivers nothing.
//
//   4. COMING BACK ON SCREEN WAKES THE PENDING RETRY; IT NEVER OPENS A SECOND ONE. `wake()` is
//      what `visibilitychange` calls. If a retry is already scheduled it fires NOW rather than in
//      however many seconds were left — he is looking at the screen, so the wait is his wait. It
//      does not reset the backoff: he can foreground the app twenty times while the Mac is off,
//      and that must not become twenty immediate attempts a second apart.
//
// No dependencies. Loads as a plain script (defines `globalThis.RichOSLink`) and as a CommonJS
// module, the same as everything else in `lib/`.

(function (root, factory) {
	const link = factory();
	if (typeof module === 'object' && module.exports) module.exports = link;
	root.RichOSLink = link;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
	'use strict';

	// THE BACKOFF, AND THE ARITHMETIC IT PRODUCES — worked out here rather than asserted, because
	// what matters is the load two numbers put on his Mac and the wait they put in front of him.
	//
	// Doubling from 1 s, capped at 30 s, gives delays of 1, 2, 4, 8, 16, 30, 30, 30 … seconds, so
	// the attempt after an outage begins lands at t = 1 s, then 3 s, 7 s, 15 s, 31 s, 61 s, and
	// every 30 s after that. Five attempts inside the first minute, two a minute thereafter.
	//
	// A failed stream also makes one authenticated JSON probe (with at most one challenge
	// retry) to read a revocation refusal. Five retries therefore use at most 15 signed requests against the Mac's limit of 60 per
	// rolling minute (`device.rs`, RATE_LIMIT = 60), and at most ONE live stream against its limit
	// of 4 (MAX_STREAMS). A phone that reconnects all day cannot crowd out the phone.
	//
	// NO JITTER, deliberately. Jitter exists to break up a herd of clients retrying in step; this
	// channel has one phone and one Mac, so there is no herd, and a deterministic sequence is one
	// that can be told to a person and read off a log.
	const FIRST_RETRY_MS = 1000;
	const MAX_RETRY_MS = 30000;

	/// `open(handlers) -> Promise<source>` — MUST re-sign; it is called again for every attempt.
	/// `refresh() -> Promise<any>` — get a live challenge. Optional; failure is swallowed.
	/// `handlers` — the app's own frame handlers, forwarded verbatim from the live source only.
	/// `onState(which)` — 'opening' | 'open' | 'away', for the sentence on screen.
	function createLink(options) {
		const opts = options || {};
		const open = opts.open;
		if (typeof open !== 'function') throw new Error('createLink needs an `open`');
		const refresh = typeof opts.refresh === 'function' ? opts.refresh : null;
		const handlers = opts.handlers || {};
		const onState = opts.onState || function () {};
		const onFailure = opts.onFailure || function () {};
		const setTimer = opts.setTimeoutImpl || ((fn, ms) => setTimeout(fn, ms));
		const clearTimer = opts.clearTimeoutImpl || ((id) => clearTimeout(id));
		const firstRetryMs = opts.firstRetryMs || FIRST_RETRY_MS;
		const maxRetryMs = opts.maxRetryMs || MAX_RETRY_MS;

		let source = null;       // the one live source, or null. Never two.
		let generation = 0;      // bumped by every attempt; a superseded attempt closes its own source
		let timer = null;        // the pending retry, or null
		let delay = firstRetryMs;
		let opening = false;     // an attempt is in flight and has not produced a source yet
		let live = false;        // this source has said `hello` — the Mac accepted the credential
		let stopped = false;     // `close()` was called. Nothing reopens after this.
		let attempts = 0;        // every attempt ever made, for the harness

		function dropSource() {
			if (source) {
				// THE LINE THAT STOPS THE BROWSER'S OWN RETRY. Without it the old source keeps
				// reopening a URL whose signature is dead, beside whatever this file opens next.
				try { source.close(); } catch { /* already gone */ }
				source = null;
			}
			live = false;
		}

		function cancelRetry() {
			if (timer !== null) { clearTimer(timer); timer = null; }
		}

		function scheduleRetry() {
			if (stopped || timer !== null) return;
			const waited = delay;
			timer = setTimer(() => { timer = null; attempt(false); }, waited);
			// Advance AFTER scheduling, so the first wait is the first delay rather than the second.
			delay = Math.min(delay * 2, maxRetryMs);
		}

		/// The handlers the source is actually wired to. Gated on the generation, so a source this
		/// file has already given up on cannot move the app's state from underneath the live one.
		function wire(mine) {
			const forward = (name) => (data) => {
				if (mine !== generation || stopped) return;
				// ANY frame is proof the Mac accepted this credential — `app.js` already treated a
				// heartbeat that way, and it was right to. It matters most for the quiet reconnect
				// described below, where the first thing that ever arrives is a heartbeat.
				accepted();
				if (handlers[name]) handlers[name](data);
			};
			// WHAT COUNTS AS THE MAC HAVING ACCEPTED THIS RE-SIGNED URL — and it is NOT `hello`
			// alone, which is where the plan and the tree disagree.
			//
			// `hello` would be the obvious signal, and slice 2's brief names it. But this Mac only
			// sends one when it cannot replay: `stream.rs:168-191` answers a reconnect that
			// presents a cursor it still holds with `Replay::Tail`, and `stream.rs:189` answers the
			// ordinary case — reconnecting with nothing having happened meanwhile — with an EMPTY
			// tail. So the commonest successful reconnect on this channel sends no `hello` and, in
			// the quiet case, no frame at all. A backoff that reset only on `hello` would climb to
			// thirty seconds across a normal day of a phone going in and out of a pocket.
			//
			// `open` is the honest signal, and it is a positive one rather than an absence: an
			// `EventSource` fires `open` only on a 200 with `text/event-stream`, and this Mac
			// refuses an unverified stream with `Outcome::NotFound` (`routes.rs:406-411`) long
			// before any stream body exists. A stream that opened is a signature the Mac accepted.
			function accepted() {
				if (live) return;
				live = true;
				delay = firstRetryMs;
				onState('open');
			}

			return {
				open: () => {
					if (mine !== generation || stopped) return;
					accepted();
					if (handlers.open) handlers.open();
				},
				hello: (data) => {
					if (mine !== generation || stopped) return;
					// Also here, because a stub in a test can deliver `hello` without an `open`, and
					// because a `hello` is no weaker a proof than the `open` that carried it.
					accepted();
					if (handlers.hello) handlers.hello(data);
				},
				message: forward('message'),
				delta: forward('delta'),
				state: forward('state'),
				heartbeat: forward('heartbeat'),
				threads: forward('threads'),
				error: (err) => {
					if (mine !== generation || stopped) return;
					dropSource();
					if (err && err.reason === 'revoked') {
						stopped = true;
						cancelRetry();
						onFailure(err);
						return;
					}
					onState('away');
					if (handlers.error) handlers.error();
					scheduleRetry();
				}
			};
		}

		/// One attempt. `firstTry` skips the challenge probe: a fresh `connect()` is the app saying
		/// it wants a stream now, with the challenge it has, and a probe on every boot would be a
		/// request per launch for a credential that is almost always fine.
		function attempt(firstTry) {
			if (stopped) return;
			cancelRetry();
			dropSource();
			const mine = ++generation;
			opening = true;
			attempts += 1;
			onState('opening');

			Promise.resolve()
				.then(() => {
					if (firstTry || !refresh) return null;
					// A probe that fails tells us nothing we can act on — the stream attempt below
					// is the real test of reachability, and it produces the error the app reads.
					return refresh().catch(() => null);
				})
				.then(() => {
					if (mine !== generation || stopped) return null;
					return open(wire(mine));
				})
				.then((opened) => {
					if (!opened) return;
					if (mine !== generation || stopped) {
						// SUPERSEDED WHILE IT WAS SIGNING. This is the second stream that used to
						// survive beside the first; it is closed here and never referenced.
						try { opened.close(); } catch { /* already gone */ }
						return;
					}
					source = opened;
					opening = false;
				})
				.catch((err) => {
					if (mine !== generation || stopped) return;
					opening = false;
					onState('away');
					onFailure(err);
					scheduleRetry();
				});
		}

		return {
			/// The app wants a stream NOW: boot, and switching threads. Resets the backoff, because
			/// this is intent rather than a retry, and drops whatever was there.
			connect() {
				stopped = false;
				delay = firstRetryMs;
				attempt(true);
			},

			/// Back on screen. Never opens a second source, never resets the backoff.
			wake() {
				if (stopped) return;
				if (timer !== null) {
					// A retry is already owed. He is looking at the screen, so it is owed now.
					cancelRetry();
					attempt(false);
					return;
				}
				if (opening) return;                 // an attempt is already in flight
				if (source && live) return;          // there is a working stream; leave it alone
				if (source) {
					// A source that has not been accepted yet. A POSITIVE SIGNAL ONLY: `readyState`
					// 2 is CLOSED, which is the browser saying it has given up, and replacing it is
					// still exactly one source. CONNECTING — or a platform that does not report a
					// state at all — is not evidence of death, and a stream killed there is a
					// working handshake thrown away on a hunch. Its own `error` will arrive, and
					// that path schedules the retry.
					if (source.readyState === 2) attempt(false);
					return;
				}
				attempt(false);
			},

			/// Final. The Mac has forgotten this phone, so nothing reopens.
			close() {
				stopped = true;
				cancelRetry();
				dropSource();
				generation += 1;
				opening = false;
			},

			/// What the harness and the tests read. Nothing in the app reads these back.
			isOpen() { return live; },
			isPending() { return timer !== null; },
			nextDelayMs() { return delay; },
			attempts() { return attempts; },
			hasSource() { return source !== null; }
		};
	}

	return { createLink, FIRST_RETRY_MS, MAX_RETRY_MS };
});
