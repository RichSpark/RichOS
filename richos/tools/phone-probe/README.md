# The phone probe

**One page, six checks, twenty minutes on the CEO's iPhone, served by his own Mac.** It answers the
question that decides whether the RichOS phone client can be a web app: **does the microphone work
inside an *installed* iOS web app?** — and, since 2026-09-18, the question underneath it: **can a Mac
on a home network serve that app to a phone at all?**

This is step 0 of `richos-hq/docs/plans/richos-phone-client-2026-09-18.md` (§3.2). It is a probe, not
a foundation. The relay, the pairing and the real PWA are separate work, written from what this
finds. Nothing here is meant to survive into the product except the recorder's approach and the one
thing that is — `public/pcm.js` — says so at the top of the file.

**Why it needs a server at all.** A PWA cannot exist without an HTTPS origin: service workers, push
and Add to Home Screen all require a secure origin with a real certificate.

**Why that server is this Mac.** CEO ruling, 2026-09-18 (`wiki/ceo-decisions.md` §57): *"the user is
not expected to have something like Railway and I shouldn't provide that for a free and open-source
app. Instead, the RichOS app should have 'something' running on the user's device (Mac in this
case)."* So the probe hosts itself and **the hosting step is now part of what the probe tests** —
because the real phone client will be served in exactly this way.

---

## The six checks

| # | Check | What a failure means |
|---|---|---|
| 0 | **Trust** — the phone opens the Mac's HTTPS origin with no warning | If this fails, nothing else runs. It is the hosting arrangement itself. |
| 1 | **Install** — Add to Home Screen gives a standalone window | Expected to pass. If it does not, nothing else is measurable. |
| 2 | **Microphone** — a held recording captures non-silent audio | **THE GATE.** If this fails, the PWA ships as text + push and voice notes move to the native track. |
| 3 | **Persistence** — is the permission asked again next launch? | A cost, never a blocker. Either answer is fine. |
| 4 | **Push with the app closed** — a notification arrives a minute later | If this fails, a PWA is a poor fit and native moves up the queue. |
| 5 | **Wake** — tapping the notification opens the app on the right view | A cost, not a verdict. |

Each check writes a line into a results panel on the page, with one tap to copy all six.

---

## Running it

```
npm start
```

That is the whole thing. On first run it mints a local certificate authority, issues a server
certificate for this Mac's names, prints both URLs and a scannable code and starts two listeners:

| | Port | What it is |
|---|---|---|
| `https://<hostname>.local:8443` | 8443 | **the probe.** Checks 1–5 live here. |
| `http://<hostname>.local:8442` | 8442 | **the trust step.** Check 0 — the page that hands the certificate to the phone. |

**Two listeners and the second one is plain HTTP for a reason that cannot be designed away.** The
certificate the HTTPS listener presents is signed by an authority that exists only on this Mac, so
until the phone has installed and trusted that authority it *cannot* open the HTTPS origin without a
warning. The page that hands over the certificate therefore has to be served over something the phone
already trusts and on a LAN with no public name, that is HTTP. Nothing sensitive crosses it: a root
certificate is a public key and the configuration profile is a wrapper around one.

**The port is part of the origin.** `:8443` and `:9443` are two different origins with two different
service workers and two different push subscriptions, so changing the port means redoing Add to Home
Screen. 8443 is the plan's (§2.1); 443 is refused because binding it needs root.

---

## The trust step, in the CEO's hands

Open the HTTP URL on the iPhone **in Safari** — a configuration profile downloaded in another browser
will not install — or point the camera at the code the server prints in the terminal.

The page then walks him through **sixteen taps and his passcode, once, ever**: download the profile,
install it in Settings and — the step everyone misses — switch it on under **Settings → General →
About → Certificate Trust Settings**. That last step is Apple's documented requirement, not extra
caution: *"If you manually install a profile that contains a certificate payload in iOS, iPadOS and
visionOS, that certificate isn't automatically trusted for SSL"* (`support.apple.com/en-us/102390`).

The count was derived independently here and matches the plan's own count in §2.1, tap for tap.

**Two things the page tells him BEFORE he meets them**, because a surprise in a security dialog is
how a person stops halfway:

- **iOS will show "Not Signed" in red.** Not a fault and not fixable: signing a configuration
  profile needs a certificate from a CA Apple already trusts, which is the exact thing this design
  exists because we do not have. The red means *nobody vouched for this file except your own Mac* —
  which is the point.
- **If the trust switch is missing entirely, stop.** Apple's forums carry a documented iOS 18.0/18.1
  defect where a manually installed root never appears under Certificate Trust Settings
  (`developer.apple.com/forums/thread/764673`, fixed in 18.2 beta 4). That is Apple's bug and if his
  phone has it, **this whole design is blocked** — which is the single most valuable thing this probe
  could discover and the best reason it moved onto his Mac.

To undo everything: Settings → General → VPN & Device Management → **RichOS on `<MAC>`** → Remove
Profile. On the Mac, `node bin/make-local-ca.js --forget`.

---

## The certificate

Minted by `lib/x509.js` and stored under
`~/Library/Application Support/RichOS/phone-probe/tls/`, mode `0600` in a `0700` directory.
**Nothing is ever written into this repository** — a committed private key in a published repository
is a live credential and this project's own write-time scanner would refuse it.

- **root** — `RichOS on <MAC>`, ECDSA P-256, ten years, `CA:TRUE, pathlen:0`, `keyCertSign` +
  `cRLSign`. **Generated once and never again** unless it is deleted on purpose: trusting it on the
  iPhone costs sixteen taps by hand and silently regenerating it would present as "HTTPS stopped
  working for no reason".
- **leaf** — `<hostname>.local` plus the LAN IPv4 address in `subjectAltName`, `serverAuth` (critical),
  `digitalSignature`, 397 days. **Reissued freely** — a new DHCP lease, a different Wi-Fi or a renamed
  Mac all change the names it must cover and reissuing from the *same* root means the phone needs no
  second trust step.

Apple's requirements for a TLS server certificate (`support.apple.com/en-us/103769`) are what shape
that list and each is a way to produce a certificate that works in `curl` and fails on the phone: the
name must be in `subjectAltName` (iOS ignores the common name), `extendedKeyUsage` must contain
`serverAuth`, the signature must be SHA-2 and an EC key must be P-256 or better.

### Why Node and not `openssl`

**`lib/der.js` assembles the certificate body as DER by hand and `crypto.sign` signs it.** Node has no
certificate-*issuing* API — `crypto.X509Certificate` parses, it does not sign — but a certificate is
DER plus a signature and `crypto.sign` signs arbitrary bytes, so the standard library is enough.

The plan's §2.2 states the opposite and prescribes `/usr/bin/openssl`. That correction is raised as
`esc-20260918T110419Z-a27e1115` with a reproduction; **everything else in §2.2 was adopted** — the
`mm1.local` origin, port 8443, `richos.local` deferred, `pathlen:0`, the critical `serverAuth`. Why
this implementation stayed:

1. **It is verified from outside Node, by the verifier whose opinion the phone shares.** `openssl
   verify` builds the chain, and `security verify-cert -p ssl -s <host>` — Apple's own trust
   evaluator — returns *certificate verification successful*, with negative controls for the wrong
   host name and for a missing anchor.
2. **No PATH ambiguity.** On this Mac `openssl` resolves to a Homebrew **OpenSSL 3.6.4** while
   `/usr/bin/openssl` is **LibreSSL 3.3.6** — two implementations with different flags. A shelled
   command is correct only while every call site remembers the absolute path.
3. **No scratch files.** The shell route writes a CSR, a `.srl` and two keys into a working directory
   that then needs its own cleanup discipline. This route writes two keys and nothing else.
4. **The DER encoder is unit-testable against published byte values**, which a shell pipeline is not.

**The app itself will do neither.** When this moves into the Rust app it should mint the certificate
through the OS — Security framework — rather than shelling out to any binary, which is the plan's own
position and makes the choice here a question about a throwaway probe, not about the product.

---

## Reachability — can the phone actually reach this Mac?

Measured on this machine, 2026-09-18:

- **`mm1.local` resolves with nothing of ours running.** `scutil --get LocalHostName` → `MM1`;
  `dscacheutil -q host -a name mm1.local` → `192.168.1.249`. mDNSResponder publishes the Mac's own
  name for free, it follows the Mac across DHCP leases and Wi-Fi networks and iOS resolves `.local`
  through Bonjour by construction (RFC 6762). **Nothing has to be installed, started or permitted.**
- **The firewall is off.** `socketfilterfw --getglobalstate` → `Firewall is disabled. (State = 0)`,
  block-all disabled, stealth mode off, on macOS 15.6. **Nothing to allow and no dialog will appear.**
  If it were on, macOS would raise *"Do you want the application node to accept incoming network
  connections?"* on first bind and the answer is **Allow** — the alternative,
  `socketfilterfw --add /path/to/node --unblockapp /path/to/node`, needs admin rights and is not worth
  it for a probe.
- **What breaks it and none of these is our bug:** a **guest network** or a router with **client
  isolation** / **AP isolation** — both block device-to-device traffic outright and also stop
  multicast DNS. The phone being on **cellular** rather than the Wi-Fi. A **VPN** on the phone that
  captures all traffic. In every one of those the LAN address fallback fails too, because the problem
  is the network, not the name. The trust page prints the address form as a fallback for the narrower
  case where only multicast DNS is filtered.
- **`richos.local` is not registered and not claimed in the certificate.** Publishing an extra
  `.local` record needs `DNSServiceRegisterRecord` or a new dependency, *plus*
  `NSLocalNetworkUsageDescription` and a local-network permission prompt on macOS 15+. The Mac's own
  name needs none of it. Deferred with its price stated (plan §2.1) — and **if this probe finds the
  Mac's own name unreliable from the iPhone, that trade flips**.

---

## Push from the Mac

Unchanged and it needs no inbound anything: the sender opens an **outbound HTTPS** connection to
Apple's or Google's push service, exactly as it did from a hosted server. The subscription is held in
memory on this Mac.

**One thing the move to the Mac improved.** A push subscription is taken against ONE public key, so if
that key changes every subscription made against the old one is dead — and check 4 then fails for a
reason that has nothing to do with the phone. The hosted version had nowhere to keep a pair and
generated one per process, warning loudly. There is a state directory now, so the pair is generated
once and kept beside the certificate at `vapid.json`, mode `0600`. Restarting the server or the Mac,
no longer costs him the subscription.

**`VAPID_SUBJECT` is the public project page**, `https://github.com/RichSpark/RichOS`. RFC 8292
§2.1 allows a `mailto:` or an `https:` URL and Apple accepts either and **a personal address is never
baked into a build, a config default or a record** — a default is the one place a value ends up in all
three. The app's own push sender will use the same URL. (Lead's decision,
`esc-20260918T105634Z-85b52f90`.) The placeholder this replaced was worse than useless: Google's FCM
accepts a fabricated subject and Apple returns `BadJwtToken` for one, so check 4 would have failed on
his iPhone for a reason that has nothing to do with the phone.

---

## Railway is gone

`deploy-railway.sh` and `railway.json` are **deleted**, not kept as a reference. Two reasons and the
second is the real one:

1. The CEO's ruling makes hosted deployment the wrong shape for this product, so the script describes
   a path nobody should take.
2. **A dead deploy script in a tool directory is an invitation.** Everything it knew that is still
   true — how Railway authentication really works and why `railway up` fails with a mangled parse
   error rather than an auth error — is in femcboost's `scripts/deploy-avelor-staging.sh`, which is
   live and maintained. Keeping a second stale copy here to be found by the next person is how a
   retired path gets taken.

Its content is in this repository's history at `6b70a704` if it is ever wanted.

---

## Commands

```
npm start                     both listeners, plus the certificate if it is missing
npm test                      83 unit tests, no dependencies, no browser
npm run ca                    what is on disk, without starting anything
node bin/make-local-ca.js --qr        also write the two QR images and print one
node bin/make-local-ca.js --reissue   new server certificate, same root (after a network change)
node bin/make-local-ca.js --forget    delete everything this tool has written
node test/tls-verify.js       the server, through three independent verifiers
node test/desktop-verify.js   the real page in a real browser, over the real HTTPS origin
node test/qr-verify.js        every QR this tool renders, decoded by Apple's Vision framework
```

### Configuration

| Variable | Default | What it does |
|---|---|---|
| `PROBE_HTTPS_PORT` | `8443` | The probe's origin. Changing it invalidates an installed home-screen app. |
| `PROBE_TRUST_PORT` | `8442` | The trust step. |
| `PROBE_BIND` | `0.0.0.0` | The phone is not on this machine. |
| `PROBE_STATE_DIR` | `~/Library/Application Support/RichOS/phone-probe` | Where the certificate lives. The test harnesses point this at a temp directory so they can never touch the real root. |
| `VAPID_PUBLIC_KEY` / `VAPID_PRIVATE_KEY` | generated once, kept in the state directory | Rarely needed. The pair is created on first run and reused, so a subscription taken today still works tomorrow. |
| `VAPID_SUBJECT` | the project URL above | Override only with another URL you control. |
| `PROBE_ACCESS_CODE` | unset (open) | An unguessable code. Unset means both origins are open to anything on the Wi-Fi — fine behind a home router with no port forwarding. |
| `PROBE_BUILD_SHA` | `unset` | Printed on the page, so a stale process cannot masquerade as a fresh one. |
| `PROBE_PRINT_QR` | on | `0` to suppress the terminal QR. |

---

## Dependencies: none

`package.json` declares an empty `dependencies` block and that is a decision rather than an accident.
Four things here would normally each be a package — **web push** (RFC 8291 + RFC 8292, `lib/webpush.js`),
**X.509 issuance** (`lib/der.js`, `lib/x509.js`), **QR encoding** (`lib/qr.js`) and **PNG writing**
(`lib/png.js`) — and Node's own `crypto` and `zlib` cover all four.

1. This repository is published and the artifact is something the CEO installs on his own phone.
   Every package is a supply-chain surface on that.
2. Each of them is checked against a published value rather than against itself: web push against RFC
   8291's own test vector, the certificate against Apple's trust evaluator, the QR against ISO/IEC
   18004's Annex I worked example *and* Apple's Vision decoder.
3. The Mac side of the real phone client will do the equivalent work in Rust. Writing it out longhand
   once, in a language where it is easy to read, is the cheapest way to de-risk that.

Playwright (`test/desktop-verify.js`) and `swiftc` (`test/qr-verify.js`) are resolved from wherever
they already exist and **skip with a message** if absent, rather than pretending.

---

## What it stores

- **His voice: nothing, anywhere.** It is measured in memory on the phone and discarded. There is no
  upload path for audio in this code at all — search for one.
- **The six results: on his phone only**, in `localStorage`. "Start over" erases them.
- **The push subscription: in the server's memory**, because a push cannot be sent without it. It is
  an endpoint URL and two keys — a mailbox, not a person. No database, no disk. A restart forgets it,
  and "Start over" deletes it immediately.
- **The certificate and its keys: on this Mac**, in the state directory above and nowhere else.

---

## Notes worth keeping

**The recording asks for permission on a separate button.** iOS raises its microphone dialog over the
page. If that happened while he was holding the record button, lifting his finger to reach "Allow"
would end the recording before the microphone ever opened — and a working microphone would be
reported as broken on the one check that decides the plan. Permission is therefore its own control,
and separately, a recording under 0.5 s is reported as "hold it longer", never as a failure.

**Check 0's verdict is his, not the page's.** After tapping through a "this connection is not private"
interstitial the origin is still HTTPS and `isSecureContext` is still true — so a page that inferred
trust from those two would report a pass for the exact arrangement the step exists to test. The page
shows the machine facts (the host reached, the certificate's names, the root's six-word name) and asks him
which of the two things happened.

**The service worker caches nothing and has no `fetch` handler.** A probe that serves a cached page
answers questions about the wrong build. Everything is `Cache-Control: no-store`.

**The push payload is also a valid Declarative Web Push document.** On iOS 18.4+ Safari may render
the declared notification itself without running our JavaScript; on 16.4–18.3 the service worker's
`push` handler is the only path. Both are covered, because we do not know which his phone takes.

**Web push content is encrypted end-to-end by the standard.** Apple relays ciphertext it cannot read.
This is the one axis on which the PWA is a *stronger* privacy story than a native app, whose push
payload is plaintext to Apple.

**A 410 from the push service is treated as a real event.** The subscription is forgotten and the
result says so. Plan risk 2: a silently dead channel is the failure that matters.

**The desktop verification pins one certificate and does not disable certificate checking.** Chromium
gets `--ignore-certificate-errors-spki-list=<sha256 of this key>`, which accepts exactly ours and keeps
rejecting every other bad certificate in the world. **The login keychain is never touched**, by that
harness or anything else here.
