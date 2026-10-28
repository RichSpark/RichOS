# RichOS CI pause

CI for `WebDevBooster/richos` was paused at the owner's request on
15 September 2026. Resume product work with targeted local verification.
There is no automatic restoration date.

## GitHub scope

These workflows are disabled through the GitHub API; their YAML files and
test code remain available:

- engine-self-verify
- engine-run-record
- app-spine-ci
- app-voice-ci
- ui-suite-ci
- packaging-ci
- windows-companion-ci

`vouch-pr` and Dependabot Updates remain active. Branch protection is unchanged.

## Local scope

`scripts/lib/ci_pause.py` reads the operator's
`~/.claude/state/ci-pauses.json` registry. Entries match an exact GitHub
repository, including its other local checkouts. `CI_PAUSE_CONFIG` selects a
different registry for isolated tests. Missing registry means no pause.

For a paused repository:

- The red probe returns `state: paused` before consulting the cache or GitHub.
- The merge guard preserves its land-completeness check and abstains from CI.
- The turn gate does not request CI verdicts or acknowledgments for its pushes.
- The surface scanner excludes it before making API calls and reports it in
  `paused_repositories`. Paused repositories are outside active counts.
- The session-start notice ignores its stale red records.
- Owned-state reporting marks its CI requirement `PAUSED`, so it cannot demand
  a repair assignment. Unrelated owned systems retain their existing checks.

Both the repository source and the installed plugin copy were updated on the
operator's machine. Hook registrations were preserved. The shared scheduled
watcher remains loaded for other repositories. Its coverage baseline was
adjusted only for the deliberate removal of RichOS and its nine monitored
workflow records, including the two automations left enabled on GitHub.

The registry is local operator configuration. Cloning this repository onto
another machine does not install that registry. GitHub workflow enablement
is a separate repository setting.

## Verification

Run the focused offline checks with:

```bash
bash richos/engine/scripts/lib/ci_pause.test.sh
bash richos/engine/scripts/ci-status.sh --repo /path/to/richos --sentence
gh workflow list --repo RichSpark/RichOS --all
```

The status command must say `CI PAUSED`; that is not a passing build result.
The local restoration record is under
`~/.claude/state/ci-pause-2026-09-15/`. It contains backups, settings snapshots
and verification evidence. Do not publish that machine-specific record.

## Restoration

Restore only after a new explicit instruction from the owner:

1. Agree which workflows are wanted and their acceptance checks.
2. Re-enable the selected GitHub workflows while local CI enforcement stays
   paused. Verify usable results on the intended revision.
3. Remove the RichOS registry entry. The next reads resume normal behaviour;
   owned-state caches are invalidated by the changed pause policy.
4. Refresh the shared watcher so old red records are replaced with current
   readings. Its coverage high-water mark grows when RichOS returns.
5. Update this record, then verify a fresh session.

Do not blindly overwrite newer files with the backups. Review source changes
against the current tree. The pause support can remain installed when the
registry entry is removed.
