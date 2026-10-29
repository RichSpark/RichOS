#!/usr/bin/env python3
"""disk-watchdog.py — the reading, the arithmetic, the alert and the notification.

Invoked by scripts/disk-watchdog.sh, which sources orchestration.config first so
every threshold arrives in the environment already declared. The header on that
file carries the reasoning; this one carries the mechanism.

WHY ONE PROGRAM AND NOT A PIPELINE OF SHELL STEPS: the state file is a
read-modify-write. It holds the previous reading (which the drop arithmetic
subtracts from) and the time each notification level was last sent (which makes
"once a day" possible for a job that has no memory between runs). Split across
processes, two overlapping launchd ticks would each read the file before the
other wrote it, and the CEO would be notified twice for one event.
"""

import calendar
import hashlib
import json
import os
import re
from pathlib import Path
import machine_settings as M
import subprocess
import sys
import time

import foreign_app_data

GB = 1024 ** 3


def env(name, default=""):
    v = (os.environ.get(name) or "").strip()
    return v if v else default


def env_num(name, default):
    v = env(name, "")
    if not v:
        return default
    try:
        return float(v)
    except ValueError:
        return default


def expand(p):
    return os.path.expanduser(os.path.expandvars(p))


def state_base():
    """Where this machine's state files live, HONORING CLAUDE_CONFIG_DIR.

    D9, AND IT IS THE CHEAPEST DEFEAT IN FRANK'S WHOLE REPORT. The sweeper's
    failures_path() has always honored CLAUDE_CONFIG_DIR. This program's reader
    did not, and SCRATCH_FAILURES_STATE was declared in no config file — so one
    exported variable in a shell profile sent the sweeper's failure record to one
    directory and this job's reader to another, and every MASSIVE ALERT about a
    path that could not be deleted went nowhere with no sign that it was off.

    Reconciled in both directions: this helper is the same resolution the sweeper
    uses, and SCRATCH_FAILURES_STATE is now DECLARED in orchestration.config so
    the two cannot drift by default either.
    """
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "state")


# ---------------------------------------------------------------------------
# reading a volume
# ---------------------------------------------------------------------------

def read_volume(mount):
    """(capacity_bytes, free_bytes) or None if it is not mounted.

    os.statvfs, not `df`: one syscall, no parsing, no locale, and no subprocess
    on a job that runs every 15 minutes. f_bavail rather than f_bfree, because
    f_bfree includes blocks reserved for root that nothing else can use — the
    number that matters is the one an ordinary write can reach.

    A VOLUME THAT IS NOT MOUNTED IS None AND NEVER ZERO. An unplugged external
    disk reporting 0 bytes free would be an instant, permanent, false alarm.
    """
    try:
        st = os.statvfs(mount)
    except OSError:
        return None
    return st.f_frsize * st.f_blocks, st.f_frsize * st.f_bavail


# ---------------------------------------------------------------------------
# attribution — only ever when an alert is firing
# ---------------------------------------------------------------------------

def measurable(candidates):
    """(allowed, refused): the candidates `du` may walk, and the ones it must not.

    NEVER ANOTHER APP'S DATA. On 2026-09-24/25 this function's `du` walked the
    folder where macOS keeps other apps' sandboxed data, and the CEO got "python3.14
    would like to access data from other apps" on every alerting run. Allowing it did
    not stick, because a grant to Homebrew's unsigned interpreter lasts only for that
    process. The declaration was the cause and it is fixed there, and this check is why
    it stays fixed: an entry that is inside that data, or that holds it ($HOME, ~/Library),
    is refused here and reported in the state file as `consumers_refused`, whatever
    orchestration.config says. The rule lives in lib/foreign_app_data.py.
    """
    allowed, refused = [], []
    for path in candidates:
        why = foreign_app_data.entering(expand(path)) if path else None
        (refused if why else allowed).append({"path": path, "why": why} if why else path)
    return allowed, refused


def top_consumers(candidates, limit=3):
    """The biggest declared candidate directories, largest first.

    MEASURED ONLY WHEN AN ALERT IS FIRING, which is what makes it affordable at
    all: `du` over these paths costs real I/O and this job runs every quarter
    hour. The cheap check (statvfs) always runs; this runs when there is
    something to explain.

    A DECLARED CANDIDATE LIST, not a walk of the volume. Walking 460 GB to find
    the top three would take minutes and would be the single most expensive
    thing on the machine at the exact moment the machine is in trouble.
    """
    out = []
    candidates, _refused = measurable(candidates)
    for path in candidates:
        path = expand(path)
        if not path or not os.path.isdir(path):
            continue
        kb = du_kb(path)
        if kb is not None:
            out.append((kb * 1024, path))
    out.sort(reverse=True)
    return out[:limit]


def du_kb(path):
    """`du -sk path` in kilobytes, or None. THE ONLY PLACE THIS PROGRAM RUNS `du`.

    It refuses, here and not only in the callers, any path that is or holds data macOS
    protects (lib/foreign_app_data.py): 2026-10-29, `du -sk ~/Library/Caches` reached
    Apple Music's cache and the CEO was asked whether RichOS may access Apple Music."""
    if foreign_app_data.entering(path):
        return None
    try:
        r = subprocess.run(["du", "-sk", path], capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired):
        return None
    first = (r.stdout or "").split("\n")[0].split("\t")
    if not first or not first[0].strip().isdigit():
        return None
    return int(first[0].strip())


def richos_garbage_bytes():
    """How much of the shortfall is OUR OWN scratch.

    CEO addendum 2: the watchdog should never bark because RichOS failed to clean
    up after itself, so an alert has to say whether that is what happened. This
    is the number that decides between "RichOS garbage (DEFECT)" and "other".

    Counted from the allocator root and the claude scratch roots only — the two
    places whose entire contents are, by construction, ours. The legacy $TMPDIR
    families are deliberately NOT counted: their names are shared with fixtures
    this program does not own, and a classification that overstated our share
    would send somebody hunting a defect in RichOS for a disk filled by Xcode.
    """
    total = 0
    roots = []
    tmp = os.environ.get("TMPDIR") or "/tmp"
    roots.append(os.path.join(os.path.realpath(tmp),
                              env("SCRATCH_ROOT_NAME", "richos-scratch")))
    uid = str(os.getuid())
    for r in env("SCRATCH_CLAUDE_ROOTS", "").split():
        roots.append(r.replace("%u", uid))
    seen = set()
    for root in roots:
        real = os.path.realpath(root)
        if real in seen or not os.path.isdir(real):
            continue
        seen.add(real)
        kb = du_kb(real)
        if kb is not None:
            total += kb * 1024
    return total


# ---------------------------------------------------------------------------
# the CEO's channel
# ---------------------------------------------------------------------------

def notify_ceo(title, message):
    """A macOS user notification. True if it was posted.

    osascript `display notification` NEEDS NO APP AND NO SESSION, which is the
    entire reason it is the channel: this fires from launchd with nobody logged
    into anything and no RichOS window open. A notification that required the
    app to be running would be absent exactly on the night the disk fills.

    Quotes are stripped rather than escaped. The text is assembled here from
    numbers and paths, so there is nothing to preserve, and AppleScript string
    escaping is a place to introduce a bug rather than a feature to support.
    """
    safe_t = title.replace('"', "'").replace("\\", "")
    safe_m = message.replace('"', "'").replace("\\", "")
    script = ('display notification "%s" with title "%s"' % (safe_m, safe_t))
    try:
        r = subprocess.run(["osascript", "-e", script],
                           capture_output=True, text=True, timeout=30)
        return r.returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def human(n):
    n = float(n)
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if n < 1024 or unit == "TB":
            return ("%d %s" % (int(n), unit)) if unit == "B" else ("%.1f %s" % (n, unit))
        n /= 1024.0
    return str(n)


# ---------------------------------------------------------------------------
# state
# ---------------------------------------------------------------------------

def read_state(path):
    try:
        with open(path, encoding="utf-8") as fh:
            obj = json.load(fh)
        return obj if isinstance(obj, dict) else {}
    except (OSError, ValueError):
        return {}


def _age_of(stamp, now):
    """Seconds since an ISO-Z stamp, or None if it cannot be read.

    None IS NOT ZERO AND IS NOT INFINITY. A stamp this cannot parse means the
    age is unknown, and the caller says "at an unknown time" rather than
    picking one of the two answers that would be a claim.
    """
    if not stamp:
        return None
    try:
        return now - calendar.timegm(time.strptime(stamp, "%Y-%m-%dT%H:%M:%SZ"))
    except (ValueError, OverflowError):
        return None


def _short_age(seconds):
    if seconds is None:
        return "an unknown time"
    seconds = max(0, int(seconds))
    if seconds < 90:
        return "%d s" % seconds
    if seconds < 5400:
        return "%d min" % (seconds // 60)
    if seconds < 172800:
        return "%d h" % (seconds // 3600)
    return "%d d" % (seconds // 86400)


def run_reaper(cmd):
    """The sweeper's CURRENT verdict, or None if it could not be produced.

    ONLY `--status` CALLS THIS, and the header on its call site says why. It is
    the sweeper's own `--json`, which deletes nothing: the plan and the verdict,
    measured now. A failure returns None so the caller falls back to the
    published file WITH ITS DATE ON IT — an old number that says how old it is
    beats no number, and both beat an old number presented as current.
    """
    if not cmd or not os.path.isfile(cmd):
        return None
    try:
        r = subprocess.run(["bash", cmd, "--json"], capture_output=True,
                           text=True, timeout=180)
    except (OSError, subprocess.TimeoutExpired):
        return None
    # EXIT 3 (something undecidable) AND 4 (a deletion failed) ARE ANSWERS, not
    # failures to read. Only 2 — a missing declaration or a missing liveness
    # primitive — means there is no verdict at all.
    if r.returncode not in (0, 3, 4):
        return None
    try:
        obj = json.loads(r.stdout or "")
    except ValueError:
        return None
    return obj if isinstance(obj, dict) else None


def write_state(path, obj):
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(obj, fh, indent=1, sort_keys=True)
        os.replace(tmp, path)          # atomic: a reader never sees half a file
    except OSError as exc:
        sys.stderr.write("disk-watchdog: could not write %s: %s\n" % (path, exc))


def log(path, line):
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(line + "\n")
    except OSError:
        pass


# ---------------------------------------------------------------------------
# cleanup health: the scheduled reaper's own published result, and ONE owner action
# ---------------------------------------------------------------------------
# Cleanup PRD 2026-10-23, R4/R5. The reaper's scheduled run failed 142 of 146
# times (lsof was not on launchd's PATH) and this job printed it as "GARBAGE
# REPORT (not an alert)", which nobody acted on. Health is read ONLY from the
# result the installed launchd job published; a manual run never writes it, an
# acknowledgement never changes it, and nothing here runs the reaper.
REAPER_LABEL = "com.richos.scratch-reaper"
WATCHDOG_LABEL = "com.richos.disk-watchdog"

# KEPT OR UNDECIDED IS NOT A FAILURE (CEO, 2026-10-24: "remove obsolete
# instructions and repeated administrative noise without hiding genuine
# failures"). The reaper names the condition of an undecided group with its own
# code; only the established ones below are quiet: reported, never alarmed on and
# never escalated. ANYTHING ELSE stays an alarm: a failed deletion, a row with no
# condition (a refused or unfinished run, a reading the run could not make, an
# older result), and any code not listed here. Decided from the code, never from
# the sentence.
KEPT_CONDITIONS = {
    "wall-1": "outside every declared scratch root",
    "wall-2": "a checkout: it contains a .git",
    "wall-3": "a registered workspace",
    "wall-4": "on the never-touch list",
    "chat-attached": "a finished checkout a Codex chat still has attached",
    "husk-attached": "an emptied worktree folder a Codex chat still names",
    "main-unreadable": "undecided: its repository's main cannot be read",
    "agent-ended": "named scratch of an agent that is no longer running",
}


def kept_leftover(row):
    """True for a published unresolved row that is a known, kept leftover."""
    return (isinstance(row.get("condition"), str)
            and row["condition"] in KEPT_CONDITIONS
            and row.get("class") not in (None, "", "deletion"))


def kept_leftovers(reaper_state):
    """The kept leftovers in the scheduled run's published result."""
    if not reaper_state or reaper_state.get("source") != "launchd" \
            or reaper_state.get("outcome") != "unresolved":
        return []
    return [u for u in (reaper_state.get("unresolved") or [])
            if isinstance(u, dict) and kept_leftover(u)]


def kept_set_id(rows):
    """WHICH leftovers in WHAT condition, never how big or how old: scope, class,
    condition and the reaper's identity of the entries and their conditions."""
    return set_id("%s|%s|%s|%s" % (r.get("scope"), r.get("class"), r.get("condition"),
                                   r.get("identity") or r.get("count")) for r in rows)


def set_id(parts):
    """A short, order-free identity of a set of descriptive strings."""
    return hashlib.sha1("\n".join(sorted(parts)).encode("utf-8")).hexdigest()[:12]


def install_gave_up(state_dir):
    """[problem, ...] for a changed scheduled job whose change was never applied.

    An --install that finds the reaper or this watchdog changed while a run is in
    progress leaves the run alone, and a waiter applies the change when the run
    ends (scripts/lib/launchd-install.sh). A waiter that gives up writes
    launchd-pending-<label>.gave-up, and the next install that succeeds removes
    it; until then the job runs a schedule the engine no longer declares.

    Only a MISSING file means nothing to report. A give-up file that exists and
    cannot be read is unknown, and so is a pending change whose waiter record
    (launchd-pending-<label>.pid) cannot be read. A pending change whose waiter
    has ended, or whose pid is now another process, will never be applied: a
    waiter that crashed or was killed never reached give_up(), and the job's old
    schedule would otherwise look healthy."""
    problems = []
    for label in (REAPER_LABEL, WATCHDOG_LABEL):
        path = os.path.join(state_dir, "launchd-pending-%s.gave-up" % label)
        try:
            with open(path, encoding="utf-8") as fh:
                text = " ".join(fh.read().split())
            problems.append("%s: %s (%s)" % (label, text or "a changed job was not applied", path))
        except FileNotFoundError:
            pass
        except (OSError, UnicodeError) as exc:
            problems.append("%s: a give-up record exists but cannot be read (%s: %s); whether "
                            "the changed job was applied is unknown" % (label, path, exc))
        waiter = pending_waiter_problem(label, os.path.join(
            state_dir, "launchd-pending-%s.pid" % label))
        if waiter:
            problems.append(waiter)
    return problems


def pending_waiter_problem(label, path):
    """A problem string when LABEL's change is pending and its waiter is not alive
    as that waiter; None when nothing is pending or the waiter is running."""
    try:
        with open(path, encoding="utf-8") as fh:
            raw = fh.read().strip()
    except FileNotFoundError:
        return None
    except (OSError, UnicodeError) as exc:
        return ("%s: a pending change's waiter record cannot be read (%s: %s); whether the "
                "change will be applied is unknown" % (label, path, exc))
    if not raw.isdigit():
        return ("%s: a pending change's waiter record does not hold a pid (%s); whether the "
                "change will be applied is unknown" % (label, path))
    pid = int(raw)
    # The same process-identity reads the engine's fences use: libproc for
    # liveness, the process's own argv for which program it is.
    import operator_fences
    row = operator_fences.proc(pid)
    if not row or row["zombie"]:
        return ("%s: a changed job is still pending, but its waiter (pid %d) has ended without "
                "applying it (%s); the job runs its previous schedule" % (label, pid, path))
    argv = operator_fences.proc_argv(pid)
    if not argv:
        return ("%s: a pending change's waiter (pid %d) cannot be identified (%s); whether the "
                "change will be applied is unknown" % (label, pid, path))
    if not any(a.endswith("launchd-install-after-run.sh") for a in argv) or label not in argv:
        return ("%s: a changed job is still pending, but its waiter's pid %d is now another "
                "process (%s); the change will not be applied" % (label, pid, path))
    return None


def cleanup_health(reaper_state, state_path, max_age_h, now, launch_agents_dir,
                   state_dir=None):
    """[problem, ...] for the last unattended cleanup result; [] when it is healthy.
    A kept leftover is not a problem (kept_leftover above)."""
    problems = []
    plist = os.path.join(launch_agents_dir, REAPER_LABEL + ".plist")
    if not os.path.isfile(plist):
        problems.append("the scheduled cleanup job is not installed (%s)" % plist)
    if state_dir:
        problems.extend(install_gave_up(state_dir))
    if not reaper_state:
        problems.append("no unattended cleanup result is published (%s is missing or "
                        "unreadable)" % state_path)
        return problems
    if reaper_state.get("source") != "launchd" \
            or reaper_state.get("outcome") not in ("healthy", "unresolved"):
        problems.append("%s does not hold a result of the scheduled job" % state_path)
        return problems
    epoch = reaper_state.get("last_apply_epoch")
    if not isinstance(epoch, (int, float)) or epoch <= 0:
        problems.append("the scheduled job has never completed a run")
    elif now - epoch > max_age_h * 3600:
        problems.append("the last completed scheduled run at %s is past the %d h window"
                        % (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(epoch)), int(max_age_h)))
    if reaper_state.get("outcome") == "unresolved":
        rows = [u for u in (reaper_state.get("unresolved") or []) if isinstance(u, dict)]
        for u in rows:
            if not kept_leftover(u):
                problems.append("%s: %s" % (u.get("scope") or "?", u.get("cause") or "?"))
        if not rows:
            problems.append("the scheduled run reported an unresolved outcome")
    return problems


def _escalate(engine_scripts, args):
    """Run escalate.sh; (exit code, stdout). Never raises."""
    try:
        r = subprocess.run(["bash", os.path.join(engine_scripts, "escalate.sh")] + args,
                           capture_output=True, text=True, timeout=60,
                           stdin=subprocess.DEVNULL)
        return r.returncode, (r.stdout or "") + (r.stderr or "")
    except (OSError, subprocess.TimeoutExpired) as exc:
        return 2, str(exc)


def _escalation_status(esc_id, now):
    """'open', 'held' (a timed ack in force), 'closed' or 'unknown'."""
    try:
        import escalations
        rows, _bad = escalations.read_rows()
    except Exception:  # noqa: BLE001  an unreadable ledger is unknown, never closed
        return "unknown"
    if rows is None:
        return "unknown"
    if not any(r.get("event") == "Escalation" and r.get("id") == esc_id for r in rows):
        return "missing"           # a readable ledger without it: raise afresh
    import datetime as _dt
    when = _dt.datetime.fromtimestamp(now, _dt.timezone.utc)
    if any(e.get("id") == esc_id for e in escalations.outstanding(rows, when)):
        return "open"
    acks = escalations.acks_for(rows, esc_id)
    if any(isinstance(a.get("until_epoch"), (int, float)) and a["until_epoch"] > now
           for a in acks):
        return "held"
    return "closed" if acks else "unknown"


FINDING_KEY_VERSION = 2   # 1 (unwritten): sha256 of the exact problem texts
_EXAMPLE_TAIL = re.compile(r", e\.g\. .*$", re.S)
_MEASURE = re.compile(r"\d+(?:\.\d+)?(?:\s?[KMGTP]?i?B\b)?")
_ENTRY_WORD = re.compile(r"\bentr(?:y|ies)\b")


def finding_key(problems):
    """WHAT FAILED, NEVER HOW MUCH OF IT: the identity of an unresolved cleanup result.

    2026-10-28/29: one finding ("/private/tmp/claude-501: N undecided claude-session
    entries (S), e.g. <path>: <why>") reached the lead under 26 escalation ids, each acked
    as "the same finding under a new id". a2025c7cb stopped the re-raise of an acked
    result whose text is unchanged, but the text carries the count, the size and an
    example, and those change whenever a second review leaves one more 96-byte folder or
    one ages out. The root and the kind of failure do not change, and they are what an
    acknowledgement decides. So the key drops the example and every measured number: a
    changed count is the same finding under the same id; another root or another kind
    of failure is a new one (W23h2)."""
    norm = set()
    for p in problems:
        text = _EXAMPLE_TAIL.sub("", str(p))
        text = _ENTRY_WORD.sub("entries", _MEASURE.sub("#", text))
        norm.add(" ".join(text.split()))
    return hashlib.sha256(json.dumps(sorted(norm)).encode("utf-8")).hexdigest()


def bridge_cleanup_escalation(problems, prev, now, stamp, log_path, reaper_cmd, state_path):
    """The scheduled tick's owner action: ONE open escalation per unresolved result.

    Unresolved and nothing associated, or a materially changed acknowledged
    failure: raise one (state proceeding, for the lead). Associated and open,
    or held by a timed acknowledgement: nothing; the ledger and the stall watcher
    already repeat it, and an expiring hold reopens the same id. Healthy again
    (kept leftovers are not a failure): acknowledge the associated id permanently,
    open or held. Returns the association to store, or None."""
    engine_scripts = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    checkout = os.path.realpath(os.path.join(engine_scripts, "..", "..", ".."))
    assoc = prev.get("cleanup_escalation") if isinstance(prev.get("cleanup_escalation"), dict) else None
    status = _escalation_status(assoc["id"], now) if assoc and assoc.get("id") else None
    problem_key = finding_key(problems)
    if assoc and assoc.get("key_version") != FINDING_KEY_VERSION:
        # An association stored under the old, text-exact key adopts the finding key
        # once, without a wake, exactly as one with no key did (a2025c7cb).
        assoc = {k: v for k, v in assoc.items() if k != "problem_key"}
    if not problems:
        if not assoc:
            return None
        # HELD IS CLOSED TOO. A timed hold left in place would expire and tell the
        # lead again about a failure that is gone (2026-10-24: the escalation of
        # 01:19Z was held to 22:00Z on superseded instructions).
        if status in ("open", "held"):
            rc, out = _escalate(engine_scripts, [
                "ack", assoc["id"], "--disposition",
                "Recovered: the scheduled cleanup run's result at %s has no failure left "
                "(what it keeps on purpose is reported, not escalated); closed by the "
                "disk watchdog." % stamp])
            log(log_path, "%s cleanup escalation %s %s" % (
                stamp, assoc["id"], "acknowledged (recovered)" if rc == 0 else
                "could not be acknowledged yet (exit %d): %s" % (rc, out.strip()[:160])))
            return None if rc == 0 else assoc
        return None if status in ("closed", "missing") else assoc
    if assoc and status in ("open", "held", "unknown"):
        return {**assoc, "problem_key": assoc.get("problem_key") or problem_key,
                "key_version": FINDING_KEY_VERSION}
    if assoc and status == "closed" and (assoc.get("problem_key") or problem_key) == problem_key:
        # An acknowledgement is a decision about this failure, not recovery. Keep
        # the unhealthy result visible without making the lead decide it every tick.
        # Older associations adopt their first fingerprint without a new wake.
        return {**assoc, "problem_key": problem_key, "key_version": FINDING_KEY_VERSION}
    title = "Scheduled cleanup has not recovered: %s" % problems[0][:140]
    question = ("Diagnose and repair the scheduled cleanup (%d unresolved: %s). Detail: %s "
                "--verbose and %s. Manual deletion does not clear this; only the next "
                "scheduled run's healthy result does. A blocker you cannot repair in an "
                "hour goes to the CEO only if it needs him."
                % (len(problems), "; ".join(p[:120] for p in problems[:3]), reaper_cmd,
                   state_path))
    rc, out = _escalate(engine_scripts, [
        "raise", "--title", title, "--state", "proceeding", "--for", "lead",
        "--question", question,
        "--meanwhile", "The scheduled cleanup keeps running every six hours; nothing is "
                       "deleted on a guess.",
        "--teammate", "scratch-reaper", "--worktree", checkout, "--no-record"])
    found = [w for w in out.split() if w.startswith("esc-")]
    if rc != 0 or not found:
        log(log_path, "%s cleanup escalation NOT RAISED (exit %d): %s"
            % (stamp, rc, out.strip()[:200]))
        return assoc
    log(log_path, "%s cleanup escalation %s raised: %s" % (stamp, found[0], problems[0][:160]))
    return {"id": found[0], "raised": stamp, "problem_key": problem_key,
            "key_version": FINDING_KEY_VERSION, "replaces": assoc.get("id") if assoc else None}


# ---------------------------------------------------------------------------
# the run
# ---------------------------------------------------------------------------

def collect_devices_for_scheduled_check():
    """A killed session may emit no end event. The 15-minute timer is its backup.

    Read-only alert/status requests never launch this collector. A sandbox
    without explicit fake device tools never inspects the account's registry.

    THE COLLECTION RUNS IN ITS OWN PROCESS WITH A HARD LIMIT. It holds the
    test-device registry lock for its whole pass, and its 8-second budget bounds
    only its subprocesses and removals, never a filesystem call. On 2026-10-07
    an open() inside that pass blocked in the kernel and the watchdog held the
    lock for 35 minutes, failing every lease cleanup and test run on the
    machine. A child is killed at the limit, and its lock dies with it.
    """
    try:
        import testdevices
        if not testdevices.machine_devices_allowed() and not (
                env("RICHOS_SIMCTL") or env("RICHOS_ANDROID_CACHES_ROOT")):
            return
    except Exception as exc:
        from testdevice_alerts import record_failure
        record_failure("scheduled device cleanup unavailable: %s" % exc)
        return
    limit = env_num("RICHOS_DEVICE_COLLECT_SECONDS", 30)
    script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "testdevices.py")
    try:
        child = subprocess.Popen([sys.executable, script, "collect", "--apply", "--budget", "8"],
                                 stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL, start_new_session=True)
    except OSError as exc:
        from testdevice_alerts import record_failure
        record_failure("scheduled device cleanup unavailable: %s" % exc)
        return
    try:
        child.wait(timeout=limit)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(child.pid, 9)
        except OSError:
            pass
        child.wait()
        from testdevice_alerts import record_failure
        record_failure("scheduled device cleanup stopped after %g s so it would not keep "
                       "the test-device registry lock; the next pass retries" % limit)


def storage_candidates():
    """RichOS's OWN folders and their volumes, never the entire bulk disk.

    ONLY OURS (2026-10-29). The list used to end with $TMPDIR, /private/tmp, ~/.cargo,
    ~/.npm, ~/Library/Caches and ~/Library/Developer. On an alerting run `du -sk
    ~/Library/Caches` reached Apple Music's cache and macOS asked the CEO whether
    "RichOS.app" may access Apple Music; `du` of $TMPDIR was refused at every other app's
    TemporaryItems. Free space is read by statvfs, which needs no permission. Size is
    measured only for what RichOS owns: the effective storage roots, the app's and the
    engine's own folders, the scratch allocator and the agents' scratch roots."""
    settings = M.environment()  # Malformed settings still fail before monitoring.
    effective = []
    # An unplugged working drive must not stop monitoring the primary disk.
    for resolver in (M.agent_root, M.cache, M.state):
        try:
            effective.append(resolver(environment=settings))
        except ValueError:
            continue
    for key, path in M.load(settings).items():
        if key == "RICHOS_BULK_ROOT":
            continue
        try:
            effective.append(M.validate(path))
        except ValueError:
            continue
    effective = list(dict.fromkeys(effective))
    extras = sorted({str(Path(*path.parts[:3])) for path in effective
                     if path.parts[:2] == ('/', 'Volumes') and len(path.parts) >= 3})
    tmp = os.path.realpath(os.environ.get("TMPDIR") or "/tmp")
    uid = str(os.getuid())
    candidates = [str(path) for path in effective] + [
        os.path.join(tmp, env("SCRATCH_ROOT_NAME", "richos-scratch")),
        os.path.expanduser("~/.claude"), os.path.expanduser("~/.richos-nightly"),
        os.path.expanduser("~/Library/Application Support/com.richos.app"),
        os.path.expanduser("~/Library/Application Support/RichOS")]
    candidates += [r.replace("%u", uid) for r in env("SCRATCH_CLAUDE_ROOTS", "").split()]
    return list(dict.fromkeys(candidates)), extras


def main():
    mode = env("MODE", "check")
    if mode == "check":
        collect_devices_for_scheduled_check()
    sbase = state_base()
    state_path = expand(env("STATE", os.path.join(sbase, "disk-watchdog.json")))
    log_path = expand(env("LOG", os.path.join(sbase, "disk-watchdog.log")))

    primary = env("DISK_PRIMARY_VOLUME", "/System/Volumes/Data")
    candidates, extras = storage_candidates()
    rich_gb = env_num("DISK_RICH_ALERT_GB", 60)
    drop_gb = env_num("DISK_DROP_ALERT_GB", 20)
    ceo_gb = env_num("DISK_CEO_NOTIFY_GB", 50)
    urgent_gb = env_num("DISK_CEO_URGENT_GB", 25)
    repeat_h = env_num("DISK_CEO_REPEAT_HOURS", 24)
    scale = env("DISK_SCALE_EXTRA_VOLUMES", "1") == "1"
    consumers_refused = measurable(candidates)[1]

    prev = read_state(state_path)
    now = time.time()
    stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now))

    pv = read_volume(primary)
    if pv is None:
        sys.stderr.write("disk-watchdog: cannot read %s\n" % primary)
        return 2
    p_cap, _p_free = pv

    volumes = []
    for mount in [primary] + extras:
        v = read_volume(mount)
        if v is None:
            continue                # not mounted: skipped, never zero
        cap, free = v
        if mount == primary or not scale:
            t_rich, t_ceo, t_urgent = rich_gb, ceo_gb, urgent_gb
        else:
            # SCALED BY CAPACITY. 25 GB free on a 954 GB disk is a much more
            # urgent situation than 25 GB on a 460 GB one, so the FRACTION is
            # what carries over, not the absolute number. The arithmetic is
            # written out in orchestration.config.
            factor = (cap / float(p_cap)) if p_cap else 1.0
            t_rich, t_ceo, t_urgent = (rich_gb * factor, ceo_gb * factor,
                                       urgent_gb * factor)
        volumes.append({
            "mount": mount,
            "capacity_bytes": cap,
            "free_bytes": free,
            "free_gb": round(free / float(GB), 1),
            "capacity_gb": round(cap / float(GB), 1),
            "percent_free": round(100.0 * free / cap, 1) if cap else 0.0,
            "rich_alert_gb": round(t_rich, 1),
            "ceo_notify_gb": round(t_ceo, 1),
            "ceo_urgent_gb": round(t_urgent, 1),
            "below_rich": free < t_rich * GB,
            "below_ceo": free < t_ceo * GB,
            "below_urgent": free < t_urgent * GB,
        })

    # --- the drop, per volume, against the previous reading -----------------
    # A RATE, NOT A DIFFERENCE, AND THE DIVISOR IS D7.
    #
    # It was `old_free - free >= DISK_DROP_ALERT_GB`, against whatever the
    # previous reading happened to be, with no elapsed-time term at all. Frank
    # aged the baseline ten hours — a closed laptop, a wake from sleep, an upgrade
    # window, a job unloaded and reloaded — and got:
    #
    #   MASSIVE ALERT — DISK SPACE
    #   /System/Volumes/Data — 181.5 GB FREE ...: DROPPED 30.0 GB since the last
    #   reading ... CLASSIFICATION: RichOS garbage (DEFECT)
    #
    # 3 GB/hour is a normal working day on this machine, and it produced a MASSIVE
    # ALERT calling it our defect. That is the false-positive engine that ends with
    # the alert being ignored.
    #
    # TWO CHANGES, AND THE SECOND MATTERS MORE THAN THE FIRST. The drop becomes a
    # rate in GB/hour; and across a gap longer than a declared number of intervals
    # THE RATE IS NOT COMPUTED AT ALL — the gap is reported instead. A rate
    # averaged over ten hours says nothing about what happened in any of them, so
    # the honest output is "there is no comparable previous reading", not a number.
    # A MINIMUM GAP AS WELL AS A MAXIMUM, AND THE MINIMUM IS A HOLE THIS FIX
    # OPENED RATHER THAN ONE IT INHERITED. Turning a difference into a rate makes
    # the DENOMINATOR dangerous at both ends: the suite caught a sub-second gap
    # turning an ordinary 100 GB fixture fall into "FALLING at 496862.1 GB/hour",
    # which in production is a launchd job that fires twice in quick succession,
    # or a person running --check a moment after the timer did. Too short to
    # divide by is exactly as meaningless as too long, and it fails in the
    # DANGEROUS direction (a false alarm) rather than the quiet one.
    elapsed_floor = env_num("DISK_WATCHDOG_MINUTES", 15) * 60
    max_gap = env_num("DISK_DROP_MAX_GAP_INTERVALS", 3) * elapsed_floor
    min_gap = env_num("DISK_DROP_MIN_GAP_SECONDS", 60)
    rate_gb_h = env_num("DISK_DROP_ALERT_GB_PER_HOUR", 80)
    prev_epoch = prev.get("reading_epoch") or 0
    gap = (now - prev_epoch) if prev_epoch else 0
    prev_vols = {v.get("mount"): v for v in (prev.get("volumes") or [])
                 if isinstance(v, dict)}
    for v in volumes:
        old = prev_vols.get(v["mount"]) or {}
        old_free = old.get("free_bytes")
        v["previous_free_bytes"] = old_free
        v["reading_gap_seconds"] = int(gap)
        v["drop_rate_gb_per_hour"] = 0.0
        v["drop_rate_stale"] = False
        if isinstance(old_free, (int, float)) and old_free > 0:
            v["drop_bytes"] = int(old_free - v["free_bytes"])
        else:
            # NO PREVIOUS READING IS NOT A DROP OF THE WHOLE DISK. The very
            # first run of this job, and the first run after the state file is
            # removed, must not alert on an imaginary fall from zero.
            v["drop_bytes"] = 0
        if v["drop_bytes"] <= 0 or not prev_epoch or gap < min_gap:
            v["drop_alert"] = False
        elif gap > max_gap:
            # TOO OLD TO DIVIDE BY. Reported, never alerted on: the gap itself is
            # the fact, and manufacturing a rate from it is what produced the
            # false alarm this arm exists to stop.
            v["drop_rate_stale"] = True
            v["drop_alert"] = False
        else:
            v["drop_rate_gb_per_hour"] = round(
                (v["drop_bytes"] / float(GB)) / (gap / 3600.0), 1)
            v["drop_alert"] = v["drop_rate_gb_per_hour"] >= rate_gb_h

    # --- the sweeper's standing failures, which are Rich's business too ------
    # Addendum 1: the alert covers "every sweep failure" as well as the space
    # thresholds. Read from the sweeper's own state file rather than by running
    # it: this job must never depend on the sweeper, only observe it.
    fails_path = expand(env("SCRATCH_FAILURES_STATE",
                            os.path.join(sbase, "scratch-failures.json")))
    sweep_failures = read_state(fails_path)
    n_fail = len(sweep_failures) if isinstance(sweep_failures, dict) else 0

    # --- THE GARBAGE ALARM (Frank's Fix 2) ----------------------------------
    # His verdict on the whole mechanism was that "its alarm is a disk-space
    # alarm and a delete-failure alarm — IT HAS NO GARBAGE ALARM", so garbage no
    # arm considers produces neither a cleanup nor a word to anybody, and the
    # reports read green while tens of gigabytes sit there. Both halves of §54
    # ("always cleaned up" OR "Rich gets a MASSIVE ALERT") were satisfied at once
    # only for the paths already swept.
    #
    # This is the missing half, and it is cheap because the sweeper now computes
    # it: SKIPPED is garbage that will never be collected by the scheduled job —
    # another program's temp directory, another user's, a symlink, or entries a
    # budgeted run could not reach. UNDECIDABLE is the pile a tripped wall leaves
    # behind, which no run will ever clear on its own.
    #
    # READ, NEVER RUN. The sweeper publishes both numbers to its own state file
    # at every --apply; this job looks at what was left behind, exactly as it does
    # for the failures. It never invokes the sweeper and never writes its file.
    reaper_state_path = expand(env(
        "SCRATCH_REAPER_STATE",
        os.path.join(sbase, "scratch-reaper-state.json")))
    reaper_state = read_state(reaper_state_path)
    # Cleanup health comes from THE PUBLISHED RESULT ONLY, read before --status
    # swaps in a live plan below: a plan is not a scheduled run.
    published = reaper_state
    # --- AND THE NUMBER MUST BE FROM A RUN, NOT FROM A MEMORY ---------------
    #
    # CEO, 2026-09-19, reading the status line: "11.4 GB in 2 place(s)
    # UNDECIDABLE — no run will clear it". He had deleted both of those piles by
    # hand an hour earlier and the live sweep said ONE place at 2,176 bytes. The
    # figure came from the state file the 12:10Z scheduled pass wrote, and
    # nothing on the line said it was six hours old.
    #
    # A CACHED NUMBER PRESENTED AS A PRESENT-TENSE FACT IS THE STALE-ARTIFACT
    # FAILURE, and this file's own header insists the job must "observe, never
    # invoke" the sweeper — which is right for the 15-minute launchd reading and
    # is what produced this. The two are reconciled by mode:
    #
    #   --status  A PERSON IS WAITING FOR AN ANSWER. The sweeper is RUN and the
    #             answer is this second's. It costs about nine seconds, once,
    #             when somebody asked.
    #   --alert / --check  still read the file, never run anything, and every
    #             number they print CARRIES THE TIME IT WAS MEASURED and the
    #             command that re-derives it. Past a declared age the figure is
    #             reported as stale instead of quoted as fact.
    reaper_cmd = env("SCRATCH_REAPER_CMD", os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "scratch-reaper.sh"))
    measured_live = False
    if mode == "status":
        live = run_reaper(reaper_cmd)
        if live is not None:
            reaper_state = live
            measured_live = True
    max_age_h = env_num("SCRATCH_REAPER_STATE_MAX_AGE_HOURS", 12)
    measured_at = str(reaper_state.get("last_apply") or "")
    measured_epoch = reaper_state.get("last_apply_epoch")
    if measured_live:
        state_age = 0.0
    elif isinstance(measured_epoch, (int, float)) and measured_epoch > 0:
        state_age = now - measured_epoch
    else:
        state_age = _age_of(measured_at, now)
    state_stale = (not measured_live) and (
        state_age is None or state_age > max_age_h * 3600)
    if measured_live:
        measured_when = "just now, by this command"
    elif state_age is None:
        measured_when = "at an unknown time"
    else:
        measured_when = "%s (%s ago)" % (measured_at or "?",
                                         _short_age(state_age))

    skipped_n = int(reaper_state.get("skipped") or 0)
    skipped_b = int(reaper_state.get("skipped_bytes") or 0)
    undec_n = int(reaper_state.get("undecidable") or 0)
    undec_b = int(reaper_state.get("undecidable_bytes") or 0)
    split = reaper_state.get("skipped_split")
    if not isinstance(split, dict):
        split = {}
    by_hand = split.get("by_hand") if isinstance(split.get("by_hand"), dict) else {}
    foreign = split.get("foreign") if isinstance(split.get("foreign"), dict) else {}
    # A SWEEPER THAT PREDATES THE SPLIT still publishes the total. Attributing
    # all of it to the foreign class would be a guess; leaving the split empty
    # and printing the total under its old heading is the honest degrade.
    by_hand_n = int(by_hand.get("count") or 0)
    by_hand_b = int(by_hand.get("bytes") or 0)
    foreign_n = int(foreign.get("count") or (skipped_n if not split else 0))
    foreign_b = int(foreign.get("bytes") or (skipped_b if not split else 0))
    skipped_floor = env_num("SCRATCH_SKIPPED_NOTICE_BYTES", 1024 ** 3)
    undec_floor = env_num("SCRATCH_UNDECIDABLE_NOTICE_BYTES", 64 * 1024 ** 2)

    # --- CLEANUP HEALTH, AND ITS ONE OWNER ACTION (cleanup PRD R4/R5) --------
    launch_agents = expand(env("RICHOS_LAUNCH_AGENTS_DIR", "~/Library/LaunchAgents"))
    cleanup_problems = cleanup_health(published, reaper_state_path, max_age_h, now,
                                      launch_agents, sbase)
    kept = kept_leftovers(published)
    kept_id = kept_set_id(kept) if kept else ""
    assoc = prev.get("cleanup_escalation") if isinstance(
        prev.get("cleanup_escalation"), dict) else None
    if not cleanup_problems and assoc:
        # Only a run that inspected the scope that failed clears it.
        inspected = set(published.get("roots") or [])
        for scope in assoc.get("scopes") or []:
            if scope not in inspected:
                cleanup_problems.append("%s: the last healthy run did not inspect it, and "
                                        "it is where cleanup last failed" % scope)
    if mode == "check" and os.environ.get("DISK_WATCHDOG_SCHEDULED") == "1":
        # THE INSTALLED JOB ONLY. A terminal --check, and any test suite, reads
        # and prints; it never writes the escalation ledger.
        kept_scopes = set((assoc or {}).get("scopes") or [])
        assoc = bridge_cleanup_escalation(cleanup_problems, prev, now, stamp, log_path,
                                          reaper_cmd, reaper_state_path)
        if assoc is not None:
            # EVERY ROOT THAT EVER FAILED STAYS ON THE ACTION, across ticks and
            # into a replacement, until a healthy scheduled run has inspected it
            # (the check above): §54's "told until it is gone". A later failure in
            # another root joins; it never waits for the list to be empty.
            failed = set()
            if published.get("source") == "launchd":
                failed = {u.get("scope") for u in (published.get("unresolved") or [])
                          if isinstance(u, dict) and not kept_leftover(u)
                          and str(u.get("scope") or "").startswith("/")}
            assoc["scopes"] = sorted(kept_scopes | set(assoc.get("scopes") or []) | failed)
    # --- THREE PILES, THREE ANSWERS, AND ONLY ONE OF THEM IS AN ALARM -------
    #
    # CEO, 2026-09-19, on one line that ran all three together: "And what is
    # this all about: MASSIVE ALERT — DISK: 218 path(s) could not be deleted
    # (delete BY HAND) | 13.9 GB in 4122 place(s) NOTHING WILL EVER COLLECT |
    # 11.4 GB in 2 place(s) UNDECIDABLE — no run will clear it".
    #
    # Under one MASSIVE ALERT heading sat: 200 paths a defect in the sweeper
    # could not delete, 18 the kernel owns, 12.8 GB of our own campaign roots a
    # person removes with a printed command, 1.15 GB of Visual Studio Code's and
    # Codex's temporary files, and one stale figure. Four different responses —
    # fix it, ignore it, run this command, and nothing — presented as one alarm.
    # An alarm whose items do not share a response is one nobody can act on, and
    # this file's own suite ranks firing wrongly as worse than not firing.
    #
    #   (a) OUR garbage that FAILED to delete, and test instances that would not
    #       close: §54's "the clean-up failed" branch. MASSIVE ALERT, exit 1,
    #       because a person must go and delete it by hand.
    #   (b) OURS, never deleted automatically by design — a campaign root past
    #       its retention, a running throwaway container. A person acts, but
    #       nothing is broken and nothing is at risk. Its own heading, with the
    #       exact command, and no alarm.
    #   (c) ANOTHER PROGRAM'S or the operating system's temp. Counted so it is
    #       visible; never ours to touch. One quiet line. No alarm, ever.
    #   (d) UNDECIDABLE — ours, refused by a wall. One line with the size and
    #       what would settle it. No alarm: it is a thing to read, not to do.
    #
    # (d) CHANGED 2026-10-23 (cleanup PRD R4): an undecided pile is an UNRESOLVED
    #     cleanup result, an alarm with one owner action (cleanup health below),
    #     never a line to read. It is no longer part of this report.
    #     CHANGED AGAIN 2026-10-24: only an undecided group that is a failure
    #     (kept_leftover above) alarms; one kept or undecided has its own heading.
    garbage_report = (by_hand_b >= undec_floor or by_hand_n > 0
                      or foreign_b >= skipped_floor)
    # RETAINED UNDER ITS OLD NAME because the state file publishes it and a
    # reader comparing two days of records should see the same key. It is no
    # longer part of `condition`: it reports, it does not alarm.
    garbage_alarm = garbage_report

    # --- test instances that would not close (§54 addendum 4) ---------------
    # CEO: "Test app windows must always close/quit when testing is finished.
    # Same hygiene as with any other garbage." So an instance that survived a
    # quit request, a SIGTERM and a SIGKILL is a failed clean-up and belongs in
    # this alert beside an undeletable directory.
    #
    # A SEPARATE FILE FROM THE SWEEPER'S, and deliberately so: rows there are
    # keyed by path and resolved when the path stops existing, which would
    # resolve a pid-keyed row on its very next read and clear the alert while
    # the window was still on his screen. This file's rows resolve when the
    # PROCESS is gone. Read, never run -- the same rule as above.
    inst_path = expand(env("APP_INSTANCE_FAILURES_STATE",
                           "~/.claude/state/app-instance-failures.json"))
    inst_failures = read_state(inst_path)
    n_inst = len(inst_failures) if isinstance(inst_failures, dict) else 0

    # --- test simulators and emulators left behind (§54, 2026-09-22) ---------
    # Written by scripts/lib/testdevices.py: a device a killed test left
    # running whose removal failed, or whose owner cannot be proven while it
    # runs. Scheduled checks collect first; all other modes only read alerts.
    dev_path = expand(env("TEST_DEVICE_FAILURES_STATE",
                          os.path.join(state_base(), "test-device-failures.json")))
    dev_failures = read_state(dev_path)
    if not isinstance(dev_failures, dict):
        dev_failures = {}
    # A collection whose own process is still running is not a failure; one that
    # was killed, interrupted or ran past its bound still is (testdevice_alerts).
    # If that reader cannot load, every row is counted, never fewer.
    try:
        from testdevice_alerts import standing as device_standing
        dev_failures = device_standing(dev_failures)
    except Exception:
        pass
    n_dev = len(dev_failures)

    alerting = [v for v in volumes if v["below_rich"] or v["drop_alert"]]
    # `condition` IS THE MASSIVE ALERT AND NOTHING ELSE NOW. Three things can
    # raise it and every one of them means a person must act on something
    # broken: the disk is low or falling, our own clean-up failed, or a test app
    # window would not close. The garbage report below is printed alongside it
    # and never raises it — see the three-piles note above.
    condition = (bool(alerting) or n_fail > 0 or n_inst > 0 or n_dev > 0
                 or bool(cleanup_problems))
    # Anything at all worth printing, alarm or not. `--alert` prints when this
    # is true so the quiet lines still reach a reader; the EXIT CODE follows
    # `condition`.
    anything = condition or garbage_report

    # --- attribution and classification, ONLY when something is wrong -------
    consumers, classification, garbage = [], "", 0
    # Attribution walks the disk, so only the conditions it explains pay for it;
    # a cleanup result alone does not (its detail is the reaper's own record).
    disk_condition = bool(alerting) or n_fail > 0 or n_inst > 0 or n_dev > 0
    # THE SHORTFALL IS SPACE ACTUALLY MISSING below a volume's own declared floor.
    # With none, there is no low-disk cause to classify and no classification is
    # printed: on 2026-10-24 a clamp of 1 byte here made any scratch at all read
    # "RichOS garbage (DEFECT)" with 74 GB free. A failed cleanup or a device left
    # behind still alarms under its own heading above.
    shortfall = max((v["rich_alert_gb"] * GB - v["free_bytes"] for v in volumes), default=0)
    if disk_condition and mode in ("check", "status"):
        consumers = [{"path": p, "bytes": b, "human": human(b)}
                     for b, p in top_consumers(candidates)]
        garbage = richos_garbage_bytes()
        # "RichOS garbage (DEFECT)" when our own scratch is a material part of
        # what is missing. The comparison is against the SHORTFALL rather than
        # against the disk: 5 GB of our scratch on a disk 200 GB short is not
        # our defect, and the same 5 GB on a disk 6 GB short is.
        if shortfall > 0:
            classification = ("RichOS garbage (DEFECT)"
                              if garbage >= 0.25 * shortfall else "other")
    elif disk_condition:
        if shortfall > 0:
            classification = prev.get("classification") or "other"
        garbage = prev.get("richos_garbage_bytes") or 0
        consumers = prev.get("top_consumers") or []

    notified = dict(prev.get("notified") or {})

    # --- the CEO's notifications, at most once a day per level ---------------
    posted = []
    if mode == "check":
        for v in volumes:
            for level, flag in (("urgent", v["below_urgent"]),
                                ("notify", v["below_ceo"])):
                if not flag:
                    continue
                key = "%s:%s" % (v["mount"], level)
                last = notified.get(key) or 0
                if now - last < repeat_h * 3600:
                    continue
                top = ", ".join("%s %s" % (c["human"], os.path.basename(c["path"]) or c["path"])
                                for c in consumers[:3]) or "not measured"
                title = ("RichOS: disk space CRITICAL" if level == "urgent"
                         else "RichOS: disk space low")
                msg = ("%s free on %s (%.0f%%). Top: %s. %s"
                       % (human(v["free_bytes"]), v["mount"],
                          v["percent_free"], top, classification))
                if notify_ceo(title, msg):
                    notified[key] = now
                    posted.append(key)
                    log(log_path, "%s NOTIFIED ceo level=%s mount=%s free=%s "
                                  "class=%s" % (stamp, level, v["mount"],
                                                human(v["free_bytes"]),
                                                classification))
                else:
                    log(log_path, "%s NOTIFY-FAILED ceo level=%s mount=%s "
                                  "(osascript would not post)"
                                  % (stamp, level, v["mount"]))
                # ONLY THE MOST SEVERE LEVEL PER VOLUME PER RUN. Below 25 GB
                # both levels are true, and sending both would be two
                # notifications for one event.
                break

    # --- Rich's alert block -------------------------------------------------
    lines = []
    if condition:
        # THE BANNER NAMES WHAT IS ACTUALLY WRONG. It said "DISK SPACE" while
        # firing on 218 undeletable paths with 198 GB free, so the first line a
        # reader saw was about a thing that was not happening.
        headline = []
        if alerting:
            headline.append("DISK SPACE")
        if n_fail > 0:
            headline.append("CLEAN-UP FAILED")
        if n_inst > 0:
            headline.append("A TEST APP WINDOW WOULD NOT CLOSE")
        if n_dev > 0:
            headline.append("TEST DEVICE CLEANUP NEEDS ATTENTION")
        if cleanup_problems:
            headline.append("CLEANUP HAS NOT RECOVERED")
        lines.append("=" * 72)
        lines.append("  MASSIVE ALERT — %s" % " + ".join(headline))
        lines.append("=" * 72)
        for v in alerting:
            why = []
            if v["below_rich"]:
                why.append("below the declared %.0f GB floor" % v["rich_alert_gb"])
            if v["drop_alert"]:
                why.append("FALLING at %.1f GB/hour (%s in the last %d min, "
                           "against a declared %.0f GB/hour)"
                           % (v["drop_rate_gb_per_hour"], human(v["drop_bytes"]),
                              int(v["reading_gap_seconds"] / 60), rate_gb_h))
            lines.append("  %s — %s FREE of %s (%.1f%%): %s"
                         % (v["mount"], human(v["free_bytes"]),
                            human(v["capacity_bytes"]), v["percent_free"],
                            "; ".join(why)))
        # THE GAP, REPORTED RATHER THAN DIVIDED BY. Only inside a block that is
        # already being printed for another reason: a stale baseline is a fact
        # worth knowing and never a reason to raise an alarm, which is the whole
        # of D7.
        stale_vols = [v for v in volumes if v.get("drop_rate_stale")]
        if stale_vols:
            lines.append("")
            lines.append("  NO COMPARABLE PREVIOUS READING (so no rate was computed):")
            for v in stale_vols:
                lines.append("    %s — the last reading was %d min ago and the "
                             "declared interval is %d min"
                             % (v["mount"], int(v["reading_gap_seconds"] / 60),
                                int(elapsed_floor / 60)))
            lines.append("      A fall measured across a gap that long says nothing")
            lines.append("      about any hour inside it. The laptop was probably asleep.")
        if n_fail > 0:
            lines.append("")
            lines.append("  %d GARBAGE PATH(S) COULD NOT BE DELETED. The CEO's rule:"
                         % n_fail)
            lines.append("  if the clean-up fails, Rich deletes it BY HAND.")
            for path, row in sorted(sweep_failures.items())[:5]:
                if not isinstance(row, dict):
                    continue
                lines.append("    %s" % path)
                lines.append("      first seen %s, %s attempt(s), %s"
                             % (row.get("first", "?"), row.get("attempts", "?"),
                                str(row.get("error", "?"))[:90]))
        if n_inst > 0:
            lines.append("")
            lines.append("  %d TEST APP INSTANCE(S) WOULD NOT CLOSE. The CEO's rule"
                         % n_inst)
            lines.append("  (§54 addendum 4): a test app window is garbage when the")
            lines.append("  test is over. Rich ends these BY HAND:")
            for key, row in sorted(inst_failures.items())[:5]:
                if not isinstance(row, dict):
                    continue
                pid = row.get("pid", key)
                lines.append("    kill -9 %s" % pid)
                lines.append("      %s" % str(row.get("argv", "?"))[:90])
                lines.append("      first seen %s, %s attempt(s), %s"
                             % (row.get("first", "?"), row.get("attempts", "?"),
                                str(row.get("why", "?"))[:90]))
        if cleanup_problems:
            # ONE SHORT SUMMARY AND WHERE THE DETAIL IS; the owner action is the
            # escalation the scheduled tick raised, never a wall of candidates.
            lines.append("")
            lines.append("  CLEANUP HAS NOT RECOVERED: %s%s. Owner action: %s. Detail: %s "
                         "--verbose and %s"
                         % (cleanup_problems[0][:160],
                            "" if len(cleanup_problems) == 1
                            else " (+%d more)" % (len(cleanup_problems) - 1),
                            "escalation %s" % assoc["id"] if assoc and assoc.get("id")
                            else "the next scheduled watchdog tick raises one",
                            reaper_cmd, reaper_state_path))
        if n_dev > 0:
            lines.append("")
            lines.append("  %d TEST DEVICE CLEANUP ISSUE(S). A device a test"
                         % n_dev)
            lines.append("  booted is garbage when the test is over (§54). These could not")
            lines.append("  be removed, ownership is unknown or collection failed. Check these BY HAND:")
            for key, row in sorted(dev_failures.items())[:5]:
                if not isinstance(row, dict):
                    continue
                lines.append("    %s" % row.get("command", key))
                lines.append("      %s (%s): %s" % (row.get("name", "?"), row.get("verdict", "?"),
                                                    str(row.get("why", "?"))[:90]))
                lines.append("      first seen %s, %s attempt(s)"
                             % (row.get("first", "?"), row.get("attempts", "?")))
        if consumers:
            lines.append("")
            lines.append("  BIGGEST MEASURED CONSUMERS:")
            for c in consumers:
                lines.append("    %-10s %s" % (c["human"], c["path"]))
        if classification:
            lines.append("")
            lines.append("  CLASSIFICATION: %s" % classification)
            if classification.startswith("RichOS"):
                lines.append("  RichOS's own scratch is %s of this. THE APP IS"
                             % human(garbage))
                lines.append("  SUPPOSED TO CLEAN THIS UP ITSELF — the CEO's words:")
                lines.append("  \"the disk watchdog will NEVER bark because the RichOS")
                lines.append("  app failed to clean up its garbage\". So this is a DEFECT")
                lines.append("  REPORT, not a disk warning.")
        lines.append("")
        lines.append("  WHAT TO DO:")
        engine_scripts = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        lines.append("    %s   reclaim what nothing owns"
                     % os.path.join(engine_scripts, "scratch-sweep.sh"))
        lines.append("    %s --status   a reading per volume"
                     % os.path.join(engine_scripts, "disk-watchdog.sh"))
        if n_fail > 0:
            lines.append("    then delete the paths above BY HAND")
        lines.append("=" * 72)

    # --- THE GARBAGE REPORT — read, not an alarm ----------------------------
    # OUTSIDE the `if condition:` block, deliberately, and this is the whole
    # shape of the fix. It prints when there is something to say and it never
    # raises the exit code, so the sentence a person reads is "here is what is
    # sitting on the disk" and not "MASSIVE ALERT". Every figure here carries
    # the time it was measured and the command that re-derives it, so nothing on
    # these lines can ever again be quoted as present tense when it is six hours
    # old — that is what produced the 11.4 GB the CEO asked about.
    if garbage_report:
        lines.append("-" * 72)
        lines.append("  GARBAGE REPORT (not an alert) — measured %s"
                     % measured_when)
        lines.append("-" * 72)
        if state_stale:
            lines.append("  THESE FIGURES ARE OUT OF DATE. The scheduled sweep last")
            lines.append("  published %s and the declared window is %d h, so read them"
                         % (measured_when, int(max_age_h)))
            lines.append("  as a record and not as the state of the disk right now:")
            lines.append("    %s --verbose" % reaper_cmd)
        if by_hand_n > 0:
            # The set id is WHICH places (all of them, and how many), never their
            # size or age: the turn end tells this pile once per set, exactly as
            # it tells the kept set below (notice-disk-alert.sh). The producer's
            # identity covers every place; the five example paths are only the
            # fallback for a result written before it published one.
            hand_parts = ["count|%d" % by_hand_n]
            if by_hand.get("identity"):
                hand_parts.append("identity|%s" % by_hand.get("identity"))
            else:
                hand_parts += ["path|%s" % row.get("path")
                               for row in (by_hand.get("paths") or [])
                               if isinstance(row, dict)]
            lines.append("  %s in %d place(s) IS OURS AND IS NEVER DELETED AUTOMATICALLY "
                         "(set %s):" % (human(by_hand_b), by_hand_n, set_id(hand_parts)))
            lines.append("  a campaign root past its retention, or a running container")
            lines.append("  on a throwaway image. Each prints its own command; a person")
            lines.append("  runs it and the space comes back:")
            for row in (by_hand.get("paths") or [])[:5]:
                if not isinstance(row, dict):
                    continue
                lines.append("    %-10s %s" % (human(int(row.get("bytes") or 0)),
                                               row.get("path", "?")))
                why = str(row.get("why") or "")
                cmd = why.rsplit(":", 1)[-1].strip()
                if cmd:
                    lines.append("      %s" % cmd[:100])
        if foreign_b >= skipped_floor:
            lines.append("  %s in %d place(s) BELONGS TO OTHER PROGRAMS — Visual"
                         % (human(foreign_b), foreign_n))
            lines.append("  Studio Code's updater, Codex, WebKit, and the directories")
            lines.append("  macOS makes for its own agents. NOT OURS, NOT AN ALERT, and")
            lines.append("  never deleted by us. Counted only so it is not invisible.")
        lines.append("-" * 72)

    # --- KEPT OR UNDECIDED — reported, never an alarm (kept_leftover above) -
    # Outside `condition` like the garbage report, and it says only what is true
    # of these entries: any failure in the same result is the alarm above it. The
    # heading carries the set's identity, so the turn end can tell a set once and
    # a changed set again (notice-disk-alert.sh); sizes and times never change it.
    if kept:
        n_kept = sum(int(u.get("count") or 0) for u in kept)
        lines.append("-" * 72)
        lines.append("  KEPT OR UNDECIDED (not an alert): %d entr%s in %d group(s) the "
                     "scheduled run of %s left in place and will not delete on its own "
                     "(set %s)"
                     % (n_kept, "y" if n_kept == 1 else "ies", len(kept),
                        published.get("last_apply") or "an unknown time", kept_id))
        lines.append("-" * 72)
        for u in kept:
            lines.append("    %s (%s): %s" % (u.get("scope") or "?",
                                              KEPT_CONDITIONS[u["condition"]],
                                              str(u.get("cause") or "?")[:240]))
        lines.append("  Detail: %s --verbose and %s" % (reaper_cmd, reaper_state_path))
        lines.append("-" * 72)

    # --- THE WATCHMAN'S OWN ABSENCE, which is an alert of its own ------------
    # A watchdog that was never scheduled, or whose timer somebody removed,
    # looks EXACTLY like a disk that never gets low. This is the only condition
    # here that cannot be detected by the job itself — it is detected by whoever
    # asks for the alert block, which is the session hook.
    plist = os.path.join(expand(env("RICHOS_LAUNCH_AGENTS_DIR",
                                    "~/Library/LaunchAgents")),
                         "com.richos.disk-watchdog.plist")
    installed = os.path.isfile(plist)
    if mode == "alert" and not installed:
        lines = (["=" * 72,
                  "  MASSIVE ALERT — THE DISK WATCHDOG IS NOT INSTALLED",
                  "=" * 72,
                  "  Nothing is watching this machine's free space. A watchdog that",
                  "  was never scheduled looks exactly like a disk that never gets",
                  "  low, which is why its absence is itself an alert.",
                  "",
                  "  Install it FROM THE MAIN CHECKOUT:",
                  "    scripts/disk-watchdog.sh --install",
                  "=" * 72] + lines)

    stale = False
    last_reading = prev.get("reading_epoch") or 0
    if mode == "alert" and installed and last_reading:
        # A timer that is installed but has not fired for many intervals is
        # suppressed in all but name — launchd will not run a job whose plist
        # is loaded but disabled. Ten intervals is generous enough that a closed
        # laptop does not produce a false alarm.
        interval = env_num("DISK_WATCHDOG_MINUTES", 15) * 60
        if now - last_reading > interval * 10:
            stale = True
            lines = (["=" * 72,
                      "  MASSIVE ALERT — THE DISK WATCHDOG HAS STOPPED REPORTING",
                      "=" * 72,
                      "  Its timer is installed but its last reading is %d minutes old"
                      % int((now - last_reading) / 60),
                      "  and the declared interval is %d minutes. A loaded-but-disabled"
                      % int(interval / 60),
                      "  launchd job fires on time, every time, and executes nothing.",
                      "",
                      "    launchctl list | grep com.richos.disk-watchdog",
                      "    scripts/disk-watchdog.sh --install",
                      "=" * 72] + lines)

    out = {
        "reading": stamp,
        "reading_epoch": int(now),
        "volumes": volumes,
        "alerting": bool(condition),
        "classification": classification,
        "richos_garbage_bytes": garbage,
        "top_consumers": consumers,
        "consumers_refused": consumers_refused,
        "sweep_failures": n_fail,
        "test_instance_failures": n_inst,
        "test_device_failures": n_dev,
        "garbage_alarm": garbage_alarm,
        "garbage_report": garbage_report,
        "skipped": skipped_n,
        "skipped_bytes": skipped_b,
        "skipped_by_hand": by_hand_n,
        "skipped_by_hand_bytes": by_hand_b,
        "skipped_foreign": foreign_n,
        "skipped_foreign_bytes": foreign_b,
        "undecidable": undec_n,
        "undecidable_bytes": undec_b,
        # WHEN THE SWEEPER'S NUMBERS ABOVE WERE MEASURED, and whether that is
        # recent enough to be read as the present. A consumer of this record has
        # exactly the same right to know that as a person reading the alert.
        "sweeper_measured_at": measured_at,
        "sweeper_measured_live": measured_live,
        "sweeper_state_age_seconds": (None if state_age is None
                                      else int(state_age)),
        "sweeper_state_stale": state_stale,
        # CLEANUP HEALTH from the scheduled job's published result, and the ONE
        # escalation associated with it. Carried forward by every reading.
        "cleanup_healthy": not cleanup_problems,
        "cleanup_problems": cleanup_problems,
        "cleanup_kept": kept,
        "cleanup_kept_set": kept_id,
        "cleanup_escalation": assoc,
        "notified": notified,
        "timer_installed": installed,
        "reading_gap_seconds": int(gap),
        "thresholds": {
            "rich_alert_gb": rich_gb,
            # RETAINED AND NO LONGER THE TEST. DISK_DROP_ALERT_GB was a fall
            # between two readings of any age; the test is now a rate. It stays in
            # this block because it is still declared and a reader comparing the
            # two should see both.
            "drop_alert_gb": drop_gb,
            "drop_alert_gb_per_hour": rate_gb_h,
            "drop_max_gap_intervals": env_num("DISK_DROP_MAX_GAP_INTERVALS", 3),
            "drop_min_gap_seconds": min_gap,
            "ceo_notify_gb": ceo_gb, "ceo_urgent_gb": urgent_gb,
            "ceo_repeat_hours": repeat_h,
            "interval_minutes": env_num("DISK_WATCHDOG_MINUTES", 15),
            "skipped_notice_bytes": skipped_floor,
            "undecidable_notice_bytes": undec_floor,
            "reaper_state_max_age_hours": max_age_h,
        },
    }

    # THE STATE FILE IS WRITTEN ONLY BY A READING. `--alert` and `--json` are
    # consumers and must never move the previous reading forward: a turn end
    # that overwrote it would destroy the baseline the drop arithmetic subtracts
    # from, and the 15-minute rate alarm would silently become a
    # per-turn-end one.
    if mode == "check":
        write_state(state_path, out)
        log(log_path, "%s reading %s" % (stamp, " ".join(
            "%s=%s free" % (v["mount"], human(v["free_bytes"])) for v in volumes)))
        if posted:
            log(log_path, "%s posted=%s" % (stamp, ",".join(posted)))
        for r in consumers_refused:
            log(log_path, "%s refused-candidate %s: %s" % (stamp, r["path"], r["why"]))

    if mode == "json":
        json.dump(out, sys.stdout, indent=1, sort_keys=True)
        sys.stdout.write("\n")
    elif mode == "alert":
        if lines:
            sys.stdout.write("\n".join(lines) + "\n")
    elif mode == "status":
        for v in volumes:
            flag = "ALERT" if (v["below_rich"] or v["drop_alert"]) else "ok   "
            sys.stdout.write("%s  %-26s %9s free of %9s (%5.1f%%)  floor %.0f GB\n"
                             % (flag, v["mount"], human(v["free_bytes"]),
                                human(v["capacity_bytes"]), v["percent_free"],
                                v["rich_alert_gb"]))
        if n_fail:
            sys.stdout.write("ALERT  %d garbage path(s) could not be deleted\n"
                             % n_fail)
        if n_inst:
            sys.stdout.write("ALERT  %d test app instance(s) would not close\n"
                             % n_inst)
        if n_dev:
            sys.stdout.write("ALERT  %d test device cleanup issue(s)\n"
                             % n_dev)
        for problem in cleanup_problems:
            sys.stdout.write("ALERT  cleanup has not recovered: %s\n" % problem)
        for u in kept:
            sys.stdout.write("kept    %s (%s): %s\n"
                             % (u.get("scope") or "?", KEPT_CONDITIONS[u["condition"]],
                                u.get("cause") or "?"))
        # THREE LINES, EACH SAYING WHOSE IT IS, AND NONE OF THEM AN ALERT.
        # It was one: "ALERT  13.9 GB skipped + 11.4 GB undecidable". The 13.9
        # was mostly our own campaign roots with a printed command, the rest was
        # Visual Studio Code's temp, and the 11.4 had been deleted by hand an
        # hour before the line was printed. A reader could act on none of it.
        sys.stdout.write("%s  the sweeper's figures below were measured %s\n"
                         % ("STALE " if state_stale else "ok    ", measured_when))
        if by_hand_n:
            sys.stdout.write("to do   %s in %d place(s) is OURS and is never deleted "
                             "automatically — each prints its own command\n"
                             % (human(by_hand_b), by_hand_n))
        if foreign_b >= skipped_floor:
            sys.stdout.write("fyi     %s in %d place(s) belongs to other programs — "
                             "counted, never ours to delete\n"
                             % (human(foreign_b), foreign_n))
        if not installed:
            sys.stdout.write("ALERT  the launchd timer is NOT installed\n")
    elif mode == "check":
        for v in volumes:
            sys.stdout.write("%s: %s free of %s (%.1f%%)\n"
                             % (v["mount"], human(v["free_bytes"]),
                                human(v["capacity_bytes"]), v["percent_free"]))
        if lines:
            sys.stdout.write("\n".join(lines) + "\n")

    # THE EXIT CODE AGREES WITH WHAT WAS PRINTED, on every mode. The first
    # version returned 0 from --status while printing
    # "ALERT the launchd timer is NOT installed", which is the shape of failure
    # that teaches a caller to ignore exit codes.
    #
    # THE GARBAGE REPORT IS PRINTED AND DOES NOT RAISE IT, and that is the
    # point of separating the two. `ALERT` means a person must act on something
    # broken; other programs' temporary files are not that, and neither is a
    # campaign root sitting where it was left. Exit 1 on those trained the
    # reader to ignore every exit code this program has — the same way an
    # unclearable failure list trains a reader to skip the failure list.
    if condition or stale:
        return 1
    if not installed and mode in ("alert", "status"):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
