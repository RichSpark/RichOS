#!/usr/bin/env bash
#
# scripts/lib/record-canary.sh — DID THIS RUN TOUCH THE OPERATOR'S RECORD?
#
# ===========================================================================
# WHERE THIS CAME FROM — three instances of one class in two days
# ===========================================================================
# The leak canary (leak-canary.sh) watches CHECKOUTS. It says so in its own
# header: "It does NOT watch the rest of the filesystem or $HOME." Three suites
# then wrote into $HOME, and every one of them was green:
#
#   2026-09-10  finish-row-completion.test.sh wrote 130 `feedbeef` fixture rows
#               into ~/.claude/worker-events.jsonl, the platform's fallback
#               event log (Sage D4, round three).
#   2026-09-10  detect-nonnative-worktree.test.sh created
#               ~/.claude/teams/session-deadbeef/ with 25 KB of fixture names
#               (round 14, D4's sibling).
#   2026-09-11  session-start-stdin.test.sh case 9b ran the real SessionStart
#               reaper with a sandbox entity and an unredirected HOME. The
#               reaper read the operator's REAL team directories, discovered
#               the operator's REAL richos registry, judged a REAL agent's
#               cross-repository tree with an entity in which its shell could
#               not exist, and appended a `terminated` row (witness
#               platform-terminal-record, 2026-09-11T00:02:34.984941+00:00,
#               agent ae904aac1949e5696, sage-fable-cert3) to
#               ~/.claude/state/worktree-ledger.jsonl — for an agent whose
#               shell was LOCKED by a running pid. Both round-four reviewers
#               found it independently; the suite reported all 11 passed.
#
# Each was fixed in its suite with a both-arms control (F15/F16, L1/L2, 9l/9m).
# This file closes the CLASS: the runner takes a baseline of the operator's
# record before EVERY suite and compares after it, so the next suite that
# reaches $HOME is named on the day it is written, not found by a reviewer.
#
# ===========================================================================
# WHAT IS WATCHED, AND WHAT THE WITNESS IS FOR EACH
# ===========================================================================
# `<config>` is `${CLAUDE_CONFIG_DIR:-$HOME/.claude}` AS IT WAS WHEN THIS FILE
# WAS SOURCED — captured, never re-read, for the reason global-state-witness.sh
# gives: a suite that moves HOME halfway through would otherwise be compared
# against its own sandbox and always pass.
#
#   <config>/state/worktree-ledger.jsonl   THE OWNERSHIP LEDGER. Every row is
#        witnessed. A row's event or agent label cannot establish who wrote it.
#   <config>/worker-events.jsonl           THE FALLBACK EVENT LOG. Witness:
#        every line, by content hash. Only a session with NO team directory
#        writes here (worker-*-handoff.sh resolve_team_dir), so on a machine
#        where the running session has one it is stable across a run (250
#        lines before and after both round-four passes).
#   <config>/teams/                        THE TEAM DIRECTORIES. Witness: the
#        entry names plus recursive file identity, metadata and content.
#   <config>/state/workspaces/             THE WORKSPACE REGISTRY
#        (mega-lander/workspaces.py). Witness: the names of its session and
#        agent records and every events.jsonl line, by content hash. Recursive
#        file witnesses also catch edits, removals and restored content. A suite
#        that registers, lands or discards against it is writing the record
#        the Stop gate and the lock-out decide from (added 2026-09-11, the day
#        two suites wrote 55 test registrations into it before they were
#        sandboxed).
#
# ITS FALSE-POSITIVE VECTORS, NAMED: a real spawn during the run (a
# `registered`/`prepared` row from detect-nonnative-worktree.sh or
# create-teammate-worktree.sh), a real reap or removal (a `terminated` row), a
# session with no team directory writing to the fallback, or the platform
# creating a team directory or a first log file of a type in one. Each prints
# the row or entry it saw, so a reader recognizes the machine's own activity
# at once — the same trade the leak canary made for an engineer's own save.
# In CI none of the three paths exists, so there the witness is exact.
#
# This snapshot does not attribute shared-record writers. A change in a mutable
# shared record remains unresolved. An isolated execution boundary must prevent
# unrelated activity from changing the observed records; event-name exemptions
# cannot supply that boundary. Kernel change times catch ordinary write/restore
# operations but are not a privileged audit of arbitrary filesystem activity.
# ===========================================================================
# AMENDED 2026-09-27 — THE RUNNERS NOW HAND EACH UNIT A RECORD OF ITS OWN
# ===========================================================================
# The paragraph above called those vectors a readable trade. On a busy session
# they stopped being one. The canary compared the LIVE record before and after
# a unit, so every spawn, land and message elsewhere on the machine during the
# unit failed it as RECORD-TOUCHED: zach-opus-amspell3's proof went red three
# times on `contract-integrity.test.sh --only SCR`, and each diff held only
# other agents' spawn and land events. Engine proofs could pass only on an idle
# session, so every engine change waited for quiet.
#
# The question this canary exists for is "did THIS unit write to the record?",
# and a before/after diff of a shared file cannot answer it while anything else
# writes there. So the runners (ci-shard.sh, run-all-tests.sh) call
# rc_sandbox <home> before each unit: it builds a throwaway home, points the
# canary at the record INSIDE it, and fills RC_SANDBOX_ENV with the env(1)
# arguments the unit is run under: HOME set to that home (the ownership ledger
# resolves from expanduser("~") alone, so moving only the config directory
# would leave it on the operator's file), and every variable whose value is the
# operator's record or lies inside it removed — CLAUDE_CONFIG_DIR and
# RICHOS_WORKSPACES_DIR included whenever they point there, so the engine's own
# defaults (<HOME>/.claude, <config>/state/workspaces) resolve inside the home.
# They are REMOVED, never SET: a unit that moves HOME or CLAUDE_CONFIG_DIR into
# a world of its own expects the other names to follow from its choice, and an
# exported RICHOS_WORKSPACES_DIR overrode that for scratch-reaper.test.sh S30
# on this change's first proof run (its session records went unread).
# Nothing else on the machine writes into that home, so the diff is exactly the
# unit's own writes: a unit that WOULD have written the operator's record
# writes this one instead, and is still red.
#
# The throwaway home is CI's shape, measured against engine-self-verify.yml: no
# record at all, and a git identity (the workflow declares one because fixtures
# commit). Nothing of the operator's own home is carried in; in particular not
# the operator's git configuration, which is read from $HOME.
#
# The home lies inside the runner's temp directory. A unit that makes a claim
# about the account's REAL home reads it from the password database, as
# disk-watchdog.test.sh W22d does: judged against the throwaway home, the
# declared $TMPDIR is "a folder holding the home" — a verdict about the runner.
#
# ===========================================================================
# AMENDED 2026-10-28 — THE APP'S SESSION IS A ROAD BACK TOO
# ===========================================================================
# A worker shell the RichOS app starts carries the app's session in its
# environment: the INSTALLED engine (RICHOS_ENGINE_ROOT, RICHOS_ENGINE_DIR), the
# app's live records (RICHOS_WORKSPACES_DIR, RICHOS_AGENT_HOLD_DIR,
# STALL_WATCH_STATE_DIR, RICHOS_SA_TEAMS_DIR, RICHOS_APP_STATE), its
# conversation (RICHOS_APP_ENTITY, RICHOS_APP_THREAD, RICHOS_SESSION_ID, ...)
# and its coordination entity (RICHOS_ENTITY_ROOT, and the launching Claude
# session's CLAUDE_PROJECT_DIR, under which left-off.test.sh wrote its fixture
# left-off.*.state files into the live coordination checkout). None of them lies under
# CLAUDE_CONFIG_DIR, so the rule above handed every one to the unit, and a unit
# then tested the installed engine instead of the one under test, spoke to the
# app's audience instead of the terminal's, and wrote into the app's live
# registry: on 2026-10-28 record-canary.test.sh 7d'' wrote its fixture wait
# record into engine-state/workspaces/<thread>/agents/wait-regression.json, and
# twelve checks of one proof run (locate-engine, pierce, spawn, stall-watch,
# contract-integrity Q, ...) failed only in the app's shells. CI and the nightly
# have none of these names (nightly-launch.sh starts the app under env -i), so
# removing them is CI's shape, as the throwaway home is. They are REMOVED,
# never set, by NAME (APP_SESSION_NAMES and every RICHOS_APP_* below): a value
# like a thread id is no path, so the path rule above cannot find it. Machine
# settings (RICHOS_BULK_ROOT, RICHOS_CLAUDE_BIN, ...) are not the session and
# pass through; a unit that tests app behaviour sets the names it needs itself.
#
# WHAT IT CANNOT SEE NOW, named: a unit that reaches the operator's record by a
# path fixed BEFORE it started (an absolute path baked into a script) or from
# the password database (pwd.getpwuid) rather than $HOME. Measured 2026-09-27:
# the engine's only two getpwuid readers (cpu_guard.py hook(), probe Q7) use it
# to DECLINE real state when HOME is redirected, never to write it.
#
# A PATH IT CANNOT READ IS A FAILURE, NEVER A QUIET PASS. RC_HEALTHY carries
# it, exactly as LC_HEALTHY does for the leak canary.
#
# Safe to source repeatedly. Never writes anything under <config>.

if [ -n "${_RECORD_CANARY_SH_SOURCED:-}" ]; then
    return 0 2>/dev/null || true
fi
_RECORD_CANARY_SH_SOURCED=1

# rc_use_config <dir> — point the canary at the record under <dir>. Explicit,
# never re-read from the environment (see the header).
rc_use_config() {
    RC_CFG="$1"
    RC_LEDGER="$RC_CFG/state/worktree-ledger.jsonl"
    RC_FALLBACK="$RC_CFG/worker-events.jsonl"
    RC_TEAMS="$RC_CFG/teams"
    RC_WORKSPACES="$RC_CFG/state/workspaces"
}
rc_use_config "${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
# The operator's record and home as they were when this file was sourced: what
# rc_sandbox keeps a unit away from, and what a runner names in its banner.
RC_LIVE_CFG="$RC_CFG"
RC_LIVE_HOME="${HOME:-}"
RC_HEALTHY=1
RC_SANDBOX_ENV=()

# _rc_roads_back — the name of every environment variable whose value is the
# operator's record or a path inside it, and of every variable of the app's
# session (the header, "AMENDED 2026-10-28"), one per line. A function of its
# own so the heredoc is not nested inside a command substitution, which bash 3.2
# (the operator's /bin/bash) can misparse.
_rc_roads_back() {
    python3 - "$RC_LIVE_CFG" "${RC_LIVE_HOME:+$RC_LIVE_HOME/.claude}" <<'PY'
import os, sys
APP_SESSION_NAMES = {
    "RICHOS_ENGINE_ROOT", "RICHOS_ENGINE_DIR", "RICHOS_ENTITY_ROOT", "RICHOS_SA_ENTITY_ROOT",
    "RICHOS_SA_TEAMS_DIR", "RICHOS_WORKSPACES_DIR", "RICHOS_AGENT_HOLD_DIR", "STALL_WATCH_STATE_DIR",
    "RICHOS_AGENT_OWNER", "RICHOS_AGENT_SESSION", "RICHOS_SESSION_ID", "RICHOS_SESSION_PID",
    "RICHOS_SPAWN_HOOK_SOURCES", "RICHOS_CLAUDE_ACCOUNT", "CLAUDE_PROJECT_DIR",
}
roots = []
for r in sys.argv[1:]:
    if r:
        roots.append(r.rstrip("/"))
        roots.append(os.path.realpath(r).rstrip("/"))
for name, value in sorted(os.environ.items()):
    if name == "HOME" or not name.replace("_", "a").isalnum():
        continue
    if name in APP_SESSION_NAMES or name.startswith("RICHOS_APP_"):
        print(name)
    elif any(value == r or value.startswith(r + "/") for r in roots if r):
        print(name)
PY
}

# rc_sandbox <home> — give ONE unit a record of its own (see the header).
# Creates <home> with a git identity and nothing else, points the canary at
# <home>/.claude, and sets RC_SANDBOX_ENV to the env(1) arguments to run the
# unit under:  env "${RC_SANDBOX_ENV[@]}" <argv...>
# Returns 1, with the canary left pointing at nothing it could vouch for, when
# the home cannot be built; a runner reports that as a blind canary.
rc_sandbox() {
    local home="$1" name names
    RC_SANDBOX_ENV=()
    rc_use_config "$home/.claude"
    mkdir -p "$home" 2>/dev/null || return 1
    home="$(cd "$home" 2>/dev/null && pwd -P)" || return 1
    rc_use_config "$home/.claude"
    mkdir -p "$RC_CFG/state" 2>/dev/null || return 1
    : >"$RC_CFG/state/scratch-ledger.jsonl" || return 1
    printf '[user]\n\tname = richos-engine-unit\n\temail = richos-engine-unit@users.noreply.github.com\n' \
        >"$home/.gitconfig" 2>/dev/null || return 1
    # Every variable whose value is the operator's record or inside it is
    # removed, whatever it is called, and so is every variable of the app's
    # session: a unit is handed no road back to either. Python reads the
    # environment because a value may hold a newline. If it cannot run, the
    # unit is NOT started with a road left open.
    names="$(_rc_roads_back)" || return 1
    for name in $names; do
        RC_SANDBOX_ENV+=(-u "$name")
    done
    # HOME is the one name SET. CLAUDE_CONFIG_DIR is removed above when it
    # points at the operator's record and RICHOS_WORKSPACES_DIR always (it is
    # the app's session); neither is ever set: their defaults then resolve
    # inside this home, and a unit that builds a world of its own keeps the
    # resolution it chose (see the header).
    RC_SANDBOX_ENV+=("HOME=$home")
    return 0
}

# rc_paths — the watched paths, one per line (for a banner).
rc_paths() { printf '%s\n%s\n%s/\n%s/\n' "$RC_LEDGER" "$RC_FALLBACK" "$RC_TEAMS" "$RC_WORKSPACES"; }

# rc_snapshot — the witnessed state of the record on stdout, one entry per
# line, sorted. Exit 1 (and a line starting with UNREADABLE) when a present
# path could not be read.
rc_snapshot() {
    python3 - "$RC_LEDGER" "$RC_FALLBACK" "$RC_TEAMS" "$RC_WORKSPACES" <<'PY'
import hashlib, json, os, stat, sys
ledger, fallback, teams, workspaces = sys.argv[1:5]
out, unreadable = [], False

def h(raw):
    return hashlib.sha256(raw).hexdigest()[:16]

def witness(path, recursive=True):
    """Keep removals and write/restore visible even when final bytes match."""
    global unreadable
    try:
        before = os.lstat(path)
        if stat.S_ISLNK(before.st_mode):
            # A link can redirect observation outside this execution's record
            # boundary. Do not silently claim coverage of its target.
            raise OSError("record path is a symbolic link: " + path)
        identity = lambda s: (s.st_dev, s.st_ino, s.st_mode, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
        digest = hashlib.sha256()
        if stat.S_ISREG(before.st_mode):
            with open(path, "rb") as stream:
                if identity(os.fstat(stream.fileno())) != identity(before):
                    raise OSError("record replaced before observation: " + path)
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
                if identity(os.fstat(stream.fileno())) != identity(before):
                    raise OSError("record changed during observation: " + path)
        elif not stat.S_ISDIR(before.st_mode):
            raise OSError("unsupported record entry: " + path)
        if identity(os.lstat(path)) != identity(before):
            raise OSError("record replaced during observation: " + path)
        out.append("witness\t%s\t%s\t%s" % (path, identity(before), digest.hexdigest()))
        if recursive and stat.S_ISDIR(before.st_mode):
            for child in sorted(os.listdir(path)):
                witness(os.path.join(path, child))
            if identity(os.lstat(path)) != identity(before):
                raise OSError("record directory changed during observation: " + path)
    except OSError as error:
        out.append("UNREADABLE\t%s\t%s" % (path, error)); unreadable = True

parents = set()
for path in (ledger, fallback, teams, workspaces):
    parent = os.path.dirname(path)
    while not os.path.lexists(parent):
        parent = os.path.dirname(parent)
    parents.add(parent)
    if os.path.lexists(path):
        witness(path)
for parent in sorted(parents):
    # Creating then deleting an initially absent record changes its containing
    # directory. Observe only that directory, never recursively scan HOME.
    witness(parent, recursive=False)
if unreadable:
    print("\n".join(sorted(out)))
    sys.exit(1)

if os.path.lexists(ledger):
    try:
        with open(ledger, "rb") as f:
            for raw in f:
                if not raw.strip():
                    continue
                try:
                    d = json.loads(raw.decode("utf-8"))
                except Exception:
                    out.append("ledger\tunparsable\t%s" % h(raw))
                    continue
                if not isinstance(d, dict):
                    out.append("ledger\tnot-an-object\t%s" % h(raw))
                    continue
                out.append("ledger\t%s\tevent=%s witness=%s source=%s teammate=%s agent=%s ts=%s"
                           % (h(raw), d.get("event"), d.get("witness") or "-", d.get("source") or "-",
                              d.get("teammate") or "-", d.get("agent_id") or "-", d.get("ts") or "-"))
    except OSError as e:
        out.append("UNREADABLE\tledger\t%s" % e); unreadable = True
else:
    out.append("ledger\tabsent")

if os.path.lexists(fallback):
    try:
        with open(fallback, "rb") as f:
            for n, raw in enumerate(f, 1):
                if not raw.strip():
                    continue
                label = ""
                try:
                    d = json.loads(raw.decode("utf-8"))
                    if isinstance(d, dict):
                        label = "event=%s agent=%s session=%s ts=%s" % (
                            d.get("event"), d.get("agent_id") or "-", d.get("session_id") or "-", d.get("timestamp") or "-")
                except Exception:
                    label = "unparsable"
                out.append("fallback\t%s\t%s" % (h(raw), label))
    except OSError as e:
        out.append("UNREADABLE\tfallback\t%s" % e); unreadable = True
else:
    out.append("fallback\tabsent")

if os.path.lexists(teams):
    try:
        for name in sorted(os.listdir(teams)):
            p = os.path.join(teams, name)
            out.append("teams\t%s%s" % (name, "/" if os.path.isdir(p) else ""))
            if os.path.isdir(p) and not os.path.islink(p):
                for child in sorted(os.listdir(p)):
                    out.append("teams\t%s/%s" % (name, child))
    except OSError as e:
        out.append("UNREADABLE\tteams\t%s" % e); unreadable = True
else:
    out.append("teams\tabsent")

if os.path.lexists(workspaces):
    try:
        for sub in ("sessions", "agents", "done"):
            d = os.path.join(workspaces, sub)
            if os.path.isdir(d):
                for name in sorted(os.listdir(d)):
                    out.append("workspaces\t%s/%s" % (sub, name))
        ev = os.path.join(workspaces, "events.jsonl")
        if os.path.exists(ev):
            with open(ev, "rb") as f:
                for raw in f:
                    if not raw.strip():
                        continue
                    label = ""
                    try:
                        d = json.loads(raw.decode("utf-8"))
                        if isinstance(d, dict):
                            label = "event=%s key=%s" % (d.get("event"), d.get("key") or d.get("session_id") or "-")
                    except Exception:
                        label = "unparsable"
                    out.append("workspaces\tevents.jsonl %s %s" % (h(raw), label))
    except OSError as e:
        out.append("UNREADABLE\tworkspaces\t%s" % e); unreadable = True
else:
    out.append("workspaces\tabsent")

print("\n".join(sorted(out)))
sys.exit(1 if unreadable else 0)
PY
}

# rc_baseline <file> — snapshot the record into <file>. Sets RC_HEALTHY=0 if
# any present path could not be read, so a later "untouched" can be refused.
rc_baseline() {
    RC_HEALTHY=1
    if ! rc_snapshot > "$1" 2>/dev/null; then
        RC_HEALTHY=0
    fi
    return 0
}

# rc_escaped <baseline-file> — every entry of the record that APPEARED or
# CHANGED since <baseline-file>, one per line, human-readable. Empty output
# means the record is as it was. A path that became unreadable is reported
# as UNREADABLE, never as unchanged.
rc_escaped() {
    local after
    after="$(mktemp)" || { printf 'UNREADABLE\tcannot allocate record observation\n'; return 0; }
    if ! rc_snapshot > "$after" 2>/dev/null; then
        grep '^UNREADABLE' "$after" || printf 'UNREADABLE\n'
        rm -f "$after"
        return 0
    fi
    python3 - "$1" "$after" <<'PY'
import sys
try:
    before = set(l.rstrip("\n") for l in open(sys.argv[1], encoding="utf-8", errors="replace") if l.strip())
    after = [l.rstrip("\n") for l in open(sys.argv[2], encoding="utf-8", errors="replace") if l.strip()]
except OSError as error:
    print("UNREADABLE\t" + str(error))
    sys.exit(0)
after_paths = {row.split("\t", 2)[1] for row in after if row.startswith("witness\t")}
seen = set()
for line in after:
    if line in before or line in seen:
        continue
    seen.add(line)
    kind, _sep, rest = line.partition("\t")
    if rest in ("absent",):
        continue
    if kind == "ledger":
        print("ledger row APPEARED: %s" % rest.split("\t", 1)[-1])
    elif kind == "fallback":
        print("fallback event log line APPEARED: %s" % rest.split("\t", 1)[-1])
    elif kind == "teams":
        print("team directory entry APPEARED: %s" % rest)
    elif kind == "workspaces":
        print("workspace registry entry APPEARED: %s" % rest)
    elif kind == "witness":
        print("record file CHANGED: %s" % rest.split("\t", 1)[0])
    else:
        print(line)
for line in sorted(before - set(after)):
    kind, _sep, rest = line.partition("\t")
    if kind == "witness" and rest.split("\t", 1)[0] not in after_paths:
        print("record file REMOVED: %s" % rest.split("\t", 1)[0])
# a path that was present and is now absent, or vice versa, is a change too
for kind in ("ledger", "fallback", "teams", "workspaces"):
    was = any(l.startswith(kind + "\t") for l in before)
    now = any(l.startswith(kind + "\t") for l in after)
    was_absent = (kind + "\tabsent") in before
    now_absent = (kind + "\tabsent") in set(after)
    if was_absent and not now_absent:
        print("%s: was ABSENT and now EXISTS" % kind)
    elif now_absent and not was_absent and was:
        print("%s: was present and is now ABSENT" % kind)
PY
    rm -f "$after"
    return 0
}
