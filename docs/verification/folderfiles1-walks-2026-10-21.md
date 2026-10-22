# folder-instructions-walk.py: the three runs of 2026-10-21 (zach-opus-folderfiles1)

Each section is read from the walk's own JSON records (report.json, the two jobs'
`*-observed.json`, preserved.json and run-walk.py's report). `received` is read from the
first message of each agent's own transcript in the guest; `stu payload` is the receipt's
payload, the call Stu read and submitted.

## 1. Red: main's engine

run-walk.py: vm `walk-e5a316258cd2`, execution `completed`, scenario exit 1, cleanup complete True, 2228 s.

| Step | Outcome | Detail |
|---|---|---|
| identity | PASS |  |
| seed | PASS |  |
| first-run | PASS |  |
| connect | PASS |  |
| connect-tools | PASS |  |
| house-rule | FAIL | the job ended running, not settled; notes.md was not landed in Acme; the commit that added notes.md does not end with [house-rule]: ''; no worker received the folder's instruction file; no reviewer received the folder's instruction file |
| agents-rule | FAIL | no worker received the folder's instruction file; no reviewer received the folder's instruction file; the payload Stu read holds the folder's instruction file |
| preserved | PASS |  |

**house-rule**: job running; landed False; rule followed False; commit message:

```text
(none)
```

No worker or reviewer was prepared for this job.

**agents-rule**: job settled; landed True; rule followed True; commit message:

```text
Add tools.md

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
[agents-rule]
```

| Role | Teammate | Received heading | Received rule | Received AGENTS.md beside CLAUDE.md | Stu payload holds the file |
|---|---|---|---|---|---|
| worker | norm | False | False | False | True |
| reviewer | mark | False | False | False | True |

Instruction files, sha256 before and after (identical: True):

- `Acme/AGENTS.md` `1453911eecd7a776dda899eb95d7f7749e6df4adbbeb003ab1d06bae9654ab68`
- `Acme/CLAUDE.local.md` `3663d9046879db9725b102031381271510763e167c82f38464de20e020241d31`
- `Acme/CLAUDE.md` `1b23d0522c5ae4fd4827abefa24b6ac5a646d2d766a9cd54556ec10c3a260c23`
- `Tools/AGENTS.md` `882ecea4f987865721c01051a54a89b4f1cf05efd993f9c4234cdaeefeecf8fc`

## 2. Green, unseeded, before the hook fix

run-walk.py: vm `walk-fa2a5267daf4`, execution `completed`, scenario exit 1, cleanup complete True, 3336 s.

| Step | Outcome | Detail |
|---|---|---|
| identity | PASS |  |
| seed | PASS |  |
| first-run | PASS |  |
| connect | PASS |  |
| connect-tools | PASS |  |
| house-rule | FAIL | the job ended running, not settled; notes.md was not landed in Acme; the commit that added notes.md does not end with [house-rule]: ''; no worker received the folder's instruction file; no reviewer received the folder's instruction file |
| agents-rule | FAIL | no worker received the folder's instruction file; no reviewer received the folder's instruction file |
| preserved | PASS |  |

**house-rule**: job running; landed False; rule followed False; commit message:

```text
(none)
```

No worker or reviewer was prepared for this job.

**agents-rule**: job settled; landed True; rule followed True; commit message:

```text
Add tools.md [agents-rule]

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>

[agents-rule]
```

| Role | Teammate | Received heading | Received rule | Received AGENTS.md beside CLAUDE.md | Stu payload holds the file |
|---|---|---|---|---|---|
| worker | norm | False | False | False | False |
| reviewer | tom | False | False | False | False |

Instruction files, sha256 before and after (identical: True):

- `Acme/AGENTS.md` `1453911eecd7a776dda899eb95d7f7749e6df4adbbeb003ab1d06bae9654ab68`
- `Acme/CLAUDE.local.md` `3663d9046879db9725b102031381271510763e167c82f38464de20e020241d31`
- `Acme/CLAUDE.md` `1b23d0522c5ae4fd4827abefa24b6ac5a646d2d766a9cd54556ec10c3a260c23`
- `Tools/AGENTS.md` `882ecea4f987865721c01051a54a89b4f1cf05efd993f9c4234cdaeefeecf8fc`

## 3. Green, seeded, after the hook fix (engine c1a2ccc94, bundle 4572beca9)

run-walk.py: vm `walk-3d9ea57ab323`, execution `completed`, scenario exit 0, cleanup complete True, 888 s.

| Step | Outcome | Detail |
|---|---|---|
| identity | PASS |  |
| seed | PASS |  |
| first-run | PASS |  |
| connect | PASS |  |
| connect-tools | PASS |  |
| house-rule | PASS |  |
| agents-rule | PASS |  |
| preserved | PASS |  |

**house-rule**: job settled; landed True; rule followed True; commit message:

```text
Add notes.md for the walk test

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
[house-rule]
```

| Role | Teammate | Received heading | Received rule | Received AGENTS.md beside CLAUDE.md | Stu payload holds the file |
|---|---|---|---|---|---|
| reviewer | tom | True | True | False | False |
| worker | norm | True | True | False | False |

**agents-rule**: job settled; landed True; rule followed True; commit message:

```text
Add tools.md for the walk test

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
[agents-rule]
```

| Role | Teammate | Received heading | Received rule | Received AGENTS.md beside CLAUDE.md | Stu payload holds the file |
|---|---|---|---|---|---|
| worker | norm | True | True | False | False |
| reviewer | tom | True | True | False | False |

Instruction files, sha256 before and after (identical: True):

- `Acme/AGENTS.md` `1453911eecd7a776dda899eb95d7f7749e6df4adbbeb003ab1d06bae9654ab68`
- `Acme/CLAUDE.local.md` `3663d9046879db9725b102031381271510763e167c82f38464de20e020241d31`
- `Acme/CLAUDE.md` `1b23d0522c5ae4fd4827abefa24b6ac5a646d2d766a9cd54556ec10c3a260c23`
- `Tools/AGENTS.md` `882ecea4f987865721c01051a54a89b4f1cf05efd993f9c4234cdaeefeecf8fc`

## The interrupted proof run (cb0c7ae43..c1a2ccc94)

3 of the plan's checks passed and 0 failed before the CEO's order of 23:05Z stopped the run; every check it had started was stopped with it. The passes were the repository-connection Rust suite (11 of 11), one Rust test of the native driver and the UI affordance suite. Nothing else ran.

## 4. Green, seeded, after merging main (engine and bundle b4b87e243, zach-opus-folderfiles2, 2026-10-22)

The merge of main `05c8035ea` changed `engine/` (the app hook, `dispatch-pretooluse.sh`,
`workspaces.py`, `mega-lander/app.py`) and app code, so the walk ran once more on the merged tip:
a development bundle `1.2.2-dev.b4b87e243` from `package-app.sh` and the engine asset from
`make-engine-asset.sh` at the same commit, with the team seeded by `command-walk.py --seed-team`.

run-walk.py: vm `walk-a77b01f00a98`, execution `completed`, scenario exit 0, cleanup complete True, 837 s.

| Step | Outcome |
|---|---|
| identity | PASS |
| seed | PASS |
| first-run | PASS |
| connect | PASS |
| connect-tools | PASS |
| house-rule | PASS |
| agents-rule | PASS |
| preserved | PASS |

**house-rule**: job settled; landed True; rule followed True; commit message:

```text
Add notes.md

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
[house-rule]
```

| Role | Teammate | Received heading | Received rule | Received AGENTS.md beside CLAUDE.md | Stu payload holds the file |
|---|---|---|---|---|---|
| worker | norm | True | True | False | False |
| reviewer | tom | True | True | False | False |

**agents-rule**: job settled; landed True; rule followed True; commit message:

```text
Add tools.md

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
[agents-rule]
```

| Role | Teammate | Received heading | Received rule | Received AGENTS.md beside CLAUDE.md | Stu payload holds the file |
|---|---|---|---|---|---|
| reviewer | tom | True | True | False | False |
| worker | norm | True | True | False | False |

Instruction files, sha256 before and after (identical: True): the same four values as run 3.
run-walk.py quit the test app and removed the guest ("clean: app quit, VM stopped, clone deleted,
state removed").
