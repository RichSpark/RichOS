#!/usr/bin/env bash
#
# install.sh — establish the SINGLE canonical hook-registration source and
#               refresh the hook content-hash manifests.
#
# CANONICAL SOURCE — CORRECTED 2026-09-15, and the correction is the reason
# this script now generates rather than merely validates.
#
# This header used to say hooks are registered in exactly ONE file,
# `.claude/settings.local.json`. That was true of the SEATED engine and it
# stopped being the whole truth on 2026-08-28, when the engine became loadable
# BY REFERENCE. There are two surfaces, for the two installation modes
# contract-integrity-probe.sh names:
#
#   hooks/hooks.json             the plugin table. This is what the host loads
#                                and what actually fires in every working
#                                session. THE SOURCE OF TRUTH.
#   .claude/settings.local.json  the seated surface: the same table expressed
#                                with `$CLAUDE_PROJECT_DIR`, which the probe's
#                                Layers A-Q audit when the subject repository
#                                IS the engine. DERIVED from the first, here.
#
# Both are committed and force-added (see .gitignore). The `.sha256` manifest
# sidecars stay gitignored and are regenerated here, so a fresh clone still
# runs install.sh once to mint what the probe's Layer B/C/K hashing needs.
#
# The doctrine line that mattered survives unchanged: THERE IS ONE PLACE A
# HUMAN WRITES A REGISTRATION, and it is hooks/hooks.json.
#
# THE DOUBLE-FIRE BUG THIS SCRIPT NOW PREVENTS: the PRE-fix install.sh wrote a
# SECOND settings file, `.claude/settings.json` (gitignored), holding a
# $CLAUDE_PROJECT_DIR-resolved COPY of the very same hook stanzas. Claude Code
# reads BOTH files and MERGES their hook arrays additively — so every hook fired
# TWICE per matching tool event (two byte-identical resume-acks.log lines; the
# sibling append-loggers doubled identically). Since $CLAUDE_PROJECT_DIR
# expansion makes the resolved copy redundant, this script no longer generates
# hook stanzas into settings.json at all.
#
# WHAT THIS SCRIPT DOES NOW:
#   1. VALIDATE the canonical source carries the two critical non-hook config
#      keys (env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS == "1",
#      worktree.baseRef == "head") — refuse loudly if either is missing, never
#      proceed on a broken source (the "ghost team" incident; probe Layers I/J
#      re-check this independently).
#   1b. GENERATE the seated surface's `hooks` key from hooks/hooks.json, so the
#      two surfaces cannot disagree and no hook author hand-edits 500 lines of
#      JSON again. Idempotent; refuses rather than guesses on a command shape
#      it does not know.
#   2. MIGRATE / DE-DUPLICATE: if a stale `.claude/settings.json` exists, strip
#      its `hooks` key. If the hook-stripped remainder is a pure duplicate of
#      `.claude/settings.local.json` (minus hooks) — the state the old install.sh
#      produced — REMOVE settings.json entirely. If it carries genuine machine-
#      specific non-hook keys, keep the hook-stripped remainder so no operator
#      config is lost. Either way, settings.json ends up with NO hook stanzas.
#   3. REFRESH MANIFESTS: regenerate the `<hook>.sh.sha256` content sidecars
#      from the current on-disk hooks (the probe's Layer B/C/K/P/Q compare live
#      hooks against these).
#
#   4. MINT THE ENGINE POINTER (~/.claude/richos-engine), unless this checkout
#      is a LINKED GIT WORKTREE, or is EPHEMERAL (a temp directory or a
#      scratchpad) while the target is the operator's real config dir — see the
#      pointer section at the bottom for why that one step, and only that step,
#      is withheld in each case.
#
# Idempotent. Re-running converges: an old duplicated settings.json is removed
# on the first run and stays absent on every subsequent run. The operator
# re-runs this at land time as the migration path. Run via:
#
#   scripts/hooks/install.sh [--force-engine-pointer]
#
#   --force-engine-pointer   mint the engine pointer even from a linked git
#                            worktree, or from an ephemeral checkout onto the
#                            operator's real config dir. Deliberate, auditable,
#                            and almost never what you want; the pointer section
#                            explains both.
#
# Exit codes:
#   0  migration (or no-op) succeeded
#   1  unexpected error
#   2  canonical `.claude/settings.local.json` missing/unreadable or missing a
#      critical config key, or an unrecognized command-line argument

set -eo pipefail

# The $CLAUDE_PROJECT_DIR substitution below shells out to python3's json
# module (no `|| true` guard — under `set -e` a missing interpreter already
# aborts the script with a raw "command not found"). Check explicitly up
# front so the failure is a clear, actionable message instead of a bare
# shell error.
command -v python3 >/dev/null 2>&1 || { echo "ERROR: install.sh: python3 is required (JSON generation + sha256 fallback) — refusing" >&2; exit 2; }

# --- Arguments -----------------------------------------------------------
# This script took none until the engine-pointer footgun below needed a
# deliberate, auditable opt-in. Unknown arguments are REFUSED rather than
# ignored: a typo'd --force-engine-pointer that silently did nothing would
# hand the operator a green run and none of the effect they asked for.
FORCE_ENGINE_POINTER=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --force-engine-pointer)
            FORCE_ENGINE_POINTER=1 ;;
        *)
            echo "ERROR: install.sh: unrecognized argument '$1'. Usage: install.sh [--force-engine-pointer]" >&2
            exit 2 ;;
    esac
    shift
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SOURCE="$REPO_ROOT/.claude/settings.local.json"
TARGET="$REPO_ROOT/.claude/settings.json"

if [ ! -r "$SOURCE" ]; then
    echo "ERROR: source settings file missing: $SOURCE" >&2
    echo "       (this script requires .claude/settings.local.json — the committed source-of-truth)" >&2
    exit 2
fi

# --- Validate the canonical source's critical non-hook config ------------
# Two keys in settings.local.json are as load-bearing as any hook —
# env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS ("1", without it the orchestrator
# sees/spawns ZERO teammates at the next session start with no error shown — a
# real recorded incident) and worktree.baseRef ("head", which the worktree-
# isolation doctrine assumes). Refuse to proceed if the source is missing
# either — never migrate/refresh on a broken source. (contract-integrity-
# probe.sh Layers I/J re-check this on every probe run, independent of
# install.sh, so drift introduced by hand-editing AFTER an install is caught.)
python3 - "$SOURCE" <<'PY'
import json, sys
src_path = sys.argv[1]
with open(src_path, "r", encoding="utf-8") as f:
    data = json.load(f)

def get_path(d, dotted):
    cur = d
    for k in dotted.split("."):
        if isinstance(cur, dict) and k in cur:
            cur = cur[k]
        else:
            return None
    return cur

problems = []
teams_flag = get_path(data, "env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS")
if teams_flag != "1":
    problems.append("env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS missing or not \"1\" (got: %r)" % (teams_flag,))
base_ref = get_path(data, "worktree.baseRef")
if base_ref != "head":
    problems.append("worktree.baseRef missing or not \"head\" (got: %r)" % (base_ref,))
if problems:
    sys.stderr.write("ERROR: install.sh: refusing to proceed — "
                      ".claude/settings.local.json is missing critical config:\n")
    for p in problems:
        sys.stderr.write("  - %s\n" % p)
    sys.stderr.write("Restore both keys in .claude/settings.local.json (see README.md's "
                      "\"Critical configuration\" section), then re-run install.sh.\n")
    sys.exit(2)
PY

# --- GENERATE the seated surface's hook stanzas from the registration ------
# settings.local.json's `hooks` key is DERIVED, here, from hooks/hooks.json.
# Nobody hand-edits it again.
#
# ===========================================================================
# WHY THIS STEP EXISTS
# ===========================================================================
# The engine has two hook surfaces and they are not alternatives — they serve
# the two installation modes contract-integrity-probe.sh names:
#
#   BY REFERENCE  hooks/hooks.json, read by the host when the plugin loads.
#                 This is what actually fires in every working session on this
#                 machine. Probe layers BR* audit it.
#   SEATED        .claude/settings.local.json, which the probe's Layers A-Q
#                 audit when the subject repository IS the engine.
#
# The second was maintained BY HAND, by every hook author, in a 500-line JSON
# file, and it had to agree with the first entry-for-entry or hook-staleness
# case 11 went red on `main` with "surfaces disagree". It was one of the four
# inventories hook-registration-completeness.sh demands, and the only one whose
# content is mechanically derivable from another file in the same tree.
#
# Measured on 2026-09-15 before this step was written (zach-opus-surface2): of
# 76 entries, 74 were already an exact transform of the plugin table including
# timeouts, and the two that were not were both drift rather than intent —
# session-start-escalations.sh carried a timeout the real table does not, and
# shell-evidence.sh differed only in shell quoting. A surface that is 97% a
# copy and 3% undocumented divergence is not a second opinion; it is a stale
# copy with a maintenance bill.
#
# THE TRANSFORM, stated once so it can be checked:
#
#   bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/x.sh   (plugin: the host expands
#                                                    CLAUDE_PLUGIN_ROOT)
#        ->  $CLAUDE_PROJECT_DIR/scripts/hooks/x.sh (seated: the host expands
#                                                    CLAUDE_PROJECT_DIR to the
#                                                    repo, which IS the engine)
#
# Event, matcher, entry grouping and timeout are carried across untouched.
#
# WHY THIS CANNOT DOUBLE-FIRE ANYTHING. REPO_ROOT is this script's own
# ../.., so the file written is ALWAYS the engine's own settings.local.json
# and never an adopting repository's. That distinction is the whole safety
# argument: measured live on 2026-09-15, the host does NOT dedupe a hook
# registered on two surfaces — the same command seated in both fired twice on
# one tool call — and this file's own header records the settings.json instance
# of that bug. A generator that wrote the engine's guards into an adopter's
# settings would re-create it at a larger scale.
#
# FAIL LOUD, NEVER GUESS: a registered command that does not carry the plugin
# root placeholder has a shape this transform does not know, and writing a
# guessed translation of it would seat a hook that points nowhere. This aborts
# and names the command instead.
GEN_ACTION="$(python3 - "$SOURCE" "$REPO_ROOT/hooks/hooks.json" <<'PY'
import json, os, re, sys, collections

src_path, plugin_path = sys.argv[1:3]

try:
    with open(plugin_path, "r", encoding="utf-8") as f:
        plugin = json.load(f)
except Exception as e:
    sys.stderr.write("ERROR: install.sh: cannot read the registration surface "
                     "%s: %s\n" % (plugin_path, e))
    sys.stderr.write("       The seated surface is derived from it; with no "
                     "source there is nothing honest to write.\n")
    sys.exit(2)

def seat(cmd):
    """The transform. One shape in, one shape out, or a loud refusal (None).

    THE INLINE CASE IS NOT AN EXCEPTION TO THE RULE, IT IS THE RULE APPLIED.
    One registered entry runs no script at all — the knowledge-verification
    notice is a literal `echo '{...}'`. It names no path, so there is nothing
    to re-root, and the correct seated form is the identical string. It is
    recognized by what it lacks (no plugin root, no script path) rather than
    by matching its text, so a second inline hook needs no edit here and a
    script-bearing command can never fall into this branch by accident.
    """
    if cmd.startswith('bash "${CLAUDE_PLUGIN_ROOT}/'):
        return '"$CLAUDE_PROJECT_DIR/' + cmd[len('bash "${CLAUDE_PLUGIN_ROOT}/'):]
    if cmd.startswith("bash ${CLAUDE_PLUGIN_ROOT}/"):
        return "$CLAUDE_PROJECT_DIR/" + cmd[len("bash ${CLAUDE_PLUGIN_ROOT}/"):]
    if cmd.startswith("bash $CLAUDE_PLUGIN_ROOT/"):
        return "$CLAUDE_PROJECT_DIR/" + cmd[len("bash $CLAUDE_PLUGIN_ROOT/"):]
    if "CLAUDE_PLUGIN_ROOT" in cmd:
        return None          # references the plugin root in a shape not known
    if re.search(r"[A-Za-z0-9._-]+\.(?:sh|py)\b", cmd):
        return None          # names a script by some other path — never guess
    return cmd               # inline: no path, so no re-rooting to do


generated = collections.OrderedDict()
unknown = []
for event, entries in (plugin.get("hooks") or {}).items():
    out_entries = []
    for entry in entries:
        out_hooks = []
        for h in (entry.get("hooks") or []):
            cmd = h.get("command", "")
            seated = seat(cmd)
            if seated is None:
                unknown.append("%s: %s" % (event, cmd))
                continue
            new = collections.OrderedDict()
            new["type"] = h.get("type", "command")
            new["command"] = seated
            if "timeout" in h:
                new["timeout"] = h["timeout"]
            out_hooks.append(new)
        if not out_hooks:
            continue
        out_entry = collections.OrderedDict()
        if entry.get("matcher"):
            out_entry["matcher"] = entry["matcher"]
        out_entry["hooks"] = out_hooks
        out_entries.append(out_entry)
    if out_entries:
        generated[event] = out_entries

if unknown:
    sys.stderr.write("ERROR: install.sh: %d registered command(s) do not carry "
                     "the plugin-root placeholder this transform knows:\n" % len(unknown))
    for u in unknown:
        sys.stderr.write("  - %s\n" % u)
    sys.stderr.write("       Refusing to guess a seated translation. Either "
                     "register it as `bash ${CLAUDE_PLUGIN_ROOT}/<path>` or "
                     "teach seat() the new shape, deliberately.\n")
    sys.exit(2)

with open(src_path, "r", encoding="utf-8") as f:
    settings = json.load(f, object_pairs_hook=collections.OrderedDict)

if settings.get("hooks") == generated:
    print("current")
    sys.exit(0)

settings["hooks"] = generated
tmp = src_path + ".tmp"
with open(tmp, "w", encoding="utf-8") as f:
    json.dump(settings, f, indent=2)
    f.write("\n")
os.replace(tmp, src_path)
print("regenerated")
PY
)"

case "$GEN_ACTION" in
    current)
        echo "✓ seated hook surface already matches hooks/hooks.json" ;;
    regenerated)
        echo "✓ regenerated .claude/settings.local.json hook stanzas from hooks/hooks.json — commit the change" ;;
    *)
        echo "ERROR: seated-surface generation returned unexpected action: '$GEN_ACTION'" >&2
        exit 1 ;;
esac

# --- Migrate / de-duplicate settings.json --------------------------------
# settings.json is gitignored and, post-fix, must NEVER carry hook stanzas.
# The Python below decides the outcome and prints a one-word action so the
# shell can log it:
#   removed  — file existed, hook-stripped remainder was a pure duplicate of
#              settings.local.json (minus hooks); file deleted.
#   stripped — file existed with machine-specific non-hook keys; hooks removed,
#              remainder written back.
#   quarantined:<path> — file existed but was not valid JSON; moved aside to
#              <path> (never deleted: it may hold the user's own settings).
#   absent   — no settings.json to migrate (already single-source).
ACTION="$(python3 - "$SOURCE" "$TARGET" "$REPO_ROOT" <<'PY'
import json, os, sys

src_path, tgt_path, repo_root = sys.argv[1:4]

def resolve(obj):
    """Resolve $CLAUDE_PROJECT_DIR placeholders so a resolved settings.json
    remainder compares equal to the placeholder-carrying source."""
    if isinstance(obj, dict):
        return {k: resolve(v) for k, v in obj.items()}
    if isinstance(obj, list):
        return [resolve(v) for v in obj]
    if isinstance(obj, str):
        return obj.replace("$CLAUDE_PROJECT_DIR", repo_root) \
                  .replace("${CLAUDE_PROJECT_DIR}", repo_root)
    return obj

if not os.path.exists(tgt_path):
    print("absent")
    sys.exit(0)

try:
    with open(tgt_path, "r", encoding="utf-8") as f:
        target = json.load(f)
except Exception:
    # Unreadable/garbage settings.json: it must stop being loaded, so the
    # canonical source is unambiguously the only settings file. But a parse
    # error (a truncated write, a stray comma) does not mean the file holds
    # nothing that belongs to the user, so it is MOVED ASIDE, never deleted. Claude Code
    # only reads the exact name settings.json, and the moved copy keeps every
    # byte for the user to repair. (No apostrophes in this block: bash 3.2
    # mis-parses them inside a dollar-paren heredoc.)
    import time
    backup = "%s.unparseable-%s.bak" % (tgt_path, time.strftime("%Y%m%dT%H%M%S"))
    n = 0
    while os.path.exists(backup):
        n += 1
        backup = "%s.unparseable-%s-%d.bak" % (tgt_path, time.strftime("%Y%m%dT%H%M%S"), n)
    os.replace(tgt_path, backup)
    print("quarantined:" + backup)
    sys.exit(0)

with open(src_path, "r", encoding="utf-8") as f:
    source = json.load(f)

# Hook-stripped views of both files, placeholder-resolved.
target_nohooks = resolve({k: v for k, v in target.items() if k != "hooks"})
source_nohooks = resolve({k: v for k, v in source.items() if k != "hooks"})

if target_nohooks == source_nohooks:
    # Pure duplicate of the canonical source — nothing unique here. Remove it
    # so settings.local.json is the sole settings file (clean single source).
    os.remove(tgt_path)
    print("removed")
else:
    # Genuine machine-specific non-hook config present — preserve it, but
    # drop every hook stanza so nothing double-fires.
    tmp = tgt_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(target_nohooks, f, indent=2)
        f.write("\n")
    os.replace(tmp, tgt_path)
    print("stripped")
PY
)"

case "$ACTION" in
    removed)
        echo "✓ migrated: removed stale hook-duplicating .claude/settings.json (hooks are canonical in settings.local.json)" ;;
    stripped)
        echo "✓ migrated: stripped hook stanzas from .claude/settings.json, kept machine-specific non-hook config" ;;
    quarantined:*)
        echo "✓ migrated: .claude/settings.json could not be parsed, so it was moved aside (not deleted) to ${ACTION#quarantined:} — repair or discard it; settings.local.json is the sole settings file now" ;;
    absent)
        echo "✓ no settings.json to migrate — settings.local.json is the sole settings file" ;;
    *)
        echo "ERROR: settings.json migration returned unexpected action: '$ACTION'" >&2
        exit 1 ;;
esac

# Refresh the content-hash manifests for the canonical hook scripts. The
# integrity probe's Layer B/C/K compares the live hook file's sha256 against
# the value stored in its .sha256 sidecar. Regenerating here (from the current
# on-disk source) keeps the sidecars in sync whenever the operator runs
# install.sh. Scope is EVERY hook the engine registers — enumerated below from
# hooks/hooks.json rather than described here, because a prose list of "which
# hooks" is a stale inventory with extra steps. (This paragraph used to carry
# such a list, and it had already drifted: it said "two entries are not under
# scripts/hooks/" while three were.)
#
# Three managed files are NOT hooks and appear in no hook table, so they are
# named explicitly further down: the worktree reaper, its sanctioned removal
# helper, and the root-resolution contract. The SessionStart reaper wrapper is
# a thin shim, but the reaper it invokes with --execute is the only
# hook-reachable code in the engine that DELETES things (worktrees and
# branches). Hashing only the wrapper would be integrity theatre, so the reaper
# gets a sidecar too and probe Layer Q verifies BOTH.
# THE hooks/ HALF OF THIS LIST IS DERIVED, NEVER TYPED.
#
# It used to be sixteen hand-written paths, and it was the THIRD copy of "the
# set of guards" in this engine. The first two both went stale within two days
# of each other (13/13 and 14/14 in the session banner, over a guard that was
# wired, present, executable and firing), which is all the evidence needed that
# a fourth reader maintaining its own copy would eventually skip a guard's
# sidecar and leave its tamper check silently not running.
#
# So it comes from hooks/hooks.json — the same registration surface the session
# banner and the probe's BR2 derive from — via the one shared parser.
# Consequence: wiring a seventeenth guard mints its sidecar with no edit here.
_RH_LIB="$REPO_ROOT/scripts/lib/registered-hooks.sh"
[ -f "$_RH_LIB" ] || { echo "ERROR: install.sh: $_RH_LIB is missing — cannot determine which hooks to hash, and hashing a guessed subset would leave real guards with no tamper check. Refusing." >&2; exit 2; }
# shellcheck source=../lib/registered-hooks.sh
. "$_RH_LIB"
# Fail loud, never skip: an unreadable hook table here means an unknown guard
# set, and minting "the sidecars we could work out" is precisely the silent
# partial success this engine refuses everywhere else.
if ! _REGISTERED_HOOKS="$(registered_hook_scripts "$REPO_ROOT/hooks/hooks.json")"; then
    echo "ERROR: install.sh: could not derive the managed hook set from $REPO_ROOT/hooks/hooks.json (missing, unreadable, or registering nothing). Refusing rather than minting an incomplete set of sidecars." >&2
    exit 2
fi

HOOK_FILES=()
while IFS= read -r _h; do
    [ -n "$_h" ] || continue
    HOOK_FILES+=("$REPO_ROOT/scripts/hooks/$_h")
done <<REGISTERED_EOF
$_REGISTERED_HOOKS
REGISTERED_EOF

# The managed files that are NOT hooks, and therefore appear in no hook table.
# These stay explicit because there is no registration surface to derive them
# from — each is here for a reason stated at its own line.
HOOK_FILES+=(
    # THE WORKSPACE REGISTRY and the one command Rich runs. Not hooks, and
    # hashed because they are the ONLY code in the engine that deletes a
    # workspace or an agent's branch (docs/plans/worktree-spec-2026-09-11.md:
    # land and discard, and their automatic retry), and because the spawn
    # guard, the lock-out and the Stop gate take their whole answer from the
    # registry. A tampered copy would leave every one of them wired, hashed,
    # executable and deciding from something nobody verifies.
    "$REPO_ROOT/mega-lander/workspaces.py"
    "$REPO_ROOT/mega-lander/workspaces.sh"
    # The model-tier parser. guard-worktree-isolation.sh sources it to decide
    # clause 6 (is this explicit model: override a move DOWN the declared
    # capability order?), and a trimmed or inverted copy would leave the guard
    # wired, hashed, executable and deciding the wrong direction — which is the
    # exact defect the clause exists to end. Layer MT compares this sidecar.
    "$REPO_ROOT/scripts/lib/model-tiers.sh"
    # The model resolver: which model does THIS spawn actually boot on? Not a
    # hook, and hashed for the reason every entry on this list is hashed — TWO
    # registered guards take their entire answer from it. guard-worktree-
    # isolation.sh decides the truthful-name and capability-tier clauses with
    # it, and guard-model-ceiling.sh decides the cost ceiling with it. A
    # tampered copy would leave both guards wired, hashed, executable and
    # judging the wrong model, and they would agree with each other while doing
    # it. Layer MC compares this sidecar.
    "$REPO_ROOT/scripts/lib/resolve-model.sh"
    # Not a hook, and hashed anyway. Every guard's bootstrap refuses to start
    # without scripts/lib/resolve-roots.sh, and every guard's answer to "which
    # repository am I protecting?" comes out of it. An unhashed resolver would
    # make the single most consequential file in the mechanical layer the only
    # one nobody verifies — the same argument that puts the reaper on this list.
    "$REPO_ROOT/scripts/lib/resolve-roots.sh"
    # The jurisdiction predicate, same argument one step further on. SEVEN
    # registered guards now route "does this artifact belong to the repository
    # I govern?" through scripts/lib/seat-jurisdiction.sh, and five of them
    # additionally take their governing root from it. Leaving it unhashed would
    # mean the file that decides whether a guard enforces at all is the one file
    # nobody verifies — check the lock, ignore the key.
    "$REPO_ROOT/scripts/lib/seat-jurisdiction.sh"
    # The git-jurisdiction resolver, same argument one step further down. FIVE
    # registered guards refuse to start without scripts/lib/git-jurisdiction.sh
    # and take the repository they are about to judge from it, so an unhashed
    # copy would be the file that decides WHICH TREE a guard inspects and the
    # one nobody verifies. It exists because that resolution used to live in
    # five hand-copied blocks and four of them resolved the session's
    # repository rather than the one being committed to.
    "$REPO_ROOT/scripts/lib/git-jurisdiction.sh"
    # The publication-boundary predicate, in both its halves. Not hooks, and
    # hashed for the reaper's reason: TWO registered guards
    # (guard-publication-writes.sh, guard-publication-commits.sh) delegate
    # their entire decision to these two files. Hashing the guards while
    # leaving the thing that actually decides unverified would check the lock
    # and ignore the key — the same sentence that put remove-agent-worktree.sh
    # on this list.
    "$REPO_ROOT/scripts/lib/publication-boundary.sh"
    "$REPO_ROOT/scripts/lib/publication-boundary.py"
    # The named-person predicate, in both its halves. Same argument once more,
    # and this pair guards a different class of harm: TWO registered guards
    # (guard-named-persons-writes.sh, guard-named-persons-commands.sh) and the
    # release chokepoint delegate their entire decision to these two files, and
    # the thing they refuse is a private individual's NAME reaching a public
    # repository. A tampered copy would leave both guards wired, hashed,
    # executable and matching nothing — and a deny-list that matches nothing is
    # indistinguishable from a clean tree at every place anybody looks. Check
    # the lock, ignore the key.
    "$REPO_ROOT/scripts/lib/named-persons.sh"
    "$REPO_ROOT/scripts/lib/named-persons.py"
    # The device-identifier predicate: guard-publication-commits.sh delegates
    # the refusal of the owner's test-device serials, UDIDs and tailnet names to
    # it. Same argument: hash the lock, hash the key.
    "$REPO_ROOT/scripts/lib/device-identifiers.py"
    # The declaration RESOLVER. Not a hook, and the newest instance of the same
    # sentence: it is the only code that answers "does this repository declare
    # X, and which file is it?", and THREE contracts take that answer whole —
    # the two leak guards through publication-boundary.sh, the completeness
    # check, and the row-currency guard. A tampered copy that returned "absent"
    # would leave every one of them wired, hashed, executable and STOOD DOWN,
    # and a stood-down publication guard is indistinguishable from a clean one
    # at every place an operator looks. Check the lock, ignore the key.
    "$REPO_ROOT/scripts/lib/declaration-path.sh"
    # The CEO-TODOs predicate, in both its halves — same argument again:
    # guard-ceo-todos-commits.sh delegates its entire decision to these two
    # files, so hashing the guard and leaving the thing that decides unverified
    # would check the lock and ignore the key.
    "$REPO_ROOT/scripts/lib/ceo-todos.sh"
    "$REPO_ROOT/scripts/lib/ceo-todos.py"
    # The CEO-ASK predicate, both halves. Fourth time the same argument, and the
    # stakes are the highest of the set: notice-ceo-unasked.sh tells the lead
    # what to put to him on what these two files decide, and notice-ceo-asks.sh
    # writes the only evidence that the CEO was ever asked anything. A tampered
    # or reverted copy would open the gate permanently and look exactly like a
    # session in which he had been asked. Check the lock, ignore the key.
    "$REPO_ROOT/scripts/lib/ceo-asks.sh"
    "$REPO_ROOT/scripts/lib/ceo-asks.py"
    # The CEO-RULED predicate, both halves. FIFTH time the same argument, and
    # this pair carries a wrinkle worth naming: ceo-ruled.py takes its tokenizer
    # from ceo-asks.py directly above, so the two CEO gates cannot drift into
    # disagreeing about what a question SAYS. A tampered copy of either would
    # leave guard-ceo-ruled-ask.sh wired, hashed, executable and refusing
    # nothing — and the thing it stops refusing is a question the CEO has
    # already answered, in his own words, in a file this session wrote.
    "$REPO_ROOT/scripts/lib/ceo-ruled.sh"
    "$REPO_ROOT/scripts/lib/ceo-ruled.py"
    # The PREMISE predicate, both halves, and the argument is the same sentence
    # with one twist: this pair decides nothing, so a tampered copy does not
    # open a gate — it removes the only step between "this is true" and "so he
    # must decide it". guard-ceo-ruled-ask.sh's first check delegates to these
    # two files entirely, and premise-ask.sh also writes the ledger that records
    # whether any question was ever changed by being checked. A reverted copy
    # would leave the gate wired, hashed, executable, examining nothing, and
    # producing a ledger that looks maintained.
    "$REPO_ROOT/scripts/lib/premise-ask.sh"
    "$REPO_ROOT/scripts/lib/premise-ask.py"
    # The publication-COMPLETENESS predicate, in both its halves. Not hooks, and
    # not under scripts/lib/ either — they are also CI's step 7, which is the
    # point: guard-completeness-commits.sh runs THIS script rather than carrying
    # a second copy of the contract, so the hook and CI can never drift into
    # disagreeing about what "complete" means. Same sentence as above: hashing
    # the guard and leaving the thing that decides unverified checks the lock
    # and ignores the key.
    "$REPO_ROOT/scripts/publication-completeness.sh"
    "$REPO_ROOT/scripts/publication-completeness.py"
    # The row-currency predicate, both halves. Third time the same argument:
    # guard-row-currency-commits.sh decides nothing itself, so hashing the
    # guard and not the predicate would check the lock and ignore the key.
    "$REPO_ROOT/scripts/lib/row-currency.sh"
    "$REPO_ROOT/scripts/lib/row-currency.py"
    # The VENDORED-MATERIAL reader. Same argument again, and here it has a
    # second edge: TWO mechanisms take their whole answer from this one file.
    # guard-vendoring-commits.sh asks it whether a vendoring was recorded, and
    # dialect-guard.sh asks it whose bytes it is about to rewrite. A tampered
    # copy that answered "third-party" to everything would leave the dialect
    # guard wired, hashed and silently checking nothing at all.
    "$REPO_ROOT/scripts/lib/vendored-material.sh"
    # The Stop-hook notice channel. Not a hook, and hashed for the reason above
    # turned inside out: the other entries decide whether to REFUSE something,
    # and this one decides whether the operator is TOLD a guard has stopped
    # deciding anything. Every Stop hook delegates its stand-down and cannot-run
    # notices to it, so a tampered or reverted copy puts them all back on a
    # channel nobody can see — and the symptom of that is silence, which is
    # exactly what a healthy engine also looks like. Checking the lock and
    # ignoring the key, one more time.
    "$REPO_ROOT/scripts/lib/stop-hook-notice.sh"
    # Its PreToolUse counterpart, and the argument is the same one turned
    # inside out a second time. Twenty-seven guards delegate to this file the
    # single sentence that distinguishes "I looked at this call and it is fine"
    # from "I never read this call at all". A tampered or reverted copy puts
    # every one of them back to exit 0 in silence — and the symptom of that is
    # a guard that looks exactly like a healthy one, which is the whole reason
    # the 2026-09-05 payload survey had to be run to find it. Checking the lock
    # and ignoring the key, once more.
    "$REPO_ROOT/scripts/lib/unevaluated-notice.sh"
    # The LIVENESS RESOLVER, in both its halves, plus its operator CLI. Not
    # hooks: guard-agent-state-claims.sh (which contradicts the lead's report
    # on the answer) and the CLI an operator runs before saying anything about
    # an agent's state delegate the question to these files. It decides no
    # deletion: no workspace is deleted on a liveness answer any more. On 2026-08-31 the lead told the CEO a
    # live agent had completed because he read a stale roster instead of this
    # answer; a tampered or reverted resolver would make the stale reading the
    # CORRECT one everywhere at once, and it would look like nothing happened.
    # Check the lock, ignore the key — for the third time, and the widest.
    "$REPO_ROOT/scripts/lib/agent-liveness.py"
    "$REPO_ROOT/scripts/lib/agent-liveness.sh"
    "$REPO_ROOT/scripts/agent-liveness.sh"
    # The STOP predicate and the ack recorder that is its only escape hatch.
    # Check the lock, ignore the key — for the fourth time, and this one has
    # the shortest fuse of any of them. guard-stop-live-work.sh decides nothing
    # itself: it asks stop-live-work.py whether a written ack exists for this
    # exact target, and hashing the guard while leaving the answer unverified
    # would be checking the lock and ignoring the key exactly as above. What
    # makes it sharper is the direction a tampered copy fails in.
    # A `return best, ""` bolted onto find_live_ack() — or any edit that hands
    # decide() an ALLOW it did not earn — leaves the guard wired, hashed,
    # green in the probe, announced in the session banner — and waving through
    # every kill it was written to refuse, silently, because an ALLOW says
    # nothing. The symptom of that is indistinguishable from a morning where
    # nobody destroyed anything. The recorder is hashed for the mirror-image
    # reason: it WRITES the ledger the predicate reads, so a copy that stamped
    # every ack `consumed: false` forever, or dropped the floors on
    # --destroying and --why, would turn a decision into a formality without
    # changing one line of the guard.
    "$REPO_ROOT/scripts/lib/stop-live-work.py"
    "$REPO_ROOT/scripts/stop-work-ack.sh"
    # scripts/stop.sh and scripts/lib/stop.py — the CEO's stop as one command
    # (ceo-decisions §67) — are deliberately NOT here, and the reason is this
    # engine's own standard rather than an oversight. Every entry in this list
    # is compared by a NAMED probe layer (R, MT, MC, AL, Q, BR4...); there is no
    # generic sweep over sidecars, so a file hashed here with no layer to read
    # its sidecar gets a hash nobody compares — the integrity theater the
    # paragraphs above refuse by name. The other half of the argument is what
    # the file is: stop.sh destroys nothing itself (no script can call TaskStop)
    # and PRINTS every decision it makes, per name, before Rich makes the calls,
    # so a tampered copy shows up in its own output. A guard's tampering looks
    # like silence; this one's does not. If it is ever to be hashed, it wants a
    # named layer to compare it, which is a round of its own.
    # The ownership ledger (an advisory record several reporters read; it
    # decides nothing about a workspace now), the TaskCompleted delivery check,
    # and the one command that creates and registers a cc/ workspace.
    "$REPO_ROOT/scripts/lib/worktree-ledger.py"
    "$REPO_ROOT/scripts/lib/completion-proof.py"
    "$REPO_ROOT/mega-lander/create-teammate-worktree.sh"
    # The dialect table and the fixer that reads it. Not hooks, and hashed for
    # the cold-open prompt's reason rather than the reaper's: they are not
    # documentation, they ARE the decision. dialect-guard.sh fixes or refuses a
    # write on nothing but what these two files say, so a quietly emptied table
    # or a fixer that stopped refusing would turn the hook into a green no-op
    # that looks exactly like a clean record — the same silent degradation
    # every entry above exists to make impossible. (Until 2026-10-24 the hook
    # read scripts/lib/dialect-en-US.dict, which is now only an input of the
    # table's generator; dialect-table.py --check proves the table current.)
    "$REPO_ROOT/scripts/lib/dialect/dialect-en-US.generated.dict"
    "$REPO_ROOT/scripts/lib/dialect/dialect-fix.py"
    # The PreToolUse rule manifest. The dialect vocabulary's argument, at the
    # widest blast radius in this list: dispatch-pretooluse.sh decides WHICH
    # GUARDS RUN on nothing but what this file says. Seventeen of them, on every
    # shell call and every write. A line quietly deleted leaves the dispatcher
    # wired, hashed, executable and running sixteen rules where it ran
    # seventeen — and the sixteen that do run all pass, so the call looks clean.
    # Hashing the dispatcher and not its manifest would be hashing the lock and
    # ignoring the key, which is the phrase four entries above this one already
    # use about smaller keys than this.
    "$REPO_ROOT/scripts/hooks/dispatch-pretooluse.manifest"
    # The verbatim cold-open prompt. Its sha256 is stamped into every transcript
    # and compared by the guard, so this file is not documentation — it is part
    # of the decision. Edit a question and every transcript on file stops
    # counting, which is correct: nobody has answered the new question.
    "$REPO_ROOT/scripts/lib/cold-open-prompt.md"
    # The interactive-prompt shape table. Same argument as the dialect
    # vocabulary directly above: guard-interactive-prompt.sh refuses a command
    # on nothing but what this file says, so a quietly trimmed copy would leave
    # the guard wired, hashed, executable and blocking nothing — and the thing
    # it stops blocking draws a window on the logged-in user's screen.
    "$REPO_ROOT/scripts/lib/interactive-prompt.py"
    # The idle-land analyzer. Same argument as the dialect vocabulary and the
    # interactive-prompt table above, and it applies harder here because this
    # one is a BLOCKING Stop guard whose wrapper decides literally nothing:
    # guard-idle-land.sh resolves roots, reads config and then hands the whole
    # verdict to guard-idle-land.py. A trimmed or reverted copy would leave the
    # guard wired, hashed, executable and refusing nothing — and the thing it
    # stops refusing is the failure the CEO has reported for months. It has
    # already been quietly wrong once, in a way nobody could see: the shipped
    # version stood itself down on 41% of landing turns.
    "$REPO_ROOT/scripts/hooks/guard-idle-land.py"
    # The stated-actions analyzer. Same argument as guard-idle-land.py directly
    # above, with one more reason: this one IMPORTS guard-idle-land.py and
    # turn-manifest.py for its terms, so a trimmed copy could keep both siblings
    # hashed and intact while deciding nothing itself — the wrapper would start,
    # announce nothing, and exit 0 on every turn that stated an action it never
    # took, which is the failure the CEO named seven times in one day.
    "$REPO_ROOT/ass-kicker/ass-kicker.py"
    # Canonical ASS Kicker predicates consumed by spawn and the brief hooks.
    "$REPO_ROOT/ass-kicker/brief-provenance.py"
    "$REPO_ROOT/ass-kicker/brief-scope.py"
    # The waiver-repetition analyzer. Same argument as guard-idle-land.py
    # directly above, one notch weaker because this wrapper reports rather
    # than blocks: it decides nothing itself, so an unhashed copy is the file
    # that decides whether a repeatedly-waived guard is ever named.
    "$REPO_ROOT/scripts/hooks/notice-waiver-repetition.py"
    # The ingress analyzer, and the argument is the strongest on this list
    # because this one WRITES. commit-ceo-inputs.sh resolves roots, reads
    # config and then hands the entire job to commit-ceo-inputs.py: which paths
    # the CEO's message named, which gates run, and — when they all pass —
    # `git hash-object`, `git commit-tree` and `git update-ref`. A trimmed copy
    # could leave the hook wired, hashed, executable and capturing nothing,
    # rebuilding the exact defect it was written for; a tampered one could
    # commit past the credential and publication gates. Every other entry here
    # protects a decision. This one also protects a write.
    "$REPO_ROOT/scripts/hooks/commit-ceo-inputs.py"
    "$REPO_ROOT/scripts/hooks/shell-evidence.py"
    # The hold behind a pause. Not a hook, and hashed because it SIGNALS: the
    # registry's pause, resume and stop, shell-evidence.py's capture and its own
    # detached watchdog take their whole answer from it about which processes are
    # an agent's to freeze and continue. A tampered copy would freeze or continue
    # processes nobody verified were that agent's.
    "$REPO_ROOT/scripts/lib/agent_hold.py"
    # The CI turn gate's analyzer. Same argument as guard-idle-land.py above,
    # and it lands in the same place: guard-ci-turn-gate.sh resolves roots and
    # then hands the ENTIRE verdict to this file — which repositories this
    # session pushed to, which commit went, what GitHub said about it, the
    # 2-second budget, and whether a `ci-red-ack:` is real. A trimmed copy would
    # leave the guard wired, hashed, executable, announced in the banner and
    # refusing nothing, and the symptom of that is indistinguishable from an
    # evening on which CI happened to be green. It also WRITES: the ack log is
    # the only record that a red workflow was waved past, and a copy that
    # dropped the reason floor would turn a decision into a keystroke.
    "$REPO_ROOT/scripts/hooks/guard-ci-turn-gate.py"
    # Shared operator pause policy used by CI gates and reports.
    "$REPO_ROOT/scripts/lib/ci_pause.py"
)
for f in "${HOOK_FILES[@]}"; do
    [ -f "$f" ] || continue
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$f" | awk '{print $1}' > "$f.sha256"
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$f" | awk '{print $1}' > "$f.sha256"
    else
        python3 -c "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$f" > "$f.sha256"
    fi
done
echo "✓ refreshed hook sha256 manifests"

# --- Mint the entity-facing engine pointer --------------------------------
# A HOOK is told where the engine is ($CLAUDE_PLUGIN_ROOT). An ENTITY's own
# scripts are not, and under a by-reference engine they have no relative path to
# it either — so an install-fresh pipeline or a CI step that must run the
# integrity probe has nothing to call. scripts/locate-engine.sh is the full
# answer; this symlink is the two-line bootstrap an adopter can use before it
# can source anything.
#
# It is a CACHE of the operator registration, never the source of truth: the
# locator consults the registration FIRST and this only as a fallback, and the
# probe's BR6 layer asserts the two agree, so a pointer left behind by a moved
# engine is a probe failure rather than a silent wrong answer.
#
# Best-effort by design: a machine with no writable ~/.claude is not a broken
# install, and this must never be the reason an installer fails.
#
# CLAUDE_CONFIG_DIR is honoured (the host's own override for where ~/.claude
# lives), and it is what lets a test sandbox its HOME properly: without it, a
# suite that runs this installer against a throwaway engine would repoint the
# REAL operator's pointer at a temp directory that is deleted seconds later.
# Observed, in this repo, before the variable was threaded through.
#
# AND IT IS WITHHELD FROM A LINKED GIT WORKTREE. This is the one step in the
# script whose blast radius is the OPERATOR'S WHOLE MACHINE rather than this
# checkout: every entity on the box, in every repository, follows this one
# symlink. A linked worktree is the exact opposite — per-agent, ephemeral,
# removed by the lander minutes later. Aiming a durable machine-wide pointer at
# an ephemeral directory is never what anyone intended, and it is INVISIBLE at
# the moment it happens: the installer prints a cheerful "✓ engine pointer ->"
# either way, and nothing goes wrong until the worktree is reaped and the next
# probe run reports BR6b DANGLING to whoever happens to be sitting there.
#
# MEASURED, on this machine: an engineer ran this installer inside his worktree
# while testing something unrelated and silently repointed the operator's live
# engine at it. He noticed only because he happened to look afterwards. Nobody
# should have to happen to look.
#
# WHY SKIP RATHER THAN REFUSE THE WHOLE RUN. Everything else install.sh does is
# repo-local and entirely correct inside a worktree — validating the config,
# migrating a stale settings.json, and above all minting the .sha256 sidecars
# the integrity probe needs to pass there. That is the REASON to run it in a
# worktree, and it is a routine part of an engineer's loop. Refusing the whole
# script would break a normal path to close an abnormal one, which trades down.
# So the dangerous half is withheld and the useful half runs untouched.
#
# WHY SKIP RATHER THAN WARN. A warning is what this already effectively was:
# the damage was done, printed among a dozen other lines, and caught only by
# somebody looking for it. A warning that fires after the irreversible act is
# an obituary, not a guard.
#
# --force-engine-pointer is the deliberate escape hatch, because there is a
# legitimate case (exercising pointer behavior itself) and because an opt-in
# flag leaves a record in shell history that an env var or a silent default
# does not.
#
# Detection is git's own definition of a linked worktree — a private --git-dir
# that differs from the shared --git-common-dir — not a path heuristic. If git
# is absent, or this is not a checkout at all (a vendored engine copy, a test
# sandbox), the answer is "not a worktree" and behavior is byte-identical to
# before this paragraph existed.
POINTER_IN_WORKTREE=0
POINTER_MAIN_HINT=""
# The FACT is computed whether or not --force-engine-pointer is set; the flag
# is consulted where the POINTER decision is made, below, and nowhere else.
# Until 2026-09-03 this block was skipped under the flag, which left the flag
# at 0 for every later reader — and the launchd step reads it. A forced run
# from a temp checkout therefore scheduled a machine-wide job at a directory
# that was deleted a second later (the retired reconciler schedule, 2026-09-03).
if true; then
    _PTR_GIT_DIR="$(git -C "$REPO_ROOT" rev-parse --path-format=absolute --git-dir 2>/dev/null || true)"
    _PTR_GIT_COMMON="$(git -C "$REPO_ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
    if [ -n "$_PTR_GIT_DIR" ] && [ -n "$_PTR_GIT_COMMON" ] && [ "$_PTR_GIT_DIR" != "$_PTR_GIT_COMMON" ]; then
        POINTER_IN_WORKTREE=1
        # Name the exact path to run instead, rather than "the main checkout":
        # the parent of the shared .git is the main checkout root, and this
        # engine's offset inside the worktree is its offset inside the twin.
        _PTR_MAIN_TOP="$(dirname "$_PTR_GIT_COMMON")"
        _PTR_WT_TOP="$(git -C "$REPO_ROOT" rev-parse --path-format=absolute --show-toplevel 2>/dev/null || true)"
        if [ -n "$_PTR_WT_TOP" ] && [ "${REPO_ROOT#"$_PTR_WT_TOP"}" != "$REPO_ROOT" ]; then
            POINTER_MAIN_HINT="$_PTR_MAIN_TOP${REPO_ROOT#"$_PTR_WT_TOP"}"
        else
            POINTER_MAIN_HINT="$_PTR_MAIN_TOP"
        fi
    fi
fi

ENGINE_CONFIG_DIR="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
ENGINE_POINTER="$ENGINE_CONFIG_DIR/richos-engine"

# --- A TEST FIXTURE MUST NOT BE ABLE TO AIM THE OPERATOR'S POINTER AT ITSELF
#
# On 2026-09-01 ~/.claude/richos-engine — the pointer EVERY session and the
# shipped app resolve the engine through — was found dangling at
# .../scratchpad/g4/red/layerR: a Layer R red-run fixture, deleted after the run
# that made it. The consequence was measured rather than imagined. A
# double-clicked RichOS reported NO COMPUTE LEASE, having attached its lease
# through that same pointer an hour earlier.
#
# THE DEFECT IS NOT THE DANGLING LINK. It is that a test could move a global
# pointer this script owns and leave it moved. The rule that follows is "a red
# run must restore what it borrows, or must never borrow the real one" — and a
# rule enforced by the person running the red run is a rule enforced by
# attention, which is the thing this engine keeps proving it cannot buy.
#
# So it is enforced HERE, at the only place that writes the pointer, and the
# condition is deliberately narrow: the source checkout is EPHEMERAL (a temp
# directory or a scratchpad) AND the target is the OPERATOR'S REAL config
# directory. A suite that sandboxes CLAUDE_CONFIG_DIR — the correct discipline,
# which contract-integrity.test.sh and locate-engine.test.sh already follow — is
# untouched, because it is not borrowing the real one. Only the run that forgot
# is stopped, and it is stopped BEFORE the irreversible act rather than warned
# about after it.
#
# --force-engine-pointer remains the deliberate way through, for the same reason
# it exists for the worktree case: there is a legitimate need (exercising
# pointer behavior itself) and an opt-in flag leaves a record in shell history.
POINTER_EPHEMERAL=0
# Computed unconditionally, for the reason given at POINTER_IN_WORKTREE: the
# force flag decides the pointer step only, and the launchd step reads this.
if true; then
    _PTR_PHYS="$( (cd "$REPO_ROOT" 2>/dev/null && pwd -P) || printf '%s' "$REPO_ROOT" )"
    _PTR_CFG="$( (cd "$ENGINE_CONFIG_DIR" 2>/dev/null && pwd -P) || printf '%s' "$ENGINE_CONFIG_DIR" )"
    _PTR_HOME_CFG="$( (cd "$HOME/.claude" 2>/dev/null && pwd -P) || printf '%s' "$HOME/.claude" )"
    if [ "$_PTR_CFG" = "$_PTR_HOME_CFG" ]; then
        case "$_PTR_PHYS" in
            /tmp/*|/private/tmp/*|/var/folders/*|/private/var/folders/*|*/scratchpad/*|*/scratch/*)
                POINTER_EPHEMERAL=1 ;;
        esac
        if [ "$POINTER_EPHEMERAL" -eq 0 ] && [ -n "${TMPDIR:-}" ]; then
            _PTR_TMP="$( (cd "$TMPDIR" 2>/dev/null && pwd -P) || printf '%s' "$TMPDIR" )"
            case "$_PTR_PHYS" in
                "${_PTR_TMP%/}"/*) POINTER_EPHEMERAL=1 ;;
            esac
        fi
    fi
fi

if [ "$POINTER_EPHEMERAL" -eq 1 ] && [ "$FORCE_ENGINE_POINTER" -ne 1 ]; then
    echo "NOTE: engine pointer SKIPPED — this checkout is EPHEMERAL:" >&2
    echo "        $REPO_ROOT" >&2
    echo "      and $ENGINE_POINTER is the OPERATOR'S REAL pointer, which every session and" >&2
    echo "      the shipped app resolve the engine through. Aiming it at a temp or scratch" >&2
    echo "      directory leaves a DANGLING pointer the moment that directory is cleaned up." >&2
    echo "      That is not hypothetical: on 2026-09-01 a red-run fixture did exactly this" >&2
    echo "      and a double-clicked RichOS reported NO COMPUTE LEASE." >&2
    echo "      Everything else in this run completed normally." >&2
    echo "      If you are testing pointer behavior, borrow a scratch one instead — it costs" >&2
    echo "      nothing:  CLAUDE_CONFIG_DIR=\$(mktemp -d) install.sh" >&2
    echo "      To repoint the real one from HERE anyway, deliberately:  install.sh --force-engine-pointer" >&2
elif [ "$POINTER_IN_WORKTREE" -eq 1 ] && [ "$FORCE_ENGINE_POINTER" -ne 1 ]; then
    echo "NOTE: engine pointer SKIPPED — this checkout is a LINKED GIT WORKTREE:" >&2
    echo "        $REPO_ROOT" >&2
    echo "      $ENGINE_POINTER is unchanged, which is what you want: a worktree is removed at" >&2
    echo "      land time, and a pointer left aimed at a removed directory is a dangling symlink" >&2
    echo "      that probe layer BR6b reports as a hard failure to whoever runs it next." >&2
    echo "      Everything else in this run completed normally — config validation, settings.json" >&2
    echo "      migration and the .sha256 sidecars — which is what install.sh is for in a worktree." >&2
    if [ -n "$POINTER_MAIN_HINT" ]; then
        echo "      To repoint the operator's engine for real, run the installer from the main checkout:" >&2
        echo "        $POINTER_MAIN_HINT/scripts/hooks/install.sh" >&2
    fi
    echo "      To repoint from HERE anyway, deliberately:  install.sh --force-engine-pointer" >&2
elif mkdir -p "$ENGINE_CONFIG_DIR" 2>/dev/null; then
    if [ -L "$ENGINE_POINTER" ] || [ ! -e "$ENGINE_POINTER" ]; then
        if ln -sfn "$REPO_ROOT" "$ENGINE_POINTER" 2>/dev/null; then
            echo "✓ engine pointer -> $ENGINE_POINTER -> $REPO_ROOT"
        else
            echo "NOTE: could not write the engine pointer at $ENGINE_POINTER — entity scripts that locate the engine will fall back to the operator registration. (install.sh)" >&2
        fi
    else
        echo "NOTE: $ENGINE_POINTER exists and is NOT a symlink — leaving it alone rather than replacing an operator's file. Entity scripts will resolve the engine from the operator registration instead. (install.sh)" >&2
    fi
else
    echo "NOTE: could not create $ENGINE_CONFIG_DIR — engine pointer not minted. (install.sh)" >&2
fi

# --- The retired nightly reconciler: taken OFF launchd --------------------
#
# Until 2026-09-11 this step scheduled com.richos.worktree-reconciler, a nightly
# launchd job that deleted worktrees. The CEO's workspace spec
# (docs/plans/worktree-spec-2026-09-11.md) has exactly two events — a workspace
# is registered when its agent is spawned, and deleted when its work is landed
# or discarded — and the only deleters are that land and discard and their
# automatic retry (mega-lander/workspaces.sh). The reconciler is not on the page,
# and its code is gone; a job still loaded under launchd would run a file that
# no longer exists, or an old copy that still does.
#
# So the installer now REMOVES it: bootout, delete the plist, verify with
# `launchctl print` that the label is gone. The same walls as before decide
# whether this run may touch the operator's launchd at all (a redirected HOME,
# a linked worktree, an ephemeral checkout, a sandboxed CLAUDE_CONFIG_DIR, a
# non-macOS host), and RICHOS_LAUNCH_AGENTS_DIR redirects the plist directory
# for tests, in which case launchctl is never invoked. A job that is present
# and cannot be removed FAILS the install: a deleter the page does not have is
# not left running quietly.
LAUNCHD_RETIRED_LABEL="com.richos.worktree-reconciler"
LAUNCHD_DIR="${RICHOS_LAUNCH_AGENTS_DIR:-$HOME/Library/LaunchAgents}"
LAUNCHD_RETIRED_PLIST="$LAUNCHD_DIR/$LAUNCHD_RETIRED_LABEL.plist"
PYTHON_BIN="$(command -v python3 || true)"
RETIRE_FAILED=""
# Set to 1 by the ONE branch of the chain below that is allowed to touch the
# operator's launchd. The disk-watchdog step at the end of this file reads it
# rather than re-deriving the five walls, because two copies of that chain would
# drift and the copy that drifted would be the one scheduling jobs on a test
# runner.
LAUNCHD_TOUCHABLE=0
if [ -n "${RICHOS_LAUNCH_AGENTS_DIR:-}" ]; then
    if [ -e "$LAUNCHD_RETIRED_PLIST" ]; then
        rm -f "$LAUNCHD_RETIRED_PLIST" || true
        if [ -e "$LAUNCHD_RETIRED_PLIST" ]; then
            RETIRE_FAILED="could not delete $LAUNCHD_RETIRED_PLIST"
        else
            echo "✓ retired reconciler plist removed (not unloaded: RICHOS_LAUNCH_AGENTS_DIR is set) -> $LAUNCHD_RETIRED_PLIST"
        fi
    fi
elif LAUNCHD_ACCOUNT_HOME="$("$PYTHON_BIN" -c 'import os, pwd; print(pwd.getpwuid(os.getuid()).pw_dir)' 2>/dev/null || true)"; \
     [ -z "$LAUNCHD_ACCOUNT_HOME" ] \
     || [ "$( (cd "$HOME" 2>/dev/null && pwd -P) || printf '%s' "$HOME" )" != "$( (cd "$LAUNCHD_ACCOUNT_HOME" 2>/dev/null && pwd -P) || printf '%s' "$LAUNCHD_ACCOUNT_HOME" )" ]; then
    : # a redirected HOME is a test fixture; gui/<uid> is machine-wide
elif [ "$POINTER_EPHEMERAL" -eq 1 ] || [ "$POINTER_IN_WORKTREE" -eq 1 ]; then
    [ -e "$LAUNCHD_RETIRED_PLIST" ] && echo "NOTE: the retired reconciler ($LAUNCHD_RETIRED_PLIST) is still installed; run install.sh from the main checkout to remove it. (install.sh)" >&2
elif [ "$(uname -s 2>/dev/null)" != "Darwin" ]; then
    : # launchd is macOS only; nothing was ever scheduled here
elif [ "$(cd "$ENGINE_CONFIG_DIR" 2>/dev/null && pwd -P)" != "$(cd "$HOME/.claude" 2>/dev/null && pwd -P)" ]; then
    : # a sandboxed install never touches the operator's launchd
else
    LAUNCHD_TOUCHABLE=1
    _domain="gui/$(id -u)"
    if command -v launchctl >/dev/null 2>&1; then
        launchctl bootout "$_domain/$LAUNCHD_RETIRED_LABEL" >/dev/null 2>&1 || true
        if launchctl print "$_domain/$LAUNCHD_RETIRED_LABEL" >/dev/null 2>&1; then
            RETIRE_FAILED="launchctl still lists $_domain/$LAUNCHD_RETIRED_LABEL after bootout"
        fi
    fi
    if [ -e "$LAUNCHD_RETIRED_PLIST" ]; then
        rm -f "$LAUNCHD_RETIRED_PLIST" || true
        [ -e "$LAUNCHD_RETIRED_PLIST" ] && RETIRE_FAILED="${RETIRE_FAILED:+$RETIRE_FAILED; }could not delete $LAUNCHD_RETIRED_PLIST"
    fi
    [ -z "$RETIRE_FAILED" ] && echo "✓ the retired nightly reconciler is not under launchd ($LAUNCHD_RETIRED_LABEL)"
fi
if [ -n "$RETIRE_FAILED" ]; then
    {
        echo "=== install.sh: FAILED — the retired nightly reconciler is still installed ==="
        echo "  $RETIRE_FAILED"
        echo "  It deletes worktrees, and the workspace spec's only deleters are land and"
        echo "  discard (docs/plans/worktree-spec-2026-09-11.md). Fix the cause above, then"
        echo "  run this script again from the main checkout:"
        echo "    $REPO_ROOT/scripts/hooks/install.sh"
    } >&2
    exit 1
fi

# --- The disk watchdog and the scratch reaper are ARMED here ----------------
#
# CEO, 2026-09-18 (§54): garbage is always cleaned up, "or Rich must get a
# MASSIVE ALERT about it". Both halves of that are scheduled jobs, and A
# SCHEDULED JOB THAT WAS NEVER INSTALLED IS THE ONE FAILURE NEITHER JOB CAN
# REPORT ABOUT ITSELF — it looks exactly like a machine with no garbage and a
# disk that never gets low.
#
# Leaving the arming to a human meant it did not happen: the reaper shipped on
# 2026-09-17 with a `--install` flag, and the 105 GB arrived that night on a
# machine where somebody had remembered to run it. That is the good case. The bad
# case is a fresh clone where nobody does, and nothing anywhere says so.
#
# So install.sh arms them, through each job's own `--install`, which keeps the
# plist-writing in one place per job. LAUNCHD_TOUCHABLE is the same five walls
# the retired-reconciler step above uses: a redirected HOME, a linked worktree,
# an ephemeral checkout, a sandboxed CLAUDE_CONFIG_DIR and a non-macOS host all
# leave the operator's launchd alone.
#
# A FAILURE HERE IS A NOTE AND NOT AN EXIT 1, and that is a deliberate
# difference from the retired reconciler above. That one fails the install
# because a job that DELETES WORKTREES and should not exist is actively
# dangerous. These two are protective: not having them is bad, but refusing to
# install the whole engine because a timer could not be scheduled would leave the
# operator with neither the engine nor the timer.
if [ "$LAUNCHD_TOUCHABLE" -eq 1 ]; then
    for _job in disk-watchdog scratch-reaper; do
        _script="$REPO_ROOT/scripts/$_job.sh"
        [ -x "$_script" ] || continue
        # Each --install leaves an unchanged, loaded job alone (it used to reload
        # it on every install, which killed a cleanup in progress) and refuses
        # to replace a changed job while it is running, exit 3, saying why.
        _rc=0; _out="$("$_script" --install 2>&1)" || _rc=$?
        if [ "$_rc" -eq 0 ]; then
            echo "✓ scheduled com.richos.$_job"
        elif [ "$_rc" -eq 3 ]; then
            echo "NOTE: com.richos.$_job changed but was NOT replaced, because it is running now:" >&2
            printf '%s\n' "$_out" | sed 's/^/  /' >&2
        else
            echo "NOTE: could not schedule com.richos.$_job. Nothing is watching" >&2
            echo "  until it is armed; run it by hand from the main checkout:" >&2
            echo "    $_script --install" >&2
            [ -z "$_out" ] || printf '%s\n' "$_out" | sed 's/^/  /' >&2
        fi
    done
elif [ "$POINTER_EPHEMERAL" -eq 1 ] || [ "$POINTER_IN_WORKTREE" -eq 1 ]; then
    # Said out loud rather than skipped silently, because this is the path an
    # agent's install takes and the operator's machine is left unarmed.
    echo "NOTE: the disk watchdog and scratch reaper were NOT scheduled (this is a" >&2
    echo "  worktree or an ephemeral checkout, and a plist baked with this path would" >&2
    echo "  fire against a directory that stops existing). Arm them from the main" >&2
    echo "  checkout:  scripts/disk-watchdog.sh --install && scripts/scratch-reaper.sh --install" >&2
fi
exit 0
