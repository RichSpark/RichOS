#!/usr/bin/env python3
"""watch.py — the phone speed check that runs by itself after every land that touches richos/mobile/.

THE CEO, 2026-10-01: the mobile apps must ALWAYS be super-fast; the cold start and
back-from-background benchmarks must always be maintained. On 2026-10-02 he found his Android cold
starts noticeably slower than the day before and a long blank screen on the iPhone's start, and
nothing had told anyone. A slower start must be caught by the system the moment it happens, never
by him. Until this file, `perf.py compare` judged a record only when someone ran it; nothing in the
engine (merge gate, land, schedules) ran a phone.

WHAT STARTS A RUN. Git, at the land. richos/app/scripts/autocheck/autocheck.py's post-merge and
post-commit hooks call `watch.py trigger --repo <checkout> --from <old main> --to <new main>` every
time main moves in a checkout (a merge, a fast-forward, a commit on main). The trigger:
  1. acts only in the operator's own main checkout (RICHOS_PHONE_WATCH_REPO, else the checkout the
     engine plugin is loaded from, ~/.claude/richos-engine/../..). A clone a test suite merges into
     never reaches a phone;
  2. names every earlier request that no run ever finished (a runner killed, a Mac restarted) as a
     MISSED run in Rich's escalation ledger, once each, and starts a runner to retry it;
  3. when old..new changes anything under richos/mobile/, records the request in the ledger and
     starts ONE detached runner (`watch.py run`), its own session, so the land returns at once.
A land that touches nothing under richos/mobile/ starts nothing.

WHAT A RUN DOES, per wired phone (Android and iPhone in parallel, each as soon as it is free), in
its own clone of main's tip on the external SSD (never a worktree of the repository):
  - does not run a phone whose app code did not change (CEO 2026-10-03): when no path that builds into
    that phone's app (is_app_code: its native-* directory minus tests, tooling, the headless CLI and
    notes) changed since its last measurement, that measurement's samples are judged AGAIN against the
    current limits and the phone is not touched (`unchanged`; a newly failing verdict is escalated).
  - measures the phone as soon as one sample shows it free (CEO 2026-10-04: the phone is for
    testing; there is no quiet period): no process outside this run's own tree names the phone's
    serial or identifier or runs a phone tool (phone-android.py, phone-ios.py,
    hidden-send-try.py, devicectl, xctrace, an adb client). Only while such a command is running
    right now does it wait, polling every POLL_S, and it never interrupts anything. A phone still
    busy after MAX_WAIT_S (6 h) is reported, never measured.
  - touches the phone ONLY through the platform command lines (CEO mandate, §76 addendum,
    CLI-first mobile development): `randroid device install` and `randroid device perf` on
    Android, `rios device install` and `rios device perf` on the iPhone (VERBS below). They build
    the RELEASE build of the clone, install it over the existing app with its data kept (never an
    uninstall), seed the fixed made-up conversation the benchmarks were taken with, measure TRIALS
    (100) cold starts and TRIALS returns from the background (the protocol of the benchmark series),
    put the app's own saved state back and write one perf.py record. The phone is left on that
    release build. (The verbs came with zach-opus-releaseonly1 and andy-sonnet-twin1; a checkout
    without them reports COULD NOT MEASURE naming the missing verb.) The perf verb is perf.py's
    measurement run, which judges its own record and exits 1 when a phase failed (the blank-screen
    check's FAIL among them) and 4 when it is slower than the benchmark: with a record written,
    those exits are a measurement and go on to the comparison and the blank-screen check below;
    without one, or with any other exit (3 refused, 6 the phone's own state NOT restored), the run
    could not measure.
  - runs `perf.py compare` on the record (it reads files only, never a phone). Both metrics must be
    compared; a NOT COMPARED metric is a run that could not measure.
  - signs the Android build: `randroid device install` and `randroid device perf` run inside
    richos-hq's scripts/with-android-signing.py (RICHOS_PHONE_WATCH_SIGNING names another), because
    randroid refuses an unsigned Release APK and a debuggable twin without the upload key. The iPhone has its own signing team (RICHOS_APPLE_TEAM).
  - puts phone_guard.py in front of adb and xcrun for everything the verbs start (first on PATH):
    an uninstall or a data clear is refused, the run stops there and is reported REFUSED (CEO rule
    2026-10-01: never uninstall the app or clear its data, on either phone).
  - BLANK SCREEN (CEO 2026-10-02: no blank screen on start): on Android the perf verb runs
    quint-opus-blank1's cold-start check inside the same measurement (perf.py's `cold-blank` phase,
    `--blank-starts` BLANK_STARTS: blankstart.android_cold_blank taps the icon with the screen
    recorded and blank.py judges each recording), and the record carries the verdict as
    `metrics.coldBlank`. blank_screen_problems() below reads it: a FAIL makes the run a REGRESSION
    exactly as a slower p95 does, and an Android record without the verdict is a run that could not
    measure. The analyzer runs inside the verb because it needs the phone, which this file reaches
    only through the verbs. The iPhone's cold-start blank check needs isaac-opus-white1's screen
    recording; until then an iPhone record without `coldBlank` says so, and one with it is judged.

WHAT REACHES RICH. Every outcome but a good or unchanged one raises an escalation in the committed
ledger (richos/engine/scripts/escalate.sh, read at every session start and turn end, louder at 1 h,
24 h and 72 h): SLOWER than the benchmark, REFUSED (it would have needed an uninstall), or COULD NOT
MEASURE with the reason (no phone wired, busy for 6 h, a verb failed or does not exist yet, not
compared). Each names the commit range since the last good run of that phone, and the commits in it
that touch richos/mobile/. A good run is recorded (good.json), so the next regression names a short
range.

HOW A MISSED RUN BECOMES VISIBLE. The trigger writes the request BEFORE it starts the runner, a
runner writes `started` and `finished` around each round, and the lock (run.lock) is held for as
long as a runner lives. A request no finished round covers, with no runner holding the lock, is
reported as MISSED by the next trigger, which is every later move of main, mobile or not. A trigger
that cannot start a runner reports it at once. `watch.py status` prints all of it.

State (private: records name the phones) lives in RICHOS_PHONE_WATCH_HOME, default
The effective phone-speed-watch state directory contains ledger.jsonl, good.json, run.lock, runner.log,
runs/<round>/<platform>/ (record, log, guard; the newest 20 rounds and every round a good run
points at are kept), checkout/ (the build clone).

  watch.py trigger --repo DIR --from OLD --to NEW
  watch.py run --repo DIR
  watch.py status [--json]
  watch.py busy android|ios [--ident ID]   which processes would keep that phone busy right now
"""
import argparse
import calendar
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import machine_storage as M
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import time

def _pc():
    """perfcore, imported when a run judges (the trigger runs from a bare copy of this file)."""
    import perfcore
    return perfcore

HERE = Path(__file__).resolve().parent
MOBILE = "richos/mobile/"
PLATFORMS = ("android", "ios")
METRICS = ("coldLaunch", "warmResume")
DEFAULT_HOME = str(M.state("phone-speed-watch", "RICHOS_PHONE_WATCH_HOME"))
# richos-hq's signing wrapper (scripts/with-android-signing.py): the one signing path, shared with a person's
# `randroid device install`. It reads the upload key from protected storage outside every repository.
def default_signing(repo):
    # A linked worktree shares its primary checkout's Git common directory.
    common = subprocess.run(["git", "-C", str(repo), "rev-parse", "--path-format=absolute", "--git-common-dir"],
                            capture_output=True, text=True, check=True, timeout=30).stdout.strip()
    wrapper = Path(common).parent.parent / "richos-hq/scripts/with-android-signing.py"
    return f"{shlex.quote(sys.executable)} {shlex.quote(str(wrapper))}"
IOS_STORE = M.cache("richos-native-ios/physical-store", "RICHOS_PHYSICAL_STORE")
SPAWN_GRACE_S = 120          # a request younger than this may still be starting its runner
# What a phone's app is built from, worked out from the build files (native-android/settings.gradle.kts
# includes :core, :cli, :app; its core tests read ../conformance/vectors only at test time; native-ios/
# project.yml builds App, DevBridge, the Core package, Release/platform.yml and the extensions). Nothing
# outside these two directories is compiled into either app, so a path counts as that app's code when
# it is under its directory and is not tooling or a test: the rest of the directory is the app.
APP_DIR = {"android": "richos/mobile/native-android/", "ios": "richos/mobile/native-ios/"}
NOT_APP_TOP = {"android": ("bin", "cli"),
               "ios": ("bin", "docs", "PlatformTests", "TestSupport", "Tools", "UITests", "UnitTests")}
NOT_APP_INNER = {"android": ("/src/test/", "/src/testDebug/"), "ios": ("/Tests/",)}
# native-ios/Release/ mixes what the build uses with release tooling. The build uses platform.yml
# (project.yml includes it), App-Info.plist (its INFOPLIST_FILE) and the two .entitlements files
# (CODE_SIGN_ENTITLEMENTS; the test phone's copy builds with the .testcopy one). These do not go into
# the built app: the TestFlight tool and its tests, ExportOptions.plist (read by the export at upload,
# never by a build), the check and test scripts (every Release/*.py, by rule below), generate.sh (the project's generator, build tooling
# like Tools/), make-app-icon.cjs (the build uses the icons it wrote under App/) and third-party/
# (license text). Anything else in Release/ counts.
NOT_APP_PATH = {"android": (),
                "ios": tuple(f"Release/{name}" for name in (
                    "testflight.ts", "testflight.test.ts", "ExportOptions.plist", "check-release.sh",
                    "platform-tests.sh", "simulator-tests.sh", "generate.sh", "make-app-icon.cjs",
                    "third-party/"))}


def is_app_code(platform, path):
    """True when `path` (repository-relative) can change `platform`'s app build. Tests, tooling, the
    headless CLI and notes cannot; anything unrecognized under the app's directory counts (measure)."""
    root = APP_DIR[platform]
    if not path.startswith(root):
        return False
    rest = path[len(root):]
    if rest.endswith(".md") or rest.split("/", 1)[0] in NOT_APP_TOP[platform]:
        return False
    if any(rest == p or (p.endswith("/") and rest.startswith(p)) for p in NOT_APP_PATH[platform]):
        return False
    if platform == "ios" and rest.startswith("Release/") and rest.endswith(".py") and "/" not in rest[8:]:
        return False
    return not any(marker in "/" + rest for marker in NOT_APP_INNER[platform])

# The platform command lines' phone verbs (zach-opus-releaseonly1, andy-sonnet-twin1). The only way
# a run reaches a phone. Each takes the phone, installs or measures, and exits 0 on success; `perf`
# writes one perf.py record to --out and may also exit MEASURED_EXITS with that record written.
VERBS = {
    "android": {"cli": "richos/mobile/native-android/bin/randroid", "phone": "--serial",
                "install": ["device", "install"], "perf": ["device", "perf"]},
    "ios": {"cli": "richos/mobile/native-ios/bin/rios", "phone": "--device",
            "install": ["device", "install"], "perf": ["device", "perf"]},
}
# perf.py's measurement run (what `device perf` execs) exits 1 when a phase failed and 4 when the
# record is slower than the benchmark; it writes the record first. With the record on disk these
# are measurements, judged below; 3 (refused) and 6 (the phone's state NOT restored) are not.
MEASURED_EXITS = (1, 4)


def setting(name, default):
    value = os.environ.get("RICHOS_PHONE_WATCH_" + name, "")
    if not value:
        return default
    return value if isinstance(default, str) else type(default)(value)


def poll_s():
    return setting("POLL_S", 30)


def max_wait_s():
    return setting("MAX_WAIT_S", 6 * 3600)


def trials():
    # The benchmark series are 100 cold launches and 100 returns; a shorter series has a wider p95 and
    # would be judged against an allowance computed from 100-trial series.
    return setting("TRIALS", 100)


def now():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def say(text):
    print(f"phone speed watch: {text}", file=sys.stderr, flush=True)


def plain_env(base=None):
    """The environment without git's hook variables, so a child's git never writes into the hook's index."""
    return {k: v for k, v in (base or os.environ).items() if not k.startswith(("GIT_", "RICHOS_AUTOCHECK"))}


def git(repo, *args, check=True):
    p = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, env=plain_env(),
                       stdin=subprocess.DEVNULL)
    if check and p.returncode:
        raise RuntimeError(f"git {' '.join(args)} exited {p.returncode}: {p.stderr.strip()[:300]}")
    return p.stdout.strip() if check else p


# ------------------------------------------------------------------------------------------------
# Where things are
# ------------------------------------------------------------------------------------------------

def home():
    return Path(setting("HOME", DEFAULT_HOME))


def operator_checkout():
    """The one checkout whose lands reach a phone, or None on a machine without the engine plugin."""
    named = setting("REPO", "")
    if named:
        return Path(named).resolve()
    engine = Path.home() / ".claude/richos-engine"
    if not engine.exists():
        return None
    root = engine.resolve().parents[1]
    return root if (root / ".git").exists() else None


def escalate_tool(repo):
    named = setting("ESCALATE", "")
    if named:
        return shlex.split(named)
    local = Path(repo) / "richos/engine/scripts/escalate.sh"
    tool = local if local.is_file() else Path.home() / ".claude/richos-engine/scripts/escalate.sh"
    return ["bash", str(tool)]


def mounted(path):
    """A path under /Volumes/<name> is usable only when that volume is mounted."""
    parts = Path(path).parts
    if len(parts) > 2 and parts[1] == "Volumes":
        return os.path.ismount(os.path.join("/", parts[1], parts[2]))
    return True


# ------------------------------------------------------------------------------------------------
# The ledger: every request, round and outcome, append-only
# ------------------------------------------------------------------------------------------------

_LEDGER_LOCK = threading.Lock()
# Every identifier of a phone this run has seen (serial, UDID, CoreDevice id). The owner's device
# identifiers never appear in the ledger or an escalation: escalation text is quoted into records,
# and a public commit refuses them (engine/scripts/lib/device-identifiers.py).
_PHONE_IDS = set()


def scrub(text):
    for ident in sorted(_PHONE_IDS, key=len, reverse=True):
        text = text.replace(ident, "<phone>")
    return text


def record(event, **fields):
    row = json.loads(scrub(json.dumps({"at": now(), "event": event, **fields})))
    with _LEDGER_LOCK:
        home().mkdir(parents=True, exist_ok=True)
        with open(home() / "ledger.jsonl", "a") as f:
            f.write(json.dumps(row, sort_keys=True) + "\n")
    return row


def rows():
    path = home() / "ledger.jsonl"
    if not path.exists():
        return []
    out = []
    for line in path.read_text().splitlines():
        with contextlib.suppress(ValueError):
            out.append(json.loads(line))
    return out


def uncovered(ledger=None):
    """Requests no finished round covers, oldest first."""
    ledger = rows() if ledger is None else ledger
    finished = {r["round"] for r in ledger if r["event"] == "finished"}
    covered = set()
    for r in ledger:
        if r["event"] == "started" and r["round"] in finished:
            covered.update(r.get("covers", []))
    return [r for r in ledger if r["event"] == "requested" and r["id"] not in covered]


def good():
    try:
        return json.loads((home() / "good.json").read_text())
    except (OSError, ValueError):
        return {}


def _set_entry(name, platform, entry):
    with _LEDGER_LOCK:
        try:
            data = json.loads((home() / name).read_text())
        except (OSError, ValueError):
            data = {}
        data[platform] = entry
        tmp = home() / (name + ".tmp")
        tmp.write_text(json.dumps(data, indent=1, sort_keys=True) + "\n")
        os.replace(tmp, home() / name)


def set_good(platform, entry):
    _set_entry("good.json", platform, entry)


def measured():
    """Each phone's last JUDGED measurement (good or slower): commit, record, verdict."""
    try:
        return json.loads((home() / "measured.json").read_text())
    except (OSError, ValueError):
        return {}


def set_measured(platform, entry):
    _set_entry("measured.json", platform, entry)


@contextlib.contextmanager
def run_lock(block):
    """Held for as long as a runner lives. Yields True when held, False when another runner has it."""
    home().mkdir(parents=True, exist_ok=True)
    with open(home() / "run.lock", "a") as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX if block else fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            yield False
            return
        try:
            yield True
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def runner_alive():
    with run_lock(block=False) as held:
        return not held


# ------------------------------------------------------------------------------------------------
# Reaching Rich
# ------------------------------------------------------------------------------------------------

def escalate(repo, title, question, tried, meanwhile="Nothing: the next land that touches richos/mobile/ "
             "runs the check again; `python3 richos/mobile/perf/watch.py status` shows every run."):
    """Raise one escalation in the lead's ledger. Returns True when delivered."""
    fields = {k: scrub(v) for k, v in {"title": title, "state": "work-complete", "for": "lead", "question": question,
                                       "tried": tried, "meanwhile": meanwhile}.items()}
    with tempfile.NamedTemporaryFile("w", prefix="phone-watch-esc-", suffix=".json", delete=False) as f:
        json.dump(fields, f)
        path = f.name
    try:
        p = subprocess.run([*escalate_tool(repo), "raise", "--fields", path, "--worktree", str(repo), "--no-record"],
                           cwd=str(repo), env=plain_env(), stdin=subprocess.DEVNULL, capture_output=True, text=True)
    finally:
        os.unlink(path)
    delivered = p.returncode == 0
    record("escalated" if delivered else "escalation-failed", title=title,
           detail=None if delivered else (p.stdout + p.stderr).strip()[-600:])
    if not delivered:
        say(f"ESCALATION NOT DELIVERED ({p.returncode}): {title}")
    return delivered


def changed_paths(repo, old, new):
    if not old or set(old) == {"0"}:
        return []
    out = git(repo, "diff", "--name-only", "--no-renames", old, new, "--", MOBILE, check=False)
    if out.returncode:
        raise RuntimeError(f"cannot tell what {old[:12]}..{new[:12]} changed: {out.stderr.strip()[:200]}")
    return out.stdout.split()


def mobile_commits(repo, base, tip, limit=30):
    limit = limit if base else 10  # with no good run to start from, only the latest few
    spec = [f"{base}..{tip}"] if base else ["-n", str(limit), tip]
    out = git(repo, "log", "--oneline", "--no-decorate", *spec, "--", MOBILE, check=False)
    return (out.stdout.strip().splitlines() if out.returncode == 0 else [])[:limit]


def last_good_base(repo, platform, tip):
    last = (good().get(platform) or {}).get("commit")
    if last and git(repo, "merge-base", "--is-ancestor", last, tip, check=False).returncode == 0:
        return last
    return None


def range_text(platform, base, tip):
    if base:
        return f"{base[:12]}..{tip[:12]} (since the last good {platform} run)"
    return f"up to {tip[:12]} (no earlier good {platform} run is recorded)"


# ------------------------------------------------------------------------------------------------
# trigger: called by git at every move of main
# ------------------------------------------------------------------------------------------------

def runner_argv(repo):
    named = setting("RUNNER", "")
    if named:
        return [*shlex.split(named), "run", "--repo", str(repo)]
    return [sys.executable, str(Path(__file__).resolve()), "run", "--repo", str(repo)]


def spawn_runner(repo):
    with open(home() / "runner.log", "a") as log:
        child = subprocess.Popen(runner_argv(repo), cwd=str(repo), env=plain_env(), stdin=subprocess.DEVNULL,
                                 stdout=log, stderr=log, start_new_session=True)
    return child.pid


def report_missed(repo, ledger):
    """Each request no finished round covers, older than the grace, with no runner alive: MISSED, once."""
    if runner_alive():
        return []
    reported = {r["request"] for r in ledger if r["event"] == "missed"}
    cutoff = time.time() - SPAWN_GRACE_S
    missed = []
    for req in uncovered(ledger):
        if req["id"] in reported or calendar.timegm(time.strptime(req["at"], "%Y-%m-%dT%H:%M:%SZ")) > cutoff:
            continue
        record("missed", request=req["id"], main=req["main"])
        escalate(repo, f"Phone speed run MISSED for main {req['main'][:12]} (requested {req['at']})",
                 f"The automatic phone speed run requested at the land of {req['main'][:12]} never finished and no "
                 "runner is alive (killed, or the Mac restarted). A new runner has been started for main's tip; "
                 f"its log is {home() / 'runner.log'}. Was anything stopped deliberately?",
                 "The trigger found the request in the ledger with no finished round covering it.")
        missed.append(req)
    return missed


def cmd_trigger(args):
    repo = Path(args.repo).resolve()
    mine = operator_checkout()
    if mine is None or repo != mine:
        say(f"{repo} is not the operator's main checkout ({mine}); no phone is measured from it")
        return 0
    changed = changed_paths(repo, args.old, args.new)
    if not mounted(home()):
        if changed:
            escalate(repo, f"Phone speed run NOT STARTED for main {args.new[:12]}: {home()} is not mounted",
                     f"The land of {args.new[:12]} changed richos/mobile/ but the phone speed watch cannot record "
                     "anything with the external SSD unmounted. Mount it; the next mobile land runs the check.",
                     "watch.py trigger checked the volume before recording the request.")
        return 0
    missed = report_missed(repo, rows())
    if not changed and not missed:
        say(f"{args.old[:12]}..{args.new[:12]} changes nothing under {MOBILE}; no phone run")
        return 0
    if changed:
        req = record("requested", id=hashlib.sha256(f"{args.new}{time.time_ns()}".encode()).hexdigest()[:16],
                     main=args.new, previous=args.old, paths=len(changed))
        say(f"{len(changed)} path(s) under {MOBILE} changed; phone run requested ({req['id']})")
    try:
        pid = spawn_runner(repo)
    except OSError as exc:
        record("spawn-failed", error=str(exc))
        escalate(repo, f"Phone speed run NOT STARTED for main {args.new[:12]}: the runner could not start",
                 f"watch.py trigger could not start `watch.py run`: {exc}. Nothing was measured for this land.",
                 "Popen of the runner in its own session.")
        return 0
    record("spawned", pid=pid, main=args.new)
    say(f"runner started (pid {pid}); it waits for each phone to be free; log {home() / 'runner.log'}")
    return 0


# ------------------------------------------------------------------------------------------------
# Is a phone free?
# ------------------------------------------------------------------------------------------------

ANDROID_TOOLS = ("phone-android.py", "hidden-send-try.py", "perf.py android", "apk-install.sh",
                 "android-install-fresh", "randroid device")
IOS_TOOLS = ("phone-ios.py", "devicectl", "xctrace", "physical-device.mjs", "rios device", "rios perf",
             "perf.py ios", "ios-install-fresh")


def processes():
    out = subprocess.run(["ps", "-axo", "pid=,ppid=,args="], capture_output=True, text=True).stdout
    procs = []
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[0].isdigit() and parts[1].isdigit():
            procs.append((int(parts[0]), int(parts[1]), parts[2]))
    return procs


def own_tree(procs, me=None):
    """This process, its ancestors and every descendant: never counted as someone using a phone."""
    me = me or os.getpid()
    parent = {pid: ppid for pid, ppid, _ in procs}
    below = {me}
    grew = True
    while grew:  # descendants of this process only: an ancestor's other children are someone else
        grew = False
        for pid, ppid, _ in procs:
            if ppid in below and pid not in below:
                below.add(pid)
                grew = True
    above, pid = set(), me
    while parent.get(pid, 0) > 1 and parent[pid] not in above:
        pid = parent[pid]
        above.add(pid)
    return below | above


def users(platform, idents, procs=None, me=None):
    """Processes outside this run that are using (or may be using) the phone. Name-matched for
    POLLING ONLY: nothing here is ever sent a signal."""
    procs = processes() if procs is None else procs
    mine = own_tree(procs, me)
    # RICHOS_PHONE_WATCH_MARKERS=ident counts only processes naming the phone itself: the fixture
    # suite's made-up phones, on a Mac whose real phones other agents are using.
    by_tool = setting("MARKERS", "ident,tools") != "ident"
    tools = (ANDROID_TOOLS if platform == "android" else IOS_TOOLS) if by_tool else ()
    found = []
    for pid, _ppid, args in procs:
        if pid in mine:
            continue
        hit = any(i and i in args for i in idents) or any(t in args for t in tools)
        if platform == "android" and by_tool and not hit:
            words = args.split()
            exe = os.path.basename(words[0]) if words else ""
            # An adb client with no serial reaches whatever is attached; the server itself does not.
            hit = exe == "adb" and "fork-server" not in args and "emulator-" not in args
        if hit:
            # Masked BEFORE it is shortened: a cut identifier would no longer be recognized.
            words = args.split(" ", 1)
            shown = " ".join([os.path.basename(words[0]), *words[1:]])
            for ident in sorted((i for i in idents if i), key=len, reverse=True):
                shown = shown.replace(ident, "<phone>")
            found.append((pid, shown[:160]))
    return found


def wait_until_free(platform, idents, sample=None, clock=time.monotonic, sleep=time.sleep, log=say):
    """Return at the first sample with no user of the phone: (True, waited_s); (False, last users) after MAX_WAIT_S."""
    sample = sample or (lambda: users(platform, idents))
    start = clock()
    while True:
        busy = sample()
        t = clock()
        if not busy:
            return True, round(t - start)
        if t - start >= max_wait_s():
            return False, busy
        log(f"{platform}: busy ({len(busy)} process(es), e.g. pid {busy[0][0]}); waiting, interrupting nothing")
        sleep(poll_s())


# ------------------------------------------------------------------------------------------------
# The phones, through the platform command lines only
# ------------------------------------------------------------------------------------------------

def real_tool(name):
    named = setting(name.upper(), "")
    if named:
        return named
    if name == "xcrun":
        return "/usr/bin/xcrun"
    found = shutil.which("adb")
    if found:
        return found
    return str(Path(os.environ.get("ANDROID_HOME", "/opt/homebrew/share/android-commandlinetools"))
               / "platform-tools/adb")


def android_phones():
    """Serials of wired Android phones, from the adb server's list (the phones are not contacted)."""
    p = subprocess.run([real_tool("adb"), "devices"], capture_output=True, text=True)
    return [line.split()[0] for line in p.stdout.splitlines()[1:]
            if len(line.split()) == 2 and line.split()[1] == "device" and not line.startswith("emulator-")]


def iphones(work):
    """Wired iPhones, from CoreDevice's list (the phones are not contacted)."""
    out = Path(work) / "devices.json"
    p = subprocess.run([real_tool("xcrun"), "devicectl", "list", "devices", "--json-output", str(out)],
                       capture_output=True, text=True)
    if p.returncode or not out.exists():
        return []
    found = []
    for dev in json.loads(out.read_text()).get("result", {}).get("devices", []):
        hw, conn = dev.get("hardwareProperties", {}), dev.get("connectionProperties", {})
        if hw.get("reality") == "physical" and hw.get("deviceType") == "iPhone" and conn.get("transportType") == "wired":
            found.append({"identifier": dev.get("identifier"), "udid": hw.get("udid")})
    out.unlink()
    return found


def find_phone(platform, work):
    """(the identifier the verbs are given, every identifier a process may name the phone by)."""
    if platform == "android":
        phones = android_phones()
        if not phones:
            raise Unmeasured("no Android phone is wired to this Mac (adb devices lists none)")
        _PHONE_IDS.update(phones)
        return phones[0], phones
    phones = iphones(work)
    if not phones:
        raise Unmeasured("no iPhone is wired to this Mac (devicectl lists none)")
    idents = [v for d in phones for v in (d["identifier"], d.get("udid")) if v]
    _PHONE_IDS.update(idents)
    return phones[0]["identifier"], idents


class Guard:
    """The per-run wrappers in front of adb and xcrun (phone_guard.py), and the run's trip file."""

    def __init__(self, work):
        self.dir = Path(work) / "guard"
        self.dir.mkdir(parents=True, exist_ok=True)
        self.trip = Path(work) / "guard-tripped.txt"
        script = HERE / "phone_guard.py"
        for tool in ("adb", "xcrun"):
            path = self.dir / tool
            path.write_text(f"#!/bin/sh\nexec {shlex.quote(sys.executable)} {shlex.quote(str(script))} {tool} \"$@\"\n")
            path.chmod(0o755)

    def env(self):
        env = plain_env()
        env["PATH"] = f"{self.dir}:{env.get('PATH', '')}"
        env["RICHOS_PHONE_GUARD_TRIP"] = str(self.trip)
        env["RICHOS_PHONE_GUARD_REAL_ADB"] = real_tool("adb")
        env["RICHOS_PHONE_GUARD_REAL_XCRUN"] = real_tool("xcrun")
        if not env.get("RICHOS_APPLE_TEAM"):
            team = newest_store_team()
            if team:
                env["RICHOS_APPLE_TEAM"] = team
        return env

    def tripped(self):
        return self.trip.read_text().strip().splitlines()[0] if self.trip.exists() else None


def newest_store_team():
    """The signing team of the newest iPhone build in the shared store: the team this Mac signs with,
    for a runner started from a land whose environment has no RICHOS_APPLE_TEAM."""
    with contextlib.suppress(OSError):
        entries = sorted(IOS_STORE.glob("*/identity.json"), key=lambda p: p.stat().st_mtime, reverse=True)
        for entry in entries:
            with contextlib.suppress(OSError, ValueError):
                team = json.loads(entry.read_text()).get("team")
                if team:
                    return team
    return ""


def run_logged(argv, log_path, env=None, cwd=None):
    with open(log_path, "a") as log:
        log.write(f"$ {' '.join(shlex.quote(a) for a in argv)}\n")
        log.flush()
        p = subprocess.run(argv, env=env or plain_env(), cwd=cwd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                           stderr=log, text=True)
        log.write(p.stdout)
    return p


def log_tail(log_path, lines=2):
    """The last lines a tool printed (the logged command lines themselves are left out)."""
    try:
        text = [line for line in Path(log_path).read_text().strip().splitlines() if not line.startswith("$ ")]
    except OSError:
        return ""
    return " / ".join(line.strip()[:200] for line in text[-lines:])


def prepare_checkout(repo, sha, log_path, name="checkout"):
    """A plain clone on the external SSD at `sha`, reused between runs so builds stay warm; never a
    worktree of the repository, so no workspace registry or reaper ever sees it. --shared reads the
    repository's own objects, so every commit of main is already there."""
    clone = home() / name
    if not (clone / ".git").exists():
        if run_logged(["git", "clone", "--quiet", "--shared", "--no-checkout", str(repo), str(clone)],
                      log_path).returncode:
            raise Unmeasured(f"could not clone {repo} for the build (see {log_path})")
    if run_logged(["git", "-C", str(clone), "checkout", "--quiet", "--force", "--detach", sha], log_path).returncode:
        raise Unmeasured(f"could not put the build clone at {sha[:12]} (see {log_path})")
    return clone


class Unmeasured(Exception):
    """The run could not produce a comparable record; the sentence says why."""


class Refused(Unmeasured):
    """The run stopped before changing the phone because it would have needed an uninstall."""


def json_tail(text):
    for line in reversed(text.strip().splitlines()):
        with contextlib.suppress(ValueError):
            return json.loads(line)
    with contextlib.suppress(ValueError):
        return json.loads(text)
    return None


def cli(platform, checkout):
    named = setting(platform.upper() + "_CLI", "")
    return shlex.split(named) if named else [str(Path(checkout) / VERBS[platform]["cli"])]


def verb(platform, name, checkout, phone, extra, guard, log_path, record_out=None):
    """One platform verb against the phone, through the guard. Raises Refused when the guard stopped
    it, Unmeasured when it failed or does not exist in this checkout yet. With `record_out`, an exit
    in MEASURED_EXITS that wrote the record there is a measurement, returned for judging."""
    spec = VERBS[platform]
    argv = [*cli(platform, checkout), *spec[name], spec["phone"], phone, *extra]
    if platform == "android":
        # randroid refuses an unsigned APK, and a phone installs only a signed one: the Release build (install)
        # and the debuggable twin the measurement builds (perf, "must carry the upload key, the key of the
        # release build on the phone") must carry the upload key, which richos-hq's wrapper puts in the child's environment (never in this
        # public repository, never on a command line).
        wrapper = shlex.split(setting("SIGNING", "") or default_signing(operator_checkout() or HERE))
        if not Path(wrapper[-1]).is_file():
            raise Unmeasured(f"the Android signing wrapper {wrapper[-1]} is missing: the Release build would be "
                             "unsigned and the phone refuses it")
        argv = [*wrapper, *argv]
    p = run_logged(argv, log_path, env=guard.env(), cwd=str(checkout))
    stopped = guard.tripped()
    if stopped:
        raise Refused(f"the phone guard stopped `{' '.join(spec[name])}` before it changed the phone: {stopped}")
    if p.returncode in MEASURED_EXITS and record_out and Path(record_out).exists():
        return p
    if p.returncode:
        tail = log_tail(log_path)
        if any(s in tail.lower() for s in ("unknown mode", "unknown command", "unknown verb", "usage")):
            raise Unmeasured(f"`{Path(spec['cli']).name} {' '.join(spec[name])}` does not exist in main yet "
                             f"(zach-opus-releaseonly1 is adding it): {tail}")
        raise Unmeasured(f"`{Path(spec['cli']).name} {' '.join(spec[name])}` exited {p.returncode}: {tail}")
    return p


def perf_py(checkout):
    return setting("PERF", "") or str(Path(checkout) / "richos/mobile/perf/perf.py")


def compare(checkout, record_path, log_path):
    """perf.py compare on the record: ("good"|"slower"|"not-compared", the comparison). Files only."""
    p = run_logged([sys.executable, perf_py(checkout), "compare", "--json", record_path], log_path)
    result = json_tail(p.stdout) or {}
    metrics = result.get("metrics") or {}
    if p.returncode == 4:
        return "slower", result
    if p.returncode == 0:
        missing = [m for m in METRICS if (metrics.get(m) or {}).get("status", "NOT COMPARED") == "NOT COMPARED"]
        if missing:
            return "not-compared", {**result, "why": f"{', '.join(missing)} NOT COMPARED: " +
                                    "; ".join(str((metrics.get(m) or {}).get("why")) for m in missing)}
        return "good", result
    return "not-compared", {**result, "why": result.get("why") or f"perf.py compare exited {p.returncode}"}


def blank_starts():
    return setting("BLANK_STARTS", "10")


# A platform whose record must carry the blank-screen verdict. The iPhone joins when its cold-start
# recording does (isaac-opus-white1); until then its record is judged only when it carries one.
BLANK_REQUIRED = ("android",)
IPHONE_BLANK_PENDING = ("not checked on the iPhone yet: its cold-start blank check needs "
                        "isaac-opus-white1's screen recording of the tap launches")


def blank_screen_problems(platform, record_data):
    """quint-opus-blank1's cold-start blank check, as the run's verdict. The analyzer
    (blankstart.android_cold_blank, judged by blank.py) ran inside the perf verb and left its
    series in the record as `metrics.coldBlank`. Returns one sentence per failure; a non-empty list
    makes the run a REGRESSION exactly as a slower p95 does. Raises Unmeasured when a platform in
    BLANK_REQUIRED has no verdict: the start was not checked for a blank screen."""
    metrics = record_data.get("metrics") or {}
    cb = metrics.get("coldBlank")
    if not cb:
        if platform in BLANK_REQUIRED:
            phase = (record_data.get("phases") or {}).get("cold-blank", "the record has no cold-blank phase")
            raise Unmeasured(f"the cold-start blank-screen check gave no verdict ({phase})")
        return []
    if cb.get("verdict") == "PASS":
        return []
    label = "Android" if platform == "android" else "iPhone"
    problems = [f"{label} cold start BLANK SCREEN: {'; '.join(cb.get('why') or ['verdict ' + str(cb.get('verdict'))])} "
                f"(limit {cb.get('limitMs')} ms, blank.py)"]
    over = cb.get("overLimit") or []
    if over:
        worst = max(o.get("worstMs") or 0 for o in over)
        problems.append(f"{len(over)} of {len(cb.get('starts') or [])} starts over the limit, the worst {worst:.0f} ms")
    return problems


def blank_screen_status(platform, record_data, problems):
    """What the outcome row says about the blank-screen check."""
    if problems:
        return problems
    cb = (record_data.get("metrics") or {}).get("coldBlank")
    if not cb:
        return IPHONE_BLANK_PENDING if platform == "ios" else "not checked"
    value = cb.get("valueMs")
    return (f"PASS: {cb.get('statistic')} {value if value is None else round(value)} ms of "
            f"{len(cb.get('samplesMs') or [])} starts, limit {cb.get('limitMs')} ms")


def cold_standard(platform, record_data):
    """The cold-start verdict (the judge lives in perfcore.cold_standard, shared with `device perf`)."""
    try:
        return _pc().cold_standard(platform, record_data)
    except _pc().NoVerdict as e:
        raise Unmeasured(str(e)) from None


def warm_standard(record_data):
    """The warm-start verdict as the perf run recorded it (the judge lives in perfcore.warm_standard)."""
    try:
        return _pc().warm_standard(record_data)
    except _pc().NoVerdict as e:
        raise Unmeasured(str(e)) from None


def standard_line(label, v):
    return _pc().standard_line(label, v)


def summary_line(result):
    parts = []
    for name in METRICS:
        m = (result.get("metrics") or {}).get(name) or {}
        if m.get("p95Ms") is not None:
            parts.append(f"{name} p95 {m['p95Ms']} ms ({m.get('status')}, limit {m.get('limitMs')} ms)")
        else:
            parts.append(f"{name} {m.get('status', 'NOT COMPARED')}")
    return "; ".join(parts)


def last_measurement(repo, platform, tip):
    """The phone's last judged measurement when its record is still on disk and its commit is an
    ancestor of `tip`, else None (then the phone is measured)."""
    entry = measured().get(platform)
    if not entry and good().get(platform):
        entry = {**good()[platform], "verdict": "good"}  # a state home from before measured.json
    if not entry or not entry.get("record") or not Path(entry["record"]).is_file():
        return None
    if git(repo, "merge-base", "--is-ancestor", entry["commit"], tip, check=False).returncode:
        return None
    return entry


def app_changed(repo, platform, base, sha):
    """True when a path that can change `platform`'s app build changed in base..sha (CEO 2026-10-03:
    a land that changed only limits, tests or tooling does not run the phone again)."""
    return any(is_app_code(platform, p) for p in changed_paths(repo, base, sha))


def judge_record(checkout, platform, out, log_path):
    """Judge one record against the CURRENT limits (perf.py compare reads files only, never a phone).
    Raises Unmeasured when it cannot be judged. Returns the pieces of the verdict."""
    # The old p95 benchmark comparison is information only and decides nothing (CEO §104).
    _, result = compare(checkout, out, log_path)
    with open(out) as f:
        data = json.load(f)
    info = summary_line(result)
    cold = cold_standard(platform, data)
    if cold["verdict"] == "INCOMPLETE":
        raise Unmeasured(f"the cold-start test gave no verdict: {cold['why']}; benchmark (information only): {info}")
    try:
        warm = warm_standard(data) if cold["verdict"] == "PASS" else None
    except Unmeasured as exc:
        raise Unmeasured(f"{exc}; {standard_line('cold', cold)}") from None
    try:
        blanks = blank_screen_problems(platform, data)
    except Unmeasured as exc:
        raise Unmeasured(f"{exc}; {standard_line('cold', cold)}") from None
    failed = [standard_line(k, v) for k, v in (("cold", cold), ("warm", warm)) if v and v["verdict"] == "FAIL"]
    return {"data": data, "info": info, "blanks": blanks, "failed": failed,
            "verdict": "slower" if failed or blanks else "good",
            "report": "; ".join(standard_line(k, v) for k, v in (("cold", cold), ("warm", warm)) if v)}


def measure(platform, checkout, sha, phone, guard, log_path, work, tag=""):
    """Install `sha`'s release build from `checkout` over the app and measure it; returns the record path."""
    verb(platform, "install", checkout, phone, ["--expect-commit", sha], guard, log_path)
    out = str(work / f"{platform}{tag}.json")
    n = str(_pc().COLD_STARTS)  # a start test is 20 normal starts (§104); the p95 benchmark is information only
    # The iPhone's judged cold start is the launch with nothing attached (`--launches`), never an Instruments one.
    cold_args = ["--cold", "0", "--launches", n] if platform == "ios" else ["--cold", n]
    extra = ["--expect-commit", sha, *cold_args, "--warm", n, "--evidence-dir", str(work / f"evidence{tag}"),
             "--out", out]
    if platform in BLANK_REQUIRED:
        extra += ["--blank-starts", blank_starts()]  # perf.py's cold-blank phase (quint-opus-blank1)
    verb(platform, "perf", checkout, phone, extra, guard, log_path, record_out=out)
    if not Path(out).exists():
        raise Unmeasured(f"the perf verb wrote no record at {out}: {log_tail(log_path)}")
    return out


def diagnose(repo, platform, base, sha, span, phone, guard, log_path, work, checkout):
    """A round just FAILED this phone's standard. At once, on the same phone, measure the last good
    build the same way: if it fails too the phone's condition is the cause, if it passes the code is.
    Returns one sentence. The last good build comes from its own clone (the other phone's thread is
    building in `checkout`), and the tip's build goes back on the phone afterwards."""
    if not base:
        return "NOT TOLD APART: no earlier good run of this phone is recorded to measure against."
    try:
        control_dir = prepare_checkout(repo, base, work / "control-checkout.log", name=f"control-{platform}")
        out = measure(platform, control_dir, base, phone, guard, log_path, work, tag="-control")
        control = judge_record(control_dir, platform, out, log_path)
    except Unmeasured as exc:
        return f"NOT TOLD APART: the last good build ({base[:12]}) could not be measured on this phone: {exc}"
    finally:
        with contextlib.suppress(Unmeasured):  # the phone goes back to the build under test
            verb(platform, "install", checkout, phone, ["--expect-commit", sha], guard, log_path)
    if control["verdict"] == "slower":
        return (f"THE PHONE'S CONDITION, NOT THE CODE: the last good build ({base[:12]}) measured on this same "
                f"phone just now also fails ({'; '.join(control['failed'] + control['blanks'])}).")
    return (f"A CODE REGRESSION in {span}: the last good build ({base[:12]}) measured on this same phone just "
            f"now still passes ({control['report']}).")


def second_look(platform, sha, phone, idents, guard, log_path, work, checkout):
    """Measure the tip once more on the same phone, waiting for the phone as the first measurement did.
    Returns ("good", judged), ("slower", judged) or ("busy", why): never a guess on one measurement."""
    try:
        free, detail = wait_until_free(platform, idents)
        if not free:
            return "busy", (f"still busy after {max_wait_s()} s: "
                            + "; ".join(f"pid {pid}: {args}" for pid, args in detail[:3]))
        out = measure(platform, checkout, sha, phone, guard, log_path, work, tag="-again")
        again = judge_record(checkout, platform, out, log_path)
    except Unmeasured as exc:
        return "busy", f"the second measurement could not be made: {exc}"
    again["record"] = out
    return ("good" if again["verdict"] == "good" else "slower"), again


def escalate_slower(repo, label, platform, sha, span, commits, judged, record_path, log_path, how, diagnosis=None):
    failed, blanks, report = judged["failed"], judged["blanks"], judged["report"]
    what = (("start FAILS the speed standard" + (" and a BLANK SCREEN" if blanks else "")) if failed
            else "start shows a BLANK SCREEN")
    verdict_title = next((t for start, t in (("THE PHONE", " (the phone's condition, not the code)"),
                                             ("A CODE REG", " (a code regression)"),
                                             ("NOT TOLD", " (code or phone not told apart)"))
                          if (diagnosis or "").startswith(start)), "")
    escalate(repo, f"Phone speed: {label} {what} at main {sha[:12]}{verdict_title}",
             f"{diagnosis + ' ' if diagnosis else ''}{label} {report}{'; ' + '; '.join(blanks) if blanks else ''}. "
             f"Range {span}. "
             f"Commits in it touching richos/mobile/: {' | '.join(commits) or 'none listed'}. Which of them "
             f"{'slowed the start' if failed else 'put a blank screen on the start'}, and who fixes it?",
             f"{how} The cold-start test (CEO 2026-10-03, §104: starts "
             f"2-5 under the per-start limit, 2-20 average under the average limit) with the benchmark's fixed "
             f"conversation. Benchmark p95 comparison (information only): {judged['info']}. "
             f"Record: {record_path}; log: {log_path}.")


def run_platform(repo, round_id, sha, platform, work, checkout):
    """One phone: wait, install, measure, compare, report. Returns its outcome row."""
    work = Path(work) / platform
    work.mkdir(parents=True, exist_ok=True)
    log_path = work / "run.log"
    guard = Guard(work)
    base = last_good_base(repo, platform, sha)
    span = range_text(platform, base, sha)
    commits = mobile_commits(repo, base, sha)
    label = "Android" if platform == "android" else "iPhone"
    outcome = {"round": round_id, "platform": platform, "main": sha, "range": span, "log": str(log_path)}
    try:
        if isinstance(checkout, Exception):
            raise checkout
        last = last_measurement(repo, platform, sha)
        if last and not app_changed(repo, platform, last["commit"], sha):
            # Nothing that builds into this phone's app changed since its last measurement: its start
            # cannot have changed. The land's limits may have: judge those samples again, phone untouched.
            judged = judge_record(checkout, platform, last["record"], log_path)
            outcome.update(verdict="unchanged" if judged["verdict"] == "good" else "slower", record=last["record"],
                           standard=judged["report"], compare=judged["info"],
                           why=f"no {platform} app code changed since the last measurement ({last['commit'][:12]}); "
                               f"its samples were judged again against the current limits, the phone not touched")
            set_measured(platform, {**last, "commit": sha, "at": now(), "verdict": judged["verdict"],
                                    "carriedFrom": last["commit"]})
            if judged["verdict"] == "good":
                set_good(platform, {"commit": sha, "at": now(), "record": last["record"],
                                    "compare": judged["report"], "carriedFrom": last["commit"]})
            elif last.get("verdict") != "slower":  # newly failing under the current limits; a repeat is not news
                escalate_slower(repo, label, platform, sha, span, commits, judged, last["record"], log_path,
                                "No phone run: no app code changed since the last measurement, so those "
                                "samples were judged again against the current limits.")
            return outcome
        phone, idents = find_phone(platform, work)
        free, detail = wait_until_free(platform, idents)
        if not free:
            raise Unmeasured(f"the phone stayed busy for {max_wait_s()} s; still using it: "
                             + "; ".join(f"pid {pid}: {args}" for pid, args in detail[:3]))
        record("phone-free", round=round_id, platform=platform, waited=detail)
        out = measure(platform, checkout, sha, phone, guard, log_path, work)
        judged = judge_record(checkout, platform, out, log_path)
        verdict = judged["verdict"]
        outcome.update(verdict=verdict, record=out, standard=judged["report"], compare=judged["info"],
                       blankScreen=blank_screen_status(platform, judged["data"], judged["blanks"]))
        set_measured(platform, {"commit": sha, "at": now(), "record": out, "verdict": verdict})
        if verdict == "good":
            set_good(platform, {"commit": sha, "at": now(), "record": out, "compare": judged["report"]})
        else:
            # Tell the code from the phone at once, on the same phone (CEO 2026-10-03: a phone in a bad
            # condition was first reported as a code regression, and a teammate spent 20 minutes finding out).
            diagnosis = diagnose(repo, platform, base, sha, span, phone, guard, log_path, work, checkout)
            if diagnosis.startswith("A CODE REG"):
                # One failure with a passing control is not yet a code regression (CEO 2026-10-04: a phone busy
                # for one run was reported as one again): measure main once more on the same phone.
                look, again = second_look(platform, sha, phone, idents, guard, log_path, work, checkout)
                if look == "good":
                    outcome.update(verdict="good", record=again["record"], standard=again["report"],
                                   compare=again["info"], firstFailure=judged["report"],
                                   blankScreen=blank_screen_status(platform, again["data"], again["blanks"]),
                                   diagnosis="THE PHONE'S CONDITION, NOT THE CODE: main passed when measured again "
                                             "straight after; the first failure is recorded as the phone's condition.")
                    set_measured(platform, {"commit": sha, "at": now(), "record": again["record"],
                                            "verdict": "good", "firstFailure": judged["report"],
                                            "firstRecord": out})
                    set_good(platform, {"commit": sha, "at": now(), "record": again["record"],
                                        "compare": again["report"]})
                    return outcome  # no escalation
                if look == "busy":
                    diagnosis = (f"NOT TOLD APART: could not tell: the phone was busy when main was to be "
                                 f"measured again ({again}); main failed once and the last good build passed.")
                else:
                    diagnosis += " Main was measured again straight after and failed again."
            outcome.update(diagnosis=diagnosis)
            escalate_slower(repo, label, platform, sha, span, commits, judged, out, log_path,
                            "The automatic run after the land measured it.", diagnosis)
    except Unmeasured as exc:
        kind = "refused" if isinstance(exc, Refused) else "unmeasured"
        outcome.update(verdict=kind, why=str(exc))
        escalate(repo, f"Phone speed: {label} run {'REFUSED' if kind == 'refused' else 'COULD NOT MEASURE'} "
                       f"at main {sha[:12]}",
                 f"{exc}. The {label} start was NOT checked for range {span}. Commits in it touching richos/mobile/: "
                 f"{' | '.join(commits) or 'none listed'}. What has to change so it can be measured?",
                 f"The automatic run after the land of {sha[:12]}; log: {log_path}.")
    except Exception as exc:  # noqa: BLE001 — a crash is a run that could not measure, said loudly
        outcome.update(verdict="unmeasured", why=f"{type(exc).__name__}: {exc}")
        escalate(repo, f"Phone speed: {label} run CRASHED at main {sha[:12]}",
                 f"{type(exc).__name__}: {exc}. The {label} start was NOT checked for range {span}.",
                 f"The automatic run after the land of {sha[:12]}; log: {log_path}.")
    finally:
        record("outcome", **outcome)
    return outcome


def platforms():
    return [p for p in setting("PLATFORMS", ",".join(PLATFORMS)).split(",") if p in PLATFORMS]


KEEP_ROUNDS = 20


def prune_runs():
    """Keep the newest KEEP_ROUNDS rounds' directories (records, logs, an iPhone series' traces) and
    every round a good run points at; the ledger keeps every outcome forever."""
    runs = home() / "runs"
    if not runs.is_dir():
        return []
    kept = {Path(g["record"]).parents[1].name for g in [*good().values(), *measured().values()] if g.get("record")}
    rounds = sorted((p for p in runs.iterdir() if p.is_dir()), key=lambda p: p.name, reverse=True)
    removed = []
    for old in rounds[KEEP_ROUNDS:]:
        if old.name not in kept:
            shutil.rmtree(old, ignore_errors=True)
            removed.append(old.name)
    if removed:
        record("pruned", rounds=removed)
    return removed


LOADED = Path(__file__).read_bytes()  # the watch code this process is running, as it was when it started


def newer_watch(checkout):
    """The land's own copy of this file when it differs from the code this runner loaded, else None."""
    if isinstance(checkout, Exception):
        return None
    mine = Path(checkout) / "richos/mobile/perf/watch.py"
    return mine if mine.is_file() and mine.read_bytes() != LOADED else None


def cmd_run(args):
    repo = Path(args.repo).resolve()
    with run_lock(block=True):
        (home() / "runner.json").write_text(json.dumps({"pid": os.getpid(), "since": now()}) + "\n")
        while True:
            pending = uncovered()
            if not pending:
                say("nothing pending")
                return 0
            sha = git(repo, "rev-parse", "refs/heads/main")
            round_id = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + "-" + sha[:12]
            work = home() / "runs" / round_id
            work.mkdir(parents=True, exist_ok=True)
            record("started", round=round_id, main=sha, covers=[r["id"] for r in pending])
            try:
                checkout = prepare_checkout(repo, sha, work / "checkout.log")
            except Unmeasured as exc:
                checkout = exc
            newer = newer_watch(checkout)
            if newer:  # a land is measured and judged by its own watch code, never an older runner's
                say(f"this land changes the watch; continuing in {newer}")
                os.execv(sys.executable, [sys.executable, str(newer), "run", "--repo", str(repo)])
            outcomes = []
            threads = [threading.Thread(target=lambda p=p: outcomes.append(
                run_platform(repo, round_id, sha, p, work, checkout))) for p in platforms()]
            for t in threads:
                t.start()
            for t in threads:
                t.join()
            record("finished", round=round_id, main=sha, verdicts={o["platform"]: o["verdict"] for o in outcomes})
            prune_runs()


def cmd_status(args):
    ledger = rows()
    state = {"home": str(home()), "runnerAlive": runner_alive(), "good": good(),
             "uncovered": [{"id": r["id"], "main": r["main"], "at": r["at"]} for r in uncovered(ledger)],
             "recent": [r for r in ledger if r["event"] in ("outcome", "missed", "spawn-failed",
                                                            "escalation-failed")][-10:]}
    if args.json:
        print(json.dumps(state, indent=1))
        return 0
    print(f"state: {state['home']}  runner alive: {state['runnerAlive']}")
    for platform in PLATFORMS:
        g = state["good"].get(platform)
        print(f"last good {platform}: {g['commit'][:12]} at {g['at']} ({g.get('compare')})" if g
              else f"last good {platform}: none")
    for r in state["uncovered"]:
        print(f"pending: main {r['main'][:12]} requested {r['at']}")
    for r in state["recent"]:
        what = r.get("verdict") or r["event"]
        print(f"{r['at']}  {r.get('platform', '-'):7s} {what:12s} {(r.get('main') or '')[:12]}  "
              f"{r.get('compare') or r.get('why') or ''}"[:220])
    return 0


def cmd_busy(args):
    found = users(args.platform, args.ident or [])
    for pid, what in found:
        print(f"{pid}\t{what}")
    print(f"{args.platform}: {'BUSY' if found else 'free at this sample'} ({len(found)} process(es))")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("trigger")
    t.add_argument("--repo", required=True)
    t.add_argument("--from", dest="old", required=True)
    t.add_argument("--to", dest="new", required=True)
    sub.add_parser("run").add_argument("--repo", required=True)
    sub.add_parser("status").add_argument("--json", action="store_true")
    b = sub.add_parser("busy")
    b.add_argument("platform", choices=PLATFORMS)
    b.add_argument("--ident", action="append")
    args = parser.parse_args(argv)
    return {"trigger": cmd_trigger, "run": cmd_run, "status": cmd_status, "busy": cmd_busy}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
