# `.richos/` — this repository's declarations

Five files here and each one is an **adoption switch**. Its presence is the
whole decision: no flag, no config key, no code change.

| File | What its presence declares |
|---|---|
| `publication-boundary` | This repository gets published. Switches on the two leak guards and the completeness contract. |
| `publication-completeness` | The reviewed exemptions the completeness contract honors. Carries no adoption switch of its own. |
| `row-currency` | Landings here are checked against a working record, which lives in another repository. |
| `vendored-material` | Where every piece of other people's work in this tree came from. Switches on `guard-vendoring-commits.sh`, which refuses a commit adding unrecorded material under a redistributable path and tells `dialect-guard.sh` which bytes are not ours to edit. |
| `worktree-setup` | What every worktree of this repository needs and none of them should build for itself. `richos/engine/mega-lander/create-teammate-worktree.sh` runs it inside each new workspace, bounded and never fatally. |

Every one of them is a commented file. Open it: the argument for the mechanism
is written at the top, above the settings.

## Why they are in a directory

They used to sit at the repository root, one entry each. This repository is
read by strangers on a page where the root listing is the first thing rendered,
and three declaration dotfiles were three of eleven rows. They are one row now.

## The names did not change

The declaration is still called `.publication-boundary` — that is the string
every guard names in its refusals, the string `richos/engine/README.md` teaches an
adopter and the string the completeness check derives its own subject list
from. Only the directory moved and the leading dot came off on the way in,
because a hidden file inside a hidden directory is one nobody browsing here
would ever see.

## The root form still works

`richos/engine/scripts/lib/declaration-path.sh` is the one place that resolves a
declaration and it looks here first and at the repository root second. Any
repository that adopted this engine while these files lived at the root keeps
working, untouched — `femcboost` carries a root `.row-currency` right now.

Two copies of one declaration is **BROKEN**, never a choice between them:
choosing one quietly is how the wrong one stays live.

## What else may live here

**This directory is shared.** It is RichOS's directory in a repository, not the
declaration resolver's — the Executive Continuity System keeps its entity
manifest here, as an `entity.json`, in the repositories that have one and
other components may use it too.

The one thing that is refused is a **declaration this engine does not read from
here** — `.ceo-todos`, say. Moving one in would switch its contract off in
silence and a stood-down guard is indistinguishable from a clean one, so the
next guard invocation refuses by name instead. Nothing else in this directory
is any of the resolver's business.

A declaration adopting this directory moves its stem from `DECL_FOREIGN_STEMS`
to `DECL_ADOPTED_STEMS` in `richos/engine/scripts/lib/declaration-path.sh`, in the same
edit that makes it call `decl_find`. Neither list can silently fall behind:
`richos/engine/scripts/publication-completeness.sh` derives every declaration this
engine ships out of shipped source and fails if one appears in neither.
