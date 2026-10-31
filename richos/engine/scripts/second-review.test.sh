#!/usr/bin/env bash
#
# second-review.test.sh: THE SECOND REVIEW, STARTED BY A COMMAND, NEVER BY HAND
# (scripts/second-review.sh running scripts/lib/second_review.py), against a
# fake reviewer program and fixture state only. Never the operator's registry,
# ~/.claude/state, ~/.codex or a real model.
#
# 2026-10-08: the CEO fetched a Codex review of an agent's work by hand four
# times, and each found defects the author's tests had passed. His words
# (ruling §113): "A regular RichOS user can never be expected anything even
# remotely close to that. So, this all must be completely automated." Every
# case below fails on a checkout without scripts/second-review.sh.
#
#   C01  a handover review by teammate name: the input holds the brief exactly
#        as the teammate received it, the exact 40-character tip, the author's
#        commit message and last report; Codex runs with the pinned model and
#        effort, a workspace-write sandbox rooted at the export, outside a git
#        repository and --ephemeral (no entry in the user's Codex app); the
#        export is not the author's workspace; one ledger row with both models
#        (Codex's from its own logged session start), the CLI version, the token
#        count and the meter; the fixture is kept; the scratch is gone
#   C02  one blocking finding forces changes-requested whatever the reviewer
#        wrote, and nothing is filed as a follow-up (ruling §116)
#   C03  a recheck of the same work carries the earlier finding into the input
#   C04  a verdict naming another commit is refused: no verdict
#   C05  an answer outside the fixed shape is no verdict
#   C06  Codex at its usage limit: the Claude fallback reviews, on Opus
#   C07  Codex missing and the quota hold in force: no Claude review starts
#   C08  work Codex wrote (a codex/ branch) is reviewed by Claude
#   C09  CPU admission closed: no reviewer starts
#   C10  a reviewer past its time limit is stopped by its own process id
#   C11  no original words: refused before any reviewer runs
#   C12  the reviewer's builds are capped at 2 jobs (environment and input): on
#        2026-10-09 the CPU breaker stopped a reviewer's rustc at 5.08 cores
#   C13  second-review stopped from outside (review-watch replacing a mid-job
#        review) stops its reviewer too, and its scratch is released
#   C14  ruling §116: only non-blocking findings (one of them a P1) pass,
#        whatever the reviewer wrote, mid-job too, and each is filed in
#        <state>/review-follow-ups.jsonl; the result line says how many, where
#   C15  a recheck marks filed findings as already on the list; a pass with
#        them still open does not file them twice
#   C16  an earlier finding still open that blocks requests changes
#   C17  one work in two repositories: each repository's review carries and
#        files only its own repository's findings
#   C18  the reviewer never runs the user's own setup: Codex --ephemeral (no
#        session or history entry his Codex or ChatGPT app shows), without his
#        config.toml (its notify program made macOS ask the CEO, 2026-10-09) or
#        his exec-policy rules, notify empty, plugins, apps, hooks, computer use
#        and browser use off; Claude with no MCP server (--strict-mcp-config)
#   C19  an app review runs Claude on the account the app's work runs on when the
#        reviewer starts: started by the app's watcher (review_watch.py's spawn of
#        an app item) through this command, the reviewer gets the added account's
#        folder as CLAUDE_CONFIG_DIR and its id; after a switch back, Account 1's
#        inherited folder; the ledger stays where it was
#   C16  the app's Codex review switch (round 20.2, CEO §114) picks the reviewer of
#        each app review when it starts, through the watcher's own spawn: off (or
#        never set, first run) Claude; on with Codex signed in, Codex with every
#        isolation flag; on with Codex signed out (what `codex login status`
#        reports), Claude, and no Codex review is run; on with Codex missing, Claude
#   C21  the lead files a blocking finding as a follow-up (--follow-up, --reopen):
#        a teammate's shell or a caller with no Claude session is refused and
#        writes nothing; the lead's ruling writes one deferral and one follow-up
#        row; the next reviewer of a descendant tip, whatever its work key, is
#        told it is already filed and never asked to judge it again
#   C22  a recheck reviews only what changed since the last review of a commit it
#        was built on: its diff.patch holds since..tip, and a merge from main in
#        between brings none of main's changes in
#   C23  the repository's own instruction file, by Claude Code's rule and the same one the
#        worker's brief carries (scripts/lib/instruction_files.py; folder-instruction-files
#        plan §5.3 item 2): CLAUDE.md in
#        full past 64 KiB, the connected folder's uncommitted CLAUDE.local.md after it, and
#        no AGENTS.md beside a CLAUDE.md
#   C24  a CLAUDE.md linked out of the repository is read as the worker's brief reads it
#   C25  --follow-up reads a finding as the land fence does: a "withdrawn" from a review of
#        an unrelated tip (one not built on the finding's tip) never closes it, so the
#        finding the fence refuses on can be filed, and the fence then admits that tip
#   C26  a recheck that rules a finding "fixed" at exactly the tip it reviewed clears it at
#        the fence for that tip, even when it was not handed that finding; and the next
#        reviewer is handed every finding still open in its own tip's history, never one a
#        review of an unrelated tip withdrew. A "still-open" ruling, or a "fixed" ruling at
#        another tip, still blocks
#   C27  a "fixed" ruling counts only from a review of the exact tip being decided, handed the
#        finding or not: one at T1 leaves the finding open at T, and T's next reviewer is handed it
#   C28  a "fixed" ruling at the finding's own tip counts through a chain of rechecks back to the
#        review that raised it, even when the review between them omitted it; a fresh review of
#        that tip still cannot clear it
#   C29  the next reviewer of a tip is handed every finding the fence holds on at that tip, from
#        every work key with a land review there, not only its own (a manual --recheck's range key)
#   C30  C27's exact-tip rule holds only for findings raised in the land's own range: a branch that
#        merged main, carrying a landed work's finding ruled fixed only at the landed commit, is
#        admitted without re-ruling it
#   C32  under a Work lease (CLAUDE_CONFIG_DIR and RICHOS_CLAUDE_ACCOUNT_DIR at the account's
#        folder, RICHOS_CLAUDE_HOME at the machine-wide home) the ledger row is written in
#        RICHOS_CLAUDE_HOME's state and every reader finds it there; without RICHOS_CLAUDE_HOME
#        all follow CLAUDE_CONFIG_DIR as before, and so does a sandbox's own CLAUDE_CONFIG_DIR
#   C31  an app review takes the app gate's own hold, in review-watch's real (cleared)
#        environment, through the watcher's own spawn: the account in use published held,
#        no Claude review starts; another account in use with room, it runs on that one
#

# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SR="$SCRIPT_DIR/second-review.sh"

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }
command -v git >/dev/null 2>&1 || { echo "FATAL: git required" >&2; exit 1; }

# shellcheck source=lib/scratch.sh
. "$SCRIPT_DIR/lib/scratch.sh"
SB="$(scratch_new second-review-test)" || { echo "FATAL: no scratch" >&2; exit 1; }
cleanup() { scratch_release "$SB" >/dev/null 2>&1 || true; }
trap cleanup EXIT

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }
check() { if [ "$2" -eq 0 ]; then ok "$1"; else bad "$1 -- $3"; fi; }
has()   { printf '%s' "$1" | grep -qF -- "$2"; }

# --- isolation -------------------------------------------------------------
unset CLAUDE_PROJECT_DIR CLAUDE_PLUGIN_ROOT RICHOS_ENGINE_ROOT CODEX_HOME
# An app lease exports these; a case that wants one sets it itself (C26).
unset RICHOS_CLAUDE_HOME RICHOS_CLAUDE_ACCOUNT_DIR RICHOS_APP_STATE
mkdir -p "$SB/bin" "$SB/state" "$SB/calls" "$SB/tmp" "$SB/claude" "$SB/codex-sessions" \
         "$SB/workspaces/agents" "$SB/workspaces/done"
export SECOND_REVIEW_STATE_DIR="$SB/state"
export SECOND_REVIEW_CODEX="$SB/bin/fake-reviewer"
export SECOND_REVIEW_CLAUDE="$SB/bin/fake-reviewer"
export SECOND_REVIEW_QUOTA_CMD="$SB/bin/fake-quota"
export SECOND_REVIEW_CPU_BUSY=10
export RICHOS_WORKSPACES_DIR="$SB/workspaces"
export RICHOS_PROJECTS_DIR="$SB/projects"
export FAKE_LOG="$SB/calls"
# The command's own scratch lands in the fixture, so the test can see it go.
RUN_TMP="$SB/tmp"
RUN_CFG="$SB/claude"

# --- the fake reviewer: Codex when called as `exec`, Claude when `-p` -------
cat >"$SB/bin/fake-reviewer" <<'PY'
#!/usr/bin/env python3
import json, os, re, sys, time
argv = sys.argv[1:]
if argv[:1] == ["--version"]:
    print("codex-cli 9.9.9-fake")
    sys.exit(0)
if argv[:2] == ["login", "status"]:
    # What codex-cli 0.162.0-alpha.2 answers, measured 2026-10-09: exit 0 and "Logged in using
    # ChatGPT" on stderr when signed in, exit 1 and "Not logged in" when not. Never a review call.
    with open(os.path.join(os.environ["FAKE_LOG"], "..", "login-status.count"), "a") as f:
        f.write("1\n")
    if os.environ.get("FAKE_CODEX_LOGIN") == "out":
        sys.stderr.write("Not logged in\n")
        sys.exit(1)
    sys.stderr.write("Logged in using ChatGPT\n")
    sys.exit(0)
log = os.environ["FAKE_LOG"]
d = os.path.join(log, "%02d" % len(os.listdir(log)))
os.makedirs(d)
prompt = sys.stdin.read()
with open(os.path.join(d, "prompt.md"), "w") as f:
    f.write(prompt)
if os.path.isfile(os.path.join("review", "diff.patch")):
    with open(os.path.join("review", "diff.patch"), "rb") as src, open(os.path.join(d, "diff.patch"), "wb") as dst:
        dst.write(src.read())
kind = "codex" if argv[:1] == ["exec"] else "claude"
if kind == "codex":
    # What Codex logs at RUST_LOG=codex_exec=info: its session start, with the model and effort it runs
    # and where its session record goes (None with --ephemeral, as measured on codex-cli 0.162.0-alpha.2).
    effort = ""
    for i, a in enumerate(argv):
        if a == "-c" and argv[i + 1].startswith("model_reasoning_effort="):
            effort = argv[i + 1].split("=", 1)[1].strip('"')
    if "codex_exec=info" in os.environ.get("RUST_LOG", ""):
        sys.stderr.write('2026-10-09T05:36:46Z  INFO codex.exec: codex_exec: Codex initialized with event: '
                         'SessionConfiguredEvent { model: "%s", model_provider_id: "openai", reasoning_effort: '
                         'Some(%s), rollout_path: %s }\n' % (argv[argv.index("-m") + 1], effort.capitalize(),
                         "None" if "--ephemeral" in argv else 'Some("/fake/rollout.jsonl")'))
        sys.stderr.flush()
try:
    tree_a = open(os.path.join("tree", "a.txt")).read()
except OSError:
    tree_a = ""
with open(os.path.join(d, "call.json"), "w") as f:
    json.dump({"argv": argv, "cwd": os.getcwd(), "pid": os.getpid(), "kind": kind, "tree_a": tree_a,
               "jobs": [os.environ.get(k, "") for k in ("CARGO_BUILD_JOBS", "MAKEFLAGS", "CMAKE_BUILD_PARALLEL_LEVEL")],
               "config": os.environ.get("CLAUDE_CONFIG_DIR", ""), "account": os.environ.get("RICHOS_CLAUDE_ACCOUNT", "")}, f)
# A limit reached while this review runs (C31): a published reading copied over the app's.
during = os.path.join(log, "..", "publish-during.json")
if os.path.isfile(during):
    with open(during) as f:
        step = json.load(f)
    with open(step["from"]) as src, open(step["to"], "w") as dst:
        dst.write(src.read())
    os.remove(during)
mode = os.environ.get("FAKE_MODE", "pass")
if mode == "sleep":
    time.sleep(600)
if mode == "fail" and kind == "codex":
    sys.stderr.write("ERROR: You've hit your usage limit.\n")
    sys.exit(1)
m = re.search(r"^TIP: ([0-9a-f]{40})$", prompt, re.M)
# Every earlier finding the input lists is answered: FAKE_EARLIER (fixed by
# default) and FAKE_EARLIER_BLOCKS=1 to say it still blocks.
earlier = [{"id": i, "status": os.environ.get("FAKE_EARLIER", "fixed"),
            "blocks": os.environ.get("FAKE_EARLIER_BLOCKS") == "1", "note": "fake"}
           for i in re.findall(r"^- \*\*(\S+)\*\* \[", prompt, re.M)]
answer = {"reviewed_commit": m.group(1) if m else "", "verdict": "passed", "summary": "fake review",
          "checks": ["fake check"], "findings": [], "earlier_findings": earlier, "not_yet_claimed": []}
if mode == "other-commit":
    answer["reviewed_commit"] = "0123456789abcdef0123456789abcdef01234567"
if mode == "blocking-passed":
    answer["findings"] = [{"priority": 2, "blocks": True, "title": "Shutdown leaves audio on disk",
                           "files": ["a.txt:1"], "evidence": "the fixture kept the file",
                           "fixture": "fixtures/witness.txt"},
                          {"priority": 3, "blocks": False, "title": "A rare race at exit", "files": ["a.txt:2"],
                           "evidence": "needs two shutdowns in one millisecond", "fixture": ""}]
if mode == "nonblocking":
    answer["verdict"] = "changes-requested"
    answer["findings"] = [{"priority": 1, "blocks": False, "title": "A folder name with spaces breaks the export",
                           "files": ["a.txt:1"], "evidence": "needs a hand-made folder name", "fixture": ""},
                          {"priority": 3, "blocks": False, "title": "CRLF line endings are kept",
                           "files": ["a.txt:2"], "evidence": "needs a hand-edited file", "fixture": ""}]
if mode == "no-blocks":
    answer["findings"] = [{"priority": 3, "title": "a finding that does not say whether it blocks",
                           "files": [], "evidence": "", "fixture": ""}]
if mode == "malformed":
    answer = {"verdict": "passed"}
os.makedirs("fixtures", exist_ok=True)
with open(os.path.join("fixtures", "witness.txt"), "w") as f:
    f.write("fixture output\n")
if kind == "codex":
    with open(argv[argv.index("-o") + 1], "w") as f:
        json.dump(answer, f)
    tid = "019fake0-0000-7000-8000-%012d" % len(os.listdir(log))
    print(json.dumps({"type": "thread.started", "thread_id": tid}))
    print(json.dumps({"type": "turn.completed", "usage": {"input_tokens": 1000, "cached_input_tokens": 900,
                                                         "output_tokens": 50}}))
else:
    print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "structured_output": answer,
                      "modelUsage": {"claude-opus-5-5": {"inputTokens": 100, "outputTokens": 10}},
                      "usage": {"input_tokens": 100, "output_tokens": 10}, "duration_ms": 5}))
PY
chmod +x "$SB/bin/fake-reviewer"
cat >"$SB/bin/fake-quota" <<'SH'
#!/bin/sh
echo "quota: fake reading"
exit "${FAKE_QUOTA_EXIT:-0}"
SH
chmod +x "$SB/bin/fake-quota"

# --- the author's repository and its registry record and transcript --------
REPO="$SB/repo"
# core.hooksPath=/dev/null: the machine's global commit-identity guard is for
# real repositories, not for a fixture's throwaway commits.
G() { git -C "$REPO" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
mkdir -p "$REPO"
G init -q -b main
printf 'base\n' >"$REPO/a.txt"
printf '# Rules of this repository\nAGENTS-RULE-LINE\n' >"$REPO/AGENTS.md"
G add -A; G commit -qm "base"
BASE="$(G rev-parse HEAD)"
G checkout -qb cc/echo-sonnet-x1
printf 'base\nslice one\n' >"$REPO/a.txt"
G commit -qam "Dictation slice 1: COMMIT-MESSAGE-LINE"
TIP1="$(G rev-parse HEAD)"

SID="beadfeed-0000-4000-8000-0000000000aa"
AID="abc123def456"
cat >"$SB/brief.txt" <<'TXT'
cross-repo-worktree: /somewhere/echo-sonnet-x1

**His words, the acceptance criterion:** *"A regular user can never be expected anything — even "quoted" $HOME `cmd` \ backslash."*

Line after a blank line, with trailing spaces
TXT
python3 - "$SB" "$SID" "$AID" "$REPO" <<'PY'
import json, os, sys
sb, sid, aid, repo = sys.argv[1:5]
brief = open(os.path.join(sb, "brief.txt")).read()
rec = {"key": sid + "--echo-sonnet-x1", "name": "echo-sonnet-x1", "agent_id": aid, "session_id": sid,
       "registered_at": "2026-10-09T00:00:00Z", "subagent_type": "echo",
       "workspaces": [{"repo": repo, "path": repo, "branch": "cc/echo-sonnet-x1", "kind": "cc", "deleted_at": None}]}
json.dump(rec, open(os.path.join(sb, "workspaces", "agents", rec["key"] + ".json"), "w"))
d = os.path.join(sb, "projects", "-fixture", sid, "subagents")
os.makedirs(d)
rows = [{"type": "user", "message": {"role": "user", "content": brief}},
        {"type": "user", "isMeta": True, "message": {"role": "user", "content": "<system-reminder>x</system-reminder>"}},
        {"type": "assistant", "message": {"id": "m1", "role": "assistant", "model": "claude-sonnet-5-5",
                                          "content": [{"type": "text", "text": "Starting."}]}},
        {"type": "assistant", "message": {"id": "m2", "role": "assistant", "model": "claude-sonnet-5-5",
                                          "content": [{"type": "text", "text": "LAST-REPORT-LINE: 12 tests green."}]}}]
with open(os.path.join(d, "agent-%s.jsonl" % aid), "w") as f:
    for r in rows:
        f.write(json.dumps(r) + "\n")
json.dump({"agentType": "echo", "name": "echo-sonnet-x1", "model": "sonnet"},
          open(os.path.join(d, "agent-%s.meta.json" % aid), "w"))
PY

run() { # <args...>: runs the command with the fixture TMPDIR and config
    # These cases intentionally ask the fake reviewer for another judgment.
    # An unchanged-source recheck must name the prior review rather than bypass
    # the production writer's shared claim.
    local recheck
    recheck="$(python3 - "$SB/state/reviews.jsonl" "$@" <<'PY'
import json,subprocess,sys
args=sys.argv[2:]
get=lambda k,d='':args[args.index(k)+1] if k in args else d
repo=get('--repo'); ref=get('--tip',get('--branch','HEAD'))
if not repo: raise SystemExit()
tip=subprocess.check_output(['git','-C',repo,'rev-parse',ref],text=True).strip()
purpose=lambda t:'handover' if t in ('manual','handover') else 'progress'
try: rows=[json.loads(l) for l in open(sys.argv[1])]
except OSError: rows=[]
for r in reversed(rows):
 if r.get('repo')==repo and r.get('tip')==tip and r.get('verdict') and purpose(r.get('trigger'))==purpose(get('--trigger','manual')):
  print(r['id']); break
PY
)"
    [ -z "$recheck" ] || set -- "$@" --recheck "$recheck"
    OUT="$(TMPDIR="$RUN_TMP" CLAUDE_CONFIG_DIR="$RUN_CFG" bash "$SR" "$@" 2>&1)"; RC=$?
}
calls() { ls "$SB/calls" | wc -l | tr -d ' '; }
last_call() { ls -d "$SB/calls"/* 2>/dev/null | tail -1; }
last_row() { tail -1 "$SB/state/reviews.jsonl" 2>/dev/null; }
field() { # <json> <python expression over r>
    printf '%s' "$1" | python3 -c 'import json,sys; r=json.loads(sys.stdin.read() or "{}"); print('"$2"')' 2>/dev/null
}
scratch_left() { find "$RUN_TMP/richos-scratch" -maxdepth 1 -name '*second-review*' 2>/dev/null | wc -l | tr -d ' '; }

echo "second-review.test.sh"

# --- C01 ---------------------------------------------------------------------
# The blocking test, in the words of Sage's check of follow-up 12 (F4): a hole in this work's
# normal use, or a regression it caused; never any hole elsewhere in the app (282530b2b).
BLOCK_TEST="It blocks only when a user could hit it in normal use AND it would matter to them AND either it breaks what this work promises (the original words above), or this work broke something that worked before; a hole that was already there, outside what this work promises, is a follow-up however real."
FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO" --trigger handover
C="$(last_call)"
check "C01 the command exits 0 on a passing verdict" "$([ "$RC" -eq 0 ] && echo 0 || echo 1)" "rc=$RC out=$OUT"
python3 - "$SB/brief.txt" "$C/prompt.md" <<'PY'
import sys
brief, prompt = open(sys.argv[1]).read(), open(sys.argv[2]).read()
sys.exit(0 if brief in prompt else 1)
PY
check "C01 the input holds the brief verbatim, as the teammate received it" $? "prompt: $C/prompt.md"
P="$(cat "$C/prompt.md" 2>/dev/null)"
has "$P" "TIP: $TIP1"; check "C01 the input names the exact 40-character tip" $? "tip $TIP1"
has "$P" "BASE: $BASE"; check "C01 the input names the base" $? "base $BASE"
has "$P" "COMMIT-MESSAGE-LINE"; check "C01 the input holds the author's commit message" $? ""
has "$P" "LAST-REPORT-LINE: 12 tests green."; check "C01 the input holds the author's last report" $? ""
has "$P" "AGENTS-RULE-LINE"; check "C01 the input holds the repository's own rules" $? ""
has "$P" "$BLOCK_TEST" && has "$P" "a deliberately hostile local process or a rare race" \
  && has "$P" "A claim you could not see hold is a finding, judged by the same test." && ! has "$P" "This replaces the duty's rule" \
  && has "$P" "list it in the report as a follow-up" && ! has "$P" "unresolved verification requirement"
check "C01 the input gives the reviewer the blocking test: normal use of THIS work, or a regression it caused (follow-up 12, D3)" $? ""
has "$P" "how text renders somewhere other than the app (GitHub, a terminal, a Markdown viewer), a missing or weak test, wording, and anything only a developer would see" \
  && has "$P" "The author's claims above are what you check the work against, never extra promises" \
  && ! has "$P" "it breaks what the work promises" && ! has "$P" "and the author's claims above)"
check "C01 the blocking test is the user's: off-app rendering, tests, wording and developer-only findings never block, and the author's claims are checked against, never counted as promises" $? ""
ARGV="$(field "$(cat "$C/call.json")" '" ".join(r["argv"])')"
CWD="$(field "$(cat "$C/call.json")" 'r["cwd"]')"
has " $ARGV " " exec "; check "C01 Codex runs as codex exec" $? "$ARGV"
has "$ARGV" "--sandbox workspace-write"; check "C01 the sandbox is workspace-write" $? "$ARGV"
has "$ARGV" "--skip-git-repo-check"; check "C01 --skip-git-repo-check" $? "$ARGV"
has "$ARGV" "-m gpt-6.1-sol"; check "C01 the model is pinned to gpt-6.1-sol" $? "$ARGV"
has "$ARGV" '-c model_reasoning_effort="high"'; check "C01 the effort is pinned to high" $? "$ARGV"
has "$ARGV" "exec --ephemeral "
check "C18 Codex runs --ephemeral: no session or history entry the user's Codex or ChatGPT app shows" $? "$ARGV"
has "$ARGV" "-C $CWD"; check "C01 the sandbox is rooted where the reviewer runs" $? "cwd=$CWD argv=$ARGV"
has "$ARGV" "--ignore-user-config --ignore-rules -c notify=[] --disable plugins --disable apps --disable hooks --disable computer_use --disable browser_use --sandbox"
check "C18 Codex runs isolated from the user's own Codex setup: no config.toml, no rules, no notify, no plugins, apps, hooks, computer or browser use" $? "$ARGV"
TREE_A="$(field "$(cat "$C/call.json")" 'r["tree_a"]')"
[ -n "$CWD" ] && [ "${CWD#"$REPO"}" = "$CWD" ] && has "$TREE_A" "slice one"
check "C01 the reviewer works in an export of the tip, never in the author's workspace" $? "cwd=$CWD tree_a=$TREE_A"
ROW="$(last_row)"
[ "$(field "$ROW" 'r["verdict"]')" = "passed" ]; check "C01 one ledger row: verdict passed" $? "$ROW"
[ "$(field "$ROW" 'r["tip"]')" = "$TIP1" ]; check "C01 the row names the exact tip" $? "$ROW"
[ "$(field "$ROW" 'r["reviewer"]')" = "codex" ] && [ "$(field "$ROW" 'r["reviewer_model"]')" = "gpt-6.1-sol" ] \
  && [ "$(field "$ROW" 'r["reviewer_effort"]')" = "high" ]
check "C01 the row records the reviewer, its model and effort as Codex's own session start logged them" $? "$ROW"
[ "$(field "$ROW" 'r["author"]')" = "echo-sonnet-x1" ] && [ "$(field "$ROW" 'r["author_model"]')" = "sonnet" ]
check "C01 the row records the author and the author's model" $? "$ROW"
[ "$(field "$ROW" 'r["cli_version"]')" = "codex-cli 9.9.9-fake" ]; check "C01 the row records the CLI version" $? "$ROW"
[ "$(field "$ROW" 'r["tokens"]["total"]')" = "1050" ] && [ "$(field "$ROW" 'r["codex_record"]')" = "None" ]
check "C01 the row records the token count, and that Codex wrote no session record" $? "$ROW"
[ "$(field "$ROW" 'r["trigger"]')" = "handover" ] && [ -n "$(field "$ROW" 'r["duration_s"]')" ]
check "C01 the row records the trigger and the duration" $? "$ROW"
RID="$(field "$ROW" 'r["id"]')"
[ -f "$SB/state/reviews/$RID/fixtures/witness.txt" ] && [ -f "$SB/state/reviews/$RID/verdict.json" ] \
  && [ -f "$SB/state/reviews/$RID/prompt.md" ]
check "C01 the full text and the fixtures are kept under reviews/<id>/" $? "id=$RID"
[ "$(scratch_left)" = "0" ] && [ ! -d "$CWD" ]; check "C01 the scratch folder is deleted" $? "left=$(scratch_left) cwd=$CWD"
has "$OUT" "SECOND-REVIEW passed"; check "C01 the verdict is printed once" $? "$OUT"

# --- C02 ---------------------------------------------------------------------
FAKE_MODE=blocking-passed run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
[ "$RC" -eq 1 ] && [ "$(field "$ROW" 'r["verdict"]')" = "changes-requested" ] && [ "$(field "$ROW" 'r["forced"]')" = "True" ] \
  && [ "$(field "$ROW" 'r["blocking"]')" = "1" ]
check "C02 one blocking finding forces changes-requested though the reviewer wrote passed" $? "rc=$RC row=$ROW"
[ ! -e "$SB/state/review-follow-ups.jsonl" ] && [ "$(field "$ROW" 'r["follow_ups"]')" = "0" ] && ! has "$OUT" "follow-up"
check "C02 a review that requests changes files nothing, not even its non-blocking finding" $? "out=$OUT row=$ROW"
P1ID="$(field "$ROW" 'r["id"]')"

# --- C03 ---------------------------------------------------------------------
printf 'base\nslice one\nfix\n' >"$REPO/a.txt"
G commit -qam "Dictation slice 1: the fix"
TIP2="$(G rev-parse HEAD)"
FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO" --trigger handover
P="$(cat "$(last_call)/prompt.md" 2>/dev/null)"
has "$P" "TIP: $TIP2" && has "$P" "$P1ID#1" && has "$P" "Shutdown leaves audio on disk"
check "C03 the recheck's input carries the earlier finding, by id, and the new tip" $? "id=$P1ID"
[ "$(field "$(last_row)" 'r["earlier_findings"]')" = "2" ]; check "C03 the row counts the earlier findings it asked about" $? "$(last_row)"

# --- C04 ---------------------------------------------------------------------
FAKE_MODE=other-commit run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
[ "$RC" -eq 2 ] && [ "$(field "$ROW" 'r["verdict"]')" = "None" ] && has "$(field "$ROW" 'r["why"]')" "0123456789ab"
check "C04 a verdict naming another commit is refused" $? "rc=$RC row=$ROW"

# --- C05 ---------------------------------------------------------------------
FAKE_MODE=malformed run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
[ "$RC" -eq 2 ] && [ "$(field "$ROW" 'r["verdict"]')" = "None" ] && has "$(field "$ROW" 'r["why"]')" "shape"
check "C05 an answer outside the fixed shape is no verdict" $? "rc=$RC row=$ROW"
FAKE_MODE=no-blocks run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
[ "$RC" -eq 2 ] && [ "$(field "$ROW" 'r["verdict"]')" = "None" ] && has "$(field "$ROW" 'r["why"]')" "'blocks'"
check "C05 a finding that does not say whether it blocks is outside the fixed shape" $? "rc=$RC row=$ROW"

# --- C06 ---------------------------------------------------------------------
N0="$(calls)"
FAKE_MODE=fail run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
CL="$(last_call)"
KIND="$(field "$(cat "$CL/call.json")" 'r["kind"]')"
CARGV="$(field "$(cat "$CL/call.json")" '" ".join(r["argv"])')"
[ "$RC" -eq 0 ] && [ "$(( $(calls) - N0 ))" = "2" ] && [ "$KIND" = "claude" ] && has "$CARGV" "--model opus" \
  && [ "$(field "$ROW" 'r["reviewer"]')" = "claude" ] && has "$(field "$ROW" 'r["fallback_why"]')" "usage limit"
check "C06 Codex at its usage limit: the Claude fallback reviews on Opus, and says why" $? "rc=$RC row=$ROW argv=$CARGV"
[ "$(field "$ROW" 'r["reviewer_model"]')" = "claude-opus-5-5" ] && [ "$(field "$ROW" 'r["author_model"]')" = "sonnet" ]
check "C06 the verdict says which model reviewed" $? "$ROW"
has "$(cat "$CL/prompt.md")" "$BLOCK_TEST"
check "C06 the Claude fallback gets the same blocking test as Codex" $? "$CL/prompt.md"
has "$CARGV" '"sandbox": {"enabled": true' && has "$CARGV" '"allowUnsandboxedCommands": false' \
  && has "$CARGV" "--disallowedTools Edit,Write,NotebookEdit" && has "$CARGV" "--setting-sources  "
check "C06 the Claude reviewer writes only through sandboxed Bash, with none of the lead's settings" $? "$CARGV"
has "$CARGV" "--setting-sources  --strict-mcp-config "
check "C18 the Claude reviewer starts no MCP server of the user's (--strict-mcp-config)" $? "$CARGV"

# --- C07 ---------------------------------------------------------------------
N0="$(calls)"
FAKE_QUOTA_EXIT=1 SECOND_REVIEW_CODEX="$SB/bin/no-such-codex" FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
[ "$RC" -eq 2 ] && [ "$(calls)" = "$N0" ] && has "$(field "$ROW" 'r["why"]')" "quota"
check "C07 Codex missing and the quota hold in force: no Claude review starts" $? "rc=$RC row=$ROW"

# --- C08 ---------------------------------------------------------------------
G checkout -qb codex/some-fix "$BASE"
printf 'base\ncodex wrote this\n' >"$REPO/a.txt"
G commit -qam "codex: a fix"
CTIP="$(G rev-parse HEAD)"
G checkout -q cc/echo-sonnet-x1
cat >"$SB/codex-words.txt" <<'TXT'
CODEX-HANDOVER-ENTRY: READY TO LAND codex/some-fix
TXT
FAKE_MODE=pass run --repo "$REPO" --branch codex/some-fix --base "$BASE" --words-file "$SB/codex-words.txt"
ROW="$(last_row)"
KIND="$(field "$(cat "$(last_call)/call.json")" 'r["kind"]')"
[ "$RC" -eq 0 ] && [ "$KIND" = "claude" ] && [ "$(field "$ROW" 'r["author"]')" = "codex" ] && [ "$(field "$ROW" 'r["tip"]')" = "$CTIP" ]
check "C08 work Codex wrote is reviewed by Claude" $? "rc=$RC kind=$KIND row=$ROW"

# --- C09 ---------------------------------------------------------------------
N0="$(calls)"
SECOND_REVIEW_CPU_BUSY=95 FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO" --admission-wait 0
ROW="$(last_row)"
[ "$RC" -eq 2 ] && [ "$(calls)" = "$N0" ] && has "$(field "$ROW" 'r["why"]')" "CPU"
check "C09 CPU admission closed: no reviewer starts, and the row says why" $? "rc=$RC row=$ROW"

# --- C10 ---------------------------------------------------------------------
SECOND_REVIEW_TIMEOUT_SECONDS=2 FAKE_MODE=sleep run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
FPID="$(field "$(cat "$(last_call)/call.json")" 'r["pid"]')"
GONE=1; if [ -n "$FPID" ] && ! kill -0 "$FPID" 2>/dev/null; then GONE=0; fi
[ "$RC" -eq 2 ] && [ "$GONE" -eq 0 ] && has "$(field "$ROW" 'r["why"]')" "time limit" && [ "$(scratch_left)" = "0" ]
check "C10 a reviewer past its time limit is stopped by its own pid; no verdict; scratch gone" $? "rc=$RC pid=$FPID row=$ROW"

# --- C11 ---------------------------------------------------------------------
N0="$(calls)"
FAKE_MODE=pass run --repo "$REPO" --tip "$TIP2" --base "$BASE"
[ "$RC" -ne 0 ] && [ "$RC" -ne 1 ] && [ "$(calls)" = "$N0" ] && has "$OUT" "original words"
check "C11 no original words: refused before any reviewer runs" $? "rc=$RC out=$OUT"

# --- C12 ---------------------------------------------------------------------
FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO"
C="$(last_call)"
JOBS="$(field "$(cat "$C/call.json")" '",".join(r.get("jobs") or [])')"
[ "$JOBS" = "2,-j2,2" ] && has "$(cat "$C/prompt.md")" "at most 2 jobs"
check "C12 the reviewer's builds are capped at 2 jobs, in its environment and its input" $? "jobs=$JOBS"

# --- C13 ---------------------------------------------------------------------
N0="$(calls)"
( TMPDIR="$RUN_TMP" CLAUDE_CONFIG_DIR="$RUN_CFG" FAKE_MODE=sleep exec python3 -c '
import os, subprocess, sys
p = subprocess.Popen(["bash", sys.argv[1], "--name", "echo-sonnet-x1", "--repo", sys.argv[2]], start_new_session=True,
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(p.pid); sys.stdout.flush(); p.wait()' "$SR" "$REPO" >"$SB/c13.pid" ) &
WAITER=$!
for _ in $(seq 1 100); do [ "$(calls)" != "$N0" ] && [ -f "$(last_call)/call.json" ] && break; sleep 0.1; done
SRPID="$(head -1 "$SB/c13.pid" 2>/dev/null)"
RPID="$(field "$(cat "$(last_call)/call.json" 2>/dev/null)" 'r["pid"]')"
[ -n "$SRPID" ] && kill -TERM -- "-$SRPID" 2>/dev/null
wait "$WAITER" 2>/dev/null
GONE=1
for _ in $(seq 1 50); do if [ -n "$RPID" ] && ! kill -0 "$RPID" 2>/dev/null; then GONE=0; break; fi; sleep 0.1; done
[ -n "$SRPID" ] && [ "$GONE" -eq 0 ] && [ "$(scratch_left)" = "0" ]
check "C13 stopped from outside, second-review stops its reviewer and releases its scratch" $? \
    "second-review pid=$SRPID reviewer pid=$RPID gone=$GONE scratch_left=$(scratch_left)"
[ -n "$RPID" ] && kill -0 "$RPID" 2>/dev/null && kill -KILL "$RPID" 2>/dev/null

# --- C14 ---------------------------------------------------------------------
FU="$SB/state/review-follow-ups.jsonl"
FAKE_MODE=nonblocking run --name echo-sonnet-x1 --repo "$REPO" --trigger long-job
ROW="$(last_row)"
RID14="$(field "$ROW" 'r["id"]')"
[ "$RC" -eq 0 ] && [ "$(field "$ROW" 'r["verdict"]')" = "passed" ] && [ "$(field "$ROW" 'r["forced"]')" = "True" ] \
  && [ "$(field "$ROW" 'r["blocking"]')" = "0" ] && [ "$(field "$ROW" 'r["p1"]')" = "1" ]
check "C14 only non-blocking findings (one a P1): passed, though the reviewer wrote changes-requested" $? "rc=$RC row=$ROW"
python3 - "$FU" "$RID14" "$TIP2" "$REPO" <<'PY'
import json, sys
path, rid, tip, repo = sys.argv[1:5]
rows = [json.loads(l) for l in open(path)]
mine = [r for r in rows if r["review"] == rid]
ok = (len(rows) == 2 and len(mine) == 2 and all(r["tip"] == tip and r["repo"] == repo for r in mine)
      and [r["title"] for r in mine] == ["A folder name with spaces breaks the export", "CRLF line endings are kept"]
      and mine[0]["files"] == ["a.txt:1"] and mine[0]["evidence"] == "needs a hand-made folder name"
      and [r["finding"] for r in mine] == [rid + "#1", rid + "#2"])
sys.exit(0 if ok else 1)
PY
check "C14 a mid-job pass files each non-blocking finding in the follow-up ledger: review id, repository, tip, title, files, evidence" $? "$(cat "$FU" 2>/dev/null)"
has "$OUT" "2 follow-up(s) filed in $FU" && [ "$(field "$ROW" 'r["follow_ups"]')" = "2" ]
check "C14 the one-line result says how many were filed and where" $? "$OUT"

# --- C15 ---------------------------------------------------------------------
FAKE_EARLIER=still-open FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO"
P="$(cat "$(last_call)/prompt.md" 2>/dev/null)"
ROW="$(last_row)"
[ "$RC" -eq 0 ] && has "$P" "**$RID14#1** [P1; already on the follow-up list]" \
  && [ "$(wc -l <"$FU" | tr -d ' ')" = "2" ] && [ "$(field "$ROW" 'r["follow_ups"]')" = "0" ]
check "C15 a recheck shows filed findings as already on the list, and a pass with them still open files nothing twice" $? \
    "rc=$RC row=$ROW ledger=$(cat "$FU" 2>/dev/null)"

# --- C16 ---------------------------------------------------------------------
FAKE_EARLIER=still-open FAKE_EARLIER_BLOCKS=1 FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
[ "$RC" -eq 1 ] && [ "$(field "$ROW" 'r["verdict"]')" = "changes-requested" ] && [ "$(field "$ROW" 'r["forced"]')" = "True" ] \
  && [ "$(wc -l <"$FU" | tr -d ' ')" = "2" ]
check "C16 an earlier finding the reviewer says is still open and blocks requests changes; nothing is filed" $? "rc=$RC row=$ROW"

# --- C17 ---------------------------------------------------------------------
# One teammate with workspaces in two repositories: the watcher gives both one
# work key. Review rv-20261009T104053Z-8662354d-47b2 filed a finding of the
# first repository under the second, and the first never filed it after that.
REPO2="$SB/repo2"
G2() { git -C "$REPO2" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
mkdir -p "$REPO2"
G2 init -q -b main
printf 'hq base\n' >"$REPO2/b.txt"
G2 add -A; G2 commit -qm "hq base"
G2 checkout -qb cc/echo-sonnet-x1
printf 'hq base\nhq change\n' >"$REPO2/b.txt"
G2 commit -qam "hq: the change"
python3 - "$SB" "$SID" "$REPO2" <<'PY'
import json, os, sys
sb, sid, repo2 = sys.argv[1:4]
p = os.path.join(sb, "workspaces", "agents", sid + "--echo-sonnet-x1.json")
rec = json.load(open(p))
rec["workspaces"].append({"repo": repo2, "path": repo2, "branch": "cc/echo-sonnet-x1", "kind": "cc", "deleted_at": None})
json.dump(rec, open(p, "w"))
PY
printf 'base\nslice one\nfix\nmore\n' >"$REPO/a.txt"
G commit -qam "Dictation slice 1: more"
FU_BEFORE="$(wc -l <"$FU" | tr -d ' ')"
FAKE_MODE=blocking-passed run --name echo-sonnet-x1 --repo "$REPO"
RID17="$(field "$(last_row)" 'r["id"]')"
FAKE_EARLIER=still-open FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO2"
P="$(cat "$(last_call)/prompt.md" 2>/dev/null)"
ROW="$(last_row)"
[ "$RC" -eq 0 ] && ! has "$P" "$RID17#" && [ "$(field "$ROW" 'r["earlier_findings"]')" = "0" ] \
  && [ "$(field "$ROW" 'r["follow_ups"]')" = "0" ] && [ "$(wc -l <"$FU" | tr -d ' ')" = "$FU_BEFORE" ]
check "C17 a review in a second repository of the same work is not asked about, and files nothing of, the first repository's findings" $? \
    "rc=$RC row=$ROW ledger=$(cat "$FU" 2>/dev/null)"
FAKE_EARLIER=still-open FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO"
ROW="$(last_row)"
python3 - "$FU" "$RID17" "$REPO" "$(field "$ROW" 'r["id"]')" <<'PY'
import json, os, sys
path, rid, repo, now = sys.argv[1:5]
rows = [json.loads(l) for l in open(path)]
mine = [r for r in rows if r["finding"].startswith(rid + "#")]
ok = (sorted(r["finding"] for r in mine) == [rid + "#1", rid + "#2"]
      and all(r["review"] == now and os.path.realpath(r["repo"]) == os.path.realpath(repo) for r in mine))
sys.exit(0 if ok else 1)
PY
check "C17 the first repository's recheck files its own findings, under its own repository" $? \
    "rc=$RC row=$ROW ledger=$(cat "$FU" 2>/dev/null)"

# --- C19 ---------------------------------------------------------------------
# The real second review of 802194f0e, finding 1: the app's watcher starts with a cleared
# environment, so after the user switched to an added Claude account its reviews still ran on
# the default one. Now it passes the app's account list (claude-accounts.json, which the app's
# work leases read through quota::Service::lease_account) and this command reads the account in
# use when the reviewer starts, setting its folder as a work lease does (engine_profile.rs).
# The real chain: the watcher's own spawn of an app item, this script, the reviewer it launches.
ROWS0="$(wc -l <"$SB/state/reviews.jsonl" | tr -d ' ')"
TMPDIR="$RUN_TMP" CLAUDE_CONFIG_DIR="$RUN_CFG" python3 - "$SCRIPT_DIR" "$REPO" "$(G rev-parse HEAD)" "$BASE" \
    "$SB/brief.txt" "$SB/claude-accounts.json" "$SB/bin/fake-reviewer" "$SB/account-two" "$SB/rw" <<'PY' >"$SB/c15.out" 2>&1
import json, os, sys, subprocess
from types import SimpleNamespace
scripts, repo, tip, base, words, accounts, claude, folder, state = sys.argv[1:]
sys.path.insert(0, os.path.join(scripts, "lib"))
import review_watch as rw
os.environ["REVIEW_WATCH_SECOND_REVIEW"] = os.path.join(scripts, "second-review.sh")
os.environ["REVIEW_WATCH_STATE_DIR"] = state
os.makedirs(state)
log = os.environ["FAKE_LOG"]
got = []
for in_use in ("2", "1"):
    subprocess.check_call(['git', '-C', repo, 'commit', '--allow-empty', '-qm', 'account fixture '+in_use])
    tip = subprocess.check_output(['git', '-C', repo, 'rev-parse', 'HEAD'], text=True).strip()
    with open(accounts, "w") as f:
        json.dump({"accounts": [{"id": "1", "label": "Home"}, {"id": "2", "label": "Work", "folder": folder}],
                   "inUse": in_use}, f)
    import second_review as sr
    work = sr.load_workspaces(os.path.dirname(scripts)).recovery_work_key(sr.find_teammate(sr.load_workspaces(os.path.dirname(scripts)), "echo-sonnet-x1"))
    it = rw.Item(work, "echo-sonnet-x1", repo, tip, base, "running", ref="echo-sonnet-x1",
                 source="app", words=[words])
    it.claude, it.accounts = claude, accounts
    watcher = rw.Watcher(os.path.dirname(scripts), "", world=SimpleNamespace())
    before = len(os.listdir(log))
    pid, _start = watcher.spawn(it, "long-job", os.path.join(state, "review-%s.log" % in_use))
    watcher.children[pid].wait()
    with open(os.path.join(state, "review-%s.log" % in_use)) as review_log:
        receipts = [json.loads(line[len("SECOND-REVIEW RECEIPT "):]) for line in review_log
                    if line.startswith("SECOND-REVIEW RECEIPT ")]
    assert len(receipts) == 1 and receipts[0]["invocation"] == it.review_invocation
    calls = sorted(os.listdir(log))[before:]
    call = json.load(open(os.path.join(log, calls[-1], "call.json"))) if calls else {}
    got.append({"in use": in_use, "reviewer": call.get("kind"), "config": call.get("config"),
                "account": call.get("account")})
print(json.dumps(got))
cfg = os.environ["CLAUDE_CONFIG_DIR"]
sys.exit(0 if got == [{"in use": "2", "reviewer": "claude", "config": folder, "account": "2"},
                      {"in use": "1", "reviewer": "claude", "config": cfg, "account": "1"}] else 1)
PY
check "C19 an app review runs Claude on the account in use when the reviewer starts: the added account's folder, then Account 1's after a switch" \
    $? "$(cat "$SB/c15.out")"
[ "$(( $(wc -l <"$SB/state/reviews.jsonl" | tr -d ' ') - ROWS0 ))" = "2" ]
check "C19 both reviews are in the same ledger: the account's folder moves only the reviewer" $? "rows before=$ROWS0"

# --- C31 ---------------------------------------------------------------------
# The second review of the usage-readings fix (rv-20261028T184535Z-d37b1f0d-e0d9, its blocking
# finding; richos-hq follow-ups.md "app-launched reviews never apply the usage hold"): the app's
# watcher starts with a cleared environment, so an app review's quota hold ran quota-watch, found
# no QUOTA_PAUSE_PERCENT, answered UNKNOWN, and a Claude review started while the account in use
# was held. Main had the same hole. The whole chain runs here in review-watch's REAL environment:
# exactly the keys review_watch.rs `environment` and `app_environment` pass, read from that source
# so the two cannot drift, a runtime PATH with no `claude` on it, a fixture HOME, and no other
# variable but this file's fake-reviewer and fixture-state seams. No entity root, project folder,
# app state or quota command, and the review's working directory (the watcher's state) holds no
# orchestration.config. The app publishes Work, the account in use, as held: no Claude review may
# start. Then Home is in use with room while Work's row is at 99%: the review runs, on Home.
APP31="$SB/app31"
mkdir -p "$APP31/engine-state" "$SB/runtime-bin" "$SB/home31" "$SB/account-two-31"
# Each by its full path: a delivered runtime's bin/python3 and bin/git are wrappers that find their
# real program beside themselves, so a link to one from elsewhere finds nothing.
for tool in python3 git bash; do
    printf '#!/bin/sh\nexec "%s" "$@"\n' "$(command -v "$tool")" >"$SB/runtime-bin/$tool"
    chmod +x "$SB/runtime-bin/$tool"
done
TMPDIR="$RUN_TMP" python3 - "$SCRIPT_DIR" "$REPO" "$BASE" "$SB/brief.txt" "$APP31" "$SB/bin/fake-reviewer" \
    "$SB/account-two-31" "$SB/rw31" "$SB/runtime-bin" "$SB/home31" "$RUN_CFG" <<'PY' >"$SB/c31.out" 2>&1
import json, os, re, shutil, subprocess, sys, time
scripts, repo, base, words, app, reviewer, folder, rwstate, runtime, home, cfg = sys.argv[1:]
source = open(os.path.join(scripts, "..", "..", "app", "crates", "richos-core", "src", "review_watch.rs")).read()
inherited = re.search(r"pub fn environment\(.*?for key in \[([^\]]*)\]", source, re.S)
search = re.search(r'pub const SEARCH_ENV: &str = "([A-Z_]+)"',
                   open(os.path.join(scripts, "..", "..", "app", "crates", "richos-core", "src", "codex_reviews.rs")).read())
assert inherited and search, "review_watch.rs no longer builds its environment as this test reads it"
path = runtime + ":/usr/bin:/bin:/usr/sbin:/sbin"
assert shutil.which("claude", path=path) is None, "a claude on the runtime PATH would let main's path probe a real account"
values = {"HOME": home, "TMPDIR": os.environ["TMPDIR"], "CLAUDE_CONFIG_DIR": cfg,
          "USER": os.environ.get("USER", "fixture"), "LOGNAME": os.environ.get("LOGNAME", "fixture"),
          "LANG": os.environ.get("LANG", "en_US.UTF-8")}
env = {"PATH": path, search.group(1): path}
for key in re.findall(r'"([A-Z_]+)"', inherited.group(1)):
    env[key] = values[key]
seams = {k: os.environ[k] for k in ("SECOND_REVIEW_STATE_DIR", "SECOND_REVIEW_CPU_BUSY", "RICHOS_WORKSPACES_DIR",
                                    "RICHOS_PROJECTS_DIR", "FAKE_LOG")}
seams.update(SECOND_REVIEW_CLAUDE=reviewer, SECOND_REVIEW_CODEX=reviewer)
quota_sources = [k for k in list(env) + list(seams)
                 if k in ("RICHOS_ENTITY_ROOT", "CLAUDE_PROJECT_DIR", "RICHOS_APP_STATE", "SECOND_REVIEW_QUOTA_CMD")
                 or k.startswith("QUOTA_")]
assert not quota_sources, "not review-watch's environment: %s" % quota_sources
env.update(seams)

now = int(time.time())
def publish(in_use, label, five, other, other_five, model=None, to=None):
    # `model`: the account in use's Fable week, in percent (a model's own week, quota.rs is_weekly).
    fable = lambda: [] if model is None else [{"id": "model:Fable", "label": "Weekly · Fable", "usedPercent": model,
                                                "resetsAt": (now + 200000) * 1000, "durationMs": 604800000}]
    windows = lambda used, reset, mine=False: [{"id": "five_hour", "label": "Five-hour", "usedPercent": used,
                                    "resetsAt": reset * 1000, "durationMs": 18000000},
                                   {"id": "seven_day", "label": "Weekly", "usedPercent": 20,
                                    "resetsAt": (now + 300000) * 1000, "durationMs": 604800000}] + (fable() if mine else [])
    held = five >= 93 or (model or 0) >= 99
    view = {"state": "fresh", "windows": windows(five, now + 4740, True), "checkedAt": (now - 30) * 1000,
            "retryAt": None, "nextCheckAt": None, "refreshIntervalMs": 300000, "message": None,
            "policy": {"enabled": True, "pausePercent": 93},
            "admission": {"state": "held", "resetsAt": (now + 4740) * 1000} if held else {"state": "ready"},
            "inUse": {"id": in_use, "label": label}, "heldUntil": None,
            # Each row with its own check time, as the app publishes them: the gate judges a call
            # on its own account's row (gate::decide), and so does the review.
            "accounts": [{"id": in_use, "label": label, "inUse": True, "windows": windows(five, now + 4740, True),
                          "checkedAt": (now - 30) * 1000},
                         {"id": other[0], "label": other[1], "inUse": False, "windows": windows(other_five, now + 900),
                          "checkedAt": (now - 30) * 1000}]}
    with open(to or os.path.join(app, "engine-state", "claude-quota.json"), "w") as f:
        json.dump(view, f)
    if to:
        return
    with open(os.path.join(app, "claude-quota-policy.json"), "w") as f:
        json.dump({"enabled": True, "pausePercent": 93}, f)
    with open(os.path.join(app, "claude-accounts.json"), "w") as f:
        json.dump({"accounts": [{"id": "1", "label": "Home"}, {"id": "2", "label": "Work", "folder": folder}],
                   "inUse": in_use}, f)

# The chain the app runs: the watcher's own spawn of an app item, second-review.sh, the reviewer.
drive = r'''
import json, os, sys, subprocess
from types import SimpleNamespace
scripts, repo, base, words, accounts, claude, rwstate, case = sys.argv[1:]
sys.path.insert(0, os.path.join(scripts, "lib"))
import review_watch as rw, second_review as sr
os.environ["REVIEW_WATCH_SECOND_REVIEW"] = os.path.join(scripts, "second-review.sh")
os.environ["REVIEW_WATCH_STATE_DIR"] = rwstate
os.makedirs(rwstate, exist_ok=True)
subprocess.check_call(["git", "-C", repo, "-c", "core.hooksPath=/dev/null", "-c", "user.name=Author",
                       "-c", "user.email=author@example.invalid", "-c", "commit.gpgsign=false",
                       "commit", "--allow-empty", "-qm", "usage hold fixture " + case])
tip = subprocess.check_output(["git", "-C", repo, "rev-parse", "HEAD"], text=True).strip()
ws = sr.load_workspaces(os.path.dirname(scripts))
work = ws.recovery_work_key(sr.find_teammate(ws, "echo-sonnet-x1"))
it = rw.Item(work, "echo-sonnet-x1", repo, tip, base, "running", ref="echo-sonnet-x1", source="app", words=[words])
it.claude, it.accounts = claude, accounts
log = os.environ["FAKE_LOG"]
before = len(os.listdir(log))
watcher = rw.Watcher(os.path.dirname(scripts), "", world=SimpleNamespace())
pid, _start = watcher.spawn(it, "long-job", os.path.join(rwstate, "review-%s.log" % case))
watcher.children[pid].wait()
calls = [json.load(open(os.path.join(log, c, "call.json"))) for c in sorted(os.listdir(log))[before:]]
ledger = os.path.join(os.environ["SECOND_REVIEW_STATE_DIR"], "reviews.jsonl")
row = json.loads(open(ledger).read().splitlines()[-1])
print(json.dumps({"reviewers": [(c["kind"], c["account"]) for c in calls], "tip": row.get("tip") == tip,
                  "quota": row.get("quota"), "why": row.get("why"), "verdict": row.get("verdict"),
                  "quota_held": row.get("quota_held")}))
'''
got = {}
os.makedirs(rwstate)
quota_file = os.path.join(app, "engine-state", "claude-quota.json")
for case, args in (("work held", ("2", "Work", 99, ("1", "Home"), 10)),
                   ("home in use, work at 99", ("1", "Home", 10, ("2", "Work"), 99)),
                   # The CEO, 2026-10-31: "You just have to avoid starting new automatic reviews. And
                   # the already running automatic reviews can finish."
                   ("fable week at its limit", ("2", "Work", 10, ("1", "Home"), 10, 99)),
                   ("fable limit reached during the review", ("2", "Work", 10, ("1", "Home"), 10, 20))):
    publish(*args)
    if case == "fable limit reached during the review":
        held_file = os.path.join(os.path.dirname(app), "quota-fable-held.json")
        publish(*args[:5], model=99, to=held_file)
        with open(os.path.join(env["FAKE_LOG"], "..", "publish-during.json"), "w") as f:
            json.dump({"from": held_file, "to": quota_file}, f)
    p = subprocess.run([os.path.join(runtime, "python3"), "-c", drive, scripts, repo, base, words,
                        os.path.join(app, "claude-accounts.json"), reviewer, rwstate, case],
                       env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, cwd=rwstate)
    lines = p.stdout.strip().splitlines()
    got[case] = json.loads(lines[-1]) if p.returncode == 0 and lines else {"failed": p.stdout}
print(json.dumps(got, indent=1))
held, room = got["work held"], got["home in use, work at 99"]
bad = []
if held.get("reviewers") != [] or not held.get("tip"):
    bad.append("a reviewer ran while Work, the account in use, was published held: %s" % held)
if "quota hold is in force" not in (held.get("why") or "") or "Work (account 2)" not in (held.get("quota") or ""):
    bad.append("the held row does not say the gate's hold on Work: %s" % held)
# The watcher settles a marked row as held, never lost (review-watch.test.sh W39; the second review
# of 3331a349416c, finding 1), so the mark is what keeps a hold from spending the commit's retries.
if held.get("quota_held") is not True or room.get("quota_held"):
    bad.append("only the held row carries quota_held: held %s, room %s" % (held, room))
if room.get("reviewers") != [["claude", "1"]] or room.get("verdict") != "passed":
    bad.append("Home in use with room: the review did not run on Home: %s" % room)
if "Home (account 1)" not in (room.get("quota") or ""):
    bad.append("the review on Home does not name Home's reading: %s" % room)
# A model's own week at its limit: no NEW review starts (quota.rs is_weekly; quota_watch.py).
fable, during = got["fable week at its limit"], got["fable limit reached during the review"]
if fable.get("reviewers") != [] or fable.get("quota_held") is not True:
    bad.append("Work's Fable week at 99%%: a new review started: %s" % fable)
# One already running is never held by it and finishes (the limit landed while it ran).
fable_now = [w for w in json.load(open(quota_file))["windows"] if w["id"] == "model:Fable"]
if not fable_now or fable_now[0]["usedPercent"] != 99:
    bad.append("the Fable limit was not published while the review ran: %s" % fable_now)
if during.get("reviewers") != [["claude", "2"]] or during.get("verdict") != "passed" or during.get("quota_held"):
    bad.append("the Fable limit reached during a running review stopped it: %s" % during)
print("\n".join(bad))
sys.exit(1 if bad else 0)
PY
check "C31 an app review in review-watch's own environment takes the gate's hold: Work published held, no Claude review starts; Home in use with room, it runs on Home; Work's Fable week at its limit, no new review starts; the limit reached during a running review, it finishes" \
    $? "$(cat "$SB/c31.out")"

# --- C16 ---------------------------------------------------------------------
# The Codex review switch in the app's Settings (round 20.2; his words, §114: "we should give
# the user a toggle/switch to manually enable that"). The app saves it as codex-reviews.json
# beside its account list; the watcher hands each app item that path, and the choice is read
# when the review starts. The real chain again: the watcher's spawn of an app item, this
# script, the reviewer it launches. Each case: (setting, Codex as the Mac reports it) ->
# which reviewer ran, whether a Codex review was run at all, and its arguments.
TMPDIR="$RUN_TMP" CLAUDE_CONFIG_DIR="$RUN_CFG" python3 - "$SCRIPT_DIR" "$REPO" "$(G rev-parse HEAD)" "$BASE" \
    "$SB/brief.txt" "$SB/bin/fake-reviewer" "$SB/codex-reviews.json" "$SB/rw16" "$SB/bin/no-such-codex" <<'PY' >"$SB/c16.out" 2>&1
import json, os, sys, subprocess
from types import SimpleNamespace
scripts, repo, tip, base, words, claude, setting, state, missing = sys.argv[1:]
sys.path.insert(0, os.path.join(scripts, "lib"))
import review_watch as rw
os.environ["REVIEW_WATCH_SECOND_REVIEW"] = os.path.join(scripts, "second-review.sh")
os.environ["REVIEW_WATCH_STATE_DIR"] = state
os.makedirs(state)
log = os.environ["FAKE_LOG"]
ledger = os.path.join(os.environ["SECOND_REVIEW_STATE_DIR"], "reviews.jsonl")
cases = [("never set (first run)", None, {}),
         ("off", False, {}),
         ("on, Codex signed in", True, {}),
         ("on, Codex signed out", True, {"FAKE_CODEX_LOGIN": "out"}),
         ("on, Codex missing", True, {"SECOND_REVIEW_CODEX": missing})]
got = []
for name, on, env in cases:
    subprocess.check_call(['git', '-C', repo, 'commit', '--allow-empty', '-qm', 'review setting fixture '+name])
    tip = subprocess.check_output(['git', '-C', repo, 'rev-parse', 'HEAD'], text=True).strip()
    if on is None:
        if os.path.exists(setting):
            os.unlink(setting)
    else:
        with open(setting, "w") as f:
            json.dump({"on": on}, f)
    saved = {k: os.environ.get(k) for k in env}
    os.environ.update(env)
    import second_review as sr
    work = sr.load_workspaces(os.path.dirname(scripts)).recovery_work_key(sr.find_teammate(sr.load_workspaces(os.path.dirname(scripts)), "echo-sonnet-x1"))
    it = rw.Item(work, "echo-sonnet-x1", repo, tip, base, "running", ref="echo-sonnet-x1",
                 source="app", words=[words])
    it.claude, it.codex_reviews = claude, setting
    watcher = rw.Watcher(os.path.dirname(scripts), "", world=SimpleNamespace())
    before = len(os.listdir(log))
    pid, _start = watcher.spawn(it, "long-job", os.path.join(state, "review-%d.log" % len(got)))
    watcher.children[pid].wait()
    for k, v in saved.items():
        if v is None:
            os.environ.pop(k, None)
        else:
            os.environ[k] = v
    calls = [json.load(open(os.path.join(log, c, "call.json"))) for c in sorted(os.listdir(log))[before:]]
    row = json.loads(open(ledger).read().splitlines()[-1])
    codex = [c for c in calls if c["kind"] == "codex"]
    got.append({"case": name, "reviewer": row.get("reviewer"), "verdict": row.get("verdict"),
                "codex runs": len(codex), "argv": " ".join(codex[0]["argv"]) if codex else "",
                "fallback": row.get("fallback_why") or ""})
print(json.dumps(got, indent=1))
ISOLATION = ("exec --ephemeral --ignore-user-config --ignore-rules -c notify=[] --disable plugins --disable apps "
             "--disable hooks --disable computer_use --disable browser_use --sandbox workspace-write")
want = {"never set (first run)": "claude", "off": "claude", "on, Codex signed in": "codex",
        "on, Codex signed out": "claude", "on, Codex missing": "claude"}
bad = []
for g in got:
    if g["reviewer"] != want[g["case"]] or g["verdict"] != "passed":
        bad.append("%s: reviewed by %s (%s), want %s" % (g["case"], g["reviewer"], g["verdict"], want[g["case"]]))
    if want[g["case"]] == "claude" and g["codex runs"]:
        bad.append("%s: a Codex review was run" % g["case"])
    if want[g["case"]] == "codex" and not g["argv"].startswith(ISOLATION):
        bad.append("%s: Codex ran without its isolation flags: %s" % (g["case"], g["argv"]))
by = {g["case"]: g for g in got}
if "signed in" not in by["on, Codex signed out"]["fallback"]:
    bad.append("signed out: the row does not say why Claude reviewed: %r" % by["on, Codex signed out"]["fallback"])
print("\n".join(bad))
sys.exit(1 if bad else 0)
PY
check "C16 the app's Codex switch picks each review's reviewer when it starts: off or never set Claude; on and signed in Codex, isolated; on and signed out or missing Claude, with no Codex review run" \
    $? "$(cat "$SB/c16.out")"

# C20: the shared writer, including a manual caller, owns source execution.
SECOND_REVIEW_STATE_DIR="$SB/claim-state" python3 - "$SCRIPT_DIR/lib" "$REPO" <<'PYTEST'
import json,os,subprocess,sys
from types import SimpleNamespace
sys.path.insert(0,sys.argv[1])
import second_review as sr
repo=sys.argv[2]; tip=sr.git(repo,'rev-parse','HEAD')
w=sr.Work(); w.repo=repo; w.tip=tip; w.base=tip; w.work_key='fixture'; w.words=[('original','authorized outcome')]
a=SimpleNamespace(trigger='handover',recheck='')
with sr.execution_claim(repo,tip):
 code="""import sys;sys.path.insert(0,sys.argv[1]);import second_review as s;
try:
 with s.execution_claim(sys.argv[2],sys.argv[3]):pass
except s.Refused:raise SystemExit(23)
"""
 assert subprocess.run([sys.executable,'-c',code,sys.argv[1],repo,tip]).returncode==23
# Dead owners release kernel claims; completed locks keep the same inode.
with sr.execution_claim(repo,tip):pass
sr.resolve_work=lambda _:w
calls=[]
def execute(a,w):
 calls.append(1)
 sr.append_row(dict(id='one',tip=tip,repo=repo,repo_id=sr.repo_identity(repo),trigger=a.trigger,
  verdict='passed',input_identity=sr.review_input_identity(w),recheck_of=a.recheck,record='fixture'))
 return 0
sr._review_owned=execute
assert sr.review(a)==0 and len(calls)==1
a.trigger='manual'
assert sr.review(a)==0 and len(calls)==1
w.claims=[('changed','new claim')]
try:sr.review(a)
except sr.Refused:pass
else:raise AssertionError('changed input needs an explicit recheck')
a.recheck='one'
assert sr.review(a)==0 and len(calls)==2
print('shared execution, manual deduplication and explicit rechecks passed')
PYTEST
check "C20 every writer claims source execution; exact duplicates reuse; changed inputs require an explicit recheck" $? "shared writer assertion failed"

# --- C21 ---------------------------------------------------------------------
# Follow-up 12 (D1, with Sage's check F1 and F2): the lead files a blocking finding as a
# follow-up with one command. On 2026-10-18 nightly 63 was held on a finding the CEO had ruled
# a follow-up, and the only way through was switching the gate off.
unset RICHOS_AGENT_OWNER CLAUDE_PID
DEF="$SB/state/review-deferrals.jsonl"
printf 'base\nslice one\nfix\nmore\nby hand\n' >"$REPO/a.txt"
G commit -qam "Dictation slice 1: reviewed by hand"
TIPM="$(G rev-parse HEAD)"
FAKE_MODE=blocking-passed run --repo "$REPO" --tip "$TIPM" --base "$BASE" --words-file "$SB/codex-words.txt"
RIDM="$(field "$(last_row)" 'r["id"]')"
[ "$RC" -eq 1 ]; check "C21 setup: a by-hand review with one blocking finding" $? "rc=$RC out=$OUT"
FU_N="$(wc -l <"$FU" | tr -d ' ')"
RICHOS_AGENT_OWNER=teammate-x run --follow-up "$RIDM#1" --why "the author's own ruling"
[ "$RC" -eq 64 ] && [ ! -e "$DEF" ] && [ "$(wc -l <"$FU" | tr -d ' ')" = "$FU_N" ] && has "$OUT" "teammate"
check "C21 a teammate's shell (RICHOS_AGENT_OWNER) is refused and writes nothing" $? "rc=$RC out=$OUT"
run --follow-up "$RIDM#1" --why "no Claude session"
[ "$RC" -eq 64 ] && [ ! -e "$DEF" ] && has "$OUT" "no Claude session"
check "C21 a caller with no Claude session in its ancestry (Codex) is refused and writes nothing" $? "rc=$RC out=$OUT"
mkdir -p "$RUN_CFG/sessions"
printf '{"pid": %d, "sessionId": "lead-session", "procStart": "%s", "kind": "interactive"}' "$$" \
    "$(TZ=UTC0 LC_ALL=C ps -o lstart= -p $$ | tr -s ' ' | sed 's/^ //;s/ $//')" >"$RUN_CFG/sessions/$$.json"
run --follow-up "$RIDM#1" --why "Rich: a narrow race, not this work's normal use"
python3 - "$DEF" "$FU" "$RIDM" "$TIPM" <<'PY'
import json, sys
dp, fp, rid, tip = sys.argv[1:5]
d = [json.loads(l) for l in open(dp)]
f = [json.loads(l) for l in open(fp) if json.loads(l).get("filed_by") == "lander"]
ok = (len(d) == 1 and d[0]["action"] == "follow-up" and d[0]["finding"] == rid + "#1" and d[0]["tip"] == tip
      and d[0]["why"] == "Rich: a narrow race, not this work's normal use" and d[0]["who"]["kind"] == "claude"
      and d[0]["title"] == "Shutdown leaves audio on disk"
      and len(f) == 1 and f[0]["finding"] == rid + "#1" and f[0]["why"] == d[0]["why"])
sys.exit(0 if ok else 1)
PY
check "C21 the lead's ruling writes one deferral row (finding, tip, who, why) and one follow-up row" $? \
    "rc=$RC out=$OUT deferrals=$(cat "$DEF" 2>/dev/null)"
run --follow-up "$RIDM#2" --why "a finding that never blocked"
[ "$RC" -eq 64 ] && has "$OUT" "not an open blocking finding" && [ "$(wc -l <"$DEF" | tr -d ' ')" = "1" ]
check "C21 a finding that holds no land is refused" $? "rc=$RC out=$OUT"
printf 'base\nslice one\nfix\nmore\nby hand\nafter the ruling\n' >"$REPO/a.txt"
G commit -qam "Dictation slice 1: after the ruling"
FAKE_MODE=pass run --name echo-sonnet-x1 --repo "$REPO" --trigger handover
P="$(cat "$(last_call)/prompt.md" 2>/dev/null)"
[ "$RC" -eq 0 ] && has "$P" "## Already filed as follow-ups" && has "$P" "\`$RIDM#1\` (P2) Shutdown leaves audio on disk" \
  && ! has "$P" "**$RIDM#1**"
check "C21 the next handover reviewer of a descendant tip is told the by-hand finding is already filed (linked by ancestry, not work key)" $? \
    "rc=$RC prompt=$(last_call)/prompt.md"
FAKE_MODE=pass run --repo "$REPO" --tip "$(G rev-parse HEAD)" --base "$BASE" --words-file "$SB/codex-words.txt"
P="$(cat "$(last_call)/prompt.md" 2>/dev/null)"
[ "$RC" -eq 0 ] && has "$P" "\`$RIDM#1\`" && ! has "$P" "**$RIDM#1**" && has "$P" "**$RIDM#2**"
check "C21 a recheck of the same work lists the filed finding as filed, never among the earlier findings to judge" $? \
    "rc=$RC prompt=$(last_call)/prompt.md"
run --reopen "$RIDM#1"
[ "$RC" -eq 0 ] && [ "$(tail -1 "$DEF" | python3 -c 'import json,sys; print(json.load(sys.stdin)["action"])')" = "reopen" ]
check "C21 --reopen undoes the ruling (the later row wins)" $? "rc=$RC out=$OUT"
FAKE_MODE=pass run --repo "$REPO" --tip "$(G rev-parse HEAD)" --base "$BASE" --words-file "$SB/codex-words.txt"
P="$(cat "$(last_call)/prompt.md" 2>/dev/null)"
! has "$P" "## Already filed as follow-ups"
check "C21 after --reopen the finding is no longer listed as filed" $? "rc=$RC prompt=$(last_call)/prompt.md"

# --- C22 ---------------------------------------------------------------------
# Follow-up 12, D2 (with Sage's F2 and F3): a recheck reviews only what changed since the last
# review of a commit it was built on. On 2026-10-18 the review of a one-commit, test-only change
# (94abf81c0) hunted the whole 98-file branch again.
G checkout -qb cc/d2-work "$BASE"
printf 'alpha\n' >"$REPO/a2.txt"
printf 'one\n' >"$REPO/d2.txt"
G add -A; G commit -qm "d2: the first slice"
TIPA="$(G rev-parse HEAD)"
FAKE_MODE=pass run --repo "$REPO" --tip "$TIPA" --words-file "$SB/codex-words.txt"
RIDA="$(field "$(last_row)" 'r["id"]')"
G checkout -q main
printf 'main change\n' >"$REPO/main.txt"
G add -A; G commit -qm "main: someone else's work"
G checkout -q cc/d2-work
G merge -q --no-edit main
printf 'two\n' >"$REPO/d2.txt"
G commit -qam "d2: the fix after the review"
TIPB="$(G rev-parse HEAD)"
FAKE_MODE=pass run --repo "$REPO" --tip "$TIPB" --words-file "$SB/codex-words.txt"
D="$(cat "$(last_call)/diff.patch" 2>/dev/null)"
P="$(cat "$(last_call)/prompt.md" 2>/dev/null)"
[ "$RC" -eq 0 ] && has "$D" "+two" && ! has "$D" "+alpha"
check "C22 the recheck's diff.patch holds only the commits after the last review (since..tip), not the reviewed range" $? \
    "rc=$RC patch=$(last_call)/diff.patch"
[ -n "$D" ] && ! has "$D" "main change"
check "C22 a merge from main between the two reviews brings none of main's changes into the patch" $? "patch=$(last_call)/diff.patch"
has "$P" "SINCE: $TIPA. Review $RIDA already covered BASE..SINCE" && [ "$(field "$(last_row)" 'r["since"]')" = "$TIPA" ] \
  && [ "$(field "$(last_row)" 'r["since_review"]')" = "$RIDA" ]
check "C22 the input and the row name the commit and the review the recheck starts after" $? "row=$(last_row)"
G checkout -q cc/echo-sonnet-x1

# --- C23 ---------------------------------------------------------------------
# richos-hq docs/plans/2026-10-21-folder-instruction-files.md §5.3 item 2: the reviewer reads the
# repository's own instruction file by the same rule as the worker's brief. Before, it read
# AGENTS.md only, cut at 64 KiB, and never CLAUDE.md, so it judged a worker by rules the worker
# was never shown.
REPO3="$SB/repo-with-claude-md"
G3() { git -C "$REPO3" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
mkdir -p "$REPO3"; G3 init -q -b main
python3 -c 'import sys; open(sys.argv[1], "w").write("# House rules\nCLAUDE-RULE-START\n" + "Keep every line of this file.\n" * 2500 + "CLAUDE-RULE-END\n")' "$REPO3/CLAUDE.md"
printf 'AGENTS-ONLY-LINE\n' >"$REPO3/AGENTS.md"
printf 'CLAUDE.local.md\n' >"$REPO3/.gitignore"
printf 'base\n' >"$REPO3/a.txt"
G3 add -A; G3 commit -qm "base"
BASE3="$(G3 rev-parse HEAD)"
printf 'CLAUDE-LOCAL-LINE\n' >"$REPO3/CLAUDE.local.md"
G3 checkout -qb cc/house-rules
printf 'base\nchange\n' >"$REPO3/a.txt"; G3 commit -qam "a change"
FAKE_MODE=pass run --repo "$REPO3" --tip "$(G3 rev-parse HEAD)" --base "$BASE3" --words-file "$SB/codex-words.txt"
# This prompt is over 64 KiB, so it is searched in its file: `has` pipes it into `grep -q`, which
# stops reading at the first match, and under pipefail the writer's SIGPIPE fails the check.
P3="$(last_call)/prompt.md"
in_p3() { grep -qF -- "$1" "$P3"; }
[ "$(wc -c <"$REPO3/CLAUDE.md")" -gt 65536 ] && in_p3 "CLAUDE-RULE-START" && in_p3 "CLAUDE-RULE-END" \
  && ! in_p3 "the whole file is tree/CLAUDE.md"
check "C23 the input holds the repository's CLAUDE.md in full, past 64 KiB" $? "rc=$RC prompt=$P3"
in_p3 "CLAUDE-LOCAL-LINE" && in_p3 "$(cd "$REPO3" && pwd -P)/CLAUDE.local.md, from the connected folder itself"
check "C23 the input holds the connected folder's uncommitted CLAUDE.local.md, named as not in tree/" $? "prompt=$P3"
! in_p3 "AGENTS-ONLY-LINE"
check "C23 beside a CLAUDE.md the AGENTS.md is not given, as Claude Code would not load it" $? "prompt=$P3"

# --- C24 ---------------------------------------------------------------------
# The mid-job review of 4fec9eb23 (rv-20261021T175714Z-4fec9eb2-df32, finding 2): a CLAUDE.md that
# links out of the repository, a common way to share one file between projects. The worker's brief
# read it through the repository; the second review followed the link inside its export, found
# nothing there, and gave the reviewer the AGENTS.md instead. Both now read by the same call.
REPO4="$SB/repo-with-shared-rules"
mkdir -p "$SB/shared-rules" "$REPO4"
printf 'SHARED-RULE-LINE\n' >"$SB/shared-rules/CLAUDE.md"
G4() { git -C "$REPO4" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
G4 init -q -b main
ln -s ../shared-rules/CLAUDE.md "$REPO4/CLAUDE.md"
printf 'AGENTS-BESIDE-THE-LINK\n' >"$REPO4/AGENTS.md"
printf 'base\n' >"$REPO4/a.txt"
G4 add -A; G4 commit -qm "base"
BASE4="$(G4 rev-parse HEAD)"
G4 checkout -qb cc/shared-rules
printf 'base\nchange\n' >"$REPO4/a.txt"; G4 commit -qam "a change"
FAKE_MODE=pass run --repo "$REPO4" --tip "$(G4 rev-parse HEAD)" --base "$BASE4" --words-file "$SB/codex-words.txt"
P4="$(last_call)/prompt.md"
grep -qF -- "SHARED-RULE-LINE" "$P4" && ! grep -qF -- "AGENTS-BESIDE-THE-LINK" "$P4"
check "C24 a CLAUDE.md linked out of the repository reaches the reviewer, as it reaches the worker" $? "rc=$RC prompt=$P4"

# --- C25 ---------------------------------------------------------------------
# 2026-10-23, richos main e0d960c6b: the land fence refused codex/iphone-verification-loop on
# rv-20261023T091432Z-e14ecb3f-a67e#1 and named --follow-up as the way through, and --follow-up
# answered "not an open blocking finding": a review of an unrelated tip on the same work key had
# "withdrawn" it. The fence reads only reviews of the land tip's own history; --follow-up read
# every later review. Fixture ledger only: tip A raises a blocking finding, tip B (cut from the
# same base, never built on A) withdraws it.
REPO5="$SB/repo-unrelated-withdrawal"
C25_STATE="$SB/c25-state"
G5() { git -C "$REPO5" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
mkdir -p "$REPO5" "$C25_STATE"
G5 init -q -b main
printf 'base\n' >"$REPO5/a.txt"; G5 add -A; G5 commit -qm "base"
BASE5="$(G5 rev-parse HEAD)"
G5 checkout -qb codex/tip-a; printf 'base\na\n' >"$REPO5/a.txt"; G5 commit -qam "tip A"
TIP5A="$(G5 rev-parse HEAD)"
G5 checkout -qb cc/tip-b "$BASE5"; printf 'base\nb\n' >"$REPO5/b.txt"; G5 add -A; G5 commit -qm "tip B"
TIP5B="$(G5 rev-parse HEAD)"
G5 checkout -q main
python3 - "$SCRIPT_DIR/lib" "$C25_STATE" "$REPO5" "$BASE5" "$TIP5A" "$TIP5B" <<'PY'
import json, os, sys
sys.path.insert(0, sys.argv[1])
import operator_fences as f
state, repo, base, a, b = sys.argv[2:7]
ident, work = f.review_repo_identity(repo), "range:%s:%s" % (repo, base)
finding = {"priority": 2, "title": "Recipe input missing", "files": ["x.sh"], "evidence": "e", "fixture": "",
           "blocks": True}
records = {"rv-c25a": {"answer": {"findings": [finding], "earlier_findings": []}, "earlier_findings_in": []},
           "rv-c25b": {"answer": {"findings": [], "earlier_findings": [{"id": "rv-c25a#1", "status": "withdrawn",
                       "blocks": False, "note": "The input this finding cites is absent from this tip."}]},
                       "earlier_findings_in": [dict(finding, id="rv-c25a#1")]}}
with open(os.path.join(state, "reviews.jsonl"), "w") as ledger:
    for rid, tip, verdict in (("rv-c25a", a, "changes-requested"), ("rv-c25b", b, "passed")):
        os.makedirs(os.path.join(state, rid))
        json.dump(records[rid], open(os.path.join(state, rid, "verdict.json"), "w"))
        ledger.write(json.dumps({"id": rid, "tip": tip, "repo": repo, "repo_id": ident, "work": work,
                                 "trigger": "handover", "verdict": verdict, "record": os.path.join(state, rid)}) + "\n")
PY
c25_gate() { python3 - "$SCRIPT_DIR/lib" "$C25_STATE/reviews.jsonl" "$REPO5" "$TIP5A" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
import operator_fences as f
print("\n".join(f.review_gaps(sys.argv[2], [sys.argv[4]], sys.argv[3]).get(sys.argv[4], [])))
PY
}
GATE="$(c25_gate)"
has "$GATE" "Blocking finding id(s): rv-c25a#1"
check "C25 setup: the fence refuses tip A on rv-c25a#1 although a review of unrelated tip B withdrew it" $? "gate=$GATE"
SECOND_REVIEW_STATE_DIR="$C25_STATE" run --follow-up "rv-c25a#1" --why "Rich: not in normal use of this work"
[ "$RC" -eq 0 ] && has "$OUT" "rv-c25a#1 filed as a follow-up" \
  && [ "$(field "$(tail -1 "$C25_STATE/review-deferrals.jsonl" 2>/dev/null)" 'r["tip"]')" = "$TIP5A" ]
check "C25 the finding the fence refuses on is filed by --follow-up (one deferral row on tip A)" $? "rc=$RC out=$OUT"
GATE="$(c25_gate)"
[ -z "$GATE" ]
check "C25 once filed, the fence admits tip A" $? "gate=$GATE"

# --- C26 ---------------------------------------------------------------------
# 2026-10-29, richos 5611a7964: rv-20261029T140028Z-3e883526-10b9 raised blocking finding #1. A
# review of an unrelated tip on the same work key (2f57a3fd, not built on it) withdrew it, so the
# reviewers of 5611a7964 were not handed it, although the fence (which reads only the land tip's own
# history) still held on it. The --recheck rv-20261029T155324Z-5611a796-3aed of 5611a7964 ruled it
# "fixed" anyway; the fence skipped that ruling because the id was not in its earlier_findings_in,
# and refused the land with "Blocking finding id(s): rv-20261029T140028Z-3e883526-10b9#1". Fixture
# ledger only: base -> A -> T1 -> T, and B cut from base. Review a (A) raises #1; review b (B)
# withdraws it; review c (T) is not handed it; review d rechecks c at T.
REPO6="$SB/repo-unhanded-fixed"
G6() { git -C "$REPO6" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
mkdir -p "$REPO6"
G6 init -q -b main
printf 'base\n' >"$REPO6/a.txt"; G6 add -A; G6 commit -qm "base"
BASE6="$(G6 rev-parse HEAD)"
G6 checkout -qb cc/work; printf 'base\na\n' >"$REPO6/a.txt"; G6 commit -qam "tip A"
TIP6A="$(G6 rev-parse HEAD)"
printf 'base\na\nt1\n' >"$REPO6/a.txt"; G6 commit -qam "tip T1"
TIP6T1="$(G6 rev-parse HEAD)"
printf 'base\na\nt1\nt\n' >"$REPO6/a.txt"; G6 commit -qam "tip T"
TIP6T="$(G6 rev-parse HEAD)"
G6 checkout -qb cc/other "$BASE6"; printf 'b\n' >"$REPO6/b.txt"; G6 add -A; G6 commit -qm "tip B"
TIP6B="$(G6 rev-parse HEAD)"
G6 checkout -q main
c26() { # <scenario>: prints "<earlier ids handed to a review of T>|<the fence's refusal of T>"
    python3 - "$SCRIPT_DIR/lib" "$SB/c26-$1" "$1" "$REPO6" "$BASE6" "$TIP6A" "$TIP6B" "$TIP6T1" "$TIP6T" <<'PY'
import json, os, sys
sys.path.insert(0, sys.argv[1])
state, scenario, repo, base, a, b, t1, t = sys.argv[2:10]
os.makedirs(state)
os.environ["SECOND_REVIEW_STATE_DIR"] = state
import operator_fences as f
import second_review as sr
ident, work = f.review_repo_identity(repo), "work-c26"
finding = {"priority": 2, "title": "Retry budget lost on continuation", "files": ["x.rs"], "evidence": "e",
           "fixture": "", "blocks": True}
note = "x.rs:505 keeps the marking through continuation creation; the probe confirms it."
ruling = {"id": "rv-c26a#1", "status": "fixed", "blocks": False, "note": note}
if scenario == "still-open":
    ruling = dict(ruling, status="still-open", blocks=True)
d_tip = t1 if scenario == "other-tip" else t
reviews = [("rv-c26a", a, "changes-requested", "", {"findings": [finding], "earlier_findings": []}, []),
           ("rv-c26b", b, "passed", "", {"findings": [], "earlier_findings": [
               {"id": "rv-c26a#1", "status": "withdrawn", "blocks": False, "note": "absent from tip B"}]},
            [dict(finding, id="rv-c26a#1")])]
c = ("rv-c26c", t, "passed", "", {"findings": [], "earlier_findings": []}, [])
d = ("rv-c26d", d_tip, "passed", "rv-c26c" if d_tip == t else "", {"findings": [], "earlier_findings": [ruling]}, [])
# The ruling at another tip comes before the land tip's own review, so the fence reads it.
reviews += [d, c] if scenario == "other-tip" else [c, d]
with open(os.path.join(state, "reviews.jsonl"), "w") as ledger:
    for n, (rid, tip, verdict, recheck, answer, handed) in enumerate(reviews):
        os.makedirs(os.path.join(state, rid))
        answer = dict(answer, reviewed_commit=tip, verdict=verdict)
        json.dump({"answer": answer, "earlier_findings_in": handed},
                  open(os.path.join(state, rid, "verdict.json"), "w"))
        ledger.write(json.dumps({"id": rid, "tip": tip, "repo": repo, "repo_id": ident, "work": work,
                                 "trigger": "manual", "verdict": verdict, "recheck_of": recheck,
                                 "record": os.path.join(state, rid)}) + "\n")
        if rid == "rv-c26b":
            # What the reviewer of T is handed once a, b are on the ledger (review c's input).
            ledger.flush()
            handed_to_t = ",".join(fid for fid, _ in sr.earlier_findings(work, t, repo, ident))
print("%s|%s" % (handed_to_t, " ".join(f.review_gaps(os.path.join(state, "reviews.jsonl"), [t], repo).get(t, []))))
PY
}
OUT6="$(c26 fixed)"
has "${OUT6%%|*}" "rv-c26a#1"
check "C26 the reviewer of T is handed rv-c26a#1, still open in T's own history (only unrelated tip B withdrew it)" $? "out=$OUT6"
[ -z "${OUT6#*|}" ]
check "C26 a recheck at T ruling rv-c26a#1 fixed, though not handed it, clears it: the fence admits T" $? "out=$OUT6"
OUT6="$(c26 still-open)"
has "${OUT6#*|}" "Blocking finding id(s): rv-c26a#1"
check "C26 guard: a still-open ruling at T, not handed it, still blocks T" $? "out=$OUT6"
OUT6="$(c26 other-tip)"
has "${OUT6#*|}" "Blocking finding id(s): rv-c26a#1"
check "C26 guard: a fixed ruling at another tip (T1, built on by T), not handed it, does not clear it at T" $? "out=$OUT6"

# --- C27, C28, C29 ------------------------------------------------------------
# Review rv-20261029T164027Z-d147e6e5-6f22 of the C26 fix, same fixture repository (base -> A ->
# T1 -> T). Each scenario writes a ledger; c27 prints "<ids handed to a new review of T on
# <handin-key>>|<the fence's refusal of T>". A finding is raised as rv-x#1.
c27() { # <scenario>
    python3 - "$SCRIPT_DIR/lib" "$SB/c27-$1" "$1" "$REPO6" "$TIP6A" "$TIP6T1" "$TIP6T" <<'PY'
import json, os, sys
sys.path.insert(0, sys.argv[1])
state, scenario, repo, a, t1, t = sys.argv[2:8]
os.makedirs(state)
os.environ["SECOND_REVIEW_STATE_DIR"] = state
import operator_fences as f
import second_review as sr
ident, app, rng = f.review_repo_identity(repo), "work-app", "range:%s:base" % repo
finding = {"priority": 2, "title": "Retry budget lost on continuation", "files": ["x.rs"], "evidence": "e",
           "fixture": "", "blocks": True}
handed = [dict(finding, id="rv-x#1")]
note = "x.rs:505 keeps the marking through continuation creation; the probe confirms it."
def rule(status):
    return [{"id": "rv-x#1", "status": status, "blocks": status == "still-open", "note": note}]
# (id, tip, work, recheck_of, findings, rulings, handed in)
raise_at = lambda tip: ("rv-x", tip, app, "", [finding], [], [])
handin_key = app
if scenario == "other-tip-handed":      # finding 1: a handed FIXED at T1; T's own review never mentions it
    reviews = [raise_at(a), ("rv-d", t1, app, "", [], rule("fixed"), handed), ("rv-c", t, app, "", [], [], [])]
elif scenario == "same-tip-omitted":    # finding 2: raised at T; recheck c of it omits it; recheck d of c fixes it
    reviews = [raise_at(t), ("rv-c", t, app, "rv-x", [], [], []), ("rv-d", t, app, "rv-c", [], rule("fixed"), handed)]
elif scenario == "same-tip-unlinked":   # guard: d is a fresh review of T, not a recheck of x's line
    reviews = [raise_at(t), ("rv-c", t, app, "rv-x", [], [], []), ("rv-d", t, app, "", [], rule("fixed"), handed)]
elif scenario == "range-key":           # finding 3: app key holds it at T; a manual recheck uses its range key
    reviews = [raise_at(a), ("rv-c", t, app, "", [], rule("still-open"), handed),
               ("rv-d", t, rng, "rv-c", [], [], [])]
    handin_key = rng
elif scenario == "range-key-fixed":     # control: the range-key reviewer, handed it, rules it fixed at T
    reviews = [raise_at(a), ("rv-c", t, app, "", [], rule("still-open"), handed),
               ("rv-d", t, rng, "rv-c", [], [], []), ("rv-e", t, rng, "rv-d", [], rule("fixed"), handed)]
    handin_key = rng
with open(os.path.join(state, "reviews.jsonl"), "w") as ledger:
    for rid, tip, work, recheck, findings, rulings, given in reviews:
        os.makedirs(os.path.join(state, rid))
        verdict = "changes-requested" if findings or any(r["blocks"] for r in rulings) else "passed"
        answer = {"reviewed_commit": tip, "verdict": verdict, "findings": findings, "earlier_findings": rulings}
        json.dump({"answer": answer, "earlier_findings_in": given},
                  open(os.path.join(state, rid, "verdict.json"), "w"))
        ledger.write(json.dumps({"id": rid, "tip": tip, "repo": repo, "repo_id": ident, "work": work,
                                 "trigger": "manual", "verdict": verdict, "recheck_of": recheck,
                                 "record": os.path.join(state, rid)}) + "\n")
handin = ",".join(fid for fid, _ in sr.earlier_findings(handin_key, t, repo, ident))
print("%s|%s" % (handin, " ".join(f.review_gaps(os.path.join(state, "reviews.jsonl"), [t], repo).get(t, []))))
PY
}
OUT7="$(c27 other-tip-handed)"
has "${OUT7#*|}" "Blocking finding id(s): rv-x#1"
check "C27 a handed fixed ruling at T1 does not clear rv-x#1 at T, whose own review never mentioned it: the fence refuses T" $? "out=$OUT7"
has "${OUT7%%|*}" "rv-x#1"
check "C27 the next reviewer of T is handed rv-x#1, as the fence still holds on it" $? "out=$OUT7"
OUT8="$(c27 same-tip-omitted)"
[ -z "${OUT8#*|}" ]
check "C28 a fixed ruling at T by a recheck of a recheck of the review of T that raised rv-x#1 clears it, though the review between omitted it" $? "out=$OUT8"
OUT8="$(c27 same-tip-unlinked)"
has "${OUT8#*|}" "Blocking finding id(s): rv-x#1"
check "C28 guard: a fixed ruling at T by a fresh review of T, not a recheck of the review that raised it, still blocks" $? "out=$OUT8"
OUT9="$(c27 range-key)"
has "${OUT9%%|*}" "rv-x#1"
check "C29 a review of T on a range key is handed rv-x#1, held at T under the app's work key" $? "out=$OUT9"
has "${OUT9#*|}" "Blocking finding id(s): rv-x#1"
check "C29 the fence still refuses T on rv-x#1 until a reviewer rules on it" $? "out=$OUT9"
OUT9="$(c27 range-key-fixed)"
[ -z "${OUT9#*|}" ]
check "C29 control: the range-key reviewer, handed rv-x#1, rules it fixed at T and the fence admits T" $? "out=$OUT9"

# --- C30 ---------------------------------------------------------------------
# The lead's ruling on esc-20261029T165526Z-fb650d7b (2026-10-29): the exact-tip rule of C27 holds
# only for findings raised in the land's own range. 6e9a03c3-f0d4#1 and #2 were raised and ruled
# fixed by an earlier work that landed (main c69f65d76); 5611a7964 carries them only through its
# merge of main. Fixture: base -> L1 -> L2 landed on main (review x at L1 raises #1, review f at L2,
# handed it, rules it fixed); branch N cut from base merges main as M, on the same work key. Review
# m of M never mentions #1, and the fence admits M.
REPO10="$SB/repo-landed-fixed"
G10() { git -C "$REPO10" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
mkdir -p "$REPO10"
G10 init -q -b main
printf 'base\n' >"$REPO10/a.txt"; G10 add -A; G10 commit -qm "base"
BASE10="$(G10 rev-parse HEAD)"
G10 checkout -qb cc/landed; printf 'base\nl1\n' >"$REPO10/a.txt"; G10 commit -qam "L1"
TIP10L1="$(G10 rev-parse HEAD)"
printf 'base\nl1\nl2\n' >"$REPO10/a.txt"; G10 commit -qam "L2"
TIP10L2="$(G10 rev-parse HEAD)"
G10 checkout -q main; G10 merge -q --ff-only cc/landed
G10 checkout -qb cc/new "$BASE10"; printf 'n\n' >"$REPO10/n.txt"; G10 add -A; G10 commit -qm "N1"
G10 merge -q --no-edit main
TIP10M="$(G10 rev-parse HEAD)"
G10 checkout -q main
OUT10="$(python3 - "$SCRIPT_DIR/lib" "$SB/c30" "$REPO10" "$TIP10L1" "$TIP10L2" "$TIP10M" <<'PY'
import json, os, sys
sys.path.insert(0, sys.argv[1])
state, repo, l1, l2, m = sys.argv[2:7]
os.makedirs(state)
os.environ["SECOND_REVIEW_STATE_DIR"] = state
import operator_fences as f
import second_review as sr
ident, work = f.review_repo_identity(repo), "work-app"
finding = {"priority": 2, "title": "Retry budget lost on continuation", "files": ["x.rs"], "evidence": "e",
           "fixture": "", "blocks": True}
fixed = [{"id": "rv-x#1", "status": "fixed", "blocks": False, "note": "x.rs:505 keeps the marking; probe confirms."}]
reviews = [("rv-x", l1, [finding], [], []), ("rv-f", l2, [], fixed, [dict(finding, id="rv-x#1")]),
           ("rv-m", m, [], [], [])]
with open(os.path.join(state, "reviews.jsonl"), "w") as ledger:
    for rid, tip, findings, rulings, given in reviews:
        os.makedirs(os.path.join(state, rid))
        verdict = "changes-requested" if findings else "passed"
        answer = {"reviewed_commit": tip, "verdict": verdict, "findings": findings, "earlier_findings": rulings}
        json.dump({"answer": answer, "earlier_findings_in": given}, open(os.path.join(state, rid, "verdict.json"), "w"))
        ledger.write(json.dumps({"id": rid, "tip": tip, "repo": repo, "repo_id": ident, "work": work,
                                 "trigger": "manual", "verdict": verdict, "recheck_of": "",
                                 "record": os.path.join(state, rid)}) + "\n")
handin = ",".join(fid for fid, _ in sr.earlier_findings(work, m, repo, ident))
print("%s|%s" % (handin, " ".join(f.review_gaps(os.path.join(state, "reviews.jsonl"), [m], repo).get(m, []))))
PY
)"
[ -z "${OUT10#*|}" ]
check "C30 a merge of main carrying a landed work's finding, ruled fixed only at the landed commit L2, is admitted" $? "out=$OUT10"
[ -z "${OUT10%%|*}" ]
check "C30 and the reviewer of the merge is not handed that landed finding" $? "out=$OUT10"

# --- C32 ---------------------------------------------------------------------
# A review written under a Work lease (2026-10-28): the app points CLAUDE_CONFIG_DIR at the
# account's folder (and RICHOS_CLAUDE_ACCOUNT_DIR at the same folder) and RICHOS_CLAUDE_HOME at the
# machine-wide home. The ledger is written in the machine-wide home, and every reader finds it
# there: the land rule, the land's gate, review-watch, the verdict delivery and its hook. integrate
# refused b20f108f because the writer and the land's reader had split. Without RICHOS_CLAUDE_HOME
# (a terminal) all of them follow CLAUDE_CONFIG_DIR, and so does a sandbox's own CLAUDE_CONFIG_DIR.
WL="$SB/work-lease"; MACHINE="$WL/home/.claude"; ACCOUNT="$WL/claude-accounts/2"
mkdir -p "$MACHINE" "$ACCOUNT"
OUT="$(env -u SECOND_REVIEW_STATE_DIR TMPDIR="$RUN_TMP" CLAUDE_CONFIG_DIR="$ACCOUNT" RICHOS_CLAUDE_ACCOUNT_DIR="$ACCOUNT" \
       RICHOS_CLAUDE_HOME="$MACHINE" FAKE_MODE=pass bash "$SR" --repo "$REPO" --tip "$TIP1" --base "$BASE" --words-file "$SB/codex-words.txt" 2>&1)"; RC=$?
[ "$RC" -eq 0 ] && [ "$(field "$(tail -1 "$MACHINE/state/reviews.jsonl" 2>/dev/null)" 'r["tip"]')" = "$TIP1" ] \
  && [ ! -e "$ACCOUNT/state/reviews.jsonl" ]
check "C32 a review under a Work lease writes its ledger row in RICHOS_CLAUDE_HOME's state, not the account's" $? \
  "rc=$RC out=$OUT"
readers() { # <expected state dir> <tip>: every reader of the ledger, in the caller's environment
    python3 - "$SCRIPT_DIR" "$1" "$2" <<'PY'
import importlib.util, os, subprocess, sys
scripts, state, tip = sys.argv[1:4]
sys.path.insert(0, os.path.join(scripts, "lib"))
import second_review, operator_fences, review_watch, review_delivery
spec = importlib.util.spec_from_file_location("ws_c26", os.path.join(scripts, "..", "mega-lander", "workspaces.py"))
ws = importlib.util.module_from_spec(spec); spec.loader.exec_module(ws)
lines = [l for l in open(os.path.join(scripts, "hooks", "deliver-review-verdict.sh"))
         if l.startswith(('case "${CLAUDE_CONFIG_DIR', "LEDGER="))]
assert len(lines) == 2, lines
hook = subprocess.run(["bash", "-uc", "".join(lines) + 'printf %s "$LEDGER"'], capture_output=True, text=True).stdout
want = os.path.join(state, "reviews.jsonl")
seen = {"second_review.ledger_path": second_review.ledger_path(),
        "operator_fences.review_ledger_default": operator_fences.review_ledger_default(),
        "review_watch.review_ledger": review_watch.review_ledger(),
        "review_delivery.ledger_path": review_delivery.ledger_path(),
        "deliver-review-verdict.sh LEDGER": hook}
bad = ["%s=%s" % kv for kv in seen.items() if kv[1] != want]
if tip and tip not in ws._reviews_by_tip():
    bad.append("workspaces._reviews_by_tip has no row for %s" % tip)
print("; ".join(bad))
PY
}
R="$(unset SECOND_REVIEW_STATE_DIR; export CLAUDE_CONFIG_DIR="$ACCOUNT" RICHOS_CLAUDE_ACCOUNT_DIR="$ACCOUNT" RICHOS_CLAUDE_HOME="$MACHINE"
      readers "$MACHINE/state" "$TIP1" 2>&1)"
[ -z "$R" ]
check "C32 under a Work lease every reader reads that row from RICHOS_CLAUDE_HOME's state" $? "$R"
R="$(unset SECOND_REVIEW_STATE_DIR RICHOS_CLAUDE_HOME; export CLAUDE_CONFIG_DIR="$ACCOUNT"
      readers "$ACCOUNT/state" "" 2>&1)"
[ -z "$R" ]
check "C32 without RICHOS_CLAUDE_HOME every reader and the writer follow CLAUDE_CONFIG_DIR, as before" $? "$R"
R="$(unset SECOND_REVIEW_STATE_DIR; export CLAUDE_CONFIG_DIR="$WL/sandbox" RICHOS_CLAUDE_ACCOUNT_DIR="$ACCOUNT" RICHOS_CLAUDE_HOME="$MACHINE"
      readers "$WL/sandbox/state" "" 2>&1)"
[ -z "$R" ]
check "C32 a sandbox inside a Work lease (its own CLAUDE_CONFIG_DIR) keeps its own ledger for every reader" $? "$R"

echo
echo "second-review.test.sh: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
