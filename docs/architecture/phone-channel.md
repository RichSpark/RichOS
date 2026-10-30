# The phone channel — the contract between the Mac and the phone app

**Status:** the contract for slice A of `richos-hq/docs/plans/richos-phone-client-2026-09-18.md`.
**Mac half:** `richos/app/src-tauri/src/phone/`. **Phone half:** `richos/web/web-app/`.

> ## RECONCILED 2026-09-18, AND THE PHONE WON
>
> This file was written first, before either half was built, so the two would be built against one
> document. The phone was then finished first and **landed** (`dfa7ed27`) against its own stub
> (a contract stub beside it, removed with the page on 2026-10-30), which had to invent several things this file had settled
> differently.
>
> **Every one of those differences is resolved in the phone's favor and this file now describes
> what `richos/web/web-app/lib/api.js` actually does.** Nothing under `richos/web/web-app/` was
> changed. The reason is not seniority: the phone is a shipped, tested artifact with 1,200 lines
> of tests behind it and the Mac's Rust was not yet reachable from anywhere — so the Mac was the
> cheaper side to move and a contract that describes something other than the code is worse than
> no contract.
>
> **What changed from the first version of this file, so the diff is readable rather than
> archaeological:**
>
> | | First version | Now |
> |---|---|---|
> | post a message | `POST /api/message`, a typed `kind` envelope | `POST /api/messages` |
> | the credential | four `X-RichOS-*` headers over a server **time** plus a client nonce | one `Authorization: RichOS-Device` header over a server **challenge** |
> | the stream's credential | `device`/`time`/`nonce`/`sig` query parameters, a ten-minute "ticket" | one `auth=` parameter, the same header value |
> | cursors | `<boot>-<seq>` strings | monotone integers |
> | the stream | `snapshot` + `live` carrying `rich://*` names verbatim | `hello`/`message`/`delta`/`state`/`heartbeat` carrying message rows |
> | pairing | base64url raw or SPKI key, camelCase answer | `public_key_jwk`, snake_case answer |
> | push registration | a `kind:"push"` envelope on the message route | an authenticated `POST /api/pair` |
> | backfill | not specified | `GET /api/events?before=&limit=` |
> | the six words | derived on both ends from a list each held | the Mac sends the **hash**; the phone derives the words |
> | DEVIATION 1 and 2 | two declared deviations from plan §2.7 | **deleted** — one freshness rule now covers the stream too |

Every section below cites the plan section it implements and names the file on the phone side that
is the authority for the bytes.

---

## 1. The two listeners and when they exist

| | Port | Scheme | Serves | Credential |
|---|---|---|---|---|
| The channel | **8443** | HTTPS, our leaf | the phone app plus four API routes | the paired device's signature |
| The trust endpoint | **8444** | plain HTTP | exactly one file, the `.mobileconfig` | none — it cannot have one |

**The origin is `https://<LocalHostName>.local:8443` and it never changes** (plan §2.1, §10.7). The
name comes from `scutil --get LocalHostName`; the port is part of the origin, so it is pinned. A
service worker, a push subscription, a cache and a Home Screen icon are all scoped to that origin,
so moving it orphans the installed app.

**Neither listener exists until the CEO pairs a phone and both stop when he unpairs the last one**
(plan §2.5 item 1). Off is the default and off means no socket rather than a closed door.
`Listener::stop` **joins** its thread, so when it returns the port is free — not soon.

**One qualification the plan implies rather than states and the phone half depends on it:** the
phone gets the app *from* the Mac, so the listener must also be up during pairing, before any
device is paired. The rule as implemented is therefore: **the listener runs while a device is
paired OR while a 60-second pairing window is open** and stops otherwise.

**It binds the LAN interfaces only, never `0.0.0.0`** (plan §2.5 item 2). Loopback is included so a
developer can `curl` the Mac's own listener; every other address is one the Mac was told about by
enumerating its own interfaces. An empty address list and a wildcard address are both refused by
name.

---

## 2. The trust endpoint — `http://<name>.local:8444/ca`

One path. `GET /ca` returns the root certificate wrapped in a configuration profile:

```
200 OK
Content-Type: application/x-apple-aspen-config
Content-Disposition: attachment; filename="richos-local-ca.mobileconfig"
```

**Every other path and every other method returns a flat 404 with an empty body.** The profile is
unsigned and iOS will show "Unverified" in red; the Mac's own settings screen says so before he
starts, in his words and the profile's own description repeats it (plan §2.1).

---

## 3. The static app

`GET /` and `GET /<asset>` on 8443 serve the files in `richos/web/web-app/`, bundled into the
application as a resource. **No credential is required and none can be:** the phone loads the app
in order to pair, so a lock here would be a lock whose key is behind the lock.

That is safe because of what those files are: **the shell only, with no conversation in it.**
Nothing served here is derived from the ledger, from a thread or from anything the CEO said.
Everything with his words in it is behind §4.

Rules: no directory listing; no path containing `..`, `.`, an empty segment or a null byte; no
symbolic link followed out of the directory (checked by canonicalizing, not by inspecting the
string); anything not present is a flat 404. `index.html` is served for `/` only, **never as a
catch-all** — a phone that asks for a file we do not have must be told so, not handed an app shell
that then fails in a way nobody can read.

---

## 4. The credential

Authority: `richos/web/web-app/lib/api.js`.

At pairing the phone generates a **non-extractable** WebCrypto ECDSA P-256 key and registers the
public half (plan §2.7). Plan §2.7's cheaper alternative — a long random bearer token — is **not**
taken: slice C did not run over, so nothing extractable is stored on the phone.

### 4.1 The header

```
Authorization: RichOS-Device <device_id>.<challenge>.<base64url signature>
```

Parsed by splitting from the **right** twice, so a device id or challenge that ever contained a dot
cannot silently produce the wrong triple.

### 4.2 What is signed

Exactly this string, UTF-8, with `\n` between the parts and no trailing newline:

```
<challenge>\n<METHOD>\n<path-with-query>\n<lowercase hex SHA-256 of the body>
```

**The last field is the EMPTY STRING when there is no body**, not the SHA-256 of zero bytes. That
is what the phone sends and a Mac that hashed nothing-as-bytes would refuse every `GET` the phone
ever makes.

`path-with-query` is the path plus every query parameter **except `auth`** — the one parameter that
is the signature. Derived by that rule on both sides rather than agreed by convention.

### 4.3 The challenge

**Server-issued and never client-generated.** The Mac puts a fresh one in `X-RichOS-Challenge` on
**every** response including a 404 and a 429, in the stream's `hello` and in the pairing answer.
The phone replays the most recent one it holds. That is why there is no challenge route and no
fifth route.

**A challenge is NOT single-use and this is a property rather than an oversight.** The phone cannot
know a newer challenge before the response that carries it arrives, so two concurrent requests
share one; and `EventSource` reconnects to *the identical URL*, so the browser itself presents the
stream's credential again on every reconnection. A challenge is therefore valid for **600000 ms**
(ten minutes) from issue and only the most recent **16** are live at once.

What that trades away, stated so nobody has to find it: **a captured request is replayable for at
most ten minutes, by somebody already on his home network and it can do nothing that a replay
could not** — his own words again, on his own thread. What it does not claim is one-shot freshness.

### 4.4 What the Mac checks, in order

1. the rate limit — **first**, so a shouting caller cannot make the Mac do elliptic-curve work;
2. the device id names the paired device;
3. the challenge is one the Mac issued and is still live;
4. the signature verifies over §4.2's exact bytes.

**Every failure is a flat `404 Not Found` with an empty body — with exactly one exception.**

### 4.5 The one deliberate non-404

A device id that **was** paired and has been forgotten gets `403` with `{"revoked":true}`. The
phone treats that as final: it clears its credential and says so and never retries.

That exception is about him, not about security. A 404 there would be a phone that hammers his Mac
forever and a CEO who is never told why his phone went quiet. A device id that was *never* paired
still gets the flat 404 — there is a test for both halves in one place.

---

## 5. The four routes

All four live under `/api/` on 8443. **The maximum request body is 65536 bytes (64 KiB)**, refused
at the edge *before* the body is read into memory. **Rate limits:** 60 requests per rolling minute
per device and at most 4 concurrent event streams (plan §2.5 item 6). Over either, the answer is
`429` with `Retry-After: 60`.

### 5.1 `POST /api/pair` — pairing and the device record

**Two requests on one route, told apart by the presence of a `code` and never by a flag.** A fifth
route would have broken plan §2.5's ceiling of four.

**Pairing** (no signature — this is the request that establishes the credential). Open for **60
seconds** from the moment the CEO opens the QR screen, one shot: the window is spent even on a
wrong code, so a guessing caller gets one attempt rather than sixty seconds of them.

```json
{ "code": "K7QF2M9X", "public_key_jwk": { "kty":"EC","crv":"P-256","x":"…","y":"…" }, "device_name": "iPhone" }
```

Response `200`:

```json
{
  "device_id": "dev_9f21c0a4",
  "ca_fingerprint_sha256": "3D:9C:…:A1",
  "vapid_public_key": "<base64url of the 65-byte uncompressed P-256 point>",
  "challenge": "<the first challenge>",
  "api_base": "https://mm1.local:8443",
  "thread_id": "thr_5c1e",
  "thread_title": "the proposal",
  "threads": [ { "id": "thr_5c1e", "title": "the proposal" } ]
}
```

**The Mac sends the HASH and the phone renders the six words itself** (`web/web-app/lib/fingerprint.js`),
and the ordering is the point: *"if the Mac sent pretty words, a Mac that wanted to could send
words that do not belong to the certificate it is actually serving."* The Mac computes the same six
words only for its **own** screen, from `web/web-app/lib/wordlist.js` — the same 256-word list, in the
same order, cross-checked by a Rust test that reads the phone's file off disk. The derivation is
the first six bytes of the SHA-256 of the root's DER, one word per byte: **48 bits**, compared by a
person. Ample for what it defends, which is not a brute-force search but somebody getting the CEO
to nod at a different Mac inside a sixty-second window.

**One device in v1.** Pairing a second while one is paired is refused; "Forget this phone" at the
Mac frees the slot and closes the listener.

**The device record**, authenticated, no `code` — this is where the push subscription goes:

```json
{ "device_id": "dev_…", "push_transport": "web-push",
  "push": { "endpoint": "https://…push.apple.com/…", "keys": { "p256dh": "…", "auth": "…" } } }
→ 200 { "ok": true }
```

A subscription whose host is not `*.push.apple.com` is **never stored**, so it can never be dialed
(plan §2.6).

### 5.2 `POST /api/messages` — post a message

```json
{ "client_id": "01J8…", "thread_id": "thr_5c1e", "kind": "text",
  "text": "where are we on the proposal?", "sent_at": "2026-09-18T13:00:00.000Z" }
```

Response `200`:

```json
{ "message_id": "msg_…", "cursor": 12, "thread_id": "thr_5c1e",
  "accepted_at": "2026-09-18T13:00:00.123Z", "duplicate": false }
```

The words are written to the intake log and `fsync`ed before the Mac answers, then handed to the
spine (plan §4.2 i, ii). **The answer comes back in the time one `fsync` takes, never after the
turn.**

**Idempotent on `client_id`.** A repeat returns the original answer with `"duplicate": true` — never
a second message. That is what makes the phone's own send queue safe to retry after a failure it
could not classify.

**A voice note** arrives as `POST /api/messages?client_id=…&kind=voice&codec=wav16k&sample_rate=16000&seconds=<n>`
with an `audio/wav` body. The route and its parameters are honored and **slice B is not built**, so
the answer is `503` with *"Voice notes are not switched on yet. Your recording is still on your
phone."* — a sentence he can read, rather than a note accepted and never transcribed.

**When the Mac cannot write his words down** the answer is `503` with *"Your Mac could not save that
message. Nothing was lost on your phone — try again."* Not a 404: a silent 404 there looks exactly
like "you are not paired" and he would re-pair instead of learning his Mac has a problem.

### 5.3 `GET /api/events` — the stream and the backfill

The credential is in the query for this route and only this route, because `EventSource` cannot set
a header:

```
GET /api/events?thread_id=<id>&since=<cursor>&auth=<the Authorization header value, percent-encoded>
```

**`before=` present means the backfill instead** — ordinary JSON, no stream:

```
GET /api/events?thread_id=<id>&before=<cursor>&limit=<n>
→ 200 { "messages": [ …rows… ], "more": true|false }
```

`limit` defaults to 40 and is clamped to 200, so one request cannot ask for the whole conversation.
Chunked loading behind infinite scroll is explicitly fine; **page numbers and pagination controls
are never built** (standing rule).

**The live stream** sends five event types and the phone subscribes to exactly these five:

| event | data | what it is |
|---|---|---|
| `hello` | `{challenge, api_base, thread_id, latest_cursor, threads:[{id,title}], vapid_public_key, capabilities:[…], build, messages:[…]}` | the opening. Carries the rows too, so the first paint needs no second request |
| `message` | a full message row | a row to merge |
| `delta` | `{message_id, cursor, text}` | text appended to a reply that is still arriving |
| `state` | `{message_id, state}` | one row's state changed |
| `heartbeat` | `{}` | reserved; the Mac currently keeps the stream warm with an SSE **comment** (`: keep-alive <ms>` every 15 s), which costs the phone nothing to ignore |

A message row:

```json
{"id":"…","thread_id":"…","cursor":12,"role":"ceo"|"rich","kind":"text"|"voice","text":"…",
 "created_at":"2026-09-18T13:00:00.000Z","client_id":null,"has_audio":false,
 "from_microphone":false,"state":"…","complete":true}
```

**`capabilities` is what this Mac can be asked for and the phone assumes nothing it is not told.**
A list of names; `["text"]` in this build, because voice transcription is slice B and the route
answers every voice note `503`. **The phone renders a control only where the capability behind it
is named here** — it shipped a "Hold to record" button as one of its two biggest controls against a
Mac that could not take one and a control that cannot work is worse than an absent one.

It is **default-deny in both directions**. An absent, empty or malformed list offers nothing, which
is the true answer for every build that predates this key: not one of them could take a voice note.
And the Mac's list is checked against its own routes rather than maintained beside them
(`routes.rs`, `voice_is_offered_exactly_when_the_route_would_take_one`), so advertising something
that does not work and building something that is never advertised are both test failures.

`capabilities` rides **every** `hello`, not the pairing response, because the phone outlives the
build it paired with: he updates the Mac and the app on his phone is the same app, holding whatever
it was last told. The phone **replaces** its answer from each frame and never merges — a capability
that was true once is not evidence about the Mac answering now.

**`build` is which RichOS is answering**, as the version string (`1.2.0`), from
`app/src-tauri/Cargo.toml` via `CARGO_PKG_VERSION` and from nowhere else. It is diagnostic: nothing
on the phone branches on it today. The phone is never shown it — plan §3's *"nothing he reads is an
identifier"* covers a build number as squarely as a hash.

**The cursor is issued by the Mac and is the only ordering** (plan §4.2 vi). It is a **monotone
integer: the row's 1-based position among the message rows of the CEO-gated projection.** That is a
deterministic function of the append-only ledger, so a reload computes the same numbers a stream
sent — which is what makes `since=` and `before=` agree with each other across a restart. Live rows
continue the count and every `hello` re-seeds it from the projection, so a drift cannot survive one
reconnection.

**Its honest limit:** the cursor is a position, not an identity. `id` is the identity.

**Reconnection.** `since=` from the URL or `Last-Event-ID` when the browser reconnected by itself
and the phone did not rebuild the URL. If the Mac's ring buffer (the most recent **512** frames)
still covers what follows, it replays exactly the tail. Otherwise it sends a fresh `hello` —
**never a partial tail**, because a gap the phone cannot see is the one failure this design refuses.
A phone that falls far enough behind for frames to be dropped gets `: re-snapshot <n>` and the
stream closes, rather than a silently thinned one.

### 5.4 `GET /api/audio/<message_id>` — one blob, by an id the Mac minted

```
→ 200  Content-Type: audio/wav   (the bytes)
→ 404  for every id the Mac did not mint, in every case, with an empty body
```

The id indexes a table the Mac wrote. **There is no path here, no file name and nothing derived
from the request**, so there is nothing to traverse — plan §2.5 item 5's *"no id it did not mint"*.

---

## 6. Push

Mac → Apple → phone, entirely outbound from the Mac and **`*.push.apple.com` is the only host the
Mac ever dials for this feature** (plan §2.6). The check is on the host parsed out of the URL, not
on the string and it happens before a connection is opened.

- **RFC 8291** `aes128gcm`, AES-**128**-GCM (not 256 — the plan corrects its own earlier text).
- **RFC 8292** VAPID, `Authorization: vapid t=<ES256 JWT>, k=<base64url public point>`, where the
  JWT's `aud` is the **origin** of the endpoint and never the whole URL.
- `sub` is `https://github.com/RichSpark/RichOS` (the repository's address since the GitHub move,
  2026-10-28).
- `TTL: 120`; `Urgency: high` for an interrupt, `normal` for a digest.

Decrypted payload:

```json
{"notification":{"title":"Rich","body":"…Rich's actual words…","navigate":"/#thread=<id>&at=<message_id>"},
 "message":{ …a message row… },
 "api_base":"https://mm1.local:8443",
 "tier":"interrupt_now"|"digest",
 "truncated":false}
```

**The attention tiers already exist in the ledger and the phone obeys them rather than inventing a
policy** (plan §3.4): `interrupt_now` pushes immediately, `digest` batches, `silent` never pushes.

**`truncated` is honest and the arithmetic is shown.** The `aes128gcm` header is fixed at
16 + 4 + 1 + 65 = **86** bytes, so against the 4096-byte APNs ceiling the plaintext ceiling is
4096 − 86 − 1 (the record delimiter) − 16 (the GCM tag) = **3993 bytes**, JSON envelope included.
When Rich's reply does not fit, the push carries its opening and `"truncated": true` and the phone
says the rest is waiting on the Mac. It never shows a cut-off answer as a whole one.

---

## 7. What the phone can never be sent

**The outbound path reads one stream and no other:** the spine's `forward_live` chokepoint, which
hands an observer **only `Visibility::Ceo` items** (`app/src-tauri/src/events.rs`,
`app/crates/richos-core/src/live.rs`). The phone emitter sits beside the desktop's
`TauriLiveEmitter` and inherits that gate by construction — the bytes of a tool call, a file path or
an internal turn never reach it.

The `hello` and the backfill have the same property for the same reason: `Timeline` deliberately
does not implement `Serialize` and `view(ViewMode::Ceo)` is the only way to obtain a payload.

**And machinery is dropped on top of that.** A gated CEO timeline still contains `work_duration`,
`activity` and `worker_activity` rows; plan §6 says the phone has none of them, so the translation
keeps `user_message` and `rich_message` and nothing else.

**The translation layer (`src/phone/rows.rs`) has no `Ledger`, no `Timeline` and no `Spine` in its
imports.** It cannot reach an ungated byte, rather than choosing not to.

---

## 8. The reachability seam — the address is DATA, never the origin

Plan §10.7 and it is what keeps the phone app from being rewritten twice.

- **The origin is fixed forever** at `https://<name>.local:8443`. Service worker, push subscription,
  cache and icon are scoped to it.
- **The API base is one stored value the phone reads before every request.** It arrives in the pair
  response, in every `hello` and in every push payload.
- **The Mac holds a ranked list of providers** and offers the best it currently has. **In this slice
  there is exactly one — `home`, the origin itself.** Later providers (a router door, an IPv6
  address, a `ts.net` name) are new entries in that list and change nothing on the phone.
- **When no provider can be offered the Mac says so rather than guessing** and the phone shows its
  queued-send state with a reason he can read. The seam reports three answers and not two —
  unchanged, moved and **lost** — because "the address we had is gone" is the change the phone most
  needs to hear.

---

## 9. Errors, in one table

| Situation | Status | Body |
|---|---|---|
| unpaired, wrong signature, unknown or stale challenge, unknown route, unminted audio id, closed pairing window, wrong pairing code, malformed JSON, missing field | **404** | empty |
| a device this Mac has forgotten | **403** | `{"revoked":true}` |
| body over 64 KiB | 413 | empty |
| over the rate limit | 429 | empty, `Retry-After: 60` |
| the Mac could not save the message — **try again** | 503 | `{"accepted":false,"reason":"…"}` |
| the Mac will not take that KIND of message in this build — a voice note today | 503 | `{"accepted":false,"retry":false,"reason":"…"}` |
| accepted | 200 | as above |

**404 for nearly everything is deliberate.** A caller that is not the paired phone gets one answer
to every question, so it cannot map the surface by the shape of the refusals.

**`retry` is the only thing separating the two 503s and for one day there was nothing.** They were
one row here: same status, same body shape, same `accepted: false`. One of them MUST be retried —
the Mac failed to write his words down and the phone is holding the only copy — and the other must
never be, because the answer will be identical every time this build is asked. A phone that guesses
either strands a message he wrote or hammers a Mac that has already answered, so **the Mac says
which it is and the phone reads it, never the other way round.**

The rules, exactly as `api.js` implements them:

- **`"retry": false` and only that**, the JSON boolean. A missing key, a body that is not JSON, the
  string `"false"` and the number `0` are all read as retryable. That default costs one wasted
  request; the other default costs a message he has to type again.
- **It is not tied to 503.** Any status the Mac refuses on may carry it and a refusal that names
  itself final is final whatever the number above it says.
- **It is about the MESSAGE, not about the phone.** This is the distinction the send queue turns on.
  A final refusal of one message leaves everything queued behind it free to go and the queue carries
  on past it. A refusal about the PHONE — `403 {"revoked":true}`, the flat `404` — applies
  identically to everything still queued, so the queue stops rather than walking the rest of his
  messages into the same answer.
- **`reason` is shown to him**, so it is a sentence about his message and never an internal name.

---

## 10. What this contract deliberately does not contain

Voice transcription (slice B — the route answers 503 and says so, `capabilities` leaves it out and
the phone therefore shows no control for it), thread creation or renaming,
more than one paired device, more than one company, any machinery or drill-down view, settings,
permission approvals and any route that takes a file path. Plan §6 is the full list and none of it
comes back through this door.
