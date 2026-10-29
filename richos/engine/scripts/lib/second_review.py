#!/usr/bin/env python3
"""second_review.py: ONE SECOND REVIEW OF A TEAMMATE'S EXACT COMMITS, BY A DIFFERENT MODEL.

===========================================================================
WHAT THIS EXISTS FOR
===========================================================================
On 2026-10-08 the CEO fetched a Codex review of an agent's work by hand four
times (richos-hq docs/reviews/2026-10-08-dictation-slices-1-4/). Each one found
defects the author's own tests had passed; the first found two release
blockers in dictation slices 1-4 after they were already on main. His words,
ruling §113: "A regular RichOS user can never be expected anything even
remotely close to that. So, this all must be completely automated."

This is slice 1 of richos-hq docs/plans/2026-10-09-automatic-second-review-
and-t3-ideas.md: the command that does one review the way those four were
done. Slice 2's watcher starts it by itself; slice 3's land refuses work whose
tip has no passing verdict. Sage's check of that plan (...-sage-check.md) is
part of the spec, and every place below that follows one of his catches says
so.

===========================================================================
WHAT IT READS (plan §2.2): RECORDS, NEVER ANYONE'S SUMMARY
===========================================================================
  * THE ORIGINAL WORDS. For a teammate (--name): the brief exactly as it
    received it, the first message of its own transcript, which carries the
    CEO's verbatim sentence. Otherwise --words-file, read as bytes. A review
    with no original words is refused before any reviewer starts: a review
    against nobody's request can only check the code against itself.
  * THE EXACT COMMITS, base..tip. The tree at the tip is exported with
    `git archive` into this run's scratch folder (allocated by the wrapper with
    the engine's scratch.sh), with the commit log and the diff beside it. The
    author cannot move what is reviewed, and the reviewer never works in the
    author's workspace. A recheck judges only what changed since the newest
    earlier review of a commit this tip was built on (review_since): `git log
    -p --remerge-diff base..tip ^since`, so main's commits merged in between
    stay out (follow-up 12, D2, with Sage's F2 and F3). The whole tree is there.
  * WHAT THE AUTHOR CLAIMS: its commit messages, its last report (the last
    text it wrote in its transcript) and any --claims-file.
  * EARLIER REVIEWS OF THE SAME WORK, from the ledger below, so a recheck
    says of each earlier finding: fixed, still-open or withdrawn.
  * THE REPOSITORY'S OWN RULES: AGENTS.md and docs/development/verification-
    retries.md as they are at the tip, and the reviewer duty
    (mega-lander/duties/reviewer.md).

===========================================================================
WHICH MODEL (plan §2.3, Sage's catches 3, 4 and 10)
===========================================================================
  * Codex reviews Claude's work: `codex exec` from the CLI inside ChatGPT.app,
    looked up on every run because it moves (it is an alpha), its --version
    recorded. Pinned, never left to a default: model gpt-6.1-sol, reasoning
    effort high (what he reviewed with, from Codex Desktop's own session
    record of 2026-10-08), approvals never, sandbox workspace-write rooted at
    the export (-C), so a fixture compiles there and nothing else is written;
    --skip-git-repo-check because an archive has no .git; --output-schema for
    the one fixed answer shape.
  * INVISIBLE IN THE USER'S OWN CODEX AND CHATGPT APPS (the app tells users
    "this review process won't be visible in their regular ChatGPT/Codex
    app"): --ephemeral. Measured 2026-10-09: a review run without it added a
    row to ~/.codex/state_5.sqlite `threads` (source exec, titled with the
    review's prompt), a rollout under ~/.codex/sessions/ and thread history;
    an --ephemeral run added none of the three and used the same login. The
    model and effort every row records come from Codex's own
    SessionConfiguredEvent, which it logs on stderr at RUST_LOG=codex_exec=info
    (`model: "gpt-6.1-sol"`, `reasoning_effort: Some(High)`, `rollout_path:
    None`); the token count from its turn.completed events. His plan meter
    lived only in the session record, so it is no longer recorded.
  * ISOLATED FROM THE USER'S OWN SETUP (CODEX_ISOLATION, CEO 2026-10-09 ~05:00Z:
    macOS asked him whether "Terminal.app" may control "Codex Computer
    Use.app"). His ~/.codex/config.toml has a `notify` program that ran at
    every turn end and sent Apple Events from the Terminal the review started
    in, plus his MCP servers and plugins (a reviewer offered Computer Use and
    control of his Chrome). So Codex runs without his config.toml
    (--ignore-user-config; the login in CODEX_HOME is still used) or his
    exec-policy rules, `notify` empty, and plugins, apps, hooks, computer use
    and browser use off. Claude runs with no MCP server at all
    (--strict-mcp-config): no setting sources does not skip them.
  * Claude reviews Codex's work (a codex/ branch), and is the fallback when
    Codex cannot run (missing, signed out, at its usage limit: any exit without
    an answer that is not the time limit). The fallback is Opus, at high
    effort, in a fresh headless context with no setting sources (none of the
    lead's hooks or plugins), every write going through Bash, which Claude
    Code's OS sandbox confines to the export (the file-writing tools, which
    the sandbox does not confine, are removed), and --json-schema for the
    same answer shape. It respects his quota
    rule: quota-watch's own reading is taken first, and while a pause is in
    force (quota-watch --once exit 1) no Claude review starts.
  * A same-model reviewer is NOT refused (Sage's catch 3: what made the four
    reviews work was a fresh context, the original words and fixtures, not a
    different tier). Both models are recorded, so use can show whether it
    matters.

===========================================================================
HOW IT RUNS (Sage's catch 5)
===========================================================================
  * Admission by the engine's own CPU rule (cpu_policy.admission_open, 80% of
    total CPU), sampled from the kernel's host counters. No cap on how many
    reviews run at once: a review is mostly remote model time. A closed
    admission is waited on (bounded, one sample per 30 s, said once); an
    unmeasurable CPU refuses.
  * A 60-minute limit (the first by-hand review took about 30). Past it the
    reviewer is stopped by its own process id (SIGTERM, then SIGKILL).
  * The reviewer runs in this command's own process group, never a session of
    its own: review-watch starts second-review.sh leading a group and stops a
    review by signaling that group, so the reviewer ends with it whenever it
    was forked (second review of b5ff41f02, finding 1).
  * The reviewer's builds are capped at REVIEW_BUILD_JOBS jobs (CARGO_BUILD_JOBS,
    MAKEFLAGS, CMAKE_BUILD_PARALLEL_LEVEL in its environment, and said in its
    input). The first real runs, 2026-10-09: the engine's CPU breaker stopped a
    `rustc` the Claude reviewer started at 5.08 cores for 10 s.

===========================================================================
THE VERDICT (plan §2.2, §2.4)
===========================================================================
  * The answer must have the fixed shape; anything else is no verdict.
  * It must name the exact tip (the full SHA, or a prefix of at least 7);
    a verdict about another commit is refused.
  * The verdict follows the findings (ruling §116): every finding, and every
    earlier finding still open, says whether it blocks. It blocks only when a
    user could hit it in normal use AND it would matter to them (CEO
    2026-10-10: "why is there a need for a change if it 'doesn't matter'?")
    AND either it breaks what this work promises, or this work broke something
    that worked before; a hole that was already there, outside what this work
    promises, is a follow-up however real. That is §116's sort ("a user could
    hit it in normal use and it breaks what the work promises"), which
    282530b2b had widened to any hole in the app; follow-up 12 (D3) restored
    it in the words of Sage's check (F4). How text renders somewhere other
    than the app, a missing or weak test, wording and anything only a
    developer would see never block.
    The author's claims are what the reviewer checks against, never extra
    promises. Changes-requested exactly when at least one blocks; passed
    otherwise, whatever the reviewer wrote. Priority is information only.
  * A passed review files each finding it did not block on, new or still open
    from earlier, in <state>/review-follow-ups.jsonl, once; the stdout line
    says how many and where.
  * THE LEAD'S RULING (§116: "Rich applies the same test"; richos-hq
    docs/plans/2026-10-18-review-rule-redesign.md D1, with Sage's check):
    `--follow-up <review-id>#<n> --why '...'` files a blocking finding as a
    follow-up in <state>/review-deferrals.jsonl and review-follow-ups.jsonl.
    The gate (operator_fences.review_of) then stops holding the work for it at
    every later commit, and each later reviewer of a commit built on that
    review is told it is already filed. `--reopen` undoes it.
  * One row per review in <state>/reviews.jsonl and the full text, answer and
    fixtures in <state>/reviews/<id>/ (<state> is ~/.claude/state, outside
    every repository and session, like the escalation ledger). One line on
    stdout says the verdict.

Set by the app's review-watch: SECOND_REVIEW_CODEX_PATH, the PATH Codex is looked for on
after ChatGPT.app (find_codex).

Test seams (second-review.test.sh only): SECOND_REVIEW_STATE_DIR,
SECOND_REVIEW_CODEX, SECOND_REVIEW_CLAUDE,
SECOND_REVIEW_QUOTA_CMD, SECOND_REVIEW_CPU_BUSY, SECOND_REVIEW_TIMEOUT_SECONDS.
"""

import argparse
import contextlib
import ctypes
import fcntl
import hashlib
import importlib.util
import json
import os
import re
import secrets
import shutil
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import cpu_policy  # noqa: E402  (sibling in scripts/lib: the engine's CPU admission rule)
import instruction_files  # noqa: E402  (sibling: the repository's own CLAUDE.md or AGENTS.md, by Claude Code's rule)

CODEX_MODEL = "gpt-6.1-sol"
CODEX_EFFORT = "high"
CLAUDE_MODEL = "opus"
CLAUDE_EFFORT = "high"
LIMIT_SECONDS = 60 * 60
ADMISSION_WAIT_SECONDS = 30 * 60
ADMISSION_SAMPLE_SECONDS = 30
CODEX_APP_ROOTS = ("/Applications/ChatGPT.app", "~/Applications/ChatGPT.app")
# The repository's own instruction file is not listed here: `instruction_files` chooses it by
# Claude Code's rule, the same file the worker was given, and it is inlined in full.
RULE_FILES = ("docs/development/verification-retries.md",)
MAX_INLINE_RULE_BYTES = 64 * 1024
MAX_INLINE_LOG_BYTES = 128 * 1024
FIXTURE_FILE_BYTES = 1024 * 1024
FIXTURE_TOTAL_BYTES = 20 * 1024 * 1024
REVIEW_BUILD_JOBS = 2
_REVIEWER = {"proc": None}

EXIT_PASSED, EXIT_CHANGES, EXIT_NO_VERDICT, EXIT_REFUSED = 0, 1, 2, 64

SCHEMA = {
    "type": "object",
    "additionalProperties": False,
    "required": ["reviewed_commit", "verdict", "summary", "checks", "findings", "earlier_findings",
                 "not_yet_claimed"],
    "properties": {
        "reviewed_commit": {"type": "string"},
        "verdict": {"type": "string", "enum": ["passed", "changes-requested"]},
        "summary": {"type": "string"},
        "checks": {"type": "array", "items": {"type": "string"}},
        "findings": {"type": "array", "items": {
            "type": "object", "additionalProperties": False,
            "required": ["priority", "blocks", "title", "files", "evidence", "fixture"],
            "properties": {
                "priority": {"type": "integer", "enum": [1, 2, 3]},
                "blocks": {"type": "boolean"},
                "title": {"type": "string"},
                "files": {"type": "array", "items": {"type": "string"}},
                "evidence": {"type": "string"},
                "fixture": {"type": "string"}}}},
        "earlier_findings": {"type": "array", "items": {
            "type": "object", "additionalProperties": False,
            "required": ["id", "status", "blocks", "note"],
            "properties": {
                "id": {"type": "string"},
                "status": {"type": "string", "enum": ["fixed", "still-open", "withdrawn"]},
                "blocks": {"type": "boolean"},
                "note": {"type": "string"}}}},
        "not_yet_claimed": {"type": "array", "items": {"type": "string"}},
    },
}


class Refused(Exception):
    """Refused before any reviewer ran (exit 64)."""


# ---------------------------------------------------------------------------
# small helpers
# ---------------------------------------------------------------------------

def iso(t=None):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() if t is None else t))


def git(repo, *args, check=True):
    p = subprocess.run(["git", "-C", repo] + list(args), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if check and p.returncode != 0:
        raise Refused("git %s failed in %s: %s" % (" ".join(args), repo,
                                                  p.stderr.decode("utf-8", "replace").strip()))
    return p.stdout.decode("utf-8", "replace")


def state_root():
    d = (os.environ.get("SECOND_REVIEW_STATE_DIR") or "").strip()
    if d:
        return d
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "state")


def ledger_path():
    return os.path.join(state_root(), "reviews.jsonl")


def follow_ups_path():
    return os.path.join(state_root(), "review-follow-ups.jsonl")


def deferrals_path():
    return os.path.join(state_root(), "review-deferrals.jsonl")


def read_ledger():
    rows = []
    try:
        with open(ledger_path(), encoding="utf-8") as f:
            for line in f:
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if isinstance(r, dict):
                    rows.append(r)
    except OSError:
        pass
    return rows


def append_row(row, path=None):
    """One row (or a list of rows, in one write) appended under a lock."""
    os.makedirs(state_root(), exist_ok=True)
    rows = row if isinstance(row, list) else [row]
    data = "".join(json.dumps(r, sort_keys=True) + "\n" for r in rows).encode("utf-8")
    fd = os.open(path or ledger_path(), os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        os.write(fd, data)
    finally:
        os.close(fd)


def _env_seconds(name, default):
    raw = (os.environ.get(name) or "").strip()
    if not raw:
        return default
    try:
        return max(0.0, float(raw))
    except ValueError:
        return default


# ---------------------------------------------------------------------------
# the teammate: registry record and transcript
# ---------------------------------------------------------------------------

def load_workspaces(engine_root):
    path = os.path.join(engine_root, "mega-lander", "workspaces.py")
    spec = importlib.util.spec_from_file_location("second_review_workspaces", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def find_teammate(ws, name):
    """The newest registry record with this name, finished or not. Read only."""
    hits = [r for r in ws.all_agents(include_done=True) if r.get("name") == name or r.get("key") == name]
    if not hits:
        raise Refused("no registered teammate %r" % name)
    return sorted(hits, key=lambda r: str(r.get("registered_at") or ""))[-1]


def teammate_workspace(rec, repo):
    live = [w for w in rec.get("workspaces") or [] if w.get("branch")]
    if repo:
        real = os.path.realpath(repo)
        live = [w for w in live if os.path.realpath(w.get("repo") or "") == real
                or os.path.realpath(w.get("path") or "") == real]
    if not live:
        raise Refused("teammate %r has no workspace%s" % (rec.get("name"), " in " + repo if repo else ""))
    cc = [w for w in live if w.get("kind") == "cc"]
    return (cc or live)[0]


def _text_of(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(b.get("text") or "" for b in content if isinstance(b, dict) and b.get("type") == "text")
    return ""


def transcript_facts(path):
    """(first brief verbatim, model of its first answer, last report text)."""
    words, model, last_id, last_text = None, "", None, {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            try:
                o = json.loads(line)
            except ValueError:
                continue
            m = o.get("message") or {}
            if o.get("type") == "user" and words is None and not o.get("isMeta"):
                words = m.get("content") if isinstance(m.get("content"), str) else _text_of(m.get("content"))
            elif o.get("type") == "assistant":
                model = model or str(m.get("model") or "")
                text = _text_of(m.get("content"))
                if text.strip():
                    mid = m.get("id") or id(o)
                    if mid != last_id:
                        last_id, last_text = mid, {}
                    last_text[len(last_text)] = text
    report = "".join(last_text[k] for k in sorted(last_text)) if last_text else ""
    return words, model, report


def meta_model(transcript):
    try:
        with open(transcript[:-len(".jsonl")] + ".meta.json", encoding="utf-8") as f:
            return str(json.load(f).get("model") or "")
    except (OSError, ValueError, AttributeError):
        return ""


# ---------------------------------------------------------------------------
# the work: what is reviewed
# ---------------------------------------------------------------------------

class Work(object):
    def __init__(self):
        self.repo = ""
        self.branch = ""
        self.base = ""
        self.tip = ""
        self.author = ""
        self.author_model = ""
        self.words = []          # [(label, text)]
        self.claims = []         # [(label, text)]
        self.work_key = ""
        self.workspace = ""


def resolve_work(a):
    w = Work()
    if a.name:
        ws = load_workspaces(a.engine_root)
        rec = find_teammate(ws, a.name)
        space = teammate_workspace(rec, a.repo)
        w.repo = os.path.realpath(space.get("repo") or a.repo)
        w.workspace = os.path.realpath(space.get("path") or "")
        w.branch = a.branch or str(space.get("branch") or "")
        w.author = str(rec.get("name") or a.name)
        w.work_key = ws.recovery_work_key(rec)
        transcript = ws.platform_agent_transcript(rec)
        if transcript:
            words, model, report = transcript_facts(transcript)
            if words:
                w.words.append(("the brief exactly as %s received it (the first message of its transcript, %s)"
                                % (w.author, transcript), words))
            if report:
                w.claims.append(("%s's last report (the last text in its transcript)" % w.author, report))
            w.author_model = meta_model(transcript) or model
    else:
        if not a.repo:
            raise Refused("name a teammate (--name) or a repository (--repo)")
        w.repo = os.path.realpath(a.repo)
        w.branch = a.branch or ""
    if a.author_model:
        w.author_model = a.author_model
    if a.tip:
        w.tip = git(w.repo, "rev-parse", "--verify", a.tip + "^{commit}").strip()
    elif w.branch:
        w.tip = git(w.repo, "rev-parse", "--verify", "refs/heads/%s^{commit}" % w.branch).strip()
    else:
        raise Refused("name the commit to review (--tip or --branch)")
    if a.base:
        w.base = git(w.repo, "rev-parse", "--verify", a.base + "^{commit}").strip()
    else:
        w.base = git(w.repo, "merge-base", a.integration, w.tip).strip()
    if w.base == w.tip:
        raise Refused("nothing to review: the tip %s is the base" % w.tip[:12])
    if subprocess.run(["git", "-C", w.repo, "merge-base", "--is-ancestor", w.base, w.tip]).returncode != 0:
        raise Refused("the base %s is not an ancestor of the tip %s" % (w.base[:12], w.tip[:12]))
    codex_author = w.branch.startswith("codex/") or (a.author or "").lower() == "codex" \
        or w.author_model.lower().startswith(("gpt", "codex"))
    if codex_author:
        w.author = "codex"
        w.author_model = w.author_model or "codex"
    if a.author:
        w.author = a.author
    w.author = w.author or "unknown"
    # Given words come FIRST: in the app they are the user's own turn, and the transcript's first
    # message is the coordinator's brief to the worker (plan §2.2: "the user's own turn, then
    # Stu's brief"). Without --name there are no transcript words, so nothing else moves.
    given = []
    for path in a.words_file or []:
        with open(path, "rb") as f:
            given.append(("%s, as given" % os.path.abspath(path), f.read().decode("utf-8", "replace")))
    w.words = given + w.words
    for path in a.claims_file or []:
        with open(path, "rb") as f:
            w.claims.append(("%s, as given" % os.path.abspath(path), f.read().decode("utf-8", "replace")))
    if not any(t.strip() for _, t in w.words):
        raise Refused("no original words: a review needs the request the work answers (the teammate's own "
                      "transcript through --name, or --words-file); refused before any reviewer started")
    if a.name and a.work and a.work != w.work_key:
        raise Refused("--work differs from the host-bound recovery identity %s" % w.work_key)
    w.work_key = a.work or w.work_key or ("branch:%s:%s" % (w.repo, w.branch) if w.branch
                                          else "range:%s:%s" % (w.repo, w.base))
    return w


def same_repo(row, repo, repo_id):
    """Whether a verdict row is of this repository: by the identity both recorded
    (`repo_id`) when both have one, otherwise by the repository's real path."""
    if row.get("repo_id") and repo_id:
        return row["repo_id"] == repo_id
    return os.path.realpath(row.get("repo") or "") == os.path.realpath(repo or "")


def earlier_findings(work_key, tip, repo, repo_id=""):
    """The findings still open from the newest earlier verdict on this work in
    THIS repository: its own findings, plus those it reported still-open.
    [(id, finding)] One teammate's workspaces in several repositories share a
    work key, so the history is taken per repository: a review is only ever
    asked about, and only ever files, findings of the repository it reviews
    (review rv-20261009T104053Z-8662354d-47b2). A finding a passed review
    already filed as a follow-up carries "filed": true, so a later pass never
    files it twice.

    The reviews read are the ones the land fence reads for `tip`
    (operator_fences.review_history), plus this work key's: only reviews of
    `tip` or a commit it was built on count, so a review of an unrelated tip on
    the same work key that withdrew a finding no longer hides it (it kept the
    reviewers of 5611a7964 from being asked about
    rv-20261029T140028Z-3e883526-10b9#1, 2026-10-29); and every work key with a
    land review of `tip` counts, so a manual --recheck on its own range key is
    handed the findings held under the app's key."""
    import operator_fences as fences
    rows = fences.review_history([r for r in read_ledger() if r.get("verdict") and same_repo(r, repo, repo_id)],
                                 tip, repo, [work_key])
    # The gate and the next reviewer see the same unresolved findings, including
    # conflicting duplicate reviews. Never carry only the last writer's subset.
    return list(fences.open_review_findings(rows, tip, repo).items())


def is_ancestor(repo, older, newer):
    return subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor", older, newer],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0


def review_since(w, repo_id):
    """(since, its review id): the tip of the newest handover or by-hand verdict in
    THIS repository whose tip is one of this work's own commits before this one
    (in base..tip, so never a commit main already holds), or ("", "") when there
    is none. A recheck reviews only what changed after it (follow-up 12, D2): the
    range before it already has a verdict, and its open findings carry by id.
    Chosen by commit ancestry, never by work key (Sage's check, F2)."""
    p = subprocess.run(["git", "-C", w.repo, "rev-list", "%s..%s" % (w.base, w.tip)],
                       stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    own = set(p.stdout.decode("ascii", "replace").split()) if p.returncode == 0 else set()
    own.discard(w.tip)
    since = None
    for r in read_ledger():
        if r.get("verdict") and r.get("trigger") in ("handover", "manual") and r.get("tip") in own \
                and same_repo(r, w.repo, repo_id):
            since = r
    return (since["tip"], since["id"]) if since else ("", "")


def filed_findings(repo, repo_id, tip):
    """(the findings the lander filed as follow-ups whose review was of `tip` or a
    commit it was built on, in THIS repository: [(id, ruling)], every filed id of
    this repository). Linked by commit ancestry, never by work key: a by-hand
    review's key is its base, so it never shares one with the handovers of the
    same branch (Sage's check of the review rule redesign, F2)."""
    import operator_fences as fences
    mine = {fid: d for fid, d in fences.read_deferrals(ledger_path()).items() if same_repo(d, repo, repo_id)}
    return ([(fid, d) for fid, d in mine.items() if d.get("tip") and is_ancestor(repo, d["tip"], tip)],
            set(mine))


def follow_up(finding, why, reopen=False):
    """`--follow-up <review-id>#<n> --why '...'`: the lander rules a blocking
    finding a follow-up (ruling §116: "Rich applies the same test to any finding
    the reviewer has not sorted"), and the gate stops holding work for it at
    every later commit of the same work. `--reopen` undoes it; the later row wins.

    Only the lead rules: refused for a teammate's call (agent_hold.py exports
    RICHOS_AGENT_OWNER into every subagent's shell) and for a caller with no
    Claude session in its ancestry (Codex), so an author never files its own
    finding. No land lease is needed or taken: a ruling writes no checkout."""
    import operator_fences as fences
    owner = (os.environ.get("RICHOS_AGENT_OWNER") or "").strip()
    who = fences.claude_session()
    if owner:
        raise Refused("only the lead rules on a finding; this is a teammate's call (RICHOS_AGENT_OWNER=%s)" % owner)
    if not who or "error" in who:
        raise Refused("only the lead rules on a finding; no Claude session runs this command%s"
                      % (" (%s)" % who["error"] if who else ""))
    rid, sep, n = finding.partition("#")
    if not sep or not rid or not n:
        raise Refused("name the finding as <review-id>#<n>, as the refusal lists it")
    rows = read_ledger()
    at = next((i for i, r in enumerate(rows) if r.get("id") == rid and r.get("verdict")), None)
    if at is None:
        raise Refused("no review %s with a verdict in %s" % (rid, ledger_path()))
    source = rows[at]
    current = fences.read_deferrals(ledger_path()).get(finding)
    if reopen:
        if not current:
            raise Refused("%s is not filed as a follow-up; there is nothing to reopen" % finding)
        f = current
    else:
        if not (why or "").strip():
            raise Refused("--why is required: who rules, and why")
        if current:
            raise Refused("%s is already filed as a follow-up (%s: %s)" % (finding, current.get("at"), current.get("why")))
        repo_id = source.get("repo_id") or ""
        # The finding as the reviews of its repository left it: the review that raised it and every
        # later one that was asked about it, of its own tip or a commit built on it (open_review_findings,
        # the gate's own reading; review_of keeps only the land tip's own history). A review of an
        # unrelated tip on the same work key never closes it here, as it never does at the fence
        # (2026-10-23: rv-20261023T091432Z-e14ecb3f-a67e#1 was refused here while the fence held on it).
        history = [r for r in rows[at:] if same_repo(r, source.get("repo"), repo_id) and r.get("verdict")
                   and (r.get("id") == rid or (fences._own_history(repo_id or source.get("repo"), source.get("tip"),
                                                                  r.get("tip"))
                                               and any(e.get("id") == finding for e in
                                                       fences.review_record(r).get("earlier_findings_in") or [])))]
        f = fences.open_review_findings(history).get(finding)
        if not f or f.get("blocks") is False:
            raise Refused("%s is not an open blocking finding in %s, so it holds no land" % (finding, ledger_path()))
    row = {"at": iso(), "action": "reopen" if reopen else "follow-up", "finding": finding, "review": rid,
           "repo": source.get("repo"), "repo_id": source.get("repo_id") or repo_identity(source.get("repo")),
           "branch": source.get("branch") or "", "tip": source.get("tip"), "work": source.get("work") or "",
           "priority": f.get("priority"), "title": f.get("title"), "files": f.get("files") or [],
           "why": why or "", "who": who}
    append_row(row, deferrals_path())
    if reopen:
        print("SECOND-REVIEW %s reopened: it blocks again. Record: %s" % (finding, deferrals_path()))
        return 0
    append_row({"at": row["at"], "review": rid, "finding": finding, "repo": row["repo"], "branch": row["branch"],
                "tip": row["tip"], "work": row["work"], "priority": f.get("priority"), "title": f.get("title"),
                "files": f.get("files") or [], "evidence": f.get("evidence"), "fixture": f.get("fixture") or "",
                "filed_by": "lander", "why": why}, follow_ups_path())
    print("SECOND-REVIEW %s filed as a follow-up: it no longer holds a land. Records: %s, %s"
          % (finding, deferrals_path(), follow_ups_path()))
    return 0


# ---------------------------------------------------------------------------
# the export and the input
# ---------------------------------------------------------------------------

def export_tree(repo, tip, dest):
    os.makedirs(dest)
    arch = subprocess.Popen(["git", "-C", repo, "archive", "--format=tar", tip], stdout=subprocess.PIPE)
    tar = subprocess.run(["tar", "-x", "-f", "-", "-C", dest], stdin=arch.stdout)
    arch.stdout.close()
    if arch.wait() != 0 or tar.returncode != 0:
        raise Refused("could not export %s from %s" % (tip, repo))


def read_rule(tree, rel):
    p = os.path.join(tree, rel)
    try:
        with open(p, "rb") as f:
            data = f.read(MAX_INLINE_RULE_BYTES + 1)
    except OSError:
        return None
    text = data[:MAX_INLINE_RULE_BYTES].decode("utf-8", "replace")
    if len(data) > MAX_INLINE_RULE_BYTES:
        text += "\n[... cut at %d bytes; the whole file is tree/%s]\n" % (MAX_INLINE_RULE_BYTES, rel)
    return text


def fence(label, text):
    """Verbatim text between two marker lines that cannot occur inside it."""
    tag = "VERBATIM-" + secrets.token_hex(4).upper()
    return "<<<%s %s\n%s%s\n%s>>>\n" % (tag, label, text, "" if text.endswith("\n") else "\n", tag)


def build_input(w, a, export, earlier):
    review = os.path.join(export, "review")
    os.makedirs(review)
    os.makedirs(os.path.join(export, "fixtures"))
    os.makedirs(os.path.join(export, "build"))
    since = getattr(w, "since", "")
    span = ["%s..%s" % (w.base, w.tip)] + (["^" + since] if since else [])
    if since:
        # Only the work's own commits after the last review, each with its patch; a merge from main
        # shows only how its conflicts were resolved (Sage's check, F3). Falls back to the whole
        # base..tip when Git cannot make it.
        with open(os.path.join(review, "diff.patch"), "wb") as f:
            made = subprocess.run(["git", "-C", w.repo, "log", "-p", "--remerge-diff", "--no-color", "--no-ext-diff",
                                   "--format=commit %H%n%n%B"] + span, stdout=f, stderr=subprocess.DEVNULL)
        if made.returncode != 0:
            w.since = w.since_review = since = ""
            span = span[:1]
    if not since:
        with open(os.path.join(review, "diff.patch"), "wb") as f:
            subprocess.run(["git", "-C", w.repo, "diff", "--no-color", "--no-ext-diff", w.base, w.tip], stdout=f,
                           check=False)
    log = git(w.repo, "log", "--format=commit %H%nAuthor: %an%nDate: %cI%n%n%B%n", *span)
    with open(os.path.join(review, "commits.txt"), "w", encoding="utf-8") as f:
        f.write(log)
    stat = (git(w.repo, "log", "--format=%h %s", "--stat", "--remerge-diff", "--no-color", *span) if since
            else git(w.repo, "diff", "--stat", "--no-color", w.base, w.tip))
    tree = os.path.join(export, "tree")
    mid_job = a.trigger in ("long-job", "quiet")
    duty = ""
    try:
        with open(os.path.join(a.engine_root, "mega-lander", "duties", "reviewer.md"), encoding="utf-8") as f:
            duty = f.read()
    except OSError:
        pass

    parts = []
    parts.append("# Second review: %s at %s\n\n" % (os.path.basename(w.repo), w.tip[:12]))
    parts.append("You are an independent reviewer, a different model from the one that wrote this work. Review "
                 "the exact commits below against the original words, and find what the author's own tests miss.\n\n")
    parts.append("## The commits you review\n\nTIP: %s\nBASE: %s\nREPOSITORY: %s\nBRANCH: %s\nAUTHOR: %s (model: %s)\n"
                 "TRIGGER: %s\n\n" % (w.tip, w.base, w.repo, w.branch or "(none)", w.author,
                                      w.author_model or "unknown", a.trigger))
    if since:
        parts.append("SINCE: %s. Review %s already covered BASE..SINCE, and its open findings are listed below. "
                     "You judge only what changed after it: the commits in `review/commits.txt` and "
                     "`review/diff.patch`. The whole tree at the tip is there to read.\n\n"
                     % (since, getattr(w, "since_review", "") or "(unknown)"))
        what = ("every commit of this work after SINCE, with its full message",
                "each of those commits with its patch (a merge from main shows only its conflict resolution)")
    else:
        what = ("every commit from base to tip, with its full message", "the full diff from base to tip")
    parts.append("Your working folder is `%s`. In it:\n"
                 "- `tree/`: the whole repository exactly as it is at the tip (a `git archive`; there is no `.git`).\n"
                 "- `review/commits.txt`: %s.\n"
                 "- `review/diff.patch`: %s.\n"
                 "- `fixtures/`: where any fixture you write goes. Its small files are kept with your verdict.\n"
                 "- `build/`: where build output goes (for example `CARGO_TARGET_DIR=build/target`). Not kept.\n\n"
                 "The change at a glance:\n\n```\n%s```\n\n" % (export, what[0], what[1], stat))
    parts.append("## The original words\n\nThese are the request the work answers, verbatim. Judge the work "
                 "against them, not against the author's account of them.\n\n")
    for label, text in w.words:
        parts.append(fence(label, text) + "\n")
    parts.append("## What the author claims\n\n")
    msgs = log if len(log) <= MAX_INLINE_LOG_BYTES else (
        log[:MAX_INLINE_LOG_BYTES] + "\n[... cut at %d characters; every commit is in review/commits.txt]\n"
        % MAX_INLINE_LOG_BYTES)
    parts.append(fence("its commit messages, %s (also review/commits.txt)" % ("after SINCE" if since else "base to tip"),
                       msgs) + "\n")
    for label, text in w.claims:
        parts.append(fence(label, text) + "\n")
    if not w.claims:
        parts.append("(No report beyond the commit messages.)\n\n")
    parts.append("Check the claims: a claimed test, receipt or result is evidence only once you have seen it "
                 "hold in the code at the tip.\n\n")
    parts.append("## Earlier reviews of this work\n\n")
    if earlier:
        parts.append("Say of EACH of these, in `earlier_findings`, by its id: `fixed`, `still-open` or "
                     "`withdrawn`, with a one-line note, and whether it blocks at this tip (`blocks`, by the test "
                     "below; false unless it is still open). Do not list it again in `findings`.\n\n")
        for fid, f in earlier:
            parts.append("- **%s** [P%s%s] %s (%s): %s\n" % (
                fid, f.get("priority"), "; already on the follow-up list" if f.get("filed") else "",
                f.get("title"), ", ".join(f.get("files") or []), f.get("evidence")))
        parts.append("\n")
    else:
        parts.append("None. `earlier_findings` is an empty list.\n\n")
    filed = getattr(w, "filed", None) or []
    if filed:
        parts.append("## Already filed as follow-ups\n\nThe lander ruled these follow-ups. Do not report them "
                     "again, in `findings` or in `earlier_findings`.\n\n")
        for fid, d in filed:
            parts.append("- `%s` (P%s) %s (%s). Why: %s\n" % (fid, d.get("priority"), d.get("title"),
                                                            ", ".join(d.get("files") or []), d.get("why")))
        parts.append("\n")
    parts.append("## The repository's own instructions (as they are at the tip)\n\n")
    # The same instruction file, by the same rule, as the worker's brief (prepare in
    # mega-lander/app.py; richos-hq docs/plans/2026-10-21-folder-instruction-files.md §5.3 item 2),
    # read at the tip: in full, never cut. Read from the repository at the tip, never from the
    # export: a link out of the repository (CLAUDE.md -> ../shared/CLAUDE.md) points nowhere
    # inside the export, and the same call as prepare's resolves every link the same way.
    for rel, text, where in instruction_files.for_repository(w.repo, w.tip):
        label = ("tree/" + rel if where == "commit"
                 else "%s, from the connected folder itself (not committed, so not in tree/)" % os.path.join(w.repo, rel))
        parts.append(fence(label, text) + "\n")
    for rel in RULE_FILES:
        text = read_rule(tree, rel)
        if text is not None:
            parts.append(fence("tree/" + rel, text) + "\n")
    if duty:
        parts.append("## Your duty\n\n" + fence("the reviewer duty (mega-lander/duties/reviewer.md)", duty) + "\n")
    parts.append(
        "## How this review runs (where the duty above differs, this wins)\n\n"
        "- There is no assigned worktree and no `RICHOS_REVIEW` line: your working folder above is the target, "
        "and your final answer is the fixed JSON shape you were given.\n"
        "- Find what the author's tests miss. Where you can, prove a finding with a small fixture: write it "
        "under `fixtures/`, run it, and quote its output in the finding's evidence. Name the fixture's path in "
        "`fixture` (empty when there is none).\n"
        "- Every finding says whether it blocks (`blocks`). It blocks only when a user could hit it in normal use "
        "AND it would matter to them AND either it breaks what this work promises (the original words above), or "
        "this work broke something that worked before; a hole that was already there, outside what this work "
        "promises, is a follow-up however real. These never block, however they sound: how text "
        "renders somewhere other than the app (GitHub, a terminal, a Markdown viewer), a missing or weak test, "
        "wording, and anything only a developer would see; nor anything that needs an unusual setup, a "
        "hand-edited file, a deliberately hostile local process or a rare race. A finding that does not block "
        "is filed as a follow-up automatically when the review passes.\n"
        "- The author's claims above are what you check the work against, never extra promises: a difference "
        "from a claim blocks only when it passes the same test.\n"
        "- The verdict follows the findings: changes-requested exactly when at least one finding, or one "
        "earlier finding still open, blocks; passed otherwise, whatever `verdict` says. A claim you could not "
        "see hold is a finding, judged by the same test.\n"
        "- `priority` is information only (1 the most serious, 3 a note); it never decides the verdict. Give a "
        "title, file:line locations (paths as in `tree/`) and evidence.\n"
        "- Never edit `tree/` to change the work, never commit, never touch the author's workspace (%s). "
        "Run no test VM, no app, no microphone, no phone, and no broad suite rerun; never touch the host "
        "display, sleep, lock or input.\n"
        "- `reviewed_commit` is the full TIP above. `checks` lists only the checks you actually ran.\n"
        "- Builds run with at most %d jobs (CARGO_BUILD_JOBS, MAKEFLAGS and CMAKE_BUILD_PARALLEL_LEVEL are set "
        "so); never raise them: the Mac is shared, and its CPU breaker stops a build that takes more.\n"
        % (w.workspace or w.repo, REVIEW_BUILD_JOBS))
    if mid_job:
        parts.append("- This work is still in progress. Review only what the author's commits and last report "
                     "claim is done at this tip; list what is not yet claimed in `not_yet_claimed`, never as "
                     "findings.\n")
    else:
        parts.append("- The author has handed this work over. `not_yet_claimed` lists anything the original words "
                     "ask for that the work does not claim at all.\n")
    prompt = "".join(parts)
    with open(os.path.join(review, "prompt.md"), "w", encoding="utf-8") as f:
        f.write(prompt)
    return prompt


# ---------------------------------------------------------------------------
# admission and the quota hold
# ---------------------------------------------------------------------------

def _host_ticks():
    if sys.platform != "darwin":
        return None
    lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    lib.mach_host_self.restype = ctypes.c_uint
    ticks = (ctypes.c_uint * 4)()
    count = ctypes.c_uint(4)
    if lib.host_statistics(lib.mach_host_self(), 3, ctypes.byref(ticks), ctypes.byref(count)):
        return None
    return list(ticks)


def cpu_busy():
    """Total CPU percent over one second (user + system + nice), or None."""
    pinned = (os.environ.get("SECOND_REVIEW_CPU_BUSY") or "").strip()
    if pinned:
        try:
            return float(pinned)
        except ValueError:
            return None
    before = _host_ticks()
    time.sleep(1.0)
    after = _host_ticks()
    if before is None or after is None:
        return None
    delta = [(b - a) % 2 ** 32 for a, b in zip(before, after)]
    total = sum(delta)
    return 100.0 * (1 - delta[2] / total) if total else None


def admit(wait_seconds, say):
    """(admitted, why). The engine's CPU rule alone; no count cap."""
    deadline = time.monotonic() + wait_seconds
    told = False
    while True:
        busy = cpu_busy()
        if busy is None:
            return False, "the CPU could not be measured, so the review was not admitted"
        if cpu_policy.admission_open(busy):
            return True, "CPU %.0f%% at admission" % busy
        if time.monotonic() >= deadline:
            return False, ("CPU admission stayed closed (%.0f%% busy, the rule admits below %d%%) for %d s"
                           % (busy, cpu_policy.DEFAULT_MAX_CPU, wait_seconds))
        if not told:
            say("second-review: waiting for CPU admission (%.0f%% busy; admits below %d%%), up to %d s"
                % (busy, cpu_policy.DEFAULT_MAX_CPU, wait_seconds))
            told = True
        time.sleep(min(ADMISSION_SAMPLE_SECONDS, max(0.0, deadline - time.monotonic())))


def quota_hold(engine_root):
    """(held, reading). His 93% rule, read through quota-watch's own --once."""
    cmd = (os.environ.get("SECOND_REVIEW_QUOTA_CMD") or "").strip()
    argv = [cmd] if cmd else ["bash", os.path.join(engine_root, "scripts", "quota-watch.sh"), "--once"]
    try:
        p = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=90)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return False, "quota reading unavailable (%s)" % exc.__class__.__name__
    text = p.stdout.decode("utf-8", "replace").strip().splitlines()
    return p.returncode == 1, (text[0] if text else "quota-watch exit %d" % p.returncode)


# ---------------------------------------------------------------------------
# the reviewers
# ---------------------------------------------------------------------------

def find_codex():
    pinned = os.environ.get("SECOND_REVIEW_CODEX")
    if pinned is not None:
        return pinned if os.path.isfile(pinned) and os.access(pinned, os.X_OK) else ""
    for root in CODEX_APP_ROOTS:
        base = os.path.join(os.path.expanduser(root), "Contents")
        if not os.path.isdir(base):
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            if dirpath[len(base):].count(os.sep) > 7:
                dirnames[:] = []
                continue
            dirnames[:] = [d for d in dirnames if d not in ("Frameworks", "_CodeSignature", "node_modules")]
            if "codex" in filenames and os.path.basename(dirpath) == "MacOS":
                p = os.path.join(dirpath, "codex")
                if os.access(p, os.X_OK):
                    return p
    # The app's watcher sets SECOND_REVIEW_CODEX_PATH to the PATH its Settings row looks for Codex on
    # (the app's own; richos-core codex_reviews.rs search_path), since this process's PATH is the
    # delivered runtime's and leaves out global installs: so a review finds the Codex the row shows
    # (the real second review of cf3c4482f). Unset, as for the operator's reviews, it is this PATH.
    return shutil.which("codex", path=os.environ.get("SECOND_REVIEW_CODEX_PATH")) or ""


def codex_signed_in(codex):
    """(signed in, what Codex said): Codex's own answer to `codex login status`, which reads its login
    and starts none. Measured 2026-10-09 on codex-cli 0.162.0-alpha.2: exit 0 with "Logged in using
    ChatGPT" on stderr when signed in, exit 1 with "Not logged in" when not. Asked before a Codex review
    so a signed-out Codex is never run (the app's switch: Claude reviews that one, round 20.2); the
    app's Settings row reads the same command (richos-core codex_reviews.rs)."""
    try:
        p = subprocess.run([codex, "login", "status"], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, timeout=30)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return False, "it could not say whether it is signed in (%s)" % exc
    said = " ".join((p.stderr + p.stdout).decode("utf-8", "replace").split())[:200]
    return p.returncode == 0, said


def find_claude(named=""):
    """The Claude CLI: the one named (--claude, the CLI the app ships), else the test seam,
    else the operator's. A named one that cannot run is no CLI, never a fallback to another."""
    if named:
        return named if os.path.isfile(named) and os.access(named, os.X_OK) else ""
    pinned = os.environ.get("SECOND_REVIEW_CLAUDE")
    if pinned is not None:
        return pinned if os.path.isfile(pinned) and os.access(pinned, os.X_OK) else ""
    found = shutil.which("claude")
    if found:
        return found
    for p in ("~/.local/bin/claude", "~/.claude/local/claude"):
        p = os.path.expanduser(p)
        if os.access(p, os.X_OK):
            return p
    return ""


def version_of(binary):
    try:
        p = subprocess.run([binary, "--version"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
        return p.stdout.decode("utf-8", "replace").strip().splitlines()[0]
    except (OSError, subprocess.TimeoutExpired, IndexError):
        return ""


def run_bounded(argv, cwd, prompt, out_path, err_path, limit, env=None):
    """(exit code or None on the time limit, seconds).

    THE REVIEWER STAYS IN THIS COMMAND'S PROCESS GROUP (second review of b5ff41f02, finding 1).
    review-watch starts second-review.sh leading a process group of its own and stops a review by
    signaling that group, so the reviewer, and whatever it starts in its group, ends with it
    whenever it was forked: nothing has to be registered before a stop can reach it. Until
    b5ff41f02 the reviewer led a session of its own, and one forked after the watcher's last read
    of the process table outlived the SIGKILL. Past the limit the reviewer itself is stopped by its
    process id: SIGTERM, ten seconds, SIGKILL."""
    start = time.monotonic()
    with open(out_path, "wb") as out, open(err_path, "wb") as err:
        p = subprocess.Popen(argv, cwd=cwd, stdin=subprocess.PIPE, stdout=out, stderr=err, env=build_env(env))
        _REVIEWER["proc"] = p
        try:
            p.communicate(prompt.encode("utf-8"), timeout=limit)
            return p.returncode, time.monotonic() - start
        except subprocess.TimeoutExpired:
            for stop in (p.terminate, p.kill):
                stop()
                try:
                    p.wait(timeout=10)
                    break
                except subprocess.TimeoutExpired:
                    continue
            return None, time.monotonic() - start
        finally:
            _REVIEWER["proc"] = None


def build_env(env=None):
    """The reviewer's environment with its builds capped (REVIEW_BUILD_JOBS)."""
    e = dict(os.environ if env is None else env)
    jobs = str(REVIEW_BUILD_JOBS)
    e["CARGO_BUILD_JOBS"] = jobs
    e["CMAKE_BUILD_PARALLEL_LEVEL"] = jobs
    e["MAKEFLAGS"] = "-j" + jobs
    return e


def _on_stop(signum, _frame):
    """Stopped from outside. review-watch signals this command's whole process group, which holds
    the reviewer too, so the reviewer has the signal already; it is passed on for a stop sent to
    this process alone (by hand), and this process exits."""
    p = _REVIEWER.get("proc")
    if p is not None:
        try:
            p.send_signal(signum)
        except OSError:
            pass
    raise SystemExit(128 + signum)


def codex_run_facts(stdout_path, stderr_path):
    """Model and effort from Codex's own SessionConfiguredEvent (stderr, RUST_LOG=codex_exec=info),
    whether it wrote a session record (rollout_path), and the token count from its turn.completed
    events (stdout). Never from what this command asked for."""
    facts = {"model": "", "effort": "", "rollout": "", "tokens": None}
    try:
        with open(stderr_path, encoding="utf-8", errors="replace") as f:
            for line in f:
                if "SessionConfiguredEvent" not in line:
                    continue
                m = re.search(r'\bmodel: "([^"]+)"', line)
                e = re.search(r"\breasoning_effort: Some\((\w+)\)", line)
                r = re.search(r"\brollout_path: (None|Some\([^)]*\))", line)
                facts["model"] = m.group(1) if m else ""
                facts["effort"] = e.group(1).lower() if e else ""
                facts["rollout"] = r.group(1) if r else ""
    except OSError:
        pass
    total = None
    try:
        with open(stdout_path, encoding="utf-8", errors="replace") as f:
            for line in f:
                try:
                    o = json.loads(line)
                except ValueError:
                    continue
                if not isinstance(o, dict) or o.get("type") != "turn.completed":
                    continue
                u = o.get("usage") or {}
                total = total or {"total": 0, "input": 0, "cached_input": 0, "output": 0}
                for k, src in (("input", "input_tokens"), ("cached_input", "cached_input_tokens"),
                               ("output", "output_tokens")):
                    total[k] += int(u.get(src) or 0)
                total["total"] = total["input"] + total["output"]
    except OSError:
        pass
    facts["tokens"] = total
    return facts


def codex_env():
    """Codex logs its SessionConfiguredEvent (model, effort, rollout path) on stderr at this level."""
    return dict(os.environ, RUST_LOG="codex_exec=info")


# The reviewer never runs the user's own Codex setup: see "ISOLATED FROM THE USER'S OWN SETUP" above.
CODEX_ISOLATION = ["--ephemeral", "--ignore-user-config", "--ignore-rules", "-c", "notify=[]",
                   "--disable", "plugins", "--disable", "apps", "--disable", "hooks",
                   "--disable", "computer_use", "--disable", "browser_use"]


def codex_argv(codex, export, schema_path, answer_path):
    return [codex, "exec"] + CODEX_ISOLATION + [
        "--sandbox", "workspace-write",
        "-C", export,
        "--skip-git-repo-check",
        "-m", CODEX_MODEL,
        "-c", 'model_reasoning_effort="%s"' % CODEX_EFFORT,
        "-c", 'approval_policy="never"',
        "--output-schema", schema_path,
        "-o", answer_path,
        "--json",
        "--color", "never",
        "-"]


def claude_settings():
    """Every write goes through Bash, and Bash is confined by Claude Code's OS
    sandbox to the working folder (it reads anywhere), the same boundary as
    Codex's workspace-write. No command may leave the sandbox."""
    return json.dumps({"sandbox": {"enabled": True, "autoAllowBashIfSandboxed": True,
                                   "allowUnsandboxedCommands": False}})


def claude_argv(claude):
    # bypassPermissions, because with acceptEdits the first real run
    # (2026-10-09, rv-20261009T004917Z-2d5aaf84-fc47) had its `cargo test` and
    # a `cd tree && ...` refused as prompts nobody answers. The boundary is the
    # OS sandbox, which bypassPermissions does not lift; the file-writing tools,
    # which the sandbox does not confine, are removed.
    return [claude, "-p",
            "--model", CLAUDE_MODEL,
            "--effort", CLAUDE_EFFORT,
            "--setting-sources", "",
            "--strict-mcp-config",
            "--settings", claude_settings(),
            "--permission-mode", "bypassPermissions",
            "--disallowedTools", "Edit,Write,NotebookEdit",
            "--permission-prompts", "none",
            "--json-schema", json.dumps(SCHEMA),
            "--output-format", "json",
            "--no-session-persistence"]


def claude_env(account=None):
    """The Claude reviewer's environment. `account` (account_in_use) is the app's account in use:
    an added account's folder becomes CLAUDE_CONFIG_DIR and its id RICHOS_CLAUDE_ACCOUNT, exactly
    as a work lease is given them (richos-core engine_profile.rs configure); Account 1 has no
    folder and keeps the inherited one. Only the reviewer gets them: this command's own state and
    scratch stay where they were."""
    env = dict(os.environ)
    for k in ("CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SSE_PORT"):
        env.pop(k, None)
    if account:
        env["RICHOS_CLAUDE_ACCOUNT"] = account["id"]
        if account.get("folder"):
            env["CLAUDE_CONFIG_DIR"] = account["folder"]
    return env


def account_in_use(path):
    """The Claude account the app's work runs on now, from the app's account list (richos-core
    claude_accounts.rs: `inUse` among `accounts`, else the first, the same as Accounts::in_use,
    which quota::Service::lease_account returns to every work lease): {"id", "folder"?}. Read when
    the reviewer starts, so a switch while the review waited for admission is honored. No list yet
    is Account 1 alone; an unreadable one is None (no review: never the wrong account)."""
    try:
        with open(path, encoding="utf-8") as f:
            stored = json.load(f)
    except FileNotFoundError:
        return {"id": "1"}
    except (OSError, ValueError):
        return None
    try:
        listed = [x for x in stored["accounts"] if isinstance(x.get("id"), str)]
        return next((x for x in listed if x["id"] == stored.get("inUse")), listed[0])
    except (KeyError, IndexError, TypeError, AttributeError):
        return None


def read_claude_result(stdout_path):
    """(answer dict or None, facts)."""
    facts = {"model": "", "tokens": None, "cost_usd": None}
    try:
        with open(stdout_path, encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError:
        return None, facts
    res = None
    for chunk in [text] + text.splitlines()[::-1]:
        try:
            o = json.loads(chunk)
        except ValueError:
            continue
        if isinstance(o, dict) and o.get("type") == "result":
            res = o
            break
    if res is None:
        return None, facts
    usage = res.get("modelUsage") or {}
    if isinstance(usage, dict) and usage:
        opus = [k for k in usage if "opus" in k]
        facts["model"] = (opus or sorted(usage, key=lambda k: -int((usage[k] or {}).get("outputTokens") or 0)))[0]
    u = res.get("usage") or {}
    if u:
        facts["tokens"] = {"total": sum(int(u.get(k) or 0) for k in ("input_tokens", "output_tokens",
                                                                     "cache_read_input_tokens",
                                                                     "cache_creation_input_tokens")),
                           "input": u.get("input_tokens"), "cached_input": u.get("cache_read_input_tokens"),
                           "output": u.get("output_tokens")}
    facts["cost_usd"] = res.get("total_cost_usd")
    answer = res.get("structured_output")
    if answer is None and isinstance(res.get("result"), str):
        body = res["result"].strip()
        m = re.search(r"```(?:json)?\s*(\{.*\})\s*```", body, re.S)
        try:
            answer = json.loads(m.group(1) if m else body)
        except ValueError:
            answer = None
    return answer, facts


# ---------------------------------------------------------------------------
# the verdict
# ---------------------------------------------------------------------------

def shape_problems(v, schema=SCHEMA, where="answer"):
    """Every way the answer departs from the fixed shape ([] = it fits)."""
    t = schema.get("type")
    if t == "object":
        if not isinstance(v, dict):
            return ["%s is not an object" % where]
        out = []
        props = schema.get("properties") or {}
        for k in schema.get("required") or []:
            if k not in v:
                out.append("%s has no %r" % (where, k))
        if schema.get("additionalProperties") is False:
            out += ["%s has an unexpected %r" % (where, k) for k in v if k not in props]
        for k, sub in props.items():
            if k in v:
                out += shape_problems(v[k], sub, "%s.%s" % (where, k))
        return out
    if t == "array":
        if not isinstance(v, list):
            return ["%s is not a list" % where]
        out = []
        for i, item in enumerate(v):
            out += shape_problems(item, schema.get("items") or {}, "%s[%d]" % (where, i))
        return out
    if t == "string" and not isinstance(v, str):
        return ["%s is not a string" % where]
    if t == "boolean" and not isinstance(v, bool):
        return ["%s is not true or false" % where]
    if t == "integer" and (isinstance(v, bool) or not isinstance(v, int)):
        return ["%s is not a whole number" % where]
    if "enum" in schema and v not in schema["enum"]:
        return ["%s is %r, not one of %s" % (where, v, schema["enum"])]
    return []


def blocking(answer):
    """The findings that block: the new ones marked so, and the earlier ones
    the reviewer says are still open and block."""
    return ([f for f in answer["findings"] if f["blocks"]]
            + [e for e in answer["earlier_findings"] if e["status"] == "still-open" and e["blocks"]])


def judge(answer, tip):
    """(verdict or None, forced, why). The verdict follows the findings:
    changes-requested exactly when one blocks, passed otherwise, whatever the
    reviewer wrote (ruling §116)."""
    if answer is None:
        return None, False, "the reviewer gave no answer"
    problems = shape_problems(answer)
    if problems:
        return None, False, "the answer is outside the fixed shape: " + "; ".join(problems[:5])
    named = answer["reviewed_commit"].strip().lower()
    if len(named) < 7 or not tip.startswith(named):
        return None, False, ("refused: the verdict names commit %r, not the tip %s it was asked to review"
                             % (answer["reviewed_commit"], tip))
    n = len(blocking(answer))
    verdict = "changes-requested" if n else "passed"
    if verdict != answer["verdict"]:
        return verdict, True, ("%d blocking finding(s) make it %s (the reviewer wrote %s)"
                               % (n, verdict, answer["verdict"]))
    return verdict, False, ""


# ---------------------------------------------------------------------------
# one review
# ---------------------------------------------------------------------------

def keep_fixtures(src, dest):
    """Copy the reviewer's fixtures, small files only: a compile's output
    belongs in build/, and a stray one must not fill ~/.claude/state.
    Returns what was left out."""
    left, total = [], 0
    for dirpath, dirnames, filenames in os.walk(src):
        dirnames[:] = [d for d in dirnames if d not in ("target", ".git", "node_modules")]
        for n in sorted(filenames):
            p = os.path.join(dirpath, n)
            rel = os.path.relpath(p, src)
            try:
                size = os.lstat(p).st_size
            except OSError:
                continue
            if os.path.islink(p) or size > FIXTURE_FILE_BYTES or total + size > FIXTURE_TOTAL_BYTES:
                left.append(rel)
                continue
            os.makedirs(os.path.join(dest, os.path.dirname(rel)), exist_ok=True)
            shutil.copyfile(p, os.path.join(dest, rel))
            total += size
    return left


def repo_identity(path):
    """The canonical identity of the repository at `path`: the real path of Git's
    common directory, exactly as Git reports it. Recorded in every verdict row when
    the review is written, so a worktree moved or removed later still matches."""
    real = os.path.realpath(path or "")
    try:
        env = {k: v for k, v in os.environ.items()
               if k not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR")}
        r = subprocess.run(["git", "-C", real, "rev-parse", "--git-common-dir"], capture_output=True,
                           timeout=30, env=env)
        raw = r.stdout[:-1] if r.stdout.endswith(b"\n") else r.stdout
        common = os.fsdecode(raw)
        if r.returncode == 0 and common:
            real = os.path.realpath(os.path.join(real, common))
    except (OSError, subprocess.TimeoutExpired):
        pass
    return real


@contextlib.contextmanager
def execution_claim(repo, tip, wait_seconds=0, say=None):
    """Every producer uses this kernel-held claim. Keep the inode after release.

    Watcher scheduling locks are not execution authority: manual reviews must
    participate too. A crash releases flock, including a kill before output.
    """
    identity = hashlib.sha256((repo_identity(repo) + "\n" + tip).encode()).hexdigest()
    path = os.path.join(state_root(), "review-claims", identity + ".lock")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        deadline=time.monotonic()+wait_seconds
        announced=False
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic()>=deadline:
                    raise Refused("a review already owns this repository and source; bounded join wait expired")
                if not announced and say:
                    say("SECOND-REVIEW joining the existing source owner without a worker permit")
                    announced=True
                time.sleep(min(1,max(0,deadline-time.monotonic())))
        yield
    finally:
        os.close(fd)


def completion_receipt(row):
    """Names existing ledger evidence. It never creates a new review timestamp."""
    if row.get('verdict'):
        receipt={k:row.get(k) for k in ('id','repo_id','tip','work','input_identity')}
        receipt['purpose']='handover' if row.get('trigger') in ('handover','manual') else 'progress'
        receipt['invocation']=os.environ.get('SECOND_REVIEW_INVOCATION')
        receipt['recheck_of']=row.get('recheck_of') or ''
        print('SECOND-REVIEW RECEIPT '+json.dumps(receipt,sort_keys=True),flush=True)


def review_input_identity(w):
    return hashlib.sha256(json.dumps({"repo": repo_identity(w.repo), "tip": w.tip,
        "base": w.base, "work": w.work_key, "words": w.words, "claims": w.claims},
        sort_keys=True).encode()).hexdigest()


def actionable_recovery(repo, work):
    import recovery
    record = recovery.read(recovery.key(repo, work))
    return record if record and record.get("mode")=="enforce" else None


def review(a):
    w = resolve_work(a)
    import recovery
    recovery_record = actionable_recovery(w.repo, w.work_key)
    w.recovery_record = recovery_record
    if recovery_record and recovery_record.get("wes"):
        wes = recovery_record["wes"]
        if wes["verdict"]["action"] in ("withdraw_finding", "correct_interpretation"):
            w.claims.append(("Wes disputed-premise evidence for independent resolution",
                "Verify these original sources independently. Resolve the exact earlier finding through the ordinary "
                "earlier_findings result; this proposal grants no bypass. " + json.dumps(wes)))
    if recovery_record and (recovery_record.get("plan") or {}).get("accepted"):
        plan = recovery_record["plan"]
        w.claims.append(("Accepted recovery acceptance matrix and independent proof requirements", json.dumps(plan)))
        markers = {c["condition"]:"RECOVERY:"+recovery.digest(c["condition"])+":" for c in plan["matrix"]}
        w.claims.append(("Required recovery proof reporting", "For each condition add one check string using its "
            "exact supplied marker followed by actual source-qualified proof evidence. Missing coverage keeps recovery "
            "unaccepted even if ordinary findings pass. Markers: "+json.dumps(markers)))
        if a.name:
            ws = load_workspaces(a.engine_root)
            registered = find_teammate(ws, a.name)
            space = teammate_workspace(registered, w.repo)
            current = git(space.get("path") or w.repo, "rev-parse", "HEAD").strip()
            if ws.finished_state(registered)[0] and current==w.tip:
                recovery.handover(recovery_record["id"], registered["key"], w.tip)
    # A join holds no CPU worker. Its bound is the existing worst-case producer budget.
    join_seconds=getattr(a,"admission_wait",ADMISSION_WAIT_SECONDS)+2*getattr(a,"limit_minutes",LIMIT_SECONDS/60)*60
    say=lambda message:(sys.stderr.write(message+"\n"),sys.stderr.flush())
    with execution_claim(w.repo, w.tip, wait_seconds=join_seconds, say=say):
        purpose = "handover" if a.trigger in ("handover", "manual") else "progress"
        existing = [r for r in read_ledger() if r.get("tip") == w.tip and r.get("verdict")
                    and same_repo(r, w.repo, repo_identity(w.repo))
                    and ("handover" if r.get("trigger") in ("handover", "manual") else "progress") == purpose]
        recheck = getattr(a, "recheck", "")
        if recheck and not any(r.get("id") == recheck for r in existing):
            raise Refused("--recheck must name an existing review of this repository/source/purpose")
        reusable = [r for r in existing if r.get("recheck_of", "") == recheck]
        if reusable:
            row = reusable[-1]
            if row.get("input_identity") != review_input_identity(w):
                raise Refused("this source already has a review with different inputs; use --recheck <review id>")
            print("SECOND-REVIEW reused %s (%s). Record: %s" %
                  (row["id"], row["verdict"], row.get("record")))
            completion_receipt(row)
            return {"passed": EXIT_PASSED, "changes-requested": EXIT_CHANGES}[row["verdict"]]
        return _review_owned(a, w)


def _review_owned(a, w):
    import recovery
    recovery_record = getattr(w, "recovery_record", None)
    say = lambda s: (sys.stderr.write(s + "\n"), sys.stderr.flush())
    repo_id = repo_identity(w.repo)
    filed, filed_ids = filed_findings(w.repo, repo_id, w.tip)
    earlier = [(fid, f) for fid, f in earlier_findings(w.work_key, w.tip, w.repo, repo_id) if fid not in filed_ids]
    w.filed = filed
    w.since, w.since_review = review_since(w, repo_id)
    started_at = time.time()
    rid = "rv-%s-%s-%s" % (time.strftime("%Y%m%dT%H%M%SZ", time.gmtime(started_at)), w.tip[:8], secrets.token_hex(2))
    record = os.path.join(state_root(), "reviews", rid)
    export = os.path.join(a.scratch, "export")
    out_dir = os.path.join(a.scratch, "out")
    os.makedirs(out_dir)
    schema_path = os.path.join(out_dir, "schema.json")
    with open(schema_path, "w", encoding="utf-8") as f:
        json.dump(SCHEMA, f)
    prompt = ""

    row = {"id": rid, "at": iso(started_at), "repo": w.repo, "repo_id": repo_id, "branch": w.branch, "base": w.base, "tip": w.tip,
           "input_identity": review_input_identity(w), "recheck_of": getattr(a, "recheck", ""),
           "work": w.work_key, "author": w.author, "author_model": w.author_model, "trigger": a.trigger,
           "reviewer": "", "reviewer_model": "", "reviewer_effort": "", "cli_version": "",
           "verdict": None, "forced": False, "findings": 0, "blocking": 0, "p1": 0, "follow_ups": 0,
           "earlier_findings": len(earlier),
           "duration_s": None, "tokens": None, "meter": None, "admission": "", "quota": "",
           "fallback_why": "", "why": "", "record": record}

    def finish(answer=None, raw_paths=()):
        row["finished_at"] = iso()
        os.makedirs(record, exist_ok=True)
        for p in [os.path.join(export, "review", "prompt.md")] + list(raw_paths):
            if os.path.isfile(p):
                shutil.copyfile(p, os.path.join(record, os.path.basename(p)))
        fx = os.path.join(export, "fixtures")
        if os.path.isdir(fx) and os.listdir(fx):
            left = keep_fixtures(fx, os.path.join(record, "fixtures"))
            if left:
                row["fixtures_not_kept"] = left[:50]
        if recovery_record and recovery_record.get("plan") and answer and row.get("verdict"):
            plan = recovery_record["plan"]
            coverage = {}
            for condition in plan["matrix"]:
                marker = "RECOVERY:"+recovery.digest(condition["condition"])+":"
                proofs = [check[len(marker):].strip() for check in answer.get("checks", []) if check.startswith(marker)]
                if proofs and proofs[-1]:
                    coverage[condition["condition"]] = proofs[-1]
            row.update(recovery_plan=plan["id"], recovery_coverage=coverage)
        if recovery_record and recovery_record.get("wes") and answer and row.get("verdict"):
            row["wes_consultation"] = recovery_record["wes"]["consultation"]
        with open(os.path.join(record, "verdict.json"), "w", encoding="utf-8") as f:
            json.dump({"row": row, "answer": answer,
                       "earlier_findings_in": [dict(f, id=i) for i, f in earlier]}, f, indent=2, sort_keys=True)
        append_row(row)
        if recovery_record and row.get("wes_consultation"):
            with recovery.locked(recovery_record["id"]):
                fresh = recovery.read(recovery_record["id"])
                if fresh.get("wes", {}).get("consultation") == row["wes_consultation"]:
                    fresh["wes"].update(independent_review=row["id"], applied=True)
                    recovery.save(fresh, "wes-independent-review-consumed")
        if recovery_record and row.get("recovery_coverage"):
            recovery.complete(recovery_record["id"], row)
        if row["verdict"]:
            filed = (" %d follow-up(s) filed in %s." % (row["follow_ups"], follow_ups_path())
                     if row["verdict"] == "passed" else "")
            line = "SECOND-REVIEW %s %s@%s by %s (%s): %d finding(s), %d blocking, %d P1%s.%s Record: %s" % (
                row["verdict"], os.path.basename(w.repo), w.tip[:12], row["reviewer"], row["reviewer_model"],
                row["findings"], row["blocking"], row["p1"], " (forced by its findings)" if row["forced"] else "",
                filed, record)
        else:
            line = "SECOND-REVIEW no verdict %s@%s: %s. Record: %s" % (os.path.basename(w.repo), w.tip[:12],
                                                                      row["why"], record)
        print(line)
        completion_receipt(row)
        sys.stdout.flush()
        return {"passed": EXIT_PASSED, "changes-requested": EXIT_CHANGES}.get(row["verdict"], EXIT_NO_VERDICT)

    admitted, why = admit(a.admission_wait, say)
    row["admission"] = why
    if not admitted:
        row["why"] = why
        return finish()
    export_tree(w.repo, w.tip, os.path.join(export, "tree"))
    prompt = build_input(w, a, export, earlier)
    row["since"], row["since_review"] = w.since, w.since_review

    limit = _env_seconds("SECOND_REVIEW_TIMEOUT_SECONDS", a.limit_minutes * 60.0)
    use_codex = w.author != "codex" and a.reviewer in ("auto", "codex")
    answer = None
    raw = []
    if use_codex:
        codex = find_codex()
        signed_in, said = codex_signed_in(codex) if codex else (False, "")
        if not codex:
            row["fallback_why"] = "the Codex CLI was not found inside ChatGPT.app or on PATH"
        elif not signed_in:
            row["fallback_why"] = "Codex isn't signed in (codex login status: %s)" % (said or "no answer")
        else:
            row["reviewer"], row["cli_version"] = "codex", version_of(codex)
            answer_path = os.path.join(out_dir, "answer.json")
            so, se = os.path.join(out_dir, "codex.stdout.jsonl"), os.path.join(out_dir, "codex.stderr.txt")
            rc, secs = run_bounded(codex_argv(codex, export, schema_path, answer_path), export, prompt, so, se, limit,
                                   env=codex_env())
            row["duration_s"] = round(secs, 1)
            facts = codex_run_facts(so, se)
            row["reviewer_model"] = facts["model"] or "unverified (Codex logged no session start; asked for %s)" % CODEX_MODEL
            row["reviewer_effort"] = facts["effort"] or "unverified (asked for %s)" % CODEX_EFFORT
            row["tokens"], row["codex_record"] = facts["tokens"], facts["rollout"]
            raw = [so, se, answer_path]
            if rc is None:
                row["why"] = "the reviewer passed its time limit of %d minutes and was stopped" % round(limit / 60.0) \
                    if limit >= 60 else "the reviewer passed its time limit of %d s and was stopped" % limit
                return finish(None, raw)
            try:
                with open(answer_path, encoding="utf-8") as f:
                    answer = json.load(f)
            except (OSError, ValueError):
                answer = None
            if answer is None and rc != 0:
                tail = ""
                try:
                    with open(se, encoding="utf-8", errors="replace") as f:
                        tail = " ".join([ln for ln in f.read().strip().splitlines()
                                         if " INFO " not in ln][-3:])[:400]
                except OSError:
                    pass
                row["fallback_why"] = "Codex exited %d without an answer: %s" % (rc, tail or "(no message)")
            elif facts["model"] and (facts["model"] != CODEX_MODEL or facts["effort"] != CODEX_EFFORT):
                row["verdict"] = None
                row["why"] = ("refused: Codex's own session record says model %s at effort %s, not the pinned "
                              "%s at %s" % (facts["model"], facts["effort"], CODEX_MODEL, CODEX_EFFORT))
                return finish(answer, raw)
            else:
                verdict, forced, why = judge(answer, w.tip)
                return conclude(row, answer, verdict, forced, why, finish, raw, earlier)
    if a.reviewer == "codex":
        row["why"] = row["fallback_why"] or "Codex could not review"
        return finish(None, raw)

    # Claude: the reviewer of Codex's work, and the fallback.
    held, reading = quota_hold(a.engine_root)
    row["quota"] = reading
    if held:
        row["why"] = ("the quota hold is in force (his 93%% rule; %s), so no Claude review started; it waits "
                      "for the reset, as every teammate does" % reading)
        return finish(None, raw)
    claude = find_claude(a.claude)
    if not claude:
        row["why"] = "no reviewer could run: %sthe claude CLI was not found" % (
            (row["fallback_why"] + "; ") if row["fallback_why"] else "")
        return finish(None, raw)
    account = account_in_use(a.accounts) if a.accounts else None
    if a.accounts and account is None:
        row["why"] = "the app's Claude account list could not be read (%s), so no review started" % a.accounts
        return finish(None, raw)
    row["reviewer"], row["cli_version"] = "claude", version_of(claude)
    row["reviewer_model"], row["reviewer_effort"] = "", CLAUDE_EFFORT
    so, se = os.path.join(out_dir, "claude.stdout.json"), os.path.join(out_dir, "claude.stderr.txt")
    rc, secs = run_bounded(claude_argv(claude), export, prompt, so, se, limit, env=claude_env(account))
    row["duration_s"] = round((row["duration_s"] or 0) + secs, 1)
    raw = raw + [so, se]
    answer, facts = read_claude_result(so)
    row["reviewer_model"], row["tokens"], row["cost_usd"] = facts["model"], facts["tokens"], facts["cost_usd"]
    row["meter"] = None
    if rc is None:
        row["why"] = "the reviewer passed its time limit and was stopped"
        return finish(None, raw)
    if facts["model"] and "opus" not in facts["model"]:
        row["why"] = "refused: the Claude reviewer ran on %s, not Opus" % facts["model"]
        return finish(answer, raw)
    verdict, forced, why = judge(answer, w.tip)
    return conclude(row, answer, verdict, forced, why, finish, raw, earlier)


def conclude(row, answer, verdict, forced, why, finish, raw, earlier=()):
    row["verdict"], row["forced"] = verdict, forced
    if verdict:
        row["findings"] = len(answer["findings"])
        row["blocking"] = len(blocking(answer))
        row["p1"] = sum(1 for f in answer["findings"] if f["priority"] == 1)
        if forced:
            row["note"] = why
        if verdict == "passed":
            row["follow_ups"] = file_follow_ups(row, answer, earlier)
    else:
        row["why"] = why
    return finish(answer, raw)


def file_follow_ups(row, answer, earlier):
    """A passed review's findings go on the follow-up list, one row each: its
    own findings, and the earlier ones it says are still open, except those an
    earlier pass already filed. Each one filed here is marked "filed" in
    `earlier`, which this verdict's earlier_findings_in records, so the next
    review carries the mark. Returns how many were filed."""
    def one(fid, f):
        return {"at": iso(), "review": row["id"], "finding": fid, "repo": row["repo"], "branch": row["branch"],
                "tip": row["tip"], "work": row["work"], "priority": f.get("priority"), "title": f.get("title"),
                "files": f.get("files") or [], "evidence": f.get("evidence"), "fixture": f.get("fixture") or ""}
    rows = [one("%s#%d" % (row["id"], i), f) for i, f in enumerate(answer["findings"], 1)]
    still = set(e["id"] for e in answer["earlier_findings"] if e["status"] == "still-open")
    for fid, f in earlier:
        if fid in still and not f.get("filed"):
            rows.append(one(fid, f))
            f["filed"] = True
    if rows:
        append_row(rows, follow_ups_path())
    return len(rows)


def main(argv):
    ap = argparse.ArgumentParser(prog="second-review.sh",
                                 description="One second review of a teammate's exact commits by a different model.")
    ap.add_argument("--scratch", required=True, help=argparse.SUPPRESS)
    ap.add_argument("--engine-root", default=os.path.dirname(os.path.dirname(HERE)), help=argparse.SUPPRESS)
    ap.add_argument("--name", default="", help="the teammate whose work this is (registry name or key)")
    ap.add_argument("--repo", default="", help="the repository (required without --name)")
    ap.add_argument("--branch", default="", help="the branch whose head is the tip")
    ap.add_argument("--tip", default="", help="the exact commit to review (default: the branch head)")
    ap.add_argument("--base", default="", help="the base commit (default: merge-base with --integration)")
    ap.add_argument("--integration", default="main", help="the branch the work lands on (default main)")
    ap.add_argument("--words-file", action="append", help="the original words, verbatim (repeatable)")
    ap.add_argument("--claims-file", action="append", help="a report or receipt the author cites (repeatable)")
    ap.add_argument("--author", default="", help="the author's name, when not a registered teammate")
    ap.add_argument("--author-model", default="", help="the author's model, when no transcript says it")
    ap.add_argument("--work", default="", help="the key that ties rechecks of one piece of work together")
    ap.add_argument("--recheck", default="", help="the prior review id intentionally rechecked on the same source")
    ap.add_argument("--follow-up", default="", metavar="REVIEW-ID#N",
                    help="file a blocking finding as a follow-up (the lead's ruling); it holds no land after this")
    ap.add_argument("--reopen", default="", metavar="REVIEW-ID#N", help="undo --follow-up: the finding blocks again")
    ap.add_argument("--why", default="", help="with --follow-up: who rules, and why")
    ap.add_argument("--trigger", default="manual", choices=["handover", "long-job", "quiet", "manual"])
    ap.add_argument("--reviewer", default="auto", choices=["auto", "codex", "claude"])
    ap.add_argument("--claude", default="", help="the Claude CLI to review with (the app passes the one it ships)")
    ap.add_argument("--accounts", default="",
                    help="the app's Claude account list (claude-accounts.json): Claude reviews on the account in use")
    ap.add_argument("--limit-minutes", type=float, default=LIMIT_SECONDS / 60.0)
    ap.add_argument("--admission-wait", type=float, default=ADMISSION_WAIT_SECONDS,
                    help="seconds to wait for CPU admission (default 1800)")
    a = ap.parse_args(argv)
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, _on_stop)
    try:
        if a.follow_up or a.reopen:
            if a.follow_up and a.reopen:
                raise Refused("--follow-up and --reopen are two rulings; give one")
            return follow_up(a.follow_up or a.reopen, a.why, reopen=bool(a.reopen))
        return review(a)
    except Refused as exc:
        sys.stderr.write("second-review: %s\n" % exc)
        return EXIT_REFUSED


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except KeyboardInterrupt:
        sys.exit(130)
