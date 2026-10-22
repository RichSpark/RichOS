# Mega Lander

Mega Lander automates the workspace lifecycle for **cross-repo agent work**.
An agent can have a native workspace in the session's base repository and a
`cc/` workspace in another repository. Mega Lander tracks them as one agent's
work so finished branches, worktrees and processes do not accumulate across
repositories or survive forgotten sessions.

The governing rules remain [the workspace spec](../../../docs/plans/worktree-spec-2026-09-11.md).
The same registry also handles native-only agents and the base-repo
workspace of cross-repo agents.

## Directory map

| Path | Responsibility |
| --- | --- |
| `workspaces.py` | Registry, session and agent lifecycle, integration records, landing eligibility, deletion and retries |
| `workspaces.sh` | Command-line entry point for the registry |
| `create-teammate-worktree.sh` | Registers and creates a workspace in another repository, including `.worktreeinclude` seeding |
| `workspace-probes.py` | Runs committed regression probes against the registry |
| `tests/` | Spec tests, cross-repo lifecycle tests, probe-runner tests and mutation harnesses |

Claude adapters remain in [`../scripts/hooks/`](../scripts/hooks/):

- `workspace-lifecycle.sh` records platform events.
- `guard-workspace-gate.sh` enforces pending-work handling at turn end.
- `guard-worktree-isolation.sh` enforces registration and isolation at spawn.
- `guard-sealed-worktree.sh` locks out finished agents.
- `guard-resume-isolation.sh` checks resumption.
- `guard-worktree-removal.sh` guards direct workspace operations.
- `observe-created-refs.sh` attributes branches to agents.

[`../hooks/hooks.json`](../hooks/hooks.json) remains the hook registration surface.
Shared orchestration stays in `../scripts/spawn.sh` and `../scripts/lib/spawn.py`.
Completion checks, container management and other shared consumers use this
registry as their shared source of workspace state and landing eligibility.

## Lifecycle

1. Record the integration branch for each repository involved in the work.
2. Register the cross-repo workspace before creating it or spawning the agent.
3. Bind the platform agent and its native workspace to the same record.
4. Record completion, pause, resume or session ending from lifecycle facts.
5. Resolve finished work by landing or authorized discard, then remove all of
   that agent's workspaces and branches together. Stop its processes first and
   retry failed deletion automatically.

`land` checks that committed work is already merged into the recorded
integration branch. It does not perform the merge. Pending-work gates enforce
that the lead handles unfinished integration before starting new work.

## Commands

Run these from the engine directory:

```sh
mega-lander/workspaces.sh --help
mega-lander/workspaces.sh status
mega-lander/workspaces.sh integration --repo /path/to/repo --branch dev/example --why 'this body of work'
mega-lander/workspaces.sh land <agent>
mega-lander/workspaces.sh retry
```

The usual spawn entry point remains `scripts/spawn.sh`. It calls Mega Lander's
creator as part of preparing a cross-repo agent. The creator can also be run
directly: `mega-lander/create-teammate-worktree.sh <repo> <agent-name>`.

One agent can work in SEVERAL other repositories: give `spawn.sh` one `--repo`
per repository and it creates and registers one `cc/` workspace in each under
the one agent name, with one `cross-repo-worktree:` line per workspace in the
payload. Run by hand, the creator does the same thing when it is run again with
a different `<repo>` and the same `<agent-name>`; a second workspace in the SAME
repository under that name stays refused. `land` and `discard` delete every
workspace and branch the agent has, across every repository, as one (spec point
10).

Existing command paths in `scripts/` forward here. The old
`scripts/lib/workspaces.py` import path also forwards here for external callers
and historical probes. These compatibility files contain no lifecycle policy.
Maintained engine consumers use the canonical feature paths.

## State and worktree locations

This source-code move does not migrate runtime state. The store remains at
`$RICHOS_WORKSPACES_DIR`, otherwise `$CLAUDE_CONFIG_DIR/state/workspaces`,
otherwise `~/.claude/state/workspaces`.

The store contains `sessions/`, `agents/`, `done/`, `ids/`, per-call snapshots
under `refs/`, `repos.json`, `integration.json`, `events.jsonl` and a mutation
lock. All repositories share that store.

Cross-repo worktrees default to a sibling `<repo>-wt/<agent-name>` directory
with branch `cc/<agent-name>`. Native worktrees remain under the base repository's
`.claude/worktrees/agent-<id>`. `codex/` protections are unchanged.

Land locks are the one piece of state that is deliberately **not** in that
store, because that store is partitioned per conversation. `app.py`'s
`integrate` takes one `flock` per target repository in
`$CLAUDE_CONFIG_DIR/state/land-locks` (otherwise `~/.claude/state/land-locks`),
keyed to the realpath of the repository's `--git-common-dir`, so two paths to
one repository — a symlinked connection, a linked worktree — are one lock and
two different repositories are two. It is held across the whole land: the
recorded-tip check, the merge and the workspace cleanup. Two
conversations may share a repository and their lands take turns; the second one
waits, reports what it waited for and refuses only if the holder has not
released within `RICHOS_LAND_LOCK_TIMEOUT` (600s) — a refusal that names the
holding thread and confirms nothing was merged. Order among waiters is the
kernel's rather than a first-come queue, because a ticket file is not
kernel-released and a queue would have to decide whether a waiter is still
alive. `RICHOS_LAND_LOCKS_DIR` overrides the home for tests and is refused if
it resolves inside either per-conversation partition. The path is derived in
`workspaces.py` rather than `app.py`, because the protected-ref check below has
to read it and `workspaces.py` cannot import `app.py`; one keying rule, in the
layer both halves can reach.

The land itself is the merge the terminal's lander runs (`workspaces.sh merge`):
`git merge --no-ff` of the reviewed commit, in the repository's checkout, with
the repository's own hooks. So its merge-time check (`pre-merge-commit`) runs
before the merge commit exists and a refusal lands nothing and is returned in
the check's own words. Where the repository's land fence is on
(`operator-fences.sh`, its `reference-transaction` hook), the land takes the
land lease as itself under the same lock, holds it for exactly its land and
releases it before the lock. Both are the repository's settings; the app names
none of them.

Beside each lock is that repository's **land record**, `<label>-<digest>.lands`
— one appended JSON line per land, naming the conversation thread, the branch,
the before tip and the commit. Appended and never overwritten, because the lock
file itself holds one holder record that the next lander rewrites in place, so
of two lands in a row only the second would survive. It is written at the moment
the merge moves the branch, under the lock, so a repeated `integrate` records
nothing.

That record is what lets a thread be TOLD its base moved. `_restore_protected_refs`
reports a fast-forward of a recorded branch when the land record attributes it to
a **different** conversation (`<branch> MOVED UNDER YOU`, through
`observe-created-refs.sh`), stays silent for this conversation's own land and
reports a move no record names **only** where that repository keeps land records
at all — a repository with none is the terminal path, where Rich lands by hand
and `guard-inflight-notify.sh` tells the teammates at the push. Nothing is
written to any repository: a fast-forward is announced, not judged.

## Verification and installation

```sh
bash mega-lander/tests/workspaces.test.sh
bash mega-lander/tests/app.test.sh
bash mega-lander/tests/create-teammate-worktree.test.sh
bash mega-lander/tests/workspaces-e2e.test.sh
bash mega-lander/tests/workspace-spec-fourteen.test.sh
bash mega-lander/tests/workspace-stopped-ending.test.sh
bash mega-lander/tests/workspace-probes.test.sh
bash mega-lander/tests/relocation.test.sh
```

The core, creator, fourteen-point and probe suites retain their mutation
harnesses. CI discovers the moved shell suites recursively. Historical probes
remain in `docs/verification`; the runner accepts their old import paths and
can test commits from before the move with `--at`.

The existing hook installer hashes the canonical registry and command here.
Integrity layer Q checks those files and exercises the real lifecycle hooks.
Sandbox builders and hook dependency discovery include `mega-lander/`.
The installed engine pointer continues to point to the engine root.
