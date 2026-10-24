// THE ONE PLACE THAT KNOWS THE ROUTES.
//
// `CONTRACT-STUB.md` in this directory says why this module exists as a module: the real route
// contract is Echo's and it was not in the tree when the phone was written, so every route, every
// header and every field name is behind this file. Reconciling with the real contract is an edit
// here and to `test/api.test.js`, and nothing above it moves.
//
// THREE THINGS IN HERE ARE LOAD-BEARING AND ARE NOT DETAILS:
//
//   1. THE API BASE IS DATA, THE ORIGIN IS IDENTITY (plan §10.7). Nothing in this file assumes the
//      Mac is at the address the app was loaded from. `state.apiBase` is read at EVERY request, it
//      starts as the origin, and it is replaced by whatever the Mac last advertised — in the stream's
//      `hello` or in a push payload. Without that seam a public address that changes orphans the
//      installed app, the push subscription and the cache, and no push can fix it because the push
//      is delivered TO the origin it is trying to replace. With it, every later way of reaching him
//      from away is an implementation of one interface.
//
//   2. THE SIGNATURE IS OVER THE SERVER'S OWN CHALLENGE. The device key is a non-extractable P-256
//      key (plan §2.7); the Mac issues the challenge; the phone replays the most recent one it was
//      given. Ordering is never client-generated, and neither is the thing being signed.
//
//   3. FAILURES ARE CLASSIFIED, because the whole honesty of the send queue rests on telling
//      "your Mac is not reachable from here" apart from "your Mac refused this". A `TypeError` from
//      `fetch` is unreachable; a 403 carrying `{"revoked":true}` is final and must never be retried;
//      a 404 is the flat refusal §2.5 specifies for an unauthorized caller and is also not a retry.
//      Everything else is a fault with a status attached.
//
// No dependencies, no framework, and no `EventSource` unless the platform gives one. Loads as a
// plain script (defines `globalThis.RichOSApi`) and as a CommonJS module.

(function (root, factory) {
	const api = factory();
	if (typeof module === 'object' && module.exports) module.exports = api;
	root.RichOSApi = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
	'use strict';

	// The reasons a request can fail, as data. The UI matches on these and never on a message
	// string, so the sentence he reads can be rewritten without changing behavior.
	const UNREACHABLE = 'unreachable';
	const REVOKED = 'revoked';
	const REFUSED = 'refused';
	const FAULT = 'fault';

	/// WHERE THE PHONE KNOCKS TO GET A CHALLENGE IT CAN SIGN. Read `refreshChallenge` below for why
	/// this exists at all; what matters about the PATH is three things:
	///
	///   1. It is under `/api/`, so `sw.js` refuses to touch it (`sw.js` line 83 returns without
	///      responding for every `/api/` request). A probe the service worker could answer from its
	///      shell cache would hand back a response with no challenge header and no Mac behind it.
	///   2. Today's Mac has no such route, so it falls through to the static table, misses, and
	///      answers 404 — WITH a fresh challenge, because every answer on that port carries one
	///      (`app/src-tauri/src/phone/listen.rs:334-342`, and its test at `listen.rs:557` pins the
	///      404 case by name). The status is deliberately not read here.
	///   3. A later Mac can answer it 200 with an empty body and this file does not change.
	const CHALLENGE_PROBE = '/api/challenge';

	/// The rules for the values that arrive from outside (`lib/inbound.js`). Resolved lazily so
	/// script order cannot matter: the page has it as a global long before a `hello` arrives, the
	/// service worker `importScripts`es it, and Node resolves it from disk on first use.
	let INBOUND = null;
	function inbound() {
		if (INBOUND) return INBOUND;
		const here = typeof globalThis !== 'undefined' ? globalThis : null;
		if (here && here.RichOSInbound) INBOUND = here.RichOSInbound;
		else if (typeof require === 'function') INBOUND = require('./inbound.js');
		else throw new Error('lib/inbound.js is not loaded, so nothing can validate what the Mac sends');
		return INBOUND;
	}

	/// THE CAPABILITY A v2 PHONE REQUIRES (Sage's pairing review §3.5, ledger row S3). A Mac that
	/// does not name it derives the old six words, which bind nothing about the connection, and a
	/// relay can strip a capability — so this phone refuses such a Mac rather than falling back.
	const PAIR_V2 = 'pair-v2';

	/// **HOW THIS PHONE WAITS FOR THE PRESS ON THE MAC** (review §3.1 step 5, CEO ruling §81).
	///
	/// After "They match" on the phone, the Mac still refuses everything until the person presses
	/// "They match" on the Mac, and says so with a retryable 409. The phone asks again on this
	/// schedule and on no other: 2, 3, 5, 8 and 13 seconds, then every 15, and never past the bound
	/// the Mac gave (`confirm_within_seconds`, at most the five-minute window), and only while the
	/// app is on screen. The arithmetic, stated rather than trusted: the first five waits are
	/// 2 + 3 + 5 + 8 + 13 = 31 s, then floor((300 - 31) / 15) = 17 more, so at most 22 requests in
	/// the whole five minutes and none before the first 2 s — a person standing at his Mac, not a
	/// background refresh (`test/api.test.js` recomputes the 22 from this schedule).
	const MAC_WAIT_DELAYS_MS = [2000, 3000, 5000, 8000, 13000];
	const MAC_WAIT_CAP_MS = 15000;
	const MAC_WAIT_WINDOW_MS = 300000;

	function macWaitDelayMs(attempt) {
		const n = Math.max(0, Math.floor(Number(attempt) || 0));
		return n < MAC_WAIT_DELAYS_MS.length ? MAC_WAIT_DELAYS_MS[n] : MAC_WAIT_CAP_MS;
	}

	/// **THE MAC CAN HOLD THIS PHONE'S ANSWER UNTIL THE PRESS** — Sage's pair-v2 hypotheses review
	/// §1, "The fix" points 2 and 4, and the capability pattern of ledger row S3. A Mac that names
	/// `pair-wait` answers a `They match` sent with `Prefer: wait=N` the moment the person presses
	/// on the Mac, holding it for at most `PAIR_WAIT_SECONDS`. A Mac that does not name it is sent
	/// no `Prefer` and the wait keeps `macWaitDelayMs`'s schedule, exactly as before.
	const PAIR_WAIT = 'pair-wait';
	const PAIR_WAIT_SECONDS = 14;

	/// **THE NO-SPIN RULE WHILE THE MAC HOLDS**: two asks never start less than this far apart. A
	/// relay that forges `pair-wait` and answers at once therefore degrades to one ask every 7 s at
	/// the most — floor(300 / 7) + 1 = 43 in the five-minute window, under the Mac's 60-a-minute
	/// device bucket — and never to a tight loop.
	const MAC_WAIT_MIN_SPACING_MS = 7000;

	/// **How long to wait before the next ask.** `tookMs` is how long the previous ask took from
	/// sending to its answer; `holds` is whether the Mac named `pair-wait`.
	///
	/// - Not holding: today's schedule, `macWaitDelayMs(attempt)`, unchanged.
	/// - Holding, and the answer took at least 7 s: ask again at once — the Mac held it, and the
	///   next hold starts where this one ended.
	/// - Holding, and the answer came sooner: the scheduled delay, but never less than what keeps
	///   the two asks 7 s apart.
	function macWaitNextDelayMs(attempt, tookMs, holds) {
		const scheduled = macWaitDelayMs(attempt);
		if (!holds) return scheduled;
		const took = Math.max(0, Number(tookMs) || 0);
		if (took >= MAC_WAIT_MIN_SPACING_MS) return 0;
		return Math.max(scheduled, MAC_WAIT_MIN_SPACING_MS - took);
	}

	/// **WHEN THE NEXT ASK GOES**, as a moment on the same clock as the arguments: the previous ask
	/// started at `askedAt` and was answered at `answeredAt`, and `until` is the phone's deadline.
	///
	/// `macWaitNextDelayMs` after the answer, and never later than the deadline — where the LAST
	/// ask goes (Sage's pair-v2 hypotheses review §2: one final ask, so a press that landed after
	/// the previous ask is heard). While the Mac holds, the last ask still keeps the 7 s spacing,
	/// so it may go a few seconds after the deadline; that answer is as settled as one at the
	/// deadline, because the phone's deadline is at or after the Mac's own `confirm_by` (Sage §2b).
	/// A Mac that does not hold keeps today's rule: the last ask at the deadline itself.
	function macWaitNextAskAt(attempt, askedAt, answeredAt, until, holds) {
		const next = answeredAt + macWaitNextDelayMs(attempt, answeredAt - askedAt, holds);
		if (next <= until) return next;
		return holds ? Math.max(until, askedAt + MAC_WAIT_MIN_SPACING_MS) : until;
	}

	/// How long the wait may last, from the pair answer. A Mac that says nothing gets the window;
	/// a Mac that says more than the window is not believed.
	function macWaitBoundMs(answer) {
		const s = Number(answer && answer.confirm_within_seconds);
		return Number.isFinite(s) && s > 0 ? Math.min(s * 1000, MAC_WAIT_WINDOW_MS) : MAC_WAIT_WINDOW_MS;
	}

	class ApiError extends Error {
		constructor(reason, message, status, aboutThisMessage) {
			super(message);
			this.name = 'ApiError';
			this.reason = reason;
			this.status = status || 0;
			// The only two reasons worth trying again: the Mac was not reachable, or it faulted in a
			// way that might not repeat. A revoked device and a flat refusal are answers, not
			// accidents, and retrying them is how a phone hammers a Mac that has forgotten it.
			this.retryable = reason === UNREACHABLE || reason === FAULT;
			// AND THE SECOND QUESTION THE QUEUE HAS TO ASK: was this about the MESSAGE, or about the
			// PHONE? A refusal the Mac marked final is about this one item — this build cannot take a
			// voice note, and the text behind it is unaffected. A forgotten device or a flat 404 is
			// about the phone, applies identically to everything still queued, and the flush stops.
			// Only `queue.js` reads this, and only to decide whether to carry on down the queue.
			this.aboutThisMessage = aboutThisMessage === true;
			// Set by `request` on the Mac's awaiting answer (a 409 carrying
			// `awaiting_mac_confirmation`) and by `pair` on a Mac too old for `pair-v2`.
			this.awaitingMac = false;
			this.macNeedsUpdate = false;
		}
	}

	function joinBase(base, path) {
		if (!base) throw new ApiError(FAULT, 'no address for your Mac is stored yet');
		return base.replace(/\/+$/, '') + path;
	}

	/// Did the Mac mark this refusal FINAL? Returns the parsed body when it did, and `null` for
	/// every other shape — no body, a body that is not JSON, `retry` absent, or `retry` present as
	/// anything but the boolean `false`. Read off a CLONE so the caller's `text()` is untouched.
	async function finalRefusal(response) {
		let body = null;
		try { body = await response.clone().json(); } catch { return null; }
		if (!body || typeof body !== 'object') return null;
		return body.retry === false ? body : null;
	}

	/// WHAT THIS MAC CAN ACTUALLY DO, and it is DEFAULT-DENY (plan §2 A). The phone shipped a
	/// "Hold to record" button against a Mac that answers every voice note 503, so a control is now
	/// rendered only where the Mac has named the capability behind it. A Mac that says nothing —
	/// which is every build before this one, and not one of them could take a voice note — offers
	/// nothing. Exported because it is the whole decision, and the screen is not where it is tested.
	function offers(capabilities, name) {
		return Array.isArray(capabilities) && capabilities.indexOf(name) !== -1;
	}

	// The string that gets signed. Written once, here, because the Mac has to build the identical
	// one and a difference of a newline is a day of two people being sure they are right.
	function signingInput(challenge, method, pathWithQuery, bodyHashHex) {
		return `${challenge}\n${method.toUpperCase()}\n${pathWithQuery}\n${bodyHashHex || ''}`;
	}

	/// `signer` is the only thing this module needs from the platform's crypto:
	///   { deviceId, sign(utf8String) -> Promise<base64url string>, sha256Hex(bytesOrString) -> Promise<hex> }
	/// The browser implements it over WebCrypto with the non-extractable key; the tests implement it
	/// over `node:crypto` with a key the test owns. Same code path, both times.
	function createApi(options) {
		const opts = options || {};
		const fetchImpl = opts.fetchImpl || (typeof fetch === 'function' ? fetch.bind(globalThis) : null);
		const EventSourceImpl = opts.eventSourceImpl || (typeof EventSource === 'function' ? EventSource : null);
		const state = opts.state;              // { apiBase, challenge, deviceId }
		const signer = opts.signer || null;
		const onState = opts.onState || function () {};
		// THE PAGE'S OWN ORIGIN — the app's identity, which never changes, as opposed to
		// `state.apiBase`, which is data and does. It is what every advertised `api_base` is
		// measured against until the Mac's listener speaks CORS (plan §2 C). The browser has it on
		// `location`; a test has no page and passes one in. Unknown is a REFUSAL of every
		// advertised base and never a pass — see `inbound.validateApiBase`.
		const origin = typeof opts.origin === 'string' && opts.origin ? opts.origin : inbound().pageOrigin();

		if (!state) throw new Error('createApi needs a state object');

		/// How many requests had to be re-signed because the Mac had replaced the challenge this
		/// phone was holding. Read by the tests and by nothing in the app — it is the count that
		/// says whether the Mac's challenge set is still being emptied underneath the phone, and
		/// a build where it climbs on an idle phone has the Mac-side half of this defect back.
		let staleCredentialRetries = 0;

		function setChallenge(next) {
			if (next && next !== state.challenge) {
				state.challenge = next;
				onState(state);
			}
		}

		/// THE ONE DOOR THE ADVERTISED ADDRESS COMES THROUGH, and it is now a checked one.
		///
		/// Two of the three places `api_base` is set are here — the stream's `hello` and the pair
		/// response — and the third is the service worker's push handler, which calls the same
		/// `inbound.validateApiBase`. It used to take any string at all and store it; every later
		/// request is `base + path` (`joinBase`), so that was "point this phone's whole
		/// conversation, its send queue and its signed credential wherever this frame says".
		///
		/// A frame that says nothing about the address is not a refusal — the stub Mac and every
		/// build before the address desk existed send `api_base: null`, and silence means "carry
		/// on with what you have". A frame that names an address this app cannot use IS a refusal,
		/// and it leaves a reason on the state rather than a phone that quietly stopped moving.
		function setApiBase(next) {
			if (next === undefined || next === null || next === '') return;
			const checked = inbound().validateApiBase(next, origin);
			if (!checked.ok) {
				state.apiBaseRefusal = {
					value: String(next).slice(0, 200),
					reason: checked.reason,
					at: new Date().toISOString()
				};
				onState(state);
				return;
			}
			// A refusal that has been superseded is a stale claim, so it does not outlive the
			// address that replaced it — and clearing it is itself a change worth persisting,
			// which is why `changed` exists rather than a second `onState` call.
			let changed = false;
			if (state.apiBaseRefusal) {
				state.apiBaseRefusal = null;
				changed = true;
			}
			if (checked.value !== state.apiBase) {
				state.apiBase = checked.value;
				changed = true;
			}
			if (changed) onState(state);
		}

		/// REPLACED on every `hello`, never merged. A capability that was true once is not evidence
		/// about the Mac answering now — he could have downgraded, or this could be a different Mac
		/// at the same address — so an absent list puts the phone back to offering nothing rather
		/// than leaving a control on screen on the strength of a frame that is no longer current.
		function setCapabilities(next, build) {
			state.capabilities = Array.isArray(next) ? next.slice() : [];
			state.build = typeof build === 'string' ? build : null;
			onState(state);
		}

		async function authorization(method, pathWithQuery, bodyHashHex) {
			if (!signer || !state.deviceId || !state.challenge) return null;
			const signature = await signer.sign(signingInput(state.challenge, method, pathWithQuery, bodyHashHex));
			return `RichOS-Device ${state.deviceId}.${state.challenge}.${signature}`;
		}

		/// `options.credential` says WHERE THIS ROUTE READS THE CREDENTIAL, and it is not a style
		/// choice — it is a property of the route on the Mac.
		///
		///   `'header'` (the default) — `POST /api/messages` (`routes.rs:304`),
		///     `GET /api/audio/…` (`:504`), `POST /api/pair` as the device record (`:210`). Each of
		///     those reads `request.authorization` and nothing else.
		///   `'query'` — `GET /api/events` (`:403-411`), which reads
		///     `query_value(&request.query, "auth")` and nothing else. `grep -n
		///     'request.authorization' routes.rs` returns `:210`, `:304`, `:504` and NOTHING inside
		///     `events()`, so a header on that route is not a fallback, it is invisible.
		///
		/// Either way the signature covers the path WITHOUT the credential, because the Mac strips
		/// `auth` before it verifies (`routes.rs:572-586` `signed_path`).
		///
		/// `options.headers` are added UNSIGNED — `Prefer` is the only one, and it is unsigned on
		/// purpose (Sage's review, "Security, plainly": stripping or forging it changes only how
		/// long an answer takes). `options.signal` is an `AbortSignal`; an ask it cancels is thrown
		/// as unreachable with `aborted: true`, so a caller can tell "I stopped it" from "the Mac
		/// could not be reached".
		async function request(method, pathWithQuery, body, contentType, options) {
			if (!fetchImpl) throw new ApiError(FAULT, 'this browser has no fetch');
			const inQuery = !!(options && options.credential === 'query');
			const signal = options && options.signal ? options.signal : undefined;
			const baseUrl = joinBase(state.apiBase, pathWithQuery);
			const headers = Object.assign({}, options && options.headers);
			let payload;
			if (body !== undefined && body !== null) {
				payload = opts.isBodyReference?.(body) || typeof body === 'string' || body instanceof Uint8Array || body instanceof ArrayBuffer
					? body
					: JSON.stringify(body);
				headers['Content-Type'] = contentType || 'application/json';
			}
			const bodyHash = payload === undefined ? '' : await signer.sha256Hex(payload);

			/// One signed attempt. Returns the answer together with the challenge this attempt
			/// actually SIGNED, which is the thing the stale-credential test below compares
			/// against — `state.challenge` has already moved on by the time it is read.
			async function attempt() {
				const presented = state.challenge;
				let url = baseUrl;
				const sent = Object.assign({}, headers);
				const auth = await authorization(method, pathWithQuery, bodyHash);
				if (auth && inQuery) {
					// The separator is derived rather than assumed, for the same reason `openEvents`
					// derives it: a path that ever lost its query string would otherwise produce
					// `…/api/events&auth=`, which the Mac refuses for a reason nobody would find
					// quickly. Appended LAST, so `signed_path`'s filter leaves the rest in order.
					url += `${url.includes('?') ? '&' : '?'}auth=${encodeURIComponent(auth)}`;
				} else if (auth) {
					sent.Authorization = auth;
				}
				let answer;
				try {
					const init = { method, headers: sent, body: payload, mode: 'cors', cache: 'no-store' };
					if (signal) init.signal = signal;
					answer = await fetchImpl(url, init);
				} catch (err) {
					// This is the branch the queue exists for. A `fetch` that throws did not reach the
					// Mac at all — wrong network, Mac asleep, name not resolving here.
					const failed = new ApiError(UNREACHABLE, String((err && err.message) || err));
					failed.aborted = !!(signal && signal.aborted);
					throw failed;
				}
				const next = answer.headers && answer.headers.get
					? answer.headers.get('X-RichOS-Challenge')
					: null;
				setChallenge(next);
				return { answer, presented, next, credentialed: Boolean(auth) };
			}

			let { answer: response, presented, next, credentialed } = await attempt();

			// **A 404 THAT HANDED BACK A DIFFERENT CHALLENGE IS A STALE CREDENTIAL, NOT A REFUSAL,
			// AND IT IS THE CAUSE OF THE SEND THAT NEVER ARRIVED** (Ray, nightly `.7`, defect 1).
			//
			// Every answer on this port carries `X-RichOS-Challenge`
			// (`app/src-tauri/src/phone/listen.rs` `render`), and the Mac keeps only the most
			// recent `LIVE_CHALLENGES` of them (`phone/device.rs` `issue_challenge`). EVERY
			// response burns one of those slots, including the flat static-asset GETs this very
			// app is served over — and `sw.js` re-fetches its whole shell with `cache: 'reload'`
			// on install, nineteen entries in one burst. A burst wider than that set pushes out
			// the challenge this phone is holding, and the next signed request is refused with
			// `UnknownChallenge`, which §2.5 item 4 renders as a flat 404 so an unpaired caller
			// learns nothing.
			//
			// The phone cannot be TOLD which 404 it got — that is the whole point of the flat
			// refusal — and it does not need to be. It has a positive signal of its own: the
			// refusal carried a challenge, and it is not the one just presented. That means the
			// Mac answered, the Mac is alive, and the credential presented is no longer current.
			// So the request is re-signed and sent again, ONCE, with the challenge that refusal
			// handed back.
			//
			// ONCE, and not a loop. A second 404 under a challenge the Mac minted itself seconds
			// earlier is not a stale credential, it is the answer — and retrying past it is rule
			// 4's "a phone hammering a Mac that has forgotten it".
			//
			// Safe to repeat on a POST: `/api/messages` is idempotent on `client_id`
			// (`phone/routes.rs` `messages` → `already_answered`), and in any case nothing here
			// is reached unless the first attempt was REFUSED, which means the Mac did not act
			// on it. Nothing new leaks either: the header is on every answer this port gives, to
			// anything that knocks, before any credential is looked at.
			if (
				response.status === 404 &&
				credentialed &&
				typeof next === 'string' && next.length > 0 &&
				next !== presented
			) {
				staleCredentialRetries += 1;
				({ answer: response } = await attempt());
			}

			if (response.status === 403) {
				let revoked = false;
				try { revoked = (await response.clone().json()).revoked === true; } catch { /* not JSON */ }
				if (revoked) throw new ApiError(REVOKED, 'this phone was removed from your Mac');
				throw new ApiError(REFUSED, 'your Mac refused this', 403);
			}
			// §2.5 item 4: an unauthorized caller learns nothing, not even that it guessed a real
			// path. So a 404 here means "not for you", never "this route is missing".
			if (response.status === 404) throw new ApiError(REFUSED, 'your Mac did not accept this phone', 404);
			if (!response.ok) {
				// A REFUSAL THE MAC MARKED FINAL. `503 {"accepted":false,"reason":"…"}` is two
				// different events wearing one status (contract §9): the Mac could not write his
				// words down, which MUST be retried, and the Mac will not take this kind of message
				// in this build, which must never be. Nothing about the status or the body shape
				// separates them, so the Mac says which it is and the phone reads it — never the
				// other way round, because a phone that guesses either strands a message or hammers.
				//
				// STRICTLY `=== false`. A missing key, a body that is not JSON, the string "false"
				// and the number 0 are all faults, which is the direction that costs one wasted
				// request rather than a message he has to type again.
				const final = await finalRefusal(response);
				if (final) {
					throw new ApiError(REFUSED, final.reason || 'your Mac will not take that', response.status, true);
				}
				// THE MAC IS WAITING FOR THE PERSON TO PRESS "They match" ON IT (Sage F1). A fault,
				// so everything already built retries it — but a named one, so the pairing screen
				// can say which press is missing instead of "temporarily unavailable".
				if (response.status === 409) {
					let body = null;
					try { body = await response.clone().json(); } catch { /* not the awaiting answer */ }
					if (body && body.awaiting_mac_confirmation === true) {
						const waiting = new ApiError(FAULT, 'Your Mac is waiting for you to press They match on it.', 409);
						waiting.awaitingMac = true;
						throw waiting;
					}
				}
				throw new ApiError(FAULT, 'Your Mac is temporarily unavailable. Your unsent messages stay on this phone.', response.status);
			}
			return response;
		}

		async function json(method, pathWithQuery, body, contentType, options) {
			const response = await request(method, pathWithQuery, body, contentType, options);
			const text = await response.text();
			if (!text) return {};
			try {
				return JSON.parse(text);
			} catch {
				throw new ApiError(FAULT, 'your Mac sent something this app could not read');
			}
		}

		function query(params) {
			const search = new URLSearchParams();
			Object.keys(params).forEach((k) => {
				if (params[k] !== undefined && params[k] !== null) search.set(k, String(params[k]));
			});
			const s = search.toString();
			return s ? `?${s}` : '';
		}

		return {
			state,
			setApiBase,
			setChallenge,
			setCapabilities,
			signingInput,

			/// "Can this Mac be asked for this?" — the only question the screen asks about
			/// capabilities, so it is the only thing exposed rather than the list itself.
			offers(name) { return offers(state.capabilities, name); },

			/// See `staleCredentialRetries` above. For the harness; nothing in the app reads it.
			staleCredentialRetries() { return staleCredentialRetries; },

			// ---- (a) post a message -------------------------------------------------------------
			//
			// Idempotent on `client_id`: the phone's own de-duplication key, which is what makes the
			// queue safe to retry after a failure it could not classify. A repeat returns the
			// original message with `duplicate: true` rather than a second message.
			async sendText(item) {
				return json('POST', '/api/messages', {
					client_id: item.clientId,
					thread_id: item.threadId,
					kind: 'text',
					text: item.text,
					sent_at: item.sentAt
				});
			},

			/// AN ANSWER TO A QUESTION CARD — `routes.rs` `messages`, `kind: "answer"`, offered when the
			/// Mac names `questions`. The Mac answers with `{outcome, question}`, the question as it
			/// now stands. `expected_revision` only when he changes an answer he already gave, as
			/// the Mac's own card does (`app/ui/questions.js` `submit`).
			async answerQuestion(answer) {
				const body = {
					kind: 'answer',
					client_id: answer.clientId,
					thread_id: answer.threadId,
					question_id: answer.questionId,
					option_ids: answer.optionIds,
					text: answer.text || ''
				};
				if (Number.isSafeInteger(answer.expectedRevision)) body.expected_revision = answer.expectedRevision;
				return json('POST', '/api/messages', body);
			},

			// The WAV goes as bytes, not base64: a 20-second note is about 640 KB (plan §3.3) and
			// base64 would make it 850 KB to say the same thing. `codec` is explicit and always
			// sent, so native can send `opus` later against this same contract.
			async sendVoice(item) {
				// The queue stores audio as a plain array, because that is what survives being
				// written to storage and read back in a later launch. It becomes bytes again HERE,
				// at the last moment, so both the body and the hash that is signed are the same
				// bytes — a `fetch` handed a plain array would quietly send the JSON text of it.
				const bytes = item.fileId && opts.voiceBody ? opts.voiceBody(item.fileId) : item.bytes instanceof Uint8Array ? item.bytes : new Uint8Array(item.bytes || []);
				const path = '/api/messages' + query({
					client_id: item.clientId,
					thread_id: item.threadId,
					kind: 'voice',
					codec: item.codec || 'wav16k',
					sample_rate: item.sampleRate || 16000,
					seconds: item.seconds,
					sent_at: item.sentAt
				});
				return json('POST', path, bytes, 'audio/wav');
			},

			// ---- (b) the stream, the way back to a signable challenge, and the backfill ---------

			/// THE ONE WAY BACK FROM A CHALLENGE THAT AGED OUT, and without it a re-signed stream
			/// URL is a signature over something the Mac has already forgotten.
			///
			/// The arithmetic, re-derived rather than quoted: a challenge is live for
			/// `CHALLENGE_LIFETIME_MS` (`app/src-tauri/src/phone/device.rs:62`) = 600,000 ms =
			/// 600 s = **10 minutes** from the moment the Mac issued it, and `device.rs:418-424`
			/// refuses anything older. The phone is the side that goes away — asleep, on cellular,
			/// out of range — so an outage longer than ten minutes is the ordinary case, not the
			/// edge, and at the end of one the challenge this phone is holding is dead.
			///
			/// Every ordinary response would hand it a live one: `request` above reads
			/// `X-RichOS-Challenge` off EVERY answer, including the refusals. **An `EventSource`
			/// cannot read a response header.** So the one channel that the reconnect actually
			/// depends on is the one channel that can never learn it has gone stale, and a phone
			/// with nothing queued — nothing to POST, nothing to refresh it — never gets back in.
			///
			/// This is a plain `fetch`, whose headers this app CAN read, and it is deliberately
			/// UNAUTHENTICATED. Signing it would mean signing the expired challenge to ask for a
			/// live one, which is the circle this exists to break; and the challenge is public by
			/// construction anyway — the Mac hands one to anything on the network that knocks
			/// (`listen.rs:336`). **The status is never read.** A 404 that carries the header is
			/// the documented contract, not a failure.
			async refreshChallenge() {
				if (!fetchImpl) throw new ApiError(FAULT, 'this browser has no fetch');
				let response;
				try {
					response = await fetchImpl(joinBase(state.apiBase, CHALLENGE_PROBE), {
						method: 'GET', mode: 'cors', cache: 'no-store'
					});
				} catch (err) {
					// Same branch, same meaning as every other request: the Mac was not reached.
					throw new ApiError(UNREACHABLE, String((err && err.message) || err));
				}
				const next = response.headers && response.headers.get
					? response.headers.get('X-RichOS-Challenge')
					: null;
				// A Mac that answered without one is not a Mac this phone can sign for. Say so
				// rather than carrying on with the dead challenge and blaming the signature.
				if (!next) throw new ApiError(FAULT, 'Your Mac is not ready to reconnect yet. Your unsent messages stay on this phone.');
				setChallenge(next);
				return next;
			},

			/// "LOAD OLDER MESSAGES", AND ITS CREDENTIAL GOES IN THE QUERY.
			///
			/// This is the same route as the stream — `GET /api/events` — and `routes.rs`
			/// `events()` reads the credential ONCE, at `:405`, BEFORE it looks at `before=` and
			/// decides whether the answer is a stream or a page of JSON. One route, one contract.
			///
			/// It used to go through `request()` with the credential in the `Authorization`
			/// header, which `events()` never reads, so every scroll into the past on a real Mac
			/// was a flat 404. It was invisible because the harness Mac accepted either place; the
			/// harness is now strict per route (`test/stub-mac.js` `authenticate`), which is what
			/// stops this returning. The stream was always right, and this is the side that moved.
			async backfill(threadId, beforeCursor, limit) {
				return json('GET', '/api/events' + query({
					thread_id: threadId, before: beforeCursor, limit: limit || 40
				}), undefined, undefined, { credential: 'query' });
			},

			/// The live stream. `EventSource` cannot carry a header, so the stream — and only the
			/// stream — puts its credential in the query string; CONTRACT-STUB.md §2(b) names that as
			/// a real weakening and says what makes it tolerable. Returns a handle with `close()`.
			async openEvents(threadId, sinceCursor, handlers) {
				if (!EventSourceImpl) throw new ApiError(FAULT, 'this browser has no event stream');
				const path = '/api/events' + query({ thread_id: threadId, since: sinceCursor });
				const auth = await authorization('GET', path, '');
				// The signature covers the path WITHOUT this parameter, because the parameter is the
				// signature. The separator is derived rather than assumed: a path that ever loses
				// its query string would otherwise produce `…/api/events&auth=`, which is a URL the
				// Mac would refuse for a reason nobody would find quickly.
				const base = joinBase(state.apiBase, path);
				const url = auth ? `${base}${base.includes('?') ? '&' : '?'}auth=${encodeURIComponent(auth)}` : base;
				const source = new EventSourceImpl(url, { withCredentials: false });

				const wire = (name) => {
					source.addEventListener(name, (event) => {
						let data = {};
						try { data = event.data ? JSON.parse(event.data) : {}; } catch { data = {}; }
						if (name === 'hello') {
							setChallenge(data.challenge);
							setApiBase(data.api_base);
							// Contract §5.3. `capabilities` is what this Mac can be asked for and
							// `build` is which RichOS is answering — both read here, at the one
							// place that knows the wire, and handed to the screen as state.
							setCapabilities(data.capabilities, data.build);
						}
						// The frame's `id:` is the hub's own number, the cursor a reconnect presents (`makeLink`).
						if (handlers[name]) handlers[name](data, event.lastEventId);
					});
				};
				['hello', 'message', 'delta', 'state', 'heartbeat', 'threads'].forEach(wire);
				source.onopen = () => { if (handlers.open) handlers.open(); };
				// EventSource hides HTTP refusal bodies. Stop its built-in retry immediately,
				// then ask the same authenticated route for an empty JSON page to distinguish
				// a forgotten phone from a temporary network failure.
				let closed = false;
				let probing = false;
				const closeSource = source.close.bind(source);
				source.close = () => { closed = true; closeSource(); };
				source.onerror = async () => {
					if (closed || probing) return;
					probing = true;
					closeSource();
					let refusal;
					try {
						await json('GET', '/api/events' + query({ thread_id: threadId, before: 0, limit: 1 }),
							undefined, undefined, { credential: 'query' });
					} catch (err) { if (err.reason === REVOKED) refusal = err; }
					if (!closed && handlers.error) handlers.error(refusal);
				};
				return source;
			},

			// ---- (c) one audio blob, by an id the Mac minted ------------------------------------
			//
			// Fetched rather than handed to an <audio src>, for two reasons that both matter: the
			// element cannot send the authorization header, and a failure here has to be a sentence
			// he can read rather than a silent control that does nothing.
			async fetchAudio(messageId, threadId) {
				const response = await request('GET', `/api/audio/${encodeURIComponent(messageId)}` + query({thread_id:threadId}));
				return response.nativeAudio || (response.blob ? response.blob() : response.arrayBuffer());
			},

			/// A FILE RICH ATTACHED, by the id its row carries (`phone/routes.rs` `rich_file`): the
			/// message route, credential in the header, the bytes as a Blob typed by the Mac. A file
			/// that is gone is a final refusal carrying the Mac's own sentence.
			async fetchFile(fileId, threadId) {
				const response = await request('GET', '/api/messages' + query({ kind: 'file', file_id: fileId, thread_id: threadId }));
				return response.blob ? response.blob() : response.arrayBuffer();
			},

			// ---- (d) pairing, and the device record ---------------------------------------------
			///
			/// `options.pairingVersion === 2` is a v2 phone (Sage's pairing review §3): it says so in
			/// the request, so the Mac shows the same six words it will, and it refuses a Mac that
			/// does not offer `pair-v2`. Without it this is the v1 call, byte for byte — which is
			/// what the preserved iPhone app (`mobile/core/client.js`) still makes and must keep
			/// making for the one release §3.5 keeps v1 for.
			async pair(code, publicKeyJwk, deviceName, options) {
				const v2 = !!(options && options.pairingVersion === 2);
				const request = { code, public_key_jwk: publicKeyJwk, device_name: deviceName };
				// A relay that strips this only makes the two lines differ, because a v2 phone shows
				// v2 words whatever the Mac does.
				if (v2) request.pairing_version = 2;
				// No signature: this is the request that establishes the credential.
				const response = await fetchImpl(joinBase(state.apiBase, '/api/pair'), {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					mode: 'cors',
					cache: 'no-store',
					body: JSON.stringify(request)
				}).catch((err) => { throw new ApiError(UNREACHABLE, String((err && err.message) || err)); });

				if (response.status === 404 || response.status === 403) {
					throw new ApiError(REFUSED, 'your Mac did not accept that pairing code', response.status);
				}
				if (!response.ok) throw new ApiError(FAULT, 'Your Mac is temporarily unavailable. Your unsent messages stay on this phone.', response.status);
				const body = await response.json();
				state.deviceId = body.device_id;
				setChallenge(body.challenge);
				setApiBase(body.api_base);
				onState(state);
				// A MAC WITHOUT `pair-v2` IS REFUSED, NEVER FALLEN BACK TO (review §3.5). The state
				// above is kept deliberately: the caller signs one last `fingerprint_confirmed: false`
				// with it, so the Mac that just registered this key forgets it again.
				if (v2 && !offers(body.capabilities, PAIR_V2)) {
					const tooOld = new ApiError(REFUSED, 'Update RichOS on your Mac, then pair this phone again.', response.status);
					tooOld.macNeedsUpdate = true;
					throw tooOld;
				}
				return body;
			},

			/// **HAS THE PERSON PRESSED "They match" ON THE MAC YET?** One signed read of the
			/// smallest thing a paired phone may read — one backfill row — which the Mac answers
			/// with the awaiting 409 until the press, and normally after it. `true` once it is
			/// answered, `false` while the Mac is still waiting; every other failure is thrown
			/// as it is, so a refusal (the person pressed "They do not match", or the window
			/// closed) reaches the screen as the final answer it is. Called on
			/// `macWaitDelayMs`'s schedule and no other.
			async macConfirmed(threadId) {
				try {
					await json('GET', '/api/events' + query({ thread_id: threadId, before: 0, limit: 1 }),
						undefined, undefined, { credential: 'query' });
					return true;
				} catch (err) {
					if (err && err.awaitingMac) return false;
					throw err;
				}
			},

			async registerNativePush(registration) {
				return json('POST', '/api/pair', {native_push:registration});
			},

            async replySeen(thread, id) {
                return json('POST', '/api/pair', { seen_reply: { thread, id } });
            },
			async registerPush(subscription) {
				return json('POST', '/api/pair', {
					device_id: state.deviceId,
					push_transport: 'web-push', reply_receipts: true,
					push: subscription
				});
			},

			/// **THE ANSWER TO THE SIX WORDS, SENT BACK TO THE MAC** — Ray's nightly `.7`, defect 2.
			///
			/// There was no such message. The person pressed `They match — pair this phone` or
			/// `They do not match`, this app acted on it locally, and the Mac was told nothing
			/// either way — so its sheet read `It is paired` while this screen was still asking
			/// the question, and a phone he had just declared suspect stayed paired over there.
			///
			/// It is the same route as the device record, told apart by the field, which is what
			/// keeps §2.5's ceiling of four routes. `false` is not a flag the Mac files away: it
			/// FORGETS this phone, which is the same thing `pair-reject` does to the key on this
			/// side, so the two ends finish in one state.
			async confirmFingerprint(matched) {
				return json('POST', '/api/pair', answerBody(matched));
			},

			/// **ASK THE MAC WITH THE ANSWER, AND LET IT HOLD THE ASK** — Sage's pair-v2 hypotheses
			/// review §1, "The fix" point 1 (and the fix for his §2c).
			///
			/// The wait for the press on the Mac sends THIS phone's own signed `They match` again —
			/// byte for byte the body `confirmFingerprint(true)` sends — instead of reading a
			/// backfill row. Every Mac with the press answers it on every outcome: still waiting
			/// (`awaiting_mac_confirmation: true`), pressed (`{"ok":true}`), a 403 revoked after
			/// `They do not match` on the Mac, a 404 after the window closed. And because the ask is
			/// the answer, a phone whose first `They match` was lost on the way is recorded by the
			/// same request that learns of the press.
			///
			/// `options.wait` (seconds) adds `Prefer: wait=N` — send it only to a Mac that names
			/// `pair-wait`. `options.signal` cancels the ask (the app does, when it leaves the
			/// screen). Resolves `true` once the Mac has been pressed, `false` while it still waits;
			/// every other failure is thrown as it is, so a refusal reaches the screen as the final
			/// answer it is.
			async macAnswer(options) {
				const wait = Math.floor(Number(options && options.wait) || 0);
				const extra = { signal: options && options.signal };
				if (wait > 0) extra.headers = { Prefer: `wait=${Math.min(wait, PAIR_WAIT_SECONDS)}` };
				try {
					const answer = await json('POST', '/api/pair', answerBody(true), undefined, extra);
					return answer.ok === true && answer.awaiting_mac_confirmation !== true;
				} catch (err) {
					if (err && err.awaitingMac) return false;
					throw err;
				}
			}
		};

		/// The phone's answer to the six words, as the Mac reads it (`routes.rs`
		/// `only_answers_the_words`: this and nothing else).
		function answerBody(matched) {
			return {
				device_id: state.deviceId,
				fingerprint_confirmed: matched === true,
				...(opts.nativeClient ? {push_transport:'apns'} : {})
			};
		}
	}

	return {
		createApi, ApiError, signingInput, offers, UNREACHABLE, REVOKED, REFUSED, FAULT,
		PAIR_V2, MAC_WAIT_DELAYS_MS, MAC_WAIT_CAP_MS, MAC_WAIT_WINDOW_MS, macWaitDelayMs, macWaitBoundMs,
		PAIR_WAIT, PAIR_WAIT_SECONDS, MAC_WAIT_MIN_SPACING_MS, macWaitNextDelayMs, macWaitNextAskAt
	};
});
