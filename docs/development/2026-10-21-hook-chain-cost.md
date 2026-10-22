# Why the Mac filled itself while the team waited: four fixes, measured (2026-10-21)

Zach, branch `cc/zach-opus-hookcost3`. The question: *"if everyone is just waiting
almost all of the time, they why is the CPU always at 100%?"* Done means the Mac stops
filling itself up while the team's jobs wait, proven by measurement.

## Step 1 is Hank's, not mine

Rich replaced my step 1 with Hank's measurement, "Why the Mac sits near 100% CPU
while the team waits", kept in the private record (commit `ffd30c3f6` there; it is not
part of this repository): waiting by polling costs about 3% of
the Mac (9.9 tool calls a minute, 48% of them polls, 1.75 CPU-s per Bash hook chain).
The polling work was dropped. Four kinds of overhead take about 31% together and
this branch fixes all four, one commit each.

## The four fixes

| # | What burned the CPU | Fix | Measured before | Measured after | Proof of same decisions |
|---|---|---|---|---|---|
| a | `testvm/test/stub-guest.sh:35` logged with `"${CMD//$'\n'/ }"`, which grows with the square of the text on `/bin/bash` 3.2.57 | `printf '%s' "$CMD" \| tr '\n' ' '` | 16 KB command: 33.2 s (loaded Mac) | 0.035 s | `cmp` of the two log lines: identical bytes. `testvm/test/run-tests.sh`: 177 passed, 0 failed |
| b | The privacy guard rebuilt the whole private corpus on every public Write and commit | Keep per-file records, the corpus and its index on disk; rebuild only what changed | 42.4 CPU-s (36.8 user + 5.6 sys) and 1.3 GB per scan, 26,956 files | warm: 0.46 CPU-s, 63 MB; cold: 35.5 CPU-s once | `publication-boundary.test.sh` 149/149 and the gaps tests, both uncached and with the cache forced on; 7 new cache tests (cold, warm and uncached agree; edited, removed, new and unreadable files are noticed) |
| c | The Bash dispatcher forked all 20 rules on every call | Ten rules name the words their own first filter needs (`# dispatch-words:`); a call naming none of them does not fork that rule | `ls` 1.255 CPU-s, `git status` 1.299 CPU-s | `ls` 0.776, `git status` 1.071 | 150 real commands replayed (from 2,795 in today's transcripts): 1,306 skipped rule runs, each run alone gave exit 0 and no output, 0 mismatches. Dispatcher tests 45/45 |
| d | Every supervised check read the native identity of all ~650 processes five times a second, plus a second `ps` for root processes | Native reads only for processes tied to the supervised tree by parent or group | one tick on the live table (623 processes): 38.4 ms CPU, 19.2% of one core per supervisor at 5 ticks/s | 13.3 ms, 6.6% of one core | `runner-reliability.test.py` 37 OK, two new: only the tree is read and no second `ps` is spent; a reused PID is still refused. `proc_tree_pause.test.sh` 7 OK on real supervisors |

Commands that produced the numbers (all re-runnable from the branch):

- (a) the 16 KB comparison: a `/bin/bash` script that builds a 16,403-byte command with
  tabs and newlines and writes it both ways, then `cmp`.
- (b) `/usr/bin/time -l python3 richos/engine/scripts/lib/publication-boundary.py <job>`
  with the real declaration's sources and limits (`.richos/publication-boundary`), run
  uncached under cProfile, then cold and warm with the cache. All three printed
  `CLEAN`, `CORPUS 1031 10073232`, `IDENTITY 1`.
- (c) the dispatcher run on one payload, best of 3, child CPU from `getrusage`, the
  installed engine (`/Users/alex/ab/richos/richos/engine`) against this branch.
- (d) `proc_tree.process_rows()` against `process_rows(seeds={pid})` for one sleeping
  child, 20 calls each, own plus child CPU from `getrusage`.

## What the four fixes should take off the Mac

Hank's shares, scaled by the measured ratios above:

| # | Hank's share | Left after | Basis |
|---|---|---|---|
| b | 9% | about 0.5% | a warm scan costs about 1% of a cold one; a cold rebuild remains once per change to a private file that takes part |
| a | 8% while the testvm suite runs | about 0% | 0.035 s against 33.2 s for 16 KB |
| d | about 8% | about 2.8% | 13.3 / 38.4 ms per tick |
| c | about 6% | about 4.2% | 18% to 38% less per Bash call, by how many git words a call names |
| | about 31% | about 7.5% | about 23 points off; Hank's 88% window would have been about 65%, under the 80% bar |

## The live Mac, before and after

`python3 richos/engine/scripts/host-busy-window.py SECONDS` (committed with this
work): the kernel's own CPU tick counters and processes created per second read off
the PID counter, sampled once a second.

| Window | Seconds | Busy | User | System | Processes created / s | What was running |
|---|---|---|---|---|---|---|
| Before, ending 22:37:56Z, 2026-10-21 | 302 | 93.7% | 59.0% | 34.6% | 535 | the installed engine (main `b87ebca10`), about 9 agents and Codex |
| Same sampler, ending 23:32:01Z, 2026-10-21 | 302 | 79.0% | 42.3% | 36.7% | 627 | still the installed engine at main `b87ebca10`: none of the four fixes was installed yet and this branch's own commit checks ran for part of the window |

The second row is NOT the fixes' effect. The installed hooks are main's until Rich
lands and installs this branch, so the 14.7-point drop is the team's own load
changing between the two windows. The per-unit measurements in the tables above are
the proof of each fix; the Mac-wide effect is the projection in the table before
this one. The true live "after" is one command once the hooks are installed, taken
in a window with a comparable team load:

    python3 richos/engine/scripts/host-busy-window.py 300

## What this does not fix, said plainly

- Ten Bash rules judge every command (display and power, public record, operator
  claim, worktree removal, interactive prompts, main-checkout writes, foreign app data,
  `rm` of a variable path, the shared scratchpad, `bash -c`). They have no word filter
  of their own, so they still run on every call; a `git` command still runs every git
  rule. Each costs 0.03 to 0.18 CPU-s.
- The cold corpus build still costs ~35 CPU-s, now once per change to a private file
  that takes part in the corpus instead of once per public write.
- Hank's uncaught 37% (processes that live under a second and the kernel's cost of
  creating them) is only reduced as far as these four fixes create fewer processes.

## Install

Rich installs the engine hooks after the land (`richos/engine/scripts/hooks/install.sh`).
Until then every number for this branch above was measured by driving the branch's
code directly. The first privacy scan after the install builds the cache once (~35
CPU-s, about 40 s); every scan after it is warm.

## Side finding

`agent_hold.py wait` printed results of earlier, already finished or refused calls
again as `TASK refused` (seen in this run, as Hank saw in his), so an agent waiting
on a running command reads stale errors instead of that command's state.
