"use strict";
// USE RICH FROM YOUR PHONE — the Mac's half of pairing (plan §4.1), and it has ONE path.
//
// **CEO §61, 2026-09-18, and it is the definition the whole of this file is built to:** *"the new
// working definition for a mobile app or PWA is this: it's an app that lets the user use RichOS
// (in some way) while being on the go and away from office i.e. outside the home network. Because
// any mobile app or PWA is utterly useless within the home network. The desktop app is a much
// better tool in that case. … The RichOS desktop app will guide technical users (with the help of
// crystal-clear and ultra-simple how-to screens in the app) to their Tailscale setup. That's it.
// The Tailscale setup is where we start now."*
//
// **SO THERE IS NO QUESTION TO ASK AND NO SECOND DOOR.** Until 2026-09-19 this screen opened on
// *"Where do you want to use it?"* with `Anywhere, including away from home` and `At home only`,
// and the second option led to a self-signed certificate, a trust code at
// `http://<name>.local:8444/ca` and sixteen taps through Apple's own settings. §61 does not keep
// that as a lesser path; it calls the thing it does *useless*. What replaces the chooser is the
// flow that was already behind the first option: the identity warning (§61.1), the Tailscale
// how-to screens, `This Mac is ready`, the code, the paired card. A reader who wants to use Rich
// on the sofa has a much better tool for it already open in front of him.
//
// **WHAT IS NOT REMOVED, because it was never about the home path:** the six words, the five-minute
// window and its announced expiry, the code sitting above the steps (Urban's G3), the paired
// card's heading and push line (G8), the account named in bold (G14), `floor` on the countdown
// (G13), the scroll that lands on the code (N2, G4), and the one control on the pairing screen
// that actually puts something down (G2). Each of those is a fix to a screen that still exists.
//
// RichOS Connect adds managed remote access while preserving the existing Tailscale route.
// Both routes use the same Mac pairing authority and phone application actions.
//
// NO NEW COLORS. Every piece of text here uses a class the shipped shell already paints and the
// contrast suite already walks. The only new visual is the QR canvas, which paints its own white
// quiet zone and black modules — 21:1, and the quiet zone is part of the code rather than
// decoration: a code drawn flush to a dark page background does not scan.
(function () {
  const bridge = window.RichBridge;
  const qr = window.RichOSQr;

  const sheet = document.createElement("div");
  sheet.id = "phone-sheet";
  sheet.className = "overlay";
  sheet.hidden = true;
  sheet.setAttribute("role", "dialog");
  sheet.setAttribute("aria-labelledby", "phone-title");
  sheet.setAttribute("data-dismiss", "control:#phone-close");
  sheet.innerHTML = `<div class="overlay-panel">
    <!-- THE BOX THAT SCROLLS, AND IT IS NOT THE PANEL ANY MORE.
         The panel is a flex column of exactly two children: this, which scrolls and the action
         row below, which does not. See .phone-scroll and .phone-actions in style.css for the
         measurements that forced the split and for why a sticky footer over one scrollport was
         tried first and rejected. Everything the sheet says lives in here; the way out lives
         underneath it and never moves. -->
    <div id="phone-scroll" class="phone-scroll">
    <h2 id="phone-title" class="overlay-title">Use Rich from your phone</h2>
    <!-- ONE LEAD, TRUE OF THE ONE PATH. It was a fixed sentence describing the option he had
         not chosen: it stayed on screen unchanged after he chose the other one, so every screen
         opened by telling him the opposite of where he was (Ray's candidate .11 defect 3.1,
         screenshots 13, 14, 15 and 17). The fix then was to key it off the route; CEO 61
         removed the route, so the key went with it. Filled from LEAD below. -->
    <p class="overlay-note" id="phone-lead"></p>
    <div id="phone-route-choice" hidden class="desk-card-actions">
      <button id="phone-use-connect" type="button" class="desk-btn">RichConnect for RichOS</button>
      <button id="phone-use-tailnet" type="button" class="desk-btn">Use Tailscale</button>
    </div>
    <div id="phone-connect" hidden>
      <h3 class="phone-step-title">RichConnect for RichOS</h3>
      <p class="overlay-note">Reach this Mac from your phone without setting up a VPN. Your Mac must stay awake with RichOS running.</p>
      <p class="overlay-note">Connections are encrypted through Cloudflare. Cloudflare can process the traffic; conversations are stored on your devices.</p>
      <p id="phone-connect-status" class="overlay-note" role="status"></p>
      <div class="desk-card-actions">
        <button id="phone-connect-start" type="button" class="desk-btn">Set up RichConnect for RichOS</button>
        <button id="phone-connect-disable" type="button" class="desk-btn" hidden>Turn off RichConnect for RichOS</button>
      </div>
    </div>


    <!-- SCREEN 6 — THE PAIRED CARD, AND IT NOW HAS A TOP — Urban's G8.
         *"Paired card has no heading and one text color for all five paragraphs — the actionable
         line reads like housekeeping."* Every line on it was --ink-soft at one size and one
         weight, labels and paragraphs alike: he measured "Forget this phone" and "Close" at
         5.78:1, identical to the four paragraphs above them (Ray's frames 21 and 22). A card
         where nothing is the top and nothing is the point.

         So the phone's own name is the heading — it is what the card is ABOUT and it is the one
         string on it that changes — and the push line, which is the only line that asks the
         reader to go and do something, is the one that takes --ink. -->
    <div id="phone-paired" hidden>
      <!-- ROUND 49: THE PHONE'S NAME, AND ITS CONNECTION BESIDE IT. The chip is a status, not a
           second button: no hover, no cursor, no focus stop. Its dot is the non-text indicator
           and its word is text in --ink. Hidden while the pairing itself is still a question. -->
      <div class="phone-card-head">
        <h3 class="phone-step-title" id="phone-device-name"></h3>
        <span class="phone-chip" id="phone-chip" role="status" hidden><span class="phone-chip-dot" aria-hidden="true"></span><span id="phone-chip-word"></span></span>
      </div>
      <!-- THE MAC DOES NOT SETTLE A QUESTION THE PERSON HAS NOT ANSWERED YET. This line was the
           fixed string "It is paired. Open Rich on it and keep talking." and it was on screen
           while the phone, beside it, was still asking "They match — pair this phone" /
           "They do not match" (Ray's nightly .7, defect 2, his frame 13). The six words exist so
           a person can detect that something other than his Mac answered; a Mac that announces
           the answer first teaches him the check is ceremonial. It is filled from
           status.fingerprintConfirmed — the Mac's own record, never anything this sheet
           remembers.
           NO BACKTICK IN THIS FILE'S MARKUP, EVER: it is one template literal, so a backtick in
           a comment ends the string and takes the whole sheet with it. -->
      <p class="overlay-note" id="phone-paired-state"></p>
      <!-- AND THE WORDS THEMSELVES, WHILE HE IS BEING ASKED ABOUT THEM. The pairing screen that
           carries them is hidden the instant "paired" flips, which is the same instant the phone
           starts asking him to compare — so the Mac took its half of the comparison off the
           screen at exactly the moment he needed it. They go away once he has answered: a
           fingerprint nobody is checking is chrome. -->
      <p class="phone-words" id="phone-paired-words" hidden></p>
      <!-- THE PRESS ON THIS MAC, AND IT IS THE ONLY THING THAT LETS THE PHONE IN — Sage's pairing
           review F1 (High), richos-hq docs/research/2026-09-24-richconnect-pairing-protocol-review.md.
           The phone's own "They match" was the only confirmation the Mac ever recorded and the
           device being judged performed the judgment: anybody who saw the code on this screen
           could pair first and confirm themselves. Until one of these two is pressed the phone's
           key can answer the six words and do nothing else. "They do not match" is the same
           teardown the phone's own answer runs. Both are kept out of the sheet's first focus
           (data-no-autofocus): a pairing is not something a reflexive Return may settle. -->
      <div class="desk-card-actions" id="phone-mac-answer" hidden>
        <button id="phone-mac-match" class="desk-btn desk-btn--confirm" type="button" data-no-autofocus>They match</button>
        <button id="phone-mac-mismatch" class="desk-btn" type="button" data-no-autofocus>They do not match</button>
      </div>
      <!-- THE SPENT-CODE ALARM, AFTER HE HAD ALREADY CONFIRMED (Sage F1 item 4, "keep it and
           warn"): the phone stays and he is told somebody else used its code. -->
      <p class="overlay-note" id="phone-code-reused" role="status" hidden></p>
      <!-- THE ONE LINE ON THIS CARD WITH AN ERRAND IN IT, in --ink, with the icon that says which
           kind (round 49): the bell when the phone can reach him, the struck bell or the phone
           when something is still to do on it, the circular arrow when there is nothing to do. -->
      <p class="overlay-note phone-push-line" id="phone-push-state"><span class="phone-push-icon" id="phone-push-icon" aria-hidden="true"></span><span id="phone-push-text"></span></p>
      <!-- SCREEN 6's LIMIT, on the one screen where they live with it. The route chooser used
           to carry it too, on the option it was a condition of; that screen is gone (CEO 61), so
           this is the one place it is said — and it is not repeated on the screens in between,
           because a limitation on every screen is nagging. -->
      <p class="overlay-note" id="phone-ts-limit" hidden></p>
      <!-- ONE FORGET NOTE, FILLED FROM THE DEVICE RECORD. It was two paragraphs, one shown and
           one hidden, chosen by the sheet's own route variable — which is forgotten on every
           open by design, so reopening this card for a paired phone redrew the other path's iOS
           profile-removal steps. Ray's candidate .11 defect 3.2 caught it on an ANDROID phone
           that had paired over Tailscale and had nothing installed on it at all. One node,
           filled from status.pairedVia, is the only shape in which the two cannot disagree —
           see forgetNoteFor below. -->
      <!-- ROUND 49: the divider between the phone's own lines and the forget note, so the errand
           is not read as part of the housekeeping under it. Decoration, as the app's dividers are. -->
      <hr class="phone-card-rule" id="phone-card-rule">
      <p class="overlay-note" id="phone-forget-note"></p>
      <div class="desk-card-actions">
        <button id="phone-forget" class="desk-btn" type="button">Forget this phone</button>
      </div>
    </div>

    <!-- SCREEN 5 — THE CODE, AND THEN THE FOUR THINGS TO DO ON THE PHONE.
         There is no second version of this screen any more. It used to carry, below the code, two
         unverified-certificate warnings, a trust QR and sixteen numbered taps through Apple's
         settings — the home path's half — hidden on the Tailscale route and shown on the other
         one. CEO §61 leaves one route, so those are gone from the product rather than hidden
         behind a condition and with them the whole class of defect where the wrong half was
         drawn (Urban's blocker "esc-20260919T041857Z-640acd39", Ray's candidate .11 defect 3.1).

         THE CODE IS FIRST — Urban's G3 and G4. At 1024x700, the app's own minimum and the size it
         restores itself to, this screen is about three viewport heights tall and pressing "Set my
         phone up" used to land on a wall of instruction with the QR, the six words, the countdown
         and the only Close all out of sight (his frames 04 and 05). *"The four phone steps are
         preparation for a person who has not started; the code is what a person who is standing
         there with their phone needs. Order the screen for the second person."*

         IT IS THE MARKUP'S OWN ORDER NOW, not a node moved at render time. The mover existed
         because the other path numbered its headings 1, 2, 3 and the numbers were load-bearing —
         the certificate at step 1 was what made the address at step 2 open at all — so the code
         could not go above it there. With one path there is no sequence to contradict and a
         subtree that is re-inserted on every two-second poll is a subtree that can take the focus
         out of itself. -->
    <div id="phone-pairing" hidden>
      <!-- Sage's F9: Android has its own RichOS app now (CEO 76), so the line no longer sends
           Android to the web app. -->
      <p id="phone-connect-pair-help" class="overlay-note" hidden>In the RichOS app on your phone, scan this code. Then compare the six words on both screens.</p>
      <!-- IT IS HIDDEN AS A WHOLE WHEN THERE IS NO CODE. Every one of its children was already
           emptied on expiry (Ray's defect 3.3 and his defect B for the last of them); with the
           block at the top of the screen an empty wrapper is a gap where the thing he came for
           used to be, so the container goes with its contents. -->
      <div id="phone-code-block">
      <h3 class="phone-step-title" id="phone-code-title">Point your phone's camera at this</h3>
      <div class="phone-qr-row">
        <canvas id="phone-qr-pair" class="phone-qr" width="1" height="1" role="img"></canvas>
        <div>
          <p class="overlay-note phone-url" id="phone-pair-url"></p>
          <p class="overlay-note" id="phone-countdown" role="status"></p>
        </div>
      </div>

      <h3 class="phone-step-title" id="phone-words-title">Check the six words match</h3>
      <!-- IT GOES WITH THE CODE, AND IT NAMES A CONTROL THAT IS ON THE SCREEN. Ray's
           candidate-.12 defect B: after the code ran out this paragraph stayed, telling him to
           compare against "these six" with no six words under it and to "tap Cancel" with no
           Cancel button anywhere in the sheet (frame 19). Two things a person is asked to do
           that are not there. The heading above it and the words below it were already cleared
           with the code; this one was missed, so it is now hidden by the same live test they
           are — and the control it names is Close, which is the button this sheet actually has
           (id phone-close and the sheet's own data-dismiss). -->
      <!-- AND THE WORDS ARE NOT HERE ANY MORE — Sage's review 3.1 steps 1 and 4. They used to be
           printed beside the code before any phone had arrived, which made them carry nothing
           the code itself does not: anybody who could see the code could see the words. They are
           now derived over the key the phone actually registered, so they can only exist once a
           phone has reached this Mac and they appear on the paired card beside the two buttons
           that answer them. -->
      <p class="overlay-note" id="phone-words-note">Once your phone opens this code, six words
        appear on it and on this Mac. If they are the same six, press They match here. If they are
        not, press They do not match.</p>
      </div>

      <!-- THESE FOUR STEPS ARE THE CEO'S OWN, AND THEY OVERRIDE URBAN'S THREE.
           He installed and uninstalled Tailscale on Android THREE TIMES looking for a "connect to
           Mac" step that does not exist. Urban's Screen 5 had three steps and stopped at "sign in",
           which is where that hunt begins: the app is signed in, nothing says it is finished and
           the user goes looking for the pairing screen Tailscale does not have. So the two missing
           steps — the VPN permission prompt and the switch reading Connected — are what end the
           hunt and the sentence below says outright that the thing he was hunting for is not
           there. Recorded in docs/verification/tailscale-path-2026-09-18.md.

           NOTHING ABOUT TAILNETS, MagicDNS OR MACHINE NAMES ON THIS SCREEN, by the same ruling.
           Those words belong on the Mac's own screens; here they are vocabulary for a thing the
           user does not have to think about. -->
      <div id="phone-ts-steps">
        <h3 class="phone-step-title">On your phone, four things</h3>
        <!-- THE WHY COMES BEFORE THE STEPS and it is here because the CEO said it is not obvious:
             *"in hindsight this sounds obvious, but it's absolutely NOT obvious at all. Especially
             given that I would normally absolutely NEVER use the same identity on the Mac and on
             the phone."* A person whose habit is to keep two identities apart will follow a step
             that says "sign in" and use the WRONG one, correctly by their own lights and the
             result is two networks that never see each other with nothing on either screen saying
             why. So the reason is given before the instruction and it names the habit it is
             asking them to break. -->
        <p class="overlay-note" id="phone-ts-why"></p>
        <ol class="phone-steps">
          <li class="overlay-note">Install Tailscale from the store.</li>
          <li class="overlay-note" id="phone-ts-step2">Sign in with the <strong>same</strong>
            identity you used on this Mac. A different provider makes a different network and then
            the two will never see each other.</li>
          <li class="overlay-note">Allow the VPN connection your phone asks about.</li>
          <li class="overlay-note">The switch says Connected.</li>
        </ol>
        <p class="overlay-note phone-url" id="phone-ts-store"></p>
        <p class="overlay-note"><strong>There is no pairing step in Tailscale.</strong> Sign in with
          the same account on both devices and they are connected.</p>
        <!-- THE MISMATCH IS DETECTED, NOT ONLY DESCRIBED — the CEO puts it at 9 in 10 users. A
             phone on the SAME account shows up in the daemon's own Peer list; one on a different
             provider's account is in a different tailnet and shows up nowhere. So this line is
             evidence rather than advice and it names the account to use. -->
        <p class="overlay-note" id="phone-ts-peer" role="status"></p>
        <!-- THE TRIPWIRE, AND IT IS NOW THE ONLY SENTENCE IN THE PRODUCT ABOUT INSTALLING
             ANYTHING ON A PHONE. Nothing this Mac serves asks for a profile any more, so a phone
             that is asked for one is on an origin that is not this Mac's — and the only person
             who can notice that is the person holding the phone. -->
        <p class="overlay-note">Nothing has to be installed on your phone for this. If your phone
          asks you to install a profile, something is wrong. Tell me.</p>
      </div>

      <!-- THE ONE FAILURE THIS PATH ACTUALLY DIES OF, named where it happens. Sage's §2.3 failure
           mode is that the name simply stops resolving; from the phone that looks like "cannot
           connect" with nothing saying why and two different Tailscale accounts produce exactly
           that, silently. -->
      <p class="overlay-note" id="phone-ts-failure" hidden>If the code opens to a page that cannot
        connect, your phone is signed in to a different Tailscale account than this Mac.</p>

      <!-- WHAT HAPPENS WHEN THE CODE RUNS OUT, ON THE SCREEN IT RAN OUT ON. It used to happen
           in silence: the Mac dropped the window, the next poll returned no code and the
           dialog fell back to an earlier screen with the QR, the code and the six words simply
           gone. Ray's candidate .11 defect 3.3 — "he comes back to a screen that looks like he
           imagined the whole thing". The screen stays; the code is replaced by this. -->
      <div id="phone-expired" hidden>
        <h3 class="phone-step-title">That code ran out</h3>
        <p class="overlay-note" id="phone-expired-note" role="status"></p>
      </div>
      <!-- THE WAY BACK OUT, ON THE ONE SCREEN THAT DID NOT HAVE ONE — Urban's G2.
           *"Once Set my phone up is pressed, the route chooser is unreachable for the life of the
           app process"* — the expired state sets "pairing" as well, so waiting did not give it
           back either and he could not re-reach "This Mac is ready" in dark at all after his
           walk.

           **THE CONTROL SURVIVES ITS OWN NAME.** It used to read "Pick a different way", which
           named the chooser it returned to; with one path there is no different way to pick and
           the half of this button that was never about the chooser is the half that matters — it
           STOPS SERVING. It undoes a socket and a live code, which is why it calls
           phone_stop_pairing rather than only redrawing and the screen it returns to is
           "This Mac is ready". See PhoneRuntime::stop_pairing for why that is not phone_forget.

           The four plain copies of this control — on the identity screen, on the two waiting
           screens and on "This Mac is ready" — are gone with the chooser they led to. Nothing was
           behind them to put down and a button that returns to the screen you are already on is
           a button that does nothing. Close is on every screen and always was.

           **AND THE TWO OF THEM NOW LIVE IN THE FOOTER, NOT HERE** — Ray's candidate-.16 new
           defect. Measured in this harness at 1024x700, dark, with a code live: the panel is
           592px tall in a 700px window and its content is 1196px, so on a REOPENED sheet the
           three controls sat at window y 1040..1116 — "Show me another code", "Stop and go back"
           and "Close", none of them on screen, under sixteen steps of how-to. Ray read the same
           thing off AX on the real app: y=1175..1364 against a window bottom of y=792.
           See .phone-actions in style.css for the pin. -->
    </div>

    <!-- SCREEN 0 — THE IDENTITY TRAP, AND IT COMES BEFORE THE DOWNLOAD.
         CEO §61.1, his ruling in his own words: *"this barrier or I would call it stupidity would
         need to be made ABSOLUTELY UBER MEGA SUPER CRYSTAL-CLEAR to ever user of RichOS"*. What he
         hit: Tailscale has no email-and-password sign-in, only "Sign in with Apple / Google /
         Microsoft / GitHub"; the account IS the network; he signed in with Apple on the Mac and
         Google on the Android, got two networks that cannot see each other and REINSTALLED
         Tailscale on the Android three times looking for a "connect to Mac" step that does not
         exist. Nothing on Tailscale's screens says any of this.

         **WHY IT IS ITS OWN SCREEN AND NOT A LINE ON THE DOWNLOAD SCREEN.** By the time somebody
         is on the download screen the next thing they do is create the account and the choice of
         identity is made inside somebody else's sign-in sheet where nothing of ours can reach
         them. A warning that arrives after that choice is a warning that costs a sign-out. His
         own sentence is the reason it cannot be a footnote: *"in hindsight this sounds obvious,
         but it's absolutely NOT obvious at all. Especially given that I would normally absolutely
         NEVER use the same identity on the Mac and on the phone."*

         It is shown while the Mac has no Tailscale account yet — which is exactly the window in
         which the choice can still be made freely — and never again after detection can name one,
         because from then on the screens say WHICH account rather than warning about the choice. -->
    <div id="phone-identity" hidden>
      <h3 class="phone-step-title">First: Tailscale has no password</h3>
      <p class="overlay-note">Tailscale has no username and password. It only offers
        <strong>Sign in with Google</strong>, <strong>Apple</strong>, <strong>Microsoft</strong> or
        <strong>GitHub</strong> and whichever one you pick, that identity <strong>is</strong> your
        private network.</p>
      <p class="overlay-note">So this Mac and your phone have to sign in with the
        <strong>same</strong> one. Two different identities make two separate networks that cannot
        see each other and neither device says so: the phone simply never finds this Mac.</p>
      <p class="overlay-note">There is no "connect to my Mac" step in Tailscale, on either device.
        Signing in on both with the same identity is the whole connection. If the phone cannot find
        this Mac, reinstalling Tailscale will not help: the identity is the only thing that
        decides it.</p>
      <p class="overlay-note">If you would rather not use a personal identity on both devices, make
        one that is only for this: a new Google account, which costs nothing and sign in with
        that one here and on the phone.</p>
      <div class="desk-card-actions">
        <button id="phone-identity-ok" class="desk-btn desk-btn--confirm" type="button">I understand, show me what to do</button>
      </div>
    </div>

    <!-- SCREENS 2, 3 and 7 — the three "something is happening elsewhere" states. One block,
         because they differ only in their words and their one address: each is a thing the user
         does in somebody else's software and detection is what moves the screen on. -->
    <div id="phone-ts-wait" hidden>
      <h3 class="phone-step-title" id="phone-ts-heading"></h3>
      <p class="overlay-note" id="phone-ts-note1"></p>
      <p class="overlay-note" id="phone-ts-note2"></p>
      <p class="overlay-note phone-url" id="phone-ts-url"></p>
      <div class="desk-card-actions">
        <button id="phone-ts-open" class="desk-btn desk-btn--confirm" type="button"></button>
        <button id="phone-ts-recheck" class="desk-btn" type="button">Check again</button>
      </div>
    </div>

    <!-- SCREEN 4 — THIS MAC IS READY. No separate "turn serving on": once the name is known there
         is no decision left, so "Set my phone up" does both, under the label the product already
         uses. -->
    <div id="phone-ts-ready" hidden>
      <h3 class="phone-step-title">This Mac is ready</h3>
      <!-- WHICH IDENTITY IT SIGNED IN WITH, READ OFF THE MAC (§61.1). "Use the same account" is
           advice nobody can follow, because the one thing the user does not know is which one they
           used — that is the CEO's own account of the evening. This names it.

           **FIRST, AND IN BOLD** — Urban's G14. It was the third paragraph, under the machine
           name, set in --ink-soft with the account given no weight at all and visually identical
           to the paragraphs either side of it (his frame 02). Two screens later the same account
           IS set in <strong> (his frame 04). *"That asymmetry is backwards: the screen that names
           the account first is the one that whispers it ... it is the thing that decides whether
           this works and the machine name is not."*

           AND THE PARAGRAPH BELOW STILL READS RIGHT, which is why the account went ABOVE the
           name rather than the name below it: "That is this Mac's name on your own Tailscale
           network" points at the line before it, so the name has to stay immediately in front
           of it. Heading, account, name, what the name is — each sentence next to the thing it
           is about. -->
      <p class="overlay-note" id="phone-ts-account"></p>
      <p class="phone-tailnet-name" id="phone-ts-name"></p>
      <p class="overlay-note">That is this Mac's name on your own Tailscale network. Only devices
        signed in to your Tailscale account can reach it and no port on this Mac is open to the
        internet.</p>
      <!-- AND THE WATCH LIVES HERE TOO, not only on the phone step. MEASURED: the pairing window
           is 60 s (PAIRING_WINDOW_MS) and the join grace is 60 s and the watch starts strictly
           AFTER the window opens — so on the phone step alone the mismatch sentence could never
           be reached at all: by the time it was due, the code had expired and the block was
           hidden. Installing Tailscale on a phone and signing in takes minutes, not seconds, so
           this screen — the one the user is returned to when the code runs out — is where the
           sentence has to be able to appear. -->
      <p class="overlay-note" id="phone-ts-peer-ready" role="status"></p>
      <div class="desk-card-actions">
        <button id="phone-ts-start" class="desk-btn desk-btn--confirm" type="button">Set my phone up</button>
      </div>
    </div>

    <!-- SCREEN 8 — THE ALARM BUTTON, ANSWERED. Ray's nightly .8 walk in the test VM, defect 1
         (HIGH): pressing "They do not match" on the phone dropped the credential and the sheet
         in front of the person did not change at all. The Mac went on showing the pairing card,
         with the six words under it, while it had already forgotten the phone — and, until the
         commit this screen arrives in, while it was still answering on 8443. *"He has no way to
         know the Mac heard him."*

         THE SIX WORDS EXIST FOR EXACTLY ONE SCENARIO: something other than his Mac on the other
         end. A Mac that says nothing when the alarm is pressed teaches him the check is
         ceremonial, which is worse than not asking at all.

         IT OUTRANKS EVERY OTHER SCREEN while it is true — see render(). What is true at that
         moment is not "This Mac is ready", even though the tailnet is; the ready screen would
         be the sheet forgetting the thing that just happened.

         AND IT HAS THE ONE CONTROL THAT LEAVES IT, which is also the only thing left to do:
         start again, from a Mac he now trusts. Close is in the footer as it is on every screen.
         The Mac cannot clear this by itself — nothing else changes it, so nothing else may. -->
    <div id="phone-rejected" hidden>
      <h3 class="phone-step-title">I stopped</h3>
      <p class="overlay-note" id="phone-rejected-note" role="status"></p>
      <div class="desk-card-actions">
        <button id="phone-rejected-again" class="desk-btn desk-btn--confirm" type="button">Set my phone up again</button>
      </div>
    </div>

    <!-- WHAT THE MAC IS DOING, OR WHY IT COULD NOT — OUTSIDE EVERY STATE BLOCK, WHICH IS A
         MOVE THIS CHANGE FORCED.

         It lived inside "#phone-pairing", which is hidden on five of the six screens. That was
         survivable while "Set my phone up" was pressed on a screen whose own failure mode was
         a certificate this Mac makes for itself and can hardly fail to make. It is not
         survivable now: with one path, "phone_begin_pairing" fails whenever Tailscale will not
         issue a certificate for this Mac and that failure is read on "This Mac is ready" — a
         screen that would have shown the sentence to nobody. "Making a certificate for your
         phone…" had the same problem pointing the other way and so did the sentence
         "Get Tailscale" shows when the Mac cannot open a link.

         The SOCKET DUMP that used to sit beside it is still gone — Urban's G5, *"the one thing
         on this flow that looks like a debug log that shipped ... deletion is the change I want
         most on this screen"*. The addresses are still printed to the log at every start, which
         is where a diagnosis is read from. -->
    <p class="overlay-note" id="phone-message" role="status"></p>
    </div>

    <!-- ONE CLOSE, OUTSIDE THE THREE STATES AND ALWAYS VISIBLE.
         The first version of this sheet had three, one inside each state block and
         ui/tests/escape.js refused it on the spot: data-dismiss names ONE control, the
         document-level Escape handler clicks it and a button hidden with its block does
         nothing at all. Two of the three states could not be dismissed from the keyboard. One
         button that is always on screen is both the simpler markup and the only shape that can
         satisfy the contract.

         **"ALWAYS VISIBLE" WAS A CLAIM ABOUT THE MARKUP AND NOT ABOUT THE SCREEN, AND A PERSON
         FOUND THE DIFFERENCE.** One Close outside the three states is what makes it always
         RENDERED; it is .phone-actions below that makes it always SEEN. With a code live at
         1024x700 the panel's content is 1196px in a 590px scrollport and this row sat 445px
         below the window's bottom edge while the six-words note three screens above it read
         *"press Close and tell me"*. The row is now pinned to the bottom of the scrollport
         with position:sticky, so the how-to scrolls under it and the way out does not leave.

         AND THE PAIRING SCREEN'S TWO CONTROLS JOIN IT, for the same reason and in one row: two
         separately-pinned rows would overlap. They are hidden with the screen they belong to —
         by their own hidden attribute, set in render() from the same condition that shows the
         pairing block, because a display:none button contributes no flex gap and the row
         collapses to Close alone. Close stays LAST, which is where the copy points. -->
    <div class="desk-card-actions phone-actions">
      <button id="phone-refresh" class="desk-btn desk-btn--confirm" type="button" hidden>Show me another code</button>
      <button id="phone-pairing-back" class="desk-btn" type="button" hidden>Stop and go back</button>
      <button id="phone-close" class="desk-btn" type="button">Close</button>
    </div>
  </div>`;
  document.body.appendChild(sheet);

  const field = (id) => sheet.querySelector("#" + id);

  /// **THE ONE BOX ON THIS SHEET THAT SCROLLS.** It used to be `.overlay-panel` itself; since the
  /// footer was taken out of the scroll (Ray's candidate-.16 defect) the panel is a flex column
  /// with `overflow-y: hidden` and this is the box inside it. Named once here so `open()` and
  /// `scrollToCode()` cannot end up aimed at different boxes.
  const scrollBox = () => field("phone-scroll");

  /// Draw a QR matrix into a canvas: black modules, white background, and the four-module quiet
  /// zone the standard requires. The white is painted rather than inherited on purpose — the quiet
  /// zone is part of the symbol, and a code flush to a dark page does not scan.
  function paint(canvas, text) {
    if (!text) {
      canvas.hidden = true;
      return;
    }
    let matrix;
    try {
      matrix = qr.encode(text);
    } catch (error) {
      // A URL too long for a version-6 symbol. Saying so beats drawing a code that does not scan.
      canvas.hidden = true;
      field("phone-message").textContent = String(error && error.message ? error.message : error);
      return;
    }
    const quiet = 4;
    const scale = 5;
    const side = (matrix.size + quiet * 2) * scale;
    canvas.hidden = false;
    canvas.width = side;
    canvas.height = side;
    canvas.setAttribute("aria-label", "A code for your phone's camera. The address is written out beside it.");
    const paper = canvas.getContext("2d");
    paper.fillStyle = "#ffffff";
    paper.fillRect(0, 0, side, side);
    paper.fillStyle = "#000000";
    for (let row = 0; row < matrix.size; row++) {
      for (let column = 0; column < matrix.size; column++) {
        if (!matrix.modules[row][column]) continue;
        paper.fillRect((column + quiet) * scale, (row + quiet) * scale, scale, scale);
      }
    }
  }

  /// **THE PAIRED CARD, ROUND 49** (CEO, 2026-10-31: round 49 "OK"; the design is
  /// richos-hq design/mockups/rounds/round-49, its README "The states"). The phone web page is
  /// retired (CEO, 2026-10-30), so a phone reaches Rich only through the RichConnect app, on
  /// iPhone and Android alike. Both apps name the same controls (Settings, then the switch
  /// "Notify me when Rich replies"), so ONE set of sentences serves both phones and nothing on
  /// the card names a browser, the Home Screen or a web page. Each state is the chip beside the
  /// phone's name, the one pairing sentence, and the one errand line with the icon that says
  /// which kind. The errand lines carry markup (the phone's control names in strong) and are
  /// constants written here, never data.
  const CARD_ICONS = {
    bell: '<svg viewBox="0 0 20 20"><path d="M5 8.5a5 5 0 0 1 10 0c0 3.6 1.4 5 1.9 5.5H3.1c.5-.5 1.9-1.9 1.9-5.5Z"/><path d="M8.2 16.4a2 2 0 0 0 3.6 0"/></svg>',
    bellOff: '<svg viewBox="0 0 20 20"><path d="M7 4.3A5 5 0 0 1 15 8.5c0 2 .4 3.3.9 4.2M13.6 14H3.1c.5-.5 1.9-1.9 1.9-5.5 0-.6.1-1.1.3-1.6"/><path d="M8.2 16.4a2 2 0 0 0 3.6 0"/><path d="M2.5 2.5l15 15"/></svg>',
    phone: '<svg viewBox="0 0 20 20"><rect x="5.5" y="2.5" width="9" height="15" rx="2"/><path d="M9 14.8h2"/></svg>',
    wait: '<svg viewBox="0 0 20 20"><path d="M16 10a6 6 0 1 1-1.8-4.3"/><path d="M16.2 3.2v3.3h-3.3"/></svg>',
  };
  const PAIRED_IN_THE_APP = "It is paired. Open RichConnect on it and keep talking.";
  const PAIRED_CARD = {
    ready: {
      chip: ["ok", "Connected"],
      state: PAIRED_IN_THE_APP,
      line: ["ok", CARD_ICONS.bell,
        "It can reach you with a notification when Rich has something for you. You can switch that off in RichConnect on the phone, under <strong>Settings</strong>."],
    },
    off: {
      chip: ["ok", "Connected"],
      state: PAIRED_IN_THE_APP,
      line: ["todo", CARD_ICONS.bellOff,
        "It cannot send you notifications yet. In RichConnect on the phone, open <strong>Settings</strong> and switch on <strong>Notify me when Rich replies</strong>, then allow notifications when the phone asks."],
    },
    // NOT REACHABLE claims only what this Mac knows (round 49 README): its listener is down and
    // the recovery monitor is restoring it. The Mac has no "phone last seen", so it cannot tell a
    // phone that is switched off from a connection being restored, and it does not try.
    away: {
      chip: ["wait", "Reconnecting"],
      state: "Its pairing is saved. RichOS is trying to restore the connection.",
      line: ["info", CARD_ICONS.wait,
        "There is nothing to do on the phone. It reconnects by itself while this Mac is awake with RichOS open."],
    },
    // OLDER PAIRING: a record the retired phone web page made (push_transport "web-push"). The
    // retirement left the Mac's records alone, so the record survives and the phone side it was
    // made with does not.
    old: {
      chip: ["todo", "Pair again"],
      state: "This phone was paired by an older version of RichOS and cannot reach Rich anymore.",
      line: ["todo", CARD_ICONS.phone,
        "To keep using Rich on it, install RichConnect on the phone, then press <strong>Forget this phone</strong> below and pair it again."],
    },
  };

  /// **WHICH OF THE FOUR, FROM THE MAC'S OWN STATUS.** `null` while the pairing itself is still
  /// a question (the six words, the press on this Mac, the phone's first request): round 49
  /// draws a finished pairing, so those screens keep their own sentences with no chip and no
  /// errand line. A RichConnect phone names its push service ("apns" or "fcm") when it answers
  /// They match (`phone/routes.rs`), so a confirmed record with anything else was made by the
  /// retired page. Older pairing outranks Not reachable: that phone cannot come back by itself.
  function pairedCardState(status) {
    const native = status.pushTransport === "apns" || status.pushTransport === "fcm";
    if (status.fingerprintConfirmed && !native) return "old";
    // Not reachable is this Mac's listener down OR, for a phone paired over RichConnect, the
    // connector restoring its link while the listener is up: the line above the card says
    // "Reconnecting this Mac" then, and the chip must not say Connected beside it.
    const connector = status.connect && status.connect.health && status.connect.health.state;
    if (status.listening === false || (status.pairedVia === "connect" && connector === "reconnecting")) return "away";
    if (status.macConfirmed === false || !status.fingerprintConfirmed || status.completed === false) return null;
    return status.notificationsOn ? "ready" : "off";
  }

  /// **THE PAIRED CARD'S COPY, DERIVED FROM THE RECORD AND FROM NOTHING THE SHEET REMEMBERS.**
  ///
  /// Ray's candidate .11 walk, defect 3.2: immediately after pairing an ANDROID phone over
  /// Tailscale the card said *"There is nothing to remove from your phone."* Closing and
  /// reopening the same card for the same phone said, instead, *"…open Settings, then General,
  /// then VPN and Device Management, then the RichOS profile, then Remove Profile"* — an iOS
  /// procedure, on a card whose own first line reads "Android phone is paired", on the one path
  /// that never installs anything. Reproduced twice.
  ///
  /// **The cause was not the copy, it was where the copy was keyed from.** The sheet's `route`
  /// was forgotten on every open, so a reopen selected a default and presented it as a fact.
  /// The answer is on the Mac instead (`device.rs`'s `paired_via`), and it arrives on every
  /// `phone_status`.
  ///
  /// **ONE ANSWER NOW, AND ONE MIGRATION SENTENCE.** With §61 there is one way to pair, so what
  /// this card says about a phone paired TODAY does not depend on the phone at all: nothing was
  /// put on it. The second entry is for a record written by a build that had a second path —
  /// it says what to do (pair again) and, because a cleanup the user has to know about is a
  /// cleanup that does not happen, it says the old build may have left something on the phone
  /// and where to look. It does not walk him through it: the product that asked for it is gone.
  const PAIRED_FORGET_NOTE_TAILNET =
    "Forgetting it here stops this Mac answering it, deletes the keys and stops this Mac " +
    "answering at its Tailscale name. There is nothing to remove from your phone.";

  /// A record from a build that paired some other way — or one written before the field existed.
  /// **It says what it knows and no more.** A default here would be the original defect with a
  /// longer comment on it.
  const PAIRED_FORGET_NOTE_OLD =
    "Forgetting it here stops this Mac answering it and deletes the keys. This phone was " +
    "paired by an older version of RichOS, which connected a way this one does not use. Pair " +
    "it again to keep using it. If that older version put a RichOS profile on the phone, you " +
    "can remove it in the phone's own settings; nothing RichOS does now needs one.";

  /// **THE FIRST PARAGRAPH OF EVERY SCREEN, AND THERE IS ONE OF IT.**
  ///
  /// Ray's candidate .11 defect 3.1. Every screen in this flow opened with *"Your phone talks
  /// to this Mac directly, over your own home network. Nothing of what you say goes anywhere
  /// else, and there is no account to make."* That described the option he had not chosen, and
  /// it stayed there word for word after he chose the other one. The fix at the time was to key
  /// the sentence off the path; §61 removes the other path, so the key goes too and what is
  /// left is the sentence that is true of the product.
  ///
  /// **IT DOES NOT CLAIM THE PRIVACY THE OTHER PATH CLAIMED, and that is deliberate.**
  /// "Nothing of what you say goes anywhere else" was true of a phone and a Mac on one local
  /// network. On the tailnet the packets are encrypted end to end between the two devices but
  /// they can be relayed by Tailscale's own servers when a direct connection cannot be made, so
  /// the honest claim is about what is ENCRYPTED and about where the conversation is KEPT —
  /// both of which this Mac can stand behind — and not about what never travels.
  const LEAD =
    "Your phone reaches this Mac over your own Tailscale network, from anywhere it has a " +
    "signal. The connection to this Mac is encrypted, your conversations stay here and it " +
    "costs one free Tailscale account signed in on both devices.";

  /// **WHAT AN EXPIRED CODE SAYS**, on the screen it expired on. It names what happened, says
  /// the codes are meant to run out, and points at the button beside it — so the one thing he
  /// does next is the one control on the screen. Before this he was shown nothing at all.
  const EXPIRED_NOTE =
    "Pairing codes run out on purpose, so one left on a screen cannot be used later. Nothing " +
    "is wrong and nothing was lost. Press Show me another code and a fresh one appears here.";

  /// **What the paired phone can do, and the one condition on it.** Same source as the note
  /// above, for the same reason: it was keyed off `route` and vanished on a reopen.
  ///
  /// It used to end *"— at home too"*, which read as a carve-out from a second path that no
  /// longer exists. The condition is not about where you are standing; it is about both devices
  /// being signed in, wherever they are.
  const PAIRED_TAILNET_LIMIT =
    "You can use this anywhere your phone has a signal. Both devices have to be signed in to " +
    "Tailscale for it to connect, wherever you are.";

  /// **THE TWO SENTENCES FOR THE PAIRED CARD'S FIRST LINE, AND THE ORDER THEY HAPPEN IN** —
  /// Ray's nightly `.7` walk, defect 2.
  ///
  /// The card said `It is paired. Open Rich on it and keep talking.` from the moment the phone
  /// redeemed the code — which is BEFORE the person has been shown the six words, let alone
  /// answered them. Both screens were up together in his frame 13: the Mac announcing the
  /// answer, the phone still asking the question.
  ///
  /// So the first of these is what is true at that moment, and it says what he is being asked
  /// to do rather than leaving him to infer it. The second is the finish, and it is reached
  /// only when the phone comes back over `POST /api/pair` with `fingerprint_confirmed: true`.
  ///
  /// **AND THE ANSWER THAT COUNTS IS NOW GIVEN HERE** — Sage's pairing review F1. The first
  /// sentence used to end "then answer on the phone", and the phone's answer was the only one the
  /// Mac recorded; a device that paired first with a code seen on a screen share answered for
  /// itself. So the question is put on this Mac, beside the words and the two buttons. The third
  /// sentence is the state after this Mac's press and before the phone's own answer arrives.
  const PAIRED_AWAITING_WORDS =
    "It has reached this Mac. Check that the six words on it are the six words below, then " +
    "press They match.";
  const PAIRED_AWAITING_PHONE =
    "This Mac accepted it. Finish on your phone by pressing They match there too.";
  /// **BOTH PRESSES ARE IN, AND THE PHONE HAS NOT USED THE PAIRING YET** — Sage's pair-v2
  /// hypotheses review §2. The press on this Mac is this Mac's yes, not evidence that the phone
  /// heard it: a phone that lost this Mac after its own press throws its key away at its deadline,
  /// and a card that already said `It is paired` would list a phone that can never connect. So
  /// `It is paired` waits for `status.completed`, the phone's first ordinary request, and this is
  /// what is true until then. The sentence is Sage's, from his table of end states; Urban has not
  /// reviewed it yet.
  /// The finish itself is round 49's (`PAIRED_CARD` above).
  const PAIRED_AWAITING_FINISH = "This Mac accepted it. Waiting for your phone to finish.";

  /// **THE SPENT-CODE ALARM, AFTER THE PRESS** (Sage F1 item 4): somebody used this phone's code
  /// again after he had confirmed it here. The phone is kept; he is told, and pointed at the one
  /// control that undoes it.
  const CODE_REUSED_NOTE =
    "Someone used this phone's pairing code again after you paired it. If you did not expect " +
    "that, press Forget this phone and pair again where nobody else can see this screen.";

  /// **WHAT THE MAC SAYS WHEN THE PERSON PRESSES THE ALARM BUTTON** — Ray's nightly `.8` walk,
  /// defect 1.
  ///
  /// One sentence, and it is a report rather than an instruction: it names what was said, and
  /// then the three things this Mac did about it, in the order it did them. Nothing is softened
  /// and nothing is hedged — *"in case he was mistaken"* is the one outcome the product has
  /// already ruled out, on both ends, and a screen that hinted otherwise would be arguing with
  /// the phone.
  ///
  /// **It claims nothing the Mac has not finished doing.** `PhoneStatus.rejected` is set by
  /// `PhoneRuntime::stop_because_the_words_were_refused`, after the device record is
  /// already gone, and the port is closed within milliseconds of it (measured at 1 ms in
  /// `phone::listen::tests::they_do_not_match_over_the_wire_stops_the_listener`).
  const REJECTED_NOTE =
    "Your phone said the six words did not match, so I stopped answering, forgot the phone " +
    "and deleted the certificate this Mac was serving.";

  /// **WHO STOPPED IT DECIDES THE SENTENCE, AND NOTHING ELSE.** `status.rejectedBy`
  /// (`device::StoppedBy`) since Sage's pairing review: the person can now refuse the words on
  /// this Mac (F1), a second phone can use the same code (the spent-code alarm), and a pairing
  /// nobody pressed before the window closed is forgotten (§3.1 step 6). Each is one sentence in
  /// the same shape as the phone's — what happened, then the three things this Mac did — so the
  /// screen, its heading and its one control are the same for all four. An unknown reason (a
  /// record from an older build) gets the sentence that was the only one before.
  const STOPPED_NOTES = {
    phone: REJECTED_NOTE,
    mac:
      "You said the six words did not match, so I stopped answering, forgot the phone and " +
      "deleted the certificate this Mac was serving.",
    "code-used-twice":
      "A second phone used the same code, which means someone else may have seen it, so I " +
      "stopped answering, forgot the phone that used it first and deleted the certificate this " +
      "Mac was serving.",
    expired:
      "Nobody pressed They match on this Mac within five minutes, so I stopped answering, forgot " +
      "the phone and deleted the certificate this Mac was serving.",
    // Sage's pair-v2 review §2: this Mac pressed They match, and the phone never used the pairing
    // within the grace after the window. Written here in the shape above; Urban has not reviewed it.
    unfinished:
      "This Mac accepted your phone, but the phone never finished pairing, so I stopped " +
      "answering, forgot the phone and deleted the certificate this Mac was serving.",
  };
  function stoppedNoteFor(by) {
    return Object.prototype.hasOwnProperty.call(STOPPED_NOTES, by) ? STOPPED_NOTES[by] : REJECTED_NOTE;
  }

  /// Which of the two the record selects. `via` is the token `device.rs` writes; anything that
  /// is not this build's own path — including an empty string from a record written before the
  /// field existed — gets the migration sentence rather than being folded into the answer that
  /// happens to be true of a phone paired today.
  function forgetNoteFor(via) {
    return String(via || "") === "tailnet"
      ? PAIRED_FORGET_NOTE_TAILNET
      : PAIRED_FORGET_NOTE_OLD;
  }

  let ticker = null;
  let busy = false;
  let selectedRoute = null;
  let latestStatus = null;
  let returnFocus = null;
  let poller = null;

  /// **Whether the identity screen has been read, this open.** It is a thing the user has been
  /// told, not a thing the Mac knows, and a "don't show me again" on the one warning §61.1
  /// exists for would be the warning quietly deleting itself. So it is forgotten on every open,
  /// which is the lifetime `route` used to have and the reason Urban's §1 gave for it.
  let identityUnderstood = false;

  /// When the phone step first appeared, so "it has not joined" can be told from "it has not
  /// finished signing in". `null` whenever that screen is not up.
  let waitingForPhoneSince = null;

  /// **How long a phone is given to appear on the tailnet before the screen calls it a mismatch.**
  ///
  /// `unverified:` **sixty seconds is a judgment, not a measurement.** Measuring it properly needs
  /// a second device signing in to a fresh account while this Mac watches, and I had one Mac and no
  /// phone. What decided the number is which way the error costs more: too SHORT and a user who is
  /// still typing their password is told they got it wrong — which would send them to re-do a
  /// sign-in that was correct, the exact loop that had the CEO uninstalling the app three times.
  /// Too long merely means a few more seconds of "waiting". So it is biased long on purpose.
  /// Settled by one measurement with a real phone, and the constant is here to be changed.
  const PHONE_JOIN_GRACE_MS = 60000;

  /// Escape text destined for `innerHTML`. Only one string needs it — the account name, which comes
  /// from another program's output — and it gets it rather than being trusted for being ours.
  function escapeText(text) {
    const box = document.createElement("span");
    box.textContent = String(text);
    return box.innerHTML;
  }

  /// Every external address these screens name, each one verified against Tailscale's own install
  /// documentation rather than recalled — see `docs/verification/tailscale-path-2026-09-18.md` §4a.
  ///
  /// **They are both written out AND opened by a control.** The printed form is Urban's own
  /// requirement — *"a control that opens somewhere the user cannot see first is a control that
  /// asks for trust it has not earned"* — and it is the fallback for a Mac where the opener does
  /// not work. The control is there because on 2026-09-19 the CEO could not find the download
  /// from `tailscale.com`'s home page nor from the admin console, and an address a person has to
  /// retype into a browser is an address half of them will mistype.
  ///
  /// **`open_external` takes these strings as KEYS, not as URLs.** `src-tauri/src/opener.rs`
  /// matches the whole string against a five-entry table and refuses everything else; a test over
  /// there reads THIS object and proves every value in it is one the command will open, so a new
  /// address added here without an allowlist entry fails the suite rather than producing a button
  /// that quietly does nothing.
  ///
  /// **The two Apple IDs are different and must not be swapped**: `1470499037` is the iPhone and
  /// iPad app, `1475387142` is the Mac app. Sending him to the wrong one sends him to a listing his
  /// device cannot install from.
  const LINKS = {
    mac: "tailscale.com/download/mac",
    phone: "apps.apple.com/app/tailscale/id1470499037",
    android: "play.google.com/store/apps/details?id=com.tailscale.ipn",
    console: "console.tailscale.com/admin/dns",
  };

  /// Screens 2, 3 and 7 — the three states whose next step happens in somebody else's software.
  /// The Mac cannot advance the user and the user cannot advance the Mac, so there is no Next and
  /// no Back: detection moves the screen, and `Check again` is the button for the impatient and
  /// for the failure.
  /// The app itself, not a page — `opener.rs`'s fifth allowlist entry. "Open Tailscale" has to
  /// bring the app the user already installed to the front; sending somebody who has it to a
  /// download page would be telling them to install it again.
  const TAILSCALE_APP = "Tailscale.app";

  const WAITING = {
    absent: {
      heading: "Install Tailscale on this Mac",
      note1:
        "Tailscale is somebody else's app and it is free for one person. It gives this Mac a name your phone can reach from anywhere and it is the whole reason this path has no certificate in it.",
      note2: "Download it, install it, then come back here. This screen moves on by itself when it sees it.",
      url: LINKS.mac,
      action: { label: "Get Tailscale", target: LINKS.mac },
    },
    "needs-sign-in": {
      heading: "Sign in to Tailscale on this Mac",
      note1:
        "Tailscale is installed here but not signed in. Open it and sign in. Any of the sign-in choices it offers is fine and the free plan is enough. I cannot do this part for you.",
      // THE SHORTENED WHY, ON THE SCREEN WHERE THE IDENTITY IS ACTUALLY CHOSEN. Same CEO ruling as
      // the phone screen's opening paragraph: he would "normally absolutely NEVER use the same
      // identity on the Mac and on the phone", so the choice has to be made KNOWING it will be
      // reused. Telling him afterwards, on the phone screen, is telling him after he has already
      // picked — and by then the fix is signing out of an account he just created.
      note2:
        "Whichever you pick, you will sign in to the SAME one on your phone: your Tailscale account is your private network and devices signed in to it are what can reach each other. Nothing else connects them.",
      url: "",
      action: { label: "Open Tailscale", target: TAILSCALE_APP },
    },
    "certificates-off": {
      heading: "Tailscale is signed in, but this Mac has no name yet",
      note1:
        "Tailscale gives each machine a name on your network and this Mac does not have one I can use. In Tailscale's own admin console, turn on MagicDNS and HTTPS certificates for your network, then come back.",
      note2: "",
      url: LINKS.console,
      action: { label: "Open the Tailscale console", target: LINKS.console },
    },
  };

  /// **Ask the Mac to open one of the five addresses these screens name.**
  ///
  /// The refusal is a sentence rather than a silence: a control that does nothing is the one
  /// failure a user cannot tell from a slow one, and the address is printed right beside it.
  async function openExternal(target) {
    try {
      await bridge.invoke("open_external", { target });
    } catch (error) {
      field("phone-message").textContent =
        "I could not open that on this Mac. The address is written out above. Type it into your browser.";
    }
  }

  /// **Is the phone on this network, and can it answer?** Three states, three different things to
  /// do, and telling them apart is the whole of §61.1's detection half.
  ///
  /// * **A live peer** — nothing to fix, and saying so is what stops a user who has done it right
  ///   going round again.
  /// * **A peer that is switched off** — MEASURED on this Mac on 2026-09-19: his Android was in
  ///   the daemon's `Peer` map TWICE, one stale registration per reinstall, and both were
  ///   `"Online": false` because Tailscale on the phone was off. The fix is one switch in the
  ///   Tailscale app. Sending that user back to check their identity is the loop this screen
  ///   exists to end.
  /// * **No peer at all, after a wait** — a phone signed in to a DIFFERENT identity is in a
  ///   different tailnet and shows up nowhere at all, so absence is the evidence. It is named
  ///   only after the grace window, because before that "not here yet" and "still signing in"
  ///   are the same thing and calling it a mistake would be the screen guessing.
  function peerLine(line, tailnet) {
    if (tailnet.phone && tailnet.phoneOnline) {
      line.textContent = "Your phone (" + tailnet.phone + ") is on this network.";
      return;
    }
    if (tailnet.phone) {
      line.textContent =
        "Your phone (" + tailnet.phone + ") is on this network but Tailscale is switched off on " +
        "it. Turn it on in the Tailscale app.";
      return;
    }
    const waited = waitingForPhoneSince === null ? 0 : Date.now() - waitingForPhoneSince;
    if (waited < PHONE_JOIN_GRACE_MS) {
      line.textContent = "Waiting for your phone to join…";
      return;
    }
    line.textContent = tailnet.account
      ? "Your phone is not on this network yet. On the phone, sign in with " + tailnet.account +
        ", then come back here. Reinstalling the app will not help: the identity is the only " +
        "thing that decides it."
      : "Your phone is not on this network yet. On the phone, sign in with the same account this " +
        "Mac uses, then come back here. Reinstalling the app will not help: the identity is the " +
        "only thing that decides it.";
  }
  // The states that are not a step of their own borrow the nearest screen that tells the truth.
  // Each of these is "Tailscale is on this Mac and is not usable yet", and the sign-in screen is
  // the one that says so without claiming to know more than detection does.
  WAITING["not-running"] = WAITING["needs-sign-in"];
  WAITING["needs-approval"] = WAITING["needs-sign-in"];
  WAITING["stopped"] = WAITING["needs-sign-in"];
  WAITING["other-user"] = WAITING["needs-sign-in"];

  function render(status) {
    latestStatus = status;
    const managed = status.connect;
    const route = status.pairedVia || status.servingVia || selectedRoute || (managed ? "connect" : "tailnet");
    const onConnect = route === "connect";
    field("phone-route-choice").hidden = !managed || status.paired || status.listening;
    field("phone-use-connect").setAttribute("aria-pressed", String(onConnect));
    field("phone-use-tailnet").setAttribute("aria-pressed", String(!onConnect));
    field("phone-connect").hidden = !onConnect || status.rejected;
    field("phone-connect-start").hidden = !!status.paired || !!status.listening;
    field("phone-connect-disable").hidden = !managed || (!managed.enabled && !managed.cleanupPending);
    const health = managed && managed.health && managed.health.state;
    field("phone-connect-status").textContent = managed && managed.cleanupPending
      ? "Connect is off on this Mac. Its remote address is being removed."
      : health === "connected" ? "This Mac is connected."
      : health === "reconnecting" ? "Reconnecting this Mac. Your phone keeps unsent messages until it can reach RichOS again."
      : health ? "Connecting this Mac…" : "First, set up this RichOS app on your Mac. Then pair your phone with it.";

    const live = !!status.pairUrl;
    // **A CODE THAT RAN OUT IS A STATE OF THIS SCREEN, NOT A REASON TO LEAVE IT.**
    //
    // Ray's candidate .11 defect 3.3: the Mac dropped the window, the next poll came back with
    // no `pairUrl`, and the dialog fell back to an earlier screen with the QR, the code and the
    // six words gone and nothing said. *"He reads the screen, walks to get his phone, unlocks
    // it, opens the camera — and comes back to a screen that looks like he imagined the whole
    // thing."*
    //
    // **`listening` IS THE EVIDENCE, and it is the Mac's rather than this sheet's.** The socket
    // comes up only inside `phone_begin_pairing` for an unpaired Mac (`mod.rs`'s `start`, and
    // `resume_if_paired` returns early unless something is paired), and it stays up after the
    // window closes. So a Mac that is serving, has nothing paired and has no live code is a Mac
    // whose code ran out — no matter how long ago, and no matter how many times this sheet has
    // been closed and reopened since. A flag in this file would have forgotten it on the open,
    // which is the same mistake defect 3.2 was.
    const expired = !live && !status.paired && !!status.listening;
    // Everything downstream that used to mean "a window is open" now means "the pairing screen
    // is the screen", because it is, in both states.
    const pairing = live || expired;
    const tailnet = status.tailnet || { state: "absent" };

    // **THE LEAD, FIRST, BECAUSE EVERY SCREEN BELOW CARRIES IT.** One path, one sentence, no
    // key: see LEAD. It used to be selected by `status.pairedVia` while paired and by
    // `status.servingVia` while a window was open, and both of those existed to tell two paths
    // apart on a screen that now has one.
    field("phone-lead").textContent = onConnect ? "Keep talking to Rich when you are away from your Mac." : LEAD;

    // **THE ALARM BUTTON, ANSWERED, AND IT OUTRANKS EVERYTHING BELOW** — Ray's nightly `.8`,
    // defect 1. It is taken before the six screens rather than folded into them because it is
    // not one of their states: `rejected` is true of a Mac that is not listening, has nothing
    // paired and has no code — which is character for character the input that draws
    // `This Mac is ready`, the screen that said nothing had happened.
    //
    // **NOTHING POLLS HERE AND THE COUNTDOWN STOPS.** Every other screen in this sheet is
    // waiting for something to happen elsewhere; this one is waiting for him to read it. The
    // only thing that leaves it is the button on it, which is `phone_begin_pairing` — the same
    // call that clears the flag on the Mac.
    if (status.rejected) {
      field("phone-rejected-note").textContent = stoppedNoteFor(status.rejectedBy);
      field("phone-rejected").hidden = false;
      for (const id of [
        "phone-identity",
        "phone-ts-wait",
        "phone-ts-ready",
        "phone-paired",
        "phone-pairing",
        "phone-refresh",
        "phone-pairing-back",
      ]) {
        field(id).hidden = true;
      }
      stopTicking();
      setPolling(false);
      waitingForPhoneSince = null;
      return;
    }
    field("phone-rejected").hidden = true;

    // **THE SIX SCREENS, AND EVERY STATUS REACHES EXACTLY ONE OF THEM.**
    //
    // Urban's §3 table, with row 1 — the route chooser — struck out by CEO §61 and its two
    // successors merged into the entry point. The order below is the order a first-time user
    // meets them, and the conditions are mutually exclusive by construction: `paired`, then
    // `pairing`, then the three states of a Mac that is neither, split by whether detection can
    // name an account and whether the tailnet is ready.
    //
    // **THERE IS NO SEVENTH, "NOTHING IS HAPPENING" SCREEN ANY MORE.** `#phone-off` was the
    // screen the home route landed on — *"I will make a certificate for your phone…"* — and it
    // was reachable only because `onTailscale` could be false. With one path a Mac that is not
    // ready is not idle, it is PARTWAY: it has no Tailscale, or it is not signed in, or it has
    // no name yet, and each of those is a how-to screen that says which. That is §61's own
    // sentence about guiding the user, and it is why the deletion is a simplification rather
    // than a hole: the three conditions below are exhaustive.
    //
    // SCREEN 0 — the identity trap (§61.1). Shown while this Mac has no Tailscale account yet,
    // which is exactly the window in which the choice of identity can still be made freely. Once
    // detection can name an account the screens say WHICH one instead, and this never returns.
    const identity =
      !onConnect && !status.paired && !pairing && !tailnet.account && !identityUnderstood;
    // Rows 2, 3 and 5: something has to happen in somebody else's software first.
    const waiting = !onConnect && !status.paired && !pairing && !identity && tailnet.state !== "ready";
    // Row 4: ready, and the only thing left is the code.
    const ready = !onConnect && !status.paired && !pairing && !identity && tailnet.state === "ready";

    field("phone-identity").hidden = !identity;
    field("phone-ts-wait").hidden = !waiting;
    field("phone-ts-ready").hidden = !ready;
    field("phone-paired").hidden = !status.paired;
    const onThePairingScreen = !(status.paired || !pairing);
    field("phone-pairing").hidden = !onThePairingScreen;
    // The pairing screen's own two controls live in the pinned footer (see the markup), so they
    // are hidden BY THEMSELVES from the same condition rather than by their container. One
    // condition, one source, evaluated once — a second copy of it here is how the footer and the
    // screen it belongs to would come to disagree.
    field("phone-refresh").hidden = !onThePairingScreen;
    field("phone-pairing-back").hidden = !onThePairingScreen;

    // Rows 2, 3 and 5 poll — and so do BOTH of the screens that are waiting for the phone, because
    // a phone joining the tailnet is another thing that happens elsewhere and has to move the
    // screen on its own. Screen 0 does not: nothing detection can find changes what it says.
    // This is the whole reason there is no Next button anywhere in the flow.
    //
    // **AND SO DOES THE PAIRED CARD WHILE IT IS STILL ASKING ABOUT THE SIX WORDS**, which it did
    // NOT do until Ray's nightly `.8` defect 1 — and that omission is why his walk saw nothing
    // change for two minutes. The answer to the six words arrives from the phone, over the
    // network, exactly like a phone joining the tailnet: it is a third screen waiting for
    // something that happens elsewhere. Without this the Mac could stop serving, forget the
    // phone and set `rejected`, and the card would sit there reading `It has reached this Mac`
    // until he closed it by hand. It moves the confirming case too — `It is paired` now arrives
    // by itself rather than on the next open.
    //
    // AND WHILE THIS MAC IS WAITING FOR ITS OWN PRESS (Sage F1): the window's timer keeps running
    // on the Mac, and a pairing nobody answers in time is forgotten there, so the card has to move
    // to the screen that says so by itself rather than keep offering two buttons that no longer do
    // anything. It stops the moment the answer is in.
    //
    // AND WHILE THE PHONE HAS NOT USED A PAIRING THIS MAC ACCEPTED (Sage's pair-v2 review §2):
    // `It is paired` arrives by itself the moment it does, and the stopped screen arrives by
    // itself if it never does. `completed === false` only: a Mac that predates the field sends
    // nothing, and that Mac said `It is paired` on the phone's answer.
    const askingAboutTheWords =
      status.paired &&
      (!status.fingerprintConfirmed || status.macConfirmed === false || status.completed === false);
    setPolling(onConnect || waiting || ready || (pairing && !status.paired) || askingAboutTheWords);

    // **THE WATCH STARTS WHEN THE USER IS FIRST TOLD TO GO TO THEIR PHONE**, which is Screen 4 as
    // well as Screen 5 — see the note beside `#phone-ts-peer-ready` for the frame math that makes
    // Screen 5 alone unreachable: window 60 s, grace 60 s, and the watch starting after the
    // window opens leaves an empty interval.
    if (ready || (pairing && !status.paired)) {
      if (waitingForPhoneSince === null) waitingForPhoneSince = Date.now();
    } else {
      waitingForPhoneSince = null;
    }

    if (onConnect && !status.paired && !pairing) { stopTicking(); return; }
    if (status.paired) {
      field("phone-device-name").textContent = status.deviceName || "Your phone";
      // Ray's defect 2. `It is paired` is a claim about something only the person can settle,
      // so it waits for him to settle it. What is true before that is that the phone reached
      // this Mac and is asking him the question — so that is what the line says, and it names
      // the thing he is being asked to do rather than leaving him to work it out.
      // SAGE F1: THE ANSWER THAT COUNTS IS GIVEN HERE. `macConfirmed === false` is a phone that
      // reached this Mac and can do nothing until one of the two buttons below is pressed; the
      // Mac sends the words for the key that phone registered, and only in this state. A status
      // from a Mac that predates the field (`undefined`) is treated as confirmed, which is what
      // that Mac did.
      const awaitingMac = status.macConfirmed === false;
      const words = (status.fingerprintWords || []).join("  ");
      // ROUND 49: one of its four states once the pairing is settled, or null while it is not.
      const card = PAIRED_CARD[pairedCardState(status)] || null;
      field("phone-paired-state").textContent = card ? card.state
        : awaitingMac ? PAIRED_AWAITING_WORDS
        : !status.fingerprintConfirmed ? PAIRED_AWAITING_PHONE
        : PAIRED_AWAITING_FINISH;
      field("phone-chip").hidden = !card;
      field("phone-chip").dataset.tone = card ? card.chip[0] : "";
      field("phone-chip-word").textContent = card ? card.chip[1] : "";
      field("phone-paired-words").textContent = words;
      field("phone-paired-words").hidden = !awaitingMac || !words;
      field("phone-mac-answer").hidden = !awaitingMac || status.listening === false;
      field("phone-code-reused").textContent = status.codeReused ? CODE_REUSED_NOTE : "";
      field("phone-code-reused").hidden = !status.codeReused;
      // Nothing about notifications, limits or forgetting while the pairing itself is still a
      // question: those are about a phone that is paired, and "They do not match" is the way out.
      for (const id of ["phone-ts-limit", "phone-card-rule", "phone-forget-note", "phone-forget"]) {
        field(id).hidden = awaitingMac;
      }
      // The errand line belongs to round 49's four states and to nothing before them.
      field("phone-push-state").hidden = awaitingMac || !card;
      field("phone-push-state").dataset.tone = card ? card.line[0] : "";
      field("phone-push-icon").innerHTML = card ? card.line[1] : "";
      field("phone-push-text").innerHTML = card ? card.line[2] : "";
      // **SCREEN 6, DERIVED FROM THE RECORD.** What the phone paired over is a fact the Mac
      // wrote down at pairing, and it is the only thing that decides these two lines — on the
      // first open and on every reopen alike. The sheet's own memory is deliberately not
      // consulted: a card that described the path from a forgotten answer is the whole of
      // defect 3.2.
      const pairedOverTailnet = status.pairedVia === "tailnet";
      field("phone-ts-limit").textContent = PAIRED_TAILNET_LIMIT;
      field("phone-ts-limit").hidden = awaitingMac || !pairedOverTailnet;
      field("phone-forget-note").textContent = onConnect ? "Forgetting this phone removes its access immediately. You can pair it again with a new code." : forgetNoteFor(status.pairedVia);
      return;
    }
    if (waiting) {
      // The Mac's own sentence for this state is the floor; the screen says more, never less and
      // never something different.
      const screen = WAITING[tailnet.state] || WAITING.absent;
      field("phone-ts-heading").textContent = screen.heading;
      field("phone-ts-note1").textContent = screen.note1 || tailnet.sentence || "";
      field("phone-ts-note2").textContent = screen.note2 || "";
      field("phone-ts-note2").hidden = !screen.note2;
      field("phone-ts-url").textContent = screen.url;
      field("phone-ts-url").hidden = !screen.url;
      // THE CONTROL THAT OPENS THE THING THE SCREEN JUST NAMED. Its label is the state's, because
      // "Get Tailscale", "Open Tailscale" and "Open the Tailscale console" are three different
      // actions and one generic verb over them would be a button whose effect the user has to
      // guess. The address stays printed above it either way.
      const opener = field("phone-ts-open");
      opener.textContent = screen.action ? screen.action.label : "";
      opener.hidden = !screen.action;
      opener.dataset.target = screen.action ? screen.action.target : "";
      return;
    }

    if (ready) {
      // THE SAME LINE THE PHONE SCREEN WILL REPEAT, on the Mac, after sign-in. The identity has to
      // be visible here or the user cannot know which one to reuse — that is the CEO's own
      // experience, and it is why the account is shown rather than only the machine name. It is
      // its own paragraph rather than two newlines inside the machine name, because the name is
      // monospace and read character by character and a sentence is neither.
      field("phone-ts-name").textContent = tailnet.name || "";
      // **`innerHTML` WITH `<strong>`, LIKE THE TWO SCREENS BELOW IT** — Urban's G14. This line
      // used `textContent` while `#phone-ts-why` and `#phone-ts-step2` both give the same
      // account weight, *"and one of the three is wrong and it is the first one"*. The account
      // is another program's output, so it is escaped rather than trusted for being ours — the
      // same `escapeText` the other two call, and the reason it exists.
      field("phone-ts-account").innerHTML = tailnet.account
        ? "You signed in with <strong>" + escapeText(tailnet.account) +
          "</strong>. Use exactly this on your phone."
        : "I cannot tell which identity this Mac is signed in to. Whichever it is, sign in with exactly the same one on your phone.";
      peerLine(field("phone-ts-peer-ready"), tailnet);
      return;
    }

    // A Mac that is not paired and has no window open has already returned above, on one of
    // the three how-to screens — the conditions are exhaustive, and the block that used to sit
    // here (`#phone-off`, *"I will make a certificate for your phone…"*) was reachable only
    // from the route this change removes. Everything below is the pairing screen.
    // **AN EXPIRED SCREEN IS THE EXPIRY AND THE WAY OUT, AND NOTHING ELSE.** The instructions
    // belong with a live code: steps pointing at a QR that is not on the screen are a screen
    // asking him to do a thing that is not there. They all come back with the next code.
    //
    // There is no second half to switch between any more. These two nodes were
    // `!onTailscale || expired` and their absent twins were `onTailscale || expired`, and the
    // whole of Urban's blocker `esc-20260919T041857Z-640acd39` was those conditions answering
    // for the wrong path on a reopen. One path cannot answer wrongly.
    const codeBlock = field("phone-code-block");
    field("phone-connect-pair-help").hidden = expired || !onConnect;
    field("phone-ts-steps").hidden = expired || onConnect;
    field("phone-ts-failure").hidden = expired || onConnect;
    field("phone-ts-store").textContent = !expired
      ? "iPhone: " + LINKS.phone + "      Android: " + LINKS.android
      : "";

    if (!expired) {
      // THE WHY COMES BEFORE THE STEPS, AND IT NAMES THE ACCOUNT (§61.1 (c)). The reason is given
      // before the instruction because it is asking the user to break a habit: *"I would normally
      // absolutely NEVER use the same identity on the Mac and on the phone."* A person who
      // follows a step that says "sign in" and uses the wrong one is being correct by their own
      // lights, and the result is two networks that never meet with nothing saying why.
      field("phone-ts-why").innerHTML = tailnet.account
        ? "Your Tailscale account <strong>is</strong> your private network. Devices signed in to " +
          "it can reach each other. So on your phone, sign in with <strong>" +
          escapeText(tailnet.account) + "</strong>, exactly what this Mac used, even if you " +
          "would normally keep your phone and your Mac apart. Nothing else connects them."
        : "Your Tailscale account <strong>is</strong> your private network. Devices signed in to " +
          "it can reach each other. So sign in on your phone with the same account you used on " +
          "your Mac, even if you normally keep them separate. Nothing else connects them.";

      // STEP 2, FILLED IN. The CEO signed in with Apple on the Mac and Google on the Android and
      // "had no way to know they were different networks, or which to reuse". Naming the account
      // is the difference between an instruction he can follow and one he cannot.
      field("phone-ts-step2").innerHTML = tailnet.account
        ? "Sign in with <strong>" + escapeText(tailnet.account) +
          "</strong>, the same one this Mac is signed in to. A different provider makes a " +
          "different network and then the two will never see each other."
        : "Sign in with the <strong>same</strong> identity you used on this Mac. A different " +
          "provider makes a different network and then the two will never see each other.";

      // WAITING, THEN SAYING SO — the same three sentences Screen 4 uses, from the same helper,
      // so the two screens cannot word the same evidence differently.
      peerLine(field("phone-ts-peer"), tailnet);
    }
    // **THE CODE, OR THE FACT THAT IT RAN OUT — never neither, and never a screen that just
    // went blank.** Defect 3.3. Everything that IS the code is drawn only while there is one;
    // when it has run out the same screen carries the reason and the button that replaces it.
    //
    // The two headings are literals in the markup now rather than a pair of ternaries — Urban's
    // N4 was *"Then"* pointing backwards at a numbered step that G3 had moved below it, and the
    // numbering it referred to belonged to the path that installed something. There is nothing
    // to number, so there is nothing to refer back to.
    //
    // THE SECOND QR IS GONE WITH IT. It carried `status.trustUrl` — `http://<name>.local:8444/ca`,
    // the page that served the profile — and so is the address printed beside it.
    paint(field("phone-qr-pair"), live ? status.pairUrl : "");
    field("phone-pair-url").textContent = live ? status.pairUrl || "" : "";
    // The headings above those blocks go with them: a numbered step over an empty space is a
    // screen telling him to do something that is not there.
    field("phone-code-title").hidden = !live;
    field("phone-words-title").hidden = !live;
    // AND THE SENTENCE UNDER THAT HEADING, which is the half defect B found: it survived the
    // expiry telling him to check "these six" with nothing to check them against.
    field("phone-words-note").hidden = !live;
    // AND THE CONTAINER, which is the first thing on this screen: an empty box at the top of
    // the panel is worse than the same emptiness at the bottom was.
    codeBlock.hidden = !live || (onConnect && health !== "connected");
    field("phone-expired").hidden = !expired;
    if (expired) {
      field("phone-expired-note").textContent = EXPIRED_NOTE;
      // One label, in both states, because it is the same act: ask the Mac for a code. It reads
      // "Show me another code" and that is true of a first code after an expiry too.
      field("phone-countdown").textContent = "";
    }
    tick(live ? status.pairingSecondsLeft : null);
  }

  /// **How long is left, in the units a person would say it in.**
  ///
  /// The window is five minutes now (`PAIRING_WINDOW_MS`), and "This code lasts 287 more
  /// seconds." is a number nobody converts. Minutes while there are minutes, seconds under one
  /// minute — and the last minute is where the seconds start to matter, which is the only place
  /// they are shown.
  ///
  /// **`floor`, NEVER `ceil`** — Urban's G13, filmed at 30-second intervals: *"Countdown says
  /// '2 more minutes' at 61 s remaining, then jumps to '59 more seconds' … a countdown must
  /// never overpromise."*
  ///
  /// The arithmetic, at the four values that decide it. `ceil`: 120 -> 2, 61 -> **2**, 60 -> 1,
  /// 59 -> "59 more seconds". So the label said two minutes when 61 seconds were left — 59
  /// seconds more than it had — and then fell 61 seconds in one step. `floor`: 120 -> 2,
  /// 61 -> 1, 60 -> 1, 59 -> "59 more seconds", which never states a number the code cannot
  /// meet and never falls by more than the minute it just finished. The cost of `floor` is that
  /// the label reads "1 more minute" for the whole of the second minute; the cost of `ceil` is a
  /// person walking to their phone on a promise the Mac has already broken.
  function remaining(seconds) {
    if (seconds < 60) {
      return seconds + (seconds === 1 ? " more second" : " more seconds");
    }
    const minutes = Math.floor(seconds / 60);
    return minutes + (minutes === 1 ? " more minute" : " more minutes");
  }

  /// The countdown, which is the one thing on this screen that has to be honest by the second: a
  /// code that has expired and still looks live is a code he scans and then has to be told about.
  function tick(secondsLeft) {
    if (ticker) {
      window.clearInterval(ticker);
      ticker = null;
    }
    let left = Number.isFinite(secondsLeft) ? secondsLeft : 0;
    const paintCountdown = () => {
      const label = field("phone-countdown");
      if (left > 0) {
        label.textContent = "This code lasts " + remaining(left) + ".";
      } else {
        // The last second of the countdown, and then the expiry block below takes over on the
        // next poll. Both say it; neither leaves the screen.
        label.textContent = "That code has run out. Ask for another one.";
        if (ticker) {
          window.clearInterval(ticker);
          ticker = null;
        }
      }
    };
    paintCountdown();
    if (left > 0) {
      ticker = window.setInterval(() => {
        left -= 1;
        paintCountdown();
        // **AND THE SCREEN REDRAWS ITSELF AT ZERO.** It was written when the poll ran on one
        // of two routes and the other one would have sat at zero forever with nothing asking
        // the Mac again. The poll now covers every open window, so this is belt and braces
        // rather than the only mechanism — and it is kept because it is what makes the expiry
        // land on the same tick the countdown reaches zero instead of up to two seconds later.
        // One refresh, from inside the interval that has just been cleared, so it cannot recur:
        // the render that follows calls `tick(null)`, which starts no interval.
        if (left <= 0) refresh();
      }, 1000);
    }
  }

  async function refresh() {
    try {
      render(await bridge.invoke("phone_status"));
    } catch (error) {
      field("phone-message").textContent = String(error);
    }
  }

  function stopTicking() {
    if (ticker) {
      window.clearInterval(ticker);
      ticker = null;
    }
  }

  /// **The screens that wait, redraw themselves; nothing else polls.**
  ///
  /// Urban's §3: rows 2, 3 and 5 advance with no user action, because every step in them is
  /// completed in somebody else's software. The interval matches the Mac's own recheck ceiling
  /// (`TAILNET_RECHECK_MS`, two seconds in `phone/mod.rs`), so a poll is never served a cached
  /// answer it has already seen and the two cannot drift into a slower effective rate.
  ///
  /// **It stops the moment the screen stops waiting, and on close.** A settings sheet that keeps
  /// spawning a subprocess after it is hidden is exactly the untended-garbage shape the standing
  /// cleanup rule is about.
  function setPolling(on) {
    if (on && !poller) {
      poller = window.setInterval(refresh, 2000);
    } else if (!on && poller) {
      window.clearInterval(poller);
      poller = null;
    }
  }

  function close() {
    if (busy) return;
    stopTicking();
    setPolling(false);
    sheet.hidden = true;
    if (returnFocus && returnFocus.focus) returnFocus.focus();
  }

  async function open() {
    returnFocus = document.activeElement;
    field("phone-message").textContent = "";
    // THE IDENTITY WARNING IS RE-SHOWN ON EVERY OPEN, while this Mac still has no account.
    // Urban's §1 said the same thing about the route: a thing the user has been TOLD is not a
    // preference the Mac may remember on their behalf.
    identityUnderstood = false;
    sheet.hidden = false;
    await refresh();
    // **THE FIRST BUTTON IN THE MARKUP IS NOT THE FIRST BUTTON ON THE SCREEN** — Ray's
    // candidate-.16 new defect, second half.
    //
    // `querySelector("button:not([disabled])")` answers a question about DOM order and this is a
    // question about what is drawn. On every screen but the paired card the first match is
    // `#phone-forget`, which lives inside `#phone-paired` and is hidden with it — and `.focus()`
    // on a hidden element does nothing AT ALL, silently. So the sheet opened with focus still on
    // `document.body`: Ray pressed Tab three times on a reopened live-code sheet and never
    // reached a control, and Page Down did nothing because the body is not the box that scrolls.
    // Reproduced in this harness before the fix — `activeElement` BODY, `inSheet: false`, three
    // Tabs and still BODY.
    //
    // `offsetParent !== null` is the same visibility test the Tab trap at the bottom of this file
    // already uses, so the set focus may land in and the set Tab cycles through cannot diverge.
    // The PRIMARY control first — `Show me another code` on the pairing screen, `Set my phone up`
    // on "This Mac is ready", `I understand` on the identity trap — and Close when a screen has
    // no primary, which is the one every screen has.
    // `data-no-autofocus` keeps the two answers to the six words out of it (Sage F1): a pairing
    // is settled by a person who read the words, never by a Return pressed on arrival.
    const shown = [...sheet.querySelectorAll("button")].filter(
      (node) => !node.disabled && node.offsetParent !== null && !node.hasAttribute("data-no-autofocus")
    );
    const first = shown.find((node) => node.classList.contains("desk-btn--confirm")) || shown[0];
    if (first) first.focus();
    // **AND THE SHEET OPENS WHERE THE SCREEN BEGINS, NOT WHERE HE LEFT IT** — Urban's N2.
    //
    // G12's "reset the scroll on open" was applied to the Settings panel and not to this sheet,
    // so the reopen G1 is about — close the sheet mid-pairing, go and find your phone, come back
    // — landed at the BOTTOM of a two-viewport screen, a page below the live code he came back
    // for (his frames 05 and 06).
    //
    // **`hidden` DOES NOT FORGET A SCROLL OFFSET, MEASURED RATHER THAN ASSUMED.** Probed on this
    // build in the WebKit Tauri ships: the panel came back at 708 of 708 at 1400x950 and at 928
    // of 928 at 1024x700 — the app's own minimum and the size Urban walked — each exactly where
    // it was left. A `display: none` box reports 0 while it is hidden, which is what makes this
    // easy to talk yourself out of.
    //
    // Two branches, and the live one is the same helper `Show me another code` uses, so the two
    // doors into this screen cannot land in different places. `scrollToCode` no-ops when the code
    // block is hidden, so the order below is: top first, then the code if there is one.
    const panel = scrollBox();
    if (panel) panel.scrollTop = 0;
    scrollToCode();
  }

  /// **PUT THE CODE BACK IN FRONT OF HIM** — Urban's G4.
  ///
  /// *"`Show me another code` issues the code AND returns the panel to the top (frame 08), so
  /// the user must scroll the whole way down again to reach the thing they just asked for
  /// (frame 09)."*
  ///
  /// `#phone-scroll` is what scrolls, not the window and — since Ray's candidate-.16 defect —
  /// not `.overlay-panel` either. The panel is now a flex column whose `overflow-y` is `hidden`,
  /// holding one scroll box and a footer that does not move (style.css, `.phone-scroll`), and
  /// `.overlay` itself is `position: fixed` and does not scroll at all. So the measurement is
  /// taken between the scroll box and the code rather than from `offsetTop`, whose `offsetParent`
  /// here is the fixed `.overlay` and not the box that moves.
  ///
  /// 12px of air above the heading rather than 0, so the code reads as the top of a screen and
  /// not as a block cut off by the box's edge.
  function scrollToCode() {
    const panel = scrollBox();
    const code = field("phone-code-block");
    if (!panel || !code || code.hidden) return;
    const delta = code.getBoundingClientRect().top - panel.getBoundingClientRect().top - 12;
    panel.scrollTop = Math.max(0, panel.scrollTop + delta);
  }

  /// `toCode` is TRUE only for "Show me another code", and false for `Set my phone up` on
  /// `This Mac is ready`. Entering a screen scrolled past its own opening sentence would be the
  /// same defect pointing the other way.
  ///
  /// **IT CARRIES NO ANSWER, BECAUSE THERE IS NO QUESTION** (CEO §61). It used to send the route
  /// the user had chosen — Urban's N1, after a build where the answer never crossed the bridge
  /// at all and `At home only` produced the Tailscale screen. With one path the Mac plans the
  /// one path, and `phone_begin_pairing` takes no argument on either side of the bridge.
  ///
  /// **AND IT CAN NOW FAIL IN A WAY THE USER MUST READ.** On the old flow a Mac that could not
  /// get a Tailscale certificate quietly served the other path instead. There is no other path,
  /// so the Mac refuses with a sentence and this writes it out — on `#phone-message`, which for
  /// exactly this reason no longer lives inside the screen that is hidden when it happens.
  async function begin(toCode) {
    if (busy) return;
    busy = true;
    // NOT "making a certificate for your phone" any more: nothing is made FOR the phone on this
    // path, and the sentence described the work the other one did.
    field("phone-message").textContent = "Getting this Mac ready…";
    field("phone-connect-start").disabled = true;
    field("phone-ts-start").disabled = true;
    field("phone-refresh").disabled = true;
    // The rejection screen's own control calls this too, so it is held with the other two:
    // two presses would ask for two windows.
    field("phone-rejected-again").disabled = true;
    try {
      const route = latestStatus && (latestStatus.servingVia || latestStatus.pairedVia) || selectedRoute || (latestStatus && latestStatus.connect ? "connect" : "tailnet");
      render(await bridge.invoke(route === "connect" ? "phone_connect_enable" : "phone_begin_pairing"));
      field("phone-message").textContent = "";
      if (toCode) scrollToCode();
    } catch (error) {
      field("phone-message").textContent = String(error);
    } finally {
      busy = false;
      field("phone-connect-start").disabled = false;
      field("phone-ts-start").disabled = false;
      field("phone-refresh").disabled = false;
      field("phone-rejected-again").disabled = false;
    }
  }

  // `() => begin(false)` rather than the bare function, because a DOM listener is called with
  // the EVENT as its first argument — `begin(event)` would be truthy and every one of these
  // would scroll.
  //
  // TWO DOORS INTO THE PAIRING SCREEN, NOT THREE. `#phone-start`, on the screen the other route
  // landed on, went with that route.
  field("phone-connect-start").addEventListener("click", () => begin(false));
  field("phone-use-connect").addEventListener("click", () => { selectedRoute = "connect"; render(latestStatus); });
  async function disableConnect(useTailnet) {
    if (busy) return;
    busy = true;
    field("phone-connect-disable").disabled = true;
    try {
      const status = await bridge.invoke("phone_connect_disable");
      selectedRoute = useTailnet ? "tailnet" : "connect";
      render(status);
      field("phone-message").textContent = "";
    } catch (error) { field("phone-message").textContent = String(error); }
    finally { busy = false; field("phone-connect-disable").disabled = false; }
  }
  field("phone-use-tailnet").addEventListener("click", () => {
    if (latestStatus && latestStatus.connect && latestStatus.connect.enabled) return disableConnect(true);
    selectedRoute = "tailnet"; render(latestStatus);
  });
  field("phone-connect-disable").addEventListener("click", () => disableConnect(false));
  field("phone-ts-start").addEventListener("click", () => begin(false));
  // **THE ONE WAY OFF THE SCREEN THAT SAYS THE MAC STOPPED.** It is `begin` and not a
  // dismiss: the flag lives on the Mac (`PhoneRuntime::rejected`) and is cleared by
  // `phone_begin_pairing` and by nothing else, so a control that only redrew this sheet would
  // put the screen straight back on the next open. `false`, so it lands at the top of the
  // pairing screen rather than scrolled to the code — he is starting over, not refreshing.
  field("phone-rejected-again").addEventListener("click", () => begin(false));
  field("phone-refresh").addEventListener("click", () => begin(true));
  field("phone-close").addEventListener("click", close);

  // SCREEN 0's one control. "I understand" is not a preference and is not remembered: it lasts
  // as long as this open. The `Pick a different way` that used to sit beside it went with the
  // chooser — there is nothing to pick between, and Close is on every screen.
  field("phone-identity-ok").addEventListener("click", async () => {
    identityUnderstood = true;
    await refresh();
  });
  // AND THE LINKS ARE LIVE. The target is read off the control the render put it on, so the only
  // strings that can ever be asked for are the ones in `LINKS` — and `opener.rs` refuses anything
  // that is not one of its five entries anyway.
  field("phone-ts-open").addEventListener("click", () =>
    openExternal(field("phone-ts-open").dataset.target || "")
  );
  // **`Stop and go back`, AND IT IS THE ONLY BACK CONTROL WITH SOMETHING TO PUT DOWN** —
  // Urban's G2, under a name that survives §61.
  //
  // The four copies that used to sit beside it moved between screens that were waiting on the
  // user, and they all led to the route chooser; with the chooser gone they would return to the
  // screen they were pressed on, which is a control that does nothing. This one is pressed on a
  // screen with a live code and an open socket behind it, so it cannot be a redraw either way:
  // `pairing` is true while the Mac is listening and unpaired, so the Mac has to be asked to
  // stop before this screen can be left. The render that follows uses the status that call
  // returns rather than a second round trip — one answer, no window in which the two disagree.
  //
  // WHERE IT LANDS: `This Mac is ready`, because `stop_pairing` leaves a Mac that is signed in
  // and certified exactly where it was, minus the window.
  field("phone-pairing-back").addEventListener("click", async () => {
    if (busy) return;
    busy = true;
    field("phone-message").textContent = "";
    // The ticker is stopped before the call rather than after the render: it fires once a
    // second and calls `refresh()` at zero, and a countdown for a window that is being closed
    // is the one thing on this screen that must not outlive it.
    stopTicking();
    try {
      render(await bridge.invoke("phone_stop_pairing"));
    } catch (error) {
      field("phone-message").textContent = String(error);
    } finally {
      busy = false;
    }
  });
  // **THE TWO ANSWERS TO THE SIX WORDS, GIVEN ON THIS MAC** — Sage F1. "They match" is the only
  // act that lets the phone reach Rich; "They do not match" is the same teardown the phone's own
  // answer runs, and the screen that follows says who stopped it. Each renders the status the
  // Mac returns, so the card never shows a state the Mac is not in.
  async function answerTheWords(command) {
    if (busy) return;
    busy = true;
    field("phone-message").textContent = "";
    field("phone-mac-match").disabled = true;
    field("phone-mac-mismatch").disabled = true;
    try {
      render(await bridge.invoke(command));
    } catch (error) {
      field("phone-message").textContent = String(error);
      await refresh();
    } finally {
      busy = false;
      field("phone-mac-match").disabled = false;
      field("phone-mac-mismatch").disabled = false;
    }
  }
  field("phone-mac-match").addEventListener("click", () => answerTheWords("phone_confirm_on_mac"));
  field("phone-mac-mismatch").addEventListener("click", () => answerTheWords("phone_reject_on_mac"));

  // `Check again` — the button for the impatient and for the failure. The poll is invisible, and a
  // detection path with no manual retry is a dead end the moment detection is wrong once.
  field("phone-ts-recheck").addEventListener("click", refresh);
  field("phone-forget").addEventListener("click", async () => {
    if (busy) return;
    busy = true;
    field("phone-message").textContent = "";
    try {
      await bridge.invoke("phone_forget");
      await refresh();
    } catch (error) {
      field("phone-message").textContent = String(error);
    } finally {
      busy = false;
    }
  });

  sheet.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      close();
    }
    if (event.key === "Tab") {
      const targets = [...sheet.querySelectorAll("button")].filter(
        (node) => !node.disabled && node.offsetParent !== null
      );
      if (!targets.length) return;
      if (event.shiftKey && document.activeElement === targets[0]) {
        event.preventDefault();
        targets.at(-1).focus();
      } else if (!event.shiftKey && document.activeElement === targets.at(-1)) {
        event.preventDefault();
        targets[0].focus();
      }
    }
  });

  /// **WHAT THE SETTINGS ROW SAYS WHEN THE SHEET IS SHUT** — Ray's candidate-.16 defect D2.
  ///
  /// *"With port 8443 open and code 56WS37AH live, the Settings panel reads `Use Rich from your
  /// phone >`. Nothing else. Byte-for-byte the same row as with no code at all, and the same
  /// again while a phone is paired."*
  ///
  /// Three states, two of which have something to say:
  ///
  ///   * paired    -> the phone's own name, which is the one fact that identifies it
  ///   * code live -> that it is live AND how long is left, in `remaining()`'s units — the same
  ///                  sentence the countdown on the sheet is built from, so the row and the sheet
  ///                  cannot state different amounts of time
  ///   * anything else -> nothing, and the row reads exactly as it always did. "Not pairing" is
  ///                  not a state worth a line; a live code and a paired phone are.
  ///
  /// SPEAKABLE, because this app is read out loud (CEO, 2026-09-04): "Paired with Pixel 8" and
  /// "A code is live for 4 more minutes" mean the same thing spoken as written, with no glyph,
  /// no abbreviation and no number whose unit has to be inferred.
  ///
  /// **The expired state deliberately says nothing.** The socket stays up after a window closes
  /// (see `listening` in `render`), so a Mac that is merely still serving is not a Mac with
  /// anything live on it, and a row that said so would be telling him about a code that no
  /// longer works.
  function settingsLine(status) {
    if (status.paired) return "Paired with " + (status.deviceName || "your phone");
    const left = status.pairingSecondsLeft;
    if (Number.isFinite(left) && left > 0) return "A code is live for " + remaining(left);
    return "";
  }

  /// Read once, on the explicit click that opens the menu — never on a timer. See the call site
  /// in `settings-button.js`'s `open()`. A failure leaves the row bare rather than putting an
  /// error into a settings menu: the sheet behind the row is where a phone error is read.
  async function describeForSettings() {
    try {
      window.RichSettings.setPhoneState(settingsLine(await bridge.invoke("phone_status")));
    } catch (error) {
      window.RichSettings.setPhoneState("");
    }
  }

  window.RichSettings.registerPhone({ open, onOpen: describeForSettings });
})();
