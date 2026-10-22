# Handover: a connected folder's own CLAUDE.md / AGENTS.md (zach-opus-folderfiles1, 2026-10-21)

Branch `cc/zach-opus-folderfiles1`. Handed over UNFINISHED by the CEO's order of 23:05Z (free the
Mac for the CPU fix): the code, its tests and the VM walk are done; the final merge of main and the
branch proof run are not. Plan: the folder-instruction-files plan of 2026-10-21 in the private
record (richos-hq), §5.3 option D, items 1 to 5 (item 6, the Connect-screen line, is not built, by
the brief).

## Done

- **prepare** gives every worker and reviewer the repository's own instruction file, chosen by
  Claude Code's rule (`CLAUDE.md`, `.claude/CLAUDE.md`, then `CLAUDE.local.md` from the connected
  folder; `AGENTS.md` / `.claude/AGENTS.md` only when none of those exist), in full, every line
  quoted with `> ` so no line of it can act as a spawn control line
  (`engine/scripts/lib/instruction_files.py`, `engine/mega-lander/app.py`).
- **Never through Stu** (mid-job review finding 1): the payload Stu reads carries one placeholder
  line; `dispatch_intent` puts the section in its place once the dispatch is admitted.
- **The hook answers an Agent call with one JSON envelope** (`engine/scripts/app-engine-hook.py`):
  Claude Code 2.1.306 ignores a PreToolUse `updatedInput` when the hook printed another JSON object
  first (measured locally) and the canonical guards' output was forwarded, so the swap above and
  the receipt `name` this path always added, never reached the teammate.
- **The second review** reads the same file by the same call, at the reviewed tip, in full; a
  `CLAUDE.md` linked out of the repository now resolves the same for both (finding 2).
- **Duty text** in `worker.md`, `reviewer.md`, `consult.md`.
- **Preservation**: Rust test (connecting a folder without Git leaves every instruction file
  byte-identical) and an app test (no instruction file changes across worker, review and land).
- **Tests red before, green after**: the app cases (5 of 6 red on main on the assertion; the
  no-file guard holds on main by design), `test_stu_never_reads_the_repositorys_instruction_file`,
  second-review C23 and C24, the hook's one-envelope test.
- **VM walk** `app/scripts/testvm/folder-instructions-walk.py`, its three runs' records in
  `docs/verification/folderfiles1-walks-2026-10-21.md`:
  1. Red, main's engine. Tools job settled and its commit still ended with
     `[agents-rule]`, because Stu read AGENTS.md itself and relayed the rule; no worker or reviewer
     received the file. Acme job stalled on hiring. Escalation `esc-20261021T191249Z-bbf10a3f`.
  2. Green, unseeded, before the hook fix: Stu's payloads held the placeholder, but no agent
     received the file. This is what found the hook defect.
  3. Green, seeded, after the hook fix: **PASS on every step** at engine `c1a2ccc94`, bundle
     `4572beca9` (app code identical): both jobs settled and landed, each commit ends with its
     folder's tag, worker and reviewer in both repositories received the file, no payload Stu read
     held it, Acme's AGENTS.md was left out beside its CLAUDE.md, all 4 instruction files have
     identical sha256 before and after; guest cleaned.
- Mid-job second reviews of `1bc7d2bf1`, `4b4a54267`, `3b2df4790` and `c1a2ccc94`: PASSED. A
  non-blocking P2 is filed by the review (a `.claude` directory that is itself a symlink is not
  followed).

## Left

1. `git merge main` once in the worktree.
2. The branch proof run, once. A run on `cb0c7ae43..c1a2ccc94` was interrupted by this order
   after 3 of 55 checks passed (the last section of `folderfiles1-walks-2026-10-21.md`); HEAD
   has moved since, so it is run fresh:

   ```sh
   cd richos/app && python3 scripts/proof-run.py "$(git merge-base HEAD main)..HEAD"
   ```

The walk need not be repeated unless the merge changes `engine/` or app code: it passed on the
code this branch hands over. If it must be, seed the team first, as its usage text says:

```sh
python3 app/scripts/testvm/command-walk.py --seed-team EMPTY_HOME engine/team/shelf/norm.md engine/team/shelf/tom.md
python3 app/scripts/testvm/run-walk.py --bundle ZIP --home EMPTY_HOME --engine ENGINE --report REPORT --wait 600 -- \
    app/scripts/testvm/folder-instructions-walk.py --out DIR --expect-sha SHA
```

## Finished by zach-opus-folderfiles2 (2026-10-22)

Main `05c8035ea` merged once (`b4b87e243`; the only conflict was the verification pins, resolved
with both sides' evidence and renewed hashes). The merge changed `engine/` and app code, so the walk
ran once more on the merged tip and passed every step (run 4 in `folderfiles1-walks-2026-10-21.md`).
The branch proof run follows this commit; its logs directory is in the handover report.

## Noted, not changed

- On main and here, Stu reads a connected folder's files as ordinary files and may relay their
  rules in its own words; this change only stops the APP from handing the file to Stu.
- A large instruction file costs its size in every worker's and reviewer's context, as in the
  terminal (plan §2 case 8).
- In a fresh guest no engineer is active and hired teammates register only at the next provider
  connection, so a first job can stall for 45 minutes (both unseeded walks).
