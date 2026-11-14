# Skill Library

Skills are self-contained knowledge packages (`SKILL.md` + optional reference
files) that a teammate loads for a specific kind of work. This directory is the
engine's **installed** skill set — see `docs/briefs/` conventions and Dean's Skill
Assignment Workflow (`.claude/agents/dean.md`) for how new skills get added over
time via `hr-inbox/team-skills/`.

Exported per the skill-export manifest (2026-07-14)
in the source project — every skill below was read in full and classified as
one of: **as-is** (zero project content, ship unmodified), **scrubbed** (had
project literals, mapped to engine config/role-template conventions) or
**template** (the body is inherently per-project; ships as a structured TODO
slot). Two skills from the source project's library were excluded outright and
are not part of this engine at all: a product-domain skill with no portable core,
and a banned version-control (`jj`) workflow skill that contradicts this engine's
Git-only doctrine.

**2026-07-14 correction (hostile-buyer review):** the original export pass
missed source-product role vocabulary ("coach"/"client" as product roles) in
three files claimed as "as-is"/"scrubbed" — `live-app-assessment/SKILL.md`,
`ui-ux-design/references/ux-heuristics.md` and
`health-data-sync-contracts/SKILL.md`. All three are now genericized (see
their entries below) and the whole library was swept for the same vocabulary
class; no further instances found. The lesson: "zero project content" is a
claim that must be verified by grep, not by memory of the export pass.

## Meta-role skills (doctrine, not domain)

| Skill | Status | Purpose |
|---|---|---|
| `using-git-worktrees` | as-is | The teammate worktree workflow — isolation, atomic commits, commit-is-the-handoff, courtesy summaries. Read this before writing your first file. |
| `rich-lander` | as-is | The orchestrator's per-handoff land sequence — single-writer-to-main, durable-signal detection, serialized landing. Orchestrator-only. |
| `bootstrap-interview` | as-is (engine-authored) | First-session orchestrator skill — interviews the CEO, fills `CLAUDE.md`/`orchestration.config`, staffs the initial roster via Dean and seeds confirmed knowledge in the private Loro corpus. Orchestrator-only, run once per adoption. |

## Ship-as-is (14) — zero project content, proven byte-identical to an independent instantiation

| Skill | Purpose |
|---|---|
| `android-emulator-qa` | Third-party vendored Android-emulator QA workflow (includes Python helper scripts). |
| `copywriting` | Generic conversion-copy skill with frameworks and worked examples. |
| `customer-research` | Conducting, analyzing and synthesizing customer research / voice-of-customer data. |
| `frontend-design` | Generic "distinctive UI" front-end design skill. |
| `live-app-assessment` | Live-staging assessment workflow — inspect the real running product, not just code/screenshots. |
| `marketing-psychology` | Reference of psychological/mental-model frameworks for marketing work. |
| `marketing-strategy-pmm` | Third-party product-marketing-management strategy skill. |
| `playwright-cli` | Reference for the Playwright CLI (mocking, sessions, storage state, test generation, tracing, video). |
| `playwright-e2e-testing` | Generic Playwright E2E test-framework authoring reference. |
| `product-marketing-context` | Framing a product's market context for marketing/positioning work. |
| `resend` | Official vendor skill (v3.5.0) for the Resend transactional-email API — sending, receiving, webhooks, templates, API keys, broadcasts, contacts/segments, domains, events/logs, automations. |
| `svelte-code-writer` | Thin wrapper skill for writing Svelte code via the `@sveltejs/mcp` CLI. |
| `svelte-core-bestpractices` | Official upstream Svelte 5 documentation and best-practice references. |
| `use-railway` | Railway operations skill — projects, services, object storage, deploys, environments, domains, feature flags, MCP-based agent tooling and unattended/device-code sign-in. |

## Ship-scrubbed (7) — native/mobile-QA cluster, project literals mapped to engine conventions

| Skill | Purpose | What was scrubbed |
|---|---|---|
| `android-native-dev` | Native Android app development conventions (Kotlin/Compose, Health Connect, WorkManager, security, release checklist). | Product name → `${APP_ROOT}`; dropped dead cross-refs to excluded skills; fixed a stale `jj` version-control reference to `using-git-worktrees`. |
| `ios-native-dev` | Native iOS app development conventions (Swift/SwiftUI, HealthKit, background tasks, security, release checklist). | Same pattern as `android-native-dev`, iOS-flavored. |
| `appium-vision-mobile-testing` | Strict human-like black-box mobile testing policy via Appium (screenshot/recording-only observation, tap/swipe-only action). | Product name → `${APP_ROOT}` / generic "target app". |
| `health-data-sync-contracts` | Cross-platform health-data contract discipline (HealthKit/Health Connect ↔ backend ↔ QA — one metric means one thing everywhere). | One product-name literal genericized, plus a role-specific "coach-facing" reference generalized to "user-facing" (2026-07-14 hostile-buyer review); the entire contract methodology is portable as-is. |
| `mobile-qa-reporting-and-device-matrix` | Native mobile QA coverage planning and structured bug reporting across a real device matrix. | One product-name literal genericized. |
| `mobile-release-and-build-ops` | Native build/release workflow discipline (signing, TestFlight/Play Console, done-criteria). | Product-specific "Repo Rules" section collapsed to an adopter TODO stub. |
| `native-client-visual-qa` | Mandatory three-column (platform A / platform B / Mismatch) prose comparison format for native visual-parity audits — no silent `✓`. | Named teammates → role-template placeholders (functional QA / device QA / design gatekeeper / orchestrator); install-fresh script citations mapped to `reference/advanced-tier/{android,ios}-install-fresh.sh` gated by `ENABLE_QA_INSTALL_FRESH_GATE` in `orchestration.config` (never bare-deleted). |

## Template-only (1) — structure ships, body is a per-project slot

| Skill | Purpose |
|---|---|
| `ui-ux-design` | Senior UI/UX design & audit skill. `SKILL.md`'s Product Context / Quality Bar references / heuristics are TODO-marked slots — proven by an independent sibling project having re-filled the identical scaffold for its own product. The three `references/*.md` engine files (senior judgment, UX heuristics, output formats) ship as-is. |

## Marketing-surface skills (vendored, scope-pinned) — 2026-08-23

| Skill | Status | Purpose |
|---|---|---|
| `landing-page-taste` | vendored, scope-pinned | Anti-slop frontend taste skill for landing pages, portfolios and redesigns — brief inference, design-system selection, pre-flight checks. Vendored verbatim (below a binding scope pin) from `taste-skill`'s `skills/taste-skill/SKILL.md` @ `72e299530e2eb31ed8da06181bc19f6c18a00821` (MIT, `LICENSE` alongside). **Marketing surfaces only — never product UI.** |
| `landing-page-redesign` | vendored, scope-pinned | Audits an existing marketing site, flags generic-AI patterns and applies high-end design standards without breaking functionality. Vendored verbatim (below a binding scope pin) from `taste-skill`'s `skills/redesign-skill/SKILL.md` @ `72e299530e2eb31ed8da06181bc19f6c18a00821` (MIT, `LICENSE` alongside). **Marketing surfaces only — never product UI.** |

Unlike the rest of this library, these two ship as unmodified upstream
third-party content (MIT-licensed) with a prepended scope pin, not
as-is/scrubbed/template engine exports — see each `SKILL.md`'s pin block
for the re-vendor procedure and each directory's `LICENSE`.

## Installed-tooling skills (engine-authored) — 2026-10-07

| Skill | Purpose |
|---|---|
| `transcribe-audio` | Any audio or video file to a timestamped transcript with one command (`transcribe.sh`), through RichOS's own transcription pipeline (`richos/tools/richos-service`) and the whisper.cpp model already on this Mac. Exists so nobody downloads Whisper again. |
| `watch-video` | A video or screen recording to a transcript plus still frames at every scene change and every 5 s, interleaved in one index (`watch.sh`). Reuses `transcribe-audio` for the sound. |

## Not exported

- A product-domain coaching-habit skill with no portable core once its product and named-persona dependencies are removed.
- A `jj` (Jujutsu) version-control workflow skill — this engine is Git-only; `jj` commands are out of scope entirely and must never ship in any form.

## Using a skill

Reference a skill from an agent definition with a one-line load-bearing mention
and its path — never paste its contents into another file and never inventory
it as a table entry unless you're Dean updating the "Skills" table per his
Skill Assignment Workflow. Role templates that already have an obvious skill
pairing (QA ↔ Playwright/emulator/Appium skills, frontend ↔ Svelte skills,
marketing/copywriter ↔ marketing skills) cite the relevant skill(s) directly —
see `.claude/agents/templates/`.
