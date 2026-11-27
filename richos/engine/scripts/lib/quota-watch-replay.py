"""Historical quota timelines without wall-clock waits.

Only quota_watch's clock is replaced. Its poll loop, payload parsing, registry,
rule decisions and (R04) subprocess control transport execute unchanged. Real
deadline, polling and registry journeys remain in quota-watch.test.sh.

The simulated lead stays idle. The fixture answers get_usage at the shared
reader's cadence; newer status-line frames can arrive between those answers.
A pause or the end of the timeline ends the replay.
"""
import io
import json
import os
from pathlib import Path
import re
import sys
import time
from types import SimpleNamespace
from unittest.mock import patch

import quota_watch as watch


class EndOfReplay(BaseException):
    """Not an Exception: the poll loop reports and survives any Exception."""


def replay_local_deadline(work):
    for provider_at, retry_at, expected in (
        (1020, None, 1021),
        (1025, None, 1025),
        (None, None, 1030),
        (1045, None, 1030),
        (1045, 1028, 1028),
        (None, 1040, 1030),
    ):
        reading = {"provider_next_at": provider_at * 1000} if provider_at else {}
        status = {"resets": {"retryAt": retry_at * 1000}} if retry_at else {}
        with patch.object(watch, "time", SimpleNamespace(time=lambda: 1020)), \
                patch.object(watch, "app_state", return_value=None), \
                patch.object(watch, "_nap") as nap:
            watch._sleep_after_poll(1000, 30, 1015, status, reading, None)
            nap.assert_called_once_with(expected - 1020, None)

    # A hard local deadline also wins inside the ordinary alignment tolerance.
    store = watch.quota_usage_store.Store(work, "fixture")
    store.folder.mkdir(parents=True)
    store.write(store.record, {"finishedAt": 730750})
    now = 1020

    def advance(seconds, _alive):
        nonlocal now
        now += seconds

    with patch.object(watch, "time", SimpleNamespace(time=lambda: now)), \
            patch.object(watch, "app_state", return_value=None), \
            patch.object(watch, "_nap", side_effect=advance), \
            patch.object(watch, "claude_binary", return_value="fixture"), \
            patch.object(type(store), "for_account", return_value=store), \
            patch.object(watch, "read_get_usage") as provider:
        args = SimpleNamespace(usage_root=work)
        reading = watch.shared_get_usage(args, now)
        assert reading["provider_next_at"] == 1030750, reading
        watch._sleep_after_poll(1000, 30, 1015, {}, reading, None, local_due=1030)
        assert now == 1030, now
        watch.shared_get_usage(args, now)
        provider.assert_not_called()


def replay(case, payload, work, engine, fake):
    work.mkdir(parents=True, exist_ok=True)
    historical = case in ("r01", "r02")
    origin = int(time.time())
    renders = [(28, 49), (33, 51), (38, 54), (43, 56),
               (73, 71), (98, 81), (113, 89), (138, 100)]
    reset_min = 220 if case == "r01" else 140
    elapsed = 28 * 60 if historical else 0
    end = 140 * 60 if historical else 1200
    reset = origin + (reset_min * 60 if historical else 7200)
    buf = io.StringIO()
    seen = [0]
    done = None

    def truth(seconds):
        if not historical:
            return int(91 + 4 * seconds / 600)
        minute = seconds / 60
        for (m0, v0), (m1, v1) in zip(renders, renders[1:]):
            if m0 <= minute <= m1:
                return int(v0 + (v1 - v0) * (minute - m0) / (m1 - m0))
        return renders[-1][1]

    def write(value, at):
        payload.write_text(json.dumps({"rate_limits": {"five_hour": {
            "used_percentage": value, "resets_at": reset}}}))
        os.utime(payload, (origin + at, origin + at))

    def lead_reads():
        nonlocal done
        text = buf.getvalue()[seen[0]:]
        seen[0] = len(buf.getvalue())
        threshold = re.search(r"^QUOTA-THRESHOLD (\d+)%", text, re.M)
        if threshold:
            minutes = re.search(r"minutes to reset: (\d+)", text)
            done = ["THRESHOLD", elapsed / 60 if historical else elapsed, int(threshold[1])]
            if historical:
                done.append(int(minutes[1]) if minutes else None)
            raise EndOfReplay
    def sleep(seconds):
        nonlocal elapsed
        if seconds <= 0:
            raise AssertionError("watch requested a nonpositive sleep")
        lead_reads()
        target = min(elapsed + seconds, end)
        if historical:
            for minute, value in renders:
                if elapsed < minute * 60 <= target:
                    write(value, minute * 60)
        elapsed = target
        (fake / "used").write_text(str(truth(elapsed)))
        if elapsed >= end:
            raise EndOfReplay

    # Rebinding this module attribute leaves the transport fixture, subprocess
    # timeouts and registry's real process-generation clock untouched.
    watch.time = SimpleNamespace(time=lambda: origin + elapsed, sleep=sleep)
    os.environ["QUOTA_CLAUDE_BIN"] = str(fake / "claude")
    (fake / "mode").write_text("ok")
    (fake / "used").write_text(str(truth(elapsed)))
    (fake / "reset_in").write_text(str(reset - origin))
    write(49 if historical else 91, elapsed)
    args = SimpleNamespace(threshold=93, threshold_problem="", config="fixture",
                           payload=str(payload), engine_root=str(engine), poll=300,
                           stale=300, until_reset=False, command="quota-watch.sh",
                           fast_poll=60, hold=60, usage_root=work / "usage")
    try:
        rc = watch.poll_loop(args, watch.Out(lines=buf, events=buf))
        raise AssertionError("the poll loop returned (%r) instead of polling on" % (rc,))
    except EndOfReplay:
        pass
    text = buf.getvalue()
    (work / "transcript.log").write_text(text)
    lead_woken = bool(re.search(r"^QUOTA-(STALE|REFRESH|UNKNOWN)", text, re.M))
    if historical:
        return dict(done=done, lead_woken=lead_woken,
                    said_no_pause="less than 20 minutes: no pause" in text)
    return dict(fired_at=done[1] if done else None, value=done[2] if done else None,
                at_hold=truth(done[1] + args.hold) if done else None,
                lead_woken=lead_woken, via_get_usage="via get_usage" in text)


def replay_fallback(payload, work, engine):
    """Replay a live-source outage with two independent status-line renders."""
    work.mkdir(parents=True, exist_ok=True)
    origin = int(time.time())
    elapsed = 0
    lines, events = io.StringIO(), io.StringIO()
    polls, blind = [], []
    threshold_at = None
    renders = [(0, 91), (720, 92), (1500, 93)]

    def render(at, used):
        payload.write_text(json.dumps({"rate_limits": {"five_hour": {
            "used_percentage": used, "resets_at": origin + 7200}}}))
        os.utime(payload, (origin + at, origin + at))

    def sleep(seconds):
        nonlocal elapsed, threshold_at
        polls.append(elapsed)
        text = events.getvalue()
        events.seek(0)
        events.truncate()
        if re.search(r"^QUOTA-(STALE|UNKNOWN)", text, re.M):
            blind.append(elapsed)
        match = re.search(r"^QUOTA-THRESHOLD (\d+)%", text, re.M)
        if match:
            threshold_at = [elapsed, int(match[1])]
            raise EndOfReplay
        for at, used in renders:
            if elapsed < at <= elapsed + seconds:
                render(at, used)
        elapsed += seconds
        if elapsed > 1800:
            raise EndOfReplay

    watch.time = SimpleNamespace(time=lambda: origin + elapsed, sleep=sleep)
    # The suite supplies an unavailable CLI, so this drives the real fallback reader.
    render(*renders[0])
    args = SimpleNamespace(threshold=93, threshold_problem="", config="fixture",
                           payload=str(payload), engine_root=str(engine), poll=300,
                           stale=300, until_reset=False, command="quota-watch.sh",
                           fast_poll=60, hold=60, usage_root=work / "usage")
    try:
        rc = watch.poll_loop(args, watch.Out(lines=lines, events=events))
        raise AssertionError("the poll loop returned (%r) instead of polling on" % (rc,))
    except EndOfReplay:
        pass
    (work / "transcript.log").write_text(lines.getvalue())
    return dict(blind=blind, threshold_at=threshold_at,
                max_gap=max(b - a for a, b in zip(polls, polls[1:])))


# Ruling §108. Seconds after the watcher's poll phase, and the five-hour value.
# r05: 2026-09-29, 15 Fable workers, the lead's transcript (session 30e3850f).
# The watcher's monitor started with the session at 18:28:05Z and polled at
# 18:28:10Z + k * 300 s (its wake came at 18:53:10Z). Readings: 7% at 18:29:11Z,
# 91% at 18:50:15Z, 98% at 18:53:10Z, and 100% at 18:54:16Z (a cap: 100 can be
# reached sooner). Between readings the value is interpolated; after the last
# uncapped one the 91->98 rise goes on, which reaches 100 at about 18:54:00Z,
# before the 18:54:16Z read. The window reset at 21:50:00Z. R05 also shifts the
# watcher's poll phase through every 30 s of its cycle (PHASES): where the
# polls happened to fall that evening is luck, and the rule must not need it.
# r07: the fastest normal rise in every logged pair of polls, 2026-09-26
# 07:04:46Z 6% to 07:10:01Z 10% (4 points in 315 s, session c11805a4), started
# at 80% so that it crosses 93 (the level is invented, the speed is real).
# r09: the same run had it not slowed at the end: its own measured speed from
# 18:29:11Z to 18:50:15Z (84 points in 1264 s, 3.99 points per minute) held to
# 100%. The recorded run slowed to about 2.4 per minute after 91%, where the
# check alone is enough; at 3.99 the early wake is what holds it.
# r10: usage flat at 20% (21% after 15 minutes) while the suite has just
# registered 10 teammates: the expected rise, nothing measured.
# r11: 80% and flat for 4 minutes, then a JUMP to 7 points a minute (invented:
# 1.75 times the 15-worker run, about 26 such workers).
# r12: a burst at 20 points per 5 minutes (the 15-worker run's speed) from 10%
# to 70%, then back to 3 per 5 minutes (normal work's fastest, 3.8, rounded
# down), crossing 93% at normal speed.
SPEED_CASES = {
    "r05": dict(points=[(61, 7), (1325, 91), (1500, 98)], reset=12110, end=1800),
    "r07": dict(points=[(0, 80), (315, 84)], reset=12110, end=1800),
    "r09": dict(points=[(61, 7), (1325, 91)], reset=12110, end=1800),
    "r10": dict(points=[(0, 20), (900, 21)], reset=12110, end=900),
    "r11": dict(points=[(0, 80), (240, 80), (240 + 20 * 60 / 7.0, 100)], reset=12110, end=900),
    "r12": dict(points=[(0, 10), (900, 70), (3200, 93)], reset=12110, end=4200),
}
PHASES = range(0, 300, 15)


_real_read_source = watch.read_source


def replay_speed(case, payload, work, engine, fake, start=0):
    """The real poll loop and get_usage transport on a simulated clock, the
    value from SPEED_CASES. The watcher's first poll is `start` seconds into
    the timeline (its poll phase); every time reported is from that poll. Ends
    at the pause (QUOTA-THRESHOLD) or at `end`."""
    work.mkdir(parents=True, exist_ok=True)
    spec = SPEED_CASES[case]
    pts = spec["points"]
    origin = int(time.time())
    elapsed = start
    buf = io.StringIO()
    seen = [0]
    polls, fast_alerts, done = [], [], None

    def truth(s):
        if s <= pts[0][0]:
            return float(pts[0][1])
        for (t0, v0), (t1, v1) in zip(pts, pts[1:]):
            if s <= t1:
                return v0 + (v1 - v0) * (s - t0) / float(t1 - t0)
        (t0, v0), (t1, v1) = pts[-2], pts[-1]
        return min(100.0, v1 + (v1 - v0) * (s - t1) / float(t1 - t0))

    def feed():
        # The control request sees the same fixture as Claude's status frames.
        # The frame between provider requests can be newer without a second probe.
        used = int(truth(elapsed))
        (fake / "used").write_text(str(used))
        (fake / "reset_in").write_text(str(spec["reset"]))
        payload.write_text(json.dumps({"rate_limits": {"five_hour": {
            "used_percentage": used, "resets_at": origin + spec["reset"]}}}))
        os.utime(payload, (origin + elapsed, origin + elapsed))

    def lead_reads():
        nonlocal done
        text = buf.getvalue()[seen[0]:]
        seen[0] = len(buf.getvalue())
        for m in re.finditer(r"^QUOTA-FAST: usage is rising ([\d.]+) points per 5 minutes", text, re.M):
            fast_alerts.append([elapsed - start, float(m[1])])
        threshold = re.search(r"^QUOTA-THRESHOLD (\d+)%", text, re.M)
        if threshold:
            done = [elapsed - start, int(threshold[1])]
            raise EndOfReplay
        if re.search(r"^QUOTA-(UNKNOWN|STALE|REFRESH)", text, re.M):
            raise AssertionError("unexpected wake-up: %s" % text)

    def sleep(seconds):
        nonlocal elapsed
        if seconds <= 0:
            raise AssertionError("watch requested a nonpositive sleep")
        lead_reads()
        elapsed = min(elapsed + seconds, start + spec["end"])
        feed()
        if elapsed >= start + spec["end"]:
            raise EndOfReplay

    def read_source(a, now):
        polls.append(elapsed - start)
        return _real_read_source(a, now)

    watch.time = SimpleNamespace(time=lambda: origin + elapsed, sleep=sleep)
    watch.read_source = read_source
    os.environ["QUOTA_CLAUDE_BIN"] = str(fake / "claude")
    (fake / "mode").write_text("ok")
    feed()
    args = SimpleNamespace(threshold=93, threshold_problem="", config="fixture",
                           payload=str(payload), engine_root=str(engine), poll=300,
                           stale=300, until_reset=False, command="quota-watch.sh",
                           fast_poll=60, hold=60, usage_root=work / "usage")
    try:
        rc = watch.poll_loop(args, watch.Out(lines=buf, events=buf))
        raise AssertionError("the poll loop returned (%r) instead of polling on" % (rc,))
    except EndOfReplay:
        pass
    text = buf.getvalue()
    (work / "transcript.log").write_text(text)
    gaps = [b - a for a, b in zip(polls, polls[1:])]
    wake_at, value = done if done else (None, None)
    return dict(wake_at=wake_at, value=value, polls=polls, gaps=gaps, fast_alerts=fast_alerts,
                at_hold=round(truth(start + wake_at + args.hold), 1) if done else None,
                early="EARLY, AT HIGH SPEED" in text, via_get_usage="via get_usage" in text)


def replay_outage(case, payload, work, engine, fake):
    work.mkdir(parents=True, exist_ok=True)
    origin = int(time.time())
    end = 660 if case in ("n02", "n07") else 1030
    elapsed = 0
    polls, wire = [], []
    lines, events = io.StringIO(), io.StringIO()
    read_source = watch.read_source
    read_usage = watch.read_get_usage

    def source(args, now):
        polls.append(elapsed)
        return read_source(args, now)

    def usage(now):
        wire.append(elapsed)
        return read_usage(now, deadline_s=1)

    def render(used, age=0):
        payload.write_text(json.dumps({"rate_limits": {"five_hour": {
            "used_percentage": used, "resets_at": origin + 7200}}}))
        os.utime(payload, (origin + elapsed - age, origin + elapsed - age))

    def sleep(seconds):
        nonlocal elapsed
        previous = elapsed
        elapsed += seconds
        if case == "n03" and previous < 360 <= elapsed:
            render(94)
        if elapsed >= end:
            raise EndOfReplay

    render(7, age=1500)
    watch.time = SimpleNamespace(time=lambda: origin + elapsed, sleep=sleep)
    watch.read_source = source
    watch.read_get_usage = usage
    os.environ["QUOTA_CLAUDE_BIN"] = str(fake / "claude")
    args = SimpleNamespace(threshold=93, threshold_problem="", config="fixture",
                           payload=str(payload), engine_root=str(engine), poll=300,
                           stale=60 if case == "n04" else 300, until_reset=False,
                           command="quota-watch.sh", fast_poll=60, hold=60,
                           usage_root=work / "usage")
    try:
        rc = watch.poll_loop(args, watch.Out(lines=lines, events=events))
        raise AssertionError("the poll loop returned (%r) instead of polling on" % (rc,))
    except EndOfReplay:
        pass

    log, notices = lines.getvalue(), events.getvalue()
    (work / "transcript.log").write_text(log + notices)
    assert len(polls) >= 3 and polls[-1] >= end - 300, polls
    assert "no answer to get_usage within 1 s (initialize" in log, log
    if case in ("n02", "n07"):
        assert wire == [0, 300, 600], wire
        assert "get_usage answers again" in log and not notices, log + notices
        if case == "n02":
            assert 360 in polls and "threshold cannot have been reached before" in log, log
    elif case == "n03":
        assert re.search(r"^QUOTA-THRESHOLD 94%", notices, re.M), notices
        assert polls[-1] >= 660, polls
    elif case == "n04":
        assert wire[:3] == [0, 300, 360], wire
        assert re.findall(r"^QUOTA-(FAST|STALE):", notices, re.M) == ["FAST", "STALE"], notices
        assert "at the fastest rise measured this window" in notices, notices
    elif case in ("n05", "n08"):
        assert wire == [0, 300, 600, 900], wire
        assert len(re.findall(r"^QUOTA-STALE:", notices, re.M)) == 1, notices
        if case == "n05":
            assert "a check the rule orders every 5 minutes has been missed" in notices, notices
    return {"checks": wire, "lastPoll": polls[-1], "events": re.findall(r"^QUOTA-([A-Z-]+)", notices, re.M)}


if __name__ == "__main__":
    case, payload, work, engine, fake = sys.argv[1:]
    if case == "local-deadline":
        replay_local_deadline(Path(work))
        raise SystemExit(0)
    if case in ("n02", "n03", "n04", "n05", "n07", "n08"):
        print(json.dumps(replay_outage(case, Path(payload), Path(work), Path(engine), Path(fake))))
        raise SystemExit(0)
    if case in SPEED_CASES:
        out = replay_speed(case, Path(payload), Path(work), Path(engine), Path(fake))
        if case in ("r05", "r09"):  # every 15 s of the 5-minute cycle
            out["phases"] = []
            for start in PHASES:
                d = replay_speed(case, Path(payload), Path(work) / ("phase-%d" % start), Path(engine), Path(fake), start)
                out["phases"].append(dict(start=start, wake_at=d["wake_at"], value=d["value"], at_hold=d["at_hold"],
                                          early=d["early"]))
        print(json.dumps(out))
        raise SystemExit(0)
    if case == "r03":
        print(json.dumps(replay_fallback(Path(payload), Path(work), Path(engine))))
        raise SystemExit(0)
    if case not in ("r01", "r02", "r04"):
        raise SystemExit("unknown replay")
    print(json.dumps(replay(case, Path(payload), Path(work), Path(engine), Path(fake))))
