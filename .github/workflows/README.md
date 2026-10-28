# What runs in this directory, what does not and why

**State on 2026-09-10.** Eight workflows, all eight firing on their own.

- **Enabled:** `app-spine-ci`, `app-voice-ci`, `ui-suite-ci` and `windows-companion-ci`, on
  pushes and pull requests, each behind its own path filter.
- **Enabled:** `vouch-pr`, on every pull request opened or reopened. It is the only file here that
  acts on a person rather than on a diff and it can close somebody's work.
- **Enabled 2026-09-10:** `engine-self-verify`, on every push and pull request. Its triggers had
  been absent since 2026-09-01. What had to exist before they could go back — a fast diff-scoped
  required gate, the full pass sharded with its coverage proven and a check for runs that were
  DROPPED rather than failed — is in `richos/engine/docs/ci-sharding.md`.
- **Added 2026-09-10:** `engine-run-record`, every six hours. It is the only file here that asks
  whether a verification HAPPENED rather than whether it passed.
- **Enabled 2026-09-10:** `packaging-ci`, on pushes and pull requests behind a path filter, after
  nine days in which it ran not once. It was stopped in **two** places and each of them alone was
  enough: the `push`/`pull_request` triggers had been removed from the file on 2026-09-04, **and**
  it was `disabled_manually` at the Actions API — the last of the five switched off over the
  2026-09-01 billing block never to be switched back on. The second one also voided the first
  one's escape hatch: `gh workflow run packaging-ci.yml --ref main` answered
  `HTTP 422: Cannot trigger a 'workflow_dispatch' on a disabled workflow`, so the documented way
  to demand the gate on a named SHA had never worked. Both are fixed; the file's header carries
  the measurement and `docs/verification/packaging-ci-2026-09-10/` carries the runs.

**A workflow's state is not in git.** Six of these files were re-enabled by editing triggers and
that is visible in a diff; `packaging-ci` also needed a repository SETTING flipped, which is not,
so it survived a re-enable pass that this document describes. If one is ever switched off at the
API again, say so in this list on the same day and name the id — the check is one command:

    gh api repos/RichSpark/RichOS/actions/workflows --jq '.workflows[] | "\(.state)\t\(.name)"'

The title of this document read "Why every workflow here is disabled" until 2026-09-05, which was
true when it was written and had stopped being true on 2026-09-04. The rest of it is the history
that produced today's state, kept because the history is the reason.

## Why they were all disabled at once and by whose ruling

**Disabled 2026-09-01, deliberately, by CEO ruling: "I'm certainly not gonna pay anything for this
open-source repo."**

### What happened

GitHub Actions stopped running across this repository at **2026-08-30T04:23:17Z** — a **billing
block, not a code failure**. Every workflow completed as `failure` in three to four seconds with:

> The job was not started because recent account payments have failed or your spending limit needs
> to be increased

Twelve consecutive runs. **Everything CI "verified" between that timestamp and 2026-09-01 verified
nothing.**

### Why they were disabled rather than left failing

A workflow that fails for a reason unrelated to the code is worse than no workflow: it puts a red
cross on every push and a red cross that always means nothing teaches everyone to ignore the one
that will someday mean something. Disabling is the honest state.

### Why this was not a cost that needed paying

**Actions is free and unlimited on PUBLIC repositories.** The bill was a side effect of the
repository not being public yet — the license decision was sequenced last by CEO ruling
(`richos-hq/wiki/ceo-decisions.md` §18) and nothing went public before it — and never the price of
running CI. That condition is gone: `gh api repos/RichSpark/RichOS --jq .visibility` answers
`public`, which is why three of these are enabled again and why nothing in this directory costs
anything to run.

The cost mix, for whoever re-enables these: two Linux workflows bill at 1x, `windows-companion-ci`
at 2x and **`packaging-ci`, `ui-suite-ci` and `app-voice-ci` run on macOS, which bills at 10x** —
that group is most of the spend. `app-voice-ci` is the cheapest of the three by a wide margin: the
work it does is 44 seconds cold on the machine it was measured on, against `ui-suite-ci`'s measured
25 minutes. It joined the list on 2026-09-05 and none of it is a bill while the repository is
public.

### What replaced it meanwhile

Local verification, which is currently stronger than what CI was providing: every engineer runs the
suites in their own worktree and the lander re-runs them independently before merging. The
meta-suite, the hook suites, the browser suites and the contrast walk all run on this machine.

**What is genuinely lost, stated rather than glossed:** the Linux and Windows runners. Nothing on
this Mac exercises `windows-companion-ci`, so the Windows capture path is now proven by neither CI
nor hardware — see CEO item 2.3. Do not let "local verification is stronger" quietly cover that gap.

### When to re-enable

**The day this repository goes public** — at which point every workflow here is free, forever, on
standard runners. Re-enable with `gh workflow enable <name>.yml`.

## "Nothing else about them needs to change" was wrong and here is the measurement

That sentence stood here until 2026-09-04. It was an inference — the runs had failed on billing, so
nothing about the code was implicated — and it was never tested, because a billing-blocked run
produces no log to test it against. **All five were then exercised, verbatim, inside a real clone
of this tree in a directory with no private `richos-hq` sibling and under a throwaway `$HOME` —
the engine's on Linux, in a container, as an unprivileged user, because that is what its runner
is. Two of them were not fine.** The clone, the commands and the transcripts are in
`docs/verification/workflows-on-a-clone-2026-09-04/`.

| workflow | state | why |
|---|---|---|
| `app-spine-ci` | **enabled** | green in the clone; `--locked` accepted; only "does it build on Linux" is left to the runner |
| `app-voice-ci` | **enabled** | added 2026-09-05, after the flip, so no part of the billing story above applies to it either. It was not in the clone exercise because it did not exist; what was measured instead is the crate and it retired three of the four clauses `app-spine-ci` had excluded it for — whisper is not a build dependency, no test opens an audio endpoint without `RICHOS_VOICE_LIVE_AUDIO` and the cold build is 12.6 s. Never yet executed and it carries one premise no Mac can settle: how many of the suite's seven candidate voices a runner has. See its header and `docs/verification/voice-ci-2026-09-05/` |
| `ui-suite-ci` | **enabled** | green in the clone under WebKit from its own `node_modules`, after one load-sensitive check in `splash.js` was found flaking and fixed |
| `windows-companion-ci` | **enabled** | restore, the `net8.0-windows` build and all 26 core tests pass off Windows; only the `doctor` smoke step needs the runner |
| `engine-self-verify` | **ENABLED 2026-09-10** | was disabled in the file when this table was written, for the reason kept in the next row's shape: `ci-verify.sh` exited 1 on Linux, 55 of 60 suites. That was true and is superseded. At `4f01e5c7` the pass was 117/121 in 123 minutes (run `34396549904`); it is now twelve parallel shards at ~16 minutes with a coverage job that refuses unless the shards' receipts union to the whole inventory at one commit, plus a diff-scoped `affected` job that is the one to mark required. Of the four reds, two were Linux portability defects and were FIXED; two were contract regressions red identically on macOS, proven by execution to be `6472bb60` and `2afb9703`, were DECLARED with a 14-day expiry rather than skipped, were re-specified by their owner on `zach-opus-cr2` the same day and had their rows deleted by the known-red table's own negative control — a declared unit that PASSES fails the build. Full account: `richos/engine/docs/ci-sharding.md` |
| `engine-run-record` | **ADDED 2026-09-10** | on a schedule, every six hours and it is deliberately NOT a job inside `engine-self-verify`: if that workflow's run was the one dropped, a job inside it did not run either, so a check for absence cannot be hosted by the thing that might be absent. GitHub Free allows 20 concurrent jobs ACCOUNT-WIDE and an exceeded limit DROPS runs rather than queueing them, leaving no record at all — so every tool reading "the latest run" reports an earlier commit's verdict. Measured on 2026-09-10 before the trigger went back: 95 of 98 pushes to main had no run |
| `packaging-ci` | **ENABLED 2026-09-10** | the row above it said 3 of 7 suites could not pass on a public runner and it was RIGHT about the tree it measured on 2026-09-04 and wrong about the cause of two of the three. Re-measured on the runner itself, run `34444955517`: 8 suites, 4 red. Two were the runner image shipping no `cargo-tauri` — an environment defect, fixed by installing a version- and digest-pinned CLI in the job. One was a defect in `make-release.test.sh`, whose R6-R9 could only pass on a machine holding the operator's private named-person deny-list; they now put that gate into a known state and assert its precedence on purpose, as R5a. ONE is genuinely structural and is DECLARED rather than fixed: `gui-boot.test.sh` needs the loro compiler, which is not tracked here. `run-tests.sh` now understands the exit code that suite has always used for "this host cannot answer" — but only for a gap declared by name WITH a reason and it goes red on an undeclared gap, on a bare declaration and on a declaration whose suite has started answering. `run-tests.test.sh` executes all seven of those claims on every run. The green run that replaced the seven consecutive reds is named in `docs/verification/packaging-ci-2026-09-10/` |
| `vouch-pr` | **enabled** | added 2026-09-05, after the flip, so no part of the billing story above applies to it. It runs no suite: it reads `.github/VOUCHED.td` and closes a pull request from an author who is not on it. Never yet executed — see its header and `docs/verification/pr-trust-gate-2026-09-05/README.md` |

**So the post-flip step is "dispatch three", not "dispatch five".** The two disabled files have had
their `push` and `pull_request` triggers removed for exactly this reason; dispatching either anyway
produces a red run that says nothing about going public.

> **Superseded entirely on 2026-09-10.** There is no dispatch step any more: all eight files fire
> on their own. The paragraph is kept because its reasoning — never dispatch a workflow whose
> redness would be about the runner rather than the code — is the reason `packaging-ci` took a
> tauri-cli step, a suite fix and a declared gap rather than a trigger and a hope.

> **Superseded for `engine-self-verify` on 2026-09-10.** Its triggers are back, so it is no longer
> something to dispatch — it fires on every push and `engine-run-record` reports any push that got
> no run at all. `packaging-ci` is the only file this paragraph still describes. Nothing else in
> this section is corrected: it was right about the set it was written over and the reasoning is
> why the trigger restoration was sequenced behind three pieces of machinery rather than done as a
> two-line YAML edit.

**That count was about the five files the flip found here and `app-voice-ci` makes it four to
dispatch rather than three.** It is not a correction to the sentence above — it was right about the
set it was written over — but whoever runs the dispatch should run four and should expect
`app-voice-ci` to be the one most likely to come back red, for the reason its own header gives.

**And the important half of that sentence:** two of `engine-self-verify`'s five reds are red on
the machine the engine is developed on, today and a third is red on the platform CI uses. **The
flip does not cause them; it publishes them.** Each disabled file names its own list and states,
in one line, what puts its triggers back.

> **That half was right and 2026-09-10 measured how right.** Of the four reds at `4f01e5c7`, TWO
> are red on macOS and Linux identically — `shell-worktree-sparse.test.sh` and
> `terminalize-agent-worktrees.test.sh` — and both were proven by execution to be contract
> regressions rather than platform findings: 21/21 and 42/42 respectively at the parent of the
> commit that broke each and neither commit updated the suite it invalidated. A red list is not
> a portability list and the only thing that told them apart was running each suite on both
> hosts.

**The red lists were measured twice, on purpose.** The first Linux pass ran on a machine carrying
eleven agents at a load average up to 58 and a red list taken under those conditions cannot tell
a defect from a flake — it proved it, by failing a suite that passes 57/57 when quiet. Everything
was re-run on a quiet host before it was written down and two suites changed classification
between the passes.

**None of this makes any of them CI-verified.** A clone on the author's Mac answers "does this
depend on his machine". It does not answer "does this work on a runner" and no workflow here
should be described as verified until a run of that workflow is green on a SHA somebody can name.

## Every action here is pinned to a commit and here is how to move a pin

**Pinned 2026-09-04.** A tag is a pointer. `actions/checkout@v5` says "whatever
`v5` means on the morning the job runs" and whoever can move that pointer runs
code on a runner that has already checked this repository out — in
`packaging-ci`, next to release machinery. Only a full 40-character commit SHA is
immutable, so that is what every `uses:` line here carries, with the
human-readable version as a trailing comment.

**The inventory is not repeated here on purpose.** The workflow files are the
list; a table in this README would be a second copy of it, free to drift and the
first thing a reader trusts. `grep -n 'uses:' *.yml` answers the question with no
possibility of being out of date.

**To move a pin, resolve it from the action's own repository and never from
memory, another project or a search result:**

```
git ls-remote --tags https://github.com/actions/checkout.git 'refs/tags/v5.2.0'
```

If the output has a second line ending `^{}`, the tag is annotated and the peeled
line is the commit — pin that one. Then confirm the SHA belongs to the repository
you think it does, which is the check that catches a SHA copied out of a fork:

```
gh api repos/actions/checkout/commits/<sha> --jq .sha
```

Update the trailing `# vX.Y.Z` comment in the same edit. A version comment that
no longer matches its SHA is worse than no comment, because it is the only thing
a reader can read.

**The worked example for why the peel matters is in this directory.** `vouch-pr`
pins `mitchellh/vouch/action/check-pr`, whose `v1` is an ANNOTATED tag and the
object that tag names is not a commit:

```
$ git ls-remote https://github.com/mitchellh/vouch.git 'refs/tags/v1' 'refs/tags/v1^{}' 'refs/tags/v1.5.0'
f23dbb5e745334f97414ec70463ce7301071a661        refs/tags/v1
d66fa29a64600490892131ad87597c30c91fcac4        refs/tags/v1^{}
d66fa29a64600490892131ad87597c30c91fcac4        refs/tags/v1.5.0
```

Pinning `f23dbb5e…` would have been the natural mistake: it is a real object ID,
it comes from the right repository and it fails at run time rather than at
review time. The peeled line is the commit and it is what the file carries.
`v1.5.0` has no `^{}` line at all because it is a lightweight tag — so the
PRESENCE of a second line is the signal, not the version number and the two
must be resolved together or the pair in the file will not match.

**A pin is only as deep as the action is.** `check-pr` is a composite: its own
first step installs Nushell through `hustcer/setup-nu`, which upstream does pin,
at `version: "*"`, which upstream does not. So pinning a composite action pins
the code it runs and not necessarily the interpreter that runs it. That is named
here because this is where somebody will look for it; what it costs that
particular workflow is in that workflow's own header.

**These pins WILL go stale and that is the trade being made.** A pinned action
never picks up an upstream security fix on its own. The automated route is
Dependabot's `github-actions` ecosystem, which opens a pull request that bumps
the SHA and the comment together; until that is switched on, moving these is a
manual job with a real cost — which is the honest price of the tag not being able
to move underneath us.

### The repository policy that would ENFORCE this is off, on purpose

GitHub can refuse to run any workflow whose actions are not pinned, rather than
trusting everyone to remember. On **2026-09-04** the repository reports:

```
$ gh api repos/RichSpark/RichOS/actions/permissions
{"enabled":true,"allowed_actions":"all","sha_pinning_required":false}
```

**It is off and turning it on is the CEO's call, not an engineer's**, because it
does not only judge the seven references pinned today — it judges every workflow
this repository ever gains. A new one added with `actions/checkout@v5` in it does
not warn; per GitHub's documentation, "workflows referencing actions this way will
be blocked and unable to run", which on a bad day looks like CI silently not
starting. Reusable workflows are exempt and may still be referenced by tag.

If it is decided, it is one call — `Settings > Actions > General > Actions
permissions` in the web interface or:

```
gh api -X PUT repos/RichSpark/RichOS/actions/permissions \
  -F enabled=true -f allowed_actions=all -F sha_pinning_required=true
```

Nothing in the tree needs to change first: as of the commit that added this
section, every action reference under `.github/workflows/` is already a
full-length SHA, so the policy would be satisfied on the day it is switched on.

## Nightly releases

Nightlies are triggered manually on the release Mac. There is no Actions nightly
workflow or schedule. Signing keys stay local and existing CI can stay disabled.
See [the local nightly runbook](../../richos/app/NIGHTLY.md).
