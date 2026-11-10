READY — nightly v1.3.1-nightly.20261110.95 (run 20261110T092909Z-1624682b): every item walked passes in the real packaged app; three non-blocking notes below.

# Nightly 95 candidate walk, 2026-11-10 (Ray)

**Verification mode.** Packaged app unpacked from the candidate zip, run only in the test VM (run-walk.py, headless, one guest slot, released at the end; `scenario_exit` 0, `cleanup_complete` true, clone deleted, app quit; no `richos-tauri` on this Mac afterward). Guest screen 1680x1050. One window. No second user. Basis for every claim below: **verified visually and by accessibility reads on the live app in the VM**, with frames kept in `docs/verification/2026-11-10-nightly-95-candidate-walk/`.

**Identity.** `--expect-sha 41fdb912dec34d263b20f82ad456904cdab32209`: the app's own log says it was built from that commit; Settings > Updates on screen reads "RichOS 1.3.1-nightly.20261110.95 is up to date" (`smoke-update-check.png`).

**Method, toolkit (`richos/app/scripts/qa/`, last commit touching it `2edbb3489`).** `contrast.py` (every ratio below, `--min-frac 0.003`, printed pair per measurement in `walk-report.txt`), `ocr-gate.sh` (frames and report text: 0 hits), `wait-for.sh`, `frame.py crop` (inside folders-walk's OCR). Scripts written from scratch: **one**, `richos/app/scripts/testvm/folder-map-walk.py`, committed with this record. It is genuinely new: no committed walk drove the Folder Map (it landed 2026-11-10). It subclasses `search-walk.py` / `folders-walk.py` and reuses their drivers. Besides that, five tiny one-off text-edit scripts in my scratch directory were used to edit that walk file (not walk helpers).

Final run: `run-walk.py --bundle <candidate zip> ... -- folder-map-walk.py --expect-sha 41fdb912...`; all 9 steps PASS (`walk-report.txt`, `run-walk-report.txt`). Four earlier runs while building the walk failed in my own driver code, never in the app.

## Items

1. **Identity: PASS.** SHA and version string as above.
2. **Settings, Search folders: PASS.** Opens from Settings ("Search folders", row says "1 search folder" with one folder ticked, "2 search folders" after two). Off: "Nothing is searched until you choose". Chosen folders: ticked Alpha, coverage "1 documents", then "2 documents" with a second folder. All eligible: caption "Everything eligible is included", "5 documents". Reindex shows "Reindex started." A folder starting with a dot (`.stash`, `.claude`, `.config`), `node_modules`, `Library`, `System`: each is a row with "Never searched · <reason>" and a **disabled** box; a click is refused ("disabled") and the box stays unticked; their notes were never indexed (the search found none of their text). Coverage says RichFinder, never Super Finder.
   Contrast (contrast.py, WCAG AA 4.5:1 normal text), final run: dark lowest 5.78:1 (All search folders), light lowest 5.98:1 (Never searched hidden-folder note); all 22 texts per theme ≥ 4.5:1. Two light entries (heading "Rich in each company" and the Limit button) read 1.41 and 4.06 in that run only because the frame still showed the card's first "1 indexing" line and the pane boxes were 32 px off; re-measured on that frame at the true boxes: 18.07:1 and 18.07:1 (see the "Contrast" note). Company pane on a settled frame, light then dark: lowest 6.33 and 5.78. No exemptions claimed.
3. **Rich in each company: PASS.** Beta limited to Alpha: "1 folder", Acme stays "All search folders". Ticking the parent (Docs): Beta still "1 folder" (box list Alpha ticked, Docs unticked), only Acme says "All search folders". Switching to All eligible: Beta's limit is **kept** ("Remove limit" offered, still only Acme says "All search folders"); see note 3 for how it reads.
4. **RichFinder: PASS.** Sources list shows Alpha 1, Bravo 1; "2 results" for the query; unchecking Alpha gives "1 result", Alpha's note gone, Bravo's stays and "Uncheck a folder to narrow this search." stays. Checkbox "RichFinder", status "RichFinder ready · 2 files"; "Super Finder" appears nowhere. The hidden and generated-output notes never answer. Contrast on the palette: light lowest 5.37:1, dark lowest 4.9:1 (DOCUMENTS label).
5. **Split reply: PASS.** The guest has no way to make Rich answer in several parts on demand, so the walk appended a synthetic turn to the ledger with the app stopped: one reply stored as two runs (sequence 1 and 3) holding "quartzmeadow" in the second part, then relaunched. The palette listed it under Conversations; opening the hit closed the palette and showed that conversation with both parts (`split-opened.png`). Honest limit: both parts fit on one screen, so this proves the hit opens the right conversation and finds the word inside a split reply, not that the view scrolled to the part.
6. **Smoke: PASS.** A message sent in a conversation received a reply (a new completed turn; guest is signed in); Settings opened; "Check for updates" pressed and answered "up to date".

## Non-blocking notes (none is a hole in normal use of the new features)

1. **A locked row with a long name garbles in the Folder Map.** Expand a folder holding `node_modules` (or `dist`, `build`, `target`): the name column collapses to one letter per line and the note "Never searched · Code dependencies or generated output" overprints it (`map-chosen-light.png`, `map-chosen-dark.png`). The row is still locked and unselectable; only its look is broken, only for those names. Cause, from `ui/search-folder-settings.css` line 34: the note column is `auto` and takes all the width, leaving the name `minmax(0,1fr)` zero. Suggested fix: give the name column `minmax(8ch,1fr)` and let the note wrap. Cheap; worth a follow-up.
2. **"1 documents"** (singular) in the coverage card with exactly one file.
3. **After All eligible, a company with a limit reads "No folders"** and its ticked folder is flagged "Outside current search folders" (`limit-after-all-light.png`). The limit is kept, which is what the fix was for and it fails closed, but the wording will puzzle someone who only meant "search everything". "Remove limit" is right there.
4. **Contrast caveat of this walk.** The modes step measures light contrast on a frame that can still show the first indexing line; the settled company and finder frames are the ones to trust (all ≥ 4.5:1). The walk comment says so.

## Surfaces applied / not applicable

Applied: Settings entrance to the Folder Map and the palette's "Change saved folders" entrance (the second was visible, the first was used for every open); all three modes and the reverse transitions (Off → Chosen → All → Off → Chosen); light and dark; first run (fresh guest) only. Not applicable: returning user after a relaunch (the relaunch for item 5 kept the choices: Settings row read "2 search folders" afterwards), phone and voice clients, `richos/mobile/**` (so the Battery check does not apply: no mobile change), network reconnect.

## Handling of test data

Synthetic folders only, inside the guest's own home. Nothing touched this Mac's home, the real app data or the screen (§65). Scratch directory deleted at the end.
