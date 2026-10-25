// RichOS UI — dev mock harness. ONLY active when `window.__TAURI__` is absent (i.e. this
// page is opened directly in a browser, not inside the Tauri shell). Lets design and QA
// exercise every state in the v1 front-end UX direction §6 — first-run,
// streaming, working, thread switching, voice, proactive, drill-down — with NO live Claude
// and NO Tauri window. Simulates the exact `rich://*` payload shapes from app/STREAMING.md
// and the exact command return shapes from app/src-tauri/src/main.rs, so main.js never
// branches on "am I mocked" — it only ever talks to `window.RichBridge`.
//
// Never shipped-live behavior: in the real Tauri build this file loads but no-ops (guarded
// by the `window.__TAURI__` check below), so it is inert in production.
(function () {
  if (window.__TAURI__) return; // real shell present — this harness stays dormant.

  const now = () => Date.now();

  // §15/§63 appearance state, and the person. `theme` starts SYSTEM because that is
  // `Theme::default()` in `crates/richos-core/src/config.rs` — CEO §63, "system should be
  // the default for a freshly installed app" — and `user_name` starts NULL because "nobody
  // has said who this is" is the state the product actually ships in.
  //
  // IT STANDS IN FOR `Theme::default()` AND MUST NOT DRIFT FROM IT. This one literal is
  // what every browser walk that does not seed a theme actually opens on, so a harness
  // still saying "dark" after the Rust said "system" would make every unseeded suite
  // measure a product that no longer ships. `tests/appearance.js` check 1 reads the Rust
  // default out of config.rs and asserts this side matches, so the drift is a failing
  // check rather than a silent one.
  //
  // IT IS DURABLE, because the thing it stands in for is. `config.rs` survives a relaunch,
  // so a harness whose "stored preference" evaporated on reload would model the one
  // property these settings exist to have — and would make the whole reconciliation path
  // (`get_appearance` -> `RichTheme.sync`, backend wins) untestable in a browser, since the
  // backend would answer with the shipped default every single time. One key, its own
  // namespace, and never the mirror's keys: this is the STORE, not the cache of it.
  // THE PHONE CHANNEL'S HARNESS STATE. Six words from the same list the Mac and the phone both
  // use, so a browser walk shows what he would actually read rather than lorem.
  // **THE PAIRING WINDOW, AND IT IS THE MAC'S NUMBER.** `src-tauri/src/phone/mod.rs`'s
  // `PAIRING_WINDOW_MS`. It was sixty seconds and it is five minutes: Ray's candidate .11 defect
  // 3.3 measured the errand this screen asks for — read it, walk to the phone, unlock it, open
  // the camera — against sixty seconds and the code was gone. A harness that modeled a different
  // window would draw a countdown the product never shows, so `ui/tests/phone.js` check 8c reads
  // the constant out of the Rust and asserts this line matches it.
  const PAIRING_WINDOW_MS = 300000;
  let mockConnect = window.__RICHOS_MOCK_PRESET__?.phoneConnect || null;
  const mockPhone = {
    paired: window.__RICHOS_MOCK_PRESET__?.phonePaired === true,
    pairing:
      window.__RICHOS_MOCK_PRESET__?.phonePairing === true ||
      window.__RICHOS_MOCK_PRESET__?.phonePairingExpired === true,
    // A FOURTH STATE, and it is the one a real person meets most often: he opened the screen,
    // went to find his phone, and came back after the sixty seconds were up. Reaching it by
    // waiting a minute is not a test anybody runs, so the harness starts the clock in the past.
    openedAt:
      window.__RICHOS_MOCK_PRESET__?.phonePairingExpired === true
        ? now() - (PAIRING_WINDOW_MS + 1000)
        : now(),
    // THE RECORDED PATH AND PLATFORM (`device.rs`: `paired_via`, `platform`). The default is
    // what this build writes — there is one way to pair (CEO §61) — and the preset is how a
    // record written by an older build, which had a second path, is reached without one.
    pairedVia:
      window.__RICHOS_MOCK_PRESET__?.phonePairedVia === undefined
        ? "tailnet"
        : window.__RICHOS_MOCK_PRESET__.phonePairedVia,
    platform:
      window.__RICHOS_MOCK_PRESET__?.phonePlatform === undefined
        ? "ios"
        : window.__RICHOS_MOCK_PRESET__.phonePlatform,
    // **THE PERSON PRESSED `They do not match` ON THE PHONE** — `PhoneStatus::rejected`, Ray's
    // nightly `.8` defect 1.
    //
    // It is STATE here and not a preset read at answer time, because on the Mac it is state:
    // `PhoneRuntime::rejected` is set when the channel stops and cleared by
    // `begin_pairing` and by nothing else. A harness that read the preset every time could not
    // model the one transition off this screen.
    //
    // Deliberately INDEPENDENT of `phonePaired` and `phonePairing`: a Mac that has stopped is
    // not listening and has nothing paired, which is character for character the status that
    // draws `This Mac is ready`. Telling those two apart is the whole point of the field.
    rejected: window.__RICHOS_MOCK_PRESET__?.phoneRejected === true,
    // WHO STOPPED IT (`device::StoppedBy`), for the sentence. The phone's answer is the default,
    // which is what `phoneRejected` always meant.
    rejectedBy: window.__RICHOS_MOCK_PRESET__?.phoneRejectedBy || "phone",
    // **HAS A PERSON PRESSED "They match" ON THIS MAC?** — Sage F1, `PhoneStatus::mac_confirmed`.
    // Confirmed by default for the same reason `fingerprintConfirmed` is: an established pairing
    // is what almost every check on the paired card is about. `phoneMacUnconfirmed: true` is the
    // state between the code being redeemed and the press — the only state with the words and
    // the two buttons on the card.
    macConfirmed: window.__RICHOS_MOCK_PRESET__?.phoneMacUnconfirmed !== true,
    // **HAS THE PHONE USED THE PAIRING SINCE THIS MAC SAID YES?** — `PhoneStatus::completed`,
    // Sage's pair-v2 hypotheses review §2. Used by default, like the two answers above;
    // `phoneUnfinished: true` is the state between the press on this Mac and the phone's first
    // ordinary request, the only state that reads `Waiting for your phone to finish`.
    unfinished: window.__RICHOS_MOCK_PRESET__?.phoneUnfinished === true,
    // The spent-code alarm after the press ("keep it and warn").
    codeReused: window.__RICHOS_MOCK_PRESET__?.phoneCodeReused === true,
  };
  // WHERE THIS MAC IS ON THE TAILSCALE PATH, for the harness. The shipped sheet draws its four
  // Tailscale screens from `status.tailnet` and nothing else, so without this the whole of CEO
  // §61's path was unreachable in a browser — which is exactly why it shipped with no browser
  // check over it. The shape is `TailnetView` (`src-tauri/src/phone/mod.rs`): a state token, a
  // name, an origin, a sentence, the account phrase, the phone's name and whether it is online.
  //
  // The default is `absent`, which is what a Mac with no Tailscale reports and what the real
  // `TailnetView::from(&TailnetState::Absent)` serializes.
  const TAILNET_ABSENT = {
    state: "absent",
    name: null,
    origin: null,
    sentence:
      "Tailscale is not on this Mac. Installing it here and on your phone and signing in to the " +
      "same free account on both, is what lets you reach me when you are away from your desk.",
    account: null,
    phone: null,
    phoneOnline: false,
  };
  function tailnetOf() {
    const preset = window.__RICHOS_MOCK_PRESET__?.phoneTailnet;
    if (!preset) return TAILNET_ABSENT;
    return Object.assign({}, TAILNET_ABSENT, preset);
  }
  function phoneStatusOf() {
    const elapsed = now() - mockPhone.openedAt;
    // **`phonePairingSecondsLeft` PINS THE ANSWER; IT DOES NOT START THE CLOCK IN THE PAST.**
    //
    // Urban's G13 lives at 61 seconds and at no other value, and the first version of this knob
    // set `openedAt` backwards so the running clock would arrive there. It never did: `openedAt`
    // is fixed when this file loads, the sheet is not opened until several seconds later, and by
    // the time anything read the status the window had drifted to 60 or below — where `ceil` and
    // `floor` give the same answer. The proof is on the record: the check ran against `2c95c27d`,
    // which still had `Math.ceil`, and printed `61s -> "This code lasts 1 more minute."` — the
    // FIXED tree's output, from the broken tree. A green that cannot go red.
    //
    // Pinned, the value does not move between polls, so what the screen says is what the number
    // asked for. The clock still runs for every other preset, including the expired one.
    const pinned = window.__RICHOS_MOCK_PRESET__?.phonePairingSecondsLeft;
    const secondsLeft = Number.isFinite(pinned)
      ? Math.max(0, pinned)
      : Math.max(0, Math.ceil((PAIRING_WINDOW_MS - elapsed) / 1000));
    const open = mockPhone.pairing && !mockPhone.paired && secondsLeft > 0;
    // **WHICH PATH THE OPEN WINDOW IS BEING SERVED OVER** — `PhoneStatus::serving_via`.
    //
    // There is one path (CEO §61), so the Mac's own answer is `"tailnet"` while a channel is up
    // and `null` when nothing is listening — `serving_plan` has no second plan to return, and a
    // Mac it cannot plan for refuses to start rather than serving something else. The harness
    // says the same thing with the same two values.
    //
    // `phoneServingVia` remains as an override so a suite can pin a value the flow itself cannot
    // produce; nothing in the shipped flow reads anything but `"tailnet"`.
    //
    // It is a local rather than an inline field because the PAIRING URL is drawn from it too —
    // see `pairUrl` below.
    const servingVia =
      mockPhone.paired || mockPhone.pairing
        ? window.__RICHOS_MOCK_PRESET__?.phoneServingVia || (mockConnect?.enabled ? "connect" : "tailnet")
        : null;
    return {
      ...(mockConnect ? { connect: mockConnect } : {}),
      listening: window.__RICHOS_MOCK_PRESET__?.phoneListenerStopped === true ? false : mockPhone.paired || mockPhone.pairing,
      paired: mockPhone.paired,
      // WHAT THE PAIRED PHONE IS AND HOW IT GOT HERE — the two fields `PhoneStatus` grew for
      // Ray's defect 3.2, and the reason the harness can reach all four combinations. The
      // device NAME follows the platform rather than being pinned to "iPhone", because the
      // defect was a card reading "Android phone is paired" over iOS instructions and a
      // harness that could only ever be an iPhone could not have shown it.
      deviceName: mockPhone.paired ? (mockPhone.platform === "android" ? "Android phone" : "iPhone") : null,
      pairedVia: mockPhone.paired ? mockPhone.pairedVia : null,
      servingVia,
      platform: mockPhone.paired ? mockPhone.platform : null,
      pushReady: mockPhone.paired && window.__RICHOS_MOCK_PRESET__?.phonePushReady === true,
      pushTransport: window.__RICHOS_MOCK_PRESET__?.phonePushTransport || "web-push",
      // **HAS THE PERSON ANSWERED THE SIX WORDS?** Ray's nightly `.7`, defect 2: the Mac read
      // `It is paired` while the phone was still asking, so the card now waits for the phone to
      // come back with `fingerprint_confirmed` (`phone/routes.rs`, `POST /api/pair`).
      //
      // The default is CONFIRMED, and that is deliberate rather than convenient: it is what
      // every screen of an established pairing looks like, which is the state almost every
      // check on this card is about. `phoneFingerprintUnconfirmed: true` reaches the window
      // between the code being redeemed and the person answering — the state that used to be
      // undrawable, which is why nothing caught the defect.
      fingerprintConfirmed:
        mockPhone.paired && window.__RICHOS_MOCK_PRESET__?.phoneFingerprintUnconfirmed !== true,
      // **AND THE CODE GOES OUT UNDER THE ORIGIN THAT IS ACTUALLY SERVING IT.** The Mac builds
      // this string from `Running::origin`, which is `serving_plan`'s answer — and there is one
      // answer now, the tailnet name. A suite that opens a window without a `ready` tailnet is
      // modeling a Mac the shipped flow refuses to start a channel on, so it gets a placeholder
      // origin rather than a second real one: `TAILNET_ABSENT.origin` is null and the string
      // below is the tell that the fixture, not the product, chose it.
      pairUrl: open
        ? ((servingVia === "connect" ? mockConnect.endpoint : tailnetOf().origin) || "https://mm1.example.ts.net:8443") + "/#pair=K7QF2M9X"
        : null,
      macConfirmed: mockPhone.paired && mockPhone.macConfirmed,
      completed: mockPhone.paired && mockPhone.macConfirmed && !mockPhone.unfinished,
      // THE WORDS EXIST ONLY WHILE A PHONE THAT REACHED THIS MAC WAITS FOR THE PRESS (Sage §3.1
      // steps 1 and 4): the Mac derives them over the key that phone registered, so before a phone
      // arrives there is nothing to derive, and after the press nothing is left to compare.
      fingerprintWords:
        mockPhone.paired && !mockPhone.macConfirmed
          ? ["harbor", "candle", "meadow", "lantern", "fossil", "juniper"]
          : [],
      codeReused: mockPhone.paired && mockPhone.codeReused,
      confirmSecondsLeft: mockPhone.paired && !mockPhone.macConfirmed ? 240 : null,
      rejectedBy: mockPhone.rejected ? mockPhone.rejectedBy : "",
      fingerprintHex: mockPhone.paired || mockPhone.pairing ? "3D:9C:2A:5E:7B:11" : null,
      bound:
        mockPhone.paired || mockPhone.pairing
          ? ["127.0.0.1:8443", "192.168.1.249:8443"]
          : [],
      pairingSecondsLeft: open ? secondsLeft : null,
      tailnet: tailnetOf(),
      // **THE PERSON PRESSED `They do not match` ON THE PHONE** — `PhoneStatus::rejected`.
      //
      // Ray's nightly `.8` defect 1. The real Mac reaches this state by stopping: the listener
      // goes, the device record goes, the authority goes, and this is the only thing left
      // saying why. So the preset is deliberately INDEPENDENT of `phonePaired` and
      // `phonePairing` — a Mac that has stopped is not listening and has nothing paired, which
      // is character for character the status that draws `This Mac is ready`, and telling those
      // two apart is the whole point of the field.
      rejected: mockPhone.rejected,
    };
  }

  const MOCK_CONFIG_KEY = "richos-mock-config";
  const mockConfig = (function () {
    // `rich_model`/`stu_model` stand in for config.rs's §119 pair: absent reads as Opus.
    // `oxford_comma` stands in for config.rs's Oxford comma setting: absent reads as off (CEO, 2026-10-19).
    const fresh = { theme: "system", font_scale: 100, user_name: null, rich_model: "opus", stu_model: "opus", oxford_comma: false };
    try {
      const raw = window.localStorage.getItem(MOCK_CONFIG_KEY);
      if (!raw) return fresh;
      const parsed = JSON.parse(raw);
      return {
        theme: ["dark", "light", "system"].includes(parsed.theme) ? parsed.theme : fresh.theme,
        font_scale: Number.isFinite(parsed.font_scale) ? parsed.font_scale : fresh.font_scale,
        user_name: typeof parsed.user_name === "string" && parsed.user_name.trim() ? parsed.user_name : null,
        rich_model: parsed.rich_model === "fable" ? "fable" : "opus",
        stu_model: parsed.stu_model === "fable" ? "fable" : "opus",
        oxford_comma: parsed.oxford_comma === true,
      };
    } catch (e) {
      // A corrupt or unavailable store degrades to the shipped defaults rather than
      // failing the harness — the same posture `ConfigStore::open` takes.
      return fresh;
    }
  })();
  const persistMockConfig = () => {
    try {
      window.localStorage.setItem(MOCK_CONFIG_KEY, JSON.stringify(mockConfig));
    } catch (e) {
      /* storage unavailable; the in-memory value still serves this session */
    }
  };
  // `initials_from` in config.rs, mirrored: first and last, two letters at most, one
  // letter for one token, and null rather than a guess when there is nothing to derive.
  const mockInitials = (name) => {
    if (!name) return null;
    const t = String(name).split(/\s+/).filter(Boolean);
    if (!t.length) return null;
    const first = [...t[0]][0].toUpperCase();
    if (t.length === 1) return first;
    return first + [...t[t.length - 1]][0].toUpperCase();
  };
  const uid = (p) => `${p}_${Math.random().toString(36).slice(2, 10)}`;

  // --- fixture state -------------------------------------------------------
  // SIX COMPANIES THAT BELONG TO NOBODY, at paths that exist on no machine.
  //
  // They were the CEO's own six until 2026-09-04 — real names, bound to absolute roots under
  // his own home directory — mirroring `EntityRegistry::ceos_companies()`, which was a
  // `const` table compiled into the binary. THIS FILE SHIPS: `build.rs` stages `app/ui` into
  // `ui-dist` (excluding `tests/` and `node_modules/`) and Tauri embeds every file under it
  // in the executable. So that fixture put one man's company list and his home directory's
  // name inside every copy of RichOS ever built, where `strings` could read it.
  //
  // The registry is per-user now and the shipping default is EMPTY, so a harness fixture is
  // the only place a company list has any business being. These six keep the SHAPE the
  // surfaces are tested against and none of the identity:
  //
  //   * `harbor` HAS TWO ROOTS AND IS ONE ENTITY — the property the home screen's company
  //     row gets wrong if anything ever builds its list from directories rather than from
  //     the registry, so the harness carries it rather than flattening it;
  //   * `harbor-private` is also a STRING prefix match of `harbor`'s first root, which is
  //     the case component-wise resolution exists for;
  //   * six of them, because several suites assert positions in a six-long row.
  const entities = [
    { id: "northwind", display_name: "Northwind Traders", status: "active", roots: ["/Users/you/Projects/northwind"] },
    { id: "lumen", display_name: "Lumen Labs", status: "active", roots: ["/Users/you/Projects/lumen"] },
    { id: "meridian", display_name: "Meridian Group", status: "active", roots: ["/Users/you/Projects/meridian"] },
    { id: "harbor", display_name: "Harbor Analytics", status: "active", roots: ["/Users/you/Projects/harbor", "/Users/you/Projects/harbor-private"] },
    { id: "tidewater", display_name: "Tidewater Films", status: "active", roots: ["/Users/you/Projects/tidewater"] },
    { id: "kestrel", display_name: "Kestrel Supply", status: "active", roots: ["/Users/you/Projects/kestrel"] },
  ];

  // THE HOME SCREEN'S COMPANY BUTTONS (CEO, 2026-09-01) — his two preferences, mirroring
  // `home_entity_labels` and `home_entity_hidden` in config.rs. Durable there; in-memory here,
  // which is enough: every test that needs them drives both ends in one page.
  //
  // Both start EMPTY, because that is the state every install is in until he opens the panel:
  // no label overrides, nothing hidden. `homeEntityRowOf` resolves them the way the Rust
  // command does — an absent label means the registry's display name, an absent hidden flag
  // means visible — so a surface can never be tested against a resolution the app does not do.
  const homeEntityLabels = {};
  const homeEntityHidden = {};

  function homeEntityRowOf() {
    return entities.map((e) => {
      const custom = typeof homeEntityLabels[e.id] === "string" && homeEntityLabels[e.id].trim()
        ? homeEntityLabels[e.id].trim()
        : null;
      return {
        id: e.id,
        display_name: e.display_name,
        label: custom || e.display_name,
        custom_label: custom,
        visible: homeEntityHidden[e.id] !== true,
      };
    });
  }

  // `entity_id: null` is the LEGACY-THREAD case slice 1 introduced (`ThreadEntity::Unbound`):
  // a record written before entity scoping existed. It is LISTED — an operator has to be
  // able to see it — but every scoped read against it refuses. `legacy` below reproduces
  // that exactly, including the ledger's own refusal message, so the binding-failure state
  // (§21) can be exercised without a pre-entity ledger on disk.
  const threads = [
    { id: "general", title: "Running", entity_id: "harbor", created_at: now() - 1000 * 60 * 60 * 24 * 3, message_count: 0, last_activity: now(), last_turn_state: null, has_pending_turn: false },
    { id: "acme", title: "Acme deal", entity_id: "northwind", created_at: now() - 1000 * 60 * 60 * 20, message_count: 4, last_activity: now() - 1000 * 60 * 30, last_turn_state: "completed", has_pending_turn: false },
    { id: "hiring", title: "Q4 hiring", entity_id: "northwind", created_at: now() - 1000 * 60 * 60 * 40, message_count: 2, last_activity: now() - 1000 * 60 * 60 * 5, last_turn_state: "completed", has_pending_turn: false },
    { id: "partner", title: "Partner book review", entity_id: "lumen", created_at: now() - 1000 * 60 * 60 * 30, message_count: 2, last_activity: now() - 1000 * 60 * 60 * 9, last_turn_state: "interrupted", has_pending_turn: false },
    { id: "ecs", title: "ECS architecture", entity_id: "harbor", created_at: now() - 1000 * 60 * 60 * 50, message_count: 0, last_activity: now() - 1000 * 60 * 60 * 12, last_turn_state: "in_flight", has_pending_turn: true },
    { id: "legacy", title: "Notes from before", entity_id: null, created_at: now() - 1000 * 60 * 60 * 24 * 40, message_count: 0, last_activity: now() - 1000 * 60 * 60 * 24 * 40, last_turn_state: null, has_pending_turn: false },
    // §10.1's thread, EMPTY until the §26 fixture is driven. It exists at load so the
    // scenario starts from "before send" (§10.1) rather than conjuring a thread as its
    // first act — the CEO's first observable moment is his own message landing in a thread
    // that was already there.
    { id: "memory", title: "Design RichOS memory strategy", entity_id: "northwind", created_at: now() - 1000 * 60 * 5, message_count: 0, last_activity: now() - 1000 * 60 * 5, last_turn_state: null, has_pending_turn: false },
  ];
  // A PRE-BOOT SWITCH, and the only one in this file. The launch state it drives — no
  // company chosen, so the app asks — is decided BEFORE `main.js` runs its `init()`, so a
  // setter called afterwards cannot reach it: by then the shell has already branched on
  // whether a thread was active. The browser harness sets this with `addInitScript`, which
  // runs before any of the page's own scripts.
  //
  //   window.__RICHOS_MOCK_PRESET__ = { chosenEntity: null }          // never answered
  //   window.__RICHOS_MOCK_PRESET__ = { pinnedByEnvironment: true }   // RICHOS_ENTITY set
  //   window.__RICHOS_MOCK_PRESET__ = { memory: "none" }              // fresh install, no corpus
  //   window.__RICHOS_MOCK_PRESET__ = { memory: "no-compiler" }       // a corpus it cannot read
  //   window.__RICHOS_MOCK_PRESET__ = { homeField: { objects: 4000, records: 9000 } }
  //                                                                   // his own loro, before the field starts
  //   window.__RICHOS_MOCK_PRESET__ = { onboarding: "not-yet" }       // nothing on file, never asked
  //
  // `homeField` IS THE SECOND STATE A SETTER CANNOT REACH, and it is measured rather than
  // assumed. `home.js` starts the field from its own `requestIdleCallback`, and that callback
  // asks `home_field_data` at ~348-358ms from navigation on this machine (five runs:
  // 358.0, 349.0, 348.0, 352.0, 347.0ms). A browser harness gets its first turn at ~85-98ms,
  // so `__RICHOS_MOCK__.homeFieldSet()` called after load wins that race by about 260ms and
  // loses it on any machine slower than this one — home.js's own `RICHOS_SPLASH_LAG_MS` note
  // records the harness's next instruction landing ~2,000ms late on a GitHub `macos-latest`
  // runner, which is 7.6x the whole margin. The answer arrives before the page can ask, or it
  // is a coin toss decided by machine load.
  const preset = (typeof window !== "undefined" && window.__RICHOS_MOCK_PRESET__) || {};
  let activeThreadId = "chosenEntity" in preset && !preset.chosenEntity ? null : "general";

  // Verbatim from `LedgerError::UnboundThread` (ledger.rs). Copied rather than paraphrased
  // so the harness cannot drift from the sentence the real backend raises.
  const UNBOUND_ERR = (id) =>
    "thread " + id + " has no entity binding: it predates entity scoping, and Rich will not guess " +
    "which entity this work belongs to. An operator must bind it explicitly.";

  const navPrefs = {
    sidebar_width: 300,
    inspector_width: 336,
    output_width: 400,
    sidebar_collapsed: false,
    collapsed_entities: [],
    pinned_threads: [],
    archived_threads: [],
    renamed_threads: {},
  };

  // THE OUTPUT PANEL'S SPLIT WIDTH SURVIVES A RELOAD, as `navigation.json` survives a relaunch
  // (output side-panel PRD §9.5). Its own key, beside the sidebar's below; "open completely" is
  // never stored, here or in nav.rs.
  const MOCK_OUTPUT_WIDTH_KEY = "richos-mock-output-width";
  try {
    const stored = Number(window.localStorage.getItem(MOCK_OUTPUT_WIDTH_KEY));
    if (Number.isFinite(stored) && stored > 0) navPrefs.output_width = stored;
  } catch (e) {
    /* storage unavailable: the shipped default, 400 */
  }

  // WHETHER THE SIDEBAR IS AWAY SURVIVES A RELOAD, as `navigation.json` survives a relaunch
  // (output side-panel PRD §8: "the choice survives a relaunch"). Its own key, kept beside
  // `richos-mock-config` the same way, and NOT the page's first-paint mirror
  // (`richos-sidebar-hidden`): a suite must be able to set one without the other to prove that
  // nav.rs, not the mirror, decides. A fresh browser context has neither, and boots open.
  const MOCK_SIDEBAR_KEY = "richos-mock-sidebar-collapsed";
  try {
    navPrefs.sidebar_collapsed = window.localStorage.getItem(MOCK_SIDEBAR_KEY) === "1";
  } catch (e) {
    /* storage unavailable: the shipped default, open */
  }
  const persistMockSidebar = () => {
    try {
      window.localStorage.setItem(MOCK_SIDEBAR_KEY, navPrefs.sidebar_collapsed ? "1" : "0");
    } catch (e) {
      /* storage unavailable; the in-memory value still serves this session */
    }
  };

  function displayTitleOf(t) {
    return navPrefs.renamed_threads[t.id] || t.title;
  }

  function threadRowOf(t) {
    return {
      id: t.id,
      title: t.title,
      display_title: displayTitleOf(t),
      entity_id: t.entity_id,
      binding_revision: t.entity_id ? 1 : 0,
      created_at: t.created_at,
      last_activity: t.last_activity,
      message_count: (messagesByThread[t.id] || []).length,
      pinned: navPrefs.pinned_threads.includes(t.id),
      archived: navPrefs.archived_threads.includes(t.id),
      // An unbound thread's turns are NOT read (the scoped accessor refuses), so it
      // reports no state at all — matching `thread_turn_facts` in main.rs.
      last_turn_state: t.entity_id ? t.last_turn_state : null,
      has_pending_turn: t.entity_id ? !!t.has_pending_turn : false,
    };
  }

  function setMembership(list, id, member) {
    const at = list.indexOf(id);
    if (member && at < 0) list.push(id);
    if (!member && at >= 0) list.splice(at, 1);
  }

  /** thread_id -> Message[] ({ role, text, turn_id, at }) — matches ledger::Message field
   * names verbatim (snake_case, no serde rename in the Rust struct). */
  // IMPORTANT: each user+assistant pair below shares ONE turn_id (a fresh `uid("t")` per
  // turn, reused for both lines) — exactly like the real ledger (ledger.rs `messages()`:
  // both the user and assistant Message for a turn are stamped `t.id.clone()` from the
  // SAME Turn). Giving each line its own id (an earlier bug here) made every ordinary
  // reply misfire the proactive "reached out" heuristic in main.js — a good example of why
  // the fixture shape has to mirror the real data model exactly, not just look plausible.
  const acmeTurn1 = uid("t");
  const acmeTurn2 = uid("t");
  const hiringTurn1 = uid("t");
  const partnerTurn1 = uid("t");
  const messagesByThread = {
    general: [],
    memory: [], // §26's fixture thread — filled only when the scenario is driven.
    acme: [
      { role: "user", text: "what's the status on Acme?", turn_id: acmeTurn1, at: now() - 1000 * 60 * 60 * 20 },
      {
        role: "assistant",
        text: "Their counter came in this morning — 8% below list. I've pulled comparables and it's within range. Want me to draft a response or do you want to see the comps first?",
        turn_id: acmeTurn1,
        at: now() - 1000 * 60 * 60 * 20 + 4000,
      },
      { role: "user", text: "draft it, keep it firm", turn_id: acmeTurn2, at: now() - 1000 * 60 * 30 },
      {
        role: "assistant",
        text: "Done — firm counter drafted, holding at list minus 3%. Sitting in your review queue.",
        turn_id: acmeTurn2,
        at: now() - 1000 * 60 * 30 + 3000,
      },
    ],
    hiring: [
      { role: "user", text: "where are we on the Q4 reqs?", turn_id: hiringTurn1, at: now() - 1000 * 60 * 60 * 5 },
      {
        role: "assistant",
        text: "Three of five roles have candidates in final round. The platform-eng req is still thin — I've asked the recruiter for a wider pass.",
        turn_id: hiringTurn1,
        at: now() - 1000 * 60 * 60 * 5 + 2500,
      },
    ],
    partner: [
      { role: "user", text: "how did the partner book review land?", turn_id: partnerTurn1, at: now() - 1000 * 60 * 60 * 9 },
      {
        role: "assistant",
        text: "Two partners pushed back on the carry split. I have the numbers but I stopped short of a recommendation — I want your read on the Hensley relationship first.",
        turn_id: partnerTurn1,
        at: now() - 1000 * 60 * 60 * 9 + 3000,
      },
    ],
    ecs: [],
    legacy: [],
  };

  // TURN RECORDS — the harness's stand-in for `ledger::Turn`, holding exactly the fields
  // `Timeline::project` reads to build a `work_duration` row. Without them the harness could
  // only fake a timeline; with them it PROJECTS one, from the same fields, using the same
  // derived item ids (`{turnId}:user`, `{turnId}:text:{n}`, `{turnId}:duration`).
  //
  // This is NOT §26's deterministic `memory-strategy` fixture — that is slice 8, with its own
  // scripted 16-step scenario and injectable clock. This is the existing canned turn, given
  // the shape the §13 contract actually has, so the browser harness is not dead against the
  // renderer that ships.
  const turnsById = new Map(); // turnId -> { threadId, entityId, userText, runs, state, createdAt, startedAt, endedAt }
  // thread_id -> the `rich_file` timeline items `rich_files::item` writes (round 36 screen 3).
  // Empty unless a suite asks for the `{ richFiles: "round-36" }` preset.
  const richFilesByThread = {};
  // What `rich_file_open` / `rich_file_reveal` were asked to do, for a suite to read.
  const richFileActions = [];

  function seedTurn(threadId, turnId, userText, replyText, at, durationMs) {
    const t = threads.find((x) => x.id === threadId);
    turnsById.set(turnId, {
      threadId,
      entityId: t ? t.entity_id : null,
      userText,
      runs: replyText ? [{ text: replyText, startSeq: 0, at: at + 2000 }] : [],
      state: "completed",
      createdAt: at,
      startedAt: at,
      endedAt: at + durationMs,
      activities: [],
    });
  }

  /// The harness's `Timeline::project` + `view(ViewMode::Ceo)`, in the same ORDER and with
  /// the same derived ids as timeline.rs. Item order within a turn is
  /// `(slot, sequence)` — opening, then the stream in shared-counter order, then terminal —
  /// which is `TimelineBase::order_key`.
  function projectTimeline(threadId, mode) {
    const technical = mode === "technical" || mode === "technical-empty";
    // `technical-empty` is the TECHNICAL view of a thread whose journal gave nothing back —
    // unreadable, or never written. The mode on the wire is still `technical`; what is
    // absent is the rows, exactly as `Spine::timeline` produces them from an empty read.
    const noMachinery = mode === "technical-empty";
    const t = threads.find((x) => x.id === threadId);
    const items = [];
    const rev = t && t.entity_id ? 1 : 0;
    const baseOf = (turn, id, seq, slot, at) => ({
      id,
      entityId: turn.entityId,
      threadId: turn.threadId,
      turnId: null, // set by the caller
      bindingRevision: rev,
      createdAt: at,
      sequence: seq,
      slot,
      visibility: "ceo",
    });
    for (const [turnId, turn] of turnsById) {
      if (turn.threadId !== threadId) continue;
      // A SUPERSEDED TURN CONTRIBUTES NOTHING. `Timeline::project` demotes it wholesale to
      // `Internal` (`turn.superseded_by.is_some()` -> `internal_turn`), and `view(Ceo)`
      // removes it — so the CEO sees ONE clean exchange rather than a duplicated prompt.
      // Reproduced here because without it the harness projects something the real backend
      // never would, and would hide exactly the defect this case exists to catch.
      if (turn.supersededBy) continue;
      if (turn.userText) {
        items.push(
          Object.assign(baseOf(turn, turnId + ":user", null, "opening", turn.createdAt), {
            kind: "user_message",
            turnId,
            text: turn.userText,
            source: "text",
          })
        );
      }
      turn.runs.forEach((run, idx) => {
        items.push(
          Object.assign(baseOf(turn, turnId + ":text:" + idx, run.startSeq, "stream", run.at), {
            kind: "rich_message",
            turnId,
            // ALWAYS "unknown" for a streamed reply — `STREAMED_MESSAGE_PHASE` (live.rs). A line
            // Rich raised himself is "proactive", the one real phase (the ledger records it).
            phase: turn.proactive ? "proactive" : "unknown",
            text: run.text,
          })
        );
      });
      for (const a of turn.activities) {
        // `Timeline::view` REMOVES the technical half from a CEO view rather than masking
        // it, and drops `Visibility::Technical` items outright. Reproduced here — a mock
        // that merely hid them would let a renderer bug through that the real backend makes
        // structurally impossible.
        if (noMachinery) continue;
        if (!technical && a.visibility === "technical") continue;
        const item = Object.assign({}, a, { turnId, bindingRevision: rev });
        if (!technical) delete item.detail;
        items.push(item);
      }
      // `active_ms` is MEASURED (`ended_at - started_at`) and is null whenever either
      // endpoint is missing — never `now() - startedAt` (§6.3).
      const activeMs =
        typeof turn.startedAt === "number" && typeof turn.endedAt === "number" && turn.endedAt >= turn.startedAt
          ? turn.endedAt - turn.startedAt
          : null;
      const dur = Object.assign(baseOf(turn, turnId + ":duration", null, "terminal", turn.createdAt), {
        kind: "work_duration",
        turnId,
        state: turn.state,
        startedAt: turn.startedAt,
        endedAt: turn.endedAt,
      });
      if (activeMs !== null) dur.activeMs = activeMs;
      items.push(dur);
    }
    return {
      entityId: t ? t.entity_id : null,
      threadId,
      mode: technical ? "technical" : "ceo",
      items,
      // §1.5's lane travels inside the gated view, beside the items — never as a second
      // command. `technical-empty` (an unreadable or never-written store) gives nothing
      // back here for the same reason it gives no activity rows: the journal answered with
      // nothing, and this is a projection of the journal.
      betweenTurns: noMachinery ? [] : betweenTurnsFor(threadId, technical ? "technical" : "ceo"),
    };
  }

  // The canned history above, as turn records. Durations are deliberately spread across
  // three of §6.2's four display bands so the format is exercised, not just the code path:
  //   acmeTurn1   4_207_000ms -> "1h 10m 7s"   (hour band)
  //   acmeTurn2     247_000ms -> "4m 7s"       (minute band)
  //   hiringTurn1    18_360ms -> "18s"         (second band)
  //   partnerTurn1  interrupted, endedAt absent -> no number at all
  seedTurn("acme", acmeTurn1, "what's the status on Acme?",
    "Their counter came in this morning — 8% below list. I've pulled comparables and it's within range. Want me to draft a response or do you want to see the comps first?",
    now() - 1000 * 60 * 60 * 20, 4207000);
  seedTurn("acme", acmeTurn2, "draft it, keep it firm",
    "Done — firm counter drafted, holding at list minus 3%. Sitting in your review queue.",
    now() - 1000 * 60 * 30, 247000);
  seedTurn("hiring", hiringTurn1, "where are we on the Q4 reqs?",
    "Three of five roles have candidates in final round. The platform-eng req is still thin — I've asked the recruiter for a wider pass.",
    now() - 1000 * 60 * 60 * 5, 18360);
  seedTurn("partner", partnerTurn1, "how did the partner book review land?",
    "Two partners pushed back on the carry split. I have the numbers but I stopped short of a recommendation — I want your read on the Hensley relationship first.",
    now() - 1000 * 60 * 60 * 9, 0);
  // A turn that ended without finishing and never wrote an end time: `active_ms` is None
  // FOREVER for it (ledger.rs), so the row must claim no number.
  {
    const p = turnsById.get(partnerTurn1);
    p.state = "interrupted";
    p.endedAt = null;
  }
  // Real semantic activity on the Acme turn, so §5.3's rollup and §6.4's collapse have
  // something to act on. Ids are machinery ids, as `activity_item` derives them.
  //
  // EVERY ROW ALSO CARRIES ITS `detail`, which is what `ViewMode::Technical` keeps and
  // `ViewMode::Ceo` REMOVES — `projectTimeline` strips it for the calm view, exactly as
  // `TimelineItem::redacted` does. The titles, summaries and paths below are the shapes the
  // 2026-08-28 emission probe actually recorded and the `machinery-payload.json` fixture
  // holds: a MERGED title that is the real command (never the opening event's placeholder
  // "Terminal"), an 84-char bounded summary, and `locations` from `[{path}]`.
  turnsById.get(acmeTurn1).activities = [
    { kind: "activity", id: "mach_a1", slot: "stream", sequence: 1, visibility: "ceo",
      entityId: "northwind", threadId: "acme", createdAt: now() - 1000 * 60 * 60 * 20 + 1000,
      activityType: "read", state: "completed", summary: "Read a file", detailRef: "mach_a1",
      detail: { title: "Read comparables/q3-acme.csv", summary: "18 rows, 4 columns",
                locations: ["/Users/you/Projects/northwind/comparables/q3-acme.csv"] } },
    { kind: "activity", id: "mach_a2", slot: "stream", sequence: 2, visibility: "ceo",
      entityId: "northwind", threadId: "acme", createdAt: now() - 1000 * 60 * 60 * 20 + 1100,
      activityType: "read", state: "completed", summary: "Read a file", detailRef: "mach_a2",
      detail: { title: "Read comparables/q3-market.csv", summary: "31 rows, 4 columns",
                locations: ["/Users/you/Projects/northwind/comparables/q3-market.csv"] } },
    { kind: "activity", id: "mach_a3", slot: "stream", sequence: 3, visibility: "ceo",
      entityId: "northwind", threadId: "acme", createdAt: now() - 1000 * 60 * 60 * 20 + 1200,
      activityType: "read", state: "completed", summary: "Read a file", detailRef: "mach_a3",
      detail: { title: "Read notes/hensley-relationship.md", summary: "9 lines",
                locations: ["/Users/you/Projects/northwind/notes/hensley-relationship.md"] } },
    // NO STATUS EVER ARRIVED for this one — 34 of the 58 measured tool events carried none.
    // It must read "outcome not recorded" and must NEVER be folded into done.
    { kind: "activity", id: "mach_a4", slot: "stream", sequence: 4, visibility: "ceo",
      entityId: "northwind", threadId: "acme", createdAt: now() - 1000 * 60 * 60 * 20 + 1400,
      activityType: "command", state: "unknown", summary: "Ran a command", detailRef: "mach_a4",
      detail: { title: "python3 scripts/counter-model.py --list 4200000 --offer 3864000",
                summary: "spread 8.0% \u00b7 within comparable range", locations: [] } },
    { kind: "activity", id: "mach_a5", slot: "stream", sequence: 5, visibility: "ceo",
      entityId: "northwind", threadId: "acme", createdAt: now() - 1000 * 60 * 60 * 20 + 1600,
      activityType: "search", state: "completed", summary: "Searched", detailRef: "mach_a5",
      detail: { title: "grep -rn \"carry split\" notes/", summary: "4 matches", locations: [] } },
    // A FAILED call. The status dot's other terminal value, and the row the CEO most wants
    // to be able to see the output of.
    { kind: "activity", id: "mach_a6", slot: "stream", sequence: 6, visibility: "ceo",
      entityId: "northwind", threadId: "acme", createdAt: now() - 1000 * 60 * 60 * 20 + 1700,
      activityType: "command", state: "failed", summary: "Ran a command", detailRef: "mach_a6",
      detail: { title: "cat comparables/q4-acme.csv", summary: "Exit code 1", locations: [] } },
    // TECHNICAL-ONLY, and therefore ABSENT from the calm view entirely: an untyped vendor
    // frame. `stream_event:message_delta` carries the token accounting the rotation
    // watermark reads, and it is not something Rich DID, so it renders as one dim line here
    // and as nothing at all in the conversation. Slice 3 fixed a live 6:1 noise defect by
    // moving exactly this row out of the CEO view.
    { kind: "activity", id: "mach_a7", slot: "stream", sequence: 7, visibility: "technical",
      entityId: "northwind", threadId: "acme", createdAt: now() - 1000 * 60 * 60 * 20 + 1800,
      activityType: "other", state: "unknown", summary: "Worked", detailRef: "mach_a7",
      detail: { title: "stream_event:message_delta", locations: [],
                vendorKind: "stream_event:message_delta" } },
    // A PERMISSION REQUEST, also technical-only. Auto-approved by the client and
    // recorded as a FACT — never a decision awaiting the CEO. It rendered as a CEO row
    // reading "Requested approval 7 times" until 2026-08-29, which manufactured demand for
    // an approval queue that does not exist.
    { kind: "activity", id: "mach_a8", slot: "stream", sequence: 8, visibility: "technical",
      entityId: "northwind", threadId: "acme", createdAt: now() - 1000 * 60 * 60 * 20 + 1850,
      activityType: "approval", state: "completed", summary: "Requested approval", detailRef: "mach_a8",
      detail: { title: "python3 scripts/counter-model.py", summary: "auto-approved: allow", locations: [] } },
  ];

  // THREE DELEGATED WORKERS on the Q4-hiring turn (UX §7.1, §26's multi-agent fixture).
  // The payload shape is `TimelineItem::WorkerActivity` verbatim: `kind: "worker_activity"`,
  // a flattened base, and a camelCase `worker` object.
  //
  // The three states are the three the ENGINE CAN ACTUALLY WITNESS, one each — `created`,
  // `started`, `run_ended` (`richos_core::worker_events::ObservedWorkerState`). There is
  // deliberately no `completed`, `failed`, `interrupted` or `waiting` worker in this
  // fixture, because none of those can occur: `WorkerState::from_observed` cannot produce
  // them and a mock that showed one would be teaching the design a state the product does
  // not have.
  turnsById.get(hiringTurn1).activities = [
    { kind: "worker_activity", id: "mach_w1", slot: "stream", sequence: 1, visibility: "ceo",
      entityId: "northwind", threadId: "hiring", createdAt: now() - 1000 * 60 * 60 * 5 + 900,
      detailRef: "mach_w1",
      worker: { agentId: "agt_sage_1", workerName: "Sage", agentType: "architecture",
                observedState: "created", state: "pending_init", eventsObserved: 1,
                firstObservedAt: "2026-08-29T04:00:00+00:00", lastObservedAt: "2026-08-29T04:00:00+00:00" } },
    { kind: "worker_activity", id: "mach_w2", slot: "stream", sequence: 2, visibility: "ceo",
      entityId: "northwind", threadId: "hiring", createdAt: now() - 1000 * 60 * 60 * 5 + 1000,
      detailRef: "mach_w2",
      worker: { agentId: "agt_frank_1", workerName: "Frank", agentType: "red team",
                observedState: "started", state: "running", eventsObserved: 2,
                firstObservedAt: "2026-08-29T04:00:01+00:00", lastObservedAt: "2026-08-29T04:01:44+00:00" } },
    { kind: "worker_activity", id: "mach_w3", slot: "stream", sequence: 3, visibility: "ceo",
      entityId: "northwind", threadId: "hiring", createdAt: now() - 1000 * 60 * 60 * 5 + 1100,
      detailRef: "mach_w3",
      worker: { agentId: "agt_clark_1", workerName: "Clark", agentType: "research",
                observedState: "run_ended", state: "unknown", eventsObserved: 5,
                latestUpdate: "Pulled the platform-eng comparables from three sources",
                firstObservedAt: "2026-08-29T04:00:02+00:00", lastObservedAt: "2026-08-29T04:07:31+00:00" } },
  ];

  // =======================================================================================
  // THE OUTPUT RECORD (`list_output`, `rich://output`) — output side-panel PRD §4.3, §6
  // =======================================================================================
  //
  // `outputByThread[thread]` is the projected list `list_output` answers: Entry objects in the
  // exact camelCase shape `richos_core::output::Entry` serializes, newest first. EMPTY BY
  // DEFAULT for every thread, so every suite that photographs the conversation sees what a
  // thread with no recorded files shows — the two buttons and nothing else.
  //
  // `window.__RICHOS_MOCK_PRESET__ = { output: "round-17" }` loads round 17's world into the
  // `acme` thread: its nine files across four turns (two of them added here, with the
  // mockup's own words), Rich's replies naming files in code spans as he does, and one file a
  // worker wrote. `__RICHOS_MOCK__.outputArrive()` runs the mockup's fifth turn ("make me a
  // one-pager for the board") through the ordinary turn path and announces its two files.
  // `hiring` ("Q4 hiring") is the mockup's empty thread in either case.
  const outputByThread = {};
  let outputUnreadable = false;
  /// VERBATIM from `RECORD_UNREADABLE` in `src-tauri/src/output_files.rs`.
  const OUTPUT_RECORD_UNREADABLE = "I can't read this thread's output record right now.";
  const OUTPUT_HOME = "/Users/you/FemcBoost/";

  /// `out_` + 16 hex digits, stable for (thread, path) — the shape `entry_id` makes, from a
  /// smaller hash, which is all a harness needs.
  function outputId(threadId, path) {
    let a = 0x811c9dc5;
    let b = 0x01000193;
    const s = threadId + "\u0000" + path;
    for (let i = 0; i < s.length; i += 1) {
      a = Math.imul(a ^ s.charCodeAt(i), 0x01000193) >>> 0;
      b = Math.imul(b ^ s.charCodeAt(i), 0x85ebca6b) >>> 0;
    }
    return "out_" + a.toString(16).padStart(8, "0") + b.toString(16).padStart(8, "0");
  }

  function outputKind(name) {
    const ext = (name.split(".").pop() || "").toLowerCase();
    const kinds = {
      md: "md", txt: "txt", json: "txt", csv: "csv", xlsx: "xlsx", numbers: "xlsx",
      docx: "docx", pages: "docx", rtf: "docx", pptx: "pptx", key: "pptx", pdf: "pdf",
      png: "png", jpg: "png", jpeg: "png", gif: "png", webp: "png", heic: "png", svg: "png",
      mp4: "mp4", mov: "mp4", m4v: "mp4", webm: "mp4", m4a: "audio", mp3: "audio", wav: "audio",
    };
    return kinds[ext] || "other";
  }

  function outputEntry(threadId, turnId, folder, name, bytes, writtenAt, extra) {
    const path = OUTPUT_HOME + folder + "/" + name;
    return Object.assign(
      {
        id: outputId(threadId, path),
        threadId,
        turnId,
        firstTurnId: turnId,
        writes: 1,
        path,
        name,
        folder: "~/FemcBoost/" + folder,
        kind: outputKind(name),
        actor: "rich",
        source: "tool",
        exists: true,
        bytes,
        modifiedAt: writtenAt,
        writtenAt,
      },
      extra || {}
    );
  }

  function outputListOf(threadId) {
    const files = (outputByThread[threadId] || []).map((e) => Object.assign({}, e));
    return { files, count: files.length, missing: files.filter((e) => !e.exists).length, corrupt: 0 };
  }

  const acmeTurn3 = uid("t");
  const acmeTurn4 = uid("t");
  if (preset.output === "round-17") {
    const t1 = turnsById.get(acmeTurn1);
    const t2 = turnsById.get(acmeTurn2);
    const reply1 =
      "Their counter came in this morning — 8% below list. I pulled the comparables: two deals closed in range this year and both landed above their number. The sheet is in Output, and `comps-summary.md` is the one-page read of it. Want me to draft a response, or see the comps first?";
    const reply2 =
      "Done — firm counter drafted, holding at list minus 3%. The letter is `counter-draft-v1.docx`, with the plain-text draft and the brief I gave the writer beside it. Nothing has been sent.";
    t1.runs[0].text = reply1;
    t2.runs[0].text = reply2;
    messagesByThread.acme[1].text = reply1;
    messagesByThread.acme[3].text = reply2;
    const at3 = now() - 1000 * 60 * 14;
    const at4 = now() - 1000 * 60 * 5;
    const ask3 = "pull the signed term sheet and the Q3 numbers before I send it";
    const ask4 = "and a 30-second walkthrough of the comps for Priya";
    const reply3 =
      "Both in Output. The March term sheet says 55/45 and the draft quotes it right. Q3 revenue came in 6% over plan — `q3-revenue-chart.png` is the chart to put in front of Hensley if he pushes.";
    const reply4 =
      "Recorded: 31 seconds, the two in-range deals called out, Acme's counter on screen at the end. It is in Output as `comps-walkthrough.mp4`. Say the word and it goes to Priya.";
    seedTurn("acme", acmeTurn3, ask3, reply3, at3, 168000);
    seedTurn("acme", acmeTurn4, ask4, reply4, at4, 72000);
    messagesByThread.acme.push(
      { role: "user", text: ask3, turn_id: acmeTurn3, at: at3 },
      { role: "assistant", text: reply3, turn_id: acmeTurn3, at: at3 + 2000 },
      { role: "user", text: ask4, turn_id: acmeTurn4, at: at4 },
      { role: "assistant", text: reply4, turn_id: acmeTurn4, at: at4 + 2000 }
    );
    const end1 = t1.endedAt;
    const end2 = t2.endedAt;
    const end3 = at3 + 168000;
    const end4 = at4 + 72000;
    // Newest first, by latest write — the order `OutputStore::project` lists them in — and
    // timed so each group reads in round 17's own order.
    outputByThread.acme = [
      outputEntry("acme", acmeTurn4, "acme/for-priya", "comps-walkthrough.mp4", 14200000, end4, { source: "command" }),
      outputEntry("acme", acmeTurn3, "acme/reference", "term-sheet-march.pdf", 212000, end3 - 1000),
      // A worker's file that was LANDED (S2b, `WriteSource::Land`): one entry at the path that
      // opens, the worker's name on it, its worktree write folded in (`writes: 2`) and the
      // earlier of the two turns as `firstTurnId`. The name is the engine's own, as the record
      // carries it (`<teammate>-<model>-<id>`); the row says "by Clark".
      outputEntry("acme", acmeTurn3, "acme/reference", "q3-revenue.csv", 1100, end3 - 2000, {
        actor: "worker", workerName: "clark-sonnet-1a2b3c4d5e6f", source: "land", writes: 2, firstTurnId: acmeTurn2,
      }),
      outputEntry("acme", acmeTurn3, "acme/reference", "q3-revenue-chart.png", 96000, end3 - 3000, { source: "command" }),
      outputEntry("acme", acmeTurn2, "acme/counter", "counter-draft-v1.docx", 18000, end2 - 1000, { source: "command" }),
      outputEntry("acme", acmeTurn2, "acme/counter", "counter-draft-v1.md", 1000, end2 - 2000),
      outputEntry("acme", acmeTurn2, "acme/counter", "brief.md", 1200, end2 - 3000),
      outputEntry("acme", acmeTurn1, "acme/deals", "comps-2026-10-04.xlsx", 41000, end1 - 1000, { source: "command" }),
      outputEntry("acme", acmeTurn1, "acme/deals", "comps-summary.md", 2100, end1 - 2000),
    ];
  }

  /// A `rich_file` item exactly as `rich_files::item` + `rich_files_host::item` write it.
  function richFileItem(threadId, turnId, id, name, ext, meta, app, at, exists) {
    const t = threads.find((x) => x.id === threadId);
    const file = { id, name, ext, type: meta.split(" · ")[0], meta: exists ? meta : meta.split(" · ")[0],
      bytes: exists ? 1 : null, exists, refused: false };
    if (exists && app) file.app = app;
    return { kind: "rich_file", id, entityId: t ? t.entity_id : null, threadId, turnId, visibility: "ceo",
      createdAt: at, sequence: at, text: name, file };
  }

  // ROUND 36 SCREEN 3, "A file from Rich": his ask, Rich's reply, and the two files under it — the
  // round's own words and files. `round-36-missing` is the same turn with the PDF gone from the Mac.
  if (preset.richFiles === "round-36" || preset.richFiles === "round-36-missing") {
    const turnId = uid("t");
    const at = now() - 1000 * 60 * 3;
    const ask = "send me the final counter as a PDF, and the comps";
    const reply =
      "Here they are. The counter holds at list minus 3%, with onboarding waived. The comps cover the eight deals like this one from the last two years; Acme’s ask is the lowest of them.";
    seedTurn("acme", turnId, ask, reply, at, 161000);
    messagesByThread.acme.push(
      { role: "user", text: ask, turn_id: turnId, at },
      { role: "assistant", text: reply, turn_id: turnId, at: at + 2000 }
    );
    richFilesByThread.acme = [
      richFileItem("acme", turnId, "rf_0a1b2c3d4e5f6071", "Acme-counteroffer.pdf", "PDF", "PDF · 184 KB · 2 pages", "Preview",
        at + 1000, preset.richFiles !== "round-36-missing"),
      richFileItem("acme", turnId, "rf_8192a3b4c5d6e7f8", "Acme-comparables.xlsx", "XLSX", "Spreadsheet · 46 KB", "Numbers",
        at + 1100, true),
    ];
  }

  // ROUND 36 SCREEN 1, "Your instructions": the user's own `instructions.md`, as
  // `your_instructions_read` reads it. By default a returning user's, the round's six rules;
  // `{ yourInstructions: "" }` is a new user with an empty file. `{ instructionsCard: "round-36" }`
  // adds the round's Onboarding conversation, where Rich asked how the user wants him to work and
  // saved four rules (`mcp__richos_onboarding__save_your_instructions`), with its card;
  // `{ onboardingConversation: true }` adds that conversation empty, where "Have Rich ask me" goes;
  // `{ secondOnboarding: "lumen" }` adds another company's empty Onboarding conversation beside it,
  // `onboarding-lumen`, so a check can tell which company's the button went to.
  const ROUND_36_RULES = [
    "- Ask me before spending more than $500 on anything.",
    "- Never email a customer without showing me the draft first.",
    "- Write in plain English. No jargon in anything a customer reads.",
    "- Keep anything non-urgent until after 1 pm. Mornings are for deep work.",
    "- Every report starts with the answer, then the detail.",
    "- When something breaks, tell me what broke, why, and what stops it happening again.",
  ].join("\n") + "\n";
  const INTERVIEW_RULES = [
    "Ask me before spending more than $500 on anything.",
    "Never email a customer without showing me the draft first.",
    "Write in plain English. No jargon in anything a customer reads.",
    "Keep anything non-urgent until after 1 pm. Mornings are for deep work.",
  ];
  const yourInstructions = {
    text: typeof preset.yourInstructions === "string" ? preset.yourInstructions : ROUND_36_RULES,
    exists: true,
    saves: [],
    opened: [],
    failSave: null,
  };
  // thread_id -> the `instructions_saved` items `your_instructions::item` writes.
  const instructionCardsByThread = {};
  function instructionsCardItem(threadId, turnId, rules, at) {
    const t = threads.find((x) => x.id === threadId);
    return { kind: "instructions_saved", id: "yi_" + uid("c").replace(/[^0-9a-z]/gi, "").slice(-16), entityId: t ? t.entity_id : null,
      threadId, turnId, visibility: "ceo", createdAt: at, sequence: at, rules: rules.slice() };
  }
  if (preset.onboardingConversation && preset.instructionsCard !== "round-36") {
    threads.push({ id: "onboarding", title: "Onboarding", entity_id: "northwind", created_at: now() - 1000 * 60 * 60, message_count: 0,
      last_activity: now() - 1000 * 60 * 60, last_turn_state: null, has_pending_turn: false });
    messagesByThread.onboarding = [];
  }
  if (typeof preset.secondOnboarding === "string") {
    const id = "onboarding-" + preset.secondOnboarding;
    threads.push({ id, title: "Onboarding", entity_id: preset.secondOnboarding, created_at: now() - 1000 * 60 * 90, message_count: 0,
      last_activity: now() - 1000 * 60 * 90, last_turn_state: null, has_pending_turn: false });
    messagesByThread[id] = [];
  }
  if (preset.instructionsCard === "round-36") {
    const at = now() - 1000 * 60 * 4;
    threads.push({ id: "onboarding", title: "Onboarding", entity_id: "northwind", created_at: at - 1000 * 60 * 20, message_count: 3,
      last_activity: at + 6000, last_turn_state: "completed", has_pending_turn: false });
    messagesByThread.onboarding = [];
    const ask = uid("t"), answer = uid("t");
    const question = "That covers Northwind: what it is, who it serves, and the team you want around you. I saved all of it.\n\n" +
      "One more, and this one is about you rather than the company: **how do you want me to work?** Anything I should always do or never do, wherever we are. Spending, emails, how you like reports, when not to disturb you.";
    const you = "Ask me before anything over $500. Never email a customer without showing me first. Plain English, no jargon. And nothing non-urgent before 1 pm, mornings are for deep work.";
    const reply = "Got it. I wrote those down as your instructions. I follow them in every company, starting with your next new conversation.";
    // The question turn answered a pressed Start the questions, which has no bubble of his.
    seedTurn("onboarding", ask, null, question, at - 1000 * 60, 9000);
    seedTurn("onboarding", answer, you, reply, at, 6000);
    messagesByThread.onboarding.push(
      { role: "assistant", text: question, turn_id: ask, at: at - 1000 * 60 + 2000 },
      { role: "user", text: you, turn_id: answer, at },
      { role: "assistant", text: reply, turn_id: answer, at: at + 2000 }
    );
    instructionCardsByThread.onboarding = [instructionsCardItem("onboarding", answer, INTERVIEW_RULES, at + 1500)];
    yourInstructions.text = INTERVIEW_RULES.map((r) => "- " + r).join("\n") + "\n";
  }

  /// A snapshot with the conversation's file cards added, as each Rust snapshot path adds them,
  /// and the "Your instructions · Saved" cards `rich_files_host::with_instruction_cards` adds.
  function withRichFiles(payload, threadId) {
    for (const f of richFilesByThread[threadId] || []) payload.items.push(JSON.parse(JSON.stringify(f)));
    for (const c of instructionCardsByThread[threadId] || []) payload.items.push(JSON.parse(JSON.stringify(c)));
    return payload;
  }

  /// `rich_file_open` / `rich_file_reveal`: Rust resolves the id in the conversation's record and
  /// refuses a gone file with the opener's own sentence (`opener.rs` `FILE_GONE`).
  function richFileAction(cmd, args) {
    const threadId = args.threadId ?? args.thread_id;
    const id = args.fileId ?? args.file_id;
    const f = (richFilesByThread[threadId] || []).find((x) => x.file.id === id);
    if (!f) return Promise.reject("I don't have that file in this conversation.");
    if (!f.file.exists) return Promise.reject("No longer on this Mac. It was moved or deleted.");
    richFileActions.push({ cmd, threadId, id, name: f.file.name });
    return Promise.resolve(null);
  }

  /// Round 17's `arrives`: his fifth message, Rich's reply, and the two files that turn wrote,
  /// appended to the front of the record and announced on `rich://output` once the turn is done.
  function outputArrive(threadId) {
    const thread = threadId || "acme";
    simulateTurn(thread, "make me a one-pager for the board", {
      reply: "Here — one page, the counter and the comps on it, as PDF for the pack and as the source beside it. Both in Output.",
      onCompleted(turnId, endedAt) {
        const added = [
          outputEntry(thread, turnId, "acme/board", "board-one-pager.pdf", 88000, endedAt, { source: "command" }),
          outputEntry(thread, turnId, "acme/board", "board-one-pager.md", 2000, endedAt - 500),
        ];
        outputByThread[thread] = added.concat(outputByThread[thread] || []);
        const list = outputListOf(thread);
        emit("rich://output", { threadId: thread, added: added.map((e) => Object.assign({}, e)), count: list.count });
      },
    });
  }

  // ---- THE PREVIEWS (`output_preview`) — output side-panel PRD §7, slice S5 ------------------
  //
  // What the shell answers for each of round 17's files, in the exact tagged shape
  // `output_files::Preview` serializes (`view` + camelCase). The words are the mockup's own
  // (`output.html`'s COMPS_SUMMARY, COUNTER_MD, BRIEF_MD, ONEPAGER_MD, CSV_ROWS, CHART_SVG), so a
  // preview here reads as the round reads. Media cross as `data:` URLs where the app serves
  // `richos-output://<id>`: the chart is the round's SVG, the PDF a one-page PDF of the term
  // sheet (1,092 bytes), the walkthrough a one-second 320 x 180 stand-in clip (1,859 bytes) —
  // the FACTS are the round's (0:31, 1920 x 1080), because in the app they come from the file's
  // own header (`media_facts`), never from what the player loaded. The two documents are drawn as
  // SVG pages where the app shows QuickLook's PNG of the first page.
  //
  // `__RICHOS_MOCK__.outputPreviewAs(name, answer)` makes one file answer something else — too
  // large, no preview, a read that failed — so every §6.7 state is reachable from a suite.
  const OUTPUT_MD = {
    "comps-summary.md": [
      "# Acme — comparables, 2026-10-04",
      "",
      "Two deals in range closed in the last ninety days. Both landed **above Acme's counter**.",
      "",
      "| Deal | Closed | Size | Off list |",
      "|---|---|---:|---:|",
      "| Northwind renewal | Aug 14 | $1.9M | −3% |",
      "| Bellweather new | Sep 2 | $2.4M | −5% |",
      "| Acme (counter) | — | $2.1M | −8% |",
      "",
      "## What it means",
      "",
      "- Their 8% is below anything that closed this year. It is an anchor, not a number.",
      "- Holding at list minus 3% keeps the margin and matches Northwind, which they know about.",
      "- Meeting at minus 5% gives up about $84,000 and closes faster.",
      "",
      "> Source: the deal ledger, `femcboost/deals/2026.xlsx`, rows 14–41.",
    ].join("\n"),
    "counter-draft-v1.md": [
      "# Counter to Acme — draft 1",
      "",
      "**To:** Mark Hensley, Acme  ",
      "**From:** Alex Booster  ",
      "**Re:** Your proposal of October 3",
      "",
      "Mark —",
      "",
      "Thank you for the proposal. We have read it carefully and we want to do this deal.",
      "",
      "We cannot do it at eight percent below list. Two comparable agreements closed this year, and both landed within five percent of list; our pricing has held because the service behind it has.",
      "",
      "Here is where we can go: **three percent below list**, with the onboarding fee waived and the first quarterly review brought forward to week six. That is the firmest offer we will make, and it is a good one.",
      "",
      "If that works, we can have paper to you by Friday and your team live before your board meets on the 14th.",
      "",
      "Alex",
      "",
      "---",
      "",
      "*Draft 1 · held at list minus 3% · 168 words*",
    ].join("\n"),
    "brief.md": [
      "# Brief — the Acme counter",
      "",
      "Written for the writer before the draft. Kept beside it so the reasoning is on the record.",
      "",
      "## The ask",
      "",
      "\"Draft it, keep it firm.\" A counter to Acme's proposal of October 3, which came in at 8% below list.",
      "",
      "## The position",
      "",
      "1. Hold at **list minus 3%** — the number the comparables support.",
      "2. Give something that costs us little: waive onboarding, pull the first review forward.",
      "3. Name the deadline they care about: their board on the 14th.",
      "",
      "## Tone",
      "",
      "- Warm, short, no hedging. One number, said once.",
      "- Never mention the 55/45 split; the term sheet handles it.",
      "- Under 200 words. It is read on a phone.",
    ].join("\n"),
    "board-one-pager.md": [
      "# Acme — the one page for the board",
      "",
      "**Where it stands:** Acme countered at 8% below list on October 3. Our reply holds at 3% below, with onboarding waived.",
      "",
      "## The numbers",
      "",
      "| | List | Their counter | Our reply |",
      "|---|---:|---:|---:|",
      "| Annual contract | $2.10M | $1.93M | $2.04M |",
      "| First-year margin | 41% | 36% | 40% |",
      "| Close by | — | Oct 14 | Oct 14 |",
      "",
      "## Why 3%",
      "",
      "- Northwind closed at −3% in August; Bellweather at −5% in September. Both are above Acme's number.",
      "- Q3 revenue came in 6% over plan. We do not need this at any price.",
      "- Their board meets on the 14th. The deadline is theirs, which is why the offer can be firm.",
    ].join("\n"),
  };
  const OUTPUT_CSV = [
    ["Month", "Plan", "Actual", "Variance", "Note"],
    ["July", "$640,000", "$668,000", "+4.4%", "Northwind renewal landed"],
    ["August", "$650,000", "$702,000", "+8.0%", "Two expansions"],
    ["September", "$660,000", "$697,000", "+5.6%", "Bellweather new"],
    ["Q3 total", "$1,950,000", "$2,067,000", "+6.0%", ""],
  ];
  const OUTPUT_CHART_SVG =
    '<svg viewBox="0 0 640 400" xmlns="http://www.w3.org/2000/svg">' +
    '<rect width="640" height="400" fill="#FDFCF8"/>' +
    '<text x="40" y="44" font-family="Newsreader, serif" font-size="26" fill="#0C1322">Q3 revenue — plan and actual</text>' +
    '<text x="40" y="70" font-family="Inter, sans-serif" font-size="15" fill="#4A4E56">In thousands of dollars. Actual beat plan every month; +6.0% for the quarter.</text>' +
    '<g font-family="Inter, sans-serif" font-size="14" fill="#4A4E56">' +
    '<line x1="80" y1="330" x2="600" y2="330" stroke="#B8B2A5" stroke-width="1"/>' +
    '<line x1="80" y1="250" x2="600" y2="250" stroke="#E2DDD1" stroke-width="1"/><text x="40" y="254">650</text>' +
    '<line x1="80" y1="170" x2="600" y2="170" stroke="#E2DDD1" stroke-width="1"/><text x="40" y="174">700</text>' +
    '<line x1="80" y1="90" x2="600" y2="90" stroke="#E2DDD1" stroke-width="1"/><text x="40" y="94">750</text>' +
    '<text x="40" y="334">600</text>' +
    '<rect x="120" y="266" width="52" height="64" fill="#C8C2B4"/><rect x="178" y="221" width="52" height="109" fill="#9C7C34"/><text x="175" y="358" text-anchor="middle" font-size="15" fill="#0C1322">July</text>' +
    '<rect x="290" y="250" width="52" height="80" fill="#C8C2B4"/><rect x="348" y="167" width="52" height="163" fill="#9C7C34"/><text x="345" y="358" text-anchor="middle" font-size="15" fill="#0C1322">August</text>' +
    '<rect x="460" y="234" width="52" height="96" fill="#C8C2B4"/><rect x="518" y="175" width="52" height="155" fill="#9C7C34"/><text x="515" y="358" text-anchor="middle" font-size="15" fill="#0C1322">September</text>' +
    '<rect x="420" y="30" width="14" height="14" fill="#C8C2B4"/><text x="440" y="42">Plan</text>' +
    '<rect x="500" y="30" width="14" height="14" fill="#9C7C34"/><text x="520" y="42">Actual</text>' +
    "</g></svg>";
  /// A first page as QuickLook draws one: the file's own white, its lines in the file's ink.
  function outputPageSvg(lines) {
    let y = 70;
    let body = "";
    for (const [size, text] of lines) {
      body += '<text x="60" y="' + y + '" font-family="Newsreader, serif" font-size="' + size + '" fill="#0C1322">' + text + "</text>";
      y += size + 18;
    }
    return '<svg viewBox="0 0 612 792" xmlns="http://www.w3.org/2000/svg"><rect width="612" height="792" fill="#FDFCF8"/>' + body + "</svg>";
  }
  const svgUrl = (svg) => "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
  const OUTPUT_TERM_PDF = "data:application/pdf;base64,JVBERi0xLjQKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCA2MTIgNzkyXSAvUmVzb3VyY2VzIDw8IC9Gb250IDw8IC9GMSA1IDAgUiA+PiA+PiAvQ29udGVudHMgNCAwIFIgPj4KZW5kb2JqCjQgMCBvYmoKPDwgL0xlbmd0aCA1NDcgPj4Kc3RyZWFtCkJUCi9GMSAxMSBUZgo1NiA3NjAgVGQKL0YxIDI0IFRmCihUZXJtIHNoZWV0IC0gc2lnbmVkIE1hcmNoIDEyLCAyMDI2KSBUagowIC0yNiBUZAovRjEgMTIgVGYKKFBhcnRpZXM6IEZlbWNCb29zdCBhbmQgRGVlcGx5IFBhcnRuZXJzKSBUagowIC0yOCBUZAovRjEgMTQgVGYKKDMuIFJldmVudWUgc3BsaXQpIFRqCjAgLTE2IFRkCi9GMSAxMSBUZgooTmV0IHJldmVudWUgZnJvbSBqb2ludCBlbmdhZ2VtZW50cyBpcyBzcGxpdCA1NSAvIDQ1IGluIGZhdm9yIG9mIEZlbWNCb29zdCwpIFRqCjAgLTE2IFRkCi9GMSAxMSBUZgooY2FsY3VsYXRlZCBxdWFydGVybHksIHNldHRsZWQgd2l0aGluIHRoaXJ0eSBkYXlzIG9mIHF1YXJ0ZXIgZW5kLikgVGoKMCAtMjggVGQKL0YxIDE0IFRmCig0LiBUZXJtKSBUagowIC0xNiBUZAovRjEgMTEgVGYKKFR3ZW50eS1mb3VyIG1vbnRocyBmcm9tIHNpZ25hdHVyZSwgcmVuZXdpbmcgYW5udWFsbHkgdW5sZXNzIGVpdGhlciBwYXJ0eSkgVGoKMCAtMTYgVGQKL0YxIDExIFRmCihnaXZlcyBuaW5ldHkgZGF5cycgbm90aWNlLikgVGoKRVQKZW5kc3RyZWFtCmVuZG9iago1IDAgb2JqCjw8IC9UeXBlIC9Gb250IC9TdWJ0eXBlIC9UeXBlMSAvQmFzZUZvbnQgL0hlbHZldGljYSA+PgplbmRvYmoKeHJlZgowIDYKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDA5IDAwMDAwIG4gCjAwMDAwMDAwNTggMDAwMDAgbiAKMDAwMDAwMDExNSAwMDAwMCBuIAowMDAwMDAwMjQxIDAwMDAwIG4gCjAwMDAwMDA4MzkgMDAwMDAgbiAKdHJhaWxlcgo8PCAvU2l6ZSA2IC9Sb290IDEgMCBSID4+CnN0YXJ0eHJlZgo5MDkKJSVFT0YK";
  const OUTPUT_CLIP_MP4 = "data:video/mp4;base64,AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1wNDEAAANjbW9vdgAAAGxtdmhkAAAAAAAAAAAAAAAAAAAD6AAAA+gAAQAAAQAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAo50cmFrAAAAXHRraGQAAAADAAAAAAAAAAAAAAABAAAAAAAAA+gAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAAUAAAAC0AAAAAAAkZWR0cwAAABxlbHN0AAAAAAAAAAEAAAPoAAAgAAABAAAAAAIGbWRpYQAAACBtZGhkAAAAAAAAAAAAAAAAAABAAAAAQABVxAAAAAAALWhkbHIAAAAAAAAAAHZpZGUAAAAAAAAAAAAAAABWaWRlb0hhbmRsZXIAAAABsW1pbmYAAAAUdm1oZAAAAAEAAAAAAAAAAAAAACRkaW5mAAAAHGRyZWYAAAAAAAAAAQAAAAx1cmwgAAAAAQAAAXFzdGJsAAAAwXN0c2QAAAAAAAAAAQAAALFhdmMxAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAUAAtABIAAAASAAAAAAAAAABFExhdmM2My4xLjEwMiBsaWJ4MjY0AAAAAAAAAAAAAAAAGP//AAAAN2F2Y0MBZAAM/+EAGmdkAAys2UFBn58BEAAAAwAQAAADAIDxQplgAQAGaOvjyyLA/fj4AAAAABBwYXNwAAAAAQAAAAEAAAAUYnRydAAAAAAAAB2AAAAAAAAAABhzdHRzAAAAAAAAAAEAAAAEAAAQAAAAABRzdHNzAAAAAAAAAAEAAAABAAAAKGN0dHMAAAAAAAAAAwAAAAEAACAAAAAAAQAAQAAAAAACAAAQAAAAABxzdHNjAAAAAAAAAAEAAAABAAAABAAAAAEAAAAkc3RzegAAAAAAAAAAAAAABAAAA4MAAAARAAAADgAAAA4AAAAUc3RjbwAAAAAAAAABAAADkwAAAGF1ZHRhAAAAWW1ldGEAAAAAAAAAIWhkbHIAAAAAAAAAAG1kaXJhcHBsAAAAAAAAAAAAAAAALGlsc3QAAAAkqXRvbwAAABxkYXRhAAAAAQAAAABMYXZmNjMuMS4xMDIAAAAIZnJlZQAAA7htZGF0AAACrQYF//+p3EXpvebZSLeWLNgg2SPu73gyNjQgLSBjb3JlIDE2NSByMzIyMiBiMzU2MDVhIC0gSC4yNjQvTVBFRy00IEFWQyBjb2RlYyAtIENvcHlsZWZ0IDIwMDMtMjAyNSAtIGh0dHA6Ly93d3cudmlkZW9sYW4ub3JnL3gyNjQuaHRtbCAtIG9wdGlvbnM6IGNhYmFjPTEgcmVmPTMgZGVibG9jaz0xOjA6MCBhbmFseXNlPTB4MzoweDExMyBtZT1oZXggc3VibWU9NyBwc3k9MSBwc3lfcmQ9MS4wMDowLjAwIG1peGVkX3JlZj0xIG1lX3JhbmdlPTE2IGNocm9tYV9tZT0xIHRyZWxsaXM9MSA4eDhkY3Q9MSBjcW09MCBkZWFkem9uZT0yMSwxMSBmYXN0X3Bza2lwPTEgY2hyb21hX3FwX29mZnNldD0tMiB0aHJlYWRzPTYgbG9va2FoZWFkX3RocmVhZHM9MSBzbGljZWRfdGhyZWFkcz0wIG5yPTAgZGVjaW1hdGU9MSBpbnRlcmxhY2VkPTAgYmx1cmF5X2NvbXBhdD0wIGNvbnN0cmFpbmVkX2ludHJhPTAgYmZyYW1lcz0zIGJfcHlyYW1pZD0yIGJfYWRhcHQ9MSBiX2JpYXM9MCBkaXJlY3Q9MSB3ZWlnaHRiPTEgb3Blbl9nb3A9MCB3ZWlnaHRwPTIga2V5aW50PTI1MCBrZXlpbnRfbWluPTQgc2NlbmVjdXQ9NDAgaW50cmFfcmVmcmVzaD0wIHJjX2xvb2thaGVhZD00MCByYz1jcmYgbWJ0cmVlPTEgY3JmPTIzLjAgcWNvbXA9MC42MCBxcG1pbj0wIHFwbWF4PTY5IHFwc3RlcD00IGlwX3JhdGlvPTEuNDAgYXE9MToxLjAwAIAAAADOZYiEABL//ujJ/MstpqZ0tNoYhWnHMI9HTDq9Ryj5DaMReURDAAGAiihI8NsiIvVOwOaFf4z4WIADJYmK3QHIjynk1BSx5p8IDNh2i1Dpwm5HqBTHiOyScOtXimk1M1Y53uK0v5sGBhwxivC2jDDKTMEZ9N7tym//gjgAAY8d7825DEt5AJW3/pp/cYI9PpmMRsm8tvPP5D+m+fxM/TE3/VdPAOwSXv9vsPMO0mhnjrTE50JicVS9K7///r7CNJH+/72PPMuUACNphaYzS4EAAAANQZojbEEP/qpVAACFgAAAAApBnkF4gj8AACyhAAAACgGeYmpBDwAAVMA=";
  const outputPreviewOverrides = new Map();

  function outputFixturePreview(e) {
    if (outputPreviewOverrides.has(e.name)) return Object.assign({}, outputPreviewOverrides.get(e.name));
    if (!e.exists) {
      return { view: "none", why: "missing", reason: "This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here." };
    }
    const bytes = e.bytes;
    if (OUTPUT_MD[e.name] != null) return { view: "text", text: OUTPUT_MD[e.name], truncated: false, bytes };
    switch (e.name) {
      case "q3-revenue.csv":
        return { view: "table", rows: OUTPUT_CSV.map((r) => r.slice()), totalRows: OUTPUT_CSV.length, countedAll: true, bytes, app: "Numbers" };
      case "q3-revenue-chart.png":
        return { view: "image", url: svgUrl(OUTPUT_CHART_SVG), width: 1280, height: 800, bytes, app: "Preview" };
      case "term-sheet-march.pdf":
      case "board-one-pager.pdf":
        return { view: "pdf", url: OUTPUT_TERM_PDF, bytes, app: "Preview" };
      case "comps-walkthrough.mp4":
        return { view: "video", url: OUTPUT_CLIP_MP4, bytes, durationMs: 31000, width: 1920, height: 1080, app: "QuickTime Player" };
      case "counter-draft-v1.docx":
        return {
          view: "rendition", bytes, app: "Pages",
          url: svgUrl(outputPageSvg([[26, "Counter to Acme — draft 1"], [15, "To: Mark Hensley, Acme"], [15, "From: Alex Booster"], [15, "Mark —"], [15, "Thank you for the proposal. We want to do this deal."], [15, "Here is where we can go: three percent below list."]])),
        };
      case "comps-2026-10-04.xlsx":
        return {
          view: "rendition", bytes, app: "Numbers",
          url: svgUrl(outputPageSvg([[22, "Deals 2026"], [15, "Northwind renewal   2026-08-14   $1,900,000   −3%"], [15, "Bellweather new   2026-09-02   $2,400,000   −5%"], [15, "Harbor expansion   2026-07-30   $880,000   0%"], [15, "Acme   —   $2,100,000   −8% asked"]])),
        };
      default:
        return { view: "none", why: "noViewer", reason: "I don't have a preview for this kind of file. Open has it." };
    }
  }

  /// `output_preview(output_id)`: the id resolves only in the thread the conversation shows, as
  /// the shell's `locate` does (§5.1, §5.2 step 1).
  function outputPreviewOf(outputId) {
    const thread = activeThreadId;
    const e = (outputByThread[thread] || []).find((f) => f.id === outputId);
    if (!e) return Promise.reject("I don't have that file in this thread's output.");
    return outputFixturePreview(e);
  }

  // ---- THE ACTIONS (slice S6): `output_file`'s app list and problem, `output_open`,
  // `output_reveal`, `output_save_copy` — the shell's contract and its sentences, VERBATIM from
  // `src-tauri/src/output_files.rs`, with nothing opened and nothing written. Every call is
  // kept in `outputActionCalls` for the suite to read (`__RICHOS_MOCK__.outputCalls()`).
  //
  // The apps are round 17's (`output.html` FILES), as Launch Services would give them: the
  // default, then the others without it, ordered by name.
  const OUTPUT_APPS = {
    md: ["Obsidian", "TextEdit", "Visual Studio Code"],
    txt: ["TextEdit", "Visual Studio Code"],
    csv: ["Numbers", "Microsoft Excel", "TextEdit"],
    xlsx: ["Numbers", "Google Sheets", "Microsoft Excel"],
    docx: ["Pages", "Google Docs", "Microsoft Word"],
    pdf: ["Preview", "Adobe Acrobat", "Safari"],
    png: ["Preview", "Photos", "Pixelmator Pro"],
    mp4: ["QuickTime Player", "IINA", "VLC"],
  };
  const OUTPUT_SAID = {
    notInRecord: "I don't have that file in this thread's output.",
    linked: "This file is a link to somewhere else, so I won't open it from here. Show in Finder still works.",
    missing: "This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here.",
    appsChanged: "The apps that open this file changed since the list was shown. Choose one again.",
    appNotOffered: "That app is not one this Mac offers for this file. Choose one again.",
    nothingSaved: "Nothing was saved.",
  };
  const outputActionCalls = [];
  /// output id -> the app list `output_file` last showed (the shell's `shown`).
  const outputShown = new Map();
  /// Names whose recorded path is now a link (`__RICHOS_MOCK__.outputLinked`).
  const outputLinks = new Set();
  /// The next answer of the save sheet: a path, or null for Cancel. Undefined: the Desktop.
  let outputSheetAnswer;
  /// One app list change on this Mac, taken by the next `output_open` that names an app.
  let outputAppsChangeOnce = false;

  function outputFind(id) {
    for (const thread of Object.keys(outputByThread)) {
      const e = (outputByThread[thread] || []).find((x) => x.id === id);
      if (e) return e;
    }
    return null;
  }

  function outputApps(e) {
    const names = OUTPUT_APPS[e.kind] || [];
    const app = (name) => ({ name, bundleId: "com.example." + name.toLowerCase().replace(/[^a-z]+/g, "") });
    return { defaultApp: names.length ? app(names[0]) : null, otherApps: names.slice(1).map(app) };
  }

  /// `null` | `missing` | `refused`, as §5.2 answers it.
  function outputProblem(e) {
    if (!e.exists) return "missing";
    if (outputLinks.has(e.name)) return "refused";
    return null;
  }

  /// `acme/counter/` — the last two folders, as the shell's `folder_label` names them.
  function outputFolderLabel(path) {
    const parts = path.split("/").filter(Boolean);
    parts.pop();
    return parts.slice(-2).join("/") + "/";
  }

  function outputAction(cmd, args) {
    const id = args.outputId ?? args.output_id;
    outputActionCalls.push({ cmd, args: Object.assign({}, args) });
    const e = outputFind(id);
    if (!e) return Promise.reject(OUTPUT_SAID.notInRecord);
    const problem = outputProblem(e);
    const refusal = problem === "missing" ? OUTPUT_SAID.missing : OUTPUT_SAID.linked;
    switch (cmd) {
      case "output_file": {
        const apps = problem ? { defaultApp: null, otherApps: [] } : outputApps(e);
        outputShown.set(id, apps.otherApps.map((a) => a.name));
        return Object.assign({}, e, apps, {
          previewable: "none",
          reason: problem ? refusal : null,
          problem,
        });
      }
      case "output_open": {
        if (problem) return Promise.reject(refusal);
        const apps = outputApps(e);
        const index = args.appIndex ?? args.app_index;
        if (typeof index !== "number") {
          return apps.defaultApp ? "Opening " + e.name + " in " + apps.defaultApp.name + "." : "Opening " + e.name + ".";
        }
        const shown = outputShown.get(id);
        if (!shown) return Promise.reject(OUTPUT_SAID.appsChanged);
        if (index < 0 || index >= shown.length) return Promise.reject(OUTPUT_SAID.appNotOffered);
        if (outputAppsChangeOnce) {
          outputAppsChangeOnce = false;
          const names = OUTPUT_APPS[e.kind];
          if (names && names.length > 2) OUTPUT_APPS[e.kind] = [names[0]].concat(names.slice(1).reverse());
          return Promise.reject(OUTPUT_SAID.appsChanged);
        }
        return "Opening " + e.name + " in " + shown[index] + ".";
      }
      case "output_reveal": {
        if (problem === "missing") return Promise.reject(OUTPUT_SAID.missing);
        return "Finder opens " + outputFolderLabel(e.path) + " with " + e.name + " selected.";
      }
      case "output_save_copy": {
        if (problem) return Promise.reject(refusal);
        const answer = outputSheetAnswer;
        outputSheetAnswer = undefined;
        if (answer === null) return OUTPUT_SAID.nothingSaved;
        const dest = answer || "/Users/you/Desktop/" + e.name;
        const given = dest.split("/").pop();
        return (
          "Saved a copy of " + e.name + " to " + outputFolderLabel(dest) + (given === e.name ? "." : " as " + given + ".")
        );
      }
      default:
        return Promise.reject(OUTPUT_SAID.notInRecord);
    }
  }

  // WHICH COMPANY THIS COPY OF RICH WORKS FOR (`entity_choice` / `choose_entity`).
  //
  // The preview's default is CHOSEN, and deliberately so: every fixture in this harness
  // starts with a rail full of threads that already have a company, which is what a machine
  // that has been answered once looks like. `setCompanyChosen(null)` drives the launch
  // state — the one a double-clicked bundle is always in until the CEO answers — so the
  // picker, the composer block and its control can be exercised without a real Finder
  // launch, and `setCompanyPinnedByEnvironment` drives the operator's variant where the
  // answer came from outside the window and there is nothing here to press.
  let chosenEntityId = "chosenEntity" in preset ? preset.chosenEntity : "harbor";
  let chosenEntitySource = chosenEntityId
    ? preset.pinnedByEnvironment
      ? "environment"
      : "saved-choice"
    : null;
  let entityPinnedByEnvironment = preset.pinnedByEnvironment === true;

  // WHERE THE COMPANY LIST CAME FROM (2026-09-04). `file` by default, for the reason
  // `chosenEntityId` defaults to a company: every other fixture here is a machine that has
  // been answered once.
  //
  // The two other states are new surfaces and each has its own preset, because neither is
  // reachable by any other route:
  //
  //   `registry: "absent"`      a FIRST LAUNCH — the list is empty, nothing is wrong, and
  //                             the picker has nothing to offer but the add form. Before
  //                             today this state could not exist: the registry was compiled
  //                             into the binary and always held six companies.
  //   `registry: "unreadable"`  the file is there and could not be parsed. A DIFFERENT
  //                             sentence from the empty one, because "you haven't told me"
  //                             and "you told me and I can't read it" call for opposite
  //                             responses.
  const registrySource = preset.registry || "file";
  const registryPath = "/Users/you/Library/Application Support/com.richos.app/entities.json";
  if (registrySource !== "file") entities.length = 0;

  // WHERE HIS MEMORY IS. `ready` by default for the reason the company answer is CHOSEN by
  // default: every other fixture in this harness is a machine that has been set up once.
  // `memory: "none"` is the fresh install — no corpus anywhere, which is the state the
  // installed bundle was measurably in the moment its hand-made pointer was removed.
  let memoryState = preset.memory || "ready";
  let memoryRoot = memoryState === "none" ? null : "/Users/you/RichOS/corpus";
  // Whether provisioning here ends `ready` or `no-compiler`. False drives the honest
  // degrade: a corpus is created and the program that reads it is not installed.
  const memoryCompilerPresent = preset.memoryCompiler !== false;

  // ---------------------------------------------------------------------------------------
  // WHERE THIS INSTALL STANDS ON ONBOARDING (`main.rs::onboarding_view`, `onboarding.rs`).
  //
  // **THE DEFAULT IS `described`, AND THAT IS THE CONSERVATIVE CHOICE RATHER THAN THE
  // CONVENIENT ONE.** Every other fixture in this preview is a machine that has been set up
  // once, and a default of `not-yet` would paint the first-run notice on top of every other
  // suite's screenshots — which is how a surface gets shown to be working by a fixture that
  // was already showing it. A suite that wants the offer has to ask for it.
  //
  //   window.__RICHOS_MOCK_PRESET__ = { onboarding: "not-yet" }   // nothing on file, never asked
  //   window.__RICHOS_MOCK_PRESET__ = { onboarding: "declined" }  // asked, and he said not now
  //   window.__RICHOS_MOCK_PRESET__ = { onboarding: "unusable" }  // notes exist and cannot be read
  //   __RICHOS_MOCK__.onboardingSet("not-yet")                    // the same, after load
  //
  // `onboardingDeclineFails: true` drives the refusal arm, because a press whose write
  // silently went nowhere is the one outcome the surface must never render as success. It is
  // a BOOLEAN and the sentence is fixed here, verbatim from
  // `main.rs::ONBOARDING_DECLINE_REFUSED` — a harness free to invent its own refusal text
  // would be rehearsing the surface against a sentence the product does not raise.
  let onboardingState = preset.onboarding || "described";
  const onboardingDeclineFails = preset.onboardingDeclineFails === true;
  // `onboardingDeclineHold: true` keeps "Not now"'s answer back until
  // `__RICHOS_MOCK__.releaseDecline()`, so a suite can have it finish while something else is
  // on screen, as the real write does when the spine is slow (nightly 49's walk, D28: it
  // finished behind an open Bust a bug panel).
  let onboardingDeclineHeld = preset.onboardingDeclineHold === true ? [] : null;
  const ONBOARDING_DECLINE_REFUSED =
    "I couldn't write that down, so it isn't recorded and I'll ask again next time. I'd " +
    "rather say so than let you think it was settled. Whoever set RichOS up will need to " +
    "look at that.";
  const onboardingCalls = [];
  const onboardingByEntity = new Map();

  function onboardingViewOf() {
    // NO BINDING, NO ANSWER — and this arm is not a nicety. `onboarding_view_of` returns
    // `no-central-folder` with a null entity when `active_binding()` is `None`, because
    // onboarding is a question about ONE company and there is no company to ask it about
    // yet. Without this the preview served `not-yet` to a launch that had not chosen a
    // company, and the notice painted over the company picker — a state the product cannot
    // produce, rehearsed by the harness, which is the defect this file keeps naming in its
    // own comments. Found by driving the real first-run order, 2026-09-06.
    const entityId = activeContextOf() && activeContextOf().entity_id;
    if (!entityId) return { state: "no-central-folder", entityId: null, headline: null, message: null };
    const state = onboardingByEntity.get(entityId) || onboardingState;
    return {
      state,
      entityId,
      // Verbatim from `main.rs::ONBOARDING_UNUSABLE_HEADLINE` / `_MESSAGE` — copied rather
      // than paraphrased, for the reason `UNBOUND_ERR` above is copied: a harness that writes
      // its own version of a backend sentence drifts from it the moment either side is edited.
      headline:
        state === "unusable" ? "I couldn't read your notes about this company." : null,
      message:
        state === "unusable"
          ? "I'm working without them and I won't guess at what they said. Whoever set " +
            "RichOS up will need to look at that."
          : null,
    };
  }

  function memoryStatusOf() {
    return {
      state: memoryState,
      root: memoryRoot,
      source: memoryState === "ready" ? "~/RichOS/corpus" : null,
      compiler: memoryState === "ready" ? "/Users/you/Library/Application Support/RichOS/loro-tools" : null,
      tried: [],
      detail: null,
      // The location the CEO is OFFERED, pre-filled — the whole reason his part is a click
      // and not a path. It comes from the backend in the real app and from here in the
      // preview, and it is the same string in both.
      offered_location: "/Users/you/RichOS/corpus",
      provisioned_now: false,
    };
  }

  // ---------------------------------------------------------------------------------------
  // THE HOME SCREEN'S PICTURE OUT OF HIS OWN LORO (`main.rs::home_field_data`).
  //
  // **THE DEFAULT IS "no corpus", AND IT IS NOT A SHORTCUT.** Every other surface in this
  // preview assumes a machine that has been set up once, and this one deliberately does not:
  // the home screen the CEO asked to keep is the DEMONSTRATION — *"The demo is definitely
  // needed, initially, for the user"* — so the preview, the file:// open and every acceptance
  // run must land on exactly that, byte for byte, without opting out of anything.
  //
  // `__RICHOS_MOCK__.homeField` drives the other side. It takes a partial dataset and fills in
  // whatever the field needs; `objects` is the only number the preference reads.
  //
  //   window.__RICHOS_MOCK__.homeField = { objects: 4000, records: 9000 };
  //
  // This mock does NOT compile anything and does not pretend to. What it rehearses is the
  // SURFACE's half of the seam — the shape of the answer, the counts, and the one comparison
  // `home.js` makes against them.
  //
  // `preset.homeField` is the same fact supplied BEFORE any of the page's own scripts run,
  // which is the only way to have it in place before the field's own idle callback asks for
  // it — see the pre-boot switch above. Absent, this stays `null` and the demonstration is
  // what the preview lands on, byte for byte, without anything opting out.
  let homeField = preset.homeField ? { ...preset.homeField } : null;

  // THE TEAMMATES HE ADDED (`main.rs::home_team`, product hunt 2026-10-15, finding 7). None by
  // default, so the home screen lands on the first-run demonstration exactly as before;
  // `preset.homeTeam` (`[{ name, role }]`) is a user who has hired, in place before the field
  // asks.
  let homeTeam = Array.isArray(preset.homeTeam) ? preset.homeTeam.map((t) => ({ ...t })) : [];

  function homeFieldAnswer() {
    if (!homeField) {
      return {
        available: false,
        // The real command's own sentence for the ordinary state of a fresh install.
        reason: "no corpus is configured on this machine",
      };
    }
    const objects = Number(homeField.objects || 0);
    const records = Number(homeField.records || objects);
    // A dataset the field can actually draw: `objects` nodes over `Math.max(1, objects/8)`
    // groups, one lane, laid out exactly as `home_field.rs` lays one out. It is generated
    // rather than fixtured because a 2,000-node fixture in a shipped file would be a second
    // baked dataset, and this repository already has one too many opinions about that.
    const groups = Math.max(1, Math.round(objects / 8));
    const nodes = [];
    const links = [];
    const sources = [];
    for (let g = 0; g < groups; g++) {
      const gid = "grp-" + String(g + 1).padStart(5, "0");
      const first = nodes.length;
      const n = g === groups - 1 ? objects - nodes.length : Math.min(8, objects - nodes.length);
      for (let i = 0; i < n; i++) {
        nodes.push({
          id: "obj-" + String(nodes.length + 1).padStart(5, "0"),
          type: i === 0 ? "decision" : "memory",
          loroKind: i === 0 ? "decision" : "passage",
          domain: "northwind",
          cluster: gid,
          degree: i === 0 ? n - 1 : 1,
          significance: 0.8,
          activity: 0,
          recency: 0,
          createdDay: 0,
        });
      }
      if (n <= 0) continue;
      for (let i = 1; i < n; i++) {
        links.push({ s: nodes[first].id, t: nodes[first + i].id, kind: "same_source", class: "membership", weight: 1, cross: false });
      }
      sources.push({
        id: "src-" + String(g + 1).padStart(5, "0"),
        kind: "document",
        domain: "northwind",
        day: 40000,
        ingestedDay: 40000,
        primaryNodeId: nodes[first].id,
        derivedCount: n,
      });
    }
    return {
      available: true,
      field: {
        meta: {
          name: "RichOS company memory",
          version: 1,
          // THE ONE FIELD THE BANNER READS.
          synthetic: false,
          corpusRoot: memoryRoot || "/Users/you/RichOS/corpus",
          corpusLayout: "corpus",
          fingerprint: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
          horizonDays: 40000,
          counts: { records: records, objects: nodes.length, links: links.length, sources: sources.length, domains: 1 },
          query: { topics: ["northwind"], company: "all" },
          absent: [],
        },
        bragSignals: {
          specialistsManaged: 0,
          specialistsActiveNow: 0,
          tasksHandledWithoutCeo: 0,
          ceoMinutesSaved: 0,
          monthsWorkingTogether: 0,
          sourcesUnderstood: sources.length,
          memoryObjects: records,
          decisionsRemembered: groups,
          lessonsAccumulated: 0,
          commitmentsTracked: 0,
        },
        specialists: [],
        domains: [{ id: "northwind", label: "Northwind Traders", nodeCount: nodes.length, clusters: sources.map((s, i) => ({ id: "grp-" + String(i + 1).padStart(5, "0"), label: "Group " + (i + 1), nodeCount: s.derivedCount })) }],
        nodes: nodes,
        links: links,
        tasks: [],
        sources: sources,
      },
    };
  }

  // ---------------------------------------------------------------------------------------
  // FIRST-RUN SETUP (`setup.rs`, `setup_view.rs`) — Option D's surface
  //
  // `preset.setup` drives it:
  //   undefined / "ready"  a machine that has everything (every other fixture's assumption)
  //   "missing-both"       the customer's Mac — no Claude Code, no engine (§19)
  //   "missing-engine"     Claude Code is there and the engine is not
  //   "missing-claude"     the engine is there and Claude Code is not — a Mac that was set
  //                        up once and then had the binary removed or replaced
  //   "unpinned"           the engine is missing AND this build cannot install one, which is
  //                        the state that must EXPLAIN rather than offer a button that fails
  //
  // `preset.setupFails` makes `run_setup` reject with one of the real `SetupError` sentences,
  // because the failure states are the ones a surface gets wrong.
  // ---------------------------------------------------------------------------------------
  let providerState = preset.providerAuth || "connected";
  const providerView = () => ({state: providerState, message: {
    connected: "Your Anthropic account is connected.",
    "signed-out": "Connect your Anthropic account to start working with Rich.",
    connecting: "Complete sign-in in your browser, then return here.",
    cancelled: "Sign-in was canceled. You can try again when you are ready."
  }[providerState] || "Account connection could not be verified. Try again."});
  const setupPreset = preset.setup || "ready";
  let setupClaudePresent = setupPreset === "ready" || setupPreset === "missing-engine";
  let setupEnginePresent = setupPreset === "ready" || setupPreset === "missing-claude";
  const setupPinned = setupPreset !== "unpinned";
  const setupFails = preset.setupFails || null;
  // ROUND 19 (dictation plan slice 4). `preset.dictation: true` is a build where dictation is
  // there (`ask.dictation`, `richos_core::dictation_ready`): the sheet is round 19's and Rich
  // offers dictation after Start. `preset.videoTools` is the voice and video tools' state:
  // undefined / "present" (every other fixture's), "downloading" (the launch's background
  // download is running, as on a customer's first launch) or "missing" (not running).
  const setupDictationOn = !!preset.dictation;
  let setupVideoTools = preset.videoTools || "present";
  // Rich's one offer: whether it was made already (a relaunch), and every call the window made.
  let dictationOffered = !!preset.dictationOffered;
  const dictationOfferCalls = [];
  // THE DICTATION SHEET AND ROW (dictation plan slice 2): `dictation_status`, as
  // `dictation_app.rs` `view_of` reports it. `preset.dictationView` overrides any fact; what macOS
  // and the tool answer later (a prompt allowed, System Settings, a captured key, another app
  // hiding keys) is driven through `__RICHOS_MOCK__.dictationSet` and `dictationKey`.
  const dictationView = Object.assign({ ready: setupDictationOn, on: false, key: preset.dictationKey || 1, accuracy: "accurate",
    mic: "unknown", ax: "unknown", owner: "none", keyTap: false, secure: null }, preset.dictationView || {});
  let dictationAxAsked = !!preset.dictationAxAsked;
  const dictationCalls = [];
  // The "RichScribe feature" conversation (`dictation_offer.rs` FEATURE_TITLE and feature_text,
  // the CEO's words, 2026-10-13): created once, its id kept as `featureThread` is.
  let dictationFeatureThread = null;
  const dictationFeatureText = (key) =>
    "Good news! RichScribe is set up and enabled.\n\n" +
    "So, in any other app on this Mac (Mail, Slack Chrome/Safari etc.) you can now do this:\n\n" +
    "Click into where you want your words to appear, tap F" + key + ", say what you want written, then tap it again. " +
    "See how your words appear where your cursor is.\n\n" +
    "If you have any questions about this, we can discuss it here.";
  // "LET CODEX REVIEW YOUR TEAM'S WORK" (round 20.2): `codex_reviews_status`, as richos-core
  // codex_reviews.rs reports it. Off on first run, Codex installed and signed in, as the round
  // opens; `preset.codexReviews` overrides either fact, `__RICHOS_MOCK__.codexSet` changes what
  // the Mac reports about Codex later.
  const codexReviews = Object.assign({ on: false, codex: "ready" }, preset.codexReviews || {});
  const codexReviewCalls = [];
  // API-EQUIVALENT TOKEN SPEND (round 22.1, `spend.js`): `spend_view` and `spend_live`, in the shape
  // richos-core's spend ledger answers them (`spend::ledger::Totals`). A seeded record of 47 days
  // ANCHORED TO THE FIRST OF THIS MONTH (30 days before it, 17 from it), so this month's figure is
  // the same on every day of every month: the Settings menu shows it, and the committed menu
  // pictures must not move with the calendar. Two earlier Opus versions, so a mark names three.
  // `preset.spend.ready: false` is the first count still running; `preset.spend.agents` the agents
  // working. `__RICHOS_MOCK__.spendLand` puts a request on record now (the dot on Refresh, the live
  // line), `spendReady` ends the first count, `spendAgents` sets the count working.
  // `preset.spend.rows` replaces the seeded record with exactly those rows (the shape below), so a
  // suite can put one small month on record (the Settings row's cents, `spend.js` check 1b).
  const spendMock = (() => {
    const p = preset.spend || {};
    const DAY = 86400000, now = new Date();
    const first = new Date(now.getFullYear(), now.getMonth(), 1, 12).getTime(); // noon on the 1st
    const rows = [];
    let seed = 118;
    const rand = () => { seed = (seed * 1103515245 + 12345) % 2147483648; return seed / 2147483648; };
    // per model: requests on a full day, dollars a request, tokens a request (round 22's sizes)
    const SHAPE = { fable: [620, 0.29, 290000], opus: [9200, 0.105, 237000], sonnet: [1500, 0.04, 126000], haiku: [2600, 0.0012, 45000] };
    const NAME = { fable: "Fable 5.1", opus: "Opus 5.5", sonnet: "Sonnet 5.5", haiku: "Haiku 5.5" };
    const EFFORT = { fable: "high", opus: "high", sonnet: "medium", haiku: "low" };
    if (Array.isArray(p.rows)) p.rows.forEach(r => rows.push(Object.assign({ cacheRead: 0 }, r)));
    else for (let d = -30; d <= 16; d++) {
      const at = first + d * DAY;
      for (const f of ["fable", "opus", "sonnet", "haiku"]) {
        if (f === "haiku" && d < 8) continue;
        const n = Math.round(SHAPE[f][0] * (0.6 + rand() * 0.5));
        const tokens = n * SHAPE[f][2];
        rows.push({ at, family: f, name: NAME[f], effort: EFFORT[f], requests: n, cost: n * SHAPE[f][1] * (0.85 + rand() * 0.3), tokens, cacheRead: Math.round(tokens * 0.965) });
      }
    }
    // Earlier versions on record, as on the CEO's own Mac (2026-10-11: Opus 5, 5.5 and 4.8 in one
    // record), so a mark names three and the widest legend line is drawn.
    if (!Array.isArray(p.rows)) {
      rows.push({ at: first + 2 * DAY, family: "opus", name: "Opus 5", effort: "high", requests: 4000, cost: 21261.91, tokens: 920000000, cacheRead: 888000000 });
      rows.push({ at: first + 4 * DAY, family: "opus", name: "Opus 4.8", effort: "medium", requests: 200, cost: 338.57, tokens: 46000000, cacheRead: 44000000 });
    }
    const state = { ready: p.ready !== false, agents: p.agents === undefined ? 4 : p.agents, last: null, calls: [] };
    const counted = () => rows.reduce((a, r) => a + r.requests, 0);
    function view(from) {
      const t = { cost: 0, requests: 0, tokens: 0, cacheRead: 0, unpricedRequests: 0, unpricedTokens: 0, firstAt: null, families: [], counted: counted() };
      const fam = {};
      for (const r of rows) {
        if (r.at < from) continue;
        t.cost += r.cost; t.requests += r.requests; t.tokens += r.tokens; t.cacheRead += r.cacheRead;
        t.firstAt = t.firstAt === null ? r.at : Math.min(t.firstAt, r.at);
        const f = fam[r.family] || (fam[r.family] = { family: r.family, cost: 0, requests: 0, names: {}, efforts: {}, effortUnrecorded: false });
        f.cost += r.cost; f.requests += r.requests;
        f.names[r.name] = (f.names[r.name] || 0) + r.cost;
        if (r.effort) f.efforts[r.effort] = (f.efforts[r.effort] || 0) + r.cost;
        else f.effortUnrecorded = true;   // a request with no effort, as an app lease records it
      }
      t.families = ["fable", "opus", "sonnet", "haiku"].filter(f => fam[f]).map(f => fam[f]);
      return { ready: state.ready, asOf: Date.now(), priceDate: "October 11, 2026", totals: t };
    }
    return {
      state,
      view: from => (state.calls.push({ cmd: "spend_view", from }), view(from || 0)),
      live: () => (state.calls.push({ cmd: "spend_live" }), { ready: state.ready, counted: counted(), agentsWorking: state.agents, last: state.last }),
      land(r) {
        rows.push({ at: Date.now(), family: r.family, name: r.name, effort: r.effort, requests: 1, cost: r.cost, tokens: r.tokens, cacheRead: 0 });
        state.last = { name: r.name, tokens: r.tokens, cost: r.cost };
      },
      rows: () => rows.map(r => ({ at: r.at, cost: r.cost })),
    };
  })();

  // The strings are the BACKEND'S, copied verbatim from `Component::why` and
  // `SETUP_ACCOUNT_NOTE`. A mock that paraphrased them would rehearse the surface against
  // copy the product does not have.
  const SETUP_WHY = {
    "claude-code":
      "the program I think with. It comes from Anthropic and installs itself; I only ask it to.",
    engine: "the part of me that knows how I work: my instructions and my team.",
  };
  const SETUP_NAME = { "claude-code": "Claude Code", engine: "the RichOS engine" };
  // The video tools' copy, verbatim from `setup.rs` (`MEDIA_TOOLS_NAME`, `MEDIA_TOOLS_WHY` once
  // dictation is there; the media-tools plan's line before), and his download line from
  // `setup_view.rs` (`SETUP_DOWNLOAD_LINE`). `setup.js` case 23 holds the mock to the Rust.
  const SETUP_TOOLS_NAME = { on: "my voice and video tools", off: "my video tools" };
  const SETUP_TOOLS_WHY = {
    on: "the tools I use to watch and download videos for you. Plus, it gives you a free & private/local replacement for Wispr Flow. So, it saves you $140+/year\u{1F44D} and allows you to talk instead of typing anywhere on this computer.",
    off: "the tools I use to watch, hear and download videos for you, which I keep up to date myself.",
  };
  const SETUP_DOWNLOAD_LINE =
    "Sit tight, we need to download about 1 GB of local voice AI so that you can just talk to Rich instead of typing. This will save you $140+/year\u{1F44D} because you won't need Wispr Flow with this setup.";
  const SETUP_ACCOUNT_NOTE =
    "You need your own Anthropic account and a Max subscription there. You can sign in through your browser after setup; I never see your password. If you don't already have that subscription, sign up there first and pick the Max/Max 20x tier.";
  // WHAT A SEND IS REFUSED WITH WHEN THE SETTING UP WAS NEVER DONE. Verbatim from
  // `setup_view::SETUP_INCOMPLETE_*` (app/src-tauri/src/setup_view.rs). The preview must
  // rehearse the sentence the product ships, not a paraphrase of it — the same rule
  // `_notConnected` follows for `LEASE_UNAVAILABLE_MESSAGE`.
  const SETUP_INCOMPLETE = {
    engine:
      "I can't take that on yet: the RichOS engine isn't on this Mac and that's the part " +
      "of me that knows how I work. I've put the setting up back on your screen: press Set " +
      "it up and I'll fetch it. There's nothing to quit and nothing to reopen.",
    claude:
      "I can't take that on yet: Claude Code isn't on this Mac and that's the program I " +
      "think with. I've put the setting up back on your screen: press Set it up and I'll " +
      "fetch it. There's nothing to quit and nothing to reopen.",
    both:
      "I can't take that on yet: this Mac doesn't have Claude Code or the RichOS engine " +
      "and those are what I think with. I've put the setting up back on your screen: press " +
      "Set it up and I'll fetch them. There's nothing to quit and nothing to reopen.",
  };

  const SETUP_UNPINNED =
    "This copy of RichOS wasn't built with an engine to install, so I can't fetch one. " +
    "It needs whoever set RichOS up to publish one and pin it.";

  function setupNeeds() {
    const out = [];
    if (!setupClaudePresent) out.push("claude-code");
    if (!setupEnginePresent) out.push("engine");
    // `setup_view::sheet_needs`: the video tools only when missing and not downloading.
    if (setupVideoTools === "missing") out.push("media-tools");
    return out;
  }

  /// `setup_view::ask_for_gated`'s list: the sheet's needs, plus the video tools while they
  /// download once dictation is there, on a sheet that asks for something else.
  function setupListed(needs) {
    if (setupDictationOn && needs.length && setupVideoTools === "downloading") return [...needs, "media-tools"];
    return needs;
  }

  function setupItem(c) {
    if (c === "media-tools") {
      const k = setupDictationOn ? "on" : "off";
      return { component: c, name: SETUP_TOOLS_NAME[k], why: SETUP_TOOLS_WHY[k] };
    }
    return { component: c, name: SETUP_NAME[c], why: SETUP_WHY[c] };
  }

  function setupStatusOf() {
    const needs = setupNeeds();
    const blocked = !setupEnginePresent && !setupPinned;
    return {
      status: {
        claude: {
          component: "claude-code",
          present: setupClaudePresent,
          at: setupClaudePresent ? "/Users/you/.local/bin/claude" : null,
          detail: setupClaudePresent ? "installed at 2.1.257" : null,
          looked_in: setupClaudePresent ? [] : ["/Users/you/.local/bin/claude", "/usr/bin/claude"],
        },
        engine: {
          component: "engine",
          present: setupEnginePresent,
          at: setupEnginePresent ? "/Users/you/.claude/richos-engine" : null,
          detail: setupEnginePresent ? "version 1.0.0" : null,
          looked_in: setupEnginePresent
            ? []
            : [
                "/Applications/RichOS.app/Contents/Resources/engine",
                "/Users/you/.claude/richos-engine",
                "/Users/you/Library/Application Support/RichOS/engine",
              ],
        },
        media_tools: {
          component: "media-tools",
          present: setupVideoTools === "present",
          at: setupVideoTools === "present" ? "/Users/you/Library/Application Support/RichOS/tools" : null,
          detail: null,
          looked_in: [],
        },
        engine_installable: setupPinned,
        engine_pin_version: setupPinned ? "1.0.0" : null,
        installed_now: false,
      },
      ask: {
        items: setupListed(needs).map(setupItem),
        account_note: SETUP_ACCOUNT_NOTE,
        can_install: needs.length > 0 && !blocked,
        cannot_install_reason: blocked ? SETUP_UNPINNED : null,
        dictation: setupDictationOn,
        download_line: setupDictationOn ? SETUP_DOWNLOAD_LINE : null,
      },
      complete: needs.length === 0,
      downloading: setupVideoTools === "downloading",
    };
  }

  const setupSleep = (ms) => new Promise((r) => setTimeout(r, ms));

  /// A suite can freeze the run after each step's "started" event (`setupHold(true)`) and let
  /// it go on with `setupRelease()`, so a row is read while the mock is provably at that step
  /// and not while a timer happens to be between two events. Off by default: nothing changes
  /// for a suite that never asks.
  let setupHeld = false;
  let setupReleaseGate = null;
  function setupWaitIfHeld() {
    if (!setupHeld) return Promise.resolve();
    return new Promise((r) => { setupReleaseGate = r; });
  }

  async function runSetupMock() {
    // `setup_view::run`: missing video tools are never a step a press waits for; the press
    // starts their background download instead (`foreground_steps`).
    if (setupVideoTools === "missing") setupVideoTools = "downloading";
    const needs = setupNeeds();
    const total = needs.length;
    // The same channel and the same shape the real command emits on, so the surface under
    // test is driven by events and not by a return value it could have rendered without one.
    //
    // SPACED IN TIME, deliberately. The real run takes minutes — Anthropic's installer alone
    // downloads 197,220,928 B on a Mac with no zstd — and a mock that emitted every step in
    // one synchronous burst would let a surface that renders only the LAST event pass, which
    // is exactly the "looks hung for minutes" defect the events exist to prevent.
    for (let i = 0; i < needs.length; i++) {
      const c = needs[i];
      emit("richos://setup", {
        state: "started",
        component: c,
        what:
          c === "claude-code"
            ? "Getting Claude Code from Anthropic. This is the big one: a few minutes."
            : "Getting my instructions.",
        index: i + 1,
        total,
        detail: null,
        kind: null,
        machine_unchanged: null,
      });
      await setupSleep(30);
      await setupWaitIfHeld();
    }
    if (setupFails) {
      emit("richos://setup", {
        state: "failed",
        component: needs[0],
        what: SETUP_NAME[needs[0]] + " could not be installed.",
        index: 1,
        total,
        detail: setupFails,
        kind: "no-network",
        machine_unchanged: true,
      });
      return Promise.reject(setupFails);
    }
    setupClaudePresent = true;
    setupEnginePresent = true;
    emit("richos://setup", {
      state: "finished",
      component: null,
      what: "The software is installed. Connect your account to start working.",
      index: total,
      total,
      detail: null,
      kind: null,
      machine_unchanged: true,
    });
    const after = setupStatusOf();
    after.status.installed_now = true;
    return after;
  }

  function entityChoiceOf() {
    return {
      chosen: chosenEntityId,
      source: chosenEntityId ? chosenEntitySource : null,
      pinnedByEnvironment: entityPinnedByEnvironment,
      options: entities.map((e) => ({
        id: e.id,
        display_name: e.display_name,
        roots: e.roots || [],
        thread_count: threads.filter((t) => t.entity_id === e.id).length,
      })),
      // The two fields `EntityChoiceView` gained on 2026-09-04. The window renders three
      // different openings from them, so a harness that omitted them would exercise one.
      registrySource,
      registryPath,
      active: activeContextOf(),
    };
  }

  /// `entity_id_from_name` in `src-tauri/src/main.rs`, mirrored — the id is DERIVED from the
  /// name he typed and never asked of him. Same character class, same collapse, same bound,
  /// same suffix on collision, because a harness that derived a different id would let a
  /// suite pass over a mapping the app does not make.
  function mockEntityIdFromName(name) {
    let slug = "";
    for (const c of String(name).trim().toLowerCase()) {
      if (/[a-z0-9]/.test(c)) slug += c;
      else if (!slug.endsWith("-")) slug += "-";
    }
    const base = slug.replace(/^-+|-+$/g, "").slice(0, 64).replace(/-+$/, "");
    if (!base) return null;
    for (let n = 1; n <= 99; n++) {
      const candidate = n === 1 ? base : base.slice(0, 64 - String(n).length - 1).replace(/-+$/, "") + "-" + n;
      if (!entities.some((e) => e.id === candidate)) return candidate;
    }
    return null;
  }

  // Mirrors apply_company_choice: choosing the first company also supplies its first
  // conversation. A chosen company without an active binding is not an onboarded shell.
  function activateCompanyIfNeeded(entityId) {
    if (activeContextOf()) return;
    const existing = threads.filter(t => t.entity_id === entityId).sort((a, b) => b.created_at - a.created_at)[0];
    if (existing) { activeThreadId = existing.id; return; }
    const id = uid("thread");
    threads.push({ id, title: "Onboarding", entity_id: entityId, created_at: now(), message_count: 0,
      last_activity: now(), last_turn_state: null, has_pending_turn: false });
    messagesByThread[id] = [];
    activeThreadId = id;
  }

  function activeContextOf() {
    const t = threads.find((x) => x.id === activeThreadId);
    if (!t || !t.entity_id) return null;
    return { thread_id: t.id, entity_id: t.entity_id, binding_revision: 1 };
  }

  const listeners = {}; // eventName -> Set<fn>
  function emit(eventName, payload) {
    const set = listeners[eventName];
    if (!set) return;
    for (const fn of set) fn({ event: eventName, payload });
  }

  // Canned reply text the mock "streams" back, chunked by word so seq-ordering is
  // exercised exactly like the real spine (see app/STREAMING.md ordering guarantees).
  const CANNED_REPLIES = [
    "Got it — I'll take it from here. Give me a moment to pull what I need.",
    "Understood. I've made a note and I'll follow up once I have something concrete.",
    "On it. I'll check with the relevant thread and come back to you shortly.",
  ];

  /// The fence every §13 payload carries. `bindingRevision` is the ACTIVATION revision —
  /// it advances on thread activation and is legitimately HIGHER than what a re-projection
  /// of the same thread reports, which is exactly why a renderer must treat it as a
  /// staleness floor and never as an equality key.
  function fenceOf(threadId, turnId) {
    const t = threads.find((x) => x.id === threadId);
    return { entityId: t ? t.entity_id : null, threadId, turnId, bindingRevision: t && t.entity_id ? 1 : 0 };
  }

  /// Both families for one turn: the four ORIGINAL events verbatim (byte-for-byte
  /// unchanged — a consumer of only those keeps working), and the six additive §13 events.
  ///
  /// `opts.crashAt` drives the mid-turn-crash replay: after that many deltas the turn emits
  /// `recovering`, and a REPLACEMENT turn's `queued` carries `supersedesTurnId`. The
  /// replacement re-streams from the beginning, exactly as a replay does — which is why a
  /// renderer that ignores the merge instruction draws the CEO's one prompt twice.
  function simulateTurn(threadId, userText, opts) {
    opts = opts || {};
    const turnId = uid("turn");
    const userAt = now();
    messagesByThread[threadId] = messagesByThread[threadId] || [];
    if (!opts.pressed) messagesByThread[threadId].push({ role: "user", text: userText, turn_id: turnId, at: userAt });
    turnsById.set(turnId, {
      threadId,
      entityId: (threads.find((x) => x.id === threadId) || {}).entity_id,
      userText: opts.pressed ? null : userText,
      runs: [],
      state: "queued",
      createdAt: userAt,
      startedAt: null,
      endedAt: null,
      activities: [],
    });

    const fence = fenceOf(threadId, turnId);
    if (opts.supersedes && turnsById.has(opts.supersedes)) {
      // `Turn::superseded_by` — set on the CRASHED turn by the ledger before the replay is
      // journaled. It is what makes the crashed turn unrenderable, forever.
      turnsById.get(opts.supersedes).supersededBy = turnId;
    }
    emit("rich://turn-status", Object.assign({}, fence, {
      status: "queued", startedAt: null, activeDurationMs: null, visibility: "ceo", at: now(),
      supersedesTurnId: opts.supersedes,
    }));
    // **AND WHAT HE ACTUALLY SAID**, beside the queued status and in that order, because that
    // is where `spine.rs` emits it (`accept_prompt`, and `drain_intake`'s channel and steering
    // arms). THIS MOCK WAS MISSING IT ENTIRELY, which is worth stating: the event has been on
    // the real wire since `2c95c27d` and nothing here rehearsed it, so a window that ignored it
    // looked identical under the mock and took 5.68 s on his phone. `messageId` is the
    // PROJECTION'S `{turn}:user` and `createdAt` is the turn's own instant — an id minted here
    // would make his one sentence two rows.
    if (!opts.pressed) emit("rich://ceo-message", Object.assign({}, fence, {
      messageId: turnId + ":user", text: userText, source: opts.source || "text",
      createdAt: userAt, visibility: "ceo", at: now(),
    }));

    const startedAt = now();
    turnsById.get(turnId).startedAt = startedAt;
    turnsById.get(turnId).state = "working";
    emit("rich://turn-started", { threadId, turnId, at: startedAt });
    emit("rich://turn-status", Object.assign({}, fence, {
      status: "working", startedAt, activeDurationMs: null, visibility: "ceo", at: startedAt,
    }));

    const reply = opts.reply || CANNED_REPLIES[Math.floor(Math.random() * CANNED_REPLIES.length)];
    const words = reply.split(" ");
    const messageId = turnId + ":text:0";
    let seq = 0;
    let acc = "";
    let i = 0;
    let opened = false;
    let activityFired = false;

    function next() {
      if (opts.crashAt && i === opts.crashAt) {
        // A POSITIVE termination signal, mid-turn. The crashed turn emits `recovering` and
        // is about to be superseded — never `failed`, and a reload will not render it at all.
        turnsById.get(turnId).state = "interrupted";
        emit("rich://turn-status", Object.assign({}, fence, {
          status: "recovering", startedAt, activeDurationMs: null, visibility: "ceo", at: now(),
        }));
        setTimeout(function () {
          simulateTurn(threadId, userText, { supersedes: turnId, reply: opts.reply });
        }, 400);
        return;
      }
      if (i >= words.length) {
        const endedAt = now();
        const text = acc.trim();
        messagesByThread[threadId].push({ role: "assistant", text, turn_id: turnId, at: endedAt });
        const turn = turnsById.get(turnId);
        turn.runs = [{ text, startSeq: 0, at: startedAt }];
        turn.state = "completed";
        turn.endedAt = endedAt;
        emit("rich://message-completed", Object.assign({}, fence, {
          messageId, phase: "unknown", text, visibility: "ceo", at: endedAt,
        }));
        emit("rich://turn-completed", { threadId, turnId, stopReason: "end_turn", at: endedAt });
        emit("rich://turn-status", Object.assign({}, fence, {
          status: "completed", startedAt, activeDurationMs: endedAt - startedAt, visibility: "ceo", at: endedAt,
        }));
        emit("rich://thread-summary-updated", Object.assign({}, fence, {
          title: (threads.find((x) => x.id === threadId) || {}).title || "",
          messageCount: messagesByThread[threadId].length,
          lastActivity: endedAt, status: "idle", visibility: "ceo", at: endedAt,
        }));
        // What the turn wrote, announced where the real record announces it: after the turn's
        // rows are appended (`OutputStore::append` -> `rich://output`, output side-panel §6.6).
        if (typeof opts.onCompleted === "function") opts.onCompleted(turnId, endedAt);
        return;
      }
      if (!opened) {
        opened = true;
        emit("rich://message-started", Object.assign({}, fence, {
          messageId, phase: "unknown", seq: 0, visibility: "ceo", at: now(),
        }));
      }
      const delta = (i === 0 ? "" : " ") + words[i];
      acc += delta;
      emit("rich://chunk", { threadId, turnId, seq, textDelta: delta, at: now() });
      emit("rich://message-delta", Object.assign({}, fence, {
        messageId, seq, textDelta: delta, visibility: "ceo", at: now(),
      }));
      seq += 1;
      i += 1;
      // One real semantic activity row partway through, so the live activity lane and the
      // §6.4 collapse are exercised rather than only the reload path.
      if (!activityFired && i === Math.ceil(words.length / 2)) {
        activityFired = true;
        const act = {
          kind: "activity", id: "mach_" + turnId, entityId: fence.entityId, threadId,
          turnId, bindingRevision: fence.bindingRevision, createdAt: now(),
          sequence: seq, slot: "stream", visibility: "ceo",
          activityType: "command", state: "running", summary: "Ran a command",
        };
        turnsById.get(turnId).activities = [act];
        emit("rich://activity-upserted", Object.assign({}, act, { at: now() }));
        seq += 1;
        // The SAME row again as it reaches a terminal state — one tool call is ONE row that
        // arrives several times. A renderer that keyed on anything but `id` shows two.
        setTimeout(function () {
          const done = Object.assign({}, act, { state: "completed", completedAt: now(), updatedAt: now() });
          turnsById.get(turnId).activities = [done];
          emit("rich://activity-upserted", Object.assign({}, done, { at: now() }));
        }, 300);
      }
      setTimeout(next, 60 + Math.random() * 90);
    }
    // Small "thinking" delay before the first chunk so the `Working` state is visibly
    // exercised — including §6.1's under-one-second row, which has no number yet.
    setTimeout(next, 900 + Math.random() * 400);
  }

  // =======================================================================================
  // THE TWO CORRECTION DESKS (§7 "ask, never infer") — `correction.rs` and `staging.rs`
  // =======================================================================================
  //
  // Fourteen Tauri commands, mocked with the SAME state machine the Rust desks enforce,
  // because a preview harness that let `confirm` succeed twice, or let a proposal be
  // confirmed while suppressed, would rehearse a product that does not exist.
  //
  //   * `confirm` is the only path to a write, and refuses anything not awaiting an answer
  //     (`correction.rs:541`) — a double click must not write twice.
  //   * a decline is NOT permanent and the item stays re-askable (§7).
  //   * a permanent decline suppresses by REF/KEY, on a list that reads back and lifts.
  //   * `propose` refuses an empty `why` before anything is started, and refuses a
  //     suppressed target.
  //
  // THE TWO "NOT HERE" SENTENCES ARE COPIED VERBATIM from `app/src-tauri/src/main.rs`
  // (`desk()` and `spoken_desk()`), the way `_notConnected` copies
  // `LEASE_UNAVAILABLE_MESSAGE`. `corrections.js` asserts each is byte-identical to the
  // Rust const, so this preview can never rehearse a sentence the product no longer says.
  const LORO_DESK_ABSENT =
    "This install has no company memory it can write to, so there is nothing to read " +
    "or correct here. That is a statement about this install, not about what is recorded.";
  const SPOKEN_DESK_ABSENT =
    "I can't record corrections right now: my correction log could not be opened. " +
    "Nothing you say is being lost from the conversation itself.";

  // Written the way `loro-write --dry-run --json` writes it: front matter plus body, byte
  // for byte, because that IS what the CEO approves (`correction.rs:366-369`).
  const LORO_RECORD_NOW =
    "---\nid: decision-ship-thursday\nkind: decision\nscope: ceo-private\n---\n\n" +
    "We ship on Thursday.\n";
  const LORO_PREVIEW =
    "---\nid: decision-ship-date\nkind: decision\nscope: ceo-private\nsupersedes: rec:ceo/records/decision-ship-thursday\n---\n\n" +
    "No ship date is decided. Thursday was floated and never agreed.\n";

  let loroDeskOn = true;
  let spokenDeskOn = true;
  // A desk that IS there and refuses to answer — the transient half, which the surface
  // must render differently from "not installed" because only one of them has a retry.
  let loroReadFailure = null;
  let spokenReadFailure = null;

  let proposalSeq = 2;
  const proposals = [
    {
      id: "prop-1",
      at: now() - 1000 * 60 * 12,
      entity_id: "northwind",
      thread_id: "acme",
      write: {
        op: "supersede",
        recordRef: "rec:ceo/records/decision-ship-thursday",
        newId: "decision-ship-date",
        kind: "decision",
        scope: "ceo-private",
        body: "No ship date is decided. Thursday was floated and never agreed.",
      },
      why: "we never decided Thursday, that was Sara thinking out loud",
      preview: LORO_PREVIEW,
      state: "awaiting-ceo",
      outcome: null,
      failure: null,
    },
  ];
  const loroSuppressed = [];

  const candidates = [
    {
      key: "deep gram|Deepgram",
      at: now() - 1000 * 60 * 4,
      threadId: "acme",
      turnId: "turn_mock_spoken",
      utterance: "it's Deepgram, not deep gram",
      ask: {
        from: "deep gram",
        to: "Deepgram",
        key: "deep gram|Deepgram",
        frame: "pivot-first",
        orthographic: 0.89,
        phonetic: 1,
        leg: "both",
        anchor: "I'll get deep gram to transcribe the call",
      },
      declinedBefore: 0,
      // Verbatim from `staging.rs`'s `prompt_for` — the sentence §7 asks for, built in one
      // place so every surface asks it the same way.
      prompt: 'Add "Deepgram" to your vocabulary?',
    },
  ];
  const spokenSuppressed = [];
  /// How many times each pair has been plainly declined. §7: the next ask must say so,
  /// "or it reads as the system having forgotten".
  const declinedCounts = {};


  // ---- the feedback channel (`feedback.rs`) ---------------------------------------------
  //
  // A REAL STATE MACHINE, not a table of answers — the same standard the two correction
  // desks are held to here, and for the same reason. `feedback.rs` refuses a report on a
  // `3`, refuses one with no diagnosis term, refuses one whose rating disagrees with the
  // rating given, sorts and de-duplicates a selection so the same terms always render the
  // same bytes, and refuses an approval whose text is not what this build would say. A
  // harness that answered `{}` to every command would let a browser suite pass over a
  // surface that records consent for text nobody was shown.
  //
  // EVERY STRING BELOW IS A COPY, AND `feedback.js` CHECKS EACH ONE against
  // `tests/fixtures/feedback-vocabulary.json`, which a cargo test regenerates from the live
  // Rust constants on every run. A preview harness that rehearses wording the product no
  // longer uses is a fixture certifying the fixture.

  /// Verbatim from `FEEDBACK_STORE_UNAVAILABLE`, app/src-tauri/src/main.rs.
  const FEEDBACK_STORE_ABSENT =
    "I can't keep an answer right now: the file I record them in wouldn't open and I'm not going to ask you what you think and then lose it. That one is for whoever set RichOS up to look at; it isn't yours to fix.";
  /// Verbatim from `FEEDBACK_PREVIEW_MISMATCH`, app/src-tauri/src/main.rs.
  const FEEDBACK_PREVIEW_MISMATCH =
    "I won't record that. What you were shown isn't what I would say now, so approving it would be approving something you haven't read. Ask me to show it again.";

  /// `PROMPT_QUESTION`, `PROMPT_OPTIONS`, `REPORT_OFFER`, `DISCLOSURE_HEADING`.
  const FEEDBACK_QUESTION = "How is RichOS doing this session?";
  const FEEDBACK_OPTIONS = "1: Bad | 2: OK, but could be better | 3: Good | 0: Dismiss";
  const FEEDBACK_REPORT_OFFER =
    "Will you let your Rich tell the RichOS developers (fully anonymized and generically) what annoyed you and why it happened?";
  const FEEDBACK_DISCLOSURE_HEADING =
    "This is exactly what your Rich would report. In this version it is written to this machine and nowhere else; nothing in RichOS can carry it any further.";

  /// `Rating` — three variants, and `0` deliberately not one of them.
  const FEEDBACK_RATINGS = [
    { key: "1", label: "Bad", wire: "bad", invitesReport: true },
    { key: "2", label: "OK, but could be better", wire: "ok-but-could-be-better", invitesReport: true },
    { key: "3", label: "Good", wire: "good", invitesReport: false },
  ];

  /// The vocabulary, in DECLARATION ORDER — which is also the order `assemble` sorts a
  /// selection into, so the index in these arrays is the sort key.
  const FEEDBACK_FAILURE_CLASS = [
    { wire: "unprepared-task-handed-to-user", label: "The assistant handed the user a task it had not prepared." },
    { wire: "checking-handed-to-user", label: "The assistant left the user to notice a failure that machinery should have caught." },
    { wire: "assurance-handed-to-user", label: "The assistant left the user to ask whether a class of failure would recur." },
    { wire: "decision-handed-to-user", label: "The assistant asked the user a question whose answer was already determined." },
    { wire: "scheduling-handed-to-user", label: "The assistant left the user to sequence work it should have sequenced itself." },
  ];
  const FEEDBACK_OCCURRENCES = [
    { wire: "1", label: "once" },
    { wire: "2", label: "twice" },
    { wire: "3", label: "three times" },
    { wire: "4", label: "four times" },
    { wire: "5", label: "five times" },
    { wire: "more-than-5", label: "more than five times" },
  ];
  const FEEDBACK_DIAGNOSIS = [
    {
      wire: "request-repeated-without-preparation",
      sentence:
        "The assistant asked the user to carry out a manual verification task more than once in a single session without preparing the artifact the task required.",
    },
    { wire: "no-input-artifact-named", sentence: "No input file was named." },
    { wire: "no-location-within-input-specified", sentence: "No locations within it were specified." },
    { wire: "no-method-given", sentence: "No method was given." },
    { wire: "no-acceptance-criterion-stated", sentence: "No acceptance criterion was stated." },
    {
      wire: "automated-executors-received-self-contained-briefs",
      sentence:
        "In the same session the assistant produced detailed, self-contained briefs for its automated sub-agents.",
    },
    {
      wire: "human-executor-received-the-least-prepared-instruction",
      sentence: "The asymmetry is the defect: the human executor received the least prepared instruction.",
    },
  ];
  const FEEDBACK_CONDITIONS = [
    {
      wire: "record-section-for-items-awaiting-the-user",
      sentence:
        "The durable task record contained a section for items awaiting the user, which made relaying an item feel equivalent to preparing it.",
    },
    {
      wire: "no-user-facing-item-carried-an-acceptance-criterion",
      sentence: "No user-facing item carried an acceptance criterion.",
    },
    {
      wire: "rule-enforced-by-attention-rather-than-machinery",
      sentence: "The rule that would have prevented this was enforced by attention rather than by machinery.",
    },
  ];

  let feedbackStoreOpen = true;
  let feedbackReadFailure = null;
  /// The one file, as an array. Every entry is the shape `FeedbackEntry` serializes to.
  const feedbackEntries = [];

  /// `feedback.rs`'s `WRAP_COLUMNS`.
  const FEEDBACK_WRAP_COLUMNS = 76;

  /// `fold` — greedy word wrap into a two-space-indented folded block. Words are never
  /// split, so every token in the output is a token from the vocabulary.
  function feedbackFold(text) {
    let out = "";
    let line = "  ";
    for (const word of text.split(/\s+/).filter(Boolean)) {
      if (line.length > 2 && line.length + 1 + word.length > FEEDBACK_WRAP_COLUMNS) {
        out += line + "\n";
        line = "  ";
      }
      if (line.length > 2) line += " ";
      line += word;
    }
    if (line.length > 2) out += line + "\n";
    return out;
  }

  const feedbackTermIndex = (list, wire) => list.findIndex((t) => t.wire === wire);

  /// `FeedbackPayload::assemble` — sorted into vocabulary order and de-duplicated, so the
  /// same set of terms always produces the same payload and the same rendered text.
  function feedbackAssemble(rating, sel) {
    if (!rating.invitesReport)
      throw "rating '" + rating.key + "' does not invite a report — the offer is made on 1 and 2 only";
    const diagnosis = (sel.generic_diagnosis || []).slice();
    if (diagnosis.length === 0) throw "a report needs at least one diagnosis term";
    const order = (list) => (a, b) => feedbackTermIndex(list, a) - feedbackTermIndex(list, b);
    const uniq = (xs) => xs.filter((x, i) => xs.indexOf(x) === i);
    return {
      taxonomy_version: "v1",
      rating: rating.wire,
      failure_class: sel.failure_class,
      occurrences_this_session: sel.occurrences_this_session,
      generic_diagnosis: uniq(diagnosis.sort(order(FEEDBACK_DIAGNOSIS))),
      contributing_condition: uniq((sel.contributing_condition || []).slice().sort(order(FEEDBACK_CONDITIONS))),
    };
  }

  const feedbackSentence = (list, wire) => (list.find((t) => t.wire === wire) || {}).sentence;

  /// `render_disclosure` — deterministic and total, and the empty condition list is OMITTED
  /// rather than shown as a key with nothing under it.
  function feedbackRender(payload) {
    const ratingKey = (FEEDBACK_RATINGS.find((r) => r.wire === payload.rating) || {}).key;
    let out = "";
    out += "taxonomy_version: " + payload.taxonomy_version + "\n";
    out += "rating: " + ratingKey + "\n";
    out += "failure_class: " + payload.failure_class + "\n";
    out += "occurrences_this_session: " + payload.occurrences_this_session + "\n";
    out += "generic_diagnosis: >\n";
    out += feedbackFold(payload.generic_diagnosis.map((w) => feedbackSentence(FEEDBACK_DIAGNOSIS, w)).join(" "));
    if (payload.contributing_condition.length > 0) {
      out += "contributing_condition: >\n";
      out += feedbackFold(payload.contributing_condition.map((w) => feedbackSentence(FEEDBACK_CONDITIONS, w)).join(" "));
    }
    return out;
  }

  const feedbackFullText = (payload) => FEEDBACK_DISCLOSURE_HEADING + "\n\n" + feedbackRender(payload);

  /// `PromptOutcome::from_key` — anything that is not one of the four keys is not an answer
  /// at all, and is never silently recorded as a dismissal.
  function feedbackOutcome(key) {
    const rating = FEEDBACK_RATINGS.find((r) => r.key === key);
    if (rating) return { kind: "rated", value: rating.wire };
    if (key === "0") return { kind: "dismissed" };
    return null;
  }

  // ---- techy mode (techy-mode design §3.1) ---------------------------------------------
  //
  // The THREE sentences below are `machinery_view.rs`'s consts, verbatim. `techy.js` check 2
  // compares them to the Rust source, so a reworded sentence reaches this file through a
  // failing check rather than through a stale copy nobody looked at.
  const TECHY_NOTHING_RECORDED =
    "No machinery was recorded for this conversation. Retention started on 2026-08-28 and " +
    "anything Rich did before that was never written down, so this is a gap in the record, " +
    "not a quiet conversation.";
  const TECHY_NOT_RETAINED =
    "Nothing has been recorded on this machine yet. The technical view reads a store that " +
    "hasn't been written to. It fills up as Rich works.";
  const TECHY_UNREADABLE =
    "I can't read the technical record for this conversation. It's on this machine and I " +
    "haven't lost it. Something is refusing to open it and whoever set RichOS up needs to " +
    "look.";
  // §1.5's between-turn lane, when it is empty. Verbatim from `machinery_view.rs`'s
  // `BETWEEN_TURNS_QUIET`; `techy.js` check 2 compares them.
  const TECHY_BETWEEN_TURNS_QUIET =
    "Nothing was recorded between turns in this conversation. Rich started keeping this on " +
    "2026-08-30, so in an older conversation that is a gap in the record, not proof the " +
    "session was quiet.";
  const TECHY_RAW_NOT_RETAINED =
    "The full output isn't kept this long. What's above is the whole record that was.";
  const TECHY_RAW_TRUNCATED = "This output was longer than RichOS keeps; you're seeing the start of it.";
  const TECHY_RAW_UNREADABLE =
    "I can't read the stored output for this one. It's on this machine and I haven't lost it. " +
    "Whoever set RichOS up needs to look.";

  let techyDefault = false;
  const techyThreads = new Map(); // threadId -> bool  (ABSENT means "follows the tier above")
  // §7.1's middle tier (the CEO's answer, 2026-09-18). ABSENT means "follows the global
  // default", exactly as an absent thread does — the two maps are the same shape because
  // `config.rs` makes them the same shape.
  const techyEntities = new Map(); // entityId -> bool

  // ---- §7.2: the raw-retention window, modelled rather than stubbed ---------------------
  //
  // The window is a SETTING (`config.rs`'s `raw_retention`, `journal.rs`'s `RawRetention`),
  // so the mock has to hold one — and it has to EVICT, because the sentence the surface says
  // after a change ("removed the stored output from N earlier days") is the whole reason the
  // control does not delete silently, and a mock that always returned 0 would let that
  // sentence rot untested.
  //
  // A stand-in journal: one raw day-shard per entry, of a known age and size. `evicted`
  // counts shards whose age is past the window, which is exactly what `evict_raw_within`
  // does with `unlink` — and NOTHING here touches the records, because Tier A is never
  // evicted at any setting.
  const RETENTION_WINDOWS = {
    "two-weeks": { ageDays: 14, totalBytes: 2147483648 },
    "three-months": { ageDays: 90, totalBytes: 2147483648 },
    forever: { ageDays: "forever", totalBytes: "forever" },
  };
  // STARTS AT `forever`, which is also what a fresh install is now (CEO 2026-10-06, feedback
  // item 6; `config.rs`, proven there). Under a two-week window a 120-day-old raw shard cannot
  // exist, because boot eviction removed it. So the interesting state for a control that
  // DELETES is this one, where there is something to delete: a CEO who keeps everything and
  // later tightens it.
  let retentionChoice = "forever";
  let rawShards = [
    { ageDays: 120, bytes: 41_000_000 },
    { ageDays: 60, bytes: 12_500_000 },
    { ageDays: 20, bytes: 8_100_000 },
    { ageDays: 9, bytes: 3_300_000 },
    { ageDays: 0, bytes: 900_000 },
  ];
  /// The view the two commands return, in the shape `RetentionView` puts on the wire.
  function retentionView(evicted) {
    const w = RETENTION_WINDOWS[retentionChoice] || RETENTION_WINDOWS["two-weeks"];
    return {
      choice: retentionChoice,
      ageDays: w.ageDays,
      totalBytes: w.totalBytes,
      retainedBytes: rawShards.reduce((n, s) => n + s.bytes, 0),
      evicted: evicted || 0,
    };
  }
  /// Thread ids the OS would refuse to read. Not "empty" — a different state entirely.
  const machineryUnreadable = new Set();

  /// §2.4's Tier-B raw payloads, per machinery id. A row ABSENT from this map is one whose
  /// raw window has passed: the normalized record still renders and the pane says so.
  /// `mach_a5` is deliberately missing for exactly that reason, and `mach_a6` is over the
  /// 32 KB cap and comes back as a truncated PREFIX.
  const machineryRaw = new Map([
    ["mach_a1", { payload: { command: "cat comparables/q3-acme.csv", stdout: "list,offer,spread\n4200000,3864000,0.08\n" }, truncated: false }],
    ["mach_a2", { payload: { command: "cat comparables/q3-market.csv", stdout: "31 rows" }, truncated: false }],
    ["mach_a3", { payload: { command: "cat notes/hensley-relationship.md", stdout: "Hensley has carried the relationship since 2019." }, truncated: false }],
    ["mach_a4", { payload: { command: "python3 scripts/counter-model.py --list 4200000 --offer 3864000", stdout: "spread 8.0% · within comparable range" }, truncated: false }],
    ["mach_a6", { payload: "{\"command\":\"cat comparables/q4-acme.csv\",\"stderr\":\"cat: comparables/q4-acme.csv: No such file or directory", truncated: true }],
    ["mach_a7", { payload: { type: "stream_event", event: { type: "message_delta",
        usage: { input_tokens: 2, cache_read_input_tokens: 41991, cache_creation_input_tokens: 3603 } } },
      truncated: false }],
    ["mach_a8", { payload: { chosen: "allow", auto: true, options: ["allow", "reject"] }, truncated: false }],
  ]);

  /// §1.5's BETWEEN-TURN LANE, per thread — what the session said with no turn in flight.
  ///
  /// `acme` has traffic; `hiring` deliberately has none, so the honest empty state is a
  /// screen this harness can actually open rather than a branch nobody drives. The shape is
  /// `timeline::BetweenTurnItem` — note what is NOT here: `turnId` (these records have
  /// none, §1.4 G4) and `sessionId` (never on the wire, because a lane whose rows arrive at
  /// session boundaries is where a session id would become a rotation tell).
  const betweenTurnsByThread = new Map([
    [
      "acme",
      [
        {
          id: "mach_bt1",
          bindingRevision: 1,
          sequence: 0,
          at: now() - 1000 * 60 * 60 * 21,
          visibility: "technical",
          vendorKind: "system:init",
          detailRef: "mach_bt1",
          // NO `summary`, and that is the fixture speaking rather than a gap. A SessionMeta
          // frame has no typed route (`MachineryKind::Unknown`), so `from_native_event`
          // produces no bounded preview for it — the live fixture at
          // `fixtures/machinery-payload.json` has none either. One dim line carrying its
          // vendor frame name is exactly what §1.4 G5 asks for.
          detail: {
            title: "system:init",
            locations: [],
            vendorKind: "system:init",
          },
        },
        {
          id: "mach_bt2",
          bindingRevision: 1,
          sequence: 1,
          at: now() - 1000 * 60 * 29,
          visibility: "technical",
          vendorKind: "system:status",
          detailRef: "mach_bt2",
          detail: {
            title: "system:status",
            locations: [],
            vendorKind: "system:status",
          },
        },
      ],
    ],
  ]);

  /// The lane as `Timeline::view` hands it over: rows in TECHNICAL mode, nothing in CEO
  /// mode. Every row is `Visibility::Technical`, so the calm view is handed an empty lane
  /// by construction — reproduced here rather than filtered in the renderer, because that
  /// is where the real gate is.
  function betweenTurnsFor(threadId, mode) {
    if (mode !== "technical") return [];
    const t = threads.find((x) => x.id === threadId);
    const rows = betweenTurnsByThread.get(threadId) || [];
    return rows.map((r) => ({ ...r, entityId: t ? t.entity_id : null, threadId }));
  }

  /// THREAD PIN, ELSE COMPANY PIN, ELSE THE GLOBAL DEFAULT — `ConfigStore::techy_mode`'s
  /// order, and the mock resolves it rather than stubbing an answer, because the whole
  /// point of the three tiers is which one wins.
  ///
  /// The company comes from the thread's own `entity_id`, the same field the real command
  /// gets from `Ledger::thread_binding`. A thread this mock does not know, or one with no
  /// company, skips the middle tier instead of guessing — again matching the real path,
  /// where an unreadable binding falls through to the global default.
  function techyCompanyOf(threadId) {
    const t = threads.find((x) => x.id === threadId);
    return t && t.entity_id ? t.entity_id : null;
  }

  function techyModeOf(threadId) {
    if (techyThreads.has(threadId)) {
      return { enabled: techyThreads.get(threadId), source: "thread", default: techyDefault };
    }
    const company = techyCompanyOf(threadId);
    if (company && techyEntities.has(company)) {
      return { enabled: techyEntities.get(company), source: "entity", default: techyDefault };
    }
    return { enabled: techyDefault, source: "default", default: techyDefault };
  }

  /// The FOUR states of `get_machinery`, kept apart exactly as `ThreadMachinery` keeps them.
  /// "There is nothing in it" and "I could not read it" are different answers and this mock
  /// refuses to collapse them, because collapsing them is the defect under test.
  function machineryStateOf(threadId) {
    if (machineryUnreadable.has(threadId)) {
      return { state: "unreadable", sentence: TECHY_UNREADABLE, reason: "machinery/" + threadId + ": Permission denied (os error 13)" };
    }
    const rows = [];
    for (const [, turn] of turnsById) {
      if (turn.threadId === threadId) for (const a of turn.activities) rows.push(a);
    }
    if (!rows.length) return { state: "nothing_recorded", sentence: TECHY_NOTHING_RECORDED, reason: null };
    return { state: "recorded", sentence: null, reason: null, rowCount: rows.length };
  }

  const loroGate = () => (loroDeskOn ? loroReadFailure : LORO_DESK_ABSENT);
  const spokenGate = () => (spokenDeskOn ? spokenReadFailure : SPOKEN_DESK_ABSENT);

  function refOf(write) {
    return write && (write.recordRef || write.record_ref) ? write.recordRef || write.record_ref : null;
  }

  // ---- the launch record, RECORDED and never reimplemented -----------------------------
  //
  // The shell's two launch commands are answered here so the browser suite can prove the
  // whole path — the id that was drawn reaching the recency ring, and the LOCAL offset
  // reaching the buckets. What this deliberately does NOT do is bucket anything: a second
  // implementation of "today / this week / this month" living in the mock is a second thing
  // that can disagree with `launch.rs`, and the arithmetic is already proven there over all
  // 292,194 days from 1600 to 2400. So the mock records the calls and returns a fixed
  // record; the numbers are Rust's job.
  const launchCalls = { splashShown: [], stateReads: [] };
  const LAUNCH_STATE = {
    kind: "fresh",
    counts: { today: 1, thisWeek: 3, thisMonth: 12, thisYear: 47, total: 47 },
    installedAt: 1788166800000,
    recentSplashes: [],
    readable: true,
    schemaVersion: 1,
  };

  // ---- the update path (RICH-TODOs row 12) ------------------------------------------------
  //
  // `unconfigured` is the OPENING state and that is deliberate: it is what the shipped
  // config actually produces today, because the endpoint is the RFC 2606 `.invalid`
  // placeholder and where RichOS updates are hosted has not been decided. A harness that
  // opened on "up to date" would be modelling a product we do not have.
  const mockUpdate = {
    view: {
      state: "unconfigured",
      currentVersion: "0.1.0",
      availableVersion: null,
      notes: null,
      pubDate: null,
      downloadedBytes: 0,
      totalBytes: null,
      percent: null,
      failure: null,
      endpoint: "https://updates.richos.invalid/{{target}}/{{arch}}/{{current_version}}",
      endpointIsPlaceholder: true,
      checkedAt: null,
      // THE WORK GATE (`src-tauri/src/updates.rs`, CEO 2026-09-05). `false` is the opening
      // value because the preview has no spine, no lease and no workers — there is nothing
      // here that could be busy, and opening with the control hidden would model a product
      // that never offers an update at all. A suite drives it through `updateSet`.
      busy: false,
      busyReason: null,
      unchecked: [],
      readySince: null,
    },
    script: [],
    calls: [],
  };

  // ---- what did not load out of the ledger (2026-09-05) -------------------------------------
  //
  // CLEAN is the opening state, because a clean load is what every real launch on every real
  // ledger produces today — 71 records across five real files, none of them skipped. The
  // notice is meant to be invisible; a harness that opened with it showing would be modeling
  // a product that routinely loses records.
  const mockHistoryHealth = {
    records_read: 42,
    records_applied: 42,
    skipped: 0,
    from_future: 0,
    damaged: 0,
    ambiguous: 0,
    headline: "",
    detail: "",
  };

  /// Pop the next scripted view, or — with nothing scripted — leave the state alone. A
  /// harness that invented a transition would let a suite pass against a flow that never ran.
  function mockUpdateAdvance() {
    if (mockUpdate.script.length) mockUpdate.view = mockUpdate.script.shift();
    return { ...mockUpdate.view };
  }

  // ---- screenshots and files on the Mac composer (CEO §86, `src-tauri/src/mac_attachments.rs`)
  //
  // The desk's accepted kinds and the Mac's sentences, VERBATIM from `phone/attachments.rs`
  // (`ACCEPTED`, `Origin::Mac`) — `tests/attachments.js` reads the Rust and asserts each one
  // matches, so the preview cannot rehearse a refusal the product no longer says.
  const MOCK_ATTACH_KINDS = [
    { mediaType: "image/jpeg", extension: "jpg", extensions: ["jpg", "jpeg"], label: "JPEG photo", magic: [0xff, 0xd8, 0xff] },
    { mediaType: "image/png", extension: "png", extensions: ["png"], label: "PNG image", magic: [0x89, 0x50, 0x4e, 0x47] },
    { mediaType: "image/heic", extension: "heic", extensions: ["heic", "heif"], label: "HEIC photo" },
    { mediaType: "image/heif", extension: "heif", extensions: ["heif", "heic"], label: "HEIF photo" },
    { mediaType: "image/gif", extension: "gif", extensions: ["gif"], label: "GIF image", magic: [0x47, 0x49, 0x46, 0x38] },
    { mediaType: "image/webp", extension: "webp", extensions: ["webp"], label: "WebP image" },
    { mediaType: "application/pdf", extension: "pdf", extensions: ["pdf"], label: "PDF", magic: [0x25, 0x50, 0x44, 0x46, 0x2d] },
    { mediaType: "text/plain", extension: "txt", extensions: ["txt", "text", "log"], label: "text file" },
    { mediaType: "text/markdown", extension: "md", extensions: ["md", "markdown"], label: "Markdown file" },
    { mediaType: "text/csv", extension: "csv", extensions: ["csv"], label: "CSV file" },
    { mediaType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document", extension: "docx", extensions: ["docx"], label: "Word document" },
    { mediaType: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", extension: "xlsx", extensions: ["xlsx"], label: "Excel workbook" },
    { mediaType: "application/vnd.openxmlformats-officedocument.presentationml.presentation", extension: "pptx", extensions: ["pptx"], label: "PowerPoint presentation" },
  ];
  const MOCK_ATTACH_SAYS = {
    unknownType: "RichOS can't take this kind of file yet. Photos, PDFs, text and Office documents work. Nothing was attached.",
    empty: "This file is empty. Nothing was attached.",
    tooLarge: "This file is larger than 25 MB, the most RichOS takes in one file. Nothing was attached.",
    notWhatItSays: (label) => "This file does not look like a " + label + ". Nothing was attached.",
    tooMany: "A message can carry at most 10 files.",
  };
  /// Where the mock "keeps" a committed file. A path of the real shape, under a folder that
  /// cannot exist on anybody's Mac.
  const MOCK_ATTACH_HOME = "/mock/Library/Application Support/com.richos.app/attachments";
  const mockAttach = { staged: new Map(), drops: new Map(), nextDrop: 0, calls: [], missingNext: null };

  function mockKindFor(declared, name) {
    const bare = String(declared || "").split(";")[0].trim().toLowerCase();
    const byType = MOCK_ATTACH_KINDS.find((k) => k.mediaType === bare);
    if (byType) return byType;
    const dot = String(name || "").lastIndexOf(".");
    if (dot < 0) return null;
    const ext = name.slice(dot + 1).toLowerCase();
    return MOCK_ATTACH_KINDS.find((k) => k.extension === ext) || MOCK_ATTACH_KINDS.find((k) => k.extensions.includes(ext)) || null;
  }

  /// The desk's `stage_from(Origin::Mac, …)`, as far as a browser preview needs it.
  function mockStage(draftId, id, name, declared, bytes) {
    const kind = mockKindFor(declared, name);
    if (!kind) return Promise.reject(MOCK_ATTACH_SAYS.unknownType);
    if (!bytes.length) return Promise.reject(MOCK_ATTACH_SAYS.empty);
    if (bytes.length > 25 * 1024 * 1024) return Promise.reject(MOCK_ATTACH_SAYS.tooLarge);
    if (kind.magic && !kind.magic.every((b, i) => bytes[i] === b)) return Promise.reject(MOCK_ATTACH_SAYS.notWhatItSays(kind.label));
    const inDraft = [...mockAttach.staged.values()].filter((s) => s.draftId === draftId);
    if (inDraft.length >= 10) return Promise.reject(MOCK_ATTACH_SAYS.tooMany);
    const staged = {
      draftId,
      id,
      name: String(name || "attachment").split("/").pop(),
      mediaType: kind.mediaType,
      label: kind.label,
      size: bytes.length,
      // Not a real SHA-256: the mock binds the id to its length and name, which is all a
      // browser preview can check. The Rust desk hashes the bytes.
      sha256: ("m" + bytes.length + "-" + name).replace(/[^A-Za-z0-9]/g, "").padEnd(64, "0").slice(0, 64),
    };
    mockAttach.staged.set(draftId + "/" + id, staged);
    const { draftId: _d, ...view } = staged;
    return Promise.resolve(view);
  }

  let quotaPolicy = { enabled: true, pausePercent: 93 }; // a fresh install pauses (D13)
  try { quotaPolicy = JSON.parse(localStorage.getItem("richos-mock-quota-policy")) || quotaPolicy; } catch (_) {}
  let resetOffers = structuredClone(preset.quota?.resets || { state: "unknown", offers: [], approval: null, lastAttempt: null, weeklyThreshold: 99, message: "Reset availability is unknown in this preview." });
  window.__richosResetCalls = [];
  function quotaView() {
    const fixture = preset.quota || { state: "unavailable", windows: [], checkedAt: null, retryAt: null, message: "Quota is unavailable in this browser preview." };
    const window = fixture.windows.find(w => w.id === "five_hour");
    const remaining = (window?.resetsAt || 0) - Date.now();
    const high = window?.usedPercent >= quotaPolicy.pausePercent;
    const weekly = fixture.windows.find(w => w.id === "seven_day" && w.usedPercent >= 99 && w.resetsAt > Date.now());
    // As quota.rs decides it: with no current reading, work waits only while a check is on its
    // way ("unknown"); once a check came back with no figures (`emptyAt`), nothing is held on it.
    const unread = { state: fixture.emptyAt ? "noReading" : "unknown" };
    const admission = !quotaPolicy.enabled ? { state: "disabled" } : weekly ? (weekly.resetsAt > Date.now() ? { state: "held", resetsAt: weekly.resetsAt } : { state: "unknown" }) : !window || remaining <= 0 ? unread
      : high && remaining < 20 * 60000 ? { state: "ready" } : high ? { state: "held", resetsAt: window.resetsAt }
      : fixture.state === "fresh" ? { state: "ready" } : unread;
    // Fill-first in the preview: the preset may carry `accounts`, `atThreshold`, `heldUntil`,
    // `actAt` and `refreshIntervalMs`; Add and Remove change the preview's own list.
    const list = mockAccounts.length ? mockAccounts : (fixture.accounts || []);
    const accounts = list.length > 1 ? structuredClone(list) : [];
    const atThreshold = mockAtThreshold !== "pause" ? mockAtThreshold : (fixture.atThreshold || "pause");
    // Round 18 (accounts.js): Recent changes, Rich's suggestion and its answer, his lines about
    // accounts by turn, and the one account's name; the preset may carry each, and the preview's
    // own Add, Use this one now, Remove and Not now change them as the app's record does.
    const inUse = accounts.find(a => a.inUse) || list[0];
    const firstLabel = accounts.length ? null : (inUse && inUse.label && inUse.label !== "Account 1" ? inUse.label : (fixture.firstLabel ?? null));
    return { refreshIntervalMs: 5 * 60000, ...fixture, resets: structuredClone(resetOffers), policy: { ...quotaPolicy }, admission,
      accounts, atThreshold, heldUntil: fixture.heldUntil ?? null, changes: structuredClone(mockChanges), nudge: mockNudge && { ...mockNudge },
      notes: structuredClone(mockNotes), firstLabel, lastSwitch: mockLastSwitch === undefined ? (fixture.lastSwitch ?? null) : mockLastSwitch };
  }
  let mockAccounts = structuredClone(preset.quota?.accounts || []);
  let mockAtThreshold = preset.quota?.atThreshold || "pause";
  let mockChanges = structuredClone(preset.quota?.changes || []);
  let mockNudge = structuredClone(preset.quota?.nudge || null);
  let mockNotes = structuredClone(preset.quota?.notes || []);
  let mockLastSwitch = undefined;
  // The add flow's sign-in, answered in order by `preset.signIn` (round 18: "connected", or
  // "same-account" for the same Claude account signed in again); "connected" when it runs out.
  let mockSigning = null;
  // Without a `signIn` preset the sign-in ends with no answer, as the preview always answered
  // (tests/quota.js reads an added account as "Not read yet" then).
  const mockSignIn = preset.signIn ? structuredClone(preset.signIn) : null;
  window.__accountCalls = [];
  const changed = (kind, id, label, extra) => mockChanges.push({ at: Date.now(), kind, id, label, ...(extra || {}) });
  // BUST A BUG (round 21; `src-tauri/src/bug_report.rs` is the real half). The connection is a
  // switch, as round 21's strip makes it: `preset.bugNet` is "online" (default), "offline",
  // "down" or "not-set-up", and `__RICHOS_MOCK_BUG__.setNet()` moves it while the page runs.
  // Going back "online" sends every waiting report by itself, the way the shell's retry does,
  // and says so on `rich://bug-report`. Every call is recorded with its arguments, so a suite can
  // read exactly what would have gone to GitHub.
  //
  // The write-up here stands in for Rich's checked one, with this preview's names as the private
  // words. What is private, and Rich's own write-up, are decided in Rust and tested there
  // (`crates/richos-core/tests/bug_report_tests.rs`); this only has to answer in the same shape.
  // `preset.bugClaude` "down" is Claude failing: the write-up answers `{state: "unchecked"}` and
  // keeps the report, a change is refused, and `claudeAnswers()` is the shell's check loop once
  // Claude answers again (review rv-20261009T162841Z-69294215-70e6 finding 2).
  //
  // THE RACES the second review found (rv-20261009T102303Z-1c3dda1d-4c78), as switches:
  // `preset.bugChangeHold` keeps a change Rich is making unanswered until `releaseChange()`;
  // `preset.bugCancel` is "fail" (the waiting copy could not be deleted) or "hold" (cancel waits,
  // as the shell's does behind a retry already sending, until `releaseCancel()`);
  // `retryGoesOut()` is that retry succeeding: the waiting report is filed and the event says so.
  // `preset.bugRichPrivate` is the private words Rich names in his write-up beyond the app's.
  // `preset.bugPrivateWords` is what the core's scrubber finds in the card's words beyond the
  // names this preview holds (an address, a path): the heads-up's answer, `bug_report_private_in`,
  // whose rules are Rust's and tested there (fourth review rv-20261009T142223Z-3d74fe4c-3b6d).
  // `preset.bugPrivateHold` keeps every one of those answers back until `releasePrivate()`, so a
  // suite can press Send before the heads-up's answer arrives (review
  // rv-20261009T151254Z-84d1bdce-20df finding 2).
  //
  // SEND, AS THE SHELL DECIDES IT (`bug_report_send`, `richos_core::bug_report::at_send`; review
  // rv-20261009T174727Z-3007e320-578a): what Rich last checked of each report is kept here by its
  // id (`checks`), and a sheet goes only when it is that, word for word. Words that changed (by
  // hand, or a change told to Rich) go through the scanner and then Rich first (CEO §115): what
  // they leave out is the report's own private words, `preset.bugPrivateWords` (the scanner's) and
  // `preset.bugRichCheck` (Rich's last pass); when that changes nothing the sheet goes, otherwise
  // the card comes back with his version. With Claude "down" the changed words are kept
  // (`unchecked()`, with `edited`).
  // `preset.bugSendCheckHold` keeps that check unanswered until `releaseSendCheck()`. `filed()` is
  // every sheet that actually went out.
  // `preset.bugSteps` is the numbered steps Rich writes under What happened (none by default), so
  // a suite can change and paste into a step (review rv-20261009T225657Z-b3b5c573-96c0).
  // `preset.bugRichWrote` is What happened as Rich wrote it, with his own stand-ins and nothing
  // listed beside them (nightly 48's walk, D23). The core marks each by its kind alone, with no
  // replaced words (`richos_core::bug_report::finished`, `Segment::unaccounted`); so does this.
  // `preset.bugRichSection` (`{heading, text}`) is one more section Rich writes, last, its heading
  // as he wrote it: a heading is plain words in the draft, stand-ins and all (review
  // rv-20261010T183313Z-fe4cd6b1-1e66).
  //
  // REQUEST A FEATURE (round 24): every command takes `post` ("idea"; none is a bug report), as
  // the shell's do. An idea's write-up has round 24's sections (The idea, Where it came up,
  // Version), and it is posted as a discussion: `filed()` entries carry `post`, `ideasPosted()` is
  // every idea that went to the (stand-in) Discussions, and an idea never reaches an issue.
  // `preset.bugNet` "refused" is an account GitHub will not let post there.
  const bugMock = {
    net: preset.bugNet || "online",
    account: preset.bugAccount || { kind: "reporting" },
    issue: 412,
    discussion: 413,
    waiting: [],
    sent: [],
    calls: [],
    seq: 0,
    held: { change: [], cancel: [], private: [], sendcheck: [], stop: [] },
    claude: preset.bugClaude || "up",
    unchecked: [],
    checks: {},
    filed: [],
  };
  // A release lets through what is held now and anything that arrives after it.
  const bugHold = (kind) => (bugMock.held[kind] === "released" ? Promise.resolve() : new Promise((r) => bugMock.held[kind].push(r)));
  const bugRelease = (kind) => {
    const waiting = bugMock.held[kind];
    bugMock.held[kind] = "released";
    if (Array.isArray(waiting)) waiting.forEach((r) => r());
  };
  const BUG_REASON = { offline: "offline", down: "github-down", "not-set-up": "not-set-up", refused: "refused" };
  const BUG_VERSION = "RichOS 1.2.0, preview · macOS 15.6 · Apple silicon";
  const BUG_KINDS = { conversation_name: "[a conversation]", company_name: "[a company]", person_name: "[a person]", folder_name: "[a folder]", file_path: "[a file on this Mac]", email_address: "[an email address]", private_word: "[a private word]" };
  function bugTerms() {
    const list = [];
    threads.forEach((t) => { if (/\s/.test(t.title)) list.push({ text: t.title, kind: "conversation_name" }); });
    entities.forEach((e) => list.push({ text: e.display_name, kind: "company_name" }));
    if (mockConfig.user_name) list.push({ text: mockConfig.user_name, kind: "person_name" });
    return list.sort((a, b) => b.text.length - a.text.length);
  }
  function bugScrub(text, extra) {
    const terms = bugTerms().concat(extra || []).sort((a, b) => b.text.length - a.text.length);
    const parts = terms.map((t) => t.text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    parts.push("(?:~|/Users)/[^\\s,;)]+");
    const re = new RegExp(parts.join("|"), "gi");
    const out = [];
    let last = 0, m;
    while ((m = re.exec(text))) {
      if (m.index > last) out.push({ text: text.slice(last, m.index) });
      const hit = terms.find((t) => t.text.toLowerCase() === m[0].toLowerCase());
      const kind = hit ? hit.kind : "file_path";
      out.push({ text: BUG_KINDS[kind], was: m[0], kind });
      last = m.index + m[0].length;
    }
    if (last < text.length || !out.length) out.push({ text: text.slice(last) });
    return out;
  }
  /// Words Rich wrote, each stand-in in them marked by its kind with no replaced words (the core's
  /// `marked` when nothing accounts for one).
  function bugMarked(text) {
    const kinds = Object.keys(BUG_KINDS);
    const re = new RegExp(kinds.map((k) => BUG_KINDS[k].replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|"), "g");
    const out = [];
    let last = 0, m;
    while ((m = re.exec(text))) {
      if (m.index > last) out.push({ text: text.slice(last, m.index) });
      out.push({ text: m[0], kind: kinds.find((k) => BUG_KINDS[k] === m[0]) });
      last = m.index + m[0].length;
    }
    if (last < text.length || !out.length) out.push({ text: text.slice(last) });
    return out;
  }
  function bugSentence(t) {
    t = t.trim();
    t = t.charAt(0).toUpperCase() + t.slice(1);
    return /[.!?…]$/.test(t) ? t : t + ".";
  }
  /// Rich's checked draft, in the shape the shell answers it, with this preview's names and
  /// `preset.bugRichPrivate` as the private words.
  function bugDraft(answerText, screen, post) {
    const answer = String(answerText || "").trim();
    let first = answer.split(/[.!?]\s/)[0].replace(/[.!?]+$/, "");
    if (first.length > 76) first = first.slice(0, 76).replace(/\s+\S*$/, "");
    const pub = String((screen || {}).public || "a RichOS screen");
    const rich = preset.bugRichPrivate || [];
    if (post === "idea") {
      // Round 24's plain write-up of an idea: its first sentence as the title, the idea with
      // private words replaced, where it came up, the version; and "What the user pictured" as a
      // bulleted list when `preset.ideaPictured` gives its points.
      const pictured = preset.ideaPictured || [];
      return {
        title: bugScrub(first.charAt(0).toUpperCase() + first.slice(1), rich),
        sections: [{ heading: "The idea", paragraphs: [bugScrub(bugSentence(answer), rich)], steps: [] }]
          .concat(pictured.length ? [{ heading: "What the user pictured", paragraphs: [], steps: pictured.map((t) => bugScrub(t, rich)) }] : [])
          .concat([
            { heading: "Where it came up", paragraphs: [[{ text: pub.charAt(0).toUpperCase() + pub.slice(1) + "." }]], steps: [] },
            { heading: "Version", paragraphs: [[{ text: BUG_VERSION }]], steps: [] },
          ]),
        private: rich,
      };
    }
    return {
      title: bugScrub(first.charAt(0).toUpperCase() + first.slice(1), rich),
      sections: [
        { heading: "What happened", paragraphs: [preset.bugRichWrote ? bugMarked(preset.bugRichWrote) : bugScrub(bugSentence(answer), rich)], steps: (preset.bugSteps || []).map((t) => bugScrub(t, rich)) },
        { heading: "Where", paragraphs: [[{ text: pub.charAt(0).toUpperCase() + pub.slice(1) + ". It was on screen when the report was started." }]], steps: [] },
        { heading: "Version", paragraphs: [[{ text: BUG_VERSION }]], steps: [] },
      ].concat(preset.bugRichSection ? [{ heading: preset.bugRichSection.heading, paragraphs: [bugMarked(preset.bugRichSection.text)], steps: [] }] : []),
      private: rich,
    };
  }
  function bugDigest(screen) {
    screen = screen || {};
    return "Noted the screen you were on" + (screen.textSize && screen.textSize !== 100 ? ", at " + screen.textSize + "% text size" : "") + " · Checked the version";
  }
  /// The card's words as the window reads them (`sheetOf`; the core's `sheet_of`).
  function bugSheetOf(draft) {
    const words = (segs) => (segs || []).map((s) => s.text).join("").trim();
    return {
      title: words(draft.title),
      sections: draft.sections.map((s) => ({ heading: s.heading, paragraphs: (s.paragraphs || []).map(words).filter(Boolean), steps: (s.steps || []).map(words).filter(Boolean) })),
    };
  }
  /// Rich's check of a report on a card, kept under its id (the shell's `checks`).
  function bugKeepCheck(report, draft) {
    bugMock.checks[report] = { sheet: JSON.stringify(bugSheetOf(draft)), private: draft.private || [] };
  }
  /// The card's words as a draft again, with `named` left out (the core's `redraft`).
  function bugRedraft(sheet, named) {
    return {
      title: bugScrub(sheet.title, named),
      sections: sheet.sections.map((s) => ({ heading: s.heading, paragraphs: s.paragraphs.map((p) => bugScrub(p, named)), steps: s.steps.map((p) => bugScrub(p, named)) })),
      private: named,
    };
  }
  /// What Rich names private when he checks words the user changed by hand.
  function bugRichCheck(report) {
    const kept = (bugMock.checks[report] || {}).private || [];
    const scrubbed = (preset.bugPrivateWords || []).map((w) => ({ text: w, kind: /@/.test(w) ? "email_address" : "private_word" }));
    return kept.concat(preset.bugRichCheck || [], scrubbed);
  }
  function bugDeliver(sheet, id, post) {
    const reason = BUG_REASON[bugMock.net];
    if (!reason) {
      bugMock.filed.push(Object.assign({ post: post || "bug" }, sheet));
      if (post === "idea") {
        // A discussion in the Ideas category, never an issue (the shell's `post_discussion`).
        const number = bugMock.discussion++;
        return { state: "sent", id, number, url: "https://github.com/WebDevBooster/richos/discussions/" + number, account: bugMock.account, title: sheet.title, sent_at_ms: now(), post: "idea" };
      }
      const number = bugMock.issue++;
      return { state: "sent", id, number, url: "https://github.com/WebDevBooster/richos/issues/" + number, account: bugMock.account, title: sheet.title, sent_at_ms: now(), post: "bug" };
    }
    if (!bugMock.waiting.some((w) => w.id === id)) bugMock.waiting.push({ id, sheet, post: post || "bug" });
    return { state: "waiting", id, reason };
  }
  window.__RICHOS_MOCK_BUG__ = {
    calls: bugMock.calls,
    waiting: () => bugMock.waiting.slice(),
    releaseChange: () => bugRelease("change"),
    releaseCancel: () => bugRelease("cancel"),
    releasePrivate: () => bugRelease("private"),
    releaseStop: () => bugRelease("stop"),
    /// An utterance recognized by the shell, as `rich://voice-transcript` carries it. The shell
    /// emits it whether or not it held the words back for a report (`bug_report_voice`).
    say(text) { emit("rich://voice-transcript", { text, at: now() }); },
    /// A retry that was already sending when the user pressed Cancel: it goes out, and says so.
    retryGoesOut() {
      bugMock.waiting.splice(0).forEach((w) => {
        const idea = w.post === "idea";
        const number = idea ? bugMock.discussion++ : bugMock.issue++;
        const d = { state: "sent", id: w.id, number, url: "https://github.com/WebDevBooster/richos/" + (idea ? "discussions/" : "issues/") + number, account: bugMock.account, title: w.sheet.title, sent_at_ms: now(), post: w.post || "bug" };
        bugMock.sent.push(d);
        emit("rich://bug-report", { delivery: d });
      });
    },
    setNet(net) {
      bugMock.net = net;
      if (net !== "online") return;
      const due = bugMock.waiting.splice(0);
      due.forEach((w) => setTimeout(() => {
        const d = bugDeliver(w.sheet, w.id, w.post);
        if (d.state === "sent") bugMock.sent.push(d);
        emit("rich://bug-report", { delivery: d });
      }, 300));
    },
    /// The reports Rich could not check, kept on this Mac (`Outbox::unchecked`).
    unchecked: () => bugMock.unchecked.slice(),
    /// Claude can't be asked (`preset.bugClaude` "down") or can again.
    setClaude(state) { bugMock.claude = state; },
    /// A report kept before the app was last quit: on this Mac, with no card in this window.
    keptBefore(answer, screen, post) {
      bugMock.unchecked.push({ id: "00000000-0000-4000-9000-" + String(++bugMock.seq).padStart(12, "0"), answer, screen: screen || {}, post: post || "bug" });
    },
    /// Claude answers again: the shell's check loop asks Rich about each report he could not
    /// check and says each one on `rich://bug-report` as `{ checked }`, as it does on every pass
    /// until the window takes it.
    claudeAnswers() {
      bugMock.claude = "up";
      bugMock.unchecked.forEach((u) => {
        // Changed words he could not check at Send: the scanner and then he go over those (the
        // core's `checked_edit`).
        const draft = u.edited ? bugRedraft(u.edited.sheet, u.edited.private.concat(preset.bugRichCheck || [])) : bugDraft(u.answer, u.screen, u.post);
        if (!bugMock.checks[u.id]) bugKeepCheck(u.id, draft);
        const digest = u.edited ? "Checked your changes for private details" : bugDigest(u.screen);
        emit("rich://bug-report", { checked: { id: u.id, report: u.id, draft, digest, here: u.screen.here || "", post: u.post || "bug" } });
      });
    },
    releaseSendCheck: () => bugRelease("sendcheck"),
    /// Every sheet that actually went out to the (stand-in) GitHub.
    filed: () => bugMock.filed.slice(),
    /// Every idea posted to the (stand-in) Discussions, under Ideas.
    ideasPosted: () => bugMock.filed.filter((f) => f.post === "idea"),
  };

  window.RichBridge = {
    isMock: true,

    async invoke(cmd, args, options) {
      args = args || {};
      switch (cmd) {
        case "claude_quota": return quotaView();
        case "approve_claude_reset": {
          window.__richosResetCalls.push({ cmd, offer: args.offer });
          if (preset.resetApprovalFails) throw "A reset check is in progress. Try again shortly.";
          if (resetOffers.state !== "fresh" || !resetOffers.offers.some(o => o.id === args.offer.id && o.clears.includes("seven_day"))) throw "Refresh offers first.";
          resetOffers.approval = { offer: structuredClone(args.offer), approvedAt: Date.now(), weeklyThreshold: 99 };
          return structuredClone(resetOffers);
        }
        case "revoke_claude_reset": {
          window.__richosResetCalls.push({ cmd }); resetOffers.approval = null;
          return structuredClone(resetOffers);
        }
        case "claude_quota_activity": {
          const activity = preset.quotaActivity || { held: [], released: [] };
          const scoped = rows => (rows || []).filter(r => !args.threadId || r.threadId === args.threadId);
          const admission = quotaView().admission;
          const held = scoped(activity.held);
          if (["disabled", "ready"].includes(admission.state) && held.length) {
            return { held: [], released: held.map(r => ({ ...r, releasedAt: Date.now() })), resumesAt: null };
          }
          // Round 16's working row: the account in use, named, with two or more accounts.
          const inUse = quotaView().accounts.find(a => a.inUse);
          return { ...activity, held, released: scoped(activity.released), ...(inUse ? { account: inUse.label } : {}), resumesAt: admission.state === "held" ? admission.resetsAt - (quotaView().windows.some(w => w.id === "seven_day" && w.usedPercent >= 99 && w.resetsAt === admission.resetsAt) ? 0 : 20 * 60000 - 1) : null };
        }
        case "set_claude_quota_policy": {
          if (!Number.isInteger(args.policy.pausePercent) || args.policy.pausePercent < 1 || args.policy.pausePercent > 99) throw new Error("Invalid pause threshold.");
          quotaPolicy = { ...args.policy }; localStorage.setItem("richos-mock-quota-policy", JSON.stringify(quotaPolicy)); return quotaView();
        }
        case "claude_account_add": {
          // Going from one account to two names both (round 16): `currentLabel` names Account 1.
          if (!mockAccounts.length) mockAccounts.push({ id: "1", label: (args.currentLabel || "").trim() || "Account 1", inUse: true, windows: structuredClone(quotaView().windows), checkedAt: Date.now(), exhaustedUntil: null, message: null });
          const id = String(Math.max(...mockAccounts.map(a => Number(a.id))) + 1);
          mockAccounts.push({ id, label: (args.label || "").trim() || `Account ${id}`, inUse: false, windows: [], checkedAt: null, exhaustedUntil: null, message: null });
          window.__accountCalls.push({ cmd, ...args }); mockSigning = id;
          return quotaView();
        }
        case "claude_account_remove": {
          if (args.id === "1") throw "Account 1 is your own Claude Code sign-in and cannot be removed here.";
          const gone = mockAccounts.find(a => a.id === args.id);
          if (gone && gone.windows.length) changed("removed", gone.id, gone.label);
          mockAccounts = mockAccounts.filter(a => a.id !== args.id);
          if (!mockAccounts.some(a => a.inUse) && mockAccounts.length) mockAccounts[0].inUse = true;
          return quotaView();
        }
        case "claude_account_discard": {
          // Cancel in round 18's add flow: the half-added account goes, with no Recent changes row.
          window.__accountCalls.push({ cmd, ...args }); mockSigning = null;
          mockAccounts = mockAccounts.filter(a => a.id !== args.id);
          if (mockAccounts.length === 1) { mockAccounts[0].inUse = true; }
          return quotaView();
        }
        case "claude_account_email": {
          // Round 31's Show email, as main.rs answers it from that account's own `claude auth
          // status`: `preset.emails[id]` is the address, `null` a signed-out folder; no entry is
          // an answer that names no address. No sample address is made up here.
          window.__accountCalls.push({ cmd, ...args });
          const said = (preset.emails || {})[args.id];
          if (said === null) return { state: "signedOut" };
          return typeof said === "string" ? { state: "email", email: said } : { state: "unknown" };
        }
        case "claude_account_nudge_answer": {
          window.__accountCalls.push({ cmd, ...args });
          mockNudge = { ...(mockNudge || { at: Date.now() }), answer: args.answer };
          return quotaView();
        }
        case "claude_account_use_first": {
          // Feedback item 8: he picks the account that drains first (quota.rs use_account_first).
          const pick = mockAccounts.find(a => a.id === args.id);
          if (!pick) throw "That account is no longer on this Mac.";
          if (!pick.windows.length) throw `${pick.label} has no reading yet. Sign it in, then refresh.`;
          if (!pick.inUse) { changed("chose", pick.id, pick.label); mockLastSwitch = null; }
          for (const a of mockAccounts) a.inUse = a.id === args.id;
          return quotaView();
        }
        case "claude_account_sign_in": {
          window.__accountCalls.push({ cmd, ...args }); mockSigning = args.id;
          return { state: "connecting", message: "Complete sign-in in your browser, then return here." };
        }
        case "claude_account_sign_in_poll": {
          // As main.rs answers it: (id, view, the name of the account it already is).
          if (!mockSigning || !mockSignIn) { mockSigning = null; return null; }
          const id = mockSigning; mockSigning = null;
          const added = mockAccounts.find(a => a.id === id);
          if ((mockSignIn.shift() || "connected") === "same-account") {
            return [id, { state: "same-account", message: "That is the Claude account this Mac already uses. Sign in with your other one." }, mockAccounts[0].label];
          }
          if (added) {
            const at = Date.now();
            added.windows = [{ id: "five_hour", label: "Five-hour", usedPercent: 0, resetsAt: at + 3 * 3600000, durationMs: 5 * 3600000 },
              { id: "seven_day", label: "Weekly", usedPercent: 3, resetsAt: at + 4 * 86400000, durationMs: 7 * 86400000 }];
            added.checkedAt = at;
            changed("added", added.id, added.label);
          }
          return [id, { state: "connected", message: "Your Anthropic account is connected." }, null];
        }
        case "set_claude_at_threshold": { mockAtThreshold = args.value; return quotaView(); }
        // ---- screenshots and files on the Mac composer (CEO §86) -------------------------
        case "attach_pasted_file": {
          const h = (options && options.headers) || {};
          mockAttach.calls.push({ cmd, name: decodeURIComponent(h["x-richos-name"] || ""), type: h["x-richos-type"] || "", size: args.length });
          return mockStage(h["x-richos-draft"], h["x-richos-attachment"], decodeURIComponent(h["x-richos-name"] || ""), h["x-richos-type"], args);
        }
        case "attach_dropped_file": {
          const drop = mockAttach.drops.get(args.drop);
          const file = drop && drop[args.index];
          mockAttach.calls.push({ cmd, drop: args.drop, index: args.index, name: file ? file.name : null });
          if (!file) return Promise.reject("That drop is no longer available. Drop the file again.");
          drop[args.index] = null;
          if (file.folder) return Promise.reject("This is a folder, not a file. Drop the files inside it instead. Nothing was attached.");
          return mockStage(args.draftId, args.attachmentId, file.name, "", file.bytes);
        }
        // Add to chat from the Output panel (output side-panel PRD §12.7): `output_attach` in
        // `mac_attachments.rs`. The id resolves in the ACTIVE thread's record only (§5.1); a file
        // no longer there is refused with `output_files.rs`'s MISSING; the size is refused before
        // anything is read; then it is the drop's road (`mockStage`), on bytes of the entry's size
        // that open with the kind's own magic, so the desk's sniffing passes as it does on disk.
        case "output_attach": {
          mockAttach.calls.push({ cmd, draftId: args.draftId, attachmentId: args.attachmentId, outputId: args.outputId });
          const recorded = (outputByThread[activeThreadId] || []).find((e) => e.id === args.outputId);
          if (!recorded) return Promise.reject("I don't have that file in this thread's output.");
          if (!recorded.exists) {
            return Promise.reject("This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here.");
          }
          const kind = mockKindFor("", recorded.name);
          if (!kind) return Promise.reject(MOCK_ATTACH_SAYS.unknownType);
          if (recorded.bytes > 25 * 1024 * 1024) return Promise.reject(MOCK_ATTACH_SAYS.tooLarge);
          const body = new Uint8Array(Math.max(0, recorded.bytes || 0));
          (kind.magic || []).forEach((b, i) => { if (i < body.length) body[i] = b; });
          return mockStage(args.draftId, args.attachmentId, recorded.name, "", body);
        }
        case "discard_attachment": {
          mockAttach.calls.push({ cmd, draftId: args.draftId, attachmentId: args.attachmentId });
          return mockAttach.staged.delete(args.draftId + "/" + args.attachmentId);
        }
        case "commit_attachments": {
          mockAttach.calls.push({ cmd, threadId: args.threadId, draftId: args.draftId, text: args.text, attachments: args.attachments });
          const missing = (mockAttach.missingNext || []).filter((id) => args.attachments.some((a) => a.id === id));
          mockAttach.missingNext = null;
          const found = args.attachments.map((a) => mockAttach.staged.get(args.draftId + "/" + a.id));
          args.attachments.forEach((a, i) => { if (!found[i] && !missing.includes(a.id)) missing.push(a.id); });
          if (missing.length) return { text: null, files: [], missing };
          found.forEach((f) => mockAttach.staged.delete(args.draftId + "/" + f.id));
          const count = found.length === 1 ? "1 file" : found.length + " files";
          const lines = found.map((f) => `- ${MOCK_ATTACH_HOME}/${args.threadId}/${args.draftId}/${f.name} (${f.mediaType}, ${f.size} bytes)`);
          const words = String(args.text || "").trim();
          return {
            text: (words ? words + "\n\n" : "") + `Attached on this Mac (${count}, saved by RichOS):\n` + lines.join("\n"),
            files: found.map(({ draftId: _d, ...f }) => f),
            missing: [],
          };
        }
        case "list_threads":
          return threads.map((t) => ({ ...t, message_count: (messagesByThread[t.id] || []).length }));
        case "active_thread":
          return activeThreadId;
        case "navigation_tree": {
          // Grouping happens HERE, not in the renderer — same division of labour as the
          // real `navigation_tree` command, so main.js exercises the identical shape.
          const groups = entities.map((entity) => ({
            entity,
            threads: threads
              .filter((t) => t.entity_id === entity.id)
              .map(threadRowOf)
              .sort((a, b) => b.last_activity - a.last_activity),
          }));
          return {
            groups,
            unbound: threads.filter((t) => !t.entity_id).map(threadRowOf),
            active: activeContextOf(),
            unbound_explanation:
              "This thread has no entity home: it predates entity scoping and Rich will not guess " +
              "which entity this work belongs to. An operator must bind it explicitly.",
          };
        }
        case "get_timeline": {
          const id = args.threadId ?? args.thread_id;
          const t = threads.find((x) => x.id === id);
          // Fails closed on an unbound thread exactly like the real command.
          if (t && !t.entity_id) return Promise.reject(UNBOUND_ERR(id));
          // The files Rich attached ride on this snapshot because `question_host::payload_in`
          // adds them, one item each, in the turn that attached them.
          return withRichFiles(projectTimeline(id), id);
        }
        // ---- techy mode (techy-mode design §3.1/§3.4) ------------------------------
        case "get_machinery": {
          const id = args.threadId ?? args.thread_id;
          const t = threads.find((x) => x.id === id);
          // Fails closed on an unbound thread exactly like `get_timeline` and the real one.
          if (t && !t.entity_id) return Promise.reject(UNBOUND_ERR(id));
          const st = machineryStateOf(id);
          // The SAME projection the payload carries, so the sentence is computed from what
          // will actually be on screen rather than from a second read. `machinery_view.rs`
          // does exactly this — it asks `view.between_turns()`, the gated lane — and the
          // difference matters: an unreadable store projects NO lane rows, so a sentence
          // derived from the raw fixture instead of from the projection would be answering
          // about rows the CEO cannot see.
          // ...and on Techy Mode's because `rich_files_host::machinery_with_files` adds the same rows
          // (review rv-20261021T083350Z-8ca9bacf-e592: until it did, the cards vanished here).
          const projected = withRichFiles(projectTimeline(id, st.state === "recorded" ? "technical" : "technical-empty"), id);
          return {
            threadId: id,
            state: st.state,
            rowCount: st.rowCount || 0,
            sentence: st.sentence,
            reason: st.reason,
            // §1.5: WHY the lane is empty when it is empty — and `null` when the store was
            // UNREADABLE, because "nothing was recorded between turns" is a claim a store
            // that refused to open never supported. `machinery_view.rs` decides it exactly
            // this way.
            betweenTurnsSentence:
              st.state === "unreadable"
                ? null
                : projected.betweenTurns.length === 0
                  ? TECHY_BETWEEN_TURNS_QUIET
                  : null,
            // The conversation still renders in EVERY state — what changes is the sentence
            // over it. An unreadable store is not an empty thread.
            //
            // AND THE MACHINERY ROWS DO NOT. `Spine::timeline` reads the journal through
            // `read_thread`, which returns nothing for a directory the OS refused, so the
            // real backend serves prose and a duration row and NO activity rows in this
            // state. A mock that kept showing them would let the renderer be tested against
            // a screen the product cannot produce.
            timeline: projected,
          };
        }
        case "get_machinery_raw": {
          const threadId = args.threadId ?? args.thread_id;
          const machId = args.machineryId ?? args.machinery_id;
          if (machineryUnreadable.has(threadId)) {
            return { state: "unreadable", payload: null, truncated: false, note: TECHY_RAW_UNREADABLE, reason: "machinery/" + threadId + ": Permission denied (os error 13)" };
          }
          const hit = machineryRaw.get(machId);
          // ABSENT = the Tier-B window passed over it. The record still rendered above; this
          // says why the bytes are gone rather than showing a blank (§2.4's honest degrade).
          if (!hit) return { state: "not_retained", payload: null, truncated: false, note: TECHY_RAW_NOT_RETAINED };
          return {
            state: "retained",
            payload: hit.payload,
            truncated: hit.truncated,
            note: hit.truncated ? TECHY_RAW_TRUNCATED : null,
          };
        }
        case "techy_mode":
          return techyModeOf(args.threadId ?? args.thread_id ?? "");
        case "set_techy_mode": {
          const id = args.threadId ?? args.thread_id;
          // `enabled: null` CLEARS the override and hands the thread back to the global
          // default. That arm is what keeps §7.1 reversible; without it a pin is one-way.
          if (args.enabled === null || args.enabled === undefined) techyThreads.delete(id);
          else techyThreads.set(id, !!args.enabled);
          return techyModeOf(id);
        }
        case "set_techy_default":
          techyDefault = !!args.enabled;
          return techyDefault;
        /// §7.1's three-way scope choice, applied the way `ConfigStore::apply_techy_scope`
        /// applies it — INCLUDING the part that is a decision rather than a transcription:
        /// a scope also clears the pins below it ON THE PATH TO THIS THREAD and on no
        /// other path. A mock that only set the chosen tier would let the renderer pass a
        /// test the product fails, which is the one thing a mock must never do.
        case "set_techy_scope": {
          const id = args.threadId ?? args.thread_id;
          const company = techyCompanyOf(id);
          const on = !!args.enabled;
          if (args.scope === "all-companies") {
            techyDefault = on;
            if (company) techyEntities.delete(company);
            techyThreads.delete(id);
          } else if (args.scope === "company") {
            // Refused rather than promoted a tier, exactly as the store refuses it.
            if (!company) throw new Error("this conversation is in no company, so there is no company to set");
            techyEntities.set(company, on);
            techyThreads.delete(id);
          } else if (args.scope === "thread") {
            techyThreads.set(id, on);
          } else {
            throw new Error("unknown techy scope: " + args.scope);
          }
          return techyModeOf(id);
        }

        // ---- §15: appearance, and the person at the foot of the rail ----------------
        // The real store is `config.rs`; this harness stands in for it with the same
        // shapes and the same honesty about the unset case. `user_name` starts as `null`
        // ON PURPOSE — the unset state is what almost every install actually has, and it
        // is the state the acceptance suite has to be able to reach without arranging
        // anything.
        case "get_appearance":
          return { theme: mockConfig.theme, font_scale: mockConfig.font_scale, oxford_comma: mockConfig.oxford_comma };
        case "set_oxford_comma":
          mockConfig.oxford_comma = args.keep === true;
          persistMockConfig();
          return null;
        case "set_theme": {
          const t = String(args.theme);
          // Refused, not coerced, exactly as the real command refuses it: quietly writing
          // "dark" over an unexpected string looks like the CEO changing his own mind.
          if (t !== "dark" && t !== "light" && t !== "system") {
            throw new Error(`unknown theme "${t}"`);
          }
          mockConfig.theme = t;
          persistMockConfig();
          return null;
        }
        case "set_font_scale": {
          const steps = [80, 90, 100, 110, 120, 135, 150];
          const want = Number(args.scale);
          // Snapped, not rejected — `snap_font_scale` in config.rs, mirrored.
          mockConfig.font_scale = steps.reduce((a, b) => (Math.abs(b - want) < Math.abs(a - want) ? b : a), 100);
          persistMockConfig();
          return null;
        }
        case "get_user_identity":
          return { name: mockConfig.user_name, initials: mockInitials(mockConfig.user_name) };
        case "set_user_name": {
          const n = String(args.name || "").trim();
          mockConfig.user_name = n === "" ? null : n;
          persistMockConfig();
          return null;
        }

        // ---- §119: Rich's model, both rows at once (main.rs's get/set_lease_models) ----
        case "get_lease_models":
          return { rich: mockConfig.rich_model, stu: mockConfig.stu_model };
        case "set_lease_models": {
          const known = (m) => m === "opus" || m === "fable";
          // An unknown name refuses both and writes neither, as the real command does.
          if (!known(args.rich) || !known(args.stu)) {
            return Promise.reject(`unknown model: ${known(args.rich) ? args.stu : args.rich}`);
          }
          mockConfig.rich_model = args.rich;
          mockConfig.stu_model = args.stu;
          persistMockConfig();
          return null;
        }

        // ---- §7.2: the raw-retention window ----------------------------------------
        case "raw_retention":
          return retentionView(0);
        case "set_raw_retention": {
          const next = String(args.choice);
          // An unknown choice — `custom` included, which describes a hand-edited file and
          // never instructs one — is REFUSED, exactly as the real command refuses it.
          // Nothing set, nothing evicted: a bad argument to a command that deletes must do
          // nothing at all.
          if (!Object.prototype.hasOwnProperty.call(RETENTION_WINDOWS, next)) {
            return Promise.reject("unknown retention choice: " + next);
          }
          retentionChoice = next;
          const w = RETENTION_WINDOWS[next];
          const before = rawShards.length;
          if (w.ageDays !== "forever") rawShards = rawShards.filter((sh) => sh.ageDays <= w.ageDays);
          return retentionView(before - rawShards.length);
        }

        // WHERE HIS MEMORY IS (`memory.rs`). The preview's default is `ready`, because a
        // machine that has been set up once is what every other fixture assumes; `memory:
        // "none"` drives the fresh install, which is the state the real defect lived in.
        case "memory_status":
          return memoryStatusOf();
        // HIS OWN LORO, DRAWN OR NOT (`main.rs::home_field_data`). `available: false` by
        // default — the demonstration is the first-run experience and the preview must land
        // on it without being told to.
        case "home_field_data":
          return homeFieldAnswer();
        case "home_team":
          return homeTeam.map((t) => ({ ...t }));
        // WHAT DID NOT LOAD OUT OF THE LEDGER (`Ledger::history_health`). The default is a
        // CLEAN load, because that is what every real launch produces — the notice is
        // supposed to be invisible almost always, and a preview that showed it constantly
        // would be rehearsing a state the product does not have. `__RICHOS_MOCK__.historySet`
        // drives the states that are not clean.
        case "history_health":
          return { ...mockHistoryHealth };
        // WHAT THIS MACHINE IS MISSING (`setup.rs`, `setup_view.rs`). The preview's default
        // is a machine that has everything, because every other fixture assumes one;
        // `setup: "missing-both"` is the customer's Mac, which is the state §19 says every
        // machine but the CEO's is actually in.
        case "provider_auth_status":
          return providerView();
        case "provider_auth_poll":
          if (providerState === "connecting" && !preset.providerAuthHold) providerState = "connected";
          return providerView();
        case "provider_auth_start":
          providerState = "connecting";
          return providerView();
        case "provider_auth_cancel":
          providerState = "cancelled";
          return providerView();
        case "setup_status":
          return setupStatusOf();
        // HE PRESSES THE BUTTON. It installs nothing here — the point of the mock is the
        // SURFACE — but it emits the same progress events the real command emits, on the
        // same channel, and it can be made to fail, because the failure states are the ones
        // a surface gets wrong.
        case "run_setup":
          return runSetupMock();
        // RICH'S ONE OFFER OF DICTATION (`dictation_offer.rs`): whether dictation is there,
        // whether the offer was made, and the key; and the record that it was shown.
        case "dictation_offer":
          dictationOfferCalls.push("dictation_offer");
          return { ready: setupDictationOn, offered: dictationOffered, key: preset.dictationKey || 1 };
        // THE DICTATION SHEET (`dictation_app.rs`): the same facts and the same refusals.
        case "dictation_status":
          dictationCalls.push({ cmd });
          return structuredClone(dictationView);
        case "dictation_set_on":
          dictationCalls.push({ cmd, on: !!args.on });
          dictationView.on = !!args.on;
          if (args.on && dictationView.owner === "none") dictationView.owner = preset.dictationOwner || "self";
          dictationView.keyTap = !preset.dictationTapNever && dictationView.on && dictationView.owner === "self" && dictationView.ax === "allowed";
          return structuredClone(dictationView);
        case "dictation_set_key":
          dictationCalls.push({ cmd, key: args.key });
          if (!(Number.isInteger(args.key) && args.key >= 1 && args.key <= 19)) throw `F${args.key} is not a key RichScribe can use`;
          dictationView.key = args.key;
          return structuredClone(dictationView);
        case "dictation_set_accuracy":
          dictationCalls.push({ cmd, accuracy: args.accuracy });
          if (args.accuracy !== "accurate" && args.accuracy !== "fast") throw `${args.accuracy} is not an accuracy RichScribe offers`;
          dictationView.accuracy = args.accuracy;
          return structuredClone(dictationView);
        case "dictation_ask_microphone":
          dictationCalls.push({ cmd });
          if (dictationView.mic === "unknown") dictationView.mic = "asking";
          return structuredClone(dictationView);
        case "dictation_ask_accessibility":
          dictationCalls.push({ cmd });
          if (dictationView.ax === "allowed" || dictationAxAsked) return false;
          // Asked once: from here macOS's "not trusted" reads as denied (`ax_word`).
          dictationAxAsked = true;
          dictationView.ax = "denied";
          return true;
        case "dictation_permissions_changed":
          dictationCalls.push({ cmd, forward: args.forward });
          // A tool whose key tap never comes (the case recheck finding 6 is about) stays without one.
          dictationView.keyTap = !preset.dictationTapNever && dictationView.on && dictationView.owner === "self" && dictationView.ax === "allowed";
          return null;
        case "dictation_open_settings":
          dictationCalls.push({ cmd, pane: args.pane });
          if (args.pane !== "microphone" && args.pane !== "accessibility") throw `${args.pane} is not a pane RichScribe opens`;
          return null;
        case "dictation_take_sheet_request": {
          // A sheet request kept until a page takes it (main.rs `dictation_take_sheet_request`): true once.
          dictationCalls.push({ cmd });
          const pending = !!preset.dictationSheetPending;
          preset.dictationSheetPending = false;
          return pending;
        }
        case "dictation_capture_key":
          dictationCalls.push({ cmd, on: !!args.on });
          return !!args.on && dictationView.keyTap;
        case "dictation_offer_shown":
          dictationOfferCalls.push("dictation_offer_shown");
          dictationOffered = true;
          return null;
        // Dictation was just set up (`main.rs` dictation_feature_thread): the conversation, under
        // the company he is in, with Rich's message naming the key in the settings now. Once.
        case "dictation_feature_thread": {
          dictationCalls.push({ cmd, create: !!args.create });
          const key = dictationView.key, at = now(), text = dictationFeatureText(key), turnId = uid("turn");
          if (dictationFeatureThread) {
            // A key change replaces the message (`dictation_offer::feature_action`, Replace): the
            // old turn is superseded, so the conversation shows the new one only.
            if (dictationFeatureThread.key === key) return null;
            const old = turnsById.get(dictationFeatureThread.turn);
            const thread = threads.find((x) => x.id === dictationFeatureThread.thread);
            turnsById.set(turnId, { threadId: thread.id, entityId: thread.entity_id, userText: null, proactive: true,
              runs: [{ text, startSeq: 0, at }], state: "completed", createdAt: at, startedAt: at, endedAt: at, activities: [] });
            if (old) old.supersededBy = turnId;
            messagesByThread[thread.id] = [{ role: "assistant", text, turn_id: turnId, at }];
            dictationFeatureThread = { thread: thread.id, turn: turnId, key };
            return thread.id;
          }
          if (!args.create) return null;
          const here = threads.find((x) => x.id === activeThreadId);
          if (!here || !here.entity_id) return Promise.reject("No company is chosen yet, so I won't guess which one this belongs to.");
          const id = uid("thread");
          threads.push({ id, title: "RichScribe feature", entity_id: here.entity_id, created_at: at, message_count: 1,
            last_activity: at, last_turn_state: "completed", has_pending_turn: false });
          messagesByThread[id] = [{ role: "assistant", text, turn_id: turnId, at }];
          turnsById.set(turnId, { threadId: id, entityId: here.entity_id, userText: null, proactive: true,
            runs: [{ text, startSeq: 0, at }], state: "completed", createdAt: at, startedAt: at, endedAt: at, activities: [] });
          dictationFeatureThread = { thread: id, turn: turnId, key };
          return id;
        }
        // API-EQUIVALENT TOKEN SPEND (spend.js): the ledger's snapshot and its live line.
        case "spend_view": return spendMock.view(args.from);
        case "spend_live": return spendMock.live();
        // THE CODEX REVIEW SWITCH (codex_reviews.rs): the same facts and the same refusal: on is
        // refused while Codex is not ready, off always goes through.
        case "codex_reviews_status":
          codexReviewCalls.push({ cmd });
          return structuredClone(codexReviews);
        case "codex_reviews_set":
          codexReviewCalls.push({ cmd, on: !!args.on });
          if (!(args.on && !codexReviews.on && codexReviews.codex !== "ready")) codexReviews.on = !!args.on;
          return structuredClone(codexReviews);
        // The answer. It provisions nothing here — the point of the mock is the SURFACE —
        // but it returns the shape the real command returns, including the honest
        // `no-compiler` outcome, which is what a machine with no compiler actually gets.
        case "provision_memory": {
          const given = args && args.location;
          if (!given)
            return Promise.reject(
              "no location was given for the corpus. There is no default: a corpus root " +
                "nobody named would compile the wrong memory, or none, and report success " +
                "either way."
            );
          memoryState = memoryCompilerPresent ? "ready" : "no-compiler";
          // THE ANSWER THE SHIPPED BACKEND ACTUALLY GIVES, which is not the location it was
          // handed. `provision` creates the corpus at `given` and writes a symlink beside it
          // at `~/Library/Application Support/RichOS/corpus`; `provision_memory` then
          // re-resolves through `resolve_corpus`, whose candidate 3 IS that symlink, so the
          // status it returns names the POINTER. Measured on the installed v1.0.0 by
          // ray-opus-a1 on 2026-09-04:
          //
          //   ~/Library/Application Support/RichOS/corpus -> /Users/alex/RichOS/corpus
          //
          // One directory, two names. Until this line the mock echoed `given` straight back,
          // so the surface was rehearsed against an agreement the real backend does not
          // supply — and the one screen where the app asks him to trust it with his record
          // could name a folder he never agreed to with every suite green.
          memoryRoot = "/Users/you/Library/Application Support/RichOS/corpus";
          // AND THE DESK OPENS, in the same call, because the real one now does
          // (`main.rs::install_correction_desk`, called from `provision_memory`). Until
          // 2026-09-01 the field it writes to was fixed at boot, so a fresh user got a
          // readable memory and a shut desk until he relaunched. A mock that kept the old
          // shape would rehearse the SURFACE against behavior the backend no longer has,
          // which is worse than having no mock for it.
          //
          // It follows the compiler, not the corpus: a `no-compiler` outcome has no
          // `loro-write.mjs` either, so there is nothing for a desk to write with.
          if (memoryCompilerPresent) loroDeskOn = true;
          return { ...memoryStatusOf(), provisioned_now: true };
        }
        // --- the first-run notice (2026-09-06) ---
        //
        // WHERE THIS INSTALL STANDS ON ONBOARDING (`main.rs::onboarding_view`). The default
        // is `described`, so this preview lands on a machine that has been set up — see the
        // note over `onboardingViewOf`.
        case "onboarding_view":
          onboardingCalls.push({ cmd: "onboarding_view" });
          return onboardingViewOf();
        // HE PRESSED "Not now". It writes no file here — the point of the mock is the
        // SURFACE — but it moves the state the same way the real command moves it, and it
        // can be made to REFUSE, because a press whose write went nowhere is the one
        // outcome the surface must never render as success.
        case "decline_onboarding":
          onboardingCalls.push({ cmd: "decline_onboarding", entityId: args.entityId });
          if (!args.entityId || args.entityId !== onboardingViewOf().entityId) return Promise.reject("The selected company changed. Open its conversation and choose Not now again.");
          if (onboardingDeclineHeld) await new Promise((r) => onboardingDeclineHeld.push(r));
          if (onboardingDeclineFails) return Promise.reject(ONBOARDING_DECLINE_REFUSED);
          onboardingByEntity.set(args.entityId, "declined");
          return onboardingViewOf();
        // --- THE PHONE CHANNEL (`phone.js`, plan §4.1) ---
        //
        // Three states, because the sheet has three and each one has to be reachable in a browser
        // with no Tauri behind it: off, a pairing window open, and a phone paired. The preset
        // switch is what lets the contrast walk and the acceptance tests each open the one they
        // need. The default is OFF, because that is the state a fresh install actually ships in.
        case "phone_status":
          return phoneStatusOf();
        // **IT TAKES NO ARGUMENT** (CEO §61). It briefly took the user's chosen route —
        // Urban's N1 — and the route went with the chooser. The Mac plans the one path, and a
        // Mac it cannot plan for refuses rather than planning a different one.
        case "phone_connect_enable":
          window.__RICHOS_CONNECT_ACTIONS__ = (window.__RICHOS_CONNECT_ACTIONS__ || []).concat("enable");
          mockConnect = { enabled:true,phase:"active",endpoint:"https://c-00000000000000000000000000000000-g1.richos.ceo",health:{state:"connected",restarts:0} };
          mockPhone.pairing = true;
          mockPhone.openedAt = now();
          return phoneStatusOf();
        case "phone_connect_disable":
          window.__RICHOS_CONNECT_ACTIONS__ = (window.__RICHOS_CONNECT_ACTIONS__ || []).concat("disable");
          mockConnect = { enabled:false,phase:"disabled",health:null };
          mockPhone.paired = false; mockPhone.pairing = false;
          return phoneStatusOf();
        case "phone_begin_pairing":
          mockPhone.pairing = true;
          mockPhone.openedAt = now();
          // **AND IT IS WHAT LEAVES THE SCREEN THAT SAYS THE MAC STOPPED** — the Mac clears
          // `rejected` here and nowhere else (`PhoneRuntime::begin_pairing`), including when the
          // start itself fails, because he has read the sentence and moved on.
          mockPhone.rejected = false;
          return phoneStatusOf();
        // **"STOP AND GO BACK" ON THE PAIRING SCREEN** — Urban's G2, and it stops serving.
        // The Mac's own rule, `PhoneRuntime::stop_pairing`: close the window, then take the
        // socket down only if nothing is paired. `listening` is derived from `paired ||
        // pairing` here exactly as the Mac derives it from `listener_should_run`, so clearing
        // `pairing` is the harness's whole half of it — and a paired phone keeps `listening`
        // true through the same expression rather than through a second rule that agrees.
        case "phone_stop_pairing":
          mockPhone.pairing = false;
          return phoneStatusOf();
        // **THE TWO ANSWERS ON THE MAC** — Sage F1. `PhoneRuntime::confirm_on_mac` refuses with
        // nothing paired, and so does this; `reject_on_mac` is the phone's own teardown with the
        // Mac named as the one who stopped it.
        case "phone_confirm_on_mac":
          if (!mockPhone.paired) return Promise.reject("No phone is paired with this Mac yet.");
          mockPhone.macConfirmed = true;
          // What the real Mac reports right after its press: the phone has not used it yet.
          mockPhone.unfinished = true;
          return phoneStatusOf();
        case "phone_reject_on_mac":
          mockPhone.paired = false;
          mockPhone.pairing = false;
          mockPhone.rejected = true;
          mockPhone.rejectedBy = "mac";
          return phoneStatusOf();
        // THE HOW-TO SCREENS' LINKS (§61.1). Recorded rather than only answered: what a test needs
        // to know is WHICH address the control asked for, and `opener.rs` is where the allowlist
        // that governs it is proved.
        case "open_external":
          window.__RICHOS_OPENED__ = window.__RICHOS_OPENED__ || [];
          window.__RICHOS_OPENED__.push(args?.target ?? null);
          return null;
        case "phone_forget":
          mockPhone.paired = false;
          mockPhone.pairing = false;
          return null;
        // The shell's system folder chooser. The mock records every ask (with the asking
        // field's title) and answers with the suite's `pickedFolder`, or `null`, which is
        // what the shell returns when he closes the chooser without a folder.
        case "pick_folder":
          window.__RICHOS_FOLDER_ASKS__ = window.__RICHOS_FOLDER_ASKS__ || [];
          window.__RICHOS_FOLDER_ASKS__.push(args?.title ?? null);
          return window.__RICHOS_MOCK_PRESET__?.pickedFolder ?? null;
        case "repository_connections":
          return {companies: entities.map(e => ({id:e.id, name:e.display_name, repositories:e.connected_repositories || []}))};
        case "connect_repository": {
          const company = entities.find(e => e.id === args.entityId);
          if (!company) return Promise.reject("Choose a registered company.");
          if (window.__RICHOS_MOCK_PRESET__?.repositoryRefusal) return Promise.reject("That folder overlaps another company's folder.");
          if (!args.folder || !args.folder.startsWith("/")) return Promise.reject("Choose an absolute folder path.");
          if (!company.connected_repositories) company.connected_repositories = [];
          if (!company.connected_repositories.includes(args.folder)) company.connected_repositories.push(args.folder);
          // `repositories::connect` gives every folder Git (CEO, 2026-10-06): `initialized` is
          // false only for a folder that already had it, which a suite names in `gitFolders`.
          const hadGit = (window.__RICHOS_MOCK_PRESET__?.gitFolders || []).includes(args.folder);
          return {entity_id:company.id, repository:{root:args.folder, branch:"main", initialized:!hadGit}};
        }
        case "entity_choice":
          return entityChoiceOf();
        // --- the home screen's company buttons (2026-09-01) ---
        case "home_entity_row":
          return homeEntityRowOf();
        case "set_home_entity_label": {
          const id = args.entityId ?? args.entity_id;
          if (!entities.some((e) => e.id === id))
            return Promise.reject(
              'I don\'t have a company called "' + id + '" on file, so I won\'t file ' +
                "anything under it. Pick one of the companies I do have or whoever set " +
                "RichOS up can add it."
            );
          const label = args.label == null ? "" : String(args.label).trim();
          // Empty CLEARS, exactly as `ConfigStore::set_home_entity_label` does — that is how
          // he un-anonymizes the screen, so the mock does it too rather than simplifying it
          // away into something the app does not actually do.
          if (label) homeEntityLabels[id] = label;
          else delete homeEntityLabels[id];
          return homeEntityRowOf();
        }
        case "set_home_entity_visible": {
          const id = args.entityId ?? args.entity_id;
          if (!entities.some((e) => e.id === id))
            return Promise.reject(
              'I don\'t have a company called "' + id + '" on file, so I won\'t file ' +
                "anything under it. Pick one of the companies I do have or whoever set " +
                "RichOS up can add it."
            );
          if (args.visible === false) homeEntityHidden[id] = true;
          else delete homeEntityHidden[id];
          return homeEntityRowOf();
        }
        // HE ADDS ONE OF HIS OWN COMPANIES (2026-09-04). The refusals below are the
        // sentences `register_entity` actually returns, not paraphrases — `affordances.js`
        // scrapes the Rust and would fail on a harness that invented its own wording.
        //
        // The folder is NOT checked against a real filesystem here, and that is the one
        // deliberate divergence: a browser cannot stat a path. The app refuses a folder that
        // is not absolute or is not there; this harness refuses only the first, and says so
        // rather than implying it checks both.
        case "register_entity": {
          const displayName = String(args.displayName ?? args.display_name ?? "").trim();
          const folder = args.folder == null ? "" : String(args.folder).trim();
          if (!displayName)
            return Promise.reject(
              "I need a name for the company before I can file anything under it. Anything " +
                "you'd recognize on a button is fine. You can change it later."
            );
          const id = mockEntityIdFromName(displayName);
          if (!id)
            return Promise.reject(
              "That name doesn't have any letters or numbers in it, so I can't make a " +
                "file-safe label out of it. Try a name with a word in it."
            );
          if (folder && !folder.startsWith("/") && !folder.startsWith("~"))
            return Promise.reject(
              'I couldn\'t use "' + folder + '" as this company\'s folder: it isn\'t a full ' +
                "path from the top of the disk. Give me a folder that's already on this Mac, " +
                "or leave it blank: a company works without one."
            );
          entities.push({ id, display_name: displayName, status: "active", roots: folder ? [folder] : [], connected_repositories: folder ? [folder] : [] });
          // The real command makes the first company the one in force and opens a thread in
          // it — only when nothing has been chosen and the environment has not decided.
          if (!chosenEntityId && !entityPinnedByEnvironment) {
            chosenEntityId = id;
            chosenEntitySource = "saved-choice";
            activateCompanyIfNeeded(id);
          }
          return entityChoiceOf();
        }
        case "choose_entity": {
          const entityId = args.entityId ?? args.entity_id;
          if (entityPinnedByEnvironment)
            return Promise.reject(
              "This copy of me was told which company it works for when it was started up, " +
                "from outside this window, so I can't move it from in here. Whoever set " +
                "RichOS up is the one who changes that."
            );
          if (!entities.some((e) => e.id === entityId))
            return Promise.reject(
              'I don\'t have a company called "' + entityId + '" on file, so I won\'t file ' +
                "anything under it. Pick one of the companies I do have or whoever set " +
                "RichOS up can add it."
            );
          chosenEntityId = entityId;
          chosenEntitySource = "saved-choice";
          // The real command activates a thread ONLY when nothing is open — it never
          // re-homes an existing conversation (ECS §3.2). Same rule here.
          activateCompanyIfNeeded(entityId);
          return entityChoiceOf();
        }
        case "active_context":
          return activeContextOf();
        case "thread_scope": {
          const id = args.threadId ?? args.thread_id;
          const t = threads.find((x) => x.id === id);
          if (!t) return Promise.reject("unknown thread: " + id);
          if (!t.entity_id) return Promise.reject(UNBOUND_ERR(id));
          return { thread_id: id, entity_id: t.entity_id, binding_revision: 1 };
        }
        case "create_thread_in": {
          const entityId = args.entityId ?? args.entity_id;
          if (!entities.some((e) => e.id === entityId)) return Promise.reject("unknown entity: " + entityId);
          const id = uid("thread");
          threads.push({
            id,
            title: (args.title || "New thread").trim() || "New thread",
            entity_id: entityId,
            created_at: now(),
            message_count: 0,
            last_activity: now(),
            last_turn_state: null,
            has_pending_turn: false,
          });
          messagesByThread[id] = [];
          activeThreadId = id;
          return id;
        }
        case "search_nav": {
          const q = String(args.query || "").trim().toLowerCase();
          if (!q) return [];
          const labelOf = (eid) => (entities.find((e) => e.id === eid) || {}).display_name || "No entity";
          const hits = [];
          for (const e of entities) {
            if (e.display_name.toLowerCase().includes(q) || e.id.includes(q)) {
              hits.push({ kind: "entity", entity_id: e.id, entity_label: e.display_name, thread_id: null, thread_title: null, excerpt: e.display_name, at: 0 });
            }
          }
          for (const t of threads) {
            if (displayTitleOf(t).toLowerCase().includes(q) || t.title.toLowerCase().includes(q)) {
              hits.push({ kind: "thread", entity_id: t.entity_id, entity_label: labelOf(t.entity_id), thread_id: t.id, thread_title: displayTitleOf(t), excerpt: "", at: t.last_activity });
            }
          }
          for (const t of threads) {
            // An unbound thread's BODY is never searched — the scoped read refuses, so the
            // real command skips it, and so does this.
            if (!t.entity_id) continue;
            let perThread = 0;
            for (const m of (messagesByThread[t.id] || []).slice().reverse()) {
              if (perThread >= 3) break;
              const at = m.text.toLowerCase().indexOf(q);
              if (at < 0) continue;
              const start = Math.max(0, at - 60);
              const end = Math.min(m.text.length, at + q.length + 60);
              const excerpt = (start > 0 ? "…" : "") + m.text.slice(start, end) + (end < m.text.length ? "…" : "");
              // Same one-match-one-row rule the real `search_nav` applies.
              if (excerpt === displayTitleOf(t)) continue;
              hits.push({
                kind: "message",
                entity_id: t.entity_id,
                entity_label: labelOf(t.entity_id),
                thread_id: t.id,
                thread_title: displayTitleOf(t),
                excerpt,
                at: m.at,
              });
              perThread += 1;
            }
          }
          return hits.slice(0, args.limit || 40);
        }
        // UX §7.3's background-work summary. Shaped exactly like `WorkerStatusView`
        // (worker_status.rs): a real `active` count, a real `liveness_unknown`, and
        // `needs_you` STRUCTURALLY ZERO — no decision-required signal exists anywhere in
        // the engine, so there is no honest non-zero value to mock either.
        case "get_work_status": {
          const preset = window.__RICHOS_MOCK_PRESET__ || {};
          if (preset.workSummaryError) throw new Error(preset.workSummaryError);
          return preset.workSummaries?.[args.threadId] || {items: [], omitted: 0};
        }
        // Stu's saved work reports for this conversation.
        case "get_assignments": {
          const preset = window.__RICHOS_MOCK_PRESET__ || {};
          if (preset.assignmentsError) throw new Error(preset.assignmentsError);
          return {assignments: preset.assignments?.[args.threadId] || []};
        }
        // Durable notices are read at the thread open and at every turn boundary. Empty
        // here: this harness drives the surface, not the delivery lane.
        case "take_work_notices":
          return [];
        case "stop_assignment":
          return null;
        case "get_worker_status":
          return {
            active: 1,
            needs_you: 0,
            liveness_unknown: 1,
            items: [
              { label: "Frank", state: "active", agent_id: "agt_frank_1" },
              { label: "Sage", state: "unknown", agent_id: "agt_sage_1" },
              { label: "mark-sonnet-f1: wire the tenantGuard fixture", state: "done" },
            ],
          };
        // The output record, projected (output side-panel PRD §5.4). Refuses with the shell's
        // own sentence when the suite has made the record unreadable.
        case "list_output": {
          if (outputUnreadable) return Promise.reject(OUTPUT_RECORD_UNREADABLE);
          return outputListOf(args.threadId ?? args.thread_id);
        }
        // One file's preview (output side-panel PRD §5.4, §7, slice S5), by its output id.
        case "output_preview":
          return outputPreviewOf(args.outputId ?? args.output_id);
        // The actions (S6): the shell's contract, nothing opened and nothing written.
        case "output_file":
        case "output_open":
        case "output_reveal":
        case "output_save_copy":
          return outputAction(cmd, args);
        // A file Rich put in front of the user (round 36 screen 3): opened through the opener.
        case "rich_file_open":
        case "rich_file_reveal":
          return richFileAction(cmd, args);
        // The user's own instructions (round 36 screen 1): `main.rs` `your_instructions_read`,
        // `your_instructions_save` and `instructions_file_open`.
        case "your_instructions_read":
          return { path: "/Users/you/myrichos/instructions.md", shown: "~/myrichos/instructions.md",
            exists: yourInstructions.exists, text: yourInstructions.text };
        case "your_instructions_save":
          if (yourInstructions.failSave) return Promise.reject(yourInstructions.failSave);
          yourInstructions.text = String(args.text);
          yourInstructions.exists = true;
          yourInstructions.saves.push(String(args.text));
          return null;
        case "instructions_file_open":
          if (!["rich.md", "stu.md", "instructions.md"].includes(args.name)) return Promise.reject(args.name + " is not one of the instruction files");
          yourInstructions.opened.push({ name: args.name, reveal: !!args.reveal });
          return null;
        case "nav_state":
          return JSON.parse(JSON.stringify(navPrefs));
        case "set_sidebar_width":
          navPrefs.sidebar_width = Math.max(224, Math.min(420, Number(args.width) || 300));
          return navPrefs.sidebar_width;
        // Same clamp bounds as nav.rs's `clamp_inspector_width`, and it returns the width
        // the store ACCEPTED — so the harness cannot drift from the real contract.
        case "set_inspector_width":
          navPrefs.inspector_width = Math.max(280, Math.min(520, Number(args.width) || 336));
          return navPrefs.inspector_width;
        // nav.rs's `clamp_output_width`: the floor only, a non-number is the default.
        case "set_output_width": {
          const w = Number(args.width);
          navPrefs.output_width = Number.isFinite(w) ? Math.max(320, w) : 400;
          try {
            window.localStorage.setItem(MOCK_OUTPUT_WIDTH_KEY, String(navPrefs.output_width));
          } catch (e) {
            /* storage unavailable; the in-memory value still serves this session */
          }
          return navPrefs.output_width;
        }
        case "set_sidebar_collapsed":
          navPrefs.sidebar_collapsed = !!args.collapsed;
          persistMockSidebar();
          return null;
        case "set_entity_collapsed":
          setMembership(navPrefs.collapsed_entities, args.entityId ?? args.entity_id, !!args.collapsed);
          return null;
        case "set_thread_pinned":
          setMembership(navPrefs.pinned_threads, args.threadId ?? args.thread_id, !!args.pinned);
          return null;
        case "set_thread_archived":
          setMembership(navPrefs.archived_threads, args.threadId ?? args.thread_id, !!args.archived);
          return null;
        case "rename_thread": {
          const id = args.threadId ?? args.thread_id;
          const title = String(args.title || "").trim().slice(0, 200);
          if (title) navPrefs.renamed_threads[id] = title;
          else delete navPrefs.renamed_threads[id];
          return null;
        }
        case "create_thread": {
          const id = uid("thread");
          const title = (args.title || "New thread").trim() || "New thread";
          threads.push({ id, title, created_at: now(), message_count: 0, last_activity: now() });
          messagesByThread[id] = [];
          return id;
        }
        case "switch_thread": {
          const id = args.threadId ?? args.thread_id;
          const t = threads.find((x) => x.id === id);
          // An unbound thread cannot become the active context — activation re-reads the
          // entity from the durable record and there isn't one. Mirrors `Spine::activate`.
          if (t && !t.entity_id) return Promise.reject(UNBOUND_ERR(id));
          activeThreadId = id;
          return null;
        }
        case "get_messages":
          // A DEFENSIVE COPY — real Tauri IPC always deep-serializes a command's return
          // value, so main.js can never end up holding a live reference into the Rust
          // ledger's own storage. Returning the live array here (an earlier bug) let
          // main.js's `messages.push(...)` optimistic-append silently mutate the mock's
          // canonical store, permanently leaking the optimistic placeholder into history
          // — a class of bug that is IMPOSSIBLE against the real backend, so the mock must
          // not manufacture it either.
          {
            const id = args.threadId ?? args.thread_id;
            const t = threads.find((x) => x.id === id);
            // THE REJECTION THAT USED TO BREAK THE SHELL. `get_messages` became fallible
            // when threads gained an entity home: an unbound thread refuses rather than
            // returning an empty list. Reproduced here so the calm §21 binding-failure
            // state is exercised in the browser harness, not only against a real ledger.
            if (t && !t.entity_id) return Promise.reject(UNBOUND_ERR(id));
            return (messagesByThread[id] || []).map((m) => ({ ...m }));
          }
        case "send_message": {
          // `thread_id: String` in main.rs: Tauri refuses a send with no thread before the command
          // runs, and so does this, rather than quietly sending to the active thread (the swallowed
          // first send after a relaunch, esc-20261020T171021Z-8fd0fca7, was invisible here).
          if (typeof args.threadId !== "string") {
            return Promise.reject(
              "invalid args `threadId` for command `send_message`: invalid type: null, expected a string"
            );
          }
          // THE SETTING UP WAS NEVER DONE — main.rs's `send_message` asks the disk before it
          // reaches for `LEASE_UNAVAILABLE_MESSAGE`, because the two no-lease causes need
          // opposite things from the CEO. Missing pieces are the arm where a restart cannot
          // help; the mock takes the same branch in the same order.
          {
            const missing = setupNeeds();
            if (missing.length) {
              const blocked = !setupEnginePresent && !setupPinned;
              if (blocked) return Promise.reject(SETUP_UNPINNED);
              const key =
                missing.length === 2 ? "both" : missing[0] === "engine" ? "engine" : "claude";
              return Promise.reject(SETUP_INCOMPLETE[key]);
            }
          }
          if (window.__RICHOS_MOCK__ && window.__RICHOS_MOCK__._notConnected) {
            // Mirrors main.rs's no-lease path (`send_message`'s `!spine.has_lease()`):
            // rejects before any turn starts, so NO rich:// events ever fire for this
            // attempt.
            // VERBATIM from `LEASE_UNAVAILABLE_MESSAGE`, app/src-tauri/src/main.rs. The
            // affordance suite asserts the two are byte-identical, so the preview cannot
            // quietly rehearse a sentence the product no longer says.
            return Promise.reject(
              "RichOS couldn't reach Claude Code, which Rich runs on. Quit RichOS and open it again, then send your message again."
            );
          }
          // §26's scenario is started BY THE CEO PRESSING ENTER, not by a side door. The
          // shell's ordinary send path runs first — the optimistic bubble goes up with a
          // synthetic id and `onTurnStatus`'s `adoptPendingUserMessage` re-keys it onto
          // `turn_memory_01:user` when `queued` arrives — so "just after send" is the state
          // the CEO actually sees, not one the fixture painted.
          if (activeThreadId === MEMORY_THREAD_ID && args.text === MEMORY_PROMPT) {
            memoryScenario = memoryStrategy({ anchor: memoryStrategyAnchor });
            memoryScenario.runTo(2);
            return messagesByThread[MEMORY_THREAD_ID];
          }
          // A pressed button's words reach Rich and are never his row (`Turn::pressed`, finding 14).
          simulateTurn(activeThreadId, args.text, args.pressed ? { pressed: true } : undefined);
          return messagesByThread[activeThreadId];
        }

        // ---- the loro correction desk (`correction.rs`) -------------------------------
        case "loro_available":
          return loroDeskOn;
        case "loro_pending_corrections": {
          const gate = loroGate();
          if (gate) return Promise.reject(gate);
          return proposals.filter((p) => p.state === "awaiting-ceo");
        }
        case "loro_suppressed_records": {
          const gate = loroGate();
          if (gate) return Promise.reject(gate);
          return loroSuppressed.slice();
        }
        case "loro_show_record": {
          const gate = loroGate();
          if (gate) return Promise.reject(gate);
          const ref = args.recordRef ?? args.record_ref;
          if (ref !== "rec:ceo/records/decision-ship-thursday")
            return Promise.reject('loro write: no record with ref "' + ref + '"');
          return { op: "show", dryRun: false, ref, file: "loro/ceo/records/decision-ship-thursday.md", text: LORO_RECORD_NOW, changed: [] };
        }
        case "loro_propose_correction": {
          const gate = loroGate();
          if (gate) return Promise.reject(gate);
          const why = String(args.why || "").trim();
          // Refused before a process is started: a correction with no stated reason is the
          // shape an INFERRED one takes (`correction.rs:496-500`).
          if (!why) return Promise.reject("a correction needs the CEO's own words for what was wrong (--why)");
          const target = refOf(args.write);
          if (target && loroSuppressed.includes(target))
            return Promise.reject('"' + target + '" was permanently declined for correction — clear the suppression to propose again');
          const p = {
            id: "prop-" + proposalSeq++,
            at: now(),
            entity_id: "northwind",
            thread_id: args.threadId ?? args.thread_id ?? "",
            write: args.write,
            why,
            preview: LORO_PREVIEW,
            state: "awaiting-ceo",
            outcome: null,
            failure: null,
          };
          proposals.push(p);
          return p;
        }
        case "loro_confirm_correction": {
          const gate = loroGate();
          if (gate) return Promise.reject(gate);
          const p = proposals.find((x) => x.id === args.id);
          if (!p) return Promise.reject("no proposal " + args.id);
          // A double click must not write twice (`correction.rs:541-543`).
          if (p.state !== "awaiting-ceo")
            return Promise.reject("proposal " + p.id + " is " + p.state + ", not awaiting the CEO — it cannot be confirmed twice");
          if (window.__RICHOS_MOCK__ && window.__RICHOS_MOCK__._loroWriterRefusal) {
            p.state = "failed";
            p.failure = window.__RICHOS_MOCK__._loroWriterRefusal;
            return p;
          }
          p.state = "written";
          p.outcome = {
            op: "supersede",
            dryRun: false,
            ref: "rec:ceo/records/decision-ship-date",
            supersededRef: refOf(p.write),
            file: "loro/ceo/records/decision-ship-date.md",
            text: LORO_PREVIEW,
            changed: [],
          };
          return p;
        }
        case "loro_decline_correction": {
          const gate = loroGate();
          if (gate) return Promise.reject(gate);
          const p = proposals.find((x) => x.id === args.id);
          if (!p) return Promise.reject("no proposal " + args.id);
          if (p.state !== "awaiting-ceo")
            return Promise.reject("proposal " + p.id + " is " + p.state + ", not awaiting the CEO — it cannot be confirmed twice");
          p.state = "declined";
          const target = refOf(p.write);
          if (args.permanent && target && !loroSuppressed.includes(target)) loroSuppressed.push(target);
          return null;
        }
        case "loro_unsuppress_record": {
          const gate = loroGate();
          if (gate) return Promise.reject(gate);
          const ref = args.recordRef ?? args.record_ref;
          const at = loroSuppressed.indexOf(ref);
          if (at >= 0) loroSuppressed.splice(at, 1);
          return null;
        }

        // ---- the spoken correction desk (`staging.rs`) --------------------------------
        case "spoken_corrections_available":
          return spokenDeskOn;
        case "spoken_pending_corrections": {
          const gate = spokenGate();
          if (gate) return Promise.reject(gate);
          return candidates.slice();
        }
        case "spoken_suppressed_terms": {
          const gate = spokenGate();
          if (gate) return Promise.reject(gate);
          return spokenSuppressed.slice();
        }
        case "spoken_confirm_correction": {
          const gate = spokenGate();
          if (gate) return Promise.reject(gate);
          const at = candidates.findIndex((c) => c.key === args.key);
          if (at < 0) return Promise.reject("no correction " + args.key + " is awaiting an answer");
          if (window.__RICHOS_MOCK__ && window.__RICHOS_MOCK__._noVocabulary) {
            // `staging.rs:74`: RichOS will not report a term as learned when nothing wrote
            // it, and the candidate stays answerable.
            return Promise.reject(
              "no vocabulary backend is attached — RichOS will not report a term as learned when nothing wrote it"
            );
          }
          const c = candidates.splice(at, 1)[0];
          const already = window.__RICHOS_MOCK__ && window.__RICHOS_MOCK__._vocabularyAlreadyKnew;
          return { file: "loro/entities.json", changed: !already, created: false, version: "2026-08-30T09:12:00Z" };
        }
        case "spoken_decline_correction": {
          const gate = spokenGate();
          if (gate) return Promise.reject(gate);
          const at = candidates.findIndex((c) => c.key === args.key);
          if (at < 0) return Promise.reject("no correction " + args.key + " is awaiting an answer");
          const c = candidates.splice(at, 1)[0];
          if (args.permanent) {
            if (!spokenSuppressed.includes(c.key)) spokenSuppressed.push(c.key);
          } else {
            // §7: a plain decline is re-asked on the very next repeat, and the second ask
            // has to SAY it was asked before. The count is kept, not the card.
            declinedCounts[c.key] = (declinedCounts[c.key] || 0) + 1;
          }
          return null;
        }
        case "spoken_unsuppress_term": {
          const gate = spokenGate();
          if (gate) return Promise.reject(gate);
          const at = spokenSuppressed.indexOf(args.key);
          if (at >= 0) spokenSuppressed.splice(at, 1);
          return null;
        }

        // ---- the feedback channel (`feedback.rs`) --------------------------------------
        //
        // NOTHING HERE SENDS ANYTHING, in the harness for the same reason as in the
        // product: there is no transport to mock, because there is none to run.
        case "feedback_available":
          return feedbackStoreOpen;
        case "feedback_wording":
          return {
            question: FEEDBACK_QUESTION,
            options: FEEDBACK_OPTIONS,
            reportOffer: FEEDBACK_REPORT_OFFER,
            disclosureHeading: FEEDBACK_DISCLOSURE_HEADING,
            taxonomyVersion: "v1",
            ratings: FEEDBACK_RATINGS.map((r) => ({ ...r })),
            dismiss: { key: "0", label: "Dismiss" },
          };
        case "feedback_taxonomy":
          return {
            version: "v1",
            failureClass: FEEDBACK_FAILURE_CLASS.map((t) => ({ ...t })),
            occurrences: FEEDBACK_OCCURRENCES.map((t) => ({ ...t })),
            diagnosis: FEEDBACK_DIAGNOSIS.map((t) => ({ ...t })),
            conditions: FEEDBACK_CONDITIONS.map((t) => ({ ...t })),
          };
        case "feedback_preview": {
          const rating = FEEDBACK_RATINGS.find((r) => r.key === args.key);
          if (!rating) return Promise.reject("That isn't one of the four answers, so I haven't written anything down.");
          try {
            const payload = feedbackAssemble(rating, args.selection || {});
            return {
              heading: FEEDBACK_DISCLOSURE_HEADING,
              text: feedbackRender(payload),
              full: feedbackFullText(payload),
            };
          } catch (e) {
            return Promise.reject(String(e));
          }
        }
        case "feedback_record": {
          const outcome = feedbackOutcome(args.key);
          if (!outcome)
            return Promise.reject("That isn't one of the four answers, so I haven't written anything down.");
          const rating = FEEDBACK_RATINGS.find((r) => r.key === args.key);
          const choice = args.report || { decision: "not_offered" };
          let report = { decision: "not_offered" };
          if (choice.decision !== "not_offered") {
            // `FeedbackEntry::with_report`: a `3` or a dismissal never invited an offer, so
            // a report attached to one is not a thing that happened.
            if (!rating || !rating.invitesReport)
              return Promise.reject(
                "rating '" + (rating ? rating.key : "0") + "' does not invite a report — the offer is made on 1 and 2 only"
              );
          }
          if (choice.decision === "declined") report = { decision: "declined" };
          if (choice.decision === "approved") {
            let payload;
            try {
              payload = feedbackAssemble(rating, choice.selection || {});
            } catch (e) {
              return Promise.reject(String(e));
            }
            // THE CHECK THAT MAKES "he saw exactly this" SURVIVE THE BRIDGE. In-process the
            // type system carries it (`ApprovedReport` has no public constructor); across
            // an IPC boundary it has to be re-rendered and compared.
            if (feedbackFullText(payload) !== choice.shown) return Promise.reject(FEEDBACK_PREVIEW_MISMATCH);
            report = { decision: "approved", report: payload };
          }
          if (!feedbackStoreOpen) return Promise.reject(FEEDBACK_STORE_ABSENT);
          const entry = { recorded_at_millis: now(), outcome, report };
          feedbackEntries.push(entry);
          return JSON.parse(JSON.stringify(entry));
        }
        case "feedback_history": {
          if (!feedbackStoreOpen) return Promise.reject(FEEDBACK_STORE_ABSENT);
          if (feedbackReadFailure) return Promise.reject(feedbackReadFailure);
          return feedbackEntries.map((e) => ({
            entry: JSON.parse(JSON.stringify(e)),
            // Re-rendered from the STORED payload, exactly as `ApprovedReport::as_shown`
            // does — no second copy of the text is kept anywhere.
            shown: e.report.decision === "approved" ? feedbackRender(e.report.report) : null,
          }));
        }

        case "launch_state":
          launchCalls.stateReads.push(args);
          return { ...LAUNCH_STATE, recentSplashes: launchCalls.splashShown.slice().reverse().slice(0, 5) };
        case "launch_note_splash_shown":
          launchCalls.splashShown.push(args.id);
          return null;

        // ---- the update path (RICH-TODOs row 12) ---------------------------------
        // Shapes are `UpdateView` from `src-tauri/src/updates.rs`, verbatim. The harness
        // does NOT simulate a download: `mockUpdate.script` is a queue of views a test
        // pushes, so a suite drives the panel through downloading -> installing -> ready,
        // or straight to a refused signature, without any timing to be flaky about.
        case "update_state":
          mockUpdate.calls.push("update_state");
          return { ...mockUpdate.view };
        case "update_check":
          mockUpdate.calls.push("update_check");
          return mockUpdateAdvance();
        case "update_install":
          mockUpdate.calls.push("update_install");
          // REFUSES WHILE BUSY, exactly as `updates.rs` does. The preview must not be able to
          // prove an install that the real product would decline — that is the whole reason
          // the mock mirrors the shell's refusals rather than only its happy paths.
          if (mockUpdate.view.busy) return { ...mockUpdate.view };
          return mockUpdateAdvance();
        case "update_rollback":
          mockUpdate.calls.push("update_rollback");
          // The same gate, and the same reason: `updates.rs::rollback` reads the work
          // verdict before it looks at anything else, so a preview that went ahead here
          // would prove a rollback the product declines.
          if (mockUpdate.view.busy) return { ...mockUpdate.view };
          return mockUpdateAdvance();
        // WHETHER THE PREVIEW OFFERS VOICE, answered rather than left to the default reject.
        //
        // `main.js` treats every unknown as "not available" and hides the talk button, which
        // is the right direction for a real machine with no speech model. The preview is not
        // that: ◉ is part of the shipping composer row the browser harness exists to
        // exercise, and pressing it here already says exactly what is true —
        // "Talking out loud needs the desktop app. Here in the preview, type to me."
        // Offered, and honest when pressed, which is the second of the two shapes the
        // 2026-09-04 voice fix allows.
        case "voice_readiness":
          // A CUSTOMER'S MAC, on demand. `voice: "unavailable"` is what a fresh install
          // reports — the bundle ships no whisper binary and no model — and it is the
          // preset the first-run suite drives the not-offered affordance from.
          if (preset.voice === "unavailable")
            return {
              available: false,
              // VERBATIM from `SttError::BinaryNotFound`'s `ceo_message()`, crates/richos-voice/src/stt.rs.
              // The window does not render it (it hides the button instead); it is here so
              // the preview reports what the product reports.
              reason:
                "My ears aren't installed on this machine yet. Whoever set RichOS up adds those. I can still read what you type.",
              // NO DECODER, so there is nothing RichOS can download that would help. `offer`
              // is absent, and its absence is what keeps the talk button off this machine.
              state: "toolchain-missing",
              offer: null,
            };
          // THE MODEL IS THERE AND IS NOT THE MODEL RICHOS PINNED — different weights under
          // the right name, which is the one speech failure that would otherwise be
          // invisible. The nightly of 2026-09-17 reached this state with 17 corrupted bytes
          // at an identical file size, and the backend refused correctly and beautifully; the
          // window then removed the talk control from the screen AND from the accessibility
          // tree, leaving the explanation in a log the CEO will never open (audit §D3).
          //
          // `offer` is null because `SpeechReadiness::provisionable()` is false here — this
          // is not a gap RichOS can close by downloading, it is tampering somebody has to
          // look at. That absence is exactly what used to hide the button.
          if (preset.voice === "refused")
            return {
              available: false,
              // VERBATIM from `SttError::ToolchainRefused`'s `ceo_message()`,
              // crates/richos-voice/src/stt.rs. The window now RENDERS this rather than
              // hiding the control and saying nothing.
              reason:
                "Something about my hearing changed on this machine and I'd rather not guess at what you said than get it wrong. Whoever set RichOS up can put it right. I can still read what you type.",
              state: "refused",
              offer: null,
            };
          // A MACHINE WITH A DECODER AND NO WEIGHTS — the state the 2026-09-17 provisioning
          // work exists for, and the one a fresh Mac that has run `brew install whisper-cpp`
          // is actually in. `available` is still false (speech cannot happen right now) and
          // `offer` is present, so the talk button appears and opens on the offer row.
          //
          // THE NUMBERS ARE THE REAL ONES. 487,614,201 bytes is `small.en` in
          // engine/voice/models/model-pins.json, which is what a machine with no weights at
          // all resolves to (hardware.rs's safe rung); 536,375,622 is that plus the 10%
          // headroom `Pin::required_free_bytes` computes. A preview that rehearses this state
          // with invented sizes rehearses a different product.
          // AND IT STOPS BEING MODEL-MISSING ONCE THE MODEL IS INSTALLED, which is what the real
          // backend does: `stt::readiness` resolves the file from disk on the next call, so the
          // answer genuinely changes mid-session. A mock that kept answering "model-missing"
          // after emitting `installed` would let a suite prove the finish screen while quietly
          // making the step after it unreachable — and would have hidden the fact that
          // `#voice-model-listen` re-asks readiness before it opens anything.
          if ((preset.voice === "model-missing" || mockVoiceModel.offer) && !mockVoiceModel.installed)
            return {
              available: false,
              // VERBATIM from `SttError::ModelNotFound`'s `ceo_message()`, split from the
              // toolchain-missing sentence above on 2026-09-17 (nightly audit §D2) — nobody
              // but RichOS itself closes this gap now, so it names no party.
              reason:
                "My speech model isn't on this machine yet. I can fetch it myself now, so I'll offer to download it the next time you turn voice on. I can still read what you type.",
              state: "model-missing",
              offer: mockVoiceModel.offer || {
                modelId: "small.en",
                bytes: 487614201,
                sizeLabel: "487.6 MB",
                needFreeBytes: 536375622,
                freeBytes: 91234567890,
                enoughRoom: true,
                alreadyHave: 0,
                singleWitness: false,
              },
            };
          return { available: true, reason: null, state: "ready", offer: null };

        // The download itself. The preview never opens a socket: a suite drives the phases
        // through `voiceModelSet`, which is the same shape `rich://voice-model` carries.
        case "provision_speech_model":
          mockVoiceModel.calls.push("provision_speech_model");
          if (mockVoiceModel.rejectWith) throw new Error(mockVoiceModel.rejectWith);
          return { status: "installed" };
        case "cancel_speech_model_download":
          mockVoiceModel.calls.push("cancel_speech_model_download");
          return { stopping: true };

        case "update_relaunch":
          // Compatibility endpoint is a read. No live update restarts a session.
          mockUpdate.calls.push("update_relaunch");
          return { ...mockUpdate.view };

        // ---- Bust a bug (round 21): the shapes `src-tauri/src/bug_report.rs` answers in ----
        case "bug_report_context":
          bugMock.calls.push({ cmd });
          return { account: bugMock.account, repository: "WebDevBooster/richos" };
        case "bug_report_private_in": {
          // The words named in the order they are written, each once: the shell's answer shape.
          bugMock.calls.push({ cmd, text: args.text, private: args.private });
          if (preset.bugPrivateHold) await bugHold("private");
          const text = String(args.text || "");
          const hits = [];
          // A space in a name takes a whole run of whitespace, a line break included, as the
          // core's `folded_match_end` does: "Jane" and "Doe" on two lines are still the name.
          bugTerms().concat(args.private || []).forEach((t) => {
            const words = t.text.trim().split(/\s+/).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
            const m = new RegExp("(^|[^\\p{L}\\p{N}_])(" + words.join("\\s+") + ")(?![\\p{L}\\p{N}_])", "iu").exec(text);
            if (m) hits.push([m.index + m[1].length, m[2]]);
          });
          (preset.bugPrivateWords || []).forEach((w) => { const at = text.indexOf(w); if (at !== -1) hits.push([at, w]); });
          hits.sort((a, b) => a[0] - b[0]);
          return hits.map((h) => h[1]).filter((w, i, all) => all.indexOf(w) === i);
        }
        case "bug_report_write": {
          bugMock.calls.push({ cmd, answer: args.answer, screen: args.screen, post: args.post });
          await new Promise((r) => setTimeout(r, preset.bugWriteMs ?? 600));
          // Claude could not be asked: no draft, the report is kept until Rich can check it.
          if (bugMock.claude === "down") {
            const id = "00000000-0000-4000-9000-" + String(++bugMock.seq).padStart(12, "0");
            bugMock.unchecked.push({ id, answer: String(args.answer || ""), screen: args.screen || {}, post: args.post || "bug" });
            return { state: "unchecked", id, workedMs: 90000 };
          }
          const draft = bugDraft(args.answer, args.screen, args.post);
          const report = "00000000-0000-4000-a000-" + String(++bugMock.seq).padStart(12, "0");
          bugKeepCheck(report, draft);
          return { state: "checked", report, draft, digest: bugDigest(args.screen), workedMs: 9000 };
        }
        case "bug_report_take_unchecked": {
          bugMock.calls.push({ cmd, id: args.id, post: args.post });
          const at = bugMock.unchecked.findIndex((u) => u.id === args.id);
          if (at !== -1) bugMock.unchecked.splice(at, 1);
          return at !== -1;
        }
        case "bug_report_change": {
          bugMock.calls.push({ cmd, said: args.said, sheet: args.sheet, private: args.private, post: args.post });
          await new Promise((r) => setTimeout(r, preset.bugWriteMs ?? 400));
          if (preset.bugChangeHold) await bugHold("change");
          // Claude could not be asked: nothing is added (the shell's answer, word for word).
          if (bugMock.claude === "down") throw "I couldn't check that change just now, so nothing was added.";
          const said = String(args.said || "").replace(/^(also|and|please)\s+/i, "").replace(/^(say|mention|add)\s+(that\s+)?/i, "");
          // The report's own private words go on applying to every change (the shell's `scrub_change`),
          // and every stand-in already in his words is marked by its kind, as in the first draft.
          const kept = args.private || [];
          const add = bugScrub(bugSentence(said), kept).flatMap((s) => (s.kind ? [s] : bugMarked(s.text)));
          // The card with his words added is a changed card: at Send it goes through the scanner
          // and then Rich, last (the core's `at_send`, CEO §115), as words changed by hand do.
          return { section: args.post === "idea" ? "The idea" : "What happened", add, private: kept, workedMs: 4000 };
        }
        case "bug_report_send": {
          bugMock.calls.push({ cmd, report: args.report, sheet: args.sheet, screen: args.screen, post: args.post });
          await new Promise((r) => setTimeout(r, 300));
          const report = args.report || "00000000-0000-4000-a000-" + String(++bugMock.seq).padStart(12, "0");
          const held = bugMock.checks[report];
          if (!held || held.sheet !== JSON.stringify(args.sheet)) {
            // Not what Rich last checked: he checks the report as it is now first.
            if (preset.bugSendCheckHold) await bugHold("sendcheck");
            const named = bugRichCheck(report);
            if (bugMock.claude === "down") {
              const id = "00000000-0000-4000-9000-" + String(++bugMock.seq).padStart(12, "0");
              bugMock.unchecked.push({ id, answer: "", screen: args.screen || {}, post: args.post || "bug", edited: { sheet: args.sheet, private: (held || {}).private || [] } });
              return { state: "unchecked", id };
            }
            const again = bugRedraft(args.sheet, named);
            bugKeepCheck(report, again);
            if (JSON.stringify(bugSheetOf(again)) !== JSON.stringify(args.sheet)) return { state: "checked", report, draft: again, digest: "Checked your changes for private details" };
          }
          const d = bugDeliver(args.sheet, "00000000-0000-4000-8000-" + String(++bugMock.seq).padStart(12, "0"), args.post);
          if (d.state === "sent") delete bugMock.checks[report];
          return d;
        }
        case "bug_report_try_now": {
          bugMock.calls.push({ cmd, id: args.id, post: args.post });
          const at = bugMock.waiting.findIndex((w) => w.id === args.id);
          if (at === -1) return null;
          const w = bugMock.waiting[at];
          const d = bugDeliver(w.sheet, w.id, w.post);
          if (d.state === "sent") { bugMock.waiting.splice(at, 1); bugMock.sent.push(d); }
          return d;
        }
        case "bug_report_cancel": {
          // The shell's answer: `{state: "canceled"}` once the waiting copy is gone, or the issue
          // when it had already gone out; an error when it could not be removed.
          bugMock.calls.push({ cmd, id: args.id });
          if (preset.bugCancel === "hold") await bugHold("cancel");
          if (preset.bugCancel === "fail") throw "The waiting copy could not be removed from this Mac.";
          const went = bugMock.sent.find((d) => d.id === args.id);
          if (went) return went;
          const at = bugMock.waiting.findIndex((w) => w.id === args.id);
          if (at !== -1) bugMock.waiting.splice(at, 1);
          return { state: "canceled" };
        }
        case "bug_report_look":
          // The shell takes a picture of the window for the user's own Claude; here there is none.
          bugMock.calls.push({ cmd });
          return { taken: false };
        case "bug_report_voice":
          bugMock.calls.push({ cmd, on: !!args.on });
          return !!args.on;
        // The preview has no microphone, so voice mode never opens here (main.js says so in one
        // line). `preset.bugVoice` opens it for the Bust a bug suite, which drives what the user
        // says with `__RICHOS_MOCK_BUG__.say` and reads the hold off `bug_report_voice`'s calls.
        // `preset.bugStopHold` keeps each stop unanswered until `releaseStop()`, as a microphone
        // that takes a moment to close does (review rv-20261010T183313Z-fe4cd6b1-1e66 finding 2).
        case "start_voice_capture":
        case "stop_voice_capture":
          if (!preset.bugVoice) return Promise.reject(`mock: no such command "${cmd}"`);
          bugMock.calls.push({ cmd, threadId: args.threadId });
          if (cmd === "stop_voice_capture" && preset.bugStopHold) await bugHold("stop");
          return { started: cmd === "start_voice_capture" };
        case "bug_report_open_issue":
          bugMock.calls.push({ cmd, number: args.number, post: args.post });
          return null;

        default:
          // Unwired-yet commands (voice capture, worker status, assertiveness persistence)
          // reject exactly like a real Tauri call to an unregistered command would — main.js
          // must degrade gracefully rather than assume these exist.
          return Promise.reject(`mock: no such command "${cmd}"`);
      }
    },

    async listen(eventName, cb) {
      listeners[eventName] = listeners[eventName] || new Set();
      listeners[eventName].add(cb);
      return () => listeners[eventName].delete(cb);
    },
  };

  // =======================================================================================
  // §26 — THE DETERMINISTIC `memory-strategy` FIXTURE
  // =======================================================================================
  //
  // §26 prescribes sixteen steps and calls the result the acceptance instrument for Phase
  // 3's exit gate. An acceptance instrument that emits signals the product cannot emit
  // certifies nothing — it certifies the fixture. So this scenario was built to what the
  // runtime can ACTUALLY produce, and every step §26 asks for that the runtime cannot
  // produce is recorded below as unrepresentable, with the file and line that makes it so.
  //
  // ---------------------------------------------------------------------------------------
  // THE THREE THINGS THIS FIXTURE REFUSES TO DO, AND WHY
  // ---------------------------------------------------------------------------------------
  //
  // 1. NO WORKER FAILURE (§26 step 10, §10.6, §7.4).
  //    `ObservedWorkerState` has exactly four variants — `Created`, `Started`, `Updated`,
  //    `RunEnded` (worker_events.rs:128-139) — and `RunEnded`'s own doc says "**The reason
  //    is not observable.** Never `Completed`." (worker_events.rs:137-138).
  //    `WorkerState::from_observed` maps it to `RUN_ENDED_WORKER_STATE`, a named constant
  //    equal to `WorkerState::Unknown` (timeline.rs:403, :413-421), and the renderer draws
  //    that as `Ended · outcome not recorded` (timeline.js:336-360). There is no payload
  //    anywhere in the worker path that carries an outcome. A fixture that emitted
  //    `state: "failed"` would light up a renderer branch production can never reach, and a
  //    green run over it would certify a state that cannot occur.
  //
  // 2. NO PLAN (§26 step 13, §8).
  //    `TimelineItem::Plan` is "MODELLED, NEVER PRODUCED" (timeline.rs:611-617) and the
  //    module's own source table says "**NO SOURCE.** `plan` updates are Phase-2 machinery"
  //    (timeline.rs:62). Those updates are retained verbatim as `MachineryKind::Unknown`
  //    (machinery.rs:556) with their entries living only inside the EVICTABLE raw payload,
  //    so a plan projected from them empties out after the Tier-B window (timeline.rs:466-469).
  //    `rich://plan-updated` is DEFERRED in the emitter (live.rs:27). Nothing constructs a
  //    `PlanItem`. `Step 2 of 5 -> Step 3 of 5` has no source at either end.
  //
  // 3. NO FAILURE CLAIM IN RICH'S OWN WORDS EITHER — the subtle half.
  //    §10.6 scripts Rich saying *"Sage stopped after writing most of the architecture."*
  //    Prose is the one channel where anything at all can be typed, so faking the failure
  //    through Rich's mouth would sail past every state check in this file while making
  //    exactly the claim the data cannot support. Rich's recovery commentary here says only
  //    what the runtime witnessed: the run ended and no outcome was recorded.
  //
  // ---------------------------------------------------------------------------------------
  // THE PATH — the same one a real turn takes, with ONE named divergence
  // ---------------------------------------------------------------------------------------
  //
  // Prose, activity, turn state AND WORKER ROWS all go out as §13 events the emitter
  // actually emits, and reach the renderer through `main.js`'s listeners — the real live
  // path, with no divergence left in this fixture.
  //
  // Worker rows were the one exception until 2026-08-29: `rich://worker-upserted` was
  // deferred in the emitter (live.rs:26) and had no listener in `main.js`, so their only
  // route to the screen was the durable snapshot and this fixture measured the consequence
  // — 0 chips live, 3 after a `get_timeline` read. The emitter now sends it (live.rs, "the
  // delegation the CEO sees while it happens"), so the fixture sends it too: every worker
  // row is written into the durable turn record AND emitted live, exactly as the backend
  // does. The payload shape is `TimelineItem::WorkerActivity` + `at`, which is what
  // `LiveEvent::WorkerUpserted::payload` puts on the wire.
  //
  // ---------------------------------------------------------------------------------------
  // THE CLOCK
  // ---------------------------------------------------------------------------------------
  //
  // Injected as `anchor` — the wall-clock instant virtual t=0 maps onto — and every event
  // timestamp is `anchor + offset`. Nothing sleeps and nothing polls: `runTo(n)` walks the
  // whole two-hour turn in under a millisecond. This is deliberately NOT a monkey-patched
  // `Date.now`, because §6.2 requires the display to be DERIVED from persisted timestamps
  // ("Persist timestamps and derive the display locally"), and driving the fixture through
  // real timestamps is what proves that derivation instead of bypassing it. A test wanting
  // `Working for 18s` anchors 18s + the lease handoff in the past and reads the real row.
  //
  // THE DURATION, DERIVED RATHER THAN ASSERTED (§26 step 16 = `2h 17m 50s`):
  //   2h 17m 50s = (2 x 3600) + (17 x 60) + 50 = 7200 + 1020 + 50 = 8270 s = 8_270_000 ms
  // and back through `RichTimeline.formatDuration(8270000)` (timeline.js:72-86):
  //   totalSec  = floor(8270000/1000) = 8270 ;  s = 8270 mod 60          = 50
  //   totalMin  = floor(8270/60)      =  137 ;  m =  137 mod 60          = 17
  //   totalHour = floor(137/60)       =    2 ;  h =    2 mod 24          =  2
  //   d = 0, totalHour >= 1           -> "2h 17m 50s"
  // The measured span is `endedAt - startedAt` (`ledger::Turn::active_ms`), so the fixture
  // sets `endedAt = startedAt + 8_270_000` and never writes `activeMs` by hand.
  //
  // ONE HONEST LIMIT OF ANCHORING RATHER THAN PATCHING. While a turn is LIVE the row is
  // `Date.now() - startedAt` (timeline.js:151), so mid-scenario it shows the REAL elapsed
  // time since the anchor — a few hundred milliseconds — not the virtual position. That is
  // the renderer being right, not the fixture being wrong: a live timer that read from
  // scripted timestamps would be a fixture-only code path. §26's two duration screenshots
  // are `Working for 18s` (anchored, live, real) and `Worked for 2h 17m 50s` (measured,
  // terminal), and both are exact. A live row reading `Working for 1h 6m 0s` would need an
  // anchor 1h06m in the past — one line in a test, and no step of §26 asks for it.
  // =======================================================================================

  /// The live scenario, if one has been started, and the wall-clock instant its virtual
  /// t=0 maps onto. A test sets the anchor BEFORE pressing Enter — that is the whole of
  /// the clock injection, and it is why a two-hour turn takes under a millisecond.
  let memoryScenario = null;
  let memoryStrategyAnchor;

  const MS_ACTIVE = 8270000; // see the arithmetic above
  const MS_LEASE_HANDOFF = 600; // t=0 accepted -> t=600 a lease has it and the clock starts
  const MS_TURN_END = MS_LEASE_HANDOFF + MS_ACTIVE;
  const MEMORY_TURN_ID = "turn_memory_01";
  const MEMORY_THREAD_ID = "memory";

  const MEMORY_PROMPT = [
    "Read all the linked material and design a proper replacement for Claude auto memory.",
    "",
    "Sources, in the order I want them read:",
    "",
    "  /Users/you/Projects/harbor/docs/plans/ecs-architecture-2026-08-27.md",
    "  /Users/you/Projects/harbor/wiki/decisions.md",
    "  /Users/you/Projects/northwind/NOTES.md",
    "  /Users/you/Projects/northwind/memory/MEMORY.md",
    "  https://docs.anthropic.com/en/docs/claude-code/memory",
    "  https://code.claude.com/docs/en/memory",
    "",
    "What I actually want to know:",
    "",
    "  - what the Executive Continuity System has to hold, and what it must refuse to hold",
    "  - whether it is one store or several, and who is allowed to write to it",
    "  - how a correction reaches it, and what happens to the thing it corrects",
    "  - how any of it survives a restart, and what is lost when it does not",
    "  - what the cutover from Claude auto memory looks like on the day it happens",
    "",
    "Take the passes in parallel if that is faster. Do not build anything yet.",
    "Ask me 3 to 5 questions first, and make them the ones you actually cannot answer",
    "yourself — I would rather answer five hard questions than read five easy ones.",
  ].join("\n");

  /// §26's sixteen steps, each judged against the runtime rather than against the brief.
  ///
  /// `status` is one of:
  ///   `represented`    — the step is emitted and rendered, in full.
  ///   `partial`        — what the runtime witnesses is emitted; a named part of §26's
  ///                      wording has no source and is NOT drawn.
  ///   `unrepresentable`— nothing is emitted at all. The browser suite asserts these are
  ///                      ABSENT from the DOM, so "we did not fake it" is a test rather
  ///                      than a promise.
  ///
  /// This table is the deliverable as much as the pixels are. If a signal lands later, the
  /// negative assertions in `memory-strategy.js` fail loudly and point back at this row.
  const MEMORY_STRATEGY_STEPS = [
    { n: 1, spec: "Long CEO prompt with file paths and two URLs", status: "partial",
      have: "The full prompt renders, over §5.1's 18-line clamp, with Show more/Show less.",
      gap: "The two URLs are TEXT, not links. §10.2 wants them clickable; the CEO bubble is " +
           "built with textContent and nothing linkifies it (timeline.js:969).",
      source: "timeline.js:940,969" },
    { n: 2, spec: "Turn accepted", status: "represented",
      have: "rich://turn-status queued at t=0 with startedAt null -> §6.1's bare `Working`, " +
            "then working at t=600 -> `Working for {d}` once a second has passed.",
      source: "live.rs:107" },
    { n: 3, spec: "Rich commentary", status: "partial",
      have: "The commentary streams as a real message run and stays in the transcript.",
      gap: "phase is `unknown`, never `commentary`. STREAMED_MESSAGE_PHASE is a named " +
           "constant equal to RichMessagePhase::Unknown because no signal separates " +
           "commentary from the final answer on this wire.",
      source: "live.rs:120, live.rs:42-62" },
    { n: 4, spec: "Seven file reads grouped into one summary", status: "represented",
      have: "Seven `Read a file` rows, consecutive, rolled up by the renderer into " +
            "`Read 7 files` — the grouping §26 asks for, done where it is honest to do it.",
      source: "timeline.rs:1400-1417, timeline.js:290-308" },
    { n: 5, spec: "One web search", status: "partial",
      have: "One search activity row, rendered `Searched`.",
      gap: "§26/§5.3's `Searched the web for Claude Code memory` cannot be produced: " +
           "semantic_summary builds the line from the activity TYPE alone and deliberately " +
           "carries no query text, so the CEO default stays semantic and never leaks syntax.",
      source: "timeline.rs:1400-1417" },
    { n: 6, spec: "Sage, Frank and Clark start", status: "represented",
      have: "Three worker rows, `created` then `started` -> Starting/Working, grouped as " +
            "`Sage, Frank and Clark started working` — LIVE, during the turn. Was " +
            "snapshot-only until 2026-08-29, when rich://worker-upserted stopped being " +
            "deferred; this fixture measured that gap as 0 chips live and 3 after the read.",
      source: "live.rs:26, worker_events.rs:128-139" },
    { n: 7, spec: "Clark update", status: "represented",
      have: "`updated` -> Running, with the summary Clark actually authored, live and in " +
            "place — one run is one row however many lifecycle events it produced.",
      source: "worker_events.rs:134-136" },
    { n: 8, spec: "Frank completion", status: "partial",
      have: "Frank's run ENDS, and the row says so: `Ended · outcome not recorded`.",
      gap: "`completion` is not witnessed. run_ended is the honest superset of completed, " +
           "interrupted and failed; §7.1's verb `Frank finished` would claim the one thing " +
           "nobody observed.",
      source: "timeline.rs:394-403, worker_events.rs:147-153" },
    { n: 9, spec: "Sage partial update", status: "partial",
      have: "`updated` -> Running, with Sage's authored summary.",
      gap: "Snapshot-only, as step 6.",
      source: "worker_events.rs:134-136" },
    { n: 10, spec: "Sage failure", status: "unrepresentable",
      have: "Sage's run ends and is drawn `Ended · outcome not recorded` — which is what " +
            "was actually witnessed, and is emitted.",
      gap: "THE FAILURE ITSELF IS NOT EMITTED AND NOT DRAWN. No payload in the worker path " +
           "carries an outcome; WorkerState::Failed has no witness anywhere in the hook " +
           "set. A `failed` chip here would be a green test over a state production cannot " +
           "produce.",
      source: "worker_events.rs:137-138, timeline.rs:361, timeline.rs:413-421" },
    { n: 11, spec: "Rich recovery commentary", status: "partial",
      have: "Rich posts commentary about the ended run, in his own voice, saying only what " +
            "the runtime witnessed.",
      gap: "TWO parts have no source. phase is `unknown`, not `recovery` (live.rs:120) — " +
           "TimelineItem::Recovery is real but means a mid-turn CRASH REPLAY " +
           "(TurnSuperseded) and is Internal visibility, a different event entirely. And " +
           "§10.6's scripted line *\"Sage stopped after writing most of the architecture\"* " +
           "is a failure claim; putting it in Rich's mouth would fake step 10 through the " +
           "one channel that accepts any string.",
      source: "live.rs:120, timeline.rs:652-661" },
    { n: 12, spec: "Sage replacement start", status: "partial",
      have: "A SECOND Sage agent id opens a new run — `created` then `started` — in its own " +
            "group below the commentary, and the parent turn's clock never resets.",
      gap: "Nothing on the wire links the two runs. `replacement` is a relationship the " +
           "data model does not carry, so the UI shows two runs, not a retry.",
      source: "worker_events.rs:128-139" },
    { n: 13, spec: "Plan update from step 2 of 5 to step 3 of 5", status: "unrepresentable",
      have: "Nothing. No plan row is emitted at either step count.",
      gap: "TimelineItem::Plan is modelled and never produced; plan session updates are " +
           "retained as MachineryKind::Unknown with entries only in the EVICTABLE raw " +
           "payload, so a projected plan silently empties after the Tier-B window; and " +
           "rich://plan-updated is deferred in the emitter.",
      source: "timeline.rs:62, timeline.rs:466-469, timeline.rs:611-617, machinery.rs:556, live.rs:27" },
    { n: 14, spec: "Final Sage completion", status: "partial",
      have: "The replacement run ends: `Ended · outcome not recorded`.",
      gap: "As step 8 — no completion signal exists at worker grain.",
      source: "timeline.rs:394-403" },
    { n: 15, spec: "Final Rich response with five questions", status: "partial",
      have: "The five questions stream as Rich's last message run and sit below the " +
            "completed-duration divider, where §25 wants the final response.",
      gap: "It is not LABELLED final (phase `unknown`), so it gets no distinct treatment; " +
           "and TimelineItem::Question — the answerable card — has no source, because §11's " +
           "waiting_for_user state does not exist in this runtime.",
      source: "live.rs:120, timeline.rs:637-647, timeline.js:1416-1421" },
    { n: 16, spec: "Turn completion at a deterministic active duration of 2h 17m 50s", status: "represented",
      have: "endedAt - startedAt = 8_270_000ms exactly, measured not asserted, and the row " +
            "freezes at `Worked for 2h 17m 50s` with the transcript collapsed under it.",
      source: "timeline.js:72-86, timeline.rs:539-556" },
  ];

  /// The scenario. Deterministic by construction: fixed ids, fixed offsets, no Math.random,
  /// no setTimeout, no wall clock except the injected `anchor`.
  ///
  ///     const s = window.__RICHOS_MOCK__.memoryStrategy({ anchor: Date.now() - 18600 });
  ///     s.runTo(5);                      // through "one web search"
  ///     s.runTo(16);                     // the whole two-hour turn, instantly
  ///
  /// Constructing it RESETS the thread, so two runs produce byte-identical output for the
  /// same anchor.
  function memoryStrategy(options) {
    options = options || {};
    const anchor = typeof options.anchor === "number" ? options.anchor : Date.now();
    const startedAt = anchor + MS_LEASE_HANDOFF;
    const at = (offset) => anchor + offset;
    const fence = fenceOf(MEMORY_THREAD_ID, MEMORY_TURN_ID);

    // Reset — the same construction twice must not stack two turns in one thread.
    turnsById.delete(MEMORY_TURN_ID);
    messagesByThread[MEMORY_THREAD_ID] = [];

    const turn = {
      threadId: MEMORY_THREAD_ID,
      entityId: "northwind",
      userText: MEMORY_PROMPT,
      runs: [],
      state: "queued",
      createdAt: anchor,
      startedAt: null,
      endedAt: null,
      activities: [],
    };
    turnsById.set(MEMORY_TURN_ID, turn);

    // ONE dense sequence across prose runs and work rows, exactly as the ledger keeps it
    // (`text_machinery_and_timeline_records_share_one_dense_per_turn_sequence`), so the
    // renderer's interleave is the real chronology and not this file's idea of one.
    let seq = 0;
    const nextSeq = () => seq++;

    function activityRow(id, offset, activityType, summary, state) {
      const row = {
        kind: "activity",
        id,
        entityId: fence.entityId,
        threadId: MEMORY_THREAD_ID,
        bindingRevision: fence.bindingRevision,
        createdAt: at(offset),
        sequence: nextSeq(),
        slot: "stream",
        visibility: "ceo",
        activityType,
        state,
        summary,
      };
      if (state === "completed") row.completedAt = at(offset);
      turn.activities.push(row);
      emit("rich://activity-upserted", Object.assign({ turnId: MEMORY_TURN_ID }, row, { at: at(offset) }));
      return row;
    }

    /// A worker row goes into the durable record AND onto the live wire, because the
    /// backend now does both. See the path note above for what changed and when.
    function workerRow(id, offset, worker) {
      const row = {
        kind: "worker_activity",
        id,
        entityId: fence.entityId,
        threadId: MEMORY_THREAD_ID,
        bindingRevision: fence.bindingRevision,
        createdAt: at(offset),
        sequence: nextSeq(),
        slot: "stream",
        visibility: "ceo",
        detailRef: id,
        worker,
      };
      turn.activities.push(row);
      emitWorker(row, at(offset));
      return row;
    }

    /// Update a worker row IN PLACE, by id — one run is one row however many events it
    /// produces, which is the invariant slice 5 landed and slice 7 tested. The live event
    /// carries the WHOLE row under the same id, which is why the renderer's upsert leaves
    /// one chip rather than two.
    function workerUpdate(id, patch, offset) {
      const row = turn.activities.find((a) => a.id === id);
      if (!row) return row;
      Object.assign(row.worker, patch);
      emitWorker(row, typeof offset === "number" ? at(offset) : now());
      return row;
    }

    /// One `rich://worker-upserted`, shaped exactly as `LiveEvent::WorkerUpserted::payload`
    /// shapes it: the flattened timeline item, the fence already inside it, plus `at`.
    function emitWorker(row, atMs) {
      emit("rich://worker-upserted", Object.assign({ turnId: MEMORY_TURN_ID }, row, { at: atMs }));
    }

    /// One contiguous run of Rich's prose, streamed sentence by sentence through the three
    /// real message events. The run index is the item id suffix, so the live item and the
    /// re-projected one are the SAME id and a reload re-states rather than duplicates.
    function prose(offset, text) {
      const index = turn.runs.length;
      const messageId = MEMORY_TURN_ID + ":text:" + index;
      const startSeq = nextSeq();
      turn.runs.push({ text, startSeq, at: at(offset) });
      emit("rich://message-started", Object.assign({}, fence, {
        messageId, phase: "unknown", seq: startSeq, visibility: "ceo", at: at(offset),
      }));
      // Deterministic chunking: split after each newline-terminated block, never randomly.
      const parts = text.split(/(?<=\n)/);
      let d = 0;
      for (const part of parts) {
        emit("rich://chunk", { threadId: MEMORY_THREAD_ID, turnId: MEMORY_TURN_ID, seq: startSeq, textDelta: part, at: at(offset + d) });
        emit("rich://message-delta", Object.assign({}, fence, {
          messageId, seq: startSeq, textDelta: part, visibility: "ceo", at: at(offset + d),
        }));
        d += 40;
      }
      emit("rich://message-completed", Object.assign({}, fence, {
        messageId, phase: "unknown", text, visibility: "ceo", at: at(offset + d),
      }));
      messagesByThread[MEMORY_THREAD_ID].push({ role: "assistant", text, turn_id: MEMORY_TURN_ID, at: at(offset) });
      return messageId;
    }

    // -- the sixteen steps, in order ------------------------------------------------------
    const actions = {
      1() {
        messagesByThread[MEMORY_THREAD_ID].push({ role: "user", text: MEMORY_PROMPT, turn_id: MEMORY_TURN_ID, at: anchor });
        return "the CEO's prompt, " + MEMORY_PROMPT.split("\n").length + " lines, 2 URLs, 4 file paths";
      },
      2() {
        emit("rich://turn-status", Object.assign({}, fence, {
          status: "queued", startedAt: null, activeDurationMs: null, visibility: "ceo", at: anchor,
        }));
        turn.state = "working";
        turn.startedAt = startedAt;
        emit("rich://turn-started", { threadId: MEMORY_THREAD_ID, turnId: MEMORY_TURN_ID, at: startedAt });
        emit("rich://turn-status", Object.assign({}, fence, {
          status: "working", startedAt, activeDurationMs: null, visibility: "ceo", at: startedAt,
        }));
        return "accepted at t=0 (`Working`), lease at t=" + MS_LEASE_HANDOFF + "ms (clock starts)";
      },
      3() {
        prose(2000,
          "I'm reading the full source set now — the ECS architecture package, your decisions " +
          "file, Northwind's own record and both vendor memory docs — and tracing where RichOS " +
          "actually reads and writes memory today rather than where the docs say it does.\n" +
          "Once I can see the seams I'll split the work so the passes run in parallel."
        );
        return "commentary run 0, phase `unknown` (no phase signal exists)";
      },
      4() {
        for (let i = 1; i <= 7; i++) activityRow("mach_ms_read_" + i, 6000 + (i - 1) * 900, "read", "Read a file", "completed");
        return "7 x `Read a file` -> the renderer rolls up to `Read 7 files`";
      },
      5() {
        activityRow("mach_ms_search_1", 13000, "search", "Searched", "completed");
        return "1 x `Searched` (the query text has no source — see step 5's gap)";
      },
      6() {
        workerRow("mach_ms_w_sage_1", 22000, {
          agentId: "agt_ms_sage_1", workerName: "Sage", agentType: "architecture",
          observedState: "started", state: "running", eventsObserved: 2,
          firstObservedAt: iso(at(22000)), lastObservedAt: iso(at(22400)),
        });
        workerRow("mach_ms_w_frank_1", 22100, {
          agentId: "agt_ms_frank_1", workerName: "Frank", agentType: "red team",
          observedState: "started", state: "running", eventsObserved: 2,
          firstObservedAt: iso(at(22100)), lastObservedAt: iso(at(22500)),
        });
        workerRow("mach_ms_w_clark_1", 22200, {
          agentId: "agt_ms_clark_1", workerName: "Clark", agentType: "research",
          observedState: "started", state: "running", eventsObserved: 2,
          firstObservedAt: iso(at(22200)), lastObservedAt: iso(at(22600)),
        });
        // All three `started`, which is what makes the group summary read §10.4's sentence
        // verbatim — `workerGroupSummary` only says "started working" when every member is
        // running, and says "Delegated to ..." for a mixed group. The `created` -> Starting
        // state is real and is exercised by the `hiring` fixture above; §26 step 6 asks for
        // the three-way START, so this is the three-way start.
        return "3 worker rows in ONE group -> `Sage, Frank and Clark started working`";
      },
      7() {
        workerUpdate("mach_ms_w_clark_1", {
          observedState: "updated", state: "running", eventsObserved: 4,
          latestUpdate: "Read both vendor memory docs and the Northwind record; three claims in CLAUDE.md contradict what is on disk",
          lastObservedAt: iso(at(1320000)),
        });
        return "Clark `updated` -> Working, with the summary he authored";
      },
      8() {
        workerUpdate("mach_ms_w_frank_1", {
          observedState: "run_ended", state: "unknown", eventsObserved: 6,
          latestUpdate: "Red-teamed the retention rules; the correction path is the one that loses data",
          lastObservedAt: iso(at(2460000)),
        });
        return "Frank's RUN ended -> `Ended · outcome not recorded` (not `finished`)";
      },
      9() {
        workerUpdate("mach_ms_w_sage_1", {
          observedState: "updated", state: "running", eventsObserved: 9,
          latestUpdate: "Four entities drafted; the write path and the restart proof are still open",
          lastObservedAt: iso(at(3300000)),
        });
        return "Sage `updated` -> Working, with a partial architecture summary";
      },
      10() {
        workerUpdate("mach_ms_w_sage_1", {
          observedState: "run_ended", state: "unknown", eventsObserved: 11,
          lastObservedAt: iso(at(3960000)),
        });
        // §26 asks for a FAILURE here. Nothing is emitted for it — see the header, and the
        // `unrepresentable` row for step 10. What IS emitted is the run ending, which is
        // the entirety of what the runtime witnessed.
        return "Sage's RUN ended. NO failure signal emitted — step 10 is unrepresentable";
      },
      11() {
        prose(4020000,
          "Sage's run ended and nothing recorded how — the runtime witnesses that a run " +
          "stopped, never why, so I'm not going to put a reason on it that I don't have.\n" +
          "What I can see is the work: four entities drafted, the write path and the restart " +
          "proof still open. That's intact and worth keeping, so I'm putting a second " +
          "architecture pass on just those two open pieces rather than starting the whole " +
          "thing again. Your clock keeps running; nothing resets."
        );
        return "recovery commentary — phrased to what was witnessed, no failure claim";
      },
      12() {
        workerRow("mach_ms_w_sage_2", 4140000, {
          agentId: "agt_ms_sage_2", workerName: "Sage", agentType: "architecture",
          observedState: "started", state: "running", eventsObserved: 2,
          firstObservedAt: iso(at(4140000)), lastObservedAt: iso(at(4140400)),
        });
        return "a SECOND Sage run opens, in its own group; the parent clock does not reset";
      },
      13() {
        // §26 step 13: "Plan update from step 2 of 5 to step 3 of 5."
        // NOTHING IS EMITTED. There is no plan at step 2 to update and no source for step 3.
        return "SKIPPED — no plan row emitted; step 13 is unrepresentable";
      },
      14() {
        workerUpdate("mach_ms_w_sage_2", {
          observedState: "run_ended", state: "unknown", eventsObserved: 8,
          latestUpdate: "Closed the write path and the restart proof; the package is on disk",
          lastObservedAt: iso(at(7920000)),
        });
        return "the replacement run ends -> `Ended · outcome not recorded`";
      },
      15() {
        prose(8100000,
          "I've read the full source set and had Sage, Frank and Clark work the architecture, " +
          "the threat model and the product research independently. The boundary is clear " +
          "enough to build against, and five things are genuinely yours to decide.\n" +
          "\n" +
          "1. What is the first deliverable — the typed store itself, or the correction path " +
          "into it? They can't both be first and the second one shapes the first.\n" +
          "2. One Rich or several? A shared store across sessions and a per-session store are " +
          "different products, and the answer changes the write path.\n" +
          "3. How far does Executive Continuity reach — decisions and their reversals only, or " +
          "the working context around them too?\n" +
          "4. When you correct me, does the old record stay visible with a supersession link, " +
          "or does it go? Frank's pass says this is where data gets lost.\n" +
          "5. On cutover day, does Claude auto memory go read-only, get exported, or stay live " +
          "beside the new store until you say otherwise?"
        );
        return "final run: five questions, below the completed divider";
      },
      16() {
        const endedAt = startedAt + MS_ACTIVE;
        turn.state = "completed";
        turn.endedAt = endedAt;
        emit("rich://turn-completed", { threadId: MEMORY_THREAD_ID, turnId: MEMORY_TURN_ID, stopReason: "end_turn", at: endedAt });
        emit("rich://turn-status", Object.assign({}, fence, {
          status: "completed", startedAt, activeDurationMs: endedAt - startedAt, visibility: "ceo", at: endedAt,
        }));
        emit("rich://thread-summary-updated", Object.assign({}, fence, {
          title: "Design RichOS memory strategy",
          messageCount: messagesByThread[MEMORY_THREAD_ID].length,
          lastActivity: endedAt, status: "idle", visibility: "ceo", at: endedAt,
        }));
        const t = threads.find((x) => x.id === MEMORY_THREAD_ID);
        if (t) { t.last_turn_state = "completed"; t.last_activity = endedAt; }
        return "completed; activeMs = " + (endedAt - startedAt) + " -> `Worked for 2h 17m 50s`";
      },
    };

    let cursor = 0;
    const driver = {
      anchor,
      startedAt,
      turnId: MEMORY_TURN_ID,
      threadId: MEMORY_THREAD_ID,
      activeMs: MS_ACTIVE,
      leaseHandoffMs: MS_LEASE_HANDOFF,
      steps: MEMORY_STRATEGY_STEPS,
      get cursor() { return cursor; },
      /// Apply the next step. Returns its manifest row plus what it actually did.
      step() {
        if (cursor >= 16) return null;
        cursor += 1;
        const did = actions[cursor]();
        return Object.assign({ did }, MEMORY_STRATEGY_STEPS[cursor - 1]);
      },
      /// Apply steps up to and including `n`. Instant — nothing here waits on a clock.
      runTo(n) {
        const out = [];
        while (cursor < n) out.push(driver.step());
        return out;
      },
    };
    return driver;
  }

  /// RFC-3339 with a `+00:00` offset — the shape the worker-lifecycle emitter writes and
  /// `WorkerActivityItem` carries verbatim (timeline.rs:455-461).
  function iso(ms) {
    return new Date(ms).toISOString().replace(/\.\d{3}Z$/, "+00:00");
  }

  /// Speech-model provisioning, for the preview and for the suites.
  ///
  /// `offer` overrides what `voice_readiness` answers; `calls` records which commands the
  /// surface actually issued, which is how a suite proves the Stop button stops rather than
  /// merely looking pressed.
  const mockVoiceModel = { offer: null, calls: [], rejectWith: null, installed: false };

  // --- dev-only test hooks, exercised by a headless check, never by real users ----------
  window.__RICHOS_MOCK__ = {
    // ---- a file Rich put in front of the user (round 36 screen 3) --------------------------
    /// What the cards asked the shell to open or show in Finder, oldest first.
    richFileActions() { return richFileActions.slice(); },
    /// A file attached between turns (Stu's, say) arrives live on `rich://rich-file`, under the
    /// turn named, the way `rich_files_host::start` sends it.
    /// The user's instructions file as the shell has it now, every save the sheet made, and what
    /// Open the file / Show in Finder asked for (round 36 screen 1).
    yourInstructions() { return JSON.parse(JSON.stringify({ text: yourInstructions.text, saves: yourInstructions.saves, opened: yourInstructions.opened })); },
    /// The file edited outside the app (in TextEdit, say) while the app runs.
    yourInstructionsEditedElsewhere(text) { yourInstructions.text = String(text); },
    /// The next saves are refused with this sentence; `null` lets them through again.
    yourInstructionsFailSave(why) { yourInstructions.failSave = why || null; },
    /// Rich's save arrives live on `rich://rich-file`, under the turn named, the way
    /// `rich_files_host::start` sends it.
    instructionsCardArrive(threadId, turnId, rules) {
      const item = instructionsCardItem(threadId, turnId, rules, now());
      (instructionCardsByThread[threadId] = instructionCardsByThread[threadId] || []).push(item);
      emit("rich://rich-file", JSON.parse(JSON.stringify(item)));
      return item.id;
    },
    richFileArrive(threadId, turnId, name) {
      const item = richFileItem(threadId, turnId, "rf_" + uid("x").replace(/[^0-9a-z]/gi, "").slice(-16),
        name, name.split(".").pop().toUpperCase(), "PDF · 12 KB · 1 page", "Preview", now(), true);
      (richFilesByThread[threadId] = richFilesByThread[threadId] || []).push(item);
      emit("rich://rich-file", JSON.parse(JSON.stringify(item)));
      return item.file.id;
    },
    // ---- the output record (output side-panel PRD §6) --------------------------------------
    /// Round 17's `arrives`: a turn that writes two files, announced on `rich://output`.
    outputArrive(threadId) { outputArrive(threadId); },
    /// A recorded file is no longer where it was written (§4.6): the next read says so.
    outputMissing(threadId, name) {
      for (const e of outputByThread[threadId] || []) {
        if (e.name === name) {
          e.exists = false;
          e.bytes = null;
          e.modifiedAt = null;
        }
      }
    },
    /// A recorded file grew on disk (S7: the desk's 25 MB ceiling); the next read re-stats it.
    outputResize(threadId, name, bytes) {
      for (const e of outputByThread[threadId] || []) if (e.name === name) e.bytes = bytes;
    },
    /// The record cannot be read: `list_output` refuses with the shell's sentence (§6.7).
    outputUnreadable(v) { outputUnreadable = v !== false; },
    /// What the harness's record holds for a thread, for asserting on the store.
    outputList(threadId) { return outputListOf(threadId); },
    /// One file answers `output_preview` with `answer` (a `Preview` shape) until set back with
    /// `null`: too large, no preview, a read that failed (S5, §6.7).
    outputPreviewAs(name, answer) {
      if (answer == null) outputPreviewOverrides.delete(name);
      else outputPreviewOverrides.set(name, answer);
    },
    /// The actions (S6): every `output_*` call the panel made, in order.
    outputCalls() { return outputActionCalls.map((c) => ({ cmd: c.cmd, args: Object.assign({}, c.args) })); },
    /// The recorded path of `name` is now a link (§5.2 step 2): refused, Show in Finder lit.
    outputLinked(name) { outputLinks.add(name); },
    /// The save sheet's next answer: a path, or null for Cancel.
    outputSaveSheet(answer) { outputSheetAnswer = answer; },
    /// This Mac's app list for a file changes once, under the next *Open with…* choice.
    outputAppsChange() { outputAppsChangeOnce = true; },
    // ---- screenshots and files on the Mac composer (CEO §86) -------------------------------
    /// Drop files on the window, the way the shell reports one: a `rich://file-drag` enter and
    /// leave, then `rich://file-drop` with a drop number and names only. Each file is
    /// `{ name, bytes: number[] }` or `{ name, folder: true }`.
    dropFiles(files) {
      const id = ++mockAttach.nextDrop;
      mockAttach.drops.set(id, files.map((f) => ({ name: f.name, folder: !!f.folder, bytes: Uint8Array.from(f.bytes || []) })));
      emit("rich://file-drag", { phase: "enter", count: files.length });
      emit("rich://file-drag", { phase: "leave", count: 0 });
      emit("rich://file-drop", { drop: id, files: files.map((f, index) => ({ index, name: f.name })) });
      return id;
    },
    /// A drag hovering over the window, without landing.
    dragOver(on) {
      emit("rich://file-drag", on ? { phase: "enter", count: 1 } : { phase: "leave", count: 0 });
    },
    /// Every attachment command the surface issued, in order.
    attachCalls() {
      return mockAttach.calls.map((c) => ({ ...c }));
    },
    /// The next `commit_attachments` answers these ids as no longer held (the sweep, or room
    /// made for newer files), exactly as the desk's `Missing` does.
    attachMissingNext(ids) {
      mockAttach.missingNext = ids.slice();
    },
    /// How many files the mock desk holds un-sent.
    attachStagedCount() {
      return mockAttach.staged.size;
    },
    // ---- the update path (RICH-TODOs row 12) ------------------------------------------
    /// Put the panel into a state, and optionally queue what the NEXT command returns.
    /// Emits `rich://update` as well as setting the value, because the real shell reaches
    /// the UI both ways and a suite must be able to prove the event path works on its own.
    updateSet(view, script) {
      mockUpdate.view = view;
      mockUpdate.script = Array.isArray(script) ? script.slice() : [];
      const subs = listeners["rich://update"];
      if (subs) subs.forEach((cb) => cb({ payload: { ...view } }));
      return { ...mockUpdate.view };
    },
    /// Which update commands the surface actually issued, in order. Asserting on this is
    /// how a suite proves the Restart button restarts rather than merely looking pressed.
    updateCalls() { return mockUpdate.calls.slice(); },
    /// ---- getting the speech model (2026-09-17) ---------------------------------------
    /// Put `voice_readiness` into the model-missing state with a given offer, or `null` to
    /// go back to the preview's ordinary "voice needs the desktop app" answer.
    ///
    ///     __RICHOS_MOCK__.voiceOfferSet({ sizeLabel: "487.6 MB", enoughRoom: false, ... });
    voiceOfferSet(offer) {
      mockVoiceModel.offer = offer || null;
      return mockVoiceModel.offer;
    },
    /// Emit one `rich://voice-model` event, exactly as the shell emits it. Phases:
    /// started | progress | verifying | installed | failed.
    ///
    /// `installed` also flips what `voice_readiness` answers, because that is what installing a
    /// model does on a real machine.
    voiceModelEmit(payload) {
      if (payload && payload.phase === "installed") mockVoiceModel.installed = true;
      const subs = listeners["rich://voice-model"];
      if (subs) subs.forEach((cb) => cb({ payload: { ...payload } }));
      return { ...payload };
    },
    /// Emit one `richos://setup` event, exactly as the shell emits it: the video tools'
    /// background download (`setup_view::start_video_tools_download`) is
    /// `{state, component: "media-tools", what, ...}` with state started | done | failed.
    setupEmit(payload) {
      // The status follows the download, as `setup_status` re-read from disk would.
      if (payload && payload.component === "media-tools") {
        if (payload.state === "done") setupVideoTools = "present";
        else if (payload.state === "failed") setupVideoTools = "missing";
        else if (payload.state === "started" && setupVideoTools === "missing") setupVideoTools = "downloading";
      }
      emit("richos://setup", { ...payload });
      return { ...payload };
    },
    /// Freeze the setup run after each step's "started" event, or (false) let it go on.
    setupHold(on) {
      setupHeld = !!on;
      if (!setupHeld && setupReleaseGate) { const g = setupReleaseGate; setupReleaseGate = null; g(); }
    },
    /// Let a frozen run past the step it is held at; the next step freezes again while held.
    setupRelease() {
      if (setupReleaseGate) { const g = setupReleaseGate; setupReleaseGate = null; g(); }
    },
    /// Rich's offer of dictation: every `dictation_offer` / `dictation_offer_shown` call, in order.
    dictationOfferCalls() { return dictationOfferCalls.slice(); },
    /// The Dictation sheet's calls, in order, each with its arguments.
    dictationCalls() { return dictationCalls.slice(); },
    /// What macOS or the tool says now (a prompt answered, System Settings, another app hiding
    /// keys, another copy's tool): merged into `dictation_status`, and the window told.
    dictationSet(patch) { Object.assign(dictationView, patch || {}); emit("rich://dictation", { changed: true }); },
    /// The tool's key tap answered "Press a different key" with F`n`.
    dictationKey(n) { emit("rich://dictation", { key: n }); },
    /// What the Mac reports about Codex now: "ready", "signedout" or "missing". The row reads it
    /// when Settings next opens, as the app's does.
    codexSet(codex) { codexReviews.codex = codex; },
    /// The Codex switch's calls, in order, each with its arguments.
    codexReviewCalls() { return codexReviewCalls.slice(); },
    /// Token spend (spend.js): a request on record now; the first count done; the agents working.
    spendLand(r) { spendMock.land(r); },
    spendReady() { spendMock.state.ready = true; },
    spendAgents(n) { spendMock.state.agents = n; },
    spendRows() { return spendMock.rows(); },
    spendCalls() { return spendMock.state.calls.slice(); },
    /// Which provisioning commands the surface issued, in order.
    voiceModelCalls() { return mockVoiceModel.calls.slice(); },
    /// Make `provision_speech_model` reject with a sentence, the way the real command does
    /// when there is no decoder, no pin, or nowhere to put the file.
    voiceModelRejectWith(message) { mockVoiceModel.rejectWith = message || null; },
    /// ---- his own loro on the home screen (2026-09-06) ---------------------------------
    /// Give `home_field_data` a corpus to answer with, or `null` to go back to answering
    /// that there is none. The default is NONE, so a suite that wants his loro on the screen
    /// has to ask for it — the demonstration is the shipped first-run experience and no
    /// preview may quietly leave it.
    ///
    ///     __RICHOS_MOCK__.homeFieldSet({ objects: 4000, records: 9000 });
    ///
    /// `objects` is the only number `home.js`'s preference reads.
    ///
    /// THIS SETTER CANNOT BEAT THE FIELD'S OWN START, and a caller that needs the answer in
    /// place for the launch itself wants `__RICHOS_MOCK_PRESET__.homeField` instead — the
    /// page asks on its own idle callback ~260ms after a harness gets its first turn, which
    /// is a margin, not a guarantee. Use this one to change the answer for an EXPLICIT
    /// `askForCustomerField()` call, which is what it is exact for.
    homeFieldSet(field) {
      homeField = field ? { ...field } : null;
      return homeField;
    },
    /// A hire lands on disk mid-session: what `home_team` answers from now on, the way
    /// `preset.homeTeam` sets it before launch (product hunt 2026-10-15, finding 7).
    ///
    ///     __RICHOS_MOCK__.homeTeamSet([{ name: "Will", role: "Marketing Director" }]);
    homeTeamSet(team) {
      homeTeam = Array.isArray(team) ? team.map((t) => ({ ...t })) : [];
      return homeTeam.map((t) => ({ ...t }));
    },
    /// ---- the first-run notice (2026-09-06) --------------------------------------------
    /// Put onboarding into a state. The default is `described`, so a suite that wants the
    /// offer on screen has to ask for it — the surface can never be shown to be working by
    /// a fixture that was already showing it.
    ///
    ///     __RICHOS_MOCK__.onboardingSet("not-yet");
    ///
    /// `no-central-folder` | `not-yet` | `declined` | `described` | `unusable`, the five the
    /// backend can answer. A caller that needs the state in place for the LAUNCH itself wants
    /// `__RICHOS_MOCK_PRESET__.onboarding` instead.
    onboardingSet(next) {
      const entityId = onboardingViewOf().entityId;
      if (entityId) onboardingByEntity.set(entityId, next);
      else onboardingState = next;
      return onboardingViewOf();
    },
    /// Every onboarding command the surface issued, in order. Asserting on this is how a
    /// suite proves "Not now" actually recorded something rather than merely looking pressed
    /// — the exact failure the command it calls was written to end.
    onboardingCalls() { return onboardingCalls.slice(); },
    /// Lets a "Not now" held by `onboardingDeclineHold` answer, and every later one at once.
    releaseDecline() {
      const held = onboardingDeclineHeld || [];
      onboardingDeclineHeld = null;
      held.forEach((r) => r());
    },
    /// ---- what did not load out of the ledger (2026-09-05) -----------------------------
    /// Put the history notice into a state. The default is CLEAN, so a suite that wants the
    /// notice on screen has to ask for it — the surface can never be shown to be working by
    /// a fixture that was already showing it.
    historySet(health) {
      Object.assign(mockHistoryHealth, health);
      return { ...mockHistoryHealth };
    },
    /// Every `launch_state` read and every splash id handed to the recency ring, in order.
    /// The suite asserts against these rather than against a re-derived count — see the
    /// note over `launchCalls`.
    launchCalls() { return { splashShown: launchCalls.splashShown.slice(), stateReads: launchCalls.stateReads.slice() }; },
    // ---- techy mode, driven the way the CEO drives it and the way the OS breaks it -----
    /// Make one thread's machinery unreadable — a real `chmod 000` in the product, a set
    /// membership here. The point of the control is that "unreadable" must never be served
    /// as "empty", so a test needs to be able to produce it.
    breakMachinery(threadId) { machineryUnreadable.add(threadId); },
    healMachinery(threadId) { machineryUnreadable.delete(threadId); },
    /// Evict one row's Tier-B payload, the way `evict_raw` does — an unlink of the sibling.
    /// The normalized record is untouched, so the row still renders.
    evictRaw(machineryId) { machineryRaw.delete(machineryId); },
    techyState() {
      return {
        default: techyDefault,
        companies: Object.fromEntries(techyEntities),
        threads: Object.fromEntries(techyThreads),
      };
    },
    /// Pin a company directly, for a check that needs the middle tier already set before
    /// the CEO's hand arrives — the "he pinned this company last week" starting state.
    setTechyCompany(entityId, enabled) { techyEntities.set(entityId, !!enabled); },
    TECHY_NOTHING_RECORDED,
    TECHY_NOT_RETAINED,
    TECHY_UNREADABLE,
    TECHY_RAW_NOT_RETAINED,
    TECHY_RAW_TRUNCATED,
    TECHY_BETWEEN_TURNS_QUIET,
    /// Empty ONE thread's between-turn lane, so a suite can drive the honest empty state on
    /// a thread that had traffic a moment ago — the state a CEO reaches by opening an older
    /// conversation, not a separate fixture pretending to be one.
    clearBetweenTurns(threadId) { betweenTurnsByThread.delete(threadId); },
    /// §26's `memory-strategy` scenario. See the block above it for what it refuses to do.
    memoryStrategy,
    /// Set the injected clock anchor BEFORE the CEO presses Enter. `Date.now() - 18600`
    /// puts the turn 18s past its lease handoff, so the row reads `Working for 18s` with
    /// no waiting and no patched `Date.now` — the renderer derives it from the timestamp,
    /// which is what §6.2 asks for.
    setMemoryStrategyAnchor(ms) { memoryStrategyAnchor = ms; },
    /// The driver for the scenario the CEO's send started, so a test can walk steps 3-16.
    activeMemoryStrategy() { return memoryScenario; },
    MEMORY_STRATEGY_STEPS,
    MEMORY_STRATEGY_ACTIVE_MS: MS_ACTIVE,
    MEMORY_STRATEGY_LEASE_MS: MS_LEASE_HANDOFF,
    MEMORY_STRATEGY_PROMPT: MEMORY_PROMPT,
    simulateProactiveDigest(threadId = "general") {
      const turnId = uid("turn");
      const msg = {
        role: "assistant",
        text: "Morning. Three things when you have a moment —\n  • Launch plan's ready for your sign-off.\n  • Finance found a gap in the Q4 forecast; I've got them digging.\n  • Partnerships wants a call on the Acme economics.\nNo rush on any of these. Say the word and I'll take each one.",
        turn_id: turnId,
        at: now(),
      };
      messagesByThread[threadId].push(msg);
      emit("rich://mock-proactive", { threadId, message: msg, tier: "digest" });
    },
    /// ROUND 18: one of Rich's lines about Claude accounts, as the back end writes it: a
    /// proactive turn (`raise_proactive`), recorded by kind (`Accounts::noted`) so the
    /// conversation draws its buttons. `kind` is "nudge", "switched" or "bothNear".
    simulateAccountNote(kind, text, threadId) {
      const thread = threadId || threads[0].id;
      const turnId = uid("turn"), at = now();
      const t = threads.find((x) => x.id === thread);
      turnsById.set(turnId, { threadId: thread, entityId: t ? t.entity_id : null, userText: null, proactive: true,
        runs: [{ text, startSeq: 0, at }], state: "completed", createdAt: at, startedAt: at, endedAt: at, activities: [] });
      mockNotes.push({ turn: turnId, kind });
      if (kind === "nudge" && !mockNudge) mockNudge = { at, turn: turnId };
      emit("rich://proactive-message", { threadId: thread, turnId, tier: "digest", at });
      return turnId;
    },
    /// ROUND 18: an automatic switch has just happened (the record's `lastSwitch` and its Recent
    /// changes row), as `claude_accounts.rs switch_to` leaves it.
    simulateAccountSwitch(fromId, toId, why, used) {
      const from = mockAccounts.find(a => a.id === fromId), to = mockAccounts.find(a => a.id === toId);
      for (const a of mockAccounts) a.inUse = a.id === toId;
      mockLastSwitch = { from: fromId, to: toId, at: Date.now(), why, used };
      changed("switched", toId, to.label, { from: from.label, why, used });
    },
    simulateProactiveInterrupt(threadId = "acme") {
      const turnId = uid("turn");
      const msg = {
        role: "assistant",
        text: "The Acme counter-offer expires at noon and I need your walk-away number before I respond. What's the floor?",
        turn_id: turnId,
        at: now(),
      };
      messagesByThread[threadId].push(msg);
      emit("rich://mock-proactive", { threadId, message: msg, tier: "interrupt" });
    },
    simulateDrillChip(threadId = "acme") {
      emit("rich://mock-worker-status", {
        threadId,
        items: [
          { label: "pulling comparables", state: "active" },
          { label: "drafting the counter", state: "active" },
          { label: "pulled Q3 economics", state: "done" },
          { label: "walk-away price", state: "needs_you" },
        ],
      });
    },
    clearDrillChip(threadId = "acme") {
      emit("rich://mock-worker-status", { threadId, items: [] });
    },
    /// Start a turn in a thread OTHER than the selected one, so §25's "a working thread
    /// remains visibly active while another thread is selected" can be exercised without a
    /// live compute lease. It runs the SAME `simulateTurn` a real send runs — the rail's
    /// mark comes from the ordinary `rich://turn-started` event, not from a special path.
    simulateBackgroundTurn(threadId = "hiring", text = "run the numbers again") {
      simulateTurn(threadId, text);
    },
    /// A turn with a REPLY LONG ENOUGH TO WATCH. The canned replies stream in about two
    /// seconds, which is shorter than the §25 working-state checks need — this exists so
    /// "updates once per second" and "survives navigation" can be observed against a live
    /// turn rather than asserted about one that already finished.
    simulateSlowTurn(threadId, text, words) {
      const n = words || 90;
      const reply = Array.from({ length: n }, function (_, i) {
        return ["pulling", "the", "comparables", "now", "and", "checking", "each", "line"][i % 8];
      }).join(" ");
      simulateTurn(threadId || activeThreadId, text || "run the numbers", { reply: reply });
    },
    /// A MID-TURN CRASH and its automatic replay. The crashed turn emits `recovering`; the
    /// replacement's `queued` carries `supersedesTurnId`. THE PROOF: the CEO's one prompt
    /// must appear exactly ONCE when this finishes.
    simulateMidTurnCrash(threadId, text) {
      simulateTurn(threadId || activeThreadId, text || "check the Acme numbers again", {
        crashAt: 3,
        reply: "Picked it straight back up — the comparables hold and the counter stands.",
      });
    },
    setNotConnected(v) {
      window.__RICHOS_MOCK__._notConnected = v;
    },
    _notConnected: false,

    // ---- which company this copy of Rich works for ---------------------------------
    /// `null` is the LAUNCH STATE: no company chosen, so the app asks. It is what every
    /// double-clicked bundle is in until the CEO answers once, and it must be drivable
    /// here because a unit test could not produce it and a real Finder launch is not a
    /// browser fixture.
    setCompanyChosen(id) {
      chosenEntityId = id || null;
      chosenEntitySource = id ? "saved-choice" : null;
      if (!id) activeThreadId = null;
    },
    /// The operator's variant: `RICHOS_ENTITY` decided it, nothing in the window can move
    /// it, so the settings row renders a statement rather than a dead control.
    setCompanyPinnedByEnvironment(v) {
      entityPinnedByEnvironment = v !== false;
      if (entityPinnedByEnvironment) chosenEntitySource = "environment";
    },

    // ---- the two correction desks --------------------------------------------------
    /// `loro_available` / `spoken_corrections_available` are SEPARATE FACTS about one
    /// install, and each one false is an ordinary machine (no corpus, no service binary),
    /// not a fault. Driving them independently is how the surface's "state the reason"
    /// path gets exercised without deleting anything from disk.
    setLoroAvailable(v) { loroDeskOn = v !== false; },
    setSpokenCorrectionsAvailable(v) { spokenDeskOn = v !== false; },
    /// A desk that IS there and refuses to answer — a poisoned lock, an unreadable log.
    /// Different from the above and rendered differently, because this one has a retry.
    setLoroReadFailure(message) { loroReadFailure = message || null; },
    setSpokenReadFailure(message) { spokenReadFailure = message || null; },
    /// The writer refused the confirmed write (exit 5, "that is a PROSE section"). The
    /// proposal lands in `failed` with the reason kept, exactly as `correction.rs` does.
    setLoroWriterRefusal(message) { window.__RICHOS_MOCK__._loroWriterRefusal = message || null; },
    _loroWriterRefusal: null,
    /// `StagingError::NoVocabulary`: a confirm with no service configured, which must
    /// refuse loudly rather than report a term learned that nothing wrote.
    setNoVocabulary(v) { window.__RICHOS_MOCK__._noVocabulary = v === true; },
    _noVocabulary: false,
    /// `changed: false` — the vocabulary already knew the pair. A different fact from a
    /// refusal, and the CEO is entitled to both (`staging.rs:141-144`).
    setVocabularyAlreadyKnew(v) { window.__RICHOS_MOCK__._vocabularyAlreadyKnew = v === true; },
    _vocabularyAlreadyKnew: false,
    /// The staging trigger firing mid-turn — `TauriCorrectionEmitter` -> the webview.
    /// `withheld` carries the repeats that were NOT staged because the pair is suppressed;
    /// they are reported rather than dropped (`staging.rs`, `Withheld`).
    stageSpokenCorrection(candidate) {
      const c = Object.assign(
        {
          key: "loro|Loro",
          at: now(),
          threadId: activeThreadId,
          turnId: uid("turn"),
          utterance: "it's Loro, not loro",
          ask: { from: "loro", to: "Loro", key: "loro|Loro", frame: "pivot-first", orthographic: 0.8, phonetic: 1, leg: "both", anchor: null },
          declinedBefore: 0,
          prompt: 'Add "Loro" to your vocabulary?',
        },
        candidate || {}
      );
      if (spokenSuppressed.includes(c.key)) {
        emit("rich://correction-staged", { candidates: [], withheld: [{ key: c.key, from: c.ask.from, to: c.ask.to, reason: "permanently declined" }] });
        return null;
      }
      c.declinedBefore = declinedCounts[c.key] || 0;
      if (c.declinedBefore > 0) c.prompt = 'Add "' + c.ask.to + '" to your vocabulary? (you corrected this before)';
      candidates.push(c);
      emit("rich://correction-staged", { candidates: [c], withheld: [] });
      return c;
    },
    /// Replace the loro desk's proposals with EXACTLY these objects, byte for byte.
    ///
    /// The one seeding hook that does NOT compose anything, and that is the point: the
    /// browser suite feeds it `ui/tests/fixtures/loro-proposal.json`, which is the proposal
    /// `belief.rs` ACTUALLY files, checked against the Rust detector on every run by
    /// `belief_trigger_tests::the_ui_fixture_is_the_proposal_the_detector_really_files`. A
    /// hook that filled in its own `preview` — as `loro_propose_correction` does, because a
    /// composer would have to — would let the screenshot show a card the backend would
    /// never produce.
    seedLoroProposals(list) {
      proposals.length = 0;
      for (const p of list || []) proposals.push(p);
      proposalSeq = proposals.length + 1;
    },
    /// Read-only views, so a test can assert on the DESK rather than only on the DOM.
    correctionDeskState() {
      return {
        loroAvailable: loroDeskOn,
        spokenAvailable: spokenDeskOn,
        proposals: proposals.map((p) => ({ id: p.id, state: p.state })),
        candidates: candidates.map((c) => c.key),
        loroSuppressed: loroSuppressed.slice(),
        spokenSuppressed: spokenSuppressed.slice(),
        declinedCounts: Object.assign({}, declinedCounts),
      };
    },
    LORO_DESK_ABSENT,
    SPOKEN_DESK_ABSENT,

    // ---- the feedback channel --------------------------------------------------------
    /// The one file would not open. Every command then refuses with the backend's own
    /// sentence — a DIFFERENT fact from an empty history, and the surface must not collapse
    /// the two into one empty list.
    setFeedbackAvailable(v) { feedbackStoreOpen = v !== false; },
    /// The store IS there and a read refused. Transient, and it has a retry — the same
    /// distinction both correction desks draw.
    setFeedbackReadFailure(message) { feedbackReadFailure = message || null; },
    /// Answers already on this machine, byte for byte. Used by the browser suite to render
    /// entries that a cargo test wrote from the real types, so a screenshot cannot show a
    /// record shape the backend would never produce.
    seedFeedbackEntries(list) {
      feedbackEntries.length = 0;
      for (const e of list || []) feedbackEntries.push(e);
    },
    /// Read-only view, so a test can assert on the STORE rather than only on the DOM.
    feedbackStoreState() {
      return {
        available: feedbackStoreOpen,
        entries: feedbackEntries.map((e) => JSON.parse(JSON.stringify(e))),
      };
    },
    /// What this harness believes the product says. `feedback.js` joins every one of these
    /// to a fixture a cargo test regenerates from the live Rust constants.
    feedbackVocabulary() {
      return {
        question: FEEDBACK_QUESTION,
        options: FEEDBACK_OPTIONS,
        reportOffer: FEEDBACK_REPORT_OFFER,
        disclosureHeading: FEEDBACK_DISCLOSURE_HEADING,
        ratings: FEEDBACK_RATINGS.map((r) => ({ ...r })),
        failureClass: FEEDBACK_FAILURE_CLASS.map((t) => ({ ...t })),
        occurrences: FEEDBACK_OCCURRENCES.map((t) => ({ ...t })),
        diagnosis: FEEDBACK_DIAGNOSIS.map((t) => ({ ...t })),
        conditions: FEEDBACK_CONDITIONS.map((t) => ({ ...t })),
      };
    },
    FEEDBACK_STORE_ABSENT,
    FEEDBACK_PREVIEW_MISMATCH,
  };
})();
