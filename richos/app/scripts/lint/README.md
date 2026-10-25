# Rust and shell lint

From the repository root:

```sh
bash richos/app/scripts/lint.sh --fast
bash richos/app/scripts/lint.sh --all
```

`--fast` runs ShellCheck, the project rules and Clippy for the core/voice workspace
and the detached updater. `--all` adds the detached Tauri workspace. All Clippy
commands use `--locked --all-targets` with default features. They type-check test
and example targets but do not run them or launch the app. Compiler warnings are
counted by diagnostic ID; compiler errors always fail. Rust settings warn during
ordinary compilation and the lint entry point refuses growth above the baseline.

Install ShellCheck 0.11.0 and the Clippy component for the Rust toolchain recorded
in the baseline. Python 3.11 or newer is needed for the TOML reader; its major/minor version is recorded
for the custom rules. Commands, versions, rules and scanned paths are visible in
`baselines/*.json`. Missing tools, empty inventories, invalid output and failed
compilation refuse the check. `CARGO_TARGET_DIR` is respected.

The tracked inventory includes shell scripts nested under `app/scripts`, Rust
files under `app` and JavaScript under `app/ui`. Every file has a language and a
role. Files in `fixture` or `fixtures` directories are recorded but excluded from
product scans. Test suites remain in the scan. The `suite-inputs` rule applies
only to top-level `*.test.sh` files discovered by the app script runner.

## Rules and limits

| Rule | Treatment | Supported evidence |
| --- | --- | --- |
| ShellCheck diagnostic IDs | Blocking count ceiling | ShellCheck's parser and documented rules |
| Rust/Clippy diagnostic IDs | Blocking count ceiling | Compiler and Clippy output for the recorded commands |
| `process-pattern-kill` | Blocking count ceiling | Direct `pkill` by name, `kill $(pgrep ...)`, simple scalar assignments and `for` variables carrying name-selected PIDs into `kill` |
| `process-self-wait` | Blocking count ceiling | Literal `sh -c` or `bash -c` wait whose literal `pgrep -f` pattern appears in that command line |
| `timeout-success` | Blocking count ceiling | Named timeout comparison followed immediately by literal `exit 0` in its `then` branch |
| `timeout-indirect` | Advisory | Such a branch delegates to a function or other control flow |
| `suite-inputs` | Blocking count ceiling | A discovered suite lacks a nonempty input declaration |
| `dialect` | Blocking count ceiling | Every word the engine's dialect hook would fix or refuse in a Write of the file, through the hook's own fixer module, preserving locale, file-class and ownership exemptions |
| `test-positive-control` | Advisory | A refusal assertion without an acceptance assertion recognized in that file |
| `test-retry` | Advisory | Retry or sleep syntax in a test; product retries can look identical |
| `ui-absence` | Advisory, JavaScript report only | A string that may assert an absence |
| `wall-clock-verdict`, `short-deadline`, `sleep-then-assert`, `host-sample`, `env-mutation-unguarded` | Blocking per SITE (`load_rules.py`) | Load-sensitive test code, below |

Structural shell checks cover the syntax above. They are not a shell interpreter
or a proof of arbitrary data flow. Heredoc contents, indirect functions, arrays
and `eval` are outside their contract. PID-only selectors and captured child PIDs
are not name selection. Positive controls can live in other files; a matching
assertion in the same file does not establish semantic adequacy. Advisory counts
never block. Public incident references live beside each project rule.

The dialect adapter imports the engine's fixer
(`richos/engine/scripts/lib/dialect/dialect-fix.py`, the module
`dialect-guard.sh` runs) rather than maintaining a second scanner. It reads the
locale from the entity's `orchestration.config` the way the hook does; a locale
other than `en-US` refuses, because an unevaluated scan is never clean. Its
word list stays `richos/engine/scripts/lib/dialect-en-US.dict` until the
ceiling is re-derived for the generated table (stream C). Explicit exemptions in
the hook remain valid.

## Load-sensitive test code

`load_rules.py` is the rule from section 5 of
`docs/verification/2026-09-29-load-sensitive-checks-audit.md`: *a test may bound TIME only to
catch a hang, never to decide a verdict; and a deadline measures execution, never queueing.*
It covers test code only: `*.test.sh`, `*.test.py`, `ui/tests/**/*.js` (blocking, unlike the
other JavaScript rules), Rust files under `tests/` and `#[cfg(test)]` items.

| ID | Refuses |
| --- | --- |
| `wall-clock-verdict` | an assertion comparing a measured duration (`Date.now()`/`performance.now()` difference, `Instant::elapsed`, `time.monotonic()` difference, `$SECONDS` or a name assigned from one) against an upper literal |
| `short-deadline` | a literal deadline under 30 s: `timeout=` in a call, Playwright `timeout:`, `waitForFact(..., N)`, `recv_timeout`, `Instant::now() + Duration` |
| `sleep-then-assert` | `waitForTimeout(N)`, `time.sleep(N)`, `sleep N`, `thread::sleep` followed, before any condition wait, by an assertion; a sleep inside a loop is a poll interval and is not a site |
| `host-sample` | `/usr/bin/top`, `os.loadavg`/`getloadavg`, `vm_stat`, `memory_pressure`, `uptime` executed (a line naming a mock, fake, stub or fixture is not) |
| `env-mutation-unguarded` | `std::env::set_var`/`remove_var` in a Rust test with no `static` `Mutex`/`RwLock` guard in the file |

A site is exempt only with `load-bound: <why this cannot depend on host load>` on the line or
the line above, where a reviewer sees it (for example `load-bound: virtual clock, page.clock
installed at :40`). A bare marker or a reason under ten characters, exempts nothing.

These rules are baselined by SITE, not count: `baselines/load.json` holds each existing site
(file, rule, hash of the whitespace-normalized line). Any site not in it refuses, even while
another is being removed; a duplicated line is a new site. `--lower` drops paid-down sites.
Against `refs/heads/main` a branch cannot add a site to the file or drop a rule. Moving a
line to another file makes it a new site: fix it or declare it. The recognizers are line
patterns, not parsers; what each one sees is written beside it in `load_rules.py`. The engine
suites (`richos/engine/**/tests/`) are outside this lint's inventory, as the audit notes.

## The commit check: `--changed`

`--changed` is what the automatic commit check runs (`scripts/autocheck/`). It enforces the
same ceilings as `--static`/`--fast` at a fraction of the cost:

- Versions, rule sets, commands, inventory and the baselines against `refs/heads/main` are
  checked in full every time; they are cheap.
- Every static count is per file, so only the files that differ from `HEAD` (plus the
  scripts that `source` a changed script) are counted, before (`HEAD`, in a scratch copy of
  the scripts tree) and after. If no count grows, no ceiling can newly fail. If one grows,
  the full static pass decides, so the verdict is never looser than the full lint's.
- Load rules are per site, so scanning only the changed files is exact.
- Clippy runs only for a Rust set whose inputs changed: the fast set for Rust under
  `richos/app` outside `src-tauri`, Tauri Clippy for `src-tauri`.
- A change to the lint itself or its baselines runs the full static pass and the fast set.

It checks the working tree, including unstaged edits. The land check (`lint.sh --all` or
`lint.test.sh` when `proof-for.sh` selects it) sees exactly the merged tree.

`--changed --strict` is what a commit runs. A ceiling that nobody lowered leaves room: on
2026-09-29 `let_underscore_must_use` stood at 747 under a ceiling of 777, so thirty new
discarded results would each have passed. Under `--strict` a commit may not grow any count,
whatever room the ceiling has: a static count that grows over the changed files refuses and
each Clippy set's counts are compared with the counts recorded for HEAD's Rust. Every Clippy
run records its counts under a digest of what the set reads (every Rust input's blob, the
commands and the tool versions) in `<git-common-dir>/richos-lint-cache/`, shared by every
worktree; a land's run records main's, so a branch cut from main is compared from its first
commit. With no record for HEAD, the commit is held to the ceiling only and the run records
one. An intended exception is made explicit where a reviewer sees it (`# shellcheck
disable=SCnnnn`, `#[allow(clippy::...)]`), not absorbed by stale room. Merges into a branch
run `--changed` without `--strict`: what they bring from main was held to its land.

## Maintaining the ceiling

Normal checks never edit baselines. Counts prevent a **net increase**, not every
new violation: fixing one and adding another can leave the same count.

```sh
bash richos/app/scripts/lint.sh --all --lower
git diff -- richos/app/scripts/lint/baselines
```

`--lower` proposes decreases as a working-tree diff for a separate reviewed
commit. It refuses growth. Comparison with `refs/heads/main` also rejects a
raised ceiling, removed rule, weakened classification or smaller inventory.
`--trusted-ref` selects another integration reference explicitly. That reference
must exist. Changing a tool version requires an explicit integration migration;
a normal baseline update cannot silently waive it. A scanned file that is gone
from the tree (nothing exists at its path) leaves the inventory without one: the
normal check passes, `--lower` records the removal and the comparison with
integration accepts a baseline that lost only such paths. A file that still
exists but is no longer scanned or whose role changed, is still refused. A
rename is a removal plus a new file. New files are scanned immediately, even
before their inventory is added with `--lower`. A new diagnostic ID starts at
zero.

`--bootstrap` creates missing initial baselines only if they are also absent on
integration. Existing baselines are still checked, so an interrupted first run
can resume without replacing previously established ceilings. Initial ceilings
need review at introduction; there is no prior trusted ceiling to compare then.

`--static` runs only the shell and custom checks. It is useful during development
but does not satisfy the complete build gate. `--json-out FILE` writes detailed
findings and phase timings to an explicitly chosen report location. `--js-report`
reports JavaScript dialect matches and advisory candidates without enforcing a
JavaScript ceiling. It names shell-only rules that do not cover JavaScript.

## Build integration

The script runner discovers `lint.test.sh`, which runs the refusal/acceptance
fixtures and `lint.sh --all`: the fast set and Tauri Clippy. That suite is what a
land runs (`proof-for.sh` selects it for every change under `richos/app`), so the
land counts the same Tauri ceiling the nightly refuses on; with only the fast set,
a merge could pass its land and then stop the build. The nightly then invokes
`lint.sh --all` with its current suite receipt. Tauri Clippy runs every time. A passed fast-suite receipt
from the same nightly run avoids repeating fast lint. A missing, invalid or
skipped receipt runs fast lint again before Tauri Clippy. Receipt reuse requires
the nightly's execution marker; standalone `--all` runs both sets.
The lint never probes the parent's release lock or waits for the parent's load.
Scheduling standalone measurements is an operator responsibility.

The Tauri deadline is 180 seconds of Clippy's own work. Waiting for Cargo's lock
(from Cargo's "Blocking waiting for file lock" line to its next line) is queueing,
not work: it is reported and not counted and a wait past 1800 seconds is refused
as a hang (`rust.py` `LOCK_WAIT_GUARD`). Every Clippy run goes through `run_cargo`.
Timeout cleanup terminates the process group created for the command, escalating
to a kill if needed. It never selects processes by name. Descendants that
deliberately detach into another session are outside process-group ownership.
A cold check may exceed the deadline and refuse; the cap does not promise that
cold compilation finishes within it.
