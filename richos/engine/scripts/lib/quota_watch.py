#!/usr/bin/env python3
"""quota_watch.py — the CEO's 93% quota rule, as one reusable reader and watcher.

HIS WORDS ARE THE WHOLE BEHAVIOR (ruling §87, richos-hq/wiki/ceo-decisions.md):

    "quota polling: every 5 minutes from now. And once it crosses the 93%
    threshold: PAUSE subagents. Then resume after quota rest."

HIS UPDATE, 2026-09-25 (ruling §87, and the boundary cases in
richos-hq/docs/plans/claude-five-hour-quota-rule-2026-09-25.md). At or above
the threshold, do NOT pause when the reset is LESS than 20 minutes away;
exactly 20 minutes still pauses. A hold already in place releases inside that
window, keeping the same agent. The same day he dropped the adaptive schedule
he had also given: "Well, I've changed my mind. Let's drop the 30-minute check
nonsense. Keep it consistent at 5 minutes." So polling is 300 s at every
usage level, and the stale bound stays one 300 s poll.

The five-hour policy below is retained. Weekly support is in quota_weekly.py:
all reported windows are printed; at 99% overall weekly use, the shared reset
service consumes a prior user approval or the lead receives the standard pause
message. Weekly holds release only after fresh weekly allowance and an allowing
five-hour verdict. The twenty-minute exception applies only to five-hour quota.

The original five-hour watcher READS the five-hour window,
it POLLS every 300 seconds, and it WAKES THE LEAD at the threshold, at the
hold's release (the reset less than 20 minutes away) and at the reset, with
the exact messages to send. The lead does the pausing and the resuming; this
file never messages, stops or spawns anything.

THE LAST 20 MINUTES, and how it matches the desktop app (Codex, branch
codex/claude-quota-settings, richos-core/src/quota.rs: RESET_EXEMPTION_MS and
its threshold_and_twenty_minute_boundaries test): the exception applies when
0 < resets_at - now < 1200 s. It applies to a stale reading too, because
resets_at does not move inside one window. A window that has ended is
UNKNOWN, as before, never "inside the exception".

PAUSE MESSAGES ARE NOT WRITTEN BY THE LEAD. scripts/lib/pause_protocol.py renders
the one standard message used here and by manual pauses. The terminal and desktop
SendMessage gates validate its complete text before delivery. The lead may not
append termination, cancellation, hand-in or restart instructions. The pause-until
line retains the existing registry binding; delivery is a request, not evidence
that execution is already held. No subagent implementation is changed here.
Each wake-up names the command that makes the pause or the resume
(pause_protocol.py --to / --resume --to): it holds or releases the agent itself,
before any message, so a refused or lost message never leaves an agent running
through a pause or frozen after one (2026-10-11).

THE READING, FIRST: CLAUDE CODE'S OWN `get_usage` (2026-09-25). Each poll starts
`claude` as a CONTROL-ONLY connection and asks it for the account's usage,
exactly as the desktop app does (Codex, codex/claude-quota-settings at
c53f85c9, richos-core/src/quota/probe.rs; spec: richos-hq
docs/plans/desktop-claude-quota-settings-2026-09-25.md, "Data source and
process ownership"): stream-json in and out, no setting sources, no session
persistence, no tools, strict and empty MCP config, CLAUDECODE removed, an
`initialize` then a `get_usage` control request, each bounded at 20 s, frames
capped at 256 KiB. It never sends a user message, so it is not a model turn.
One difference, on purpose: the desktop keeps one connection open; this
watcher is a short-lived script with 5 minutes between polls, so it starts
the process in its own process group, captured at spawn, and ends that group
on every read. The working directory is `/var/empty` (CONTROL_CWD), which is
empty and which this user cannot write, so a read can neither leave anything
behind nor read anything there. Never `/` (it was until 2026-10-07): at start
Claude Code lists every file under its working directory (`rg --no-config
--files --hidden <cwd>`, 2.1.292), so from `/` every read walked the whole
disk, and run from launchd, where no Terminal with Full Disk Access stands in
front of it, each protected place it reached (Desktop, Downloads, the external
SSD, network volumes) put a macOS permission dialog on the CEO's screen.
Measured here on 2026-09-25 with claude
2.1.282: initialize 1.2 s, get_usage 7.2 s, five_hour answered, the only
message type on stdout `control_response`, nothing written to the working
directory and no transcript under ~/.claude/projects.

Why it is first: the status-line file below re-renders only when the lead
speaks, so an idle lead left the reading 10 minutes old at the 91%/95%
readings of 07:57Z and 08:07Z. get_usage is fresh at every poll. When it
fails (no `claude`, a timeout, an error, no subscription limits reported, or
an answer this file cannot read), the watcher says why and falls back to the
status-line file, where every rule below still applies, including waking the
lead to refresh it.

THE FALLBACK READING (design notes: richos-hq/docs/plans/budget-self-management-2026-09-10.md)
  ~/.claude/statusline-payload.json -> rate_limits.five_hour.used_percentage and
  .resets_at (epoch seconds). ~/.claude/statusline.sh copies the host's payload
  there on every status-line render (since 2026-09-10). Nothing else is read,
  and no credential is ever touched.

  R6 of the design notes: a reading that is missing, stale or unreadable is its
  OWN state and is never rounded to "plenty left". The file only refreshes when
  a status line renders, so its AGE is checked on EVERY poll.

  HOW THE RULE FAILED ON 2026-09-25, AND WHY STALE IS ONE POLL. The lead's own
  one-off watcher read five_hour stuck at 56% from 00:43Z to 01:08Z, at 71%
  from 01:13Z to 01:33Z and at 89% from 01:53Z to 02:13Z, then 100% at 02:18Z.
  The lead was idle waiting on teammates, its status line did not render, the
  payload did not refresh, and the 93% crossing was never seen. The threshold
  was right; the reading was old. So a reading older than ONE poll interval is
  UNKNOWN — never the current value — and the watcher wakes the lead with
  QUOTA-STALE, because the lead's own reply is what re-renders the status line
  (the host runs it when "a new assistant message arrives"). The replay of
  that log is quota-watch.test.sh case R01.

  WHAT ELSE REFRESHES IT, checked 2026-09-25: only ~/.claude/statusline.sh
  writes the file (grep over ~/.claude's scripts and settings, and this engine,
  which only reads it). The host runs that script at session start, on a new
  assistant message, after /compact, on a permission or vim mode change, when a
  rate-limit window's resets_at passes, and on an optional statusLine
  `refreshInterval` timer (code.claude.com/docs/en/statusline, "When it
  updates"; not set on this machine). Teammate activity is not on that list.
  A refreshInterval would re-render while the lead is idle, but whether the
  host's rate_limits figure moves with teammates' API responses is NOT
  verified, and a re-render that rewrites an old figure would make the file's
  age a false freshness signal. So nothing here depends on it.

  Within one window usage only grows, so a STALE reading at or above the
  threshold is still at or above it: it is reported as "at least", and the
  lead is woken to pause. A stale reading BELOW the threshold proves nothing
  and is UNKNOWN. A reading whose window has already ended describes a window
  that no longer exists and is UNKNOWN.

THE THRESHOLD is data, declared once: QUOTA_PAUSE_PERCENT in the entity's
orchestration.config, beside MODEL_CEILING. quota-watch.sh resolves the entity
and hands the raw value in; an undeclared or malformed value is UNKNOWN, never
a default, because a second place holding 93 is how the number drifts.

THE POLLING NEVER STOPS (2026-09-28). Until today --watch EXITED to wake the
lead, because a background command's exit is what reaches an idle lead. It did
so at the 14:30Z reset and on a get_usage timeout at 15:32Z, and each time
nothing polled until the lead noticed and started it again by hand. Waking the
lead was right; ending the polling to do it was the defect. So the two are now
separate processes:

  THE POLLER polls every 300 s for as long as its session lives and never
  stops on an event. It writes each poll line to polls.log and each wake-up,
  once, to events.jsonl, in ~/.claude/state/quota-watch/<session>/
  (QUOTA_WATCH_STATE_DIR overrides, for tests). One per session: it holds
  poller.lock for its whole life, and it ends when the session's own claude
  process has gone (its pid and start time, recorded at spawn).

  THE DOORBELL is how a wake-up reaches the lead:
    --monitor  the engine's plugin monitor (monitors/monitors.json), which
               Claude Code starts with EVERY interactive session and stops
               when the session ends. Each line a monitor prints reaches the
               lead as a notification without the monitor ending, so it prints
               every wake-up and keeps going. It starts the poller if none is
               running, and starts it again if it ever dies.
    --watch    the fallback, for a session where plugin monitors do not run:
               a background command that exits on the next wake-up, as before.
               The poller keeps polling between one --watch and the next, and
               a wake-up that comes while none is running waits in the journal
               for the next --watch to print it.
  With no session process to tie a poller to (a plain terminal), --watch polls
  in the foreground and prints everything, never stopping.
    --monitor --host-json  the RichOS app host's own child (richos-core
               team_watch.rs), from the app's launch to its quit: his team's
               lead there runs with --print, where plugin monitors never
               start. It polls in the foreground while its parent is the host,
               and prints each wake-up as one JSON line, which the host sends
               to that lead (host_child.py).

  A get_usage failure (timeout, refusal) is retried every 60 s instead of
  every 300 s, and it wakes nobody by itself. Once get_usage has answered in
  this run, an outage wakes the lead only when the threshold could have been
  crossed unseen: when the freshest reading plus the fastest rise this watcher
  has measured over the time since it was taken reaches the threshold, or when
  the freshest reading is two polls old, whichever comes first (the rule's own
  resolution is one poll; two polls means a check he ordered has been missed,
  and the measured rise is not a guarantee). See outage_verdict().

HIGH SPEED (ruling §108, 2026-10-04, richos-hq 1d027daa and addendum 56f17efd).
His words: "The 93% rule is sufficient for regular work. But when the token
consumption velocity increases a lot, we need to start checking the remaining
quota every 2 minutes (instead of 5) and then also re-calculate the estimate
for when the workers must be paused etc." And: "Rich must give the user an
alert when such a crazy high token consumption velocity is detected".

  WHAT HAPPENED, 2026-09-29, the lead's transcript (session 30e3850f; the
  watcher's own polls.log for it was pruned after two days): 7% at 18:29:11Z,
  15 Fable hunters started about 18:31Z, 91% at 18:50:15Z, the watcher's poll
  read 98% at 18:53:10Z (its first wake, so its 18:48:10Z poll read under 93),
  the lead sent the pause to all 15 by 18:53:54Z (44 s), and 100% at 18:54:16Z.
  About 20 points per 5 minutes: one 5-minute check covered the whole 7-point
  margin above 93%.

  THE SPEED (recent_speed): the rise between this window's get_usage readings
  over the latest stretch of at least one 5-minute poll, or the rise since the
  previous reading when that is faster and at least one fast poll back, so a
  measured JUMP moves the pause point at once (his third addendum). Never over
  less: a reading moves in whole points.

  "A LOT" (fast_speed): the speed at which his rule at a 5-minute check stops
  being enough, the points from the threshold to 100% used up within one check
  plus the hold (HOLD_SECONDS): (100 - 93) / (300 + 60) s, 5.8 points per 5
  minutes. Checked against every logged pair of consecutive get_usage polls in
  the lead transcripts of 2026-09-26 to 2026-09-28 (sessions c11805a4 and
  356e9552, 69 pairs): normal work peaks at 3.8 points per 5 minutes
  (2026-09-26 07:04:46Z 6% to 07:10:01Z 10%), 4.8 with one point of reading
  resolution, and 67 of the 69 pairs are at or under 1. The 15-worker run was
  about 20.

  AT HIGH SPEED: QUOTA-FAST wakes the lead once per window, the first time it
  is measured, with the speed and the time the window would be used up, so the
  CEO can be told. Polls come every FAST_POLL_SECONDS (60: his "every 2
  minutes ... Use every minute instead if that causes no problems", and the
  measured cost of a read, at the constant). And the pause message
  (QUOTA-THRESHOLD, marked EARLY) comes as soon as the reading plus the speed
  over the next check and the hold reaches 100% (early_wake), below 93% if it
  must. The 20-minute rule near the reset still applies.

  A RISE EXPECTED: when EXPECT_SPAWNS of the session's working teammates were
  registered in the last EXPECT_SECONDS (the workspace registry's
  registered_at), polls come every FAST_POLL_SECONDS before anything is
  measured. No alert: nothing has been detected yet.

  BACK TO NORMAL (his fourth addendum): the speed is measured afresh at every
  poll, so once it is under the high-speed line and no rise is expected, polls
  return to 300 s and the pause point to the threshold. The speed, the last
  reading and the recent teammate count are in poller.json; --alive prints
  them.

EXIT CODES
  --once    0 below the threshold, 1 at or above it (pause), 2 unknown,
            3 at or above it with the reset less than 20 minutes away (no
            pause; a hold in place releases)
  --status  the same codes as --once
  --watch   0 after printing the next wake-up (QUOTA-THRESHOLD, QUOTA-RELEASE,
            WINDOW-RESET, QUOTA-REFRESH, QUOTA-STALE, QUOTA-UNKNOWN or a weekly
            one), or at once when the session's monitor already delivers them;
            2 when it cannot watch at all. The poller does not stop with it.
  --monitor runs for the whole session; 2 when it cannot watch at all
  --alive   0 a poller is polling and something delivers its wake-ups, 1 a
            poller is polling but nothing delivers them, 2 no poller
  --notice  always 0 (it is a SessionStart hook's body)
"""

import argparse
import contextlib
import datetime as _dt
import fcntl
import importlib.util
import io
import json
import math
import os
import selectors
import shutil
import signal
import subprocess
import sys
import time

# Shared with the terminal and desktop SendMessage delivery guards.
import pause_protocol
import quota_weekly
import host_child  # the RichOS app host's child (--monitor --host-json)

# --monitor --host-json: this poller is the app host's own child, for the lead the host names.
HOST = {"on": False}

HIS_WORDS = ('"quota polling: every 5 minutes from now. And once it crosses the 93% '
             'threshold: PAUSE subagents. Then resume after quota rest."')
RULING = "ruling §87, richos-hq/wiki/ceo-decisions.md"
# His 2026-09-25 update, as the rule now stands. Not a quotation: the record
# of it is §87 and docs/plans/claude-five-hour-quota-rule-2026-09-25.md.
RULE_UPDATE = ("Updated 2026-09-25: at or above the threshold, do not pause when the reset is less than "
               "20 minutes away (exactly 20 minutes still pauses), and a hold already in place is released "
               "inside that window, keeping the same agent. Polling stays every 5 minutes at every usage level.")
# Ruling §108 and its addendum (2026-10-04). Not a quotation either.
FAST_RULE = ("Updated 2026-10-04 (ruling §108): when usage speeds up a lot, the watcher alerts the lead at once "
             "(QUOTA-FAST, for the CEO), checks every minute, and wakes the lead with the pause message early "
             "enough that the hold lands before 100%, re-estimated at once from a measured jump. When many "
             "teammates have just started, it checks every minute before any rise is measured. At normal speed "
             "nothing changes: every 5 minutes, and the threshold.")

# His "every 5 minutes". The environment override exists for the test suite
# only, which cannot wait five minutes per case.
POLL_SECONDS = 300
# A reading older than ONE poll interval is stale (see "HOW THE RULE FAILED"
# above): the stale bound IS the poll interval, set in main(). The test suite
# may override it separately, only so that its accelerated clock is not at the
# mercy of a one-second scheduler.

# His 2026-09-25 update: at or above the threshold, no pause when the reset is
# LESS than this far away; exactly this far still pauses. The same seconds
# release a hold already in place. The desktop app's RESET_EXEMPTION_MS.
RESET_EXEMPTION_SECONDS = 20 * 60

# RULING §108 (2026-10-04; the module docstring, "HIGH SPEED", says why). When
# usage speeds up a lot, or a big rise is expected: a check every minute
# instead of every 5 (his words: "every 2 minutes ... Use every minute instead
# if that causes no problems"). Measured 2026-10-04 on this 10-core Mac: one
# get_usage read is 3.1 s, about 10 CPU-seconds and 395 MB for those seconds,
# so a read a minute is about 0.17 of a core while it lasts, and no quota.
FAST_POLL_SECONDS = 60
# A big rise is EXPECTED when this many of the session's working teammates were
# registered within EXPECT_SECONDS. From the workspace registry, 2026-10-04:
# 18 of 20 sessions (992 registrations) never registered more than 7 in any 5
# minutes; the 15 Fable workers of 2026-09-29 were 15, and 2026-09-30 16:37Z
# (session 212ab083) 13.
EXPECT_SPAWNS = 10
EXPECT_SECONDS = 300
# ... and the lead is woken early enough that the hold lands before 100%. How
# long a hold takes to land once the lead is woken, measured on 2026-09-29: the
# wake at 18:53:10Z to the last of 15 pause messages at 18:53:54Z is 44 s,
# rounded up to a minute.
HOLD_SECONDS = 60


def speed_settings(a):
    """Fill in the §108 check interval and hold time from a.poll where the
    caller did not set them, and return a. Both scale down with an accelerated
    poll (seconds long, in the suites), so fixtures' fast rises keep their
    meaning; production is 60 and 60. main() and poll_loop() both call it, so a
    caller that builds its own arguments (the weekly suite, the replay) never
    meets a missing one; values a caller set are kept."""
    if getattr(a, "fast_poll", None) is None:
        a.fast_poll = min(a.poll, FAST_POLL_SECONDS)
    if getattr(a, "hold", None) is None:
        a.hold = min(HOLD_SECONDS, a.poll // 5)
    return a

# WAKE THE LEAD BEFORE THE READING TURNS ONE POLL OLD (2026-09-25, measured).
# The lead read 91% at 07:57Z and 95% at 08:07Z: the pause fired at 95, not 93.
# A reading is current up to one poll (300 s) old, so a loop that polls every
# 300 s first saw it as stale at its SECOND poll, about 600 s after the render.
# With the lead idle, only the lead's reply re-renders the payload, so the
# effective refresh was every 10 minutes against his "Keep it consistent at 5
# minutes". Below the threshold, with a worker running, the watcher wakes the
# lead (QUOTA-REFRESH) this many seconds BEFORE the reading turns one poll old,
# and schedules its poll for that moment. A tenth of the bound when the bound
# is shorter (the suite's accelerated clock).
REFRESH_AHEAD_SECONDS = 30

# get_usage: one control request's bound (the desktop's DEADLINE) and the
# largest frame read (its MAX_FRAME). The environment override is for the test
# suite's hanging fixture only.
GET_USAGE_SECONDS = 20
MAX_FRAME = 256 * 1024
CONTROL_ARGS = ["--print", "--input-format=stream-json", "--output-format=stream-json", "--verbose",
                "--setting-sources", "", "--no-session-persistence", "--tools", "",
                "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}']
# Empty and unwritable: Claude Code lists every file under its working directory
# at start, so this is the only place it looks (the module docstring says why).
CONTROL_CWD = "/var/empty"

PAUSE_UNTIL_PREFIX = "the five-hour quota reset"
CONFIG_KEY = "QUOTA_PAUSE_PERCENT"

# Captured at import, so a test that swaps this module's `time` for a
# simulated clock (quota-watch-replay.py) still times real requests.
_monotonic = time.monotonic

# THE POLLING NEVER STOPS (2026-09-28; the module docstring says why).
# A failed get_usage is tried again after this long instead of a whole poll:
# "retried at the next poll, or sooner".
RETRY_SECONDS = 60
# An outage wakes the lead at the latest when the freshest reading is this
# many polls old: one poll is the resolution of his rule, so two polls means a
# check he ordered has been missed, whatever the measured rise says.
BLIND_POLLS = 2
FIVE_HOUR_SECONDS = 5 * 3600
STATE_KEEP_SECONDS = 2 * 86400
POLLS_LOG_MAX = 1024 * 1024
# The kinds a --until-reset run still wakes for.
UNTIL_RESET_KINDS = ("RELEASE", "RESET", "WEEKLY-THRESHOLD", "WEEKLY-RELEASE", "CANNOT-WATCH", "FAST")


def _last_line(fh):
    """The last non-empty line of a small stderr capture, at most 160 characters."""
    try:
        fh.seek(0, os.SEEK_END)
        size = fh.tell()
        fh.seek(max(0, size - 4096))
        text = fh.read().decode("utf-8", "replace")
    except (OSError, AttributeError, ValueError):
        return ""
    lines = [ln.strip() for ln in text.splitlines() if ln.strip()]
    return "".join(c for c in (lines[-1] if lines else "") if c.isprintable())[:160]


def _env_int(name, default):
    v = (os.environ.get(name) or "").strip()
    if v.isdigit() and int(v) > 0:
        return int(v)
    return default


def default_payload_path():
    return (os.environ.get("QUOTA_PAYLOAD") or "").strip() or os.path.join(
        os.path.expanduser("~"), ".claude", "statusline-payload.json")


def hhmm(epoch):
    return _dt.datetime.fromtimestamp(epoch, _dt.timezone.utc).strftime("%H:%MZ")


def span(seconds):
    s = max(0, int(seconds))
    if s < 90:
        return "%d s" % s
    m = (s + 30) // 60
    if m < 120:
        return "%d min" % m
    return "%d h %02d min" % (m // 60, m % 60)


# ---------------------------------------------------------------------------
# the threshold
# ---------------------------------------------------------------------------

def parse_threshold(raw):
    """(value, problem). The raw string exactly as the config declares it."""
    raw = (raw or "").strip()
    if not raw:
        return None, "not declared"
    try:
        v = float(raw)
    except ValueError:
        return None, "%s=%r is not a number" % (CONFIG_KEY, raw)
    if not (0 < v <= 100):
        return None, "%s=%r is outside 1..100" % (CONFIG_KEY, raw)
    return v, ""


def fmt_pct(v):
    return ("%d" % v) if float(v).is_integer() else ("%g" % v)


# ---------------------------------------------------------------------------
# the reading
# ---------------------------------------------------------------------------

def read_reading(path, now):
    """One read of the payload. Never raises.

    The status line writes the file with `cp`, which is not atomic, so a read
    can land mid-write and see truncated JSON. One retry after a short pause
    separates that race from a file that is really malformed."""
    r = {"path": path, "state": "", "why": "", "used": None, "resets_at": None,
         "age": None, "ended": False}
    data = None
    for attempt in (0, 1):
        try:
            st = os.stat(path)
        except FileNotFoundError:
            r["state"], r["why"] = "missing", "no payload at %s (the status line has never written one)" % path
            return r
        except OSError as e:
            r["state"], r["why"] = "unreadable", "cannot read %s: %s" % (path, e)
            return r
        r["age"] = max(0.0, now - st.st_mtime)
        try:
            with open(path, "rb") as fh:
                data = json.loads(fh.read().decode("utf-8"))
            break
        except (ValueError, UnicodeDecodeError) as e:
            if attempt == 0:
                time.sleep(0.25)
                continue
            r["state"], r["why"] = "malformed", "%s is not valid JSON (%s)" % (path, e.__class__.__name__)
            return r
        except OSError as e:
            r["state"], r["why"] = "unreadable", "cannot read %s: %s" % (path, e)
            return r
    fh5 = None
    if isinstance(data, dict):
        rl = data.get("rate_limits")
        if isinstance(rl, dict):
            r["windows"] = quota_weekly.windows(rl, _parse_reset, fallback=True)
            fh5 = rl.get("five_hour")
    if not isinstance(fh5, dict):
        r["state"], r["why"] = "malformed", "%s carries no rate_limits.five_hour" % path
        return r
    used, resets = fh5.get("used_percentage"), fh5.get("resets_at")
    if isinstance(used, bool) or not isinstance(used, (int, float)) or used < 0:
        r["state"], r["why"] = "malformed", "rate_limits.five_hour.used_percentage is %r" % (used,)
        return r
    if isinstance(resets, bool) or not isinstance(resets, (int, float)) or resets <= 0:
        r["state"], r["why"] = "malformed", "rate_limits.five_hour.resets_at is %r" % (resets,)
        return r
    r["state"], r["used"], r["resets_at"] = "ok", used, int(resets)
    r["ended"] = int(resets) <= now
    return r


def claude_binary():
    """QUOTA_CLAUDE_BIN (tests point it at a fixture or at nothing), else
    `claude` on PATH."""
    return (os.environ.get("QUOTA_CLAUDE_BIN") or "").strip() or shutil.which("claude") or ""


def _parse_reset(v):
    if isinstance(v, bool):
        return None
    if isinstance(v, (int, float)) and v > 0:
        return int(v)
    if isinstance(v, str):
        try:
            d = _dt.datetime.fromisoformat(v.strip().replace("Z", "+00:00"))
        except ValueError:
            return None
        if d.tzinfo is None:
            return None
        return int(d.timestamp())
    return None


def read_get_usage(now, deadline_s=None):
    """One reading from Claude Code's get_usage control request. Never raises.
    The same shape as read_reading(), with source 'get_usage' and age 0."""
    r = {"path": "get_usage", "source": "get_usage", "state": "", "why": "", "used": None,
         "resets_at": None, "age": 0.0, "ended": False}
    b = claude_binary()
    if not b:
        r["state"], r["why"] = "failed", "no `claude` on PATH"
        return r
    deadline_s = deadline_s or _env_int("QUOTA_WATCH_GET_USAGE_SECONDS", GET_USAGE_SECONDS)
    env = {k: v for k, v in os.environ.items() if k != "CLAUDECODE"}
    # WHY A READ FAILED IS KEPT (2026-09-28). At 15:32Z get_usage gave no answer
    # within 20 s and nothing recorded why: stderr went to /dev/null and no
    # step was timed. cpu-guard's own log was the only witness (host 100% busy
    # at 15:32:39Z, 99.6% at 15:27:38Z). So each request is timed, and the last
    # line claude wrote on stderr goes into the reason. The file is unlinked at
    # creation (TemporaryFile), so a crash leaves nothing behind.
    import tempfile
    try:
        errf = tempfile.TemporaryFile()
    except OSError:
        errf = subprocess.DEVNULL
    started = _monotonic()
    try:
        proc = subprocess.Popen([b] + CONTROL_ARGS, cwd=CONTROL_CWD, env=env, stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=errf, start_new_session=True)
    except OSError as e:
        r["state"], r["why"] = "failed", "cannot start %s (%s)" % (b, e.__class__.__name__)
        return r
    buf = [b""]
    sel = selectors.DefaultSelector()
    timing = []

    def request(rid, subtype):
        proc.stdin.write((json.dumps({"type": "control_request", "request_id": rid,
                                      "request": {"subtype": subtype, "hooks": {}}}) + "\n").encode())
        proc.stdin.flush()
        until = time.time() + deadline_s
        while True:
            while b"\n" in buf[0]:
                line, buf[0] = buf[0].split(b"\n", 1)
                v = json.loads(line.decode("utf-8"))
                if (isinstance(v, dict) and v.get("type") == "control_response"
                        and isinstance(v.get("response"), dict) and v["response"].get("request_id") == rid):
                    return v["response"]
            if len(buf[0]) > MAX_FRAME:
                raise ValueError("a frame over %d bytes" % MAX_FRAME)
            left = until - time.time()
            if left <= 0 or not sel.select(timeout=left):
                raise TimeoutError("no answer to %s within %d s" % (subtype, deadline_s))
            chunk = os.read(proc.stdout.fileno(), 65536)
            if not chunk:
                raise EOFError("claude closed its output before answering %s" % subtype)
            buf[0] += chunk

    def step(rid, subtype):
        t0 = _monotonic()
        try:
            return request(rid, subtype)
        finally:
            timing.append("%s %.1f s" % (subtype, _monotonic() - t0))

    try:
        sel.register(proc.stdout, selectors.EVENT_READ)
        init = step("quota-1", "initialize")
        if init.get("subtype") != "success":
            raise ValueError("initialize was refused")
        usage = step("quota-2", "get_usage")
    except (OSError, ValueError, TimeoutError, EOFError, UnicodeDecodeError) as e:
        r["state"], r["why"] = "failed", "get_usage failed: %s" % (str(e) or e.__class__.__name__)
        r["why"] += " (%s)" % "; ".join(timing + ["claude's last stderr line: %s" % (_last_line(errf) or "none")])
        return r
    finally:
        # The process group is this call's own (start_new_session, pid captured
        # at spawn): end it on every read, whatever happened.
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(proc.pid, sig)
            except (ProcessLookupError, PermissionError):
                break
            try:
                proc.wait(timeout=5)
                break
            except subprocess.TimeoutExpired:
                continue
        sel.close()
        for fh in (proc.stdin, proc.stdout):
            try:
                fh.close()
            except OSError:
                pass
        r["timing"] = ", ".join(timing)
        r["seconds"] = round(_monotonic() - started, 1)
        if errf is not subprocess.DEVNULL:
            try:
                errf.close()
            except OSError:
                pass
    if usage.get("subtype") != "success":
        r["state"], r["why"] = "failed", "get_usage was refused"
        err = usage.get("error")
        if isinstance(err, str) and any(t in err for t in DEAD_LOGIN):
            # The saved login is dead (2026-09-26): the CEO must run /login.
            # Only Claude Code's own first line is kept, never anything else.
            r["auth_failed"] = err.strip().splitlines()[0][:160]
            r["why"] = "get_usage was refused: %s" % r["auth_failed"]
        return r
    body = usage.get("response") if isinstance(usage.get("response"), dict) else {}
    if body.get("rate_limits_available") is False:
        r["state"], r["why"] = "failed", "Claude Code did not report subscription limits for this account"
        return r
    r["windows"] = quota_weekly.windows(body.get("rate_limits", {}), _parse_reset)
    five = (body.get("rate_limits") or {}).get("five_hour") if isinstance(body.get("rate_limits"), dict) else None
    used = five.get("utilization") if isinstance(five, dict) else None
    resets = _parse_reset(five.get("resets_at")) if isinstance(five, dict) else None
    if (body.get("rate_limits_available") is not True or isinstance(used, bool)
            or not isinstance(used, (int, float)) or not (0 <= used <= 100) or resets is None):
        r["state"], r["why"] = "failed", "get_usage answered in a shape this watcher cannot read"
        return r
    r["state"], r["used"], r["resets_at"] = "ok", used, resets
    r["ended"] = resets <= now
    return r


# Texts that mean the saved Claude login is dead, the same three login_alarm.py
# acts on (Claude Code 2.1.283's own strings).
DEAD_LOGIN = ("Login expired", "OAuth token revoked", "OAuth session expired")


def report_login_failure(engine_root, text):
    """Hand a dead-login refusal to login-alarm.sh, which tells the CEO once per
    expiry. Best effort and bounded: the quota reading goes on either way."""
    script = os.path.join(engine_root or "", "scripts", "login-alarm.sh")
    if not os.path.isfile(script):
        return False
    try:
        r = subprocess.run(["bash", script, "--report", "--source", "quota-watch",
                            "--detail", text], capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        return False
    return r.returncode == 0


def app_state():
    """The RichOS app's state folder when this runs for the app (RICHOS_APP_STATE), else ""."""
    return (os.environ.get("RICHOS_APP_STATE") or "").strip()


def app_account(data):
    """{"id", "label"}: the account the published file's top level describes. `inUse`; for an app
    older than the field, its accounts row in use; with no list, the one account, Account 1, by its
    name when it has one. A list with no row in use names nobody (id None): never guessed as
    Account 1, which named Home beside Work's figures in the live file of 2026-10-28 17:14Z."""
    who = data.get("inUse") if isinstance(data.get("inUse"), dict) else None
    listed = [x for x in data.get("accounts") or [] if isinstance(x, dict)]
    if who is None:
        who = next((x for x in listed if x.get("inUse")), None)
    if who is None and not listed:
        who = {"id": "1", "label": data.get("firstLabel") or "Account 1"}
    label = "".join(c for c in str((who or {}).get("label") or "not named")[:80] if c.isprintable())
    return {"id": str(who["id"]) if who and who.get("id") else None, "label": label}


def _ms(value):
    """A published moment (milliseconds) as an int, or None."""
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
        return None
    return int(value)


def gate_admission(state, now, account=None):
    """THE APP'S OWN HOLD, AS ITS GATE DECIDES IT (richos-hq follow-ups.md, "app-launched reviews
    never apply the usage hold"). Never raises. {"held", "until" (epoch s or None), "account",
    "line"}.

    In the app the hold is the desktop gate's (quota/gate.rs `decide`), read from the file the
    quota service publishes (<state>/claude-quota.json) and the pause setting beside it. `account`
    is the id of the account the work will run on: the gate judges a call on that account's own
    row of `accounts`, and on the top level (the account the work runs on now, `inUse`) only when
    there is no row for it, one account or no id. During a move the two differ (main's `moving`:
    the top level is still the account being left, while a new session, a review's included,
    starts on the account in use). It needs no terminal entity config: the app's watcher starts with
    a cleared environment (review_watch.rs `environment`), so quota-watch's QUOTA_PAUSE_PERCENT was
    never found there and every app review answered UNKNOWN and started during a hold.

    The gate's own order, rule for rule:
      1. heldUntil in the future: every account is exhausted; held until then.
      2. the pause setting (claude-quota-policy.json beside engine-state, else the published one)
         off: admitted.
      3. a setting out of 1..99, or a check dated in the future: Unknown, admitted.
      4. the published admission. Only `held` holds (quota.rs `Admission::allows_work`), and only
         until its resetsAt: with the figures unchanged, time can only end a hold at its window's
         reset (quota.rs `view_with`), and a new reading is republished by the service.
    No file, or one that cannot be read, is the gate's Unknown: admitted, and said so.

    One case is not decided here as the gate decides it: a pause setting newer than the published
    reading (the publication after a settings change failed). The gate recomputes the figures under
    the new setting; this reports UNKNOWN and admits until the service publishes again."""
    path = os.path.join(state, "claude-quota.json")
    now_ms = int(now * 1000)
    out = {"held": False, "until": None, "account": None, "line": ""}

    def admitted(why):
        out["line"] = "quota: the app's gate admits work: %s" % why
        return out

    try:
        with open(path, "rb") as fh:
            raw = fh.read(262145)
        if len(raw) > 262144:
            raise ValueError("oversized")
        data = json.loads(raw.decode("utf-8"))
    except FileNotFoundError:
        return admitted("the app has published no usage reading at %s (the gate's Unknown)" % path)
    except (OSError, ValueError, UnicodeDecodeError) as e:
        return admitted("its usage reading at %s cannot be read (%s; the gate's Unknown)" % (path, e.__class__.__name__))
    if not isinstance(data, dict):
        return admitted("%s is not the app's usage reading (the gate's Unknown)" % path)
    row = next((x for x in data.get("accounts") or [] if isinstance(x, dict) and account is not None
                and x.get("id") == str(account)), None)
    src = row if row is not None else data
    if row is not None:
        label = "".join(c for c in str(row.get("label") or "not named")[:80] if c.isprintable())
        who = out["account"] = {"id": str(row["id"]), "label": label}
    else:
        who = out["account"] = app_account(data)
    named = "%s (account %s)" % (who["label"], who["id"]) if who["id"] else "the account in use (%s)" % who["label"]
    five = next((w for w in src.get("windows") or [] if isinstance(w, dict) and w.get("id") == "five_hour"), None)
    figure = "; five-hour %s%%" % fmt_pct(five["usedPercent"]) if five and isinstance(
        five.get("usedPercent"), (int, float)) and not isinstance(five.get("usedPercent"), bool) else ""

    until = _ms(data.get("heldUntil"))
    if until is not None and until > now_ms:
        out.update(held=True, until=until // 1000)
        out["line"] = ("quota: the app's gate HOLDS work until %s: every account is exhausted (in use: %s)"
                       % (hhmm(until // 1000), named))
        return out
    policy = _policy(data.get("policy"))
    if policy is None:
        return admitted("%s does not parse as the app's reading: no pause setting (the gate's Unknown)" % path)
    try:
        with open(os.path.join(os.path.dirname(os.path.abspath(state)), "claude-quota-policy.json"), "rb") as fh:
            policy = _policy(json.loads(fh.read(262145).decode("utf-8"))) or policy
    except (OSError, ValueError, UnicodeDecodeError):
        pass
    enabled, percent = policy
    if not enabled:
        return admitted("its automatic pause is off (%s)" % named)
    if not 1 <= percent <= 99:
        return admitted("UNKNOWN: its pause setting %d%% is not 1-99%% (the gate's Unknown; %s)" % (percent, named))
    checked = _ms(src.get("checkedAt"))
    if checked is not None and checked > now_ms:
        return admitted("UNKNOWN: its reading is dated in the future (the gate's Unknown; %s)" % named)
    decided = _gate_recompute(data, src, percent, now_ms)
    if decided is None:
        return admitted("%s does not parse as the app's reading (the gate's Unknown)" % path)
    kind, resets, why = decided
    if kind == "held":
        out.update(held=True, until=resets // 1000)
        out["line"] = "quota: the app's gate HOLDS work on %s until %s (%s)" % (named, hhmm(resets // 1000), why)
        return out
    return admitted("%s, %s%s" % (named, why, figure))


def _policy(value):
    """(enabled, pausePercent) as the gate's serde reads a Policy, or None when it would not parse."""
    if not isinstance(value, dict):
        return None
    enabled, percent = value.get("enabled"), value.get("pausePercent")
    if not isinstance(enabled, bool) or isinstance(percent, bool) or not isinstance(percent, int) or not 0 <= percent <= 255:
        return None
    return enabled, percent


# quota.rs constants the gate's decision uses.
REFRESH_INTERVAL_MS = 5 * 60_000
FAST_REFRESH_INTERVAL_MS = 60_000
FAST_POINTS_PER_CHECK = 5.0
HANDOFF_DEFAULT_MS = 5 * 60_000
WEEKLY_THRESHOLD = 99.0
RESET_EXEMPTION_MS = 20 * 60_000


def _gate_recompute(data, src, percent, now_ms):
    """("held" | another admission, resets_at ms or None, why), or None when the file would not parse.

    THE GATE'S OWN RECOMPUTE, rule for rule (second review of 2097df51c: the published admission is
    the service's, computed with the account's measured handoff, token speed and expected rise; the
    gate builds a fresh Snapshot from the published windows, checkedAt, message, speeds, preHold and
    emptyAt alone and decides again, quota/gate.rs `decide` -> quota.rs `Snapshot::view_with`). So:
    no token speed, no since-reading points, no expected rise, no cut-off, and the 5-minute default
    handoff with no points. `src` is what the gate judges the call on: the call's own account's row
    of `accounts` with two or more accounts, else the top level (`data`, the account the figures
    describe). quota::tests::the_review_hold_is_the_gates_own_decision runs this against the gate's
    `decide` over the same files, so the two cannot drift apart unseen.

    Two rules came with the staggered release (main, Rich 2026-10-29): the five-hour point is set by
    the larger of the speed measured now and the speed before the hold (`preHold`), and the last 20
    minutes before the five-hour reset no longer admit: the window still HOLDS, and the gate lets
    held tool calls out one at a time (`stagger::admit`, a per-call permit). A review takes no such
    permit, so it waits for the reset, as the gate's admission says."""
    if not all(k in data for k in ("state", "windows", "refreshIntervalMs", "admission")):
        return None
    windows = []
    for w in src.get("windows") if isinstance(src.get("windows"), list) else [None]:
        if not isinstance(w, dict) or not isinstance(w.get("id"), str) or not isinstance(w.get("label"), str):
            return None
        used, reset, duration = w.get("usedPercent"), w.get("resetsAt"), w.get("durationMs")
        if isinstance(used, bool) or not isinstance(used, (int, float)):
            return None
        if reset is not None and (isinstance(reset, bool) or not isinstance(reset, int) or reset < 0):
            return None
        if isinstance(duration, bool) or not isinstance(duration, int) or duration < 0:
            return None
        windows.append({"id": w["id"], "used": float(used), "resets": reset})
    number = lambda v: not isinstance(v, bool) and isinstance(v, (int, float))
    speeds, pre_hold = src.get("speeds") or {}, src.get("preHold") or {}
    for table in (speeds, pre_hold):
        if not isinstance(table, dict) or not all(number(s) for s in table.values()):
            return None
    speed = lambda w: float(speeds.get(w["id"], 0.0))
    checked = src.get("checkedAt")
    empty_at = src.get("emptyAt")
    error = src.get("message") is not None
    # Reading::interval with `expected` false: one minute while any speed is fast, else five.
    is_fast = lambda table: any(float(s) * REFRESH_INTERVAL_MS >= FAST_POINTS_PER_CHECK for s in table.values())
    interval = FAST_REFRESH_INTERVAL_MS if is_fast(speeds) else REFRESH_INTERVAL_MS
    clamp = lambda v: min(max(v, 0.0), 100.0)
    def weekly_point(w):
        s = speed(w)
        points = s * interval + max(s * HANDOFF_DEFAULT_MS, 0.0)
        return clamp(WEEKLY_THRESHOLD - max(math.ceil(points) - 1.0, 0.0))
    # Snapshot::pre_hold over the gate's one-entry rise log (0% -> the published speed): 0% is under
    # every valid line, so it is the published five-hour speed, counted only above 0 (`view_with`).
    before = float(pre_hold.get("five_hour", 0.0))
    judged = dict(speeds)
    if before > 0.0:
        judged["five_hour"] = max(float(judged.get("five_hour", 0.0)), before)
    judged_interval = FAST_REFRESH_INTERVAL_MS if is_fast(judged) else REFRESH_INTERVAL_MS
    act_point = lambda w: clamp(min(float(percent), 100.0 - float(judged.get(w["id"], 0.0)) * judged_interval))
    live = [w for w in windows if w["resets"] is None or w["resets"] > now_ms]
    expired = any(w["resets"] is not None and w["resets"] <= now_ms for w in windows)
    fresh = checked is not None and now_ms >= checked and now_ms - checked < interval and not error and not expired
    answered_empty = empty_at is not None and (checked is None or empty_at >= checked)
    if checked is None:
        return ("noReading" if answered_empty else "unknown"), None, "no reading yet"
    weekly = [w for w in live if w["id"] == "seven_day" and w["used"] >= weekly_point(w)
              and w["resets"] is not None and w["resets"] > now_ms]
    if weekly:
        w = max(weekly, key=lambda x: x["resets"])
        return "held", w["resets"], "weekly %s%% at its handoff point %s%%" % (fmt_pct(w["used"]), fmt_pct(weekly_point(w)))
    five = next((w for w in windows if w["id"] == "five_hour"), None)
    if five is not None and five["used"] >= act_point(five) and five["resets"] is not None and five["resets"] > now_ms:
        why = "five-hour %s%% at its %d%% pause point %s%%" % (fmt_pct(five["used"]), percent, fmt_pct(act_point(five)))
        if five["resets"] - now_ms < RESET_EXEMPTION_MS:
            why += "; its reset is under 20 minutes away, where held work is let out one call at a time and a review waits for the reset"
        return "held", five["resets"], why
    if five is not None and fresh and five["resets"] is not None and five["resets"] > now_ms:
        return "ready", None, "ready"
    return ("noReading" if answered_empty else "unknown"), None, "no current reading (the gate admits work without one)"


def read_app(state, now):
    """THE ACCOUNT IN USE, FROM THE APP ITSELF (richos-hq follow-ups.md, 7fed0bef). Never raises.

    Inside the app, a `claude` probe or ~/.claude/statusline-payload.json reads whichever account
    this process's environment happens to name, not the one the app's work runs on: on 2026-10-28
    the app ran on Work (10% of its week) while a lead deferred work on Home's 99. The app publishes
    the account in use at the top level of <state>/claude-quota.json (quota.rs `view_at`), so in
    app context that file is the only source. Missing, unreadable or old, it is reported missing,
    malformed or stale through the same age rules as the status-line file, never replaced by a
    probe. The same shape as read_reading()."""
    path = os.path.join(state, "claude-quota.json")
    r = {"path": path, "state": "", "why": "", "used": None, "resets_at": None,
         "age": None, "ended": False, "source": "the app"}
    try:
        with open(path, "rb") as fh:
            data = json.loads(fh.read(262145).decode("utf-8"))
    except FileNotFoundError:
        r["state"], r["why"] = "missing", "the app has published no usage reading at %s" % path
        return r
    except OSError as e:
        r["state"], r["why"] = "unreadable", "cannot read %s: %s" % (path, e)
        return r
    except (ValueError, UnicodeDecodeError) as e:
        r["state"], r["why"] = "malformed", "%s is not valid JSON (%s)" % (path, e.__class__.__name__)
        return r
    if not isinstance(data, dict):
        r["state"], r["why"] = "malformed", "%s is not the app's usage reading" % path
        return r
    r["account"] = app_account(data)
    label = r["account"]["label"]
    r["source"] = "the app (account %s)" % label
    windows = []
    for w in data.get("windows") or []:
        if not isinstance(w, dict) or not isinstance(w.get("id"), str):
            continue
        used, reset = w.get("usedPercent"), w.get("resetsAt")
        if isinstance(used, bool) or not isinstance(used, (int, float)) or not math.isfinite(used) or not 0 <= used <= 100:
            continue
        reset = int(reset // 1000) if isinstance(reset, (int, float)) and not isinstance(reset, bool) and reset > 0 else None
        windows.append({"id": w["id"], "label": "".join(c for c in str(w.get("label") or w["id"])[:80] if c.isprintable()),
                        "used": used, "resets_at": reset})
    r["windows"] = windows
    checked = data.get("checkedAt")
    if isinstance(checked, bool) or not isinstance(checked, (int, float)) or checked <= 0:
        r["state"], r["why"] = "missing", "the app has no reading of account %s yet" % label
        return r
    r["age"] = max(0.0, now - checked / 1000.0)
    five = next((w for w in windows if w["id"] == "five_hour"), None)
    if five is None or five["resets_at"] is None:
        r["state"], r["why"] = "malformed", "the app's reading of account %s carries no five-hour window" % label
        return r
    r["state"], r["used"], r["resets_at"] = "ok", five["used"], five["resets_at"]
    r["ended"] = five["resets_at"] <= now
    return r


def read_source(a, now):
    """get_usage first; the status-line file when it fails, saying why. For the app
    (RICHOS_APP_STATE set): the app's own reading of the account in use, and nothing else."""
    state = app_state()
    if state:
        return read_app(state, now)
    g = read_get_usage(now)
    if g["state"] == "ok":
        return g
    if g.get("auth_failed"):
        told = report_login_failure(getattr(a, "engine_root", ""), g["auth_failed"])
        g["why"] += " (login alarm %s)" % ("raised" if told else "NOT raised")
    f = read_reading(a.payload, now)
    f["source"] = "the status line"
    f["fallback_why"] = g["why"]
    if g.get("windows"):
        f["windows"] = g["windows"]
    return f


def classify(r, threshold, stale_after):
    """('below' | 'at-or-above' | 'unknown', why)."""
    if r["state"] != "ok":
        return "unknown", r["why"]
    if r["ended"]:
        return "unknown", "the reading is from a window that ended at %s" % hhmm(r["resets_at"])
    if r["used"] >= threshold:
        # Monotone within a window: a stale reading at or above is still true.
        return "at-or-above", ""
    if r["age"] is not None and r["age"] > stale_after:
        return "unknown", ("the reading is stale: %s old, and below the threshold a stale reading "
                           "proves nothing" % span(r["age"]))
    return "below", ""


def in_last_twenty(resets_at, now):
    """True when the reset is LESS than 20 minutes away and has not passed.
    Exactly 20 minutes is False: it still pauses (his words, 2026-09-25)."""
    return resets_at is not None and 0 < resets_at - now < RESET_EXEMPTION_SECONDS


def rule_verdict(r, threshold, stale_after, now):
    """classify(), with his near-reset exception applied:
    ('below' | 'at-or-above' | 'near-reset' | 'unknown', why)."""
    verdict, why = classify(r, threshold, stale_after)
    if verdict == "at-or-above" and in_last_twenty(r["resets_at"], now):
        return "near-reset", ("the reset is %s away, less than 20 minutes: no pause, and a hold in place is "
                              "released (%s)" % (span(r["resets_at"] - now), RULING))
    return verdict, why


def describe(r, threshold, now):
    if r["state"] != "ok":
        return "UNKNOWN: %s" % r["why"]
    bits = ["%s%% of the five-hour window" % fmt_pct(r["used"]),
            "threshold %s%%" % fmt_pct(threshold) if threshold is not None else "threshold UNDECLARED",
            "read %s ago" % span(r["age"] or 0) + (" via %s" % r["source"] if r.get("source") else "")
            + (" (answered in %.1f s)" % r["seconds"] if r.get("source") == "get_usage" and r.get("seconds") else "")]
    if r["ended"]:
        bits.append("window ended at %s" % hhmm(r["resets_at"]))
    else:
        bits.append("resets %s (in %s)" % (hhmm(r["resets_at"]), span(r["resets_at"] - now)))
    extra = quota_weekly.describe(r, now)
    return ", ".join(bits) + ("; " + extra if extra else "")


# ---------------------------------------------------------------------------
# the workers — read from the workspace registry, never written
# ---------------------------------------------------------------------------

def _load_registry(engine_root):
    lib = os.path.join(engine_root, "mega-lander", "workspaces.py")
    if not os.path.isfile(lib):
        return None, "the workspace registry library is missing at %s" % lib
    try:
        spec = importlib.util.spec_from_file_location("quota_watch_workspaces", lib)
        ws = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ws)
        return ws, ""
    except Exception as e:  # noqa: BLE001 — any failure is "cannot name them"
        return None, "the workspace registry could not be loaded (%s)" % e.__class__.__name__


def workers(engine_root):
    """{'known': bool, 'why', 'session', 'working': [names], 'quota_paused': [names],
    'other_paused': [names]} for THIS session's registered teammates.

    Read-only: finished_state() over the recorded facts. A record the platform
    ended without a hook (a stopped agent) may still read as working here; the
    registry's own pending() reconciles that, and this watcher never writes."""
    out = {"known": False, "why": "", "session": "", "working": [], "quota_paused": [], "weekly_paused": [], "other_paused": [],
           "registered": []}
    if HOST["on"] and not host_child.session():
        out["why"] = "his team has no lead running in RichOS yet"
        return out
    ws, why = _load_registry(engine_root)
    if ws is None:
        out["why"] = why
        return out
    try:
        me = ws.current_session()
        if not me:
            out["why"] = "this command is not running inside a recorded session"
            return out
        out["session"] = me
        for rec in ws.all_agents():
            if rec.get("session_id") != me or rec.get("disposition"):
                continue
            fin, paused, _why = ws.finished_state(rec)
            if fin:
                continue
            name = rec.get("name") or rec.get("key") or "?"
            if paused:
                until = ((rec.get("pause") or {}).get("until") or "")
                (out["quota_paused"] if until.startswith(PAUSE_UNTIL_PREFIX) else
                 out["weekly_paused"] if until.startswith("the weekly quota reset") else out["other_paused"]).append(name)
            else:
                out["working"].append(name)
                at = _parse_reset(rec.get("registered_at"))
                if at:
                    out["registered"].append(at)
        out["known"] = True
    except Exception as e:  # noqa: BLE001
        out["why"] = "the workspace registry could not be read (%s)" % e.__class__.__name__
    for k in ("working", "quota_paused", "weekly_paused", "other_paused"):
        out[k] = sorted(set(out[k]))
    return out


def worker_line(w):
    if not w["known"]:
        return "live workers: UNKNOWN (%s)" % w["why"]
    s = "live workers: %d working" % len(w["working"])
    if w["working"]:
        s += " (%s)" % ", ".join(w["working"])
    s += "; %d paused for the quota" % len(w["quota_paused"])
    if w["other_paused"]:
        s += "; %d paused for other reasons" % len(w["other_paused"])
    return s


# ---------------------------------------------------------------------------
# the two messages the lead sends
# ---------------------------------------------------------------------------

def pause_message(r, threshold):
    return pause_protocol.render("quota", hhmm(r["resets_at"]))


def resume_message(reset_at):
    # The generated RESUME, unchanged: the registry ends a generated pause on it
    # and on nothing else (Sage's catch 3), and it never carries `pause-until:`.
    # The reset time is printed beside it for the lead, never inside it.
    return pause_protocol.render_resume(reason="quota")


def release_message(reset_at):
    # His 2026-09-25 update: the hold releases when the reset is less than 20
    # minutes away. The same agent resumes, with the same generated RESUME.
    return pause_protocol.render_resume("quota")


def protocol_command(args):
    """The one command that makes a pause or a resume take effect before its message."""
    return "python3 %s %s" % (os.path.abspath(pause_protocol.__file__), args)


def _print_message(text, summary):
    print("  ---- message begins ----")
    for ln in text.splitlines():
        print("  " + ln)
    print("  ---- message ends ----")
    print("  Use summary: " + summary)


# ---------------------------------------------------------------------------
# modes
# ---------------------------------------------------------------------------

def _exit_for(verdict):
    return {"below": 0, "at-or-above": 1, "near-reset": 3}.get(verdict, 2)


def mode_once(a, now):
    r = read_source(a, now)
    if a.threshold is None:
        print("quota: UNKNOWN: %s is %s in %s" % (CONFIG_KEY, a.threshold_problem, a.config or "(no config)"))
        return 2
    verdict, why = rule_verdict(r, a.threshold, a.stale, now)
    if quota_weekly.held(r, now):
        verdict, why = "at-or-above", "overall weekly usage reached 99%"
    line = describe(r, a.threshold, now)
    if verdict == "unknown" and r["state"] == "ok":
        line += "  [UNKNOWN: %s]" % why
    elif verdict == "at-or-above":
        line += "  [AT OR ABOVE THE THRESHOLD]"
    elif verdict == "near-reset":
        line += "  [AT OR ABOVE THE THRESHOLD, NO PAUSE: %s]" % why
    print("quota: " + line)
    if r.get("fallback_why"):
        print("quota: source: the status-line file, because %s" % r["fallback_why"])
    return _exit_for(verdict)


def mode_status(a, now):
    r = read_source(a, now)
    print("THE 93%% QUOTA RULE (%s)" % RULING)
    print("  his words : %s" % HIS_WORDS)
    print("  update    : %s" % RULE_UPDATE)
    print("  speed     : %s" % FAST_RULE)
    if a.threshold is None:
        print("  threshold : UNKNOWN: %s is %s in %s" % (CONFIG_KEY, a.threshold_problem, a.config or "(no config)"))
    else:
        print("  threshold : %s%%  (%s=%s in %s)" % (fmt_pct(a.threshold), CONFIG_KEY, a.threshold_raw, a.config))
    if app_state():
        print("  source    : %s, published at %s (the account in use; no probe in the app)" % (r["source"], r["path"]))
    elif r.get("fallback_why"):
        print("  source    : the status-line file %s, because %s" % (a.payload, r["fallback_why"]))
    else:
        print("  source    : Claude Code's get_usage (%s), fresh at this read" % claude_binary())
    if r["state"] != "ok":
        print("  reading   : UNKNOWN: %s" % r["why"])
    else:
        print("  reading   : %s%% of the five-hour window" % fmt_pct(r["used"]))
        print("  age       : %s%s" % (span(r["age"] or 0), " (the payload refreshes only when a status line renders)"
                                       if r.get("fallback_why") else " (the app's last check)" if app_state()
                                       else " (read at this poll)"))
        if r["ended"]:
            print("  window    : ENDED at %s; this reading describes a window that no longer exists"
                  % hhmm(r["resets_at"]))
        else:
            print("  resets    : %s, in %s" % (hhmm(r["resets_at"]), span(r["resets_at"] - now)))
    w = workers(a.engine_root)
    print("  windows   : " + quota_weekly.describe(r, now))
    print("  %s" % worker_line(w))
    if a.threshold is None:
        print("  verdict   : UNKNOWN")
        return 2
    verdict, why = rule_verdict(r, a.threshold, a.stale, now)
    if quota_weekly.held(r, now):
        verdict, why = "at-or-above", "overall weekly usage reached 99%"
    print("  verdict   : %s%s" % ({"below": "below the threshold", "at-or-above": "AT OR ABOVE the threshold",
                                    "near-reset": "AT OR ABOVE the threshold, NO PAUSE",
                                    "unknown": "UNKNOWN"}[verdict], (": " + why) if why and r["state"] == "ok" else ""))
    print("  watcher   : %s --watch   (as a background command)" % a.command)
    return _exit_for(verdict)


KEEPS_POLLING = "  The watcher keeps polling every 5 minutes; nothing needs starting again."


def _emit_threshold(a, r, now, w, early=""):
    print("QUOTA-THRESHOLD %s%%" % fmt_pct(r["used"]))
    print("  %s" % describe(r, a.threshold, now))
    if early:
        print("  %s" % early)
    if r["age"] is not None and r["age"] > a.stale:
        print("  The reading is stale, so this is a FLOOR: usage is at least %s%% now (it only grows inside a"
              " window)." % fmt_pct(r["used"]))
    print("  minutes to reset: %d" % max(0, (r["resets_at"] - now) // 60))
    print("  %s" % worker_line(w))
    print("  The CEO's rule (%s): %s" % (RULING, HIS_WORDS))
    if w["known"]:
        names = w["working"]
        print("  Send this message, unchanged, to each working agent: %s" % ", ".join(names))
    else:
        print("  Send this message, unchanged, to each working agent (the registry could not name them).")
    print("  First, for each agent, run: " + protocol_command("--to <agent> --reason quota --reset %s"
                                                             % hhmm(r["resets_at"])))
    print("  It freezes that agent before any message and prints this same message to send; a refused")
    print("  message never undoes the freeze.")
    print("  ---- message begins ----")
    for ln in pause_message(r, a.threshold).splitlines():
        print("  " + ln)
    print("  ---- message ends ----")
    print("  Use summary: " + pause_protocol.SUMMARY)
    print("  Delivery is only a request. Check the actual hold before reporting anyone paused.")
    print(KEEPS_POLLING)
    print("  It wakes you with the resume message when the reset is less than 20 minutes away (the hold")
    print("  releases there, %s) and at the reset. It does not wake you for this window's threshold" % RULING)
    print("  again unless an agent that was not named above starts working.")


def _emit_reset(a, reset_at, w):
    print("WINDOW-RESET at %s" % hhmm(reset_at))
    print("  The CEO's rule (%s): %s" % (RULING, HIS_WORDS))
    print("  %s" % worker_line(w))
    if w["known"]:
        if w["quota_paused"]:
            print("  Wake each quota-paused agent with this message: %s" % ", ".join(w["quota_paused"]))
        else:
            print("  No agent of this session is paused for the quota.")
        if w["other_paused"]:
            print("  Paused for OTHER reasons, not named in this rule's wake: %s" % ", ".join(w["other_paused"]))
    else:
        print("  The registry could not name the paused agents; wake every agent you paused for the quota.")
    print("  First, for each agent, run: " + protocol_command("--resume --to <agent> --reason quota"))
    print("  It releases that agent's hold before any message and prints this same message to send.")
    _print_message(resume_message(reset_at), pause_protocol.RESUME_SUMMARY)
    print(KEEPS_POLLING + " It goes on into the new window.")


def _emit_release(a, reset_at, now, w):
    print("QUOTA-RELEASE at %s" % hhmm(now))
    print("  The five-hour window resets at %s, in %s: less than 20 minutes, so the quota hold releases."
          % (hhmm(reset_at), span(reset_at - now)))
    print("  The CEO's rule (%s): %s" % (RULING, HIS_WORDS))
    print("  %s" % RULE_UPDATE)
    _wake_quota_paused(w, reset_at)
    print(KEEPS_POLLING + " Inside the last 20 minutes it pauses nobody, and it wakes you at the reset.")


def _wake_quota_paused(w, reset_at):
    print("  %s" % worker_line(w))
    if w["known"]:
        print("  Wake each quota-paused agent with this message: %s" % ", ".join(w["quota_paused"]))
        if w["other_paused"]:
            print("  Paused for OTHER reasons, not named in this rule's wake: %s" % ", ".join(w["other_paused"]))
    else:
        print("  The registry could not name the paused agents; wake every agent you paused for the quota.")
    print("  First, for each agent, run: " + protocol_command("--resume --to <agent> --reason quota"))
    print("  It releases that agent's hold before any message and prints this same message to send.")
    _print_message(release_message(reset_at), pause_protocol.RESUME_SUMMARY)


def _emit_switch_release(a, left, r, verdict, now, w):
    """The hold of the account just left releases: the account in use has room (finding 2 of the
    second review of d37b1f0de; the hold belongs to the account it was taken on)."""
    print("QUOTA-RELEASE at %s" % hhmm(now))
    now_on = r["account"]["label"] if isinstance(r.get("account"), dict) else "another account"
    state = ("below the threshold" if verdict == "below" else
             "at or above the threshold, but its reset is less than 20 minutes away")
    print("  The account in use changed from %s to %s, at %s%% of its five-hour window: %s. The quota hold"
          " taken on %s releases." % (left["label"], now_on, fmt_pct(r["used"]), state, left["label"]))
    print("  The CEO's rule (%s): %s" % (RULING, HIS_WORDS))
    _wake_quota_paused(w, r["resets_at"])
    print(KEEPS_POLLING + " From here it follows %s's own window and reset." % now_on)


def refresh_point(stale_after):
    """The age at which a below-threshold reading is refreshed: before it turns
    one poll old, never at or after."""
    return stale_after - min(REFRESH_AHEAD_SECONDS, stale_after // 10)


def _emit_refresh(a, r, w, at):
    print("QUOTA-REFRESH: the reading is %d s old and stops counting as current at %d s (one poll); refresh it now"
          % (int(r["age"] or 0), a.stale))
    if r.get("fallback_why"):
        print("  The reading comes from the status-line file, because %s." % r["fallback_why"])
    print("  Last value: %s%%, below the threshold %s%%. The CEO's rule (%s) checks every 5 minutes, so the"
          % (fmt_pct(r["used"]), fmt_pct(a.threshold), RULING))
    print("  watcher wakes you at %d s, before the reading ages past one poll, not a poll after it." % at)
    print("  %s" % worker_line(w))
    print("  REFRESH THE READING: reply once, in this turn. The payload is rewritten only when your")
    print("  status line renders, and it renders when a new assistant message of yours arrives;")
    print("  teammates' work does not render it.")
    print(KEEPS_POLLING)


def _emit_blind(a, r, w, why, outage=None):
    stale = r["state"] == "ok" and not r.get("ended")
    if outage is not None:
        print("QUOTA-STALE: get_usage has not answered since %s, and the reading is %s old (one poll is %d s), so"
              " the current value is UNKNOWN" % (_hms(outage["at"]), span(r["age"] or 0), a.poll))
        print("  Why the lead is woken now: %s." % outage["why"])
        print("  The last failure: %s." % outage["fallback_why"])
        print("  Last value seen: %s%%. It is NOT the current value: usage has only grown since."
              % fmt_pct(r["used"]))
    elif stale:
        print("QUOTA-STALE: the reading is %s old (one poll is %d s), so the current value is UNKNOWN"
              % (span(r["age"] or 0), a.poll))
        if r.get("fallback_why"):
            print("  It came from the status-line file, because %s." % r["fallback_why"])
        print("  Last value seen: %s%%. It is NOT the current value: usage has only grown since."
              % fmt_pct(r["used"]))
    else:
        print("QUOTA-UNKNOWN: %s" % (why or r["why"]))
    print("  The watcher cannot tell whether the threshold was crossed.")
    print("  %s" % worker_line(w))
    print("  REFRESH THE READING: reply once, in this turn. The payload is rewritten only when your")
    print("  status line renders, and it renders when a new assistant message of yours arrives;")
    print("  teammates' work does not render it.")
    print(KEEPS_POLLING + " get_usage is tried again every %d s while it fails." % _retry_seconds(a))


def _hms(epoch):
    return _dt.datetime.fromtimestamp(epoch, _dt.timezone.utc).strftime("%H:%M:%SZ")


def _retry_seconds(a):
    # The environment override is for the test suite only, whose polls are
    # seconds long: it must be able to retry sooner than one of them.
    return max(1, min(_env_int("QUOTA_WATCH_RETRY_SECONDS", RETRY_SECONDS), a.poll))


# ---------------------------------------------------------------------------
# a get_usage outage: retried, and a wake-up only when the threshold could
# have been crossed unseen (2026-09-28)
# ---------------------------------------------------------------------------

def same_window(a_reset, b_reset):
    """Two resets_at values name the same window. get_usage reports the reset
    to the second and it jitters by one (19:29:59Z / 19:30:00Z, 2026-09-28's
    log), so anything under ten minutes apart is the same five-hour window."""
    return a_reset is not None and b_reset is not None and abs(a_reset - b_reset) < 600


def fastest_rise(history, reading):
    """Percentage points per second: the fastest rise this watcher has MEASURED
    in the current window, between two consecutive get_usage readings, or on
    average since the window began, whichever is faster. Measured, never
    assumed: no constant here says how fast usage can grow."""
    rate = 0.0
    for (t0, u0), (t1, u1) in zip(history, history[1:]):
        if t1 > t0 and u1 > u0:
            rate = max(rate, (u1 - u0) / float(t1 - t0))
    at = reading.get("taken")
    if at is not None and reading.get("resets_at"):
        began = reading["resets_at"] - FIVE_HOUR_SECONDS
        if at > began and reading.get("used"):
            rate = max(rate, reading["used"] / float(at - began))
    return rate


def recent_speed(history, span, jump):
    """Percentage points per second between this window's get_usage readings,
    None until it can be measured. The rise over the latest stretch of at least
    `span` seconds (one 5-minute poll), or, when it is faster, the rise since
    the previous reading if that is at least `jump` seconds (one fast poll)
    back: a measured JUMP counts at once (§108), not averaged over 5 minutes.
    Nothing shorter is used: a reading moves in whole points, and one point
    over 88 s already reads as 3.4 points per 5 minutes (2026-09-27 18:08:37Z
    to 18:10:05Z, 46% to 47%)."""
    if len(history) < 2:
        return None
    t1, u1 = history[-1]
    rates = []
    t0, u0 = history[-2]
    if t1 - t0 >= jump:
        rates.append(max(0.0, (u1 - u0) / float(t1 - t0)))
    for t0, u0 in reversed(history[:-1]):
        if t1 - t0 >= span:
            rates.append(max(0.0, (u1 - u0) / float(t1 - t0)))
            break
    return max(rates) if rates else None


def expected_rise(w, now):
    """How many of this session's working teammates were registered in the
    last EXPECT_SECONDS: a big rise is expected from EXPECT_SPAWNS (§108)."""
    return sum(1 for t in w.get("registered", []) if now - t <= EXPECT_SECONDS)


def fast_speed(a):
    """Points per second from which usage counts as HIGH SPEED (ruling §108):
    the speed at which his rule at a 5-minute check stops being enough, because
    the points between the threshold and 100% are used up within one check plus
    the time the hold takes to land. 93% gives 7 points in 360 s, 5.8 points per
    5 minutes (the module docstring has the readings it was checked against)."""
    return (100.0 - a.threshold) / float(a.poll + a.hold)


def early_wake(a, used, speed):
    """At high speed: True when waiting for the next fast check would let
    usage reach 100% before the hold lands, so the lead must be woken now."""
    return used + speed * (a.fast_poll + a.hold) >= 100


def per5(rate):
    return rate * 300


def _early_text(a, r, speed, now):
    full = now + int((100 - r["used"]) / speed) if speed > 0 else now
    return ("EARLY, AT HIGH SPEED (ruling §108, richos-hq/wiki/ceo-decisions.md): usage is rising %.1f points per"
            " 5 minutes (high speed is %.1f or more). At that speed the window reaches 100%% at about %s, before"
            " the next check (%d s) plus the minute a hold takes to land, so the pause is due now, below the"
            " %s%% threshold." % (per5(speed), per5(fast_speed(a)), _hms(full), a.fast_poll, fmt_pct(a.threshold)))


def _emit_fast(a, r, speed, now, w):
    """The §108 addendum: the moment high speed is first measured in a window,
    the lead is told, separately from and before any pause message, so the CEO
    can be told."""
    full = now + int((100 - r["used"]) / speed) if speed > 0 else now
    print("QUOTA-FAST: usage is rising %.1f points per 5 minutes; high speed is %.1f or more"
          % (per5(speed), per5(fast_speed(a))))
    print("  At that speed the five-hour window is used up at about %s, in %s (now %s%%, resets %s)."
          % (_hms(full), span(full - now), fmt_pct(r["used"]), hhmm(r["resets_at"])))
    print("  TELL THE CEO: the ruling's addendum (§108, richos-hq/wiki/ceo-decisions.md) says the user is alerted"
          " when usage is this fast.")
    print("  %s" % worker_line(w))
    print("  This is an alert, not the pause. The watcher now checks every %d s and wakes you with the pause"
          " message as soon as the measured speed would carry the window to 100%% before the next check plus"
          " the minute a hold takes to land." % a.fast_poll)


def outage_verdict(a, reading, history, now):
    """(wake, why) for a below-threshold reading that is older than one poll
    because get_usage stopped answering.

    WAKE when the threshold could have been crossed unseen: the reading plus
    the fastest rise measured over the time since it was taken reaches the
    threshold, or the reading is BLIND_POLLS polls old. Otherwise keep retrying
    and say until when the crossing is ruled out."""
    age = now - reading["taken"]
    rate = fastest_rise(history, reading)
    reach = reading["used"] + rate * age
    cap = BLIND_POLLS * a.stale
    if reach >= a.threshold:
        return True, ("at the fastest rise measured this window (%.1f points per 5 minutes) usage could have gone"
                      " from %s%% to %s%% in %s" % (rate * 300, fmt_pct(reading["used"]), fmt_pct(a.threshold),
                                                   span(age)))
    if age >= cap:
        return True, ("no reading for %s, %d polls: a check the rule orders every 5 minutes has been missed"
                      % (span(age), BLIND_POLLS))
    if rate > 0:
        cross = reading["taken"] + int((a.threshold - reading["used"]) / rate)
    else:
        cross = None
    until = reading["taken"] + cap if cross is None else min(cross, reading["taken"] + cap)
    return False, ("the threshold cannot have been reached before %s at the fastest rise measured (%.1f points"
                   " per 5 minutes); the lead is woken then if no reading has arrived"
                   % (_hms(until), rate * 300))


# ---------------------------------------------------------------------------
# where the lines and the wake-ups go
# ---------------------------------------------------------------------------

def state_root():
    return (os.environ.get("QUOTA_WATCH_STATE_DIR") or "").strip() or os.path.join(
        os.path.expanduser("~"), ".claude", "state", "quota-watch")


def _append(path, text, rotate=False):
    try:
        if rotate and os.path.getsize(path) > POLLS_LOG_MAX:
            os.replace(path, path + ".1")
    except OSError:
        pass
    try:
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(text)
    except OSError:
        pass


def _write_json(path, data):
    tmp = "%s.%d.tmp" % (path, os.getpid())
    try:
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(data, fh)
        os.replace(tmp, path)
    except OSError:
        try:
            os.unlink(tmp)
        except OSError:
            pass


def _read_json(path):
    try:
        with open(path, encoding="utf-8") as fh:
            v = json.load(fh)
        return v if isinstance(v, dict) else {}
    except (OSError, ValueError):
        return {}


def _try_lock(path, patience=0.0):
    """An open descriptor holding an exclusive flock on path, or None when
    another process holds it. The OS drops the lock when its holder dies.
    lock_held() takes the lock for an instant to look, so a process that must
    not lose a race with a mere look waits `patience` seconds before giving up."""
    try:
        fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    except OSError:
        return None
    until = _monotonic() + patience
    while True:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return fd
        except OSError:
            if _monotonic() >= until:
                os.close(fd)
                return None
            _real_sleep(0.05)


def lock_held(path):
    """True when a live process holds the lock at path. Takes nothing."""
    if not os.path.exists(path):
        return False
    fd = _try_lock(path)
    if fd is None:
        return True
    try:
        fcntl.flock(fd, fcntl.LOCK_UN)
    finally:
        os.close(fd)
    return False


class Out:
    """Where the poller's output goes.

    inline:  Out(lines=sys.stdout, events=sys.stdout): poll lines and wake-ups
             to this process's stdout (a terminal, the replay, and --watch with
             no session process).
    journal: Out(sd=<session dir>): poll lines to polls.log and each wake-up,
             as one JSON line, to events.jsonl, where --monitor and --watch
             deliver it. Nothing on stdout.
    The streams are captured when Out is made, before poll_loop points stdout
    at the Out itself, so every print() in the loop lands in line()."""

    def __init__(self, sd=None, lines=None, events=None):
        self.sd = sd
        self.lines = lines
        self.events = events
        self.partial = ""

    # the file-like face, so every print() in the poll loop lands in line()
    def write(self, s):
        self.partial += s
        while "\n" in self.partial:
            ln, self.partial = self.partial.split("\n", 1)
            self.line(ln)
        return len(s)

    def flush(self):
        pass

    def line(self, text):
        if self.lines is not None:
            self.lines.write(text + "\n")
            self.lines.flush()
        if self.sd:
            _append(os.path.join(self.sd, "polls.log"), text + "\n", rotate=True)

    def wake(self, kind, text, now):
        """ONE wake-up, written whole in one write, so a monitor's host sees
        the block arrive together."""
        if not text.endswith("\n"):
            text += "\n"
        if self.sd:
            _append(os.path.join(self.sd, "events.jsonl"),
                    json.dumps({"at": now, "kind": kind, "text": text}) + "\n")
        if self.events is not None:
            self.events.write(text)
            self.events.flush()

    def beat(self, now, a, extra=None):
        if self.sd:
            hb = _read_json(os.path.join(self.sd, "poller.json"))
            if hb.get("pid") == os.getpid():
                hb["last_poll"] = now
                hb.update(extra or {})
                _write_json(os.path.join(self.sd, "poller.json"), hb)


def _capture(fn, *args):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        fn(*args)
    return buf.getvalue()


def _nap(seconds, alive):
    """Sleep, in slices of at most 5 s when there is a session to outlive, so
    a poller stops within seconds of its session and not a poll later."""
    if alive is None:
        time.sleep(seconds)
        return
    left = seconds
    while left > 0:
        step = min(5, left)
        time.sleep(step)
        left -= step
        if not alive():
            return


# ---------------------------------------------------------------------------
# the poll loop — every 300 s, for as long as its session lives
# ---------------------------------------------------------------------------

def poll_loop(a, out, alive=None):
    """His rule, applied at every poll, forever. Returns only when the session
    it belongs to has ended (0) or it cannot watch at all (2): an event is
    printed and the loop goes on. A poll that raises is reported and the
    polling goes on: nothing but the end of its session stops it."""
    # Consecutive failed polls. Each failure restarts the inner loop and its blind
    # timers, so the inner loop cannot notice an outage made of exceptions; this one
    # does (hunt part 5, P5-15): once the failures span one poll's worth of time, the
    # lead is told the watcher is blind, once per episode. A completed poll ends the episode.
    health = {"first": None, "woken": False, "count": 0}
    speed_settings(a)
    with contextlib.redirect_stdout(out):
        while True:
            try:
                return _poll_loop(a, out, alive, health)
            except Exception as e:  # noqa: BLE001 — any failure is one lost poll, never the end of polling
                print("quota-watch: a poll failed (%s: %s); polling goes on" % (e.__class__.__name__, e), flush=True)
                now = int(time.time())
                health["count"] += 1
                if health["first"] is None:
                    health["first"] = now
                if not health["woken"] and now - health["first"] >= a.stale:
                    health["woken"] = True
                    out.wake("UNKNOWN",
                             "QUOTA-UNKNOWN: the quota watcher is blind. %d polls in a row have failed since %s"
                             " (latest: %s: %s), so whether the threshold was crossed is UNKNOWN.\n"
                             "  The watcher cannot tell whether the threshold was crossed.\n"
                             "  Workers are not being paused or released by it until a poll completes.\n"
                             % (health["count"], _hms(health["first"]), e.__class__.__name__, e), now)
                _nap(a.poll, alive)


def _poll_loop(a, out, alive, health=None):
    if a.threshold is None:
        out.wake("CANNOT-WATCH", "QUOTA-UNKNOWN: cannot watch: %s is %s in %s\n" % (
            CONFIG_KEY, a.threshold_problem, a.config or "(no config)"), int(time.time()))
        return 2
    window_end = None
    blind_since = None
    refresh_in = None
    blind_woken = False          # one QUOTA-STALE/UNKNOWN per blind episode
    refreshed_for = None         # one QUOTA-REFRESH per status-line render
    threshold_named = {}         # window -> the working names already told to pause
    released = None              # the window whose hold release was announced
    reset_done = None            # the window whose reset was announced
    weekly_seen = set()
    last_gu = None               # the last get_usage reading: {"used", "resets_at", "taken", "why"}
    history = []                 # (taken, used) get_usage readings in the current window
    outage = None                # {"at": first failure, "fallback_why"} while get_usage fails
    fast_alerted = None          # the window whose high speed was announced (QUOTA-FAST, once)
    # The account every per-window value above describes (the app's reading names it; a terminal's
    # never does, so there it stays None and nothing below changes). Second review of d37b1f0de
    # (rv-20261028T184535Z-d37b1f0d-e0d9, finding 2): after Home -> Work the loop kept Home's
    # window_end, so Work, paused at 99%, was released on Home's reset timer while Work's own reset
    # was still 4,740 s away.
    bound = None                 # {"id", "label"} of that account
    left_held = None             # the account just left while its hold may still hold workers
    first = True
    print("quota-watch: polling every %d s; threshold %s%%%s" % (
        a.poll, fmt_pct(a.threshold), "; waking only at the release and the reset" if a.until_reset else ""),
        flush=True)
    while True:
        if alive is not None and not alive():
            print("quota-watch: the session this poller belongs to has ended; it ends with it", flush=True)
            return 0
        now = int(time.time())
        reset_status = quota_weekly.reset_tick(a.engine_root)
        r = read_source(a, now)
        w = workers(a.engine_root)
        retry = False
        asked_refresh = False
        who = r.get("account") if isinstance(r.get("account"), dict) and r["account"].get("id") else None
        if who is not None and bound is not None and who["id"] != bound["id"]:
            # A SWITCH: every timer and dedupe above was the account left's. They are cleared and
            # bound again from the account in use's own reading below, by window_end's usual rule.
            print("  the account in use changed: %s -> %s; its own window and notices from here"
                  % (bound["label"], who["label"]), flush=True)
            if window_end is not None:
                left_held = bound
            window_end = released = reset_done = fast_alerted = refreshed_for = None
            threshold_named, weekly_seen, history, last_gu, outage = {}, set(), [], None, None
            blind_since, blind_woken = None, False
        if who is not None:
            bound = who
        if r.get("source") == "get_usage":
            if last_gu is not None and not same_window(last_gu["resets_at"], r["resets_at"]):
                history = []
            history = (history + [(now, r["used"])])[-24:]
            last_gu = {"used": r["used"], "resets_at": r["resets_at"], "taken": now, "state": "ok"}
            if outage is not None:
                print("  get_usage answers again (it failed first at %s)" % _hms(outage["at"]), flush=True)
            outage = None
        else:
            retry = True
            if outage is None:
                outage = {"at": now, "fallback_why": r.get("fallback_why", "")}
            outage["fallback_why"] = r.get("fallback_why", "")
            if r.get("fallback_why"):
                print("  source: the status-line file, because %s" % r["fallback_why"], flush=True)
            # Once get_usage has answered in this window, its last reading is
            # kept: the status-line file stands in for it only when it is fresher.
            if last_gu is not None and last_gu["resets_at"] > now:
                file_taken = now - r["age"] if r["state"] == "ok" and not r["ended"] and r["age"] is not None else None
                if file_taken is None or file_taken < last_gu["taken"]:
                    kept = dict(r)
                    kept.update({"state": "ok", "used": last_gu["used"], "resets_at": last_gu["resets_at"],
                                 "age": float(now - last_gu["taken"]), "ended": False,
                                 "source": "get_usage at %s" % _hms(last_gu["taken"])})
                    r = kept
        # RULING §108: the speed, measured from this window's get_usage readings.
        speed = recent_speed(history, a.poll, a.fast_poll) if r.get("source") == "get_usage" else None
        fast = speed is not None and speed >= fast_speed(a)
        # ... and a big rise EXPECTED (many teammates just started) checks
        # every minute before any rise is measured. Back at normal speed with
        # nothing expected, everything is as before: 5 minutes, and 93%.
        spawned = expected_rise(w, now)
        expected = spawned >= EXPECT_SPAWNS
        out.beat(now, a, {"interval": a.fast_poll if fast or expected else a.poll, "fast_from": per5(fast_speed(a)),
                          "spawned_recently": spawned,
                          "speed": None if speed is None else per5(speed),
                          "reading": {"used": r["used"], "at": now, "source": r.get("source")}
                          if r["state"] == "ok" else None})
        if fast and r["state"] == "ok" and not r["ended"] and not same_window(fast_alerted, r["resets_at"]):
            # The §108 addendum: alert the lead the moment high speed is first
            # measured in this window, before and apart from any pause message.
            out.wake("FAST", _capture(_emit_fast, a, r, speed, now, w), now)
            fast_alerted = r["resets_at"]
        weekly_text = io.StringIO()
        with contextlib.redirect_stdout(weekly_text):
            weekly_blocked, weekly_event = quota_weekly.handle(a, r, now, w, reset_status,
                lambda: rule_verdict(r, a.threshold, a.stale, now)[0])
        if weekly_event:
            text = weekly_text.getvalue()
            # The key names the weekly WINDOW as well as the headline and the workers
            # (hunt part 5, P5-16): the next window's crossing is a new alarm for the
            # same workers. A threshold and a release each end the other's dedupe, so
            # a hold that is released and then needed again wakes the lead again.
            kind = "threshold" if weekly_blocked else "release"
            this_window = quota_weekly.weekly(r)
            key = (kind, text.split("\n", 1)[0], tuple(w["working"]), tuple(w.get("weekly_paused", [])),
                   this_window["resets_at"] if this_window else None)
            if key not in weekly_seen:
                weekly_seen = {k for k in weekly_seen if k[0] == kind}
                weekly_seen.add(key)
                out.wake("WEEKLY-THRESHOLD" if weekly_blocked else "WEEKLY-RELEASE", text, now)
        if weekly_blocked:
            print(quota_weekly.describe(r, now), flush=True)
            _poll_completed(health)
            _nap(a.poll, alive)
            continue
        if first and r["state"] == "ok" and r["ended"]:
            # Started after a reset had already passed: the agents it held
            # still need waking, and nothing else does.
            if not w["known"] or w["quota_paused"]:
                window_end = r["resets_at"]
        first = False
        if window_end is not None and now >= window_end:
            if reset_done is None or not same_window(reset_done, window_end):
                out.wake("RESET", _capture(_emit_reset, a, window_end, workers(a.engine_root)), now)
                reset_done = window_end
            a.until_reset = False
            window_end = None
            blind_since, blind_woken = None, False
        if window_end is not None and in_last_twenty(window_end, now) and released != window_end:
            # His 2026-09-25 update: a hold in place releases inside the last
            # 20 minutes. Only when somebody is held (or the registry cannot
            # say): with nobody held there is nothing to release.
            w = workers(a.engine_root)
            if not w["known"] or w["quota_paused"]:
                out.wake("RELEASE", _capture(_emit_release, a, window_end, now, w), now)
                released = window_end
        if window_end is None and r["state"] == "ok" and not r["ended"]:
            window_end = r["resets_at"]
        verdict, why = rule_verdict(r, a.threshold, a.stale, now)
        print("%s  %s" % (_dt.datetime.fromtimestamp(now, _dt.timezone.utc).strftime("%H:%M:%SZ"),
                          describe(r, a.threshold, now) + ("  [UNKNOWN: %s]" % why if verdict == "unknown" and r["state"] == "ok" else "")),
              flush=True)
        early = ""
        if expected and not fast:
            print("  RISE EXPECTED: %d teammates started in the last %d s (%d or more); checking every %d s"
                  % (spawned, EXPECT_SECONDS, EXPECT_SPAWNS, a.fast_poll), flush=True)
        if fast:
            print("  HIGH SPEED: %.1f points per 5 minutes (%.1f or more); checking every %d s"
                  % (per5(speed), per5(fast_speed(a)), a.fast_poll), flush=True)
            if verdict == "below" and early_wake(a, r["used"], speed):
                if in_last_twenty(r["resets_at"], now):
                    print("  at this speed the pause is due, but the reset is less than 20 minutes away: no pause",
                          flush=True)
                else:
                    verdict, early = "at-or-above", _early_text(a, r, speed, now)
        if left_held is not None and r["state"] == "ok" and not r["ended"] and verdict != "unknown":
            # The hold in place was the account just left's. Once the account in use reads below the
            # line (or at it with its reset less than 20 minutes away), whoever that hold paused may
            # work again now, not at the new account's reset. At or above it, the new account's own
            # hold takes over, and its release follows its own reset.
            if verdict in ("below", "near-reset"):
                w = workers(a.engine_root)
                if not w["known"] or w["quota_paused"]:
                    out.wake("RELEASE", _capture(_emit_switch_release, a, left_held, r, verdict, now, w), now)
                    if verdict == "near-reset":
                        released = window_end  # this window's own release has now been told
            left_held = None
        # A get_usage OUTAGE: it answered earlier in this window and fails now.
        # Its last reading, or the status-line file when that is fresher, is
        # the reading; the 2026-09-25 status-line rules (wake to refresh before
        # one poll) apply only when get_usage has not answered in this window.
        in_outage = (outage is not None and last_gu is not None and last_gu["resets_at"] > now
                     and r["state"] == "ok" and not r["ended"])
        if not a.until_reset:
            if verdict == "near-reset":
                print("  at or above the threshold, but %s" % why, flush=True)
            if verdict == "at-or-above":
                w = workers(a.engine_root)
                if w["known"] and not w["working"]:
                    print("  at the threshold with nothing working: nothing to pause; waiting for the reset", flush=True)
                else:
                    told = threshold_named.get(window_end)
                    if told is None or (w["known"] and not set(w["working"]) <= told):
                        out.wake("THRESHOLD", _capture(_emit_threshold, a, r, now, w, early), now)
                        threshold_named[window_end] = set(w["working"]) if w["known"] else set()
                    else:
                        print("  at or above the threshold; the lead was told for this window at the first crossing",
                              flush=True)
            if verdict == "unknown" and in_outage:
                # A get_usage OUTAGE, not a blind watcher: retried every
                # RETRY_SECONDS, and the lead is woken only when the threshold
                # could have been crossed unseen (outage_verdict).
                reading = {"used": r["used"], "resets_at": r["resets_at"], "taken": now - int(r["age"] or 0)}
                wake, owhy = outage_verdict(a, reading, history, now)
                w = workers(a.engine_root)
                if not wake:
                    print("  get_usage is not answering; %s; trying again in %d s" % (owhy, _retry_seconds(a)),
                          flush=True)
                elif w["known"] and not w["working"]:
                    print("  the reading is unknown but nothing is working: no spend it could hide", flush=True)
                elif not blind_woken:
                    out.wake("STALE", _capture(_emit_blind, a, r, w, why,
                                               {"at": outage["at"], "why": owhy,
                                                "fallback_why": outage["fallback_why"]}), now)
                    blind_woken = asked_refresh = True
            elif verdict == "unknown":
                if blind_since is None:
                    blind_since = now
                # A STALE reading (older than one poll) is blind NOW: it is the
                # 2026-09-25 failure, and waiting another poll on it is how the
                # 93% crossing went unseen. Any other unknown (missing,
                # malformed, a window that has ended) counts from this
                # watcher's own clock, so a payload that has not yet
                # re-rendered after a reset gets one poll to do so.
                stale = r["state"] == "ok" and not r["ended"]
                if stale or now - blind_since >= a.stale:
                    w = workers(a.engine_root)
                    if w["known"] and not w["working"]:
                        print("  the reading is unknown but nothing is working: no spend it could hide", flush=True)
                    elif not blind_woken:
                        out.wake("STALE" if stale else "UNKNOWN", _capture(_emit_blind, a, r, w, why), now)
                        blind_woken = asked_refresh = True
            else:
                blind_since = None
                blind_woken = False
            if verdict == "below" and r["age"] is not None and r.get("source") == "the status line" and not in_outage:
                at = refresh_point(a.stale)
                if r["age"] >= at:
                    w = workers(a.engine_root)
                    if w["known"] and not w["working"]:
                        print("  the reading is about to turn one poll old, but nothing is working: no refresh needed",
                              flush=True)
                    elif refreshed_for != int(now - r["age"]):
                        out.wake("REFRESH", _capture(_emit_refresh, a, r, w, at), now)
                        refreshed_for = int(now - r["age"])
                        asked_refresh = True
                else:
                    refresh_in = at - r["age"]
        sleep_for = a.fast_poll if fast or expected else a.poll
        if window_end is not None:
            sleep_for = max(1, min(sleep_for, window_end - now))
            # Wake at the first second inside the last 20 minutes, where a
            # hold releases, rather than up to one poll late.
            to_release = window_end - now - (RESET_EXEMPTION_SECONDS - 1)
            if to_release > 0:
                sleep_for = max(1, min(sleep_for, to_release))
        if refresh_in is not None:
            # Poll again when the reading reaches the refresh point, not a full
            # poll later (the 07:57Z/08:07Z defect above).
            sleep_for = max(1, min(sleep_for, int(math.ceil(refresh_in))))
            refresh_in = None
        if retry:
            # "retried at the next poll, or sooner" (2026-09-28).
            sleep_for = max(1, min(sleep_for, _retry_seconds(a)))
        if asked_refresh:
            # The lead was asked to refresh the status line: read it again
            # shortly, so the next refresh point is scheduled from the new
            # render and not a whole poll after it (until 2026-09-28 the lead
            # restarted the watcher, which re-read at once).
            sleep_for = max(1, min(sleep_for, REFRESH_AHEAD_SECONDS, a.stale // 10))
        _poll_completed(health)
        # Counted from the START of this poll, so the polls are 300 s apart
        # and not 300 s plus however long get_usage took (20 s on a timeout).
        _nap(max(1, sleep_for - int(time.time() - now)), alive)


def _poll_completed(health):
    """A failure episode ends only when a poll has done ALL of its job: read the quota,
    judged it AND delivered whatever it had to tell the lead. Reset any earlier (it
    used to be reset right after the read, hunt part 5, P5-15 v2) and a threshold
    notice that fails to deliver at every poll never reaches the blind alarm."""
    if health is not None:
        health["first"], health["woken"], health["count"] = None, False, 0


# ---------------------------------------------------------------------------
# the session, its poller and its doorbells
# ---------------------------------------------------------------------------

def session_identity(engine_root):
    """(session id, claude pid, that pid's start time) for the session this
    command runs in, from the workspace registry's own resolution. Empty when
    there is none (a plain terminal)."""
    ws, _why = _load_registry(engine_root)
    if ws is None:
        return "", None, ""
    try:
        sid = ws.current_session() or ""
        pid = ws.session_pid(sid) if sid else ws.session_pid()
        start = ws.process_start(pid)[1] if pid else ""
    except Exception:  # noqa: BLE001 — no session is a state, not a crash
        return "", None, ""
    return sid, pid, start or ""


def _pid_start(pid):
    """The OS's start time of pid, as `ps -o lstart=` prints it; '' if gone."""
    try:
        res = subprocess.run(["ps", "-o", "lstart=", "-p", str(int(pid))], capture_output=True, text=True, timeout=10)
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return ""
    return res.stdout.strip() if res.returncode == 0 else ""


def session_alive_check(pid):
    """A callable: is the session's claude process still the one this was
    started for? Its pid at every call, and its start time (captured now) once
    a minute, so a pid the OS has reused is not mistaken for it."""
    start = _pid_start(pid)
    calls = [0]

    def alive():
        try:
            os.kill(int(pid), 0)
        except PermissionError:
            pass
        except (OSError, ValueError, TypeError):
            return False
        calls[0] += 1
        if start and calls[0] % 12 == 0:
            now_start = _pid_start(pid)
            if now_start and now_start != start:
                return False
        return True
    return alive


def session_dir(sid):
    d = os.path.join(state_root(), sid or "no-session")
    os.makedirs(d, exist_ok=True)
    return d


def prune_state(keep):
    """§54: a session's directory goes once nothing holds its locks and it has
    not been written for two days. Never the one in use."""
    root = state_root()
    try:
        names = os.listdir(root)
    except OSError:
        return
    now = time.time()
    for n in names:
        d = os.path.join(root, n)
        if d == keep or not os.path.isdir(d):
            continue
        if lock_held(os.path.join(d, "poller.lock")) or lock_held(os.path.join(d, "monitor.lock")):
            continue
        try:
            newest = max([os.path.getmtime(os.path.join(d, f)) for f in os.listdir(d)] + [os.path.getmtime(d)])
        except OSError:
            continue
        if now - newest > STATE_KEEP_SECONDS:
            shutil.rmtree(d, ignore_errors=True)


def ensure_poller(a, sd, spid):
    """(pid, started_now) of this session's poller, starting one if nothing
    holds poller.lock. The poller is detached (its own session), so it
    outlives the command that started it and ends with the claude process."""
    lock = os.path.join(sd, "poller.lock")
    if lock_held(lock):
        return _read_json(os.path.join(sd, "poller.json")).get("pid"), False
    args = [sys.executable, os.path.abspath(__file__), "--poll", "--session-dir", sd,
            "--session-pid", str(spid), "--threshold-raw", a.threshold_raw, "--config", a.config or "",
            "--engine-root", a.engine_root, "--command", a.command]
    try:
        err = open(os.path.join(sd, "poller.err"), "ab")
    except OSError:
        err = subprocess.DEVNULL
    try:
        p = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=err,
                             start_new_session=True, close_fds=True)
    except OSError:
        return None, False
    finally:
        if err is not subprocess.DEVNULL:
            err.close()
    for _ in range(100):
        if lock_held(lock):
            return p.pid, True
        if p.poll() is not None:
            break
        _real_sleep(0.1)
    return (p.pid, True) if lock_held(lock) else (None, False)


# The doorbells wait on files with the real clock, whatever a test does to `time`.
_real_sleep = time.sleep
_real_time = time.time


def mode_poll(a):
    """The detached poller: one per session, holding poller.lock for its life."""
    sd = a.session_dir
    fd = _try_lock(os.path.join(sd, "poller.lock"), patience=1.0)
    if fd is None:
        return 0  # another poller already polls this session
    _write_json(os.path.join(sd, "poller.json"), {"pid": os.getpid(), "started": int(_real_time()),
                                                  "poll": a.poll, "last_poll": None, "session_pid": a.session_pid})
    prune_state(sd)
    alive = session_alive_check(a.session_pid) if a.session_pid else None
    try:
        return poll_loop(a, Out(sd=sd), alive)
    finally:
        os.close(fd)


def _read_events(sd, offset):
    """[(end_offset, event)] after offset in events.jsonl."""
    path = os.path.join(sd, "events.jsonl")
    out = []
    try:
        with open(path, "rb") as fh:
            fh.seek(offset)
            pos = offset
            for raw in fh:
                if not raw.endswith(b"\n"):
                    break
                pos += len(raw)
                try:
                    ev = json.loads(raw.decode("utf-8"))
                except ValueError:
                    continue
                if isinstance(ev, dict) and isinstance(ev.get("text"), str):
                    out.append((pos, ev))
    except OSError:
        pass
    return out


def _cursor(sd):
    v = _read_json(os.path.join(sd, "delivered.json")).get("offset")
    return v if isinstance(v, int) and v >= 0 else 0


def _set_cursor(sd, offset):
    _write_json(os.path.join(sd, "delivered.json"), {"offset": offset, "at": int(_real_time())})


def _doorbell_setup(a):
    """(sd, spid) for a doorbell, or None to poll inline (no session)."""
    sid, spid, _start = session_identity(a.engine_root)
    if not spid:
        return None
    return session_dir(sid), spid


def mode_monitor(a):
    """The plugin monitor's body: runs for the whole session, and every line it
    prints reaches the lead as a notification. It prints each wake-up once and
    keeps going; it keeps a poller running (starting it again if it dies)."""
    if not a.config:
        return 0  # a repository that never adopted the engine: nothing to watch, and nothing said
    if a.threshold is None:
        print("QUOTA-UNKNOWN: cannot watch: %s is %s in %s" % (CONFIG_KEY, a.threshold_problem, a.config), flush=True)
        return 2
    setup = _doorbell_setup(a)
    if setup is None:
        # No session process to hand a poller to: poll here, wake-ups only
        # (every line a monitor prints wakes the lead).
        return poll_loop(a, Out(events=sys.stdout))
    sd, spid = setup
    mfd = _try_lock(os.path.join(sd, "monitor.lock"), patience=1.0)
    if mfd is None:
        return 0  # this session already has its monitor
    _write_json(os.path.join(sd, "monitor.json"), {"pid": os.getpid(), "started": int(_real_time())})
    alive = session_alive_check(spid)
    offset = _cursor(sd)
    try:
        while alive():
            pid, started = ensure_poller(a, sd, spid)
            if pid is None:
                print("QUOTA-UNKNOWN: the quota poller could not start; see %s" % os.path.join(sd, "poller.err"),
                      flush=True)
                _real_sleep(60)
                continue
            for end, ev in _read_events(sd, offset):
                sys.stdout.write(ev["text"])
                sys.stdout.flush()
                offset = end
                _set_cursor(sd, offset)
            _real_sleep(1)
    finally:
        os.close(mfd)
    return 0


class HostEvents:
    """--host-json's wake-up stream: each wake-up, whole, as one JSON line on stdout."""

    def __init__(self, stream):
        self.stream = stream

    def write(self, text):
        self.stream.write(host_child.line(text) + "\n")

    def flush(self):
        self.stream.flush()


def mode_host(a):
    """--monitor --host-json: the RichOS app host's own child (richos-core team_watch.rs), from
    the app's launch to its quit. It polls right here, as a monitor with no session process to
    hand a poller to does, for as long as its parent is the host; each wake-up is one JSON line
    the host sends to his team's lead, and each poll line goes to its log (stderr). The live
    workers it names are those of the lead the host names (host_child.session)."""
    if not a.config:
        return 0  # a repository that never adopted the engine: nothing to watch, and nothing said
    alive = host_child.start()
    if not alive():
        return 0  # its host is gone already
    sd = session_dir("operator-host")
    fd = _try_lock(os.path.join(sd, "monitor.lock"), patience=1.0)
    if fd is None:
        return 0  # another copy of the app already watches
    HOST["on"] = True
    try:
        return poll_loop(a, Out(lines=sys.stderr, events=HostEvents(sys.stdout)), alive)
    finally:
        os.close(fd)


def mode_watch(a):
    """The fallback doorbell, run as a background command: it prints the poll
    lines as they come and EXITS on the next wake-up, which is what wakes the
    lead. The poller does not stop with it."""
    if a.threshold is None:
        print("QUOTA-UNKNOWN: cannot watch: %s is %s in %s" % (CONFIG_KEY, a.threshold_problem, a.config or "(no config)"))
        return 2
    setup = _doorbell_setup(a)
    if setup is None:
        # No session process to tie a poller to: poll right here, forever.
        return poll_loop(a, Out(lines=sys.stdout, events=sys.stdout))
    sd, spid = setup
    if lock_held(os.path.join(sd, "monitor.lock")):
        mon = _read_json(os.path.join(sd, "monitor.json"))
        print("quota-watch: this session's watcher is its plugin monitor (pid %s); it polls every 5 minutes and wakes"
              " you by itself. Nothing to start." % mon.get("pid", "?"))
        return 0
    wfd = _try_lock(os.path.join(sd, "waker.lock"), patience=1.0)
    if wfd is None:
        print("quota-watch: another --watch already waits for this session's next wake-up. Nothing to start.")
        return 0
    try:
        polls = os.path.join(sd, "polls.log")
        try:
            ppos = os.path.getsize(polls)
        except OSError:
            ppos = 0
        pid, started = ensure_poller(a, sd, spid)
        if pid is None:
            print("QUOTA-UNKNOWN: cannot watch: the poller could not start; see %s" % os.path.join(sd, "poller.err"))
            return 2
        print("quota-watch: polling every %d s; threshold %s%%%s; poller pid %s (%s)" % (
            a.poll, fmt_pct(a.threshold), "; waking only at the release and the reset" if a.until_reset else "",
            pid, "started now" if started else "already polling"), flush=True)
        offset = _cursor(sd)
        alive = session_alive_check(spid)
        while True:
            ppos = _print_new_lines(polls, ppos)
            woke = False
            for end, ev in _read_events(sd, offset):
                offset = end
                if a.until_reset and ev.get("kind") not in UNTIL_RESET_KINDS:
                    continue
                age = int(_real_time()) - int(ev.get("at") or 0)
                if age > 90:
                    print("(this wake-up came at %s, %s ago, while no --watch was running)" % (
                        _hms(ev.get("at") or 0), span(age)))
                sys.stdout.write(ev["text"])
                woke = True
            _set_cursor(sd, offset)
            if woke:
                print("  To be woken at the next one, start this again as a background command:")
                print("    %s --watch" % a.command)
                sys.stdout.flush()
                return 0
            if not lock_held(os.path.join(sd, "poller.lock")):
                if not alive():
                    print("quota-watch: the session has ended")
                    return 0
                pid, started = ensure_poller(a, sd, spid)
                print("quota-watch: the poller had stopped; %s" % (
                    "started again, pid %s" % pid if pid else "it could not be started again"), flush=True)
            _real_sleep(0.5)
    finally:
        os.close(wfd)


def _print_new_lines(path, pos):
    try:
        size = os.path.getsize(path)
    except OSError:
        return pos
    if size < pos:
        pos = 0  # rotated
    if size == pos:
        return pos
    try:
        with open(path, "rb") as fh:
            fh.seek(pos)
            data = fh.read()
    except OSError:
        return pos
    cut = data.rfind(b"\n") + 1
    if cut:
        sys.stdout.write(data[:cut].decode("utf-8", "replace"))
        sys.stdout.flush()
    return pos + cut


def mode_alive(a):
    """Is this session's quota rule being watched right now, and by what?"""
    setup = _doorbell_setup(a)
    if setup is None:
        print("quota-watch: no session process here; nothing to check")
        return 2
    sd, _spid = setup
    poller = lock_held(os.path.join(sd, "poller.lock"))
    hb = _read_json(os.path.join(sd, "poller.json"))
    monitor = lock_held(os.path.join(sd, "monitor.lock"))
    waker = lock_held(os.path.join(sd, "waker.lock"))
    now = int(_real_time())
    if not poller:
        print("quota-watch: NOT WATCHED: no poller for this session (%s)." % sd)
        print("  Start one as a background command (Bash with run_in_background: true): %s --watch" % a.command)
        return 2
    last = hb.get("last_poll")
    print("quota-watch: poller pid %s, polling every %s s; last poll %s" % (
        hb.get("pid"), hb.get("interval") or hb.get("poll"),
        ("%s, %s ago" % (_hms(last), span(now - last))) if last else "not yet"))
    rd = hb.get("reading")
    if isinstance(rd, dict):
        print("  last reading: %s%% of the five-hour window at %s via %s" % (
            fmt_pct(rd.get("used")), _hms(rd.get("at") or 0), rd.get("source")))
    sp, fast_from = hb.get("speed"), hb.get("fast_from")
    if sp is None:
        print("  measured speed: not yet (it needs two get_usage readings one poll apart in this window)")
    elif fast_from is not None:
        print("  measured speed: %.1f points per 5 minutes, %s (high speed is %.1f or more, ruling §108)" % (
            sp, "HIGH" if sp >= fast_from else "normal", fast_from))
    if hb.get("spawned_recently"):
        print("  teammates started in the last %d s: %d (a big rise is expected from %d)" % (
            EXPECT_SECONDS, hb["spawned_recently"], EXPECT_SPAWNS))
    if monitor:
        print("  wake-ups reach you through the plugin monitor (pid %s)" % _read_json(os.path.join(sd, "monitor.json")).get("pid"))
        return 0
    if waker:
        print("  wake-ups reach you when the running --watch exits")
        return 0
    print("  NOTHING DELIVERS ITS WAKE-UPS: the poller keeps polling and journals them, but no monitor or --watch is")
    print("  running. Start as a background command (Bash with run_in_background: true): %s --watch" % a.command)
    return 1


def mode_notice(a, now):
    """The SessionStart notice body. Printed as plain text; the hook wraps it.
    Silent (prints nothing) only where the rule cannot apply at all: a
    repository that never adopted the engine has no orchestration.config, and
    engine-status.sh already announces the stand-down there."""
    if not a.config:
        return 0
    lines = ["=== THE 93%% QUOTA RULE applies to this session (%s) ===" % RULING,
             "  His words: %s" % HIS_WORDS,
             "  %s" % RULE_UPDATE,
             "  %s" % FAST_RULE]
    if a.threshold is None:
        lines.append("  THE WATCHER CANNOT RUN HERE: %s is %s in %s. Declare it there, e.g. %s=93."
                     % (CONFIG_KEY, a.threshold_problem, a.config or "(no orchestration.config)", CONFIG_KEY))
    else:
        # The app's own reading in app context (read_app, a file read, no process), else the file.
        r = read_app(app_state(), now) if app_state() else read_reading(a.payload, now)
        verdict, why = rule_verdict(r, a.threshold, a.stale, now)
        lines.append("  Threshold: %s=%s (%s)" % (CONFIG_KEY, a.threshold_raw, a.config))
        now_line = describe(r, a.threshold, now)
        if verdict == "unknown" and r["state"] == "ok":
            now_line += " [UNKNOWN: %s]" % why
        elif verdict == "at-or-above":
            now_line += " [ALREADY AT OR ABOVE THE THRESHOLD]"
        elif verdict == "near-reset":
            now_line += " [AT OR ABOVE THE THRESHOLD, NO PAUSE: %s]" % why
        lines.append("  Now: %s" % now_line)
        lines.append("  Weekly: at 99% overall use, consume one eligible user-approved free reset or send the standard pause.")
        lines.append("  There is no weekly 20-minute exception. Resume only after fresh weekly allowance and the five-hour rule both permit work.")
        lines.append("  Only the user can approve through scripts/quota-reset.sh approve <offer-id>; you may read status or revoke, never approve.")
        lines.append("  THE WATCHER STARTS BY ITSELF with this session: the engine's plugin monitor `quota-watch` runs")
        lines.append("    %s --monitor" % a.command)
        lines.append("  for the whole session. It polls every 5 minutes and never stops on an event: each wake-up")
        lines.append("  reaches you as a notification and the polling goes on. Do not start it yourself.")
        lines.append("  Check it at any time; it says what polls and what delivers the wake-ups:")
        lines.append("    %s --alive" % a.command)
        lines.append("  Only if --alive says nothing delivers them (a session where plugin monitors do not run), start")
        lines.append("  this as a background command (Bash with run_in_background: true); it exits on each wake-up")
        lines.append("  while the polling goes on, so start it again after each one:")
        lines.append("    %s --watch" % a.command)
        lines.append("  Every 5 minutes it reads Claude Code's own get_usage (the status-line file only as a fallback;")
        lines.append("  the reading above is that file, read without waiting on a process at session start).")
        lines.append("  For the five-hour rule it wakes you at the threshold with the pause message and the")
        lines.append("  names to send it to (never inside the last 20 minutes before the reset), when the reset is")
        lines.append("  less than 20 minutes away with the resume message and the paused names, and at the reset.")
    print("\n".join(lines))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(prog="quota-watch.sh", add_help=True)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--once", action="store_true")
    g.add_argument("--watch", action="store_true")
    g.add_argument("--status", action="store_true")
    g.add_argument("--notice", action="store_true")
    g.add_argument("--monitor", action="store_true",
                   help="the plugin monitor's body: runs for the whole session and prints each wake-up")
    g.add_argument("--alive", action="store_true", help="what polls for this session, and what delivers")
    g.add_argument("--poll", dest="poller", action="store_true", help=argparse.SUPPRESS)
    ap.add_argument("--host-json", action="store_true",
                    help="with --monitor: the app host's child; each wake-up as one JSON line")
    ap.add_argument("--until-reset", action="store_true",
                    help="with --watch: wake only at the hold's release and the reset (the lead has decided this window)")
    ap.add_argument("--session-dir", default="", help=argparse.SUPPRESS)
    ap.add_argument("--session-pid", type=int, default=0, help=argparse.SUPPRESS)
    ap.add_argument("--threshold-raw", default="")
    ap.add_argument("--config", default="")
    ap.add_argument("--engine-root", default="")
    ap.add_argument("--command", default="quota-watch.sh")
    a = ap.parse_args(argv)
    if a.until_reset and not a.watch:
        ap.error("--until-reset goes with --watch")
    if a.poller and not a.session_dir:
        ap.error("--poll is started by --watch or --monitor")
    a.payload = default_payload_path()
    a.poll = _env_int("QUOTA_WATCH_POLL_SECONDS", POLL_SECONDS)
    a.stale = _env_int("QUOTA_WATCH_STALE_SECONDS", a.poll)
    a.fast_poll = a.hold = None
    speed_settings(a)  # ruling §108
    a.threshold_raw = (a.threshold_raw or "").strip()
    a.threshold, a.threshold_problem = parse_threshold(a.threshold_raw)
    a.engine_root = a.engine_root or os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    now = int(time.time())
    # Test only: pins the clock for --once, --status and --notice, so the
    # 19/20/21-minute boundaries are exact to the second. --watch ignores it.
    pinned = (os.environ.get("QUOTA_WATCH_NOW") or "").strip()
    if pinned.isdigit():
        now = int(pinned)
    if a.once:
        return mode_once(a, now)
    if a.status:
        return mode_status(a, now)
    if a.notice:
        return mode_notice(a, now)
    if a.monitor and a.host_json:
        return mode_host(a)
    if a.monitor:
        return mode_monitor(a)
    if a.alive:
        return mode_alive(a)
    if a.poller:
        return mode_poll(a)
    return mode_watch(a)


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except KeyboardInterrupt:
        sys.exit(130)
