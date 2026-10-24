#!/usr/bin/env bash
#
# scripts/lib/seat-jurisdiction.sh — THE JURISDICTION CONTRACT.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# Every guard answers two questions, and until 2026-08-30 it answered them from
# two different places that were never compared:
#
#   "Am I governed?"     -> the SESSION'S SEAT, via resolve_entity_root().
#   "What am I judging?" -> the COMMAND'S OWN REPOSITORY, via the hook payload
#                           (tool_input.file_path, or ct_repo_root of the
#                           payload cwd).
#
# With the engine loaded as a USER-SCOPE plugin those two are never the same
# repository by construction: the seat is wherever the session happens to sit,
# the artifact is wherever the command points. Measured that day
# (docs/audits/seat-vs-inspection-divergence-2026-08-30.md), across the 26
# scripts hooks/hooks.json actually registers: 20 resolve a seat and 7 derived
# their inspection target independently of it.
#
# The damage was not that the guards were wrong. It was that DIVERGENCE AND
# SUCCESS WERE THE SAME BYTE. A guard whose target fell outside its seat ran off
# the end of its loop and exited 0 — identical, to every observer, to a guard
# that had looked and approved. Three consequences, all reproduced:
#
#   * a write to the REAL product source (richos/app/) exited 0;
#   * a write to richos/engine/app/ — A DIRECTORY THAT DOES NOT EXIST — was
#     blocked, because the seat had resolved to the engine subdirectory and
#     PROTECTED_PATHS was joined onto it;
#   * richos-hq, seated `not-adopted`, exited 0 for everything, silently, while
#     taking 28 commits that day.
#
# ===========================================================================
# THE RULE — one sentence, and it holds for every guard
# ===========================================================================
#
#   A guard enforces on an artifact IF AND ONLY IF that artifact lies inside
#   the repository the guard resolved as its seat; an artifact outside the seat
#   is OUT OF JURISDICTION and is ANNOUNCED, never silently allowed.
#
# Note what the rule does NOT say. It does not say "block it". Blocking every
# out-of-jurisdiction artifact would brick every cross-repository operation on
# the machine, and this engine has already learned once that "refuse what you
# cannot resolve" is not the same rule as "never pretend you resolved it"
# (resolve-roots.sh, THE GOVERNING RULE). The rule says the guard must not be
# SILENT about it. Silence is the whole defect; the exit code is not.
#
# ===========================================================================
# WHY THE ANNOUNCEMENT IS DEDUPLICATED, AND WHY THAT IS NOT A MUTE
# ===========================================================================
# An out-of-jurisdiction artifact is normal and frequent — an agent reading
# another repository, a lander running from elsewhere. One line per (hook,
# repository, kind) per session is loud; the same line on every tool call is
# noise, and noise is how a real signal gets filtered out by a human. So the
# notice fires ONCE per triple and is then suppressed for that session.
#
# There is deliberately no configuration key that turns it off. A mute for "this
# artifact is not protected" is the silent skip walking back in through the door
# marked configuration — the exact reasoning engine-status.sh already records
# for the stood-down banner.
#
# STATE LIVES IN TMPDIR, NEVER IN A REPOSITORY. On 2026-08-28
# guard-definition-drift.sh wrote its state log into a repository that had
# nothing to do with the session; that is precisely the class of bug this file
# exists to end, so it must not reproduce it while doing so.
#
# Safe to source repeatedly. Never changes the caller's cwd.

if [ -n "${_SEAT_JURISDICTION_SH_SOURCED:-}" ]; then
    return 0 2>/dev/null || true
fi
_SEAT_JURISDICTION_SH_SOURCED=1

# --- notice state ----------------------------------------------------------
# One directory per user per session-ish lifetime. TMPDIR is cleared by the OS,
# which is the right retention: the guarantee is "once per session", not
# "once ever" — a new session must be told again.
_sj_notice_dir() {
    local base="${TMPDIR:-/tmp}"
    base="${base%/}"
    local uid d
    uid="$(id -u 2>/dev/null || echo 0)"
    d="$base/richos-engine-notices-$uid"
    [ -d "$d" ] || mkdir -p "$d" 2>/dev/null || return 1
    printf '%s' "$d"
}

# _sj_once <key> — rc 0 the FIRST time this key is seen, rc 1 afterwards.
# Fails OPEN (rc 0, i.e. announce) if state cannot be written: an announcement
# repeated is a nuisance, an announcement lost is the bug.
_sj_once() {
    local key="$1" dir stamp
    dir="$(_sj_notice_dir)" || return 0
    # The session id is part of the identity: a new session must be told again.
    stamp="$dir/$(printf '%s' "${CLAUDE_SESSION_ID:-nosession}|$key" | cksum | tr -d ' /')"
    [ -e "$stamp" ] && return 1
    : >"$stamp" 2>/dev/null || return 0
    return 0
}

# ---------------------------------------------------------------------------
# richos_physical <path>
# ---------------------------------------------------------------------------
# A path with every symlink resolved, working from the nearest EXISTING
# ancestor so a not-yet-created leaf still physicalizes.
#
# THIS IS NOT PEDANTRY, IT IS THE BUG. On macOS /var IS A SYMLINK to
# /private/var, so the same directory arrives as two different strings
# depending on who produced it: a hook payload carries the logical form, while
# anything that has been through `cd && pwd` carries the physical one. Compare
# them as strings and a repository is not itself. Caught by case 2f of this
# file's test suite, where a linked worktree of the seat was reported as
# belonging to a different repository — the FALSE POSITIVE that teaches a
# reader to ignore the notice. scripts/lib/publication-boundary.sh carries
# pb_physical for precisely the same reason.
richos_physical() {
    local p="${1:-}" dir rest
    [ -n "$p" ] || return 1
    if [ -d "$p" ]; then
        ( cd "$p" 2>/dev/null && pwd -P ) && return 0
        printf '%s' "$p"; return 0
    fi
    dir="$(dirname "$p")"
    rest="$(basename "$p")"
    while [ -n "$dir" ] && [ "$dir" != "/" ] && [ ! -d "$dir" ]; do
        rest="$(basename "$dir")/$rest"
        dir="$(dirname "$dir")"
    done
    if [ -d "$dir" ]; then
        dir="$( (cd "$dir" 2>/dev/null && pwd -P) || printf '%s' "$dir" )"
        printf '%s/%s' "${dir%/}" "$rest"
        return 0
    fi
    printf '%s' "$p"
}

# ---------------------------------------------------------------------------
# richos_repo_of <path>
# ---------------------------------------------------------------------------
# The repository a path belongs to: its git top-level, normalized to the MAIN
# checkout so a linked worktree answers with the checkout it shares. Prints
# nothing (rc 1) when the path is in no repository at all.
#
# Normalization matters here for the same reason it matters in resolve-roots.sh:
# an agent editing inside .claude/worktrees/<x>/ IS editing the seat's
# repository, and a jurisdiction check that said otherwise would fire on every
# legitimate worktree edit and train people to ignore it.
richos_repo_of() {
    local p="${1:-}" dir top
    [ -n "$p" ] || return 1
    if [ -d "$p" ]; then dir="$p"; else dir="$(dirname "$p")"; fi

    # WALK UP TO THE NEAREST EXISTING ANCESTOR. `git -C <dir>` fails outright on
    # a directory that does not exist, and for a Write that is the ORDINARY
    # case: creating src/new/deep/file.txt means neither `deep` nor `new` is
    # there yet. Without this walk the answer was "in no repository at all",
    # which in a linked worktree of the seat produced a false OUT OF
    # JURISDICTION notice on perfectly legitimate work — and a notice that fires
    # on legitimate work is one the reader learns to ignore, which is how a real
    # signal gets lost. Caught by case 2f of this file's own test suite.
    while [ -n "$dir" ] && [ "$dir" != "/" ] && [ ! -d "$dir" ]; do
        dir="$(dirname "$dir")"
    done
    [ -d "$dir" ] || return 1

    top="$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null || true)"
    [ -n "$top" ] || return 1
    if command -v resolve_main_checkout >/dev/null 2>&1; then
        top="$(resolve_main_checkout "$top" "$top" 2>/dev/null || printf '%s' "$top")"
    fi
    ( cd "$top" 2>/dev/null && pwd -P ) || printf '%s\n' "$top"
}

# ---------------------------------------------------------------------------
# richos_in_jurisdiction <seat> <target>
# ---------------------------------------------------------------------------
# rc 0  the target lies inside the seat (path containment OR same repository)
# rc 1  it does not
# rc 2  undecidable — the target is in no repository and is not under the seat
#
# Path containment is checked FIRST and answers on its own. A target under the
# seat is in jurisdiction even when it is untracked, ignored, or not yet
# created — all three are ordinary for a Write, and asking git about a file that
# does not exist yet would make the answer depend on creation order.
richos_in_jurisdiction() {
    local seat="${1:-}" target="${2:-}"
    [ -n "$seat" ] && [ -n "$target" ] || return 2

    # BOTH SIDES PHYSICALIZED, ONCE, BEFORE ANY COMPARISON. Physicalizing only
    # one side is worse than physicalizing neither: it turns a repository into
    # a stranger to itself, and the resulting false notice is the kind a reader
    # learns to scroll past.
    seat="$(richos_physical "$seat")"
    target="$(richos_physical "$target")"
    seat="${seat%/}"

    case "$target" in
        "$seat"|"$seat"/*) return 0 ;;
    esac

    local trepo
    trepo="$(richos_repo_of "$target" 2>/dev/null || true)"
    [ -n "$trepo" ] || return 2
    [ "${trepo%/}" = "$seat" ] && return 0
    return 1
}

# ---------------------------------------------------------------------------
# richos_assert_jurisdiction <hook> <seat> <target> [what] [disposition]
# ---------------------------------------------------------------------------
# THE CALL EVERY DIVERGING GUARD MAKES. Returns:
#
#   rc 0  in jurisdiction  -> the guard proceeds and enforces
#   rc 1  out of jurisdiction -> ANNOUNCED, so whatever the caller does next is
#         no longer indistinguishable from a silent skip
#
# The announcement is the product. It names the hook, the seat, the artifact and
# the repository the artifact actually belongs to, so the reader can see the two
# repositories that failed to be the same one.
#
# ===========================================================================
# THE FIFTH PARAMETER, AND THE DAY THAT PAID FOR IT
# ===========================================================================
# rc 1 does NOT mean the guard stood down. Ten call sites route through here and
# NINE of them proceed to judge the artifact anyway, by the TARGET repository's
# own declaration — because a guard that switched itself off on a seat mismatch
# would wave through exactly the cross-repository operation it exists to catch,
# which is stated at length in richos_governing_root below. Only
# guard-main-checkout-writes.sh's first call really declines.
#
# The banner said "the guard did NOT judge it / THIS IS NOT A PASS" to all ten.
# For nine of them that is FALSE, and on 2026-09-04 the falsehood cost a day: an
# engineer read it, reported that dialect-guard.sh does not judge files in
# cross-repository worktrees — meaning the American-English rule would have been
# unenforced for most writes on this machine, 48 of 53 worktrees — and it was
# relayed to the CEO as a finding. A second engineer replayed the exact
# configuration and measured EXIT 2 WITH A FULL REFUSAL. The guard worked; the
# message lied.
#
# A DIAGNOSTIC THAT MISREPORTS ITS OWN VERDICT IS WORSE THAN NO DIAGNOSTIC,
# because it manufactures findings, and a manufactured finding costs an engineer
# a day and a lead his credibility.
#
# So the caller says which of the two things happened:
#
#   declines  (default) the guard will NOT judge this artifact. Nothing about
#                       it was checked. This is the wording that was always
#                       right, for the one call site it was written for.
#   proceeds            the guard judged it anyway, out of the target's own
#                       declaration. What is worth saying then is not "nothing
#                       was checked" but "the CONFIGURATION may be the seat's".
#
# THE DEFAULT IS `declines` ON PURPOSE. A caller that forgets the parameter then
# under-claims — it says nothing was checked when something was — and an
# under-claim prompts a check, while an over-claim ("this was judged") is a
# false pass and is how a defense becomes a rumour. Case 4a of
# seat-jurisdiction.test.sh refuses any call site that leaves it implicit, so
# forgetting is caught rather than defaulted around.
richos_assert_jurisdiction() {
    local hook="${1:-<unknown hook>}" seat="${2:-}" target="${3:-}" what="${4:-artifact}"
    local disposition="${5:-declines}"
    local rc=0
    richos_in_jurisdiction "$seat" "$target" || rc=$?
    [ "$rc" = 0 ] && return 0

    local trepo reason
    trepo="$(richos_repo_of "$target" 2>/dev/null || true)"
    if [ "$rc" = 2 ]; then
        reason="it is in no git repository at all"
        trepo="<none>"
    else
        reason="it belongs to a different repository"
    fi

    local headline
    if [ "$disposition" = "proceeds" ]; then
        headline="OUT OF JURISDICTION — JUDGED ANYWAY"
    else
        headline="OUT OF JURISDICTION — NOT ENFORCED"
    fi

    if _sj_once "jurisdiction|$hook|${trepo:-none}|${seat:-none}"; then
        {
            echo "=== RichOS engine: $headline ==="
            printf '  %-9s: %s\n' "hook" "$hook"
            printf '  %-9s: %s   (the repository this session governs)\n' "seat" "${seat:-<unresolved>}"
            printf '  %-9s: %s\n' "$what" "$target"
            printf '  %-9s: %s\n' "its repo" "${trepo:-<none>}"
            echo "  This guard is seated in one repository and was handed an artifact in"
            if [ "$disposition" = "proceeds" ]; then
                echo "  another, so $reason — and this guard JUDGED IT ANYWAY."
                echo "  IT DID LOOK. It reads its contract out of the artifact's own"
                echo "  repository, and standing down on a seat mismatch would wave through"
                echo "  the cross-repository operation this mechanism exists to catch. So"
                echo "  whatever verdict it reached is a real verdict."
                echo "  WHAT TO CHECK IS THE CONFIGURATION: anything it took from the SEAT"
                echo "  rather than from the repository above — thresholds, allowlists,"
                echo "  exempt paths — may not be that repository's."
            else
                echo "  another, so $reason and the guard did NOT judge it."
                echo "  THIS IS NOT A PASS. Nothing about that artifact was checked."
                echo "  To have it enforced, work from a session seated in its repository, or"
                echo "  adopt the engine there (commit an orchestration.config at its root)."
            fi
            echo "  (said once per hook+repository per session)"
            echo "=========================================================="
        } >&2
    fi
    return 1
}

# ---------------------------------------------------------------------------
# richos_governing_root <target> [seat]
# ---------------------------------------------------------------------------
# WHICH REPOSITORY'S RULES APPLY TO THIS ARTIFACT. Prints it, or nothing (rc 1)
# when no repository governs it.
#
# THIS IS THE ACTUAL RESOLUTION FIX, and it is worth being precise about why the
# obvious version is wrong.
#
# The obvious version is "if the artifact is not inside my seat, skip it". That
# closes the divergence and it is a REGRESSION, which the test suites caught
# within minutes: guard-row-currency-commits.sh and its four siblings read their
# contract out of the TARGET repository — `.row-currency`, `.ceo-todos`,
# `.publication-boundary` — and the seat contributes nothing to what they judge.
# Skipping on a seat mismatch would have switched OFF a guard that was working:
# a merge into richos-hq, refused that morning from an adopted seat, would have
# sailed through. A jurisdiction rule is never allowed to move enforcement in
# the less safe direction.
#
# So the two questions are not reconciled by adding a comparison. They are
# reconciled by ASKING BOTH OF THE SAME REPOSITORY:
#
#   the artifact's own repository governs it, if it declares anything;
#   otherwise the seat governs it, but only if the artifact is inside the seat;
#   otherwise nothing governs it, and that is said out loud.
#
# Under that resolution "am I governed?" and "what am I inspecting?" cannot
# disagree, because they are the same question about the same repository rather
# than two questions about two.
#
# The marker is the same one adoption uses everywhere else — nothing new to
# maintain, and no second definition of "governed" to drift.
richos_governing_root() {
    local target="${1:-}" seat="${2:-}" repo
    [ -n "$target" ] || return 1

    repo="$(richos_repo_of "$target" 2>/dev/null || true)"
    if [ -n "$repo" ] && [ -f "$repo/${RICHOS_ADOPTION_MARKER:-orchestration.config}" ]; then
        printf '%s' "$repo"
        return 0
    fi

    # PHYSICALIZED, exactly as the repo arm above is. richos_repo_of ends in
    # `cd && pwd -P`, so the first arm always answers in physical spelling; the
    # seat arrives from the payload in whatever spelling the caller was handed,
    # which on macOS is routinely the /var -> /private/var symlink. Returning it
    # raw made this function answer the SAME question in TWO spellings depending
    # on which arm fired, and every caller compares the answer against a
    # physicalized seat — so the seat arm reported "a DIFFERENT repository
    # governs this file" about the seat itself. In guard-main-checkout-writes.sh
    # that sent it down the foreign-repository branch and then matched
    # PROTECTED_PATHS against a path with the other spelling: the guard ran, took
    # every branch, printed nothing, and blocked nothing. A defense that is
    # inert while reporting healthy is the failure mode this whole file exists to
    # prevent, so the normalization belongs HERE, at the single point of answer,
    # not in each caller. Positive control: case 2h of seat-jurisdiction.test.sh.
    if [ -n "$seat" ] && richos_in_jurisdiction "$seat" "$target"; then
        richos_physical "$seat"
        return 0
    fi

    return 1
}

# ---------------------------------------------------------------------------
# richos_announce_stand_down <hook> [reason]
# ---------------------------------------------------------------------------
# The LOUD stand-down. Called on the `not-adopted` arm, where every guard used
# to `exit 0` with no output whatsoever.
#
# engine-status.sh already announces the stand-down once at SessionStart. That
# is necessary and it is not sufficient: it fires before any work happens, it
# names no specific action, and a session that scrolls past it has no second
# chance. This fires at the MOMENT OF THE DECISION and names the guard that just
# declined — which is the only moment at which the absence actually costs
# something.
richos_announce_stand_down() {
    local hook="${1:-<unknown hook>}" reason="${2:-}"
    local where="${CLAUDE_PROJECT_DIR:-$PWD}"
    local repo
    repo="$(richos_repo_of "$where" 2>/dev/null || true)"
    [ -n "$repo" ] || repo="$where"

    _sj_once "standdown|$hook|$repo" || return 0
    {
        echo "=== RichOS engine: STOOD DOWN — $hook DID NOT RUN ==="
        echo "  repository : $repo"
        echo "  reason     : ${reason:-no orchestration.config at any candidate root, so this repository has not adopted the engine}"
        echo "  This guard declined to enforce. That is a STAND-DOWN, not a pass —"
        echo "  nothing it checks for was checked in this repository."
        echo "  Adopt by committing an orchestration.config at the repository root."
        echo "  (said once per hook per repository per session)"
        echo "===================================================="
    } >&2
    return 0
}
