# The write-checks walk: a teammate's write meets the engine's checks in the app

One run of `richos/app/scripts/testvm/write-checks-walk.py` under `run-walk.py --wait 0` in the
test VM (guest `walk-6c1c07f72276`), 2026-10-28 10:12:57Z to 10:21:45Z. All five steps PASS,
`cleanup_complete: true` (app quit by its recorded pid, VM stopped, clone deleted).

## What ran

- **App:** `branch-bundle.sh` at `5b7af7a32` (echo-opus-dde1's branch tip), pinned to the v1.2.4
  release's engine (1.2.0, sha256 `33ada63a…eede4`, matching the release tarball). The running app
  reported `built_from: 5b7af7a325f32ba44c676a7734d9c522158e4fe6` (identity step).
- **Engine:** `make-engine-asset.sh --runtime-dir` with the v1.2.4 tarball's `engine/runtime`, built
  from this branch, so the guest ran this branch's `dispatch-pretooluse.sh`.
- **Scenario:** `write-checks-walk.py` at `1b1d85fc5`. That commit changes only the walk script and
  its test; `git diff --stat 5b7af7a32 1b1d85fc5 -- richos/app/crates richos/app/src-tauri
  richos/app/ui` is empty, so the bundle at `5b7af7a32` is the app this branch builds.

## The three results

1. **A fake key-shaped secret is refused.** walkwriter's Write of `walk-secret.txt` carrying a fake
   AWS-key-shaped string got a PreToolUse and no PostToolUse; no copy of the file holding the key is
   anywhere in the fixture home; and walkwriter's transcript holds the refusal, routed through the
   named-rule dispatcher this branch adds:
   `PreToolUse:Write hook error: [/bin/bash '…/engine/scripts/hooks/dispatch-pretooluse.sh' Write
   scan-secrets.sh guard-publication-writes.sh guard-named-persons-writes.sh]: === Secret scan
   BLOCKED === Refusing to write '…/walk-secret.txt'…`
2. **A British word is fixed to American.** walkwriter's Write of `walk-dialect-note.md` asked for
   the British spelling of "color"; `dialect-guard.sh` rewrote the input before it ran ("changed this
   write … to American spelling before it ran … (line 1)"), the write completed and the only copy on
   disk reads `The color of the walk.`
3. **A teammate with an explicit `tools:` line starts and does its small job.** walkwriter's
   definition names `Read, Write, Edit, Bash, Grep, Glob, Skill, TaskUpdate, TaskGet, TaskList,
   SendMessage`. It started (`agentType: richos-app-engine:walkwriter`, model sonnet), made 10 tool
   calls (Write, Bash), wrote the note, checked it on disk and started a commit of it in its
   worktree; the walk ended its capture while that commit's wait was still running (assignment
   state `running`), so the commit's result is not part of this record.

## The run before it and why it failed

Guest `walk-5ad2d81155e3`, 10:02:36Z: identity, first-run, connect and config PASS; checks FAIL in
the harness: `cat: …/home/Library/Application: No such file or directory`. The teammate's worktree
is under the app's `Application Support` folder and the walk split `find`'s output on whitespace.
Fixed in `1b1d85fc5` (one path per line, with a test that is red on `5b7af7a32`'s script). Its hook
record already showed the same two writes (note completed with the American word, key write asked
and never completed); this run is the one that judged them.

In both runs Stu's first `Agent` call was refused by the app engine ("prepare this assignment with
the app work tool") and Stu prepared it and launched again: the app's own assignment flow, not this
branch's change.

## Files

`report.json.txt` (the walk's per-step report), `facts.json.txt`, `run-report.json.txt` (boot,
scenario and cleanup timing; the host lock path is `<TESTVM_ROOT>`). Guest paths only. The raw
hook record and transcripts carry the fixture key string and are not committed.
