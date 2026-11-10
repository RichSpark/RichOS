# RichPass

<details>
<summary><b>Contents</b> (click to open)</summary>

**For everyone**

- [What RichPass is](#what-richpass-is)
- [Why it is safer than a password manager](#why-it-is-safer-than-a-password-manager)
- [Get RichPass](#get-richpass)
- [First-time setup](#first-time-setup)
- [Everyday use](#everyday-use)
- [Good to know](#good-to-know)
- [Optional: let RichOS guard your copy](#optional-let-richos-guard-your-copy)
- [Why it works this way](#why-it-works-this-way)
  - [Why RichPass checks your master passphrase first](#why-richpass-checks-your-master-passphrase-first)
  - [Why a tab remembers your master passphrase](#why-a-tab-remembers-your-master-passphrase)
  - [Why changing your master passphrase is protected](#why-changing-your-master-passphrase-is-protected)
  - [Why all settings are fixed](#why-all-settings-are-fixed)
  - [Why passwords use only one special character](#why-passwords-use-only-one-special-character)
  - [Why passwords are exactly 20 characters long](#why-passwords-are-exactly-20-characters-long)

**For technical readers**

- [For technical readers](#for-technical-readers)
- [Source layout and dependencies](#source-layout-and-dependencies)
- [Derivation profile](#derivation-profile)
- [Domain parsing and reference vectors](#domain-parsing-and-reference-vectors)
- [Master verification](#master-verification)
- [Changing the master fingerprint](#changing-the-master-fingerprint)
- [Tab session storage](#tab-session-storage)
- [Browser and platform notes](#browser-and-platform-notes)
- [Verifying a download and running the checks](#verifying-a-download-and-running-the-checks)
- [Design references and layout tests](#design-references-and-layout-tests)
- [RichOS file protection](#richos-file-protection)
- [Limitations](#limitations)
- [Provenance and license](#provenance-and-license)

</details>

## What RichPass is

**Safer Than Any Password Manager.**

RichPass makes strong passwords for your accounts and it makes the exact same password again every time you need it. It never saves them. It recreates each one from scratch.

It uses the world's best tool for the job ([Argon2id](https://en.wikipedia.org/wiki/Argon2)) to always recreate the exact same unbreakable password from the same input. That input is:

- your master passphrase (the one phrase you memorize)
- the website's domain or login address
- an optional label, such as the username or email you use for that login
- a version, such as v1 (used for changing the password when needed)

Change any one of these and you get a completely different password. Even changing the version from v1 to v2 does that.

Every password is 20 characters long and made of letters, digits and `@`. Every password has at least one digit and at least one uppercase letter.

RichPass is one file, `RichPass.html`. You open it in your web browser. Everything it needs sits inside that file, so it works offline. And all the code in the file was made easy to understand.

While it looks deceptively simple, a lot of time and effort went into making it work as well as it does for practical everyday use.

## Why it is safer than a password manager

- **No password vault to steal.** A password manager keeps your passwords stored somewhere. RichPass never saves the passwords it makes and keeps no list of your accounts. It simply makes each password again when you need it.
- **No cloud account.** There is nothing to sign up for and nothing to sync.
- **It runs offline.** The file makes no connections to the internet.
- **Easy to trust and verify.** It is open source and its code is easy to read for humans and AI.

One thing stays true with any method: your master passphrase must be strong and must stay secret.

## Get RichPass

- **From the RichOS app:** open the RichPass page and choose **Download RichPass**.
- **From GitHub:** go to the [RichPass downloads](https://github.com/RichSpark/RichOS/releases?q=richpass-v1), open the newest RichPass release and download `RichPass.html`.

To open it, double-click `RichPass.html`. It opens in your web browser. There is nothing to install. It works in Safari, Chrome and Firefox. For the best protection, use Safari on a Mac.

## First-time setup

The first time you open RichPass, it asks you to type your master passphrase five times, one after another. You only do this once.

1. Type your master passphrase and choose **Get started**.
2. Type it again four more times, choosing **Next** each time and **Finish setup** at the end. If what you type stops matching your first entry, RichPass warns you right away. **Start over** lets you begin again if your first entry was wrong.
3. RichPass saves a fingerprint of your master passphrase (more on that [below](#why-richpass-checks-your-master-passphrase-first)). Choose **OK. Let's go!**

Need help picking a good master passphrase? Choose **Master** at the bottom of the page. It shows three simple ways (Photos, Music and Places) to build one that is strong and that you can piece back together if you ever forget it. We recommend at least three words and at least 15 characters in total.

**Make sure you can always remember or recover your master passphrase. RichPass cannot recover it for you.**

## Everyday use

1. Open `RichPass.html` and type your master passphrase. (If you left the tab open, RichPass is ready and you can skip this.)
2. In **Domain or URL**, type the website (for example `example.com`) or paste its login address.
3. Optionally, type the username, email or label for that account. Use the same one every time.
4. Leave **Version** at `v1` unless you are changing the password.
5. Choose **Derive password from master + above input**, then **Copy**. Paste the password into the website.

Next time, type the same things and you get the same password.

Under each field, RichPass shows exactly what it will use, for example "Will use domain: example.com". That is handy to note down, because those are the details that bring back the same password.

**For a new account,** simply paste the RichPass password when you sign up.

**To switch an existing account over to RichPass,** you change the password on the website itself. RichPass only makes the password. It cannot change it at the website for you.

1. Make the RichPass password for that website and copy it.
2. Log in to the website with your old password.
3. Open the website's change-password page. It is usually in your account or security settings.
4. Type your old password where the website asks for the current one. Paste the RichPass password as the new one and save.

From then on, you log in with the RichPass password.

**To change a password for a website,** change the version from `v1` to `v2` (then `v3` and so on). You get a brand-new password for that account. Then set it as the new password on the website's change-password page, the same way as above.

**When you are done,** you can choose **Forget master passphrase** at the bottom of the page. That clears your master passphrase from this tab. It does not delete anything else. The next time you type your master passphrase RichPass still checks it for typos.

After you copy a password, RichPass shows a 20-second reminder. It does not clear your clipboard for you, so copy something else when you are finished.

## Good to know

- **A new browser or computer needs the quick setup again.** The same goes for clearing your browser data. Type the same master passphrase as before and you get all the same passwords back. Setup never changes any of your passwords at websites.
- **If RichPass says your passphrase doesn't match,** it is most likely a typo. RichPass stops there so it can't make wrong passwords. You can choose **Show what you typed, spaces marked** to check it. RichPass also tells you when Caps Lock is on.
- **Some websites do not accept RichPass passwords.** RichPass's rules are fixed, so a website with unusual password rules may refuse them.
- **The Algo Card** (at the bottom of the page) is a printable card with RichPass's fixed rules. Keep it separate from your master passphrase.

## Optional: let RichOS guard your copy

If you use the RichOS app, it can watch your copy of RichPass while RichOS is open, to protect it against tampering. If anything changes, renames or removes it, RichOS puts back the exact original. Turn it on with **Enable protection** on the RichPass page in RichOS.

After RichOS repairs your copy, close any open RichPass tabs and open the file again, because a tab that is already open keeps running what it loaded.

## Why it works this way

### Why RichPass checks your master passphrase first

Your master passphrase is part of the input used to make a password. So a typo in your master would completely change the password.

For an existing account, that would mean a failed login. And for a new account, it would mean a catastrophic screw-up: registering a password you cannot reproduce later.

Showing the master while you type doesn't eliminate the possibility of a typo, because long phrases are hard to proofread and are hidden by default for privacy.

So, to completely eliminate the possibility of a typo, RichPass does this. During the one-time setup, you re-type your master passphrase 5 times. If all 5 entries are identical, RichPass assumes there aren't any typos. Then it saves a fingerprint (a one-way scrambled summary) of that input. This lets RichPass check your future entries for typos without knowing the actual passphrase.

Only if the fingerprint of what you type matches the saved one will RichPass let you make a password.

The fingerprint is only a typo check. It cannot tell anyone your passphrase and it cannot be used in place of it.

### Why a tab remembers your master passphrase

That's designed for practicality. Most people would hate having to enter a long master several times a day.

That's why RichPass lets you leave the tab open and come back to it without re-entering your master passphrase. To make that work, RichPass keeps an encrypted copy of it in that browser tab, so it survives when the browser reloads the tab. It never saves your master passphrase as readable text.

**Forget master passphrase** clears it from that tab whenever you want. For the best protection, open RichPass in Safari on a Mac.

### Why changing your master passphrase is protected

Changing your master passphrase completely changes every password RichPass makes, because the master is part of the input. The fingerprint is what RichPass uses to guard against accidental changes to your master passphrase.

So changing it takes two confirmations and the same five-entry setup. Your current master passphrase stays in use until the new one is fully set up. You can back out at any point with **Keep my current master passphrase**. Closing or reloading the page backs out too.

Setting up the same master passphrase again gives you the same passwords.

### Why all settings are fixed

RichPass's settings are fixed. This is **very deliberate**.

Any small change to how RichPass works could silently turn a working account password into a different one. Fixed settings mean the same input always gives the same password.

That's also why RichPass shows what it actually uses under each field, so you can note it down and repeat it. If two accounts would otherwise get the same details (for example, two logins at the same website), give each its own label.

### Why passwords use only one special character

Many websites demand "at least one special character" in your password. But many of those same websites dictate which special characters you can and can't use.

So, to make sure even the pickiest websites accept your password, RichPass only ever adds `@` as a special character. And every RichPass password always contains at least one digit and at least one uppercase letter, too.

### Why passwords are exactly 20 characters long

Long passwords are good. But many websites complain if you try to use a password they deem "too long".

So, to make sure even the pickiest websites accept your password, RichPass passwords are exactly 20 characters long.

And 20 characters is plenty. Even if you could try 1 million possible passwords every second, in 13.8 billion years (the age of the universe) you'd only get through about 0.000000000045% of the possible combinations.

So, as long as your master passphrase is strong and stays secret, guessing a 20-character RichPass password is not feasible.

<br>

---
---

# For technical readers

**Everything from here down is the technical detail: the derivation profile, storage and verification, release checks, platform notes and limitations. You do not need any of it to use RichPass.**

---

## Source layout and dependencies

Open `RichPass.html` directly in a browser. No server, build, installation or internet connection is needed. RichOS can optionally protect a separately saved copy (see [RichOS file protection](#richos-file-protection)). Download the standalone file from the [versioned RichPass releases](https://github.com/RichSpark/RichOS/releases?q=richpass-v1) in this repository.

The HTML is the first-party source, executable and distributable. Start with the opening source comment for the design rationale, data flow, trust boundaries and a test vector to run. The numbered sections then take you through markup, styles, input rules, derivation and UI state. The script blocks are `richpass-derivation` (derivation path), `richpass-session` and `richpass-verifier` (browser key handling and record formats) and `richpass-interface` (forms, validation, transitions and stale-work checks).

The two explicitly marked vendor scripts are the compiled exception: hash-wasm 4.12.0 supplies Argon2id plus its embedded WebAssembly and tldts 7.0.12 supplies the pinned domain parser plus suffix data. Their original sources, versions, licenses and file hashes are under `vendor/`. No CDN or runtime dependency fetch occurs. The HTML makes no automatic network requests.

## Derivation profile

- **KDF:** Argon2id v19, 128 MiB, three passes, one lane, 32 raw bytes (`argon2id m128 t3 p1 h32`). If the fixed-cost computation fails, RichPass reports failure instead of deriving with cheaper settings.
- **Salt:** effective domain, label and rotation joined with U+001F, encoded as UTF-8.
- **Output:** 20 ASCII letters, digits and `@`, with at least one digit and one uppercase letter and at most two consecutive digits. The final steps repair a missing class and replace any base64url hyphen or underscore with a lowercase letter. Compliant results stay intact.
- **Master:** JavaScript `trim()` only, before UTF-8 encoding.
- **Label normalization,** always in this order: NFKC, outer trim, whitespace to `-`, lowercase, restriction to `[a-z0-9._@-]`, repeated hyphen collapse, edge hyphen trim. Empty labels become `default`.
- **Rotation:** the first decimal digit sequence, minimum 1, with a `v` prefix; defaults to `v1`. It is an account input, independent of the RichPass release version.

The rotation and domain rules are part of the RichPass profile. The first public profile freezes input validation, normalization, suffix data, KDF parameters and the resulting formatter.

## Domain parsing and reference vectors

The browser's URL parser participates in domain normalization. For example, the free-text input `my bank` can produce different effective domains in Chromium and WebKit. RichPass shows the effective value before derivation. Use a website domain for a website account.

`tests/vectors.json` and `tests/vectors-webkit.json` record RichPass's effective inputs, UTF-8 salts, raw Argon2id bytes and final passwords for Chromium and WebKit. The raw bytes are independently checked with Argon2. The three public account examples and three formatter examples are embedded in `RichPass.html`; the opening source comment shows how to check the first account vector from the browser console on a trusted local copy.

`tests/formatter.cjs` checks the three embedded raw-byte formatter vectors and the output rules on seeded and synthetic inputs. A fixed corpus digest guards the exact formatter outputs.

## Master verification

### Setup flow

- First load without a saved fingerprint starts at 20% directly in the HTML and accessibility attributes, with no backwards fill transition. Later setup steps keep their forward animation.
- First use follows the approved round-62.1 design: one field is retyped through five steps, at 20/40/60/80/95 percent. The labels and buttons are Master passphrase / Get started, Confirm master passphrase / Next, Confirm master passphrase again / Next, Master passphrase once more / Next and Confirm master passphrase one last time! / Finish setup. These five exact raw matches establish the confirmed master.
- Show/Hide affects the current field. A prefix is incomplete; divergence or extra length immediately shows the approved mismatch warning, red edge and one shake unless reduced motion is requested. A full match enables the action without positive-match text. Start over clears the first entry and restarts all five.
- No hashing or persistent record is created after only one through four entries. Finish setup saves the real encrypted verifier; 100% completion follows actual success and shows the approved saved-fingerprint and recovery text. The user selects "OK. Let's go!" to open Generate.
- Master always sits between Why & How and Algo Card in those footers and opens Master Passphrase Ideas, with the approved Photos, Music and Places guidance and matching top and bottom back actions. The three methods each remain exactly two optional steps; spelling, case and separators are user choices, not new input restrictions.
- Home attention starts three seconds after load only when storage has confirmed no saved fingerprint; it stops on actual completion. Existing, unreadable or unchecked records never trigger it. Completion has no attention animation. The mock replay controls are not part of the app.

### Later entries

- Later entries must pass verification before RichPass accepts a master or enables password generation. Recovered tab masters are verified too. Account password derivation is unchanged.
- A mistyped master stays on entry with the exact round-63.1 failure panel, selected input and Try again. Checking... reflects the real verifier call. No fake KDF or artificial checking delay is used. Repeated misses update the approved third-try guidance.
- Caps Lock appears only when the browser reports it on. Explicit Show/reveal can mark spaces and show a Unicode character count; masking or leaving entry removes those revealed DOM copies. The wrong-entry readout is a live region, so a screen reader may announce revealed text.
- On resize, an already-open failure panel uses normal block flow to avoid WebKit retaining an old grid height and clipping text. Closing restores the original animated grid entrance; the approved colors, spacing and geometry stay unchanged.

### Verifier record

A random 16-byte salt is created once at setup and saved with a full 32-byte Argon2id verifier. Verification uses v19, 128 MiB, three passes and one lane with a separate purpose prefix (the UTF-8 text "RichPass master verification v1", a NUL byte, then the random bytes), never an account salt. The salt is reused when checking later entries and prevents precomputing one verifier for all installations.

The full payload is AES-GCM encrypted under the existing non-extractable IndexedDB key with a separate authenticated context and fresh IV. Only its encrypted envelope is written to `localStorage` under `richpass:verifier`. The verifier is never the account-password seed. Argon2id raises the cost of guessing a master from an exposed verifier, but cannot make a predictable master strong. Setup and later verification work offline without another password or a native service.

### Failure and recovery

- Missing, inaccessible, invalid or undecryptable verification starts fresh five-entry setup. For existing accounts, register the same original master to recover the same passwords. Setup never resets website passwords.
- If verification cannot be saved and read back, generation stays blocked with a retry message.
- Only confirmed fresh setup may repair an invalid encryption key; ordinary recovery never replaces it, because a new key cannot open the old records.
- Forget keeps verification but clears the active master and tab session. Other tabs keep their own tab sessions and share the confirmed master.
- Changed verification invalidates stale entry and output authority, using storage notifications plus checks before generation, publication and Copy.
- Browser data clearing or another browser, profile, file location or computer can require fresh setup.

`tests/verification.cjs` exercises enrollment, mismatches, encrypted record formats, saved-salt reuse, independent verifier bytes, fresh setup, blocked persistence, recovered-master checks and cross-tab invalidation. Its public vector is independently computed with argon2-cffi 25.1.0 / argon2-cffi-bindings 26.1.0 and is also documented in the HTML source.

## Changing the master fingerprint

Change master fingerprint is available when a readable fingerprint exists. Both exact warning dialogs start with Cancel focused. Cancel, Escape, backdrop press and navigation dismiss them without a write. After both affirmative actions, the same five-entry wizard opens.

The current verifier, master and encrypted tab record remain authoritative until Finish setup successfully writes and reads back the replacement. Keep my current master passphrase cancels pending work and returns to Master. Browser Back to the earlier entry or Generate state cancels the draft too, so the first Back remains meaningful. A view-only change-fingerprint hash distinguishes that draft route without putting secrets in URLs.

The draft exists only in RAM. Reload or exit abandons it and uses normal recovery of the saved fingerprint and session. Cross-tab changes and failed or ambiguous storage use the existing recovery checks, never a silent backup overwrite. Finish uses the existing salted Argon2id verifier, encryption and key, validates the expected current envelope and keeps the temporary save guard. A successful replacement updates tab retention for the newly confirmed master.

Different masters produce different account passwords; re-enrolling the same master leaves account outputs unchanged. Neither the warnings nor opening or canceling the wizard changes settings, account salts or stored records. The in-page Why & How wording is kept as is pending a separate copy revision; the current change-flow details are in the source comments and this README.

The public test owners cover confirmation exits, retained old records until final commit, cancellation and stale jobs, expected replacement, back/history, reduced motion and a controlled three-second attention clock.

## Tab session storage

- The master is retained by default as an authenticated AES-GCM-256 ciphertext record in this tab's `sessionStorage` under `richpass:master`. Each save uses a fresh random 96-bit IV and a 128-bit authentication tag.
- A non-extractable browser-managed key in IndexedDB (`richpass-session-keys`, key name `master-key`) restores the session through reloads. The verifier and tab master use distinct authenticated contexts, so a record for one cannot be used as the other. Each tab holds its own session record; the verifier and key are shared within the applicable browser storage scope.
- Account fields and generated passwords stay page-local. RichPass keeps no account list.
- Forget cancels pending saves and recovery, removes this tab's ciphertext and clears the live master. The shared random key remains so other tabs keep working.
- While an encrypted save is pending, a temporary `beforeunload` listener requests the browser's leave confirmation for reload, navigation or closing. It is removed after success, failure or timeout, Forget or actual page exit. The browser controls and sometimes suppresses that prompt and the user can override it. It cannot protect an unfinished save from crashes, force-quitting or every tab discard. Reloading before saving finishes can still require re-entry if departure proceeds.
- Unreadable tab storage is reported as unavailable; a restore failure is reported only after a saved record was actually read. A failed removal warns that an earlier session may remain only when that record was observed. A corrupt record still produces an honest recovery warning.
- Denied or unavailable storage keeps generation usable in memory without implying that a nonexistent saved session was lost. If storage access stops working mid-session, the warning says: "Your master remains usable on this page. RichPass could not access tab storage or confirm that it was cleared." This reports uncertainty without adding persistent state or claiming that old data was erased.
- This session-only fallback requires intact master verification. If verification itself is unavailable, fresh setup must save successfully before generation. There is no plaintext, raw-verifier or exported-key fallback.

## Browser and platform notes

- Safari 18.6 on macOS 15.7.7 was verified to depend on its original login Keychain for persisted Web Crypto keys (observed with the Keychain unlocked under a no-timeout policy, including reload recovery). Use Safari on a supported Mac for that OS-backed boundary. This is not an attestation for every Safari/macOS version or for locked or unavailable Keychains.
- Chrome uses the same encrypted-session flow, with a non-blocking Safari recommendation. Native Chrome 156 verification demonstrated that a copied profile could expose the key without a Keychain call. Other browsers must not be described as having verified OS-backed protection.
- The Safari recommendation is hidden only for a Mac/Safari identity reporting exactly zero touch points. Touch-capable Mac-style browsers keep it visible, as do browsers with unknown touch support, including desktop-mode iPad Safari. This is an advisory heuristic, never an OS-key attestation or an encryption or support switch.
- Encryption protects against a record-only leak; it does not stop code with the browser origin's authority from asking the browser to decrypt. An unlocked browser session, a privileged extension or compromised code can expose a live master.
- The first desktop integration and watcher are for macOS. Native Safari, Chrome and Firefox were exercised with this standalone runtime on macOS. Windows browser and watcher qualification is separate.
- In the RichOS app, Settings > RichPass saves a bundled verified copy without internet access.

## Verifying a download and running the checks

For independent verification, download `RichPass.html`, `RichPass.html.sha256` and `RichPass.html.minisig` together. Use the RichOS distribution public key from the trusted source checkout. From the download folder, run the following commands with Minisign installed.

```sh
shasum -a 256 -c RichPass.html.sha256
minisign -Vm RichPass.html -p /path/to/richos-updater.pub
```

Release assets are versioned. Verify downloaded bytes before trusting them. Readable source and verified bytes do not prove the absence of bugs.

From this folder:

- `python3 scripts/verify-release.py` runs the checked-in byte and provenance gate.
- `python3 scripts/verify-release.py --production` also verifies the canonical file, manifest, compiled updater trust anchor and vendored inputs.
- `node tests/standalone.cjs` runs using the RichOS Playwright harness and its shared worker admission. `RICHPASS_BROWSER=webkit` selects WebKit. WebKit's offline emulation refuses file URLs even for a trivial document, so that harness blocks HTTP(S) instead and records the limitation.

## Design references and layout tests

`RichPass.html` is the sole maintained product page. The alternate page previously used for styling is retired and is not read, compared against or updated. The frozen design references below are test fixtures, not additional product pages; their mock controls, mock-dock scaffolding and fake behavior never ship.

| Fixture | SHA-256 | Authority for |
|---|---|---|
| `tests/RichPass.setup-reference.html` (immutable round-62.1 setup mock) | `63f06796919bcf54007f978a1b7a85e24907560cfe824ad98c8167822c25236b` | First-use setup |
| `tests/RichPass.style-reference.html` (canonical 1.1.0) | `ae0ded1256449b06691fb5d90593f6037fbd7f4da771f46a1f4c1804d27b5f09` | Unchanged account controls, Help and Card |
| `tests/RichPass.states-reference.html` (round 63.1) | `4c0de31d128d5fc172334ad501a0533736c616086b2fc37482be2e88f5c3f050` | Completion, wrong master, the Generate heading and footer |
| `tests/RichPass.master-reference.html` (round 64.1) | `e7f2f6f8659e91f736f78694d717c4c4172e10d3ee972e2ada1d2d898131ec45` | Master page markup and product CSS, plus the explicitly requested action-label wrap |

Product CSS and theme match these designs, excluding only mock-dock CSS. Embedded fonts, title marks, speckle and ordinary controls stay within the approved visual system.

- `tests/setup-layout.cjs` compares the setup with the approved mock in both themes, through every step and mismatch.
- `tests/layout.cjs` preserves unchanged controls and Help/Card.
- `tests/states.cjs` compares all three new states, space-marked entry, Caps Lock and third-try feedback across the existing 18 viewport boundaries and both themes, with real verification and generation.

These use actual local files in Chromium and WebKit. Setup bounds are checked at every responsive boundary and input, Show and action bounds are checked through all five steps at 200% zoom. Persistent-profile tests use the configured or system temp directory, with no machine-specific path requirement.

## RichOS file protection

The page's Content Security Policy denies connections and form submissions and permits only the inline code, WebAssembly and embedded assets the file needs. Those restrictions travel with the file, but someone who can alter the file can also alter its policy. That is why RichOS can watch the exact bytes of a selected local copy against an approved checksum and restore the approved bytes when protection can write them.

- The RichOS watcher uses the approved SHA-256 built into the app.
- The initial watcher targets macOS local filesystems. It reserves one exact `RichPass.html` location only after native consent. Other copies are unprotected.
- The watcher sees only file bytes and a path. It has no part in handling the master and cannot attest what a browser tab already loaded or what a different copy runs. It cannot repair an already loaded page or secure an already running compromised browser.
- Stop protection before permanently editing, moving or deleting the protected copy.

## Limitations

- The browser controls tab-session restoration and may recover session data when reopening tabs or restoring a browser session. Use Forget to clear the master explicitly.
- A separately opened tab starts a separate session, although browsers may copy session storage when duplicating a tab or opening one through an opener.
- Browser extensions, browser password-save behavior, swap, crash dumps and clipboard histories are outside RichPass's control. Forget cannot prove physical memory erasure; clearing app references and byte buffers does not guarantee it.
- Clipboard clearing is an honest 20-second reminder, never an automatic overwrite, because RichPass cannot safely tell whether something else was copied in the meantime. The reminder can be delayed in a background tab.
- A readable fingerprint permits verification of a typed or recovered master; missing or unreadable verification starts fresh setup. The opening source comment explains these choices and the boundaries of encrypted tab continuity. The manifest identifies the approved artifact.

## Provenance and license

RichPass was adopted from [PassBooster](https://github.com/WebDevBooster/passbooster). The source provenance and the licenses of the vendored third-party code are recorded in the HTML and in `vendor/`. The RichPass source and vectors define the password profile from here.

RichPass is part of RichOS and is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0-only) like the rest of RichOS: see the [RichOS LICENSE](https://github.com/RichSpark/RichOS/blob/main/LICENSE). The RichPass name and the RichPass logo and icon artwork are not licensed under the AGPL: see [BRAND-ASSETS.md](https://github.com/RichSpark/RichOS/blob/main/docs/legal/BRAND-ASSETS.md).
