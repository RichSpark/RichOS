#!/usr/bin/env bash
#
# scripts/lib/ceo-asks.sh — "HE WAS ASKED" AS A WITNESSED FACT.
#
# ===========================================================================
# THE DEFECT THIS FILE EXISTS FOR
# ===========================================================================
# On 2026-08-31 a session ended having prepared two decisions for the CEO. The
# next session opened, he asked "what's next", and the orchestrator answered
# with a backlog report and dispatched an engineer. The prepared questions were
# never put to him. He had to ask three times — the last in capitals — before
# his own question reached him.
#
# EVERY GUARD WAS GREEN THROUGHOUT. ceo-todos-lint.sh passed. row-currency-lint.sh
# passed. The commit guards passed. The record was perfect. Nobody had been
# asked anything.
#
# That is a REPEAT, one level up. scripts/cold-open.sh exists for the same
# defect one level down, and its header is the best statement of it in this
# codebase: "A CEO-facing surface was built, gated, tested and landed, and the
# person it was for could not find it. The record was correct. The lint was
# green." Cold-open made his TODOs FINDABLE.
#
#   FINDABLE IS PASSIVE. NOTHING IN THIS ENGINE MAKES HIM GET ASKED.
#
# Every mechanism here verifies the RECORD. None of them verifies the
# CONVERSATION. This file is the missing half, and it is built on exactly one
# idea, borrowed wholesale from guard-inflight-notify.sh and its PostToolUse
# witness, which solved the structurally identical problem one event over:
#
#   AN OBLIGATION A MESSAGE CAN CLAIM TO HAVE DISCHARGED IS MADE REAL BY
#   WITNESSING THE ACTUAL TOOL CALL.
#
# There, the obligation was "tell the teammate main moved" and the witness is
# PostToolUse[SendMessage]. Here the obligation is "put a prepared decision to
# the CEO" and the witness is PostToolUse[AskUserQuestion]. The only way to
# produce the record is to genuinely call the tool. Asserting that you asked
# produces nothing.
#
# ===========================================================================
# THE FOUR PIECES, AND WHY THE BLOCK IS AT THE Agent EVENT
# ===========================================================================
#   scripts/hooks/notice-ceo-asks.sh    PostToolUse[AskUserQuestion] — the
#                                       witness. Names WHICH item was asked,
#                                       from the words he actually saw.
#   scripts/hooks/notice-ceo-unasked.sh Stop — a turn does not end quietly with
#                                       a prepared item this session never
#                                       surfaced.
#   scripts/hooks/session-start-ceo-ask.sh  SessionStart — opens with the top
#                                       item AS A QUESTION.
#
# THE CHOKEPOINT IS DELIBERATE. Dispatching a teammate is the exact act that
# beat the ask on 2026-08-31: he asked "what's next", and what happened next was
# an engineer being spawned. MOTION IS WHAT WINS, so motion is what gets
# blocked. Candidates that were rejected, written down so nobody re-derives it:
#
#   Stop (block the turn)   — NO. A turn that ends BECAUSE he interrupted, or
#     to answer him, is a turn ending correctly; refusing those is how a guard
#     becomes something to switch off. The Stop event carries the NOTICE
#     instead, which is the honest use of it.
#   Every Bash call         — NO. Reading a file is not the act that beat the
#     ask, and a gate that fires on everything is a gate nobody can work behind.
#   SendMessage             — NO. It is the follow-up to a dispatch, not the
#     dispatch; blocking it strands teammates already running.
#
# ===========================================================================
# FAIL-OPEN vs FAIL-CLOSED — the deliberate choice, and its reasoning
# ===========================================================================
# THREE STATES, not two, and the middle one is the whole argument:
#
#   NOT-DECLARED — no CEO_TODOS_REPOS in orchestration.config, and the governed
#     repository itself carries no `.ceo-todos`. There is no CEO list here.
#     STAND DOWN, silently. This mirrors resolve-roots.sh's `not-adopted`
#     exactly: the engine is loaded at USER scope and runs in every directory on
#     the machine, and a repository that never declared a CEO list has no
#     protection to lose. It is also the one place this file DIVERGES from the
#     brief it was built to, and the divergence is stated rather than smuggled:
#     the brief asked for a loud notice on an "absent" list. A notice in every
#     unadopted directory on the machine is precisely the noise this engine
#     already decided not to make (see stop-hook-notice.sh, "EVERY TURN —
#     rejected"), and it would be attached to the case that carries no risk at
#     all. So loudness is spent on the case below, which is the one that can
#     actually hide a failure.
#
#   BROKEN — a list IS declared and cannot be read: the repository is not on
#     this machine, it carries no declaration, its record is missing, the parse
#     failed, python3 is absent. FAIL OPEN, LOUDLY, on every channel available
#     to the hook. Open, because a guard that wedges every dispatch over its own
#     plumbing gets switched off within a day, and a switched-off guard protects
#     nothing forever. Loud, because "declared and unreadable" is the exact
#     shape of a defense that reports 'on' while protecting nothing — the
#     condition resolve-roots.sh calls BROKEN and treats as the serious one.
#
#   DECLARED AND READABLE, holding prepared items — FAIL CLOSED. The gate has a
#     real subject, it can name it, and the escape hatch below costs one line.
#
# THE ESCAPE HATCH IS A LIVE PROMPT LINE, in the house idiom of
# guard-worktree-isolation.sh's `main-checkout-run:` and guard-resume-isolation
# .sh's `resume-ack:`:
#
#     ceo-todos-deferred: <reason>
#
# in the Agent spawn prompt. It permits that one dispatch and appends to
# .claude/state/ceo-todos-defers.log. When the CEO says "get on with it",
# nothing wedges — and the fact that he said it is on the record.
#
# ===========================================================================
# WHAT THIS CANNOT DO — named, not implied
# ===========================================================================
#  1. IT CANNOT JUDGE THE QUESTION. A question that matches item 1.1 may still
#     be a poor rendering of it. No text predicate can tell those apart, and one
#     that claimed to would become a score to optimize instead of a fact. What
#     is engineered out is a session where he was never asked AT ALL.
#  2. IT CANNOT STOP FABRICATION. Anyone with Bash can append a line to the
#     ledger. The failure being engineered out is FORGETTING; fabrication is a
#     different act and no hook in this engine claims to stop it. (Stated in
#     these words by notice-inflight-sends.sh, and true here for the same
#     reason.)
#  3. IT CANNOT MAKE HIM ANSWER. The obligation discharged is the ASK.
#
# ===========================================================================
# THE LEDGER
# ===========================================================================
#     <entity root>/.claude/state/ceo-asks.jsonl
#
# One JSON line per question actually put to the user, written by the witness
# inside the orchestrator's own tool call. Keyed on session_id, because the rule
# is per-session: a question asked yesterday did not reach the person who opened
# a session today.
#
# THE ONE WIDENING: THE APP RUN (operator contract notes §4 item 2, Frank's F10).
# In the RichOS app every conversation has its own lead, so every conversation
# is its own session, and a per-session key made each new conversation's lead
# refuse its first dispatch until it had put the SAME top item to him again. He
# is one person looking at one running app, so there the unit is the app run:
# the claim id the app gives every lead it starts (RICHOS_OPERATOR_LEAD, one
# `app-<uuid>` per launch, operator_claim.rs) and writes onto every question it
# witnesses as `app_run`. An ask witnessed in this run discharges the gate for
# every lead in this run. What that does NOT widen, and why (§17): the key comes
# from the lead's process environment, which the app set and the lead's model
# cannot change, and from a ledger line only the witness writes. No sentence a
# lead writes ("another conversation already asked him") is read by anything
# here. A terminal session has no RICHOS_OPERATOR_LEAD and stays per-session;
# the next launch has a new claim id, so yesterday's run discharges nothing.
#
# It lives in the governed repository's state directory — the
# same place main-checkout-runs.log, resume-acks.log and the other opt-out
# ledgers live — rather than in the session team directory, because the guard,
# the notice and the session-start announcement all resolve that root the same
# way and can therefore never look in three different places for one fact.
#
# ===========================================================================
# USAGE
# ===========================================================================
#     . scripts/lib/ceo-asks.sh
#     ca_require                     || echo "$CA_BROKEN"
#     ca_resolve "<entity-root>"     # -> CA_STATUS / CA_REPOS / CA_REASON
#     ca_items_json "<out.json>"     # -> the prepared items, one parse per repo
#     ca_ledger_path "<entity-root>"
#     ca_assess "<entity-root>" "<session-id>" ["<app-run>"]  # -> CA_VERDICT / CA_* counts
#     ca_app_run                     # -> this process's app run, or nothing
#
# Safe to source repeatedly. Never changes the caller's cwd.

if [ -n "${_CEO_ASKS_SH_SOURCED:-}" ]; then
    return 0 2>/dev/null || true
fi
_CEO_ASKS_SH_SOURCED=1

_CA_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# The declaration key an adopter puts in orchestration.config. Space-separated
# paths, absolute or relative to the governed repository's root.
#
# WHY A KEY AT ALL, rather than "look in the repo you are seated in": the shape
# of this operation is a session seated in ONE repository whose CEO list lives
# in ANOTHER (femcboost seat, richos-hq list, richos engine). That is the
# normal case here, not an edge — guard-inflight-notify.sh learned the same
# lesson about pushes. Nothing is inferred across repositories; it is DECLARED,
# so a list that stops being watched is a visible, reviewable diff.
CA_REPOS_KEY="CEO_TODOS_REPOS"

CA_LEDGER_NAME="ceo-asks.jsonl"
# RENAMED 2026-08-31, CEO-confirmed. These shipped for a few hours as
# `ceo-queue-defers.log` / `ceo-queue-deferred:`, and `queue` is the British
# word for the CEO's list — the exact thing his 2026-08-29 ruling renamed to
# CEO-TODOs. NO BACKWARD-COMPATIBILITY SHIM, and the reason is that this is not
# the `.ceo-queue` case: that was a FILE ON AN ADOPTER'S DISK, so a clean cut
# would have made a declared repository stand down silently. This is a PROMPT
# LINE nobody has on disk and a log nobody had yet written (verified: no
# ceo-queue-defers.log existed in either repository at the rename). Carrying a
# compatibility alias here would preserve, in the mechanism, the very word the
# mechanism's own ruling removed.
CA_DEFER_LOG_NAME="ceo-todos-defers.log"
CA_DEFER_MARKER="ceo-todos-deferred:"

CA_BROKEN=""
CA_STATUS=""
CA_REASON=""
CA_REPOS=""

# ---------------------------------------------------------------------------
# ca_require — everything this predicate needs is present, or say what is not.
# ---------------------------------------------------------------------------
# rc 0 usable; rc 1 with CA_BROKEN set otherwise. Callers decide what to do with
# that: the guard fails OPEN and shouts, because see the header.
ca_require() {
    CA_BROKEN=""
    if ! command -v python3 >/dev/null 2>&1; then
        CA_BROKEN="python3 is not on PATH, so neither the record nor the ledger can be read"
        return 1
    fi
    if [ ! -f "$_CA_LIB_DIR/ceo-asks.py" ]; then
        CA_BROKEN="scripts/lib/ceo-asks.py is missing at $_CA_LIB_DIR/ceo-asks.py — the whole predicate lives there"
        return 1
    fi
    if [ ! -f "$_CA_LIB_DIR/ceo-todos.sh" ]; then
        CA_BROKEN="scripts/lib/ceo-todos.sh is missing at $_CA_LIB_DIR/ceo-todos.sh — the CEO's items are parsed there and nowhere else"
        return 1
    fi
    return 0
}

# ---------------------------------------------------------------------------
# ca_resolve <entity-root> — which repositories hold this seat's CEO TODOs?
# ---------------------------------------------------------------------------
# Sets:
#   CA_STATUS  declared | not-declared | broken
#   CA_REPOS   newline-separated absolute repository roots (status=declared)
#   CA_REASON  why, when status is not `declared`
#
# rc 0 declared; 1 not-declared; 2 broken.
ca_resolve() {
    local root="${1:-}" spec="" p abs found=0 problems=""
    CA_STATUS=""; CA_REASON=""; CA_REPOS=""

    if [ -z "$root" ] || [ ! -d "$root" ]; then
        CA_STATUS="broken"
        CA_REASON="no governed repository was resolved, so there is nowhere to look for a declaration"
        return 2
    fi

    # The declaration is READ from orchestration.config, which the engine's
    # other hooks source directly. Sourced in a SUBSHELL so an adopter's config
    # cannot clobber this library's own variables — the same care
    # scan-secrets.sh takes with its thresholds.
    if [ -f "$root/orchestration.config" ]; then
        spec="$(
            # shellcheck disable=SC1091
            . "$root/orchestration.config" >/dev/null 2>&1 || true
            eval "printf '%s' \"\${$CA_REPOS_KEY:-}\""
        )"
    fi

    # NO KEY: the governed repository may still own its OWN TODOs, which is the
    # ordinary single-repository case and needs no configuration at all.
    if [ -z "$spec" ]; then
        if [ -f "$root/.ceo-todos" ] || [ -f "$root/.ceo-queue" ]; then
            spec="."
        else
            CA_STATUS="not-declared"
            CA_REASON="no $CA_REPOS_KEY in $root/orchestration.config and no .ceo-todos at $root — this repository declares no CEO TODOs"
            return 1
        fi
    fi

    for p in $spec; do
        case "$p" in
            "~"|"~/"*) p="$HOME${p#\~}" ;;
        esac
        case "$p" in
            /*) abs="$p" ;;
            *)  abs="$root/$p" ;;
        esac
        if [ ! -d "$abs" ]; then
            problems="$problems; declared repository '$p' is not on this machine (looked at $abs)"
            continue
        fi
        abs="$(cd "$abs" 2>/dev/null && pwd -P)" || {
            problems="$problems; declared repository '$p' could not be resolved"
            continue
        }
        if [ ! -f "$abs/.ceo-todos" ] && [ ! -f "$abs/.ceo-queue" ]; then
            problems="$problems; declared repository '$p' carries no .ceo-todos, so it declares no CEO TODOs and $CA_REPOS_KEY is pointing at the wrong place"
            continue
        fi
        CA_REPOS="$CA_REPOS$abs
"
        found=$((found + 1))
    done

    # ANY declared-but-unusable repository is BROKEN, even when another one
    # resolved. A gate that quietly enforced against 1 of 2 declared lists
    # would report green over an unread one, which is the defect this engine has
    # now found in itself several times.
    if [ -n "$problems" ]; then
        CA_STATUS="broken"
        CA_REASON="${problems#; }"
        return 2
    fi
    if [ "$found" -eq 0 ]; then
        CA_STATUS="broken"
        CA_REASON="$CA_REPOS_KEY is set in $root/orchestration.config but named no usable repository"
        return 2
    fi
    CA_STATUS="declared"
    return 0
}

# ---------------------------------------------------------------------------
# ca_items_json <out-path> — the prepared items of every declared repository.
# ---------------------------------------------------------------------------
# Writes a JSON array to <out-path>. Requires ca_resolve to have succeeded.
# Each element: repo, section, id, state, title, open, time, done, unblocks.
#
# THE PARSE IS ceo-todos.py's `items` MODE, never a reader of CEO-TODOs.md. That
# page is a PROJECTION of the record; parsing it would be a second parser of one
# file, which is the defect ceo-todos.py's own header was written about.
#
# rc 0 produced; 2 broken, with CA_BROKEN set.
ca_items_json() {
    local out="${1:-}" repo tsv rc all=""
    CA_BROKEN=""
    [ -n "$out" ] || { CA_BROKEN="ca_items_json: no output path"; return 2; }

    # shellcheck source=ceo-todos.sh
    . "$_CA_LIB_DIR/ceo-todos.sh"

    while IFS= read -r repo; do
        [ -n "$repo" ] || continue
        rc=0
        ct_load_declaration "$repo" >/dev/null 2>&1 || rc=$?
        if [ "$rc" -ne 0 ]; then
            CA_BROKEN="$repo declares CEO TODOs but its declaration could not be read (${CT_BROKEN_REASON:-rc $rc})"
            return 2
        fi
        if [ ! -f "$repo/$CT_TODO_RECORD" ]; then
            CA_BROKEN="$repo declares TODO_RECORD=$CT_TODO_RECORD and that file is not on disk — the CEO's items cannot be read, and an unread list must never look like an empty one"
            return 2
        fi
        ct_resolve_roots "$repo" >/dev/null 2>&1 || true
        rc=0
        tsv="$(ct_items "$CT_TODO_RECORD" "$repo/$CT_TODO_RECORD" "$repo" 2>/dev/null)" || rc=$?
        if [ "$rc" -ne 0 ]; then
            CA_BROKEN="$repo/$CT_TODO_RECORD could not be parsed ($(printf '%s' "$tsv" | sed -n '1p'))"
            return 2
        fi
        # awk, not sed: BSD sed does not read `\t` as a tab in a replacement, so
        # the obvious `sed "s|^ITEM\t|ITEM\t$repo\t|"` inserts the letter t on
        # macOS and every field after it shifts by one. Silently.
        all="$all$(printf '%s\n' "$tsv" | awk -F'\t' -v r="$repo" \
            'BEGIN{OFS="\t"} $1=="ITEM"{print "ITEM", r, $2, $3, $4, $5, $6, $7, $8, $9}')
"
    done <<EOF
$CA_REPOS
EOF

    python3 -c '
import json, os, sys
rows = []
for line in sys.stdin.read().split("\n"):
    f = line.split("\t")
    if not f or f[0] != "ITEM":
        continue
    f = (f + [""] * 10)[:10]
    rows.append({
        "repo": f[1], "section": f[2], "id": f[3], "state": f[4],
        "title": f[5], "open": f[6], "time": f[7], "done": f[8],
        "unblocks": f[9],
    })
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(rows, fh)
' "$out" <<<"$all" || { CA_BROKEN="the item records could not be encoded"; return 2; }
    return 0
}

# ---------------------------------------------------------------------------
# ca_ledger_path <entity-root>
# ---------------------------------------------------------------------------
ca_ledger_path() {
    printf '%s\n' "${1:-}/.claude/state/$CA_LEDGER_NAME"
}

# ---------------------------------------------------------------------------
# ca_app_run — the RichOS app run this process belongs to, or nothing.
# ---------------------------------------------------------------------------
# The claim id the app put in its lead's environment (operator_profile.rs
# LEAD_CLAIM_ENV). A hook inherits it from the lead's `claude`; a terminal
# session has none. Read from the environment only, never from a payload or a
# prompt, so nothing the lead writes can supply it (see THE ONE WIDENING above).
ca_app_run() {
    printf '%s' "${RICHOS_OPERATOR_LEAD:-}" | tr -d '[:space:]'
}

# ---------------------------------------------------------------------------
# ca_assess <entity-root> <session-id> [<app-run>]
# ---------------------------------------------------------------------------
# The verdict for THIS session, or for this app run. Requires ca_resolve to
# have succeeded. <app-run> defaults to ca_app_run when it is not given at all;
# pass "" to assess the session alone.
#
# Sets:
#   CA_VERDICT    OPEN | SATISFIED | NOTHING-PREPARED
#   CA_PREPARED   count of prepared items across every declared repository
#   CA_ASKED      how many of them were put to him this session
#   CA_UNASKED    how many were not
#   CA_ASK_LINES  one "repo<TAB>id<TAB>title<TAB>one-line-ask" row per unasked
#                 item, in document order
#
# rc 0 SATISFIED or NOTHING-PREPARED; 1 OPEN; 2 broken (CA_BROKEN set).
ca_assess() {
    local root="${1:-}" sid="${2:-}" items job out rc ledger run
    if [ "$#" -ge 3 ]; then run="$3"; else run="$(ca_app_run)"; fi
    CA_BROKEN=""; CA_VERDICT=""; CA_ASK_LINES=""
    CA_PREPARED=0; CA_ASKED=0; CA_UNASKED=0

    items="$(mktemp -t ceo-asks-items.XXXXXX.json)" || { CA_BROKEN="no temp file"; return 2; }
    job="$(mktemp -t ceo-asks-job.XXXXXX.json)" || { rm -f "$items"; CA_BROKEN="no temp file"; return 2; }

    if ! ca_items_json "$items"; then
        rm -f "$items" "$job"
        return 2
    fi

    ledger="$(ca_ledger_path "$root")"
    CA_J_ITEMS="$items" CA_J_LEDGER="$ledger" CA_J_SID="$sid" CA_J_RUN="$run" python3 -c '
import json, os, sys

with open(os.environ["CA_J_ITEMS"], encoding="utf-8") as fh:
    items = json.load(fh)

sid = os.environ.get("CA_J_SID") or ""
run = os.environ.get("CA_J_RUN") or ""
asks = []
path = os.environ.get("CA_J_LEDGER") or ""
try:
    # errors="replace": a byte that is not UTF-8 makes that ONE line corrupt (and
    # skipped below), it must not make the whole ledger unreadable.
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except Exception:
                # A corrupt line is SKIPPED, never fatal: the ledger is
                # append-only evidence, and one bad line must not be able to
                # erase every good one above it. It also cannot manufacture a
                # discharge, because only a parsed record can carry an item id.
                continue
            if not isinstance(rec, dict):
                continue
            # THE SESSION FILTER. A question put to him yesterday did not reach
            # the person who opened a session today, so it discharges nothing
            # today. An empty session id on either side never matches: a record
            # that cannot be attributed to this session is not evidence about
            # this session.
            if not sid or str(rec.get("session_id") or "") != sid:
                # THE APP RUN: another conversation of the SAME running app
                # (see THE ONE WIDENING in the header). Both sides must carry
                # it; an empty run never matches, exactly like the session.
                if not (run and str(rec.get("app_run") or "") == run):
                    continue
            asks.append(rec)
except FileNotFoundError:
    pass                      # no ledger yet: nothing has been asked
except Exception:
    # A ledger that EXISTS but cannot be read (a directory, a permission fault)
    # is not evidence that nothing was asked: every prepared item would turn
    # into a fresh obligation. Exit non-zero so the shell reports BROKEN (P5-50).
    sys.exit(3)

json.dump({"mode": "assess", "items": items, "asks": asks}, open(sys.argv[1], "w", encoding="utf-8"))
' "$job" 2>/dev/null || { rm -f "$items" "$job"; CA_BROKEN="the ledger could not be read"; return 2; }

    rc=0
    out="$(python3 "$_CA_LIB_DIR/ceo-asks.py" "$job" 2>/dev/null)" || rc=$?
    rm -f "$items" "$job"
    if [ "$rc" -ne 0 ]; then
        CA_BROKEN="the CEO-ask predicate could not run (rc $rc)"
        return 2
    fi

    CA_PREPARED="$(printf '%s\n' "$out" | awk -F'\t' '$1=="PREPARED"{print $2; exit}')"
    CA_ASKED="$(printf '%s\n' "$out" | awk -F'\t' '$1=="ASKED"{print $2; exit}')"
    CA_UNASKED="$(printf '%s\n' "$out" | awk -F'\t' '$1=="UNASKED"{print $2; exit}')"
    CA_VERDICT="$(printf '%s\n' "$out" | awk -F'\t' '$1=="VERDICT"{print $2; exit}')"
    CA_ASK_LINES="$(printf '%s\n' "$out" | awk -F'\t' '$1=="ASK"{print $2"\t"$3"\t"$4"\t"$5}')"

    [ -n "$CA_VERDICT" ] || { CA_BROKEN="the predicate produced no verdict"; return 2; }
    # An `if`, not `[ ... ] && return 1`: an AND-OR list whose whole status is
    # non-zero DOES trip `set -e` in a caller, so the shorthand would end the
    # hook on the SATISFIED path — the one path that must carry on quietly.
    if [ "$CA_VERDICT" = "OPEN" ]; then
        return 1
    fi
    return 0
}

# ---------------------------------------------------------------------------
# ca_match <question-text-file> <items-json> — which item was asked?
# ---------------------------------------------------------------------------
# Prints the raw MATCH / UNMATCHED line from ceo-asks.py. rc 0 always unless the
# predicate itself could not run (2).
ca_match() {
    local qfile="${1:-}" items="${2:-}" job rc out
    job="$(mktemp -t ceo-asks-match.XXXXXX.json)" || return 2
    CA_M_Q="$qfile" CA_M_I="$items" python3 -c '
import json, os, sys
q = open(os.environ["CA_M_Q"], encoding="utf-8", errors="replace").read()
items = json.load(open(os.environ["CA_M_I"], encoding="utf-8"))
json.dump({"mode": "match", "question": q, "items": items}, open(sys.argv[1], "w", encoding="utf-8"))
' "$job" 2>/dev/null || { rm -f "$job"; return 2; }
    rc=0
    out="$(python3 "$_CA_LIB_DIR/ceo-asks.py" "$job" 2>/dev/null)" || rc=$?
    rm -f "$job"
    [ "$rc" -eq 0 ] || return 2
    printf '%s\n' "$out"
    return 0
}
