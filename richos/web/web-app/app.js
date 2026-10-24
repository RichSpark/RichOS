'use strict';

// Rich, on a phone. One screen, four verbs: send text, send a voice note, receive Rich, hear Rich.
//
// This file is the wiring. The parts that can be wrong in a way a person notices live in `lib/`,
// where they are driven by tests without a browser: the recorder's arithmetic (`pcm.js`), the send
// queue's state machine (`queue.js`), the thread's ordering (`thread.js`), the routes (`api.js`),
// and what survives being closed (`storage.js`). What is left here is the part that only means
// anything with a screen attached, and the browser harness drives it in a real browser.
//
// THREE DECISIONS IN HERE ARE THE PRODUCT, not implementation detail:
//
//   1. THE MICROPHONE PROMPT IS PART OF THE GESTURE, NOT A MODAL BEFORE IT. iOS asks for the
//      microphone on every launch of an installed web app — measured on his own iPhone X on
//      2026-09-18, and the browser reported the permission as "granted" while still asking. So the
//      first hold of the session opens the microphone and the dialog appears under his finger. When
//      he lifts his finger to tap Allow, the release does NOT become a dead recording: the app says
//      the microphone is on and asks him to hold again. A dead recording there is the app telling
//      him the microphone is broken when it is working, which is the worst thing it could say.
//
//   2. THE THREE SEND STATES ARE HONEST, and the middle one is the real one. An optimistic bubble;
//      then "waiting to send — your Mac isn't reachable from here" with the reason; then delivered
//      when the Mac has it. The middle state is not a reassurance — it is the truth that his message
//      is still on this phone.
//
//   3. NOTHING HE READS IS AN IDENTIFIER. No message id, no device id, no cursor, no hash. His probe
//      walk ended with a horizontal scroll bar caused by a SHA-256 printed in one line; the rule
//      that came out of it is not "wrap the hash", it is "do not put one in front of him".

(function () {

const $ = (id) => document.getElementById(id);
const TIME = new Intl.DateTimeFormat('en-US', { hour: 'numeric', minute: '2-digit' });

const pcm = globalThis.RichOSPcm;
const fingerprint = globalThis.RichOSFingerprint;
const Storage = globalThis.RichOSStorage;

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

let db = null;
let settings = null;
let messageCache = null;
let keyStore = null;
let queue = null;
let thread = null;
let api = null;
let signer = null;

let apiState = { apiBase: location.origin, challenge: null, deviceId: null };
let threads = [];
let currentThreadId = null;
let notificationTarget = null;
let conversationReady=false;
// The ONE owner of the event stream (`lib/link.js`). It used to be a bare `stream` handle plus a
// boolean, and both the closing and the reconnecting were spread across four places that could not
// see each other. There is no `stream` variable any more on purpose: nothing outside the link is
// allowed to hold a source, which is what makes "never more than one alive" a property rather than
// a hope.
let link = null;
/// The sentence currently under the header, by name. Read only to keep a retry from flickering it.
let linkStateNow = null;
let lastConnectionProbe = -Infinity;
let connectionDetail = null;
let connectionProbeGeneration = 0;
let vapidPublicKey = null;
// See `refreshPushOffer` — his answer, not his permission state.
let notificationsDeclined = false;
let loadingOlder = false;
let renderQueued = false;
let pendingPairCode = null;
/// **IS HE READING THE NEWEST MESSAGE?** Remembered, never re-measured at the moment it matters —
/// see `atBottom`, `stickToBottom` and the observer under `Rendering`. It starts true because an
/// empty thread is at its own bottom.
const following=RichOSFollow.attach($('thread'),$('messages'),$('latest'));
/// Where the thread was the last time it reported a position. The scroll handler needs the
/// DIRECTION of a move, not just the new number — see the comment there.


// The recorder, and the one flag the whole permission design hangs on.
let micStream = null;
let audioCtx = null;
let captureNode = null;
let chunks = [];
let recording = false;
let opening = false;
let captureGeneration=0;
let voiceDraftStore=null;
let voicePreview=null, recordingWakeLock=null;
function releaseRecordingWakeLock(){const lock=recordingWakeLock;recordingWakeLock=null;if(lock)void lock.release().catch(()=>{});}
function stopVoicePreview(){if(!voicePreview)return;const preview=voicePreview;voicePreview=null;player.pause();player.removeEventListener("ended",preview.ended);URL.revokeObjectURL(preview.url);preview.button.textContent="Play recording";}
let askedForMicrophoneThisLaunch = false;
let autoStopTimer = null;
const MAX_RECORD_SECONDS = 30 * 60;

// One audio element, unlocked on his first tap, reused for every "hear it". Safari will not start
// playback that is not inside a user gesture, and an `await` loses the gesture — so the element is
// started (on silence) inside the first tap he makes anywhere, and from then on it may be played.
let player = null;
let playerUnlocked = false;
let silentWavUrl = null;
let playingButton = null;

// ---------------------------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------------------------

async function boot() {
	try {
		db = await Storage.open();
	} catch (err) {
		// Private browsing, or a storage quota refusal. Without storage there is no queue and no
		// key, so this is said plainly rather than limped past.
		showTakeover('revoked');
		$('revoked-detail').textContent = 'This phone cannot store anything for Rich, so pairing and the send queue cannot work here. That is usually private browsing.';
		return;
	}

	settings = Storage.settings(db);
	voiceDraftStore=Storage.voiceDrafts(db);
	messageCache = Storage.messages(db);
	keyStore = Storage.keys(db);

	const saved = await settings.all();
	apiState = {
		// The origin is the app's identity and never changes; the base is data, and it starts as
		// the origin (plan §10.7).
		apiBase: saved.apiBase || location.origin,
		challenge: saved.challenge || null,
		deviceId: saved.deviceId || null,
        capabilities: Array.isArray(saved.capabilities) ? saved.capabilities : []
	};
	vapidPublicKey = saved.vapidPublicKey || null;
	// He was asked about notifications and did not say yes. Remembered across a relaunch for one
	// reason: so the banner keeps telling him what that MEANT rather than reverting to the generic
	// offer, which is a reload asking him a question he has already answered.
	notificationsDeclined = Boolean(saved.notificationsDeclined);
	currentThreadId = saved.threadId || null;
	threads = saved.threads || [];

	const pair = /[#&]pair=([^&]+)/.exec(location.hash || '');
	pendingPairCode = pair ? decodeURIComponent(pair[1]) : null;

	const keys = await keyStore.load();

	// A PAIRING THAT WAS STILL WAITING FOR THE PRESS ON THE MAC when the app was closed (Sage F1).
	// Inside its bound it goes straight back to waiting. Past it, this phone still asks the Mac ONE
	// last time before it gives up (Sage's pair-v2 hypotheses review §2): the person may have
	// pressed on the Mac while this app was closed, and a Mac that said yes to a phone that then
	// threw its key away is the half-paired state that review is about.
	const awaitingUntil = Number(saved.awaitingMacUntil) || 0;
	if (keys && apiState.deviceId && !pendingPairCode && awaitingUntil) {
		makeApi(keys);
		macHolds = saved.macPairWait === true;
		if (Date.now() < awaitingUntil) {
			showWaitingForMac(null);
		} else {
			showTakeover('pairing');
			$('fingerprint-box').hidden = true;
			$('pairing-row').hidden = true;
			$('pairing-lede').textContent = 'Pairing with your Mac\u2026';
			$('pairing-detail').textContent = '';
		}
		waitForMac(awaitingUntil);
		return;
	}

	if (!keys || !apiState.deviceId || pendingPairCode) {
		await startPairing(keys);
		return;
	}

    await restoreComposer();
	await startConversation(keys);
    await openNotificationURL(location.pathname+location.hash);
}

async function persistState() {
	await settings.set('apiBase', apiState.apiBase);
	await settings.set('challenge', apiState.challenge);
	await settings.set('deviceId', apiState.deviceId);
    await settings.set('capabilities',apiState.capabilities || []);
	// An address the Mac advertised that this app would not take, kept where it can be found.
	// Nothing renders it — said here so nobody goes looking for a message on the screen — but a
	// phone that declined to move to a new address now has the reason attached to it rather than
	// having silently ignored its Mac (plan §2 C).
	await settings.set('apiBaseRefusal', apiState.apiBaseRefusal || null);
}

function makeApi(keys) {
	signer = Storage.createSigner(crypto.subtle, keys, apiState);
	api = globalThis.RichOSApi.createApi({
		state: apiState,
		signer,
		onState: () => { persistState().catch(() => { /* a failed write must not kill a send */ }); }
	});
	return api;
}

// ---------------------------------------------------------------------------------------------
// Pairing — the phone half (plan §4.1)
// ---------------------------------------------------------------------------------------------

function showTakeover(which) {
	document.body.classList.toggle('pairing', which === 'pairing');
	document.body.classList.toggle('revoked', which === 'revoked');
	$('pairing').hidden = which !== 'pairing';
	$('revoked').hidden = which !== 'revoked';
}

function hideTakeovers() {
	document.body.classList.remove('pairing', 'revoked');
	$('pairing').hidden = true;
	$('revoked').hidden = true;
}

async function startPairing(existingKeys) {
	showTakeover('pairing');

	if (!pendingPairCode) {
		// There is no control on this phone that can pair it — the code comes from the Mac. So the
		// screen says exactly what to do THERE, rather than offering a button that cannot work.
		$('pairing-lede').textContent = 'This phone is not paired with your Mac yet.';
		$('pairing-detail').textContent = 'On your Mac, open RichOS and choose "Use Rich from your phone". Point this phone\'s camera at the second code it shows, and this screen will carry on from there.';
		return;
	}

	$('pairing-lede').textContent = 'Pairing with your Mac…';
	$('pairing-detail').textContent = '';

	let keys = existingKeys;
	if (!keys) {
		// Generated HERE, on the phone, and the private half is non-extractable: it cannot be read
		// out by this script or any other, now or later (plan §2.7, §20's rule).
		keys = await Storage.generateDeviceKey(crypto.subtle);
		await keyStore.save(keys);
	}
	makeApi(keys);

	let publicJwk;
	try {
		publicJwk = await crypto.subtle.exportKey('jwk', keys.publicKey);
	} catch (err) {
		$('pairing-lede').textContent = 'This phone could not make a key for itself.';
		$('pairing-detail').textContent = String(err && err.message ? err.message : err);
		return;
	}

	// THE ORIGIN THIS PHONE DIALED, taken before the answer can move `apiBase` (Sage F2): the v2
	// words are over the address this phone actually opened, so a relay shows different words.
	const dialed = apiState.apiBase;
	let answer;
	try {
		answer = await api.pair(pendingPairCode, publicJwk, deviceName(), { pairingVersion: 2 });
	} catch (err) {
		// A MAC WITHOUT `pair-v2` (review §3.5): never fall back to the old words, which a relay
		// could pass. The Mac has already registered this key, so it is told to forget it with the
		// one signed request this phone can still make, and the person is told what to do.
		if (err && err.macNeedsUpdate) {
			try { await api.confirmFingerprint(false); } catch { /* said locally either way, below */ }
			await forgetThisPairing();
			// Urban's review, state 5, and Sage's pair-v2 review §3: the older Mac this answer came
			// from shows its own "Your phone said the six words did not match" screen, because
			// the one request above is the only way to make it forget this key. The middle
			// sentence accounts for that and must NOT reassure: a relay that strips `pair-v2` from
			// a current Mac produces the same two screens.
			$('pairing-lede').textContent = 'Your Mac needs an update.';
			$('pairing-detail').textContent = 'This phone cannot pair with the version of RichOS on it. Your Mac may say the six words did not match: it stopped because this phone did. Update RichOS on your Mac, then show a fresh code there and open it on this phone again.';
			return;
		}
		$('pairing-lede').textContent = err && err.reason === 'unreachable'
			? 'Your Mac could not be reached from here.'
			: 'Your Mac did not accept that pairing code.';
		$('pairing-detail').textContent = err && err.reason === 'unreachable'
			? 'Make sure you are on the same Wi-Fi as your Mac, that RichOS is open on it, and try the code again.'
			// Urban's review, state 9: the Mac's window is five minutes (`phone/mod.rs`
			// PAIRING_WINDOW_MS), not the sixty seconds this used to say.
			: 'A pairing code works once, for one phone, and runs out after a few minutes. Show a fresh one on your Mac and open it again.';
		return;
	}

	vapidPublicKey = answer.vapid_public_key || null;
	threads = answer.threads || [];
	currentThreadId = (threads[0] && threads[0].id) || answer.thread_id || null;
	macWaitBoundMs = globalThis.RichOSApi.macWaitBoundMs(answer);
	// Ledger row S3's capability pattern: `Prefer: wait` goes only to a Mac that names `pair-wait`.
	macHolds = globalThis.RichOSApi.offers(answer.capabilities, globalThis.RichOSApi.PAIR_WAIT);

	// SIX WORDS, computed here — never taken from the Mac as words, and never shown to him as a
	// hash. v2 (Sage F2): over the origin this phone dialed, the Mac's pairing value and this
	// phone's own key, so a relay or a device that paired first shows him a different line.
	let words;
	try {
		words = await fingerprint.phraseV2(dialed, answer.ca_fingerprint_sha256, fingerprint.pointFromJwk(publicJwk), crypto.subtle);
	} catch (err) {
		$('pairing-lede').textContent = 'Your Mac sent something this app could not read.';
		$('pairing-detail').textContent = 'Tell Rich. Pairing has not happened and nothing was stored.';
		return;
	}

	$('pairing-lede').textContent = 'Almost done. One thing to check, and it is the thing that matters.';
	$('fingerprint-note').textContent = whatTheSixWordsAre();
	$('fingerprint-words').textContent = words;
	$('fingerprint-box').hidden = false;
	$('pair-confirm').hidden = false;
	$('pair-confirm').disabled = false;
	$('pairing-row').hidden = false;
}

function deviceName() {
	// Named for him, not for a log. "iPhone" is what he calls it.
	if (/iPad/.test(navigator.userAgent)) return 'iPad';
	if (/iPhone/.test(navigator.userAgent)) return 'iPhone';
	if (/Android/.test(navigator.userAgent)) return 'Android phone';
	return 'Phone';
}

$('pair-confirm').addEventListener('click', async () => {
	$('pair-confirm').disabled = true;
	// THE PRESS IS THE FIRST ASK (Sage's pair-v2 hypotheses review §1, "The fix" point 1). The
	// phone's signed `They match` tells the Mac he answered — until that existed the Mac's sheet
	// read `It is paired` while this screen was still asking (Ray's nightly `.7`, defect 2) — and
	// every later ask of the wait is the same request again, so the answer and the question about
	// the Mac's own press travel together.
	//
	// It no longer lets this phone in by itself (Sage F1): the Mac answers "waiting" until the
	// person presses They match ON THE MAC. The deadline counts from this press, as before, and it
	// is written down first so a relaunch in the middle of a held ask resumes the wait.
	const until = Date.now() + macWaitBoundMs;
	await settings.set('threads', threads);
	await settings.set('threadId', currentThreadId);
	await settings.set('vapidPublicKey', vapidPublicKey);
	await settings.set('macPairWait', macHolds);
	await settings.set('awaitingMacUntil', until);
	await persistState();
	// The pairing code is single use; leaving it in the address would re-run pairing on the next
	// launch and fail with a stale code.
	history.replaceState(null, '', location.pathname);
	pendingPairCode = null;
	const words = $('fingerprint-words').textContent;
	waitForMac(until, () => showWaitingForMac(words));
});

// ---------------------------------------------------------------------------------------------
// Waiting for the press on the Mac — Sage's pairing review F1, §3.1 step 5
// ---------------------------------------------------------------------------------------------
//
// The phone has said the words match; the Mac has not. Until the person presses They match on
// the Mac, every request this phone makes is answered "waiting". So this screen says which press
// is missing, keeps the words up so he can still compare them, keeps `They do not match` as the
// way out, and asks the Mac again — with its own signed `They match`, which is the answer and the
// question at once (Sage's pair-v2 hypotheses review §1, point 1).
//
// **A MAC THAT NAMES `pair-wait` HOLDS THE ASK** until the press, for at most 14 s, so the phone
// hears the press within one round trip instead of at its next scheduled ask. The no-spin rule is
// `RichOSApi.macWaitNextDelayMs`: an answer that took 7 s or more is followed by the next ask at
// once; a quicker one by the scheduled delay, never less than 7 s after the previous ask started.
// **A Mac that does not name it** is asked on `macWaitDelayMs`'s schedule — 2, 3, 5, 8, 13 s, then
// every 15 s — exactly as before.
//
// Either way: only while the app is on screen (leaving the screen cancels the ask in flight with
// an `AbortController`, and nothing is scheduled while hidden), and never past the bound the Mac
// gave, at most five minutes — **with one last ask AT that bound** (Sage's review §2), so a press
// that landed after the previous ask is heard rather than dropped. CEO ruling §81: never a tight
// poll and nothing off screen.

let macWaitBoundMs = globalThis.RichOSApi.MAC_WAIT_WINDOW_MS;
let macHolds = false;
let macWait = null;

/// How long the first ask may take before the six words give way to the waiting screen. An answer
/// inside it — the Mac was already pressed, or a Mac that does not hold — goes straight to its own
/// screen with no flash of this one in between.
const SHOW_WAITING_AFTER_MS = 500;

function showWaitingForMac(words) {
	showTakeover('pairing');
	$('fingerprint-box').hidden = !words;
	$('pairing-row').hidden = false;
	$('pair-confirm').hidden = true;
	$('pairing-lede').textContent = 'Now press They match on your Mac.';
	// Urban's review, state 3: "carries on by itself" is true however long the Mac takes to be
	// heard, "here or on your Mac" names both places, and the control's name is set in bold.
	const detail = $('pairing-detail');
	detail.textContent = 'This phone carries on by itself once you do. If the words on your Mac are different, press ';
	const control = document.createElement('strong');
	control.textContent = 'They do not match';
	detail.append(control, ', here or on your Mac.');
}

function stopWaitingForMac() {
	if (macWait) macWait.stop();
	macWait = null;
}

/// `onWaiting` is called once, the first time the phone knows it is waiting: the first answer
/// says so, or the first ask is still out after `SHOW_WAITING_AFTER_MS` (the Mac is holding it).
function waitForMac(until, onWaiting) {
	stopWaitingForMac();
	const A = globalThis.RichOSApi;
	let attempt = 0;
	let timer = null;
	let asking = null;      // the AbortController of the ask in flight
	let lastAskAt = 0;
	let stopped = false;
	let told = !onWaiting;
	const tellWaiting = () => { if (!told) { told = true; onWaiting(); } };
	const ask = async () => {
		timer = null;
		// Hidden: nothing is asked and nothing is scheduled. Coming back on screen asks again.
		if (stopped || asking || document.hidden) return;
		const final = Date.now() >= until;
		// Held only by a Mac that offered it, and never past this phone's own deadline.
		const wait = macHolds && !final ? Math.min(A.PAIR_WAIT_SECONDS, Math.floor((until - Date.now()) / 1000)) : 0;
		const controller = new AbortController();
		asking = controller;
		lastAskAt = Date.now();
		const shortly = told ? null : setTimeout(tellWaiting, SHOW_WAITING_AFTER_MS);
		let answered = false;
		let refused = false;
		try {
			answered = await api.macAnswer({ wait, signal: controller.signal });
		} catch (err) {
			if (err && err.aborted) {
				// Canceled by leaving the screen or by a stop: whoever canceled it decides what next.
				clearTimeout(shortly);
				if (asking === controller) asking = null;
				return;
			}
			// A refusal is the Mac's final answer: the person pressed They do not match there, or
			// the window closed. Anything else (unreachable, a fault) waits on the same rules.
			refused = !!err && (err.reason === A.REVOKED || err.reason === A.REFUSED);
		}
		clearTimeout(shortly);
		asking = null;
		if (stopped) return;
		if (answered) { stopWaitingForMac(); await finishPairing(); return; }
		if (refused) { await gaveUpWaiting('refused'); return; }
		// THE LAST ASK WAS AT THE DEADLINE, and the Mac still had not said yes (or could not be
		// reached): this phone stops, and says it did not hear back — which is true either way.
		if (final) { await gaveUpWaiting('expired'); return; }
		tellWaiting();
		const now = Date.now();
		timer = setTimeout(ask, Math.max(0, A.macWaitNextAskAt(attempt++, lastAskAt, now, until, macHolds) - now));
	};
	const onVisibility = () => {
		if (stopped) return;
		if (document.hidden) {
			// Nothing runs off screen: the ask in flight is canceled, and nothing stays scheduled.
			if (asking) { asking.abort(); asking = null; }
			if (timer) { clearTimeout(timer); timer = null; }
			return;
		}
		if (timer || asking) return;
		// Back on screen: ask again — at once, or as soon as the 7 s spacing allows while the Mac
		// holds (past the deadline too: that ask is the last one, and it keeps the spacing).
		const spacing = macHolds ? Math.max(0, A.MAC_WAIT_MIN_SPACING_MS - (Date.now() - lastAskAt)) : 0;
		timer = setTimeout(ask, spacing);
	};
	document.addEventListener('visibilitychange', onVisibility);
	macWait = {
		stop() {
			stopped = true;
			if (timer) clearTimeout(timer);
			timer = null;
			if (asking) asking.abort();
			asking = null;
			document.removeEventListener('visibilitychange', onVisibility);
		}
	};
	// The first ask goes at once. From the press it IS the phone's `They match`; after a relaunch
	// it is the first word from a phone that has been away.
	void ask();
}

async function gaveUpWaiting(why) {
	stopWaitingForMac();
	await forgetThisPairing();
	$('fingerprint-box').hidden = true;
	$('pairing-row').hidden = true;
	// Urban's review, states 6 and 7. `expired` is reached when nobody pressed on the Mac AND when
	// the phone could not reach the Mac for the whole wait, so it says only what the phone knows.
	$('pairing-lede').textContent = why === 'refused'
		? 'Your Mac did not accept this phone.'
		: 'Pairing timed out.';
	$('pairing-detail').textContent = why === 'refused'
		? 'Nothing was paired. Either someone said the words did not match on your Mac, or pairing was stopped there. Show a fresh code on your Mac and open it on this phone again.'
		: 'This phone did not hear back from your Mac in time, so it stopped. Show a fresh code on your Mac and open it on this phone again.';
}

async function finishPairing() {
	stopWaitingForMac();
	await settings.set('awaitingMacUntil', null);
	$('pair-confirm').hidden = false;
	const keys = await keyStore.load();
	hideTakeovers();
	await startConversation(keys);
}

/// This phone's half of a pairing that did not happen: the key, the device id and the wait.
async function forgetThisPairing() {
	await keyStore.clear();
	await settings.set('deviceId', null);
	await settings.set('awaitingMacUntil', null);
	await settings.set('macPairWait', null);
	apiState.deviceId = null;
}

$('pair-reject').addEventListener('click', async () => {
	stopWaitingForMac();
	// He said the words do not match. That is the one outcome where continuing would be worse than
	// stopping: the credential is thrown away rather than kept "just in case".
	//
	// AND THE MAC IS TOLD, WHICH IT WAS NOT. This side threw its key away and the Mac went on
	// holding a device record for a phone the person had just said may not be his Mac's phone at
	// all. The Mac forgets it on this message. Sent FIRST, while the credential that signs it
	// still exists — `keyStore.clear()` below is what makes it unsendable.
	try { await api.confirmFingerprint(false); } catch { /* said locally either way, below */ }
	await forgetThisPairing();
	$('pair-confirm').hidden = false;
	$('fingerprint-box').hidden = true;
	$('pairing-row').hidden = true;
	// Urban's review, state 4 (the BLOCKER): the one security decision in the flow ends by saying
	// that nothing was paired, and with an action the person can finish. "On your Mac", because an
	// unpaired phone cannot reach Rich.
	$('pairing-lede').textContent = 'Stopped, and nothing was paired.';
	$('pairing-detail').textContent = 'If the words on this phone and your Mac were different, this phone was not talking to your Mac. Tell Rich on your Mac before you pair again.';
});

// ---------------------------------------------------------------------------------------------
// The conversation
// ---------------------------------------------------------------------------------------------

async function startConversation(keys) {
	await renderVoiceDrafts();
	hideTakeovers();
	if (!api) makeApi(keys);
	applyCapabilities();

	thread = globalThis.RichOSThread.createThread();
	queue = globalThis.RichOSQueue.createQueue({
		storage: Storage.queueStorage(db),
		onChange: () => { scheduleRender(); updateQueueBanner(); }
	});

	await queue.load();
	renderThreadPicker();

	// What he last saw, straight away, before any network. Opening the app on a train shows the
	// conversation rather than a spinner — the cache is a view, and the Mac replaces it the moment
	// it is reachable.
	const cached = await messageCache.recent(currentThreadId, 60);
	if (cached.length) thread.merge(cached);
	render(true);
    conversationReady=true;

	setLinkState('opening');
	connectStream();
	await refreshPushOffer();
	flushQueue();
    if(notificationTarget)void resolveNotificationTarget().catch(handleApiError);
}

let connectionTimer=null;
function setLinkState(which, detail) {
    const el=$('link-state');linkStateNow=which;
    const generation=++connectionProbeGeneration;
    if(which==='connected'){clearTimeout(connectionTimer);connectionTimer=null;connectionDetail=null;el.textContent='';el.classList.remove('away');return;}
    el.classList.toggle('away',which==='away');
    if(detail){clearTimeout(connectionTimer);connectionTimer=null;el.textContent=detail;return;}
    const show=()=>{el.textContent=RichOSConnection.sentence(navigator.onLine===false?'phone-offline':connectionDetail || 'reconnecting');};
    if(navigator.onLine===false){show();return;}
    if(!connectionTimer)connectionTimer=setTimeout(show,3000);
    if(which!=='away' || !RichOSConnection.managed(location.origin) || Date.now()-lastConnectionProbe<60000)return;
    lastConnectionProbe=Date.now();
    RichOSConnection.probe({fetch,online:()=>navigator.onLine}).then(info=>{
        if(generation!==connectionProbeGeneration || linkStateNow==='connected')return;
        const reason=RichOSConnection.classify(info);
        connectionDetail=reason==='mac-unreachable'?'reconnecting':reason;
        if(el.textContent)show();
    });
}

// A CONTROL IS RENDERED ONLY WHERE THE MAC HAS NAMED THE CAPABILITY BEHIND IT (plan §2 A).
//
// The app shipped "Hold to record" as one of the two biggest controls on the screen against a Mac
// that answers every voice note `503`. The decision itself is in `lib/api.js` and is default-deny,
// so this function cannot show a control by omission: it is only ever reached with an answer the
// Mac gave, and both halves ship `hidden` in `index.html`. What is left here is the DOM.
function applyCapabilities() {
	if (!api) return;
	const phase=voiceGesture.snapshot().phase;
	const voice = api.offers('voice');
	hold.hidden = !voice || (phase==='locked' && hold.dataset.pointerHeld!=='true') || phase==='finishing';
	$('hold-note').hidden = !voice;
}

/// Is there a stream the Mac has accepted, right now? The link is the only thing that knows.
function streamIsOpen() { return Boolean(link && link.isOpen()); }

/// Build the one link, once. Everything it needs — the thread, the cursor, the credential — is
/// read at the moment of each attempt rather than captured here, because an attempt three minutes
/// into an outage must sign the challenge the phone has THEN, not the one it had when the app
/// booted. That is the whole point of the slice.
function makeLink() {
    let firstOpen=true;
    // **THE CONVERSATION WHOSE `hello` THIS STREAM HAS HAD** (Ray's nightly 67 walk: a reopened
    // conversation drew its question cards above the request that asked them). The page's cursors
    // mix two numberings: a projection position (a `hello`, a backfill, a question row) and the
    // hub's one live sequence for every conversation (`phone/stream.rs`). Presented on a reopen,
    // the page's highest is read by the Mac as a hub number and the live frames after it put some
    // rows back on hub numbers and leave others on positions. So a conversation that is opened,
    // at boot or from the picker, asks for a `hello`, whose rows are all the Mac's own positions;
    // only a reconnect of the conversation already heard presents a cursor.
    let heard=null;
    // **THAT CURSOR IS THE STREAM'S OWN NUMBER, NEVER A ROW'S** (review
    // `rv-20261019T112205Z-d98f7bde-d3fb`: after a `hello`, a lost connection presented a projection
    // position and the replay put the question card above its request again). It is the SSE `id:`
    // of the last frame this stream delivered, which is always the hub's number (`phone/stream.rs`
    // `Frame::to_wire`, a `hello` included: `routes.rs` `hello_frame`). A Mac that sends no `id:`
    // leaves it unset, and the reconnect asks for a `hello`.
    let streamAt=null;
    const heardFrame=(id)=>{const n=id===''||id==null?NaN:Number(id);if(Number.isSafeInteger(n)&&(streamAt===null||n>streamAt))streamAt=n;};
	return globalThis.RichOSLink.createLink({
		open: async (handlers) => {
            if(firstOpen){firstOpen=false;await api.refreshChallenge();}
            const incomplete=thread.view([]).filter(row=>row.complete===false && Number.isSafeInteger(row.cursor)).map(row=>Math.max(0,row.cursor-1));
            // A conversation the page knows only from its rows has no title yet: no cursor, so the
            // Mac answers with a `hello` and its list (`noteThread`).
            const named=threads.some(t=>t.id===currentThreadId && t.title);
            const resume=heard===currentThreadId && named && streamAt!==null;
            return api.openEvents(currentThreadId,resume?Math.min(streamAt,...incomplete):null,handlers);
        },
		refresh: () => api.refreshChallenge(),
		onFailure: handleApiError,

		// The sentence on screen, and nothing else decides it.
		onState: (which) => {
			// A RETRY DOES NOT FLICKER THE LINE. Once he is being told the Mac is not reachable,
			// that sentence is TRUE and it stays up. An attempt against a Mac that is off fails in
			// a fraction of a second, so mapping every attempt to "Looking for your Mac…" would
			// flash the line at him six times in the first minute and then twice a minute for as
			// long as the walk lasts — the app fidgeting rather than telling him anything new.
			// The first attempt still says it, because then it is news.
			if (which === 'opening' && linkStateNow === 'away') return;
			if (which === 'open') setLinkState('connected');
			else if (which === 'opening') setLinkState('opening');
			else setLinkState('away');
			updateQueueBanner();
		},

		handlers: {
			open() { flushQueue(); },
			hello(data, id) {
				heard=currentThreadId;
				// A `hello` starts the count again: a Mac that restarted numbers from its projection.
				streamAt=null;heardFrame(id);
				// What this Mac can actually be asked for, before anything else in this handler:
				// the screen should never be one frame ahead of what the Mac has said it can do.
				applyCapabilities();
				oxfordFrom(data);
				// **BOTH SIDES OF THE CONVERSATION, AND THIS IS WHERE ONE OF THEM WENT.**
				//
				// The Mac puts the whole gated projection in every `hello` — *"The rows
				// themselves ride along, so the first paint needs no second request"*
				// (`app/src-tauri/src/phone/routes.rs:492-494`) — and it puts BOTH roles in it:
				// `rows_from_payload` keeps `user_message` and `rich_message` and drops only
				// machinery (`phone/rows.rs:49-109`, proved by its own
				// `only_the_conversation_reaches_the_phone_and_no_machinery_does`). This handler
				// read `vapid_public_key` and `threads` out of that frame and dropped
				// `data.messages` on the floor.
				//
				// What that looked like on his phone, on 2026-09-18: a list of answers with no
				// questions. At the time the live stream could only open a `role: "rich"` row, so
				// Rich's replies arrived and nothing he wrote ever did — not from this phone, not
				// from the Mac, and this was the one frame that carried his own words.
				//
				// **THAT LAST CLAUSE IS NO LONGER TRUE AND THIS HANDLER IS NO LESS NECESSARY.**
				// `rich://ceo-message` has existed since `102b7c07` (`phone/rows.rs`
				// `event_from_live`), so his own row arrives live as well — but only for turns
				// this stream was open for. `hello` is what a phone that was asleep, out of
				// range, or opened for the first time is caught up by, and it is still the only
				// frame that can carry a message from before the socket existed. So the rows are
				// merged before anything else in this handler touches the screen.
				if (Array.isArray(data.messages) && data.messages.length) {
					thread.merge(data.messages);
					messageCache.put(data.messages).catch(() => {});
					scheduleRender();
				}
				if (data.vapid_public_key) {
					vapidPublicKey = data.vapid_public_key;
					settings.set('vapidPublicKey', vapidPublicKey).catch(() => {});
				}
				takeThreadList(data.threads);
				if(notificationTarget)void resolveNotificationTarget().catch(handleApiError);
				flushQueue();
			},
			message(row, id) {
				heardFrame(id);
				messageCache.put(row).catch(() => {});
				if (row && elsewhere(row.thread_id)) { noteThread(row.thread_id); return; }
				thread.merge(row);
				scheduleRender();
				if (row && row.role === 'rich' && row.complete) askForTheQuestion(row);
			},
			// A conversation created or renamed on the Mac (`phone/mod.rs` `threads_changed`).
			threads(data, id) { heardFrame(id); takeThreadList(data && data.threads); },
			delta(data, id) { heardFrame(id); if (data && elsewhere(data.thread_id)) return; thread.applyDelta(data); scheduleRender(); },
			state(data, id) { heardFrame(id); if (oxfordFrom(data)) return; thread.applyState(data); scheduleRender(); }
		}
	});
}

function connectStream() {
	if (!currentThreadId) return;
	if (!link) link = makeLink();
	link.connect();
}

/// **ANOTHER CONVERSATION'S ROW IS KEPT FOR IT, NEVER DRAWN UNDER THIS ONE** (Ray's nightly 66
/// walk, `esc-20261019T075013Z-233dbe9d`). The Mac's hub is one broadcast for every conversation,
/// each frame naming its `thread_id` (`phone/rows.rs` `event_from_live`), so the stream opened for
/// the conversation on screen also carries every other one's. A row that names no conversation is
/// from an older Mac and is drawn where it always was.
function elsewhere(threadId) { return Boolean(threadId) && threadId !== currentThreadId; }

/// **A CONVERSATION CREATED AFTER THE PAGE LOADED.** The list rides only in a `hello`, which the Mac
/// sends only to a stream that presents no cursor (`phone/stream.rs` `replay_after_marked`), so a
/// new conversation is first seen here as a row naming a `thread_id` the page does not know. It is
/// listed at once, untitled, which brings up the picker. Its title comes in the `threads` frame
/// the Mac sends when a conversation is created or renamed (`phone/mod.rs` `threads_changed`), or
/// in the `hello` the stream asks for when it is opened (`makeLink`); the stream is not reopened
/// here, because reopening would drop the frames still in flight.
function noteThread(id) {
	if (!id || threads.some((t) => t.id === id)) return;
	threads = threads.concat([{ id, title: '' }]);
	settings.set('threads', threads).catch(() => {});
	renderThreadPicker();
}

/// The Mac's list from a `hello`, keeping a conversation this page learned from its rows that the
/// list does not name yet (the same lag as above).
function listFromHello(named) {
	return named.concat(threads.filter((t) => !t.title && !named.some((n) => n.id === t.id)));
}

/// The Mac's list, from a `hello` or from the `threads` frame it sends when a conversation is
/// created or renamed, kept for the next launch and shown in the picker.
function takeThreadList(named) {
	if (!Array.isArray(named) || !named.length) return;
	threads = listFromHello(named);
	settings.set('threads', threads).catch(() => {});
	renderThreadPicker();
}

/// The Oxford comma setting from a `hello` or a `state` frame (`said` below says what it decides).
/// True when the frame carried it; a `state` frame that did is the setting and never a row.
function oxfordFrom(data) {
	if (!data || typeof data.oxford_comma !== 'boolean') return false;
	if (globalThis.RichOxford.setKeeping(data.oxford_comma)) scheduleRender();
	return true;
}

// ---------------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------------

/// THE OXFORD COMMA SETTING, AS THE MAC SAYS IT (CEO, 2026-10-19). The switch is in the Mac's
/// Settings; this phone draws what Claude writes the way it says: with the comma before and/or/nor
/// deleted while the setting is off (the default), untouched while it is on. The Mac sends it in
/// every `hello` and, when he flips it, in a `state` frame of its own, so the change shows at once.
/// The rule is the desktop's own file (`app/ui/oxford-comma.js`, served here as
/// `/lib/oxford-comma.js`), so the two screens cannot disagree about what it deletes.
function said(text) { return globalThis.RichOxford.display(text); }

function scheduleRender() {
	if (renderQueued) return;
	renderQueued = true;
	requestAnimationFrame(() => {
        renderQueued=false;render(false);
        const model=thread;
        if(model && signer)void model.reconcileVoice(text=>signer.sha256Hex(text)).then(changed=>{if(changed && model===thread)render(false);}).catch(handleApiError);
    });
}

function stickToBottom() { following.resume(); }

const acknowledgeReply=RichOSNotificationTarget.receipts((thread,id)=>api.replySeen(thread,id));

function render(forceBottom) {
	const list = $('messages');
	const before=following.capture();
	const rows = thread.view(queue ? queue.all() : []);

	// "Delivered." belongs on the LAST thing he sent and nowhere else. Repeating it under every
	// message he has ever written turns the one state he might actually look for into wallpaper.
	let lastDelivered = -1;
	rows.forEach((row, i) => { if (!row.pending && row.role === 'ceo') lastDelivered = i; });

	// A question card he is typing an answer into is the SAME node across renders (see
	// `questionCard`), and it is never taken out of the list: a node leaving the document loses
	// focus, which on a phone closes the keyboard mid-word every time a reply streams in.
	const nodes = rows.map((row, i) => renderRow(row, i === lastDelivered));
	const keep = new Set(nodes);
	[...list.children].forEach((child) => { if (!keep.has(child)) child.remove(); });
	let at = list.firstChild;
	nodes.forEach((node) => { if (node === at) at = at.nextSibling; else list.insertBefore(node, at); });

	const older = $('older');
	if (thread.atTheBeginning() && rows.length) {
		older.hidden = false;
		older.textContent = 'This is the beginning of the conversation.';
	} else if (loadingOlder) {
		older.hidden = false;
		older.textContent = 'Loading earlier messages…';
	} else {
		older.hidden = true;
	}

	if (!rows.length) {
		const empty = document.createElement('li');
		empty.className = 'msg msg-rich';
		const p = document.createElement('p');
		p.className = 'msg-text';
		// **THE SENTENCE NAMES WHAT IS ON SCREEN, AND IT READS THE SCREEN TO FIND OUT.**
		//
		// It used to say "or hold the button and speak" unconditionally, on a phone whose composer
		// is a text field and a Send button and nothing else (Ray, candidate .11, §4.4). The
		// control it names is hidden unless the Mac's `hello` lists `voice` in `capabilities`, and
		// this build's Mac answers every voice note 503 and advertises `["text"]`
		// (`app/src-tauri/src/phone/routes.rs` `CAPABILITIES`) — so the sentence pointed at
		// something that was never going to be there.
		//
		// It asks `hold.hidden` rather than `api.offers('voice')` on purpose. `applyCapabilities`
		// stays the ONE place that decides whether the control is on screen (`test/controls.test.js`
		// holds that property); this reads the answer rather than deciding it a second time, so the
		// sentence cannot disagree with the screen it is describing.
		const canSpeak = !$('hold').hidden;
		p.textContent = streamIsOpen()
			? (canSpeak
				? 'Nothing here yet. Write to Rich, or hold the button and speak.'
				: 'Nothing here yet. Write to Rich.')
			: 'Nothing here yet. When your Mac is reachable, this is where the conversation appears.';
		empty.appendChild(p);
		list.appendChild(empty);
	}

	following.restore(before,{force:forceBottom});
    if(notificationTarget?.thread===currentThreadId) {
        const row=[...list.children].find(row=>row.dataset.messageId===notificationTarget.at);
        if(row){
            following.focus(row);
            const params=new URLSearchParams(location.hash.slice(1));
            if(params.get('thread')===notificationTarget.thread && params.get('at')===notificationTarget.at)history.replaceState(history.state,'',location.pathname+location.search);
            notificationTarget=null;
        }
    }
    void acknowledgeReply(currentThreadId,rows,!document.hidden && following.following);
}

function renderRow(row, isLastDelivered) {
	if (row.kind === 'question' && row.question && api && api.offers('questions')) return questionCard(row);
	if (row.kind === 'file' && row.file) return fileCard(row);
	const li = document.createElement('li');
	const mine = row.role === 'ceo' || row.pending;
	li.className = `msg ${mine ? 'msg-mine' : 'msg-rich'}`;
    li.dataset.messageId=row.id;

	if (row.kind === 'voice' && !row.text) {
		const voice = document.createElement('p');
		voice.className = 'msg-voice msg-text';
		voice.textContent = row.seconds
			? `Voice note, ${Math.round(row.seconds)} seconds.`
			: 'Voice note.';
		li.appendChild(voice);
	} else if (mine) {
		const text = document.createElement('p');
		text.className = 'msg-text';
		text.textContent = row.text || '';
		li.appendChild(text);
	} else {
		// Rich writes Markdown and the Mac draws it (Ray's nightly 67 walk: `**323**` on this
		// page); his own words above stay exactly as he typed them. What Claude wrote is drawn
		// through the Oxford comma setting first (CEO, 2026-10-19); the rule skips code.
		const text = document.createElement('div');
		text.className = 'msg-text';
		RichOSMarkdown.renderInto(text, said(row.text || ''), document);
		li.appendChild(text);
	}

	if (row.truncated) {
		const note = document.createElement('p');
		note.className = 'truncated';
		note.textContent = 'The rest of this is waiting on your Mac.';
		li.appendChild(note);
	}

	const meta = document.createElement('p');
	meta.className = 'msg-meta';

	if (row.from_microphone) {
		const mic = document.createElement('span');
		mic.textContent = 'Spoken';
		meta.appendChild(mic);
	}

	const when = document.createElement('span');
	when.textContent = timeOf(row);
	meta.appendChild(when);

	if (row.pending) {
		const state = document.createElement('span');
		if (row.sendState === 'blocked') {
			state.className = 'msg-state blocked';
			state.textContent = 'Not sent.';
		} else if (row.sendState === 'sending') {
			state.className = 'msg-state';
			state.textContent = 'Sending…';
		} else {
			state.className = 'msg-state waiting';
			state.textContent = 'Waiting to send — your Mac isn’t reachable from here.';
		}
		meta.appendChild(state);
	} else if (mine && isLastDelivered) {
		const state = document.createElement('span');
		state.className = 'msg-state';
		state.textContent = 'Delivered.';
		meta.appendChild(state);
	}

	li.appendChild(meta);

	// A message he sent that can never go needs the control that acts on it, beside it.
	if (row.pending && row.sendState === 'blocked') {
		const drop = document.createElement('button');
		drop.className = 'hear';
		drop.textContent = 'Remove this message';
		drop.addEventListener('click', () => queue.discard(row.clientId));
		li.appendChild(drop);
	}

	// "Hear it" — one tap, per reply, and only when the Mac says there is audio to fetch. Nothing
	// is synthesized for a reply he only reads.
	if (!mine && row.complete!==false && row.id && (row.has_audio || api?.offers('audio'))) {
		const hear = document.createElement('button');
		hear.className = 'hear';
		hear.type = 'button';
		hear.textContent = 'Hear it';
		hear.addEventListener('click', () => hearIt(row.id, hear));
		li.appendChild(hear);
	}

	return li;
}

// ---------------------------------------------------------------------------------------------
// A file from Rich — the Mac's card (`app/ui/rich-files.js`, round 36 screen 3) on this page
// ---------------------------------------------------------------------------------------------
//
// The Mac sends each file Rich attached as a Rich row of kind `file` (`phone/rows.rs`), carrying
// the card's own description and an id, never a path. Until this card the page drew that row as a
// bubble holding the file's name and nothing to open it with. A tap brings the bytes over on one
// signed `GET /api/messages?kind=file` (`api.fetchFile`) and opens them in the browser's own
// viewer, as the Mac opens the file in its usual app; a type the browser does not show is saved
// under its own name instead. A file moved or deleted on the Mac says so in
// the Mac's own sentence and offers "Ask Rich again", which sends what the Mac's card sends.
//
// What a tap is doing is kept per file id, because a streaming reply rebuilds every row.
const FILE_GONE = 'No longer on this Mac. It was moved or deleted.';
const FILE_PAGE = '<svg viewBox="0 0 40 48" aria-hidden="true"><path class="pg" d="M4 4.5A4 4 0 0 1 8 .5h18l13.5 13.5v29.5a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z"/><path class="fold" d="M26 .5v9.5a4 4 0 0 0 4 4h9.5z"/></svg>';
const fileTaps = new Map();
// What a phone's browser shows itself: the types the Mac names (`output_files.rs` `mime_of`) that
// are a document, a picture or a recording. SVG is left out on purpose: opened as a page it would
// run as this app. Anything else is saved under its own name, never opened as an unnamed page.
const FILE_SHOWN = /\.(pdf|png|jpe?g|gif|webp|heic|mp4|mov|m4v|webm|m4a|mp3|wav)$/i;

function fileCard(row) {
	const f = row.file;
	const name = f.name || row.text || 'File';
	const tap = fileTaps.get(f.id) || {};
	const state = f.exists === false || tap.gone ? 'missing' : tap.busy ? 'opening' : 'ready';
	const line = state === 'missing' ? FILE_GONE : tap.busy ? 'Coming over from your Mac…' : tap.note || f.meta || f.type || '';
	const el = (tag, cls, text) => { const n = document.createElement(tag); if (cls) n.className = cls; if (text) n.textContent = text; return n; };
	const li = el('li', 'msg msg-rich msg-file');
	li.dataset.messageId = row.id;
	li.dataset.s = state;
	const card = el('button', 'file-card');
	card.type = 'button';
	// The name a screen reader says stands in for the card's children, so it carries the line.
	card.setAttribute('aria-label', `Open ${name}` + (line ? `, ${line}` : ''));
	const glyph = el('span', 'file-glyph');
	glyph.innerHTML = FILE_PAGE;
	// The type inside the page, as on the Mac's card (16px here: this page claims no exemption).
	glyph.appendChild(el('span', 'file-ext', f.ext || 'FILE'));
	const text = el('span', 'file-text');
	text.append(el('span', 'file-name', name), el('span', 'file-line', line));
	card.append(glyph, text);
	card.addEventListener('click', () => openFile(row, name));
	li.appendChild(card);
	if (state === 'missing') {
		const ask = el('button', 'hear', 'Ask Rich again');
		ask.type = 'button';
		ask.addEventListener('click', () => askAgain(row.thread_id || currentThreadId, name));
		li.appendChild(ask);
	}
	const meta = el('p', 'msg-meta');
	meta.appendChild(el('span', null, timeOf(row)));
	li.appendChild(meta);
	return li;
}

async function openFile(row, name) {
	const f = row.file;
	const tap = fileTaps.get(f.id) || {};
	if (f.exists === false || tap.gone || tap.busy) return;
	// The viewer's window is opened INSIDE the tap: a browser lets a page open one only then, and
	// the file takes longer than that to come over. It is pointed at the file when it is here.
	const view = FILE_SHOWN.test(name) ? window.open('', '_blank') : null;
	fileTaps.set(f.id, { busy: true });
	scheduleRender();
	try {
		const blob = await api.fetchFile(f.id, row.thread_id || currentThreadId);
		const file = new File([blob], name, { type: blob.type || 'application/octet-stream' });
		const url = URL.createObjectURL(file);
		if (view) view.location.href = url;
		else {
			// A file the browser does not show, or no window allowed (an installed app may refuse
			// one): it is saved, under its own name.
			const link = document.createElement('a');
			link.href = url;
			link.download = name;
			link.click();
		}
		setTimeout(() => URL.revokeObjectURL(url), 60000);
		fileTaps.delete(f.id);
	} catch (err) {
		if (view) view.close();
		// A refusal the Mac marked final carries its own sentence (`routes.rs` `rich_file`).
		const sentence = err && err.message;
		fileTaps.set(f.id, sentence === FILE_GONE ? { gone: true }
			: { note: err && err.reason === 'unreachable' ? 'Your Mac isn’t reachable from here. Tap to try again.'
				: err && err.aboutThisMessage ? sentence : 'Your Mac could not send this file. Tap to try again.' });
		if (err && err.reason === 'revoked') handleApiError(err);
	}
	scheduleRender();
}

/// "Ask Rich again" on a file that is gone: the sentence the Mac's card sends, through the same
/// queue as anything typed here.
async function askAgain(threadId, name) {
	try {
		await queue.enqueue({ clientId: newClientId(), threadId, kind: 'text', text: `Please send ${name} again.`, sentAt: new Date().toISOString() });
		render(currentThreadId === threadId);
		flushQueue();
	} catch {
		setHoldNote('Your message was not saved. Try again.', true);
	}
}

// ---------------------------------------------------------------------------------------------
// A question card — his answers as buttons, one tap answers, as a click does on the Mac
// ---------------------------------------------------------------------------------------------
//
// Ray, nightlies 63 and 64: this page showed a question as its plain text, so clicking "Casual"
// did nothing. The Mac's card (`app/ui/questions.js`) and the native apps' (`QuestionCard.kt`) are
// the design, in the same words. What the phone shows after an answer is what the Mac says the
// question now is: the answer's reply, and then the Mac's own changed row on the stream.
//
// One node per question, kept while the question is unchanged, so a choice he is making or an
// answer he is typing survives every render a streaming reply causes.
const questionCards = new Map();

function questionCard(row) {
	const q = row.question;
	const key = JSON.stringify([row.text, row.created_at, q, globalThis.RichOxford.keeping()]);
	const held = questionCards.get(q.id);
	if (held && held.key === key) return held.li;
	const draft = held ? held.draft : { selected: new Set(), other: false, text: '', editing: false, busy: false, note: '' };
	if (held && held.revision !== q.revision) { draft.editing = false; draft.selected.clear(); draft.other = false; draft.text = ''; }

	const el = (tag, cls, text) => { const n = document.createElement(tag); if (cls) n.className = cls; if (text) n.textContent = text; return n; };
	const li = el('li', 'msg msg-rich msg-question');
	li.dataset.messageId = row.id;
	li.dataset.questionState = q.state;
	if (q.set_count > 1) li.appendChild(el('p', 'ask-aside', `Question ${q.set_index} of ${q.set_count}`));
	if (q.asker === 'Your team') li.appendChild(el('p', 'ask-aside', 'Your team, through Rich'));
	li.appendChild(el('p', 'msg-text', said(q.text)));
	const controls = el('div', 'ask-controls');
	const status = el('p', 'ask-status');
	status.setAttribute('role', 'status');
	li.append(controls, status);
	const meta = el('p', 'msg-meta');
	meta.appendChild(el('span', null, timeOf(row)));
	li.appendChild(meta);
	if (row.id && (row.has_audio || api.offers('audio'))) {
		const hear = el('button', 'hear', 'Hear it');
		hear.type = 'button';
		hear.addEventListener('click', () => hearIt(row.id, hear));
		li.appendChild(hear);
	}

	const labels = (answer) => q.options.filter((o) => answer.option_ids.includes(o.id)).map((o) => o.label)
		.concat(answer.text ? [answer.text] : []).join('; ');

	async function submit(optionIds, text) {
		if (draft.busy) return;
		draft.busy = true; draft.note = 'Saving your answer…'; draw();
		// The conversation this answer belongs to, fixed now: he can switch to another one while
		// it is on its way to the Mac.
		const threadId = row.thread_id || currentThreadId;
		try {
			const result = await api.answerQuestion({
				clientId: newClientId(), threadId, questionId: q.id,
				optionIds, text, expectedRevision: draft.editing ? q.revision : undefined
			});
			draft.busy = false;
			if (result.outcome === 'conversation_deleted') { draft.note = 'This conversation was deleted. Your answer was not sent.'; draw(); return; }
			draft.note = ''; draft.editing = false;
			if (result.question) {
				// Kept for its own conversation either way; shown only if that is still the one on
				// screen, because `thread` holds whichever conversation he is looking at now.
				const updated = Object.assign({}, row, { thread_id: threadId, question: result.question });
				if (currentThreadId === threadId) thread.merge(updated);
				messageCache.put(updated).catch(() => {});
			}
			// The same answer again is `already_answered` with the question unchanged, so no new
			// card is built for it: this one redraws as answered.
			if (!result.question || result.question.revision === q.revision) draw();
			scheduleRender();
		} catch (err) {
			draft.busy = false;
			draft.note = err && err.reason === 'refused' && err.aboutThisMessage
				? err.message
				: 'Your answer did not reach your Mac. Tap it again when your Mac is reachable.';
			draw();
			if (err && err.reason === 'revoked') handleApiError(err);
		}
	}

	function draw() {
		controls.textContent = '';
		status.textContent = draft.note;
		status.hidden = !draft.note;
		if (q.state === 'withdrawn') {
			controls.appendChild(el('p', 'ask-line', `Rich no longer needs this: ${q.withdrawal_reason || 'The work has ended'}`));
			return;
		}
		if (q.state === 'answered' && q.answer && !draft.editing) {
			controls.appendChild(el('p', 'ask-answer', `You answered: ${labels(q.answer)}`));
			controls.appendChild(el('p', 'ask-line', q.delivered ? 'Rich has your answer'
				: q.ended_before_taken ? 'This job stopped before Rich got your answer.'
					: q.remaining > 0 ? 'Waiting for the remaining answers'
						: q.waiting_for_turn ? 'It reaches Rich when his current reply ends' : 'On its way to Rich'));
			if (!q.delivered && !q.handoff_started) {
				const change = el('button', 'hear', 'Change answer');
				change.type = 'button';
				change.addEventListener('click', () => {
					draft.editing = true; draft.selected = new Set(q.answer.option_ids); draft.text = q.answer.text || '';
					draft.other = Boolean(draft.text); draw();
				});
				controls.appendChild(change);
			}
			return;
		}
		if (draft.editing) controls.appendChild(el('p', 'ask-line', 'Change your answer before Rich receives it.'));
		// A multiple-choice card has ONE Send answer: the chosen options and anything typed in
		// Other answer go together.
		const typed = () => (draft.other ? draft.text.trim() : '');
		const send = el('button', 'hear', 'Send answer');
		send.type = 'button';
		send.disabled = draft.busy || !(draft.selected.size || typed());
		send.addEventListener('click', () => submit([...draft.selected], typed()));
		q.options.forEach((option) => {
			const button = el('button', 'ask-option');
			button.type = 'button';
			button.disabled = draft.busy;
			button.appendChild(el('span', 'ask-label', said(option.label)));
			if (option.description) button.appendChild(el('span', 'ask-desc', said(option.description)));
			if (q.recommended === option.id) button.appendChild(el('span', 'ask-rec', q.asker === 'Your team' ? 'Your team recommends' : 'Rich recommends'));
			if (q.multiple) {
				button.setAttribute('aria-pressed', String(draft.selected.has(option.id)));
				button.addEventListener('click', () => {
					if (draft.selected.has(option.id)) draft.selected.delete(option.id); else draft.selected.add(option.id);
					draw();
				});
			} else {
				button.addEventListener('click', () => submit([option.id], ''));
			}
			controls.appendChild(button);
		});
		if (q.free_answer) {
			const other = el('button', 'ask-link', 'Other answer');
			other.type = 'button';
			other.setAttribute('aria-expanded', String(draft.other));
			other.addEventListener('click', () => { draft.other = !draft.other; draw(); if (draft.other) controls.querySelector('textarea').focus(); });
			controls.appendChild(other);
			if (draft.other) {
				const form = el('form', 'ask-other');
				const input = el('textarea');
				input.setAttribute('aria-label', 'Other answer');
				input.placeholder = 'Your answer';
				input.maxLength = 16384;
				input.value = draft.text;
				const custom = el('button', 'hear', 'Send answer');
				custom.type = 'submit';
				custom.disabled = draft.busy || !draft.text.trim();
				input.addEventListener('input', () => {
					draft.text = input.value;
					custom.disabled = draft.busy || !input.value.trim();
					send.disabled = draft.busy || !(draft.selected.size || typed());
				});
				form.appendChild(input);
				if (!q.multiple) form.appendChild(custom);
				form.addEventListener('submit', (event) => {
					event.preventDefault();
					if (input.value.trim()) submit(q.multiple ? [...draft.selected] : [], input.value.trim());
				});
				controls.appendChild(form);
			}
		}
		if (q.multiple) controls.appendChild(send);
	}

	draw();
	questionCards.set(q.id, { key, li, draft, revision: q.revision });
	return li;
}

function timeOf(row) {
	const raw = row.created_at || row.queuedAt;
	if (!raw) return '';
	const date = new Date(raw);
	return isNaN(date.getTime()) ? '' : TIME.format(date);
}

function renderThreadPicker() {
	const label = $('thread-picker-label');
	const picker = $('thread-picker');
	const title = $('thread-title');

	const current = threads.find((t) => t.id === currentThreadId);
	title.textContent = (current && current.title) || 'Rich';

	// A picker ONLY if there is more than one, and it lists what exists. It does not create threads
	// (plan §3.4) and there is no control here that pretends it can.
	if (threads.length <= 1) { label.hidden = true; return; }

	label.hidden = false;
	picker.textContent = '';
	threads.forEach((t) => {
		const option = document.createElement('option');
		option.value = t.id;
		option.textContent = t.title || 'Conversation';
		if (t.id === currentThreadId) option.selected = true;
		picker.appendChild(option);
	});
}

async function selectThread(id) {
    if(!threads.some(item=>item.id===id))return;
    await settings.set('draft:'+currentThreadId,composer.value);
	await voiceGesture.interrupt();
	currentThreadId = id;
    await restoreComposer();
	await renderVoiceDrafts();
	await settings.set('threadId', currentThreadId);
	thread.reset();
	const cached = await messageCache.recent(currentThreadId, 60);
	if (cached.length) thread.merge(cached);
	renderThreadPicker();
	render(true);
	connectStream();
}
$('thread-picker').addEventListener('change',event=>{notificationTarget=null;void selectThread(event.target.value).catch(handleApiError);});


// ---------------------------------------------------------------------------------------------
// Older messages, behind a scroll. Chunked loading, never a page number.
// ---------------------------------------------------------------------------------------------

$('thread').addEventListener('scroll', () => {
	const top=$('thread').scrollTop;
	if (top > 120 || loadingOlder || !thread || thread.atTheBeginning()) return;
	const oldest = thread.oldestCursor();
	if (oldest === null) return;
	loadOlder(oldest);
});

/// **A REPLY WITH NO QUESTION IN FRONT OF IT — the fallback, named as one.**
///
/// **THE CAUSE THIS WAS WRITTEN AROUND HAS BEEN FIXED, AND THIS STAYS AS THE FALLBACK.**
///
/// It used to say, correctly at the time, that *"there is no live event in this build that
/// carries a CEO turn"* — so a message the CEO typed on his Mac never reached this phone live
/// and Rich answered a question the phone never saw. `LiveEvent::CeoMessage` is that event and
/// it landed in `2c95c27d`; `rows.rs` translates `rich://ceo-message` into a row of his, and
/// `opens_a_row` counts it, so his words now arrive here the moment they are durable.
///
/// What is kept is the fallback, and it is worth keeping: a phone that was asleep, offline, or
/// between streams when that event went out has no way to ask for it again. So when a reply
/// settles and this phone is holding nothing of his in front of it, it asks the Mac for the rows
/// before it, on the backfill route it already has and already signs. The Mac-side limit that
/// made this necessary is real and unchanged — `PhoneBridge::snapshot` `try_lock`s the spine and,
/// while a turn holds it, serves a cache from BEFORE that turn (`phone/bridge.rs`).
///
/// Bounded on purpose: at most one request per reply (`reconciled`), and five rows — the question,
/// its answer, and room for the turn before them.
const reconciled = new Set();

async function askForTheQuestion(row) {
	if (!api || !currentThreadId || !row || !row.id) return;
	if (reconciled.has(row.id)) return;
	const shown = thread.view([]);
	const at = shown.findIndex((seen) => seen.id === row.id);
	if (at > 0 && shown[at - 1].role === 'ceo') return;
	reconciled.add(row.id);
	// The Mac's own numbering is the only ordering, and the LIVE sequence can sit behind it: the
	// hub takes one cursor per streamed reply and the projection counts one per message, so a turn
	// he starts on the Mac moves the projection by two and the hub by one. Asking from one past the
	// highest cursor either side knows is what makes the answer include his row rather than stop
	// just short of it.
	const from = Math.max(row.cursor || 0, thread.latestCursor() || 0) + 1;
	const threadId = currentThreadId;
	try {
		const page = await api.backfill(threadId, from, 5);
		messageCache.put(page.messages || []).catch(() => {});
		// He may have opened another conversation while this was on its way (review
		// `rv-20261019T081330Z-785564c3-187f`): the rows are kept for their own, never drawn here.
		if (currentThreadId !== threadId) return;
		thread.prependOlder(page);
		scheduleRender();
	} catch (err) {
		// Never a screen of its own: the reply he is reading is real and already on screen, and the
		// row in front of it arrives with the next `hello` whatever happens here.
		reconciled.delete(row.id);
		handleApiError(err);
	}
}

async function loadOlder(beforeCursor) {
	loadingOlder = true;
	render(false);
	const threadId = currentThreadId;
	const before = following.capture();
	try {
		const page = await api.backfill(threadId, beforeCursor, 40);
		messageCache.put(page.messages || []).catch(() => {});
		// The same rule as `askForTheQuestion`: an answer for a conversation he has left is kept
		// for it and never merged into the one on screen.
		if (currentThreadId === threadId) thread.prependOlder(page);
	} catch (err) {
		handleApiError(err);
	} finally {
		loadingOlder = false;
		render(false);
		// Keep his place: the new content went in above him, so the scroll position moves by
		// exactly as much as the document grew.
		if (currentThreadId === threadId) following.restore(before,{prepend:true});
	}
}

// ---------------------------------------------------------------------------------------------
// Verb 1 — send text
// ---------------------------------------------------------------------------------------------

const composer = $('composer');

async function restoreComposer() {
    composer.value=await settings.get('draft:'+currentThreadId,'');
    resizeComposer();
}
function resizeComposer() {
    $('send').disabled=composer.value.trim().length===0;
    composer.style.height='auto';
    composer.style.height=`${Math.min(composer.scrollHeight,window.innerHeight*0.4)}px`;
}
composer.addEventListener('input', () => {
    if(settings)settings.set('draft:'+currentThreadId,composer.value).catch(()=>setHoldNote('Your draft could not be saved. Keep this app open.',true));
	$('send').disabled = composer.value.trim().length === 0;
	// Grow with what he writes, up to the cap the stylesheet sets.
	composer.style.height = 'auto';
	composer.style.height = `${Math.min(composer.scrollHeight, window.innerHeight * 0.4)}px`;
});

composer.addEventListener('keydown', (event) => {
	// Enter sends, Shift+Enter is a new line — the desktop habit. On the iPhone's own keyboard the
	// return key inserts a line, which is why the Send button is the primary control and not a
	// convenience.
	if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
		event.preventDefault();
		sendText();
	}
});

$('send').addEventListener('click', () => { unlockPlayer(); sendText(); });

let sendingText=false;
async function sendText() {
    const text=composer.value.trim(), threadId=currentThreadId, original=composer.value;
    if(!text || sendingText)return;
    sendingText=true;$('send').disabled=true;
    try {
        await queue.enqueue({clientId:newClientId(),threadId,kind:'text',text,sentAt:new Date().toISOString()});
        notificationTarget=null;
        if(currentThreadId===threadId && composer.value===original){composer.value='';resizeComposer();}
        render(currentThreadId===threadId);flushQueue();
        try {await settings.set('draft:'+threadId,currentThreadId===threadId?composer.value:'');}
        catch {setHoldNote('Your message was saved to send. Keep this app open while it finishes.',true);}
    } catch(error) { setHoldNote('Your message was not saved. Your draft is still here. Try again.',true); }
    finally {sendingText=false;$('send').disabled=composer.value.trim().length===0;}
}

/// **THE SENTENCE UNDER THE SIX WORDS. There is one of it, because there is one way in.**
///
/// This was a pair of functions — `onTheTailscalePath()`, which read `location.hostname`, and
/// `whatTheSixWordsAre()`, which picked one of two sentences from it. The Mac served this app on
/// two kinds of origin: `https://<name>.local:8443` with a certificate it had made for itself and
/// asked the phone to install, and `https://<name>.ts.net:8443` with a publicly trusted one.
///
/// **CEO §61, 2026-09-18** removed the first: *"any mobile app or PWA is utterly useless within
/// the home network. The desktop app is a much better tool in that case … The Tailscale setup is
/// where we start now."* So the branch has one arm, and a branch with one arm is a sentence.
///
/// **§4.2 of Ray's walk is what the surviving sentence is for.** On the Tailscale path the phone
/// used to say *"They are the name of the certificate your Mac made for itself"* while the Mac had
/// just said, in bold on the same flow, that there is no certificate to install. A careful person
/// reading both concludes something IS wrong, at the moment he is being asked whether the two
/// screens match. What is true is that the words come from a root the Mac made and keeps, and that
/// nothing was put on the phone to get here.
// SINCE SAGE'S F2 THE WORDS ARE NOT THE NAME OF THE MAC'S KEY ALONE: they are worked out from this
// phone's key, the address it opened and the Mac's key, so the first sentence says that. The rest
// is the sentence it always was.
function whatTheSixWordsAre() {
	return 'They are worked out from this phone\'s own key, the address it opened and your Mac\'s key. Nothing was installed on this phone to get here, and nothing needs to be. If they match what is on your Mac, this phone is talking to your Mac and to nothing else.';
}

/// **WHAT THIS PHONE CALLS THE CONTROL THAT PUTS THE APP ON ITS HOME SCREEN.**
///
/// The Mac's card says *"Add Rich to your phone's Home Screen"*. On the CEO's Android phone, Chrome's
/// menu offers **"Install and create shortcut"** and there is no item by the other name at all
/// (Ray, candidate .11, §4.5, verified on the device). On his iPhone X, Safari's Share menu does
/// say "Add to Home Screen". One instruction cannot be right for both, and an instruction that
/// names a control the device does not have is one he cannot follow.
///
/// Returns `{ menu, item }` in the device's own words, or `null` when this app cannot say which
/// browser it is in. **`null` means SAY NOTHING**, not "guess": a wrong menu name sends him
/// looking for something that is not there, which is worse than the silence this replaced.
///
/// The user agent is the only thing that can answer this, and it is a string a browser is free to
/// lie in — which is survivable here because the cost of being wrong is a sentence, not a
/// decision. Nothing else in this app reads it.
function installControlName() {
	const ua = String(navigator.userAgent || '');
	// iPadOS reports itself as a Mac and gives itself away with touch points; iPhone and iPod say
	// what they are.
	const iOS = /iPad|iPhone|iPod/.test(ua)
		|| (/Macintosh/.test(ua) && typeof navigator.maxTouchPoints === 'number' && navigator.maxTouchPoints > 1);
	if (iOS) return { menu: 'the Share menu', item: 'Add to Home Screen' };
	// Chrome on Android. Every other Chromium browser on Android puts its own token in the same
	// string and has its own wording, so they are not claimed.
	if (/Android/.test(ua) && /Chrome\//.test(ua) && !/EdgA|OPR\/|SamsungBrowser|Firefox|DuckDuckGo/.test(ua)) {
		return { menu: "your browser's menu", item: 'Install and create shortcut' };
	}
	return null;
}

/// Is this app already on his home screen and running as one?
function alreadyInstalled() {
	if (navigator.standalone === true) return true;               // iOS, all versions
	return Boolean(window.matchMedia && window.matchMedia('(display-mode: standalone)').matches);
}

/// The sentence for a browser that cannot take a push at all until the app is installed — which
/// is iOS Safari in a tab, and is the case that used to be told NOTHING.
///
/// Android Chrome is deliberately not this case: it subscribes from a tab, which is why the page
/// needs no install instruction there and why §4.5 was only ever about the Mac's card.
function installSentence() {
	const control = installControlName();
	if (!control) return null;
	return `Rich can only reach you while this app is open. To let him reach you when it is closed, open ${control.menu} and choose \u201c${control.item}\u201d.`;
}

function newClientId() {
	if (crypto.randomUUID) return crypto.randomUUID();
	const bytes = crypto.getRandomValues(new Uint8Array(16));
	return Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join('');
}

// ---------------------------------------------------------------------------------------------
// The queue, and the sentence that tells him the truth about it
// ---------------------------------------------------------------------------------------------

let flushing = false;

// **ONE TIMER, AND IT BELONGS TO THE OUTBOX** — T3 Code's connection runtime,
// `docs/internals/connection-runtime.md:5-7`: *"Keeping retries and session lifetime here
// prevents competing reconnect loops when several views need the same environment."*
//
// The send path had no retry owner at all, so it borrowed the STREAM's: a message that failed
// sat still until `lib/link.js` happened to reconnect, on a backoff capped at 30 s. That is
// the 30.67 s, 54.47 s and 48.79 s Ray measured on nightly `.7`. `lib/queue.js` owns the
// policy now and answers `dueInMs()`; this is the single timer that honors it. Exactly one
// exists at a time, it is always cleared before it is replaced, and it is never set at all
// while the phone knows it is offline — T3's *"offline states ... wait for a wakeup instead
// of spending attempts on unchanged conditions"*.
let outboxTimer = null;

function scheduleOutboxDrain() {
	if (outboxTimer !== null) { clearTimeout(outboxTimer); outboxTimer = null; }
	if (!queue) return;
	const owed = queue.dueInMs();
	if (owed === null) return;                       // nothing is waiting on a clock
	// THE WAKEUP, NOT AN ATTEMPT. `online` below is what moves this, and the browser fires
	// it the moment the platform says the network is back — which is a better signal than
	// any number this app could pick.
	if (typeof navigator === 'object' && navigator && navigator.onLine === false) return;
	outboxTimer = setTimeout(() => { outboxTimer = null; flushQueue(); }, owed);
}

// Back on the network. A wakeup fires what is already owed rather than adding an attempt.
if (typeof window !== 'undefined' && window.addEventListener) {
	window.addEventListener('online', () => { connectionDetail = null; lastConnectionProbe = -Infinity; if (link) link.wake(); flushQueue(); });
}

async function flushQueue() {
	if (flushing || !queue || !api) return;
	flushing = true;
	try {
		const result = await queue.flush(api);
		// His own message, where he put it, from the Mac's own answer. See `thread.confirm`.
		(result.accepted || []).forEach(({ item, answer }) => thread.confirm(item, answer));
		if (result.reason === 'revoked') { await goRevoked(); return; }
		if (result.sent > 0 && result.waiting === 0 && result.blocked === 0) setLinkState('connected');
		if (result.reason === 'unreachable') setLinkState('away');
	} catch (err) {
		handleApiError(err);
	} finally {
		flushing = false;
		// The outbox decides when it is owed another try; this is the only place that acts
		// on the answer, so there is one timer however many things called `flushQueue`.
		scheduleOutboxDrain();
		updateQueueBanner();
		scheduleRender();
	}
}

function updateQueueBanner() {
	if (!queue) return;
	const waiting = queue.waitingCount();
	const blocked = queue.blocked().length;
	const banner = $('queue-banner');

	if (!waiting && !blocked) { banner.hidden = true; return; }
	banner.hidden = false;

	if (blocked) {
		$('queue-text').textContent = blocked === 1
			? 'One message could not be sent. Your Mac did not accept it.'
			: `${blocked} messages could not be sent. Your Mac did not accept them.`;
		$('queue-retry').textContent = 'Try again';
	} else {
		// The REASON lives on the message itself and in the line under the title; the banner is the
		// count and the control. Saying the whole sentence in all three places at once is how a
		// screen stops being read.
		$('queue-text').textContent = waiting === 1
			? 'One message is waiting to send.'
			: `${waiting} messages are waiting to send.`;
		$('queue-retry').textContent = 'Try now';
	}
}

$('queue-retry').addEventListener('click', () => {
	// He asked, so anything blocked gets one more chance rather than staying stuck forever —
	// and it goes NOW. This used to set `item.state` from out here, which left the outbox's
	// own `notBefore` where it was, so his tap would have been answered with a wait.
	queue.retryEverythingNow();
	flushQueue();
});

function handleApiError(err) {
	if (!err) return;
	if (err.reason === 'revoked') { goRevoked(); return; }
	if (err.reason === 'unreachable') { setLinkState('away'); return; }
	setLinkState('away', 'Your Mac answered, but not with something this app could use.');
}

async function goRevoked() {
	// FINAL, and it has to stop the retry loop as well as the socket. A revoked phone that went on
	// reconnecting every thirty seconds would be this app knocking on a door it has been told it is
	// not welcome at — which is the thing `lib/api.js` classifies REVOKED as never-retry to avoid.
	if (link) link.close();
	showTakeover('revoked');
	$('revoked-detail').textContent = 'Nothing you wrote has been lost from your Mac — this phone simply has no way in any more.';
	try {
		await queue.clear();
		await Storage.forgetEverything(db);
	} catch { /* the screen above is the important half */ }
}

// ---------------------------------------------------------------------------------------------
// Verb 2 — send a voice note
//
// The permission design is decision 1 at the top of this file. `opening` is the state between his
// finger going down and the microphone actually being open; `heldDown` is whether his finger is
// still there when it opens.
// ---------------------------------------------------------------------------------------------

const hold = $('hold');

const voiceGesture=globalThis.RichOSVoice.createGesture({
    start: async () => {
        const generation=++captureGeneration;
        opening=true;stopVoicePreview();unlockPlayer();
        if(player)player.pause();
        try {
            const stream=await navigator.mediaDevices.getUserMedia({audio:{channelCount:1,echoCancellation:false,noiseSuppression:false,autoGainControl:false}});
            if(generation!==captureGeneration){stream.getTracks().forEach(t=>t.stop());return;}
            micStream=stream;await startRecording(generation);
        } catch(error) {
            if(generation===captureGeneration){recording=false;teardownGraph();releaseMicrophone();chunks=[];}
            throw error;
        } finally {if(generation===captureGeneration)opening=false;}
    },
    finish: async ({send,context}) => stopRecording({send,context}),
    cancel: async () => {captureGeneration++;opening=false;recording=false;clearTimeout(autoStopTimer);teardownGraph();releaseMicrophone();chunks=[];},
    error: error => {setHoldNote(error.name==='NotAllowedError'?'Microphone access was denied. Allow it in your browser settings, or type your message.':error.message,true);},
    changed: state => {
        const active=state.phase!=='idle',locked=state.phase==='locked';document.body.dataset.voice=state.phase;
        hold.classList.toggle('recording',active);hold.classList.toggle('opening',state.phase==='preparing');
        hold.setAttribute('aria-label',state.phase==='preparing'?'Opening microphone…':active?'Recording · release to send':'Hold to record');
        applyCapabilities();
        $('voice-actions').hidden=!active;$('voice-lock').hidden=state.phase!=='held';$('voice-cancel').hidden=!locked;$('voice-send').hidden=!locked;
        $('meter').hidden=!active;$('meter-read').hidden=!active;
        if(active)setHoldNote(locked?'Recording hands free. Send when you’re finished.':'Slide left to cancel · slide up to lock',false);
        else {setLevel(0);applyCapabilities();}
    }
});
const gestureAction=promise=>Promise.resolve(promise).catch(()=>{});
let voicePointer=null,voiceX=0,voiceY=0;
hold.oncontextmenu=e=>e.preventDefault();
hold.addEventListener('pointerdown',event=>{
    if(voicePointer!==null || event.button!==0)return;
    event.preventDefault();hold.dataset.pointerHeld='true';voicePointer=event.pointerId;voiceX=event.clientX;voiceY=event.clientY;hold.setPointerCapture(voicePointer);
    gestureAction(voiceGesture.press({threadId:currentThreadId,origin:apiState.apiBase}));
});
hold.addEventListener('pointermove',event=>{if(event.pointerId===voicePointer)gestureAction(voiceGesture.move(event.clientX-voiceX,event.clientY-voiceY));});
hold.addEventListener('pointerup',event=>{if(event.pointerId===voicePointer){voicePointer=null;hold.dataset.pointerHeld='false';gestureAction(voiceGesture.release());applyCapabilities();}});
hold.addEventListener('pointercancel',()=>{voicePointer=null;if(voiceGesture.snapshot().phase!=='locked')gestureAction(voiceGesture.interrupt());});
hold.addEventListener('lostpointercapture',()=>{if(voicePointer!==null){voicePointer=null;if(voiceGesture.snapshot().phase!=='locked')gestureAction(voiceGesture.interrupt());}});
hold.addEventListener('click',event=>{if(event.detail===0)gestureAction(voiceGesture.press({threadId:currentThreadId,origin:apiState.apiBase}).then(()=>voiceGesture.lock()));});
$('voice-lock').onclick=()=>voiceGesture.lock();$('voice-send').onclick=()=>gestureAction(voiceGesture.send());$('voice-cancel').onclick=()=>gestureAction(voiceGesture.cancel());
setInterval(()=>{const start=voiceGesture.snapshot().startedAt, seconds=start?Math.floor((Date.now()-start)/1000):0;const value=`${Math.floor(seconds/60)}:${String(seconds%60).padStart(2,'0')}`;if($('voice-timer').textContent!==value)$('voice-timer').textContent=value;},250);
async function renderVoiceDrafts() {
    if(!voiceDraftStore)return;
    stopVoicePreview();
    const drafts=(await voiceDraftStore.all()).filter(d=>d.threadId===currentThreadId && d.origin===apiState.apiBase);
    const area=$('voice-drafts');area.hidden=!drafts.length;area.replaceChildren();
    for(const draft of drafts){
        const row=document.createElement('div');row.textContent=`Unsent voice message · ${Math.round(draft.seconds)} seconds `;
        const send=document.createElement('button');send.textContent='Send voice message';
        send.onclick=async()=>{send.disabled=true;try{await queue.enqueue(draft);await voiceDraftStore.remove(draft.clientId);await renderVoiceDrafts();render(true);flushQueue();}catch(e){send.disabled=false;setHoldNote(e.message,true);}};
        const discard=document.createElement('button');discard.textContent='Discard recording';discard.onclick=async()=>{await voiceDraftStore.remove(draft.clientId);await renderVoiceDrafts();};
        const play=document.createElement('button');play.textContent='Play recording';
        play.onclick=async()=>{
            if(voicePreview?.button===play){stopVoicePreview();return;}
            stopVoicePreview();unlockPlayer();
            const url=URL.createObjectURL(new Blob([draft.bytes],{type:'audio/wav'}));
            const ended=()=>{if(voicePreview?.button===play)stopVoicePreview();};
            voicePreview={url,button:play,ended};player.src=url;player.addEventListener('ended',ended,{once:true});play.textContent='Stop recording playback';
            try{await player.play();}catch(error){ended();setHoldNote(error.message,true);}
        };
        row.append(play,send,discard);area.append(row);
    }
}

async function startRecording(generation) {
	if (recording || !micStream || generation!==captureGeneration) return;
	chunks = [];
	recording = true;
	hold.classList.add('recording');
	$('meter').hidden = false;
	$('meter-read').hidden = false;
	setHoldNote('Speak now. Let go and it goes to your Mac.', false);

	if (!audioCtx) {
		const Ctx = window.AudioContext || window.webkitAudioContext;
		try {
			// Asking for 16 kHz directly saves the resampling when it is honored. Safari has
			// historically ignored or thrown on the option, so the REAL rate is read back from the
			// context and `pcm.js` converts from whatever we actually got.
			audioCtx = new Ctx({ sampleRate: pcm.TARGET_RATE });
		} catch {
			audioCtx = new Ctx();
		}
	}
	if (audioCtx.state === 'suspended') await audioCtx.resume();
	if(generation!==captureGeneration)return;

	const source = audioCtx.createMediaStreamSource(micStream);

	if (audioCtx.audioWorklet) {
		try {
			await audioCtx.audioWorklet.addModule('/lib/recorder-worklet.js');
			if(generation!==captureGeneration){source.disconnect();return;}
			const node = new AudioWorkletNode(audioCtx, 'richos-capture');
			node.port.onmessage = (event) => {
				if (!recording) return;
				chunks.push(pcm.downsample(event.data,audioCtx.sampleRate,pcm.TARGET_RATE));
				setLevel(pcm.measure([event.data]).rms);
			};
			source.connect(node);
			// A muted gain node keeps the graph being pulled WITHOUT routing the microphone to the
			// speaker, which on a phone held to the face is an instant feedback loop.
			const mute = audioCtx.createGain();
			mute.gain.value = 0;
			node.connect(mute);
			mute.connect(audioCtx.destination);
			captureNode = { node, source, mute, kind: 'worklet' };
		} catch {
            if(generation!==captureGeneration){source.disconnect();return;}
			captureNode = null;
		}
	}

	if (!captureNode) {
		const node = audioCtx.createScriptProcessor(4096, 1, 1);
		node.onaudioprocess = (event) => {
			if (!recording) return;
			const copy = new Float32Array(event.inputBuffer.getChannelData(0));
			chunks.push(pcm.downsample(copy,audioCtx.sampleRate,pcm.TARGET_RATE));
			setLevel(pcm.measure([copy]).rms);
		};
		source.connect(node);
		const mute = audioCtx.createGain();
		mute.gain.value = 0;
		node.connect(mute);
		mute.connect(audioCtx.destination);
		captureNode = { node, source, mute, kind: 'script' };
	}

	void navigator.wakeLock?.request('screen').then(lock=>{if(recording && generation===captureGeneration)recordingWakeLock=lock;else void lock.release();}).catch(()=>{});
	autoStopTimer = setTimeout(() => { if (recording) gestureAction(voiceGesture.interrupt()); }, MAX_RECORD_SECONDS * 1000);
}

function setLevel(rms) {
	const db = pcm.toDbfs(rms);
	// -60 dBFS at the left, 0 at the right. A linear amplitude bar spends nearly all its travel in
	// the top few decibels and reads as dead for ordinary speech, which is the opposite of what a
	// level meter is for.
	const pct = Math.max(0, Math.min(100, ((db + 60) / 60) * 100));
	$('meter-fill').style.setProperty('--level', `${pct.toFixed(1)}%`);
	const heard=db>-50?'Sound detected':'Waiting for speech';
    if($('meter-read').textContent!==heard)$('meter-read').textContent=heard;
}

function teardownGraph() {
	if (!captureNode) return;
	try {
		if (captureNode.kind === 'worklet') captureNode.node.port.postMessage('stop');
		else captureNode.node.onaudioprocess = null;
		captureNode.source.disconnect();
		captureNode.node.disconnect();
		captureNode.mute.disconnect();
	} catch { /* teardown must not throw over a message */ }
	captureNode = null;
}

function releaseMicrophone() {
    releaseRecordingWakeLock();
	// The tracks are stopped between notes so the phone's own recording indicator is not left on
	// after he has finished speaking. Within one launch iOS does not ask again, so the next hold
	// opens straight away; across launches it asks whatever we do, which is what the design above
	// is built around.
	if (!micStream) return;
	micStream.getTracks().forEach((track) => { try { track.stop(); } catch { /* already stopped */ } });
	micStream = null;
}

async function stopRecording({send=false,context}={}) {
	if (!recording) return;
	recording = false;
	clearTimeout(autoStopTimer);
	hold.classList.remove('recording');
	hold.setAttribute('aria-label','Hold to record');
	// His finger is off it, so a `hello` that arrived mid-hold and withdrew voice takes effect now.
	applyCapabilities();
	teardownGraph();
	setLevel(0);
	$('meter').hidden = true;
	$('meter-read').hidden = true;

	const captureRate = pcm.TARGET_RATE;
	const result = pcm.analyze(chunks, captureRate);
	chunks = [];
	releaseMicrophone();

	if (result.outcome === 'too-short') {
		setHoldNote('That was too quick — hold the button while you speak.', true);
		return;
	}
	if (result.outcome === 'no-samples') {
		setHoldNote('The microphone opened but nothing came through. Try once more.', true);
		return;
	}
	if (result.outcome === 'silent') {
		// Sending it would cost him a transcription of nothing and a reply about nothing.
		setHoldNote('We could not hear your message. Try again closer to the microphone.', true);
		return;
	}

	setHoldNote('Hold the button, speak, and let go to send.', false);

    const draft={clientId:newClientId(),threadId:context.threadId,origin:context.origin,kind:'voice',bytes:new Uint8Array(result.wav),seconds:Number(result.seconds.toFixed(2)),codec:'wav16k',sampleRate:pcm.TARGET_RATE,sentAt:new Date().toISOString()};
    // Persist before enqueue so a storage/network failure cannot discard the recording.
    await voiceDraftStore.put(draft);
    if(send){await queue.enqueue(draft);await voiceDraftStore.remove(draft.clientId);render(true);flushQueue();}
    else setHoldNote(result.seconds>=MAX_RECORD_SECONDS-1 ? 'Your 30-minute voice message is saved below. Send it, then start another.' : 'Recording interrupted. Your voice message is saved below.',false);
    await renderVoiceDrafts();

}

function setHoldNote(text, trouble) {
	const note = $('hold-note');
	note.textContent = text;
	note.classList.toggle('trouble', Boolean(trouble));
}

// Leaving the app mid-hold suspends the audio context; stop cleanly rather than keeping a dead
// graph and a recording that will never end.
document.addEventListener('visibilitychange', () => {
	if (document.hidden) {
		gestureAction(voiceGesture.interrupt());
		return;
	}
	// Back on screen: this is one of the three moments a queued message can go, because a PWA
	// cannot flush anything in the background (§2.4).
	//
	// WAKE, NEVER CONNECT. `if (!streamOpen) connectStream()` looks like the same thing and is
	// not: `streamOpen` was false for the whole of an outage, so every foreground during one
	// started another attempt, on top of the browser's own reconnect loop and on top of anything
	// already in flight — three openers, and the app could only ever hold the last source any of
	// them produced. `wake()` fires the retry that is ALREADY OWED, immediately, because he is
	// looking at the screen; it opens nothing beside anything.
	if (link) link.wake(); else connectStream();
	if (thread) render();
	flushQueue();
});

// ---------------------------------------------------------------------------------------------
// Verb 4 — hear Rich
// ---------------------------------------------------------------------------------------------

function unlockPlayer() {
	if (playerUnlocked || !player) return;
	// Safari will not start audio outside a user gesture, and an `await` loses the gesture. So the
	// element is started on ten milliseconds of our own silence inside his first tap; from then on
	// it can be played with real audio after a fetch.
	player.src = silentWavUrl;
	const attempt = player.play();
	if (attempt && attempt.catch) attempt.catch(() => { /* it will be unlocked by a later tap */ });
	playerUnlocked = true;
}

async function hearIt(messageId, button) {
    stopVoicePreview();
	if (playingButton && playingButton !== button) {
		playingButton.textContent = 'Hear it';
		playingButton.classList.remove('playing');
	}
	if (player && !player.paused && playingButton === button) {
		player.pause();
		button.textContent = 'Hear it';
		button.classList.remove('playing');
		return;
	}

	button.disabled = true;
	button.textContent = 'Getting it…';
	try {
		const blob = await api.fetchAudio(messageId);
		const url = URL.createObjectURL(blob);
		player.onended = () => {
			button.textContent = 'Hear it';
			button.classList.remove('playing');
			URL.revokeObjectURL(url);
		};
		player.src = url;
		await player.play();
		button.textContent = 'Stop';
		button.classList.add('playing');
		playingButton = button;
	} catch (err) {
		button.textContent = 'Hear it';
		handleApiError(err);
		if (err && err.name === 'NotAllowedError') setLinkState('away', 'Tap "Hear it" once more — this phone wants a tap before it will play sound.');
	} finally {
		button.disabled = false;
	}
}

// ---------------------------------------------------------------------------------------------
// Push (plan §3.4). The phone obeys the tiers the Mac sends and invents no policy of its own.
// ---------------------------------------------------------------------------------------------

async function refreshPushOffer() {
	const offer = $('push-offer');
	if (!('serviceWorker' in navigator) || !('PushManager' in window) || !('Notification' in window)) {
		// THIS BROWSER CANNOT TAKE A PUSH AT ALL, and on iOS that is not a dead end — it is an app
		// that has not been installed yet. Saying nothing, which is what this used to do, left him
		// with no way to find out. The control is named in the device's own words or not at all.
		const sentence = installSentence();
		if (sentence && !alreadyInstalled()) {
			offer.hidden = false;
			$('push-offer-text').textContent = sentence;
			// There is no control this app can offer that does this: the item is in the browser's
			// own menu, so the sentence says where it is instead of pretending to be a button.
			$('push-on').hidden = true;
			return;
		}
		offer.hidden = true;
		return;
	}
	if (Notification.permission === 'denied') {
		// There is no control this app can offer that will change this — iOS only allows it from
		// Settings — so the sentence says where the control is instead of pretending to be one.
		offer.hidden = true;
		$('push-offer-text').textContent = 'Notifications are switched off for this app in your phone\u2019s settings, so Rich cannot reach you when it is closed.';
		$('push-on').hidden = true;
		return;
	}
	if (Notification.permission === 'granted') {
		const registration = await navigator.serviceWorker.getRegistration();
		const existing = registration ? await registration.pushManager.getSubscription() : null;
		if (existing && api) await api.registerPush(existing.toJSON());
		offer.hidden = Boolean(existing) || notificationsDeclined;
		return;
	}
	offer.hidden = false;
	$('push-on').hidden = false;
	// **THE ANSWER HE ALREADY GAVE, KEPT ACROSS A RELOAD.** Ray, candidate .11, §5: after he
	// declined, the banner said the right thing — "Notifications stayed off, so Rich can only
	// reach you while this app is open" — and a reload put the generic offer back.
	//
	// The permission cannot carry this. Both a phone that has never been asked and a phone whose
	// owner dismissed the system prompt report `Notification.permission === 'default'`, which is
	// exactly what Ray hit: he allowed Chrome's site prompt and dismissed Android's, and the
	// browser's state was indistinguishable from never having asked. So what is remembered is HIS
	// ANSWER, on this phone, and it is cleared the moment the permission is granted.
	$('push-offer-text').textContent = notificationsDeclined
		? 'Notifications stayed off, so Rich can only reach you while this app is open.'
		: 'Notifications are off, so Rich cannot reach you when this app is closed.';
    offer.hidden=notificationsDeclined;
}

$('push-later').onclick=async()=>{notificationsDeclined=true;$('push-offer').hidden=true;await settings.set('notificationsDeclined',true);};
$('push-on').addEventListener('click', async () => {
	$('push-on').disabled = true;
	try {
		// The request must be inside the gesture, which this is.
		const permission = await Notification.requestPermission();
		if (permission !== 'granted') {
			notificationsDeclined = true;
			settings.set('notificationsDeclined', true).catch(() => { /* the sentence is the half that matters now */ });
			$('push-offer-text').textContent = 'Notifications stayed off, so Rich can only reach you while this app is open.';
			return;
		}
		// He said yes. The earlier no is not his position any more, and a stale one would put the
		// wrong sentence in front of him the next time the offer is shown for any other reason.
		notificationsDeclined = false;
		settings.set('notificationsDeclined', false).catch(() => {});
		if (!vapidPublicKey) {
			$('push-offer-text').textContent = 'Your Mac has not sent this phone a notification key yet. Open this again once your Mac is reachable.';
			return;
		}

		const registration = await navigator.serviceWorker.register('/sw.js', { scope: '/' });
		await navigator.serviceWorker.ready;

		let subscription = await registration.pushManager.getSubscription();
		if (subscription) {
			const current = new Uint8Array(subscription.options.applicationServerKey || new ArrayBuffer(0));
			const wanted = base64UrlToBytes(vapidPublicKey);
			const same = current.length === wanted.length && current.every((v, i) => v === wanted[i]);
			if (!same) { await subscription.unsubscribe(); subscription = null; }
		}
		if (!subscription) {
			subscription = await registration.pushManager.subscribe({
				// Required by WebKit: every push must display a notification, so there is no such
				// thing as a silent one to ask for.
				userVisibleOnly: true,
				applicationServerKey: base64UrlToBytes(vapidPublicKey)
			});
		}
		await api.registerPush(subscription.toJSON());
		$('push-offer').hidden = true;
	} catch (err) {
		$('push-offer-text').textContent = 'Notifications could not be turned on just now. Try again when your Mac is reachable.';
		handleApiError(err);
	} finally {
		$('push-on').disabled = false;
        if($('settings').open)void notificationSettings();
	}
});

function base64UrlToBytes(value) {
	const padding = '='.repeat((4 - (value.length % 4)) % 4);
	const raw = atob((value + padding).replace(/-/g, '+').replace(/_/g, '/'));
	const out = new Uint8Array(raw.length);
	for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
	return out;
}

if ('serviceWorker' in navigator) {
	navigator.serviceWorker.addEventListener('message', (event) => {
		const data = event.data || {};
		// A push arrived while the app was open, or he tapped one. Either way the service worker
		// hands over the row it already wrote to storage, so the screen and the cache agree.
		if (data.type === 'push-message' && data.message) {
			if (data.api_base && api) api.setApiBase(data.api_base);
			if (!thread) return;
			if (elsewhere(data.message.thread_id)) { noteThread(data.message.thread_id); return; }
			thread.merge(data.message);
			scheduleRender();
		}
        if(data.type==='notification-clicked')void openNotificationURL(data.url).catch(handleApiError);
	});
	if (navigator.clearAppBadge) navigator.clearAppBadge().catch(() => {});
	// Registered at boot as well as at the notification offer, because the worker is also what
	// caches the shell so the app opens where the origin does not resolve.
	navigator.serviceWorker.register('/sw.js', { scope: '/' }).catch(() => { /* reported by the offer */ });
}

// ---------------------------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------------------------

player = new Audio();
player.preload = 'auto';
silentWavUrl = URL.createObjectURL(new Blob([pcm.encodeWav(new Float32Array(160), pcm.TARGET_RATE)], { type: 'audio/wav' }));

boot().catch((err) => {
	setLinkState('away', 'This app could not start. Tell Rich what it said: ' + String(err && err.message ? err.message : err));
});

// Exposed for the browser harness only: it drives the real app rather than a copy of it, and this
// is how it reads the state it is asserting about. Nothing in the app reads these back.
globalThis.__richosPhone = {
	get queue() { return queue; },
	get thread() { return thread; },
    get notificationTarget(){return notificationTarget;},
	get api() { return api; },
	get state() { return apiState; },
	get link() { return link; },
	/// Whether the app believes he is reading the newest message. The harness asserts the
	/// PIXELS either way; this is here so a failure can say which half went wrong.
	get pinnedToBottom() { return following.following; },
	render, flushQueue, connectStream, applyCapabilities
};

async function openNotificationURL(url) {
    const checked=RichOSInbound.validateNavigate(url);
    if(!checked.ok)return;
    const params=new URLSearchParams(new URL(checked.value,location.origin).hash.slice(1));
    const id=params.get('thread'),at=params.get('at');
    if(!id || !at)return;
    notificationTarget={thread:id,at};
    await resolveNotificationTarget();
}
let resolvingNotification=false, notificationAgain=false;
async function resolveNotificationTarget() {
    const target=notificationTarget;
    if(!target || !conversationReady || !thread || !threads.some(item=>item.id===target.thread))return;
    if(currentThreadId!==target.thread)await selectThread(target.thread);
    render(false);
    if(notificationTarget!==target)return;
    if(resolvingNotification){notificationAgain=true;return;}
    resolvingNotification=true;
    try {
      const row=await RichOSNotificationTarget.find({model:thread,matches:row=>row.id===target.at,isCurrent:()=>notificationTarget===target && currentThreadId===target.thread,fetchPage:async before=>{const page=await api.backfill(target.thread,before,50);await messageCache.put(page.messages || []);return page;}});
      if(row)render(false);
    }finally{resolvingNotification=false;if(notificationAgain){notificationAgain=false;void resolveNotificationTarget().catch(handleApiError);}}
}
async function notificationSettings() {
    $('notification-previews').checked=await settings.get('notificationPreviews',true);
    const supported='Notification' in globalThis && 'PushManager' in globalThis && 'serviceWorker' in navigator;
    const registration=supported?await navigator.serviceWorker.getRegistration():null;
    const subscription=registration?.pushManager?await registration.pushManager.getSubscription():null;
    $('notification-status').textContent=!supported?'This browser does not support reply notifications.':subscription?'Reply notifications are on.':Notification.permission==='denied'?'Notifications are denied. Enable them in your phone’s settings.':'Reply notifications are off.';
    $('notifications-enable').hidden=!!subscription;$('notifications-enable').disabled=!supported || Notification.permission==='denied';
    $('notifications-disable').hidden=!subscription;
}
$('settings-open').onclick=()=>{$('notification-previews').disabled=true;$('settings').showModal();notificationSettings().catch(()=>{$('notification-status').textContent='Notification settings could not be loaded. Try again.';}).finally(()=>{$('notification-previews').disabled=false;});};
$('settings-close').onclick=()=>$('settings').close();
$('notifications-enable').onclick=()=>{$('push-on').click();};
$('notifications-disable').onclick=async()=>{
    try {const registration=await navigator.serviceWorker.getRegistration();const subscription=await registration?.pushManager.getSubscription();if(subscription && !await subscription.unsubscribe())throw Error('unsubscribe');notificationsDeclined=true;await settings.set('notificationsDeclined',true);await api.registerPush(null);await notificationSettings();await refreshPushOffer();}
    catch {$('notification-status').textContent='Notifications could not be disabled. Try again.';}
};
$('notification-previews').onchange=async()=>{
    try {await settings.set('notificationPreviews',$('notification-previews').checked);}
    catch {$('notification-previews').checked=!$('notification-previews').checked;$('notification-status').textContent='Your preview preference could not be saved. Try again.';}
};

})();
