# Walkthrough — one feature, the whole machine

**What this is:** an illustrative, narrated trace of one realistic feature
through this engine's *actual* doctrine and mechanics — every step cites the
real file/hook/doctrine section that governs it and the hook messages quoted
below are the real ones these hooks actually print.

**What this is NOT:** a captured transcript of a real session. No live agents
ran this; the exact commit SHAs, teammate names and timestamps below are
illustrative, chosen to make the story concrete rather than abstract. The
**runnable counterpart** of the enforcement beats — the parts of this story
that are real hooks and real git mechanics, not narration — is
`scripts/demo.sh` (README's "60-second proof"): run it and you'll watch the
isolation guard block a bad spawn, the write-guard block a direct edit, a
real worktree/commit/merge, a planted defect get rejected, the fix get
re-verified and the integrity probe confirm the wiring — unattended, in
about a minute, against a throwaway sample. This document tells the fuller
story `demo.sh` can't (it's plain bash, not live agents): the full four-step
QA pipeline, a wiki consultation and a real FIX-FIRST bounce with a design
gatekeeper signoff at the end.

The feature: **add a bulk CSV export button to a fictional "Orders" list
screen** — the same illustrative feature already used in
`qa-audits/EXAMPLE_AUDIT.md` and `ui-ux-signoffs/EXAMPLE_SIGNOFF.md`, so this
walkthrough, that audit and that signoff read as one continuous story
instead of three disconnected samples.

---

## Step 0: the CEO's request and the Loro context check

**CEO, to the orchestrator:** *"Add a CSV export button to the Orders list —
customers keep asking for it."*

Per `CLAUDE.md`'s "The Orchestrator as COO" escalation ladder, the
orchestrator's first move is never to just start working — it's to check
whether this is already decided:

Consult the explicitly configured private Loro corpus for a relevant decision,
preference or precedent. Use the active company scope and fetch the cited records
before relying on them. Legacy adopters may still have a `ceo-wiki/` store; new
installations use the external corpus described in [Loro](loro/README.md).

The orchestrator retrieves the company's product principles and architecture
context, if present, for any standing rule about data exports. In this
story, nothing on file speaks to CSV export specifically — so the ladder's
next rung applies:

> **Step 2: operational and unrecorded → decide autonomously.** You make
> coordination and operational calls yourself; don't manufacture an
> escalation just because the wiki is silent on it.

Adding an export button to an existing list screen is exactly this: routine
product work, not a new-spending/strategic/irreversible decision (step 3's
bar). The orchestrator proceeds without interrupting the CEO — and, per the
write-back rule, will file whatever comes out of this feature (any
preference the CEO states along the way) back into the wiki so the same
question never needs asking twice.

*Doctrine cited: `CLAUDE.md` → "The Orchestrator as COO" (escalation ladder,
write-back rule).*

---

## Step 1 — routing and the spawn

The orchestrator identifies the work as a backend + frontend change and picks
the owning engineer from the Team Directory (`CLAUDE.md` → Team Directory /
Routing — "bug fixes and feature work route DIRECTLY to the owning
engineer"). It spawns a named teammate with the `Agent` tool, exactly per
`CLAUDE.md` → "How to Delegate":

```json
{
  "tool_name": "Agent",
  "tool_input": {
    "subagent_type": "backend-engineer",
    "name": "be-sonnet-high-1",
    "effort": "high",
    "isolation": "worktree",
    "prompt": "Add a bulk CSV export button to the Orders list. Export every row matching the active filter (not just the current page), with a collision-safe file name. Commit on your worktree branch when done; do not merge, push, or deploy. If I message you that main moved under you, acknowledge it durably — I cannot rely on a reply reaching me: scripts/inflight-ack.sh --sha <sha> --impact <conflict|stale-record|grew-scope|none> --detail \"<your own words>\" --paths \"<paths or none>\"."
  }
}
```

The `be` role token isn't arbitrary — Dean picks it per `team/NAMING.md`'s
mnemonic roster convention (short, distinctive, mnemonic where natural); the
full `<role>-<model>-<effort>-<identifier>` shape around it is guard-enforced, with the
`<model>` token kept truthful to what the instance actually boots on and the
`<effort>` word to the level it is started at.

Four things are non-negotiable here, per doctrine and the guards enforce
all four:

- **`isolation: "worktree"`** — every file-writing teammate spawn requires
  one (`CLAUDE.md` → Git Worktree Isolation).
- **A well-formed, truthful `<role>-<model>-<effort>-<identifier>` name** — never bare,
  never reused, the `<model>` token must name the model the instance boots on and
  the `<effort>` word the level it is started at.
- **The whole task in the spawn prompt** — the mailbox is lossy, so the
  prompt has to be self-contained (`CLAUDE.md` → How to Delegate).
- **The in-flight ack contract in the prompt** — a worktree is a snapshot, so a
  land can move `main` under this teammate and nothing will tell it. The lead
  messages it; the teammate answers with a file the lead can `stat`, because a
  reply travels the same lossy channel. The instruction has to ride in the
  spawn prompt for the same reason: an instruction sent later is lost with the
  message it exists to make verifiable. `verify-agent-prompt.sh` check 6
  refuses a worktree spawn without it (opt out on the record with a live
  `no-inflight-ack: <reason>` line). See `skills/rich-lander/SKILL.md` §8b.

If any of these were missing, `scripts/hooks/guard-worktree-isolation.sh`
would block the spawn before it ever reaches an agent — this is the exact
mechanism `scripts/demo.sh` Beat 1 exercises live. Its real refusal message,
for a spawn missing `isolation`:

```
=== Teammate-spawn guard: BLOCKED ===
  Agent 'backend-engineer' can edit repo files, so it must satisfy the
  teammate-spawn contract. It failed:
    - missing native isolation — add  isolation: "worktree"  to run in an isolated worktree (got isolation='unset'); OR, if this is a deliberate main-checkout run, add a live prompt line starting with 'main-checkout-run: <reason>'.
  Re-issue the Agent call fixing the above.
  (If this agent is genuinely READ-ONLY, add its type to READONLY_ALLOWLIST
   in orchestration.config.)
(hook: scripts/hooks/guard-worktree-isolation.sh)
```

The spawn above is compliant, so `guard-worktree-isolation.sh` →
`guard-definition-drift.sh` → `verify-agent-prompt.sh` all pass (exit 0) and
the harness lands the new teammate in a real, dedicated git worktree:
`.claude/worktrees/agent-<id>/` on branch `worktree-<id>` — this is what
`isolation: "worktree"` *produces*, not just a config flag
(`skills/using-git-worktrees/SKILL.md`).

*Doctrine cited: `CLAUDE.md` → Team Directory/Routing, How to Delegate, Git
Worktree Isolation. Hook: `scripts/hooks/guard-worktree-isolation.sh` (real
message quoted above, matching `scripts/demo.sh` Beat 1).*

---

## Step 2 — the engineer builds and the commit is the handoff

`be-sonnet-high-1` works inside its worktree: adds the export endpoint
(queries the full filtered result set server-side, not just the rendered
page) and the button. It never touches the shared main checkout — if it
tried, `guard-main-checkout-writes.sh` would block the write with its own
real message:

```
=== Main-checkout write BLOCKED ===
```

(full message continues with the protected-path and worktree-path detail;
`scripts/demo.sh` Beat 2 exercises this exact hook live). Instead, the
engineer commits on its own branch:

```
$ git -C .claude/worktrees/agent-eng1 commit -m "Add bulk CSV export to Orders list"
[worktree-eng1 a1b2c3d] Add bulk CSV export to Orders list
```

Per `skills/using-git-worktrees/SKILL.md`: **the commit is the handoff** —
no message required and none trusted if one arrives. The teammate goes idle;
the `TeammateIdle` hook appends a durable line to
`idle-events.jsonl` (session team dir) and `TaskCompleted` does the same to
`task-events.jsonl`. The orchestrator never waits on or trusts a chat
message — it reads **ground truth**:

```bash
BASE=$(git merge-base main worktree-eng1)
git log --oneline "$BASE..worktree-eng1"        # the commit(s) in this handoff
git diff --stat "$BASE" worktree-eng1            # what actually changed
git status --short                                # must be empty (fully committed)
```

*Doctrine cited: `CLAUDE.md` → Git Worktree Isolation (commit discipline) and
"Handoff = the commit, not a message." Hook: `scripts/hooks/guard-main-checkout-writes.sh`
(matching `scripts/demo.sh` Beat 2).*

---

## Step 3 — the orchestrator lands: single-writer merge

The orchestrator is the only writer to `main` (`CLAUDE.md` → Git Worktree
Isolation; `skills/rich-lander/SKILL.md`). It runs the land sequence from the
**main checkout**, never from inside a worktree (merging from inside a
worktree merges the branch into itself — a documented no-op trap):

```bash
cd <repo root>                                  # the main checkout
git merge --no-ff worktree-eng1
git status --short                              # empty
git symbolic-ref HEAD                            # refs/heads/main (attached)
git push origin main
```

Then it deploys, per `CLAUDE.md` → Deployment ("what's on `main` MUST be what's
on staging — no exceptions"). This is exactly the real mechanic
`scripts/demo.sh` Beat 7 exercises (`git merge --no-ff` from a throwaway
sample, then `contract-integrity-probe.sh` confirming the wiring is still
intact after the change).

*Doctrine cited: `skills/rich-lander/SKILL.md` (the full land sequence);
`CLAUDE.md` → Git Worktree Isolation, Deployment.*

---

## Step 4 — automation QA: clean pass

Per `CLAUDE.md` → QA Pipeline, step 2 is **automation QA — 10/10 required**
before functional QA ever looks at the work. The orchestrator spawns the
automation QA teammate (`isolation: "worktree"`, same spawn discipline as
Step 1). This run, every automated check passes cleanly: column order
matches the spec, the full filtered result set exports (not just the current
page), a 12,000-row synthetic export completes without timeout, the
file-name is collision-safe and — per the doctrine's "pair a positive-shape
probe" rule — the permission-denied test is paired with a positive
authorized-success test, not left as a negative-only check. Audit committed
to `qa-audits/`.

**This time nothing fails.** `qa-audits/EXAMPLE_AUDIT.md` — the engine's worked
model for this exact fictional feature — shows the shape this step takes
*when it does find something* (a FIX-FIRST bounce, a defect list, a
re-verification pass to 10/10): read it for what a real automation-QA
rejection and recovery looks like. In this walkthrough's run, automation QA
clears on the first pass.

*Doctrine cited: `CLAUDE.md` → QA Pipeline step 2; `CLAUDE.md` → QA doctrine
(positive-shape-probe rule). Worked example: `qa-audits/EXAMPLE_AUDIT.md`.*

---

## Step 5 — functional QA finds a real defect

Step 3 of the QA pipeline: **functional QA — 10/10 required**, human-paced,
on the real target surface (`CLAUDE.md` → QA Pipeline). The functional QA
teammate exercises the feature live rather than trusting code review, per
`CLAUDE.md` → QA doctrine: *"Diagnosis is visual-first — capture what you
saw, then read source to explain it."*

**The defect:** triggering a large export (thousands of rows) shows no
in-progress indicator at all — the button just sits there, indistinguishable
from a hang, for several seconds. This is a genuine, human-perceived defect
functional QA is the right role to catch (automation QA's lane is automated
coverage, not UX judgment — `CLAUDE.md` → QA Pipeline boundaries). Functional
QA fails the gate and commits its audit to `qa-audits/`, citing the missing
progress state with full-scroll screenshot evidence (`CLAUDE.md` → QA
doctrine: "audit screenshots must cover the FULL scroll depth").

**FIX-FIRST bounce:** per `CLAUDE.md` → QA Pipeline, *"any failure at any
step loops back to step 1"* — no partial credit, no shipping around a failed
gate. The orchestrator re-spawns `be-sonnet-high-1` (or the same teammate,
resumed) with the defect: add a visible in-progress state for large exports.
The engineer commits the fix on its worktree branch; the orchestrator lands
it through the same single-writer sequence as Step 3.

**Re-verification, not trust:** functional QA re-runs against the new
commit — never against the original failing one and never taking the
engineer's word for it (the same discipline `qa-audits/EXAMPLE_AUDIT.md`
demonstrates for its own two defects). This time the large-export progress
state shows correctly. **PASS.**

*Doctrine cited: `CLAUDE.md` → QA Pipeline step 3 and boundaries; QA doctrine
(visual-first diagnosis, full-scroll evidence, FIX-FIRST loop-back).*

---

## Step 6 — the design gatekeeper: signoff, with a documented gap

Step 4, the final pipeline step: the **design gatekeeper** — UX audit on the
real target surface, **≥9/10 REQUIRED, with every gap below 10 documented in
the committed signoff file** (`CLAUDE.md` → QA Pipeline). The gatekeeper
reviews the whole feature live — the same discipline
`ui-ux-signoffs/EXAMPLE_SIGNOFF.md` (this exact fictional feature) models in
full: a verification-mode disclosure, a first-glance verdict and a
"Documented gaps" section instead of a silent score.

The gatekeeper confirms the large-export progress state now works (the fix
from Step 5), the underlying data correctness from Step 4's automation pass
holds, empty/error states are handled and navigation is never trapped. But
during this broader pass it notices something functional QA's narrower check
didn't cover: **a *fast* export (small result set) completes with no visible
success acknowledgment at all** — a real but minor trust gap, distinct from
the large-export bug already fixed.

Per `CLAUDE.md` → QA doctrine, this is exactly the distinction a
documented-gaps signoff exists to make: not every imperfection is a blocker.
The gatekeeper's actual committed verdict (`ui-ux-signoffs/EXAMPLE_SIGNOFF.md`):

> **Score: 9/10 — GO.** Held back the 10th point for the missing success
> acknowledgment on fast exports — a real, if minor, trust gap. It does not
> block release: the feature is correct, discoverable and recoverable in
> every state that matters. **Recommendation: ship this feature.** The
> documented gap above should be scheduled as a small follow-up, not looped
> back to the engineer as a blocking fix.

The signoff file is committed to `ui-ux-signoffs/` — never a silent 9, per
doctrine.

*Doctrine cited: `CLAUDE.md` → QA Pipeline step 4. Worked example:
`ui-ux-signoffs/EXAMPLE_SIGNOFF.md` (cited verdict is that file's actual
content).*

---

## Step 7 — only now does the CEO see it

Per `CLAUDE.md` → QA Pipeline: *"The owner/CEO sees NOTHING before the
gatekeeper's signoff file exists — not a preview, not 'it's basically done,'
nothing."* The signoff file from Step 6 is what changes that. Only now does
the orchestrator tell the CEO the export button shipped — with the one
documented, non-blocking gap named plainly, not buried.

Per the write-back rule (`CLAUDE.md` → "The Orchestrator as COO"), if the CEO
reacts to the feature with any stated preference — "always ship the toast
follow-up within the week," say — Rich proposes a Loro record or correction with its conversation source. The
existing confirmation desk writes it after confirmation, so the next similar
decision can retrieve the recorded preference. Operational follow-ups belong in
ECS and do not become knowledge corrections.

---

## What's illustrative here vs. what's real and runnable

| In this document | In `scripts/demo.sh` |
|---|---|
| The full four-step QA pipeline (automation → functional → gatekeeper), a wiki consultation, a design signoff with a documented gap | A single scripted "QA check," narrated as a stand-in for a live QA agent's verdict (its own `[NARRATED SIMULATION]` labeling) |
| Illustrative teammate names, SHAs and hook output reproduced from the real source | The literal hook binaries and real git mechanics, run live, right now, on your machine |
| A story you read | A verdict you watch happen — `0`/`2` exit codes, real refusals, a real merge, a real probe pass |

Run `scripts/demo.sh` for the runnable proof of the mechanics this document
narrates the full lifecycle around. Read `qa-audits/EXAMPLE_AUDIT.md` and
`ui-ux-signoffs/EXAMPLE_SIGNOFF.md` for the two worked artifacts this
walkthrough stitches into one story.
