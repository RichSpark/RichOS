---
name: bootstrap-interview
description: First-session orchestrator skill — interviews the CEO conversationally to fill CLAUDE.md and orchestration.config, staff the initial domain roster via Dean and seed confirmed knowledge in a private Loro corpus, in one guided ~20-minute session. Use on first session with a fresh, un-adopted copy of this engine or whenever CLAUDE.md/orchestration.config still carry unfilled sample content.
---

# Bootstrap Interview

**Audience: the orchestrator, running this in its own session** (not a
spawned subagent — this is the orchestrator's own bounded self-configuration
work; see "Why the orchestrator does this directly," below). Run this the
first time you're operating in a freshly copied/cloned engine, before you do
anything else — or any time you detect `CLAUDE.md` or `orchestration.config`
still carry unfilled sample content (Stage 0 tells you how to check).

## The promise and the discipline that protects it

The promise is: **the CEO answers questions for about 20 minutes and ends
with a staffed company** — a filled `CLAUDE.md`, a filled
`orchestration.config`, real named teammates registered and spawnable and a
the private Loro corpus that already contains what the CEO said. That promise is fragile
in two opposite directions and both are non-negotiable:

- **Too shallow and everything downstream is under-filled** — Dean can't
  staff a role he has no domain context for and the wiki's first pages are
  thin.
- **Too deep and the 20-minute promise breaks** — this is a conversation,
  not a deposition. Ask naturally, follow the stages below as a checklist
  in your own head, not a script to read verbatim and move on the moment a
  stage has enough to work with. Short sessions are fine — this is
  **resumable** (see "Resumability" below); a CEO who has to stop after
  Stage 2 can pick back up later without redoing anything.

**Never fabricate.** Every `CLAUDE.md`/`orchestration.config` value you fill
must trace back to something the CEO actually said. If a topic never came up
or the CEO says "not sure yet" / "decide later," leave the TODO block in
place, explicitly marked deferred (see "Honesty rules" below) — never
invent a plausible-sounding answer to make the session look more complete
than it is. A confidently-wrong `CLAUDE.md` is worse than an honest TODO.

## Prefer voice?

Answering by typing isn't required. Built-in OS dictation works today, needs
nothing from this engine and turns this into a spoken conversation: **macOS**
Dictation (press the mic key or Edit menu → Start Dictation) or **Windows**
voice typing (`Win+H`). Either dictates straight into the chat with the
orchestrator, so the live interview below just becomes a spoken one.

Prefer to do the whole thing away from a terminal entirely — on your phone, in
a voice assistant, before you've even opened this engine's repo? See "Transcript
mode" next: `skills/bootstrap-interview/references/portable-interview-prompt.md`
runs this same interview in any chat/voice assistant and hands you back a
transcript to drop in `ceo-inbox/for-wiki/`.

## Why the orchestrator does this directly (not a delegated task)

Filling your own operating manual and config from an interview you conducted
yourself is orchestrator self-administration, not product/feature work — the
same class of bounded exception as running the land sequence or maintaining
the private Loro corpus (see `CLAUDE.md`'s Core Guardrail — this is the third of the
three reserved exceptions to "never carry out work directly"). You do NOT
delegate the interview, the `CLAUDE.md`/`orchestration.config` fill or the
wiki-seeding to a teammate. You DO delegate the actual role instantiation to
Dean (Stage G3) — staffing has always been Dean's job, interview or not.

## Resumability — record progress, never guess at prior state

Keep a running progress note at `.claude/state/bootstrap-interview-progress.md`
(already gitignored by the engine's own `.gitignore` — machine-local, never
committed). After each stage, append what was covered and the raw answers
(not yet distilled prose — just enough for a resumed session to pick up
context without re-asking). This file is **advisory, not authoritative** —
if it's ever missing or you're unsure of real state, re-derive from ground
truth per Stage 0 below, the same durable-substrate-over-message discipline
the rest of this engine's doctrine follows. Delete or archive the progress file
once bootstrapping is complete (Stage G5's checklist passes) — it has no job
after that point.

## Stage 0 — detect state before you ask anything

Before your first question, check what's already true, so you never re-ask
answered questions or clobber prior progress:

- `grep -c '<!-- TODO (adopter)' CLAUDE.md` (or `CLAUDE.md.template` if
  `CLAUDE.md` doesn't exist yet at the repo root) — a nonzero count means
  unfilled blocks remain.
- `grep -n 'PROTECTED_PATHS="app packages"' orchestration.config` — the
  literal shipped sample; still present means config is unfilled.
- `ls .claude/agents/*.md` minus the four meta-roles (`dean`, `clark`,
  `reed`, `frank`) — any additional real (non-template) file means some
  staffing has already happened.
- `.claude/state/bootstrap-interview-progress.md` — if present, read it and
  resume from the last completed stage instead of starting over.
- `grep -rl "BOOTSTRAP-INTERVIEW-TRANSCRIPT" ceo-inbox/` — a completed
  interview transcript conducted elsewhere (see "Transcript mode" below) and
  dropped in the inbox. If found, this takes priority over a live interview.

Four outcomes:
1. **Nothing above** → fresh start, Stage 1 (live interview).
2. **A transcript found** → **Transcript mode** (below) — skip the live
   interview, extract answers from the file instead.
3. **Partial signal** (some TODOs filled, some agents staffed or a progress
   file exists, no transcript) → resume: tell the CEO plainly what's already
   done, confirm before continuing, pick up at the first incomplete stage.
4. **Fully bootstrapped already** (zero TODOs, config filled, roles staffed,
   wiki has real pages) → this skill has nothing left to do; say so and
   point to Dean directly for adding a *new* role later — that's ordinary
   staffing, not re-bootstrapping.

## Transcript mode — when the interview already happened elsewhere

**Prefer voice or already talked this through with another assistant?** The
CEO doesn't have to run the live interview with the orchestrator at all.
`skills/bootstrap-interview/references/portable-interview-prompt.md` is a
self-contained prompt the CEO can paste into any voice or chat assistant
(Claude, ChatGPT, a phone voice app) to have the *same* conversation there —
by voice if they prefer (macOS Dictation, Windows `Win+H` or a voice-mode
assistant), then export the transcript and drop it in
`ceo-inbox/for-wiki/`. See "Prefer voice?" further down for the two OS
dictation options — those work today, need nothing from this engine and are
worth mentioning to the CEO even outside the portable-prompt path.

When Stage 0 finds a transcript (its content contains the literal marker
`BOOTSTRAP-INTERVIEW-TRANSCRIPT`, per the portable prompt's own
instructions):

1. **Read the whole transcript** before extracting anything — same
   discipline as any other source ingest.
2. **Extract answers per stage** (the same six categories as "The interview
   stages" below — product & domain, users & surfaces, stack/deploy, team
   shape, quality bar & hard rules, escalation preferences). Map what the
   transcript actually says onto each stage; don't assume a stage was
   covered just because the transcript is long.
3. **Unanswered stays TODO, exactly as in a live interview.** If the
   transcript never touched a stage or the CEO's answer there was itself
   "not sure yet," that stage's `CLAUDE.md`/`orchestration.config` block
   stays an explicit TODO — extraction never fabricates an answer the
   transcript doesn't actually contain, any more than a live interview would.
4. **Run the exact same Generation phase (G1–G5) below** — nothing about
   G1–G5 changes based on entry path.
5. **Return to the CEO only for discuss-before-write confirmations** (G4's
   page-plan proposal) and anything genuinely ambiguous in the transcript —
   not to re-ask questions the transcript already answered clearly. This is
   still a conversation, just a shorter one: confirm the plan, don't re-run
   the interview.

Once extraction + generation complete, move the transcript file itself into
an explicitly agreed private source archive only if that move preserves its existing references. Otherwise retain the original file and cite it in place. Do not create `ceo-wiki/` during new installation.

## The interview stages

Ask conversationally, in roughly this order, adapting phrasing naturally.
Each stage lists what you need by the end of it — not a fixed question list.

### Stage 1 — Product & domain
What is this, in one or two sentences? Who is it for? What's the core
promise/category? Is there an existing name or still deciding? (Feeds
`CLAUDE.md`'s intro and a confirmed vision page in the selected private company corpus.)

### Stage 2 — Users & surfaces
Is there more than one user-facing surface (web/native/admin/multiple apps
sharing a backend)? If so, map them and ask for (or propose) a canonical
topology doc name. Single surface → note that and skip the Surfaces TODO
block cleanly (state explicitly "single surface, no map needed" rather than
leaving it silently blank). (Feeds `CLAUDE.md`'s "Surfaces — WHO USES WHAT.")

### Stage 3 — Stack, repo layout & deploy
Real source-tree names (→ `orchestration.config`'s `PROTECTED_PATHS`);
language/framework per surface; deploy target(s) and, if more than one, which
paths map to which deploy script (→ `CLAUDE.md`'s Deployment TODO table); any
existing CI. If the product has native mobile apps, ask for the Android/iOS
source roots too (→ `orchestration.config`'s `APP_ROOT` /
`NATIVE_ANDROID_ROOT` / `NATIVE_IOS_ROOT` — leave blank if not applicable,
per that block's own instructions).

### Stage 4 — Team shape
Walk `.claude/agents/templates/` categories at a level the CEO can answer
without reading code: do they need an architect, a CTO, which kinds of
engineers (backend/frontend/full-stack/infra/mobile — and which platforms),
which QA roles (automation is close to universal; functional and the design
gatekeeper are the other two mandatory pipeline steps; adversarial-visual and
device QA only matter if there's more than one native platform to keep in
parity), a copywriter, marketing, domain advisors, a domain expert. Don't
recite all 16 templates as a checklist question — infer likely roles from
Stages 1-3 and confirm/adjust with the CEO, e.g. "sounds like you'll want a
backend engineer, a frontend engineer and the three mandatory QA roles to
start — anything else or does that cover it?" For each confirmed role,
capture enough for Dean/Clark to work with: the role, anything specific
about this domain that shapes the persona (e.g. "backend engineer, this is a
Node/Postgres API") and whether it's needed now vs. later.

### Stage 5 — Quality bar & hard rules
State plainly that the QA pipeline ships with proven default bars (10/10
automation, 10/10 functional, ≥9/10 gatekeeper with documented gaps) and ask
only whether the CEO wants to **keep** them (the recommended default) or
**override** — don't imply the bars are up for casual negotiation. Then ask
for the product's real non-negotiable hard rules (data-handling,
accessibility, brand, compliance — whatever replaces the shipped "no
pagination" example). If nothing comes to mind yet, that's a valid answer —
leave the block as an explicit TODO, not a fabricated rule.

### Stage 6 — Escalation preferences
What does the CEO consider genuinely their call vs. safe for the
orchestrator to decide autonomously? Spending thresholds, irreversible
external commitments, anything they want to always see before it happens.
This doesn't need to be exhaustive — the escalation ladder in `CLAUDE.md`'s
"The Orchestrator as COO" section already covers the general shape (wiki
answers it → act; operational+unrecorded → decide; genuine CEO-level →
escalate); this stage just captures any CEO-specific overrides or examples
worth recording as the wiki's first precedent.

## Generation phase — after the interview, before you report back

### G1 — Fill `CLAUDE.md`

Copy `CLAUDE.md.template` to `CLAUDE.md` if it doesn't exist yet. The template
carries **ten** `<!-- TODO (adopter) -->` blocks — enumerate and resolve **every
one** (a "finished" `CLAUDE.md` with a live TODO comment still sitting in it is
the F3 trap). Each block has a definite resolution: fill it, delete it or
explicitly defer it — never leave it silent. Walk the full set:

1. **Surfaces map** — fill from Stage 2; or, for a single-surface product,
   delete the block cleanly (the block's own inline instruction says so — state
   "single surface, no map needed" rather than leaving it blank).
2. **Team Directory (domain team)** — **leave this block for Dean.** He fills it
   in his normal per-hire workflow in G3; filling it here would duplicate or
   conflict with his update. This is the ONE block you do not touch.
3. **Routing** — fill from Stages 1–4 (who owns what, escalation flow).
4. **Deploy table** — fill from Stage 3; keep the deploy-always rule intact.
5. **Product Hard Rules** — fill from Stage 5. Note the heading is already the
   real one ("Product Hard Rules"); only the example `no pagination` bullet is
   the placeholder — replace the bullet, do NOT ship your real rules under an
   "example" heading. If nothing came up, leave the block an explicit TODO.
6. **Version-control notes** — fill if any history/migration/banned-command note
   came up; otherwise delete the block (its own instruction says so).
7. **Additional repo-root directories** — add any product-specific top-level
   dirs the CEO named (e.g. one per deployable service); otherwise delete it.
8. **Freshness Contract (advanced tier, OPTIONAL)** — **delete this whole
   section unless** you actually adopted `reference/advanced-tier/` and have an
   analogous deploy/device pipeline. Do not leave its TODO comment live.
9. **Client Data-Render Contract (advanced tier, OPTIONAL)** — **delete this
   whole section unless** you set `ENABLE_QA_INSTALL_FRESH_GATE=1` in G2 for a
   real device/install-fresh render pipeline. Do not leave its TODO live.
10. **QA-pipeline teammate mapping** — this block maps the four QA steps to
    real teammate NAMES, which don't exist until Dean staffs them in G3. **You
    cannot fill it in G1** — leave it as a marked TODO now and complete it in
    the G3-reconciliation step (see G3), citing it here so it isn't forgotten.

Any block with no CEO answer stays an explicit, still-visible TODO — never
delete a TODO block just to make the file look finished (deferral is honest;
silence is not). The only two blocks that legitimately remain unfilled when you
report back are the Team Directory (Dean fills it in G3) and the QA-pipeline
mapping (you finish it in the G3-reconciliation step).

### G2 — Fill `orchestration.config`

Set `PROTECTED_PATHS` to the real source trees from Stage 3. Set
`APP_ROOT`/`NATIVE_ANDROID_ROOT`/`NATIVE_IOS_ROOT` if native mobile came up,
otherwise leave them blank (per the file's own instructions — blank is a
valid, intentional answer, not an omission). Leave
`READONLY_ALLOWLIST`/`CREATOR_TEAMMATE`/`ARTIFACT_*` at
their shipped defaults unless the interview surfaced a reason to change
them — these are platform-generic, not domain-specific and shouldn't be
touched without cause. Leave `ENABLE_QA_INSTALL_FRESH_GATE=0` unless the CEO
described exactly the device/install-fresh pipeline that gate is for.

### G3 — Staff the roster via Dean

Compile the confirmed roles from Stage 4 into one packet: for each role, the
template it maps to (`.claude/agents/templates/<template>.md`) and the
domain context captured for it. Spawn Dean (`subagent_type: dean`, a proper
truthful `<role>-<model>-<effort>-<identifier>` name with the matching `effort`,
`isolation: "worktree"` — Dean
writes files) with a single task
covering the whole batch: instantiate each confirmed role from its template
into a real named `.claude/agents/<slug>.md` + `team/<name>.md`, update
`team/ROSTER.md` and `CLAUDE.md`'s Team Directory (Dean's normal workflow
already does this — don't re-specify it, just don't do it yourself first).
Tell Dean to ask Clark to research any role where the domain context is
thin. Land Dean's branch through the normal land sequence once the commits
exist — this is an ordinary teammate handoff, not a special bootstrap case.

**Naming — offer the standard roster or custom.** Dean names hires per the team
naming convention in `team/NAMING.md`. Give the CEO the one choice that matters
here: **keep the standard mnemonic roster wholesale** (Milo/Archie/Hawke/Nix/Vince…
— the mnemonics bind to roles, not products, so they transfer to this domain) or
**invent fresh names under the same principles** (short, distinctive, easy to
say/type, distinct first letters). Both are correct; point them at `team/NAMING.md`
and pass their choice to Dean in the G3 packet. Absent a preference, Dean defaults
to the standard roster for matching roles and convention-conforming inventions for
novel ones.

**G3-reconciliation — finish the QA-pipeline teammate mapping (the block G1
deferred).** Dean's workflow fills `CLAUDE.md`'s *Team Directory* block, but NOT
the separate *QA-pipeline mapping* block (the one that maps step 2 =
automation-QA, step 3 = functional-QA, step 4 = design-gatekeeper to your actual
named teammates). Nobody else owns it, so once Dean's roster has landed and the
QA roles have real names, **you** fill that block yourself here — the QA-pipeline
names must match the QA roles Dean just staffed. This closes the F4 sequencing
gap: the block that needed names G1 didn't have yet is completed the moment the
names exist. Keep the bars, the loop-back rule and the CEO-sees-nothing-before-
signoff rule exactly as shipped.

### G4 — Seed confirmed knowledge through Loro

Use `loro/docs/authoring.md` and the selected external private corpus. The app
supplies that corpus and the selected company. A terminal adopter must choose the
private location and stable company ID explicitly; never create a knowledge store
inside the engine or public product repository.

Propose the specific records or pages to create and confirm the content under the
existing discuss-before-write contract. Reuse an explicit approval already given
in this interview. Keep unanswered questions unresolved. Typed beliefs go through
`loro/bin/loro-write.mjs` with the explicit central root, partition, scope and
source reference. The app uses its existing correction desk. Do not add another
writer or confirmation mechanism.

For explanatory pages, use `loro/templates/page.md.template` in the selected
company's private pages directory. Cite actual interview evidence. A supplied
transcript keeps its original reference; do not move or rewrite it merely to fit a
new folder convention. Operational commitments belong to ECS rather than belief
records. Report the writer's actual receipts and any refused or unresolved item.
Do not claim that setup has migrated earlier personal context.

### G5 — Verification pass

Initialize the adopter repository's CEO TODOs before checking completion:

```bash
scripts/ceo-todos-init.sh /path/to/adopter-repository
```

Use the actual repository path established in this interview. If `.ceo-todos`
already exists, preserve its record and run `scripts/ceo-todos-lint.sh` against
that repository instead. Never use `--force` to replace existing commitments.
The initializer creates the declaration, renders `CEO-TODOs.md` and links it
from the root README. Resolve any reported lint failure before continuing.

Run, in order and show the CEO the real output, not a paraphrase:

```bash
scripts/hooks/install.sh
scripts/hooks/contract-integrity-probe.sh
scripts/demo.sh
```

Present the green summary plainly: hooks wired (committed
`.claude/settings.local.json`; `install.sh` mints the sidecars), every layer
A–Q confirmed (`contract-integrity-probe.sh` — Layer N's git-tracked check may
read `⚠ … not yet git-tracked` if you haven't committed the adoption yet; that
warning is fine, a hard `✗ N` is not — commit `settings.local.json`, force-adding
past any global-gitignore rule with `git add -f` if the probe says so) and the
full 7-beat enforcement loop passing (`scripts/demo.sh`, README's "60-second
proof"). If anything fails, stop and fix it before declaring the session done —
never report "bootstrapped" over a red verification pass.

## Definition of "bootstrapped" — the completion checklist

- Zero remaining `<!-- TODO (adopter) -->` blocks in `CLAUDE.md` that aren't
  explicitly marked deferred by the CEO.
- `orchestration.config` has no unexamined shipped sample literals — every
  value is either a real, confirmed answer or a deliberate, stated "leave at
  default."
- At least the roles confirmed in Stage 4 are registered as real named
  `.claude/agents/<slug>.md` + `team/<name>.md` pairs and `CLAUDE.md`'s Team
  Directory + `team/ROSTER.md` list them.
- Confirmed Loro records or pages exist in the selected private corpus with actual source references and writer receipts; declined or unanswered content remains unwritten.
- `install.sh` + `contract-integrity-probe.sh` + `scripts/demo.sh` all ran
  green in this same session.
- The repository carries a `.ceo-todos`, `CEO-TODOs.md` exists at its root and
  is named in the first lines of `README.md` and `scripts/ceo-todos-lint.sh`
  ran clean in this same session. A shipped TODOs mechanism with no declaration
  is enforcement that can never fire — "installed" is not the same as "on".

Only once every item above is true do you tell the CEO the company is
staffed and ready — and only then, per "Resumability" above, retire the
progress file.

## Honesty rules (worth repeating — these are the whole point)

- Unanswered → TODO, always. Never fabricate a fact the CEO didn't state,
  no matter how plausible it would look filled in.
- If a stage runs short on time, say so and record exactly where you
  stopped — don't quietly skip ahead and let the CEO assume more was covered
  than was.
- A resumed session re-confirms what Stage 0 detected before continuing —
  never silently assume prior progress is still accurate; the CEO may have
  changed their mind between sessions.
