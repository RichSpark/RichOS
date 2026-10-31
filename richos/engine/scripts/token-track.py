#!/usr/bin/env python3
"""Tokens used per percentage point of the Claude quota, per account.

  token-track.py sample   [--account LABEL=DIR ...] [--accounts-file F]
  token-track.py report   [--account LABEL=DIR ...] [--accounts-file F]

`sample` reads each account's five-hour and weekly percentages (the same
get_usage read quota_watch.py uses, with CLAUDE_CONFIG_DIR set to the account's
folder) and appends one row per account to <state>/readings.jsonl. Run it every
few minutes (the weekly-switch test rounds run it before and after a job).

`report` joins those rows with the token counts in each account's Claude Code
transcripts (<folder>/projects/**/*.jsonl: input, output, cache write, cache
read, by model) and prints, per account, how many tokens by kind raised the
weekly and the five-hour percentage by one point, and how many rises that is
based on. Between two consecutive readings of the same window the tokens used
are summed; the answer is total tokens / total points over those pairs, so
percentage rounding averages out. A window that reset between two readings is
skipped.

Run once a minute by token-track-launchd.sh (a per-user launchd agent that needs no session). From
the per-minute readings `report` finds each percentage value's start (the first reading showing it,
after a lower one) and end (the first reading showing a higher one), and counts the tokens between
those two edges; the last five such values are listed per window.

The default account is ~/.claude. State: ~/.claude/state/token-track/
(override with TOKEN_TRACK_STATE). Nothing is written into a repository.
"""
import argparse
import datetime
import glob
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
KINDS = ("input", "output", "cache_write", "cache_read")
FIELDS = {"input": "input_tokens", "output": "output_tokens",
          "cache_write": "cache_creation_input_tokens", "cache_read": "cache_read_input_tokens"}


def state_dir():
    return os.environ.get("TOKEN_TRACK_STATE") or os.path.join(os.path.expanduser("~"), ".claude", "state", "token-track")


def accounts(args):
    result = {}
    if not args.account and not args.accounts_file:
        result["default"] = os.path.join(os.path.expanduser("~"), ".claude")
    for spec in args.account or []:
        label, _, folder = spec.partition("=")
        result[label] = os.path.expanduser(folder)
    if args.accounts_file:
        with open(args.accounts_file) as fh:
            for a in json.load(fh).get("accounts", []):
                result[str(a.get("label") or a.get("id"))] = os.path.expanduser(a.get("folder") or "~/.claude")
    return result


def parse_ts(text):
    return datetime.datetime.fromisoformat(text.replace("Z", "+00:00")).timestamp()


def read_tokens(folder):
    """[(epoch, model, {kind: n})] from every transcript under folder/projects.
    A streamed message appears several times with the same id; the last (largest) copy wins."""
    last = {}
    for path in glob.glob(os.path.join(folder, "projects", "**", "*.jsonl"), recursive=True):
        try:
            fh = open(path, encoding="utf-8", errors="replace")
        except OSError:
            continue
        with fh:
            for line in fh:
                if '"usage"' not in line:
                    continue
                try:
                    row = json.loads(line)
                    msg = row["message"]
                    usage = msg["usage"]
                    at = parse_ts(row["timestamp"])
                except (ValueError, KeyError, TypeError):
                    continue
                if not isinstance(usage, dict):
                    continue
                counts = {k: int(usage.get(f) or 0) for k, f in FIELDS.items()}
                key = msg.get("id") or row.get("uuid") or (path, at)
                old = last.get(key)
                if old is None or counts["output"] >= old[2]["output"]:
                    last[key] = (at, msg.get("model") or "unknown", counts)
    return sorted(last.values(), key=lambda t: t[0])


def window(reading, which):
    """(percent, resets_at) of 'five_hour' or 'seven_day' from a row."""
    w = reading.get(which)
    return (w["used"], w["resets_at"]) if w else (None, None)


def episodes(readings, which):
    """Each percentage value as the readings show it: [(value, start, end, next_value)].
    start = time of the first reading showing the value, taken only when the reading before it
    (same window) showed a lower value, so the rise itself was seen; end = time of the first reading
    showing a higher value in the same window. A value still current, or whose rise was not seen
    (the first reading, a window reset), is not returned."""
    rows = sorted((r for r in readings if r.get(which)), key=lambda r: r["at"])
    out = []
    cur = None  # [value, start, resets_at]
    for r in rows:
        used, resets = window(r, which)
        if cur is not None and abs(resets - cur[2]) > 120:
            cur = None  # the window reset between two readings
        if cur is None:
            cur = [used, None, resets]
        elif used > cur[0]:
            if cur[1] is not None:
                out.append((cur[0], cur[1], r["at"], used))
            cur = [used, r["at"], resets]
        elif used < cur[0]:
            cur = [used, None, resets]
    return out


def per_point(readings, tokens, which):
    total = {k: 0 for k in KINDS}
    by_model = {}
    points = 0.0
    eps = episodes(readings, which)
    for value, start, end, nxt in eps:
        for at, model, counts in tokens:
            if start < at <= end:
                for k in KINDS:
                    total[k] += counts[k]
                    by_model.setdefault(model, {x: 0 for x in KINDS})[k] += counts[k]
        points += nxt - value
    return {"points": points, "rises": len(eps), "tokens": total, "by_model": by_model, "episodes": eps,
            "per_point": {k: round(total[k] / points) for k in KINDS} if points else None}


def iso(epoch):
    return datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def load_readings(path):
    rows = []
    try:
        with open(path) as fh:
            for line in fh:
                try:
                    rows.append(json.loads(line))
                except ValueError:
                    pass
    except OSError:
        pass
    return rows


def cmd_report(args):
    readings = load_readings(os.path.join(state_dir(), "readings.jsonl"))
    for label, folder in accounts(args).items():
        mine = [r for r in readings if r.get("account") == label]
        tokens = read_tokens(folder)
        print("account %s (%s): %d readings, %d token rows" % (label, folder, len(mine), len(tokens)))
        for which, name in (("seven_day", "weekly"), ("five_hour", "five-hour")):
            r = per_point(mine, tokens, which)
            if not r["per_point"]:
                print("  %s: no rise measured yet (%d readings of it)" % (name, sum(1 for x in mine if x.get(which))))
                continue
            p = r["per_point"]
            print("  %s: 1 point = %s tokens (input %d, output %d, cache write %d, cache read %d); "
                  "%g points over %d rises" % (name, format(sum(p.values()), ","), p["input"], p["output"],
                                               p["cache_write"], p["cache_read"], r["points"], r["rises"]))
            for value, start, end, nxt in r["episodes"][-5:]:
                print("    %g%% from %s to %s (%d s)" % (value, iso(start), iso(end), end - start))
            for model, c in sorted(r["by_model"].items()):
                print("    %s: %s" % (model, ", ".join("%s %s" % (k, format(c[k], ",")) for k in KINDS)))
    return 0


def cmd_sample(args):
    sys.path.insert(0, os.path.join(HERE, "lib"))
    import quota_watch
    os.makedirs(state_dir(), exist_ok=True)
    status = 0
    for label, folder in accounts(args).items():
        old = os.environ.get("CLAUDE_CONFIG_DIR")
        os.environ["CLAUDE_CONFIG_DIR"] = folder
        try:
            now = int(time.time())
            r = quota_watch.read_get_usage(now)
        finally:
            if old is None:
                os.environ.pop("CLAUDE_CONFIG_DIR", None)
            else:
                os.environ["CLAUDE_CONFIG_DIR"] = old
        # An idle account answers with five_hour resets_at null (no window open): that is no five-hour
        # reading, but its weekly window is still a reading. Only a missing answer is a failure.
        row = {"at": now, "account": label}
        for w in r.get("windows", []):
            key = {"five_hour": "five_hour", "seven_day": "seven_day"}.get(w["id"])
            if key and w["resets_at"]:
                row[key] = {"used": w["used"], "resets_at": w["resets_at"]}
        if "five_hour" not in row and r["state"] == "ok" and r["resets_at"]:
            row["five_hour"] = {"used": r["used"], "resets_at": r["resets_at"]}
        if "five_hour" not in row and "seven_day" not in row:
            print("account %s: no reading (%s)" % (label, r["why"]), file=sys.stderr)
            status = 1
            continue
        with open(os.path.join(state_dir(), "readings.jsonl"), "a") as fh:
            fh.write(json.dumps(row) + "\n")
        # An account with no five-hour limit (quota_watch.no_five_hour) shows none, not "idle".
        five = "" if r.get("five") is False else "five-hour %s, " % (
            ("%g%%" % row["five_hour"]["used"]) if "five_hour" in row else "idle")
        print("account %s: %sweekly %s" % (label, five, ("%g%%" % row["seven_day"]["used"]) if "seven_day" in row else "unknown"))
    return status


def main(argv):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("command", choices=("sample", "report"))
    p.add_argument("--account", action="append", metavar="LABEL=DIR", help="a Claude config folder; default ~/.claude")
    p.add_argument("--accounts-file", help="the app's claude-accounts.json")
    args = p.parse_args(argv)
    return {"sample": cmd_sample, "report": cmd_report}[args.command](args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
