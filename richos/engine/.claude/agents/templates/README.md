# Role templates — skeletons, NOT live agents

These files are **skeleton role templates**, distilled from a real 21-persona
production team into clean, reusable starting points. They are deliberately **not
spawnable agents**: each frontmatter block carries a placeholder `name:` (e.g.
`TODO-architect-slug`) that does NOT match its filename, so the harness will not
register any template as an agent type.

## How to turn a template into a live teammate

**Dean** (the HR meta-role) does this — never spawn a template directly:

1. Pick the template for the role you need.
2. Have **Clark** research the role for your specific domain if depth is required.
3. **Naming your hires:** before you copy the template, pick the name per
   `team/NAMING.md` — the canonical roster is recommended as-is, or invent one
   under the same floor (short, distinctive, easy to say and type). Copy the
   template to `.claude/agents/<slug>.md` with a **real kebab `name:` that
   matches the new filename**, built from the name you picked.
4. Fill in the frontmatter (`name` / `description` / `model` / `tools`) and the
   body: real persona, real stack, real surfaces, real guardrails — grounded in
   the research, following each template's "What to customize" checklist.
5. Write the matching `/team/<name>.md` HR profile (see `/team/templates/`).
6. Register the hire in `CLAUDE.md`'s team directory and `/team/ROSTER.md`.

## Model tiering

Every template defaults to `opus` (CEO 2026-11-13). The tier is set per template — never inherit it from the lead.

## Template inventory (16 templates ← 20 source personas)

Frank (devil's advocate) ships as a **live agent** in `.claude/agents/frank.md`,
not a template — he is near-generic. The remaining 20 domain personas collapse into
these 16 templates:

| Template file | Covers (source personas) | Tier |
|---|---|---|
| `architect.md` | software architect | opus |
| `cto.md` | CTO / sprint-planning lead | opus |
| `backend-engineer.md` | backend engineer | opus |
| `frontend-engineer.md` | frontend engineer (+ hands-on front-end designer variant) | opus |
| `fullstack-engineer.md` | full-stack / feature-domain engineer | opus |
| `infra-engineer.md` | infrastructure / deploy engineer | opus |
| `mobile-engineer.md` | native mobile engineer — instantiate once per platform (Android, iOS) | opus |
| `automation-qa.md` | automation QA (regression/perf/security) | opus |
| `functional-qa.md` | functional QA / user advocate | opus |
| `adversarial-visual-qa.md` | adversarial visual QA (2nd non-collusive key) | opus |
| `device-qa.md` | native device QA (install/data/sync/push) | opus |
| `designer-ux-gatekeeper.md` | UX-quality gatekeeper + hands-on front-end designer | opus |
| `copywriter.md` | conversion copywriter | opus |
| `marketing.md` | marketing director | opus |
| `domain-advisor.md` | target-customer voice — instantiate one per persona in your panel | opus |
| `domain-expert.md` | specialist domain expert (e.g. gamification/engagement) | opus |

Collapses made: the two native-mobile personas (Android + iOS) share one
`mobile-engineer.md` (instantiate per platform); the three target-customer voices
share one `domain-advisor.md` (instantiate per persona); the hands-on front-end
designer and the UX-quality gatekeeper share `designer-ux-gatekeeper.md` (two
documented variants). Each template names its origin role in its "What to
customize" section.
