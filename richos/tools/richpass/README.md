# RichPass

Open `RichPass.html` directly in a browser. No server, build, installation or internet connection is needed. RichOS optionally protects a separately saved copy. Use the [versioned RichPass releases in this repository](https://github.com/RichSpark/RichOS/releases?q=richpass-v) to download the standalone file.

The HTML is the first-party source, executable and distributable. Start with the opening source comment for the design rationale, data flow, trust boundaries and a test vector to run. The numbered sections then take you through markup, styles, input rules, derivation and UI state. The two explicitly marked vendor scripts are the compiled exception: hash-wasm 4.12.0 supplies Argon2id plus its embedded WebAssembly and tldts 7.0.12 supplies the pinned domain parser plus suffix data. Their original sources, versions, licenses and file hashes are under `vendor/`. No CDN or runtime dependency fetch occurs.

The fixed profile is Argon2id v19, 128 MiB, three passes, one lane and 32 raw bytes. The output is 20 ASCII letters, digits and `@`, with at least one digit and uppercase letter. It has at most two consecutive digits. The final steps repair a missing class and replace any base64url hyphen or underscore with a lowercase letter. Compliant results stay intact. The master receives JavaScript `trim()` only. Labels always use NFKC, outer trim, whitespace to `-`, lowercase, restriction to `[a-z0-9._@-]`, repeated hyphen collapse and edge hyphen trim, in that order. Empty labels become `default`. The rotation and domain rules are part of the RichPass profile. The first public profile freezes input validation, normalization, suffix data, KDF parameters and the resulting formatter.

## Master verification

First load without a saved fingerprint starts at 20% directly in the HTML and accessibility attributes, with no backwards fill transition. Later setup steps retain their forward animation. First use follows the approved round-62.1 design: one field is retyped through five steps, at 20/40/60/80/95 percent. The labels/buttons are Master passphrase/Get started, Confirm master passphrase/Next, Confirm master passphrase again/Next, Master passphrase once more/Next and Confirm master passphrase one last time!/Finish setup. These five exact raw matches establish the confirmed master. Show/Hide affects the current field. A prefix is incomplete; divergence or extra length immediately shows the approved mismatch warning, red edge and one shake unless reduced motion is requested. A full match enables the action without positive-match text. Start over clears the first entry and restarts all five. No hashing or persistent record is created after only one through four entries. Finish setup saves the real encrypted verifier; 100% completion follows actual success and shows the approved saved-fingerprint/recovery text. The user selects “OK. Let's go!” to open Generate. Master is always between Why & How and Algo Card in those footers and opens Master Passphrase Ideas, with the approved Photos, Music and Places guidance and matching top/bottom back actions. Home attention starts three seconds after load only when storage has confirmed no saved fingerprint; it stops on actual completion. Existing, unreadable or unchecked records never trigger it. Completion has no attention animation. The mock replay controls are not part of the app. Later entries must pass verification before RichPass accepts a master or enables password generation. A mistyped master stays on entry with the exact round-63.1 failure panel, selected input and Try again. Checking... reflects the real verifier call. Caps Lock appears only when the browser reports it on. Explicit Show/reveal can mark spaces and show a Unicode character count; masking or leaving entry removes those revealed DOM copies. Repeated misses update the approved third-try guidance. No fake KDF or artificial checking delay is used. On resize, an already-open failure panel uses normal block flow to avoid WebKit retaining an old grid height and clipping text. Closing restores the original animated grid entrance; the approved colors, spacing and geometry stay unchanged. Recovered tab masters are verified too. Account password derivation is unchanged.

A random 16-byte salt is created once at setup and saved with a full 32-byte Argon2id verifier. Verification uses v19, 128 MiB, three passes and one lane with a separate purpose prefix, never an account salt. The salt is reused when checking later entries. The full payload is AES-GCM encrypted under the existing non-extractable IndexedDB key with a separate authenticated context and fresh IV. Only its encrypted envelope is written to localStorage under `richpass:verifier`. The verifier is never the account-password seed. Setup and later verification work offline without another password or a native service.

Missing, inaccessible, invalid or undecryptable verification starts fresh five-entry setup. For existing accounts, register the same original master to recover the same passwords. Setup never resets website passwords. If verification cannot be saved/read back, generation stays blocked with a retry message. Only confirmed fresh setup may repair an invalid encryption key; ordinary recovery never replaces it. Forget keeps verification but clears the active master and tab session. Other tabs keep their own tab sessions and share the confirmed master. Changed verification invalidates stale entry/output authority using storage notifications plus checks before generation, publication and Copy. Browser data clearing or another browser/profile/file location/computer can require fresh setup.

`tests/verification.cjs` exercises enrollment, mismatches, encrypted record formats, saved-salt reuse, independent verifier bytes, fresh setup, blocked persistence, recovered-master checks and cross-tab invalidation. Its public vector is independently computed with argon2-cffi 25.1.0 / argon2-cffi-bindings 26.1.0 and is also documented in the HTML source.

## Changing the master fingerprint

Change master fingerprint is available when a readable fingerprint exists. Both exact warning dialogs start with Cancel focused. Cancel, Escape, backdrop press and navigation dismiss them without a write. After both affirmative actions, the same five-entry wizard opens. The current verifier, master and encrypted tab record remain authoritative until Finish setup successfully writes/read-backs the replacement. Keep my current master passphrase cancels pending work and returns to Master. Browser Back to the earlier entry/Generate state cancels the draft too, so the first Back remains meaningful. A view-only change-fingerprint hash distinguishes that draft route without putting secrets in URLs.

The draft exists only in RAM. Reload/exit abandons it and uses normal recovery of the saved fingerprint/session. Cross-tab changes and failed/ambiguous storage use the existing recovery checks, never a silent backup overwrite. Finish uses the existing salted Argon2id verifier/encryption/key, validates the expected current envelope and keeps the temporary save guard. A successful replacement updates tab retention for the newly confirmed master. Different masters produce different account passwords; re-enrolling the same master leaves account outputs unchanged. Neither warnings nor opening/cancelling the wizard changes settings, account salts or stored records. Why & How wording remains as requested for its later separate copy revision; current change-flow details are in these source comments and this README.

## Reference vectors and layout evidence

RichPass was adopted from [PassBooster](https://github.com/WebDevBooster/passbooster).
RichPass is part of RichOS and is licensed under the GNU Affero General Public
License v3.0 and no later version (AGPL-3.0-only): the [LICENSE](../../../LICENSE)
at the root of this repository. The RichPass name and logo/icon artwork are
RichOS brand material and are not licensed under the AGPL; see
[BRAND-ASSETS.md](../../../docs/legal/BRAND-ASSETS.md). The source provenance
and the licenses of the vendored third-party code are recorded in the HTML
and in `vendor/`.
The RichPass source and vectors define the password profile from here.

`tests/formatter.cjs` checks the three embedded raw-byte formatter vectors and
the output rules on seeded and synthetic inputs. A fixed corpus digest
guards the exact formatter outputs.

RichPass.html is the sole maintained product page. The alternate page previously used for styling is retired and is not read, compared against or updated. The immutable round-62.1 setup mock is captured as `tests/RichPass.setup-reference.html` (SHA-256 `63f06796919bcf54007f978a1b7a85e24907560cfe824ad98c8167822c25236b`). Canonical 1.1.0 is captured as `tests/RichPass.style-reference.html` (SHA-256 `ae0ded1256449b06691fb5d90593f6037fbd7f4da771f46a1f4c1804d27b5f09`) for unchanged account controls/Help/Card. Round-63.1 is frozen as `tests/RichPass.states-reference.html` (SHA-256 `4c0de31d128d5fc172334ad501a0533736c616086b2fc37482be2e88f5c3f050`) for completion, wrong master and the new Generate heading/footer. Its mock controls and fake behavior never ship. Product CSS/theme match this design, excluding only mock-dock CSS. These are test fixtures, not additional product pages. Embedded fonts, title marks, speckle and ordinary controls stay within the approved visual system. `tests/setup-layout.cjs` compares the setup with the approved mock in both themes, through every step and mismatch; `tests/layout.cjs` preserves unchanged controls and Help/Card. `tests/states.cjs` compares all three new states, space-marked entry, Caps Lock and third-try feedback across the existing 18 viewport boundaries and both themes, with real verification and generation. Both use actual local files in Chromium/WebKit. Setup bounds are checked at every responsive boundary and input/Show/action bounds are checked through all five steps at 200% zoom. Persistent-profile tests use the configured/system temp directory, with no machine-specific path requirement.

`tests/vectors.json` and `tests/vectors-webkit.json` record RichPass's
effective inputs, UTF-8 salts, raw Argon2id bytes and final passwords for
Chromium and WebKit. The raw bytes are independently checked with Argon2.
The three public account examples and three formatter examples are
embedded in `RichPass.html`.

The browser's URL parser participates in domain normalization. For example,
the free-text input `my bank` can produce different effective domains in
Chromium and WebKit. RichPass shows the effective value before derivation.
Use a website domain for a website account.

## Verification

Run `python3 scripts/verify-release.py` here for the checked-in byte/provenance gate. Run `node tests/standalone.cjs` using the RichOS Playwright harness and its shared worker admission. `RICHPASS_BROWSER=webkit` selects WebKit. WebKit's offline emulation refuses file URLs even for a trivial document, so that harness blocks HTTP(S) instead and records the limitation. See the platform scope below.

For independent verification, download `RichPass.html`, `RichPass.html.sha256`
and `RichPass.html.minisig` together. Use the RichOS distribution public key
from the trusted source checkout. From the download folder run the following
commands with Minisign installed.

```sh
shasum -a 256 -c RichPass.html.sha256
minisign -Vm RichPass.html -p /path/to/richos-updater.pub
```

From the source folder, `python3 scripts/verify-release.py --production`
also verifies the canonical file, manifest, compiled updater trust anchor
and vendored inputs. The RichOS watcher uses the approved SHA-256 built into
the app. Release assets are versioned. Verify downloaded bytes before trusting
them.

The first desktop integration and watcher are for macOS. Native Safari, Chrome
and Firefox were exercised with this standalone runtime on macOS. Windows browser and
watcher qualification is separate. Use Settings > RichPass to save a bundled
verified copy without internet access.

## Limitations

The master is retained by default as an authenticated AES-GCM-256 ciphertext record in this tab's `sessionStorage` under `richpass:master`. Each save uses a fresh random 96-bit IV. A non-extractable browser-managed key in IndexedDB restores the session through reloads. Account fields and generated passwords stay page-local. Forget cancels pending saves/recovery, removes this tab's ciphertext and clears the live master. The shared random key remains so other tabs keep working. While an encrypted save is pending, a temporary `beforeunload` listener requests the browser's leave confirmation for reload, navigation or closing. It is removed after success, failure/timeout, Forget or actual page exit. The browser controls/sometimes suppresses that prompt and the user can override it. It cannot protect an unfinished save from crashes, force-quitting or every tab discard. Reloading before saving finishes can still require re-entry if departure proceeds. Unreadable tab storage is reported as unavailable; a restore failure is reported only after a saved record was actually read. A failed removal warns that an earlier session may remain only when that record was observed. Denied or unavailable storage keeps generation usable in memory without implying that a nonexistent saved session was lost. If storage access stops working mid-session, the warning says: “Your master remains usable on this page. RichPass could not access tab storage or confirm that it was cleared.” This reports uncertainty without adding persistent state or claiming that old data was erased. A corrupt record still produces an honest recovery warning. This session-only fallback requires intact master verification. If verification itself is unavailable, fresh setup must save successfully before generation. There is no plaintext, raw-verifier or exported-key fallback.

Safari 18.6 on macOS 15.7.7 was verified to depend on its original login Keychain for persisted Web Crypto keys. Use Safari on a supported Mac for that OS-backed boundary. This is not an attestation for every Safari/macOS version or locked/unavailable Keychains. The recommendation is hidden only for a Mac/Safari identity reporting exactly zero touch points. Touch-capable Mac-style browsers keep it visible, as do browsers with unknown touch support. This includes desktop-mode iPad Safari. This is an advisory heuristic, never an OS-key attestation or an encryption/support switch. Chrome uses the same encrypted-session flow, with a non-blocking Safari recommendation. Native Chrome 156 verification demonstrated that a copied profile could expose the key without a Keychain call. Other browsers must not be described as having verified OS-backed protection. Encryption protects a record-only leak; it does not stop code with the browser origin's authority from asking the browser to decrypt. The RichOS watcher checks file integrity but cannot secure an already running compromised browser.

The Master page explains three ways to construct a memorable master and offers a confirmed fingerprint change. Generate uses the master plus the account inputs shown on the page. A readable fingerprint permits verification of a typed or recovered master. Missing or unreadable verification starts fresh setup. The opening source comment explains these choices and the boundaries of encrypted tab continuity. The manifest identifies the approved artifact.

The browser controls tab-session restoration and may recover session data when reopening tabs or restoring a browser session. Use Forget to clear the master explicitly. A separately opened tab starts a separate session, although browsers may copy session storage when duplicating a tab or opening one through an opener. Readable source and verified bytes do not prove absence of bugs. The HTML makes no automatic network requests. Browser extensions, browser password-save behavior, swap, crash dumps and clipboard histories are outside RichPass's control. Forget cannot prove physical memory erasure. Clipboard clearing is an honest 20-second reminder, never an automatic overwrite.

The initial watcher targets macOS local filesystems. It reserves one exact `RichPass.html` location only after native consent. It cannot repair an already loaded browser page. Other copies are unprotected. Stop protection before permanently editing, moving or deleting the protected copy.

Round 64.1 is frozen as tests/RichPass.master-reference.html (SHA-256 e7f2f6f8659e91f736f78694d717c4c4172e10d3ee972e2ada1d2d898131ec45). Its Master markup/product CSS are the design authority, with only mock-dock scaffolding excluded and the explicitly requested action-label wrap added. The three methods each remain exactly two optional steps; spelling/case/separators are user choices, not new input restrictions. The public test owners cover confirmation exits, retained old records until final commit, cancellation/stale jobs, expected replacement, back/history, reduced motion and a controlled three-second attention clock.
