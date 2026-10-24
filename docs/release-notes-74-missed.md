# Nightly 74: the lines its entry missed

Draft for the lead. The published entry for `1.2.3-nightly.20261024.74` is unchanged; where these lines are published is the lead's decision.

- Range: `fa6892a88` (nightly 71, the previous published nightly) to `95b10c215` (nightly 74's source). Every fact below was read at `95b10c215` with `git show 95b10c215:<path>` and `git diff fa6892a88 95b10c215`.
- Why they were missed: the changelog instructions said a user never sees "the tooling that runs the team behind the product (the engine, guards, hooks, agent definitions)", so every engine file was left out except the three behind line 3. That rule is corrected in `richos/app/scripts/changelog-instructions.md` on this branch.
- Line 3 (work left on hold or finished but not merged) is not redone. The engine changes that belong to it are listed under "Folded into lines already published".

## What the app runs of the engine

Every release carries the whole engine, but the app runs only part of it. Each line below is traced to that part and anything outside it reaches no user.

- The app starts Claude with `--setting-sources ''` (`native.rs` `child_args`), so the development team's own settings and the engine's terminal hook list (`engine/hooks/hooks.json`) never load in the app. The only engine plugin is the one the app writes for each session (`engine_profile.rs` `prepare`).
- That plugin runs `scripts/app-engine-hook.py` on nine events, four command checks (`guard-interactive-prompt.sh`, `guard-no-shell-c.sh`, `shell-evidence.sh`, `guard-foreign-app-data.sh`) and the spawn checks marked `user-work` in `spawn-guard-audience.declaration`, including the brief inspector (`guard-pierce.sh`).
- `app-engine-hook.py` calls Mega Lander (`mega-lander/app.py`, `workspaces.py`), the deferral notice (`notice-unasked-deferral.sh`), the record check (`ass-kicker/ass-kicker.py`) and the review notice (`scripts/lib/app_review.py`).
- The app also starts these itself: Stu's stall watcher (`stall-watch.sh --monitor`, `native.rs` `StallWatch`), the review watcher (`review_watch.py --app-state`, `review_watch.rs`) and the question-card checks (`guard-ceo-ruled-ask.sh`, `notice-ceo-asks.sh`, `question_engine.rs`).
- It also gives every user the five always-active teammates and the shelf of ready-made teammates (`engine_profile.rs` `ALWAYS_ACTIVE`, `engine/team/shelf/`) and copies the shipped `orchestration.config` into each company's coordination folder.

## (a) Lines an ordinary user would notice

- Improved Stu's reminder about a teammate's finished work that is not yet merged: it now comes as soon as he is free and every 30 minutes until the work is handled, instead of after about 12 hours

  - Before: Stu's stall watcher (run by the app for every Stu session) reminded him of finished work still waiting only about every 12 hours (`stall_watch.py` at `fa6892a88`: `PENDING_WORK_SECONDS = 12 * 60 * 60`, "UNRESOLVED FINISHED WORK: about every twelve hours"). The app walk `testvm/stu-reminder-walk.py` at `fa6892a88` says so for the regular app ("12 hours otherwise"). Held work with no end was told only after 30 minutes.
  - Now: the watcher reads the due work (`workspaces.py` `due_set`: finished work with no wait, no continuation and no merge under way and held work with no checkable end or whose end came true) at every look while Stu is idle. It looks every 60 seconds (`POLL_SECONDS`), shows at most eight items and one pointer and repeats every 30 minutes (`REPEAT_MINUTES`) while the item stays due. Work that stops being due ends without a line. Each look loads the engine's code fresh, so an updated engine is used without restarting Stu.
  - Reaches the user: the watcher's line goes to Stu's inbox (`session_tools.rs` `observe_stall`), which starts a Stu turn.
  - Evidence: commit `077b9bc72` (merged in `af3f467c4`); `stall-watch-reminder.test.sh` (6 failures and 1 error at its base, 10 of 10 after, per the commit); `stall-watch.test.sh` HD0 to HD5 and T2 to T5; `stu-reminder-walk.py` now looks for the `[DUE]` line. The watcher's own reliability, also in `af3f467c4`: `f883a4755` (fresh code at every look), `1ec7ed138`, `0b6fddbbc`, `3af441d16`, `ec1627475`, `e8c8c57bc`. Each due item told once per change, merged in `61861453f`: `c1ced55ee`, `ef74f415b`, `fbc338fba`, `e566afeba`, `2b262cc43`.
  - Limit: only Stu has a stall watcher (`StallWatch::start` returns nothing for Rich's session). Rich's side of the same work is line 3.

- Fixed Rich saying he had saved or written something down when he had not: he is now told as he finishes and either saves it or tells you it was not saved

  - Before: the app ran the record check at the end of each of Rich's turns, but its alert was printed as a bare line (`app-engine-hook.py` at `fa6892a88`, `run(... guard-stated-actions.py)` copying its output unchanged). Claude Code shows a turn-end hook's plain output to nobody (`scripts/lib/stop-hook-notice.sh`, the measurement on Claude Code 2.1.251), so the alert was lost.
  - Now: `app-engine-hook.py` picks out the `RICHOS_RECORD_ALERT` line and sends it to Rich as part of one turn-end message (`hookSpecificOutput.additionalContext` on `Stop`). Claude Code's hooks documentation says this text goes into Claude's context and the conversation continues so Claude can act on it. The alert quotes his sentence and says nothing was saved, then: "write it to the record now (a repository file, committed) or say it was not recorded" (`ass-kicker.py` `arm3_alert_line`). It fires when a reply says he recorded, saved, wrote down, logged or filed something or put it on the follow-up list and the turn wrote it nowhere lasting.
  - Reaches the user: Rich's reply in the conversation. It runs only while the turn may act (`actions_allowed`). It runs on Stu's turns too and only Rich's replies reach the user directly.
  - Evidence: `1b00c6026` and `da6011bf7` (the follow-up list claims, merged in `33bc430de`), `c137d0ab8` (renamed to `ass-kicker.py`, merged in `ccb764185`), `2f2286cbf` (the single turn-end message), `348ecf63b` and `0cbe9c573` (a claim that cites an earlier record), the last three merged in `af3f467c4`. The check's own suites: `ass-kicker/tests/ass-kicker.test.sh`, `record-claims.replay.py`. There is no app-level test that drives a record alert through `app-engine-hook.py`: the wiring was read in the code.

## (b) Lines that ship but only a technical person would care about

Each line is shown once as written and once with the tag the CEO may choose.

- Changed jobs that change only a project's changelog files so they get no automatic review while they run
  - `[technical]` Changed jobs that change only a project's changelog files so they get no automatic review while they run
  - Before: the app's review watcher started a review of a running job whatever files it changed.
  - Now: work whose every changed file is `docs/CHANGELOG-NIGHTLY.md`, `docs/CHANGELOG.md` or `docs/release-notes.json` is dropped from the watcher's list (`review_watch.py` `Watcher`, `items = [it for it in items if not notes_only(it)]`; `operator_fences.py` `RELEASE_NOTES`, `notes_only`). The app's watcher uses the same `Watcher` (`review_watch.py` `--app-state`, `AppWorld`). The review that merging requires (`integrate` needs a passing reviewer) is unchanged.
  - Evidence: `c8bab74fa` (merged in `e0d960c6b`).
  - Limit: the three paths are fixed names, so in a user's project only those exact paths count.

- Changed the ready-made release notes writer on the teammate shelf to run on Opus instead of Sonnet
  - `[technical]` Changed the ready-made release notes writer on the teammate shelf to run on Opus instead of Sonnet
  - Before: `engine/team/shelf/newt.md` said `model: sonnet`.
  - Now: `model: opus`. The shelf is what the app's team tool lists and copies when a user adds a teammate (`app.py` `team`; `engine_profile.rs`: "The shelf itself (`engine/team/shelf/`) is never registered").
  - Evidence: `3fc99e27c` and `b8639bc32` (merged in `1783896ea`).
  - Limit: a copy the user already added keeps its own model; only a newly added copy gets Opus.

- Improved the check that reads each job before a teammate starts it: it now reads the whole job first and no longer turns a job back over a contradiction it misread or a setting it could not see
  - `[technical]` Improved the check that reads each job before a teammate starts it: it now reads the whole job first and no longer turns a job back over a contradiction it misread or a setting it could not see
  - Before: the brief inspector, which the app runs on every job before a teammate starts (`app-engine-hook.py`, `guard-pierce.sh`; always active, `engine_profile.rs` `ALWAYS_ACTIVE`), had no rule about reading the whole job before alleging a contradiction. A finding turns the job back (`dispatch_intent(... review_refused=True)`).
  - Now: its rules (`pierce.py` `REVIEW_RULES` and `engine/agents/pierce.md`) say to read the whole job first, to let an explicit narrow constraint govern broader shorthand, not to require one worker to own every part of the request and, when a claim depends on settings or state it cannot read, to note the gap rather than refuse.
  - Evidence: `873ec8189` (merged in `bd38371cc`).
  - Limit: these are instructions to a model. How often a job is no longer turned back is not measured.

- Fixed a teammate's ordinary command being refused as a hidden shell command when a folder name in it started with a dash, as Claude's own temporary folders do
  - `[technical]` Fixed a teammate's ordinary command being refused as a hidden shell command when a folder name in it started with a dash, as Claude's own temporary folders do
  - Before: the app's command check refuses a shell started with an inline script (`bash -c`). It read any dash-led word containing a "c" after the word `bash`, `sh` or `zsh` as that option, including a path piece such as `/-Users-<user>-<project>/`, which is the shape of Claude Code's session temporary folder (`shell_c.py` at `fa6892a88`, `OPTION_WORD`).
  - Now: an option counts only where it starts a word of the command, never in the middle of a word or after a `/` (`shell_c.py` `OPTION_WORD`).
  - Evidence: `84d7a1e76` and `c5fa2072c` (merged in `ec61433f8`); `guard-no-shell-c.test.sh`.

## Folded into lines already published

- Line 3, work left on hold or finished but not merged: the deferral notice now reaches Rich at his turn end in the same message, once per quote and records nothing (`2f2286cbf`, `bb7f4bec8`, `5e0198c8d`, `b591a1b8b`, `f8067df39`). Before, a reply that put work off opened a hold. A reply saying nothing is on hold no longer counts as one (`7f476dd0f`, `fd2354683`, `hold_words.py` `held`). Waits now need an end that can be checked (`2760d3e0c`). Teammates' words no longer create work (`dfa10716d`). Stopped work is offered only setting aside and dropping (`92f6df700`, `abcb35c80`, `1122f2ff3`). A new session opens with "DUE WORK (N)" (`2f2286cbf`) and Rich's turn-end fallback says nothing when nothing is due (`c68788ad7`, `01f1e19c8`).
- Line 4, a job sitting still after Stu added a teammate: the team tool's description and `DESKTOP.md` now tell Stu that a `team_reloaded` message means carry on with the work that waited (`6a45d9e6c`, merged in `d7d06ffc7` with the line's own fix).

## (c) Engine changes that never reach a user, with the reason

- Turn-end refusal removed (`2f2286cbf`: `guard-workspace-gate.sh`, `gate_stop`, the answer allowance): this is a terminal hook. The app's turn end never ran it (`app-engine-hook.py` `Stop` at `fa6892a88` ran only the deferral and record checks).
- The cleanup alert and disk notices (`048691a91`, `95b10c215`: `scratch-reaper.py`, `scratch-reaper.sh`, `disk-watchdog.py`, `disk-watchdog.sh`, `notice-disk-alert.sh`, `session-start-scratch.sh`, `codex_worktrees.py`, the `SCRATCH_*` settings in `orchestration.config`): the scheduled cleanup, the disk watchdog and their notices are the development Mac's. The app neither starts nor registers them (no reference in the app's code). The one reaper path the app reaches is the cleanup of the merged worker's own scratch after a merge (`workspaces.py` `land`, `sweep_scratch_after_land`). The range's changes on that path (`8a98fdcb3`, `be31fe8ee`, `30e4f755b`) change which leftover folders it keeps and no effect on anything a user sees is established.
- Asking the CEO first (`7033c8205`, `2cc9081bd`, `3569ec178`, `dd85a9873`, `ab702a9f2`: `guard-ceo-ask-first.sh` deleted, `notice-ceo-unasked.sh`, `session-start-ceo-ask.sh`, `ceo-asks-status.sh`): `guard-ceo-ask-first.sh` was an operator-session check by its own declaration and the other two are terminal hooks. The app ran none of them. The one change on the app's question-card path (`guard-ceo-ruled-ask.sh`: a question that matches a prepared item skips the premise check) reads a prepared-question list from `CEO_TODOS_REPOS`, which neither the shipped `orchestration.config` nor the app's generated one sets.
- Duplicate and stale notices (`238302c76`, `01f2412d6`; the third such merge, `61861453f`, is Stu's due-work reminder in (a)): `cpu_guard_live.py` and the land-lock notices (`7d1fdf11b`, `af64a2103`, `6c0bbca0f`) are terminal hooks the app does not run. The review watcher's "NO VERDICT TWICE" notices (`d3877eee2`, `cdb1d11d1` and the landed-work skip in `review_watch.py` `tell`) are printed to a log in the app ("its output goes to a log", `review_watch.rs`).
- Brief inspector restart authority and measurement (`bfde7e8f7`, `295bbc914`, `080644b26`, `fe4830508`, `1485b45fc`, `90d844cc7`, `ee0b06fe5`, `6149282a8`, `568ce40bc`, `6f0285987`, `pierce_shadow.py`): the restart-note reading is in the inspector's terminal branch (`pierce.py` `human_context`, the `else` path). The shadow collection never runs in the app (`shadow_model` returns nothing under `RICHOS_APP_STATE`). The app's read access is the same company folders as before (`configured_read_roots`).
- Workspace changes the app does not reach: never deleting an earlier job's unmerged work when its continuation is dropped (`07c29cf29`, `workspaces.py` `discard`). The app's Mega Lander never calls `discard` and the `workspaces.sh` commands that due-work lines name are not on the app's command path (`engine_profile.rs` `lease_path`), so whether anything in the app reaches it is not established. A release merge's temporary checkout moved out of the operator's folder (`a66d599aa`): the failure it fixes came from the RichOS repository's own test runner.
- Escalation ledger (`a1813f9fe`, `6d7802f2e`, `escalations.py`): ships with Stu's stall watcher, but the duplicate id needed one teammate to raise the same escalation twice within one second, which only a test did.
- Other operator hooks and checks: `guard-owned-state.sh` (`b60be6627`, `0aa619f56`), `turn-manifest.py` (`f121df836`, `f0247367c`), `release-land-leases.sh` (`47c48c649`), the integrity probe (`37731ccbe`), `second_review.py` follow-up filing (`008af46eb`), `ceo-asks.sh` and `strip-ack-lines.py` and `guard-model-ceiling.sh` (comments only), `monitors.json` (a terminal plugin monitor), `owned-systems.declaration`, `hooks/hooks.json` (the terminal hook list).
- Tests, test devices and records: every `*.test.*`, `*.mutation.sh` and replay file, `testdevices.py` (phone test devices), `verification-dependencies.json`, `ci-unit-weights.tsv`, the engine README and docs and `.claude/settings.local.json`.
