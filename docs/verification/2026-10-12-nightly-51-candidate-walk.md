# Walk of nightly candidate 51 (`v1.2.0-nightly.20261012.51`), 2026-10-12

Candidate 51 was built from richos main `43abf72eb` (run `20261011T233230Z-61ca3486`). Candidate
50 was walked READY (`docs/verification/2026-10-11-nightly-49-candidate-rewalk.md` and the walk
before it). Nothing from those walks was walked again. This walk covers only what a user sees that
changed since 50.

| Item | Result |
|---|---|
| 1. Request a feature, directly under Bust a bug: open it, write an idea, press **Post idea** in the real app | **PASS**. The press works. With no reporting account on the Mac, the card turns to "Waiting to post · saved on this Mac". Rich says the idea wasn't posted and is saved exactly as approved. The idea is on disk in `feature-requests/waiting/`. Nothing reached GitHub (4 of 4 runs) |
| 2. Token spend: the screen opens with figures, and the Settings row shows the screen's figure | **PASS**. Row and screen agreed to the cent in every read: $0.65 = $0.65 = $0.65 (run B) and $0.64 = $0.64, twice (run C). No figure was rounded to a whole dollar |
| 3. Not now on the first-run interview card takes effect at once | **PASS**. In all 4 runs the card was gone and the receipt was on screen at the first look after the press, 4 to 8 s later. Run C2's spine trace: the boot prime held the spine through the whole press and for 30+ s after it. On 50's code the card stayed 91 s |
| 4. Rich's model rows in Preferences: change one, close, reopen, the change is kept | **PASS**. Both rows, both themes. The toast and `config.json` agree. Changing back to Opus is kept too |
| Readable text of everything walked, WCAG AA, light and dark | **PASS**. 4.72:1 to 18.07:1, computed (table below) |

One new layout flaw was seen and is **not blocking** (D30, below): after a failed post, the
"Feature idea" title breaks onto two lines. The "Waiting to post" pill also runs 2 to 4 px past
the card's right edge.

## What changed since candidate 50, re-derived

`git log --first-parent 1388a3fdd..43abf72eb` lists 37 merges. These are the ones a user sees:

- `0e8cae794` Request a feature (item 1).
- `2e52e153a` Token spend screen, and `97b7cc5ff` its Settings row with cents (item 2).
- `527dc5acd` Not now without waiting for boot priming (item 3).
- `1bb4b6eb5` Rich's model rows in Preferences (item 4).
- `a498a1e1c` window commands no longer block a worker. It is seen here as the model save
  landing within 2 s of closing (item 4).
- `d4e069db1` Rich and Stu default to Opus at high effort. It is seen here as "Opus · high effort",
  "default", on both rows of a fresh install.
- **Not walked, outside this brief:**
  - `c1fb1b161`, the Claude accounts sheet's one-second redraw;
  - `46213036b` and `9c5f0f6dc`, Axel/Wes, which ship with enforcement off.

  The rest are engine, gate and tooling merges.

## Identity, checked before any claim: PASS

- **The zip.** `shasum -a 256` of `RichOS-1.2.0-nightly.20261012.51-macos-aarch64.zip` gives
  `552d3ca27e4b84c83a753bea4e96990793adba245173fcfa5785bd0d0c5a05e7`. That matches both of its
  `SHA256SUMS` lines (the versioned name and `RichOS-macos-aarch64.zip`).
- **The engine.** `richos-engine-1.2.0.tar.gz` gives
  `1565ba8e51dc276a1bff9da92c8d7222eacd9b24fc46309a1c88958d5634a02a`, its `SHA256SUMS` line.
- **`candidate.json`:**
  - `source_commit` `43abf72eb788c1b8c28a89ae9f0c74b8bb57e963`;
  - `run_id` `20261011T233230Z-61ca3486`;
  - `version` `1.2.0-nightly.20261012.51`;
  - `build_commit` `3c2f9276a3236bae8c69b40ca5d4fc26a8ebe31c`;
  - `checks_skipped` `[]`.
- **The build commit.** Its parent is `43abf72eb`. `git diff --stat 43abf72eb 3c2f9276a` touches
  only `nightly-build.json`, `Cargo.lock`, `Cargo.toml` and `tauri.conf.json`.
- **The running app.**
  - The guest's `app.log` reads `this app: built from 3c2f9276a3236bae8c69b40ca5d4fc26a8ebe31c`
    (run C2).
  - The Settings update line reads "You are still running RichOS 1.2.0-nightly.20261012.51."
    (`b-03-settings-menu`).
  - Every idea card reads "RichOS 1.2.0, nightly 51".

## Nothing could reach GitHub

- **No reporting token in any guest.** The keychain lookup exited 44 ("no such item") in runs A,
  B, C, C2 and D.
- **GitHub blocked before any step.** Every run pointed `api.github.com` and `github.com` at the
  guest itself in `/etc/hosts` first. A `curl` to `api.github.com` then had to fail or the run
  stopped. It failed in every run: before the walk, again right before Post idea, and at the end.
- **What the app did with the press:**
  - It read no token and answered `not-set-up`.
  - The idea file says `"attempts":1,"reason":"not-set-up","next_try_at_ms":…` (run D), five
    minutes later.
  - Each guest was deleted before that retry, and it would have met no token and no route.
- **Side effect of the block:** the Settings menu says "RichOS could not reach the update server.
  Check your internet connection." That is the walk's own block of `github.com`, not a defect.

## 1. Request a feature, and the Post idea press: PASS

**Where it is.** In Settings, "Request a feature" (lightbulb) is the row directly under "Bust a
bug!", in both themes (`a-03-settings-menu`, `b-03-settings-menu`). It is the only way in
(`settings-button.js:755`, `bug.open("idea")`; no shortcut and no other caller).

**The exchange**, in runs A (light), B (dark), C2 (light) and D (dark):

1. **The press.** It opened the "Request a feature · Running" panel. Rich asked "What would you
   like RichOS to do? Tell me in your own words, typed or out loud…", with Never mind and the box
   "Tell Rich your idea…" (`a-13-feature-panel`).
2. **The idea.** Typed into the panel's box: "It would help if Rich sent me a short summary of the
   week's conversations every Friday afternoon, so I can see what changed at a glance." Then
   Return.
3. **Rich's write-up**, after "Worked for 12s · Looked at the screen you were on · Checked the
   version":
   - the Feature idea card, "Not posted yet";
   - From "the RichOS reporting account";
   - To "RichOS on GitHub, as a post in its Discussions, under Ideas";
   - a title, The idea, Why it would help, What the user pictured, and Version;
   - the buttons **Post idea**, **Change it** and **Cancel** (`a-14-idea-card`).
   - A search of the screen for the word "issue" found nothing (runs A, B).
4. **Before the press,** `feature-requests/` did not exist in the app's data folder (run D).

**The press, then what the app showed** (runs A, B, C2, D: the same every time):

- **The card.** Its pill changed to **"Waiting to post · saved on this Mac"** within 2 to 8 s. The
  first look found it in every run.
- **The buttons.** Post idea, Change it, Cancel became **Try now**, **Change it**, **Cancel post**.
- **Rich, under the card:** "The RichOS reporting account isn't set up on this Mac yet, so your
  idea wasn't posted. Nothing is lost: it's saved on this Mac exactly as you approved it. I'll post
  it by myself once the account is set up, and tell you here when it's up." (`a-16-after-post`,
  `b-16-after-post`).
- **"Posted" appeared nowhere** on screen (runs A, B).
- **On disk** (run D, 23 s after the press):
  - `feature-requests/waiting/5f190398-83aa-4422-9983-1b0dc65f87a0.json`, 852 bytes;
  - the title Rich wrote ("Weekly Friday afternoon summary of the week's conversations") and the
    body as the card showed it;
  - `"reason":"not-set-up"`.
- **Both themes:** switching the theme with the card on screen redrew it whole (runs A and B,
  frames `a-17` and `b-17`, not committed: the same as `a-16` and `b-16` in the other theme).

**Verdict basis:** seen on the running candidate in the guest: its accessibility tree, its frames
and the guest's files. That the press never reaches GitHub without a token is read from the card,
Rich's sentence and the outbox file. It is not shown by a network capture.

## 2. Token spend: PASS

**The fixture.** Each guest home held the 14 transcript rows of
`richos-core/src/spend/fixture-requests.jsonl`, dated 2026-10-10 (by hand: $0.5234329). The
guest's own Rich then spent more today, for real.

| Run | Settings row, menu opened | The screen, This month | Settings row, opened again |
|---|---|---|---|
| A (light) | "Token spend **$0.88** this month" | **$0.88**, "Updated 1:02:06 AM", Refresh with its new-requests dot | **$0.94** (43 s later) |
| B (dark) | **$0.65** | **$0.65** | **$0.65** |
| C2 (light) | **$0.64** | **$0.64** | **$0.64**, then the screen again: **$0.64** |

- **Run A's rise from $0.88 to $0.94** is new spend landing between the two reads. The screen
  showed it as it happened: Refresh carried its dot ("new requests since this reading") in
  `a-04-spend-screen`. The row read $0.94 at its next opening. No moment showed the row and the
  screen disagreeing.
- **Every figure has cents.** None was a whole dollar. Each figure is the AXImage the screen names
  by its amount (`AXImage title='$0.64'`), so these are the app's own words.
- **The screen** (`a-04`, `b-04`, `c-05`) shows:
  - the heading "API-EQUIVALENT TOKEN SPEND · OCTOBER 2026" and the figure;
  - "What the same work would have cost at Claude's API prices. Billed by the API: nothing. It ran
    on your Claude subscription.";
  - the bar with its legend (Fable 5.1, Opus 5.5 and 5, Sonnet 5.5, Haiku 5.5 and 4.5);
  - the token and request counts, and "No agents working right now";
  - the footer.
- **Ways out:** Back and Esc both close it. Esc was used in every run.

## 3. Not now on the first-run interview card: PASS

The press came as soon as the card ("I don't know your business yet." with Start the questions and
Not now, `b-01-interview-card`) was found.

| Run | Press started | Card gone, "Left with you. Ask me any time and we'll go through it." on screen |
|---|---|---|
| A | 01:00:07Z | found at the first look, which ended 01:00:15Z |
| B | 01:00:16Z | first look, ended 01:00:23Z |
| C | 01:12:59Z | first look, ended 01:13:09Z |
| C2 | 01:14:52Z (guest clock) | first look; guest clock 01:15:01 when it returned |

**Why this proves "at once" and not "fast this time" (run C2, `RICHOS_HOP_TRACE=1`):**

- **The press fell inside the boot prime.** The app's `app.log` shows
  `spine TAKEN at src/main.rs:1219` (`prime_front_desk`) at +25 ms after launch (01:14:13Z), and
  `RELEASED … after holding 81458 ms` at +81484 ms. That puts the release at about 01:15:35Z, so
  the prime held the spine through the whole press.
- **The decline did not wait for it.**
  - The answer was on disk at 01:14:57.062: `onboarding.json` `"declined_by_entity": {"acme":
    1791767697062}`.
  - The receipt was on screen by 01:15:01.
  - Both came about 35 s before the prime let go.
  - The log has no "a window command waited … for the spine" line.
- **On 50's code** the same press waited for the prime: the card stayed 90899 ms
  (`docs/verification/2026-10-11-preferences-close-walk/README.md`, step 17, quoted in `cffc73c86`).

The resolution is the walk's own: one look at the screen takes 4 to 8 s here. The table bounds the
wait from above; it does not measure milliseconds.

## 4. Rich's model rows in Preferences: PASS

Opened with the gear at the foot of the rail. Closed with Esc.

**Run A (light), the "When he talks with you" row:**

| Step | What the user sees | `config.json` |
|---|---|---|
| Open | both rows "Opus · high effort", each marked "default" | `rich_model: opus`, `stu_model: opus` |
| Pick Fable (still open) | Fable selected. The note reads "**Rich talks with you on Fable.** Fable is more capable than Opus and uses up your Claude limits faster." (`a-07`) | still `opus`: nothing saved while open |
| Close | toast "Rich now talks with you on Fable." (`a-08`) | `rich_model: fable`, read 2 s after Esc |
| Reopen | **Fable still selected** (AXValue 1), the note as before (`a-09`) | `fable` |
| Pick Opus, close | toast "Rich talks with you on Opus at high effort again, the default." | `rich_model: opus` |
| Reopen | Opus selected on both rows | `opus` |

**Run B (dark), the "When he directs your team" row:** the same sequence.
- After the close, the toast read "Rich now directs your team on Fable." (`b-08`), and
  `config.json` read `stu_model: fable`.
- On reopening, Fable was still selected on that row, Opus · high effort "default" on the other,
  and the note read "**Rich directs your team on Fable.** … Your teammates keep their own models."
  (`b-09`).
- Back to Opus: kept the same way.

## Contrast (`qa/contrast.py`), every new element walked

All ratios are computed on the frames. Normal text is held to 4.5:1. The spend figure is held to
3:1 as large text.

| What | Light | Dark |
|---|---|---|
| Settings: "Request a feature", "Bust a bug!", "Token spend", the row's amount | 18.07:1 | 12.06:1 |
| Settings: the row's "this month" | 6.35:1 | 5.80:1 |
| Spend: title, legend names, token counts | 14.90:1 | 14.55:1 |
| Spend: "this month", "Updated…", Back, both sentences, "No agents working", footer | 5.83:1 | 6.48:1 |
| Spend: Today / This week / All time | 6.04:1 | 5.14:1 |
| Spend: This month (selected, on gold) | 4.72:1 | 7.68:1 |
| Spend: heading, legend effort lines | 5.90:1 | 5.91:1 |
| Spend: "OCTOBER 2026" (gold) | 5.48:1 | 7.68:1 |
| Spend figure: the whole dollars / the "$" and cents (large) | 14.90:1 / 5.83:1 | 14.55:1 / 6.48:1 |
| Preferences: "Rich's model" | 5.37:1 | 5.51:1 |
| Preferences: "When he talks with you", the note | 18.07:1 | 12.06:1 |
| Preferences: unselected half / selected half (on gold) | 5.96:1 / 4.72:1 | 5.02:1 / 7.68:1 |
| Preferences: "default" | 6.33:1 | 5.78:1 |
| Model toast | 18.07:1 | 12.06:1 |
| Feature panel: title / "· Running" | 17.02:1 / 6.15:1 | 13.02:1 / 6.09:1 |
| Feature card: section headings, "Version" | 5.98:1 | 6.51:1 |
| Feature card: body, bullets | 15.58:1 | 14.78:1 |
| Feature card: "Feature idea", the pill text, From, the account | 18.07:1 | 12.06:1 |
| Feature card: route lines, the "Nothing private…" line | 6.33:1 | 5.78:1 |
| Feature card: title Rich wrote (large) | 15.58:1 | 14.78:1 |
| Post idea / Try now (on gold) | 4.72:1 | 7.68:1 |
| Change it, Cancel, Cancel post | 6.33:1 | 5.78:1 |
| Rich's sentence after the press | 17.02:1 | 13.02:1 |
| "Rich" label / the box's hint "Tell Rich what to change, or post it…" | 5.27:1 / 6.33:1 | 5.68:1 / 5.78:1 |

**Not measured, declared:** the dashed outline of the "Waiting to post" pill. It is not the
indicator: the words inside it say the state, at 18.07:1 and 12.06:1. It is the same outlined style
as Bust a bug's pill.

## Seen, not blocking

### D30. After a failed post, the card's head is cramped

**What a user sees** (both themes, runs C2 and D, `d-01-card-head`, `d-02-card-head-light` and
`pill-light-crop`), when they scroll up to the card after Post idea:
- the title breaks over two lines, "Feature / idea";
- the "Waiting to post · saved on this Mac" pill's right end runs past the card's dashed edge.

**Measured:**
- The card's right border is at x 1487 (`frame.py edges --row 283|290|296`).
- The pill's outline is at x 1489 to 1491, so it overruns the border by 2 to 4 px.
- The accessibility tree gives the title 59 x 40 (two lines), against 96 x 20 before the press.
- The pill text box is 262 px wide.

**Why:**
- `.bug-pill` is `white-space: nowrap` with `margin-left: auto`, so the long waiting pill takes the
  head's width.
- The style is Bust a bug's, unchanged since `029160e80` (2026-10-09).
- So by the code, not seen, Bust a bug's "Waiting to send · saved on this Mac" card is likely
  cramped the same way.

**Why not blocking:**
- Every word stays readable.
- It shows only after a post that could not go out.
- What happened is said in full by Rich's sentence right under the card.

**Suggested fix:** let the pill wrap under the title when it does not fit, or shorten the waiting
pill.

### Other notes

- **The panel and the card say opposite things about what was left out.** Rich's line above the
  card always says "Anyone can read GitHub Discussions, so I left out names, company details and
  file paths." The card itself said "Nothing private was in it, so nothing was left out." (all 4
  runs). Both are true in their way (a policy and an outcome), but a careful reader sees "I left
  out" next to "nothing was left out". It is a copy note.
- **The floating button row covers the card's head** when the panel is scrolled to its top
  (`c-04`, not committed). The 50 walk noted the same floating row over a report card. Scrolling a
  little brings the head into view (`d-01`).
- **Rich's write-up speaks of "the user"** ("The user would like Rich to send them…"). That is how
  the post would read on GitHub. It is Rich's wording, not a screen defect.

## Verification mode

- **Environment:** the test VM, one fresh guest per run through `run-walk.py`, never this Mac's
  screen.
  - Two runs at a time (A and B), then one at a time (C, C2, D).
  - One app window per guest. No two-user test.
  - Guest screen 1680x1050 at 1x; app window 1400x864.
  - Light and dark through the guest's macOS appearance.
- **Artifact:** the candidate's own zip, with the same folder's `richos-engine-1.2.0.tar.gz` as
  `--engine`.
- **Home:** a scratch home holding only `.claude/projects/-Users-admin-Acme/fixture-session.jsonl`,
  the spend fixture.
- **Account:** the real Claude login that `run.sh` copies into each guest. No account file on this
  Mac was written.
- **GitHub:** no reporting token, `api.github.com` and `github.com` unreachable, and no post went
  out.
- **Audio:** none played or recorded.
- **Verification basis:**
  - Every PASS above was seen on the running candidate in the guest: its accessibility tree, its
    frames, the guest's `config.json`, `onboarding.json`, `feature-requests/` and `app.log`, and
    the walks' step records.
  - Lines marked "by the code" are read from source and were not seen.

### Runs

| Run | VM | Walk | Result | Seconds |
|---|---|---|---|---|
| A | `walk-ef309ce98a11` | `steps/walkA.json` | light: Not now; Token spend; Preferences (talks with you), there and back; Request a feature, Post idea, then dark. Exit 0 | 599 |
| B | `walk-60ca7b3a2d1b` | `steps/walkB.json` | the same in dark, with the "directs your team" row, then light. Exit 0 | 594 |
| C | `walk-45e959f89256` | `steps/walkC.json` (first form) | my error: a log step looked for `*.log` in the data folder (no match, exit 1) after the Not now part. Its Not now timing is row C above | 50 |
| C2 | `walk-48a5a1ab0a09` | `steps/walkC.json` | light, `RICHOS_HOP_TRACE=1`: Not now with the spine trace; Post idea; the panel's top; Token spend read twice. Exit 0 | 226 |
| D | `walk-afb72e0e3208` | `steps/walkD.json` | dark: the outbox before and after Post idea, the card's head, then light. Exit 0 | 169 |

**Cleanup.**
- Every run ended with run-walk.py's "clean: app quit, VM stopped, clone deleted, state removed."
  and released its slot ("slot released: guest.lock after 201s" for the last).
- `slots.py status` afterwards: `guest.lock: free`, `guest-2.lock: free`.
- No app instance was started on this Mac.

## Method and toolkit

The `qa/` and `testvm/` tools are those of `43abf72eb` (this branch's base). None was changed.

**`testvm/`:**
- `run-walk.py --wait 900` with `TESTVM_AX_TIMEOUT=60`, and `TESTVM_APP_ENV=RICHOS_HOP_TRACE=1` for
  C and C2;
- `steps-walk.py --steps … --out …`, and `--check` on every list;
- through it, `ax.sh` (find, click, type, `--key`), `guest.sh`, `shot.sh` and `pointer-drag.sh`
  (the gear, and the panel's scroll bar);
- `slots.py status`.

**`qa/`:**
- `contrast.py` on frame regions;
- `frame.py crop` and `frame.py edges`;
- `ocr-gate.sh` with its positive control: "0 of 19 frame(s) carry something that must not ship";
- `wait-for.sh --log` for each run's end.

**Elsewhere:** `nightly-local.py candidate`, `shasum`, `jq`, `git log`, `git diff`, `date`, and
`sips` for the JPEG copies.

**Scripts written from scratch: none committed and none run.**
- One step-list generator (`makeC.py`) was written in scratch and deleted unrun. `walkC.json` was
  written by hand instead.
- `walkB.json` was derived from `walkA.json` by an inline substitution (theme and row swapped).
- `steps.json` outputs were read with inline `python3 -c` one-liners.
- The step lists themselves are data for `steps-walk.py`, kept in `steps/`. They are added to the
  `inputs` and `covers` rows of `richos/app/scripts/steps-walk-data.test.sh`, which passes: "ok
  covers row names every committed step list", "ok every committed step list is valid", "ok
  malformed step lists fail the check".

Verdict: READY
