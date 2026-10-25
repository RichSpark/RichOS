# The app cleans up its own leftovers: the garbage walk on the real app (2026-10-25)

The CEO (§54 addendum 2, 2026-09-18): "The RichOS app must always clean up its garbage
automatically, no matter what." This is the one real-app check of branch
`cc/echo-opus-appgarbage2` (built from `cc/echo-opus-appgarbage1`, tip `91f589b05`, merged into
main `69ae2bf60`). It ran in the test VM, on fixture data only (company "Acme", an empty fixture
home). Nothing ran on the host's screen.

## Build identity

| What | Identity |
|---|---|
| Commit the bundle and the engine were built from | `7f2ca066227ac47e4da491181da4d9ee91e25d11` (the running app reported it: `report.json` step `identity`, `built_from`) |
| Bundle | `testvm/branch-bundle.sh` output, `RichOS.zip`, sha256 `206c78141d2d07383d245425a063648395e41f9e2b2b4cbbdea75783fdd8465e`, identity `com.richos.app 1.2.3` |
| Engine | `make-engine-asset.sh --runtime-dir <nightly state>/runtime-eb026316eb08` (the runtime cache for `runtime-sources.json` sha256 `eb026316eb08…`), `richos-engine-1.2.0.tar.gz`, sha256 `21887c433273bfb976dc39abcecb4dad8ce39a0c1978ca709ff800ab08c07b5d`, handed to the guest with `--engine` (so `RICHOS_ENGINE_DIR` names it and the app's sweep runs this branch's `scratch-reaper.py`) |
| Guest | `walk-0653463caba2`, 4 CPUs, 5120 MB (`run-report.json`) |

## Command

```
run-walk.py --bundle RichOS.zip --home <empty dir> --engine richos-engine-1.2.0.tar.gz \
  --report run-report.json --wait 1200 -- \
  richos/app/scripts/testvm/garbage-walk.py --out <dir> \
  --expect-sha 7f2ca066227ac47e4da491181da4d9ee91e25d11 \
  --gone-file richos/app/scripts/testvm/garbage-walk.gone
```

Run once. `run-report.json`: boot 120 s, scenario 908 s, `scenario_exit` 0, `cleanup_complete`
true (app quit, VM stopped, clone deleted), 1034 s in all.

## Every step (`report.json`)

| Step | Outcome | Seconds |
|---|---|---|
| identity | PASS | 0.4 |
| mark | PASS | 2.9 |
| first-run | PASS | 28.7 |
| job (a job for Stu, SETTLED) | PASS | 602.3 |
| idle | PASS | 14.9 |
| list-running | PASS | 4.7 |
| quit (the app's own Quit) | PASS | 5.6 |
| list-quit | PASS | 0.8 |
| restart | PASS | 135.9 |
| list-restart | PASS | 2.3 |
| kill (SIGKILL of the recorded pid) | PASS | 1.8 |
| list-kill | PASS | 1.3 |
| restart-after-kill | PASS | 104.3 |
| list-final | PASS | 1.6 |

The after-kill steps (restart, its listing, the kill, its listing, restart after kill, final
listing) had not passed on any build before this run.

## Before and after: what the leftover judgment saw

The `--gone` kinds are the four in `garbage-walk.gone`: Claude Code's session scratch for the
app's coordination folder, a lease's engine profile and its `-standing` copy, Claude Code's MCP
logs for that folder and the voice probe utterance.

| Judgment | Before (listing) | Paths of those kinds there before | Still there after |
|---|---|---|---|
| after the Quit (`summary-quit.json`) | `listing-running.json` | 28: 6 engine profiles (3 pairs), 20 MCP logs, 2 session scratch folders under `/private/tmp/claude-501/` | 0 |
| after the restart that follows the kill (`summary-final.json`) | `listing-kill.json` | 18: 4 engine profiles (2 pairs), 14 MCP logs | 0 |

`were_there` and `still_there` in each summary list the paths. Every listing found its positive
control and none had an unreadable root or directory, so a search that saw nothing would have
failed rather than passed.

Not exercised by this walk: the voice probe kind. No speed measurement ran, so no probe was there
to judge in either listing; the probe's cleanup is covered by `richos-voice` `stt.rs`
`a_probe_is_removed_after_every_walk_rendered_left_or_cached`.

## One observation, not a failure

After the SIGKILL, one process of the killed run was still running: `review_watch.py --monitor`,
reparented to pid 1 (`summary-kill.json` `processes`). It was gone by the final listing, after
the restart (`summary-final.json`). Nothing in this branch changes it.

## Files

Each file is the walk's own JSON output, unchanged, saved with `.txt` added to its name
(`report.json` is `report.json.txt`): it is a record, not code and `proof-for.sh` treats a
`.json` file as code that needs a suite. The names below are the walk's.

`report.json` (every step and its evidence), `run-report.json` (boot, scenario and cleanup
timing; the host lock path written as `<TESTVM_ROOT>`), `facts.json` (the mark, the roots
searched, the job), `lead-files.json`, `listing-<step>.json` (every file, link and new directory
written since the mark, with its size) and `summary-<step>.json` (grouped by place, the processes
still running and for `quit` and `final` the judgment above). All paths are the guest's. No
screenshots.
