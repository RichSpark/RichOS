# Teammate Naming Convention — the Mnemonic Roster

Every teammate gets a short human name, not a role string. `backend-engineer-2` is a
slot; **Milo** is a colleague. Names that read like people make the roster easier to hold
in your head, faster to say out loud and pleasant to work with over hundreds of spawns.
This doc is the convention for choosing those names. Dean proposes names that conform to it
by default; the orchestrator and the CEO can always override.

Two layers: a **mandatory floor** every name must clear and an **optional mnemonic bonus**
that binds a name to its specialty when one comes naturally.

## The floor (mandatory)

Every teammate name MUST be:

- **Short** — usually one syllable (Max, Lou, Vince, Wynn, Hawke, Nix, Jinx, Gus, Piers,
  Tess, Ash, Pax, Frank, Reed, Clark, Dean). Two syllables is the ceiling and only when it
  earns it (Archie, Anders, Una, Ivo, Milo).
- **Distinctive** — instantly separable from every other teammate in speech and in text.
- **Easy to say and easy to type** — no awkward clusters, no homophone traps, no names a
  human stumbles over when talking to the orchestrator about the team.
- **Distinct first letters across the roster, as much as possible** — so a name can be
  recognized from its initial and two teammates rarely collide on a glance. This is a strong
  preference, not a hard uniqueness rule. Where a collision is genuinely unavoidable, it
  carries an obligation: the colliding names must then be **very distinct from each other
  overall** — unmistakable in full even though they share an initial. The standard roster
  demonstrates it: **Archie / Anders / Ash** all start with A yet are impossible to confuse;
  likewise **Max / Milo** and **Piers / Pax**. A strong mnemonic can be worth a shared
  initial, provided the names stay unmistakable.

A name that clears the floor is already a good name. The mnemonic layer is a bonus on top,
never a prerequisite.

## The mnemonic layer (optional bonus)

When it comes naturally, attach a **mnemonic hook** that binds the name to the teammate's
specialty — so the role is recallable from the name alone. **Vince** channels da **Vinci**
(front-end design). **And**ers does **And**roid. **Nix** nixes flaky tests. These are
gifts to the human running the team, not rules the name must satisfy. Force nothing: a
strained mnemonic is worse than none.

## Three doctrine lessons

The convention has been used long enough to surface three things worth writing down.

1. **Retrofits are legitimate.** The mnemonic serves the human, so inventing one *after* the
   name is already in use is completely fine — the value is the same whether the hook came
   first or second. **Jinx's** hook ("**Jinx** jinxes a premature sign-off" — the visual QA
   counter-key) was retrofitted onto a name chosen first and it works as well as if it had
   been designed in.

2. **Mnemonics are disposable scaffolding.** Even a mnemonic you later *forget* has already
   done its job: it helped the name→role association set and that association is durable long
   after the hook itself is gone. **Lou's** original mnemonic has been forgotten entirely,
   yet "Lou is the copywriter" is rock-solid. Don't mourn a lost hook and don't feel obliged
   to reconstruct one.

3. **When no mnemonic comes, the floor alone suffices.** Several excellent names carry no
   hook at all (see "Floor-only names" below). A short, distinctive, easy name is a complete,
   correct name. Never hold up a hire waiting for a clever mnemonic to arrive.

## The canonical roster

This is the standard roster, name→role with its mnemonic. **The mnemonics bind to ROLES, not
to any product** — "Vince does front-end design" transfers to any codebase — so an adopter may
**keep this roster wholesale** or **invent their own under the floor + mnemonic principles
above**. Either is correct.

### Meta-roles (ship with the engine)

| Name | Role | Mnemonic |
|------|------|----------|
| **Dean** | HR Director — hires/creates teammates | Felt right for the hiring role. *(Retrofit hook available — see below.)* |
| **Clark** | Senior Researcher | Clark Kent, the reporter — a perfect reporter-researcher name. |
| **Reed** | Source-Reading & Knowledge-Extraction Specialist | **Reed** → *read*; the reading specialist. |
| **Frank** | Expert Advisor / Devil's Advocate | Always **frank** with you — brutally honest stress-testing. |

### The standard domain roster

| Name | Role | Mnemonic |
|------|------|----------|
| **Archie** | Software Architect | **Arch**ie draws the **arch**itecture. |
| **Gus** | CTO | Chosen for fit rather than for a hook — a legitimate non-mnemonic origin. *(Retrofit hook available — see below.)* |
| **Max** | Marketing Director | **Max**imise reach — the growth drive behind go-to-market. |
| **Lou** | Conversion Copywriter | *Original mnemonic forgotten* — the canonical "disposable scaffolding" example (lesson 2). The name→role bond survives regardless. |
| **Vince** | Front-End Designer | **Vince** → da **Vinci** — the visual/UI hand. |
| **Una** | Principal Product Designer / UX Gatekeeper | **U** → **U**I/**U**X — the design-quality gate. |
| **Wynn** | Gamification Expert | **Wynn** → **win** — engagement, winning loops. |
| **Hawke** | Functional / Visual QA | **Hawk**-eyed for UI problems. |
| **Nix** | Automation / Performance / Security QA | **Nix**es the flaky test. |
| **Jinx** | Adversarial Visual QA (counter-key) | A second pair of eyes to **jinx** a premature sign-off — the visual counter-key. *(Retrofitted — lesson 1.)* |
| **Anders** | Android Engineer | **And**ers → **And**roid. |
| **Ivo** | iOS Engineer | **I** → **i**OS. |

## Floor-only names

These teammates carry **no canonical mnemonic** — they clear the floor (short, distinctive,
easy) and that is enough (lesson 3). Present them with no hook unless the CEO adopts one.

| Name | Role |
|------|------|
| **Milo** | Backend Engineer |
| **Piers** | Infrastructure / DevOps Engineer |
| **Tess** | Full-Stack Engineer |
| **Ash** | Frontend Engineer |
| **Pax** | Mobile / Native QA |

## Suggested retrofits — NOT CEO intent

If a hook is ever *wanted* for a floor-only name (or for Dean/Gus, whose primary origins are
not mnemonic), these are **suggestions labelled as such** — never present them as the CEO's
original intent. Adopt only if they genuinely help:

- **Dean** — the academic dean who hires the faculty. *(Labelled retrofit suggestion.)*
- **Gus** — short for **Aug**ustus, if a "holds the whole thing together" image helps.
- **Pax** — **pax**, peace: nothing ships until every device in the matrix is at peace.
- **Ash** — what is left once everything unnecessary has burned away: the clean UI layer.
- **Tess** — **tess**ellate: every layer fits the next; the everyday full-stack integrator.
- **Milo** — rhymes with "silo"; the schema / data layer the product stores itself in.
- **Piers** — the **piers** a bridge stands on; the infra the whole stack rests on.

## Composing with the spawn-name format

This convention picks the **role name** (the human first name = the agent's `subagent_type`
slug, e.g. `milo`). It composes cleanly with the enforced spawn-name format
`<role>-<model>-<effort>-<identifier>` (e.g. `milo-sonnet-high-f1`): the convention chooses
the `<role>` token and `guard-worktree-isolation.sh` keeps the `<model>` token and the
`<effort>` word truthful and the whole name unique. The two never conflict — one names the person, the other stamps the instance.

## Adopting this

- **Novel roles the standard roster doesn't cover:** Dean invents a name under the floor
  (short, distinctive, easy, fresh first letter) and adds a mnemonic only if one arrives
  naturally — presenting the hook with the proposal so the CEO can feel the fit.
- **Keeping the standard roster:** for a role that matches one above, reuse the canonical
  name and its mnemonic; they transfer because they bind to the role, not the product.
- **Reinventing wholesale:** any roster is fine as long as every name clears the floor. The
  mnemonic layer is a bonus you may take or leave.
