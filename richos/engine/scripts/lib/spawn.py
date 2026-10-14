#!/usr/bin/env python3
"""spawn.py - STARTING ONE TEAMMATE IS ONE COMMAND THAT EITHER SUCCEEDS OR NAMES
EVERY PROBLEM BEFORE ANYTHING IS CREATED.

The page: docs/plans/worktree-spec-2026-09-11.md. The registry: workspaces.py.
The command: scripts/spawn.sh.

===========================================================================
WHY THIS EXISTS - CEO, 2026-09-13, his third time asking
===========================================================================
  "it was taking you 10 fucking minutes to spin up one fucking agent properly
   at the beginning of the session. how much longer do I fucking have to wait
   for that retarded fuckshit to be fixed once and forever?"

Starting one teammate was FOUR commands plus a hand-authored JSON file, with at
least four independent ways to fail AFTER work had already begun:

  1. create-teammate-worktree.sh <ABSOLUTE-repo> <name>
       - refused a bare repository name                     (one round trip)
       - refused with "no branch is recorded as the one this work integrates
         on", naming a SECOND command to run first          (one round trip)
       - the richos record was STALE: it named dev/workspace-spec, deleted from
         the remote, so it needed --correct                 (one round trip)
  2. a hand-written JSON input file
  3. prepare-agent-spawn.py --file <input.json>  (adds the ack contract, runs
     no guard at all)
  4. the tool call - where the PreToolUse[Agent] guards finally run, so a
     wording mistake in the brief surfaced only at the very end, one refusal
     per attempt.

On one brief, on one night, that cost: two refusals from
guard-brief-verification-scope.sh, one for a reused name, one for an
unrecorded integration branch, one for a relative repository path. Every one
was fixable in seconds. Every one cost a full round trip in front of him.

===========================================================================
WHAT THIS DOES DIFFERENTLY, AND THE ONE IDEA UNDER ALL OF IT
===========================================================================
EVERY REFUSAL IS EVALUATED BEFORE ANYTHING IS CREATED, AND THEY ARE REPORTED
TOGETHER. A brief that breaks three rules is fixed once, not three times.

  1. The integration branch is RESOLVED, and RECORDED when it is absent and
     the answer is not a human's to give - rather than refusing and naming a
     second command. A STALE record (one naming a branch that no longer
     exists) is CORRECTED in place, keeping the body of work's id so agents
     already bound to it move with it. Where the answer genuinely is a human
     decision - the repository is not on its default branch, or this work must
     not reach main yet - it says WHICH decision, in one line, and stops.
     It never guesses one: point 14 is "nothing infers it and nothing guesses
     it", and the deleted first-registration floor is what guessing costs.
  2. The repository is accepted by absolute path AND by bare name where that is
     unambiguous; an ambiguous name is answered with the candidates.
  3. EVERY PreToolUse[Agent] guard is evaluated against the payload, FROM BOTH
     SURFACES - the engine's hooks/hooks.json and the governed repository's
     .claude/settings*.json - and every failure is reported together. Reading
     only the engine's list is the exact hole that cost two refusals that
     night: guard-brief-verification-scope.sh lives in femcboost, not here.
     A matcherless PreToolUse group matches EVERY tool, so it is collected too;
     the stopgap this replaces missed those.
  4. Only then is the workspace created, and the guards are evaluated a SECOND
     time against the real thing - because clause 7a asks a question about a
     workspace that must exist to be answered.
  5. On ANY failure nothing is left behind: before creation there is nothing to
     leave, and after it the workspace, its branch and its registration are
     withdrawn (workspaces.sh withdraw-cc), which frees the name for the retry.
  6. A TEAMMATE MAY HAVE A WORKSPACE IN SEVERAL OTHER REPOSITORIES: --repo is
     given once per repository, and each gets its own registered cc/ workspace
     under the ONE name, with its own `cross-repo-worktree:` line in the
     payload. Point 10 is "All of an agent's workspaces go together"; the
     example in that sentence is two, and the rule is "every workspace and
     branch it has". Until 2026-09-17 this command took ONE --repo, so a
     teammate needing a second repository had to be given a SECOND NAME -
     which made it a second agent, landed separately, and on 2026-09-17 the CEO
     found the stray branch of one (`cc/norm-opus-wireguide1`) left behind by a
     land that named only `norm-opus-wire1`. Nothing else needed widening: the
     registry has always held a list of workspaces, land and discard have
     always deleted every one of them across every repository, clause 4c of
     guard-worktree-isolation.sh has always read EVERY marker line, and
     register_spawn has always required a registration for each.

THE CLAUSE-7 FALSE POSITIVE IS GONE, NOT SPECIAL-CASED. The stopgap
(femcboost/scripts/preflight-agent-spawn.sh, deleted with this) always reported
guard-worktree-isolation.sh as refusing, because a synthetic envelope has no
session_id or tool_use_id and only the host mints those. A check that always
fails protects nothing and teaches its reader to skip it. So workspaces.py
grew `check-spawn`: the SAME code as `register-spawn` with the write removed,
reached only when the platform minted no tool_use_id. See `is_spawn_check`.
"""
import importlib.util
import json
import os
import re
import shlex
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ENGINE = os.path.dirname(os.path.dirname(HERE))          # .../engine


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


W = _load("richos_workspaces", os.path.join(HERE, "..", "..", "mega-lander", "workspaces.py"))
PREPARE = _load("richos_prepare_agent_spawn",
                os.path.join(ENGINE, "scripts", "prepare-agent-spawn.py"))
PROV = _load("richos_brief_provenance",
             os.path.join(ENGINE, "ass-kicker", "brief-provenance.py"))
SCOPE = _load("richos_brief_scope",
              os.path.join(ENGINE, "ass-kicker", "brief-scope.py"))
DONE = _load("richos_brief_done",
             os.path.join(ENGINE, "ass-kicker", "brief-done.py"))
AUDIENCE = _load("richos_spawn_guard_audience",
                 os.path.join(HERE, "spawn-guard-audience.py"))
QATOOLS = _load("richos_qa_toolkit", os.path.join(HERE, "qa-toolkit.py"))

# `--audience app` — WHO IS THIS DISPATCH FOR, AND THEREFORE WHOSE GUARDS JUDGE
# IT. The RichOS desktop app dispatches background work on a non-technical
# user's own Mac; the development session dispatches teammates. They are not
# the same audience and they never were, but until 2026-09-18 they shared one
# guard surface by default and the app's short list was typed by hand in two
# places instead of being derived from anything.
#
# In `app` audience this command evaluates ONLY the guards
# spawn-guard-audience.declaration puts on the `user-work` side, and a refusal
# is reported in the APP'S OWN WORDS from that same declaration. `operator` is
# the default and is byte-for-byte today's behavior.
APP, OPERATOR = "app", "operator"


class Refusal(Exception):
    """One thing that must be fixed before this teammate can be started. Never
    raised one at a time where several can be collected."""


def sh(args, cwd=None, timeout=60):
    try:
        p = subprocess.run(args, cwd=cwd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr
    except (OSError, subprocess.SubprocessError) as exc:
        return 127, "", str(exc)


# ---------------------------------------------------------------------------
# 1. WHICH REPOSITORY
# ---------------------------------------------------------------------------

def candidate_repos(project_dir):
    """Every repository this machine has a reason to know about, deduplicated:
    the ones the registry remembers, the ones with an integration record, the
    session's own, and its siblings. Used ONLY to resolve a bare name and to
    print candidates when one does not resolve - never to pick one."""
    out = []
    seen = set()

    def add(path):
        if not path:
            return
        main = W.main_checkout(path)
        if main and main not in seen:
            seen.add(main)
            out.append(main)

    add(project_dir)
    for r in W.known_repos():
        add(r)
    for r in W.all_integration_records():
        add(r)
    parent = os.path.dirname(W.realpath(project_dir)) if project_dir else ""
    if parent and os.path.isdir(parent):
        try:
            for n in sorted(os.listdir(parent)):
                p = os.path.join(parent, n)
                if os.path.isdir(os.path.join(p, ".git")):
                    add(p)
        except OSError:
            pass
    return out


def resolve_repo(arg, project_dir):
    """A repository by ABSOLUTE PATH, by relative path, or by BARE NAME where
    that is unambiguous. `create-teammate-worktree.sh richos` answered "'richos'
    is not a directory", which is true and is not the answer to the question
    that was asked."""
    arg = (arg or "").strip()
    if not arg:
        raise Refusal("name the repository: --repo <absolute path, or a bare name like 'richos'>")
    looks_like_path = ("/" in arg) or arg in (".", "..") or arg.startswith("~")
    if looks_like_path:
        path = os.path.expanduser(arg)
        if not os.path.isdir(path):
            raise Refusal("--repo %s: no such directory. Give an absolute path, or a bare repository "
                          "name (%s)." % (arg, ", ".join(os.path.basename(r) for r in
                                                         candidate_repos(project_dir)) or "none known"))
        main = W.main_checkout(path)
        if not main:
            raise Refusal("--repo %s is not inside a git repository" % arg)
        return main
    hits = [r for r in candidate_repos(project_dir) if os.path.basename(r) == arg]
    if len(hits) == 1:
        return hits[0]
    if not hits:
        known = ", ".join(os.path.basename(r) for r in candidate_repos(project_dir))
        raise Refusal("--repo %s: no repository of that name is known here. Known: %s. An absolute "
                      "path always works." % (arg, known or "none"))
    raise Refusal("--repo %s is ambiguous - it names %d repositories: %s. Give the absolute path of "
                  "the one you mean." % (arg, len(hits), ", ".join(hits)))


# ---------------------------------------------------------------------------
# 2. WHICH BRANCH THIS WORK INTEGRATES ON (point 14)
# ---------------------------------------------------------------------------

def scoped_values(entries, raw_repos, repos, option):
    """`--dir` and `--integration` for a teammate that has workspaces in SEVERAL
    repositories: `<repo>=<value>`, where <repo> is spelled the way it was
    spelled to --repo (bare name or path) or is the repository's resolved path.

    THE BARE FORM IS KEPT, AND IT IS KEPT NARROW. With exactly one --repo,
    `--dir <path>` and `--integration <branch>` mean what they have always
    meant. With several, an unscoped value would have to be attached to one of
    them by guessing which - and point 14's whole sentence is that nothing
    guesses the branch a body of work integrates on. So it is refused, naming
    the form that answers it."""
    out = {}
    for entry in entries:
        repo, value = "", entry
        if "=" in entry:
            head, tail = entry.split("=", 1)
            for raw, resolved in zip(raw_repos, repos):
                if head in (raw, resolved, os.path.basename(resolved)):
                    repo, value = resolved, tail
                    break
        if not repo:
            if len(repos) != 1:
                raise Refusal("%s %s: this teammate has workspaces in %d repositories (%s), so say "
                              "which one this is for: %s <repo>=<value>, where <repo> is spelled as "
                              "it was to --repo."
                              % (option, entry, len(repos), ", ".join(repos), option))
            repo = repos[0]
        if repo in out:
            raise Refusal("%s is given twice for %s" % (option, repo))
        out[repo] = value
    return out


def default_branch(repo):
    """The repository's own default branch, read from git and never guessed:
    origin/HEAD if the remote declares one, else `main` or `master` if exactly
    one of them exists. "" when git does not answer - and "" is an answer here,
    not a reason to pick something."""
    rc, out, _ = sh(["git", "-C", repo, "symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"])
    if rc == 0 and out.strip().startswith("refs/remotes/origin/"):
        b = out.strip()[len("refs/remotes/origin/"):]
        if b and W.branch_tip(repo, b):
            return b
    have = [b for b in ("main", "master") if W.branch_tip(repo, b)]
    return have[0] if len(have) == 1 else ""


def checkout_branch(repo):
    wl = W.worktree_list(repo) or []
    return wl[0]["branch"] if wl else ""


def ensure_integration(repo, explicit, why, session, native_only=False):
    """Point 14, as one step of one command instead of a refusal naming a second
    command. Returns (a one-line note for the report, the branch THIS teammate
    alone is bound to, or "").

    --integration NAMING ANOTHER BRANCH THAN THE RECORDED ONE NEVER MOVES
    ANYBODY ELSE (2026-10-14). It used to CORRECT the repository's current body
    of work: `--integration richos=v1.3.0` for echo-opus-tap22 moved richos-001
    and every running main-bound teammate onto v1.3.0. Now that branch is this
    teammate's own (workspaces.teammate_body_of_work, bound when its cc/
    workspace is registered), and the repository's record is untouched; moving
    the whole repository is `workspaces.sh integration --correct`. In the
    session's own repository (`native_only`: a native workspace, no cc/
    registration) it is bound by workspaces.bind_own_target before the spawn,
    and the native workspace is moved onto it when the agent starts.

    THE THREE STATES, and the third is the one that bit us:
      recorded and its branch exists  -> nothing to do
      not recorded                    -> RECORD it, when the answer is not a
                                         human's to give
      recorded and its branch is GONE -> CORRECT it in place (same work id, so
                                         every agent already bound to it moves
                                         with it), when the answer is not a
                                         human's to give

    WHEN IT IS A HUMAN'S TO GIVE, IT SAYS SO AND STOPS. Recording the default
    branch is safe only where nothing suggests this work is being kept off it:
    the repository's main checkout is sitting ON its default branch. Anywhere
    else the question "does this work integrate on main, or on a branch that
    must not reach main yet?" has an answer this command cannot read, and
    point 14 is explicit that nothing may guess it - the deleted
    first-registration floor guessed exactly this and froze the wrong answer
    onto agents already in flight."""
    rec = W.integration_record(repo)
    live = bool(rec and W.branch_tip(repo, rec["branch"]))
    if rec and live and not explicit:
        return ("integration: %s (recorded %s, body of work %s)" % (rec["branch"], rec["recorded_at"],
                                                                    rec["id"]), "")
    if rec and live and explicit and explicit == rec["branch"]:
        return "integration: %s (recorded, matches --integration)" % rec["branch"], ""

    dflt = default_branch(repo)
    on = checkout_branch(repo)
    branch = explicit
    reason = "named with --integration"
    if not branch:
        if not dflt:
            raise Refusal(
                "%s: this command will not guess the branch this work integrates on (point 14). "
                "DECISION: git names no unambiguous default branch here, so say which branch it is: "
                "re-run with --integration <branch> [--integration-why '<this body of work>']." % repo)
        if on != dflt:
            raise Refusal(
                "%s: this command will not guess the branch this work integrates on (point 14). "
                "DECISION: its main checkout is on %r, not on its default branch %r - does this work "
                "integrate on %s, or on a branch that must not reach main yet? Re-run with "
                "--integration <branch> [--integration-why '<this body of work>']."
                % (repo, on, dflt, dflt))
        branch = dflt
        reason = "its main checkout is on %s and git names %s as its default branch" % (dflt, dflt)

    if not W.branch_tip(repo, branch):
        raise Refusal("%s: there is no branch %s to integrate on. The branch a body of work "
                      "integrates on exists before its first agent is spawned (point 14)."
                      % (repo, branch))

    if rec and not live:
        note = ("%s was recorded as this work's integration branch and no longer exists; %s"
                % (rec["branch"], reason))
        W.record_integration(repo, branch, why or note, session, correct=True)
        return ("integration: CORRECTED %s from %s (no such branch) to %s - %s"
                % (repo, rec["branch"], branch, reason), "")
    if rec and live and explicit and explicit != rec["branch"]:
        return ("integration: %s for this teammate only; %s stays on %s for everybody else (body of work %s)%s"
                % (branch, repo, rec["branch"], rec["id"],
                   "; its native workspace starts from %s" % branch if native_only else ""), branch)
    W.record_integration(repo, branch, why or reason, session)
    if explicit:
        return "integration: RECORDED %s for %s - %s" % (branch, repo, reason), ""
    return ("integration: RECORDED %s for %s - %s. If this work must not reach %s yet, re-run with "
            "--integration <branch>." % (branch, repo, reason, branch), "")


# ---------------------------------------------------------------------------
# 3. THE GUARDS, FROM EVERY SURFACE THAT REGISTERS THEM
# ---------------------------------------------------------------------------

def _hook_groups(node):
    if isinstance(node, dict):
        if "hooks" in node and isinstance(node.get("hooks"), list):
            yield node
        for v in node.values():
            for g in _hook_groups(v):
                yield g
    elif isinstance(node, list):
        for v in node:
            for g in _hook_groups(v):
                yield g


def _matches_agent(matcher):
    """Claude Code matches a hook group's `matcher` against the tool name as a
    regular expression. NO MATCHER AT ALL (and "" and "*") MATCHES EVERY TOOL -
    which is how guard-sealed-worktree.sh is registered, and which the stopgap
    this replaces silently skipped because it only looked for the string
    "Agent"."""
    if matcher is None:
        return True
    m = str(matcher).strip()
    if m in ("", "*"):
        return True
    try:
        return bool(re.search(m, "Agent"))
    except re.error:
        return m == "Agent"


def settings_sources(project_dir):
    """Every file that can register a PreToolUse[Agent] hook for a session
    seated in `project_dir`, in the order the platform layers them.

    RICHOS_SPAWN_HOOK_SOURCES ('<label>=<path>' entries, ':'-separated) replaces
    the list. It is a TEST AFFORDANCE, in the same spirit as
    RICHOS_WORKSPACES_DIR, and it is not a way to spawn past a guard: the guards
    that actually refuse a call are the ones the platform runs at PreToolUse.
    This command only tells you in advance what they will say."""
    override = (os.environ.get("RICHOS_SPAWN_HOOK_SOURCES") or "").strip()
    if override:
        out = []
        for item in override.split(":"):
            if "=" in item:
                label, path = item.split("=", 1)
                out.append((label.strip(), path.strip()))
            elif item.strip():
                out.append(("override", item.strip()))
        return out
    cfg = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    return [
        ("engine", os.path.join(ENGINE, "hooks", "hooks.json")),
        ("user", os.path.join(cfg, "settings.json")),
        ("project", os.path.join(project_dir, ".claude", "settings.json")),
        ("local", os.path.join(project_dir, ".claude", "settings.local.json")),
    ]


def collect_guards(project_dir):
    """[(source_label, source_path, command)] for every PreToolUse hook that a
    real Agent call would run, deduplicated by command, in registration order."""
    out, seen = [], set()
    for label, path in settings_sources(project_dir):
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
        except (OSError, ValueError):
            continue
        pre = None
        if isinstance(data, dict):
            pre = (data.get("hooks") or {}).get("PreToolUse")
            if pre is None and isinstance(data.get("PreToolUse"), list):
                pre = data["PreToolUse"]
        if not isinstance(pre, list):
            continue
        for group in _hook_groups({"_": pre}):
            if not _matches_agent(group.get("matcher")):
                continue
            for hook in group.get("hooks") or []:
                cmd = str((hook or {}).get("command") or "").strip()
                if not cmd or cmd in seen:
                    continue
                seen.add(cmd)
                out.append((label, path, cmd))
    return out


def guard_name(cmd):
    m = re.findall(r"[^\s/'\"]+\.(?:sh|py)", cmd)
    if m:
        return m[-1]
    return (cmd.split() or ["?"])[0][:60]


def for_audience(guards, audience):
    """(guards this audience is judged by, notes about what was left out).

    `operator` keeps every guard, unchanged. `app` keeps only the `user-work`
    rows of spawn-guard-audience.declaration.

    AN UNDECLARED GUARD IS NOT RUN FOR THE APP, and it is NAMED rather than
    dropped quietly. That direction is the declaration's, and the reason is
    that "judged only by guards whose reason protects the user's own work" is
    an allowlist: a guard nobody classified has not been shown to protect
    anything of the user's, and the app dying at spawn over an unreviewed new
    guard is the exact failure this exists to end. The other hazard — a real
    user-work guard silently falling off the app's list — is closed at
    development time by scripts/lib/spawn-guard-audience.test.py, which fails
    the moment hooks.json and the declaration disagree either way."""
    if audience != APP:
        return guards, []
    rows = {row["id"]: row for row in AUDIENCE.load()}
    kept, notes = [], []
    for label, source, cmd in guards:
        row = rows.get(guard_name(cmd))
        if row is None:
            notes.append("not run (nobody has classified it): %s" % guard_name(cmd))
        elif row["audience"] == AUDIENCE.USER_WORK:
            kept.append((label, source, cmd))
        else:
            notes.append("not run (its reason is the development session's, not this user's): %s"
                         % guard_name(cmd))
    return kept, notes


def app_problems_from(results):
    """A refusal the app's user can still see, IN THE APP'S OWN WORDS.

    THE RAW GUARD OUTPUT NEVER LEAVES THIS FUNCTION, on either channel. Not
    because it would be embarrassing, but because the sentence the front desk
    says is read off whatever this command printed: `mega-lander/app.py`'s
    `run()` raises with our stderr, the work tool hands that to the model, and
    the model says it. A guard's own text is written for an operator in a
    terminal — it names files, flags and acknowledgement lines — and on a
    user's screen it is noise wearing the costume of an explanation. So the
    declaration carries the app's sentence for every guard the app runs, and
    that sentence is the only thing that comes out of here. A developer who
    needs the guard's own words re-runs the same command with `--audience
    operator`, which prints all of it."""
    out = []
    for r in results:
        if r["verdict"] != "blocked":
            continue
        message = AUDIENCE.user_message(r["name"])
        out.append({"headline": message or
                    "I could not set this work up safely, so I did not start it. "
                    "Nothing was created.", "detail": ""})
    return out


def redacted_for_app(results):
    """The guard table, with every guard's raw output replaced by the declared
    reason. Emitted on stdout for anyone inspecting a run; there is deliberately
    nothing operator-worded left in it, so no future reader of this command's
    stdout can leak one to a user."""
    rows = {row["id"]: row for row in AUDIENCE.load()}
    out = []
    for r in results:
        row = rows.get(r["name"], {})
        clean = dict(r)
        clean["output"] = row.get("reason", "")
        out.append(clean)
    return out


def expand(cmd, project_dir):
    for var, val in (("CLAUDE_PROJECT_DIR", project_dir), ("CLAUDE_PLUGIN_ROOT", ENGINE)):
        cmd = cmd.replace("${%s}" % var, val).replace("$%s" % var, val)
    return cmd


def run_guards(guards, envelope, project_dir):
    """Run every guard against one envelope and return every verdict. NEVER
    stops at the first refusal: reporting one problem per attempt is the cost
    this command exists to remove.

    THE VERDICT IS THE PLATFORM'S, NOT A GUESS. Claude Code blocks a tool call
    on exit code 2, or on a JSON stdout that denies it; any other nonzero is a
    non-blocking error, which is reported loudly and separately because a guard
    that cannot run protected nothing."""
    env = dict(os.environ)
    env["CLAUDE_PROJECT_DIR"] = project_dir
    env.setdefault("CLAUDE_PLUGIN_ROOT", ENGINE)
    env["RICHOS_SPAWN_CHECK"] = "1"
    results = []
    for label, source, cmd in guards:
        line = expand(cmd, project_dir)
        t0 = time.time()
        try:
            p = subprocess.run(["bash", "-c", line], input=envelope, capture_output=True,
                               text=True, cwd=project_dir, env=env, timeout=180)
            rc, out, err = p.returncode, p.stdout, p.stderr
        except subprocess.TimeoutExpired:
            rc, out, err = 124, "", "the guard did not finish inside 180s"
        except OSError as exc:
            rc, out, err = 127, "", str(exc)
        verdict = "ok"
        if rc == 2:
            verdict = "blocked"
        elif rc != 0:
            verdict = "error"
        else:
            try:
                j = json.loads(out)
            except ValueError:
                j = None
            if isinstance(j, dict):
                hso = j.get("hookSpecificOutput") or {}
                if hso.get("permissionDecision") == "deny" or j.get("decision") == "block":
                    verdict = "blocked"
        results.append({"name": guard_name(cmd), "source": label, "source_path": source,
                        "command": line, "rc": rc, "verdict": verdict,
                        "output": (err.strip() or out.strip()), "seconds": round(time.time() - t0, 1)})
    return results


# ---------------------------------------------------------------------------
# 4. THE PAYLOAD
# ---------------------------------------------------------------------------

def build_payload(args, brief, workspaces, targets=None):
    """ONE `cross-repo-worktree:` LINE PER WORKSPACE, and the guard has always
    been written that way: clause 4c of guard-worktree-isolation.sh reads EVERY
    such line and checks each against the registry, and register_spawn collects
    them all into `cc_paths` and requires a registration for each. So an agent
    with workspaces in two other repositories needs no new marker and no new
    clause - it needs both of its lines written."""
    prompt = brief.rstrip("\n")
    if workspaces:
        have = [W.realpath(p) for p in W.prompt_lines(prompt, "cross-repo-worktree")]
        wrong = [p for p in have if p not in workspaces]
        if wrong:
            raise Refusal("the brief already names a workspace on a 'cross-repo-worktree:' line (%s) "
                          "that this command is not creating; it would create %s. Remove the line and "
                          "let it be written, or pass --dir %s."
                          % (wrong[0], ", ".join(workspaces), os.path.dirname(wrong[0])))
        missing = [w for w in workspaces if w not in have]
        if missing:
            prompt = "\n".join("cross-repo-worktree: %s" % w for w in missing) + "\n\n" + prompt
    ti = {"name": args["name"], "subagent_type": args["type"], "prompt": prompt,
          "isolation": "worktree"}
    if args.get("description"):
        ti["description"] = args["description"]
    if args.get("model"):
        ti["model"] = args["model"]
    if args.get("effort"):
        ti["effort"] = args["effort"]
    # The acknowledgement contract is prepare-agent-spawn.py's, and it stays
    # prepare-agent-spawn.py's: one answer, one place. This calls it, with the
    # branch each repository's work lands on, so its last step merges that
    # branch (never main into a release branch).
    try:
        return PREPARE.prepare(ti, targets)
    except ValueError as exc:
        raise Refusal(str(exc))


def envelope_for(payload, session, project_dir, planned):
    return json.dumps({
        "session_id": session,
        "cwd": project_dir,
        "hook_event_name": "PreToolUse",
        "tool_name": "Agent",
        "tool_input": payload,
        # THE MARKER, AND WHY IT IS SAFE: clause 7 treats a payload as a dry
        # evaluation only when this is present AND the platform minted no
        # tool_use_id. There is no tool_use_id here because no call has been
        # made; a live call always has one.
        "richos_spawn_check": {"planned": planned},
    })


# ---------------------------------------------------------------------------
# 5. THE REPORT
# ---------------------------------------------------------------------------

def report_problems(title, problems, out=sys.stderr):
    print("", file=out)
    print("spawn: %s - NOTHING WAS CREATED." % title, file=out)
    print("", file=out)
    for i, p in enumerate(problems, 1):
        print("  [%d] %s" % (i, p["headline"]), file=out)
        for line in (p.get("detail") or "").splitlines():
            print("      %s" % line, file=out)
        print("", file=out)
    print("  %d problem(s). Fix them together and run the same command again." % len(problems),
          file=out)


def report_app_refusal(problems, out=sys.stderr):
    """What the app says. Nothing else is on this channel.

    The operator's report opens `spawn: refused by 1 of 9 guard(s) - NOTHING
    WAS CREATED.`, numbers each problem, quotes the guard verbatim and prints a
    table of every guard's verdict. Every one of those is right for a terminal
    and wrong for a person who asked Rich to get something done: the count of
    guards is not his business, `spawn` is not a word he has, and the table
    describes machinery he does not have to know exists. So this prints the
    declared sentence and stops."""
    for p in problems:
        print(p["headline"], file=out)


def problems_from(results):
    out = []
    for r in results:
        if r["verdict"] == "blocked":
            out.append({"headline": "%s REFUSES this spawn  (%s: %s)"
                                    % (r["name"], r["source"], r["source_path"]),
                        "detail": r["output"] or "(the guard printed nothing; exit %d)" % r["rc"]})
    return out


def errors_from(results):
    return [r for r in results if r["verdict"] == "error"]


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

USAGE = """usage: spawn.sh <teammate-name> --repo <repo> [--repo <repo> ...]
                --type <subagent-type> --brief <file>
                [--model <alias>] [--effort <level>] [--description <text>]
                [--base [<repo>=]<ref>] [--dir [<repo>=]<path>]
                [--integration [<repo>=]<branch>]
                [--integration-why <text>] [--payload-out <file>] [--json] [--dry-run]
                [--audience operator|app]

  <teammate-name>      <role>-<model>-<identifier>, e.g. zach-opus-spawn1
  --repo <repo>        absolute path, OR a bare name where it is unambiguous.
                       GIVE IT ONCE PER REPOSITORY this teammate works in: one
                       cc/ workspace is created and registered in each, under
                       the ONE name, and the payload carries one
                       cross-repo-worktree: line per workspace. All of them go
                       together at the land (point 10).
  --type <t>           the subagent_type the definition is registered under
  --brief <file>       the brief, as a file; `-` reads stdin
  --effort <level>     the teammate's effort: low, medium, high, xhigh or max
                       (the levels Claude Code 2.1.292 accepts). Omitted, the
                       payload has no effort key.
  --integration <b>    the branch this body of work integrates on; needed only
                       where the answer is a human's to give, and the refusal
                       says so when it is. With several --repo, scope it:
                       --integration <repo>=<branch>
  --dir <path>         where to put the workspace; with several --repo, scope
                       it the same way: --dir <repo>=<path>
  --audience <who>     whose guards judge this dispatch. `operator` (default)
                       runs every registered PreToolUse[Agent] guard. `app` is
                       the RichOS desktop app dispatching work on its user's own
                       Mac: it runs only the guards
                       spawn-guard-audience.declaration puts on the `user-work`
                       side, and reports a refusal in the app's own words.
  --dry-run            evaluate everything and create nothing
  --json               machine-readable result on stdout instead of the payload

Prints the finished Agent tool payload on stdout, ready to use. Everything a
person reads goes to stderr, so `spawn.sh ... > payload.json` is a payload.
Exit 0 ready; 1 refused (nothing created); 2 usage; 4 created but rolled back.
"""


EFFORT_LEVELS = ("low", "medium", "high", "xhigh", "max")


def parse_args(argv):
    args = {"name": "", "repos": [], "type": "", "brief": "", "model": "", "effort": "", "description": "",
            "bases": [], "dirs": [], "integrations": [], "integration_why": "", "payload_out": "",
            "json": False, "dry_run": False, "project_dir": "", "audience": OPERATOR}
    flags = {"--type": "type", "--brief": "brief", "--model": "model", "--effort": "effort",
             "--description": "description",
             "--integration-why": "integration_why",
             "--payload-out": "payload_out", "--project-dir": "project_dir",
             "--audience": "audience"}
    # REPEATABLE, which is this engine's convention for a list-valued option
    # (`--repo` in handoff-facts.py and ci-surface.py, `--workspace` in
    # containers.py, `--only` in workspace-probes.py are all action="append").
    # A comma list was the alternative and is not used: a repository is a PATH,
    # and a separator a path may legally contain cannot be the one that splits
    # it.
    repeatable = {"--repo": "repos", "--dir": "dirs", "--integration": "integrations",
                  "--base": "bases"}
    i = 0
    while i < len(argv):
        a = argv[i]
        if a in repeatable:
            if i + 1 >= len(argv):
                raise Refusal("%s needs a value" % a)
            args[repeatable[a]].append(argv[i + 1])
            i += 2
        elif a in flags:
            if i + 1 >= len(argv):
                raise Refusal("%s needs a value" % a)
            args[flags[a]] = argv[i + 1]
            i += 2
        elif a == "--json":
            args["json"] = True
            i += 1
        elif a == "--dry-run":
            args["dry_run"] = True
            i += 1
        elif a in ("-h", "--help"):
            sys.stderr.write(USAGE)
            raise SystemExit(2)
        elif a.startswith("-"):
            raise Refusal("unknown option %s" % a)
        else:
            if args["name"]:
                raise Refusal("unexpected argument %r" % a)
            args["name"] = a
            i += 1
    if args["effort"] and args["effort"] not in EFFORT_LEVELS:
        raise Refusal("--effort must be one of %s" % ", ".join(EFFORT_LEVELS))
    if args["audience"] not in (APP, OPERATOR):
        raise Refusal("--audience must be %s or %s" % (APP, OPERATOR))
    return args


def _main(argv):
    try:
        args = parse_args(argv)
    except Refusal as exc:
        sys.stderr.write("spawn: %s\n\n%s" % (exc, USAGE))
        return 2
    missing = [k for k in ("name", "repos", "type", "brief") if not args[k]]
    if missing:
        sys.stderr.write("spawn: missing %s\n\n%s"
                         % (", ".join("repo" if m == "repos" else m for m in missing), USAGE))
        return 2

    project_dir = args["project_dir"] or os.environ.get("CLAUDE_PROJECT_DIR") or os.getcwd()
    project_dir = W.main_checkout(project_dir) or W.realpath(project_dir)
    session = W.current_session()
    notes = []
    problems = []

    # --- everything that can be decided without creating anything -----------
    try:
        if not session:
            raise Refusal("this session is not recorded, so nothing spawned from it could be "
                          "registered (point 12). Start the session through the engine, or export "
                          "RICHOS_SESSION_ID.")
        try:
            brief = sys.stdin.read() if args["brief"] == "-" else open(args["brief"],
                                                                       encoding="utf-8").read()
        except OSError as exc:
            raise Refusal("--brief %s: %s" % (args["brief"], exc))
        if not brief.strip():
            raise Refusal("--brief %s is empty. The prompt carries the whole task." % args["brief"])

        repos = []
        for raw in args["repos"]:
            r = resolve_repo(raw, project_dir)
            if r in repos:
                # Point 3's "one registration" rule, per repository: a second
                # workspace in the SAME repository under one name is refused,
                # here and in create-teammate-worktree.sh. A DIFFERENT
                # repository under the same name is the whole point of this.
                raise Refusal("--repo %s names %s, which is already this teammate's repository. One "
                              "workspace per repository per teammate: a second one in the same "
                              "repository under the same name is refused." % (raw, r))
            repos.append(r)
        notes.append("repositor%s %s" % ("y:  " if len(repos) == 1 else "ies:", ", ".join(repos)))

        dirs = scoped_values(args["dirs"], args["repos"], repos, "--dir")
        bases = scoped_values(args["bases"], args["repos"], repos, "--base")
        integrations = scoped_values(args["integrations"], args["repos"], repos, "--integration")

        # Point 14 for EVERY repository this teammate will work in - the
        # session's own included, because register_spawn checks the entity too.
        own = {}                # repository -> the branch this teammate alone lands on there
        lands_on = {}           # repository -> the branch this teammate's work there lands on
        targets = []            # (repository, the branch its work lands on), for the last step
        for r in ([project_dir] if project_dir not in repos else []) + repos:
            note, mine = ensure_integration(r, integrations.get(r, ""),
                                            args["integration_why"] if r in integrations else "",
                                            session, native_only=(r == project_dir))
            notes.append(note)
            if mine:
                own[r] = mine
            lands_on[r] = mine or (W.integration_record(r) or {}).get("branch") or ""
            targets.append((r, lands_on[r] or "main"))

        workspaces = []
        planned = []
        for r in repos:
            if r == project_dir:
                notes.append("workspace:   native isolation only for %s (it IS this session's "
                             "repository)" % r)
                continue
            w = W.realpath(dirs[r]) if r in dirs else W.realpath(
                os.path.join(os.path.dirname(r), os.path.basename(r) + "-wt", args["name"]))
            if w in workspaces:
                raise Refusal("two of this teammate's workspaces would be at the same path (%s); "
                              "scope them with --dir <repo>=<path>." % w)
            workspaces.append(w)
            planned.append({"path": w, "repo": r, "branch": W.CC_PREFIX + args["name"]})
            notes.append("workspace:   %s  (%s%s in %s)" % (w, W.CC_PREFIX, args["name"], r))

        payload = build_payload(args, brief, workspaces, targets)
        # THE PROVENANCE OF THE BRIEF ITSELF, decided here so the guards evaluate the
        # text that will actually be dispatched. It never refuses: a statement with no
        # source is named to the agent, and the agent re-derives it. On a brief that
        # sources everything, nothing is appended and the payload is byte-identical.
        # Both annotators read ONE repository's record (brief-provenance resolves
        # the brief's citations in it; brief-scope reads its verdict history), so
        # they are given the FIRST --repo: the repository this work is primarily
        # in. With one --repo that is exactly today's behavior.
        payload["prompt"], findings = PROV.annotate(payload["prompt"], repos[0])
        # WHO CHOOSES THE MECHANISM, appended for the same reason and in the same
        # place. It fires only when the brief anchors an item to a spec point that
        # a previous round left red — the CEO's own scoping clause for a prescribed
        # design, computed from the verdict history rather than read off the prose.
        # On every other brief it appends nothing and the payload is unchanged.
        payload["prompt"], stamp_notes = SCOPE.annotate(payload["prompt"], repos[0])
        for _n in stamp_notes:
            notes.append("design:      %s" % _n)
        # THE TOOLKIT'S INDEX, for a QA-type teammate only. The CEO, 2026-09-20:
        # "what else must be done to ensure the QA toolkit actually gets used?"
        # Its README's rule is that a QA brief names the tool from it, and that
        # rule fails in one way — the brief does not, because whoever wrote it
        # did not have the list in front of them. So the list rides with the
        # dispatch, read from the governed repository at spawn time so it can
        # never be a stale copy. The TYPE decides (QA_TOOLKIT_AGENTS), never the
        # prose. Given EVERY repository, not just the first: the toolkit lives
        # in one of them, which need not be the one this work is primarily in.
        # It never refuses; a toolkit it cannot read is a note, not a dead spawn.
        payload["prompt"], qa_notes = QATOOLS.annotate(payload["prompt"], args["type"], repos)
        for _n in qa_notes:
            notes.append("qa tools:    %s" % _n)
        # THE BRIEF'S OWN EVIDENCE, RUN. On 2026-09-28 a brief said a quoted `git log`
        # "shows no fix"; run, it printed 40 lines including all ten fix commits, and the
        # teammate spent 75 minutes finding that out. Each command the brief quotes as
        # evidence is run once here, read-only and time-limited (brief-done.py says
        # exactly which programs), and its real output is appended beside the claim.
        # Appended LAST, so no earlier annotator reads command output as the lead's prose.
        # Never for the app: its dispatches are the user's, not the operator's (see
        # spawn-guard-audience.declaration), and nothing is run on a user's behalf here.
        # A brief that quotes no evidence command is byte-identical after this.
        if args["audience"] == OPERATOR:
            payload["prompt"], ev_notes = DONE.annotate(payload["prompt"], repos)
            for _n in ev_notes:
                notes.append("evidence:    %s" % _n)
    except (Refusal, W.SpecError) as exc:
        report_problems("refused", [{"headline": str(exc), "detail": ""}])
        if args["json"]:
            # Only this phase can attest that no workspace creation was attempted.
            # Later failures may leave state behind even when rollback was attempted.
            print(json.dumps({"ready": False, "phase": "preflight", "created": []}))
        return 1

    registered = collect_guards(project_dir)
    if not registered:
        report_problems("refused", [{
            "headline": "no PreToolUse[Agent] guard was found on any surface",
            "detail": "Sources looked at:\n  " + "\n  ".join(p for _l, p in
                                                             settings_sources(project_dir)) +
                      "\nRefusing to call that a pass: an unguarded spawn is not a verified one."}])
        return 1
    # WHOSE GUARDS JUDGE THIS DISPATCH. A declaration that cannot be read is a
    # refusal and never an empty allowlist: the app's guard surface is supposed
    # to be a declared list, and an unreadable list is not a list.
    try:
        guards, audience_notes = for_audience(registered, args["audience"])
    except AUDIENCE.DeclarationError as exc:
        report_problems("refused", [{
            "headline": "the spawn-guard classification could not be read, so nothing here "
                        "knows which guards judge this dispatch",
            "detail": str(exc)}])
        return 1
    for note in audience_notes:
        notes.append("audience:    %s" % note)
    if not guards:
        # The same rule as "no guard was found", one level in: an app dispatch
        # that ends up with an empty user-work list is unguarded, and an
        # unguarded spawn is not a verified one whoever it is for.
        report_problems("refused", [{
            "headline": "no guard of this audience (%s) is registered for this dispatch"
                        % args["audience"],
            "detail": "Registered here: %s\nRefusing to call that a pass."
                      % ", ".join(guard_name(c) for _l, _p, c in registered)}])
        return 1

    pre = run_guards(guards, envelope_for(payload, session, project_dir, planned), project_dir)
    problems = (app_problems_from(pre) if args["audience"] == APP else problems_from(pre))
    if problems:
        if args["audience"] == APP:
            report_app_refusal(problems)
        else:
            report_problems("refused by %d of %d guard(s)" % (len(problems), len(guards)), problems)
            _print_guard_table(pre)
        return 1

    if args["dry_run"]:
        print("", file=sys.stderr)
        print("spawn: --dry-run - every check passed and nothing was created.", file=sys.stderr)
        _print_report(notes, pre, guards, None, args, findings)
        return _emit(payload, args, pre, notes, created=None)

    # --- create ------------------------------------------------------------
    # WHOSE REGISTRATION IS IT? Asked HERE, before anything is created, because
    # after a failure it can no longer be told apart. A rollback may only undo
    # what THIS run made: on 2026-09-13 an earlier draft of this file, with the
    # spawn guard standing down in a test repository that had not adopted the
    # engine, reused a name, failed to create, and withdrew the EXISTING
    # workspace of that name - deleting a live teammate's tree and branch while
    # reporting a refusal. `withdraw_cc` cannot see the difference (the record
    # looks identical either way), so the caller carries the answer.
    mine = not os.path.exists(W.agent_path(W.named_key(session, args["name"])))
    created = None
    # AN INTERRUPT IS A FAILURE OF OUR OWN MAKING TOO (see main()): from here on a
    # SIGTERM, SIGINT or SIGHUP raises _Interrupted, and main() runs the same _rollback
    # a refusal uses, with the facts recorded here.
    _CREATING.update(session=session, name=args["name"], mine=mine,
                     quiet=args["audience"] == APP)
    _arm_interrupt()
    # A BRANCH OF ITS OWN IN THE SESSION'S OWN REPOSITORY has no cc/ workspace to
    # carry it, so it is registered here, before the spawn, as register_cc does
    # for a cc/ workspace; the rollback below withdraws it with the rest.
    if project_dir in own:
        try:
            W.bind_own_target(session, args["name"], project_dir, own[project_dir])
        except W.SpecError as exc:
            report_problems("could not bind %s to %s" % (args["name"], own[project_dir]),
                            [{"headline": str(exc), "detail": ""}])
            _rollback(session, args["name"], mine)
            return 1
    # EVERY WORKSPACE, UNDER THE ONE NAME, AND A FAILURE OF ANY OF THEM UNDOES
    # ALL OF THEM. `withdraw_cc` withdraws the whole registration - every
    # workspace on it, its branches and the record - so a half-created teammate
    # never survives this loop (points 3, 10).
    for p in planned:
        cmd = [os.path.join(ENGINE, "mega-lander", "create-teammate-worktree.sh"), p["repo"],
               args["name"], "--dir", p["path"], "--session", session]
        # A WORKSPACE STARTS FROM THE BRANCH ITS WORK LANDS ON, never from
        # whatever the main checkout has checked out. With the repository
        # recorded on v1.3.0 and its main checkout on main, no --base used to
        # be passed, so the workspace started at HEAD (main), and its land
        # into v1.3.0 would have pulled main in (review
        # rv-20261014T105153Z-f22eff3f-da5a). An explicit --base still wins.
        if p["repo"] in bases or lands_on.get(p["repo"]):
            cmd += ["--base", bases.get(p["repo"]) or lands_on[p["repo"]]]
        if p["repo"] in own:
            cmd += ["--integration", own[p["repo"]]]
        rc, out, err = sh(cmd, timeout=300)
        if rc != 0:
            report_problems("could not create the workspace in %s" % p["repo"],
                            [{"headline": "create-teammate-worktree.sh exited %d" % rc,
                              "detail": (err.strip() or out.strip())}])
            _rollback(session, args["name"], mine)
            return 1
    if planned:
        created = [p["path"] for p in planned]

    # --- the same guards again, against the real thing ----------------------
    post = run_guards(guards, envelope_for(payload, session, project_dir, []), project_dir)
    problems = (app_problems_from(post) if args["audience"] == APP else problems_from(post))
    if problems:
        if args["audience"] == APP:
            # Still the app's own words. The rollback below is what makes the
            # declared sentence "Nothing was created." true on this path too.
            report_app_refusal(problems)
        else:
            report_problems("refused AFTER the workspace was created - rolling it back", problems)
            _print_guard_table(post)
        _rollback(session, args["name"], mine, quiet=args["audience"] == APP)
        return 4

    _print_report(notes, post, guards, created, args, findings)
    return _emit(payload, args, post, notes, created)


class _Interrupted(BaseException):
    """A termination signal arrived after creation began. BaseException so no
    `except Exception` between the signal and main() can swallow it."""


_SIGNALS = ("SIGTERM", "SIGINT", "SIGHUP")
_CREATING = {}


def _arm_interrupt():
    def _raise(signum, _frame):
        raise _Interrupted(signal.Signals(signum).name)
    for n in _SIGNALS:
        signal.signal(getattr(signal, n), _raise)


def _disarm_interrupt(ignore=False):
    """`ignore` while the rollback itself runs, so a second signal cannot cut it short."""
    for n in _SIGNALS:
        signal.signal(getattr(signal, n), signal.SIG_IGN if ignore else
                      (signal.default_int_handler if n == "SIGINT" else signal.SIG_DFL))


def main(argv):
    """Rich stopped a running spawn on 2026-10-06 and it left a registered, never-started
    workspace and its branch, which stall-watch then reported as a silent agent. The
    handlers are armed only once creation starts (before that nothing exists). The signal
    raises out of whatever is running, so subprocess.run kills the creating child on the
    way, and the very same _rollback a refusal uses runs. Exit 4: created and rolled back."""
    try:
        return _main(argv)
    except _Interrupted as exc:
        _disarm_interrupt(ignore=True)
        print("spawn: interrupted by %s - rolling back." % exc, file=sys.stderr)
        _rollback(_CREATING["session"], _CREATING["name"], _CREATING["mine"],
                  quiet=_CREATING["quiet"])
        return 4
    finally:
        _disarm_interrupt()


def _rollback(session, name, mine, quiet=False):
    """On ANY failure of OUR OWN making, leave nothing behind.

    `quiet` silences the operator-facing narration for an app dispatch. It never
    silences a ROLLBACK FAILURE: that one means real state is left on the user's
    machine, and §54 is that garbage either gets cleaned up or somebody is told
    about it loudly.

    `mine` is false when a registration of this name already existed when this
    run started. Then the failure is "that name is taken", the workspace belongs
    to somebody else, and withdrawing it would delete a live teammate's tree and
    branch. `withdraw_cc` refuses anything that ever RAN, which is a different
    question and does not cover this one: a registered, created, not-yet-spawned
    workspace looks the same whoever made it."""
    if not mine:
        if not quiet:
            print("  left alone: %s was already registered before this run, so its workspace is "
                  "not this command's to withdraw. Pick a fresh identifier." % name,
                  file=sys.stderr)
        return
    try:
        W.withdraw_cc(session, name, "its spawn was refused before it was made")
        if not quiet:
            print("  rolled back: workspace, branch and registration withdrawn; the name %s is "
                  "free again." % name, file=sys.stderr)
    except W.SpecError as exc:
        if "there is no registration" not in str(exc):
            print("  ROLLBACK FAILED: %s" % exc, file=sys.stderr)
            print("  Nothing was spawned into it. Clean it up with: workspaces.sh discard %s "
                  "--reason '<why>' --not-ceo-ordered '<why>'" % name, file=sys.stderr)


def _print_guard_table(results, out=sys.stderr):
    print("  guards evaluated (all of them, every time):", file=out)
    for r in results:
        mark = {"ok": "ok     ", "blocked": "REFUSED", "error": "ERROR  "}[r["verdict"]]
        print("    %s %-38s %-8s %5.1fs" % (mark, r["name"], r["source"], r["seconds"]), file=out)
    print("", file=out)


def _print_report(notes, results, guards, created, args, findings=()):
    out = sys.stderr
    print("", file=out)
    print("spawn: READY - %s" % args["name"], file=out)
    for n in notes:
        print("  %s" % n, file=out)
    for path in (created or []):
        print("  created:     %s" % path, file=out)
    ok = len([r for r in results if r["verdict"] == "ok"])
    print("  guards:      %d evaluated, %d passed (%s)"
          % (len(guards), ok, ", ".join(sorted(set("%s:%d" % (lbl, len([g for g in guards
                                                                        if g[0] == lbl]))
                                                   for lbl, _p, _c in guards)))), file=out)
    print("", file=out)
    _print_guard_table(results)
    PROV.report(list(findings), args["name"], out=out)
    print("", file=out)
    for r in errors_from(results):
        print("  NOTE: %s could not run (exit %d) - it is a non-blocking error for the platform "
              "too, but it protected nothing here:\n    %s" % (r["name"], r["rc"], r["output"]),
              file=out)
    print("  The payload is on stdout. It is complete: pass it to the Agent tool as-is.", file=out)
    print("", file=out)


def _emit(payload, args, results, notes, created):
    text = json.dumps(payload, indent=2)
    if args["payload_out"]:
        try:
            with open(args["payload_out"], "w", encoding="utf-8") as f:
                f.write(text + "\n")
            print("  payload also written to %s" % args["payload_out"], file=sys.stderr)
        except OSError as exc:
            print("spawn: could not write --payload-out %s: %s" % (args["payload_out"], exc),
                  file=sys.stderr)
            return 1
    if args["json"]:
        # In `app` audience the guard table carries the DECLARED REASON in place
        # of each guard's own output, so there is nothing operator-worded left
        # on this channel either. `mega-lander/app.py` reads this JSON and could
        # one day put part of it in front of a person; that must not be a thing
        # anyone has to remember.
        table = redacted_for_app(results) if args["audience"] == APP else results
        print(json.dumps({"ready": True, "payload": payload, "created": created, "notes": notes,
                          "audience": args["audience"], "guards": table}, indent=2))
    else:
        print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
