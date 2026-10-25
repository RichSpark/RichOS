#!/usr/bin/env bash
#
# guard-vendoring-commits.sh — BLOCKING PreToolUse guard on the Bash tool.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# On 2026-09-04 a pre-publication audit asked this repository a question it
# could not answer: for each of the 27 skills under `engine/skills/`, did
# RichOS write it?
#
#   FIFTEEN DID NOT COME FROM RICHOS. EXACTLY ONE OF THE FIFTEEN HAD EVER BEEN
#   WRITTEN DOWN AS VENDORED, AND IT WAS WRITTEN DOWN IN A COMMIT MESSAGE.
#
# Establishing the other fourteen took a full agent fetching upstream
# repositories and comparing bytes. Two of the verdicts came back "high
# confidence" rather than certain, because by then the evidence had decayed —
# an upstream had renamed a skill, a history had been rewritten, an arrival
# commit had been squashed. Every one of those facts was KNOWN, for free, at
# the moment of vendoring, by the person doing it.
#
# The same missing fact produced a second, unrelated failure the same day.
# dialect-guard.sh Americanized four British spellings inside
# `engine/skills/copywriting/references/natural-transitions.md` — Corey
# Haines's MIT-licensed prose. That guard was working exactly as designed. It
# simply had no way to tell a vendored file from one we wrote, because nothing
# in this repository knew.
#
# The obvious response is a document, and the obvious response is wrong. A
# ledger a human is trusted to update IS the next thing an audit reconstructs:
# `docs/legal/THIRD-PARTY-NOTICES.md` already existed, already had an "Adding
# something new" section telling you to add a row, and fifteen skills reached a
# publication audit anyway. Every rule in this engine that survived is a rule
# with a write-time or commit-time chokepoint; every rule left to attention
# decayed, usually within days.
#
# So the recording is UNSKIPPABLE: a commit that adds files under a path
# holding redistributable material, with no entry in `.richos/vendored-material`
# covering them, is REFUSED, and the refusal names the path and the line to add.
#
# ===========================================================================
# THE ONE QUESTION IT ASKS, AND THE ONE IT REFUSES TO
# ===========================================================================
# IT ASKS: was this fact written down?
#
# IT DOES NOT ASK, AND MUST NEVER ASK: is this file third-party? A guard that
# inferred provenance — from a license header, from prose style, from an
# absent git history — would be confidently wrong some of the time, and a
# confidently wrong provenance claim is worse than an absent one: the absent
# one prompts a check. The 2026-09-04 audit is what inference costs when it is
# done properly, by a whole agent, with upstream repositories in hand. This
# guard has none of that and pretends to none of it.
#
# The consequence, stated so nobody is surprised by it: registering something
# is not a claim that it is correctly registered. `origin`, `license` and
# `revision` are checked for SHAPE and never for TRUTH. What the guard
# guarantees is that a person looked at the question at the moment they had the
# answer, and wrote down what they found.
#
# ===========================================================================
# WHERE IT FIRES, AND WHY ONLY THERE
# ===========================================================================
# `git commit`, against the STAGED INDEX — the same subject and the same reason
# as guard-publication-commits.sh: vendoring is `git add` followed by
# `git commit`, and the index is where the bytes are by then, whether a Write
# tool touched them or a `curl | tar` did. A PreToolUse[Write] guard would miss
# every vendoring that arrives as a download, which is most of them.
#
# NOT `git merge`, and that is deliberate rather than an omission. A merge
# carries commits that were already refused-or-permitted on the branch where
# the bytes first appeared; re-asking there would refuse a lander for a
# teammate's decision, at the one moment the lander cannot fix it. NOT
# `git push` either: nothing new arrives at a push.
#
# `--amend` widens the base to HEAD~1, because an amend rewrites the commit
# that is already there and its additions are additions again.
#
# ===========================================================================
# THE ESCAPE HATCH
# ===========================================================================
#     vendoring-ack: <reason>
#
# on its own line in the COMMIT MESSAGE. Same idiom as `model-ceiling-ack:`,
# `main-checkout-run:` and `resume-ack:`, and the same rule: A BARE MARKER
# EXEMPTS NOTHING. The reason must be a real sentence, and it must not be a
# promise to do the recording later — "I'll add the entry afterwards" is not a
# reason, it is the exact behavior the registry exists to replace, and it is
# named in the refusal rather than silently accepted.
#
# Accepted uses are appended to `<repo>/.claude/state/vendoring-acks.log`, so a
# habit of typing the marker is visible as a habit rather than as one commit.
#
# ===========================================================================
# FAILURE MODES
# ===========================================================================
#   no python3                          -> REFUSE (fail-closed)
#   unparseable payload                 -> pass (fail-open), like its siblings
#   the command is not a git commit     -> pass, silently
#   repository declares no registry     -> pass, silently: it has made no claim
#   registry declared but unreadable    -> REFUSE, with the banner
#   git cannot be queried for the index -> REFUSE (fail-closed): a guard that
#                                          cannot see what is being committed
#                                          must not say it looked

set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-vendoring-commits.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

# --- ROOT RESOLUTION -------------------------------------------------------
# TWO ROOTS, NEVER ONE. The full contract, and why the old single-root
# resolution was wrong the moment the engine became loadable by reference,
# is in scripts/lib/resolve-roots.sh. This bootstrap block is byte-identical
# in every hook that needs a root; contract-integrity-probe.sh Layer R asserts
# that, so a divergent copy is a probe failure rather than a surprise.
#
# THE BANNER BELOW GOES OUT ON TWO CHANNELS AND ONLY THE SECOND IS HEARD.
# Measured on Claude Code 2.1.270 (macOS, 2026-09-14) by registering one probe
# hook per channel on five events at once and reading the transcript back:
#
#   channel, exit 0     SessionStart  UserPromptSubmit  PreToolUse  PostToolUse  Stop
#   stderr              silent        silent            silent      silent       silent
#   stdout, plain text  model         model             silent      silent       silent
#   {"systemMessage"}   PERSON        PERSON            PERSON      PERSON       PERSON
#   additionalContext   model         model             model       model        model
#
# `silent` is not shorthand: the host records a `hook_success` attachment
# carrying the text in its stderr field and renders it to NO ONE. So a hook
# that could not find its own engine announced a dead enforcement layer
# exactly as loudly as a clean pass. Of the 60 files carrying this block, 35
# exit 2 here — where the host does render stderr, as the refusal reason — and
# 24 exit 0 and were inaudible. The 24 are the notices and observers, which is
# the trap: the hooks that must never block are the hooks nobody could hear.
#
# WHY `systemMessage` AND NOT `additionalContext`. additionalContext must name
# its own event in the envelope, and this block is identical in hooks
# registered on eight different events — it cannot know which one it is on.
# `systemMessage` is event-agnostic and it reaches the operator rather than
# only the model, which is the right audience for "your guards are off".
# Measured too: adding it to an exit-2 hook leaves the refusal untouched —
# same `hook error:` tool result, same blocked write — and only adds a render.
# Nothing here changes what any hook detects, refuses, or exits with.
#
# The escaping is deliberately pure bash (verified on 3.2.57, the macOS system
# shell) and calls nothing external: this is the one code path in the engine
# that runs when the install is already known to be broken, so it must not
# depend on python3, jq, or any file it has just failed to find.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    _RR_MSG="=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ===
  hook: scripts/hooks/guard-vendoring-commits.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 2
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

# --- JURISDICTION ----------------------------------------------------------
# Deliberately BELOW the root-resolution bootstrap, never inside it: Layer R of
# contract-integrity-probe.sh extracts that block verbatim and asserts it is
# byte-identical across every rooted hook.
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-vendoring-commits.sh"
        echo "  scripts/lib/seat-jurisdiction.sh is missing at: $_SJ_LIB"
        echo "  Without it this guard cannot tell whether the artifact it was"
        echo "  handed belongs to the repository it governs."
    } >&2
    exit 2
fi
# shellcheck source=../lib/seat-jurisdiction.sh
. "$_SJ_LIB"

# --- WHICH REPOSITORY IS THE COMMAND TALKING TO? ---------------------------
# ONE resolver, shared by every guard that asks — never a local copy. A copy is
# how the same `cd <repo> && git commit` hole ended up in five files.
_GJ_LIB="$SCRIPT_DIR/../lib/git-jurisdiction.sh"
if [ ! -f "$_GJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-vendoring-commits.sh"
        echo "  scripts/lib/git-jurisdiction.sh is missing at: $_GJ_LIB"
        echo "  Without it this guard cannot tell WHICH REPOSITORY the command"
        echo "  it was handed will actually commit to."
    } >&2
    exit 2
fi
# shellcheck source=../lib/git-jurisdiction.sh
. "$_GJ_LIB"

_VM_LIB="$SCRIPT_DIR/../lib/vendored-material.sh"
if [ ! -f "$_VM_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/guard-vendoring-commits.sh"
        echo "  scripts/lib/vendored-material.sh is missing at: $_VM_LIB"
        echo "  This guard's entire predicate lives there. Without it it cannot"
        echo "  tell a recorded vendoring from an unrecorded one, and it will"
        echo "  not guess."
    } >&2
    exit 2
fi
# shellcheck source=../lib/vendored-material.sh
. "$_VM_LIB"

INPUT="$(cat)"

if resolve_entity_root "$INPUT"; then
    # CAPTURED, not discarded — the seat decides whether this guard runs, and
    # is REPORTED when it differs from the repository being committed to. It is
    # never obeyed: this guard reads its contract out of the TARGET repository.
    SEAT_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # DELIBERATELY NOT AN EXIT, for guard-row-currency-commits.sh's reason: the
    # artifact's own repository still governs itself below, by its own committed
    # declaration. An unadopted seat that exited here is what once made a
    # committed declaration readable by nothing at all.
    SEAT_ROOT=""
else
    root_failure_banner "scripts/hooks/guard-vendoring-commits.sh" >&2
    exit 2
fi
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes: the tool-name extraction ends
# in `|| true`, so "this call is not mine" and "I could not tell whose call this
# is" are one exit. That is why 17 of 25 PreToolUse guards were measured passing
# a call in complete silence on 2026-09-05. This separates the two. NO VERDICT
# CHANGES — the exit is the one already taken — only the silence does. The
# measurement, the channel and the argument: scripts/lib/unevaluated-notice.sh.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-vendoring-commits.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this commit vendors third-party material without recording where it came from"
fi

# --- IS THIS A `git commit`, AND WHAT IS ITS MESSAGE? ----------------------
# The walk is deliberately narrow: `commit`, its message sources, and `--amend`.
# WHERE the command points is answered by richos_git_anchor and never here —
# emitting a second answer is the divergent copy that put one hole in five
# files.
# dispatch-words: git
#   (dispatch-pretooluse.sh skips this rule on a call that names none of these
#   words: they are the words the filter below needs before it can do anything.)
read -r -d '' _VG_CLASSIFIER <<'PYEOF' || true
import json, os, re, shlex, sys

def out(*fields):
    sys.stdout.write("\t".join(str(f) for f in fields) + "\n")
    raise SystemExit

try:
    d = json.loads(sys.stdin.read() or "{}")
except Exception:
    out("PASS")
if not isinstance(d, dict) or d.get("tool_name") != "Bash":
    out("PASS")
ti = d.get("tool_input") or {}
cmd = (ti.get("command", "") if isinstance(ti, dict) else "") or ""
cwd = str(d.get("cwd", "") or "")

if not re.search(r"\bgit\b", cmd):
    out("PASS")

# Heredoc bodies, so `git commit -F -` with an inline message is readable. The
# alternative is telling an operator their acknowledgement was not seen because
# of how they typed it.
heredocs = {}
lines = cmd.split("\n")
i = 0
while i < len(lines):
    m = re.search(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1", lines[i])
    if m:
        tag = m.group(2)
        body, j = [], i + 1
        while j < len(lines) and lines[j].strip() != tag:
            body.append(lines[j])
            j += 1
        heredocs.setdefault(tag, "\n".join(body))
        i = j
    i += 1

# SPLIT ON TOP-LEVEL SEPARATORS ONLY, and this is the one place this classifier
# deliberately differs from the one guard-row-currency-commits.sh carries.
#
# That one splits with `re.split(r"(?:\|\||&&|[;\n|])", cmd)`, which cuts inside
# quotes. A commit message written the way every commit message in this project
# is written —
#
#     git commit -m "add a skill
#
#     vendoring-ack: ..."
#
# — is therefore cut at the blank line, both halves fail to shlex, no `git
# commit` is recognized at all, and the guard passes the commit WITHOUT LOOKING.
# Measured on this guard's own first smoke run: three cases returned 0 and every
# one of them was a negative test passing for the wrong reason. A multi-line
# message is not an exotic input; it is the house style.
#
# So quote state is tracked, and a separator inside quotes is just text. Small,
# auditable, and it is the difference between a guard and a guard-shaped hole.
def top_level_segments(text):
    segs, cur, quote, esc = [], [], None, False
    i = 0
    while i < len(text):
        ch = text[i]
        if esc:
            cur.append(ch); esc = False; i += 1; continue
        if quote:
            if ch == "\\" and quote == '"':
                cur.append(ch); esc = True; i += 1; continue
            if ch == quote:
                quote = None
            cur.append(ch); i += 1; continue
        if ch == "\\":
            cur.append(ch); esc = True; i += 1; continue
        if ch in ("'", '"'):
            quote = ch; cur.append(ch); i += 1; continue
        if text[i:i + 2] in ("&&", "||"):
            segs.append("".join(cur)); cur = []; i += 2; continue
        if ch in ";\n|":
            segs.append("".join(cur)); cur = []; i += 1; continue
        cur.append(ch); i += 1
    segs.append("".join(cur))
    return segs

segments = top_level_segments(cmd)

def parse(seg):
    try:
        argv = shlex.split(seg, comments=False)
    except ValueError:
        return None
    while argv and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", argv[0]):
        argv.pop(0)
    if not argv:
        return None
    if os.path.basename(argv[0]) != "git":
        return None
    return argv

for seg in segments:
    argv = parse(seg)
    if not argv:
        continue
    k = 1
    sub = ""
    while k < len(argv):
        a = argv[k]
        if a == "-C" and k + 1 < len(argv):
            k += 2; continue
        if a.startswith("--git-dir") or a.startswith("--work-tree"):
            k += 2 if "=" not in a else 1
            continue
        if a.startswith("-c") and a != "-c":
            k += 1; continue
        if a == "-c" and k + 1 < len(argv):
            k += 2; continue
        if a.startswith("-"):
            k += 1; continue
        sub = a
        k += 1
        break
    if sub != "commit":
        continue

    rest = argv[k:]
    parts = []
    msource = "unavailable"
    amend = 0
    skip = False
    n = 0
    while n < len(rest):
        a = rest[n]
        if a == "--dry-run":
            skip = True
            break
        if a in ("-m", "--message") and n + 1 < len(rest):
            parts.append(rest[n + 1]); msource = "commit -m"; n += 2; continue
        if a.startswith("--message="):
            parts.append(a.split("=", 1)[1]); msource = "commit -m"; n += 1; continue
        if a.startswith("-m") and len(a) > 2:
            parts.append(a[2:]); msource = "commit -m"; n += 1; continue
        if a in ("-F", "--file") and n + 1 < len(rest):
            src = rest[n + 1]
            if src == "-":
                if len(heredocs) == 1:
                    parts.append(list(heredocs.values())[0])
                    msource = "commit -F - (heredoc)"
                else:
                    msource = "commit -F - (stdin, not readable here)"
            else:
                p = src if os.path.isabs(src) else os.path.join(cwd or ".", src)
                try:
                    with open(p, encoding="utf-8") as fh:
                        parts.append(fh.read())
                    msource = "commit -F"
                except Exception:
                    msource = "commit -F (file unreadable)"
            n += 2
            continue
        if a == "--amend":
            amend = 1
            if msource == "unavailable":
                msource = "commit --amend (message from HEAD or an editor)"
            n += 1; continue
        if a.startswith("-"):
            if a in ("-C", "--reuse-message", "-c", "--reedit-message", "--author",
                     "--date", "--cleanup", "--gpg-sign", "-S", "--fixup", "--squash"):
                n += 2
            else:
                n += 1
            continue
        n += 1
    if skip:
        continue
    out("ACT", amend, msource, json.dumps("\n\n".join(parts)) if parts else "")

out("PASS")
PYEOF

# Payload bytes use stdin: environment strings have a much smaller per-value
# limit on Linux. A failed classifier must never become an unevaluated pass.
if ! CLASS="$(python3 -c "$_VG_CLASSIFIER" <<<"$INPUT")"; then
    echo "ERROR: guard-vendoring-commits.sh: payload classifier failed; refusing unevaluated operation" >&2
    exit 2
fi
[ "$(printf '%s' "$CLASS" | cut -f1)" = "ACT" ] || exit 0

AMEND="$(printf '%s' "$CLASS" | cut -f2)"
MSRC="$(printf '%s' "$CLASS" | cut -f3)"
MSG_JSON="$(printf '%s' "$CLASS" | cut -f4-)"

_VG_GJ="$(richos_git_anchor "$INPUT" "commit")"
ANCHOR="$(printf '%s' "$_VG_GJ" | cut -f2)"
[ -n "$ANCHOR" ] || ANCHOR="$PWD"

REPO="$(git -C "$ANCHOR" rev-parse --show-toplevel 2>/dev/null || true)"
[ -n "$REPO" ] || exit 0

# --- GOVERNANCE: the artifact's OWN repository decides ---------------------
# "Am I governed?" and "what am I inspecting?" are asked of the SAME repository
# — the one this commit lands in. The seat gets no veto and is only reported,
# because a guard that switched itself off on a seat mismatch would wave
# through exactly the cross-repository commit it was built to catch.
VM_RC=0
vm_load "$REPO" || VM_RC=$?
case "$VM_RC" in
    0) ;;
    1) exit 0 ;;   # this repository declares no vendoring contract
    *) vm_broken_banner "scripts/hooks/guard-vendoring-commits.sh" "$VM_BROKEN_REASON" >&2
       echo "(hook: scripts/hooks/guard-vendoring-commits.sh)" >&2
       exit 2 ;;
esac

if [ -n "${SEAT_ROOT}" ]; then
    richos_assert_jurisdiction "scripts/hooks/guard-vendoring-commits.sh" "${SEAT_ROOT}" "$REPO" "commit in" "proceeds" || true
fi

# --- WHAT DOES THIS COMMIT ADD? -------------------------------------------
# The STAGED INDEX, against HEAD — or against HEAD~1 for an amend, because an
# amend rewrites the commit that is already there and its additions are
# additions again. On a repository with no commits yet, the empty tree.
BASE="HEAD"
if [ "$AMEND" = "1" ]; then
    if git -C "$REPO" rev-parse --verify -q HEAD~1 >/dev/null 2>&1; then
        BASE="HEAD~1"
    else
        BASE=""
    fi
elif ! git -C "$REPO" rev-parse --verify -q HEAD >/dev/null 2>&1; then
    BASE=""
fi
if [ -z "$BASE" ]; then
    BASE="$(git -C "$REPO" hash-object -t tree /dev/null 2>/dev/null || true)"
    if [ -z "$BASE" ]; then
        echo "ERROR: guard-vendoring-commits.sh: could not resolve a base tree in $REPO — refusing (fail-closed). A guard that cannot see what is being committed must not report that it looked." >&2
        exit 2
    fi
fi

ADDED=""
if ! ADDED="$(git -C "$REPO" diff --cached --name-only --diff-filter=A -z "$BASE" 2>/dev/null | tr '\0' '\n')"; then
    echo "ERROR: guard-vendoring-commits.sh: could not read the staged index of $REPO — refusing (fail-closed). A guard that cannot see what is being committed must not report that it looked." >&2
    exit 2
fi

# --- WHICH ADDITIONS ARE UNRECORDED? --------------------------------------
UNRECORDED=""
COVERED_N=0
GOVERNED_N=0
while IFS= read -r f; do
    [ -n "$f" ] || continue
    vm_governed "$f" || continue
    GOVERNED_N=$((GOVERNED_N + 1))
    if vm_covering "$f"; then
        COVERED_N=$((COVERED_N + 1))
        continue
    fi
    UNIT="$(vm_unit "$f")"
    case "
$UNRECORDED" in
        *"
$UNIT
"*) ;;
        *) UNRECORDED="${UNRECORDED}${UNIT}
" ;;
    esac
done <<EOF
$ADDED
EOF

[ -n "$UNRECORDED" ] || exit 0

# --- ACKNOWLEDGED, OR REFUSED ---------------------------------------------
MSG=""
if [ -n "$MSG_JSON" ]; then
    MSG="$(python3 -c 'import json,sys
sys.stdout.write(json.load(sys.stdin))' <<<"$MSG_JSON" 2>/dev/null || printf '')"
fi

ACK_MARKER="vendoring-ack"
# ONE extraction, not a test followed by an extraction — two greps with the
# same pattern are two chances to relax one of them and not the other. The
# reason is required by the pattern itself: at least one non-blank character
# after the marker, so a bare `vendoring-ack:` extracts to nothing.
ACK_REASON="$(printf '%s' "$MSG" \
    | grep -E "^[[:space:]]*${ACK_MARKER}:[[:space:]]*[^[:space:]]" \
    | sed -n '1p' \
    | sed -E "s/^[[:space:]]*${ACK_MARKER}:[[:space:]]*//" || true)"

# _vendoring_reason_problem <reason> — echo NOTHING when the reason is a
# well-formed justification; echo the refusal reason otherwise. A function
# rather than an inline heredoc because bash will not parse a quoted heredoc
# containing an apostrophe inside a command substitution. The floors are
# guard-model-ceiling.sh's, deliberately: one engine, one idea of what a reason
# looks like.
_vendoring_reason_problem() {
  python3 /dev/fd/3 3<<'PY' <<<"$1"
import sys, re
r = sys.stdin.read().strip()
MIN_CHARS = 30
MIN_WORDS = 5
MIN_CONTENT = 3
STOP = {
    "the","and","for","that","this","with","have","has","had","been","from",
    "just","only","need","needs","needed","want","wants","because","none",
    "null","reason","tbd","todo","fine","okay","yes","not","but","was","were",
    "are","its","here","there","thing","things","stuff","some","any","all",
    "does","doesnt","dont","cant","will","would","should","could","which",
    "them","they","their","when","what","also","into","over","than","then",
    "very","really","quite","sure","done","doing","make","made","use","used",
    "using","work","works","working","file","files","path","paths","entry",
    "entries","commit","registry","record","add","adds","added","adding",
}
# THE DEFERRAL REFLEX. Every other escape hatch in this engine guards against a
# reason that asserts merit; this one guards against a reason that is a PROMISE.
# "I'll register it in the next commit" is not a justification for not
# registering it — it is the behavior the registry replaces, and it was the
# actual state of fourteen skills for six months.
DEFER = re.compile(
    r"(\blater\b|afterwards?|follow[\s-]?up|next\s+(commit|pass|pr|land|round)|"
    r"\bsoon\b|\btemporar\w*|\bfor\s+now\b|\bwill\s+(add|record|register|document)|"
    r"\bgoing\s+to\s+(add|record|register)|\bTODO\b|\bpending\b)", re.I)
if not r:
    print("no 'vendoring-ack: <reason>' line is present in the commit message.")
else:
    words = re.findall(r"[A-Za-z][A-Za-z'-]*", r)
    content = {w.lower() for w in words if len(w) >= 4 and w.lower() not in STOP}
    hit = DEFER.search(r)
    if len(r) < MIN_CHARS:
        print("the reason given is %d character(s) long; a real justification needs "
              "at least %d. A bare or token marker exempts nothing."
              % (len(r), MIN_CHARS))
    elif len(words) < MIN_WORDS:
        print("the reason given is %d word(s) long; a real justification needs at "
              "least %d. A bare or token marker exempts nothing."
              % (len(words), MIN_WORDS))
    elif len(content) < MIN_CONTENT:
        print("the reason given carries %d substantive word(s) (needs %d) — it "
              "reads as filler, not a justification." % (len(content), MIN_CONTENT))
    elif hit:
        print("the reason given is a PROMISE TO RECORD IT LATER (%r). That is not a "
              "justification for committing it unrecorded; it is precisely the "
              "behavior this registry exists to replace, and it is how fourteen "
              "vendored skills went six months with no provenance. Writing the "
              "entry now costs less than typing this line."
              % hit.group(0))
    else:
        print("")
PY
}

ACK_WHY="$(_vendoring_reason_problem "$ACK_REASON" 2>/dev/null || printf 'the justification could not be evaluated.')"

if [ -z "$ACK_WHY" ]; then
    # Acknowledged. Best-effort log — never fail a commit because logging
    # failed; the line in the commit message is itself the audit trail.
    LOG_DIR="$REPO/.claude/state"
    mkdir -p "$LOG_DIR" 2>/dev/null || true
    {
        printf '%s\trepo=%s\tpaths=%s\t%s: %s\n' \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
            "$REPO" \
            "$(printf '%s' "$UNRECORDED" | tr '\n' ',' | sed 's/,$//')" \
            "$ACK_MARKER" \
            "$ACK_REASON"
    } >>"$LOG_DIR/vendoring-acks.log" 2>/dev/null || true
    exit 0
fi

# --- REFUSE — and CARRY the rule, do not point at it -----------------------
N="$(printf '%s' "$UNRECORDED" | grep -c . || true)"
{
    echo "=== UNRECORDED VENDORING — REFUSING THIS COMMIT ==="
    echo "  repository : $REPO"
    echo "  registry   : $VM_REGISTRY"
    echo ""
    echo "  This commit adds material under a path that holds work RichOS"
    echo "  REDISTRIBUTES, and $N path(s) have no entry saying where it came from:"
    echo ""
    printf '%s' "$UNRECORDED" | while IFS= read -r u; do
        [ -n "$u" ] || continue
        echo "    - $u"
    done
    echo ""
    echo "  Governed paths (REDISTRIBUTABLE_PATHS): $VM_REDISTRIBUTABLE_PATHS"
    echo ""
    echo "  WRITE IT DOWN NOW, WHILE YOU KNOW IT. Add one TAB-separated line per"
    echo "  thing to $VM_REGISTRY, ten fields:"
    echo ""
    echo "    path<TAB>origin<TAB>license<TAB>holder<TAB>upstream<TAB>revision<TAB>arrived<TAB>confidence<TAB>modified<TAB>evidence"
    echo ""
    echo "  origin is 'third-party' or 'richos' — RECORD YOUR OWN WORK TOO. That"
    echo "  is not bookkeeping: it is what lets this guard tell 'we wrote it and"
    echo "  said so' from 'nobody wrote anything down', and it is what stops"
    echo "  dialect-guard.sh editing somebody else's prose."
    echo ""
    echo "  This guard never guesses provenance and is not claiming your entry is"
    echo "  right. It is claiming somebody looked, at the one moment the answer"
    echo "  was free. On 2026-09-04 an audit reconstructed fifteen of these after"
    echo "  the fact; two came back short of certain because the evidence was gone."
    echo ""
    echo "  If this genuinely does not belong in the registry, say so where a"
    echo "  reviewer sees it — add a line to the COMMIT MESSAGE:"
    echo "    vendoring-ack: <why this material needs no entry>"
    echo "  $ACK_WHY"
    if [ "$MSRC" != "commit -m" ] && [ "$MSRC" != "commit -F" ] && [ "$MSRC" != "commit -F - (heredoc)" ]; then
        echo "  NOTE: this commit's message came from '$MSRC', which this guard cannot"
        echo "        read. An acknowledgement must arrive via -m or -F to be seen."
    fi
    echo "  Accepted uses are appended to .claude/state/vendoring-acks.log, so a"
    echo "  habit of typing the marker is visible as a habit."
    echo ""
    echo "  To stand this contract down for a repository that redistributes"
    echo "  nothing, delete $VM_REGISTRY — a visible, reviewable diff, never a flag."
    echo "(hook: scripts/hooks/guard-vendoring-commits.sh)"
} >&2
exit 2
