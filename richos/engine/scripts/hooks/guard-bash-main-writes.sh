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
# parser or a sandbox: it checks shell substitutions in their own directory
# scope, but cannot know whether an interpreter invocation like
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

_BMW_PARSER="$SCRIPT_DIR/../lib/unguarded_rm.py"
_BMW_HEREDOC="$SCRIPT_DIR/../lib/host_sleep.py"
read -r -d '' _BMW_CLASSIFIER <<'PYEOF' || true
import json, re, os, sys, runpy
reader = runpy.run_path(os.environ['GUARD_PARSER'])
tokenize, match_paren = reader['tokenize'], reader['_match_paren']
sys.path.insert(0, os.path.dirname(os.environ['GUARD_PARSER']))
heredocs = runpy.run_path(os.environ['GUARD_HEREDOC'])['heredocs']
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

def check_targets(tokens, base):
    for tok in tokens:
        if not tok or tok.startswith('-'):
            continue
        target = os.path.realpath(os.path.join(base, os.path.expanduser(tok)))
        for prefix in prefixes:
            protected = os.path.join(ROOT, prefix.strip('/'))
            if target == protected or target.startswith(protected + os.sep):
                return 'physical:' + target[:120]
    return None

# 1) write-verb clauses — only THAT verb's own argument list (up to the next
#    ; & | control operator) is checked, never the whole command line.
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

def shell_tokens(text):
    # Keep syntax separate from word values: a quoted or escaped > is data.
    word, started, quote, i = [], False, '', 0
    while i < len(text):
        c = text[i]
        if text.startswith('$(', i) and quote != Q1:
            end = match_paren(text, i)
            word.append(text[i:end]); started = True; i = end
            continue
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
    # The existing engine tokenizer skips data heredocs and keeps each command
    # substitution inside its outer word. It never changes the parent's cwd.
    for raw_stmt in tokenize(text)[0]:
        stmt, redirects, pending, nested = [], [], None, []
        for raw in raw_stmt:
            nested.extend(tokenize(raw)[1])
            parts = list(shell_tokens(raw))
            if (len(parts) > 1 and not parts[0][1] and parts[0][0].isdigit()
                    and parts[1][1] and ('<' in parts[1][0] or '>' in parts[1][0])):
                parts = parts[1:]  # adjacent IO number: 2>file is not an operand
            for tok, operator in parts:
                if operator:
                    if '<' in tok or '>' in tok:
                        pending = tok
                elif pending:
                    if pending in ('>', '>>'):
                        redirects.append(tok)
                    pending = None
                else:
                    stmt.append(tok)
        yield stmt, redirects, nested

def verb_and_args(stmt, base):
    k = 0
    while k < len(stmt):
        t = stmt[k].lstrip(BT)
        if t in KEYWORDS or ASSIGN.match(t):
            k += 1
            continue
        name = os.path.basename(t)
        if name in WRAPPERS:
            valued = WRAPPERS[name]
            k += 1
            while k < len(stmt) and (stmt[k].startswith('-') or ASSIGN.match(stmt[k]) or stmt[k - 1] in valued):
                k += 1
            continue
        break
    if k >= len(stmt):
        return None, [], base
    word = os.path.basename(stmt[k].lstrip(BT))
    rest = stmt[k + 1:]
    if word == 'git':
        location = base
        i = 0
        while i < len(rest) and rest[i].startswith('-'):
            if rest[i] == '-C' and i + 1 < len(rest):
                location = os.path.realpath(os.path.join(location, rest[i + 1]))
            elif rest[i].startswith('-C') and len(rest[i]) > 2:
                location = os.path.realpath(os.path.join(location, rest[i][2:]))
            i += 2 if rest[i] in ('-C', '-c', '--git-dir', '--work-tree') else 1
        if i < len(rest) and rest[i] in ('rm', 'mv'):
            return rest[i], rest[i + 1:], location
        return None, [], base
    return word, rest, base

def heredoc_projection(text):
    # Heredoc bytes are data to the surrounding shell, including in $(...).
    # Keep their bodies for shell consumers, but never parse prose as quoting.
    chars, docs, cursor = list(text), {}, 0
    for head, body in heredocs(text):
        start = text.find(head + '<<', cursor)
        if start < 0:
            continue
        start = text.find('\n', start + len(head)) + 1
        end = start + len(body)
        if not start or text[start:end] != body:
            continue
        # Also hide the terminator so a delimiter named like a shell keyword
        # cannot change the surrounding control flow.
        marker = re.compile(r'\s*').match(text, end).end()
        terminator = text.find('\n', marker)
        for i in range(start, len(text) if terminator < 0 else terminator):
            if chars[i] != '\n':
                chars[i] = ' '
        cursor = end
        # Inside a substitution the returned head can start in its parent's
        # quote. The last $( introduces the command that owns this heredoc.
        try:
            heads = list(statements(head))
        except ValueError:
            heads = list(statements(head.rsplit('$(', 1)[-1]))
        if heads:
            docs.setdefault(tuple(heads[-1][0]), []).append(body)
    return ''.join(chars), docs


def flow_tokens(text):
    # Only operators outside shell words can change control flow. Keep the
    # existing word, redirection and substitution classifier for each chunk.
    start = end = 0
    quote = ''
    word = in_test = False
    while end < len(text):
        c = text[end]
        if c == BS and quote != Q1:
            end += 2
            word = True
            continue
        if quote != Q1 and (text.startswith('$(', end) or
                            (not quote and text.startswith(('<(', '>('), end))):
            end = match_paren(text, end)
            word = True
            continue
        if c == BT and quote != Q1:
            # Backtick substitutions, like $(...), have their own cwd.
            end += 1
            while end < len(text) and text[end] != BT:
                end += 2 if text[end] == BS else 1
            end += 1
            word = True
            continue
        if quote:
            if c == quote:
                quote = ''
        elif c in (Q1, Q2):
            quote = c
            word = True
        elif c == '#' and not word:
            comment = text.find('\n', end)
            if not text[start:end].strip():
                start = len(text) if comment < 0 else comment
            end = len(text) if comment < 0 else comment
            continue
        elif not word and text.startswith('[[', end) and text[end + 2:end + 3] in (' ', '\t', '\n'):
            in_test, word = True, True
            end += 2
            continue
        elif in_test and not word and text.startswith(']]', end):
            in_test, word = False, True
            end += 2
            continue
        elif not in_test and not word and text.startswith('((', end):
            # _match_paren starts inside an opener. With ((, it sees the second
            # parenthesis first, leaving one more closing parenthesis to consume.
            end = match_paren(text, end)
            end += text.startswith(')', end)
            word = True
            continue
        elif not in_test and c in ';\n&|()' and not (c == '&' and (
                text[end - 1:end] in ('<', '>') or text.startswith(('&>', '&>>'), end))):
            if text[start:end].strip():
                yield text[start:end], False
            op = c
            end += 1
            if c in '&|' and text.startswith(c, end):
                op += c
                end += 1
            yield (';' if c == '\n' else op), True
            start, word = end, False
            continue
        else:
            word = c not in ' \t\n<>'
        end += 1
    if text[start:].strip():
        yield text[start:], False


LITERAL_STATUS = re.compile(r'\s*(?:!\s+)?(true|false|:)(?:[ \t]+#[^\n]*)?\s*')
CONTROL = re.compile(r'^\s*(if|then|elif|else|fi|\{|\})(?=[ \t\n]|$)')


def control_items(text):
    for chunk, operator in flow_tokens(text):
        if not operator:
            while match := CONTROL.match(chunk):
                yield match[1], True
                chunk = chunk[match.end():]
            if not chunk.strip():
                continue
        yield chunk, operator


def command_flow(text):
    """Split commands into lists, AND/OR chains, pipelines and conditional branches."""
    items = list(control_items(text))
    index = 0

    def take(value):
        nonlocal index
        if index < len(items) and items[index] == (value, True):
            index += 1
            return True
        return False

    def require(value):
        if not take(value):
            raise ValueError('unclosed shell compound')

    def command(depth):
        nonlocal index
        if depth > 64:
            raise ValueError('shell compounds nested too deeply')
        if take('if'):
            branches = []
            while True:
                condition = commands(('then',), depth + 1)
                require('then')
                body = commands(('elif', 'else', 'fi'), depth + 1)
                branches.append((condition, body))
                if not take('elif'):
                    break
            alternate = commands(('fi',), depth + 1) if take('else') else []
            require('fi')
            return ('if', branches, alternate)
        for opening, closing in (('(', ')'), ('{', '}')):
            if take(opening):
                body = commands((closing,), depth + 1)
                require(closing)
                return (opening, body)
        if index < len(items) and not items[index][1]:
            chunk = items[index][0]
            index += 1
            return ('simple', chunk)
        raise ValueError('unsupported shell control')

    def commands(stop, depth):
        result = []
        while index < len(items):
            if take(';'):
                continue
            if items[index][1] and items[index][0] in stop:
                break
            chain, gate = [], ';'
            while True:
                pipeline = [command(depth)]
                while take('|'):
                    while take(';'):
                        pass
                    pipeline.append(command(depth))
                chain.append((gate, pipeline))
                gate = '&&' if take('&&') else '||' if take('||') else ';'
                if gate == ';':
                    break
                while take(';'):
                    pass
            result.append((chain, take('&')))
        return result

    return commands((), 0)


def inspect(text, bases, depth=0, docs=None):
    if depth > 6 or not bases:
        return
    if docs is None:
        text, docs = heredoc_projection(text)

    def simple(st, redirects, nested, incoming, literal=None):
        for body in nested:
            inspect(body, incoming, depth + 1, docs)
        bodies = docs.get(tuple(st), [])
        body = bodies.pop(0) if bodies else None
        good, bad = set(), set()
        for base in incoming:
            verb, args, location = verb_and_args(st, base)
            if body is not None and verb in ('bash', 'sh', 'zsh', 'dash', 'ksh'):
                inspect(body, {base}, depth + 1)
            hit = check_targets(redirects, base)
            if not hit and verb in ('mkdir', 'cp', 'mv', 'rm', 'tee', 'touch', 'rsync'):
                hit = check_targets(write_targets(verb, args), location)
            if hit:
                print('BLOCK ' + hit); raise SystemExit
            if verb == 'cd':
                dest = [arg for arg in args if arg not in ('--', '-P', '-L')]
                target = dest[0] if dest else os.path.expanduser('~')
                good.add(os.path.realpath(os.path.join(base, os.path.expanduser(target))))
            elif literal != 'false':
                good.add(base)
            if literal not in ('true', ':'):
                bad.add(base)
        if st and st[0] == '!':
            return bad, good
        return good, bad

    def run_command(node, incoming):
        if not incoming:
            return set(), set()
        if len(incoming) > 64:
            raise ValueError('too many possible shell directories')
        kind, *parts = node
        if kind == 'if':
            branches, alternate = parts
            good, bad = set(), set()
            remaining = incoming
            for condition, body in branches:
                entered, remaining = run_list(condition, remaining)
                yes, no = run_list(body, entered)
                good.update(yes)
                bad.update(no)
            yes, no = run_list(alternate, remaining)
            return good | yes, bad | no
        if kind in ('(', '{'):
            yes, no = run_list(parts[0], incoming)
            if kind == '(':
                return (incoming if yes else set()), (incoming if no else set())
            return yes, no
        text = parts[0]
        constant = LITERAL_STATUS.fullmatch(text)
        good, bad = incoming, set()
        for st, redirects, nested in statements(text):
            good, bad = simple(st, redirects, nested, good | bad, constant[1] if constant else None)
        return good, bad

    def run_list(commands, incoming):
        good, bad = incoming, set()
        for chain, background in commands:
            initial = good | bad
            for gate, pipeline in chain:
                passed = good if gate == '&&' else bad if gate == '||' else initial
                if len(pipeline) == 1:
                    yes, no = run_command(pipeline[0], passed)
                else:
                    for node in pipeline:
                        run_command(node, passed)
                    yes = no = passed
                good = yes | (good if gate == '||' else set())
                bad = no | (bad if gate == '&&' else set())
            if background:
                good, bad = initial, set()
        return good, bad

    try:
        commands = command_flow(text)
    except ValueError:
        # Other compound syntax (for example case patterns) still gets target
        # checking. Keep both cwd outcomes when its control flow is unknown.
        for st, redirects, nested in statements(text):
            good, bad = simple(st, redirects, nested, bases)
            bases = good | bad | bases
            if len(bases) > 64:
                raise ValueError('too many possible shell directories')
    else:
        run_list(commands, bases)


try:
    inspect(cmd, {cwd})
except ValueError:
    print('BLOCK unreadable-shell-syntax'); raise SystemExit

print('PASS')
PYEOF
if ! RESULT="$(GUARD_ROOT="$ENTITY_ROOT" GUARD_TREES="$PROTECTED_PATHS" GUARD_PARSER="$_BMW_PARSER" GUARD_HEREDOC="$_BMW_HEREDOC" python3 -B -c "$_BMW_CLASSIFIER" <<<"$INPUT")"; then
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
