#!/usr/bin/env bash
#
# guard-bash-main-writes.sh — PreToolUse hook on Bash.
#
# Closes the Bash-cwd drift vector: native-isolation agents' Bash cwd can
# default to the MAIN checkout, so a compound `cd <repo-root> && mkdir/cp/rm`
# command (or an absolute-path write) scaffolds files / drops artifacts directly
# into the shared checkout's SOURCE trees. The Write/Edit/MultiEdit/NotebookEdit
# guard (guard-main-checkout-writes.sh) never sees raw Bash, so these otherwise
# surface as interactive permission prompts to the human operator — who must
# never be the fallback guardrail. This hook auto-DENIES instead: exit 2 with
# worktree guidance, the agent self-corrects, no human prompt.
#
# The protected trees are project-specific and read from orchestration.config
# (PROTECTED_PATHS) — the SAME trees guard-main-checkout-writes.sh protects.
#
# Policy (conservative, matches the isolation model):
#   BLOCK a Bash command when a write operation's OWN TARGET — never merely
#   some unrelated text elsewhere on the same command line — is a protected
#   tree in the main checkout:
#     1. a write verb (mkdir/cp/mv/rm/tee/touch/rsync): only THAT verb's own
#        argument list, up to the next `;`/`&`/`|`, is scanned for a protected
#        target. A write verb in one clause never condemns an unrelated
#        protected mention in a different clause of the same line; AND/OR
#     2. a real write-redirection (`>` / `>>`) whose TARGET TOKEN is protected.
#        fd-duplication (`2>&1`, `>&2`, `N>&M`) is excluded up front: it
#        duplicates a file descriptor and creates no file at all.
#   Resolve each target against the call's cwd and preceding cd commands, then
#   follow filesystem aliases before comparing with the main checkout's trees.
#   A worktree-looking symlink is no exemption.
#   Everything else passes: git commands, worktree-scoped paths, reads, tests,
#   scratchpad writes, docs/scripts/config writes.
#
# WHY THE TARGET ANCHORING (root-caused 2026-08-19 in a downstream adopter,
# into the engine 2026-08-28). The earlier rule was pure co-occurrence: "a
# write-ish token appears ANYWHERE" AND "a protected path appears ANYWHERE".
# The two facts were never tied to the same operation, so ordinary reads were
# blocked — measured, on the engine's own shipped guard:
#     ls <tree>/ && ls *.csv                       -> BLOCKED (a listing)
#     python3 <tree>/gate.py --check 2>&1 | head   -> BLOCKED (2>&1 matched the
#                                                     bare redirect regex)
#     mkdir -p /tmp/scratch && cat <tree>/x.py     -> BLOCKED (unrelated clauses)
# All three now pass, and every genuine write (relative, redirect, absolute,
# cd-compound) still blocks — pinned as pairs in guard-bash-main-writes.test.sh.
#
# Acknowledged, deliberate limit — this is shell-TEXT analysis, not a shell
# parser or a sandbox: it does not see through command substitution
# (`$(mkdir ...)`), and it cannot know whether an interpreter invocation like
# `python3 <tree>/tool.py` performs file I/O of its own. That was never caught
# before or after this change; the hook's scope is and always was the Bash-cwd
# drift vector, not arbitrary-code sandboxing.
#
# REPO_ROOT MUST be the TRUE main checkout, never a per-session worktree copy —
# this guard's cd-compound and cwd-relative branches compare against ROOT
# directly, so a worktree-resolved ROOT would BOTH over-block a legitimate
# `cd <my-worktree> && mkdir <tree>/...` AND under-block the real drift. So we
# resolve via scripts/lib/resolve-main-checkout.sh (git --git-common-dir), which
# returns the main checkout from ANY invocation location; SCRIPT_DIR/../.. is
# only the no-git fallback.
#
# Fail-closed on missing python3 (same contract as the other payload parsers):
# an unparsed payload must never silently allow a protected write.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: guard-bash-main-writes.sh: python3 required — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/guard-bash-main-writes.sh
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

INPUT="$(cat)"

# Resolve the governed repository. Three outcomes, three different behaviors —
# see the contract for why "block everything unresolvable" is NOT the rule.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # This repository never adopted the engine, so there is no enforcement to
    # lose here. Stand down. NOT a silent skip: engine-status.sh announces the
    # stand-down into the orchestrator's own context at every session start.
    exit 0
else
    # BROKEN: this guard believes it is governing something and cannot. Block.
    root_failure_banner "scripts/hooks/guard-bash-main-writes.sh" >&2
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
    unevaluated_or_continue "guard-bash-main-writes.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this command writes into the main checkout of a governed repository"
fi

# Load the GOVERNED repository's config. Note what changed: the old code
# resolved the main checkout from THIS SCRIPT'S OWN LOCATION, which is right
# only while the engine is the repository. Loaded by reference it returned the
# engine's enclosing repo, so every path comparison below was made against a
# repository the session was not editing.
CONFIG="$ENTITY_ROOT/orchestration.config"
[ -f "$CONFIG" ] && . "$CONFIG"
: "${PROTECTED_PATHS:=}"

# Sensible failure: with no protected paths configured, the guard cannot know
# what to protect. Surface it loudly (never silently) and allow the command —
# consistent with guard-main-checkout-writes.sh.
if [ -z "${PROTECTED_PATHS// /}" ]; then
    echo "(hook: guard-bash-main-writes.sh) NOTE: PROTECTED_PATHS is unset in orchestration.config — Bash-write guard is INACTIVE. Fill PROTECTED_PATHS to enable it." >&2
    exit 0
fi

# (payload already read above, before root resolution)

read -r -d '' _BMW_CLASSIFIER <<'PYEOF' || true
import json, re, os, sys
try:
    d = json.loads(sys.stdin.read() or '{}')
except Exception:
    print('PASS'); raise SystemExit
if d.get('tool_name') != 'Bash':
    print('PASS'); raise SystemExit
cmd = (d.get('tool_input') or {}).get('command', '') or ''
ROOT = (os.environ.get('GUARD_ROOT') or '').rstrip('/')
prefixes = [p for p in (os.environ.get('GUARD_TREES') or '').split() if p]
if not prefixes:
    print('PASS'); raise SystemExit
cwd = d.get('cwd') or ROOT
ROOT = os.path.realpath(ROOT)

def check_targets(tokens):
    for tok in tokens:
        if not tok or tok.startswith('-'):
            continue
        target = os.path.realpath(os.path.join(cwd, os.path.expanduser(tok)))
        for prefix in prefixes:
            protected = os.path.join(ROOT, prefix.strip('/'))
            if target == protected or target.startswith(protected + os.sep):
                return 'physical:' + target[:120]
    return None

# 1) write-verb clauses — only THAT verb's own argument list (up to the next
#    ; & | control operator) is checked, never the whole command line.
VERB_CLAUSE = re.compile(r'(?:^|[;&|]\s*|\s)(mkdir|cp|mv|rm|tee|touch|rsync)\b(?P<args>[^;&|]*)')
def write_targets(verb, toks):
    # cp and rsync only READ their sources: just the destination is a write.
    # mv removes its source and rm/tee/touch/mkdir write every argument, so
    # those keep checking them all. A copy OUT of a protected file into scratch
    # is therefore allowed; a copy INTO a protected tree is still blocked.
    if verb not in ('cp', 'rsync'):
        return toks
    # P3-33: rsync --remove-source-files / --remove-sent-files DELETES its sources.
    # Quoting the flag does not change it ('--remove-source-files' is the same
    # argument to rsync), so compare it with its quotes removed (v3 re-check).
    if verb == 'rsync' and any(t.strip('\'"') in ('--remove-source-files', '--remove-sent-files') for t in toks):
        return toks
    dest = []
    plain = []
    i = 0
    while i < len(toks):
        t = toks[i]
        if t in ('-t', '--target-directory') and i + 1 < len(toks):
            dest.append(toks[i + 1]); i += 2; continue
        if t.startswith('--target-directory='):
            dest.append(t.split('=', 1)[1]); i += 1; continue
        if t.startswith('-') and t != '-':
            i += 1; continue
        plain.append(t); i += 1
    if dest:
        return dest
    return plain[-1:]  # the last operand is the destination

# The verb must be the COMMAND WORD of a statement, after VAR=value words,
# wrappers (sudo, env, xargs ...) and keywords (do, then ...), or the
# subcommand of git (git rm, git mv). A verb that is only an ARGUMENT
# (printf '%s' touch FILE) or sits in a COMMENT (true # touch FILE) writes
# nothing (hunt part 3, finding 39). Text the shell cannot split (an unclosed
# quote) falls back to the verb anywhere, as before.
Q1, Q2, BS, BT = chr(39), chr(34), chr(92), chr(96)
WRAPPERS = {'sudo': ('-u', '-g', '-C', '-D', '-h', '-p', '-R', '-T', '-U'),
            'env': ('-u', '-C', '-S', '--unset', '--chdir'),
            'xargs': ('-n', '-I', '-L', '-P', '-s', '-d', '-E', '-a'),
            'nice': ('-n',), 'command': (), 'exec': (), 'nohup': (), 'time': (), 'builtin': ()}
KEYWORDS = ('!', '{', '}', 'then', 'do', 'else', 'elif', 'if', 'while', 'until')
ASSIGN = re.compile(r'^[A-Za-z_][A-Za-z0-9_]*=')

def strip_comments(text):
    # A # that begins a word, outside quotes, starts a comment to end of line.
    out, q, i = [], '', 0
    while i < len(text):
        c = text[i]
        if q:
            if c == q:
                q = ''
            elif c == BS and q == Q2 and i + 1 < len(text):
                out.append(c); i += 1; c = text[i]
        elif c == BS and i + 1 < len(text):
            out.append(c); i += 1; c = text[i]
        elif c in (Q1, Q2):
            q = c
        elif c == '#' and (i == 0 or text[i - 1] in ' \t\n;&|()'):
            j = text.find('\n', i)
            if j < 0:
                break
            i = j
            continue
        out.append(c); i += 1
    return ''.join(out)

def shell_tokens(text):
    # Keep syntax separate from word values: a quoted or escaped > is data.
    word, started, quote, i = [], False, '', 0
    while i < len(text):
        c = text[i]
        if quote:
            if c == quote:
                quote = ''
            elif c == BS and quote == Q2 and i + 1 < len(text) and text[i + 1] in (Q2, BS, '$', BT, '\n'):
                i += 1
                if text[i] != '\n':
                    word.append(text[i])
            else:
                word.append(c)
        elif c in (Q1, Q2):
            quote, started = c, True
        elif c == BS and i + 1 < len(text):
            i += 1
            if text[i] != '\n':
                word.append(text[i]); started = True
        elif c.isspace() or c in '();&|<>':
            if started:
                yield ''.join(word), False
                word, started = [], False
            if c == '\n':
                yield ';', True
            elif not c.isspace():
                op = c
                while i + 1 < len(text) and text[i + 1] == c and c in '&|<>':
                    i += 1; op += c
                # >& and <& duplicate file descriptors, not file writes.
                if c in '<>' and i + 1 < len(text) and text[i + 1] == '&':
                    i += 1; op += '&'
                yield op, True
        else:
            word.append(c); started = True
        i += 1
    if quote:
        raise ValueError('unclosed shell quote')
    if started:
        yield ''.join(word), False


def statements(text):
    stmt, redirects, pending = [], [], None
    for tok, operator in shell_tokens(strip_comments(text)):
        if operator:
            if '<' in tok or '>' in tok:
                pending = tok
                continue
            yield stmt, redirects
            stmt, redirects, pending = [], [], None
        elif pending:
            if pending in ('>', '>>'):
                redirects.append(tok)
            pending = None
        else:
            stmt.append(tok)
    yield stmt, redirects

def verb_and_args(stmt):
    k = 0
    while k < len(stmt):
        t = stmt[k].lstrip(BT)
        if t in KEYWORDS or ASSIGN.match(t):
            k += 1
            continue
        base = os.path.basename(t)
        if base in WRAPPERS:
            valued = WRAPPERS[base]
            k += 1
            while k < len(stmt) and (stmt[k].startswith('-') or ASSIGN.match(stmt[k]) or stmt[k - 1] in valued):
                k += 1
            continue
        break
    if k >= len(stmt):
        return None, []
    word = os.path.basename(stmt[k].lstrip(BT))
    rest = stmt[k + 1:]
    if word == 'git':
        i = 0
        while i < len(rest) and rest[i].startswith('-'):
            i += 2 if rest[i] in ('-C', '-c', '--git-dir', '--work-tree') else 1
        if i < len(rest) and rest[i] in ('rm', 'mv'):
            return rest[i], rest[i + 1:]
        return None, []
    return word, rest

try:
    for st, redirects in statements(cmd):
        verb, args = verb_and_args(st)
        hit = check_targets(redirects)
        if not hit and verb in ('mkdir', 'cp', 'mv', 'rm', 'tee', 'touch', 'rsync'):
            hit = check_targets(write_targets(verb, args))
        if hit:
            print('BLOCK ' + hit); raise SystemExit
        # Only an actual cd changes the base for later relative operands.
        if verb == 'cd' and args:
            dest = [t for t in args if t not in ('--', '-P', '-L')]
            if dest:
                cwd = os.path.realpath(os.path.join(cwd, os.path.expanduser(dest[0])))
except ValueError:
    # Preserve the conservative malformed-input fallback.
    for vm in VERB_CLAUSE.finditer(cmd):
        hit = check_targets(write_targets(vm.group(1), re.findall(r'\S+', vm.group('args'))))
        if hit:
            print('BLOCK ' + hit); raise SystemExit

print('PASS')
PYEOF
if ! RESULT="$(GUARD_ROOT="$ENTITY_ROOT" GUARD_TREES="$PROTECTED_PATHS" python3 -c "$_BMW_CLASSIFIER" <<<"$INPUT")"; then
    echo "ERROR: guard-bash-main-writes.sh: payload classifier failed; refusing unevaluated operation" >&2
    exit 2
fi

case "$RESULT" in
  PASS) exit 0 ;;
  BLOCK*)
    echo "=== guard-bash-main-writes: BLOCKED ===" >&2
    echo "  This Bash command writes into the MAIN checkout's SOURCE tree" >&2
    echo "  (protected trees: $PROTECTED_PATHS) — reason: $RESULT." >&2
    echo "  Source is edited ONLY inside your own worktree:" >&2
    echo "    $ENTITY_ROOT/.claude/worktrees/<your-agent-id>/<tree>/..." >&2
    echo "  Use ABSOLUTE paths under your worktree for every file operation; never" >&2
    echo "  'cd $ENTITY_ROOT' for writes. Main receives source only via git merge." >&2
    echo "(hook: scripts/hooks/guard-bash-main-writes.sh)" >&2
    exit 2 ;;
  *) exit 0 ;;
esac
