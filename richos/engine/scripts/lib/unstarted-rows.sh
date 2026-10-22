#!/usr/bin/env bash
#
# scripts/lib/unstarted-rows.sh — WRITING THE WORK DOWN IS NOT STARTING IT.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# The lead finds work, writes it down as a row, and stops — because writing the
# row satisfies the same urge as doing the work. On 2026-08-31, three times in
# one day:
#
#   1. It emptied the queue's table, announced "the backlog is essentially
#      you", and stopped — while the working record's section 3, a section
#      LITERALLY TITLED "Buildable now — nobody blocked", still held open rows.
#   2. It found four payload defects, opened four rows, and dispatched one.
#   3. It told the CEO "Zach is building it now" about THIS HOOK, and had not
#      dispatched it. He caught it by asking where the fourth agent was.
#
# THE PROSE FIX ALREADY FAILED. After (1) the lead amended the queue's own
# header to say the queue is two files and an empty table is not an empty
# queue. That text is still there, in his own words, and (2) and (3) happened
# anyway within hours.
#
#   A RULE ENFORCED BY ATTENTION LASTS EXACTLY AS LONG AS THE ATTENTION.
#
# That sentence is written at the top of two other predicates in this engine.
# It is the same defect a third time, and a paragraph a person wrote for
# themselves is the weakest form of it.
#
# ===========================================================================
# THE EVENT IS THE TURN END, NOT THE LAND — the whole design decision
# ===========================================================================
# There is already a rule that fires when a land ends: start the top unblocked
# item before reporting. It did not catch any of the three, and the reason is
# structural rather than a matter of degree.
#
#   WORK IS CREATED WHEN SOMETHING IS NOTICED, AND NOTICING HAPPENS WHILE
#   WRITING A REPORT. By the time a land ends, the noticing turn is over.
#
# Failure (3) is the clean proof: nothing landed. A row was created and a
# sentence was said about it in the same turn, and no land-shaped gate could
# ever have been standing there. So this fires at EVERY turn end — and the
# reporting turn, the one where this dies, is the one it must catch.
#
# ===========================================================================
# WHAT IT COSTS TO GET THIS WRONG IN EITHER DIRECTION
# ===========================================================================
# TOO LOUD. A row genuinely waiting on the CEO — his machine, his billing, his
# decision, a toolchain that is not on this Mac — shouted about every turn
# trains the eye to skip the line, and a muted notice is worse than none: the
# gap is still there and now there is a line on screen that reports it, which
# nobody reads. Two mechanisms hold that down, and neither is a threshold:
#
#   * A ROW DECLARES ITS BLOCKER, AND A DECLARED BLOCKER IS SILENT. In the
#     queue that is the `Blocked by` cell the table already has; in the
#     section it is `**Blocked:** <who>` in the row's prose. On the day this
#     was written, all four genuinely-CEO rows were ALREADY silent under that
#     rule, with no edit to anything, because the record was already carrying
#     the fact.
#   * THE NOTICE IS STATE-CHANGE DE-DUPLICATED, through the same ledger every
#     other Stop notice uses (scripts/lib/stop-hook-notice.sh). A stable set of
#     unstarted rows is announced ONCE. It speaks again when the SET changes —
#     which is precisely the moment a row is created and not started, and
#     precisely the moment one is finally picked up.
#
# TOO QUIET. Silence must never be producible by the check failing to look.
# Every unreadable, unparseable or empty-corpus condition is LOUD, listed in
# scripts/lib/unstarted-rows.py, and the sweep writes a RECEIPT on every run —
# so "it said nothing" can always be told apart from "it did not run".
#
# ===========================================================================
# WHAT IT CANNOT SEE — stated here, not discovered later
# ===========================================================================
#   * WHETHER A CLAIM IS TRUE. A worktree named for a row is taken at its word.
#     The claim is cheap to make and nobody gains from a false one; the
#     expensive direction is the other one, and that fails loud.
#   * WORK RUNNING WITH NO WORKTREE — a teammate on somebody else's machine, a
#     job in CI, a thing the CEO is doing himself. It has no observable trace
#     here and will be reported as unstarted. `**Blocked:**` is the answer.
#   * WHETHER THE ROW IS WORTH DOING. It reads states, never prose. A row that
#     should be deleted is reported until somebody deletes it, which is the
#     correct nag.
#   * A TURN IN A SESSION THAT DID NOT LOAD THIS HOOK. Hooks snapshot at
#     session start; engine-status.sh announces that at every session start.
#
# ===========================================================================
# USAGE
# ===========================================================================
#     . "$SCRIPT_DIR/../lib/unstarted-rows.sh"
#     ur_resolve "$repo"      # 0 governed / 1 stand down / 2 broken
#     ur_sweep                # -> UR_VERDICT UR_COUNTS UR_ROWS UR_UNSTARTED
#     ur_receipt "$path"      # the positive probe, written on every path

# The declaration name, in the convention publication-completeness.sh derives
# from shipped source. It is OPTIONAL: a repository whose queue record sits at
# the default path is governed without it. The file exists to move the path and
# to narrow the actionable vocabulary.
: "${UNSTARTED_ROWS_DECLARATION:=.unstarted-rows}"

# The queue record, when nothing declares one. Named rather than derived
# because there is no other surface to derive it from — .ceo-todos names the
# WORKING record and knows nothing about the lead's own ordered backlog.
UR_DEFAULT_QUEUE="RICH-TODOs.md"

# Which warrant tokens describe work somebody could pick up now: OPEN, BUILT,
# BOUNDED, and BLOCKED-ON-RICH. By the record's own contract that last one
# means "Rich has not prepared this for the CEO yet", and the record also uses
# it for work Rich consciously parked: a hold on his work by Rich, which stays
# in sight (2026-10-22, richos-hq docs/plans/2026-10-22-no-silent-holds.md; the
# CEO: "WHEN WILL [THIS] STOP ONCE AND FOR ALL???"). Until then it was left out
# as "consciously parked", which is how such rows went quiet.
UR_DEFAULT_ACTIONABLE="OPEN BUILT BOUNDED BLOCKED-ON-RICH"

_UR_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_UR_RC_LIB="$_UR_LIB_DIR/row-currency.sh"
_UR_PY="$_UR_LIB_DIR/unstarted-rows.py"

# ---------------------------------------------------------------------------
# ur_require_libs
# ---------------------------------------------------------------------------
# The record's row grammar and the declaration parsing both live in the
# row-currency library, which owns .ceo-todos through ceo-todos.sh. This file
# re-uses both rather than carrying a second copy, for the reason those files
# state at length: a predicate in two copies is the defect class this engine
# keeps finding in itself.
ur_require_libs() {
    UR_BROKEN_REASON=""
    if [ ! -f "$_UR_RC_LIB" ]; then
        UR_BROKEN_REASON="scripts/lib/row-currency.sh is missing at $_UR_RC_LIB. The record's location, its governed sections and its status vocabulary are read through that library and nowhere else."
        return 2
    fi
    # shellcheck source=./row-currency.sh
    . "$_UR_RC_LIB"
    if ! rc_require_ceo_todos_lib; then
        UR_BROKEN_REASON="$RC_BROKEN_REASON"
        return 2
    fi
    if [ ! -f "$_UR_PY" ]; then
        UR_BROKEN_REASON="scripts/lib/unstarted-rows.py is missing at $_UR_PY. The predicate is not present, so nothing was checked — and a sweep that reports a clean queue because its checker is absent is the exact shape of the failure it exists to catch."
        return 2
    fi
    if ! command -v python3 >/dev/null 2>&1; then
        UR_BROKEN_REASON="python3 is not on PATH, so the two records cannot be parsed."
        return 2
    fi
    return 0
}

# ---------------------------------------------------------------------------
# ur_load_declaration <record_repo>
# ---------------------------------------------------------------------------
# PARSED, never sourced: sourcing a file to read two settings out of it hands
# arbitrary code execution to anything that can write a config.
#
#   rc 0  parsed (UR_DECLARED=1) or absent (UR_DECLARED=0)
#   rc 2  BROKEN
ur_load_declaration() {
    local root="${1:-}" f line key val
    UR_DECLARED=0
    UR_QUEUE_REL=""
    UR_ACTIONABLE_TOKENS="$UR_DEFAULT_ACTIONABLE"
    UR_DECLARATION_FILE=""
    UR_BROKEN_REASON=""

    [ -n "$root" ] || return 2
    f="$root/$UNSTARTED_ROWS_DECLARATION"
    [ -f "$f" ] || return 0
    UR_DECLARED=1
    UR_DECLARATION_FILE="$f"

    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            ''|'#'*) continue ;;
        esac
        case "$line" in
            *=*) ;;
            *)
                case "$line" in
                    *[![:space:]]*)
                        UR_BROKEN_REASON="unparseable line in $UNSTARTED_ROWS_DECLARATION: $line"
                        return 2 ;;
                esac
                continue ;;
        esac
        key="${line%%=*}"
        val="${line#*=}"
        key="$(printf '%s' "$key" | tr -d '[:space:]')"
        val="$(printf '%s' "$val" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' -e 's/^"//' -e 's/"$//' -e "s/^'//" -e "s/'$//")"
        case "$key" in
            QUEUE_RECORD)       UR_QUEUE_REL="$val" ;;
            ACTIONABLE_TOKENS)  UR_ACTIONABLE_TOKENS="$val" ;;
            *)
                UR_BROKEN_REASON="unknown key '$key' in $UNSTARTED_ROWS_DECLARATION. A key nobody reads is a setting somebody believes is in force; the declaration refuses rather than ignoring it."
                return 2 ;;
        esac
    done < "$f"

    if [ -z "$UR_QUEUE_REL" ]; then
        UR_BROKEN_REASON="$UNSTARTED_ROWS_DECLARATION names no QUEUE_RECORD. The file exists to say where the queue is; an empty one switches a check on over nothing."
        return 2
    fi
    case "$UR_QUEUE_REL" in
        /*|*..*)
            UR_BROKEN_REASON="QUEUE_RECORD must be a plain repository-relative path, not '$UR_QUEUE_REL'"
            return 2 ;;
    esac
    return 0
}

# ---------------------------------------------------------------------------
# ur_resolve <repo_root>
# ---------------------------------------------------------------------------
#   rc 0  governed  -> UR_RECORD_REPO UR_QUEUE_FILE UR_QUEUE_LABEL
#                      UR_RECORD_FILE UR_RECORD_LABEL UR_SECTIONS
#                      UR_STATUS_TOKENS UR_TERMINAL_TOKENS UR_ACTIONABLE_TOKENS
#                      UR_SCAN_ROOTS
#   rc 1  stand down -> UR_STANDDOWN_REASON
#   rc 2  broken     -> UR_BROKEN_REASON
ur_resolve() {
    local root="${1:-}" here rrc pair abs
    UR_RECORD_REPO=""; UR_QUEUE_FILE=""; UR_QUEUE_LABEL=""
    UR_RECORD_FILE=""; UR_RECORD_LABEL=""; UR_SECTIONS=""
    UR_STATUS_TOKENS=""; UR_TERMINAL_TOKENS=""; UR_SCAN_ROOTS=""
    UR_STANDDOWN_REASON=""; UR_BROKEN_REASON=""

    ur_require_libs || return 2
    [ -n "$root" ] || { UR_BROKEN_REASON="ur_resolve called with no repository"; return 2; }
    here="$(ct_repo_root "$root")" || {
        UR_STANDDOWN_REASON="$root is not inside a repository."
        return 1
    }

    # WHERE THE RECORD IS comes from the row-currency contract, which reads it
    # out of .ceo-todos. Not re-declared here: a second copy of a fact is the
    # drift this engine keeps finding in itself.
    rrc=0
    rc_load_declaration "$here" || rrc=$?
    case "$rrc" in
        0) ;;
        1)
            UR_STANDDOWN_REASON="no $ROW_CURRENCY_DECLARATION in $here, so no working record is declared and there are no governed rows to sweep."
            return 1 ;;
        *) UR_BROKEN_REASON="$RC_BROKEN_REASON"; return 2 ;;
    esac
    rrc=0
    rc_resolve_record "$here" || rrc=$?
    case "$rrc" in
        0) ;;
        1) UR_STANDDOWN_REASON="$RC_STANDDOWN_REASON"; return 1 ;;
        *) UR_BROKEN_REASON="$RC_BROKEN_REASON"; return 2 ;;
    esac

    UR_RECORD_REPO="$RC_RECORD_REPO"
    UR_RECORD_LABEL="$RC_RECORD_REL"
    UR_RECORD_FILE="$RC_RECORD_REPO/$RC_RECORD_REL"
    UR_SECTIONS="$RC_ROW_SECTIONS"
    UR_STATUS_TOKENS="$RC_STATUS_TOKENS"
    UR_TERMINAL_TOKENS="$RC_TERMINAL_TOKENS"

    ur_load_declaration "$UR_RECORD_REPO" || return 2
    if [ "$UR_DECLARED" -eq 1 ]; then
        UR_QUEUE_LABEL="$UR_QUEUE_REL"
        UR_QUEUE_FILE="$UR_RECORD_REPO/$UR_QUEUE_REL"
        if [ ! -f "$UR_QUEUE_FILE" ]; then
            # A DECLARED queue that is not there is BROKEN, never a stand-down.
            # Somebody wrote down where it lives; believing them and finding
            # nothing is the loudest fact available.
            UR_BROKEN_REASON="$UNSTARTED_ROWS_DECLARATION in $UR_RECORD_REPO declares QUEUE_RECORD='$UR_QUEUE_REL' and there is no such file. The queue is two files; one of them is missing and no sweep over the other one is honest."
            return 2
        fi
    else
        UR_QUEUE_LABEL="$UR_DEFAULT_QUEUE"
        UR_QUEUE_FILE="$UR_RECORD_REPO/$UR_DEFAULT_QUEUE"
        if [ ! -f "$UR_QUEUE_FILE" ]; then
            UR_STANDDOWN_REASON="$UR_RECORD_REPO carries neither $UNSTARTED_ROWS_DECLARATION nor $UR_DEFAULT_QUEUE, so this repository has no ordered backlog for anything to be unstarted in."
            return 1
        fi
    fi
    if [ ! -f "$UR_RECORD_FILE" ]; then
        UR_BROKEN_REASON="the working record declared by .ceo-todos ($RC_RECORD_REL) is not present in $UR_RECORD_REPO."
        return 2
    fi

    # WHERE TO LOOK FOR WORK IN PROGRESS. Derived from the record's own
    # artifact roots — the repositories it already says the work lives in —
    # plus the repository this session is seated in. Never a typed list.
    ct_resolve_roots "$UR_RECORD_REPO" >/dev/null 2>&1 || true
    UR_SCAN_ROOTS="$here"
    for pair in $(printf '%s' "$CT_ROOTS_OK" | tr '\t' ' '); do
        abs="${pair#*=}"
        [ -d "$abs" ] || continue
        case " $UR_SCAN_ROOTS " in
            *" $abs "*) ;;
            *) UR_SCAN_ROOTS="$UR_SCAN_ROOTS $abs" ;;
        esac
    done
    return 0
}

# ---------------------------------------------------------------------------
# ur_collect_claims
# ---------------------------------------------------------------------------
# Every live worktree across the governed repositories, reduced to the set of
# row ids something is running for. Requires ur_resolve.
#
# THREE WAYS TO CLAIM, AND ALL THREE ARE EXACT MATCHES.
#   * the branch name contains  row-<id>  (or row<id>, or row-3-3f for 3.3f)
#   * the worktree directory name contains the same
#   * <worktree>/.claude/row-claims.txt lists the id, one per line
#
# A FUZZY MATCH WOULD FAIL TOWARD SILENCE, which is the one direction this must
# never fail in: a branch that merely resembles a row id would switch the
# notice off for work nobody started. So a worktree that claims nothing is
# reported as claiming nothing, and the cost of that is one line in a text file.
#
# Sets UR_CLAIMS_JSON (a JSON object: spelling -> where it came from).
ur_collect_claims() {
    local root line wt br claimfile id wtlist
    local pairs=""
    UR_CLAIMS_BROKEN=""
    for root in $UR_SCAN_ROOTS; do
        wt=""
        br=""
        # THE LIST IS THE EVIDENCE OF RUNNING WORK, so a list that cannot be read
        # is an unknown inventory and never an empty one: rows would be called
        # unstarted over work that is in flight.
        wtlist="$(git -C "$root" worktree list --porcelain 2>/dev/null)" || {
            UR_CLAIMS_BROKEN="the worktree list of $root could not be read, so the existing work claims are unknown; coverage is unknown."
            return 0
        }
        while IFS= read -r line; do
            case "$line" in
                "worktree "*)
                    wt="${line#worktree }"
                    br="" ;;
                "branch "*)
                    br="${line#branch }"
                    br="${br#refs/heads/}" ;;
                "")
                    [ -n "$wt" ] || continue
                    pairs="$pairs$(printf '%s\t%s' "$wt" "$br")"$'\n'
                    wt=""; br="" ;;
            esac
        done <<EOF
$wtlist
EOF
        [ -n "$wt" ] && pairs="$pairs$(printf '%s\t%s' "$wt" "$br")"$'\n'
    done

    # The claim files are read here, in shell, and handed over as data: the
    # predicate never touches the filesystem, so it can be driven entirely from
    # a fixture — which is what makes every case in the suite a real case.
    local claimlines=""
    while IFS=$'\t' read -r wt br; do
        [ -n "$wt" ] || continue
        claimfile="$wt/.claude/row-claims.txt"
        [ -f "$claimfile" ] || continue
        while IFS= read -r id || [ -n "$id" ]; do
            id="$(printf '%s' "$id" | sed -e 's/#.*//' -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
            [ -n "$id" ] || continue
            claimlines="$claimlines$(printf '%s\t%s' "$wt" "$id")"$'\n'
        done < "$claimfile"
    done <<EOF
$pairs
EOF

    UR_CLAIMS_JSON="$(python3 -c '
import json, os

# ONE ENTRY PER LIVE WORKTREE, carrying the RAW haystack rather than a set of
# pre-chopped tokens. Tokenizing here was the first design and it was wrong in
# a way that failed silently: splitting a branch name on punctuation turns
# "agent/row-11" into "agent", "row", "11", and the claim disappears. The
# matching is a bounded regex and it belongs with the row ids, in the
# predicate, where it can be tested against both near-misses.
# DE-DUPLICATED BY PATH. The scan roots overlap by construction — a linked
# worktree and its main checkout list the same set — and a receipt that names
# one worktree three times is a receipt nobody reads twice.
wts = []
seen_paths = set()
for line in os.fdopen(3).read().split("\n"):
    if not line.strip():
        continue
    parts = line.split("\t")
    wt = parts[0] if parts else ""
    br = parts[1] if len(parts) > 1 else ""
    if not wt or wt in seen_paths:
        continue
    seen_paths.add(wt)
    wts.append({
        "path": wt,
        "where": "worktree %s%s" % (wt, " (branch %s)" % br if br else ""),
        "hay": (br + " " + os.path.basename(wt.rstrip("/"))).lower(),
        "ids": [],
    })

by_path = {w["path"]: w for w in wts}
for line in os.fdopen(4).read().split("\n"):
    if not line.strip():
        continue
    wt, _, rid = line.partition("\t")
    w = by_path.get(wt.strip())
    if w is not None and rid.strip():
        w["ids"].append(rid.strip())

print(json.dumps({"worktrees": wts}))
' 3<<<"$pairs" 4<<<"$claimlines" 2>/dev/null)" || {
        UR_CLAIMS_JSON=""
        UR_CLAIMS_BROKEN="the existing work claims could not be encoded; coverage is unknown."
        return 0
    }
    return 0
}

# ---------------------------------------------------------------------------
# ur_sweep
# ---------------------------------------------------------------------------
# Requires ur_resolve + ur_collect_claims.
#   UR_VERDICT     SWEPT | UNSTARTED | BROKEN
#   UR_N_UNSTARTED UR_N_CLAIMED UR_N_DECLARED UR_N_CLOSED UR_N_OTHER UR_N_ROWS
#   UR_UNSTARTED   space-separated ids
#   UR_ROWS        the full ROW table, one per line
#   UR_BROKEN_REASON when UR_VERDICT is BROKEN
ur_sweep() {
    local job out head
    UR_VERDICT="BROKEN"; UR_BROKEN_REASON=""
    UR_N_UNSTARTED=0; UR_N_CLAIMED=0; UR_N_DECLARED=0
    UR_N_CLOSED=0; UR_N_OTHER=0; UR_N_ROWS=0
    UR_UNSTARTED=""; UR_ROWS=""
    if [ -n "${UR_CLAIMS_BROKEN:-}" ]; then
        UR_BROKEN_REASON="$UR_CLAIMS_BROKEN"
        return 0
    fi

    job="$(UR_QF="$UR_QUEUE_FILE" UR_QL="$UR_QUEUE_LABEL" \
           UR_RF="$UR_RECORD_FILE" UR_RL="$UR_RECORD_LABEL" \
           UR_SEC="$UR_SECTIONS" UR_ST="$UR_STATUS_TOKENS" \
           UR_TT="$UR_TERMINAL_TOKENS" UR_AT="$UR_ACTIONABLE_TOKENS" \
           python3 -c '
import json, os

def read(p):
    try:
        with open(p, encoding="utf-8") as fh:
            return fh.read()
    except Exception:
        return None

try:
    claims = json.loads(os.fdopen(3).read().strip() or "{}")
except Exception:
    claims = {}

print(json.dumps({
    "queue_label":  os.environ["UR_QL"],
    "queue_text":   read(os.environ["UR_QF"]),
    "record_label": os.environ["UR_RL"],
    "record_text":  read(os.environ["UR_RF"]),
    "row_sections": os.environ["UR_SEC"].split(),
    "status_tokens": os.environ["UR_ST"].split(),
    "terminal_tokens": os.environ["UR_TT"].split(),
    "actionable_tokens": os.environ["UR_AT"].split(),
    "claims": claims,
}))' 3<<<"${UR_CLAIMS_JSON:-}" 2>/dev/null)"
    if [ -z "$job" ]; then
        UR_BROKEN_REASON="the sweep job could not be assembled — python3 produced nothing."
        return 0
    fi

    out="$(printf '%s' "$job" | python3 "$_UR_PY" - 2>/dev/null)"
    if [ -z "$out" ]; then
        UR_BROKEN_REASON="the predicate produced no verdict at all. A silent checker and a clean queue are not the same answer and must never look the same."
        return 0
    fi

    head="$(printf '%s\n' "$out" | sed -n '1p')"
    UR_ROWS="$(printf '%s\n' "$out" | tail -n +2)"
    case "$head" in
        BROKEN*)
            UR_VERDICT="BROKEN"
            UR_BROKEN_REASON="$(printf '%s' "$head" | cut -f2-)"
            return 0 ;;
        UNSTARTED*|SWEPT*)
            UR_VERDICT="$(printf '%s' "$head" | cut -f1)"
            UR_N_UNSTARTED="$(printf '%s' "$head" | cut -f2)"
            UR_N_CLAIMED="$(printf '%s' "$head" | cut -f3)"
            UR_N_DECLARED="$(printf '%s' "$head" | cut -f4)"
            UR_N_CLOSED="$(printf '%s' "$head" | cut -f5)"
            UR_N_OTHER="$(printf '%s' "$head" | cut -f6)"
            UR_N_ROWS="$(printf '%s' "$head" | cut -f7)"
            UR_UNSTARTED="$(printf '%s\n' "$UR_ROWS" | awk -F'\t' '$2=="UNSTARTED"{printf "%s ", $3}' | sed 's/ $//')"
            return 0 ;;
        *)
            UR_BROKEN_REASON="the predicate answered in a shape this caller does not recognize: ${head:0:120}"
            return 0 ;;
    esac
}

# ---------------------------------------------------------------------------
# ur_receipt <path>
# ---------------------------------------------------------------------------
# THE POSITIVE PROBE. Written on EVERY path, including the silent ones and the
# broken ones, because the completion criterion this was built against is that
# a silence can be told apart from a check that never ran. Silence plus a
# receipt saying 21 rows were swept and 5 declared a blocker is a fact; silence
# on its own is a hope.
#
# Best effort by design: a receipt that could not be written must never turn
# into a refusal, because then an unwritable state directory would end turns.
ur_receipt() {
    local path="${1:-}" dir
    [ -n "$path" ] || return 0
    dir="$(dirname "$path")"
    mkdir -p "$dir" 2>/dev/null || return 0
    {
        echo "# unstarted-rows sweep receipt — the positive probe."
        echo "# Its presence proves the sweep RAN. Silence with a receipt is a"
        echo "# result; silence without one is a check that never happened."
        echo "when-utc:      $(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || echo unknown)"
        echo "verdict:       ${UR_VERDICT:-UNKNOWN}"
        echo "queue:         ${UR_QUEUE_FILE:-<unresolved>}"
        echo "record:        ${UR_RECORD_FILE:-<unresolved>} sections ${UR_SECTIONS:-?}"
        echo "scan-roots:    ${UR_SCAN_ROOTS:-<none>}"
        echo "rows-swept:    ${UR_N_ROWS:-0}"
        echo "unstarted:     ${UR_N_UNSTARTED:-0}  [${UR_UNSTARTED:-}]"
        echo "claimed:       ${UR_N_CLAIMED:-0}"
        echo "blocker-named: ${UR_N_DECLARED:-0}"
        echo "closed:        ${UR_N_CLOSED:-0}"
        echo "other-token:   ${UR_N_OTHER:-0}"
        [ -n "${UR_BROKEN_REASON:-}" ] && echo "broken:        $UR_BROKEN_REASON"
        [ -n "${UR_STANDDOWN_REASON:-}" ] && echo "stood-down:    $UR_STANDDOWN_REASON"
        echo "worktrees:     ${UR_CLAIMS_JSON:-<none scanned>}"
        echo "--- rows ---"
        printf '%s\n' "${UR_ROWS:-}"
    } > "$path" 2>/dev/null || true
    return 0
}
