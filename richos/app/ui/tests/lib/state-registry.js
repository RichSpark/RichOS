// THE CLASSIFICATION. One row per user-visible state string, and the source is the
// authority over it — not the other way round.
//
// THE RULE THIS SERVES
// ====================
//   A state the user could change must be rendered together with the control that changes
//   it. A state change requiring a human action is not a status, it is a request.
//
// HOW THIS FILE IS KEPT HONEST
// ============================
// `lib/state-strings.js` scrapes the shipped source and produces the inventory. This file
// annotates it. `affordances.js` asserts the two sets are EQUAL — a string added to ANY
// shipped UI file, to the Tauri command layer or to a `ceo_message()` and not classified
// here fails the suite, and a row here whose string no longer exists in the product fails it
// too. There is no way to add a state and quietly skip the question.
//
// "ANY SHIPPED UI FILE" IS NEW WORDING AND IT USED TO BE A LIE. This sentence read
// "index.html, main.js, timeline.js" until 2026-09-05, and so did the scraper — three files
// against nine `<script src>` tags. `lib/ui-sources.js` derives the list from the tree now;
// the 55 rows that arrived the day it landed are grouped at the bottom of this file under
// the heading that says which surfaces had no gate over them.
//
// This file is therefore a list, and a list is the thing this whole sequence has been
// burned by eleven times. The difference is that nothing READS this list as an inventory:
// the inventory is derived, every run, and this file is checked against it. A list that is
// verified against a derivation is an annotation. A list that IS the derivation is a defect
// waiting for its first drift.
//
// IT LIVES IN `lib/` FOR A SMALL, EXACT REASON. `run.js` discovers suites as "every .js
// file in tests/ that is not run.js", and `lib/` is where it expects shared harness code
// rather than suites. A data module sitting in tests/ would be spawned as a suite, exit 0
// having asserted nothing, and be counted in "N suites passed" — a reassuring fraction over
// a file that tests nothing, which is the exact shape of every defect above.
//
// THE BUCKETS
// ===========
// The three the rule needs:
//
//   ACTIONABLE          the user could change this. MUST name a `control`: a selector for a
//                       button, field or row that is present, visible and enabled in the
//                       same view when the state renders. A doc link is not a control.
//   NEEDS-SOMEONE-ELSE  he cannot fix it; somebody can. MUST either name a party in the
//                       text itself (checked by regex — a future edit that deletes the
//                       party fails the suite) or carry `explainedBy`, pointing at the
//                       state in the same view that names it.
//   INFORMATIONAL       genuinely nothing to do. MUST NOT read as a request: no imperative
//                       aimed at the reader.
//
// And three that exist because the scrape returns strings, not states, and pretending
// otherwise would force a clean split the material does not have:
//
//   CONTROL       the string IS a control's label or accessible name. It is an affordance,
//                 not a state that needs one.
//   FRAGMENT      a piece of a composed sentence that never renders on its own (blind spot
//                 B1). Classified against the sentence it belongs to.
//   UNREACHABLE   present in source, and no path in the shipped UI reaches it. The reason
//                 is stated per row and is a claim someone can check.
//   NOT-RENDERED  a literal the prose filter caught that never reaches the DOM at all.
//
// WHERE A STATE IS TWO THINGS AT ONCE, both are said in `why` rather than forced into one
// bucket. The §21 unbound screen is the clearest case: the binding needs an operator
// (NEEDS-SOMEONE-ELSE) and starting fresh elsewhere is entirely the CEO's (ACTIONABLE), so
// it is classified by the thing the CEO cannot do and carries the control for the thing he
// can.

"use strict";

module.exports = [
  {
    "s": "unsupported web push service; refused: {}",
    "c": "NOT-RENDERED",
    "why": "Internal PhoneError from the outbound Web Push allowlist. The Mac logs this refusal; the phone receives the existing generic API error. It is not rendered as a CEO instruction or a desktop state."
  },
  {
    "s": "the navigation file could not be read, so it was left untouched and this change was not saved",
    "c": "INFORMATIONAL",
    "why": "The io::Error from NavStore::persist (nav.rs) over an unreadable navigation file, returned by rename_thread, set_thread_pinned and set_thread_archived as Err(String). main.js `saveNavChange` renders it, sentence-cased with a full stop, in `#nav-save-note` in the rail where he pinned, archived or renamed; the next successful change clears it. It states the outcome and asks nothing. tests/thread-switch.js checks it and its contrast in both themes."
  },
  {"s":"Manage reply notifications in the RichOS iPhone app under Settings.","c":"ACTIONABLE","control":"#phone-close","fixture":null,"why":"Native preferences are on the phone. This Mac view names their exact location and retains the close control, like the existing phone-side Web Push instructions. phone.js check 9d proves the native instruction excludes Safari installation."},
  // Native notification API states.
  {"s": "Invalid notification registration", "c": "UNREACHABLE", "why": "Defensive duplicate validation inside the native notification desk. The authenticated route rejects malformed registrations before calling it and the native adapter supplies fixed bundle/environment values."},
  {"s": "Native notifications are unavailable", "c": "UNREACHABLE", "why": "Default bridge refusal. Production overrides it and routes refuse unsupported native-push capability before calling the default."},
  {"s": "Reply notifications could not reach the service. Your conversation still works. Retry notifications in settings.", "c": "ACTIONABLE", "control": "#notifications-refresh", "fixture": null, "why": "Phone notification setup failure. The native settings view contains Retry notifications alongside its status; shared client tests prove that failure does not disable text."},
  {"s": "This phone is no longer paired.", "c": "INFORMATIONAL", "why": "A registration racing revocation is rejected. No notification registration is retained for the revoked device. The route translates it to a bounded failure response."},

  // ---- SCREENSHOTS AND FILES ON THE MAC COMPOSER (CEO §86, 2026-09-24) -------------------
  // `ui/attachments.js`, `src-tauri/src/mac_attachments.rs` and the `Origin::Mac` arm of
  // `src-tauri/src/phone/attachments.rs`. Every refusal renders in `#attach-note` under the
  // composer as "<file name>: <sentence>"; `tests/attachments.js` checks 3 and 5 render them.
  // Drop and paste are gestures, not buttons, so a sentence asking him to add a file again
  // names the composer field (`#input`), which is where both land.
  {"s": "1 file attached", "c": "INFORMATIONAL", "why": "The accessible name of the tray's list (`#attach-list`), counting what is on the composer. It asks nothing."},
  {"s": "Attaching 1 file:", "c": "FRAGMENT", "why": "The head of the placeholder bubble a new thread's first message shows for the moment before the thread exists; the file names follow it, and the real bubble replaces it with the words Rich receives."},
  {"s": "Drop to attach to your message", "c": "ACTIONABLE", "control": "#input", "fixture": null, "why": "Shown only while a drag carrying files is over the window (`rich://file-drag`). The action is the drop he is already making; the composer it lands on is on screen. tests/attachments.js check 8 shows and hides it."},
  {"s": "A file is still being added. Press Send again in a moment.", "c": "ACTIONABLE", "control": "#send", "fixture": null, "why": "Send pressed while a pasted or dropped file is still crossing to the desk. Send stays enabled beside it."},
  {"s": "A message can carry at most 10 files.", "c": "INFORMATIONAL", "why": "The desk's count ceiling, prefixed with the name of the file that crossed it. Nothing was attached; the ten already on the composer stay. tests/attachments.js check 3."},
  {"s": "RichOS can't take this kind of file yet. Photos, PDFs, text and Office documents work. Nothing was attached.", "c": "INFORMATIONAL", "why": "The Mac's unknown-type refusal (`Origin::Mac`), prefixed with the file's name. It names what does work and asks nothing."},
  {"s": "RichOS couldn't add this file. Nothing was attached.", "c": "INFORMATIONAL", "why": "The tray's fallback when a refusal arrives with no sentence of its own. States the outcome only."},
  {"s": "RichOS couldn't attach your files just now. They are still attached; press Send to try again.", "c": "ACTIONABLE", "control": "#send", "fixture": null, "why": "`commit_attachments` failed without a sentence. His words and files stay on the composer, and Send is the control that retries."},
  {"s": "RichOS couldn't read this file. Nothing was attached.", "c": "INFORMATIONAL", "why": "A dropped path that could not be opened (moved or deleted between drag and drop, or unreadable). States the outcome only."},
  {"s": "RichOS couldn't save this file just now. Nothing was attached; try again.", "c": "ACTIONABLE", "control": "#input", "fixture": null, "why": "The desk could not write the staged file. Trying again is a paste or drop onto the composer field, which is on screen."},
  {"s": "RichOS is holding too many unsent files. Send or remove some, then try again.", "c": "ACTIONABLE", "control": "#send", "fixture": null, "why": "The desk's 200 MiB unsent budget for this Mac. Send, and each chip's own Remove control, are both in the same view."},
  {"s": "That drop is no longer available. Drop the file again.", "c": "ACTIONABLE", "control": "#input", "fixture": null, "why": "A drop number the Rust shelf no longer holds (read once, or more than ten minutes old). The drop lands on the composer, which is on screen."},
  {"s": "This file is empty. Nothing was attached.", "c": "INFORMATIONAL", "why": "The Mac's empty-file refusal. tests/attachments.js check 3 renders it."},
  {"s": "This file is larger than 25 MB, the most RichOS takes in one file. Nothing was attached.", "c": "INFORMATIONAL", "why": "The Mac's size refusal, the same 25 MiB ceiling as the phone. tests/attachments.js check 3 renders it for a paste that never crosses the bridge."},
  {"s": "This is a folder, not a file. Drop the files inside it instead. Nothing was attached.", "c": "ACTIONABLE", "control": "#input", "fixture": null, "why": "A folder dropped on the window. The files inside it land on the composer, which is on screen. tests/attachments.js check 3 renders it."},
  {"s": "is no longer waiting to be sent. Attach it again; your words are still in the box.", "c": "FRAGMENT", "why": "Follows a file's name: the desk no longer holds it (the seven-day sweep, or room made for newer files), so it leaves the tray and nothing is sent. The whole sentence is ACTIONABLE against `#input`, where it is attached again. tests/attachments.js check 5."},
  {"s": "The files in this message add up to more than 100 MB, the most RichOS takes in one message.", "c": "INFORMATIONAL", "why": "The desk's per-message ceiling, shared by the phone and the Mac. On the Mac it follows the file that crossed it; the files already on the composer stay. States the limit only."},
  // Add to chat from the Output panel (Output side panel PRD §12.7, slice S7): a recorded file
  // goes on the same tray through the same desk (`output_attach`), so every refusal above is
  // its refusal too, rendered the same way. tests/attachments.js checks 10–12.
  {"s": "is attached to your next message.", "c": "FRAGMENT", "why": "Follows the file's name (PRD §6.7, 'Action succeeded'): `RichAttachments.addRecorded` answers it once the chip is on the tray, and the Output panel says it in its on-screen notice (`#op-notice`, role=status, slice S6's) and through the conversation's live region. INFORMATIONAL as a whole: it says what was done and asks nothing. Add to chat is the last item of S6's menus (the row's More actions, the file view's More actions and the right-click), through `hooks.menuItems`."},
  // The phone's own wording of the same refusals, now reached through `Origin::Phone`. They are
  // the refusal bodies the phone channel returns to a phone and are never drawn in this window.
  {"s": "RichOS can't take this kind of file yet. Photos, PDFs, text and Office documents work. The file is still on your phone.", "c": "NOT-RENDERED", "why": "`Origin::Phone`: returned to the phone in an upload refusal and drawn by the phone, never in this window."},
  {"s": "This file is empty. Nothing was sent.", "c": "NOT-RENDERED", "why": "`Origin::Phone`: returned to the phone in an upload refusal and drawn by the phone, never in this window."},
  {"s": "This file is larger than 25 MB, the most RichOS takes from a phone. It is still on your phone.", "c": "NOT-RENDERED", "why": "`Origin::Phone`: returned to the phone in an upload refusal and drawn by the phone, never in this window."},
  {"s": "Your Mac is holding too many unsent files from this phone. Send or remove some, then try again.", "c": "NOT-RENDERED", "why": "`Origin::Phone`: returned to the phone in an upload refusal and drawn by the phone, never in this window."},

  // Native phone voice API states.
  {"s": "I could not hear speech in that recording. Your recording is still on your phone.", "c": "ACTIONABLE", "control": "#record-start", "fixture": null, "why": "Authenticated phone voice refusal. The native conversation keeps the microphone, queued-message recovery and typed composer available in the same view. Mobile client voice checks cover retained files and no dispatch on cancellation; the Mac renderer does not display this phone API response."},
  {"s": "I could not transcribe that recording. Your recording is still on your phone.", "c": "ACTIONABLE", "control": "#record-start", "fixture": null, "why": "Authenticated phone voice refusal. The native conversation keeps the microphone, queued-message recovery and typed composer available in the same view. Mobile client voice checks cover retained files and no dispatch on cancellation; the Mac renderer does not display this phone API response."},
  {"s": "I could not understand that recording. Your recording is still on your phone.", "c": "ACTIONABLE", "control": "#record-start", "fixture": null, "why": "Authenticated phone voice refusal. The native conversation keeps the microphone, queued-message recovery and typed composer available in the same view. Mobile client voice checks cover retained files and no dispatch on cancellation; the Mac renderer does not display this phone API response."},
  {"s": "This voice message exceeds the 30-minute sending limit. It has been kept on your phone.", "c": "ACTIONABLE", "control": "#record-start", "fixture": null, "why": "Authenticated phone voice refusal. The native conversation keeps the microphone, queued-message recovery and typed composer available in the same view. Mobile client voice checks cover retained files and no dispatch on cancellation; the Mac renderer does not display this phone API response."},
  {"s": "The recording exceeds the upload limit.", "c": "ACTIONABLE", "control": "#record-start", "fixture": null, "why": "Authenticated phone voice refusal. The native conversation keeps the microphone, queued-message recovery and typed composer available in the same view. Mobile client voice checks cover retained files and no dispatch on cancellation; the Mac renderer does not display this phone API response."},
  {"s": "This recording is not a supported WAV file.", "c": "ACTIONABLE", "control": "#record-start", "fixture": null, "why": "Authenticated phone voice refusal. The native conversation keeps the microphone, queued-message recovery and typed composer available in the same view. Mobile client voice checks cover retained files and no dispatch on cancellation; the Mac renderer does not display this phone API response."},
  {"s": "Audio playback is unavailable. The reply remains in the conversation.", "c": "INFORMATIONAL", "why": "Playback failure leaves the already visible reply readable. No setup action is required to continue the conversation."},
  {"s": "This reply is not available for playback.", "c": "INFORMATIONAL", "why": "Playback failure leaves the already visible reply readable. No setup action is required to continue the conversation."},
  {"s": "Audio playback is unavailable", "c": "UNREACHABLE", "why": "Bridge default for test or unsupported implementations. The production PhoneBridge overrides this method and unsupported capabilities prevent dispatch."},
  {"s": "Speech recognition is unavailable", "c": "UNREACHABLE", "why": "Bridge default for test or unsupported implementations. The production PhoneBridge overrides this method and unsupported capabilities prevent dispatch."},
  {"s": "This reply is too long for audio playback. The complete reply is in the conversation.", "c": "INFORMATIONAL", "why": "The complete reply is already displayed in the conversation. Reading it requires no hidden action or navigation."},
  {"s": "Speech recognition is not ready on this Mac. Whoever set RichOS up needs to prepare it. Your recording is still on your phone.", "c": "NEEDS-SOMEONE-ELSE", "party": true, "fixture": null, "why": "The Mac speech prerequisite needs its operator. The phone retains the recording and its typed composer remains available."},

  // -------------------------------------------------------------------------------------
  // ACTIONABLE
  // -------------------------------------------------------------------------------------
  {
    s: "Anything I'd written is above. Your message is safe: I'll put it back in the box for you.",
    c: "ACTIONABLE",
    control: ".tl-intervention--quiet button.tl-intervention-action",
    fixture: "unknown-turn",
    why: "§14's unknown-outcome card. Offers to restore his message; the button is in the card.",
  },
  {
    s: "This turn was still running the last time RichOS was open and I can't tell you how it ended.",
    c: "ACTIONABLE",
    control: ".tl-intervention--quiet button.tl-intervention-action",
    fixture: "unknown-turn",
    why: "The same card's body. The outcome is unknowable; picking the message back up is his move.",
  },
  {
    s:
      "Not now is fine. I'll leave the message box switched off until you pick one and " +
      "the button to do it stays right there.",
    c: "ACTIONABLE",
    control: "#entity-picker-later",
    fixture: null,
    why:
      "The cost of deferring the first-run company question — the 2026-09-17 nightly's D5. " +
      "First run stacked three sheets and this was the one with no \"Not now\" on it; the " +
      "audit called it \"at least inconsistent and at most a wall\". The control it names is " +
      "the deferral itself, because this sentence is about what pressing THAT does. It is " +
      "modelled on the interview offer, which the audit singled out for saying what its own " +
      "\"Not now\" costs — and the cost here is the opposite one: the question returns, " +
      "because he cannot type until it is answered. Driven by ui/tests/setup.js check 1a, " +
      "which also asserts the composer is still blocked and #composer-choose-company still " +
      "on screen afterwards, so deferring can never become the §21 armed-composer leak.",
  },
  {
    s: "Rich's answer was cut off before he finished.",
    c: "ACTIONABLE",
    control: ".tl-intervention button.tl-intervention-action",
    fixture: "failed-turn",
    why:
      "The renderer's fallback when no interruption or outage record explains an incomplete " +
      "reply, including after a normal close. It speaks about Rich without guessing a cause or " +
      "claiming saved answer text. The original message can be restored with this control.",
  },

  // -----------------------------------------------------------------------------------
  // WHY A TURN ENDED WITHOUT FINISHING — richos-core `interruption.rs`, the nightly's D2
  //
  // `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D2: the
  // published nightly held `Not logged in · Please run /login` in its own ledger and showed
  // the generic card above it — a permanent condition called a snag, a promise about saved
  // work that did not exist, and a retry that could not succeed.
  //
  // `fixture: null` on all five, for the reason the richos-voice rows above it give: these
  // sentences are produced by a CLASSIFIED interruption, which the mock bridge does not
  // reach — it has no lease and therefore no `CognitionError` to classify. They are driven
  // directly, against the real renderer, by `ui/tests/interruption.js`, which asserts each
  // one's control (or its deliberate absence) on the rendered card.
  // -----------------------------------------------------------------------------------
  {
    s: "RichOS couldn't send this to Rich because no Claude account is signed in on this Mac or the sign-in has expired. Open Settings, choose Claude accounts and sign in, then send your message again.",
    c: "ACTIONABLE",
    control: "#set-btn",
    fixture: null,
    why:
      "`InterruptionCause::NotSignedIn`. HIS to fix, which is why it is ACTIONABLE and not " +
      "NEEDS-SOMEONE-ELSE: the account is his and the screen exists. The audit's third " +
      "finding was that the app never mentioned the account at all 'even though the app " +
      "offered to connect one on first run, knows it was declined, and has a working route " +
      "to it'. The control is the Settings button at top right, which is on screen in every view " +
      "this card can appear in, and the sentence itself names the route in words. It was TWO sentences briefly, the second returned by a `route()` method — deleted because under app/crates this registry can only see literals inside a function called `ceo_message`, so a route authored anywhere else would reach his screen while being invisible to the one inventory that asks whether he can act on what he is told.",
  },
  {
    s: "RichOS couldn't send this to Rich because the Claude key or sign-in token set up on this Mac was refused. Sending again won't help until that key or token is replaced.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "The notice names the key or program that must be replaced; this card cannot perform that installation.",
    fixture: null,
    why:
      "`InterruptionCause::CredentialRejected`, and a DIFFERENT class from the sign-in row " +
      "on purpose: the four vendor constants behind it (`Invalid API key`, `Invalid auth " +
      "token`, `Invalid ANTHROPIC_CUSTOM_HEADERS`, `Invalid request header from the " +
      "environment`, read verbatim out of the installed bundle) describe a credential in " +
      "this machine's ENVIRONMENT. Sending him to the account screen would send him " +
      "somewhere that cannot help. It names the party and offers no control, which is the " +
      "honest shape for a state he does not own.",
  },
  {
    s: "RichOS couldn't send this to Rich because Claude Code, which Rich runs on, could not be found on this Mac. Sending again won't help until Claude Code is installed again.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "The notice names the key or program that must be replaced; this card cannot perform that installation.",
    fixture: null,
    why:
      "`InterruptionCause::ProviderMissing` — the 2026-09-17 candidate-walk defect. " +
      "`resolve_claude_bin` used to fall through to a bare `claude` that a Finder-launched " +
      "provider's own replaced PATH could never resolve, and the resulting failure " +
      "(`provider-supervisor.py`'s own `Provider could not start: [Errno 2] No such file or " +
      "directory`) was misclassified as `Transient`, promising a retry that could never work. " +
      "A DIFFERENT class from the credential row above for the same reason that row is " +
      "different from the sign-in row: the CEO's own account is fine, and the fix (installing " +
      "or pointing at Claude Code) is whoever set RichOS up's to make, not his.",
  },
  {
    s: "Stopped, as you asked.",
    c: "INFORMATIONAL",
    fixture: null,
    why:
      "`InterruptionCause::StoppedByCeo`. Nothing failed, so there is nothing to retry and " +
      "no control belongs on the card — a control that hands his words back would dress his own " +
      "deliberate stop as an error. It issues no instruction and asks for nothing.",
  },
  {
    s: "RichOS lost its connection to Claude Code while Rich was answering, so his answer is incomplete. Press Restore my message, then send it again.",
    c: "ACTIONABLE",
    control: ".tl-intervention button.tl-intervention-action",
    fixture: null,
    why:
      "`InterruptionCause::Transient` — the ONE class where asking again is honest advice, " +
      "and therefore the one that still draws the card's button. It is the positive control " +
      "for the whole change: without a class that keeps the control, 'the sign-in case has " +
      "no button' could just mean the card stopped drawing one. It said 'This usually fixes " +
      "itself' until 2026-10-21, when a new user was shown it on every message of a Mac where it " +
      "never did; the same failure again right after it is `Repeated`, the row below.",
  },
  {
    s: "Rich could not answer again because Claude Code stopped before he replied, so sending again is unlikely to help. Open Settings and choose Claude accounts to check that your account is signed in on this Mac. If it is, press Bust a bug in Settings so we can see what went wrong.",
    c: "ACTIONABLE",
    control: "#set-btn",
    fixture: "interruption-repeated",
    why:
      "`InterruptionCause::Repeated` (2026-10-21, docs/development/failure-classes/" +
      "permanent-failure-told-as-passing.md): the lost connection again, right after the last " +
      "one, with no answer in between. A failure that repeats is not passing, so it offers no " +
      "retry; it names the one place that says whether the account is signed in (Claude " +
      "accounts, which now says Not signed in with Sign in on the card) and the way to report " +
      "it when it is. The Settings button is on screen in every view this card appears in.",
  },
  {
    s: "Rich's answer stopped early and RichOS can't tell you why. Press Restore my message, then send it again.",
    c: "ACTIONABLE",
    control: ".tl-intervention button.tl-intervention-action",
    fixture: null,
    why:
      "`InterruptionCause::Unknown`, the arm a reworded vendor string falls to. ACTIONABLE " +
      "rather than NEEDS-SOMEONE-ELSE because the first thing offered is his and is drawn " +
      "on the card; the named party is the second sentence, for when the first does not " +
      "work. It claims no cause it cannot support and points at no screen that might be " +
      "wrong.",
  },
  {
    s: "Rich stopped before finishing.",
    c: "ACTIONABLE",
    control: ".tl-intervention button.tl-intervention-action",
    fixture: "failed-turn",
    why:
      "The §18 announcement that mirrors the failure card. For a screen-reader user it is the " +
      "only signal, so the card's control must be in the DOM when it fires.",
  },
  {
    s: "Set your name",
    c: "ACTIONABLE",
    control: "#rail-identity",
    why:
      "The rail footer's honest unset state (CEO correction to round 10.1). There is no " +
      "user-name preference on a fresh install, so this is what almost every install shows, " +
      "and it is deliberately NOT an invented name or a '??' placeholder. It is ACTIONABLE " +
      "because it is an imperative aimed at the reader, and the control is the row itself — " +
      "in the unset state `#rail-identity` is a button that opens the one Settings panel " +
      "on the field (round 27). Once a name is set the row stops being a control and the " +
      "string stops rendering, which is the correct pairing in both directions.",
  },
  {
    s: "No name is set. Open Settings to add yours.",
    c: "ACTIONABLE",
    control: "#rail-identity",
    why:
      "The accessible name of the same row, and for a screen-reader user it is the whole " +
      "message — the empty initials circle is `aria-hidden`. Same control, stated longer " +
      "because a two-word label reads as a heading when it is announced alone.",
  },
  {
    s: "I can't hear anything. Check your mic isn't muted, then try again.",
    c: "ACTIONABLE",
    control: "#voice-retry",
    fixture: "voice-no-audio",
    why: "The mic is open and silent. He unmutes, then presses the control this line names.",
  },
  {
    s: "I couldn't record that stop, so I haven't acted on it. Press Stop again and I'll have another go.",
    c: "ACTIONABLE",
    control: "#stop",
    fixture: "working-turn",
    why: "Only reachable with a live turn, which is exactly when syncComposerMode shows Stop.",
  },
  {
    s: "I couldn't start that thread just now. Your words are still in the box below. Press Send to try again.",
    c: "ACTIONABLE",
    control: "#send",
    fixture: "thread-create-refused",
    why: "create_thread_in refused. Entity view, sendBlockedReason null, so Send is live.",
  },
  {
    s: "Your words are back in the box below, word for word. Press Send when you want me to try again.",
    c: "ACTIONABLE",
    control: "#send",
    fixture: "not-connected",
    why: "The second half of every refused send. Names the control that retries.",
  },
  {
    s: "Nothing matches that.",
    c: "ACTIONABLE",
    control: "#search-input",
    fixture: "search-empty",
    why: "A search with no hits. The field he retypes into is the affordance, and it is focused.",
  },
  {
    s: "I can't find a microphone on this machine. Plug one in and tap ◉ again.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why: "Precise instruction naming ◉, which is #talk-toggle and never hides (index.html:150).",
  },
  {
    s: "I couldn't open the microphone. In System Settings, under Privacy and Security, give RichOS microphone access, then tap ◉ again.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why: "Names the settings pane AND the control to press afterwards.",
  },
  {
    s: "This microphone gives me audio I can't work with. Pick a different one in System Settings, under Sound, then tap ◉ again.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why: "There is no device picker in RichOS, so the instruction names the one that exists.",
  },
  {
    s: "I can't reach the speakers on this machine, so I'll keep answering in text. Plug in headphones or speakers and tap ◉ again if you want me talking.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why: "Every PlayoutError variant is a missing or unusable output device. Plugging one in is his.",
  },
  {
    s: "The mic still won't open. I've switched us back to typing. Tap ◉ when you want to try voice again.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why: "Said after #voice-retry fails and voice mode is torn down. ◉ is the way back in.",
  },
  {
    s: "I didn't catch that. Say it again?",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why:
      "The mic is already open, so the action is simply to speak; ◉ is present either way and " +
      "is the control if he would rather stop.",
  },
  {
    s: "ended with an error",
    c: "ACTIONABLE",
    control: ".nav-thread",
    fixture: "rail-mark",
    why:
      "A rail status mark. The mark is appended INSIDE the thread's own <button> " +
      "(main.js buildThreadRow), so the affordance is structural: the state cannot render " +
      "outside the control that opens the thread carrying it.",
  },
  {
    s: "last turn ended without finishing",
    c: "ACTIONABLE",
    control: ".nav-thread",
    fixture: "rail-mark",
    why: "Same structural affordance as 'ended with an error'.",
  },
  {
    s: "outcome unknown: a turn never finished",
    c: "ACTIONABLE",
    control: ".nav-thread",
    fixture: "rail-mark",
    why: "Same structural affordance. Opening the thread reaches §14's card, which has the button.",
  },

  // THE OTHER NO-LEASE CAUSE, and the reason it is ACTIONABLE where the sentence above it is
  // NEEDS-SOMEONE-ELSE. Nothing is signed out here; something was never installed, and the
  // app can install it. ray-opus-a2 met this state on published v1.0.1 and was handed the
  // sentence below instead — "quit and reopen" on a machine with no engine, which the next
  // boot would fail in exactly the same way.
  {
    s: "I can't take that on yet: the RichOS engine isn't on this Mac and that's the part of me that knows how I work. I've put the setting up back on your screen: press Set it up and I'll fetch it. There's nothing to quit and nothing to reopen.",
    c: "ACTIONABLE",
    control: "#setup-go",
    fixture: "setup-refused-engine",
    why:
      "He can fix this himself, in one press, without leaving the window. The sheet is " +
      "reopened by the same catch that renders the notice, so the control the sentence " +
      "names is on screen when he reads it.",
  },
  {
    s: "I can't take that on yet: Claude Code isn't on this Mac and that's the program I think with. I've put the setting up back on your screen: press Set it up and I'll fetch it. There's nothing to quit and nothing to reopen.",
    c: "ACTIONABLE",
    control: "#setup-go",
    fixture: "setup-refused-claude",
    why: "Same state, other component. A Mac set up once and since had its binary removed.",
  },
  {
    s: "I can't take that on yet: this Mac doesn't have Claude Code or the RichOS engine and those are what I think with. I've put the setting up back on your screen: press Set it up and I'll fetch them. There's nothing to quit and nothing to reopen.",
    c: "ACTIONABLE",
    control: "#setup-go",
    fixture: "setup-refused-both",
    why: "The customer's Mac on the day he installs, after he answers the sheet with 'Not now'.",
  },
  {
    s: "Your words are back in the box below, word for word. They'll be there when the setting up is done.",
    c: "FRAGMENT",
    why:
      "Appended to the three sentences above, and only to those. The ordinary tail names " +
      "Send; this one deliberately does not, because the setup sheet is covering the " +
      "composer at the moment he reads it and an instruction he cannot follow is worse " +
      "than none.",
  },

  // -------------------------------------------------------------------------------------
  // NEEDS SOMEONE ELSE
  // -------------------------------------------------------------------------------------
  {
    s: "RichOS couldn't reach Claude Code, which Rich runs on. Quit RichOS and open it again, then send your message again.",
    c: "ACTIONABLE",
    control: "#send",
    fixture: "not-connected",
    why:
      "Two contexts in one sentence, and both are stated. Quitting and reopening is his and " +
      "needs no control; signing the lease back in is not, and is named as somebody else's. " +
      "Send is required because the same notice restores his words.",
  },
  {
    s: "I can't open this one. It has no entity home: it predates entity scoping and I won't guess which entity this work belongs to. Filing it under the wrong one would mix up two companies' records and that's not a mistake worth risking to save you a question.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "the detail line beneath it, which names whoever set RichOS up",
    control: "#unbound-new-thread",
    fixture: "unbound",
    why: "§21's body. He cannot bind the thread; he can start the work again where it belongs.",
  },
  {
    s: "Filing it under a company is a job for whoever set RichOS up. There is no control for it in the app yet, so it will not sort itself out. Meanwhile the button above starts a fresh thread wherever you say and I'll carry on there.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    control: "#unbound-new-thread",
    fixture: "unbound",
    why: "The line that names the party and points at the way out. 'The button above' must exist.",
  },
  {
    s: "This thread has no entity home: it predates entity scoping and Rich will not guess which entity this work belongs to. An operator must bind it explicitly.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    control: "#unbound-new-thread",
    fixture: "unbound",
    why:
      "The CORE's own wording, relayed verbatim so the screen and the guard cannot drift " +
      "(main.rs:915 <- ledger.rs:52). 'An operator' is the party; the CEO-legible restatement " +
      "follows it in the same paragraph.",
  },
  {
    s: "This thread has no entity home, so I can't take a message in it.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "#unbound-view-detail, in the same view",
    control: "#unbound-new-thread",
    fixture: "unbound",
    why: "The composer-blocked line. Deliberately short; the screen above it carries the who.",
  },
  {
    s: "Send is off for this thread",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "#unbound-view-detail, in the same view",
    control: "#unbound-new-thread",
    fixture: "unbound",
    why:
      "The disabled composer's placeholder. §21: an inviting placeholder over a dead field is " +
      "the composer telling a small lie, so it states the block instead.",
  },
  {
    s: "Needs an entity",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "#unbound-view, which every row in this group opens",
    control: ".nav-group--unbound .nav-thread",
    fixture: "rail-mark",
    why: "The rail group heading. Two words; the destination screen carries the party and the way out.",
  },
  {
    s: "no entity home",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "#unbound-view, which the row carrying this mark opens",
    control: ".nav-thread",
    fixture: "rail-mark",
    why: "The rail status mark for an unbound thread, rendered inside the row's own button.",
  },
  {
    s: "I can't show priorities for this area yet and the area itself is set up inside RichOS rather than in settings. Whoever set RichOS up is the one who changes it. Nothing here needs you.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    fixture: "entity-view",
    why:
      "No control, correctly: there is nothing here for him to press. A NEEDS-SOMEONE-ELSE " +
      "state owes a party and a next step, not an affordance, and it closes the loop explicitly.",
  },
  {
    s:
      "I can hear sound, but I'm not getting words out of it. The microphone may be " +
      "picking up the room rather than you. Voice is still on.",
    c: "INFORMATIONAL",
    why:
      "`VoiceNotice::SoundButNoWords`, added 2026-09-04. Three utterances in a row came " +
      "back as whisper's documented silence noise, which is the ONE path in the recognizer " +
      "thread that used to produce nothing at all: an utterance in, an eprintln out, and " +
      "the panel still saying \"listening…\". Ray measured 25+ seconds of exactly that on " +
      "published v1.0.0. It is INFORMATIONAL rather than ACTIONABLE deliberately — it " +
      "invents no control, because the ◉ that ends voice is already on screen with its own " +
      "footnote two lines below the notice, and a sentence pointing at a control that is " +
      "already offered is a request wearing a status's clothes.",
  },
  {
    s:
      "That didn't come through as speech, so I haven't sent anything. If you said " +
      "something, please say it again. I'm still listening.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why:
      "ACTIONABLE SINCE 2026-10-21, with its two siblings: a line saying his words may not " +
      "have gone anywhere now says what to do, conditionally (a cough reaches this path " +
      "too), and the open microphone (◉, #talk-toggle) is the control. The class and its " +
      "check: docs/development/failure-classes/lost-input-without-a-next-step.md. The " +
      "history before that change follows. " +
      "`VoiceNotice::HeardNoVoice`, added 2026-09-05. The audio-grounded gate in " +
      "`crates/richos-voice/src/voiced.rs` measured the recording and found no voice in " +
      "it, so whisper was never called and nothing was submitted. It exists because the " +
      "alternative shipped: on 2026-09-04, on published v1.0.1, on the CEO's own Mac, a " +
      "tap on the talk button in a quiet room produced \"1, 2, 3, testing.\" and the app " +
      "SENT IT as his message (ray-opus-a2). " +
      "Two things are load-bearing in the wording. It says nothing was sent, because the " +
      "defect being fixed is the app sending a sentence he never said and a refusal that " +
      "leaves that question open is no better than the drop. And it ends with a status " +
      "rather than \"say it again\": the affordance for repeating himself is the open " +
      "microphone, there is no button to point at, and an imperative with no control is a " +
      "request wearing a status's clothes — the same reason `SoundButNoWords` ends with " +
      "\"Voice is still on.\" It is latched per run of refusals, so it states the refusal " +
      "once and never becomes a drip.",
  },
  {
    s:
      "While I was speaking, I couldn't tell your voice from my own, so if you said " +
      "something just then, it didn't reach me and I haven't sent anything. If you did, " +
      "please say it again now. I'm listening.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why:
      "ACTIONABLE SINCE 2026-10-21. A new user on 1.2.1 got this line right after a failed " +
      "answer (Rich had just spoken the cut-off notice through his speakers) and it ended " +
      "with a status, so it explained a loss and gave no next step. It now says the step, " +
      "conditionally: say it again now. The microphone is open (◉, #talk-toggle, is on " +
      "while voice is on), so the step needs nothing else. What follows is the history of " +
      "the sentence before that change. " +
      "`VoiceNotice::CouldNotListenWhileSpeaking`, added 2026-09-17 for audit-3 §4 #1 and " +
      "REWRITTEN the same day after Ray's candidate-.4 walk proved the first wording false " +
      "on the CEO's own rig. It used to read \"You started talking while I was still " +
      "speaking…\" and it fired at him while he was provably silent: at output volume 85 a " +
      "spoken turn produced it with nobody talking, and the identical turn with the output " +
      "volume set to 0 produced no discard and no notice at all. The single variable was " +
      "whether Rich's own voice was audible in the room. " +
      "THE CODE SAYS THE SAME THING STRUCTURALLY. `push_residual` sets `tainted = speaking " +
      "&& !barged && !confident`, so a tainted discard REQUIRES the canceller to have " +
      "declined to vouch for its residual; when it IS confident the utterance is admitted " +
      "rather than discarded and there is nothing to announce. The old sentence was " +
      "therefore emitted only, and exactly, on the path where the app cannot know whose " +
      "voice it heard — so the accusation was deleted rather than gated behind a condition " +
      "that never occurs. " +
      "EVERY CLAUSE ABOUT THE CEO IS NOW CONDITIONAL (\"if you said something just then\"), " +
      "and what it asserts outright is only what this file can prove: Rich was speaking, the " +
      "two voices could not be separated, and that audio was not used. " +
      "AND IT IS NOT A WARM-UP STATE ON HIS HARDWARE. Measured on his Mac on 2026-09-17 " +
      "(docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md): Mac mini Speakers out, " +
      "Elgato Wave:3 in, steady-state ERLE of −0.5 to +0.7 dB across five live runs — nothing " +
      "measurably removed — while confidence needs the residual to hold under −52.04 dBFS and " +
      "the microphone reads −42 to −46 dBFS during Rich's speech. A recording of that exact " +
      "path is committed as a test fixture and replays under `cargo test` without reaching " +
      "confidence, so every spoken answer through the speakers produces one of these " +
      "discards. " +
      "It still makes NO capability claim: \"I can't hear you while I'm " +
      "speaking\" would be false twice, because talking over him for 5.008 s cuts him off " +
      "today and a confident canceller admits the utterance outright. It still states the " +
      "consequence, because the failure it explains is the app sending the TAIL of his " +
      "sentence as though it were the whole of it. Its step is \"say it again now\", never \"wait " +
      "until I finish\", which would be an instruction to use the product more carefully to " +
      "work around a limitation. Latched ONCE PER VOICE SESSION (`RefusalNotices`), not " +
      "per run: cleared-on-heard would mean one line after every single spoken answer, " +
      "forever, about a condition nothing he does can change. " +
      "AND SINCE 2026-09-17 IT SPENDS ONE BUDGET WITH THE THREE RECOGNIZER LINES. Ray's " +
      "candidate-.6 walk found THREE cards stacked on the first spoken answer of a voice " +
      "session while the CEO had said nothing, because each of the three latches honored " +
      "\"at most once\" independently, in two different threads. This is the strongest " +
      "sentence in that family, so it is the one that stands and the other two cannot add " +
      "themselves on top of it.",
  },
  {
    s: "I didn't catch that, so I haven't sent anything. Please say it again. I'm still listening.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: "shell",
    why:
      "ACTIONABLE SINCE 2026-10-21, with its two siblings: a voice WAS measured, so his words " +
      "are lost unless he says them again, and the line now says so; the open microphone " +
      "(◉, #talk-toggle) is the control. The class and its check: " +
      "docs/development/failure-classes/lost-input-without-a-next-step.md. The history " +
      "before that change follows. " +
      "`VoiceNotice::DidNotCatchThat`, added 2026-09-17 for audit-3 §4 #4. Whisper RAN and " +
      "returned non-speech noise — `(clears throat)` — which `stt::is_meaningful` refuses. " +
      "It closes an ASYMMETRY between the two refusal paths that sit side by side in " +
      "`RecognizerDesk::handle`: the pre-whisper gate in `voiced.rs` spoke on the FIRST " +
      "refusal (`HeardNoVoice`, above), while this path said nothing at all until the THIRD " +
      "in a row (`SoundButNoWords`). So the case where he definitely DID speak — a voice was " +
      "measured, a stronger signal than the pre-whisper path ever has — was the quieter of " +
      "the two. Ray walked it on the CEO's screen at frame `a3-15`: a 2.3 s utterance, and " +
      "\"the window did not change at all\", leaving him unable to tell \"not heard\" from " +
      "\"heard and discarded\" from \"broken\". " +
      "INFORMATIONAL for the reason its two neighbors are: it invents no control, because " +
      "the affordance for saying it again IS the open microphone and there is no button to " +
      "point at — so it ends with a status, in the same three words `HeardNoVoice` ends " +
      "with, rather than an imperative aimed at a reader. It is latched per run of discards " +
      "and cleared by an admitted utterance that was also UNDERSTOOD, so it states the " +
      "discard once and never becomes a drip; at the third discard `SoundButNoWords` takes " +
      "over and this line stands aside, so a run produces two sentences in total and never " +
      "two at once. Since 2026-09-17 it also cannot follow `HeardNoVoice` for one noise " +
      "(audit-5 #8, the double card) or follow the half-duplex line at all: the four share " +
      "one budget and only a STRICTLY stronger sentence replaces the one standing. It rides " +
      "`rich://voice-notice`, not `rich://voice-error`: nothing failed and voice did not " +
      "stop, and the notice listener in main.js is the only one of the four that is not " +
      "suppressed when the voice panel is closed.",
  },

  {
    s: "My ears aren't installed on this machine yet. Whoever set RichOS up adds those. I can still read what you type.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    fixture: null,
    why:
      "Needs a real SttError::BinaryNotFound from a live pipeline; the mock bridge does not " +
      "reach richos-voice. The fallback it names (typing) is the composer, always present.",
  },
  {
    s:
      "My speech model isn't on this machine yet. I can fetch it myself now, so I'll offer " +
      "to download it the next time you turn voice on. I can still read what you type.",
    c: "INFORMATIONAL",
    why:
      "SttError::ModelNotFound (stt.rs:112), split from BinaryNotFound's row above on " +
      "2026-09-17 — the nightly QA audit's §D2 — the day it stopped being true that nobody " +
      "but 'whoever set RichOS up' could close this gap. `crate::provision` (landed " +
      "516975db) fetches the model itself, so this one names no party and states what " +
      "happens next instead, phrased like the `Basis::Unmeasured` row above: a retry tied " +
      "to the ◉ voice toggle already on screen, never a command aimed at the reader. Needs " +
      "a real SttError::ModelNotFound from a live pipeline; the mock bridge does not reach " +
      "richos-voice, same as the row above.",
  },
  {
    s: "Something about my hearing changed on this machine and I'd rather not guess at what you said than get it wrong. Whoever set RichOS up can put it right. I can still read what you type.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    fixture: null,
    why:
      "`SttError::ToolchainRefused` at stt.rs:74, and DIFFERENT WORDS from the row above on " +
      "purpose — its own comment says why: the transcriber is installed and refused, so " +
      "'not installed yet' would be false and would send whoever helps him to install " +
      "something already there. Same class, same named party, same always-present fallback. " +
      "No fixture for the same reason as its neighbour: the mock bridge does not reach " +
      "richos-voice, so only a live pipeline produces this variant.",
  },
  {
    s: "My voice isn't working on this machine. Whoever set RichOS up would need to look at that. I'll keep answering in text.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    fixture: null,
    why: "Same: a real TtsError from a live pipeline, unreachable from the browser harness.",
  },


  // -------------------------------------------------------------------------------------
  // INFORMATIONAL
  // -------------------------------------------------------------------------------------
  {
    s: "Active time from when Rich accepted the message to when the turn ended.",
    c: "INFORMATIONAL",
    why: "The duration row's accessible description. States a measure; asks nothing.",
  },
  {
    s: "Added while Rich was working",
    c: "INFORMATIONAL",
    why: "§9.2's cue on a steering bubble. A fact about ordering.",
  },
  {
    s: "Anything I'd written is above. Nothing of yours was lost.",
    c: "INFORMATIONAL",
    why:
      "§14's card when there is no message to restore. Deliberately carries NO control: a " +
      "that puts nothing back is worse than none.",
  },
  {
    s: "Press Restore my message to put your words back in the box.",
    c: "ACTIONABLE",
    control: ".tl-intervention button.tl-intervention-action",
    fixture: "failed-turn",
    why: "The fallback's note names the restoration control. Pressing it restores the original text without sending it.",
  },
  {
    s: "How long it ran was not recorded: the turn ended without writing an end time.",
    c: "INFORMATIONAL",
    why: "A statement about what the ledger holds. Nothing anyone can do.",
  },
  {
    s: "How long this took was not recorded.",
    c: "INFORMATIONAL",
    why: "A completed turn whose activeMs never made it to disk. Nothing recovers it after the fact.",
  },
  {
    s: "This turn was still running when RichOS last closed and nothing recorded how it ended.",
    c: "INFORMATIONAL",
    why: "The duration row's tooltip. The card beneath it is where the action lives.",
  },
  {
    s: "I've noted that you stopped this. I couldn't interrupt the work already in flight, so it may finish on its own, but nothing new will start.",
    c: "INFORMATIONAL",
    why:
      "Reports a durable fact and claims nothing further. Correctly offers no control: the stop " +
      "IS recorded, and pressing it again would change nothing.",
  },
  {
    s: "Your stop is recorded. Rich is letting go of this turn.",
    c: "INFORMATIONAL",
    why: "The stopping row's description. In flight; nothing to press.",
  },
  { s: "You stopped it", c: "INFORMATIONAL", why: "§6.1's attribution label." },
  { s: "You stopped it.", c: "INFORMATIONAL", why: "The §18 announcement of the same." },
  {
    s: "Stopped before it finished",
    c: "INFORMATIONAL",
    why: "The duration label. The failure card renders beside it and carries the control.",
  },
  {
    s: "Rich started working",
    c: "INFORMATIONAL",
    why: "§18's once-per-turn announcement.",
  },
  { s: "Rich is speaking", c: "INFORMATIONAL", why: "Voice state. #voice-barge-in sits beside it anyway." },
  { s: "Rich reached out", c: "INFORMATIONAL", why: "§5.2 proactive marker." },
  {
    s: "The spawn was accepted. Nothing has reported back yet.",
    c: "INFORMATIONAL",
    why:
      "Worker state note. FLAGGED FOR URBAN, not changed here: 'spawn' is machinery vocabulary " +
      "on a §7.2 surface the CEO reads, and that is a copy decision, not an affordance defect.",
  },
  {
    s: "This run is open: no end has been recorded for it.",
    c: "INFORMATIONAL",
    why: "Worker state note.",
  },
  {
    s: "This run has ended. Nothing recorded whether the work finished, stopped or failed, so I'm not going to call it either way.",
    c: "INFORMATIONAL",
    why: "The `unknown` worker note. Refuses a verdict; asks nothing.",
  },
  {
    s: "This run has ended. Nothing recorded whether the work finished, was cut short or failed, so I'm not going to call it either way.",
    c: "INFORMATIONAL",
    why: "The inspector's copy of the same statement.",
  },
  {
    s: "This run was first seen already underway, so no display name was recorded for it.",
    c: "INFORMATIONAL",
    why: "Explains an absent name.",
  },
  {
    s: "This worker reported a state RichOS does not know how to read.",
    c: "INFORMATIONAL",
    why: "The unrecognized-state fallback. Nothing for the CEO to do about a protocol surprise.",
  },
  {
    s: "I don't have this worker's brief or its output. The files it touched are in Output.",
    c: "INFORMATIONAL",
    why:
      "§7.2's honest empty pane, shortened by the Output side panel PRD (§12.10) to what is still " +
      "true once the output record exists: the files a worker touched are listed under its name in " +
      "the Output panel, one click away on the Output buttons. Nothing for the CEO to do.",
  },

  // ---- THE OUTPUT PANEL (Output side panel PRD §6, slice S4; `output-panel.js`) -------------
  // `tests/output.js` renders every one of these. The shell's own sentences that the panel also
  // shows (`MISSING`, `RECORD_UNREADABLE` in `src-tauri/src/output_files.rs`) are S3's rows.
  {s: "Output: nothing produced yet in this thread", c: "CONTROL", why: "The accessible name of both Output buttons (`#out-top`, `#out-bottom`) on a thread with no recorded files; with files it is `Output — N files from this thread`. Pressing either opens the panel."},
  {s: "Everything this thread produced (⌘⇧O)", c: "CONTROL", why: "The pointer tooltip of both Output buttons, naming the shortcut. The name is the aria-label, never this (control-names.js)."},
  {s: "Close the output panel", c: "CONTROL", why: "The accessible name of the panel's ✕ (`#op-close`)."},
  {s: "All output ·", c: "CONTROL", why: "The head of the file view's way back to the list (`#of-back`), followed by the thread's file count."},
  {s: "Wrote 1 file", c: "CONTROL", why: "The turn's own *Wrote N files* button in its duration row; it opens the Output panel on that turn's group. `Wrote N files` for more than one."},
  {s: "Everything written here, newest first.", c: "INFORMATIONAL", why: "The panel's subline under `N files from this thread`. It describes the list; it asks nothing."},
  {s: "Nothing produced yet", c: "INFORMATIONAL", why: "The panel's title on a thread that produced no file, beside the empty sentence. Nothing to do: the list fills as files are written."},
  {s: "No longer where it was written", c: "INFORMATIONAL", why: "A recorded file that is not at its path now (§4.6): the dimmed row's line, and the file view's subline. The file view's sentence (S3's `MISSING`) says what he can do, and it is his choice."},
  {s: "from this thread", c: "FRAGMENT", why: "Follows `N files` (or `1 file`) in the panel's title, the buttons' names and the count announcement."},
  {s: "no longer where", c: "FRAGMENT", why: "Part of the panel title's `· N no longer where it was written` (or `they were`), when any recorded file is missing. INFORMATIONAL as a whole."},
  {s: "has produced a file yet. The moment Rich or the team writes one, it is listed here and the count on the Output button says so.", c: "FRAGMENT", why: "The tail of the empty sentence, after `Nothing in <thread>`. INFORMATIONAL as a whole: it says when the list will fill and asks nothing."},
  // ---- THE OUTPUT PANEL'S PREVIEWS (Output side panel PRD §7, slice S5; `output-panel.js`) -------
  // `tests/output.js` renders every one of these. The §6.7 sentences the viewer shows verbatim from
  // `output_preview` are the shell's rows (`MISSING`, `LINKED`, `NOT_A_FILE` below, and the
  // `too_large` / `no_preview` / `read_failed` sentences `output_files.rs` builds with format!).
  {s: "I couldn't show this file here.", c: "FRAGMENT", why: "The head of `I couldn't show this file here. Open in <app> has it.` (or `Open has it.` when Launch Services names no app), said in the file view when the webview could not draw a picture, a page or a recording the shell served. INFORMATIONAL as a whole: it states the outcome and where the file opens; the Open control is S6's, and nothing here asks him to act. `tests/output.js` renders it."},
  {s: "Showing the first 2 MB", c: "INFORMATIONAL", why: "Under a Markdown or text preview the shell capped at 2 MiB (§7, §6.9: a cap, not a page). States how much is shown; asks nothing. `tests/output.js` renders it."},
  {s: "Showing the first", c: "FRAGMENT", why: "The head of `Showing the first 200 rows` under a CSV the shell capped at 200 rows (§7). INFORMATIONAL as a whole. `tests/output.js` renders it."},
  {s: "Preview or source", c: "CONTROL", why: "The accessible name of the Preview | Source switch's group in the file view's tools row (§6.4, §11 row 11), for Markdown and plain text."},
  {s: ", the first rows", c: "FRAGMENT", why: "The tail of the accessible name of a CSV preview's scrolling table region, `<file name>, the first rows`. Read by a screen reader as the region's name; asks nothing."},
  {s: "The first page of", c: "FRAGMENT", why: "The head of the alt text of a QuickLook rendition, `The first page of <file name>` (§7: a rendition is one image of the first page). Read by a screen reader in place of the picture; asks nothing."},
  {s: "Rich text document", c: "INFORMATIONAL", why: "The first part of a QuickLook rendition's facts line for an .rtf file, beside `Word document`, `Pages document`, the spreadsheets and presentations (§6.4's facts line, §7). Names the kind; asks nothing."},
  {s: "· also written earlier in this thread", c: "FRAGMENT", why: "Appended to the file view's subline when the file's first witnessed write was in an earlier turn. INFORMATIONAL."},
  {s: "Between turns ·", c: "FRAGMENT", why: "The tooltip of the group of files written outside any turn, followed by the time. The group's visible head is `Between turns`. INFORMATIONAL."},

  // ---- THE WIDE PULL (Output side panel PRD §9, slice S9; `output-panel.js` "THE WIDE PULL") -----
  // `tests/output.js` renders every one of these ("stop and snap", "keys", "the pill", "the
  // floating composer is THE composer").
  {s: "Output panel width", c: "CONTROL", why: "The accessible name of the panel's divider (`#op-resizer`, role separator): drag it or use ← → End Home; its value says the width, the stop and open completely."},
  {s: "Drag to resize; pull past the stop to open it completely", c: "CONTROL", why: "The divider's pointer tooltip. The name is the aria-label, never this (control-names.js)."},
  {s: "Show the conversation (⌘⇧O closes the panel)", c: "CONTROL", why: "The pointer tooltip of the ‹ <thread> pill shown while the panel is open completely; the pill brings the conversation back at the stop."},
  {s: "Show the conversation", c: "CONTROL", why: "The accessible name of the ‹ <thread> pill (`#op-conv`), followed by ` — <thread>`; pressing it brings the conversation back at the stop."},
  {s: "pixels, at the stop; pull on to open it completely", c: "FRAGMENT", why: "The divider's spoken value at the stop, after the width in pixels. INFORMATIONAL as a whole: it says where the divider is and what pulling on does; the divider itself is the control."},
  {s: "Open completely. The conversation is one click back and closing the panel brings it all back.", c: "INFORMATIONAL", why: "Announced once in the polite live region when the pull opens the panel completely (round 17.1's notice). It names the ways back, which are on screen (the pill and ×); nothing is asked."},
  {s: "The conversation is back.", c: "INFORMATIONAL", why: "Announced once in the polite live region when the conversation returns from open completely. Nothing to do."},
  {
    s: "I'm Rich, your chief of staff. Tell me what you're working on and I'll take it from there.",
    c: "INFORMATIONAL",
    why:
      "First-run greeting. An invitation, not a state he could change, and it names the " +
      "control it invites (#input).",
  },
  {
    s: "You can type or tap ◉ to talk to me.",
    c: "INFORMATIONAL",
    why:
      "The voice half of the first-run greeting, SPLIT OFF on 2026-09-04 and appended only " +
      "when `voice_readiness` says this machine can transcribe. It was one sentence with " +
      "the line above, and on a fresh Mac with no speech model it named a control that " +
      "opened a hot microphone, said \"listening…\" and never transcribed. A sentence that " +
      "names a control is a promise the control works, so it is now withheld with the " +
      "control rather than shipped beside its absence.",
  },
  {
    s: "Talking out loud needs the desktop app. Here in the preview, type to me.",
    c: "INFORMATIONAL",
    why:
      "Browser-preview only: reachable solely when Bridge.isMock. Never renders in the shipped " +
      "app, and the composer it points at is on screen.",
  },
  { s: "Edited a file", c: "INFORMATIONAL", why: "§5.3 activity rollup label." },
  { s: "Ran a command", c: "INFORMATIONAL", why: "§5.3 activity rollup label." },
  { s: "Set up the environment", c: "INFORMATIONAL", why: "§5.3 activity rollup label." },
  { s: "Updated a thread", c: "INFORMATIONAL", why: "§5.3 activity rollup label." },
  { s: "Used an integration", c: "INFORMATIONAL", why: "§5.3 activity rollup label." },
  { s: "Used the web", c: "INFORMATIONAL", why: "§5.3 activity rollup label." },
  { s: "Viewed an image", c: "INFORMATIONAL", why: "§5.3 activity rollup label." },
  { s: "name not recorded", c: "INFORMATIONAL", why: "Worker chip qualifier." },
  { s: "outcome not recorded", c: "INFORMATIONAL", why: "Worker chip qualifier." },
  { s: "new result ready", c: "INFORMATIONAL", why: "Rail mark: navigational, nothing broken." },
  { s: "Time spent working", c: "INFORMATIONAL", why: "Inspector field label." },

  {
    s: "Shown at the foot of the rail, with your initials.",
    c: "INFORMATIONAL",
    why:
      "Sits under the 'Your name' field and says what setting it does. It is not a request — " +
      "the imperative is on the rail row that sent him here, and this line is the consequence " +
      "of the field he is already looking at.",
  },

  // -------------------------------------------------------------------------------------
  // CONTROL — the string is an affordance, not a state that needs one
  // -------------------------------------------------------------------------------------
  { s: "+ New chat or company", c: "CONTROL", why: "#rail-new-thread since round 32: opens #new-chat-menu." },
  {
    s: "New chat or company",
    c: "CONTROL",
    why: "The accessible name of #new-chat-menu, the button's own menu (index.html and openNewChatMenu); it names the control he opened.",
  },
  {
    s: "No chats yet",
    c: "INFORMATIONAL",
    why: "Under a company with no chats in the rail (round 32). Nothing is asked of him; the heading's plus starts one.",
  },
  {
    s: "menu new-chat-menu is-form",
    c: "NOT-RENDERED",
    why: "A class list main.js sets on #new-chat-menu when it shows the company form; never text on screen.",
  },
  {
    s: "nav-thread-row is-draft is-active",
    c: "NOT-RENDERED",
    why: "A class list main.js sets on a new chat's \"New chat\" row in the rail; never text on screen.",
  },
  { s: "Start a new thread instead", c: "CONTROL", why: "#unbound-new-thread — §21's way out." },
  {
    s: "Restore my message",
    c: "CONTROL",
    why:
      "`timeline.js`'s RETRY_LABEL — the one control on both intervention cards. It read " +
      "\"Pick it back up\" until the 2026-09-17 nightly's D6 measured what pressing it did: " +
      "his text went back in the composer, with no new turn and no log line. The behavior " +
      "is right (resending spends his subscription and starts work, so it is his to " +
      "trigger) and the words were the defect, so the label is now what it does.",
  },
  { s: "Jump to latest", c: "CONTROL", why: "#jump-latest accessible name." },
  { s: "Close worker details", c: "CONTROL", why: "#inspector-close accessible name." },
  { s: "open worker details", c: "CONTROL", why: "Worker chip accessible name." },
  { s: "Copy Rich's message", c: "CONTROL", why: "Copy button accessible name." },
  { s: "Copy your message", c: "CONTROL", why: "Copy button accessible name." },
  { s: "Hide what Rich did.", c: "CONTROL", why: "Duration-row disclosure accessible name." },
  { s: "Show what Rich did.", c: "CONTROL", why: "Duration-row disclosure accessible name." },
  { s: "Show what Rich did:", c: "CONTROL", why: "Collapsed-summary button accessible name." },
  {
    s: "<kbd>Enter</kbd> to save · <kbd>Esc</kbd> to cancel",
    c: "ACTIONABLE",
    control: ".nav-rename-input",
    fixture: null,
    why: "Round 35's hint under a chat's rename field (main.js `startRename`): the two keys that end the rename, pressed in the field directly above it.",
  },
  {
    s: 'A chat needs a name.<span class="nav-rename-sub"><kbd>Esc</kbd> keeps the old one.</span>',
    c: "ACTIONABLE",
    control: ".nav-rename-input",
    fixture: null,
    why: "Round 35's refusal of an empty title (main.js `refuseRename`), under the field it refuses: he types a name there, or presses Esc in it to keep the old one.",
  },
  { s: "Restore from archive", c: "CONTROL", why: "Thread overflow menu item." },
  { s: "Entities and threads", c: "CONTROL", why: "#rail accessible name." },
  { s: "Left navigation width", c: "CONTROL", why: "#rail-resizer accessible name." },
  { s: "Hide the sidebar", c: "CONTROL", why: "#rail-toggle accessible name while the sidebar shows (output side-panel PRD §8, S8); `setRailOpen` keeps it true." },
  { s: "Hide the sidebar (⌘⇧S)", c: "CONTROL", why: "#rail-toggle title while the sidebar shows, naming its shortcut (S8)." },
  { s: "Show the sidebar", c: "CONTROL", why: "#rail-toggle accessible name while the sidebar is away (S8): the way back, always in the conversation's header." },
  { s: "Worker panel width", c: "CONTROL", why: "#inspector-resizer accessible name." },
  { s: "Under the hood", c: "CONTROL", why: "Slide-over title and its trigger." },
  { s: "back to Rich", c: "CONTROL", why: "#slideover-close." },
  { s: "How much should Rich interrupt you?", c: "CONTROL", why: "The interruption choice's title over its radios, in the Settings panel's Working with Rich column (round 27)." },
  { s: "Only when it's urgent", c: "CONTROL", why: "Assertiveness radio label." },
  { s: "Show it when RichOS starts", c: "CONTROL", why: "The splash screen's off switch in the Settings panel's foot band (CEO 2026-10-06 item 7, round 27): a checkbox label, and the control IS the state." },
  { s: "Oxford comma: OFF", c: "CONTROL", why: "The Oxford comma switch's heading while it is off (CEO 2026-10-19, round 33; settings-button.js): the label of the switch beside it, and the control IS the state." },
  { s: "Oxford comma: ON", c: "CONTROL", why: "The same switch's heading while it is on (CEO 2026-10-19): the label of the switch beside it." },
  {
    s: 'Oxford comma AKA serial comma is what some style guides place before "and" and "or". RichOS recommends to keep that nuisance off.',
    c: "INFORMATIONAL",
    why: "His explainer under the Oxford comma switch, word for word (CEO 2026-10-19): it says what the switch above it is, and the switch is the control.",
  },
  { s: "Which company is this work in?", c: "CONTROL", why: "Entity picker title over its list. \"company\", never \"entity\" — Ray candidate-.11 §3.5." },
  { s: "Search entities, threads and conversations…", c: "CONTROL", why: "#search-input placeholder." },
  { s: "Talk to Rich", c: "CONTROL", why: "#talk-toggle title." },
  {
    s: "Message to Rich",
    c: "CONTROL",
    why:
      "#input's accessible name — candidate-.2 defect #7. The box had none and fell back to " +
      "its PLACEHOLDER, which is state: it reads 'Send is off until I know the company' while " +
      "a company block is up, so the control was renamed underneath a screen-reader user as " +
      "the app changed condition. The name holds still now and the placeholder carries the " +
      "state. Not 'Talk to Rich', which is #talk-toggle's name three rows up: two controls " +
      "with one name is the thing a screen reader cannot tell apart.",
  },
  { s: "Talk to Rich…", c: "CONTROL", why: "#input idle placeholder." },
  { s: "Add context or steer Rich…", c: "CONTROL", why: "#input working placeholder (§9.2)." },
  { s: "more threads in", c: "CONTROL", why: "'Show more' accessible name." },

  // -------------------------------------------------------------------------------------
  // FRAGMENT — half of a composed sentence, never rendered alone (blind spot B1)
  // -------------------------------------------------------------------------------------
  {
    s: "Everything I'm holding for",
    c: "FRAGMENT",
    why: "Entity overview line, completed with the entity name.",
  },
  {
    s: "Nothing here yet. I'll keep work for",
    c: "FRAGMENT",
    why: "Empty-entity line, completed with the entity name and 'in this area.'",
  },
  { s: "in this area.", c: "FRAGMENT", why: "The tail of the empty-entity line." },
  { s: "Talk to Rich about", c: "FRAGMENT", why: "Composer scope prefix, completed with the entity name." },
  {
    s: "I couldn't get that to my desk just now and nothing is running.",
    c: "FRAGMENT",
    why:
      "The generic first half of a refused send, used only when the backend supplied no " +
      "sentence of its own. Always rendered followed by the 'back in the box' half, which is " +
      "classified ACTIONABLE and carries the control.",
  },
  {
    s: "Press Stop again and I'll have another go.",
    c: "FRAGMENT",
    why: "Appended to the backend's own sentence in the failed-stop notice.",
  },
  // A PRESSED BUTTON'S SEND THAT WAS REFUSED (product hunt 2026-10-15, finding 14). The only
  // caller that passes `send(…, { pressed })` is `startFirstRunInterview` (main.js), with the
  // label of `#first-run-start` ("Start the questions" / "Resume the questions"), so each
  // sentence below is completed with that label. His words never go in the box: `send`
  // returns false and the caller re-renders the first-run offer (`renderFirstRunNotice`),
  // which sits inside `#conversation`, so the button the sentence names is back in the same
  // view. Each whole sentence is ACTIONABLE against `#first-run-start`.
  {
    s: "I couldn't start that thread just now. Press",
    c: "FRAGMENT",
    why:
      "Head of `#composer-blocked`'s line when creating the thread for a pressed button failed: " +
      "'I couldn't start that thread just now. Press <label> to try again.' ACTIONABLE as a " +
      "whole against `#first-run-start`, which the refused send brings back.",
  },
  {
    s: "to try again.",
    c: "FRAGMENT",
    why: "Tail of the same `#composer-blocked` line, after the pressed button's label.",
  },
  {
    s: "again when the setting up is done.",
    c: "FRAGMENT",
    why:
      "Tail of the local notice when a pressed button's send was refused while setup still " +
      "asks for something: '<reason> Press <label> again when the setting up is done.' The " +
      "setup ask is opened beside it (`maybeAskAboutSetup`). ACTIONABLE as a whole against " +
      "`#first-run-start`, which the refused send brings back.",
  },
  {
    s: "when you want me to try again.",
    c: "FRAGMENT",
    why:
      "Tail of the same local notice with nothing left to set up: '<reason> Press <label> when " +
      "you want me to try again.' ACTIONABLE as a whole against `#first-run-start`, which the " +
      "refused send brings back.",
  },
  { s: "You stopped after", c: "FRAGMENT", why: "Completed with a duration." },
  { s: "Set up the environment ( steps)", c: "FRAGMENT", why: "Template hole: the step count." },
  { s: "Used the web times", c: "FRAGMENT", why: "Template hole: the count." },
  { s: "Worked ( steps)", c: "FRAGMENT", why: "Template hole: the step count." },
  {
    s: "What Rich did",
    c: "CONTROL",
    why:
      "The label of a turn's disclosure button (`.tl-duration-btn`, timeline.js " +
      "`renderDurationRow`) when the turn has work to show and no measured time: it names what " +
      "the button opens instead of reading 'Worked' (product hunt 2026-10-15, finding 13).",
  },
  { s: "are no longer running", c: "FRAGMENT", why: "Worker group verb, completed with names." },
  { s: "is no longer running", c: "FRAGMENT", why: "Worker group verb, completed with a name." },
  { s: "headphones recommended · tap", c: "FRAGMENT", why: "Voice footnote, completed by the ◉ glyph." },
  { s: "to end voice", c: "FRAGMENT", why: "The tail of the same footnote." },


  // -------------------------------------------------------------------------------------
  // THE CORRECTION DESK (§7 "ask, never infer") — RICH-TODOs row 5b
  //
  // Two families on one surface, and the classification turns on a distinction the desk
  // itself is built around: `loro_available` false is a statement about THIS INSTALL that
  // nobody in the app can change (NEEDS-SOMEONE-ELSE), while a desk that is present and
  // did not answer is transient and carries a retry (ACTIONABLE). Rendering either as an
  // empty list would say "nothing to correct", which is the one thing neither means.
  // -------------------------------------------------------------------------------------
  {
    s: "This install has no company memory it can write to, so there is nothing to read or correct here. That is a statement about this install, not about what is recorded.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "Switching that on is a job for whoever set RichOS up. There is no control for it in here.",
    fixture: "corrections-off",
    why:
      "The loro desk's own refusal (main.rs, `desk()`), relayed verbatim and never reworded. " +
      "It names no party by itself, so the surface appends the owner line in the SAME " +
      "paragraph rather than paraphrasing the backend — the backend says what is missing, " +
      "the UI says who can do something about it, and neither guesses the other's half.",
  },
  {
    s: "I can't record corrections right now: my correction log could not be opened. Nothing you say is being lost from the conversation itself.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "Switching that on is a job for whoever set RichOS up. There is no control for it in here.",
    fixture: "corrections-off",
    why: "The spoken desk's refusal (main.rs, `spoken_desk()`), same relay and same owner line.",
  },
  {
    s: "Switching that on is a job for whoever set RichOS up. There is no control for it in here.",
    c: "NEEDS-SOMEONE-ELSE",
    fixture: "corrections-off",
    why:
      "The owner half of both sentences above, and the only part of an unavailable desk this " +
      "UI authors. It names the party the regex requires and promises no control, because " +
      "configuring a corpus or a service binary is not reachable from any screen in the app.",
  },
  {
    s: "I couldn't read that just now. Nothing has changed.",
    c: "ACTIONABLE",
    control: ".desk-broke:not([hidden]) .desk-btn",
    fixture: "corrections-read-failed",
    why:
      "The other unavailable state: the desk said it was there (`loro_available` true) and " +
      "then a read refused. Transient in the common case, so the retry sits inside the block; " +
      "where the reason is NOT transient the backend's own sentence renders directly beneath " +
      "and names its owner (see the entity row above).",
  },
  {
    s: "Nothing about the company record is waiting on you.",
    c: "INFORMATIONAL",
    fixture: "corrections-empty",
    why:
      "A readable desk with nothing pending — and it renders ONLY when the read succeeded, " +
      "which is why it is a different element from the two unavailable states rather than " +
      "the same empty list serving all three.",
  },
  {
    s: "No word is waiting on you.",
    c: "INFORMATIONAL",
    fixture: "corrections-empty",
    why: "The spoken half of the same fact, under its own availability check.",
  },
  {
    s: "What I believe",
    c: "INFORMATIONAL",
    fixture: "corrections",
    why: "Section heading over the loro family — what the company record says, in the CEO's terms rather than 'loro'.",
  },
  {
    s: "Words I may have got wrong",
    c: "INFORMATIONAL",
    fixture: "corrections",
    why: "Section heading over the spoken family — a word Rich mis-transcribed, not a belief.",
  },
  {
    s: "Never ask again",
    c: "ACTIONABLE",
    control: ".desk-suppressed:not([hidden]) .desk-btn--lift",
    fixture: "corrections-loro-never",
    why:
      "§7 requires the suppression list to be inspectable 'or a term silently refuses to " +
      "learn with no way to see why', and a list you can see and cannot clear is only half " +
      "of that. Every row under this heading carries its own lift button, so the heading is " +
      "classified by what the CEO can still do about what is under it.",
  },
  {
    s: "Because you said:",
    c: "INFORMATIONAL",
    fixture: "corrections",
    why:
      "Labels the CEO's OWN words — the loro proposal's `why`, or the utterance the spoken " +
      "trigger fired on. A correction with no stated reason is the shape an inferred one " +
      "takes, so this label is what makes the difference visible on the card.",
  },
  {
    s: "What would be written, exactly:",
    c: "INFORMATIONAL",
    fixture: "corrections",
    why:
      "Labels the writer's own `--dry-run` bytes. 'Exactly' is a claim the card can make " +
      "because the preview is `WriteOutput.text` and not a description of it " +
      "(`correction.rs:366-369`).",
  },
  {
    s: "Yes, that's right",
    c: "CONTROL",
    fixture: "corrections",
    why: "The confirm button on a loro proposal — the only path in this application to a loro write.",
  },
  {
    s: "Never ask about this record",
    c: "CONTROL",
    fixture: "corrections",
    why: "§7's third outcome on a loro proposal: suppress this ref, on a list that reads back and lifts.",
  },
  {
    s: "Show me what's on record now",
    c: "CONTROL",
    fixture: "corrections",
    why:
      "`loro_show_record`. Reading is not correcting (`correction.rs:592-595`), so it needs " +
      "no proposal — and it is absent on an append, where there is no prior record to show.",
  },
  {
    s: "Yes, learn it",
    c: "CONTROL",
    fixture: "corrections",
    why: "The confirm button on a spoken candidate — the only path from a staged pair to the vocabulary.",
  },
  {
    s: "Never ask about this term",
    c: "CONTROL",
    fixture: "corrections",
    why: "§7's third outcome on a spoken candidate.",
  },
  {
    s: "Ask about this again",
    c: "CONTROL",
    fixture: "corrections-loro-never",
    why: "Lifts one suppression. The half of 'inspectable' that a read-only list would be missing.",
  },
  {
    s: "Done. That's what I have on record now.",
    c: "INFORMATIONAL",
    fixture: "corrections-written",
    why:
      "The confirmed write landed. Said only when the returned proposal is in state `written` " +
      "— never optimistically, because the same call can come back `failed`.",
  },
  {
    s: "I said yes to that and the write didn't land, so nothing changed. Here is exactly what my writer said:",
    c: "INFORMATIONAL",
    fixture: "corrections-write-failed",
    why:
      "He confirmed and the writer refused. Nothing to press: `confirm` refuses a proposal " +
      "that is not awaiting an answer (`correction.rs:541`), so the card is spent. The " +
      "writer's own sentence follows verbatim on the next line and is often an instruction to " +
      "HIM ('that is a PROSE section — edit the page'), which is exactly why it is relayed " +
      "rather than reworded.",
  },
  {
    s: "Left it alone. I'll ask again if it comes up.",
    c: "INFORMATIONAL",
    fixture: "corrections-loro-declined",
    why:
      "§7: a plain decline is NOT permanent, because a decline is ambiguous — not a record / " +
      "not now / misclicked — while a repeat is the evidence. The sentence says so rather " +
      "than letting him assume he has settled it.",
  },
  {
    s: "I won't ask about that record again. It's in the list below if you change your mind.",
    c: "ACTIONABLE",
    control: "#desk-loro-suppressed-list .desk-btn--lift",
    fixture: "corrections-loro-never",
    why:
      "A permanent decline that vanished would be a lost correction. It names where the " +
      "suppression went, and the lift control is in that list in the same view.",
  },
  {
    s: "Learned. I'll write it that way from now on.",
    c: "INFORMATIONAL",
    fixture: "corrections-learned",
    why: "`LearnOutcome.changed` true — the pair reached the vocabulary.",
  },
  {
    s: "I already had that one, so nothing changed.",
    c: "INFORMATIONAL",
    fixture: "corrections-already-knew",
    why:
      "`changed: false` — the vocabulary already knew the pair. A DIFFERENT fact from a " +
      "refusal, and `staging.rs:141-144` says he is entitled to both, so the two are not " +
      "collapsed into one cheerful sentence.",
  },
  {
    s: "Left it alone. I'll ask again the next time you say it.",
    c: "INFORMATIONAL",
    fixture: "corrections-spoken-declined",
    why:
      "The spoken half of the same outcome, and more specific because §7 is more specific " +
      "for words: re-ask on the very next repeat, no threshold and no cool-off, because " +
      "repetition IS the evidence and waiting dilutes it.",
  },
  {
    s: "I won't ask about that word again. It's in the list below if you change your mind.",
    c: "ACTIONABLE",
    control: "#desk-spoken-suppressed-list .desk-btn--lift",
    fixture: "corrections-spoken-never",
    why: "The spoken suppression, and its way back out, in the same view.",
  },
  {
    s: "Back on the table. I'll ask about it if it comes up again.",
    c: "INFORMATIONAL",
    fixture: "corrections-lifted",
    why:
      "A suppression was lifted. It promises a future ask rather than an immediate one, " +
      "because nothing here re-proposes — something has to raise the correction again.",
  },

  // -------------------------------------------------------------------------------------
  // THE FEEDBACK CHANNEL (`feedback.rs`) — the local half, made reachable
  //
  // Almost every sentence on this surface comes from the BACKEND (`feedback_wording`,
  // `feedback_taxonomy`), and `feedback.rs` holds them as constants precisely so a UI
  // cannot paraphrase them — which is why the question, the four key labels, the report
  // offer, the disclosure heading and every term's sentence appear in NO row below. They
  // are not in this inventory because they are not in `index.html`, `main.js` or the
  // command layer; they are projections of one module's constants, and `feedback.js` joins
  // each of them to a fixture a cargo test regenerates from the live Rust values.
  //
  // What IS below is the small amount this surface genuinely authors: four group legends,
  // four button labels, three outcome sentences, two history labels, one section heading,
  // one empty line, and the three refusals the command layer writes.
  // -------------------------------------------------------------------------------------
  {
    s: "I can't keep an answer right now: the file I record them in wouldn't open and I'm not going to ask you what you think and then lose it. That one is for whoever set RichOS up to look at; it isn't yours to fix.",
    c: "NEEDS-SOMEONE-ELSE",
    fixture: "feedback-unavailable",
    why:
      "`FEEDBACK_STORE_UNAVAILABLE`. The one file would not open, which nothing in the app " +
      "can change, so it names the party who can and offers no control. The four keys are " +
      "NOT rendered beside it: asking him what he thinks and then dropping the answer is " +
      "worse than not asking, and a set of buttons that record nothing would be exactly that.",
  },
  {
    s: "What's on this machine",
    c: "INFORMATIONAL",
    fixture: "feedback",
    why:
      "The history section's heading, and a deliberate statement of scope rather than a " +
      "neutral label — 'this machine' is the whole of where an answer goes in this version.",
  },
  {
    s: "Nothing is recorded here yet. RichOS never puts the question to you on its own. This panel is the only place it is asked.",
    c: "INFORMATIONAL",
    fixture: "feedback",
    why:
      "A readable store holding nothing — and it renders ONLY when the read succeeded, " +
      "which is why it is a different element from the store-would-not-open state. The " +
      "second sentence exists because an empty history could otherwise read as 'you had " +
      "nothing to say'; the truthful reason is that nothing has ever asked him, and this " +
      "build deliberately never will (`feedback.rs`: all five moments in the reference case " +
      "were volunteered mid-work, none at session end).",
  },
  {
    s: "What kind of failure was it?",
    c: "CONTROL",
    fixture: "feedback-choosing",
    why: "The accessible name of the failure-class radio group — a `<legend>` over its own controls.",
  },
  {
    s: "How many times this session?",
    c: "CONTROL",
    fixture: "feedback-choosing",
    why: "The accessible name of the occurrence radio group. Closed, never an integer field.",
  },
  {
    s: "What went wrong",
    c: "CONTROL",
    fixture: "feedback-choosing",
    why: "The accessible name of the diagnosis checkbox group — the terms that compose the report.",
  },
  {
    s: "What let it happen",
    c: "CONTROL",
    fixture: "feedback-choosing",
    why: "The accessible name of the contributing-condition group. Optional, and omitted from the report entirely when empty.",
  },
  {
    s: "Show me exactly what you'd say",
    c: "CONTROL",
    fixture: "feedback-choosing",
    why:
      "#feedback-show-preview. Disabled until the selection can actually be assembled, " +
      "because a control that appears and then refuses teaches him the surface is unreliable.",
  },
  {
    s: "Yes, report that",
    c: "CONTROL",
    fixture: "feedback-previewing",
    why: "#feedback-approve — and it is only reachable after the report has been rendered and read.",
  },
  {
    s: "No, don't report that",
    c: "CONTROL",
    fixture: "feedback-previewing",
    why: "#feedback-refuse. A declined report is not a report: the payload is dropped and nothing about it is recorded.",
  },
  {
    s: "Taken down. It stays on this machine.",
    c: "INFORMATIONAL",
    fixture: "feedback-rated",
    why: "A rating recorded with no offer made — a `3`, or a dismissal. States where it went, and promises nothing else.",
  },
  {
    s: "Taken down, with no report attached.",
    c: "INFORMATIONAL",
    fixture: "feedback-declined-report",
    why: "He was offered the chance to report and said no. Says what was kept, which is the rating and only the rating.",
  },
  {
    s: "Taken down, word for word as you read it and it stays on this machine.",
    c: "INFORMATIONAL",
    fixture: "feedback-approved",
    why:
      "An approved report. 'Word for word as you read it' is a checked claim rather than a " +
      "reassurance: `feedback_record` re-renders the selection and refuses the write if it " +
      "is not byte-identical to the block he was shown.",
  },
  {
    s: "You approved this report:",
    c: "INFORMATIONAL",
    fixture: "feedback-approved",
    why:
      "The label over a stored approval's text, which is re-rendered from the stored payload " +
      "rather than kept as a second free-text copy — the copy would have put an unvalidated " +
      "string in the durable record, which is the channel this design exists to close.",
  },
  {
    s: "You were offered a report and said no.",
    c: "INFORMATIONAL",
    fixture: "feedback-declined-report",
    why:
      "A stored `ReportDecision::Declined`. The offer having been made is part of what " +
      "happened, and a row that showed only the rating would lose it.",
  },

  // -------------------------------------------------------------------------------------
  // WHICH COMPANY THIS COPY OF RICH WORKS FOR (slice 4, 2026-09-01)
  //
  // WHAT IS NOT HERE, AND WHY. The durable SETTING for this — the row in the universal
  // settings menu — lives in `settings-button.js`, which `UI_SOURCES` does not scan
  // (`index.html`, `main.js`, `timeline.js` only). So its four strings are outside this
  // registry, exactly as Techy Mode's and the opening screen's rows in the same file
  // already are. That is a blind spot of the derivation, not an exemption granted here, and
  // it is covered instead by two named checks at the foot of `affordances.js` which drive
  // the real menu and assert the control and the pinned statement. The strings BELOW are
  // the ones the shell itself renders: the launch picker, the composer's block, and the
  // Rust refusals the shell relays verbatim.
  //
  // Every row here exists because a double-clicked bundle has working directory `/`, which
  // owns no entity, so `EntityRegistry::resolve_root` refuses to guess (ECS §3.3 — correct,
  // and unchanged). Before this pass the ONLY routes to an entity were `RICHOS_ENTITY` and
  // a working directory, and a CEO opening an app from Finder has neither. These are the
  // states of the surface that gives him one.
  // -------------------------------------------------------------------------------------
  {
    s: "Choose the company",
    c: "CONTROL",
    why: "The button beneath the composer's block (`#choose-company-btn`). It IS the affordance.",
  },
  {
    s: "Which company is this copy of Rich for?",
    c: "CONTROL",
    why:
      "Two places, one string, and it is a LABEL in both: the picker dialog's title, which " +
      "`aria-labelledby` points the listbox at, and the settings popover's heading, which " +
      "names the radiogroup below it. Neither is a state; each is the accessible name of the " +
      "control it sits above.",
  },
  {
    s: "I don't know which company this work is for yet, so I won't file it anywhere. Pick one and I'll take it from there.",
    c: "ACTIONABLE",
    control: "#choose-company-btn",
    fixture: "company-unchosen",
    why:
      "The composer's block on a launch that resolved no company — the state EVERY " +
      "double-clicked launch is in until he answers once. It is his to clear, so the control " +
      "is rendered directly beneath the sentence rather than described: the same button also " +
      "reopens the picker if he dismissed it.",
  },
  {
    s: "Send is off until I know the company",
    c: "ACTIONABLE",
    control: "#choose-company-btn",
    fixture: "company-unchosen",
    why:
      "The composer's placeholder while a company block is up — candidate-.2 defect #6. The " +
      "box used to read 'Talk to Rich…' over a field that would take his sentence and refuse " +
      "to send it, which `showUnboundView` had already named for the OTHER blocked state: " +
      "'an inviting Talk to Rich… above a dead field is the composer telling a small lie " +
      "about what it will do'. Same treatment, same reason, and the sibling row 'Send is off " +
      "for this thread' is the one this follows. " +
      "ONE STRING FOR THREE BLOCKS, and the class is the one that fits the two he can clear: " +
      "no company chosen and no company known are both his, and `#choose-company-btn` sits " +
      "directly beneath. The third — a company pinned from outside the window that cannot be " +
      "made sense of — is NEEDS-SOMEONE-ELSE, and it is `COMPANY_PINNED_BLOCK`'s own row that " +
      "carries that, rendered in the line immediately above the box and attached to the box " +
      "itself by `aria-describedby`. The placeholder says only what the box will do and names " +
      "neither a party nor an instruction, which is why it does not have to.",
  },
  {
    s: "I'll keep everything you tell me under the company you pick and I'll remember it, so you won't be asked again. You can change it later in Settings.",
    c: "ACTIONABLE",
    control: "#entity-picker-list .picker-item",
    fixture: "company-unchosen",
    why:
      "The picker's second line, shown only when the question is about this COPY of Rich " +
      "rather than about one thread. It exists because the same dialog asked the same-looking " +
      "question on every launch before this pass and remembered nothing; saying the answer is " +
      "kept is the difference between one question and a permanent one. The control is the " +
      "list of companies directly beneath it.",
  },
  // ---- ADDING A COMPANY (2026-09-04) ----------------------------------------------------
  //
  // WHY THESE ROWS EXIST. Until today the picker's rows WERE the registry, the registry was
  // a `const` table of the app author's six companies compiled into the binary, and there
  // was no door anywhere in the product for a second person to add his own. On any machine
  // but his the app offered him a choice among businesses that were not his, or a composer
  // that refused every send. Every string below is part of the door.
  {
    s: "I don't know about any of your companies yet. Tell me one and I'll start keeping its work together. You can add the rest whenever you like.",
    c: "ACTIONABLE",
    control: "#entity-add-name",
    fixture: "company-none-registered",
    why:
      "The picker's opening line on a FIRST LAUNCH, where the list is empty. It is his to " +
      "clear and the field that clears it is directly beneath, which is why this is the " +
      "line rather than a silent empty dialog — a registry-driven picker renders nothing at " +
      "all when the registry is empty, and nothing is not an answer.",
  },
  {
    s: "Not one of these? Add it here.",
    c: "ACTIONABLE",
    control: "#entity-add-name",
    fixture: "company-unchosen",
    why:
      "The same form's lead when he DOES have companies listed. Two sentences rather than " +
      "one, because 'I don't know about any of your companies' is false on an install that " +
      "has five of them, and a line that is false in one of its two states is a line nobody " +
      "reads in either.",
  },
  {
    s: "What's the company called?",
    c: "CONTROL",
    why:
      "The `<label for=\"entity-add-name\">` — the field's accessible name, not a state. It " +
      "is a question because the control is a text field and the field has nothing else to " +
      "say what belongs in it.",
  },
  {
    s: "Its folder on this Mac",
    c: "CONTROL",
    why:
      "The `<label for=\"entity-add-folder\">`. The word 'optional' is inside the same " +
      "label element, so a screen reader announces the field as optional rather than " +
      "leaving that in a hint below it.",
  },
  {
    s: "You can leave this blank.",
    c: "INFORMATIONAL",
    fixture: "company-unchosen",
    why:
      "Beside the folder field: a person who does not know his own folder path must not stop " +
      "here. It promises nothing about the folder (product hunt 2026-10-15, finding 16: the " +
      "old sentence promised the company would be picked whenever RichOS started from inside " +
      "the folder, which a Mac app opened from the Dock or Finder never does).",
  },
  {
    s: "Add this company",
    c: "CONTROL",
    why: "The button's label (`#entity-add-go`). It IS the affordance the two rows above name.",
  },
  {
    s: "I don't know about any of your companies yet, so I've nothing to file this under. Tell me one and I'll take it from there.",
    c: "ACTIONABLE",
    control: "#choose-company-btn",
    fixture: "company-none-registered",
    why:
      "The composer's block on a first launch, and a SEPARATE sentence from 'Pick one and " +
      "I'll take it from there' — because 'pick one' is not an instruction a person with an " +
      "empty list can follow. The control reopens the picker, where the form that clears it " +
      "lives.",
  },
  {
    s: "Your list of companies is saved at",
    c: "FRAGMENT",
    fixture: "company-registry-unreadable",
    why:
      "The opening of the unreadable-registry sentence, composed in `registryUnreadableLine` " +
      "around the file's own path. Classified against the whole sentence, whose row is " +
      "below; the path between the halves is a fact about one machine, not a string.",
  },
  {
    s: "and I couldn't read it just now, so I'm not showing any, rather than showing you a wrong list. That file is fixed by whoever set RichOS up. You can also add a company here in the meantime.",
    c: "NEEDS-SOMEONE-ELSE",
    fixture: "company-registry-unreadable",
    why:
      "The close of the same sentence, and the state it describes is not his: a file that " +
      "will not parse is fixed by whoever set RichOS up, and it names that party. It is " +
      "NOT the empty-list sentence and must never be collapsed into it — telling him he has " +
      "not named a company, when he has and it is one typo from working, would invite him " +
      "to enter it twice. The last clause carries the thing he CAN do, which is add one " +
      "here without waiting.",
  },
  // ---- and the Rust refusals the shell relays verbatim ---------------------------------
  {
    s: "I need a name for the company before I can file anything under it. Anything you'd recognize on a button is fine. You can change it later.",
    c: "ACTIONABLE",
    control: "#entity-add-name",
    fixture: "company-none-registered",
    why:
      "`register_entity`'s refusal of a blank name, rendered into `#entity-add-error` " +
      "beside the field that fixes it. It says what a good answer looks like rather than " +
      "only that the answer was wrong.",
  },
  {
    s: "That name doesn't have any letters or numbers in it, so I can't make a file-safe label out of it. Try a name with a word in it.",
    c: "ACTIONABLE",
    control: "#entity-add-name",
    fixture: "company-none-registered",
    why:
      "The other way a name is refused: the id is DERIVED from what he typed, and a name " +
      "with nothing in the id's character class derives nothing. Same field, same place.",
  },
  {
    s: "I already have a lot of companies with names like \"{name}\" and I couldn't make a distinct label for another one. Try a name that's a bit more specific.",
    c: "UNREACHABLE",
    why:
      "`register_entity`'s 99-collision bound. It needs ninety-nine companies whose names " +
      "all derive to one label; no fixture builds that and no person will. It exists so the " +
      "suffix loop terminates in a sentence rather than in a panic, and it is listed here " +
      "so the claim that it is unreachable is on the record rather than assumed.",
  },
  {
    s: "there's nothing at that path on this Mac",
    c: "FRAGMENT",
    why:
      "One of three reasons composed into `company_folder_message`, which wraps them in a " +
      "sentence naming the folder and the way out ('or leave it blank'). Never rendered " +
      "alone. This is the common one — a typed path that names nothing — and it is checked " +
      "rather than assumed precisely because a company registered against a folder that is " +
      "not there can never be selected by launching from it, and would surface weeks later " +
      "as 'Rich keeps asking me which company this is'.",
  },
  {
    s: "it isn't a full path from the top of the disk",
    c: "FRAGMENT",
    why:
      "The second reason in the same sentence: resolution is lexical, so a relative root " +
      "can never match anything and storing one would be a dead entry that looks live.",
  },
  {
    s: "that's a file, not a folder",
    c: "FRAGMENT",
    why: "The third reason in the same sentence, kept distinct because the fix differs.",
  },
  // ---- FIRST-RUN SETUP (Option D) — `setup.rs`, `setup_view.rs` -------------------------
  //
  // §19 states the condition these rows exist for: "today RichOS runs on his Mac and would
  // not run on anyone else's". Every machine but the CEO's opens on this sheet.
  {
    s: "There's a bit of setting up to do first.",
    c: "CONTROL",
    why:
      "The setup sheet's title, and `aria-labelledby` points the dialog at it. It is the " +
      "accessible name of the control beneath it, not a state — the same classification, " +
      "for the same reason, as the memory dialog's and the entity picker's titles. It is " +
      "replaced at open by one of the two counted titles below.",
  },
  {
    s: "There are a couple of things I need on this Mac.",
    c: "ACTIONABLE",
    control: "#setup-go",
    fixture: "setup-missing-both",
    why:
      "The title a customer's Mac opens on — no Claude Code and no engine directory. It is " +
      "entirely his to clear and it takes one press, so the button sits in the same dialog. " +
      "It counts in words rather than items because \"2 items\" is a package manager's " +
      "sentence and this is a conversation.",
  },
  {
    s: "There's one thing I need on this Mac.",
    c: "ACTIONABLE",
    control: "#setup-go",
    fixture: "setup-missing-engine",
    why:
      "The same state with one piece missing rather than two — a machine that already has " +
      "Claude Code and no engine, which is what a customer who installed Claude Code " +
      "himself is in. Same control, same press. It has its OWN fixture because the plural " +
      "title and this one cannot both render at once, and a fixture that happened to show " +
      "the other would have asserted nothing.",
  },
  {
    s: "I can get them myself. You just have to say so.",
    c: "ACTIONABLE",
    control: "#setup-go",
    fixture: "setup-missing-both",
    why:
      "The sheet's second line, shown only when this build can actually install what is " +
      "missing. It is the sentence that makes the press meaningful, so it is classified " +
      "with the press and not as a description — and it is deliberately absent in the " +
      "unpinned state below, where no press would help.",
  },
  {
    s: "I can get it myself. You just have to say so.",
    c: "ACTIONABLE",
    control: "#setup-go",
    fixture: "setup-missing-engine",
    why:
      "The same sentence for one missing piece rather than two. It has its own row for the " +
      "reason the two titles do: until 2026-09-04 there was only the plural, so a machine " +
      "missing only the engine read \"There's one thing I need on this Mac. I can get them " +
      "myself\" — the first screen a customer ever sees, disagreeing with itself in its own " +
      "second sentence.",
  },
  {
    s: "I couldn't finish the setup.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy:
      "the note directly beneath it — \"That's everything I could do — something is still " +
      "missing. That part is for whoever set RichOS up to look at.\" — which names the party",
    why:
      "The other ending's heading, and it carries no fixture for the same reason its note " +
      "carries none: reaching it needs a component to vanish between the install and the " +
      "backend's re-read of the disk. It does not name the party itself because the sentence " +
      "under it does, in the same view, and repeating it would make a two-line dialog say the " +
      "same thing twice.",
  },
  {
    s:
      "This copy of RichOS wasn't built with an engine to install, so I can't fetch one. It " +
      "needs whoever set RichOS up to publish one and pin it.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    fixture: "setup-unpinned",
    why:
      "A build carrying no engine pin. `setup.rs` refuses rather than fetching whatever a " +
      "URL returns, so the sheet explains and names the party — and `#setup-go` is HIDDEN, " +
      "because a button that would certainly fail is worse than no button. It ships as " +
      "`setup_view::SETUP_UNPINNED_NOTE` as well as `SetupError::EngineUnpinned`'s Display, " +
      "with a Rust test requiring the two to be one string: an `#[error(...)]` attribute is " +
      "not somewhere this scrape can see, and a sentence this registry cannot see is one " +
      "nobody has said whether the CEO can act on.",
  },
  {
    s: "Setup is done.",
    c: "INFORMATIONAL",
    fixture: "setup-finished",
    why:
      "The installation heading confirms that software setup completed. Account connection is reported separately below it.",
  },
  // ---- ROUND 19, STATES 1 TO 4 (dictation plan slice 4), drawn only once dictation is there
  // (`ask.dictation`, off until slice 5). `tests/setup.js` cases 2a and 23 to 26 render each one.
  {
    s: "Setting things up",
    c: "INFORMATIONAL",
    fixture: null,
    why: "Round 19's heading while the press runs (state 2). The rows under it say each step's state; nothing asks him to act.",
  },
  {
    s: "You're all set.",
    c: "INFORMATIONAL",
    fixture: null,
    why: "Round 19's heading once Claude Code, the engine and the voice and video tools are all in (state 3). Start, beside it, is the way on.",
  },
  {
    s: "Voice is ready. You can just talk to me now instead of typing.",
    c: "INFORMATIONAL",
    fixture: null,
    why: "Rich's line on the finished sheet (state 3): voice works now. It names no control; the talk button beside the message box is the offer's subject, and Start closes the sheet.",
  },
  {
    s:
      "Sit tight, we need to download about 1 GB of local voice AI so that you can just talk to Rich instead of typing. " +
      "This will save you $140+/year👍 because you won't need Wispr Flow with this setup.",
    c: "INFORMATIONAL",
    fixture: null,
    why:
      "His download line, word for word (`setup_view::SETUP_DOWNLOAD_LINE`), shown as Rich's line only while both speech models are " +
      "fetched. It says what is happening and why; the download needs nothing from him.",
  },
  {
    s: "Voice is ready. Press the round button beside the message box and just talk to me.",
    c: "ACTIONABLE",
    control: "#talk-toggle",
    fixture: null,
    why:
      "The first paragraph of Rich's one offer of dictation (state 4), shown only once voice is ready, which is when the talk " +
      "button beside the message box exists and works. The control it names is that button.",
  },
  {
    s: "If you turn on RichScribe, I will automatically give you a free & private/local replacement for Wispr Flow. (saves you $140+/year)",
    c: "INFORMATIONAL",
    fixture: null,
    why: "The CEO's copy (2026-10-13), the offer's second paragraph: what dictation gives him. Turn on dictation is beside it.",
  },
  {
    s: "So, instead of typing you'll be able to do this: Just click into any other app (Mail, Slack, Chrome/Safari, anything), press",
    c: "FRAGMENT",
    why: "The CEO's copy (2026-10-13), the offer's third paragraph, up to the key's name (`F1`, a key cap). ACTIONABLE as a whole through Turn on dictation beside it.",
  },
  {
    s: "(or another key you'll pick as the trigger) and say what you want written. Your words will appear where your cursor is.😊",
    c: "FRAGMENT",
    why: "The CEO's copy (2026-10-13), the offer's third paragraph after the key's name.",
  },
  {s: "Turn on RichScribe", c: "CONTROL", why: "The offer's first answer: hands over to the Dictation sheet's turn-on (slice 2)."},
  {
    s: "Okay. It's in Settings, under RichScribe, whenever you want it.",
    c: "INFORMATIONAL",
    fixture: null,
    why: "What the offer says after Not now: where dictation lives. Nothing is asked of him.",
  },
  {
    s: "It's in Settings, under RichScribe.",
    c: "INFORMATIONAL",
    fixture: null,
    why: "What the offer says after Turn on dictation when dictation did not end up on: where it lives. Nothing is asked of him.",
  },
  {
    s: "{FILE_NAME} is not a JSON object",
    c: "NOT-RENDERED",
    why: "`dictation_offer.rs`: why the offer was not made, printed to the log; the window shows no offer and no sentence.",
  },
  {
    s: "{FILE_NAME} is not readable: {e}",
    c: "NOT-RENDERED",
    why: "`dictation_offer.rs`: why the offer was not made or not recorded, printed to the log; never on screen.",
  },
  {
    s: "{FILE_NAME} could not be read: {e}",
    c: "NOT-RENDERED",
    why:
      "Two sources, one string, never on screen from either. (1) `dictation_offer.rs:48`, for a " +
      "dictation.json that could not be opened: `view` prints it to the log (`dictation_offer.rs:67`) " +
      "and shows no offer; `mark_shown` returns it from `dictation_offer_shown` (main.rs), which " +
      "logs it and whose only caller, `invokeQuiet` (ui/main.js), discards the error. (2) " +
      "`dictation/store.rs:61`, dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md " +
      "rev 2): the dictation tool is the same executable started with `--richos-dictation` " +
      "(dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no " +
      "command; slice 1 has no screens (dictation/mod.rs). In the tool it is written to dictation.log " +
      "(`log::line`, tool.rs:128 and :320); in the app `dictation_app` prints it with " +
      "`eprintln!(\"[richos] dictation: {why}\")` (dictation_app.rs:200). Neither returns it to the webview.",
  },
  {
    s: "That's everything I could do. Something is still missing. That part is for whoever set RichOS up to look at.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    why:
      "The other ending, and the reason `run_setup` re-reads the disk instead of trusting " +
      "that no step threw: a run whose steps all returned Ok while something is still " +
      "absent must not say \"I'm ready\". It names the party because the fact of WHICH piece " +
      "is missing is on the operator's boot line and not on his screen. Reaching it needs a " +
      "component to vanish between the install and the re-read, so it is classified here " +
      "rather than fixtured — an unreachable-from-the-mock state that is still shipped is " +
      "exactly the kind this registry exists to keep honest.",
  },
  {
    s: "Your memory folder.",
    c: "CONTROL",
    why:
      "The same dialog's heading AFTER he has answered it, and it exists because the question " +
      "used to stay on screen above its own answer: \"Where should I keep what you tell me?\" " +
      "over \"That's set up.\" — and, on a machine with no compiler, over the sentence saying " +
      "the folder cannot be read yet. It is the accessible name of the dialog it labels, not a " +
      "state, which is the same classification the question below carries.",
  },
  {
    s: "Where should I keep what you tell me?",
    c: "CONTROL",
    why:
      "The first-run memory dialog's title, and `aria-labelledby` points the dialog at it. " +
      "It is the accessible name of the control beneath it, not a state — the same " +
      "classification, for the same reason, as the entity picker's title.",
  },
  {
    s:
      "I'll keep your decisions, your companies and how you work in a folder on this Mac and " +
      "nothing in it leaves this Mac. If this looks right, I'll set it up now.",
    c: "ACTIONABLE",
    control: "#memory-setup-go",
    fixture: "memory-unprovisioned",
    why:
      "The state a fresh install is in: no corpus anywhere, which is what the installed " +
      "bundle measurably reported the moment its hand-made pointer was removed. It is " +
      "entirely his to clear and it takes one click, so the button sits directly beneath " +
      "the sentence and the location it will use is SHOWN above it — his part is a choice, " +
      "never a path he types. Nothing behind the button picks a location when he has not: " +
      "`provision` refuses an unset target by name.",
  },
  {
    s:
      "That's set up. From now on I'll keep what you tell me in that folder and read it back " +
      "when it matters.",
    c: "INFORMATIONAL",
    why:
      "What he sees after answering, when the corpus resolved AND the compiler is installed. " +
      "Nothing to do — it is the confirmation that the question is over, and it carries no " +
      "imperative aimed at him.",
  },
  {
    s:
      "Your memory folder is on this Mac and I can't read or write it yet: the part of me that " +
      "does isn't in this version. Nothing else is affected: our conversations stay on this Mac " +
      "and I pick them up when you come back. There's nothing for you to install and nothing for " +
      "you to fix. I'll start using the folder on my own as soon as that part arrives.",
    c: "INFORMATIONAL",
    fixture: "memory-no-compiler",
    why:
      "Two moments, one sentence: a boot that resolves a corpus and no compiler, and a setup " +
      "that finishes the same way. " +
      "IT WAS NEEDS-SOMEONE-ELSE UNTIL 2026-09-04 AND THE PARTY WAS THE DEFECT. The sentence " +
      "read \"It needs whoever set RichOS up to add it\", which is the right shape on a machine " +
      "somebody else set up and a dead end on a customer's: Andreas installs RichOS himself, so " +
      "the party is HIM, and the product's headline promise ended on an instruction to fetch a " +
      "third party who does not exist. That is the same argument the Anthropic-account row above " +
      "makes for its own bucket — when the person is him, NEEDS-SOMEONE-ELSE is the wrong " +
      "answer, because that bucket exists to point him at somebody who is not him. " +
      "HERE THERE IS NOBODY AT ALL, WHICH IS WHY IT IS INFORMATIONAL AND NOT ACTIONABLE. The " +
      "missing piece is the loro compiler, and it has no route onto any machine: the public " +
      "product repo tracks no `loro/` file, the signed bundle's Resources hold `icon.icns` and " +
      "nothing else, and the engine release asset carries `engine/`, which holds no compiler. " +
      "Neither he nor an operator can install what ships from nowhere, so no control is drawn " +
      "and no party is named — the sentence states what does not work, what still does, and " +
      "that nothing is required of him. It issues no instruction to him, which is the property " +
      "the INFORMATIONAL bucket owes.",
  },
  {
    s: "This copy of me was told which company it works for when it was started up, from outside this window and I can't make sense of what it was told, so I won't file anything until whoever set RichOS up has sorted it out.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    fixture: "company-pinned-unresolved",
    why:
      "`RICHOS_ENTITY` set to something this build does not have. The variable short-circuits " +
      "resolution BEFORE the saved choice, so no picker is opened — every answer he could give " +
      "would be written to disk and then never read, and asking a question whose answer is " +
      "guaranteed to be swallowed is worse than saying who owns the fix.",
  },
  {
    s: "This copy of me was told which company it works for when it was started up, from outside this window, so I can't move it from in here. Whoever set RichOS up is the one who changes that.",
    c: "UNREACHABLE",
    why:
      "`ENTITY_PINNED_MESSAGE` — `choose_entity`'s refusal when `RICHOS_ENTITY` is set. No " +
      "path in the shipped UI reaches it, and that is checkable rather than asserted: the two " +
      "callers of `choose_entity` are the picker's rows and the settings radios, and " +
      "`buildCompanyRow` (settings-button.js) draws a statement and no control while " +
      "`pinnedByEnvironment` is true, and `requireCompanyChoice` returns before opening the " +
      "picker in the same condition. It is " +
      "the guard for a caller that is not this UI. If it ever renders, the bug is ours.",
  },
  {
    s: "first-run setup is incomplete",
    c: "NOT-RENDERED",
    why:
      "`send_message` could not attach a lease AND `setup_view::detect` found something missing on disk. The CEO gets `SETUP_INCOMPLETE_*` instead; this is the operator half. " +
      "It is the `why` argument to `main.rs::refused_send`, which prints it with " +
      "`eprintln!` and returns the CEO sentence UNCHANGED — the nightly's D4, where a " +
      "typed message that never became a turn reached the window and said nothing to the " +
      "log. This scrape sees it because it sits beside `Err(` in the command layer, " +
      "which is the one shape `RUST_CEO_CONTEXT` cannot tell apart from CEO copy.",
  },
  {
    s: "no compute lease and no factory",
    c: "NOT-RENDERED",
    why:
      "The same refusal with nothing missing on disk, so the cause is the account rather than the install. The CEO gets `LEASE_UNAVAILABLE_MESSAGE`. " +
      "It is the `why` argument to `main.rs::refused_send`, which prints it with " +
      "`eprintln!` and returns the CEO sentence UNCHANGED — the nightly's D4, where a " +
      "typed message that never became a turn reached the window and said nothing to the " +
      "log. This scrape sees it because it sits beside `Err(` in the command layer, " +
      "which is the one shape `RUST_CEO_CONTEXT` cannot tell apart from CEO copy.",
  },
  {
    s: "no active thread",
    c: "NOT-RENDERED",
    why:
      "No thread is open to file the message under. The CEO gets the open-a-conversation line. " +
      "It is the `why` argument to `main.rs::refused_send`, which prints it with " +
      "`eprintln!` and returns the CEO sentence UNCHANGED — the nightly's D4, where a " +
      "typed message that never became a turn reached the window and said nothing to the " +
      "log. This scrape sees it because it sits beside `Err(` in the command layer, " +
      "which is the one shape `RUST_CEO_CONTEXT` cannot tell apart from CEO copy.",
  },
  // GONE, 2026-09-17, and the deletion is the point rather than the tidying: "the active
  // thread changed between render and send" and its CEO sentence, "The conversation changed
  // before your message was sent. Open the original conversation to try again.", were the
  // refusal `send_message` gave when the named conversation was not the single active one.
  // The CEO's Two Riches page is "any number of conversation threads" and "the CEO could
  // open and run multiple things in parallel"; a message to any conversation is now
  // answered on that conversation, so there is no state here to classify. Two states
  // removed from the product is two rows removed from this file — the suite requires it in
  // both directions, and it is what failed when the rows outlived the strings.
  {
    s: "entity not resolved from {}: {e}",
    c: "NOT-RENDERED",
    why:
      "A note from `resolve_boot_entity`, printed by `boot_entity` with `eprintln!` and never " +
      "returned to the webview. It is a `format!` rather than an `eprintln!` for one reason: " +
      "the resolver is a pure function of its arguments so the ORDER of the four steps can be " +
      "asserted by a test, which the version it replaced — reading `std::env::var` and " +
      "`current_dir()` inline — structurally could not be. Its audience is a terminal.",
  },

  // -------------------------------------------------------------------------------------
  // UNREACHABLE / NOT-RENDERED
  // -------------------------------------------------------------------------------------
  {
    s: "I can't tell which company this work belongs to, so I won't guess. Filing it under the wrong one would mix two companies' records together and that's not a mistake worth risking to save you a question. Pick the company and I'll keep everything under it from then on.",
    c: "ACTIONABLE",
    control: "#choose-company-btn",
    fixture: "corrections-read-failed",
    why:
      "IT CHANGED BUCKET ON 2026-09-01, and the bucket is the whole point of the change. " +
      "`loro_pending_corrections` resolves the entity BEFORE it touches the desk, so on a " +
      "launch with no company chosen `loro_available` is still true and the pending read " +
      "refuses with this sentence, verbatim, in the desk's `readFailed` branch. It used to " +
      "be NEEDS-SOMEONE-ELSE and it named its party, because picking a company genuinely " +
      "was not something the app could do — `RICHOS_ENTITY` or a working directory were " +
      "the only two routes and a CEO has neither. Slice 4 gave him both a launch picker " +
      "and a preferences row, so the party clause was retired as a lie and this is now HIS " +
      "to fix. " +
      "THE COST, stated where a reviewer sees it: the control is `#choose-company-btn` on " +
      "the composer, and while the corrections desk is open it sits BEHIND that modal — " +
      "the desk carries its own close button (`#corrections-close`) and the same choice is " +
      "in the preferences popover, so it is one dismissal away rather than nowhere, but " +
      "this suite does not check occlusion and it would be dishonest to let the row imply " +
      "otherwise.",
  },
  {
    s: 'unknown theme {theme:?}: expected "dark", "light" or "system"',
    c: "UNREACHABLE",
    why:
      "`set_theme`'s refusal (src-tauri/src/main.rs). No path in the shipped UI reaches it: " +
      "the only caller is `RichSettings`'s theme segment, whose three buttons carry " +
      "`data-th=\"light\" | \"system\" | \"dark\"` and nothing else, and `RichTheme.setTheme` " +
      "validates again before the write. It exists because the alternative — coercing an " +
      "unexpected string to the default — would look exactly like the CEO changing his own " +
      "mind about his own machine, silently. It is the guard for a caller that is not this " +
      "UI, and it carries no control because there is nothing for him to press: if it ever " +
      "renders, the bug is ours.",
  },
  {
    s: "That isn't one of the four answers, so I haven't written anything down.",
    c: "UNREACHABLE",
    why:
      "`FEEDBACK_KEY_NOT_ONE_OF_FOUR` — `PromptOutcome::from_key`'s `None`, said out loud. " +
      "No path in the shipped UI reaches it: the only callers of `feedback_preview` and " +
      "`feedback_record` are the four buttons `renderFeedbackKeys` builds from " +
      "`feedback_wording.ratings` plus its `dismiss`, and each carries its own key. It is " +
      "the guard for a caller that is not this file, and `feedback.js` check 7b invokes the " +
      "command directly to prove it refuses rather than inventing a dismissal.",
  },
  {
    s: "I won't record that. What you were shown isn't what I would say now, so approving it would be approving something you haven't read. Ask me to show it again.",
    c: "UNREACHABLE",
    why:
      "`FEEDBACK_PREVIEW_MISMATCH`. In-process, 'he saw exactly this' is structural — " +
      "`ApprovedReport` has no public constructor and the only route to one is " +
      "`Disclosure::approve`, which cannot exist without having rendered its text. That does " +
      "not survive an IPC boundary, so the command re-renders and compares. `main.js` holds " +
      "the rendered block verbatim in `feedback.shown` and posts it back unmodified, so no " +
      "path in the shipped UI produces a mismatch; `feedback.js` check 7 invokes the command " +
      "with altered text to prove the guard is real rather than decorative.",
  },
  {
    s: "I couldn't read that audio file.",
    c: "UNREACHABLE",
    why: "CaptureError::WavRead comes only from the file-input path, which no Tauri command exposes.",
  },
  {
    s: "unknown attention tier: {tier}",
    c: "UNREACHABLE",
    why: "raise_proactive_message's Err. Registered, never invoked by main.js.",
  },
  {
    s: "no correction {key} is awaiting an answer",
    c: "INFORMATIONAL",
    why:
      "`spoken_confirm_correction`'s refusal when the key names no pending candidate. It DOES " +
      "reach the DOM now — the desk relays every rejection verbatim into #corrections-notice — " +
      "but only from a card that is already stale, and the desk re-reads both families after " +
      "every answer, so the row it names is gone by the time the sentence is on screen. " +
      "Nothing to press, and the `{key}` hole means it never renders in this literal form.",
  },
  {
    s: "nothing to add",
    c: "NOT-RENDERED",
    why:
      "`steer_message`'s empty-text refusal. Two independent reasons it never reaches the DOM: " +
      "`send()` returns at main.js:1097 before any invoke when the trimmed text is empty, and " +
      "`steer()`'s catch (main.js:1212-1219) replaces whatever the backend said with its own " +
      "authored notice about the words being back in the box.",
  },
  {
    s: "unexpected intake record: {other:?}",
    c: "NOT-RENDERED",
    why:
      "A `{other:?}` Debug hole in `steer_message` — machinery for an engineer, and unreachable " +
      "besides: `TurnControl::steer` (steering.rs:470-475) constructs `IntakeRecord::Steer` and " +
      "returns nothing else. `steer()`'s catch would swallow it either way.",
  },
  {
    s: "mock: no such command",
    c: "NOT-RENDERED",
    why:
      "A comparison string (`msg.startsWith(...)` at main.js:1057), never written to the DOM. " +
      "A genuine false positive of the prose filter, kept visible rather than special-cased " +
      "away — a filter with a hidden exception list is the next drift.",
  },

  // -------------------------------------------------------------------------------------
  // TECHY MODE (open-items row 3.1, techy-mode design §3.1/§3.3/§3.4)
  //
  // The four state sentences answer ONE question — "why is there no machinery here?" — and
  // they are deliberately four different answers. Two are INFORMATIONAL (there is genuinely
  // nothing to do about a record that was never written) and two are NEEDS-SOMEONE-ELSE (a
  // store the OS is refusing has an owner, and it is not the CEO). Collapsing them into one
  // bucket would be the same mistake as collapsing them into one sentence.
  // -------------------------------------------------------------------------------------
  {
    s: "I can't read the technical record for this conversation. It's on this machine and I haven't lost it. Something is refusing to open it and whoever set RichOS up needs to look.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    fixture: "techy-unreadable",
    why:
      "`machinery_view.rs::UNREADABLE`. Nothing to press: the CEO cannot chmod a directory " +
      "from a conversation, and offering him a Retry over a permission bit would be a " +
      "control that does nothing. It names the owner instead, and says plainly that the " +
      "record is not lost — the state it must never be confused with is `nothing_recorded`, " +
      "which claims the opposite.",
  },
  {
    s: "I can't read the stored output for this one. It's on this machine and I haven't lost it. Whoever set RichOS up needs to look.",
    c: "NEEDS-SOMEONE-ELSE",
    party: true,
    why:
      "`main.rs::RAW_UNREADABLE`, the same fault one level down — the Tier-B raw shard for " +
      "ONE record. No fixture: reaching it needs the raw sibling to be unreadable while the " +
      "Tier-A shard is fine, which is a real filesystem state and not one this harness can " +
      "produce, because `breakMachinery` takes the whole thread directory (and then there " +
      "are no rows left to expand). Named rather than left unclassified.",
  },
  {
    s: "Not part of any one exchange: this is what the session said with no turn running.",
    c: "INFORMATIONAL",
    fixture: "techy-on",
    why:
      "`index.html`'s `#between-turns-lede` — the lede over §1.5's between-turn lane. It " +
      "exists because a section of rows placed OUTSIDE the conversation invites the " +
      "question 'why are these not up there with everything else', and the answer is a " +
      "fact about the records: they carry no turn (`turnId: None`, §1.4 G4), so they have " +
      "no position in the conversation to be drawn at. Nothing for the CEO to do — it " +
      "explains a layout, it does not ask for a decision.",
  },
  {
    s: "Nothing was recorded between turns in this conversation. Rich started keeping this on 2026-08-30, so in an older conversation that is a gap in the record, not proof the session was quiet.",
    c: "INFORMATIONAL",
    // `techy-empty`, not `techy-on`: the `acme` thread that `techy-on` opens HAS
    // between-turn traffic, so the sentence correctly does not render there. The fixture
    // that shows it is the conversation with nothing recorded at all — which is also the
    // commonest way a CEO will meet it, since every thread older than 2026-08-30 is one.
    fixture: "techy-empty",
    why:
      "`machinery_view.rs::BETWEEN_TURNS_QUIET`. The honest empty state for the lane, and " +
      "shaped like `NOTHING_RECORDED` for the same reason: the lane is empty in two " +
      "different situations and only one of them means the session was quiet. Nothing to " +
      "act on — between-turn retention began on 2026-08-30 and what was never written down " +
      "is unrecoverable, exactly as it is one level up.",
  },
  {
    s: "No machinery was recorded for this conversation. Retention started on 2026-08-28 and anything Rich did before that was never written down, so this is a gap in the record, not a quiet conversation.",
    c: "INFORMATIONAL",
    fixture: "techy-empty",
    why:
      "`machinery_view.rs::NOTHING_RECORDED`. THE honest empty state, and there is nothing " +
      "to do about it by construction: routing began at richos `48561e4` and dropped bytes " +
      "are unrecoverable, forever. The second clause exists because the sentence would " +
      "otherwise read as a claim about the CONVERSATION rather than about the RECORD.",
  },
  {
    s: "Nothing has been recorded on this machine yet. The technical view reads a store that hasn't been written to. It fills up as Rich works.",
    c: "INFORMATIONAL",
    why:
      "`machinery_view.rs::NOT_RETAINED` — a fact about the INSTALL, not the thread: no " +
      "machinery root at all, or a spine with no journal attached. No fixture: the mock " +
      "always has a journal, and faking the state would prove the mock. Nothing to do — " +
      "retention is unconditional (§3.2) and starts the first time Rich uses a tool.",
  },
  {
    s: "The full output isn't kept this long. What's above is the whole record that was.",
    c: "INFORMATIONAL",
    fixture: "techy-on",
    why:
      "`main.rs::RAW_NOT_RETAINED`. §2.4's honest degrade: the Tier-B window passed over " +
      "this row and the normalized record above it is untouched. Nothing to do, and it " +
      "deliberately names NO duration — how long raw payloads survive is §7.2, the CEO's " +
      "open question, and a sentence saying '14 days' would answer it in copy.",
  },
  {
    s: "This output was longer than RichOS keeps; you're seeing the start of it.",
    c: "INFORMATIONAL",
    fixture: "techy-on",
    why:
      "`main.rs::RAW_TRUNCATED` — §2.4's 32 KB per-record cap fired and what is on screen " +
      "is a prefix. Nothing to do; the label exists because a prefix that looks whole is " +
      "worse than one that says it is not.",
  },
  {
    s: "The stored output isn't reachable in this build.",
    c: "INFORMATIONAL",
    why:
      "main.js's fallback when `get_machinery_raw` is not a registered command — the mock " +
      "harness, or a shell built without the techy commands. It states the limit rather " +
      "than leaving an empty pane, and there is nothing for the CEO to do about which " +
      "commands his build registered.",
  },
  {
    s: "Technical view · this conversation",
    c: "CONTROL",
    why: "#techy-chip's label when a per-thread pin is holding it on. Pressing it turns it off.",
  },
  {
    s: "Technical view · everywhere",
    c: "CONTROL",
    why: "#techy-chip's label when the global default is holding it on. Same button, different cause.",
  },
  {
    s: "Technical view · this company",
    c: "CONTROL",
    why:
      "#techy-chip's label when a COMPANY pin is holding it on — §7.1's middle tier, added " +
      "on the CEO's answer of 2026-09-18. Third cause, same button. The chip names the tier " +
      "rather than the company, because the scope crumb beside it already names the company " +
      "and two places printing one name is two places that can disagree.",
  },
  {
    s: "Technical view is on for every conversation in this company. Open its switch in Settings.",
    c: "CONTROL",
    why: "#techy-chip's accessible name in the company case, matching the two beside it. Since round 27 the chip opens Settings on the technical view's one switch.",
  },
  {
    s: "Technical view is on for this conversation. Open its switch in Settings.",
    c: "CONTROL",
    why: "#techy-chip's accessible name in the pinned case — the label plus what pressing it does (round 27: it opens Settings on the switch).",
  },
  {
    s: "Technical view is on for every conversation. Open its switch in Settings.",
    c: "CONTROL",
    why: "#techy-chip's accessible name in the global case. Since round 27 pressing it opens Settings on the technical view's one switch.",
  },
  // THE RAIL'S TECHY GROUP HAS NO ROW HERE ANY MORE, and that is this gate working rather
  // than a gap. Its title was "Show the technical view" and its label "In every
  // conversation"; on 2026-09-18 they became "Technical view" and "Show it" — the
  // subject/act split the Opening screen group below already uses — because a label naming
  // one scope is true of exactly one of the CEO's three choices now that the switch asks
  // which. Both new strings fall under `state-strings.js`'s documented prose floor (12
  // characters AND 3 words), so the scraper does not see them, and the staleness half of
  // the classification check refused the rows the moment they were written: a registry may
  // not claim coverage of a string nothing tracks. Where the switch applies is said in
  // #techy-hint, which IS above the floor and IS classified, three tiers' worth.
  {
    s: "On for this conversation. asks where to change it.",
    c: "CONTROL",
    why:
      "#techy-hint under the Settings switch, with the `${key}` shortcut hole folded out by " +
      "the scraper — it renders as 'On for this conversation. ⌘⇧T asks where to change it.' " +
      "It names WHICH of the three tiers is holding this conversation on, which the switch's " +
      "label carried until 2026-09-18 and can no longer carry, and it describes the KEYBOARD " +
      "affordance beside it. A control's description rather than a state: there is no fault " +
      "here and nothing has gone wrong.",
  },
  {
    s: "On for all conversations in this company. asks where to change it.",
    c: "CONTROL",
    why: "The same hint over §7.1's middle tier — a company pin is holding this one on.",
  },
  {
    s: "On for all conversations in all companies. asks where to change it.",
    c: "CONTROL",
    why: "The same hint over the global tier. Three tiers, three sentences, one hint.",
  },
  {
    s: "asks where to show it.",
    c: "CONTROL",
    why:
      "The OFF branch of #techy-hint: with the technical view off there is no tier to name, " +
      "so the hint is the shortcut, and what the shortcut DOES is ask. All four branches " +
      "promised one conversation ('shows it for one conversation only', 'changes just this " +
      "one') until 2026-09-18, when ⌘⇧T stopped being the entrance that answered the CEO's " +
      "question on his behalf — and that promise had been the whole argument for the " +
      "exception.",
  },

  // ---- §7.1'S THREE-WAY SCOPE SHEET (the CEO's answer, 2026-09-18) ---------------------
  //
  // Every string here is CONTROL and none of them is a state: the sheet is a question the
  // CEO opened by touching a switch, its three answers are radio buttons in front of him,
  // and both ways out (Cancel, Escape) are one keystroke away. Nothing has gone wrong and
  // there is nothing for him to fix — which is the distinction this registry exists to
  // make, and the reason a question's own words are not an ACTIONABLE state.
  {
    s: "Turn on the technical view",
    c: "CONTROL",
    why:
      "#techy-scope-title on the switch-ON path, and the sheet's own accessible name. It " +
      "says which direction the confirm goes, so that is never inferred from which switch " +
      "was touched. Also the markup's shipped default, which is why it is scraped from " +
      "index.html as well as from main.js.",
  },
  {
    s: "Turn off the technical view",
    c: "CONTROL",
    why: "The same title on the switch-OFF path — his 'switch it off for a given company or a given thread'.",
  },
  {
    s: "For all conversations in all companies",
    c: "CONTROL",
    why:
      "His first radio, preselected, in his own words with 'threads' -> 'conversations' — " +
      "the word every other surface in this product says to him.",
  },
  {
    s: "For all conversations in this company",
    c: "CONTROL",
    why:
      "His second radio. Hidden, not disabled, for a conversation with no company binding: " +
      "`ConfigStore::apply_techy_scope` refuses a company scope without one, and offering a " +
      "choice the store will reject is worse than offering two.",
  },
  { s: "For this conversation only", c: "CONTROL", why: "His third radio — the scope ⌘⇧T applies directly." },
  {
    s: "unknown techy scope: {scope}",
    c: "NOT-RENDERED",
    why:
      "`set_techy_scope`'s refusal of a scope this build does not know (main.rs). It never " +
      "reaches the DOM, and there are two independent reasons: the only caller is " +
      "`confirmTechyScope`, which sends the `value` of a radio the SHELL ships, so the " +
      "string cannot be reached from the product at all; and `invokeQuiet` swallows a " +
      "refusal and returns null, on which that function repaints every switch from the " +
      "store rather than rendering a message. It exists so a scope is refused by name " +
      "instead of being rounded to an adjacent tier — which would apply the CEO's switch " +
      "somewhere he did not point it — and the sentence is for whoever reads the log.",
  },

  // ---- §7.2: the raw-retention window, in the same Settings group ----------------------
  //
  // Every hint below follows the #techy-hint precedent two rows up: they sit under the
  // control they describe, in the same popover, and they describe what it does rather than
  // report a fault. Nothing here is a state the CEO is being asked to fix — the three radios
  // are RIGHT THERE, and the sentence exists so that picking one is an informed act rather
  // than a guess about what "whichever binds first" will do to his stored output.
  { s: "Keep the stored output", c: "CONTROL", why: "The Settings panel's title over the raw-retention radio group (round 27: its first column)." },
  // ROUND 27: the one Settings panel's three column labels, each a heading over the controls in
  // its column (settings-button.js `addColumn`). Nothing to act on in the words themselves.
  { s: "How it looks", c: "CONTROL", why: "The Settings panel's first column label (round 27), over Theme, Text size, Technical view and stored output." },
  { s: "Working with Rich", c: "CONTROL", why: "The Settings panel's second column label (round 27), over Rich's model, interruptions, Dictation and the phone." },
  { s: "Your company and team", c: "CONTROL", why: "The Settings panel's third column label (round 27), over Company, Home screen, Token spend, Codex reviews, accounts, folders, memory and Updates." },
  { s: "For two weeks", c: "CONTROL", why: "The `two-weeks` radio's label — the shipping default (`config.rs`)." },
  { s: "For three months", c: "CONTROL", why: "The `three-months` radio's label." },
  {
    s: "Nothing is ever removed.",
    c: "CONTROL",
    why:
      "#retention-hint when both axes are `forever`. It describes what the selected radio " +
      "beside it means, and the whole point of saying it plainly is that eviction is an " +
      "`unlink` nothing else in the product would ever mention.",
  },
  {
    s: "of output, whichever comes first.",
    c: "FRAGMENT",
    why:
      "The tail of #retention-hint's two-axis sentence, with the day and byte holes folded " +
      "out by the scraper — it renders as 'Kept for 14 days, or 2.1 GB of output — whichever " +
      "comes first.' Never on its own.",
  },
  {
    s: "Kept until it reaches",
    c: "FRAGMENT",
    why:
      "The head of the other #retention-hint branch — a byte ceiling with no day window, " +
      "reachable only from a hand-edited config.json. Completed by the size and the row below.",
  },
  {
    s: ", oldest first.",
    c: "FRAGMENT",
    why: "The tail of that same sentence. Says which end of the store the ceiling eats from.",
  },
  {
    s: "Removed the stored output from",
    c: "FRAGMENT",
    why:
      "The head of #retention-hint's announcement after a window change actually evicted, " +
      "completed by the day count and 'The records are still there; their output is not.' " +
      "IT IS THE SENTENCE THE WHOLE CONTROL EXISTS FOR: a delete that says nothing is how a " +
      "CEO finds an empty row weeks later and cannot connect it to anything he did.",
  },
  {
    s: "1 earlier day",
    c: "FRAGMENT",
    why: "The singular arm of that count. 'N earlier days' is the plural and is built from the number.",
  },
  {
    s: "Set by hand in config.json, so none of the three is selected.",
    c: "CONTROL",
    why:
      "#retention-hint when the stored window matches no menu entry (`RetentionChoice::Custom`). " +
      "It explains why no radio is checked instead of checking the nearest one — rounding " +
      "would misreport his setting AND the next click on any other control would write the " +
      "rounded value back over it. The fix, if he wants one, is the radio group it sits under.",
  },
  {
    s: "This build can't reach the retention setting.",
    c: "INFORMATIONAL",
    why:
      "main.js's fallback when `raw_retention` is not a registered command — the mock harness, " +
      "or a shell built without them. Same shape and same reasoning as 'The stored output " +
      "isn't reachable in this build.' above: it states the limit rather than showing a radio " +
      "group with nothing behind it, and there is nothing for the CEO to do about which " +
      "commands his build registered.",
  },
  {
    s: "unknown retention choice: {choice}",
    c: "UNREACHABLE",
    why:
      "`set_raw_retention`'s Err. The only caller in the shipped UI is the change handler over " +
      "`input[name=raw-retention]`, whose three values are exactly the three `RetentionChoice::parse` " +
      "accepts — so the refusal arm is real, is tested in Rust, and no path in the webview reaches it.",
  },

  // -------------------------------------------------------------------------------------
  // UPSTREAM MODEL-API FAILURE (`crates/richos-core/src/upstream.rs`, row 3.30, 2026-09-05)
  //
  // On 2026-09-03 the Anthropic API returned `529 Overloaded` and killed four running
  // agents mid-task, plus a fifth on a session limit. The four `UpstreamFault` sentences
  // below are all INFORMATIONAL and all for the same reason: there is no control in RichOS
  // that makes Anthropic less busy or rolls a usage window over early. An ACTIONABLE
  // classification would owe a control, and inventing one would be worse than the silence
  // it replaces. None of them contains an imperative aimed at the reader.
  // -------------------------------------------------------------------------------------
  {
    s:
      "Anthropic's servers are at capacity, so that request never reached Claude. This " +
      "one ends when their capacity frees up and nothing on this machine brings it back " +
      "sooner.",
    c: "INFORMATIONAL",
    why:
      "`UpstreamFault::Overloaded` — HTTP 529. Its last clause is the half that does the " +
      "work: it says waiting is the only move AND that waiting has no known end, which is " +
      "exactly what separates it from the 429 row below. Before this existed both arrived " +
      "as the same raw `API Error: …` string relayed into the timeline, so the CEO could " +
      "not tell 'this comes back on its own' from 'this comes back when your window rolls " +
      "over'.",
  },
  {
    s:
      "Your Claude usage limit is used up, so that request never reached Claude. Unlike a " +
      "capacity problem, this one ends on a schedule: your plan's usage window has to roll " +
      "over. RichOS was not told what time that is.",
    c: "INFORMATIONAL",
    why:
      "`UpstreamFault::RateLimited` — HTTP 429. It names the schedule because the schedule " +
      "is the whole difference, and it explicitly declines to name a TIME because no " +
      "captured sample carries a reset timestamp or a Retry-After. Stating a wait it cannot " +
      "measure would be the reassuring-fraction defect in sentence form. INFORMATIONAL " +
      "rather than ACTIONABLE: changing a Claude plan happens on Anthropic's website, not " +
      "in this app, so there is no control here to name.",
  },
  {
    s:
      "Claude's API answered with a server error, so that request never reached the model. " +
      "It is on Anthropic's side rather than yours and it carries no schedule.",
    c: "INFORMATIONAL",
    why:
      "`UpstreamFault::ServerError` — any 5xx that is not 529. It says 'no schedule' for " +
      "the same reason the overload row does: the CEO's next decision is whether waiting " +
      "is a plan.",
  },
  {
    s:
      "Claude's API refused that request and RichOS has no name for the reason. Its own " +
      "words are kept with this message rather than summarized.",
    c: "INFORMATIONAL",
    why:
      "`UpstreamFault::Unclassified` — an `API Error:` with a status RichOS has no bucket " +
      "for (a 400, a 401, a status invented after this was written). It exists so an " +
      "unknown fault is NAMED as unknown instead of being folded into whichever known arm " +
      "looked closest, which would tell him to wait for something that is not coming. The " +
      "vendor's own line is quoted after it by `UpstreamFailure::ceo_message`.",
  },
  {
    s: "{} Claude reported: {}",
    c: "FRAGMENT",
    why:
      "`UpstreamFailure::ceo_message` — the fault's own sentence, then the vendor's line " +
      "verbatim. The attribution is load-bearing: an `API Error: 529 …` string arriving as " +
      "an assistant message is otherwise appended to the ledger as Rich's own reply, and " +
      "the CEO reads a vendor diagnostic in Rich's voice with no way to tell it from an " +
      "answer.",
  },
  {
    s: "RichOS tried once and stopped there.",
    c: "FRAGMENT",
    why:
      "The singular arm of `RetryBudget::ceo_message`. Written out rather than composed " +
      "with an 's' so the scrape sees a sentence instead of a stem.",
  },
  {
    s: "RichOS tried {} times and stopped there.",
    c: "FRAGMENT",
    why: "The plural arm of `RetryBudget::ceo_message`. The number is the billed attempt count.",
  },
  {
    s:
      "{} {} Each attempt costs against your Claude usage whether or not it produces an " +
      "answer, so it is not repeating this on its own.",
    c: "FRAGMENT",
    why:
      "`RetryBudget::ceo_message`'s frame: the fault's sentence, the attempts spent, then " +
      "why there will not be more. Four consecutive retries on 2026-09-03 consumed quota " +
      "and produced nothing, so the cost is named rather than implied — a ceiling nobody " +
      "is told about is still a silent retry, it just stops sooner.",
  },
  {
    s: "On disk: {}.",
    c: "FRAGMENT",
    why: "`TurnLoss::ceo_message`'s heading for the list of what survived the failure.",
  },
  {
    s: "what you asked for is saved",
    c: "FRAGMENT",
    why:
      "`TurnLoss::ceo_message`, first clause. Read off the ledger — the prompt is journaled " +
      "before it is delivered — never asserted.",
  },
  {
    s: "the {} characters of the answer that had already arrived are saved",
    c: "FRAGMENT",
    why:
      "`TurnLoss::ceo_message`, second clause. A measured count, because every assistant " +
      "delta is persisted before it is emitted, so the number is a fact about the file.",
  },
  {
    s: "and the {} recorded action(s) for this turn are saved",
    c: "FRAGMENT",
    why:
      "`TurnLoss::ceo_message`, third clause. Actions are claimed before they are executed, " +
      "so the record of what was DONE outlives the lease that did it.",
  },
  {
    s: "Nothing had been written to disk yet for this turn.",
    c: "FRAGMENT",
    why:
      "`TurnLoss::ceo_message` when every count is zero — which is the 2026-09-03 shape " +
      "exactly: each of the five agents died between dispatch and its first tool call. It " +
      "is a sentence rather than a list of zeros, and it is always followed by the " +
      "'Not on disk:' clause.",
  },
  {
    s:
      "Not on disk: everything the session had worked out in its head and had not yet " +
      "said. Asking again starts that part over.",
    c: "FRAGMENT",
    why:
      "`TurnLoss::ceo_message`'s closing clause, and the one that must never be dropped. " +
      "The statement never says 'nothing was lost', because the lease's working context is " +
      "always gone on this failure class and implying otherwise is the defect the row names.",
  },

  {
    s:
      "I was cut off partway through that answer, so what you heard is all I got out. " +
      "I'm still listening.",
    c: "INFORMATIONAL",
    why:
      "`VoiceNotice::ReplyCutOff`, added 2026-09-05 for open-items row 3.30. THE ONLY " +
      "NOTICE IN THIS REGISTRY THAT IS SPOKEN ALOUD AS WELL AS SHOWN, and that is the " +
      "requirement rather than a flourish: in voice mode the CEO's eyes are not on the " +
      "panel. What it replaces was worse than silence — on `rich://turn-error` the shell " +
      "called `voice_speak_end`, which FLUSHES the sentence chunker's tail, so a turn that " +
      "died mid-sentence spoke half a sentence aloud, trailed off, and said nothing " +
      "further. Trailing off is exactly what a person does while thinking, so he waited " +
      "for the rest of an answer that was never coming. " +
      "Two things are load-bearing in the wording. It states the CONSEQUENCE — what he " +
      "heard is all there is — because the alternative leaves the question open. And it " +
      "ends with a status rather than 'ask me again': the affordance for asking again is " +
      "the open microphone, there is no button to point at, and an imperative with no " +
      "control is a request wearing a status's clothes — the same reason `SoundButNoWords` " +
      "ends with \"Voice is still on.\" INFORMATIONAL for that reason and not because " +
      "nothing happened. The REASON (a 529, a 429) is a separate sentence supplied by " +
      "`richos-core`'s `upstream.rs` and classified above; voice never invents one.",
  },
  // -------------------------------------------------------------------------------------
  // REACHABILITY (`crates/richos-core/src/reachability.rs`, row 3.30, 2026-09-05)
  //
  // All three are UNREACHABLE, and that classification is the honest one rather than a
  // parking space. The module's only caller today is
  // `examples/reachability_probe.rs` — a `cargo run` operator tool that refuses to send
  // anything without `--spend`. No Tauri command exposes it and no path in the shipped
  // webview reaches these sentences. The module exists so that the FIRST surface to report
  // a reachability status cannot earn it with a cheap ping; when that surface is built,
  // these rows get reclassified against it and measured for contrast then.
  //
  // The fourth arm, `Failed`, mints no literal of its own — it returns
  // `UpstreamFault::ceo_message()`, which is already classified above.
  // -------------------------------------------------------------------------------------

  // =======================================================================================
  // THE SIX FILES THE INVENTORY COULD NOT SEE UNTIL 2026-09-05
  // =======================================================================================
  //
  // `lib/state-strings.js` read three files — index.html, main.js, timeline.js — while
  // `index.html` shipped nine `<script src>` tags and the tree held twelve product files.
  // Everything from here down was OUTSIDE the affordance rule entirely: a state in any of
  // these could have asked the CEO to do something with no control anywhere near it and
  // this suite would have reported green over it.
  //
  // The source list is derived now (`lib/ui-sources.js`), so these arrived as 55
  // unclassified states the moment the derivation landed, and each one is answered below.
  // They are grouped by file rather than by bucket, because what is worth reading here is
  // which surface had no gate over it — the buckets are on every row anyway.

  // ---- updates.js — the update surface, CEO ruling §26 ----------------------------------
  //
  // THE ONE THAT MATTERS MOST. §26 governs this file, and the clause it turns on is that
  // the update affordance must not be ACTIONABLE while work is running. The gate that would
  // check whether that state names its control could not see the file the state lives in.
  {
    s: "The update did not complete.",
    c: "ACTIONABLE",
    control: "#update-check",
    fixture: "updates-failed",
    why:
      "The `failed` headline for every failure that is not a refused signature. He can act: " +
      "`paint()` keeps `#update-check` visible in `failed` and relabels it 'Try again'. A " +
      "REFUSED SIGNATURE is the deliberate exception — `isSignature(v)` leaves the label as " +
      "'Check for updates' rather than inviting a retry of a security check that will refuse " +
      "identically — and it still has a control, so the row holds in both.",
  },
  {
    s: "It will activate automatically next time RichOS opens. Your work will continue uninterrupted.",
    c: "INFORMATIONAL",
    fixture: "updates-ready",
    why:
      "The `ready` sub-line, and it CHANGED CLASS on 2026-09-10 rather than merely changing " +
      "words. It read 'Restart when you are ready — nothing is lost.', which was an offer, " +
      "and it was ACTIONABLE because `#update-relaunch` stood directly under it. Commit " +
      "01e9b8d8 removed that button from `paint()` — an update now activates at the next " +
      "launch instead of asking him to restart — so the state stopped being one he acts on " +
      "and became one RichOS reports. INFORMATIONAL for the same reason the §26 waiting " +
      "sentence is: the control is REMOVED by design rather than missing, and nothing is " +
      "asked of anyone. Not NEEDS-SOMEONE-ELSE either — the party that will act is RichOS.",
  },
  {
    s: "This update will activate next time RichOS opens. Your work will continue uninterrupted.",
    c: "INFORMATIONAL",
    fixture: "updates-waiting-ready",
    why:
      "CEO ruling §26, and the reason this file had to become visible to this rule. RichOS " +
      "is working, an update is staged, and the restart control is REMOVED rather than " +
      "disabled — 'instead of a regular update button, they'd get some other visual cue but " +
      "not an actionable thing'. So there is deliberately nothing to press, and the sentence " +
      "is INFORMATIONAL rather than ACTIONABLE: it is RichOS saying it will act by itself. " +
      "Not NEEDS-SOMEONE-ELSE either — nobody is being waited on but RichOS. It said 'I'll " +
      "wait to restart until everything has finished' until 01e9b8d8 ended the restart.",
  },
  {
    s: "I'll wait until everything has finished, so nothing will be interrupted.",
    c: "INFORMATIONAL",
    fixture: "updates-waiting-available",
    why:
      "The same ruling in the `available` state, and mode-proof on purpose: it does not say " +
      "the button comes back and it does not say RichOS will install by itself, because each " +
      "of those is true of exactly one of the two update modes.",
  },
  {
    s:
      "There is no update server yet, so RichOS cannot check for new versions. Where updates " +
      "are published has not been decided.",
    c: "INFORMATIONAL",
    fixture: "updates-unconfigured",
    why:
      "The `unconfigured` state, and the honest one: this build points at a placeholder " +
      "endpoint. NOT ACTIONABLE — `paint()` disables `#update-check` in exactly this state, " +
      "so there is no control and the row does not claim one. NOT NEEDS-SOMEONE-ELSE either, " +
      "and that is the interesting half: there is no party to name. It is not an operator's " +
      "setting he could be pointed at, it is a decision the RichOS project has not taken, so " +
      "a sentence naming 'whoever set RichOS up' would be inventing an owner. It states the " +
      "fact and stops, which is what an INFORMATIONAL state is for.",
  },
  {
    s: "RichOS has not checked for updates yet.",
    c: "INFORMATIONAL",
    fixture: "updates-idle",
    why:
      "The `idle` state on a launch that has not reached its three-second check. 'Never " +
      "checked' is deliberately a DIFFERENT state from 'checked, and you are current' — a " +
      "row that collapsed the two would be silent about an updater that had stopped running.",
  },
  {
    s: "Checking for updates…",
    c: "INFORMATIONAL",
    why: "The `checking` state. Transient, self-resolving, and nothing is asked of anyone.",
  },
  {
    s: "Checking and preparing the update…",
    c: "INFORMATIONAL",
    why:
      "The `installing` headline. Bytes are already moving; nothing is waiting on him. It " +
      "says PREPARING rather than installing since 01e9b8d8, because the bundle is staged " +
      "for the next launch rather than swapped underneath a running app.",
  },
  {
    s: "RichOS is confirming this update was signed by us before preparing it for the next launch.",
    c: "INFORMATIONAL",
    why:
      "The `installing` sub-line, and it is on screen for a reason: the signature check is " +
      "the property the whole updater rests on, and a step nobody is told about is a step " +
      "nobody misses when it stops happening.",
  },
  {
    s: "RichOS cannot tell what state the update is in.",
    c: "INFORMATIONAL",
    why:
      "The `default` arm — a state the backend sent that this file does not know. It reports " +
      "itself as unknown, which the file's own comment calls the alternative to 'the single " +
      "worst answer this file could give', namely falling back to 'up to date'. `#update-" +
      "check` is visible and enabled here, so he is not stuck; the sentence itself asks " +
      "nothing and claims nothing.",
  },
  {
    s: "It has been ready for a day.",
    c: "INFORMATIONAL",
    why:
      "`readyFor()` at exactly one day. It appears only after a day and never under one, so " +
      "it is not a line that is always there, and it is a fact rather than a nudge — the " +
      "control it would nudge toward is either present already or deliberately absent.",
  },
  {
    s: "Check for updates",
    c: "CONTROL",
    why: "`#update-check`'s label in every state but a non-signature failure, where it reads 'Try again'.",
  },
  {
    s: "Show the technical reason",
    c: "CONTROL",
    why: "`#update-why`'s collapsed label — the vendor's own error text, one press behind the sentence.",
  },
  {
    s: "Hide the technical reason",
    c: "CONTROL",
    why: "The same button once expanded. Both labels ship, so both are in the inventory.",
  },
  {
    s: "Downloading the update",
    c: "CONTROL",
    why:
      "The `aria-label` of `#update-progress`. It is the accessible name of an element, which " +
      "is what this bucket is for — but the element is a `progressbar`, not something to " +
      "press, so it names no affordance and needs none.",
  },
  {
    s: "Open the update settings.",
    c: "CONTROL",
    why:
      "The tail of `#update-cue`'s accessible name, composed as `said + ' Open the update " +
      "settings.'`. The visible label is a STATEMENT ('RichOS 0.1.2 is available.') and a " +
      "button's name should say what pressing it does, so the name is the statement plus the " +
      "act — WCAG 2.5.3 'Label in Name' is why the visible words come first and verbatim.",
  },
  {
    s: "See what’s new",
    c: "CONTROL",
    why:
      "`#update-notes`'s visible label (updates.js `changelogLink`, round 34): the link after " +
      "Check for updates on the Updates line. It is itself the control: a click or Enter opens " +
      "the RichOS changelog in the default browser through the `open_external` allowlist key, " +
      "as the setup sheet's pricing link does.",
  },
  {
    s: "See what’s new (opens in your web browser)",
    c: "CONTROL",
    why:
      "`#update-notes`'s `aria-label`: the visible words first (WCAG 2.5.3 'Label in Name'), " +
      "then where the link goes. The accessible name of the same link, not a state.",
  },
  {
    s: "You are running",
    c: "FRAGMENT",
    why: "`'You are running ' + currentVersion + '.'` — the `available` sub-line's tail, never alone.",
  },
  {
    s: "You are still running RichOS",
    c: "FRAGMENT",
    why: "`'You are still running RichOS ' + currentVersion + '.'` — the `failed` sub-line.",
  },
  {
    s: "It has been ready for",
    c: "FRAGMENT",
    why: "`'It has been ready for ' + days + ' days.'` — the plural arm of `readyFor()`.",
  },
  {
    s: "1 minute ago",
    c: "FRAGMENT",
    why: "`when()`'s singular arm, composed into `'Checked ' + when(...) + '.'` and never rendered alone.",
  },
  {
    s: "is up to date.",
    c: "FRAGMENT",
    why: "`current + ' is up to date.'` — the `upToDate` headline's tail.",
  },
  {
    s: "is ready to install.",
    c: "FRAGMENT",
    why: "`waitingSaid()`'s `available` head: `version + ' is ready to install.'`.",
  },
  {
    s: "is ready for the next launch.",
    c: "FRAGMENT",
    why:
      "One literal, used by both `waitingSaid()`'s `ready` head and `sub()`'s `ready` " +
      "headline: `version + ' is ready for the next launch.'`. It read 'is installed and " +
      "needs a restart.' until 01e9b8d8 — the same sentence making a promise the product " +
      "stopped keeping, because there is no restart to need.",
  },

  // ---- the relaunch into a downloaded update (CEO feedback 2026-10-06, item 1) ---------
  {
    s: "Restarting into RichOS",
    c: "FRAGMENT",
    why:
      "The `restarting` headline: `'Restarting into RichOS ' + v.availableVersion + '…'`, also " +
      "the pill's label in that state. The short notice before `updates.rs` relaunches into an " +
      "update this app downloaded once nothing is running. tests/updates.js check 22.",
  },
  {
    s: "Nothing is running, so RichOS is restarting to finish the update. Your window and conversation will be right where you left them.",
    c: "INFORMATIONAL",
    fixture: null,
    why:
      "The `restarting` sub line and the pill's accessible name: RichOS is about to quit and " +
      "come back on the new version, by itself, within seconds. Nothing is asked of anyone and " +
      "there is no control to offer: the relaunch is the product acting on the CEO's own words " +
      "('the app should automatically re-launch immediately after downloading the new " +
      "version'). tests/updates.js check 22 renders it.",
  },
  {
    s: "Nothing is running, so RichOS will restart into it in a moment.",
    c: "INFORMATIONAL",
    fixture: null,
    why:
      "The `ready` sub line for an update this app downloaded while nothing runs, for the " +
      "instant before `restarting` arrives. A statement of what RichOS does next; no request.",
  },
  {
    s: "RichOS will restart into this update as soon as that work is done. Nothing will be interrupted.",
    c: "INFORMATIONAL",
    fixture: null,
    why:
      "The work gate's clause for a `ready` update this app downloaded while a turn or worker " +
      "runs, in the row and in the waiting cue: the relaunch waits for the work and then " +
      "happens by itself (`updates.rs` watcher). INFORMATIONAL for the reason the other " +
      "waiting sentences are: the control is removed by design and nothing is asked of anyone. " +
      "tests/updates.js check 22.",
  },

  // ---- update_startup.rs — the activation that runs BEFORE there is a webview -----------
  //
  // NINE STRINGS, ONE CLASSIFICATION, AND THE REASON IS ONE CALL SITE. `update_startup::
  // prepare()` runs at main.rs:898, ahead of home resolution, leases, Tauri and every native
  // worker — its own header says so. Its whole error type is `Result<_, String>`, and
  // main.rs:900-903 is the only thing that ever reads one:
  //
  //     Err(error) => { eprintln!("[richos] application startup: {error}"); return; }
  //
  // `eprintln!` then `return`, before a window exists. So not one of these sentences can
  // reach the DOM, and NOT-RENDERED is the honest class rather than a convenient one — the
  // claim is checkable at that line and fails the moment somebody routes one to the webview.
  //
  // WHAT THAT MEANS FOR THE PERSON, said here rather than filed as covered: an activation
  // that fails this way is an app that does not open, with the reason on a stderr stream
  // nobody is reading. That is a product question about a startup path, not a gap in this
  // rule, and this rule is the wrong place to fix it. It is named in the QA report that
  // added these rows so it is somebody's rather than nobody's.
  {
    s: "Update redirect did not enter an application bundle.",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:52, when the redirected executable is not inside a bundle. Read only by main.rs:900-903 — `eprintln!` and return, before Tauri starts.",
  },
  {
    s: "Application home is unavailable.",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:55, when `HOME` is unset. Same single call site, same `eprintln!` and return, no webview yet.",
  },
  {
    s: "Redirected update changed application data scope.",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:62 — a redirect that would move the user's data. Refused before anything opens; main.rs:900-903 prints it and returns.",
  },
  {
    s: "Invalid update session descriptor.",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:69, on an unparseable inherited `RICHOS_UPDATE_SESSION_FD`. Never leaves stderr.",
  },
  {
    s: "Update activation needs recovery before the app can start: {error}",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:92. A `{error}` Debug hole for an engineer, and unreachable by the webview besides — main.rs returns rather than opening a window.",
  },
  {
    s: "redirected executable disagrees with its verified release",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:105 — the identity check that stops a swapped binary. Fails the launch on stderr; nothing renders it.",
  },
  {
    s: "Changed application cannot redirect to an older release",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:111, refusing a downgrade. Same call site, same stderr-and-return.",
  },
  {
    s: "Changed application has no publication receipt.",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:125 — an activated bundle with no receipt to check. Refused before the window opens.",
  },
  {
    s: "The activated application could not start: {error}",
    c: "NOT-RENDERED",
    why: "`prepare()` at update_startup.rs:132, when the re-executed bundle will not run. The last thing this process does is print it.",
  },

  // ---- startup_alert.rs — the surface the nine rows above did not have -----------------
  //
  // WHAT CHANGED UNDER THE NINE ROWS ABOVE, said here because their line numbers and their
  // "stderr and return" wording predate it. echo-opus-st1 (b052f0b0, merged 2026-09-10)
  // routed that call site through `startup_alert::cannot_start` (now main.rs:934): the
  // engineer's sentence — which is where the nine strings above go — is written to stderr
  // AND to `~/Library/Logs/RichOS/startup.log`, the process exits 1 rather than 0, and the
  // person gets a native system alert carrying ONE fixed sentence instead. So the nine are
  // still NOT-RENDERED, now for a stronger reason: they are the log's text, and the screen
  // shows a different, deliberately generic sentence.
  //
  // The two constants below are what that alert says, and they ARE read by the person —
  // so NOT-RENDERED here means exactly the bucket's definition, "never reaches the DOM",
  // and must not be read as "nobody sees it". The surface is `CFUserNotificationDisplayAlert`
  // (`show_alert`, startup_alert.rs:408-503), raised only while the app has no window (armed
  // at `install`, disarmed at boot complete). This WebKit suite cannot render a CoreFoundation
  // alert, so the rule's question is answered on that surface instead, and the answer is
  // recorded here so a reviewer can check it: the alert carries "OK" and "Show Details", and
  // "Show Details" reveals the log in Finder (`/usr/bin/open -R`, startup_alert.rs:492-499).
  // Classified by echo-opus-ci1; these two are what turned `affordances.js` red on main,
  // first at the echo-opus-st1 merge 8cb30972 (ui-suite-ci run 34458859801).
  {
    s: "RichOS could not open",
    c: "NOT-RENDERED",
    why:
      "`startup_alert::HEADLINE` (startup_alert.rs:150) — the header of the native alert for " +
      "every member of the can't-open class. Read by the person, never by the DOM: it is " +
      "shown only while there is no window to render anything in. See the section note for " +
      "what the alert offers him.",
  },
  {
    s:
      "RichOS ran into an unexpected problem while starting up and closed itself rather " +
      "than open a window that would not work.",
    c: "NOT-RENDERED",
    why:
      "`startup_alert::PANIC_SENTENCE` (startup_alert.rs:155), the body the panic hook " +
      "passes to `cannot_start` for any panic before boot completes. Same native alert, " +
      "same two buttons, and the log's path is appended to it by `person_message` " +
      "(startup_alert.rs:238) so the details are named on the surface itself. Nothing in " +
      "it asks him to act; the fault is not his.",
  },

  // ---- the two remaining update strings, and they are NOT alike -------------------------
  {
    s: "RichOS could not prepare this update.",
    c: "ACTIONABLE",
    control: "#update-check",
    fixture: "updates-install-failed",
    why:
      "`INSTALL_HEADLINE` in updates.rs (`Failure::of_journal`) — the `failed` state with kind " +
      "`install`, which is every update-record or staging failure that is not a refused " +
      "signature and not a busy record (the row below). He can " +
      "act, and the control is the same one the network failure names: `isSignature(v)` is " +
      "false here, so `paint()` keeps `#update-check` visible and relabels it 'Try again'. " +
      "It gets its OWN fixture rather than borrowing `updates-failed`, because the row " +
      "claims this headline renders with that control and the network fixture renders a " +
      "different headline.",
  },
  {
    s: "RichOS is busy with another update task. Try again in a minute.",
    c: "ACTIONABLE",
    control: "#update-check",
    fixture: "updates-busy",
    why:
      "`BUSY_HEADLINE` in updates.rs, the `failed` state with kind `busy`: the update record's " +
      "lock stayed held for the updater's whole wait (`richos_user_update::PUBLICATION_WAIT`, " +
      "5 s) by another part of RichOS. Nothing failed, so it is said as busy; until 2026-10-21 " +
      "this was 'RichOS could not prepare this update.' over 'Resource temporarily unavailable " +
      "(os error 35)' on a plain check. He can act: `isSignature(v)` is false, so `paint()` " +
      "keeps `#update-check` visible as 'Try again', which the sentence names.",
  },
  {
    s: "Password-free application updates are not implemented on this platform.",
    c: "UNREACHABLE",
    why:
      "updates.rs:593, and the guard above it is the whole reason: the literal is inside " +
      "`#[cfg(not(target_os = \"macos\"))]`, so it is not compiled into the build this app " +
      "ships. RichOS is macOS-only at v1 — `app/README.md` and `windows-companion-ci.yml` " +
      "both say so — which makes this a sentence that exists for a port that does not exist " +
      "yet. UNREACHABLE rather than NOT-RENDERED: it WOULD render on the update row if such " +
      "a build were ever made, and on that day it needs a class of its own and a party.",
  },

  // ---- the way back off a bad release (2026-09-20) --------------------------------------
  //
  // SIX STRINGS, AND NOT ONE OF THEM ASKS ANYTHING OF HIM. That is the point of the
  // classification rather than an accident of wording: the ACT is the "Go back to <version>"
  // control, which `paint()` offers in `idle`/`upToDate`/`available`/`failed`; everything
  // below is what the row says AFTER he has pressed it, while RichOS does the rest. The same
  // shape, and the same classes, as the update sentences directly above — a rollback and an
  // update are one exchange of two directories, and a surface that treated them as two
  // different kinds of event would eventually describe them inconsistently.
  {
    s: "RichOS will go back to",
    c: "FRAGMENT",
    why:
      "One literal used three times — `sentences()`'s `ready` headline, `cue()`'s pill and " +
      "`waitingSaid()`'s `ready` head — always completed with the version and 'when you " +
      "next open it.'. The counterpart of 'is ready for the next launch.' on the way down, " +
      "and deliberately a different sentence rather than the same one with a smaller " +
      "number: 'ready' says something arrived, and going back is not an arrival.",
  },
  {
    s: "when you next open it.",
    c: "FRAGMENT",
    why:
      "The tail of the sentence above (updates.js:263 and :781). Split only by the version " +
      "in the middle; it never appears alone.",
  },
  {
    s: "It will go back automatically next time RichOS opens. Your work will continue uninterrupted.",
    c: "INFORMATIONAL",
    fixture: "updates-rolling-back",
    why:
      "The `ready` sub-line for a staged rollback, and INFORMATIONAL for exactly the reason " +
      "its update twin four hundred lines above is: the control is REMOVED by design rather " +
      "than missing, nothing is asked of anyone, and the party that will act is RichOS at " +
      "the next launch. Not NEEDS-SOMEONE-ELSE — nobody is being waited on.",
  },
  {
    s: "RichOS is confirming this earlier version was signed by us before preparing it for the next launch.",
    c: "INFORMATIONAL",
    why:
      "The `installing` sub-line while a rollback is verified. Its update twin says 'this " +
      "update'; this one says 'this earlier version', because THE SIGNATURE CHECK IS THE " +
      "SAME CHECK and the sentence has to be able to say so without calling a downgrade an " +
      "update. Bytes are already moving; nothing is waiting on him.",
  },
  {
    s: "Checking and preparing RichOS",
    c: "FRAGMENT",
    why:
      "The `installing` headline for a rollback, completed with the version and an ellipsis " +
      "(updates.js:254). The update arm says 'the update' and has nothing to name; this one " +
      "names the version, because 'preparing the update' over a downgrade would be wrong.",
  },
  {
    s: "Going back to an earlier version is not implemented on this platform.",
    c: "UNREACHABLE",
    why:
      "updates.rs's `rollback`, inside `#[cfg(not(target_os = \"macos\"))]` exactly as the " +
      "'Password-free application updates' row above is, and for the same reason: the " +
      "literal is not compiled into the build this app ships. It is worded as its own " +
      "sentence rather than reusing that one because a person meeting it is asking about a " +
      "different act. Same day as that row's port, same class, same party.",
  },

  // ---- home.js — the home screen the CEO lands on ---------------------------------------
  {
    s:
      "Every button shows a number and clicking one slides that company's name out. Give a " +
      "button its own label here instead or take it off the home screen. This changes the " +
      "button only. The company itself and everything filed under it, stays exactly as it " +
      "is.",
    c: "ACTIONABLE",
    control: "#home-prefs-list input.home-prefs-label",
    fixture: "home-prefs",
    why:
      "The company-buttons dialog's note, and it is an instruction in two clauses — 'Give a " +
      "button its own label here instead, or take it off the home screen'. Both controls are " +
      "in the same dialog: the text box named here, and the checkbox beside it. The text box " +
      "is the one the first clause points at and the one whose absence would make the " +
      "sentence a lie.",
  },
  {
    s: "I couldn't draw the picture on this display. Everything else works.",
    c: "INFORMATIONAL",
    why:
      "`degrade()` — no WebGL, a script that would not load, a shader that would not " +
      "compile. Deliberately not ACTIONABLE and deliberately not an apology with a retry " +
      "button: there is nothing he can do about a display that cannot run the composition, " +
      "and the second clause is the part that matters, because the way through to the app " +
      "stays reachable. `loading.style.pointerEvents = 'none'` is that promise in code.",
  },
  {
    s: "This is what your home screen could look like once Rich knows enough about you and your business.",
    c: "INFORMATIONAL",
    why:
      "The note under the demonstration composition. On an open-source launch it is the " +
      "first sentence a stranger reads inside RichOS, and it deliberately carries no " +
      "asterisk, no 'demo mode' and nothing to dismiss.",
  },
  {
    s: "I can't read your companies right now, so there is nothing to change here yet.",
    c: "INFORMATIONAL",
    fixture: "home-prefs-no-entities",
    why:
      "`prefsFoot()` with zero rows — the backend has not answered, or answered with " +
      "nothing. It is INFORMATIONAL rather than a failure with a retry because the read " +
      "retries itself and the dialog is already open on his own action; what it must not do " +
      "is render an empty list and let him think he has no companies.",
  },
  {
    s: "With one company shown, the buttons are off the home screen. A row of one is just noise.",
    c: "INFORMATIONAL",
    why:
      "`prefsFoot()` at one visible row. The CEO's rule — 'the company buttons should only " +
      "appear if the user has more than one company' — stated where the control that " +
      "produced it is, so a row disappearing is explained rather than mysterious.",
  },
  {
    s: "With no companies shown, the buttons are off the home screen.",
    c: "INFORMATIONAL",
    why: "The same line at zero visible rows, said without the aside that only fits the one-row case.",
  },
  {
    s: "RichOS: go to the home screen",
    c: "CONTROL",
    why:
      "The rail wordmark's accessible name once `bindWordmark()` gives it `role=button`. It " +
      "is the CEO's own instruction — 'a click on the logo (in the upper left corner) brings " +
      "the user back to the home screen' — and the name says where it goes rather than what " +
      "it is a picture of.",
  },
  {
    s: "Which company this picture is of",
    c: "CONTROL",
    why:
      "`#home-entities-label`, the accessible name of the entity row's `role=group`. It " +
      "labels a set of controls; it is not a state anything can be in.",
  },
  {
    s: "Company buttons on the home screen",
    c: "CONTROL",
    why:
      "`#home-prefs-title`, and the dialog's own accessible name through `aria-labelledby`. " +
      "A heading that names the surface it opens, not a state the surface can be in.",
  },
  {
    s: "button says on the home screen",
    c: "FRAGMENT",
    why:
      "`'What the ' + name + ' button says on the home screen'` — each label box's accessible " +
      "name, composed around the company's name and never rendered without it.",
  },
  {
    s: "showing on the home screen.",
    c: "FRAGMENT",
    why: "`shown + ' of ' + rows.length + ' showing on the home screen.'` — `prefsFoot()`'s ordinary arm.",
  },
  {
    s: "did not load",
    c: "NOT-RENDERED",
    why:
      "`new Error(src + ' did not load')` in the script loader. It reaches `state.fieldError` " +
      "and stops there: `degrade()` records the reason and paints its own fixed sentence, so " +
      "no path puts this text on screen. Verified by reading every use of `state.fieldError` " +
      "— it is written at home.js:815 and :1234 and read nowhere.",
  },
  {
    s: "the picture did not start within 8 seconds",
    c: "NOT-RENDERED",
    why:
      "`settled()`'s deadline rejection, and the bound on how long the honest failure can " +
      "take. Same path as above: it becomes `state.fieldError`, which nothing renders.",
  },
  {
    s: "the home screen would not build:",
    c: "NOT-RENDERED",
    why:
      "The `start()` catch. A home screen that will not build leaves NOTHING behind and the " +
      "app boots normally, so there is no surface for this to be printed on — which is the " +
      "posture `splash.js` takes for the same reason.",
  },

  // ---- home/field-engine.js — the WebGL composition's own text --------------------------
  {
    s: "loro · <span class=\"v\" data-k=\"months\"></span> months · <b><span class=\"v\" data-k=\"memories\"></span> memories</b>",
    c: "INFORMATIONAL",
    why:
      "`#home-brand-line`. It renders as 'loro · 14 months · 7,500 memories' — the two " +
      "numbers are filled by `setSignals()` and the markup is the sentence's skeleton. " +
      "Nothing is asked; it is the composition saying what it is a picture of.",
  },
  {
    s: "· working now",
    c: "FRAGMENT",
    why: "`'· ' + N + ' working now'`, the small line under the specialist count. Never alone.",
  },
  {
    s: "tasks handled without you",
    c: "FRAGMENT",
    why: "A signal's label, rendered under its number by `sig()`. The number is the sentence.",
  },
  {
    s: "of your attention saved",
    c: "FRAGMENT",
    why: "The same shape: `'1,234 h'` above, this beneath it.",
  },
  {
    s: "an email thread",
    c: "FRAGMENT",
    why:
      "`SOURCE_WORD['email-thread']`, composed into the learning ticker — 'Ash learned from " +
      "an email thread · Finance → 3 new memories'. One noun out of a ten-entry map.",
  },
  {
    s: "· <i></i> → new memor",
    c: "FRAGMENT",
    why:
      "The ticker's own skeleton between its interpolations, caught by the scrape as the " +
      "literal run of text between `${}` holes. It is three fragments of one sentence, and " +
      "the sentence is the row above.",
  },
  {
    s: "the picture could not start",
    c: "NOT-RENDERED",
    why:
      "The last-resort value of `window.__loroFailed` when the thrown error carried no " +
      "message. `home.js`'s `settled()` reads it, rejects, and `degrade()` paints its own " +
      "sentence — so this string is a diagnostic the CEO never sees.",
  },

  {
    s:
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), ' +
      'textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    c: "NOT-RENDERED",
    why:
      "`home.js`'s FOCUSABLE — the CSS selector the home screen uses to find the control a " +
      "desk sheet would have focused itself, had the screen let it. A selector, never prose.",
  },

  {
    s: "It has reached this Mac. Check that the six words on it are the six words below, then press They match.",
    c: "ACTIONABLE",
    control: "#phone-mac-match",
    why:
      "The phone card while a phone that redeemed the code waits for the press ON THIS MAC — " +
      "Sage's pairing review F1 (High), 2026-09-24: the phone's own confirmation was the only one " +
      "the Mac recorded, so a device that paired first with a code seen on a screen share could " +
      "confirm itself. The answer is now given here: They match (#phone-mac-match) activates the " +
      "phone, They do not match (#phone-mac-mismatch) runs the rejection teardown, and both are " +
      "on the card beside the six words. Until then the phone's key reaches nothing. ui/tests/" +
      "phone.js checks 9e and 9g press both and measure the sentence and the buttons in both themes.",
  },
  {
    s: "This Mac accepted it. Finish on your phone by pressing They match there too.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "the paired card's own name line above it, which names the phone that still has to answer",
    why:
      "The phone card after the press on this Mac and before the phone's own answer arrives " +
      "(Sage F1): the Mac's half is done, and the remaining act is on the phone the heading names. " +
      "The card polls until the phone answers, then says It is paired.",
  },
  {
    s: "This Mac accepted it. Waiting for your phone to finish.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "the paired card's own name line above it, which names the phone that has not used the pairing yet",
    why:
      "Sage's pair-v2 hypotheses review §2, 2026-09-24: both presses are in and the phone has not " +
      "made its first ordinary request yet (`PhoneStatus.completed === false`). The press on this " +
      "Mac is not evidence the phone heard it — a phone that lost the Mac throws its key away at " +
      "its deadline — so `It is paired` waits for the phone to use the pairing. Nothing is his to " +
      "do here: the remaining act is the phone's, which the card names; the card polls, and moves " +
      "to `It is paired` or to the stopped screen by itself. Forget this phone stays on the card. " +
      "ui/tests/phone.js check 9e reads the sentence.",
  },
  {
    s: "Someone used this phone's pairing code again after you paired it. If you did not expect that, press Forget this phone and pair again where nobody else can see this screen.",
    c: "ACTIONABLE",
    control: "#phone-forget",
    why:
      "Sage's spent-code alarm, the keep-it-and-warn half: a second correct use of the pairing " +
      "code AFTER the person confirmed the phone on this Mac. The phone is kept (he confirmed it), " +
      "and the card says what happened and names the control that undoes it, which is on the " +
      "same card. ui/tests/phone.js check 9h.",
  },
  {
    s: "It is paired. Open Rich on it and keep talking.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "the paired card's own name line above it, which names the phone that answered",
    why:
      "The phone card once the phone has confirmed the six words: the next act is on the phone, " +
      "which the sentence names; the Mac's own controls here are Forget this phone and Close.",
  },

  // ---- settings-button.js — the universal settings button, CEO ruling §15 ----------------
  {
    s:
      "That was set when RichOS was started up, from outside this window, so it can't be " +
      "changed from in here. Whoever set RichOS up is the one who changes it.",
    c: "NEEDS-SOMEONE-ELSE",
    why:
      "The pinned-company row's `title`. He cannot change it and the sentence names who can, " +
      "which is the whole of this bucket's contract. It is also the row `affordances.js` " +
      "already drove by hand — 'a company pinned outside the window is stated, never offered " +
      "as a dead control' — with a comment saying the string inventory could not see it. It " +
      "can now, so the hand-written cover and the derived rule agree on the same sentence.",
  },
  {
    s:
      "Got it. The bug report starts from this exact screen, as it stands. Nothing leaves " +
      "this machine until you say so.",
    c: "INFORMATIONAL",
    why:
      "The 'Bust a bug' acknowledgement toast. What the button opens is not designed yet, and " +
      "a control that appears to do nothing is indistinguishable from a broken one — so it " +
      "says the one thing that IS decided. Nothing is asked of him; the second sentence is a " +
      "promise, not an instruction.",
  },
  {
    s: "Follow the system",
    c: "CONTROL",
    why:
      "The accessible name of the theme segment's middle option. §15 makes `system` something " +
      "he can pick and never the thing he gets without picking, which is why it is a control " +
      "and not a state.",
  },

  // ---- splash.js and splash-library.js — the opening curtain ----------------------------
  {
    s: "The Operating System for the AI-Enabled CEO",
    c: "INFORMATIONAL",
    why:
      "The only text on the opening curtain, at 18px — above §15's 16px floor for text meant " +
      "to be read. The curtain is `pointer-events: none` for its whole life and yields on the " +
      "first of three signals, so there is nothing on the surface to act on by construction.",
  },
  {
    s: "Splash screen #1: the ruled standard, the rule struck along its own ghost",
    c: "NOT-RENDERED",
    why:
      "A library entry's `name` field. `splash.js` reads `id`, `seconds` and `tokens` from an " +
      "entry and never `name` — it is there so a person opening `splash-library.js` can tell " +
      "the two approved compositions apart. Verified: `entry.name` appears nowhere in " +
      "splash.js.",
  },
  {
    s: "Splash screen #2: midnight suede, the strap sewn live in gold thread",
    c: "NOT-RENDERED",
    why: "The second entry's `name`, on the same footing as the first.",
  },
  {
    s: "not a fresh launch (",
    c: "NOT-RENDERED",
    why:
      "`state.declined`, which records WHY the curtain did not run. It is read by " +
      "`RichSplash.state()` for the suite and painted nowhere — the whole point of the branch " +
      "is that a crash-restart costs nothing at all, and printing an explanation would be a " +
      "cost. Verified: every use of `state.declined` in splash.js is a write.",
  },
  {
    s: "no usable variation in the library",
    c: "NOT-RENDERED",
    why: "The same field, for a missing, unparseable or wholly malformed library.",
  },
  {
    s: "the chosen variation would not render:",
    c: "NOT-RENDERED",
    why:
      "The same field again, after `build(entry)` threw. `removeSelf()` runs first, so the " +
      "launch is a normal one with nothing drawn and there is no curtain left to print on.",
  },

  // -------------------------------------------------------------------------------------
  // WHY HIS OWN LORO IS NOT THE PICTURE (`home_field_data`, 2026-09-06)
  //
  // Four reasons the home screen keeps drawing the demonstration. NOT ONE OF THEM REACHES
  // THE DOM, and that is the design rather than an oversight: the demonstration is what the
  // CEO asked to keep — *"The demo is definitely needed, initially, for the user"* — so a
  // launch that draws it has nothing to report. There is no failure to explain, no request
  // to make of him, and nothing for him to act on; the screen already carries the one
  // sentence that matters, which is the first-run banner.
  //
  // Two land on `RichHome.state.fieldOffer`, which the acceptance suite reads and the CEO
  // never sees. Two are `eprintln!` in the boot log, which is the operator's window and not
  // his — the same split `MemoryStatus::detail` already uses.
  // -------------------------------------------------------------------------------------
  {
    s: "the backend said nothing about a corpus",
    c: "NOT-RENDERED",
    why:
      "`home.js`'s own words for an answer that was not a positive `available: true` with a " +
      "dataset attached. Recorded on `state.fieldOffer.reason` for the acceptance suite; " +
      "nothing renders it, because the screen is drawing the demonstration and that is what " +
      "it is supposed to be doing.",
  },
  {
    s: "the corpus did not compile within",
    c: "NOT-RENDERED",
    why:
      "The head of `home.js`'s timeout sentence (it continues with the millisecond ceiling). " +
      "Same destination: `state.fieldOffer.reason`, read by the suite and by nobody else. A " +
      "timeout is not evidence of anything, so the demonstration stays and says nothing new.",
  },
  {
    s: "the corpus could not be read: {e}",
    c: "NOT-RENDERED",
    why:
      "`main.rs::home_field_data`, the census-probe failure, returned as `reason` on an " +
      "`available: false` answer and printed to the boot log beside it. Machine-facing: `{e}` " +
      "is a compiler's stderr line, which is not CEO copy — the same split `MemoryStatus` " +
      "makes between its `detail` and the surface's own sentence.",
  },
  {
    s: "the corpus could not be compiled: {e}",
    c: "NOT-RENDERED",
    why:
      "The same, for the arm where every topic failed to compile. Neither string is shown to " +
      "the CEO and neither asks anything of him; whoever set RichOS up reads them in the " +
      "boot log.",
  },
  // -------------------------------------------------------------------------------------
  // THE WAITING STATE (main.js "THE WAITING STATE", 2026-09-06)
  // -------------------------------------------------------------------------------------
  //
  // Eight strings, added because the first outside user of RichOS said a long turn "looks
  // like a crashed application" and the app had one 14px line and a 5px dot to answer him
  // with. Every one is a STATEMENT of something observed — none asks the CEO for anything,
  // and none can be made true or false by an act of his. `#stop` is visible for the whole
  // of every live turn (`syncComposerMode`), so a CEO who wants the turn back always has
  // the control; it is simply not what any of these sentences is about.
  //
  // Kept together at the end of the file, on purpose: `app/ui/tests/**` belongs to another
  // engineer this session, so this block is strictly additive and touches no existing row.
  {
    s: "Rich is working",
    c: "INFORMATIONAL",
    why:
      "The waiting band's headline for `rich://turn-status: working` once something has come " +
      "back from Rich — the ledger's own transition, read back out of the ledger before it is " +
      "emitted (`spine.rs` `turn_status_event`). A statement about a turn, with nothing in it " +
      "for him to do.",
  },
  {
    s: "Rich is thinking",
    c: "INFORMATIONAL",
    why:
      "The same band's headline, in the CEO's words (2026-10-17), until anything comes back " +
      "from Rich: `queued`, and `working` with no reply, step or worker row yet. Nothing to " +
      "act on — the turn moves itself on.",
  },
  {
    s: "Waiting to start",
    c: "INFORMATIONAL",
    why:
      "The conversation row's queued duration label: the message is recorded and work has " +
      "not begun. It reports; it does not ask.",
  },
  {
    s: "Rich is picking this back up",
    c: "INFORMATIONAL",
    why:
      "The headline for `rich://turn-status: recovering` — a turn whose lease died and is " +
      "being replayed. The replay is automatic (`spine.rs`), so there is nothing to press.",
  },
  {
    s: "Rich is letting go of this turn",
    c: "INFORMATIONAL",
    why:
      "The headline while a CEO stop is in flight, set from `stop_turn`'s durable answer " +
      "rather than an event (the same source `timeline.js`'s `stopping` row uses). He has " +
      "already acted; this is the acknowledgement of it.",
  },
  // REMOVED 2026-09-20: "Nothing has come back yet". The product no longer renders it, so the
  // row goes with it — part 1's exact set comparison is the thing that makes this registry a
  // record of the shipped product rather than of its history. What used to occupy this state
  // (zero content events on a turn watched from its start, under the quiet threshold) is now
  // an EMPTY detail, which is not a string and therefore has no row: Ray's `.20260920.1` walk
  // measured the sentence on the real window for 10.5-14.5 s on five healthy turns and named
  // it a report of absence sitting where the eye settles (defect 2 of
  // `docs/verification/2026-09-20-nightly-1.2.0-nightly.20260920.1-mac-to-phone-in-the-vm-audit.md`).
  // The silence is still named, past QUIET_AFTER_MS, by "Nothing new for" below.
  {
    s: "Nothing new for",
    c: "FRAGMENT",
    why:
      "Template hole: completed by a formatted duration (`Nothing new for 41s`). The whole " +
      "sentence is informational — it says how long it has been since the last event the " +
      "timeline accepted for this thread, which is true at every value it can take.",
  },
  {
    s: "Writing the reply",
    c: "INFORMATIONAL",
    why:
      "What `rich://message-started` / `message-delta` licenses and nothing more. `phase` is " +
      "`unknown` on every message this runtime emits (`live.rs`), so naming a kind of " +
      "writing would be inventing one.",
  },

  // ---- THE FIRST-RUN NOTICE (2026-09-06) ------------------------------------------------
  //
  // The visible half of the onboarding offer. Every sentence of the offer is ACTIONABLE and
  // every one of them names a control that is on screen beside it, because that is the whole
  // shape of this surface rather than a property it happens to have: the notice exists to put
  // a state the CEO can change next to the two things that change it.
  //
  // Two of its states are NEEDS-SOMEONE-ELSE and both name the party in their own words. The
  // `PARTY` regex gained `Whoever set RichOS up` on the same day for that reason — it already
  // carried both cases of "operator" and only one of this phrase.
  {
    s: "I don't know your business yet.",
    c: "ACTIONABLE",
    control: "#first-run-start",
    fixture: "first-run-offer",
    why:
      "The headline of the first-run notice. It is a state he can change — that is the point " +
      "of it — and the control that changes it is in the same panel, two lines below.",
  },
  {
    s:
      "There's nothing on file about what this company does, who it's for or how you want to " +
      "work. I can ask you about it (about twenty minutes) and write your answers down, so I " +
      'use them from then on. You can stop partway and "not sure yet" is a real answer to any ' +
      "of it.",
    c: "ACTIONABLE",
    control: "#first-run-start",
    fixture: "first-run-offer",
    why:
      "The offer itself. It names the cost in his own units, the one promise the chain can " +
      "keep, and that stopping partway is honest — and it renders with the control that " +
      "starts it.",
  },
  {
    s: "\"Not now\" means I'll stop offering. You can start it any time by asking.",
    c: "ACTIONABLE",
    control: "#first-run-later",
    fixture: "first-run-offer",
    why:
      "What the second control DOES, stated beside it. The write is durable and a " +
      "two-syllable label cannot carry that and stay speakable, so the effect is in words " +
      "and the control it describes is the one it names.",
  },
  {
    s: "Start the questions",
    c: "CONTROL",
    why: "The label of `#first-run-start`.",
  },
  {
    s: "Let's do the twenty minutes of questions about my business.",
    c: "CONTROL",
    why:
      "TWO THINGS AT ONCE, so both are said here rather than forced into one bucket. It is " +
      "not a state of the app and never renders as one: it is the payload `#first-run-start` " +
      "sends through the ordinary `send()` path, and it renders as the CEO's own turn in the " +
      "conversation, because he did send it — he pressed a button whose label is exactly " +
      "this. Filed CONTROL as the closest honest bucket: it is an affordance's effect spelled " +
      "out, not a status that owes one.",
  },
  {
    s: "Left with you. Ask me any time and we'll go through it.",
    c: "INFORMATIONAL",
    why:
      "The receipt for a press that wrote something down, and there is genuinely nothing to " +
      "do about it — it names the way back rather than asking for one. It is not an " +
      "instruction: he has already given the answer this acknowledges, and nothing is waiting " +
      "on him. It is gone at the next launch, when the state is `declined` and the notice " +
      "renders nothing.",
  },
  {
    s: "I couldn't read your notes about this company.",
    c: "NEEDS-SOMEONE-ELSE",
    explainedBy: "I'm working without them and I won't guess at what they said. Whoever set RichOS up will need to look at that.",
    fixture: "first-run-unusable",
    why:
      "A file on disk that the app cannot use. Nothing the CEO can press fixes it, and the " +
      "notice deliberately draws no control in this state. The party is named by the sentence " +
      "directly beneath it, in the same panel, which is what `explainedBy` points at.",
  },
  {
    s:
      "I'm working without them and I won't guess at what they said. Whoever set RichOS up " +
      "will need to look at that.",
    c: "NEEDS-SOMEONE-ELSE",
    fixture: "first-run-unusable",
    why:
      "The consequence and the party, in one sentence. \"I won't guess\" is not decoration — " +
      "it is `UNUSABLE_BLOCK`'s own instruction to Rich, said on screen so the window and the " +
      "conversation make one claim rather than two.",
  },
  {
    s:
      "I couldn't write that down, so it isn't recorded and I'll ask again next time. I'd " +
      "rather say so than let you think it was settled. Whoever set RichOS up will need to " +
      "look at that.",
    c: "NEEDS-SOMEONE-ELSE",
    fixture: "first-run-decline-refused",
    why:
      "\"Not now\" could not be written. He can press it again and it may work, but what would " +
      "MAKE it work is somebody else's — so it is classified by the thing he cannot do, and " +
      "the offer is deliberately left open behind it. Closing the notice here would report " +
      "success over work that did not happen.",
  },
  { s: "Sending your message", c: "INFORMATIONAL", why: "The invoke is pending. This does not claim the backend has accepted the words." },
  { s: "Waiting for Rich to accept it", c: "INFORMATIONAL", why: "The local request has no authoritative queued event yet." },
  { s: "Your message is recorded.", c: "INFORMATIONAL", why: "The queued event follows the durable acceptance." },
  { s: "Rich is picking this back up.", c: "INFORMATIONAL", why: "A one-time announcement of an authoritative recovery transition." },
  { s: "Resume the questions", c: "CONTROL", why: "The existing Start control continues an incomplete interview." },
  { s: "Let's pick up the questions about my business where we stopped.", c: "CONTROL", why: "The explicit Resume acceptance sent through the ordinary message path." },
  { s: "Your business notes are started.", c: "ACTIONABLE", control: "#first-run-start", fixture: "first-run-partial", why: "Saved partial answers have a Resume control in the same panel." },
  { s: "Your saved answers are kept. We can pick up the remaining questions where we stopped. Press Resume the questions when you're ready.", c: "ACTIONABLE", control: "#first-run-start", fixture: "first-run-partial", why: "Explains that resuming keeps existing answers and names the control beside it." },

  { s: "Missing onboarding scope", c: "NOT-RENDERED", why: "The standalone onboarding MCP process reports this on stderr when launched without its required scope argument. It is not displayed in the app." },
  { s: "Missing status scope", c: "NOT-RENDERED", why: "The same shape one file down: the standalone status MCP process (`--status-mcp`, the front desk's read) reports this on stderr when launched without its required scope argument. A child of the running app, so `activation.rs`'s parent-pid condition is false by construction and no alert is armed. It is not displayed in the app." },
  { s: "Missing report scope", c: "NOT-RENDERED", why: "The same shape as `Missing status scope`: the standalone operator report MCP process (`--operator-mcp`, operator back-end spec r2 (c)) reports this on stderr when launched without its scope argument. Only an operator lead on an install whose operator.json passed the gate is ever given this server; it is a child process, so no alert is armed, and it is not displayed in the app." },
  { s: "operator report server: {error}", c: "NOT-RENDERED", why: "The operator's `reason` in `~/Library/Logs/RichOS/startup.log` for a report-server failure, the same machinery half as `status tool server:` beside it. It never reaches the window." },
  { s: "Missing operator scope", c: "NOT-RENDERED", why: "The same shape as `Missing report scope`: the standalone front-desk operator tool process (`--operator-desk-mcp`, main.rs, the operator-client record's §7 item 2) reports this on stderr when launched without its scope argument. Only a front desk on an install whose operator.json passed the gate is given this server; it is a child process and nothing of it is displayed in the app." },
  { s: "operator desk tool server: {error}", c: "NOT-RENDERED", why: "The operator's `reason` in `~/Library/Logs/RichOS/startup.log` for a front-desk operator tool server failure, the same machinery half as `operator report server:` above. It never reaches the window." },
  { s: "not a conversation factory", c: "NOT-RENDERED", why: "A literal in `src-tauri/src/turn_boundary_tests.rs`, which main.rs declares as `#[cfg(test)] mod turn_boundary_tests;`: a fake's refusal in a unit test, compiled into no build. The scrape excludes an INLINE `#[cfg(test)] mod … { }` but reads `src-tauri/src/` by directory, so it cannot see that a whole FILE is test-only; this row names that, and fails the suite the day the literal goes." },
  { s: "land the three branches", c: "NOT-RENDERED", why: "`HIS_TASK` in `src-tauri/src/turn_boundary_tests.rs` (`#[cfg(test)] mod turn_boundary_tests;` in main.rs): the words a unit test gives as his task, compiled into no build. Same scrape gap as `not a conversation factory`." },
  { s: "About your team", c: "INFORMATIONAL", why: "The from-line of a line on his team's lane that RichOS says ABOUT his team (`team` kind: it ended, a stop's result, his Esc; `timeline.js` `teamAttribution`). It attributes the words below it and asks nothing; any request is in those words, which are classified where they are authored. tests/operator-notice.js renders it." },
  { s: "could not finish", c: "FRAGMENT", why: "The kind half of a `failed` from-line, drawn after \"Your team\" (`timeline.js` `teamAttribution`). An attribution fragment, not an independent sentence; the notice's own words below it say what happened. tests/operator-notice.js renders it." },
  { s: "status tool server: {error}", c: "NOT-RENDERED", why: "The operator's `reason` in `~/Library/Logs/RichOS/startup.log` for a status-server failure, alongside `assignment tool server:` and `onboarding tool server:` beside it. It is the machinery half of that call and never reaches the window. The CEO-facing half beside it — the sentence about the helper that looks at work already running — is not in this inventory either, and neither are its two siblings: the Rust scrape does not reach a `startup_alert::cannot_start` argument, which is a pre-existing blind spot of the scrape rather than something this row covers." },
  { s: "The company changed. Please use the offer for the company now open.", c: "INFORMATIONAL", why: "An obsolete company-specific action was refused. The onboarding context guard refreshes the current offer and retains its controls." },
  {
    s: "Open a conversation first.",
    c: "ACTIONABLE",
    control: "#rail-new-thread",
    fixture: null,
    why:
      "`send_prompt`'s refusal at main.rs:518, three lines above its neighbour and reached " +
      "the other way: `spine.active_thread()` is None, so there is no conversation to file " +
      "the message under. ACTIONABLE and the control is `#rail-new-thread`, the rail's own " +
      "new-conversation button — one press makes the thread the message needed. NO FIXTURE, " +
      "and the reason is a real one rather than an omission: every path a browser can drive " +
      "reaches the composer through a thread that already exists, so the mock bridge has no " +
      "way to present a live composer with no active thread. The control claim is held " +
      "statically here and `#rail-new-thread` is asserted present on every shell walk.",
  },
  { s: "Waiting for its saved messages", c: "INFORMATIONAL", why: "The selected conversation has not finished loading." },
  { s: "No messages in this conversation yet.", c: "INFORMATIONAL", why: "An empty cached destination while its ordered activation is pending." },
  { s: "invalid P-256 private key encoding", c: "NOT-RENDERED", why: "Internal PhoneError::Crypto detail; phone commands map it through ceo_sentence before returning to the webview." },
  { s: "This conversation is still working. Press Stop to stop that work.", c: "ACTIONABLE", control: "#stop", fixture: "returning-conversation", why: "Returning to the working thread preserves its Stop while activation is queued." },
  { s: "I couldn't stop this conversation. Press Stop again.", c: "ACTIONABLE", control: "#stop", fixture: "returning-stop-failed", why: "A failed Stop remains retryable when returning to the working thread." },
  { s: "Stopping work in this conversation", c: "INFORMATIONAL", why: "A durable Stop receipt targets the selected working thread." },
  { s: "The previous conversation is still working. Press Stop to stop that work.", c: "ACTIONABLE", control: "#stop", fixture: "opening-conversation", why: "The previous live model keeps its actual Stop control while navigation waits for its lock." },
  { s: "I couldn't stop the previous conversation. Press Stop again.", c: "ACTIONABLE", control: "#stop", fixture: "opening-stop-failed", why: "A failed stop remains visible in the opening band beside the retryable Stop control." },

  { s: "Stopping work in the previous conversation", c: "INFORMATIONAL", why: "A durable Stop receipt disables repeat Stop while awaiting its terminal event." },

  // Shipped repository, account and saved-work surfaces.
  {
    "s": "<div class=\"overlay-panel overlay-panel--compact\"> <h2 id=\"repositories-title\" class=\"overlay-title\">Connected project folders (repositories)</h2> <p class=\"overlay-note\">Connect a project folder as this company’s home for Rich. Connecting it does not move or change its existing files.</p> <p class=\"overlay-note repository-guidance\">One folder per company is usually enough. But in rare cases additional folders might be needed. Ask Rich if you’re unsure.</p> <label class=\"entity-add-label\" for=\"repository-company\">Company</label> <select id=\"repository-company\" class=\"entity-add-input\"></select> <ul id=\"repository-list\"></ul> <label class=\"entity-add-label\" for=\"repository-folder\">Project folder location</label> <input id=\"repository-folder\" class=\"entity-add-input\" type=\"text\" placeholder=\"Click and select folder\" autocomplete=\"off\" spellcheck=\"false\"> <p id=\"repository-message\" class=\"overlay-note\" role=\"status\"></p> <div class=\"desk-card-actions\"><button id=\"repository-connect\" class=\"desk-btn desk-btn--confirm\" type=\"button\">Connect folder</button> <button id=\"repository-close\" class=\"desk-btn\" type=\"button\">Close</button></div></div>",
    "c": "FRAGMENT",
    "why": "Composite HTML for the repository connection dialog. Its interactive controls and visible wording are exercised by the dedicated browser suite; this literal is parsed as markup rather than rendered as one sentence."
  },
  // ---------------------------------------------------------------------------------------
  // THE QUIT QUESTION — the background-work spec §2.5/§2.5a, and the CEO's row 5.
  //
  // Closing the window no longer quits, so this sheet is what he meets when he chooses Quit
  // with work running. By the time any of it renders the exit has ALREADY been prevented,
  // which is why "Keep working" changes nothing at all and is the focused answer.
  // ---------------------------------------------------------------------------------------
  {
    "s": "<div class=\"overlay-panel overlay-panel--compact\"> <h2 id=\"quit-question-title\" class=\"overlay-title\">Quit while work is running?</h2> <p id=\"quit-question-say\" class=\"overlay-note\"></p> <div class=\"desk-card-actions\"> <button id=\"quit-question-stay\" class=\"desk-btn\" type=\"button\">Keep working</button> <button id=\"quit-question-quit\" class=\"desk-btn desk-btn--confirm\" type=\"button\">Quit and stop the work</button> </div> </div>",
    "c": "FRAGMENT",
    "why": "Composite HTML for the quit question. Its interactive controls and visible wording are exercised by the dedicated browser suite (ui/tests/quit-question.js), in both themes; this literal is parsed as markup rather than rendered as one sentence."
  },
  {
    "s": "Quit and stop the work",
    "c": "CONTROL",
    "why": "The destructive answer on the quit question, named for what it does rather than OK - it is the one control in this app that ends running background work on purpose. Its words are the shell's (lifecycle.rs's QUIT_AND_STOP), so what the button says and what the process does cannot drift apart. It is never the focused control."
  },
  {
    "s": "Something is still running in the background.",
    "c": "ACTIONABLE",
    "why": "The quit question's fallback sentence, used only if the shell's own wording did not arrive with the event. He is being asked to choose, and both answers are on the same sheet: Keep working (the focused, safe one) and Quit and stop the work.",
    "control": "#quit-question-stay"
  },
  {
    "s": "That could not be done:",
    "c": "FRAGMENT",
    "why": "The prefix of a refusal on the quit question, with the backend's own reason appended. It never renders alone, and it is said rather than swallowed: he pressed something, and a sheet that silently did nothing would be worse than a refusal he can read."
  },
  {
    "s": "Account connection could not be verified. Try again.",
    "c": "ACTIONABLE",
    "why": "The failed account check offers the same sign-in button for another attempt.",
    "control": "#provider-connect",
    "fixture": "provider-poll-error"
  },
  {
    "s": "Add a company before connecting a folder.",
    "c": "ACTIONABLE",
    "why": "Close the connected folders sheet to reach company creation in the main company picker; a folder cannot be connected until a company exists.",
    "control": "#repository-close"
  },
  {
    "s": "Checking this folder…",
    "c": "INFORMATIONAL",
    "why": "The native folder check, and Git set-up where the folder has none, is in progress."
  },
  {
    "s": "Checking your account connection.",
    "c": "INFORMATIONAL",
    "why": "Account status is being loaded."
  },
  {
    "s": "Choose a company",
    "c": "CONTROL",
    "why": "The unselected option asks for an explicit company choice."
  },
  {
    "s": "Click and select folder",
    "c": "CONTROL",
    "why": "The placeholder of every folder field (CEO feedback 2026-10-06, items 2c/2d and 2026-10-06_03). It names the field's own action: a click opens the system folder chooser through main.js's shared attachFolderPicker; a typed path still works."
  },
  {
    "s": "Choose a project folder",
    "c": "CONTROL",
    "why": "The title of the system folder chooser opened from the connected folders sheet's folder field (pick_folder). It names what the chooser is for; the chooser's own Open and Cancel are the controls."
  },
  {
    "s": "Choose the company's folder",
    "c": "CONTROL",
    "why": "The title of the system folder chooser opened from the first-run company sheet's folder field (pick_folder). It names what the chooser is for; the chooser's own Open and Cancel are the controls."
  },
  {
    "s": "Connected folders (repositories)",
    "c": "CONTROL",
    "why": "The settings menu row that opens the connected folders sheet (CEO feedback 2026-10-06, item 2a: it replaced Connected repositories). A row he presses, not a state."
  },
  {
    "s": "Folder connected. Git was set up to track its files.",
    "c": "INFORMATIONAL",
    "why": "Confirms a connection that also set up Git, because every connected folder gets Git tracking (CEO, 2026-10-06): the folder had none, so its files became the first commit. Nothing was pushed and nothing is left to do."
  },
  {
    "s": "Already connected to",
    "c": "FRAGMENT",
    "why": "Head of the shared-folder notice on the connected folders sheet (plan item 11, round 36 screen 2), completed by the company names that already have the folder. The button beside it, Connect to <company> too, is the action."
  },
  {
    "s": "Connect it here too and Rich can work in it from",
    "c": "FRAGMENT",
    "why": "First part of the shared-folder notice's second sentence (round 36 screen 2), completed by the company names around 'chats as well as'."
  },
  {
    "s": "chats as well as",
    "c": "FRAGMENT",
    "why": "Middle of the shared-folder notice's second sentence (round 36 screen 2), between the two company names."
  },
  {
    "s": "Nothing in the folder moves and each company keeps its own chats and notes.",
    "c": "INFORMATIONAL",
    "why": "Third sentence of the shared-folder notice (round 36 screen 2): says what does not change when a folder is in two companies. Nothing is left to do."
  },
  {
    "s": "Folder connected. It is now in",
    "c": "FRAGMENT",
    "why": "Confirmation after a folder is connected to a second company (plan item 11), completed by the company names. Nothing is left to do."
  },
  {
    "s": "No project folders connected yet.",
    "c": "ACTIONABLE",
    "why": "The empty state of the connected folders sheet (CEO copy, 2026-10-06, item 2d). The company selector begins the connection flow; repositories.js checks selection, folder entry and connection.",
    "control": "#repository-company",
    "fixture": "repository-empty"
  },

  {
    "s": "Saved work is unavailable:",
    "c": "FRAGMENT",
    "why": "Prefix joined to the native error describing why saved receipts cannot be read."
  },
  {
    "s": "Sign-in could not be canceled. Try again.",
    "c": "ACTIONABLE",
    "why": "Cancellation remains available while the provider is still connecting.",
    "control": "#provider-cancel",
    "fixture": "provider-cancel-error"
  },
  {
    "s": "Sign-in could not start. Try again.",
    "c": "ACTIONABLE",
    "why": "A failed sign-in start re-enables the initiating button.",
    "control": "#provider-connect",
    "fixture": "provider-start-error"
  },
  {
    "s": "The software is installed.",
    "c": "INFORMATIONAL",
    "why": "Confirms software installation separately from the account sign-in that follows.",
    "fixture": "setup-finished"
  },
  {
    "s": "This conversation has no company binding.",
    "c": "INFORMATIONAL",
    "why": "Explains why a saved-work query cannot select a company."
  },  {
    "s": "Work status is changing. Please try again.",
    "c": "ACTIONABLE",
    "why": "Reopening the saved-work disclosure retries the same native status request after the concurrent update.",
    "control": ".drill-chip"
  },
  {
    "s": "Worker state is not settled.",
    "c": "INFORMATIONAL",
    "why": "The native stop/exit fence reports an unsettled worker observation."
  },
  {
    "s": "You need your own Anthropic account and a Max subscription there. You can sign in through your browser after setup; I never see your password. If you don't already have that subscription, sign up there first and pick the Max/Max 20x tier.",
    "c": "INFORMATIONAL",
    "why": "Explains the separate account requirement before installation; the subsequent account sheet supplies the sign-in control.",
    "fixture": "setup-missing-both"
  },
  {
    "s": "sign up there first and pick the Max/Max 20x tier",
    "c": "CONTROL",
    "why": "The underlined pricing link inside the setup sheet's account sentence (`#setup-account a`, main.js renderSetupAccountNote). It is itself the control: a click opens claude.com/pricing through the `open_external` allowlist key."
  },
  {
    "s": "Your Anthropic account",
    "c": "CONTROL",
    "why": "The heading naming the account connection dialog."
  },
  {
    "s": "button, input, select",
    "c": "NOT-RENDERED",
    "why": "A CSS selector used to trap keyboard focus, never displayed as prose."
  },
  {
    "s": "older team shown here. Ask Rich about any of them.",
    "c": "ACTIONABLE",
    "why": "The limit on older team jobs shown directs the user to the normal message composer for any of them (hunt 57, finding 11: plain words, no 'saved records').",
    "control": "#input"
  },
  // -------------------------------------------------------------------------------------
  // GETTING THE SPEECH MODEL (2026-09-17)
  //
  // `.github/README.md` said "Voice does not work yet … Speech needs a model this build does
  // not download for you." It downloads it now, and these are every sentence that says so.
  //
  // THE SHAPE OF THE WHOLE BLOCK IS ONE RULE: a state RichOS can close by downloading
  // something is ACTIONABLE and names the control that starts or restarts the download; a gap
  // RichOS cannot close is NEEDS-SOMEONE-ELSE and names the party. Nothing here is
  // INFORMATIONAL unless there is genuinely nothing to press — the two progress lines and the
  // browser preview's refusal are the only three that qualify.
  // -------------------------------------------------------------------------------------
  {
    "s": "I can hear you once I download my speech model. It's",
    "c": "ACTIONABLE",
    "control": "#voice-model-get",
    "fixture": "voice-model-offer",
    "why":
      "THE OFFER, and the head of the sentence the size is appended to. He presses  because he " +
      "wants to talk; this is the honest answer and the button under it is the whole of what he " +
      "has to do. The size is never typed in the UI — it arrives from model-costs.json via Rust.",
  },
  {
    "s": "and it's a one-time download.",
    "c": "FRAGMENT",
    "why": "Tail of the offer sentence above, after the size label is spliced in.",
  },
  {
    "s": "Right now this disk has",
    "c": "FRAGMENT",
    "why":
      "Opens the short-disk clause of the same offer sentence. The button STAYS in that case: " +
      "free space is something he can change while the sentence is on screen, and the download " +
      "re-reads the disk when pressed, so removing the control would leave him a state he can " +
      "fix and nothing to press once he has.",
  },
  {
    "s": "free and it needs about",
    "c": "FRAGMENT",
    "why": "Middle of the same short-disk clause, between the free figure and the needed one.",
  },
  {
    "s": "an unknown amount",
    "c": "FRAGMENT",
    "why":
      "What a size reads as when the platform declined to report free space. Deliberately not a " +
      "refusal: unknown free space is not 'no room', so the offer still stands.",
  },
  {
    "s": "Part of it is already here, so this picks up where it left off.",
    "c": "FRAGMENT",
    "why":
      "Appended to the offer when a .part file survives an earlier attempt. It is the resume " +
      "promise the fetch layer actually keeps, not reassurance.",
  },
  {
    "s": "Getting my speech model ready…",
    "c": "INFORMATIONAL",
    "why":
      "The instant between the press and the first byte. Nothing to do, and the Stop control is " +
      "on the same row for the moment there is.",
  },
  {
    "s": "Downloading my speech model:",
    "c": "FRAGMENT",
    "why": "Head of the progress line; the percentage and the total are appended per event.",
  },
  {
    "s": "Checking my speech model is the one I expected…",
    "c": "INFORMATIONAL",
    "why":
      "The verification pass: the transfer is done and the sha256 is being taken, which costs " +
      "about a second per 500 MB. Its own sentence because a bar frozen at 100% under the " +
      "download's words is where somebody decides the app has hung. Nothing to do; Stop is " +
      "disabled here because there is no longer a transfer to stop.",
  },
  {
    "s": "My ears are installed. Start listening whenever you're ready.",
    "c": "ACTIONABLE",
    "control": "#voice-model-listen",
    "fixture": "voice-model-installed",
    "why":
      "The finish. The microphone is NOT opened for him: he pressed  minutes and half a " +
      "gigabyte ago, and acting on that consent now would be acting on consent that has gone " +
      "stale. One press, with the sentence in front of him.",
  },
  {
    "s": "I couldn't set up my hearing just now. Ask me again in a moment.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "why":
      "The fallback sentence for a failed event that carried no message of its own. No fixture: " +
      "every real failure path supplies its own sentence (Finding::ceo_message), so driving this " +
      "one would mean faking an event the backend does not emit — and the registry would then be " +
      "vouching for a fixture rather than for the product.",
  },
  {
    "s": "Downloading my speech model needs the desktop app. Here in the preview, type to me.",
    "c": "INFORMATIONAL",
    "why":
      "Browser-preview only: reachable solely when Bridge.isMock, exactly like the voice-mode " +
      "line above it. Never renders in the shipped app, and the composer it points at is on screen.",
  },
  {
    "s": "I've stopped the download. What arrived is saved, so asking me again picks up where it left off rather than starting over.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "fixture": "voice-model-failed",
    "why":
      "HE stopped it, so this is not a failure and must not wear one — but it renders in the " +
      "failed row, and it says what it cost him (nothing) and what happens next. The control is " +
      "there because the sentence invites him back.",
  },
  {
    "s": "There isn't enough room on this disk for my speech model: it needs about {} free and there's {}. Free up some space and ask me again. Nothing was downloaded.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "why":
      "Rust format string; the two holes are sizes. Refused BEFORE a byte is requested, because " +
      "finding out at 95% of 487.6 MB is the worst possible moment. 'Ask me again' is answered " +
      "by the retry control, which re-reads the disk.",
  },
  {
    "s": "The network sent me a sign-in page instead of my speech model. That's what hotel, airport and conference wifi does. Sign in to the network, then ask me again. Nothing was installed.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "why":
      "The captive portal, named as a captive portal. THIS ROW IS WHY `worth_asking_again` EXISTS " +
      "SEPARATELY FROM `retryable`: the automatic loop must not retry a portal (it would fetch " +
      "the same login page) and the CEO absolutely must be able to, once he has signed in. One " +
      "flag for both questions put this instruction on screen with no button under it.",
  },
  {
    "s": "The server sent back an error instead of my speech model. Ask me again in a moment. Nothing was installed.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "why": "An HTTP error or a plain-text body where a model was expected. Transient; asking again is the move.",
  },
  {
    "s": "What came back isn't my speech model, so I didn't install it. Ask me again when you're on a network you trust.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "why":
      "An archive, a non-GGML body, or a file longer than the pin. Shared by three failure kinds " +
      "because they are one event to him: those were not the bytes we meant.",
  },
  {
    "s": "What arrived isn't the speech model I was expecting, so I threw it away rather than listen to you through it. Ask me again when you're on a network you trust.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "why":
      "The sha256 mismatch — right size, right magic, wrong bytes, which is the case the pin " +
      "table exists for. Deleted rather than quarantined. Never retried automatically, and still " +
      "his to ask for again on a network he trusts.",
  },
  {
    "s": "The download stopped partway. I kept what arrived, so asking me again picks up where it left off rather than starting over.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "why": "A truncated transfer with a usable prefix. The resume is real: the next attempt sends a Range header.",
  },
  {
    "s": "The download stopped before anything useful arrived. Ask me again when the connection is steadier.",
    "c": "ACTIONABLE",
    "control": "#voice-model-retry",
    "why":
      "A transfer that died with nothing worth keeping, or with a prefix that was not a model " +
      "prefix. Nothing is kept, and it says so rather than promising a resume it cannot perform.",
  },
  {
    "s": "I don't have a speech model on this machine yet.",
    "c": "INFORMATIONAL",
    "why":
      "Failure::Absent's sentence. Reached only if the finished file vanishes between the last " +
      "write and the hash — a statement of fact with no instruction in it, and not a state he " +
      "caused or can act on differently from the retry the row beside it already offers.",
  },
  {
    "s": "I can't set up my hearing on this machine. Whoever set RichOS up adds that. I can still read what you type.",
    "c": "NEEDS-SOMEONE-ELSE",
    "why":
      "No decoder, or weights that are not the pinned weights. RichOS fetches pinned MODELS; " +
      "whisper-cli is not pinned and cannot be, because a Homebrew binary's sha256 is a property " +
      "of an arch and a bottle revision (model-pins.json says so). So there is nothing to offer " +
      "and the sentence names who can do it instead.",
  },
  {
    "s": "I can't prove the speech model this machine needs is the genuine one, so I won't download it . Whoever set RichOS up can put that right. I can still read what you type.",
    "c": "NEEDS-SOMEONE-ELSE",
    "why":
      "The resolver asked for a model with no row in model-pins.json. RichOS will not download " +
      "what it cannot verify, and adding a pin is a commit — so it is somebody else's move, not his.",
  },
  {
    "s": "I can't tell where to put my speech model on this machine. Whoever set RichOS up can put that right. I can still read what you type.",
    "c": "NEEDS-SOMEONE-ELSE",
    "why":
      "HOME is unset, which no double-click produces. A machine somebody configured, so the " +
      "sentence names that somebody.",
  },
  {
    "s": "I don't have a way to check that this speech model is genuine, so I won't install it. Whoever set RichOS up can put that right. I can still read what you type.",
    "c": "NEEDS-SOMEONE-ELSE",
    "why":
      "Failure::Unpinned's own sentence. It used to share the 'ask me again on a network you " +
      "trust' line, and no network has anything to do with it — a pin table is a source file. " +
      "Split out precisely so it does not invite him back to a refusal that cannot change.",
  },
  {
    "s": "Download my speech model",
    "c": "CONTROL",
    "why": "The offer row's button. Survives being spoken aloud on its own, which every option in this app must.",
  },
  {
    "s": "Stop the download",
    "c": "CONTROL",
    "why": "The in-flight row's button. Stops the transfer and keeps the prefix.",
  },
  {
    "s": "Try the download again",
    "c": "CONTROL",
    "why": "The failed row's button, shown only when the backend says asking again could help.",
  },
  // -------------------------------------------------------------------------------------
  // BACKGROUND WORK — the assignment surface (the background-work spec, revision 5, in the
  // private richos-hq record; §1, §4.2 and §0 row 7), as the CEO's ruling §52 leaves it.
  //
  // WHAT §52 CHANGED HERE (2026-09-18): *"There's nothing that ever not lands on its own
  // here in the terminal … So, yes, always land on its own."* Both rows below used to open
  // with "Ready for you to approve", because the one thing that could stop a background job
  // was the step that would change his repository. A job lands on its own now, so `blocked`
  // means a decision of his is outstanding on SOME step — and the row no longer claims his
  // repository is untouched, because a job may reach such a step after landing something.
  //
  // Saved work is read from Stu's existing reports.
  {
    "s": "What you have asked for",
    "c": "INFORMATIONAL",
    "why":
      "The assignments block's heading in the saved-work pane. It labels the list below it " +
      "and asks nothing of him.",
  },
  {
    "s": "Written down. Nothing has been prepared yet.",
    "c": "INFORMATIONAL",
    "why":
      "The registered state. The assignment is on disk and the work lease picks it up at the " +
      "turn boundary without him doing anything, so there is nothing here for him to act on.",
  },
  {
    "s": "Getting ready to start.",
    "c": "INFORMATIONAL",
    "why": "The preparing state: the work lease is inside the spawn preparer. Nothing is asked of him.",
  },

  {
    "s": "Waiting for your decision. Ask Rich to continue it when you are ready.",
    "c": "ACTIONABLE",
    "why":
      "The same stop, with no question left on the desk — the queue lives in the running " +
      "process, so an assignment that stopped at his decision before a relaunch has nothing " +
      "to press. Recovery is §6 and is not built, so this sentence names the path that does " +
      "exist: asking Rich to continue it, through the composer.",
    "control": "#input",
  },
  {
    "s": "Waiting for the screen to unlock. I'll carry on the moment it's back.",
    "c": "INFORMATIONAL",
    "why":
      "The CEO's ruling §56 (2026-09-18): a background job that needs the Mac's screen, found " +
      "on a locked screen, waits for the unlock and carries on by itself. It is the one " +
      "waiting sentence in the assignment rows that is NOT waiting on him — it is waiting on " +
      "the Mac — so it correctly names no control and there is no control that could exist " +
      "for it. Classifying it ACTIONABLE would be a claim that something on his screen " +
      "resolves it; pointing him at his own lock screen would be the nag §56 was given to " +
      "avoid. He unlocks his Mac when he unlocks his Mac, and the app says nothing about it " +
      "and asks nothing of him. The state resolves itself with no press anywhere, which is " +
      "exactly what INFORMATIONAL is for.",
  },

  {
    "s": "waiting for you",
    "c": "FRAGMENT",
    "why": "The work chip counts jobs Stu reports blocked. Opening it shows the existing saved-work pane; tool approval controls do not exist.",
  },
  {
    "s": "waiting for the screen",
    "c": "FRAGMENT",
    "why":
      "The tail of the work chip's own count for CEO §56's state — '1 assignment waiting for " +
      "the screen' — classified beside `waiting for you` directly above it and for the same " +
      "reason: it never renders alone, and the count IS the affordance, because the chip is " +
      "the only way to open the pane the assignments live in. It is deliberately NOT the same " +
      "words as `waiting for you`: `AssignmentState::waits_for_the_world` is not " +
      "`awaits_his_word`, the wait clears itself when the Mac is unlocked, and telling him a " +
      "self-resolving wait is his to act on is the nag §56 was given to avoid. Before it had a " +
      "count of its own, an assignment in that state as his only open work left the chip empty " +
      "and hidden — no chip, no pane, no way to see the work (esc-20260918T114550Z-64ae379a).",
  },
  {
    "s": "waiting to start",
    "c": "FRAGMENT",
    "why":
      "The tail of the work chip's own count for work held until a sign-in or the work " +
      "connection comes back — '2 assignments waiting to start' (nightly 48's walk, D26) — " +
      "classified beside `waiting for the screen` above and for the same reason: it never " +
      "renders alone, and the count IS the affordance, the chip opening the pane the " +
      "assignments live in. The hold's own notice says what ends it.",
  },

  {
    "s": "Stopped before it finished. Ask Rich what it needs.",
    "c": "ACTIONABLE",
    "why":
      "A job that did not finish. Its own recorded reason is appended beside this sentence — " +
      "and since the CEO's ruling §52 that includes a FAILED LAND: the reviewer asked for " +
      "changes, the land did not go through, or the work landed and the assignment was never " +
      "closed. The reason is read off the land record the engine wrote, never softened, and " +
      "continuing or abandoning it is a decision he makes through the composer.",
    "control": "#input",
  },
  {
    "s": "Its state could not be read.",
    "c": "INFORMATIONAL",
    "why":
      "A record written by a newer RichOS, or a state this build does not know. It is the " +
      "honest 'I cannot tell' — it never reads as finished, and there is nothing he can do " +
      "about a record this build cannot parse.",
  },

  // -------------------------------------------------------------------------------------
  // A QUESTION HE ASKED, rather than work he asked for — the CEO's ruling §58, 2026-09-18
  // -------------------------------------------------------------------------------------
  //
  // *"The answer arrives on the timeline as an answer, never as 'done'."* The eight rows
  // below are the same three surfaces as the work rows above, said about a question: the
  // saved-work pane's six states (`work-summary.js`'s `QUESTION_STATES`) and the timeline
  // timer's two accessible descriptions (`timeline.js`'s `questionRow`).
  //
  // WHY A SECOND SET AT ALL, rather than reusing the work sentences. Two of them would have
  // been outright wrong about a question: "Getting a workspace ready." describes something
  // done for work and nothing he needs to hear about a question of his, and "Finished." is
  // the one framing §58 names and refuses. The other four would merely have read like
  // machinery. The kind is on the record (`assignment.rs`'s `AssignmentKind`), so the pane
  // does not have to guess which set to use.
  //
  // CONTRAST is unchanged by any of them: every sentence here renders in the same
  // `.overlay-note` on `--card` as the work rows above (5.78:1 dark, 6.38:1 light at 16px,
  // measured by `background-work.js`), and the two timer descriptions are the row's
  // `aria-label` and `title` — read on focus and on hover, never painted as text.
  {
    "s": "Written down. Rich hasn't started looking yet.",
    "c": "INFORMATIONAL",
    "why":
      "A question of his, written down, before the back end has been asked. It covers BOTH " +
      "`registered` and `preparing`, which are two sentences for work and one here: the " +
      "workspace the preparing state describes is a thing done for work, and telling him " +
      "about it would be describing machinery in answer to a question he asked.",
  },
  {
    "s": "Rich is looking into this.",
    "c": "INFORMATIONAL",
    "why":
      "The running state for a question. The row carries its own Stop control, but the state " +
      "asks nothing of him: the answer arrives on the conversation when it arrives.",
  },

  {
    "s": "Waiting for your decision. Ask Rich to continue when you are ready.",
    "c": "ACTIONABLE",
    "why":
      "The same stop with no question left on the desk — the queue lives in the running " +
      "process, so a question that stopped at his decision before a relaunch has nothing to " +
      "press. The path that does exist is asking Rich, through the composer.",
    "control": "#input",
  },
  {
    "s": "Waiting for the screen to unlock. Rich will carry on the moment it's back.",
    "c": "INFORMATIONAL",
    "why":
      "§56's state in a question's words, and it arrived because the §58 slice rebased onto " +
      "§56's: without a row here a question waiting on the screen falls through to 'Its state " +
      "could not be read.' It names no control for the same reason the work version does not — " +
      "it is waiting on the Mac, it resolves itself, and the app never tells him to go and " +
      "unlock anything.",
  },
  {
    "s": "Answered: it's in your conversation.",
    "c": "INFORMATIONAL",
    "why":
      "**The row §58 exists for on this surface.** A question that has been answered is not " +
      "'Finished.' — the answer itself is on the timeline, said as an answer, which is where " +
      "he reads it. This sentence says where it went and asks nothing. It deliberately does " +
      "NOT repeat the answer: the pane would then be a second copy of it, able to disagree.",
  },
  {
    "s": "Rich couldn't get you an answer. Ask him again and he'll try it a different way.",
    "c": "ACTIONABLE",
    "why":
      "A question that got no answer — the back end never answered, or the connection failed. " +
      "It is said as that rather than as a job that 'stopped before it finished', which would " +
      "invite the wrong question about something that was never half-answered. Asking again " +
      "is the thing to do and the composer is where he does it.",
    "control": "#input",
  },
  {
    "s": "Rich is checking this for you. You'll get the answer here.",
    "c": "INFORMATIONAL",
    "why":
      "The accessible description of the 'Checking for {duration}' timer beside his reply " +
      "(§6.4: the timer's live updates are not announced every second, and this name is read " +
      "on focus). It claims only what is known — the question is with the other Rich and " +
      "nothing is finished — and names where the answer will appear. Nothing to act on: he " +
      "has already been told it is being checked.",
  },
  {
    "s": "Rich is looking into this for you. You'll get the answer here.",
    "c": "INFORMATIONAL",
    "why":
      "The same, for 'Investigating for {duration}' — either because he was told 'I'll " +
      "investigate.' or because a check has been running for a minute and flipped its own " +
      "label (§58: *\"a check still running at one minute flips its label to 'investigating' " +
      "on its own\"*).",
  },
  {
    "s": "Your assignments are unavailable:",
    "c": "FRAGMENT",
    "why":
      "Prefix joined to the backend's own reason, exactly as 'Saved work is unavailable:' " +
      "above it. The sentence he reads is the join.",
  },
  {
    "s": "Your assignments are changing. They will refresh in a moment.",
    "c": "INFORMATIONAL",
    "why":
      "The spine's lock is held for the length of a turn, so this read is deferred. The " +
      "surface re-reads every three seconds, which is why this does NOT tell him to try " +
      "again: an instruction over a state that resolves itself is the thing this rule exists " +
      "to catch.",
  },
  {
    "s": "Missing assignment scope",
    "c": "NOT-RENDERED",
    "why":
      "The diagnostic first argument to `startup_alert::cannot_start` in the " +
      "`--assignments-mcp` child, which reaches stderr and the startup log. The sentence a " +
      "person reads for the same failure is the second argument beside it.",
  },
  {
    "s": "assignment tool server: {error}",
    "c": "NOT-RENDERED",
    "why":
      "The same diagnostic argument, with the child's own error interpolated. A child of the " +
      "running app can never arm the alert — `activation.rs`'s parent-pid condition is false " +
      "by construction — so this is operator log text and nothing else.",
  },
  {
    "s": "stopped on request",
    "c": "NOT-RENDERED",
    "why":
      "An engineer's detail passed to PartFile::interrupted. It reaches Finding::describe on " +
      "stderr and never Finding::ceo_message, so it cannot appear on his screen. What he reads " +
      "for the same event is the STOPPED_BY_REQUEST sentence above.",
  },
  {
    s: "That folder is there and I couldn't use it. Everything else works as it does now: our conversations are kept somewhere else and are untouched and there's nothing for you to fix.",
    c: "INFORMATIONAL",
    why:
      "`MemoryStatus.state === \"unusable\"`, reached from the settings menu's `Memory folder` " +
      "row (audit-7 row 13). NOT actionable and NOT needs-someone-else, and both halves of that " +
      "are deliberate. He cannot clear it — the corpus resolved and something refused, and " +
      "`MemoryStatus.detail` carries the machine-facing half which never reaches this screen by " +
      "that field's own contract. And it names no party, for the same reason MEMORY_NO_READER " +
      "stopped naming one on 2026-09-04: on a customer's Mac the person who set RichOS up IS " +
      "him, so pointing him at a third party who does not exist is worse than saying nothing. " +
      "It says what is off, what is still on, and that nothing is required of him — the three " +
      "things that shape is for. Provisioning is not offered, because a corpus already exists.",
  },
  {
    s: "I couldn't read where your memory is kept just now. Nothing has changed.",
    c: "INFORMATIONAL",
    why:
      "`memory_status` itself would not answer when he pressed the `Memory folder` row. Nothing " +
      "was attempted and nothing was changed, and the row is still there to press again — so " +
      "this is a statement rather than a request, and it deliberately offers no control: a " +
      "`Try again` button here would be a second name for the row he just used.",
  },

  // -------------------------------------------------------------------------------------
  // THE PHONE CHANNEL (2026-09-18) — `ui/phone.js`, `ui/qr.js` and `src-tauri/src/phone/`.
  //
  // THIRTY OF THESE ROWS EXIST BECAUSE THIS GATE CHANGED THE PRODUCT. The pairing command
  // handed `PhoneError::to_string()` straight to the screen, so thirty internal sentences —
  // `add-generic-password for tls-leaf-key exited 51` — arrived here as user-visible states.
  // The honest answer was not to classify them as product copy. It was to stop showing them:
  // `PhoneError::ceo_sentence` now gives him one sentence he can read, the detail goes to the
  // log, and the rows below say NOT-RENDERED and mean it.
  // -------------------------------------------------------------------------------------
  {
    "s": "This code lasts",
    "c": "FRAGMENT",
    "why": "The first half of the countdown sentence — the seconds and the full stop are appended at run time. It never renders on its own."
  },
  {
    "s": "It can reach you with a notification when Rich has something for you.",
    "c": "INFORMATIONAL",
    "why": "The paired phone can be pushed to. Nothing to do — this is the state he wanted, said out loud so its absence is legible."
  },
  {
    "s": "It cannot send you notifications yet. On your phone, open the Share menu in Safari and choose “Add to Home Screen”, then allow notifications when Rich asks.",
    "c": "ACTIONABLE",
    "control": "#phone-forget",
    "why": "One of the three sentences the paired card shows while the phone cannot be pushed to yet, chosen by the `platform` on the device record. It was ONE sentence — \"Add Rich to your phone's Home Screen and allow notifications when it asks\" — and Chrome on the CEO's Android phone offers \"Install and create shortcut\" with no item by the other name at all, so the Mac named a control the device does not have (Ray's candidate .11 walk, defect 4.5, verified on the device). The menu and item names are exactly the phone page's own (`web/web-app/app.js`, `installControlName`), so the two surfaces cannot give one control two names. ACTIONABLE, and the control is the same one the old row named: this is the state he fixes ON THE PHONE, and the only thing this view can offer is the way back out. `ui/tests/phone.js` check 9d walks all three. iOS Safari, which genuinely cannot take a push until the app is on the Home Screen."
  },
  {
    "s": "It cannot send you notifications yet. Allow notifications on your phone when Rich asks. Chrome on Android does not need Rich installed first.",
    "c": "ACTIONABLE",
    "control": "#phone-forget",
    "why": "One of the three sentences the paired card shows while the phone cannot be pushed to yet, chosen by the `platform` on the device record. It was ONE sentence — \"Add Rich to your phone's Home Screen and allow notifications when it asks\" — and Chrome on the CEO's Android phone offers \"Install and create shortcut\" with no item by the other name at all, so the Mac named a control the device does not have (Ray's candidate .11 walk, defect 4.5, verified on the device). The menu and item names are exactly the phone page's own (`web/web-app/app.js`, `installControlName`), so the two surfaces cannot give one control two names. ACTIONABLE, and the control is the same one the old row named: this is the state he fixes ON THE PHONE, and the only thing this view can offer is the way back out. `ui/tests/phone.js` check 9d walks all three. Chrome on Android, where the old instruction was WRONG rather than misnamed: Chrome subscribes to push from a tab, so nothing has to be installed first. The phone page records the same division and, for the same reason, shows no install sentence there."
  },
  {
    "s": "It cannot send you notifications yet. Allow notifications on your phone when Rich asks. On an iPhone you have to add Rich to the Home Screen first, from Safari’s Share menu.",
    "c": "ACTIONABLE",
    "control": "#phone-forget",
    "why": "One of the three sentences the paired card shows while the phone cannot be pushed to yet, chosen by the `platform` on the device record. It was ONE sentence — \"Add Rich to your phone's Home Screen and allow notifications when it asks\" — and Chrome on the CEO's Android phone offers \"Install and create shortcut\" with no item by the other name at all, so the Mac named a control the device does not have (Ray's candidate .11 walk, defect 4.5, verified on the device). The menu and item names are exactly the phone page's own (`web/web-app/app.js`, `installControlName`), so the two surfaces cannot give one control two names. ACTIONABLE, and the control is the same one the old row named: this is the state he fixes ON THE PHONE, and the only thing this view can offer is the way back out. `ui/tests/phone.js` check 9d walks all three. A phone this Mac cannot name: it says what is true whatever the phone is and names no menu as the one to use — the phone page's own \"null means say nothing\" rule, because a wrong menu name sends him looking for something that is not there."
  },
  {
    "s": "Your phone said the six words did not match, so I stopped answering, forgot the phone and deleted the certificate this Mac was serving.",
    "c": "ACTIONABLE",
    "control": "#phone-rejected-again",
    "why": "THE MAC'S ANSWER TO THE ALARM BUTTON — Ray's nightly `.8` walk in the test VM, defect 1 (HIGH). He pressed `They do not match` on the phone; the credential really was dropped, and the sheet in front of him did not change at all — the pairing card stayed up with the six words under it, and the Mac went on answering `curl` on 8443 at t+10, 20, 30, 40, 50 and 60 s and two minutes later. *\"He has no way to know the Mac heard him.\"* One sentence, and it is a report rather than a request: what the phone said, and then the three things this Mac did about it. ACTIONABLE because he CAN change it and the control is on the screen with it: `PhoneStatus.rejected` is cleared by `phone_begin_pairing` and by nothing else, so a screen without `Set my phone up again` would be one he could not leave — Close would only put it back on the next open. The socket half is proved over real TLS by `phone::listen::tests::they_do_not_match_over_the_wire_stops_the_listener` (the port stopped answering 1 ms after the rejection went out); this half by `ui/tests/phone.js` check 9f, which also computes the sentence's contrast in both themes (5.78:1 dark, 6.38:1 light at 16px, against a 4.5:1 floor)."
  },
  {
    "s": "You said the six words did not match, so I stopped answering, forgot the phone and deleted the certificate this Mac was serving.",
    "c": "ACTIONABLE",
    "control": "#phone-rejected-again",
    "why": "The same screen as the phone's alarm button (the row above), reached from the Mac's own `They do not match` (Sage's pairing review F1): `PhoneRuntime::reject_on_mac` runs the one rejection teardown and records `StoppedBy::MAC`, so the sentence says it was him, here. One sentence, a report; `Set my phone up again` is the control that leaves it. ui/tests/phone.js check 9g presses the button and reads this sentence."
  },
  {
    "s": "A second phone used the same code, which means someone else may have seen it, so I stopped answering, forgot the phone that used it first and deleted the certificate this Mac was serving.",
    "c": "ACTIONABLE",
    "control": "#phone-rejected-again",
    "why": "Sage's spent-code alarm (F1 fix item 4): the pairing code was used correctly by a second device while the first was still unconfirmed, so two devices had it. The Mac forgets the first rather than guess which is his (`StoppedBy::CODE_USED_TWICE`), stops serving and says so in the same one-sentence shape as the other refusals. `Set my phone up again` issues a fresh code. ui/tests/phone.js check 9g."
  },
  {
    "s": "Nobody pressed They match on this Mac within five minutes, so I stopped answering, forgot the phone and deleted the certificate this Mac was serving.",
    "c": "ACTIONABLE",
    "control": "#phone-rejected-again",
    "why": "Sage's pairing review 3.1 step 6: the window's timer keeps running after a phone redeems the code, and an unconfirmed device at expiry is forgotten so an abandoned pairing never leaves a key behind (`StoppedBy::EXPIRED`). Said on the screen it happened on rather than falling back in silence (Ray's defect 3.3 was that silence); `Set my phone up again` issues a fresh code. ui/tests/phone.js check 9g."
  },
  {
    "s": "This Mac accepted your phone, but the phone never finished pairing, so I stopped answering, forgot the phone and deleted the certificate this Mac was serving.",
    "c": "ACTIONABLE",
    "control": "#phone-rejected-again",
    "why": "Sage's pair-v2 hypotheses review §2: this Mac pressed They match and the phone never used the pairing within `UNFINISHED_GRACE_MS` after the window, so the key its phone gave up on is forgotten (`StoppedBy::UNFINISHED`) rather than listed as a paired phone that can never connect, which also freed the one pairing slot. Same one-sentence shape as the other refusals; `Set my phone up again` issues a fresh code. ui/tests/phone.js check 9g reads it."
  },
  {
    "s": "A code for your phone's camera. The address is written out beside it.",
    "c": "INFORMATIONAL",
    "why": "The accessible name of a QR canvas, for a reader who cannot see it. It says the address is written out beside the code, which it is."
  },
  {
    "s": "Use Rich from your phone",
    "c": "CONTROL",
    "why": "#set-phone-open — the settings-menu row that opens the pairing screen."
  },
  {
    "s": "qr: bytes does not fit in a version 1-6 level-M symbol (106 bytes is the ceiling). Shorten the URL or extend the version table. Do not silently truncate.",
    "c": "NOT-RENDERED",
    "why": "A `throw` inside the QR encoder. Two of the three are internal invariants that can only fire on a table typo; the length ceiling is caught by `paint()` and replaced with the message it carries, which is for whoever is reading the log rather than for him — a URL too long to encode is a bug in the Mac's own address, not something he can act on."
  },
  {
    "s": "qr: interleaved codewords, the symbol holds",
    "c": "NOT-RENDERED",
    "why": "A `throw` inside the QR encoder. Two of the three are internal invariants that can only fire on a table typo; the length ceiling is caught by `paint()` and replaced with the message it carries, which is for whoever is reading the log rather than for him — a URL too long to encode is a bug in the Mac's own address, not something he can act on."
  },
  {
    "s": "qr: internal error: the payload overran the version chosen for it",
    "c": "NOT-RENDERED",
    "why": "A `throw` inside the QR encoder. Two of the three are internal invariants that can only fire on a table typo; the length ceiling is caught by `paint()` and replaced with the message it carries, which is for whoever is reading the log rather than for him — a URL too long to encode is a bug in the Mac's own address, not something he can act on."
  },
  {
    "s": ": re-snapshot {missed}",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/listen.rs:429)"
  },
  {
    "s": "AES-128-GCM rejected the content key",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:264)"
  },
  {
    "s": "AES-128-GCM sealing failed",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:271)"
  },
  {
    "s": "HKDF expand refused the requested length",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:213)"
  },
  {
    "s": "HKDF fill failed",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:215)"
  },  {
    "s": "VAPID signing failed",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:173)"
  },
  {
    "s": "a P-256 JWK's x and y are 32 bytes each, got {} and {}",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/device.rs:595)"
  },
  {
    "s": "a device key must be a 65-byte uncompressed P-256 point or its 91-byte SPKI wrapper, got {} bytes",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/device.rs:615)"
  },
  {
    "s": "a device key must be an EC P-256 JWK, got kty={kty:?} crv={crv:?}",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/device.rs:578)"
  },
  {
    "s": "a malformed endpoint",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:331)"
  },
  {
    "s": "a path is not valid UTF-8",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/ca.rs:273)"
  },
  {
    "s": "a phone is already paired; forget it first, which also closes the listener",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/device.rs:301)"
  },
  {
    "s": "a push endpoint has no host",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:190)"
  },
  {
    "s": "a push endpoint must be https",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:187)"
  },
  {
    "s": "a subscription auth secret must be 16 bytes, got {}",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:238)"
  },
  {
    "s": "a subscription p256dh must be a 65-byte uncompressed point, got {} bytes",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:232)"
  },
  {
    "s": "add-generic-password for {account} exited {}: {}",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/secrets.rs:153)"
  },
  {
    "s": "could not derive the ephemeral public key",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:295)"
  },
  {
    "s": "could not generate a VAPID key",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:118)"
  },
  {
    "s": "could not generate an ephemeral key",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:292)"
  },
  {
    "s": "delete-generic-password for {account} exited {}: {}",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/secrets.rs:173)"
  },
  {
    "s": "find-generic-password for {account} exited {code}: {}",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/secrets.rs:116)"
  },
  {
    "s": "no {label} block",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/ca.rs:454)"
  },
  {
    "s": "the JWK has no x",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/device.rs:585)"
  },
  {
    "s": "the JWK has no y",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/device.rs:591)"
  },
  {
    "s": "the content-encoding salt must be 16 bytes",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:243)"
  },
  {
    "s": "the derived nonce was not 12 bytes",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:269)"
  },
  {
    "s": "the stored authority key is not text",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/ca.rs:104)"
  },
  {
    "s": "the stored leaf key is not text",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/ca.rs:127)"
  },
  {
    "s": "the subscription's public key is not a valid P-256 point",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/push.rs:301)"
  },
  {
    "s": "the system random source refused",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/mod.rs:249)"
  },
  {
    "s": "this Mac has no address the phone could reach, so there is nothing to listen on",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/listen.rs:149)"
  },
  {
    "s": "this Mac has no conversation to add to yet",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/bridge.rs:96)"
  },
  {
    "s": "this Mac has no conversation yet",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/bridge.rs:146)"
  },
  {
    "s": "this Mac publishes no local host name, so there is no address the phone could use",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/names.rs:109)"
  },

  {
    "s": "unterminated {label} block",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/ca.rs:457)"
  },
  {
    "s": "{address} is every interface at once, which this channel never binds; give it the addresses this Mac actually answers on",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/listen.rs:155)"
  },
  {
    "s": "{address} is not an address a server binds",
    "c": "NOT-RENDERED",
    "why": "An internal `PhoneError` string. It is the Display form, which goes to the Mac's own log (`eprintln!`) and never to the webview: every refusal on the network is a flat 404 with an empty body, and the two commands that can surface a failure at all — `phone_begin_pairing` and `phone_forget` — map it through `PhoneError::ceo_sentence` first. This scrape sees it because it sits beside `Err(` in the command layer's crate, which is the one shape RUST_CEO_CONTEXT cannot tell apart from CEO copy. (richos/app/src-tauri/src/phone/listen.rs:160)"
  },
  // -------------------------------------------------------------------------------------
  // THE TAILSCALE PATH AND THE IDENTITY TRAP (CEO §61 / §61.1, 2026-09-19) — `ui/phone.js`,
  // `src-tauri/src/phone/tailnet.rs` and `src-tauri/src/opener.rs`.
  //
  // THE SHAPE OF THIS BLOCK IS THE SHAPE OF THE SCREENS. Most of it is FRAGMENTs, and that
  // is the point rather than an evasion: every sentence §61.1 required is composed at run
  // time around a string the Mac read off `tailscale status --json` — the provider and the
  // login name — because *"use the same account"* is advice nobody can follow when the one
  // thing they do not know is which account they used. A literal that ends mid-sentence is
  // what naming the account looks like in source.
  //
  // THE INSTRUCTIONS ARE ACTIONABLE AND THEY POINT AT `#phone-ts-open`, the control that
  // opens the exact thing the sentence names. Before 2026-09-19 the app could not open a
  // URL at all, so these sentences were instructions with nothing beside them but an
  // address to retype.
  // -------------------------------------------------------------------------------------
  {
    "s": ") is on this network but Tailscale is switched off on it. Turn it on in the Tailscale app.",
    "c": "FRAGMENT",
    "why": "Part of the sentence that says where the phone is (CEO §61.1 detection half). Composed at run time from the device name the daemon reports, so it never renders on its own. The whole sentence is one of three — on this network; on this network but switched off; not here yet, sign in with <account> — and `ui/tests/phone.js` checks 14 and 15 assert all three."
  },
  {
    "s": ") is on this network.",
    "c": "FRAGMENT",
    "why": "Part of the sentence that says where the phone is (CEO §61.1 detection half). Composed at run time from the device name the daemon reports, so it never renders on its own. The whole sentence is one of three — on this network; on this network but switched off; not here yet, sign in with <account> — and `ui/tests/phone.js` checks 14 and 15 assert all three."
  },
  {
    "s": ", then come back here. Reinstalling the app will not help: the identity is the only thing that decides it.",
    "c": "FRAGMENT",
    "why": "Part of the sentence that says where the phone is (CEO §61.1 detection half). Composed at run time from the device name the daemon reports, so it never renders on its own. The whole sentence is one of three — on this network; on this network but switched off; not here yet, sign in with <account> — and `ui/tests/phone.js` checks 14 and 15 assert all three."
  },
  {
    "s": "</strong>, exactly what this Mac used, even if you would normally keep your phone and your Mac apart. Nothing else connects them.",
    "c": "FRAGMENT",
    "why": "The tail of the phone step's why-line, joined to the account phrase the Mac read off `status --json`. It never renders on its own; `ui/tests/phone.js` check 13 asserts the whole sentence names the provider and login name."
  },
  {
    "s": "</strong>, the same one this Mac is signed in to. A different provider makes a different network and then the two will never see each other.",
    "c": "FRAGMENT",
    "why": "The tail of step 2 of the phone steps, joined to the account phrase. It never renders on its own — the sentence it belongs to is the one that tells the user WHICH identity to use, which is the whole of §61.1."
  },
  {
    "s": "Your phone reaches this Mac over your own Tailscale network, from anywhere it has a signal. The connection to this Mac is encrypted, your conversations stay here and it costs one free Tailscale account signed in on both devices.",
    "c": "INFORMATIONAL",
    "why": "One of the three leads the phone sheet opens with, chosen by the option he picked — and once a phone is paired, by `paired_via` on the device record rather than by the sheet's forgotten route. It was ONE fixed sentence describing the home-network option, and it stayed on screen word for word after he pressed \"Anywhere\", which is not the home network and does involve making an account: every screen of the path §61 exists for opened by telling him the opposite of the path he was on (Ray's candidate .11 walk, defect 3.1, screenshots 13, 14, 15 and 17). INFORMATIONAL: it is the standing description of the arrangement, and the controls that change the arrangement are the route buttons on the screen below it. `ui/tests/phone.js` check 9c walks all three. This is the Anywhere path. It deliberately does NOT repeat the home path's \"nothing of what you say goes anywhere else\": on the tailnet the packets are encrypted end to end between the two devices but can be relayed by Tailscale's own servers when a direct connection cannot be made, so the claim is about what is encrypted and where the conversation is kept, both of which this Mac can stand behind."
  },
  {
    "s": "Pairing codes run out on purpose, so one left on a screen cannot be used later. Nothing is wrong and nothing was lost. Press Show me another code and a fresh one appears here.",
    "c": "ACTIONABLE",
    "control": "#phone-refresh",
    "why": "The expiry block on the pairing screen (`#phone-expired`), and the fix for Ray's candidate .11 defect 3.3. A code used to run out in silence: the Mac dropped the window, the next poll came back without one, and the dialog fell back to an earlier screen with the QR, the code and the six words gone and nothing said — \"he comes back to a screen that looks like he imagined the whole thing\". It is ACTIONABLE and the control is the button immediately below it, which is the one thing on that screen. The state is derived from the Mac rather than from this sheet: serving, nothing paired, no live code means the window ran out, and that survives the sheet being closed and reopened. `ui/tests/phone.js` check 8 asserts the screen does not move, shows no code, says what happened and offers another."
  },
  {
    "s": "That code has run out. Ask for another one.",
    "c": "ACTIONABLE",
    "control": "#phone-refresh",
    "why": "The countdown's own last frame, one second before the expiry block above takes over. It read \"That code has expired.\" and now reads \"run out\", which is the word the block beside it uses — one event should not have two names on one screen. Same control: the button that issues a fresh code."
  },
  {
    "s": "Forgetting it here stops this Mac answering it, deletes the keys and stops this Mac answering at its Tailscale name. There is nothing to remove from your phone.",
    "c": "ACTIONABLE",
    "control": "#phone-forget",
    "why": "One of the five answers the paired card picks between, and the reason it picks rather than defaults: the copy is derived from `paired_via` and `platform` on the device record (`src-tauri/src/phone/device.rs`), recorded once at pairing. It used to be two paragraphs in the markup chosen by the sheet's own route variable, which `open()` resets on every open — so reopening the card for a paired Android phone on the Tailscale path printed iOS profile-removal steps for a certificate that path never installs (Ray's candidate .11 walk, defect 3.2, reproduced twice). `ui/tests/phone.js` check 9b walks all five and asserts the second open says exactly what the first did. It is ACTIONABLE for the same reason the sixteen home-path taps are: the thing acted on is his phone, and the one control the whole block is about is the button that forgets it. This is the Tailscale path, on any phone: no profile is ever installed on it."
  },
  {
    "s": "Download it, install it, then come back here. This screen moves on by itself when it sees it.",
    "c": "ACTIONABLE",
    "control": "#phone-ts-open",
    "why": "Screen 2, the second half: what to do and that the screen will move on by itself. One of Urban's three \"something is happening in somebody else's software\" screens (§3 rows 2, 3 and 5). Detection moves the screen on by itself; `#phone-ts-open` is the control that opens the exact thing the sentence names — the download page, the Tailscale app, or the admin console — and the address stays written out beside it. `ui/tests/phone.js` check 16 asserts both, on all three screens."
  },
  {
    "s": "I can only open the pages these screens name.",
    "c": "NOT-RENDERED",
    "why": "`src-tauri/src/opener.rs`'s own refusal. It does NOT reach the screen: `openExternal` in `phone.js` discards the error and shows its own sentence, because a Mac that would not open a page needs to be told to read the address printed above it rather than told which table the address was not in. The string exists for the Mac's log."
  },
  {
    "s": "I cannot tell which identity this Mac is signed in to. Whichever it is, sign in with exactly the same one on your phone.",
    "c": "ACTIONABLE",
    "control": "#phone-ts-start",
    "why": "The honest answer when the daemon reports a name but no identity for it: the screen cannot say WHICH account to reuse, so it says so and still says what to do. §61.1's whole point is that \"use the same account\" is advice nobody can follow — this is the one case where that is all there is, and it is said plainly rather than dressed up."
  },
  {
    "s": "I could not find Tailscale on this Mac to open it.",
    "c": "NOT-RENDERED",
    "why": "`src-tauri/src/opener.rs`'s own refusal. It does NOT reach the screen: `openExternal` in `phone.js` discards the error and shows its own sentence, because a Mac that would not open a page needs to be told to read the address printed above it rather than told which table the address was not in. The string exists for the Mac's log."
  },
  {
    "s": "I could not open that on this Mac. The address is written out above. Type it into your browser.",
    "c": "ACTIONABLE",
    "control": "#phone-ts-open",
    "why": "What the screen says when the Mac would not open the address. The control is the one that just failed — trying again is the first thing to do — and the sentence points at the address printed above it, which is why it is printed: Urban's §2, a control that opens somewhere the user cannot see first is asking for trust it has not earned."
  },
  {
    "s": "Install Tailscale on this Mac",
    "c": "ACTIONABLE",
    "control": "#phone-ts-open",
    "why": "Screen 2's heading. One of Urban's three \"something is happening in somebody else's software\" screens (§3 rows 2, 3 and 5). Detection moves the screen on by itself; `#phone-ts-open` is the control that opens the exact thing the sentence names — the download page, the Tailscale app, or the admin console — and the address stays written out beside it. `ui/tests/phone.js` check 16 asserts both, on all three screens."
  },
  {
    "s": "Open the Tailscale console",
    "c": "CONTROL",
    "why": "#phone-ts-open's label on Screen 7. It opens `console.tailscale.com/admin/dns`, which is one of five addresses `src-tauri/src/opener.rs` will open and the only one that screen names."
  },
  {
    "s": "Sign in to Tailscale on this Mac",
    "c": "ACTIONABLE",
    "control": "#phone-ts-open",
    "why": "Screen 3's heading. One of Urban's three \"something is happening in somebody else's software\" screens (§3 rows 2, 3 and 5). Detection moves the screen on by itself; `#phone-ts-open` is the control that opens the exact thing the sentence names — the download page, the Tailscale app, or the admin console — and the address stays written out beside it. `ui/tests/phone.js` check 16 asserts both, on all three screens."
  },
  {
    "s": "Sign in with <strong>",
    "c": "FRAGMENT",
    "why": "The head of step 2 of the phone steps, joined to the account phrase. Never rendered alone."
  },
  {
    "s": "Sign in with the <strong>same</strong> identity you used on this Mac. A different provider makes a different network and then the two will never see each other.",
    "c": "ACTIONABLE",
    "control": "#phone-refresh",
    "why": "Step 2 of the phone steps when the Mac cannot name its own account — the generic form of the sentence §61.1 exists for. One of the phone-side steps on the Tailscale route (CEO §61 / §61.1). It is an instruction and therefore not INFORMATIONAL — but the thing the user acts on is their phone, not this window, so the control named is the one control the whole block is about: the button that issues a fresh code. Same reasoning, and the same control, as the sixteen home-path taps above. `ui/tests/phone.js` check 13 asserts this block opens with the why and names the account the Mac signed in with."
  },
  {
    "s": "Tailscale gives each machine a name on your network and this Mac does not have one I can use. In Tailscale's own admin console, turn on MagicDNS and HTTPS certificates for your network, then come back.",
    "c": "ACTIONABLE",
    "control": "#phone-ts-open",
    "why": "Screen 7: signed in, and the tailnet has not been told to issue certificates. MagicDNS is named because it is Tailscale's own label for the switch and the only string that will let somebody find it. One of Urban's three \"something is happening in somebody else's software\" screens (§3 rows 2, 3 and 5). Detection moves the screen on by itself; `#phone-ts-open` is the control that opens the exact thing the sentence names — the download page, the Tailscale app, or the admin console — and the address stays written out beside it. `ui/tests/phone.js` check 16 asserts both, on all three screens."
  },
  {
    "s": "Tailscale is installed here but not signed in. Open it and sign in. Any of the sign-in choices it offers is fine and the free plan is enough. I cannot do this part for you.",
    "c": "ACTIONABLE",
    "control": "#phone-ts-open",
    "why": "Screen 3's first note. One of Urban's three \"something is happening in somebody else's software\" screens (§3 rows 2, 3 and 5). Detection moves the screen on by itself; `#phone-ts-open` is the control that opens the exact thing the sentence names — the download page, the Tailscale app, or the admin console — and the address stays written out beside it. `ui/tests/phone.js` check 16 asserts both, on all three screens."
  },
  {
    "s": "Tailscale is not running.",
    "c": "NOT-RENDERED",
    "why": "The `tailscale` command line's OWN stderr sentence, quoted in `tailnet.rs` only so `Diagnostic::classify` can recognize it. The raw text never leaves that module — it can carry a `tskey-…` — and what the screen shows is the state's own sentence instead."
  },
  {
    "s": "Tailscale is signed in, but this Mac has no name yet",
    "c": "ACTIONABLE",
    "control": "#phone-ts-open",
    "why": "Screen 7's heading. One of Urban's three \"something is happening in somebody else's software\" screens (§3 rows 2, 3 and 5). Detection moves the screen on by itself; `#phone-ts-open` is the control that opens the exact thing the sentence names — the download page, the Tailscale app, or the admin console — and the address stays written out beside it. `ui/tests/phone.js` check 16 asserts both, on all three screens."
  },
  {
    "s": "Tailscale is somebody else's app and it is free for one person. It gives this Mac a name your phone can reach from anywhere and it is the whole reason this path has no certificate in it.",
    "c": "INFORMATIONAL",
    "why": "What Tailscale is and why this path uses somebody else's app, said once before the user installs it. Nothing to do in this sentence — the doing is in the note under it and in `#phone-ts-open` beside it, which is where the instruction lives."
  },
  {
    "s": "Waiting for your phone to join…",
    "c": "INFORMATIONAL",
    "why": "The phone-watch line before the grace window elapses. Genuinely nothing to do: the phone is being installed and signed in to somewhere else, and calling that a mistake before a measured wait would be the screen guessing — which is the failure §61.1 is about, in the other direction."
  },
  {
    "s": "Whichever you pick, you will sign in to the SAME one on your phone: your Tailscale account is your private network and devices signed in to it are what can reach each other. Nothing else connects them.",
    "c": "ACTIONABLE",
    "control": "#phone-ts-open",
    "why": "The identity warning in its short form, on the screen where the account is actually created — §61.1's \"before a RichOS user creates any Tailscale account\". Telling somebody afterwards is telling them after they have already chosen, and the fix by then is a sign-out. One of Urban's three \"something is happening in somebody else's software\" screens (§3 rows 2, 3 and 5). Detection moves the screen on by itself; `#phone-ts-open` is the control that opens the exact thing the sentence names — the download page, the Tailscale app, or the admin console — and the address stays written out beside it. `ui/tests/phone.js` check 16 asserts both, on all three screens."
  },
  {
    "s": "You signed in with <strong>",
    "c": "FRAGMENT",
    "why": "The head of the sentence that names the identity the Mac signed in with; the account phrase and the instruction are appended at run time. It gained its `<strong>` for Urban's G14 — this is the screen that names the account FIRST and it was the one screen that gave it no weight, while the two screens after it set the same account in bold. `ui/tests/phone.js` check 12 asserts the whole of it and check 26 asserts the weight and the position."
  },
  {
    "s": "</strong>. Use exactly this on your phone.",
    "c": "FRAGMENT",
    "why": "The tail of that same sentence, after the account phrase. It never renders on its own, and it exists as a separate literal only because Urban's G14 put the account in `<strong>` — before that the line was one `textContent` assignment. `ui/tests/phone.js` check 26."
  },
  {
    "s": "Your Tailscale account <strong>is</strong> your private network. Devices signed in to it can reach each other. So on your phone, sign in with <strong>",
    "c": "FRAGMENT",
    "why": "The head of the phone step's why-line when the Mac CAN name its account; the account phrase and the tail are appended. Never rendered alone."
  },
  {
    "s": "Your Tailscale account <strong>is</strong> your private network. Devices signed in to it can reach each other. So sign in on your phone with the same account you used on your Mac, even if you normally keep them separate. Nothing else connects them.",
    "c": "ACTIONABLE",
    "control": "#phone-refresh",
    "why": "The phone step's why-line when the Mac cannot name its account. It comes before the steps deliberately: it is asking the user to break a habit — *\"I would normally absolutely NEVER use the same identity on the Mac and on the phone\"* — and an instruction without its reason is one they will follow correctly and wrongly. One of the phone-side steps on the Tailscale route (CEO §61 / §61.1). It is an instruction and therefore not INFORMATIONAL — but the thing the user acts on is their phone, not this window, so the control named is the one control the whole block is about: the button that issues a fresh code. Same reasoning, and the same control, as the sixteen home-path taps above. `ui/tests/phone.js` check 13 asserts this block opens with the why and names the account the Mac signed in with."
  },
  {
    "s": "Your phone (",
    "c": "FRAGMENT",
    "why": "Part of the sentence that says where the phone is (CEO §61.1 detection half). Composed at run time from the device name the daemon reports, so it never renders on its own. The whole sentence is one of three — on this network; on this network but switched off; not here yet, sign in with <account> — and `ui/tests/phone.js` checks 14 and 15 assert all three."
  },
  {
    "s": "Your phone is not on this network yet. On the phone, sign in with",
    "c": "FRAGMENT",
    "why": "Part of the sentence that says where the phone is (CEO §61.1 detection half). Composed at run time from the device name the daemon reports, so it never renders on its own. The whole sentence is one of three — on this network; on this network but switched off; not here yet, sign in with <account> — and `ui/tests/phone.js` checks 14 and 15 assert all three."
  },
  {
    "s": "Your phone is not on this network yet. On the phone, sign in with the same account this Mac uses, then come back here. Reinstalling the app will not help: the identity is the only thing that decides it.",
    "c": "ACTIONABLE",
    "control": "#phone-ts-start",
    "why": "The mismatch, named, when the Mac cannot name its own account. A phone signed in to a different identity is in a different tailnet and appears nowhere at all, so its absence is the evidence — and the sentence also closes the loop the CEO actually ran: reinstalling the app three times. The control is the one on the screen this renders on."
  },
  {
    "s": "a certificate chain with nothing in it",
    "c": "NOT-RENDERED",
    "why": "`listen.rs`'s own refusal when a certificate chain arrives empty. It becomes `PhoneError::Crypto`, and every phone error the screen can reach goes through `ceo_sentence()` — the detail goes to the log, exactly as the thirty rows above this block record."
  },
  {
    "s": "this Mac would not open it.",
    "c": "NOT-RENDERED",
    "why": "`src-tauri/src/opener.rs`'s own refusal. It does NOT reach the screen: `openExternal` in `phone.js` discards the error and shows its own sentence, because a Mac that would not open a page needs to be told to read the address printed above it rather than told which table the address was not in. The string exists for the Mac's log."
  },
  {
    "s": "A code is live for",
    "c": "FRAGMENT",
    "why": "The head of the Settings row's second line (`phone.js` `settingsLine`), completed by `remaining()` — the SAME helper the sheet's countdown is built from, so the row and the sheet can never state different amounts of time. Rendered as one sentence: \"A code is live for 4 more minutes\". Ray's candidate-.16 defect D2: with port 8443 open and a code live, `#set-phone-open` read `Use Rich from your phone >` and nothing else, byte-for-byte the row it is with nothing happening. The whole sentence is ACTIONABLE and its control is the row it sits in — `#set-phone-open` opens the sheet with `Stop and go back` on it — and `ui/tests/settings-fit.js` (two D2 checks) asserts the words, the 16px floor and the absence of a line when there is nothing to say."
  },
  {
    "s": "<div class=\"overlay-panel\"> <!-- THE BOX THAT SCROLLS, AND IT IS NOT THE PANEL ANY MORE. The panel is a flex column of exactly two children: this, which scrolls and the action row below, which does not. See .phone-scroll and .phone-actions in style.css for the measurements that forced the split and for why a sticky footer over one scrollport was tried first and rejected. Everything the sheet says lives in here; the way out lives underneath it and never moves. --> <div id=\"phone-scroll\" class=\"phone-scroll\"> <h2 id=\"phone-title\" class=\"overlay-title\">Use Rich from your phone</h2> <!-- ONE LEAD, TRUE OF THE ONE PATH. It was a fixed sentence describing the option he had not chosen: it stayed on screen unchanged after he chose the other one, so every screen opened by telling him the opposite of where he was (Ray's candidate .11 defect 3.1, screenshots 13, 14, 15 and 17). The fix then was to key it off the route; CEO 61 removed the route, so the key went with it. Filled from LEAD below. --> <p class=\"overlay-note\" id=\"phone-lead\"></p> <div id=\"phone-route-choice\" hidden class=\"desk-card-actions\"> <button id=\"phone-use-connect\" type=\"button\" class=\"desk-btn\">RichConnect for RichOS</button> <button id=\"phone-use-tailnet\" type=\"button\" class=\"desk-btn\">Use Tailscale</button> </div> <div id=\"phone-connect\" hidden> <h3 class=\"phone-step-title\">RichConnect for RichOS</h3> <p class=\"overlay-note\">Reach this Mac from your phone without setting up a VPN. Your Mac must stay awake with RichOS running.</p> <p class=\"overlay-note\">Connections are encrypted through Cloudflare. Cloudflare can process the traffic; conversations are stored on your devices.</p> <p id=\"phone-connect-status\" class=\"overlay-note\" role=\"status\"></p> <div class=\"desk-card-actions\"> <button id=\"phone-connect-start\" type=\"button\" class=\"desk-btn\">Set up RichConnect for RichOS</button> <button id=\"phone-connect-disable\" type=\"button\" class=\"desk-btn\" hidden>Turn off RichConnect for RichOS</button> </div> </div> <!-- SCREEN 6 — THE PAIRED CARD, AND IT NOW HAS A TOP — Urban's G8. *\"Paired card has no heading and one text color for all five paragraphs — the actionable line reads like housekeeping.\"* Every line on it was --ink-soft at one size and one weight, labels and paragraphs alike: he measured \"Forget this phone\" and \"Close\" at 5.78:1, identical to the four paragraphs above them (Ray's frames 21 and 22). A card where nothing is the top and nothing is the point. So the phone's own name is the heading — it is what the card is ABOUT and it is the one string on it that changes — and the push line, which is the only line that asks the reader to go and do something, is the one that takes --ink. --> <div id=\"phone-paired\" hidden> <h3 class=\"phone-step-title\" id=\"phone-device-name\"></h3> <!-- THE MAC DOES NOT SETTLE A QUESTION THE PERSON HAS NOT ANSWERED YET. This line was the fixed string \"It is paired. Open Rich on it and keep talking.\" and it was on screen while the phone, beside it, was still asking \"They match — pair this phone\" / \"They do not match\" (Ray's nightly .7, defect 2, his frame 13). The six words exist so a person can detect that something other than his Mac answered; a Mac that announces the answer first teaches him the check is ceremonial. It is filled from status.fingerprintConfirmed — the Mac's own record, never anything this sheet remembers. NO BACKTICK IN THIS FILE'S MARKUP, EVER: it is one template literal, so a backtick in a comment ends the string and takes the whole sheet with it. --> <p class=\"overlay-note\" id=\"phone-paired-state\"></p> <!-- AND THE WORDS THEMSELVES, WHILE HE IS BEING ASKED ABOUT THEM. The pairing screen that carries them is hidden the instant \"paired\" flips, which is the same instant the phone starts asking him to compare — so the Mac took its half of the comparison off the screen at exactly the moment he needed it. They go away once he has answered: a fingerprint nobody is checking is chrome. --> <p class=\"phone-words\" id=\"phone-paired-words\" hidden></p> <!-- THE PRESS ON THIS MAC, AND IT IS THE ONLY THING THAT LETS THE PHONE IN — Sage's pairing review F1 (High), richos-hq docs/research/2026-09-24-richconnect-pairing-protocol-review.md. The phone's own \"They match\" was the only confirmation the Mac ever recorded and the device being judged performed the judgment: anybody who saw the code on this screen could pair first and confirm themselves. Until one of these two is pressed the phone's key can answer the six words and do nothing else. \"They do not match\" is the same teardown the phone's own answer runs. Both are kept out of the sheet's first focus (data-no-autofocus): a pairing is not something a reflexive Return may settle. --> <div class=\"desk-card-actions\" id=\"phone-mac-answer\" hidden> <button id=\"phone-mac-match\" class=\"desk-btn desk-btn--confirm\" type=\"button\" data-no-autofocus>They match</button> <button id=\"phone-mac-mismatch\" class=\"desk-btn\" type=\"button\" data-no-autofocus>They do not match</button> </div> <!-- THE SPENT-CODE ALARM, AFTER HE HAD ALREADY CONFIRMED (Sage F1 item 4, \"keep it and warn\"): the phone stays and he is told somebody else used its code. --> <p class=\"overlay-note\" id=\"phone-code-reused\" role=\"status\" hidden></p> <!-- THE ONE LINE ON THIS CARD WITH AN ERRAND IN IT. Either it says the phone can reach him — which is the finish — or it names the thing on the phone that has not been done yet, in that phone's own menu names. Both are the point of the card and both were set in the same ink as the sentence about removing a certificate. --> <p class=\"overlay-note phone-push-line\" id=\"phone-push-state\"></p> <!-- SCREEN 6's LIMIT, on the one screen where they live with it. The route chooser used to carry it too, on the option it was a condition of; that screen is gone (CEO 61), so this is the one place it is said — and it is not repeated on the screens in between, because a limitation on every screen is nagging. --> <p class=\"overlay-note\" id=\"phone-ts-limit\" hidden></p> <!-- ONE FORGET NOTE, FILLED FROM THE DEVICE RECORD. It was two paragraphs, one shown and one hidden, chosen by the sheet's own route variable — which is forgotten on every open by design, so reopening this card for a paired phone redrew the other path's iOS profile-removal steps. Ray's candidate .11 defect 3.2 caught it on an ANDROID phone that had paired over Tailscale and had nothing installed on it at all. One node, filled from status.pairedVia, is the only shape in which the two cannot disagree — see forgetNoteFor below. --> <p class=\"overlay-note\" id=\"phone-forget-note\"></p> <div class=\"desk-card-actions\"> <button id=\"phone-forget\" class=\"desk-btn\" type=\"button\">Forget this phone</button> </div> </div> <!-- SCREEN 5 — THE CODE, AND THEN THE FOUR THINGS TO DO ON THE PHONE. There is no second version of this screen any more. It used to carry, below the code, two unverified-certificate warnings, a trust QR and sixteen numbered taps through Apple's settings — the home path's half — hidden on the Tailscale route and shown on the other one. CEO §61 leaves one route, so those are gone from the product rather than hidden behind a condition and with them the whole class of defect where the wrong half was drawn (Urban's blocker \"esc-20260919T041857Z-640acd39\", Ray's candidate .11 defect 3.1). THE CODE IS FIRST — Urban's G3 and G4. At 1024x700, the app's own minimum and the size it restores itself to, this screen is about three viewport heights tall and pressing \"Set my phone up\" used to land on a wall of instruction with the QR, the six words, the countdown and the only Close all out of sight (his frames 04 and 05). *\"The four phone steps are preparation for a person who has not started; the code is what a person who is standing there with their phone needs. Order the screen for the second person.\"* IT IS THE MARKUP'S OWN ORDER NOW, not a node moved at render time. The mover existed because the other path numbered its headings 1, 2, 3 and the numbers were load-bearing — the certificate at step 1 was what made the address at step 2 open at all — so the code could not go above it there. With one path there is no sequence to contradict and a subtree that is re-inserted on every two-second poll is a subtree that can take the focus out of itself. --> <div id=\"phone-pairing\" hidden> <!-- Sage's F9: Android has its own RichOS app now (CEO 76), so the line no longer sends Android to the web app. --> <p id=\"phone-connect-pair-help\" class=\"overlay-note\" hidden>In the RichOS app on your phone, scan this code. Then compare the six words on both screens.</p> <!-- IT IS HIDDEN AS A WHOLE WHEN THERE IS NO CODE. Every one of its children was already emptied on expiry (Ray's defect 3.3 and his defect B for the last of them); with the block at the top of the screen an empty wrapper is a gap where the thing he came for used to be, so the container goes with its contents. --> <div id=\"phone-code-block\"> <h3 class=\"phone-step-title\" id=\"phone-code-title\">Point your phone's camera at this</h3> <div class=\"phone-qr-row\"> <canvas id=\"phone-qr-pair\" class=\"phone-qr\" width=\"1\" height=\"1\" role=\"img\"></canvas> <div> <p class=\"overlay-note phone-url\" id=\"phone-pair-url\"></p> <p class=\"overlay-note\" id=\"phone-countdown\" role=\"status\"></p> </div> </div> <h3 class=\"phone-step-title\" id=\"phone-words-title\">Check the six words match</h3> <!-- IT GOES WITH THE CODE, AND IT NAMES A CONTROL THAT IS ON THE SCREEN. Ray's candidate-.12 defect B: after the code ran out this paragraph stayed, telling him to compare against \"these six\" with no six words under it and to \"tap Cancel\" with no Cancel button anywhere in the sheet (frame 19). Two things a person is asked to do that are not there. The heading above it and the words below it were already cleared with the code; this one was missed, so it is now hidden by the same live test they are — and the control it names is Close, which is the button this sheet actually has (id phone-close and the sheet's own data-dismiss). --> <!-- AND THE WORDS ARE NOT HERE ANY MORE — Sage's review 3.1 steps 1 and 4. They used to be printed beside the code before any phone had arrived, which made them carry nothing the code itself does not: anybody who could see the code could see the words. They are now derived over the key the phone actually registered, so they can only exist once a phone has reached this Mac and they appear on the paired card beside the two buttons that answer them. --> <p class=\"overlay-note\" id=\"phone-words-note\">Once your phone opens this code, six words appear on it and on this Mac. If they are the same six, press They match here. If they are not, press They do not match.</p> </div> <!-- THESE FOUR STEPS ARE THE CEO'S OWN, AND THEY OVERRIDE URBAN'S THREE. He installed and uninstalled Tailscale on Android THREE TIMES looking for a \"connect to Mac\" step that does not exist. Urban's Screen 5 had three steps and stopped at \"sign in\", which is where that hunt begins: the app is signed in, nothing says it is finished and the user goes looking for the pairing screen Tailscale does not have. So the two missing steps — the VPN permission prompt and the switch reading Connected — are what end the hunt and the sentence below says outright that the thing he was hunting for is not there. Recorded in docs/verification/tailscale-path-2026-09-18.md. NOTHING ABOUT TAILNETS, MagicDNS OR MACHINE NAMES ON THIS SCREEN, by the same ruling. Those words belong on the Mac's own screens; here they are vocabulary for a thing the user does not have to think about. --> <div id=\"phone-ts-steps\"> <h3 class=\"phone-step-title\">On your phone, four things</h3> <!-- THE WHY COMES BEFORE THE STEPS and it is here because the CEO said it is not obvious: *\"in hindsight this sounds obvious, but it's absolutely NOT obvious at all. Especially given that I would normally absolutely NEVER use the same identity on the Mac and on the phone.\"* A person whose habit is to keep two identities apart will follow a step that says \"sign in\" and use the WRONG one, correctly by their own lights and the result is two networks that never see each other with nothing on either screen saying why. So the reason is given before the instruction and it names the habit it is asking them to break. --> <p class=\"overlay-note\" id=\"phone-ts-why\"></p> <ol class=\"phone-steps\"> <li class=\"overlay-note\">Install Tailscale from the store.</li> <li class=\"overlay-note\" id=\"phone-ts-step2\">Sign in with the <strong>same</strong> identity you used on this Mac. A different provider makes a different network and then the two will never see each other.</li> <li class=\"overlay-note\">Allow the VPN connection your phone asks about.</li> <li class=\"overlay-note\">The switch says Connected.</li> </ol> <p class=\"overlay-note phone-url\" id=\"phone-ts-store\"></p> <p class=\"overlay-note\"><strong>There is no pairing step in Tailscale.</strong> Sign in with the same account on both devices and they are connected.</p> <!-- THE MISMATCH IS DETECTED, NOT ONLY DESCRIBED — the CEO puts it at 9 in 10 users. A phone on the SAME account shows up in the daemon's own Peer list; one on a different provider's account is in a different tailnet and shows up nowhere. So this line is evidence rather than advice and it names the account to use. --> <p class=\"overlay-note\" id=\"phone-ts-peer\" role=\"status\"></p> <!-- THE TRIPWIRE, AND IT IS NOW THE ONLY SENTENCE IN THE PRODUCT ABOUT INSTALLING ANYTHING ON A PHONE. Nothing this Mac serves asks for a profile any more, so a phone that is asked for one is on an origin that is not this Mac's — and the only person who can notice that is the person holding the phone. --> <p class=\"overlay-note\">Nothing has to be installed on your phone for this. If your phone asks you to install a profile, something is wrong. Tell me.</p> </div> <!-- THE ONE FAILURE THIS PATH ACTUALLY DIES OF, named where it happens. Sage's §2.3 failure mode is that the name simply stops resolving; from the phone that looks like \"cannot connect\" with nothing saying why and two different Tailscale accounts produce exactly that, silently. --> <p class=\"overlay-note\" id=\"phone-ts-failure\" hidden>If the code opens to a page that cannot connect, your phone is signed in to a different Tailscale account than this Mac.</p> <!-- WHAT HAPPENS WHEN THE CODE RUNS OUT, ON THE SCREEN IT RAN OUT ON. It used to happen in silence: the Mac dropped the window, the next poll returned no code and the dialog fell back to an earlier screen with the QR, the code and the six words simply gone. Ray's candidate .11 defect 3.3 — \"he comes back to a screen that looks like he imagined the whole thing\". The screen stays; the code is replaced by this. --> <div id=\"phone-expired\" hidden> <h3 class=\"phone-step-title\">That code ran out</h3> <p class=\"overlay-note\" id=\"phone-expired-note\" role=\"status\"></p> </div> <!-- THE WAY BACK OUT, ON THE ONE SCREEN THAT DID NOT HAVE ONE — Urban's G2. *\"Once Set my phone up is pressed, the route chooser is unreachable for the life of the app process\"* — the expired state sets \"pairing\" as well, so waiting did not give it back either and he could not re-reach \"This Mac is ready\" in dark at all after his walk. **THE CONTROL SURVIVES ITS OWN NAME.** It used to read \"Pick a different way\", which named the chooser it returned to; with one path there is no different way to pick and the half of this button that was never about the chooser is the half that matters — it STOPS SERVING. It undoes a socket and a live code, which is why it calls phone_stop_pairing rather than only redrawing and the screen it returns to is \"This Mac is ready\". See PhoneRuntime::stop_pairing for why that is not phone_forget. The four plain copies of this control — on the identity screen, on the two waiting screens and on \"This Mac is ready\" — are gone with the chooser they led to. Nothing was behind them to put down and a button that returns to the screen you are already on is a button that does nothing. Close is on every screen and always was. **AND THE TWO OF THEM NOW LIVE IN THE FOOTER, NOT HERE** — Ray's candidate-.16 new defect. Measured in this harness at 1024x700, dark, with a code live: the panel is 592px tall in a 700px window and its content is 1196px, so on a REOPENED sheet the three controls sat at window y 1040..1116 — \"Show me another code\", \"Stop and go back\" and \"Close\", none of them on screen, under sixteen steps of how-to. Ray read the same thing off AX on the real app: y=1175..1364 against a window bottom of y=792. See .phone-actions in style.css for the pin. --> </div> <!-- SCREEN 0 — THE IDENTITY TRAP, AND IT COMES BEFORE THE DOWNLOAD. CEO §61.1, his ruling in his own words: *\"this barrier or I would call it stupidity would need to be made ABSOLUTELY UBER MEGA SUPER CRYSTAL-CLEAR to ever user of RichOS\"*. What he hit: Tailscale has no email-and-password sign-in, only \"Sign in with Apple / Google / Microsoft / GitHub\"; the account IS the network; he signed in with Apple on the Mac and Google on the Android, got two networks that cannot see each other and REINSTALLED Tailscale on the Android three times looking for a \"connect to Mac\" step that does not exist. Nothing on Tailscale's screens says any of this. **WHY IT IS ITS OWN SCREEN AND NOT A LINE ON THE DOWNLOAD SCREEN.** By the time somebody is on the download screen the next thing they do is create the account and the choice of identity is made inside somebody else's sign-in sheet where nothing of ours can reach them. A warning that arrives after that choice is a warning that costs a sign-out. His own sentence is the reason it cannot be a footnote: *\"in hindsight this sounds obvious, but it's absolutely NOT obvious at all. Especially given that I would normally absolutely NEVER use the same identity on the Mac and on the phone.\"* It is shown while the Mac has no Tailscale account yet — which is exactly the window in which the choice can still be made freely — and never again after detection can name one, because from then on the screens say WHICH account rather than warning about the choice. --> <div id=\"phone-identity\" hidden> <h3 class=\"phone-step-title\">First: Tailscale has no password</h3> <p class=\"overlay-note\">Tailscale has no username and password. It only offers <strong>Sign in with Google</strong>, <strong>Apple</strong>, <strong>Microsoft</strong> or <strong>GitHub</strong> and whichever one you pick, that identity <strong>is</strong> your private network.</p> <p class=\"overlay-note\">So this Mac and your phone have to sign in with the <strong>same</strong> one. Two different identities make two separate networks that cannot see each other and neither device says so: the phone simply never finds this Mac.</p> <p class=\"overlay-note\">There is no \"connect to my Mac\" step in Tailscale, on either device. Signing in on both with the same identity is the whole connection. If the phone cannot find this Mac, reinstalling Tailscale will not help: the identity is the only thing that decides it.</p> <p class=\"overlay-note\">If you would rather not use a personal identity on both devices, make one that is only for this: a new Google account, which costs nothing and sign in with that one here and on the phone.</p> <div class=\"desk-card-actions\"> <button id=\"phone-identity-ok\" class=\"desk-btn desk-btn--confirm\" type=\"button\">I understand, show me what to do</button> </div> </div> <!-- SCREENS 2, 3 and 7 — the three \"something is happening elsewhere\" states. One block, because they differ only in their words and their one address: each is a thing the user does in somebody else's software and detection is what moves the screen on. --> <div id=\"phone-ts-wait\" hidden> <h3 class=\"phone-step-title\" id=\"phone-ts-heading\"></h3> <p class=\"overlay-note\" id=\"phone-ts-note1\"></p> <p class=\"overlay-note\" id=\"phone-ts-note2\"></p> <p class=\"overlay-note phone-url\" id=\"phone-ts-url\"></p> <div class=\"desk-card-actions\"> <button id=\"phone-ts-open\" class=\"desk-btn desk-btn--confirm\" type=\"button\"></button> <button id=\"phone-ts-recheck\" class=\"desk-btn\" type=\"button\">Check again</button> </div> </div> <!-- SCREEN 4 — THIS MAC IS READY. No separate \"turn serving on\": once the name is known there is no decision left, so \"Set my phone up\" does both, under the label the product already uses. --> <div id=\"phone-ts-ready\" hidden> <h3 class=\"phone-step-title\">This Mac is ready</h3> <!-- WHICH IDENTITY IT SIGNED IN WITH, READ OFF THE MAC (§61.1). \"Use the same account\" is advice nobody can follow, because the one thing the user does not know is which one they used — that is the CEO's own account of the evening. This names it. **FIRST, AND IN BOLD** — Urban's G14. It was the third paragraph, under the machine name, set in --ink-soft with the account given no weight at all and visually identical to the paragraphs either side of it (his frame 02). Two screens later the same account IS set in <strong> (his frame 04). *\"That asymmetry is backwards: the screen that names the account first is the one that whispers it ... it is the thing that decides whether this works and the machine name is not.\"* AND THE PARAGRAPH BELOW STILL READS RIGHT, which is why the account went ABOVE the name rather than the name below it: \"That is this Mac's name on your own Tailscale network\" points at the line before it, so the name has to stay immediately in front of it. Heading, account, name, what the name is — each sentence next to the thing it is about. --> <p class=\"overlay-note\" id=\"phone-ts-account\"></p> <p class=\"phone-tailnet-name\" id=\"phone-ts-name\"></p> <p class=\"overlay-note\">That is this Mac's name on your own Tailscale network. Only devices signed in to your Tailscale account can reach it and no port on this Mac is open to the internet.</p> <!-- AND THE WATCH LIVES HERE TOO, not only on the phone step. MEASURED: the pairing window is 60 s (PAIRING_WINDOW_MS) and the join grace is 60 s and the watch starts strictly AFTER the window opens — so on the phone step alone the mismatch sentence could never be reached at all: by the time it was due, the code had expired and the block was hidden. Installing Tailscale on a phone and signing in takes minutes, not seconds, so this screen — the one the user is returned to when the code runs out — is where the sentence has to be able to appear. --> <p class=\"overlay-note\" id=\"phone-ts-peer-ready\" role=\"status\"></p> <div class=\"desk-card-actions\"> <button id=\"phone-ts-start\" class=\"desk-btn desk-btn--confirm\" type=\"button\">Set my phone up</button> </div> </div> <!-- SCREEN 8 — THE ALARM BUTTON, ANSWERED. Ray's nightly .8 walk in the test VM, defect 1 (HIGH): pressing \"They do not match\" on the phone dropped the credential and the sheet in front of the person did not change at all. The Mac went on showing the pairing card, with the six words under it, while it had already forgotten the phone — and, until the commit this screen arrives in, while it was still answering on 8443. *\"He has no way to know the Mac heard him.\"* THE SIX WORDS EXIST FOR EXACTLY ONE SCENARIO: something other than his Mac on the other end. A Mac that says nothing when the alarm is pressed teaches him the check is ceremonial, which is worse than not asking at all. IT OUTRANKS EVERY OTHER SCREEN while it is true — see render(). What is true at that moment is not \"This Mac is ready\", even though the tailnet is; the ready screen would be the sheet forgetting the thing that just happened. AND IT HAS THE ONE CONTROL THAT LEAVES IT, which is also the only thing left to do: start again, from a Mac he now trusts. Close is in the footer as it is on every screen. The Mac cannot clear this by itself — nothing else changes it, so nothing else may. --> <div id=\"phone-rejected\" hidden> <h3 class=\"phone-step-title\">I stopped</h3> <p class=\"overlay-note\" id=\"phone-rejected-note\" role=\"status\"></p> <div class=\"desk-card-actions\"> <button id=\"phone-rejected-again\" class=\"desk-btn desk-btn--confirm\" type=\"button\">Set my phone up again</button> </div> </div> <!-- WHAT THE MAC IS DOING, OR WHY IT COULD NOT — OUTSIDE EVERY STATE BLOCK, WHICH IS A MOVE THIS CHANGE FORCED. It lived inside \"#phone-pairing\", which is hidden on five of the six screens. That was survivable while \"Set my phone up\" was pressed on a screen whose own failure mode was a certificate this Mac makes for itself and can hardly fail to make. It is not survivable now: with one path, \"phone_begin_pairing\" fails whenever Tailscale will not issue a certificate for this Mac and that failure is read on \"This Mac is ready\" — a screen that would have shown the sentence to nobody. \"Making a certificate for your phone…\" had the same problem pointing the other way and so did the sentence \"Get Tailscale\" shows when the Mac cannot open a link. The SOCKET DUMP that used to sit beside it is still gone — Urban's G5, *\"the one thing on this flow that looks like a debug log that shipped ... deletion is the change I want most on this screen\"*. The addresses are still printed to the log at every start, which is where a diagnosis is read from. --> <p class=\"overlay-note\" id=\"phone-message\" role=\"status\"></p> </div> <!-- ONE CLOSE, OUTSIDE THE THREE STATES AND ALWAYS VISIBLE. The first version of this sheet had three, one inside each state block and ui/tests/escape.js refused it on the spot: data-dismiss names ONE control, the document-level Escape handler clicks it and a button hidden with its block does nothing at all. Two of the three states could not be dismissed from the keyboard. One button that is always on screen is both the simpler markup and the only shape that can satisfy the contract. **\"ALWAYS VISIBLE\" WAS A CLAIM ABOUT THE MARKUP AND NOT ABOUT THE SCREEN, AND A PERSON FOUND THE DIFFERENCE.** One Close outside the three states is what makes it always RENDERED; it is .phone-actions below that makes it always SEEN. With a code live at 1024x700 the panel's content is 1196px in a 590px scrollport and this row sat 445px below the window's bottom edge while the six-words note three screens above it read *\"press Close and tell me\"*. The row is now pinned to the bottom of the scrollport with position:sticky, so the how-to scrolls under it and the way out does not leave. AND THE PAIRING SCREEN'S TWO CONTROLS JOIN IT, for the same reason and in one row: two separately-pinned rows would overlap. They are hidden with the screen they belong to — by their own hidden attribute, set in render() from the same condition that shows the pairing block, because a display:none button contributes no flex gap and the row collapses to Close alone. Close stays LAST, which is where the copy points. --> <div class=\"desk-card-actions phone-actions\"> <button id=\"phone-refresh\" class=\"desk-btn desk-btn--confirm\" type=\"button\" hidden>Show me another code</button> <button id=\"phone-pairing-back\" class=\"desk-btn\" type=\"button\" hidden>Stop and go back</button> <button id=\"phone-close\" class=\"desk-btn\" type=\"button\">Close</button> </div> </div>",
    "c": "FRAGMENT",
    "why": "Composite markup for the phone sheet, including managed Connect and the optional Tailscale flow. This literal is parsed as HTML, never rendered as one sentence. Connect setup has #phone-connect-start in the same view; enabled and reconnecting states have #phone-connect-disable, and a saved paired phone has #phone-forget. A phone that reached this Mac and waits for the press (Sage's pairing review F1) has #phone-mac-match and #phone-mac-mismatch beside its six words; the pairing screen's note names both, and they are the controls on the card the phone reaches. The persistent #phone-close dismisses every state. ui/tests/phone.js and affordances.js exercise these controls through the visible sheet; contrast.js separately opens managed setup, pairing and recovery alongside the Tailscale surfaces."
  },
  {
    "s": "Forgetting it here stops this Mac answering it and deletes the keys. This phone was paired by an older version of RichOS, which connected a way this one does not use. Pair it again to keep using it. If that older version put a RichOS profile on the phone, you can remove it in the phone's own settings; nothing RichOS does now needs one.",
    "c": "INFORMATIONAL",
    "why": "The paired card's forget note for a record an older build wrote, when RichOS had a second way to pair (CEO §61 removed it). It says what to do — pair the phone again — and, because a cleanup the user has to know about is a cleanup that does not happen, that the older build may have left a profile on the phone. It is INFORMATIONAL rather than ACTIONABLE because the thing to act on is the phone, not this window; the control in the same view is `#phone-forget`, which the card is about. `ui/tests/phone.js` check 9b walks it against the record, on the first open and on a reopen, which is where the defect it replaced lived."
  },
  {
    "s": "Getting this Mac ready…",
    "c": "INFORMATIONAL",
    "why": "What `Set my phone up` says while the Mac mints its keys and asks Tailscale for a certificate. It replaced *\"Making a certificate for your phone…\"*, which described work the removed path did — nothing is made FOR the phone on this one. There is nothing to act on while it is on screen; the controls that produced it are disabled for its lifetime."
  },
  {
    "s": "You can use this anywhere your phone has a signal. Both devices have to be signed in to Tailscale for it to connect, wherever you are.",
    "c": "INFORMATIONAL",
    "why": "The paired card's one condition: both devices signed in to Tailscale. It used to end *\"— at home too\"*, which read as a carve-out from a second path that no longer exists (CEO §61); the condition is not about where the user is standing. Shown only for a phone the record says paired over the tailnet, which is `ui/tests/phone.js` check 9b's `limit` column."
  },
  // Managed Connect states and the bounded backend failures they can surface.
  {
    "s": "Connect identity generation failed",
    "c": "NOT-RENDERED",
    "why": "Internal Crypto detail. The Connect commands call PhoneError::ceo_sentence(), which replaces this text with the already-classified key-generation sentence. It is never copied to the webview."
  },
  {
    "s": "Connect identity is unreadable",
    "c": "NOT-RENDERED",
    "why": "Internal Crypto detail. The Connect commands call PhoneError::ceo_sentence(), which replaces this text with the already-classified key-generation sentence. It is never copied to the webview."
  },
  {
    "s": "Connect is off on this Mac. Its remote address is being removed.",
    "c": "INFORMATIONAL",
    "why": "Durable local disable has completed and remote cleanup is automatic; no additional user action is requested."
  },
  {
    "s": "Connect signing failed",
    "c": "NOT-RENDERED",
    "why": "Internal Crypto detail. The Connect commands call PhoneError::ceo_sentence(), which replaces this text with the already-classified key-generation sentence. It is never copied to the webview."
  },
  {
    "s": "Connecting this Mac…",
    "c": "INFORMATIONAL",
    "why": "Progress of the supervised connector, not a request to configure a vendor account."
  },
  {
    "s": "Forgetting this phone removes its access immediately. You can pair it again with a new code.",
    "c": "INFORMATIONAL",
    "why": "Explains the effect of the adjacent Forget this phone control, without asking for revocation."
  },
  {
    "s": "Keep talking to Rich when you are away from your Mac.",
    "c": "INFORMATIONAL",
    "why": "Describes the phone feature; setup and paired state controls are separately classified and tested."
  },
  {
    "s": "Reconnecting this Mac. Your phone keeps unsent messages until it can reach RichOS again.",
    "c": "INFORMATIONAL",
    "why": "Automatic retry progress and retained-work reassurance. Disable and Forget remain available but are not required to recover."
  },
  {
    "s": "RichConnect for RichOS could not be reached. Your conversations are kept on your devices. Try again shortly.",
    "c": "ACTIONABLE",
    "control": "#phone-connect-start",
    "fixture": "connect-unavailable",
    "why": "Setup failed; the same-view Set up RichConnect for RichOS button is re-enabled for retry after the request completes. The fixture drives the rejection through the real button handler."
  },
  {
    "s": "RichConnect for RichOS returned an invalid Mac address. Try again later.",
    "c": "ACTIONABLE",
    "control": "#phone-connect-start",
    "fixture": "connect-address-invalid",
    "why": "Setup failed; the same-view Set up RichConnect for RichOS button is re-enabled for retry after the request completes. The fixture drives the rejection through the real button handler."
  },
  {
    "s": "RichConnect for RichOS returned an invalid connection credential.",
    "c": "ACTIONABLE",
    "control": "#phone-connect-start",
    "fixture": "connect-credential-invalid",
    "why": "Setup failed; the same-view Set up RichConnect for RichOS button is re-enabled for retry after the request completes. The fixture drives the rejection through the real button handler."
  },
  {
    "s": "The saved setup of RichConnect for RichOS is unreadable. Whoever set RichOS up needs to recover it.",
    "c": "NEEDS-SOMEONE-ELSE",
    "party": true,
    "fixture": "connect-setup-unreadable",
    "why": "Names the person who can perform this repair. The UI does not offer destructive credential/history resets or pretend retry alone repairs it. The real Connect error renderer is exercised with a rejected command."
  },
  {
    "s": "First, set up this RichOS app on your Mac. Then pair your phone with it.",
    "c": "ACTIONABLE",
    "control": "#phone-connect-start",
    "fixture": "connect-setup",
    "why": "The setup instruction and the actual setup control are in the same Connect panel."
  },
  {
    "s": "This Mac already has a phone paired through Tailscale. Forget that phone before changing its connection.",
    "c": "ACTIONABLE",
    "control": "#phone-forget",
    "fixture": null,
    "why": "Protects the existing phone from implicit replacement. The paired phone card offers Forget this phone in the same view; phone.js tests exercise its revocation flow. This backend guard also handles a pairing that raced setup."
  },
  {
    "s": "This Mac is connected.",
    "c": "INFORMATIONAL",
    "why": "Reports connector readiness. No human step is missing from this state."
  },
  {
    "s": "This RichOS build is missing its Connect helper. Whoever set RichOS up needs to install a complete build.",
    "c": "NEEDS-SOMEONE-ELSE",
    "party": true,
    "fixture": "connect-helper-missing",
    "why": "Names the person who can perform this repair. The UI does not offer destructive credential/history resets or pretend retry alone repairs it. The real Connect error renderer is exercised with a rejected command."
  },
  {
    "s": "Unknown Connect action",
    "c": "UNREACHABLE",
    "why": "Developer guard on the hard-coded signed control route table. No UI command supplies a method or path; all shipped callers use one of the accepted routes."
  },
  {
    "s": "{\"awaiting_mac_confirmation\":true,\"reason\":\"Press They match on your Mac.\"}",
    "c": "NOT-RENDERED",
    "why": "The phone channel's 409 body for an authenticated device nobody has confirmed on this Mac yet (Sage's pairing review F1; `routes::AWAITING_MAC_BODY`). It is bytes on the wire to the phone, never drawn in this Mac's webview: the phone reads the flag and shows its own waiting screen, and the Mac's sheet carries its own sentence and the two buttons (#phone-mac-match, #phone-mac-mismatch)."
  },
  {
    "s": "Your Mac could not save the delivery receipt. Your text is still on your phone.",
    "c": "INFORMATIONAL",
    "why": "The authenticated phone API reports a retryable storage failure. The existing phone queue retains the message and retries; this does not ask the person to resend it."
  },
  {
    "s": "Your Mac's message recovery history exceeds its safe limit. Whoever set RichOS up needs to recover it.",
    "c": "NEEDS-SOMEONE-ELSE",
    "party": true,
    "fixture": "connect-history-full",
    "why": "Names the person who can perform this repair. The UI does not offer destructive credential/history resets or pretend retry alone repairs it. The real Connect error renderer is exercised with a rejected command."
  },
  {
    "s": "Your Mac's message recovery history is unreadable. Whoever set RichOS up needs to recover it.",
    "c": "NEEDS-SOMEONE-ELSE",
    "party": true,
    "fixture": "connect-history-unreadable",
    "why": "Names the person who can perform this repair. The UI does not offer destructive credential/history resets or pretend retry alone repairs it. The real Connect error renderer is exercised with a rejected command."
  },
  {
    "s": "Your phone's pairing is saved. RichOS is trying to restore its connection.",
    "c": "INFORMATIONAL",
    "why": "The recovery monitor retries the stopped listener without replacing identity; the saved-pairing card retains its Forget control."
  },
  // Non-blocking questions, round 13.
  {"s":"Change your answer before Rich receives it.","c":"ACTIONABLE","control":".question-option","fixture":"question-edit","why":"The rendered state retains the adjacent retry, answer or recovery control; the fixture exercises the production renderer."},
  {"s":"Copy saved answer","c":"CONTROL","why":"Label of the inline question control, not a separate state."},
  {"s":"It reaches Rich when his current reply ends","c":"INFORMATIONAL","why":"The accepted answer waits for the active turn boundary; no extra permission or send is required."},
  {"s":"Jump to the oldest open question (⌘J)","c":"CONTROL","why":"Label of the inline question control, not a separate state."},
  {"s":"Missing question scope","c":"NOT-RENDERED","why":"Internal command/server validation or fallback implementation diagnostic. Question UI renders its own saved-answer recovery wording; this literal is not a desktop instruction."},
  {"s":"Oldest open question","c":"CONTROL","why":"Label of the inline question control, not a separate state."},
  {"s":"On its way to Rich","c":"INFORMATIONAL","why":"The app has saved the answer and owns delivery."},
  {"s":"Question answers are unavailable.","c":"NOT-RENDERED","why":"Internal command/server validation or fallback implementation diagnostic. Question UI renders its own saved-answer recovery wording; this literal is not a desktop instruction."},
  {"s":"Question receipts are unavailable.","c":"NOT-RENDERED","why":"Internal command/server validation or fallback implementation diagnostic. Question UI renders its own saved-answer recovery wording; this literal is not a desktop instruction."},
  {"s":"Question tool server: {error}","c":"NOT-RENDERED","why":"Internal command/server validation or fallback implementation diagnostic. Question UI renders its own saved-answer recovery wording; this literal is not a desktop instruction."},
  {"s":"Retry saved answer","c":"CONTROL","why":"Label of the inline question control, not a separate state."},
  {"s":"Rich has your answer","c":"INFORMATIONAL","why":"The durable receiving input has been handed off."},
  {"s":"Rich no longer needs this.","c":"INFORMATIONAL","why":"The asker withdrew the question; there is nothing left to answer. The asker's own note stays in the record, never on the card (nightly 66: it carried a card id)."},
  {"s":"Saving your answer…","c":"INFORMATIONAL","why":"The current submission is in progress; duplicate submission is prevented."},
  {"s":"This job stopped before Rich got your answer.","c":"INFORMATIONAL","why":"The job ended (his Stop, or its second failed delivery try) before any back end took the answer; nothing on this card can send it to that ended job."},
  {"s":"This conversation was deleted. This answer was not sent:","c":"ACTIONABLE","control":".question-recovery button","fixture":"question-deleted","why":"The rendered state retains the adjacent retry, answer or recovery control; the fixture exercises the production renderer."},
  {"s":"This conversation was deleted. Your answer was not sent. You can copy it below.","c":"ACTIONABLE","control":".question-recovery button","fixture":"question-deleted","why":"The rendered state retains the adjacent retry, answer or recovery control; the fixture exercises the production renderer."},
  {"s":"Unknown desktop answer method.","c":"NOT-RENDERED","why":"Internal command/server validation or fallback implementation diagnostic. Question UI renders its own saved-answer recovery wording; this literal is not a desktop instruction."},
  {"s":"Waiting for the remaining answers","c":"INFORMATIONAL","why":"This answer is accepted. Other unresolved cards retain their own answer controls."},
  {"s":"Your answer could not be saved on this Mac. Try again after freeing storage.","c":"ACTIONABLE","control":".question-option","fixture":"question-storage","why":"The rendered state retains the adjacent retry, answer or recovery control; the fixture exercises the production renderer."},
  {"s":"Your answer is saved here and has not been confirmed.","c":"ACTIONABLE","control":".question-controls button","fixture":"question-saved","why":"The rendered state retains the adjacent retry, answer or recovery control; the fixture exercises the production renderer."},
  {"s":"Your saved answer could not be confirmed. Retry it when the connection is available.","c":"ACTIONABLE","control":".question-controls button","fixture":"question-retry","why":"The rendered state retains the adjacent retry, answer or recovery control; the fixture exercises the production renderer."},
  {"s":"Your team recommends","c":"INFORMATIONAL","why":"Attributes an optional recommendation. No option is preselected."},
  {"s":"· through Rich","c":"FRAGMENT","why":"Part of the inline question renderer, completed by live content. Dedicated card tests exercise the visible controls."},
  {"s":"↑ ↓ move · 1- choose · · Esc back to the composer","c":"FRAGMENT","why":"Part of the inline question renderer, completed by live content. Dedicated card tests exercise the visible controls."},
  // Desktop Claude quota and weekly reset states. The dedicated quota.js suite walks these controls.
  // Fill-first: several Claude accounts in the quota panel.
  // Round 16: the panel rebuilt to the CEO's chosen design (lanes, the one sentence, speed, the switch).
  {"s": "1 agent took .", "c": "INFORMATIONAL", "why": "Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the fast card's sentence with one agent working: which window rose, from what to what, in how long (quota.js; quota.rs View rises and agentsWorking). It states what happened; nothing to act on. tests/quota.js renders it in both themes with contrast computed."},
  {"s":": show its windows below","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the tail of an account lane's accessible name; clicking the lane shows that account's rulers."},
  {"s":"<b></b> has room: choose <i>switch</i> above and they continue there now.","c":"ACTIONABLE","control":"#quota-verb-switch","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): while agents are paused with another account free, the verb in the sentence above switches them there now. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"<span class=\"quota-lane-note\"><span class=\"quota-lane-pulse\" aria-hidden=\"true\"></span>Claude Code is signing in through your browser. The reading arrives when it is done.</span>","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): an added account's lane while the stock browser sign-in runs; it states what happens next. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"<span>Remove <b></b> from this Mac? Its sign-in here is forgotten; nothing on the account itself changes.</span>","c":"ACTIONABLE","control":"#quota-remove-yes, #quota-remove-no","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): Remove's one inline question, with Remove <label> and Keep it beside it. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"A big rise is expected. Checking every minute.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the status card while a rise is expected and checking is every minute (quota.rs FAST_REFRESH_INTERVAL_MS; the mockup's 2 minutes is not the app's value). tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Add a second Claude account","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the inline Add account form's title when going from one account to two. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Add another Claude account","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the inline Add account form's title with two or more accounts. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Agents hold their place and resume","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the hold sentence when every account is used up and no agent is listed."},
  {"s":"At this speed the next check still comes before 100%, so the at .","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): fast usage whose speed does not move the line; states what RichOS does. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"<section class=\"overlay-panel quota-panel\"> <header class=\"quota-heading\"><div><p class=\"quota-eyebrow\">Settings · Technical view</p> <h2 id=\"quota-title\">Claude Code quota</h2> <p id=\"quota-lede\" class=\"quota-lede\">Straight from Claude Code, shared across every app and session on this account.</p></div> <button id=\"quota-close\" type=\"button\" aria-label=\"Close Claude Code quota\">×</button></header> <div class=\"quota-body\"><div class=\"quota-windows-col\"> <div class=\"quota-toolbar\"><span id=\"quota-freshness\">Loading quota…</span> <div class=\"quota-toolbar-actions\"><button id=\"quota-account-start\" class=\"quota-btn quota-btn-quiet\" type=\"button\">+ Add account</button> <button id=\"quota-refresh\" class=\"quota-btn\" type=\"button\"><svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M21 12a9 9 0 1 1-2.64-6.36\"/><path d=\"M21 3v6h-6\"/></svg><span id=\"quota-refresh-label\">Refresh</span></button></div></div> <p id=\"quota-message\" class=\"quota-message\" role=\"status\" hidden></p> <div id=\"quota-lanes\" class=\"quota-lanes\" role=\"list\" aria-label=\"Claude accounts\" hidden></div> <div id=\"quota-account-new\" class=\"quota-addform\" role=\"group\" aria-labelledby=\"quota-addform-title\" hidden> <p id=\"quota-addform-title\" class=\"quota-addform-title\">Add a second Claude account</p> <div id=\"quota-addform-rows\" class=\"quota-addform-rows\"> <label id=\"quota-account-current-row\" class=\"quota-addform-row\"><span>The account signed in now</span> <input id=\"quota-account-current\" type=\"text\" maxlength=\"40\" placeholder=\"A name only you see\"></label> <label class=\"quota-addform-row\"><span>The new account</span> <input id=\"quota-account-label\" type=\"text\" maxlength=\"40\" placeholder=\"A name only you see, like Work\"></label></div> <div class=\"quota-addform-actions\"><button id=\"quota-account-add\" class=\"quota-btn quota-btn-primary\" type=\"button\">Add and sign in</button> <button id=\"quota-account-cancel\" class=\"quota-btn\" type=\"button\">Cancel</button> <span class=\"quota-muted\">Claude Code signs in through your browser.</span></div></div> <p id=\"quota-account-feedback\" class=\"quota-account-feedback\" role=\"status\" aria-live=\"polite\"></p> <div id=\"quota-windows\" aria-label=\"Claude Code quota windows\"></div> <div id=\"quota-empty\" hidden><h3>Nothing to show yet.</h3><p>RichOS asks Claude Code for its own usage figures and shows exactly what comes back. <span id=\"quota-empty-why\">Claude Code has not answered yet</span>, so there is no number here: <b>not zero, not full, nothing guessed.</b></p><p id=\"quota-empty-next\">If this stays empty after a refresh, Claude Code may not be signed in on this Mac.</p></div> <p class=\"quota-legend\">The gold bar is what you have used; the tick is how far the clock has run. <b>Bar past the tick means you are spending faster than the window is passing.</b></p> <section id=\"quota-reset-offers\" class=\"quota-reset-offers\" aria-label=\"Weekly quota resets\" hidden><button id=\"quota-usage-open\" class=\"quota-btn\" type=\"button\" title=\"claude.ai/new#settings/usage\">Open Claude Usage</button></section> <p id=\"quota-reset-feedback\" role=\"status\" aria-live=\"polite\"></p> </div><form id=\"quota-policy\" class=\"quota-policy\" novalidate> <h3 id=\"quota-policy-title\">Automatic pause</h3> <div class=\"quota-switch-row\"><button id=\"quota-enabled\" class=\"quota-switch\" type=\"button\" role=\"switch\" aria-checked=\"false\" aria-label=\"Automatically pause Rich’s agents\"></button> <div class=\"quota-switch-label\"><span id=\"quota-sentence-lead\">Pause Rich’s agents once the five-hour window passes</span> <label class=\"quota-threshold\"><span class=\"sr-only\">Pause threshold, percent used</span><input id=\"quota-threshold\" inputmode=\"numeric\" type=\"text\" maxlength=\"3\" value=\"93\" aria-describedby=\"quota-validation\">% used</label><span id=\"quota-sentence-end\"><span class=\"quota-muted\">, unless the reset is under 20 minutes away.</span></span> <div id=\"quota-verbs\" class=\"quota-opts\" role=\"radiogroup\" aria-label=\"What happens at the line\" hidden> <button id=\"quota-verb-pause\" class=\"quota-opt\" type=\"button\" role=\"radio\" data-act=\"pause\" aria-checked=\"true\"><span class=\"quota-radio\" aria-hidden=\"true\"></span><span>pause Rich’s agents</span></button> <button id=\"quota-verb-switch\" class=\"quota-opt\" type=\"button\" role=\"radio\" data-act=\"switch\" aria-checked=\"false\"><span class=\"quota-radio\" aria-hidden=\"true\"></span><span id=\"quota-verb-switch-text\">switch to the next account</span></button></div> <span id=\"quota-sentence-tail\" class=\"quota-sentence-tail\" hidden>unless the reset is under 20 minutes away.</span> <p id=\"quota-hint\" class=\"quota-hint\" hidden></p> <div id=\"quota-draft-actions\" hidden><button id=\"quota-save\" class=\"quota-btn quota-btn-primary\" type=\"submit\">Save</button> <button id=\"quota-keep\" class=\"quota-btn\" type=\"button\">Keep 93%</button></div> <p id=\"quota-validation\" role=\"status\" hidden></p> </div></div> <p id=\"quota-save-status\" role=\"status\" aria-live=\"polite\" hidden></p> <div id=\"quota-status-card\" class=\"quota-status-card\"> <h4 id=\"quota-hold-status\">Loading pause status…</h4> <div id=\"quota-hold-detail\" class=\"quota-hold-detail\"></div><ul id=\"quota-held\" aria-label=\"Observed pauses\"></ul> <div class=\"quota-status-actions\"><button id=\"quota-hold-refresh\" type=\"button\" class=\"quota-btn\" hidden>Refresh now</button> <button id=\"quota-release\" type=\"button\" class=\"quota-btn\" hidden>Let them continue now</button></div> </div> <div id=\"quota-boundary-one\" class=\"quota-boundary\"><p><b>A pause is not a stop.</b> Each agent finishes the step it is on, then waits before the next, keeping its place and everything it knows. Because a step is allowed to finish, usage can climb a little past the line.</p> <p>Your conversation with Rich and any Claude Code you run outside RichOS, are never paused.</p></div> <div id=\"quota-boundary-many\" class=\"quota-boundary\" hidden><p><b>A pause is not a stop; a switch is not a restart.</b> Paused, an agent keeps its place and waits; switched, its next step runs on the next account with everything carried over.</p> <p>At <b>99%</b> of an account’s week Rich switches whatever you choose here, always to the account whose week ends soonest; when every account is used up, work waits for the soonest reset.</p></div> </form></div></section>","c":"NOT-RENDERED","why":"HTML scaffold assigned to innerHTML, never displayed as one raw string. The quota.js suite walks the rendered panel, its controls and both themes; the individually emitted state sentences are classified separately."},
  {"s":"and the pause acts only on a number. Rather than wait on Claude Code, Rich’s agents keep working; the pause acts again once a reading shows the five-hour window past <b>%</b>.","c":"INFORMATIONAL","why":"Walk of nightly 36, D1 (docs/verification/2026-10-05-nightly-36-candidate-walk.md): the card when a check came back with no figures (quota.rs Admission::NoReading). It says what the pause does with no number; nothing is held, so nothing is asked of the user. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"<b></b> The figures below are from ago and may have moved on.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the `refresh-failed` notice, also said for a null answer over earlier figures. Refresh sits in the same reading line and the notice's own tail says so."},
  {"s":"<span class=\"quota-muted\">, unless the reset is under 20 minutes away.</span>","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the end of the one-account sentence, set as markup after the threshold field."},
  {"s":"Ask Claude Code","c":"CONTROL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): Refresh's label while there is no reading at all (`unavailable`)."},
  {"s":"Change the number to move the line.","c":"ACTIONABLE","control":"#quota-threshold","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the one-account hint while the pause is on; the threshold field it names is beside it and enabled."},
  {"s":"Claude Code answered without its usage figures just now.","c":"INFORMATIONAL","why":"Walk of nightly 36, D1 (docs/verification/2026-10-05-nightly-36-candidate-walk.md): the head of the notice for a null answer over earlier figures. Refresh is in the same reading line."},
  {"s":"Claude Code answered without its usage figures this time","c":"FRAGMENT","why":"Walk of nightly 36, D1 (docs/verification/2026-10-05-nightly-36-candidate-walk.md): the middle of the empty state's sentence after a null answer."},
  {"s":"Claude Code has not answered yet","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the middle of the `unavailable` empty state's sentence."},
  {"s":"Holding since .","c":"ACTIONABLE","control":"#quota-release","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the `holding` card's head; Let them continue now is on the card."},
  {"s":"No current reading, so nothing is held.","c":"INFORMATIONAL","why":"Walk of nightly 36, D1 (docs/verification/2026-10-05-nightly-36-candidate-walk.md): the card when a check came back with no figures. Nothing is held; Refresh now is on the card for whoever wants a reading sooner."},
  {"s":"Off: the line is only drawn, not enforced.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the one-account hint while the pause is off; the switch beside the sentence turns it on."},
  {"s":"Pause Rich’s agents once the five-hour window passes","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the head of the one-account sentence, before the threshold field."},
  {"s":"Releasing: the reset is away.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the `releasing` card's head; the pause lifts by itself."},
  {"s":"RichOS again in ; Refresh asks sooner.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the tail of the `refresh-failed` notice; it names the control beside it and asks nothing more."},
  {"s":"RichOS asks again in <span class=\"quota-keep\">.</span>","c":"INFORMATIONAL","why":"Walk of nightly 36, D1 (docs/verification/2026-10-05-nightly-36-candidate-walk.md): when the next check comes, on the card with no reading. The span keeps its last two words together (quota-keep)."},
  {"s":"RichOS asks again in ; Ask Claude Code asks now.","c":"INFORMATIONAL","why":"Walk of nightly 36, D1 (docs/verification/2026-10-05-nightly-36-candidate-walk.md): the empty state after a null answer; it names the control in the reading line above it."},
  {"s":"RichOS tries again in .","c":"INFORMATIONAL","why":"When the automatic check runs again after a failure with no figures to show."},
  {"s":"Rich’s agents keep working through the limit. When the five-hour window is spent, Claude Code turns them away until it resets and Rich tells you.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the one-account off card; the switch beside the sentence turns the pause on."},
  {"s":"Straight from Claude Code, shared across every app and session on this account.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the sheet's lede with one account. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"The account in use","c":"FRAGMENT","why":"The account's name when none is known, inside the card's sentence."},
  {"s":"Under 20 minutes to go, so the pause lifts: <b> picking up exactly where stopped.</b> The window resets at .","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the `releasing` card's body; the agents continue by themselves."},
  {"s":"Weekly · per model","c":"FRAGMENT","why":"The label of the absent model-weekly row (round 16 names the model it drew; the app cannot know which is missing)."},
  {"s":"the one the pause watches","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the one-account hero heading's sub-label."},
  {"s":"window is at <b> used</b>, past your % line. at <b></b>, 20 minutes before the reset","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the `holding` card's body; Let them continue now is on the card."},
  {"s":": not reported by Claude Code for this account","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the `one-window` absent row. It reports what Claude Code supplied; nothing to change here."},
  {"s":"Automatic pause is off, so nothing acts before it reaches 100%.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): fast usage with one account and the automatic pause off; the switch beside it turns it on. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Automatic pause or switch","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the decision's title with two or more accounts. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Automatic pause or switch at the line","c":"CONTROL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the accessible name of the one switch with two or more accounts."},
  {"s":"Automatically pause Rich’s agents","c":"CONTROL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the accessible name of the one switch with one account (round 14's)."},
  {"s":"Claude Code opened its sign-in page in your browser. Finish there; the reading arrives here.","c":"ACTIONABLE","control":"#quota-refresh","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): Add account started the stock browser sign-in for the new folder; Refresh in the same panel reads it once it is done (the panel also re-reads when the sign-in ends). tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Every account is used up.","c":"ACTIONABLE","control":"#quota-account-start","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the hold until the soonest reset; adding an account is the way out (round 16 NOTES.md), and + Add account is in the same panel. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"In use: , since .","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the card after a switch: which account is in use and since when. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"No other account has room right now, so Rich would pause instead.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): Switch chosen with no account to switch to; states what happens. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"No other account has room right now, so at the line Rich pauses.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the card after a switch with no next account; states what happens. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Off: nothing happens at %; the line is only drawn.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the hint beside the dimmed verbs while the one switch is off; the switch is beside it. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Off. Nothing is paused.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the card while the one switch is off with two or more accounts; the switch is above it. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"On. Nothing is waiting.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the card with Pause chosen and the switch on; the switch is above it. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"On. Rich switches at the line.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the card with Switch chosen and the switch on; the switch is above it. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Once five-hour window passes","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the head of the one sentence with two or more accounts, around the account's label and the threshold field."},
  {"s":"Rich switched from <b></b> ago . Every agent’s next step ran on ; nothing restarted.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the card after a switch: from where, how long ago, why. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Rich’s agents keep working through the line. If five-hour window runs out, the step Claude Code turns away runs again on the next account with room. The switch at 99% of the week still happens; that is what a second account is for.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the off card's explanation with two or more accounts; the switch is above it. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Several agents just started at once, so Rich checks every minute before the speed is even measured. The at until a jump is measured.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): an expected rise; states what RichOS does. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Straight from Claude Code, for each account signed in on this Mac.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the sheet's lede with two or more accounts. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"The moved to so nothing reaches 100%; to % when the speed comes back down.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): fast usage: the moved lines and their way back. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"The weekly switch ; the five-hour line is off, so nothing acts there.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): fast usage with the switch off and two accounts; the switch is above it. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"Usage is fast. Checking every minute.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the status card while usage is measured fast (every minute, the app's value). tests/quota.js renders it in both themes with contrast computed."},
  {"s":"When reaches its line, the next is <b></b>.","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the card after a switch, naming the next account."},
  {"s": "agents reading at once took .", "c": "INFORMATIONAL", "why": "Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the fast card's sentence with several agents working: how many, which window rose, from what to what, in how long (quota.js; quota.rs View rises and agentsWorking). It states what happened; nothing to act on. tests/quota.js renders it in both themes with contrast computed."},
  {"s": "from % to % in", "c": "FRAGMENT", "why": "Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the rise inside the fast card's sentence (quota.js span), never shown alone; the whole sentence is classified as information."},
  {"s":"at % of its five-hour window","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): why the last switch happened."},
  {"s":"at % of its week","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): why the last switch happened."},
  {"s":"at % · was %","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): a ruler line's label while usage is fast: the moved point and the normal one."},
  {"s":"at <b></b>, when window resets, the soonest.","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the end of the every-account-used-up hold sentence."},
  {"s":"button, input, [tabindex='0']","c":"NOT-RENDERED","why":"A CSS selector for the sheet's focus trap, never displayed."},
  {"s":"button, a[href], input, select, textarea, [tabindex]:not([tabindex=\"-1\"])","c":"NOT-RENDERED","why":"A CSS selector main.js's Tab handler uses to find the focusable controls on a modal question's sheet (D22), never displayed."},
  {"s":"checks every min","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the reading line's cadence at normal speed."},
  {"s":"Asking Claude Code…","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): Refresh's own label while it asks Claude Code; it returns to Refresh by itself when the answer comes."},
  {"s":"Last reading (stale)","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the reading line's stale form, \"Last reading 47 min ago — stale\"; Refresh sits beside it in the same line."},
  {"s":"under a minute","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): a span shorter than a minute, as round 16's fmtDur writes it (\"Checked under a minute ago\")."},
  {"s":"every account used up","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the Settings row's quota summary while every account is used up."},
  {"s":"every minute (a rise is expected)","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the reading line's cadence, in gold, while a rise is expected."},
  {"s":"every minute (usage is fast)","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the reading line's cadence, in gold, while usage is fast."},
  {"s":"five-hour window is at . Agents pause the moment it passes <b>%</b>, unless the reset is under 20 minutes away. Then it is not worth stopping.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the Pause card's body after the account's label. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"five-hour window is at . At <b>%</b>, every agent’s next step runs on <b></b>, the account whose week ends soonest and Rich says so in the conversation. Nothing stops.","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the Switch card's body after the account's label. tests/quota.js renders it in both themes with contrast computed."},
  {"s":"in use: the one the % line watches","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the hero heading's note for the account in use."},
  {"s":"is filling about % a minute.","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the measured speed on the fast card."},
  {"s":"next: takes over when reaches its line","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the hero heading's note for the next account."},
  {"s":"not read yet","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): a five-hour figure not read yet on the card."},
  {"s":"pause off · %","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the one-account ruler label while the pause is off (round 14's)."},
  {"s":"quota-btn quota-btn-quiet quota-lane-remove","c":"NOT-RENDERED","why":"A class list for the lane's Remove button, never displayed."},
  {"s":"removed from this Mac. Sign in again to add it back.","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): follows the removed account's label."},
  {"s":"In use now: <b></b>.","c":"FRAGMENT","why":"Feedback item 8 (the CEO 2026-10-06, richos-hq docs/ceo-input/2026-10-06_01/feedback.md), in round 18's words (richos-hq design/mockups/rounds/round-18/): the line above the lanes opens with the account in use now. Its control, Use this one now on every other account with room, is in the same lanes; tests/quota.js item 8 renders it in both themes with contrast computed."},
  {"s":"Next: <b></b>, whose week is fresh again soonest.","c":"FRAGMENT","why":"Feedback item 8: the line's second sentence, the account Rich switches to next (claude_accounts.rs next_account, plan §15.1)."},
  {"s":"No other account has room right now.","c":"FRAGMENT","why":"Feedback item 8: the line's second sentence when no other account has room; nothing to do, each comes back at its reset, which its lane shows."},
  {"s":"Rich now uses .","c":"FRAGMENT","why":"Feedback item 8, round 18's confirmation after Use this one now, said in passing; the sentence that follows says what happens next. Asks nothing."},
  {"s":"has already used % of its week, so Rich will switch again when it reaches 99%.","c":"FRAGMENT","why":"Feedback item 8, round 18's confirmation when the picked account is at 90% of its week or more (the CEO's own test, the 97% account): says when the automatic switch follows."},
  {"s":"Use this one now","c":"CONTROL","why":"Feedback item 8, round 18's words: the lane button that puts that account in use now (claude_account_use_first). Shown on every other account with a reading and room."},
  {"s":"quota-btn quota-btn-quiet quota-lane-first","c":"NOT-RENDERED","why":"A class list for the lane's Use this one now button (feedback item 8), never displayed."},
  {"s":"% weekly used","c":"FRAGMENT","why":"The CEO 2026-10-06 (richos-hq docs/ceo-input/2026-10-06_02/feedback.md): the Settings row's quota summary with one account now names the WEEKLY window, the one the 99% switch watches; follows the rounded figure. tests/quota.js renders it."},
  {"s":"stays idle until week reaches 99%.","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the Pause card naming when the next account takes over."},
  {"s":"switch to <b></b>, the next account","c":"CONTROL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the Switch verb inside the one sentence; tests/quota.js chooses it."},
  {"s":"switch to the next account <span class=\"quota-muted\">(none has room now)</span>","c":"CONTROL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the Switch verb when no account has room."},
  {"s":"switches at %","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the weekly row's switch point with two or more accounts."},
  {"s":"used up, usable again","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): the hero heading's note for a used-up account."},
  {"s":"when it reached a usage limit","c":"FRAGMENT","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): why the last switch happened."},
  {"s": "Account 1 signs in from the main sign-in.", "c": "UNREACHABLE", "why": "claude_account_sign_in refuses Account 1 by id. The quota panel draws Sign in only on added accounts (quota.js renderAccounts skips id 1), so no control sends it."},
  {"s": "Finish the sign-in in your browser.", "c": "ACTIONABLE", "control": "#quota-refresh", "fixture":null, "why": "Sign in on an added account's row starts the stock browser sign-in; Refresh reads it once it is done. Fill-first (plan richos-hq 2026-10-04 §15): the Claude accounts section of the quota panel. quota.js renders it in both themes with contrast computed."},
  {"s": "· usable again", "c": "FRAGMENT", "why": "Appended to an account row's figures while the account is past its check point or refused by a limit, before the time it can be used again. The whole line is INFORMATIONAL: it states figures and a time and asks nothing."},
  {"s": "Usage limit reached", "c": "INFORMATIONAL", "why": "An account row with no figures that a usage limit refused, followed by the time it can be used again. Fill-first (plan richos-hq 2026-10-04 §15): the Claude accounts section of the quota panel. quota.js renders it in both themes with contrast computed."},
  {"s": "No allowance reported.", "c": "INFORMATIONAL", "why": "An added account that was read but reported no window. States the fact only. Fill-first (plan richos-hq 2026-10-04 §15): the Claude accounts section of the quota panel. quota.js renders it in both themes with contrast computed."},
  {"s": "Signed in. Reading its allowance…", "c": "INFORMATIONAL", "why": "The added account's sign-in ended connected; the panel re-reads it at once. Fill-first (plan richos-hq 2026-10-04 §15): the Claude accounts section of the quota panel. quota.js renders it in both themes with contrast computed."},
  {"s": "That change could not be made. Nothing changed.", "c": "INFORMATIONAL", "why": "Add, Remove or the Pause/Switch setting failed without a sentence of its own. States the outcome; the controls stay in the same panel. Fill-first (plan richos-hq 2026-10-04 §15): the Claude accounts section of the quota panel. quota.js renders it in both themes with contrast computed."},
  {"s":"Allow Rich to use one “” reset automatically when weekly usage reaches 99%? This approval is only for this offer, expires with it and can be revoked before use. Using a reset cannot be undone.","c":"ACTIONABLE","control":"#quota-reset-confirm","fixture":null,"why":"The reset confirmation offers Approve one automatic reset and Keep unapproved together. quota.js proves no approval is saved before confirmation."},
  {"s":"Allow Rich to use this reset…","c":"CONTROL","why":"The label names an interactive control in the quota panel or its Settings entry; quota.js exercises the corresponding flow."},
  {"s":"Anthropic did not use the reset. The approval has ended.","c":"INFORMATIONAL","why":"Reports the result of an already completed reset decision. No additional reset is implied or requested."},
  {"s":"Anthropic reports this reset was already used.","c":"INFORMATIONAL","why":"Reports the result of an already completed reset decision. No additional reset is implied or requested."},
  {"s":"Anthropic requires an exhausted limit for this offer; it may not be usable at 99%.","c":"INFORMATIONAL","why":"Reports the scope or availability supplied by the provider. This is not a request to change the five-hour pause policy or create a reset offer."},
  {"s":"Approve in advance. Rich will use one reset automatically at 99% weekly usage while the desktop app or terminal watcher is running. They share one approval and attempt record. Checks every 5 minutes. Five-hour usage never triggers it.","c":"ACTIONABLE","control":"#quota-usage-open","fixture":null,"why":"A usable offer has its preparation button, then a separate confirmation. When no eligible offer remains, Open Claude Usage still opens the account view. quota.js checks the eligible and unavailable states."},
  {"s":"Approve one automatic reset","c":"CONTROL","why":"The label names an interactive control in the quota panel or its Settings entry; quota.js exercises the corresponding flow."},
  {"s":"Approved and ready · one use at 99% weekly usage. Approval expires with this offer.","c":"ACTIONABLE","control":"#quota-reset-revoke","fixture":null,"why":"The saved one-use approval can be revoked here, including while eligibility is unavailable. quota.js walks approval, reopen and revocation."},
  {"s":"Approved in advance · waiting for a fresh eligibility check.","c":"ACTIONABLE","control":"#quota-reset-revoke","fixture":null,"why":"The saved one-use approval can be revoked here, including while eligibility is unavailable. quota.js walks approval, reopen and revocation."},
  {"s":"Approved in advance. Rich is ready to use one reset at 99% weekly usage.","c":"ACTIONABLE","control":"#quota-reset-revoke","fixture":null,"why":"The saved one-use approval can be revoked here, including while eligibility is unavailable. quota.js walks approval, reopen and revocation."},
  {"s":"Can continue just after when the reset is under 20 minutes away.","c":"ACTIONABLE","control":"#quota-release","fixture":null,"why":"These observed agents can be released through Let them continue now, which explicitly turns pause off. The dark and light quota.js hold fixtures exercise that control."},
  {"s":"Claude Code quota","c":"CONTROL","why":"The label names an interactive control in the quota panel or its Settings entry; quota.js exercises the corresponding flow."},
  {"s":"Claude usage-limit reset","c":"INFORMATIONAL","why":"Reports the scope or availability supplied by the provider. This is not a request to change the five-hour pause policy or create a reset offer."},
  {"s":"Could not change reset approval. Refresh and try again.","c":"ACTIONABLE","control":"#quota-usage-open","fixture":null,"why":"The quota panel exposes Refresh and a direct Open Claude Usage control. The account page remains reachable during a refresh backoff; quota.js proves the opener is a read-only navigation request."},
  {"s":"Could not open Claude Usage. Open claude.ai/new#settings/usage in your browser.","c":"ACTIONABLE","control":"#quota-usage-open","fixture":null,"why":"The quota panel exposes Refresh and a direct Open Claude Usage control. The account page remains reachable during a refresh backoff; quota.js proves the opener is a read-only navigation request."},
  {"s":"Could not read Claude Code quota. Try refreshing again.","c":"ACTIONABLE","control":"#quota-refresh","fixture":null,"why":"Refresh is in the same reading line; Open Claude Usage appears only inside the weekly-reset section, which round 16 does not draw (quota.js renderResets)."},
  {"s":"Could not save. Your previous setting is still active.","c":"ACTIONABLE","control":"#quota-threshold","fixture":null,"why":"The current policy remains editable in the threshold field; Save retries it and Keep discards the draft. quota.js covers invalid values, saving and cancellation."},
  {"s":"Let them continue now","c":"CONTROL","why":"The label names an interactive control in the quota panel or its Settings entry; quota.js exercises the corresponding flow."},
  {"s":"Continuing now turns automatic pause off.","c":"INFORMATIONAL","why":"Describes automatic progress or the effect of a control whose label appears beside it. It asks for no additional user action."},
  {"s":"Live pause details are unavailable.","c":"INFORMATIONAL","why":"Describes automatic progress or the effect of a control whose label appears beside it. It asks for no additional user action."},
  {"s":"No reset offer is available.","c":"INFORMATIONAL","why":"Reports the scope or availability supplied by the provider. This is not a request to change the five-hour pause policy or create a reset offer."},
  {"s":"Pick a whole number from 1 to 99. It is still % until you save.","c":"ACTIONABLE","control":"#quota-threshold","fixture":null,"why":"The current policy remains editable in the threshold field; Save retries it and Keep discards the draft. quota.js covers invalid values, saving and cancellation."},
  {"s":"Ready to pause","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Technical Settings offers the matching pause switch alongside this state. quota.js changes the switch and checks the resulting state."},
  {"s":"Releasing the pause…","c":"INFORMATIONAL","why":"Describes automatic progress or the effect of a control whose label appears beside it. It asks for no additional user action."},
  {"s":"Reset approval revoked.","c":"INFORMATIONAL","why":"Reports the result of an already completed reset decision. No additional reset is implied or requested."},
  {"s":"Reset outcome unavailable. Check Claude’s Usage page.","c":"ACTIONABLE","control":"#quota-usage-open","fixture":null,"why":"The quota panel exposes Refresh and a direct Open Claude Usage control. The account page remains reachable during a refresh backoff; quota.js proves the opener is a read-only navigation request."},
  {"s":"Reset time unavailable","c":"INFORMATIONAL","why":"Reports the scope or availability supplied by the provider. This is not a request to change the five-hour pause policy or create a reset offer."},
  {"s":"Revoke reset approval","c":"CONTROL","why":"The label names an interactive control in the quota panel or its Settings entry; quota.js exercises the corresponding flow."},
  {"s":"Some files have not reached your Mac yet. They are still on your phone and will be sent again.","c":"NOT-RENDERED","why":"An authenticated phone API delivery response. It is displayed by the phone client, not the desktop webview; retained files retry through the phone delivery path."},
  {"s":"The allowance permits work. Waiting for each pause to clear.","c":"INFORMATIONAL","why":"Describes automatic progress or the effect of a control whose label appears beside it. It asks for no additional user action."},
  {"s":"The five-hour allowance has reached %. Agents will pause when they finish their current step. No pauses observed yet.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"Technical Settings offers the matching pause switch alongside this state. quota.js changes the switch and checks the resulting state."},
  {"s":"The reset outcome is unconfirmed. Automatic retry is blocked. Check Claude’s Usage page before taking further action.","c":"ACTIONABLE","control":"#quota-usage-open","fixture":null,"why":"The quota panel exposes Refresh and a direct Open Claude Usage control. The account page remains reachable during a refresh backoff; quota.js proves the opener is a read-only navigation request."},
  {"s":"These waits have cleared. Work can continue from its saved place.","c":"INFORMATIONAL","why":"Describes automatic progress or the effect of a control whose label appears beside it. It asks for no additional user action."},
  {"s":"This offer does not reset weekly quota.","c":"INFORMATIONAL","why":"Reports the scope or availability supplied by the provider. This is not a request to change the five-hour pause policy or create a reset offer."},
  {"s":"Waiting for a current five-hour reading. Their work is saved.","c":"ACTIONABLE","control":"#quota-release","fixture":null,"why":"These observed agents can be released through Let them continue now, which explicitly turns pause off. The dark and light quota.js hold fixtures exercise that control."},
  {"s":"Waiting for the allowance to refresh. I'll continue automatically when it's available.","c":"INFORMATIONAL","why":"The conversation reports an automatic quota hold. It does not ask a regular user to manage quota; the quota controls are intentionally confined to Technical Settings."},
  {"s":"Waiting for the allowance to refresh. Rich will continue automatically when it's available.","c":"INFORMATIONAL","why":"The conversation reports an automatic quota hold. It does not ask a regular user to manage quota; the quota controls are intentionally confined to Technical Settings."},
  {"s":"Waiting to start an agent","c":"ACTIONABLE","control":"#quota-release","fixture":null,"why":"These observed agents can be released through Let them continue now, which explicitly turns pause off. The dark and light quota.js hold fixtures exercise that control."},
  {"s":"Weekly quota reset","c":"INFORMATIONAL","why":"Reports the scope or availability supplied by the provider. This is not a request to change the five-hour pause policy or create a reset offer."},
  {"s":"Weekly reset availability is unknown. Refresh to check again.","c":"ACTIONABLE","control":"#quota-refresh","fixture":null,"why":"Refresh is in the same reading line; Open Claude Usage appears only inside the weekly-reset section, which round 16 does not draw (quota.js renderResets)."},
  {"s":"<b>ended ago</b>, the new one has no reading yet","c":"INFORMATIONAL","why":"Round 16 (richos-hq design/mockups/rounds/round-16/, the CEO's chosen design): \"This window ended 4 min ago — the new one has no reading yet\": what the provider's last reading says about a window that has reset; the next reading replaces it by itself."},
  {"s":"Your Mac could not save the delivery receipt. Your files are still on your phone.","c":"NOT-RENDERED","why":"An authenticated phone API delivery response. It is displayed by the phone client, not the desktop webview; retained files retry through the phone delivery path."},
  {"s":"Your Mac could not save this file just now. It is still on your phone and will be sent again.","c":"NOT-RENDERED","why":"An authenticated phone API delivery response. It is displayed by the phone client, not the desktop webview; retained files retry through the phone delivery path."},
  {"s":"Your approved reset was used. Checking the new allowance…","c":"INFORMATIONAL","why":"Reports the result of an already completed reset decision. No additional reset is implied or requested."},
  {"s":"for the quota","c":"FRAGMENT","why":"A label or interpolated piece of the quota reading, hold status or affected-limit list. It is not an independent instruction."},
  {"s":"last known usage","c":"FRAGMENT","why":"A label or interpolated piece of the quota reading, hold status or affected-limit list. It is not an independent instruction."},
  {"s":"quota MCP requires a scope, data directory and Claude executable","c":"NOT-RENDERED","why":"A diagnostic for a malformed internal reset MCP process invocation. It is written to stderr before the desktop UI starts, not returned as panel prose."},
  {"s":"unknown native push transport {transport:?}","c":"NOT-RENDERED","why":"An internal native push transport diagnostic. PhoneError translates this detail before sending a user-facing API response."},
  {"s":"waiting to continue","c":"FRAGMENT","why":"A label or interpolated piece of the quota reading, hold status or affected-limit list. It is not an independent instruction."},
  {"s":"weekly allowance including overage","c":"FRAGMENT","why":"A label or interpolated piece of the quota reading, hold status or affected-limit list. It is not an independent instruction."},
  {"s":"· can resume just after","c":"FRAGMENT","why":"A label or interpolated piece of the quota reading, hold status or affected-limit list. It is not an independent instruction."},
  {"s":"Weekly usage reached 99%. Agents will pause at their next step until allowance is confirmed.","c":"ACTIONABLE","control":"#quota-enabled","fixture":null,"why":"The shared automatic-pause switch is present in this weekly state. quota.js checks the weekly fixture and the switch lifecycle."},
  {"s":"Weekly usage reached 99%. Waiting for a confirmed reset and available allowance.","c":"ACTIONABLE","control":"#quota-release","fixture":null,"why":"The weekly hold offers the same explicit release control as a five-hour hold. quota.js checks that this control releases the weekly hold and disables automatic pause."},

  // ---- speckle.js — THE DESIGN SYSTEM'S SPECKLED GROUND (CEO, 2026-09-29) ---------------
  // `ui/speckle.js` is copied byte for byte from richos-hq design/system (speckle-app.js says
  // why), so its wording is not edited here. Every one of these is the text of an `Error`
  // thrown by `RichOSSpeckle.preset()` or `.background()` (speckle.js:336-355) to the code that
  // called it, or a piece of one: the CEO's rule, quoted into the refusal, and the two names of
  // the surfaces it forbids. The app's one caller, `speckle-app.js` `mount()`, catches any
  // throw into `RichSpeckle.state.error`, which nothing draws, and the shell carries on without
  // the canvas. `tests/speckled-ground.js` check 7 reads the refusal off the thrown error. None
  // of them reaches the DOM.
  {"s":"The splash screens on desktop and mobile apps as well as the desktop app home screen should never have that speckled background design.","c":"NOT-RENDERED","why":"speckle.js `RULE`, the CEO's rule of 2026-09-29, quoted into the Error the engine throws when asked for the splash or home surface. Thrown to the caller; speckle-app.js keeps it in RichSpeckle.state.error, which nothing renders."},
  {"s":"the splash screen (desktop or mobile)","c":"NOT-RENDERED","why":"speckle.js `NEVER.splash`, the name of a forbidden surface interpolated into the engine's thrown refusal. Never drawn."},
  {"s":"the desktop app home screen","c":"NOT-RENDERED","why":"speckle.js `NEVER.home`, the name of a forbidden surface interpolated into the engine's thrown refusal. Never drawn."},
  {"s":"RichOSSpeckle: no speckle preset exists for","c":"NOT-RENDERED","why":"The head of the Error preset() throws for a forbidden surface. A developer-facing refusal to the calling code; speckle-app.js catches it into RichSpeckle.state.error, which nothing renders."},
  {"s":"RichOSSpeckle: unknown surface \"","c":"NOT-RENDERED","why":"The head of the Error preset() throws for a surface name it does not know. Developer-facing, caught by speckle-app.js, never rendered."},
  {"s":"\"; the surfaces are \"desktop\" and \"mobile\".","c":"NOT-RENDERED","why":"The tail of the same unknown-surface Error. Developer-facing, caught by speckle-app.js, never rendered."},
  {"s":"RichOSSpeckle: unknown theme \"","c":"NOT-RENDERED","why":"The head of the Error preset() throws for a theme other than dark or light. Developer-facing, caught by speckle-app.js, never rendered."},
  {"s":"\"; the themes are \"dark\" and \"light\".","c":"NOT-RENDERED","why":"The tail of the same unknown-theme Error. Developer-facing, caught by speckle-app.js, never rendered."},
  {"s":"RichOSSpeckle.background: refused for","c":"NOT-RENDERED","why":"The head of the Error background() throws for the splash or home surface, which the CEO's rule forbids. speckled-ground.js check 7 reads it off the throw; speckle-app.js never asks for either surface, and a throw would land in RichSpeckle.state.error, which nothing renders."},
  {"s":"RichOSSpeckle.background: `surface` must be \"desktop\" or \"mobile\" (got","c":"NOT-RENDERED","why":"The head of the Error background() throws for a missing or unknown surface. speckle-app.js always passes \"desktop\"; developer-facing, never rendered."},
  {"s": "I don't have that file in this thread's output.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). An action refused because the id is no longer in the active thread's record (the thread changed under it). It states what the shell has; there is nothing to do but choose from the list the panel shows. `tests/output.js` (S6 block) drives the commands through the mock's contract. Add to chat (S7) renders it too: `output_attach` returns it through `mac_attachments.rs` and `attachments.js` shows it in `#attach-note` as '<file name>: <sentence>', as every desk refusal is."},
  {"s": "This file is a link to somewhere else, so I won't open it from here. Show in Finder still works.", "c": "ACTIONABLE", "control": "#outpanel [data-act=\"menu\"]", "fixture": null, "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Also the tooltip of every action disabled on a link or a swapped file (§6.7). The thing it says still works is in the same view: the row's and the file view's More menu (`[data-act=\"menu\"]`), whose Show in Finder stays lit. `tests/output.js` 'disabled actions on a missing file and a link' renders both. S5's viewer renders the same sentence for the preview, and Add to chat (S7) in `#attach-note` as '<file name>: <sentence>' (`output_attach` runs `output_files::check` before the desk)."},
  {"s": "This is a folder or a special file, not a regular file, so I won't open it from here. Show in Finder still works.", "c": "ACTIONABLE", "control": "#outpanel [data-act=\"menu\"]", "fixture": null, "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). The link sentence's sibling for a folder, socket or FIFO at the recorded path; the same disabled-action tooltip and the same lit Show in Finder in the More menu beside it. S5's viewer renders the same sentence for the preview (`NOT_A_FILE`), and Add to chat (S7) in `#attach-note` as '<file name>: <sentence>'."},
  {"s": "This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here.", "c": "INFORMATIONAL", "why": "Reclassified by the Output panel (S4, `output-panel.js`), which renders it in a missing file's own view (and `output_files.rs` returns it to the commands). It states where the file is not and what will happen if Rich writes it again; where he keeps his files is his, outside the app, and nothing in this view is a control for it. `tests/output.js` renders it."},
  {"s": "I can't read this thread's output record right now.", "c": "ACTIONABLE", "control": "#op-body .op-again", "fixture": null, "why": "Reclassified by the Output panel (S4, `output-panel.js`): `list_output` refuses with it when the thread's record cannot be read, and the panel renders it with `Try again` beside it, which reads the record again; the buttons show no count meanwhile. `tests/output.js` renders it and recovers through the control."},
  {"s": "This Mac would not open it.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). `/usr/bin/open` refused Open, Open with or Show in Finder (§6.7: the shell's sentence in the notice, nothing else changes). It states the outcome and asks nothing."},
  {"s": "The apps that open this file changed since the list was shown. Choose one again.", "c": "ACTIONABLE", "control": "#op-menu .op-menu-item", "fixture": null, "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Open with... asked for an app by its place in a list the Mac no longer gives. The panel shows the list again, freshly read, at the control he used (`openWith` in output-panel.js), so the choice it asks for is on screen as it is said. `tests/output.js` 'open-menu' drives it and chooses again."},
  {"s": "That app is not one this Mac offers for this file. Choose one again.", "c": "ACTIONABLE", "control": "#op-menu .op-menu-item", "fixture": null, "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Open with... asked for a place the shown list does not have; handled as the changed-list sentence: the list is shown again at once, so the choice is on screen."},
  {"s": "Finder opens {} with {} selected.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). What Show in Finder did (§6.7). `tests/output.js` asserts it verbatim through the mock, and the S3 probe on the VM through the shell."},
  // ---- THE OUTPUT PANEL'S ACTIONS (Output side panel PRD §5.4, §6.7, slice S6; `output-panel.js`
  // "THE ACTIONS" and `output_files.rs` `output_save_copy`). `tests/output.js` renders each. -----
  {"s": "Nothing was saved.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Save a copy...: the save sheet was dismissed with Cancel (`NOTHING_SAVED`). The action says what it did; nothing to do."},
  {"s": "That is the file itself, so nothing was copied.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Save a copy...: the sheet pointed at the recorded file itself, by path or link (`SAME_FILE`); refused before a byte is written. States the outcome; Save a copy... is still in the same menu."},
  {"s": "I couldn't save the copy there: that is not a place a file can be saved.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Save a copy...: the sheet answered something with no folder or not absolute (`NOT_A_PLACE`). The system sheet never does; said rather than assumed."},
  {"s": "I couldn't save the copy.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Save a copy...: the blocking task behind the sheet stopped before it answered (`WOULD_NOT_SAVE`). States the outcome."},
  {"s": "Copied the path.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Copy path / Copy the full path: the recorded path went to the clipboard (§6.7's own sentence). `tests/output.js` asserts it and the clipboard's contents."},
  {"s": "I couldn't copy the path.", "c": "INFORMATIONAL", "why": "Rendered by the Output panel's actions (slice S6, `output-panel.js` \"THE ACTIONS\"), which call the `output_*` commands and show the shell's own sentence in the panel's notice (`#op-notice`, role=status). Copy path: the webview's clipboard refused the write. States the outcome; the same control is still beside it."},
  {"s": "Copy the full path", "c": "CONTROL", "why": "S6: the accessible name and tooltip of the copy button at the end of the file view's path line (`.of-path [data-act=\"copy\"]`, §6.4)."},
  {"s": "More actions for", "c": "FRAGMENT", "why": "S6: `More actions for <file name>`, the accessible name of a row's hover ... button (`.orow [data-act=\"menu\"]`). A CONTROL as a whole."},
  {"s": "Other ways to open", "c": "CONTROL", "why": "S6: the accessible name and tooltip of the file view's chevron beside the gold Open (`[data-act=\"open-menu\"]`, §6.4)."},
  {"s": "Save a copy…", "c": "CONTROL", "why": "S6: the menu item (rows, the file view's ... and its chevron) that shows the system save sheet through `output_save_copy`."},
  {"s": "Show in Finder", "c": "CONTROL", "why": "S6: the menu item that calls `output_reveal`."},
  // ---- CLAUDE ACCOUNTS FOR EVERYONE (round 18, the CEO's feedback item 9 of 2026-10-06; `accounts.js`). -----
  {"s": "% of this week used", "c": "FRAGMENT", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the Settings row's line with one account from 85% of the week: \"86% of this week used\", in gold. INFORMATIONAL as a whole: the row itself opens the sheet. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<b></b> is ready. Rich switches to it when is nearly full.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the sheet's line once an added account's sign-in comes back as a different account (\"Work is ready. Rich switches to it when Home is nearly full.\"). States what happened and what Rich will do. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<b></b> is signed in.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): an added account's Sign in finished. States the outcome. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<b>Used up for this week.</b>", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): a card's weekly meter at 99% or more, followed by when it is fresh again. Rich switches by himself; nothing to do. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<button class=\"acc-btn\" type=\"button\" data-act=\"use\" data-id=\"\" aria-label=\"Use now\">Use this one now</button>", "c": "CONTROL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): Use this one now on a card with room (feedback item 8, drawn in round 18). tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<button class=\"acc-slot\" type=\"button\" data-act=\"add-open\"><span class=\"acc-slot-plus\"></span><span><span class=\"acc-slot-title\">Add another Claude account</span><span class=\"acc-slot-body\">Have a second Claude account, for example one for work? Add it here and Rich switches to it when this one is nearly full.</span></span></button>", "c": "CONTROL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the empty place a second card would go, with one account: the button that starts adding one. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-add\" id=\"acc-add\"> <p class=\"acc-add-body\">Give a name only you see, so you can tell them apart.</p> <div class=\"acc-fields\"> <label class=\"acc-field\">The new account<input id=\"acc-nu\" type=\"text\" maxlength=\"24\" placeholder=\"For example: Work\" value=\"\" autocomplete=\"off\"></label></div> <div class=\"acc-add-acts\"><button class=\"acc-btn acc-btn-primary\" type=\"button\" id=\"acc-continue\" data-act=\"add-next\">Continue</button><button class=\"acc-btn\" type=\"button\" data-act=\"add-cancel\">Cancel</button></div></div>", "c": "ACTIONABLE", "control": "#acc-nu, #acc-continue", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): adding, step 1 of 2: the names, with the fields and Continue beside them. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-add\" id=\"acc-add\"> <p class=\"acc-add-body\">Sign in with your <b></b> account in the browser window that just opened, then come back here.</p> <div class=\"acc-waiting\" role=\"status\"><span class=\"acc-ring\" aria-hidden=\"true\"></span><p>Waiting for you to finish signing in…</p></div> <div class=\"acc-add-acts\"><button class=\"acc-btn\" type=\"button\" data-act=\"add-again\">Open the sign-in page again</button><button class=\"acc-btn\" type=\"button\" data-act=\"add-cancel\">Cancel</button></div></div>", "c": "ACTIONABLE", "control": "#acc-add [data-act=\"add-again\"]", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): adding, step 2 of 2, while Claude's sign-in page is open in the browser: Open the sign-in page again and Cancel are beside it. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-add\" id=\"acc-add\"> <p class=\"acc-add-body\">You signed in as <b></b> again. To add , sign in with your other Claude account.</p> <p class=\"acc-add-body is-soft\">In your browser, sign out of Claude first, then press Try again.</p> <div class=\"acc-add-acts\"><button class=\"acc-btn acc-btn-primary\" type=\"button\" id=\"acc-try-again\" data-act=\"add-again\">Try again</button><button class=\"acc-btn\" type=\"button\" data-act=\"add-cancel\">Cancel</button></div></div>", "c": "ACTIONABLE", "control": "#acc-try-again", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the same Claude account signed in again (main.rs claude_account_sign_in_poll, provider_auth::same_account): Try again is beside it. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-add\" id=\"acc-add\"> <p class=\"acc-add-body\">Your browser opens Claude's own sign-in page. Sign in with your <b></b> account, not the one you already use, then come back here.</p> <p class=\"acc-add-body is-soft\">RichOS never sees your password.</p> <div class=\"acc-add-acts\"><button class=\"acc-btn acc-btn-primary\" type=\"button\" id=\"acc-open-signin\" data-act=\"add-go\">Open sign-in</button><button class=\"acc-btn\" type=\"button\" data-act=\"\"></button></div></div>", "c": "ACTIONABLE", "control": "#acc-open-signin", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): adding, step 2 of 2: Open sign-in is beside it. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-banner\" role=\"status\"><span class=\"acc-banner-icon\"></span><div class=\"acc-banner-main\"> <p class=\"acc-banner-head\"></p><p class=\"acc-banner-body\">.</p> <div class=\"acc-banner-acts\"><button class=\"acc-btn acc-btn-primary\" type=\"button\" data-act=\"add-open\">Add another account</button></div></div></div>", "c": "ACTIONABLE", "control": ".acc-banner [data-act=\"add-open\"]", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): every account close to its weekly limit: the banner says what happens when they run out, with Add another account beside it. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-banner\" role=\"status\"><span class=\"acc-banner-icon\"></span><div class=\"acc-banner-main\"> <p class=\"acc-banner-head\">Rich switched to at </p> <p class=\"acc-banner-body\"></p> <div class=\"acc-banner-acts\"><button class=\"acc-btn\" type=\"button\" data-act=\"banner-ok\" data-at=\"\">Got it</button></div></div></div>", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): why it switched, the sheet's banner: when and why, with Got it to put it away. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-confirm\"><span>Remove <b></b> from RichOS? RichOS signs out of it here. Nothing changes on the account itself and you can add it back any time.</span> <button class=\"acc-btn acc-btn-primary\" type=\"button\" id=\"acc-remove-yes\" data-act=\"remove-yes\" data-id=\"\">Remove </button><button class=\"acc-btn\" type=\"button\" id=\"acc-remove-no\" data-act=\"remove-no\">Keep it</button></div>", "c": "ACTIONABLE", "control": "#acc-remove-yes, #acc-remove-no", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): Remove's one question, with Remove <name> and Keep it beside it. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-meter\" data-k=\"\"><span class=\"acc-meter-label\"></span><span class=\"acc-meter-bar\" role=\"img\" aria-label=\": not reported yet\"><b style=\"width:0\"></b></span> <span class=\"acc-meter-num\"></span><span class=\"acc-meter-when\">Not reported yet</span></div>", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): a card's meter for a window the reading does not carry: Not reported yet. Nothing guessed. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-more\"><button class=\"acc-btn acc-btn-quiet\" type=\"button\" data-act=\"add-open\">Add another account</button></div>", "c": "CONTROL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): Add another account under the cards, with two or more. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-note-acts\"><button class=\"acc-btn acc-btn-primary\" type=\"button\" data-note-act=\"add\">Add a second account</button><button class=\"acc-btn\" type=\"button\" data-note-act=\"not-now\">Not now</button></div>", "c": "CONTROL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the buttons under Rich's one-time suggestion of a second account (quota.rs note_accounts): Add a second account and Not now. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-note-acts\"><button class=\"acc-btn acc-btn-primary\" type=\"button\" data-note-act=\"add\">Add another account</button><button class=\"acc-btn acc-btn-quiet\" type=\"button\" data-note-act=\"see\">See your accounts</button></div>", "c": "CONTROL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the buttons under Rich's line that every account is close to its week: Add another account and See your accounts. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"acc-note-acts\"><button class=\"acc-btn acc-btn-quiet\" type=\"button\" data-note-act=\"see\">See your accounts</button></div>", "c": "CONTROL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the button under Rich's switch line (claude_accounts.rs switch_to): See your accounts. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<h3 class=\"acc-how-title\">How Rich uses your accounts</h3> <p class=\"acc-how-p\">Rich uses one account at a time. When its weekly limit is almost used up, he <b>switches to the next one by himself</b>. Nothing stops and nothing is lost.</p> <p class=\"acc-how-p is-soft\">Next is always the account whose week is fresh again soonest, so none of your weekly limit goes to waste. To use a different account first, press Use this one now on it.</p> <div class=\"acc-how-sec\"><p class=\"acc-how-h\" id=\"acc-five-h\">When a 5-hour limit is almost used up</p> <div class=\"acc-choice\" role=\"radiogroup\" aria-labelledby=\"acc-five-h\"> <button class=\"acc-opt\" type=\"button\" role=\"radio\" id=\"acc-five-pause\" data-act=\"five\" data-v=\"pause\" aria-checked=\"\"><span class=\"acc-radio\"></span><span><span class=\"acc-opt-t\">Pause the team until it is fresh again</span><span class=\"acc-opt-s\">The default. Never longer than 5 hours.</span></span></button> <button class=\"acc-opt\" type=\"button\" role=\"radio\" id=\"acc-five-switch\" data-act=\"five\" data-v=\"switch\" aria-checked=\"\"><span class=\"acc-radio\"></span><span><span class=\"acc-opt-t\">Switch to </span><span class=\"acc-opt-s\">The team keeps going and uses up the next account's week sooner.</span></span></button> </div></div> <div class=\"acc-how-sec\"><p class=\"acc-how-h\">Recent changes</p> </div>", "c": "ACTIONABLE", "control": "#acc-five-pause, #acc-five-switch", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the right column with two or more: how Rich uses them, the 5-hour choice (its two options are the control) and Recent changes. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<h3 class=\"acc-how-title\">When your account fills up</h3> <p class=\"acc-how-p\">When it is nearly used up, Rich <b>pauses the team</b> until it is fresh again. Nothing is lost: everyone picks up right where they left off.</p> <p class=\"acc-how-p is-soft\">Your own conversation with Rich never pauses.</p> <div class=\"acc-promo\"><p class=\"acc-how-h\">Keep the team going with a second account</p> <p class=\"acc-how-p\">If you have more than one Claude account, add them all. Rich uses one at a time and switches to the next when it is nearly full, so the team does not have to wait.</p> </div>", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the right column with one account: what happens when it fills up, and the offer of a second with its own button. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<p class=\"acc-after\"> is added. I will switch to it when is nearly full.</p>", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): under Rich's suggestion once a second account is added: what he will do. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<p class=\"acc-after\">Okay. You can add one any time in Settings, under Claude accounts.</p>", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): Rich's answer to Not now: where the feature lives. Nothing is asked. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<section class=\"overlay-panel acc-panel\"> <header class=\"acc-heading\"><div><p class=\"acc-eyebrow\">Settings</p><h2 id=\"acc-title\">Claude accounts</h2><p id=\"acc-lede\" class=\"acc-lede\"></p></div> <button id=\"acc-close\" type=\"button\" aria-label=\"Close Claude accounts\"><svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\" aria-hidden=\"true\"><path d=\"M6 6l12 12M18 6L6 18\"/></svg></button></header> <div class=\"acc-body\"><div id=\"acc-col\" class=\"acc-col\"></div><div id=\"acc-how\" class=\"acc-how\"></div></div> <p id=\"acc-who-status\" class=\"sr-only\" role=\"status\"></p></section>", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the sheet's scaffold: Settings, Claude accounts, and Close. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<span class=\"acc-pill is-resting\"><i></i>Not signed in</span>", "c": "ACTIONABLE", "control": "#accounts-sheet [data-act=\"sign\"], #accounts-sheet [data-act=\"sign-one\"]", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): a card for an account with no reading: Sign in is on the card. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "All accounts are close to their weekly limit", "c": "FRAGMENT", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): \"All 3 accounts are close to their weekly limit\", the banner's head with three or more. INFORMATIONAL as a whole; the banner carries Add another account. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Almost used up.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): a card's weekly meter from 90% to 99%, in gold, followed by when it is fresh again. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Both accounts are close to their weekly limit", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the banner's head with two accounts both at 90% or more of the week; the banner carries Add another account. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Finish signing in in your browser, then come back here.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): a card's line while its Sign in runs in the browser Claude's sign-in page opened; it says where the step is and that the card comes back by itself. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Give the new account a name, for example Work.", "c": "ACTIONABLE", "control": "#acc-nu", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): step 1's check: the field it asks for is beside it. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "It is fresh again .", "c": "FRAGMENT", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): \"It is fresh again Thu 9:00 AM.\" inside the banner's why. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Its week is fresh again soonest, so it goes next.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the Next card's line: why it is next (plan answer 1). tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "This account is not signed in on this Mac, so Rich cannot answer you yet. Sign in here.", "c": "ACTIONABLE", "control": "#accounts-sheet [data-act=\"sign-one\"]", "fixture": "accounts-signed-out", "why": "A new user's first run (2026-10-21): the card said \"In use now\" and \"No reading yet\" on a Mac where nobody had signed in. quota.rs `View::signed_in` carries Claude Code's own `auth status` verdict; `accounts.js` draws it. docs/development/failure-classes/unknown-state-shown-as-pending.md. The card in use, signed out: Sign in on the card opens the app's own sign-in."},
  {"s": "Not signed in on this Mac", "c": "ACTIONABLE", "control": "#set-accounts-open", "fixture": "accounts-row-signed-out", "why": "A new user's first run (2026-10-21): the card said \"In use now\" and \"No reading yet\" on a Mac where nobody had signed in. quota.rs `View::signed_in` carries Claude Code's own `auth status` verdict; `accounts.js` draws it. docs/development/failure-classes/unknown-state-shown-as-pending.md. The Settings row's line, in gold: the row opens the sheet whose card has Sign in."},
  {"s": "Checking whether this account is signed in on this Mac.", "c": "INFORMATIONAL", "fixture": "accounts-checking", "why": "A new user's first run (2026-10-21): the card said \"In use now\" and \"No reading yet\" on a Mac where nobody had signed in. quota.rs `View::signed_in` carries Claude Code's own `auth status` verdict; `accounts.js` draws it. docs/development/failure-classes/unknown-state-shown-as-pending.md. The card in use before the first check has answered; nothing to do, and it is replaced by the verdict when the check returns."},
  {"s": "Not used in the last 5 hours", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): a card's 5-hour meter at 0%. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Rich and the team are working on this one.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the In use now card's line with two or more accounts (with the agents counted when any are). tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Rich and the team do their work on your Claude account. Claude limits how much each account can do in 5 hours and in a week.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the sheet's lede with one account. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Rich and the team work on one Claude account at a time. When it is nearly full, Rich switches to the next one by himself, so the work keeps going.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the sheet's lede with two or more. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Rich and the team work on this account.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the one account's card line. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<span class=\"acc-pill is-inuse\"><i></i>Still in use</span>", "c": "INFORMATIONAL", "why": "Account-switch fix (CEO 2026-10-19, v1.2.1, `accounts.js` movingNote and the leaving/arriving pills): the pill on the account being left while work still runs on it; a status, and the Use this one now control stays on the other cards."},
  {"s": "<span class=\"acc-pill is-next\"><i></i>Switching to this one</span>", "c": "INFORMATIONAL", "why": "Account-switch fix (CEO 2026-10-19, v1.2.1, `accounts.js` movingNote and the leaving/arriving pills): the pill on the account being moved to; a status that resolves by itself when the work finishes, nothing to press."},
  {"s": "Rich moves here with his next reply.", "c": "INFORMATIONAL", "why": "Account-switch fix (CEO 2026-10-19, v1.2.1, `accounts.js` movingNote and the leaving/arriving pills): the in-use card's note: Rich is still on another account and moves here on his own. Nothing for the CEO to do."},
  {"s": "Rich moves to with his next reply.", "c": "FRAGMENT", "why": "Account-switch fix (CEO 2026-10-19, v1.2.1, `accounts.js` movingNote and the leaving/arriving pills): `Rich moves to ${to} with his next reply.`; the account label is filled in at run time."},
  {"s": "The team is finishing the step it is on here, then moves to .", "c": "FRAGMENT", "why": "Account-switch fix (CEO 2026-10-19, v1.2.1, `accounts.js` movingNote and the leaving/arriving pills): `The team is finishing the step it is on here, then moves to ${to}.`; the account label is filled in at run time."},
  {"s": "The team moves here once it finishes the step it is on.", "c": "INFORMATIONAL", "why": "Account-switch fix (CEO 2026-10-19, v1.2.1, `accounts.js` movingNote and the leaving/arriving pills): the in-use card's note while the team is still on another account; it moves by itself, nothing to press."},
  {"s": "the next account", "c": "FRAGMENT", "why": "Account-switch fix (CEO 2026-10-19, v1.2.1, `accounts.js` movingNote and the leaving/arriving pills): the fallback label for `${to}` when no account is in use; joins the two interpolated sentences above."},
  {"s": "working on this one now.", "c": "FRAGMENT", "why": "Account-switch fix (CEO 2026-10-19, v1.2.1, `accounts.js` movingNote and the leaving/arriving pills): tail of the subject plus working on this one now; the subject (Rich is, The team is, Rich and the team are) is chosen at run time."},
  {"s": "Rich can use it again once its week is fresh.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): a Resting card's line: its week is used up and Rich switches back by himself. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Rich now uses <b></b>.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the confirmation after Use this one now, with Undo beside it, and which account is next or when Rich switches again. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Rich switched to . reached % of its .", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): why it switched, Recent changes: the automatic switch and its reason (claude_accounts.rs Change). tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Rich switched to . reached a Claude usage limit.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): Recent changes for a switch after a usage-limit refusal. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Sign in to this account and Rich can use it.", "c": "ACTIONABLE", "control": "#accounts-sheet [data-act=\"sign\"]", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): an added account with no reading: Sign in is on the card. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "That is the account you already use as <b></b>. Sign in with your other Claude account.", "c": "ACTIONABLE", "control": "#accounts-sheet [data-act=\"sign\"]", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): an added account's Sign in came back as an account already in use; the card's Sign in is beside it. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "That name is taken. Pick a different one.", "c": "ACTIONABLE", "control": "#acc-nu", "fixture": null, "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): step 1's check: the field is beside it. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Using , switched", "c": "FRAGMENT", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): \"Using Work, switched 11:01 AM\", the Settings row's line for six hours after a switch. INFORMATIONAL as a whole. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "This account is signed in on this Mac. Claude Code does not report usage limits for it, so there are none to show.", "c": "INFORMATIONAL", "why": "quota.rs `ReadError::SignedInNoLimits` (Claude Code reported no limits and its own `auth status` says this account is signed in), drawn by `accounts.js` on a card with no reading whose `signedIn` is true, in use or not, and by `quota.js` on such an account's lane in Technical view (reviews rv-20261021T213648Z-b3f0990a-85f9, rv-20261021T234558Z-10df0b1f-03b0 finding 2 and rv-20261022T014956Z-56662dcd-dcc3 finding 1). Nothing to do: the account is signed in and has no limits to show. tests/accounts.js renders it in use and not in use, and measures the card not in use in both themes with contrast computed; tests/quota.js renders the lane."},
  {"s": "Waiting its turn.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): a Ready card's line. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "When run out, the team pauses until is fresh again . Nothing is lost.", "c": "FRAGMENT", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the both-near banner's second sentence. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Why: had used % of its . Nothing stopped and the team carried on.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): why it switched, the banner's body for a weekly or 5-hour switch. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Why: reached a Claude usage limit, so the step it was on ran again on .", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the banner's body after a usage-limit refusal. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "You removed .", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): Recent changes: an account he removed. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Your Claude account", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the one account's card name until it is named. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "is removed from RichOS. You can add it back any time.", "c": "INFORMATIONAL", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): the line after Remove. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "the new account", "c": "FRAGMENT", "why": "Round 18 (richos-hq design/mockups/rounds/round-18/, the CEO's feedback item 9 of 2026-10-06, `accounts.js`): step 2's title and body before a name is typed. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "<span class=\"acc-who-label\">Signed in as</span><span class=\"acc-who-addr\"></span>", "c": "INFORMATIONAL", "why": "Round 31, item 18 (richos-hq design/mockups/rounds/round-31/item-18-which-account.html; the CEO 2026-10-18, built on his yes of 2026-10-19), `accounts.js`: an account card's line after Show email: the email that account's own Claude Code sign-in names, for 8 seconds beside the ring and Hide. It states who the account is and asks nothing. tests/accounts.js renders it in both themes with contrast computed."},
  {"s": "Claude Code did not say which email uses.", "c": "FRAGMENT", "why": "Round 31, item 18 (richos-hq design/mockups/rounds/round-31/item-18-which-account.html; the CEO 2026-10-18, built on his yes of 2026-10-19), `accounts.js`: the screen-reader announcement composed with the account's name, said only into `#acc-who-status` (sr-only) after Show email when the answer names no address: `Claude Code did not say which email <name> uses.` The visible line is the card's own sentence. It states a fact and asks nothing."},
  {"s": "That account is no longer on this Mac.", "c": "NOT-RENDERED", "why": "Round 31, item 18 (richos-hq design/mockups/rounds/round-31/item-18-which-account.html; the CEO 2026-10-18, built on his yes of 2026-10-19), `accounts.js`: main.rs `claude_account_email` refusing an id no account has. accounts.js never shows this text: a refused read is drawn as the card's could-not-read line, so this never reaches the DOM."},
  {"s": "is not signed in on this Mac.", "c": "FRAGMENT", "why": "Round 31, item 18 (richos-hq design/mockups/rounds/round-31/item-18-which-account.html; the CEO 2026-10-18, built on his yes of 2026-10-19), `accounts.js`: the tail of the screen-reader announcement `<name> is not signed in on this Mac.` in `#acc-who-status` after Show email on a signed-out account. It states a fact; the card beside it carries Sign in where one applies."},
  {"s": "is signed in as . It hides again in seconds.", "c": "FRAGMENT", "why": "Round 31, item 18 (richos-hq design/mockups/rounds/round-31/item-18-which-account.html; the CEO 2026-10-18, built on his yes of 2026-10-19), `accounts.js`: the screen-reader announcement `<name> is signed in as <email>. It hides again in 8 seconds.` in `#acc-who-status` after Show email. It states who the account is and asks nothing."},
  {"s": "{} RichOS will try again in {when}, in the same conversation, so nothing is lost. This is retry {} of {MAX_OVERLOAD_RETRIES}.", "c": "ACTIONABLE", "control": "#stop", "fixture": null, "why": "`UpstreamFault::Overloaded` while RichOS waits to retry on its own schedule (1, 2, 5, 10, 20, 40, 80 minutes). The wait is his to end: Stop ends it and nothing is sent again (core test `his_stop_during_the_wait_ends_it_and_nothing_is_sent_again`); #stop is on screen while the conversation is working."},

  // Dictation slice 1: the tool's and its launcher's operator lines. None reaches the DOM.
  {"s": "--parent needs a process id", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `tool::parse` refusal, printed by `tool::main` with `eprintln!(\"[richos-dictation] {why}\")` (tool.rs:117) and exit 2. Its reader is whoever started the tool: the app passes both arguments itself (dictation_app.rs)."},
  {"s": "unknown argument {other}", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `tool::parse` refusal, printed by `tool::main` with `eprintln!` (tool.rs:117) and exit 2; the app always passes exactly `--data-dir` and `--parent`."},
  {"s": "--data-dir needs a folder", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command, so nothing it says reaches this webview. `tool::parse` refusal (tool.rs:162) of a `--data-dir` with no folder after it, printed by `tool::main` with `eprintln!` and exit 2; the app passes its own absolute data folder, and a launchd start (slice 5) passes none and computes the installed one."},
  {"s": "--data-dir must be absolute", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `tool::parse` refusal, printed by `tool::main` with `eprintln!` (tool.rs:117) and exit 2; the app passes its own absolute data folder."},
  {"s": "the dictation key could not be claimed: {e}", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `ipc::claim` failed; `tool::main` writes this to dictation.log (`log::line`, tool.rs:143) and exits 2."},
  {"s": "the dictation tool could not start its event loop: {e}", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. The tool's own window-less Tauri builder failed; written to dictation.log (`log::line`, tool.rs:220), exit 2."},
  {"s": "the microphone could not be opened: {e}", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. The tool's capture failed; written to dictation.log (`log::line`, tool.rs:375). The session records `Problem::NoMicrophone`, whose tag (not this sentence) goes over the socket; slice 1 draws nothing for it."},
  {"s": "dictation: model {id}, {secs:.2} s of audio, decoded in {latency} ms: nothing meaningful, {}", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. A dictation.log line (`log::line`, tool.rs:450): the transcript was judged empty. It never carries the words."},
  {"s": "a dictation's scratch folder could not be removed: {error}", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `Scratch`'s drop (scratch.rs:42) writes this to dictation.log with `log::line`."},
  {"s": "macOS refused the key tap (Accessibility is not allowed)", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `KeyTap::start`'s Err (keytap.rs:198); `Tool::ensure_tap` writes it to dictation.log (`log::line`, tool.rs:289)."},
  {"s": "the key tap thread ended before it started", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `KeyTap::start`'s Err (keytap.rs:224); `Tool::ensure_tap` writes it to dictation.log (`log::line`, tool.rs:289)."},
  {"s": "no general pasteboard", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `insert::insert` / `insert::copy_only` Err (insert.rs:451, :482); `tool::write` writes it to dictation.log (`log::line`, tool.rs:456 and :460) and returns a `Problem` tag."},
  {"s": "the words could not be written to the clipboard", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `insert::insert` / `insert::copy_only` Err (insert.rs:457, :461, :483); `tool::write` writes it to dictation.log (`log::line`, tool.rs:456 and :460) and returns a `Problem` tag."},
  {"s": "{} is {why}; refusing to use it", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. `ipc::prepare_dir` refusal of the runtime folder (ipc.rs:68), reached only through `ipc::claim`, so it is the `{e}` of the dictation.log line at tool.rs:143."},
  {"s": "not a directory", "c": "NOT-RENDERED", "why": "Dictation slice 1 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool is the same executable started with `--richos-dictation` (dictation/tool.rs), a separate process whose Tauri builder has no window, no plugin and no command; slice 1 has no screens (dictation/mod.rs), so nothing it says reaches this webview. The `{why}` of `ipc::prepare_dir`'s refusal (ipc.rs:70); see the row above: a dictation.log line through tool.rs:143."},
  // ---- DICTATION, THE SHEET'S COMMANDS (dictation plan slice 2; `dictation_app.rs`). -----
  {"s": "F{key} is not a key RichScribe can use", "c": "NOT-RENDERED", "why": "Dictation plan slice 2 (`dictation_app.rs`): an Err a Dictation sheet command returns for a call the window never makes (a value outside its own controls) or a macOS refusal. `dictation.js` `call()` logs it to the console and draws nothing; the sheet keeps showing the state it read."},
  {"s": "System Settings did not open ({status})", "c": "NOT-RENDERED", "why": "Dictation plan slice 2 (`dictation_app.rs`): an Err a Dictation sheet command returns for a call the window never makes (a value outside its own controls) or a macOS refusal. `dictation.js` `call()` logs it to the console and draws nothing; the sheet keeps showing the state it read."},
  {"s": "RichScribe has not started in this window yet", "c": "NOT-RENDERED", "why": "Dictation plan slice 2 (`dictation_app.rs`): an Err a Dictation sheet command returns for a call the window never makes (a value outside its own controls) or a macOS refusal. `dictation.js` `call()` logs it to the console and draws nothing; the sheet keeps showing the state it read."},
  {"s": "macOS could not be asked for the microphone", "c": "NOT-RENDERED", "why": "Dictation plan slice 2 (`dictation_app.rs`): an Err a Dictation sheet command returns for a call the window never makes (a value outside its own controls) or a macOS refusal. `dictation.js` `call()` logs it to the console and draws nothing; the sheet keeps showing the state it read."},
  {"s": "{accuracy} is not an accuracy RichScribe offers", "c": "NOT-RENDERED", "why": "Dictation plan slice 2 (`dictation_app.rs`): an Err a Dictation sheet command returns for a call the window never makes (a value outside its own controls) or a macOS refusal. `dictation.js` `call()` logs it to the console and draws nothing; the sheet keeps showing the state it read."},
  {"s": "{pane} is not a pane RichScribe opens", "c": "NOT-RENDERED", "why": "Dictation plan slice 2 (`dictation_app.rs`): an Err a Dictation sheet command returns for a call the window never makes (a value outside its own controls) or a macOS refusal. `dictation.js` `call()` logs it to the console and draws nothing; the sheet keeps showing the state it read."},
  // ---- DICTATION: THE SETTINGS ROW AND THE SHEET (round 19 and its more lines; dictation plan slice 2; `dictation.js`). -----
  {"s": "<div class=\"dict-card\"><span class=\"dict-spine\"></span> <div class=\"dict-card-main\"><h3 class=\"dict-card-title\">Dictate in any app</h3><p class=\"dict-card-sub\" id=\"dict-state\"></p></div> <button class=\"dict-switch\" id=\"dict-switch\" type=\"button\" role=\"switch\" data-act=\"toggle-on\" aria-checked=\"\" aria-label=\"Dictate in any app\"></button></div>", "c": "FRAGMENT", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`): the switch's card. Its line (#dict-state) is classified on its own, row by row below; the switch (#dict-switch) is the control."},
  {"s": "<div class=\"dict-key-row\"><p class=\"dict-key-say\"> Pick one you rarely use.</p><button class=\"dict-btn dict-btn-quiet\" id=\"dict-capture\" type=\"button\" data-act=\"capture\">Press a different key</button></div>", "c": "CONTROL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 6: the line under the key strip and Press a different key (#dict-capture). tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<div class=\"dict-key-wait\"><span class=\"dict-key\" aria-hidden=\"true\">?</span><p class=\"dict-key-say\"><b>Press the key you want to use.</b> Esc to cancel.</p><button class=\"dict-btn\" id=\"dict-capture-cancel\" type=\"button\" data-act=\"capture-cancel\">Cancel</button></div>", "c": "ACTIONABLE", "control": "#dict-capture-cancel", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 11: key capture waits for the key he presses on his keyboard; Cancel is beside it and Esc cancels. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<p class=\"dict-copy-note\" id=\"dict-copy-note\"><span><b>Works only while RichOS is open.</b> When you quit RichOS, RichScribe stops until you open it again.</span></p>", "c": "INFORMATIONAL", "why": "Round 19's more lines (richos-hq design/mockups/rounds/round-19/dictation-more-lines.html, the CEO's wording of 2026-10-08, dictation plan slice 2, `dictation.js`), line 2: every copy until the login start (slice 5) is built; says only what he sees. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<p class=\"dict-feedback\" id=\"dict-feedback\" role=\"status\"><span>RichScribe is on. Try it in the box on the right or in any app.</span></p>", "c": "ACTIONABLE", "control": "#dict-try", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 10: both allowed; Try it here is beside it, the cursor already in it. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<p class=\"dict-key-say is-soft\">Any function key works, F1 to F19.</p>", "c": "INFORMATIONAL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 11: under the waiting key. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<section class=\"dict-sec\"><div class=\"dict-sec-h\"><h4 class=\"dict-sec-t\" id=\"dict-acc-h\">Accuracy</h4></div> <div class=\"dict-choice\" role=\"radiogroup\" aria-labelledby=\"dict-acc-h\"> <button class=\"dict-opt\" type=\"button\" role=\"radio\" data-act=\"accuracy\" data-v=\"accurate\" aria-checked=\"\"><span class=\"dict-radio\"></span><span><span class=\"dict-opt-t\">More accurate</span><span class=\"dict-opt-s\">Gets names and jargon right more often. Your words appear a second or two after you stop.</span></span></button> <button class=\"dict-opt\" type=\"button\" role=\"radio\" data-act=\"accuracy\" data-v=\"fast\" aria-checked=\"\"><span class=\"dict-radio\"></span><span><span class=\"dict-opt-t\">Faster</span><span class=\"dict-opt-s\">Your words appear in about half a second. Names are misheard a little more often.</span></span></button> </div></section>", "c": "CONTROL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 6: Accuracy, the two choices (More accurate, Faster) and Saved. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<section class=\"dict-sec\"><div class=\"dict-sec-h\"><h4 class=\"dict-sec-t\">What macOS asks you for</h4><span class=\"dict-sec-note\">Once, when you turn it on.</span></div> <div class=\"dict-perm\" data-perm=\"microphone\"><span class=\"dict-perm-ic\"></span><span><span class=\"dict-perm-t\">Microphone</span><span class=\"dict-perm-s\">So I can hear you, only while you dictate.</span></span><span class=\"dict-perm-state\"></span></div> <div class=\"dict-perm\" data-perm=\"accessibility\"><span class=\"dict-perm-ic\"></span><span><span class=\"dict-perm-t\">Accessibility</span><span class=\"dict-perm-s\">So I can type your words where your cursor is, in any app.</span></span><span class=\"dict-perm-state\"></span></div></section>", "c": "FRAGMENT", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 6: the two permission rows; each row's state is classified on its own below. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<section class=\"dict-sec\"><div class=\"dict-sec-h\"><h4 class=\"dict-sec-t\">Your key</h4><span class=\"dict-sec-note\">Tap once to start, once more to stop. Nothing to hold down.</span></div>", "c": "FRAGMENT", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 6: the section's heading, completed by the key strip or the waiting key. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<section class=\"overlay-panel dict-panel\"> <header class=\"dict-heading\"><div><p class=\"dict-eyebrow\">Settings</p><h2 id=\"dict-title\">RichScribe</h2> <p class=\"dict-lede\">Type with your voice in any app on your Mac. Your words appear where your cursor is.</p></div> <button id=\"dict-close\" type=\"button\" aria-label=\"Close RichScribe\"></button></header> <div class=\"dict-body\"><div id=\"dict-col\" class=\"dict-col\"></div> <div class=\"dict-how\"> <h3 class=\"dict-how-title\">Try it here</h3> <div id=\"dict-try\" class=\"dict-try\" contenteditable=\"true\" spellcheck=\"false\" role=\"textbox\" aria-multiline=\"true\" aria-label=\"Try RichScribe here\" data-ph=\"\"></div> <p id=\"dict-try-note\" class=\"dict-how-p is-soft\" hidden></p> <div class=\"dict-how-sec\"> <p class=\"dict-how-h\">While you talk</p> <p class=\"dict-how-p is-soft\">A small bar at the bottom of your screen shows I'm listening. It never takes your cursor away.</p> <div class=\"dict-samples\" aria-hidden=\"true\"> <div class=\"dict-bar is-listen\" id=\"dict-sample-listen\"><span class=\"dict-orb\"></span><span class=\"dict-meter\"><i></i><i></i><i></i><i></i><i></i><i></i><i></i></span><span class=\"dict-label\">Listening</span><span class=\"dict-time\">0:03</span></div> <div class=\"dict-bar is-write\"><span class=\"dict-orb\"></span><span class=\"dict-label\">Writing it down&hellip;</span></div> <div class=\"dict-bar is-done\"><span class=\"dict-orb\"></span><span class=\"dict-label\">Added</span></div> </div> </div> <div class=\"dict-how-sec\"> <p class=\"dict-how-h\">Not the same as talking to me</p> <p class=\"dict-how-p is-soft\">The round button beside my message box sends what you say to me. RichScribe only types. You read it, fix it if you like and send it yourself.</p> </div> <div class=\"dict-how-sec dict-priv\"><p class=\"dict-how-p\"><b>Your voice stays on this Mac.</b> Nothing you say is sent anywhere.</p></div> </div></div></section>", "c": "FRAGMENT", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`): the sheet's frame (title, Try it here, the three bar samples, the difference from talking to Rich, privacy). Every changing line in it is classified on its own; #dict-close closes it. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<span class=\"dict-perm-no\">Not allowed</span><button class=\"dict-btn\" type=\"button\" data-act=\"open-sys\" data-pane=\"\">Open System Settings</button>", "c": "ACTIONABLE", "control": "#dictation-sheet [data-act=\"open-sys\"]", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 12: a permission refused, with Open System Settings beside it on the right pane. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<span class=\"dict-perm-wait\"><span class=\"dict-ring\" aria-hidden=\"true\"></span>macOS is asking you</span>", "c": "INFORMATIONAL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), states 7 to 9: macOS's own prompt or System Settings is up; the sheet says so and waits. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<span class=\"dict-perm-wait\">Asked when you turn it on</span>", "c": "INFORMATIONAL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 6: nothing has been asked yet. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "<span class=\"dict-who\"></span> is hiding your keys from other apps, so can't reach me.", "c": "FRAGMENT", "why": "Round 19's more lines (richos-hq design/mockups/rounds/round-19/dictation-more-lines.html, the CEO's wording of 2026-10-08, dictation plan slice 2, `dictation.js`), line 4 named: completed by the app macOS names and the key, then the fix sentence. INFORMATIONAL as a whole: the fix is in the other app. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Allow both permissions on the left to try it here.", "c": "ACTIONABLE", "control": "#dict-switch", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`): Try it here's placeholder while dictation is on and a permission is not allowed; the permission rows on the left carry Open System Settings when refused, and macOS's own prompt answers otherwise; the switch is always beside it. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Another app is hiding your keys, so can't reach me.", "c": "FRAGMENT", "why": "Round 19's more lines (richos-hq design/mockups/rounds/round-19/dictation-more-lines.html, the CEO's wording of 2026-10-08, dictation plan slice 2, `dictation.js`), line 4 unnamed: completed by the key, then the fix sentence. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Click here, tap and say something.", "c": "FRAGMENT", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 10: Try it here's placeholder, completed by the key's name. ACTIONABLE as a whole: the box (#dict-try) is the control. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Command, Option, Control and Shift can't be used: anything you type while one is down would set off shortcuts. Use a function key, F1 to F19.", "c": "ACTIONABLE", "control": "#dict-capture-cancel", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 11: a modifier refused while the key capture waits; he presses another key, or Cancel beside it. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "RichScribe is off. Turn it on in Settings, under RichScribe.", "c": "ACTIONABLE", "control": "#set-btn", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`): the off notice when his key is pressed in RichOS's window with dictation off; Settings is always on screen. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "RichScribe now starts and stops with F.", "c": "FRAGMENT", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 11: the notice after a key is chosen, completed by its number. INFORMATIONAL as a whole. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Needs a permission", "c": "ACTIONABLE", "control": "#set-dictation-open", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 5: the Settings row's line; the row itself opens the sheet that asks. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Not working yet: macOS has not allowed the microphone.", "c": "ACTIONABLE", "control": "#dictation-sheet [data-act=\"open-sys\"][data-pane=\"microphone\"]", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 12 for the microphone: Open System Settings is in the microphone's row. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Not working yet: macOS has not let me type into other apps.", "c": "ACTIONABLE", "control": "#dictation-sheet [data-act=\"open-sys\"][data-pane=\"accessibility\"]", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 12: Open System Settings is in the Accessibility row. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Off. Turn it on to type with your voice in Mail, Slack, your browser, anywhere.", "c": "ACTIONABLE", "control": "#dict-switch", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 6: the switch is beside it. tests/dictation.js renders it in both themes with contrast computed."},
  // The Settings row's "Starting" (dictation.js `rowLine`, while no tool holds the key yet) is one
  // word, under the scraper's twelve-character floor (lib/state-strings.js), so it carries no row;
  // the sheet's own line for that moment is "Starting dictation\u2026" no longer: it says nothing until
  // a tool holds the key (recheck finding 6).
  {"s": "Not working yet: the key is not being watched. Turn the switch off and on again.", "c": "ACTIONABLE", "control": "#dict-switch", "fixture": null, "why": "Recheck finding 6 (2026-10-08, `dictation.js`): the sheet never says On without a tool that holds the key and watches it. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Not working yet", "c": "ACTIONABLE", "control": "#set-dictation-open", "fixture": null, "why": "Recheck finding 6 (2026-10-08, `dictation.js` `rowLine`): the Settings row's line, with attention, while a tool holds the key but its key tap is not made; the row itself (#set-dictation-open) opens the sheet, whose line names the fix (Turn the switch off and on again) beside the switch. tests/dictation.js renders it with attention in both themes."},
  {"s": "You can try it here once RichScribe has started.", "c": "INFORMATIONAL", "why": "The try box's placeholder while no tool holds the key yet (recheck finding 6, 2026-10-08, `dictation.js`). tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "On. Tap in any app, talk and tap it again.", "c": "FRAGMENT", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 10: the switch's line, completed by the key's name. INFORMATIONAL as a whole. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "On. Tap to talk", "c": "FRAGMENT", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 5: the Settings row's line, completed by the key's name. INFORMATIONAL as a whole. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Paused: an app is hiding your keys", "c": "INFORMATIONAL", "why": "Round 19's more lines (richos-hq design/mockups/rounds/round-19/dictation-more-lines.html, the CEO's wording of 2026-10-08, dictation plan slice 2, `dictation.js`), line 4 unnamed in the Settings row; the fix is in the other app, and the row opens the sheet that says it. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Paused: is hiding your keys", "c": "FRAGMENT", "why": "Round 19's more lines (richos-hq design/mockups/rounds/round-19/dictation-more-lines.html, the CEO's wording of 2026-10-08, dictation plan slice 2, `dictation.js`), line 4 named in the Settings row, completed by the app's name. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Quitting fixes it.", "c": "FRAGMENT", "why": "Round 19's more lines (richos-hq design/mockups/rounds/round-19/dictation-more-lines.html, the CEO's wording of 2026-10-08, dictation plan slice 2, `dictation.js`), line 4's fix, completed by the app's name. INFORMATIONAL as a whole: the fix is in the other app. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Quitting the app where you last typed a password usually fixes it.", "c": "INFORMATIONAL", "why": "Round 19's more lines (richos-hq design/mockups/rounds/round-19/dictation-more-lines.html, the CEO's wording of 2026-10-08, dictation plan slice 2, `dictation.js`), line 4's fix with no app name; the fix is in another app, so there is no control in this view. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "That key types text. Use a function key, F1 to F19.", "c": "ACTIONABLE", "control": "#dict-capture-cancel", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 11: a typing key refused while the key capture waits; he presses another key, or Cancel beside it. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "This box is only for trying. Your words go wherever your cursor is.", "c": "INFORMATIONAL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 10: under Try it here. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Turn RichScribe on to try it here.", "c": "ACTIONABLE", "control": "#dict-switch", "fixture": null, "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 6: Try it here's placeholder; the switch is in the same sheet. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Waiting for macOS", "c": "INFORMATIONAL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 5: the Settings row's line while macOS's prompt or System Settings is up. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Waiting for you to allow Accessibility&hellip;", "c": "INFORMATIONAL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), states 8 and 9: the sheet waits for macOS's prompt or System Settings, reading it once a second. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "Waiting for you to allow the microphone&hellip;", "c": "INFORMATIONAL", "why": "Round 19 (richos-hq design/mockups/rounds/round-19/dictation.html, dictation plan slice 2, `dictation.js`), state 7: macOS's microphone prompt is up. tests/dictation.js renders it in both themes with contrast computed."},
  {"s": "You can try it here once your keys are back.", "c": "INFORMATIONAL", "why": "Round 19's more lines (richos-hq design/mockups/rounds/round-19/dictation-more-lines.html, the CEO's wording of 2026-10-08, dictation plan slice 2, `dictation.js`), line 4: Try it here's placeholder while another app hides keys. tests/dictation.js renders it in both themes with contrast computed."},

  // Dictation slice 3: the bar, the menu bar menu, and the tool's new operator lines.
  {"s": "</b> is hiding your keys from other apps, so", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Iris's line 4 (secure-bar) after the app's name."},
  {"s": "</span><span class=\"dh-label\">Listening</span><span class=\"dh-time\" id=\"dhTime\">0:00</span><span class=\"dh-hint\">Tap", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Composite markup for state 14: Listening, the time, and 'Tap F1 to finish' around a key cap; the key that finishes is the dictation key itself."},
  {"s": "<button class=\"sb-item\" type=\"button\" role=\"", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Composite markup for one menu row; each row's label is classified on its own."},
  {"s": "<div class=\"dh is-problem\" data-problem=\"", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Composite markup that wraps every problem line; each line is classified on its own."},
  {"s": "<div class=\"dh is-write\"><span class=\"dh-orb\"></span><span class=\"dh-label\">Writing it down…</span></div>", "c": "INFORMATIONAL", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. State 15, while whisper-cli runs; nothing to do."},
  {"s": "<div class=\"dov-notice\">RichScribe is off.</div>", "c": "INFORMATIONAL", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Round 19's notice after Turn dictation off in the menu bar menu; the tool then ends."},
  {"s": "Another app is hiding your keys, so", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Iris's line 4 with no app name (secure-bar-anon), before its key cap."},
  {"s": "I can't hear anything. Check that your microphone is on.", "c": "ACTIONABLE", "control": "the microphone itself (hardware or the Mac's Sound input), then the dictation key, named on the bar in a key cap (`#dhud .dov-kcap`): tapping it starts a new dictation", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Round 19 PROBLEMS.nosound. The fix is outside RichOS (a muted or unplugged microphone); the bar names it and the key tries again."},
  {"s": "I can't type into other apps yet, so I copied your words.", "c": "ACTIONABLE", "control": "#fixIt", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Round 19 PROBLEMS.ax; its Fix it (`#fixIt`) opens RichOS on the Dictation sheet."},
  {"s": "I didn't catch anything. Tap", "c": "ACTIONABLE", "control": "the dictation key, named on the bar in a key cap (`#dhud .dov-kcap`): tapping it starts a new dictation", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Round 19 PROBLEMS.short, word for word; tests/dictation-overlay.js renders it with contrast computed in both themes."},
  {"s": "I need the microphone to hear you.", "c": "ACTIONABLE", "control": "#fixIt", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Round 19 PROBLEMS.mic; its Fix it (`#fixIt`) opens RichOS on the Dictation sheet (tool.rs `UiEvent::FixIt`, come-forward)."},
  {"s": "I'm still downloading the voice AI. Try again in a few minutes.", "c": "ACTIONABLE", "control": "the dictation key, named on the bar in a key cap (`#dhud .dov-kcap`): tapping it starts a new dictation", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Iris's line 5a (round 19 dictation-more-lines.html, desk-nomodel), shown at the tap when neither speech model is on the Mac; the download runs by itself and the key tries again."},
  {"s": "No text box was selected, so I copied your words. Press", "c": "ACTIONABLE", "control": "Command-V, named on the bar in a key cap (`#dhud .dov-kcap`): the words are on the clipboard", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Round 19 PROBLEMS.nofield (state 18)."},
  {"s": "Sorry, I couldn't write that down. Tap", "c": "ACTIONABLE", "control": "the dictation key, named on the bar in a key cap (`#dhud .dov-kcap`): tapping it starts a new dictation", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Iris's line 5b (desk-nowrite): whisper-cli failed or passed its bound."},
  {"s": "Turn RichScribe off", "c": "CONTROL", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Round 19 state 17's row; it IS the control."},
  {"s": "Turn RichScribe on", "c": "CONTROL", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Round 19 state 17's row when off; it IS the control."},
  {"s": "Turn it on to type with your voice.", "c": "ACTIONABLE", "control": ".sb-item[data-act=onoff]", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Round 19 state 17's line when dictation is off; Turn dictation on is the row beneath it."},
  {"s": "and say it again.", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. Line 5b after its key cap."},
  {"s": "can't reach me. Quitting", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Iris's line 4 after its key cap, before the app's name."},
  {"s": "can't reach me. Quitting the app where you last typed a password usually fixes it.", "c": "ACTIONABLE", "control": "the named app's own Quit, outside RichOS (Secure Event Input belongs to that app); Dictation settings… (`.sb-item[data-act=settings]`) sits in the same menu", "fixture": null, "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Iris's line 4 with no app name, word for word."},
  {"s": "dictation could not be turned off: {why}", "c": "NOT-RENDERED", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool (dictation/tool.rs, bar.rs, appkit.rs, menubar.rs) is a separate process; this is a dictation.log line (`log::line`), never shown to anyone. It names what failed so the log says why a window, the menu bar item or a menu choice did nothing."},
  {"s": "the accuracy could not be saved: {why}", "c": "NOT-RENDERED", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool (dictation/tool.rs, bar.rs, appkit.rs, menubar.rs) is a separate process; this is a dictation.log line (`log::line`), never shown to anyone. It names what failed so the log says why a window, the menu bar item or a menu choice did nothing."},
  {"s": "the bar could not be told what to show: {e}", "c": "NOT-RENDERED", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool (dictation/tool.rs, bar.rs, appkit.rs, menubar.rs) is a separate process; this is a dictation.log line (`log::line`), never shown to anyone. It names what failed so the log says why a window, the menu bar item or a menu choice did nothing."},
  {"s": "the menu bar item could not change its picture: {e}", "c": "NOT-RENDERED", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool (dictation/tool.rs, bar.rs, appkit.rs, menubar.rs) is a separate process; this is a dictation.log line (`log::line`), never shown to anyone. It names what failed so the log says why a window, the menu bar item or a menu choice did nothing."},
  {"s": "the menu could not be drawn: {e}", "c": "NOT-RENDERED", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool (dictation/tool.rs, bar.rs, appkit.rs, menubar.rs) is a separate process; this is a dictation.log line (`log::line`), never shown to anyone. It names what failed so the log says why a window, the menu bar item or a menu choice did nothing."},
  {"s": "the panel class could not be made", "c": "NOT-RENDERED", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool (dictation/tool.rs, bar.rs, appkit.rs, menubar.rs) is a separate process; this is a dictation.log line (`log::line`), never shown to anyone. It names what failed so the log says why a window, the menu bar item or a menu choice did nothing."},
  {"s": "the {label} window could not be reached: {e}", "c": "NOT-RENDERED", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool (dictation/tool.rs, bar.rs, appkit.rs, menubar.rs) is a separate process; this is a dictation.log line (`log::line`), never shown to anyone. It names what failed so the log says why a window, the menu bar item or a menu choice did nothing."},
  {"s": "to paste them.", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's bar (ui/dictation-bar.js), a window of the tool's own over whatever app is in front, never this webview's DOM. The no-text-box line after its Command-V key cap."},
  {"s": "to start and again to stop.", "c": "FRAGMENT", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool's menu bar menu (ui/dictation-menu.js), a window of the tool's own under its menu bar item. Round 19 state 17's line after 'Tap' and its key cap."},
  {"s": "{} window stays a plain window: {why}", "c": "NOT-RENDERED", "why": "Dictation slice 3 (richos-hq docs/plans/2026-10-08-dictation-anywhere.md rev 2): the dictation tool (dictation/tool.rs, bar.rs, appkit.rs, menubar.rs) is a separate process; this is a dictation.log line (`log::line`), never shown to anyone. It names what failed so the log says why a window, the menu bar item or a menu choice did nothing."},

  // Dictation slice 5 and the reviews (the ways in ship; the tool outlives the app; voice mode
  // and dictation share one microphone): the bar's one new line, voice mode's refusal, and the
  // tool's new log lines.
  {"s": "Voice mode is still listening, so I didn't start. Tap", "c": "ACTIONABLE", "control": "the dictation key, named on the bar in a key cap (`#dhud .dov-kcap`): tapping it starts a new dictation", "fixture": null, "why": "Dictation plan section 2 (one microphone; review finding 2, `dictation-bar.js` PROBLEMS['voice-still-listening']): the dictation tool's bar, a window of the tool's own over whatever app is in front, never this webview's DOM. His key was tapped while voice mode held the microphone, so no dictation started; the key tries again once voice mode is off, which the ◉ button in RichOS's window ends. tests/dictation-overlay.js renders the bar's problem lines with contrast computed in both themes."},
  {"s": "again in a moment.", "c": "FRAGMENT", "why": "Dictation plan section 2 (`dictation-bar.js` PROBLEMS['voice-still-listening']): the voice-still-listening line after its key cap."},
  {"s": "RichScribe is still listening; tap its key to finish, then try again", "c": "ACTIONABLE", "control": "#talk-toggle", "fixture": "shell", "why": "Dictation plan section 2 (review finding 2, `start_voice_capture`, main.rs): his dictation was told to finish and did not within 3 s (`dictation_app::Link::finish`), so voice mode did not open beside it rather than send his dictated words to Rich. `enterVoiceMode` (main.js) renders the Err through `richVoiceSays` in the conversation, in text mode. The key is named on the dictation bar's key cap, still on screen because the dictation is still listening; ◉ (#talk-toggle) is the retry, beside the composer."},
  {"s": "another copy's change could not be saved: {why}", "c": "NOT-RENDERED", "why": "Dictation slice 5 (recheck finding 2, dictation/tool.rs:822): the dictation tool is a separate process; a dictation.log line (`log::line`) when another copy of RichOS's `change` could not be written to dictation.json. The settings the tool holds stay as they were; the copy that asked already logged its own refusal."},
  {"s": "menu bar item event: {} at {at}", "c": "NOT-RENDERED", "why": "Dictation slice 5 (fourth review, finding 2, dictation/menubar.rs:47): a dictation.log line (`log::line`) for every press, release or enter of the menu bar item, with where the item was, so a click that opened nothing says whether anything reached the item. Never shown to anyone."},
  {"s": "the activation watch class could not be made", "c": "NOT-RENDERED", "why": "Dictation slice 5 (dictation/appkit.rs:329): `watch_activation`'s Err when the Objective-C class that hands activations back cannot be registered; the tool's `setup` writes it to dictation.log (`log::line`, tool.rs:491) and goes on without the watch. Never shown to anyone."},
  {"s": "the engine at {} has no verified runtime ({e}); PATH and Homebrew are tried at the first dictation", "c": "NOT-RENDERED", "why": "Dictation slice 5 (a launchd start, dictation/tool.rs:189): `resolve_runtime_bin_alone`'s answer for a tool no app started, written to dictation.log at boot (tool.rs:328) so which whisper-cli it will use is never a guess. Never shown to anyone; a dictation that then finds no whisper-cli shows the bar's own could-not-write line."},
  {"s": "the writing worker could not start: {e}", "c": "NOT-RENDERED", "why": "Dictation slice 5 (dictation/tool.rs:1163): the thread that runs whisper-cli could not be spawned; a dictation.log line (`log::line`). The session is finished with `Problem::CouldNotWrite`, whose bar line (Sorry, I couldn't write that down) is classified above and carries the key that tries again."},
  {"s": "{LOCK_NAME} could not be locked: {}", "c": "NOT-RENDERED", "why": "Dictation slice 5 (review finding 3, dictation/store.rs:121): `store::lock`'s Err when `flock` on dictation.lock fails. Every reader of it is a log: the tool writes it to dictation.log (tool.rs:822), the app's `dictation_app` prints it with `eprintln!` (dictation_app.rs:315, :410, :470) or returns it as a sheet command's Err, which `dictation.js` `call()` logs to the console and draws nothing of, and Rich's offer (`dictation_offer::mark_shown`) prints it with `eprintln!` (dictation_offer.rs:66)."},
  {"s": "and it stays open beside us.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The end of Rich's question in the Rich panel: \"I've noted that you were in <window>, and it stays open beside us.\""},
  {"s": ", because RichOS isn't signed in to a GitHub account of yours", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The From line after \"the RichOS reporting account\"."},
  {"s": ", which can't take messages yet, so we're talking here.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The end of Rich's question in the Rich panel for a conversation that cannot take messages."},
  {"s": "; point at one to see what it replaced.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The end of the left-out line under the sheet."},
  {"s": "Added that to", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Rich's line after a change told to him: \"Added that to <section>, above.\""},
  {"s": "I've noted that you were in", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of the second sentence of Rich's question."},
  {"s": "I've noted that you were on", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of the second sentence of Rich's question."},
  {"s": "It still waits for you to send it.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The end of \"Added that to <section>, above.\" while the report has not been sent; the Send report button is on the card above."},
  {"s": "The report changed, so it waits for you to send it again.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The end of \"Added that to <section>, above.\" for a report that was waiting to send; Send report is on the card above."},
  {"s": "It's issue #", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Inside Rich's line after a report went out, before the issue number."},
  {"s": "Sent. It's issue #", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line after Send, before the issue number and the account it was filed from."},
  {"s": "I sent your bug report.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line when a waiting report went out by itself for a reason other than the two named ones."},
  {"s": "You're back online, so I sent your bug report.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line when a waiting report went out by itself after the Mac was offline."},
  {"s": "GitHub is answering again, so I sent your bug report.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line when a waiting report went out by itself after GitHub did not answer."},
  {"s": "Open issue #", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The sent card's link, before the issue number; the link is the control."},
  {"s": "Stands in for “", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of the stand-in tooltip, before the words it replaced."},
  {"s": "Stands in for", "c": "FRAGMENT", "why": "Bust a bug (nightly 48's walk, D23; bug-report.js). The start of the tooltip of a stand-in whose replaced words the app was not told, before its kind (\"a person's name\")."},
  {"s": "This Mac wouldn't open the issue. It's at github.com/WebDevBooster/richos/issues/", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). A notice when /usr/bin/open refused the issue's page, before the number; the address is the way there."},
  {"s": "Your bug report went out: issue #", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The notice when a waiting report went out by itself, before the number."},
  {"s": "filed from the RichOS reporting account, because RichOS isn't signed in to a GitHub account of yours.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The end of Rich's line after a report went out from the reporting account."},
  {"s": "filed from your GitHub account, @", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The end of Rich's line after a report went out from the user's own account (once RichOS can sign in to GitHub), before the login."},
  {"s": "your GitHub account,", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The From line for the user's own GitHub account, before the login."},
  {"s": "the RichOS reporting account", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The From line's account and the sent card's \"filed from\"."},
  {"s": "a RichOS window", "c": "FRAGMENT", "why": "Bust a bug (review rv-20261010T081721Z-66ba5386-a4d1 finding 2; bug-report.js openWindow). Where the user was, inside Rich's question, and the report's Where, for a dialog on screen that names no title."},
  {"s": "the RichOS window titled", "c": "FRAGMENT", "why": "Bust a bug (review rv-20261010T081721Z-66ba5386-a4d1 finding 2; bug-report.js openWindow). The report's Where for a dialog on screen, before its title."},
  {"s": "a RichOS screen", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for a screen Bust a bug does not name, inside \"<place>. It was on screen when the report was started.\""},
  {"s": "a company's page", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Where the user was, inside Rich's question."},
  {"s": "a company's page, with its conversations", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for a company's page."},
  {"s": "a conversation that isn't filed under a company yet, where sending is off", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for a conversation with no company."},
  {"s": "a conversation, the main RichOS screen", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for a conversation."},
  {"s": "the Corrections window", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for Corrections."},
  {"s": "the Feedback window", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for Feedback."},
  {"s": "the Search window", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for Search."},
  {"s": "the company picker", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Where the user was, in Rich's question and the panel's header."},
  {"s": "the home screen", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Where the user was, in Rich's question and the panel's header."},
  {"s": "the opening screen", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Where the user was, in Rich's question and the panel's header, when Bust a bug is pressed on the held opening screen."},
  {"s": "the opening screen RichOS shows as it starts", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for the held opening screen."},
  {"s": "the home screen, with a button for each company", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for the home screen."},
  {"s": "the memory folder window", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Where the user was, in Rich's question and the panel's header."},
  {"s": "the setup window", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Where the user was, in Rich's question and the panel's header."},
  {"s": "the window that asks where RichOS keeps what it is told", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for the memory folder window."},
  {"s": "the window that asks which company a new conversation belongs to", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for the company picker."},
  {"s": "the window that sets RichOS up", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The report's Where for the setup window."},
  {"s": "this company's page", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The Rich panel's header on a company's page."},
  {"s": "Bust a bug, with Rich", "c": "CONTROL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The Rich panel's accessible name."},
  {"s": "Close the bug report panel", "c": "CONTROL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The Rich panel's close button's accessible name."},
  {"s": "You said aloud", "c": "CONTROL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The screen-reader label on the user's spoken answer, in the slot where a typed one says \"You said\"."},
  {"s": "What went wrong? Tell me in your own words, typed or out loud and where it happened if it wasn't here.", "c": "ACTIONABLE", "control": "#input, #bugdock-input, #bug-never-mind", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). Rich's question. The composer (or the panel's own) takes the answer; Never mind is under the question."},
  {"s": "Tell Rich what went wrong…", "c": "ACTIONABLE", "control": "#input", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). The composer's placeholder while Rich waits for the answer: the field it is in is the control."},
  {"s": "Tell Rich what to change or press Send…", "c": "ACTIONABLE", "control": "#input, #bug-send", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). The composer's placeholder while the report waits: the field, and Send report on the card."},
  {"s": "Change any of the words above or tell Rich what to change.", "c": "ACTIONABLE", "control": "#bug-done", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). Shown while the sheet is editable in place; Done is on the card."},
  {"s": "Here's the report as I'd file it. Nothing goes out until you press Send. Anyone can read GitHub issues, so I left out names, company details and file paths.", "c": "ACTIONABLE", "control": "#bug-send", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). Rich's line above the new card: Send report, Change it and Cancel are on it."},
  {"s": "I couldn't change the report just now. Press Change it and change the words yourself or tell me again.", "c": "ACTIONABLE", "control": "#bug-change", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). The bridge failed on a change told to Rich; Change it is on the card."},
  {"s": "I couldn't save the report on this Mac, so nothing was sent. Press Send report to try again.", "c": "ACTIONABLE", "control": "#bug-send", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). bug_report_send failed to keep the report; Send report is back on the card."},
  {"s": "I couldn't write that up just now. Tell me again and I'll try once more.", "c": "ACTIONABLE", "control": "#input, #bugdock-input", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). The bridge failed on the answer (Rich's own failure keeps the report unchecked on this Mac, review rv-20261009T162841Z-69294215-70e6); the composer takes it again."},
  {"s": "I couldn't open the microphone. Type it instead.", "c": "ACTIONABLE", "control": "#bugdock-input", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). The Rich panel's talk button could not open the microphone; the panel's composer is back."},
  {"s": "This Mac is offline, so the report didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll send it by myself as soon as you're back online and tell you here when it's gone.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). It goes out by itself (spawn_retry); Try now and Cancel report are on the card for anyone who wants them."},
  {"s": "GitHub isn't answering right now, so the report didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll keep trying every few minutes and tell you here when it's filed.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). It goes out by itself; Try now and Cancel report are on the card."},
  {"s": "GitHub didn't accept the report from the RichOS reporting account, so it didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll try again in a few minutes and tell you here when it's filed.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). Tried again by itself every ten minutes; Try now and Cancel report are on the card."},
  {"s": "The RichOS reporting account isn't set up on this Mac yet, so the report didn't go out. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll send it by myself once the account is set up and tell you here when it's filed.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). The token is put in the keychain by whoever runs RichOS's reporting account, not the user; the report goes out by itself once it is there."},
  {"s": "GitHub still isn't answering. It stays saved here and I'll keep trying.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). A second failed attempt after Try now."},
  {"s": "Still offline. It stays saved here and goes out by itself when you're back online.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). A second failed attempt after Try now."},
  {"s": "It still didn't go out. It stays saved here and I'll keep trying.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). A second failed attempt after Try now, for the other reasons."},
  {"s": "Your bug report is saved on this Mac and goes out by itself when it can.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The notice when the Rich panel is closed while its report waits to send."},
  {"s": "Waiting to send · saved on this Mac", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). The waiting card's badge."},
  {"s": "Not sent yet", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). The draft card's badge; Send report is beside it."},
  {"s": "Not sent yet · changed", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). The badge after a change."},
  {"s": "Canceled · nothing sent", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). The canceled card's badge."},
  {"s": "Canceled. Nothing was sent.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Rich's line after Cancel."},
  {"s": "No problem. Nothing was written up or sent.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Rich's line after Never mind."},
  {"s": "RichOS on GitHub, as an issue anyone can read", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). The To line."},
  {"s": "Rich is changing the report…", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). While Rich folds a change into the report."},
  {"s": "Tell Rich what else to change…", "c": "ACTIONABLE", "control": "#input, #bugdock-input", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). The composer's placeholder while Rich folds a change in: a further change typed there follows it (second review rv-20261009T102303Z-1c3dda1d-4c78, finding 3)."},
  {"s": "Taking it back to change it…", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The waiting card's badge while its copy is taken off this Mac to be changed; its actions return when the shell confirms (second review finding 2)."},
  {"s": "I couldn't cancel the report: it's still saved on this Mac and will go out by itself. Press Cancel report to try again.", "c": "ACTIONABLE", "control": "#bug-cancel", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). The waiting copy could not be removed (second review finding 2); the waiting card is back with Cancel report on it."},
  {"s": "I couldn't take the report back to change it: it's still saved on this Mac and will go out by itself. Press Change it to try again.", "c": "ACTIONABLE", "control": "#bug-change", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). The waiting copy could not be removed to change it (second review finding 2); the waiting card is back with Change it on it."},
  {"s": "It had already gone out before I could cancel it.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line when Cancel report found the report already filed (second review finding 2), before \"It's issue #<n> on GitHub, filed from …\"."},
  {"s": "It had already gone out, so it can't be changed.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line when Change it found the report already filed (second review finding 2), before \"It's issue #<n> on GitHub, filed from …\"."},
  {"s": "Rich is checking…", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). While Rich writes the report up."},
  {"s": "This Mac wouldn't open {what}. It's at {}.", "c": "NOT-RENDERED", "why": "Bust a bug and Request a feature (round 21 and round 24; bug-report.js). bug_report_open_issue's Err (src-tauri/src/bug_report.rs), naming the issue or the idea; the window shows its own notice with the address instead."},
  {"s": "claude took too long", "c": "NOT-RENDERED", "why": "Bust a bug (round 21, CEO §115; bug-report.js). ask_rich's Err (src-tauri/src/bug_report.rs), logged; the report is kept on this Mac until Rich can check it (review rv-20261009T162841Z-69294215-70e6)."},
  {"s": "no section {:?}", "c": "NOT-RENDERED", "why": "Bust a bug (round 21, CEO §115; bug-report.js). bug_report_change's reason for not using Rich's answer, logged; nothing is added (review rv-20261009T162841Z-69294215-70e6)."},
  {"s": "I couldn't check this report for private details yet, because Claude didn't answer, so it isn't ready to send. It's saved on this Mac and I'll check it by myself as soon as Claude answers, then show it to you here.", "c": "ACTIONABLE", "control": "#bug-unchecked-cancel", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). Rich's line when Claude failed, timed out or answered with no report: nothing is offered for sending (review rv-20261009T162841Z-69294215-70e6 finding 2); Cancel report is under it."},
  {"s": "I've checked it now.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line when he checked a report he could not check at first, before \"Here's the report as I'd file it…\"."},
  {"s": "I've checked the bug report you told me about earlier.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line in the Rich panel when he checked a report kept before a quit or a reload, before \"Here's the report as I'd file it…\"."},
  {"s": "Canceled. Nothing was sent and it's no longer saved on this Mac.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). Rich's line after Cancel report on a report he could not check yet."},
  {"s": "I couldn't take the report off this Mac, so it's still saved here. Press Cancel report to try again.", "c": "ACTIONABLE", "control": "#bug-unchecked-cancel", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). Cancel report on a report Rich could not check yet failed to take it off this Mac (bug_report_take_unchecked's Err in src-tauri/src/bug_report.rs, and the window's own line); Cancel report is offered again."},
  {"s": "Your bug report is saved on this Mac. Rich will check it as soon as Claude answers and show it to you here.", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The notice when the Rich panel is closed while its report waits for Rich's check."},
  {"s": "nothing was sent", "c": "NOT-RENDERED", "why": "Bust a bug (round 21, CEO §115; bug-report.js). bug_report_send's Err for an attempt that returned nothing, which Outbox::send never does; the window draws its own line on any Err."},
  {"s": "Rich is checking your changes…", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). While Send is held for Rich's check of words the user changed by hand (review rv-20261009T174727Z-3007e320-578a finding 1): Rich's working line, and the card's badge meanwhile."},
  {"s": "I couldn't check your changes for private details yet, because Claude didn't answer, so I didn't send the report. It's saved on this Mac and I'll check it by myself as soon as Claude answers, then show it to you here.", "c": "ACTIONABLE", "control": "#bug-unchecked-cancel", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). Rich's line when Claude could not check words the user changed by hand at Send (review rv-20261009T174727Z-3007e320-578a finding 1): nothing was sent, the words wait on this Mac, and Cancel report is beside it."},
  {"s": "I checked it again before sending and some of its words would go out differently from how the card showed them. Here it is exactly as it would go. Nothing goes out until you press Send.", "c": "ACTIONABLE", "control": "#bug-send", "fixture": null, "why": "Bust a bug (round 21, CEO §115; bug-report.js). At Send the issue as it would be posted differed from the card in some character (Rich's check of words changed by hand, the last scrub, or a title's backtick; reviews rv-20261009T174727Z-3007e320-578a and rv-20261009T190633Z-d9cdd913-9f1c): nothing was sent and the card is shown again as it would go, Send report on it."},
  {"s": "Waiting for Rich's check · saved on this Mac", "c": "INFORMATIONAL", "why": "Bust a bug (round 21, CEO §115; bug-report.js). On the report card, whose actions are on the card itself (`.bug-actions`). The badge while words the user changed by hand wait for Rich's check; Cancel report is in Rich's line beside it."},
  {"s": "I've checked your changes now.", "c": "FRAGMENT", "why": "Bust a bug (round 21, CEO §115; bug-report.js). The start of Rich's line when he checked words the user changed by hand that he could not check at Send, before \"Here's the report as I'd file it…\"."},
  // ---- Request a feature (CEO 2026-10-11, round 24; bug-report.js, settings-button.js) ----
  {"s": ", in case it's about this screen.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The end of Rich's question on a conversation: \"I've noted that you were on <conversation>, in case it's about this screen.\""},
  {"s": ", where anyone can read and upvote it", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The end of the idea card's To line, after the Ideas label."},
  {"s": "RichOS on GitHub, as a post in its Discussions, under", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The start of the idea card's To line, before the Ideas label."},
  {"s": "Anyone can read it and upvote it there.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The end of Rich's line after an idea was posted, before its link."},
  {"s": "It's in RichOS's Discussions on GitHub, under Ideas,", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Inside Rich's line after an idea was posted, before the account it was posted from."},
  {"s": "posted from the RichOS reporting account, because RichOS isn't signed in to a GitHub account of yours.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The end of Rich's posted line from the reporting account (round 21's words)."},
  {"s": "posted from your GitHub account, @", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Rich's posted line from the user's own account (once RichOS can sign in to GitHub), before the login."},
  {"s": "I posted your idea.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The start of Rich's line when a waiting idea was posted by itself for a reason other than the two named ones."},
  {"s": "You're back online, so I posted your idea.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The start of Rich's line when a waiting idea was posted by itself after the Mac was offline."},
  {"s": "GitHub is answering again, so I posted your idea.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The start of Rich's line when a waiting idea was posted by itself after GitHub did not answer."},
  {"s": "Anyone can read this", "c": "FRAGMENT", "why": "Bust a bug and Request a feature (bug-report.js). The heads-up after a private word went back into the card, before \"report\" or \"post\" and \"on GitHub.\"; Change it / Done edit it and Send report or Post idea is the user's to press anyway."},
  {"s": "Left out, because anyone can read", "c": "FRAGMENT", "why": "Bust a bug and Request a feature (bug-report.js). The start of the left-out line under the sheet, before \"GitHub issues\" or \"GitHub Discussions\" and the counted kinds."},
  {"s": "Nothing private was in it, so nothing was left out. Anyone can read", "c": "FRAGMENT", "why": "Bust a bug and Request a feature (bug-report.js). The left-out line when nothing was replaced, before \"GitHub issues.\" or \"GitHub Discussions.\"."},
  {"s": "it isn't in the report.", "c": "FRAGMENT", "why": "Bust a bug (bug-report.js). The end of a stand-in's tooltip on a bug report: \"Only you see this; it isn't in the report.\""},
  {"s": "it isn't in the post.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The end of a stand-in's tooltip on an idea: \"Only you see this; it isn't in the post.\""},
  {"s": "GitHub didn't let", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The start of Rich's line when GitHub refused the account (a token without permission to post in Discussions), before the account."},
  {"s": "post in RichOS's Discussions, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll try again in a few minutes and tell you here when it's up.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The rest of Rich's line when GitHub refused the account; Try now, Change it and Cancel post are on the card."},
  {"s": "your GitHub account, @", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Inside Rich's refused line for the user's own account, before the login."},
  {"s": "Canceled · nothing posted", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). The canceled idea's badge."},
  {"s": "Canceled. Nothing was posted and it's no longer saved on this Mac.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Rich's line after Cancel post on an idea he could not check yet."},
  {"s": "Canceled. Nothing was posted.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Rich's line after Cancel."},
  {"s": "No problem. Nothing was written up or posted.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Rich's line after Never mind."},
  {"s": "Close the feature request panel", "c": "CONTROL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The Rich panel's close button's accessible name while it holds an idea."},
  {"s": "Request a feature", "c": "CONTROL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The Settings row directly under Bust a bug (settings-button.js), and the Rich panel's title while it holds an idea."},
  {"s": "Got it. Your idea starts from this exact screen. Nothing leaves this machine until you say so.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Request a feature's acknowledgement on a page where nothing registered what it opens (settings-button.js lifted onto another page), as Bust a bug's is: nothing is asked of the user."},
  {"s": "Here's your idea as I'd post it. Nothing goes out until you press Post idea. Anyone can read GitHub Discussions, so I left out names, company details and file paths.", "c": "ACTIONABLE", "control": "#bug-send", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). Rich's line above the new card: Post idea, Change it and Cancel are on it."},
  {"s": "I checked it again before posting and some of its words would go out differently from how the card showed them. Here it is exactly as it would go. Nothing goes out until you press Post idea.", "c": "ACTIONABLE", "control": "#bug-send", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). At Post idea the words as they would be posted differed from the card; nothing was posted and the card is shown again as it would go, Post idea on it."},
  {"s": "I couldn't cancel the post: it's still saved on this Mac and will be posted by itself. Press Cancel post to try again.", "c": "ACTIONABLE", "control": "#bug-cancel", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The waiting copy could not be removed; the waiting card is back with Cancel post on it."},
  {"s": "I couldn't change the post just now. Press Change it and change the words yourself or tell me again.", "c": "ACTIONABLE", "control": "#bug-change", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). The bridge failed on a change told to Rich."},
  {"s": "I couldn't check your changes for private details yet, because Claude didn't answer, so I didn't post your idea. It's saved on this Mac and I'll check it by myself as soon as Claude answers, then show it to you here.", "c": "ACTIONABLE", "control": "#bug-unchecked-cancel", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Changed words Rich could not check at Post idea wait on this Mac; Cancel post is in his line."},
  {"s": "I couldn't check your idea for private details yet, because Claude didn't answer, so it isn't ready to post. It's saved on this Mac and I'll check it by myself as soon as Claude answers, then show it to you here.", "c": "ACTIONABLE", "control": "#bug-unchecked-cancel", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Rich could not check the idea; it waits on this Mac and Cancel post is in his line."},
  {"s": "I couldn't check your idea or save it on this Mac, so nothing was written up or posted. Tell me again and I'll try once more.", "c": "ACTIONABLE", "control": "#input, #bugdock-input", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The bridge failed on the answer; the panel's composer takes it again."},
  {"s": "I couldn't save your idea on this Mac, so nothing was posted. Press Post idea to try again.", "c": "ACTIONABLE", "control": "#bug-send", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). bug_report_send failed to keep the idea on this Mac."},
  {"s": "I couldn't take the post back to change it: it's still saved on this Mac and will be posted by itself. Press Change it to try again.", "c": "ACTIONABLE", "control": "#bug-change", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The waiting copy could not be removed to change it; the waiting card is back with Change it on it."},
  {"s": "I couldn't take your idea off this Mac, so it's still saved here. Press Cancel post to try again.", "c": "ACTIONABLE", "control": "#bug-unchecked-cancel", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Cancel post on an idea Rich could not check yet failed to take it off this Mac."},
  {"s": "I've checked the idea you told me about earlier.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The start of Rich's line in the Rich panel when he checked an idea kept before a quit or a reload."},
  {"s": "It still waits for you to post it.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The end of \"Added that to <section>, above.\" while the idea has not been posted; Post idea is on the card above."},
  {"s": "The post changed, so it waits for you to post it again.", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The end of \"Added that to <section>, above.\" for an idea that was waiting to post; Try now is on the card above."},
  {"s": "Not posted yet", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). The idea card's badge; Post idea is beside it."},
  {"s": "Not posted yet · changed", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). The badge after a change."},
  {"s": "Posted · in Ideas", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). The posted idea's badge; the bulb lights beside it."},
  {"s": "Waiting to post · saved on this Mac", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). The waiting idea's badge."},
  {"s": "Open it on GitHub", "c": "CONTROL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The posted idea card's link to its discussion."},
  {"s": "Rich is changing the post…", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). While Rich folds a change into the idea."},
  {"s": "Tell Rich what to change or post it…", "c": "ACTIONABLE", "control": "#bugdock-input, #bug-send", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The panel composer's placeholder while the idea waits: the field, and Post idea on the card."},
  {"s": "Tell Rich your idea…", "c": "ACTIONABLE", "control": "#bugdock-input", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The panel composer's placeholder while Rich waits for the idea: the field it is in is the control."},
  {"s": "GitHub isn't answering right now, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll keep trying every few minutes and tell you here when it's up.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). Posted by itself when GitHub answers again; Try now and Cancel post are on the card."},
  {"s": "The RichOS reporting account isn't set up on this Mac yet, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll post it by myself once the account is set up and tell you here when it's up.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). Posted by itself once the reporting account's token is on this Mac; Try now and Cancel post are on the card."},
  {"s": "This Mac is offline, so your idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll post it by myself as soon as you're back online and tell you here when it's up.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). On the idea's card, whose actions are on the card itself (`.bug-actions`). Posted by itself when the Mac is back online; Try now and Cancel post are on the card."},
  {"s": "This Mac wouldn't open your idea. It's at github.com/WebDevBooster/richos/discussions/", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). A notice when /usr/bin/open refused the discussion's page, before the number; the address is the way there."},
  {"s": "What the user pictured", "c": "FRAGMENT", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). A heading of the idea's post (the core's `bug_report::BULLETED`), over its bulleted points."},
  {"s": "What would you like RichOS to do? Tell me in your own words, typed or out loud.", "c": "ACTIONABLE", "control": "#bugdock-input, #bug-never-mind", "fixture": null, "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). Rich's question. The panel's composer takes the answer; Never mind ends it."},
  {"s": "Your idea is posted in Ideas, on GitHub.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The notice when a waiting idea was posted by itself."},
  {"s": "Your idea is saved on this Mac and is posted by itself when it can.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The notice when the Rich panel is closed while its idea waits to post."},
  {"s": "Your idea is saved on this Mac. Rich will check it as soon as Claude answers and show it to you here.", "c": "INFORMATIONAL", "why": "Request a feature (CEO 2026-10-11, round 24; bug-report.js). The notice when the Rich panel is closed while its idea waits for Rich's check."},

  // The Codex review switch (round 20.2, ruling §114, `codex-reviews.js`): the Settings row's
  // markup, the lapsed state's two lines and the unavailable switch's two hover titles.
  {"s":"<div class=\"set-rule\" role=\"presentation\"></div><section class=\"cx\" id=\"cx-row\" aria-labelledby=\"cx-name\"><div class=\"cx-head\"><div class=\"cx-name\" id=\"cx-name\"><span id=\"cx-title\">Let Codex review your team’s work</span><div class=\"cx-who\"><span id=\"cx-who\">Reviewing now: <b>Claude</b></span><span class=\"cx-info-wrap\" id=\"cx-info-wrap\"><button class=\"cx-info\" id=\"cx-info\" type=\"button\" aria-label=\"About Codex reviews\" aria-describedby=\"cx-tip-off\" aria-expanded=\"false\">","c":"ACTIONABLE","control":"#cx-switch","fixture":null,"why":"Round 20.2 (richos-hq design/mockups/rounds/round-20.2/, the CEO's approved design; his words, ruling §114: \"we should give the user a toggle/switch to manually enable that\", `codex-reviews.js`): the head of the Settings row's markup, assigned to innerHTML: its name (Let Codex review your team's work), Reviewing now: Claude, and the start of the ⓘ (About Codex reviews). Who reviews is his choice, and the switch that changes it (#cx-switch; pressing the name flips it too) sits at the end of the same row in every state. tests/codex-reviews.js renders every state and computes contrast in both themes."},
  {"s":"</button></span><div class=\"cx-tip\" id=\"cx-tip\" role=\"tooltip\" hidden><div data-st=\"off\" id=\"cx-tip-off\"><p class=\"cx-p\">Before your team’s finished work is accepted, a second AI reviews it. Claude does that now. Turn this on and Codex does it instead, so a model from a different company checks the work.</p><div class=\"cx-aware\"><p><strong>Codex will read your team’s work,</strong> and each review uses a little of your Codex allowance.</p><p class=\"cx-unseen\">These reviews won’t show up in your ChatGPT or Codex app: no conversations or history appear there.</p></div></div><div data-st=\"on\" id=\"cx-tip-on\" hidden><p class=\"cx-p\">Codex reviews your team’s finished work before it’s accepted, through the Codex app on this Mac, signed in to ChatGPT.</p><div class=\"cx-aware\"><p><strong>Codex reads your team’s work,</strong> and each review uses a little of your Codex allowance. Turn this off and Claude reviews it again.</p><p class=\"cx-unseen\">These reviews won’t show up in your ChatGPT or Codex app: no conversations or history appear there.</p></div></div><div data-st=\"missing\" id=\"cx-tip-missing\" hidden><p class=\"cx-p\">Before your team’s finished work is accepted, a second AI reviews it. Claude does that now. Codex could do it instead, so a model from a different company checks the work.</p><p class=\"cx-why\"><strong>The Codex app isn’t on this Mac.</strong> Install it and sign in to ChatGPT and this switch can be turned on.</p></div><div data-st=\"signedout\" id=\"cx-tip-signedout\" hidden><p class=\"cx-p\">Before your team’s finished work is accepted, a second AI reviews it. Claude does that now. Codex could do it instead, so a model from a different company checks the work.</p><p class=\"cx-why\"><strong>Codex isn’t signed in.</strong> Open the Codex app and sign in to ChatGPT and this switch can be turned on.</p></div><div data-st=\"lapsed\" id=\"cx-tip-lapsed\" hidden><p class=\"cx-p\">You turned this on, so Codex reviews your team’s work whenever it can.</p><p class=\"cx-why cx-warn\" id=\"cx-lapsed\"></p><p class=\"cx-p cx-unseen\">These reviews won’t show up in your ChatGPT or Codex app: no conversations or history appear there.</p></div></div></div></div><button class=\"sw\" id=\"cx-switch\" type=\"button\" role=\"switch\" aria-checked=\"false\" aria-labelledby=\"cx-title cx-who\" aria-describedby=\"cx-tip-off\"></button></div></section><div class=\"set-rule\" role=\"presentation\"></div>","c":"ACTIONABLE","control":"#cx-switch","fixture":null,"why":"Round 20.2 (richos-hq design/mockups/rounds/round-20.2/, the CEO's approved design; his words, ruling §114: \"we should give the user a toggle/switch to manually enable that\", `codex-reviews.js`): the rest of the row's markup: the ⓘ's tooltip, one block per state, and the switch. Off says Turn this on, on says Turn this off, and lapsed keeps the choice that can be turned off: each is #cx-switch beside it. Missing and signed out name the fix outside RichOS (install the Codex app, or sign in to ChatGPT in it), after which the same switch can be turned on; pressing it before that shakes it and opens this reason. tests/codex-reviews.js checks each state's words word for word, in both themes with contrast computed."},
  {"s":"<strong>The Codex app isn’t on this Mac right now,</strong> so Claude is reviewing in the meantime. Install it, sign in to ChatGPT and Codex takes over again.","c":"ACTIONABLE","control":"#cx-switch","fixture":null,"why":"Round 20.2 (richos-hq design/mockups/rounds/round-20.2/, the CEO's approved design; his words, ruling §114: \"we should give the user a toggle/switch to manually enable that\", `codex-reviews.js`): the lapsed state's line in the ⓘ's tooltip (#cx-lapsed) when he turned Codex on and the Codex app has since gone from this Mac. Installing it again is outside RichOS; Claude covers in the meantime. The control in the same view is the switch, still on and still usable, which turns the choice off. tests/codex-reviews.js renders the lapsed states."},
  {"s":"<strong>Codex isn’t signed in right now,</strong> so Claude is reviewing in the meantime. Sign in to ChatGPT in the Codex app and Codex takes over again.","c":"ACTIONABLE","control":"#cx-switch","fixture":null,"why":"Round 20.2 (richos-hq design/mockups/rounds/round-20.2/, the CEO's approved design; his words, ruling §114: \"we should give the user a toggle/switch to manually enable that\", `codex-reviews.js`): the lapsed state's line in the ⓘ's tooltip (#cx-lapsed) when he turned Codex on and Codex has since signed out. Signing in to ChatGPT in the Codex app is outside RichOS; Claude covers in the meantime. The control in the same view is the switch, still on and still usable, which turns the choice off. tests/codex-reviews.js check 'on, then signed out' renders it."},
  {"s":"Codex isn’t signed in","c":"ACTIONABLE","control":"#cx-info","fixture":null,"why":"Round 20.2 (richos-hq design/mockups/rounds/round-20.2/, the CEO's approved design; his words, ruling §114: \"we should give the user a toggle/switch to manually enable that\", `codex-reviews.js`): the unavailable switch's hover title when Codex is signed out and the choice is off. The fix (sign in to ChatGPT in the Codex app) is outside RichOS and is named in the ⓘ's tooltip beside it (#cx-info, the signedout block), which pressing the switch also opens."},
  {"s":"The Codex app isn’t on this Mac","c":"ACTIONABLE","control":"#cx-info","fixture":null,"why":"Round 20.2 (richos-hq design/mockups/rounds/round-20.2/, the CEO's approved design; his words, ruling §114: \"we should give the user a toggle/switch to manually enable that\", `codex-reviews.js`): the unavailable switch's hover title when the Codex app is missing and the choice is off. The fix (install the Codex app and sign in to ChatGPT) is outside RichOS and is named in the ⓘ's tooltip beside it (#cx-info, the missing block), which pressing the switch also opens."},
  // API-equivalent token spend (round 22.1, ruling §118, `spend.js`): its own screen and the Settings row.
  {"s":", some not recorded","c":"INFORMATIONAL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): a model mark's effort words when its requests recorded no effort (the app's own conversation streams none): stated, never guessed. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"1 agent working","c":"INFORMATIONAL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the live line: how many of Rich's agents are working now, from the quota service's own count. Nothing to do. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"</b> of them were read back from cache, which is what lets Rich keep the whole company in mind.</span></p><p class=\"sp-live\" id=\"spend-live\"><span class=\"working-dot\" aria-hidden=\"true\"></span><span class=\"lv\"></span></p></div></div>","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the tail of the tokens line and the live line's frame, which spend.js fills with who is working and the last request. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"</b>. Counted on this Mac from every request. <b>Not a bill.</b>","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the footer's tail after the price date. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"</span></p><div class=\"sp-figure sp-enter sp-e2\" id=\"spend-fig\" role=\"img\"></div><p class=\"sp-sub sp-enter sp-e3\"><span>What the same work would have cost at Claude’s API prices.</span><span><b>Billed by the API: nothing.</b> It ran on your Claude subscription.</span></p><div class=\"sp-enter sp-e3\">","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the Meter's figure and the two sentences under it (What the same work would have cost at Claude's API prices. / Billed by the API: nothing. It ran on your Claude subscription.), two lines as he asked. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"<div class=\"sp-meter\"><p class=\"sp-eyebrow sp-enter sp-e1\">API-equivalent token spend · <span id=\"spend-when\">","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the head of the Meter's markup: the eyebrow naming the screen, before its period words. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"<div class=\"sp-meter\"><p class=\"sp-eyebrow\">API-equivalent token spend</p><p class=\"sp-sub sp-counting\" role=\"status\"><span>Counting every request on this Mac.</span><span>The first count reads the whole history once.</span></p></div>","c":"INFORMATIONAL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the counting state, before the first read of the whole history is done. The app reads it on its own thread and the Meter replaces this the moment it is done; there is nothing to do and no partial figure is shown (round 22.1 draws no such state). tests/spend.js renders it and computes contrast in both themes."},
  {"s":"<div class=\"sp-seg\" data-m=\"","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the opening of a model mark's markup (the bar segment or the legend key), completed by its model and figures. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"<header class=\"sp-head\"><button class=\"sp-back\" id=\"spend-back\" type=\"button\"><svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"m15 18-6-6 6-6\"/></svg>Back</button><h1 class=\"sp-title\" id=\"spend-title\">API-equivalent token spend <span id=\"spend-title-when\"></span></h1><div class=\"sp-fresh\"><span class=\"sp-updated\" id=\"spend-updated\" aria-live=\"polite\"></span><button class=\"sp-refresh\" id=\"spend-refresh\" type=\"button\" aria-label=\"Refresh the numbers\"><svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M21 12a9 9 0 1 1-2.64-6.36\"/><path d=\"M21 3v6h-6\"/></svg>Refresh<span class=\"sp-new\" aria-hidden=\"true\"></span></button></div><div class=\"sp-period\" role=\"group\" aria-label=\"Period\" id=\"spend-period\"><button type=\"button\" data-p=\"today\" aria-pressed=\"false\">Today</button><button type=\"button\" data-p=\"week\" aria-pressed=\"false\">This week</button><button type=\"button\" data-p=\"month\" aria-pressed=\"true\">This month</button><button type=\"button\" data-p=\"all\" aria-pressed=\"false\">All time</button></div></header><div class=\"sp-body\" id=\"spend-body\"></div><footer class=\"sp-foot\"><span class=\"sp-wordmark\" id=\"spend-wordmark\"></span><p class=\"sp-foot-note\" id=\"spend-foot\"></p></footer>","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the screen's frame, assigned to innerHTML: Back, the title with its period, Updated, Refresh and the period control (Today, This week, This month, All time), the body and the footer. Every control in it acts on this screen; the figures come in below it. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"<p class=\"sp-key\" data-m=\"","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the opening of a model mark's markup (the bar segment or the legend key), completed by its model and figures. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"Brought up to date at","c":"INFORMATIONAL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the Updated time's tooltip, followed by the time of the snapshot. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"No agents working right now","c":"INFORMATIONAL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the live line: how many of Rich's agents are working now, from the quota service's own count. Nothing to do. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"Refresh the numbers","c":"CONTROL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the Refresh button's accessible name (with the time of the snapshot when requests have landed since), not a separate state. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"Refresh the numbers: new requests since","c":"CONTROL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the Refresh button's accessible name (with the time of the snapshot when requests have landed since), not a separate state. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"What the same work would have cost at Claude’s API prices as published on <b>","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the footer's head, before the price date. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"effort not recorded","c":"INFORMATIONAL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): a model mark's effort words when its requests recorded no effort (the app's own conversation streams none): stated, never guessed. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"tokens are counted but not priced: no single published price matches them.","c":"INFORMATIONAL","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the footer's note, after the count, when tokens could not be priced: a model with no published rate, or output a session reported only in total over requests on two of Haiku 5.5's cards. They are counted and left out of the dollars, never guessed. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"since the first request","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): All time's period words when nothing is on record yet, after the eyebrow. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"· last request","c":"FRAGMENT","why":"Round 22.1 (richos-hq design/mockups/rounds/round-22.1/, the CEO's chosen design; his words, ruling §118: \"That's needs to be its own screen/page.\", `spend.js`): the live line's middle, between the agents working and the last request's model, tokens and price in dollars. tests/spend.js renders it and computes contrast in both themes."},
  {"s":"button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])","c":"NOT-RENDERED","why":"A CSS selector spend.js uses to give a Settings sheet that takes the Token spend screen's place the focus its own open() could not give it (round 22.1, ruling §118); never displayed."},

  // Rich's model rows in Preferences (round 23, ruling §119, `main.js` MODEL_NOTE / MODEL_TOAST
  // and `#md` in index.html; `set_lease_models` in src-tauri/src/main.rs). The rows are the
  // controls; the note and the toasts say what is true and ask nothing.
  {"s":"Opus is the default for both. Fable is more capable than Opus and uses up your Claude limits faster.","c":"INFORMATIONAL","why":"Round 23 (§119): `#md-note` under Rich's model rows when both rows show Opus (MODEL_NOTE \"opus,opus\"). It states the default and what Fable costs; the two radio rows directly above it are the controls, and nothing here asks him to use them."},
  {"s":"<strong>Rich talks with you on Fable.</strong> Fable is more capable than Opus and uses up your Claude limits faster.","c":"INFORMATIONAL","why":"Round 23 (§119): `#md-note` when the talks-with-you row shows Fable (MODEL_NOTE \"fable,opus\"). It says what is now chosen and what that costs; the rows above it change it, and the note asks nothing."},
  {"s":"<strong>Rich directs your team on Fable.</strong> Fable is more capable than Opus and uses up your Claude limits faster. Your teammates keep their own models.","c":"INFORMATIONAL","why":"Round 23 (§119): `#md-note` when the directs-your-team row shows Fable (MODEL_NOTE \"opus,fable\"). It says what is chosen, what it costs and that teammates' own models are unaffected; it asks nothing."},
  {"s":"<strong>Rich is on Fable for both.</strong> Fable is more capable than Opus and uses up your Claude limits faster, on every reply and every piece of work he directs.","c":"INFORMATIONAL","why":"Round 23 (§119): `#md-note` when both rows show Fable (MODEL_NOTE \"fable,fable\"). It says what is chosen and what it costs; it asks nothing."},
  {"s":"Opus · high effort","c":"CONTROL","why":"The Opus choice in the directs-your-team row (`#md-stu .seg-opt[data-v=opus]`, role=radio). Work retains high effort."},
  {"s":"talks with you","c":"FRAGMENT","why":"Round 23 (§119): the bold part of the row label \"When he talks with you\" (`#md-rich-who`), which also names the `#md-rich` radio group. Never renders alone."},
  {"s":"directs your team","c":"FRAGMENT","why":"Round 23 (§119): the bold part of the row label \"When he directs your team\" (`#md-stu-who`), which also names the `#md-stu` radio group. Never renders alone."},
  {"s":"Rich now talks with you on Fable.","c":"INFORMATIONAL","why":"Round 23 (§119): MODEL_TOAST rich.fable, the toast after Preferences closes and `set_lease_models` succeeds. It reports the saved change; Preferences is already closed and nothing is asked."},
  {"s":"Rich talks with you on Opus again, the default.","c":"INFORMATIONAL","why":"Round 23 (§119): MODEL_TOAST rich.opus, the toast after a successful save back to the default. It reports what is now true and asks nothing."},
  {"s":"Rich now directs your team on Fable.","c":"INFORMATIONAL","why":"Round 23 (§119): MODEL_TOAST stu.fable, the toast after a successful save. It reports what is now true and asks nothing."},
  {"s":"Rich directs your team on Opus at high effort again, the default.","c":"INFORMATIONAL","why":"Round 23 (§119): MODEL_TOAST stu.opus, the toast after a successful save back to the default. It reports what is now true and asks nothing."},
  {"s":"Rich's model could not be saved, so nothing changed.","c":"ACTIONABLE","control":"#set-btn","fixture":null,"why":"Round 23 (§119): the toast when `set_lease_models` refuses after Settings closes (`commitModelChoice`'s catch). He can act on it: the Settings button, on screen in every view this toast can appear in, reopens the one Settings panel (round 27) with the rows put back to the saved choice, and closing it after picking again sends the save again."},
  {"s":"unknown model: {rich}","c":"NOT-RENDERED","why":"Round 23 (§119): `set_lease_models`'s refusal for a name `ModelChoice::parse` does not know. The UI only ever sends \"opus\" or \"fable\" (`modelValue`), and `commitModelChoice`'s catch discards the error text and shows its own sentence, so this string never reaches the DOM."},
  {"s":"unknown model: {stu}","c":"NOT-RENDERED","why":"Round 23 (§119): the same refusal for the directs-your-team row's name. Never reaches the DOM for the same reason: the UI sends only \"opus\" or \"fable\" and the catch shows its own sentence."},
  {"s":"the server answered {}","c":"NOT-RENDERED","why":"Rich's Pocket TTS voice download: `fetch_rich_voice_file`'s refusal for a non-success HTTP status (voice_provision.rs:602). Its only caller is `fetch_rich_voice`, whose only caller is the background thread in `fetch_rich_voice_in_background`; every error ends in `eprintln!` (:577 per attempt, :554 at the end) and none is returned to the webview. Until the files verify, Rich keeps speaking with `say`, so nothing is shown and nothing is asked."},
  {"s":"more than the pinned {} bytes arrived","c":"NOT-RENDERED","why":"The same Pocket TTS voice download: `fetch_rich_voice_file`'s refusal when a file outgrows its pinned size (voice_provision.rs:614). Same single background path, same `eprintln!` at :577 and :554, never returned to the webview."},
  {"s":"create the speech synthesis folder: {e}","c":"NOT-RENDERED","why":"The same Pocket TTS voice download: `fetch_rich_voice`'s refusal when the voice folder cannot be created (voice_provision.rs:574). Its only caller is the background thread in `fetch_rich_voice_in_background` (:554), which writes the error with `eprintln!` (:559) and returns nothing to the webview."},
  {"s":"textarea, [contenteditable=\"\"], [contenteditable=\"true\"], [contenteditable=\"plaintext-only\"], input:not([type]), input[type=\"text\"], input[type=\"search\"], input[type=\"url\"], input[type=\"tel\"], input[type=\"email\"], input[type=\"password\"], input[type=\"number\"]","c":"NOT-RENDERED","why":"A CSS selector list, not text anyone reads: `TYPED_INTO` in no-native-menu.js:18 (the right-click fix), two string literals joined by `+` and passed to `closest()` to decide which elements are text fields. Never drawn, never shown to a person."},
  {"s":"file {n} of {total}: {last}","c":"NOT-RENDERED","why":"The same Pocket TTS voice download: `fetch_rich_voice`'s refusal after a file's last attempt failed (voice_provision.rs:595). Same single background path, same `eprintln!` at :559, never returned to the webview."},
  {"s":"<span class=\"acc-pill is-resting\"><i></i>Sign-in not confirmed</span>","c":"INFORMATIONAL","why":"Authentication and usage readings are separate facts. This line states only what the last check established, without asserting an unknown account is signed out or lacks limits."},
  {"s":"Ask Claude Code checks the usage figures again.","c":"INFORMATIONAL","why":"Authentication and usage readings are separate facts. This line states only what the last check established, without asserting an unknown account is signed out or lacks limits."},
  {"s":"Close this panel, choose Claude accounts and sign in, then refresh.","c":"ACTIONABLE","control":"#quota-close","fixture":"quota-signed-out","why":"The Technical view names the actual sign-in route. Its own Close control returns to Settings, where Claude accounts opens sign-in."},
  {"s":"No usage reading yet.","c":"INFORMATIONAL","why":"Authentication and usage readings are separate facts. This line states only what the last check established, without asserting an unknown account is signed out or lacks limits."},
  {"s":"Not signed in on this Mac.","c":"INFORMATIONAL","why":"Authentication and usage readings are separate facts. This line states only what the last check established, without asserting an unknown account is signed out or lacks limits."},
  {"s":"RichOS has not confirmed whether this account is signed in.","c":"INFORMATIONAL","why":"Authentication and usage readings are separate facts. This line states only what the last check established, without asserting an unknown account is signed out or lacks limits."},
  {"s":"Signed in; no usage reading.","c":"INFORMATIONAL","why":"Authentication and usage readings are separate facts. This line states only what the last check established, without asserting an unknown account is signed out or lacks limits."},
  {"s":"This account is not signed in on this Mac.","c":"INFORMATIONAL","why":"Authentication and usage readings are separate facts. This line states only what the last check established, without asserting an unknown account is signed out or lacks limits."},
  {"s":"This account is signed in on this Mac.","c":"INFORMATIONAL","why":"Authentication and usage readings are separate facts. This line states only what the last check established, without asserting an unknown account is signed out or lacks limits."},
  {"s":"Claude Code did not answer just now.","c":"ACTIONABLE","control":"#quota-refresh","fixture":null,"why":"The usage-reader verdict is rendered by the Technical quota view. quota.js tests cover the refresh control beside failed and absent readings. The account card independently presents the sign-in verdict."},
  {"s":"Claude Code did not report subscription limits for this account.","c":"ACTIONABLE","control":"#quota-refresh","fixture":null,"why":"The usage-reader verdict is rendered by the Technical quota view. quota.js tests cover the refresh control beside failed and absent readings. The account card independently presents the sign-in verdict."},
  {"s":"Claude Code returned quota data this version of RichOS could not read.","c":"ACTIONABLE","control":"#quota-refresh","fixture":null,"why":"The usage-reader verdict is rendered by the Technical quota view. quota.js tests cover the refresh control beside failed and absent readings. The account card independently presents the sign-in verdict."},
  {"s": "This Claude account is not signed in on this Mac.", "c": "ACTIONABLE", "control": "#quota-close", "fixture": "quota-signed-out", "why": "The Technical quota view explains the sign-out and names the route through its own Close control to Claude accounts. The signed-out fixture verifies that Close is present beside the verdict."},
];
