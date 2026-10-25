#!/usr/bin/env bash
#
# escalate.sh — RAISE AN ESCALATION THAT CANNOT QUIETLY FAIL TO ARRIVE.
#
# One call, from your own worktree, and you are done. The argument for every
# design decision here — why a ledger rather than a file, why it lives outside
# every repository, why `state` is required, why an unacknowledged escalation
# gets louder — is in scripts/lib/escalations.py. Read that if you want the
# reasoning; you do not need it to use this.
#
# ===========================================================================
# WHAT THIS REPLACES, AND WHY
# ===========================================================================
# The old protocol was: write `BLOCKED.md` at the root of your worktree, commit
# it, and send a one-line message. Two teammates did exactly that on 2026-09-02.
# Both were right to. Their escalations were found on 2026-09-04 by a worktree
# cleanup, because somebody was counting directories.
#
# A file on your branch is only ever seen by whoever merges your branch, and you
# cannot see whether your branch was merged. The repository root, separately, is
# now nine entries by permanent CEO ruling — so that write would be refused
# today with nowhere obvious to go.
#
# So this writes the escalation to a LEDGER OUTSIDE EVERY REPOSITORY, which the
# lead's session reads at every session start and at every turn end WITHOUT
# ANYTHING BEING MERGED. It also writes a record file under `docs/verification/`
# — the home the same ruling names — but NOTHING DEPENDS ON THAT FILE. It is
# the record; the ledger row is the delivery.
#
# ===========================================================================
# USAGE — the teammate's half is one file and one command
# ===========================================================================
#   1. Write the fields with your WRITE TOOL (not a shell command) to a file in
#      your scratch directory, e.g. esc.json:
#        {"title": "<one line>",
#         "state": "work-complete|proceeding|stopped",
#         "question": "<the smallest question that would unblock>",
#         "for": "lead|ceo",                          (optional, default lead)
#         "needs": "ceo-hands",                       (optional: the answer needs
#                                                      the CEO AT A DEVICE)
#         "tried": "<what you already tried>",        (optional)
#         "meanwhile": "<what you are proceeding on>"} (optional)
#   2. escalate.sh raise --fields <path to esc.json>
#
#   WHY A FILE: a worktree-isolated agent's Bash call is refused by Claude
#   Code itself, before this script runs, when free text on the command line
#   starts with the word `git` or carries a quote or apostrophe next to it
#   ("git status, git add", "we can't run git"). Text in a file written with the
#   Write tool never crosses that check. Measured 2026-09-28; the reasoning is
#   at FIELD_VARS in scripts/lib/escalations.py.
#
#   The options still work, and are fine for the lead's own session:
#   escalate.sh raise --title "<one line>" \
#                     --state work-complete|proceeding|stopped \
#                     --question "<the smallest question that would unblock>" \
#                     [--for lead|ceo] [--needs ceo-hands] \
#                     [--tried "<what you already tried>"] \
#                     [--meanwhile "<what you are proceeding on>"] \
#                     [--worktree <path>] [--teammate <name>] [--no-record]
#   A field may come from the file or from an option, never both.
#
#       --state IS REQUIRED, and it is the field that keeps this channel alive:
#         work-complete  the work is DONE; this is a record, NOT a stall
#         proceeding     raised, and still working on everything else
#         stopped        the whole task depends on the answer; work has stopped
#       Both of the 2026-09-02 originals were `work-complete`. A mechanism that
#       read every escalation as a failure would teach you not to raise them.
#
#       --for names whose answer it is. `ceo` means only the CEO can decide it;
#       the lead's job is then to route it or decide, but not neither.
#
#       --needs ceo-hands says whose HANDS it takes, which --for cannot: the
#       answer needs the CEO physically at a device (an approval, Touch ID, a
#       passcode, a login, a cable). The stall watcher wakes the lead with it
#       first and again every 10 minutes until it is acknowledged.
#
# ===========================================================================
# USAGE — the lead's half
# ===========================================================================
#   escalate.sh list                 every outstanding escalation, in full
#   escalate.sh show <id>            one, with its acknowledgements
#   escalate.sh ack <id> --disposition "<what you decided or did>"
#                        [--until <ISO date>]
#   escalate.sh ack <id> --fields <file>   (a file holding "disposition", and
#                                           optionally "until")
#
#       `ack` APPENDS; nothing is ever deleted. The disposition must be at
#       least 30 characters, because an ack with no disposition is a dismissal
#       wearing a ledger row.
#
#       `--until` is for A HOLD WHOSE REASON HAS A SHELF LIFE. Without it an
#       ack is permanent, which is right for a decision and wrong for a wait:
#       "held pending a decision that is the CEO's" closes the item forever,
#       INCLUDING on the day after he decides. That is failure type J in the
#       2026-09-10 record — the same shape as the lock reason ending "the agent
#       is live", which outlived its truth and blocked the fix until a person
#       removed it by hand. With `--until 2026-09-17` the acknowledgement
#       expires on that date and the escalation reopens, carrying the
#       disposition that expired so the reader sees why it is back.
#
#       It takes a DATE and refuses anything else. "until the CEO decides" is a
#       condition nothing can evaluate; an expiry that never fires is the very
#       defect the flag exists to end. Put the condition in --disposition and
#       give a date by which somebody looks again.
#
# Exit codes:
#   raise  0 delivered · 2 refused (bad arguments) or NOT DELIVERED (ledger
#          write failed — and it says so in those words)
#   list   0 nothing outstanding · 1 at least one outstanding · 2 unreadable
#   ack    0 recorded · 1 already acknowledged · 2 refused
#   show   0 found · 2 no such escalation

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/escalations.sh
. "$SCRIPT_DIR/lib/escalations.sh"

# The whole leading comment block, minus the shebang — so a header that grows
# does not silently stop being the help text.
usage() { sed -n '3,/^$/p' "$0" | sed 's/^# \{0,1\}//'; }

CMD="${1:-list}"
[ "$#" -gt 0 ] && shift || true
case "$CMD" in
    -h|--help|help) usage; exit 0 ;;
esac

TITLE=""; STATE=""; AUDIENCE="lead"; QUESTION=""; TRIED=""; MEANWHILE=""; NEEDS=""
WT=""; TEAMMATE=""; DISPOSITION=""; TARGET=""; NO_RECORD=0; FORMAT="text"; UNTIL=""
FIELDS=""; GIVEN=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --title)       TITLE="${2:-}"; GIVEN="$GIVEN title"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --state)       STATE="${2:-}"; GIVEN="$GIVEN state"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --for)         AUDIENCE="${2:-}"; GIVEN="$GIVEN for"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --needs)       NEEDS="${2:-}"; GIVEN="$GIVEN needs"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --question)    QUESTION="${2:-}"; GIVEN="$GIVEN question"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --tried)       TRIED="${2:-}"; GIVEN="$GIVEN tried"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --meanwhile)   MEANWHILE="${2:-}"; GIVEN="$GIVEN meanwhile"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --worktree)    WT="${2:-}"; GIVEN="$GIVEN worktree"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --teammate)    TEAMMATE="${2:-}"; GIVEN="$GIVEN teammate"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --disposition) DISPOSITION="${2:-}"; GIVEN="$GIVEN disposition"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --until)       UNTIL="${2:-}"; GIVEN="$GIVEN until"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --fields)      FIELDS="${2:-}"; [ -n "$FIELDS" ] || { echo "escalate.sh: --fields needs a file path" >&2; exit 2; }; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --format)      FORMAT="${2:-}"; shift 2 || { echo "$(basename "$0"): $1 needs a value" >&2; exit 2; } ;;
        --no-record)   NO_RECORD=1; shift ;;
        -h|--help)     usage; exit 0 ;;
        -*)  echo "escalate.sh: unrecognized option '$1'" >&2; exit 2 ;;
        *)   if [ -n "$TARGET" ]; then echo "escalate.sh: unexpected argument '$1'" >&2; exit 2; fi
             TARGET="$1"; shift ;;
    esac
done

escalations_require || {
    echo "escalate.sh: $ESCALATIONS_BROKEN — the escalation channel cannot run here." >&2
    echo "  YOUR ESCALATION HAS NOT BEEN RAISED. Put it verbatim in your final report and in" >&2
    echo "  your commit message, and say that this command could not run." >&2
    exit 2
}

# The fields file (see USAGE): free text read from a file, so none of it has to
# cross the command line. escalations.py validates the file and names the fields
# present; each value is read on its own and assigned through the fixed case
# below, so nothing from the file is ever evaluated. The trailing "." it prints
# keeps a value's own trailing newlines from being stripped by $(...).
if [ -n "$FIELDS" ]; then
    FIELD_FAIL="escalate.sh: nothing was raised or acknowledged; fix the fields file and run it again."
    FIELD_KEYS="$(python3 "$ESCALATIONS_PY" fields --file "$FIELDS" --given "$GIVEN")" || { echo "$FIELD_FAIL" >&2; exit 2; }
    for FIELD_KEY in $FIELD_KEYS; do
        FIELD_VAL="$(python3 "$ESCALATIONS_PY" fields --file "$FIELDS" --get "$FIELD_KEY")" || { echo "$FIELD_FAIL" >&2; exit 2; }
        FIELD_VAL="${FIELD_VAL%.}"
        case "$FIELD_KEY" in
            title)       TITLE="$FIELD_VAL" ;;
            state)       STATE="$FIELD_VAL" ;;
            for)         AUDIENCE="$FIELD_VAL" ;;
            needs)       NEEDS="$FIELD_VAL" ;;
            question)    QUESTION="$FIELD_VAL" ;;
            tried)       TRIED="$FIELD_VAL" ;;
            meanwhile)   MEANWHILE="$FIELD_VAL" ;;
            teammate)    TEAMMATE="$FIELD_VAL" ;;
            worktree)    WT="$FIELD_VAL" ;;
            disposition) DISPOSITION="$FIELD_VAL" ;;
            until)       UNTIL="$FIELD_VAL" ;;
            *) echo "escalate.sh: the fields file named '$FIELD_KEY', which this script does not know." >&2; exit 2 ;;
        esac
    done
fi

case "$CMD" in
    list)
        escalations_list "$FORMAT"
        exit $? ;;

    show)
        [ -n "$TARGET" ] || { echo "escalate.sh show: give the escalation id. \`escalate.sh list\` has them." >&2; exit 2; }
        python3 "$ESCALATIONS_PY" show --id "$TARGET"
        exit $? ;;

    ack)
        [ -n "$TARGET" ] || { echo "escalate.sh ack: give the escalation id. \`escalate.sh list\` has them." >&2; exit 2; }
        python3 "$ESCALATIONS_PY" ack --id "$TARGET" --disposition "$DISPOSITION" --until "$UNTIL"
        exit $? ;;

    raise) ;;

    *)  echo "escalate.sh: unknown subcommand '$CMD'" >&2; usage >&2; exit 2 ;;
esac

# ===========================================================================
# raise
# ===========================================================================
# EVERYTHING MECHANICAL IS DERIVED. The worktree, its branch, its HEAD and your
# name come from the workspace you are standing in — the same generator
# argument scripts/inflight-notify.sh makes about notices: the fields a machine
# can fill are filled, and the operator's fields are the only ones left. Four
# are yours: title, state, question, and whichever of tried/meanwhile apply.
[ -n "$WT" ] || WT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
[ -n "$WT" ] || WT="$PWD"
BRANCH="$(git -C "$WT" symbolic-ref --short HEAD 2>/dev/null || git -C "$WT" rev-parse --short HEAD 2>/dev/null || true)"
HEAD_SHA="$(git -C "$WT" rev-parse HEAD 2>/dev/null || true)"
# The repository NAME, not a path: a worktree path is per-agent and means
# nothing to a reader three days later, while "richos" or "femcboost" does.
REPO_NAME="$(basename "$(git -C "$WT" rev-parse --git-common-dir 2>/dev/null | sed 's|/\.git$||; s|/\.git/worktrees/.*$||' || true)" 2>/dev/null || true)"
case "$REPO_NAME" in ""|.|.git) REPO_NAME="$(basename "${WT%/}")" ;; esac

# Your name defaults to the worktree's own directory name — the same default
# scripts/inflight-ack.sh uses, and one the lead's sweeps already treat as one
# of your addresses. Nothing new has to be remembered.
[ -n "$TEAMMATE" ] || TEAMMATE="$(basename "${WT%/}")"

# --- WHERE THE RECORD FILE GOES, AND WHY IT MAY GO NOWHERE -----------------
# A record names this machine's paths, a branch, a HEAD and who raised what,
# so it is a private record: plans, research and private incident details
# belong in the private record repository, never in a publication-bound one
# (working rule 5; ceo-decisions.md 127). Written into a public repository's
# worktree, it would be published with the next land of that branch.
#
#   NOT PUBLICATION-BOUND   the worktree's own docs/verification/escalations/.
#   PUBLICATION-BOUND       the private record checkout its declaration names
#                           in PRIVATE_SOURCES (scripts/lib/publication-
#                           boundary.sh resolves it, worktree first, then the
#                           main checkout), under docs/verification/escalations/.
#   NO PRIVATE CHECKOUT     no file at all, never the public worktree instead.
#
# Either way it is written ONLY into a `docs/` directory that ALREADY EXISTS.
# Creating `docs/` in a repository that has none would add a root entry, and a
# repository root is nine entries by permanent ruling (ceo-decisions.md 27). A
# repository without one gets the ledger row and no file, and this says so
# rather than failing.
RECORD=""
RECORD_NOTE=""
RECORD_PRIVATE=0
# record_base: sets RECORD_BASE to the checkout the record goes under, or
# leaves it empty with RECORD_NOTE saying why there is none.
record_base() {
    RECORD_BASE="$WT"
    local lib="$SCRIPT_DIR/lib/publication-boundary.sh" root rc decl src top
    if [ ! -f "$lib" ]; then
        RECORD_BASE=""
        RECORD_NOTE="scripts/lib/publication-boundary.sh is missing at $lib, so it cannot be told whether $WT is publication-bound, and no record file was written. The ledger row below IS the escalation."
        return 0
    fi
    # shellcheck source=lib/resolve-main-checkout.sh
    [ -f "$SCRIPT_DIR/lib/resolve-main-checkout.sh" ] && . "$SCRIPT_DIR/lib/resolve-main-checkout.sh"
    # shellcheck source=lib/publication-boundary.sh
    . "$lib"
    root="$(pb_repo_root "$WT")" || return 0
    rc=0; pb_load_declaration "$root" || rc=$?
    case "$rc" in
        0) ;;
        1) return 0 ;;
        *) RECORD_BASE=""
           RECORD_NOTE="the publication declaration of $root cannot be read ($PB_BROKEN_REASON), so no record file was written: it may be publication-bound. The ledger row below IS the escalation."
           return 0 ;;
    esac
    decl="$PB_DECLARATION_FILE"
    RECORD_BASE=""
    rc=0; pb_resolve_sources "$root" || rc=$?
    if [ "$rc" -eq 0 ]; then
        local IFS=$'\t'
        for src in $PB_SOURCES_OK; do
            [ -n "$src" ] && [ -d "$src/docs" ] || continue
            top="$(git -C "$src" rev-parse --show-toplevel 2>/dev/null)" || continue
            [ "$(pb_physical "$top")" = "$(pb_physical "$src")" ] || continue
            rc=0; pb_load_declaration "$src" || rc=$?
            [ "$rc" -eq 1 ] || continue
            RECORD_BASE="$src"
            RECORD_PRIVATE=1
            return 0
        done
    fi
    RECORD_NOTE="$root is publication-bound ($decl) and none of its PRIVATE_SOURCES is a private record checkout with a docs/ directory on this machine, so no record file was written. A record never goes into the publication-bound repository. The ledger row below IS the escalation."
}
if [ "$NO_RECORD" -eq 0 ]; then
    record_base
    if [ -z "$RECORD_BASE" ]; then
        :
    elif [ -d "$RECORD_BASE/docs" ]; then
        RECORD_DIR="$RECORD_BASE/docs/verification/escalations"
        SLUG="$(printf '%s' "$TITLE" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9' '-' | sed 's/-\{2,\}/-/g; s/^-//; s/-$//' | cut -c1-60)"
        [ -n "$SLUG" ] || SLUG="escalation"
        RECORD="$RECORD_DIR/$(date -u +%Y-%m-%d)-$TEAMMATE-$SLUG.md"
    else
        RECORD_NOTE="no docs/ directory in $WT, so no record file was written — writing one would have created a new root entry, which ceo-decisions.md 27 forbids. The ledger row below IS the escalation."
    fi
fi

# --- THE LEDGER ROW FIRST, THE FILE SECOND ---------------------------------
# Deliberate order. The ledger is the DELIVERY and the file is the RECORD, so a
# file write that fails — a read-only tree, a guard, a full disk — must not be
# able to stop the escalation from arriving. The reverse order would put the
# thing that failed before in front of the thing that replaced it.
OUT="$(python3 "$ESCALATIONS_PY" raise \
    --title "$TITLE" --state "$STATE" --for "$AUDIENCE" --question "$QUESTION" \
    --tried "$TRIED" --meanwhile "$MEANWHILE" --needs "$NEEDS" \
    --teammate "$TEAMMATE" --worktree "$WT" --branch "$BRANCH" \
    --repo "$REPO_NAME" --head "$HEAD_SHA" --record "$RECORD")"
RC=$?
[ "$RC" -eq 0 ] || exit "$RC"

ID="$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' 2>/dev/null || true)"
LEDGER="$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["ledger"])' 2>/dev/null || escalations_ledger)"

if [ -n "$RECORD" ]; then
    if mkdir -p "$(dirname "$RECORD")" 2>/dev/null; then
        {
            printf '# Escalation: %s\n\n' "$TITLE"
            printf -- '- id: `%s`\n' "$ID"
            printf -- '- raised: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
            printf -- '- from: %s\n' "$TEAMMATE"
            printf -- '- worktree: `%s` (branch `%s`)\n' "$WT" "$BRANCH"
            [ -n "$HEAD_SHA" ] && printf -- '- head: `%s`\n' "$HEAD_SHA"
            printf -- '- state: **%s**\n' "$STATE"
            printf -- '- for: %s\n' "$AUDIENCE"
            if [ -n "$NEEDS" ]; then printf -- '- needs: **%s** (the answer needs the CEO at a device)\n' "$NEEDS"; fi
            printf '\n'
            printf '## The question\n\n%s\n\n' "$QUESTION"
            if [ -n "$TRIED" ]; then printf '## What was already tried\n\n%s\n\n' "$TRIED"; fi
            if [ -n "$MEANWHILE" ]; then printf '## Proceeding meanwhile\n\n%s\n\n' "$MEANWHILE"; fi
            printf '## How this reaches the lead\n\n'
            printf 'This file is the RECORD, not the delivery. The escalation was written to the\n'
            printf 'engine escalation ledger at `%s` as `%s`, which the\n' "$LEDGER" "$ID"
            printf "lead's session reads at session start and at every turn end WITHOUT this branch\n"
            printf 'being merged. If this file is never landed, the escalation still arrives.\n\n'
            printf 'Close it with:\n\n    escalate.sh ack %s --disposition "<what you decided or did>"\n' "$ID"
        } > "$RECORD" 2>/dev/null || {
            RECORD_NOTE="the record file could not be written to $RECORD. The ledger row was written first for exactly this reason, so the escalation IS raised."
            RECORD=""
        }
    else
        RECORD_NOTE="could not create $(dirname "$RECORD"). The ledger row was written first for exactly this reason, so the escalation IS raised."
        RECORD=""
    fi
fi

echo "escalation RAISED: $ID"
echo "  ledger  : $LEDGER"
echo "            ^^ outside every repository and every session. It reaches the lead with"
echo "               NO MERGE, and it survives this session, this worktree and you."
if [ -n "$RECORD" ]; then
    echo "  record  : $RECORD"
    if [ "$RECORD_PRIVATE" -eq 1 ]; then
        echo "            In the private record checkout, not on your branch, because your"
        echo "            repository is publication-bound. Nothing depends on it being committed."
    else
        echo "            Commit it with your work. Nothing depends on it being merged."
    fi
fi
[ -n "$RECORD_NOTE" ] && echo "  note    : $RECORD_NOTE"
echo "  state   : $STATE"
echo "  for     : $AUDIENCE"
if [ -n "$NEEDS" ]; then
    echo "  needs   : $NEEDS (the lead is woken with it first, and again every 10 minutes until acknowledged)"
fi
echo ""
echo "  It is announced to the lead at every session start and at every turn end, and it"
echo "  gets LOUDER at 1h, 24h and 72h until somebody acknowledges it. You do not have to"
echo "  chase it, wait for a reply, or check that it arrived."
echo ""
echo "  OPTIONAL, and only a doorbell: SendMessage to team-lead saying you raised $ID."
echo "  The mailbox drops roughly half of what crosses it, which is why it is no longer"
echo "  the thing your escalation depends on. Now carry on with your work."
exit 0
