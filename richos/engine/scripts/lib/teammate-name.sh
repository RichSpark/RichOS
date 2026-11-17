#!/usr/bin/env bash
#
# scripts/lib/teammate-name.sh — THE TEAMMATE NAME SHAPE, ASKED IN ONE PLACE,
#                                ANSWERED ONCE.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# Two independent readers each carried their OWN idea of "a well-formed
# <role>-<model>-<identifier> name":
#
#   guard-worktree-isolation.sh's NAME_SHAPE_RE — no length bound, any case,
#     any number of extra dash-joined segments.
#   mega-lander/create-teammate-worktree.sh's NAME_RE — lower-case only, role
#     2-16 chars, identifier 1-12 chars, EXACTLY three parts.
#
# `spawn.sh` exists to evaluate every refusal BEFORE anything is created (see
# scripts/lib/spawn.py's own header), and it does that by running EVERY
# PreToolUse[Agent] guard registered on either surface against the payload —
# and the engine's own shipped hooks.json (scripts/lib/spawn.py:
# settings_sources) is UNCONDITIONALLY one of those surfaces, so
# guard-worktree-isolation.sh always runs there, always before anything is
# created. The guard's looser rule passed `zach-sonnet-multirepodemo1` (a
# 14-character identifier), so spawn.sh's own guard table said "passed" and
# the refusal surfaced one round trip later, inside the create step, exactly
# the round trip spawn.sh was built to remove (found 2026-09-17, packaging the
# multi-repository fix landed beside it).
#
# So there is now exactly one shape rule, here, and it is the STRICTER of the
# two — the creator's, with the effort word added to it on 2026-11-17 (see
# "THE FOURTH PART" below). Both `guard-worktree-isolation.sh` and
# `mega-lander/create-teammate-worktree.sh` source this file directly and call
# `teammate_name_check`.
#
# scripts/lib/spawn.py NEEDS NO CODE OF ITS OWN FOR THIS RULE, and does not
# call this file: it already runs guard-worktree-isolation.sh, from the
# engine's own hooks.json, as the FIRST step of its own guard-evaluation phase
# — before repository resolution even finishes — so the guard's clause 2c
# (the caller below) already refuses the bad name there, in spawn.sh's own
# guard table, in this file's exact wording, before anything is registered.
# Verified 2026-09-17: `spawn.sh zach-sonnet-multirepodemo1 --repo ... --dry-run`
# prints "refused by 1 of 1 guard(s) - NOTHING WAS CREATED" naming this
# message, with zero changes to spawn.py. A second, redundant check inside
# spawn.py would be the exact defect this file exists to end — two readers
# answering one question — so it does not have one; scripts/spawn.test.sh
# carries the case that proves this end-to-end.
#
# ALLOWED_MODELS RESOLUTION IS DELIBERATELY NOT UNIFIED HERE. The two readers
# resolve it from different roots on purpose: the guard reads the GOVERNED
# REPOSITORY's own orchestration.config (a repository this engine governs may
# declare its own model roster), the creator always reads the ENGINE's own
# orchestration.config. Collapsing that into one resolution would be a real
# behavior change to one of the two, not a name-rule fix — a separate,
# unrelated cleanup. This file only holds the SHAPE the name must take once
# ALLOWED_MODELS is known; each caller keeps deciding what ALLOWED_MODELS is
# for itself.

# ===========================================================================
# THE FOURTH PART: THE EFFORT LEVEL (CEO, 2026-11-17)
# ===========================================================================
# A name is <role>-<model>-<effort>-<identifier>, the effort as the plain word:
# zach-fable-medium-effortfield1, echo2-sonnet-high-video4. The model part stays
# where it was, so every reader of it keeps working. The effort word is checked
# the way the model is: the guard refuses a name whose word is not the level the
# agent is started at, so a level is always named.
#
# The longest name this allows is 16+1+6+1+6+1+12 = 43 characters (the longest
# alias and level are six letters each); Claude Code allows 64.
#
# NAMES RECORDED BEFORE THIS (finished agents, the ledger of used names, review
# records) HAVE THREE PARTS AND STAY READABLE. This file is the rule for a NEW
# name only, asked by the guard and the creator. What reads names that already
# exist takes both shapes: scripts/lib/teammate-identity.py, the claim guards
# (guard-unresolved-claims.py, guard-agent-state-claims.py), the ass-kicker and
# the app's display name (ui/timeline.js, richos-core timeline.rs). The registry,
# the ledger of used names, the stop script and the review watcher compare names
# whole and never take one apart.

# The levels Claude Code accepts for an agent's effort.
TEAMMATE_EFFORT_LEVELS="low medium high xhigh max"

# teammate_name_re <allowed-models> — prints the anchored ERE a NEW name must
# match: lower-case only, role 2-16 chars, model the second of EXACTLY four
# dash-joined parts (one of <allowed-models>), effort the third (one of
# TEAMMATE_EFFORT_LEVELS), identifier 1-12 chars.
teammate_name_re() {
  local allowed="$1"
  printf '^[a-z][a-z0-9]{1,15}-(%s)-(%s)-[a-z0-9]{1,12}$' \
    "$(printf '%s' "$allowed" | tr ' ' '|')" "$(printf '%s' "$TEAMMATE_EFFORT_LEVELS" | tr ' ' '|')"
}

# teammate_name_effort <name> — prints the effort word of a four-part name, and
# nothing for any other name (a three-part recorded name carries none).
teammate_name_effort() {
  local name="$1" rest third level
  case "$name" in *-*-*-*-*) return 0 ;; *-*-*-*) ;; *) return 0 ;; esac
  rest="${name#*-}"; rest="${rest#*-}"; third="${rest%%-*}"
  for level in $TEAMMATE_EFFORT_LEVELS; do
    if [ "$third" = "$level" ]; then printf '%s' "$third"; return 0; fi
  done
  return 0
}

# teammate_name_check <name> <allowed-models> — exit 0 and nothing printed if
# NAME satisfies the shared rule for a new name; exit 1 and the refusal message
# on stdout otherwise. Every reader that refuses on this rule refuses in these
# words, so a name refused by one reader reads identically when another reader
# refuses it too.
teammate_name_check() {
  local name="$1" allowed="$2"
  if printf '%s' "$name" | grep -qE "$(teammate_name_re "$allowed")"; then
    return 0
  fi
  printf "'%s' is not a teammate name of the form <role>-<model>-<effort>-<identifier> (model one of: %s; effort one of: %s)." \
    "$name" "$allowed" "$TEAMMATE_EFFORT_LEVELS"
  return 1
}

# teammate_name_engine_allowed_models <engine-root> — the CREATOR's own
# ALLOWED_MODELS resolution (default, then <engine-root>/orchestration.config
# override), extracted out of create-teammate-worktree.sh unchanged so that
# resolution has one source too.
teammate_name_engine_allowed_models() {
  local engine_root="$1" out="fable opus sonnet haiku"
  if [ -f "$engine_root/orchestration.config" ]; then
    local am
    am="$(sed -n 's/^ALLOWED_MODELS="\(.*\)"$/\1/p' "$engine_root/orchestration.config" | head -1)"
    [ -n "$am" ] && out="$am"
  fi
  printf '%s' "$out"
}
