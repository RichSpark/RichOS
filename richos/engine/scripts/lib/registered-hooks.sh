#!/usr/bin/env bash
#
# scripts/lib/registered-hooks.sh — THE GUARD INVENTORY, DERIVED FROM THE
#                                    REGISTRATION SURFACE.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# "How many guards are there?" was answered, in two different places, by a list
# a human had typed. It drifted twice in two days:
#
#   13/13  guard-workflow-ban.sh added, list not updated. Observed live in a
#          real ~/ab/prospects session. Fixed at d55f54b by making the banner's
#          numerator and denominator walk the SAME typed list — which removed
#          the arithmetic error and left the typing in place.
#   14/14  79d6958 / 084eed3 wired guard-worktree-removal.sh. The typed list was
#          not touched. Within hours of the previous fix, femcboost sessions at
#          engine 1.0.0 announced "14/14 guards" — a full, reassuring fraction
#          over a stale inventory — with 15 guards plus the announcer loaded.
#
# Both times the guard was present, executable, hash-matched and firing. The
# defect was never in the guard; it was that the count of guards was a SECOND,
# INDEPENDENT record of the same fact, maintained by memory. A list a human must
# remember to update is not a mitigation for drift, it is a source of it.
#
# So the count is no longer recorded anywhere. It is DERIVED, here, from
# hooks/hooks.json — the file that actually determines what the host loads.
# Nothing else can be the source: a script present on disk but never wired
# enforces nothing (and must not inflate a count), and a script wired but absent
# from disk enforces nothing either (and must show as a SHORTFALL, which is the
# whole signal the banner's fraction carries). Those are two different questions
# and this library answers only the first one — "what does the host load?".
# Callers answer the second themselves, against the disk.
#
# ===========================================================================
# WHY A SHARED LIBRARY RATHER THAN A SECOND PARSER
# ===========================================================================
# engine-status.sh needs this inventory to size its fraction, and
# contract-integrity-probe.sh needs it to check that the banner and the probe
# are looking at the same set. Two hand-rolled parsers of the same file is how
# the original defect was born, one level down. There is one parser, both use
# it, and the probe (BR2) additionally compares this library's answer against
# its OWN independent python3 parse of hooks.json — so a bug in here cannot
# quietly shrink the inventory for both readers at once.
#
# ===========================================================================
# NO SILENT DEGRADATION
# ===========================================================================
# The engine's governing rule is fail LOUD, never skip. A parser that returned
# "0 guards" or a partial list when it could not read the registration surface
# would hand its caller a number that looks like an answer, which is the exact
# failure this file exists to remove. So the contract is: either a complete
# inventory and rc 0, or NOTHING on stdout and a non-zero rc the caller must
# report rather than paper over.

# registered_hook_scripts <path-to-hooks.json> [event]
#
# Prints one hook-script BASENAME per line ("guard-bash-main-writes.sh"),
# sorted and de-duplicated — every script the given hook table registers, on
# any event, under any matcher. Inline hooks that run no script (the knowledge-
# verification echo) contribute nothing, correctly: they are not scripts and
# cannot be present-or-missing on disk.
#
# With [event] ("Stop", "PreToolUse", …) the answer is narrowed to the scripts
# that table registers ON THAT EVENT. The narrowed form exists so that "which
# hooks run at turn-end?" is DERIVED like every other inventory here rather than
# typed: stop-hook-visibility.test.sh asks this question, and a typed answer of
# 14 where the registration held 15 is the exact defect the whole file was
# written about. A Stop hook added tomorrow is in the answer with no edit here
# and no edit there.
#
# The event filter REQUIRES python3 and says so with rc 3 rather than degrading.
# The text-scan fallback below cannot see event boundaries at all, so filtering
# through it would silently return the WHOLE inventory under an event's name —
# an over-count wearing a precise label, which is worse than the honest refusal.
#
# Exit codes — a caller MUST distinguish these from an empty inventory:
#   0  complete inventory printed
#   1  no such file (the path is wrong, or the engine install is incomplete)
#   2  present but unparseable, or parseable and registering no script at all
#      (with [event]: no script registered on that event)
#   3  [event] was requested and python3 is unavailable — cannot filter
# ===========================================================================
# THE DISPATCHER IS EXPANDED INTO THE RULES IT RUNS
# ===========================================================================
# scripts/hooks/dispatch-pretooluse.sh is ONE registered command that runs N
# rule modules in one process, named in scripts/hooks/dispatch-pretooluse.manifest.
# Seventeen guards moved out of hooks/hooks.json and into that manifest on
# 2026-09-15.
#
# WHY THE INVENTORY MUST STILL NAME THEM. Every consumer of this library is
# asking a question about ENFORCEMENT, not about process count: is this guard
# present on disk, executable, hash-matched, carrying the root bootstrap,
# acknowledged by the suite that watches the registration for drift. A guard
# that runs as a dispatcher module is enforcing exactly as much as one that runs
# as its own process — and if the inventory stopped naming it, deleting its line
# from the manifest would turn enforcement off with NOTHING going red. That is
# the ratchet running backwards: the manifest would be a second, unwatched
# registration surface, which is the defect this whole library was written about
# (a typed list drifting twice in two days, 13/13 and 14/14 over stale
# inventories).
#
# So the answer to "what does the host load?" is: the dispatcher, AND every rule
# the manifest gives it. One derivation, in one place, and every consumer keeps
# the question it was already asking.
#
# _rh_dispatch_modules <hooks.json> [event]
#   Prints the rule-module basenames the dispatcher is registered to run, on the
#   given event or on any event. Prints NOTHING (rc 0) when the dispatcher is not
#   registered or the manifest is absent — the caller's own list is then the
#   whole answer, which is the pre-dispatcher behavior exactly.
_RH_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# _rh_manifest_for <hooks.json> — the dispatcher manifest that governs this
# surface, or nothing. Same candidate order as the python path uses, for the
# same reason: the derived path is right for the shipped layout and wrong for
# every sandbox, and this library's own directory is the answer for a hook
# table that names this engine's dispatcher without carrying a tree of its own.
_rh_manifest_for() {
    local f="$1" hj_dir c
    hj_dir="$(cd "$(dirname "$f")" 2>/dev/null && pwd)" || return 1
    for c in "$(dirname "$hj_dir")/scripts/hooks/dispatch-pretooluse.manifest" \
             "$hj_dir/scripts/hooks/dispatch-pretooluse.manifest" \
             "$_RH_LIB_DIR/../hooks/dispatch-pretooluse.manifest"; do
        if [ -f "$c" ]; then printf '%s' "$c"; return 0; fi
    done
    return 1
}

# The python3-free path of _rh_dispatch_modules. See the comment at its call
# site for why "return nothing" was not an option.
_rh_dispatch_modules_textscan() {
    local f="$1" mf key
    mf="$(_rh_manifest_for "$f")" || {
        # No manifest is "the dispatcher is not in play" ONLY when this surface
        # does not register the dispatcher. One that does, with no manifest to
        # expand it, names rules this answer would silently lack (hunt P5-08).
        grep -q 'dispatch-pretooluse\.sh[[:space:]][A-Za-z]' "$f" 2>/dev/null && return 2
        return 0
    }
    for key in $(grep -o 'dispatch-pretooluse\.sh[[:space:]][A-Za-z][A-Za-z]*' "$f" 2>/dev/null \
                 | awk '{print $2}' | LC_ALL=C sort -u); do
        awk -F'|' -v k="$key" '
            /^[[:space:]]*#/ {next}
            NF>1 && $1==k {gsub(/^[[:space:]]+|[[:space:]]+$/,"",$2); if ($2 != "") print $2}
        ' "$mf"
    done | LC_ALL=C sort -u
}

_rh_dispatch_modules() {
    local f="${1:-}" event="${2:-}"
    [ -n "$f" ] && [ -f "$f" ] || return 0
    if ! command -v python3 >/dev/null 2>&1; then
        # NO SILENT UNDER-COUNT WITHOUT python3. Returning nothing here would
        # hand engine-status.sh a full, reassuring "54/54 guards" over an
        # inventory missing seventeen of them — which is the 14/14 defect this
        # whole file was written about, rebuilt. Caught by engine-status.test.sh
        # cases 6a and 6b, which run the banner with python3 removed from PATH.
        #
        # The text scan is a weaker parser, exactly as the one in
        # registered_hook_scripts is, and weaker in the same direction: it would
        # also match a chain key mentioned in a description string, which
        # OVER-counts rather than under-counts. It cannot filter by event, so a
        # narrowed request gets the whole set — the narrowed form already
        # REFUSES without python3 (rc 3) rather than degrade, so that path
        # never reaches here.
        _rh_dispatch_modules_textscan "$f"
        return $?
    fi
    local _dm_out _dm_rc=0
    _dm_out="$(PYTHONPATH="$_RH_LIB_DIR${PYTHONPATH:+:$PYTHONPATH}" python3 -c '
import json, os, re, sys
from hook_command import dispatch_keys
f, want, libdir = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    with open(f, encoding="utf-8") as fh:
        doc = json.load(fh)
except Exception:
    raise SystemExit(0)

# WHERE THE MANIFEST IS, and why this is a LIST rather than one path.
# The obvious answer — <engine>/hooks/hooks.json, so <engine> is two levels up —
# is right for the shipped layout and wrong for every sandbox. hook-staleness.
# test.sh builds a bare hooks.json in a scratch directory and asks this library
# what it registers; with one derived path the answer came back missing every
# dispatcher rule, and case 6 went red reporting scan-secrets.sh as a guard that
# had just landed. Caught by that suite, which is what it is for.
# Last candidate is THIS LIBRARY own directory: scripts/lib/../hooks is the
# shipped manifest, which is the right answer for any hook table that names this
# engine own dispatcher and does not carry a tree of its own.
_cands = [
    os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(f))),
                 "scripts", "hooks", "dispatch-pretooluse.manifest"),
    os.path.join(os.path.dirname(os.path.abspath(f)),
                 "scripts", "hooks", "dispatch-pretooluse.manifest"),
    os.path.join(libdir, "..", "hooks", "dispatch-pretooluse.manifest"),
]
manifest = ""
for _c in _cands:
    if os.path.isfile(_c):
        manifest = _c
        break

keys = set()
hooks = doc.get("hooks", {})
if isinstance(hooks, dict):
    for event, entries in hooks.items():
        if want and event != want:
            continue
        if not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            for h in entry.get("hooks", []) or []:
                if not isinstance(h, dict):
                    continue
                cmd = h.get("command", "")
                if not isinstance(cmd, str):
                    continue
                for k in dispatch_keys(cmd):
                    keys.add(k)
if not keys:
    raise SystemExit(0)
# THE DISPATCHER IS REGISTERED HERE, SO THE MANIFEST IS NOT OPTIONAL (hunt P5-08).
# "No manifest, nothing to add" is the pre-dispatcher behavior for a surface that
# does not use the dispatcher, and that is the `not keys` exit above. With a
# dispatcher key in play, a manifest that is missing or cannot be read leaves the
# inventory short of the rules that key runs, and it used to say so with exit 0
# and no output. Exit 3 = "cannot expand the dispatcher"; the shell maps it to 2.
if not manifest:
    raise SystemExit(3)
out = set()
try:
    with open(manifest, encoding="utf-8") as mh:
        for line in mh:
            line = line.strip()
            if not line or line.startswith("#") or "|" not in line:
                continue
            k, mod = line.split("|", 1)
            if k.strip() in keys:
                out.add(mod.strip())
except OSError:
    raise SystemExit(3)
for name in sorted(out):
    print(name)
' "$f" "$event" "$_RH_LIB_DIR" 2>/dev/null)" || _dm_rc=$?
    [ -z "$_dm_out" ] || printf '%s\n' "$_dm_out"
    [ "$_dm_rc" -eq 0 ] || return 2
    return 0
}

# hook_enforced_on_surface <surface-json> <script-basename>
#
# "Does this surface cause that guard to run?" — rc 0 yes, rc 1 no.
#
# THE QUESTION IS ENFORCEMENT, NOT SPELLING. Eight guard suites used to answer it
# with `grep -q <name> hooks.json`, which was exact while every guard was its own
# registration and became wrong the moment seventeen of them moved behind
# dispatch-pretooluse.sh. Each of those eight would have reported a guard that
# runs on every single tool call as NOT REGISTERED — eight red suites over
# working enforcement, which is the false alarm that teaches a reader to stop
# believing the check.
#
# Works on EITHER surface: hooks/hooks.json and .claude/settings.local.json have
# the same shape, and since step 2 the second is generated from the first.
hook_enforced_on_surface() { # <surface json> <script basename>
    local f="${1:-}" g="${2:-}"
    [ -n "$f" ] && [ -f "$f" ] && [ -n "$g" ] || return 1
    # A REGISTERED COMMAND, NOT A MENTION (P5-45). The old first line was
    # `grep -q scripts/hooks/<g> <whole file>`, which credited a guard named only
    # in a description string, a disabled entry or any other text: enforcement
    # asserted over a guard that never runs. With python3 the commands of the
    # parsed table are searched; only without it does the text scan remain, as
    # the weaker over-counting fallback this file documents everywhere else.
    if command -v python3 >/dev/null 2>&1; then
        if PYTHONPATH="$_RH_LIB_DIR${PYTHONPATH:+:$PYTHONPATH}" python3 -c '
import json, re, sys
from hook_command import executed_hooks, dispatch_keys
try:
    with open(sys.argv[1], encoding="utf-8") as fh:
        doc = json.load(fh)
except Exception:
    raise SystemExit(1)
hooks = doc.get("hooks", {})
if isinstance(hooks, dict):
    for entries in hooks.values():
        if not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            for h in entry.get("hooks", []) or []:
                cmd = h.get("command", "") if isinstance(h, dict) else ""
                if isinstance(cmd, str) and sys.argv[2] in executed_hooks(cmd):
                    raise SystemExit(0)
raise SystemExit(1)
' "$f" "$g" 2>/dev/null; then
            return 0
        fi
    else
        grep -q "scripts/hooks/$g" "$f" 2>/dev/null && return 0
    fi
    # The whole expansion is read BEFORE it is searched. It used to be piped into
    # `grep -qxF`, which exits at its first match; a writer with output still to
    # send then died of SIGPIPE (141), and under a caller's pipefail an enforced
    # dispatcher rule read as NOT enforced (dialect-guard.test.sh E2, about one run
    # in thirty on a loaded host; registered-hooks-enforced.test.sh case 10). The
    # expansion's own rc is not the answer here, as it never was: whatever it
    # printed is searched, and nothing printed is "no".
    local _rh_mods
    _rh_mods="$(_rh_dispatch_modules "$f")" || true
    grep -qxF "$g" <<<"$_rh_mods" 2>/dev/null
}

registered_hook_scripts() {
    local f="${1:-}"
    local event="${2:-}"
    local out=""
    local rc=0
    local _rh_dm=""

    [ -n "$f" ] && [ -f "$f" ] || return 1

    if [ -n "$event" ]; then
        command -v python3 >/dev/null 2>&1 || return 3
        out="$(PYTHONPATH="$_RH_LIB_DIR${PYTHONPATH:+:$PYTHONPATH}" python3 -c '
import json, re, sys
from hook_command import executed_hooks, dispatch_keys
with open(sys.argv[1], encoding="utf-8") as fh:
    doc = json.load(fh)
want = sys.argv[2]
found = set()
hooks = doc.get("hooks", {})
if isinstance(hooks, dict):
    for name, entries in hooks.items():
        if name != want or not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            for h in entry.get("hooks", []) or []:
                if not isinstance(h, dict):
                    continue
                cmd = h.get("command", "")
                if not isinstance(cmd, str):
                    continue
                for m in executed_hooks(cmd):
                    found.add(m)
for name in sorted(found):
    print(name)
' "$f" "$event" 2>/dev/null)"
        rc=$?
        [ "$rc" -eq 0 ] || return 2
        [ -n "$out" ] || return 2
        # A dispatcher that cannot be expanded makes the inventory SHORT, not
        # complete: rc 2 with the other "cannot answer" cases (hunt P5-08).
        _rh_dm="$(_rh_dispatch_modules "$f" "$event")" || return 2
        out="$(printf '%s\n%s\n' "$out" "$_rh_dm" \
               | grep -v '^$' | LC_ALL=C sort -u)"
        printf '%s\n' "$out"
        return 0
    fi

    if command -v python3 >/dev/null 2>&1; then
        # The authoritative path: a real JSON parse. If the file is present but
        # malformed, python exits non-zero and we STOP — we do not fall through
        # to the text scan. Malformed JSON means the host loads no hooks from
        # this file either, so a count scraped out of the wreckage would be a
        # number describing enforcement that is not there.
        out="$(PYTHONPATH="$_RH_LIB_DIR${PYTHONPATH:+:$PYTHONPATH}" python3 -c '
import json, re, sys
from hook_command import executed_hooks, dispatch_keys
with open(sys.argv[1], encoding="utf-8") as fh:
    doc = json.load(fh)
found = set()
hooks = doc.get("hooks", {})
if isinstance(hooks, dict):
    for entries in hooks.values():
        if not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            for h in entry.get("hooks", []) or []:
                if not isinstance(h, dict):
                    continue
                cmd = h.get("command", "")
                if not isinstance(cmd, str):
                    continue
                for m in executed_hooks(cmd):
                    found.add(m)
for name in sorted(found):
    print(name)
' "$f" 2>/dev/null)"
        rc=$?
        [ "$rc" -eq 0 ] || return 2
    else
        # No python3. engine-status.sh already carries a python3-absent path for
        # its JSON output, so "no interpreter" is a supported environment and
        # this cannot be left to fail open. A text scan of the same file is a
        # weaker parser — it would also match a path mentioned in a description
        # string — but it is weaker in the direction of OVER-counting, never
        # under-counting, and the probe's BR2 cross-check catches a divergence
        # between this answer and its own parse.
        out="$(grep -o 'scripts/hooks/[A-Za-z0-9._+-]*\.sh' "$f" 2>/dev/null \
               | sed 's|.*/||' | LC_ALL=C sort -u)"
    fi

    [ -n "$out" ] || return 2
    # The dispatcher's rules are part of the answer — see the header above.
    # With no python3 the expansion returns nothing and the dispatcher's rules
    # are missing from the inventory. That is an UNDER-count, which the text
    # scan's own comment above says it must never produce; but the alternative
    # is a text scan of the manifest that cannot tell which chain keys are
    # actually registered, i.e. a guessed association. The honest form is the
    # one that costs a check, not the one that invents a row, and BR2's
    # independent parse cross-checks this answer either way.
    _rh_dm="$(_rh_dispatch_modules "$f")" || return 2
    out="$(printf '%s\n%s\n' "$out" "$_rh_dm" \
           | grep -v '^$' | LC_ALL=C sort -u)"
    printf '%s\n' "$out"
}

# registered_hook_rows <path-to-hooks.json>
#
# The same registration surface, read at one more level of detail: prints one
# TAB-separated row per registered hook script,
#
#     <event>\t<matcher>\t<script-basename>
#
# sorted and de-duplicated, with an absent or empty matcher normalized to "-".
# Inline hooks that run no script contribute nothing, exactly as above.
#
# WHY A SECOND FUNCTION RATHER THAN A RICHER FIRST ONE
# ----------------------------------------------------
# registered_hook_scripts answers "which guards does the host load?", and its
# consumers — the session banner's fraction, install.sh's sidecar minting — want
# a flat set of filenames. The staleness pair (snapshot-enforcing-hooks.sh /
# notice-hook-staleness.sh) has to know WHERE each script is wired, because "the
# same guard newly wired onto a second event" is drift that a set of basenames
# cannot see. Widening the existing function would have meant editing three call
# sites to throw two thirds of the answer away. Both functions parse the same
# file, the same way, in the same place — which is the entire point of this
# file: one parser, not four.
#
# NO TEXT-SCAN FALLBACK, DELIBERATELY. registered_hook_scripts degrades to a
# grep when python3 is absent, and can afford to: over-counting a filename is
# survivable, and BR2 cross-checks it. This function cannot. A text scan cannot
# tell which event or matcher a command sits under — it would have to GUESS the
# association, and a guessed row is precisely the invented fact that would turn
# a staleness comparison into a false positive. So with no interpreter this
# returns 2, and its callers say out loud that they could not make the
# comparison rather than making a bad one.
#
# Exit codes — identical contract to registered_hook_scripts:
#   0  complete inventory printed
#   1  no such file
#   2  present but unparseable, registering no script at all, or no python3
registered_hook_rows() {
    local f="${1:-}"
    local out=""
    local rc=0

    [ -n "$f" ] && [ -f "$f" ] || return 1
    command -v python3 >/dev/null 2>&1 || return 2

    out="$(PYTHONPATH="$_RH_LIB_DIR${PYTHONPATH:+:$PYTHONPATH}" python3 -c '
import json, os, re, sys
from hook_command import executed_hooks, dispatch_keys
with open(sys.argv[1], encoding="utf-8") as fh:
    doc = json.load(fh)

# The dispatcher expands into its rules, each inheriting the dispatcher own
# event and matcher — which is exactly where each rule was registered before
# 2026-09-15. See the header above for why the inventory must still name them,
# and _rh_dispatch_modules for why the manifest is LOOKED FOR rather than
# derived from one path.
_f = os.path.abspath(sys.argv[1])
_cands = [
    os.path.join(os.path.dirname(os.path.dirname(_f)), "scripts", "hooks",
                 "dispatch-pretooluse.manifest"),
    os.path.join(os.path.dirname(_f), "scripts", "hooks",
                 "dispatch-pretooluse.manifest"),
    os.path.join(sys.argv[2], "..", "hooks", "dispatch-pretooluse.manifest"),
]
manifest = {}
manifest_ok = True
found_manifest = False
for _c in _cands:
    if not os.path.isfile(_c):
        continue
    found_manifest = True
    try:
        with open(_c, encoding="utf-8") as mh:
            for line in mh:
                line = line.strip()
                if not line or line.startswith("#") or "|" not in line:
                    continue
                k, mod = line.split("|", 1)
                manifest.setdefault(k.strip(), []).append(mod.strip())
    except OSError:
        manifest = {}
        manifest_ok = False      # present but unreadable: NOT the same as absent
    break

rows = set()
used_dispatcher = False
hooks = doc.get("hooks", {})
if isinstance(hooks, dict):
    for event, entries in hooks.items():
        if not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            matcher = entry.get("matcher", "")
            if not isinstance(matcher, str) or matcher == "":
                matcher = "-"
            for h in entry.get("hooks", []) or []:
                if not isinstance(h, dict):
                    continue
                cmd = h.get("command", "")
                if not isinstance(cmd, str):
                    continue
                for m in executed_hooks(cmd):
                    rows.add("%s\t%s\t%s" % (event, matcher, m))
                for dk in dispatch_keys(cmd):
                    used_dispatcher = True
                    for mod in manifest.get(dk, []):
                        rows.add("%s\t%s\t%s" % (event, matcher, mod))
# A DISPATCHER REGISTERED WITH NO READABLE MANIFEST LEAVES THE ROWS SHORT (hunt
# P5-08); it used to print them as if whole. Exit 3, which the shell maps to rc 2.
# A surface that never names the dispatcher needs no manifest, as before.
if used_dispatcher and not (found_manifest and manifest_ok):
    raise SystemExit(3)
for row in sorted(rows):
    print(row)
' "$f" "$_RH_LIB_DIR" 2>/dev/null)"
    rc=$?
    [ "$rc" -eq 0 ] || return 2

    [ -n "$out" ] || return 2
    printf '%s\n' "$out"
}

# ===========================================================================
# THE PreToolUse[Agent] CHAIN — ORDERED, AND ITS ORDER INTENT
# ===========================================================================
# Everything above answers "which scripts does the host load?" as a SET. The
# Agent chain is the one place where the ANSWER IS A SEQUENCE: every registered
# gate runs on every spawn, in the order the table lists them, and the FIRST
# refusal is the one the operator reads.
#
# WHY THESE TWO FUNCTIONS EXIST, which is the whole lesson of 2026-09-14.
# guard-brief-scope.sh was registered as the ninth entry in hooks/hooks.json.
# Two consumers inside contract-integrity-probe.sh held their own TYPED copy of
# the chain — CANONICAL_AGENT_CHAIN for Layer C, BR_AGENT_WANT for BR2 — and
# both still said eight. Nothing was broken from the author's seat: the hook
# loaded, fired, and did its job. The engine went red on `main` because a list
# somebody typed weeks earlier disagreed with the registration, and the remedy
# those consumers PRINTED — "run scripts/hooks/install.sh" — could not fix it:
# install.sh validates two config keys, migrates a stale settings.json and mints
# sha256 sidecars, and never writes a hook stanza or touches either list.
#
# A list that must be edited when a hook is added is not a mitigation for drift.
# It IS the drift. So the MEMBERSHIP and the ORDER of the chain are derived here
# from the registration, and what stays typed is the only thing the registration
# cannot state about itself: WHY that order and not another.
#
# THE SPLIT IS THE DESIGN. Do not collapse it:
#
#   MEMBERSHIP + ORDER  are FACTS about the registration. Derived. A tenth hook
#                       registered tomorrow appears in both with no edit here.
#
#   THE ORDER INTENT    is a SPEC the registration cannot check against itself.
#                       Typed. Deriving it too would compare hooks.json against
#                       hooks.json and could never fail — failure type Z, a
#                       check that passes for a reason unrelated to what it
#                       checks, which is the type this engine keeps retiring.
#
# THE INTENT IS A PREFIX RULE, NOT A SEQUENCE, and that is deliberate. The four
# STRUCTURAL gates settle whether the SPAWN IS WELL FORMED — is it isolated and
# truthfully named, is the booted definition the one on disk, is a reading task
# routed to the reader, does the prompt carry what it must. They come first, in
# that order, because a dispatch that is malformed AND something else should
# hear about the malformed half first: that is the half the operator can fix
# without leaving the keyboard.
#
# Everything after them is a POLICY question — cost, environment, standing
# state, scope, whether the CEO was asked — and the probe's own comments already
# concede that their relative order is not load-bearing ("Against the CEO-ask
# gate the order is not load-bearing (both are policy questions), so the tie was
# broken by the merge-safe choice: appending"). Four such hooks have been
# appended since, and the comment on each one claims it is LAST. So the tail is
# left UNCONSTRAINED on purpose. A tenth policy hook appended tomorrow satisfies
# this rule untouched — the property that was missing — while a hook inserted
# AHEAD of the structural four still fails as loudly as it must.
AGENT_CHAIN_STRUCTURAL_PREFIX="guard-worktree-isolation.sh guard-definition-drift.sh verify-agent-prompt.sh"

# registered_agent_chain <path-to-hooks.json>
#
# Prints one hook-script BASENAME per line, IN REGISTRATION ORDER — every script
# the table runs on PreToolUse under a matcher that names Agent. Deliberately
# NOT sorted and NOT de-duplicated: a duplicate registration is a real defect
# (the hook fires twice on every spawn, and Layer M exists for it), so this
# function's job is to show it, not to tidy it away.
#
# NO TEXT-SCAN FALLBACK, for registered_hook_rows' reason and one more: a grep
# cannot see which event or matcher a command sits under, and it cannot see
# order at all. A guessed sequence is worse than a refusal here, because its
# only consumer is a check on that exact sequence.
#
# Exit codes — a caller MUST distinguish these from an empty chain:
#   0  complete chain printed
#   1  no such file (wrong path, or an incomplete engine install)
#   2  present but unparseable, registering nothing on PreToolUse[Agent], or
#      no python3 to read it with
registered_agent_chain() {
    local f="${1:-}"
    local out=""
    local rc=0

    [ -n "$f" ] && [ -f "$f" ] || return 1
    command -v python3 >/dev/null 2>&1 || return 2

    out="$(PYTHONPATH="$_RH_LIB_DIR${PYTHONPATH:+:$PYTHONPATH}" python3 -c '
import json, re, sys
from hook_command import executed_hooks, dispatch_keys
with open(sys.argv[1], encoding="utf-8") as fh:
    doc = json.load(fh)
chain = []
entries = (doc.get("hooks") or {}).get("PreToolUse")
if isinstance(entries, list):
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        matcher = entry.get("matcher", "")
        if not isinstance(matcher, str) or "Agent" not in matcher:
            continue
        for h in entry.get("hooks", []) or []:
            if not isinstance(h, dict):
                continue
            cmd = h.get("command", "")
            if not isinstance(cmd, str):
                continue
            for m in executed_hooks(cmd):
                chain.append(m)
for name in chain:
    print(name)
' "$f" 2>/dev/null)"
    rc=$?
    [ "$rc" -eq 0 ] || return 2

    [ -n "$out" ] || return 2
    printf '%s\n' "$out"
}

# agent_chain_order_violations <name> [<name> ...]
#
# Judges an ORDERED Agent chain against AGENT_CHAIN_STRUCTURAL_PREFIX above.
# Prints one human-readable violation per line and returns 1 if there are any;
# prints nothing and returns 0 when the order is sound.
#
# It says nothing whatever about the tail. That is not an omission — see the
# intent above: the tail's order is explicitly not load-bearing, and a rule that
# constrained it would have to be edited by every future appender, which is the
# defect this whole section exists to end.
agent_chain_order_violations() {
    local -a chain=("$@")
    local -a want=()
    local i=0 bad=0 seen=""
    # shellcheck disable=SC2206
    want=($AGENT_CHAIN_STRUCTURAL_PREFIX)

    for i in "${!want[@]}"; do
        if [ "${#chain[@]}" -le "$i" ] || [ "${chain[$i]}" != "${want[$i]}" ]; then
            seen="${chain[$i]:-<nothing — the chain ends here>}"
            printf 'position %d of the PreToolUse[Agent] chain must be %s, and is %s. The four structural gates settle whether the SPAWN is well formed, and they run ahead of every policy gate so that a dispatch which is malformed AND something else hears about the malformed half first.\n' \
                "$((i+1))" "${want[$i]}" "$seen"
            bad=1
        fi
    done

    for i in "${!chain[@]}"; do
        [ "$i" -lt "${#want[@]}" ] && continue
        case " $AGENT_CHAIN_STRUCTURAL_PREFIX " in
            *" ${chain[$i]} "*)
                printf '%s is a STRUCTURAL gate wired at position %d, behind a policy gate. It belongs in the first %d entries: a spawn that fails it should never have been reasoned about by anything downstream.\n' \
                    "${chain[$i]}" "$((i+1))" "${#want[@]}"
                bad=1
                ;;
        esac
    done

    return "$bad"
}
