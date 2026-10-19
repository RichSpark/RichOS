# The route contract this app was built against — a STUB, and it is not the contract

**Status: provisional.** The contract is `phone-channel.md`, it is Echo's (`echo-opus-mac1`), it will
sit with the other architecture notes under docs/architecture — and **it did not exist on `main` at
`e39bc835`** when this app was written (that directory then held `app-engine-runtime.md`,
`data-migration.md` and `desktop-work.md`, and nothing else). The brief's instruction for that case
was to build against a stub written from the plan and to reconcile when the real file lands, saying
what changed. **Nothing below outranks that file once it exists.**

So this file is what the phone assumes. Every assumption below is derived from
`richos-hq/docs/plans/richos-phone-client-2026-09-18.md` — §2.5 (the four routes and the flat 404),
§2.6 (fetch + SSE, no WebSocket), §2.7 (P-256 device key, signed challenge, no body encryption),
§4.2 (vi) (Mac-issued cursors, the ledger is the queue) and §10.7 (the API base is DATA, the origin
is identity). Where the plan settled something, the section is cited; where the plan was silent and
the phone had to choose, it says **CHOSEN HERE** and that is a thing for Echo to overrule rather
than to inherit.

Every route lives behind one JavaScript module — `lib/api.js` — for exactly this reason. Reconciling
with the real contract is an edit to that file and its tests, not to the app.

---

## 0. The origin, and the API base (§10.7)

- The **origin never changes**: `https://<his-mac>.local:8443`. It is the app's identity — service
  worker scope, push subscription, cache, Home Screen icon. The app does not read its own address
  from anywhere; it *is* the address.
- The **API base is one stored value**, read at every send. At home it is the origin itself. Away it
  is whatever the Mac last advertised. It is updated from (a) the `hello` event of the stream and
  (b) the encrypted push payload, and from nothing else.
- When the base is not the origin, requests are ordinary cross-origin `fetch` with CORS, and the Mac
  must answer the preflight. **CHOSEN HERE:** `Access-Control-Allow-Headers` must include
  `authorization, content-type`, `Access-Control-Allow-Origin` must echo the app origin exactly, and
  `Access-Control-Allow-Credentials` is not used — the credential is a signature in a header, never a
  cookie.

## 1. Authentication — one header on every request

`Authorization: RichOS-Device <device_id>.<challenge>.<base64url(signature)>`

- `signature` is ECDSA P-256 / SHA-256, by the device's **non-extractable** WebCrypto key (§2.7),
  over the UTF-8 string:

  ```
  <challenge>\n<METHOD>\n<path-with-query>\n<sha256-hex of the request body, or the empty string>
  ```

- `challenge` is **server-issued and never client-generated** — the phone replays the most recent one
  the Mac gave it. Every response may carry a fresh one in `X-RichOS-Challenge`; the `hello` event
  carries one; `/api/pair` returns the first.
- **CHOSEN HERE, and it is the piece most likely to be wrong:** the plan says the phone "signs a
  server-issued challenge per session" without saying where the signature goes. A fifth route would
  contradict §2.5's "four routes and no more", so the signature rides on every request instead of
  being exchanged for a session token. If Echo's contract issues a session token, `lib/api.js`
  changes and nothing above it does.
- Anything unauthenticated gets a **flat 404** (§2.5 item 4) — not a 401, not a hint.
- A device the Mac has forgotten gets **403 with `{"revoked":true}`**. The phone treats that as
  final: it stops, clears its credential, and says so. It never retries a revoked device.

## 2. The four routes

### (a) `POST /api/messages` — post a message

Text:

```
POST /api/messages
Content-Type: application/json
{"client_id":"<uuid>","thread_id":"<id>","kind":"text","text":"…","sent_at":"<ISO-8601>"}
```

Voice (**CHOSEN HERE**: a binary body rather than base64 — a 20-second note is ~640 KB per §3.3 and
base64 would make it ~850 KB for nothing):

```
POST /api/messages?client_id=<uuid>&thread_id=<id>&kind=voice&codec=wav16k&sample_rate=16000&seconds=<n>&sent_at=<ISO-8601>
Content-Type: audio/wav
<the WAV bytes>
```

`codec` is explicit and always sent, so native can send `opus` later without changing the contract
(§5). Response, both forms:

```
200 {"message_id":"…","cursor":<monotone integer>,"thread_id":"…","accepted_at":"…","duplicate":false}
```

**Idempotent on `client_id`**, which is the phone's own de-duplication key and maps onto the intake
log's `intake_id` (§4.2 (i)). A repeat of a `client_id` the Mac already holds returns the original
`message_id` and `cursor` with `"duplicate":true` — never a second message. This is what makes the
on-phone queue safe to retry after an ambiguous failure.

### (b) `GET /api/events` — the stream, and the backfill

Live (§2.6, `EventSource`):

```
GET /api/events?thread_id=<id>&since=<cursor>
Accept: text/event-stream
```

**CHOSEN HERE:** `EventSource` cannot set an `Authorization` header, so the stream — and only the
stream — carries its credential in the query string as `auth=<the same token>`. Named because it is a
real weakening: the token appears in a URL. It is single-use against a server-issued challenge, so a
copied URL is not a replayable credential, and there is no proxy in the path to log it (§2.6: the Mac
answers the phone directly). If Echo would rather the stream be a `fetch` with a streamed body reader,
that is strictly better and `lib/api.js` is where it changes.

Event types the phone understands (anything else is ignored rather than treated as an error):

| event | data | what the phone does |
|---|---|---|
| `hello` | `{challenge, api_base, thread_id, latest_cursor, threads:[{id,title}], vapid_public_key}` | stores the challenge and the API base, lists threads, flushes the send queue |
| `message` | a full message row (below) | merges it by `cursor` |
| `delta` | `{message_id, cursor, text}` | appends streamed text to that reply |
| `state` | `{message_id, state}` | updates one row |
| `heartbeat` | `{}` | proves the stream is alive |
| `threads` | `{threads:[{id,title}]}` | sent when a conversation is created or renamed on the Mac; lists threads as a `hello` does |

A message row:

```
{"id":"…","thread_id":"…","cursor":12,"role":"ceo"|"rich","kind":"text"|"voice",
 "text":"…","created_at":"…","client_id":"…"|null,"has_audio":true|false,
 "from_microphone":true|false,"state":"…","complete":true|false}
```

`cursor` is **issued by the Mac and is the only ordering** (§4.2 (vi)). The phone never invents an
order and never sorts by its own clock.

Backfill for infinite scroll (**CHOSEN HERE**, same route, no stream):

```
GET /api/events?thread_id=<id>&before=<cursor>&limit=<n>
Accept: application/json
→ {"messages":[…],"more":true|false}
```

Chunked loading behind a scroll is explicitly fine; **page numbers and pagination controls are
never built** (standing rule).

### (c) `GET /api/audio/<message_id>` — one blob, by an id the Mac minted

Returns `audio/wav` for "hear it" (§3.4 verb 4). Synthesized on request; nothing is produced for a
reply he only reads. The phone never constructs an id — it uses one the Mac sent it (§2.5 item 5).

### (d) `POST /api/pair` — pairing, and the device record

Pairing (open for 60 seconds after he opens the QR, then not — §4.1):

```
POST /api/pair
{"code":"<one-shot>","public_key_jwk":{…},"device_name":"iPhone"}
→ {"device_id":"…","ca_fingerprint_sha256":"<hex>","vapid_public_key":"…","challenge":"…",
   "api_base":"…","threads":[…]}
```

The phone renders `ca_fingerprint_sha256` **as six words** (`lib/fingerprint.js`) and never as hex.
The Mac must show **the same six words**, which means it must use the same word list and the same
derivation — the one thing in this file Echo cannot choose freely, because the whole point is that
the two screens match. The derivation is deliberately trivial to reimplement in Rust:

> take the SHA-256 of the CA certificate's DER; the first six bytes index
> `lib/wordlist.js`'s 256-word list; join with spaces.

Updating the device record afterwards (**CHOSEN HERE** — the push subscription has to go somewhere
and a fifth route is not available):

```
POST /api/pair            (authenticated, no code)
{"device_id":"…","push":{"endpoint":"…","keys":{"p256dh":"…","auth":"…"}},"push_transport":"web-push"}
→ {"ok":true,"challenge":"…"}
```

`push_transport` exists from day one so APNs and FCM slot in beside Web Push rather than replacing
it (§5).

## 3. The push payload

Encrypted end to end by RFC 8291 (`aes128gcm`) — Apple relays a blob it cannot read (§2.7). The
phone expects:

```
{"notification":{"title":"Rich","body":"<his actual sentence>","navigate":"/#thread=<id>&at=<message_id>"},
 "message":{…a message row…},
 "api_base":"…",
 "tier":"interrupt_now"|"digest",
 "truncated":true|false}
```

- The body carries **Rich's words**, not "you have a message" (§2.4).
- If the reply did not fit in the ~3,500 characters APNs leaves after encryption overhead, `truncated`
  is true and the phone says *"the rest is waiting on your Mac"* — never a truncation dressed up as a
  complete answer (§2.4).
- The service worker **writes `message` into the cache as it shows the notification**, so opening the
  app away from the Mac shows the reply (§2.4).
- **The phone obeys the tier and invents no policy** (§3.4). `silent` never arrives because the Mac
  never sends it; `digest` arrives already batched. Every push that arrives shows a notification,
  because WebKit requires it.

## 4. What the phone needs from the Mac that is not a route

- The **manifest** at `/manifest.webmanifest` as `application/manifest+json` with
  `display: standalone` — without it iOS gives no push at all (§3.1).
- The **static app** at `/`, and `/lib/*.js`, `/styles.css`, `/sw.js`, `/icons/*` beside it. `sw.js`
  must be served from the root so its scope is the whole origin.
- **No directory listing, no file paths, no id the Mac did not mint** (§2.5 item 5).
