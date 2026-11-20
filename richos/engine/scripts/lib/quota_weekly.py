"""Weekly quota extension. The five-hour rule remains in quota_watch.

Reset redemption is delegated to the desktop's Rust service. This module cannot
approve a reset. Unknown weekly data never releases a recorded weekly hold.
"""
import datetime
import json
import math
import os
import subprocess
import pause_protocol

THRESHOLD = 99
PREFIX = "the weekly quota reset"


def windows(limits, parse_reset, fallback=False):
    result = []
    if not isinstance(limits, dict):
        return result
    for key, value in limits.items():
        rows = value if isinstance(value, list) else [value]
        for row in rows:
            if not isinstance(row, dict):
                continue
            used = row.get("used_percentage" if fallback else "utilization")
            if isinstance(used, bool) or not isinstance(used, (int, float)) or not math.isfinite(used) or not 0 <= used <= 100:
                continue
            reset = row.get("resets_at")
            reset = int(reset) if fallback and isinstance(reset, (int, float)) and not isinstance(reset, bool) and math.isfinite(reset) else parse_reset(reset)
            name = row.get("display_name")
            label = str(name)[:80] if name else key
            label = "".join(c for c in label if c.isprintable())
            result.append({"id": key if not isinstance(value, list) else "model:" + label,
                           "label": label, "used": used, "resets_at": reset})
    return result


def weekly(reading):
    return next((w for w in reading.get("windows", []) if w["id"] == "seven_day"), None)


def held(reading, now, point=THRESHOLD):
    w = weekly(reading)
    return bool(w and w["used"] >= point)


def fresh_below(reading, now, stale, point=THRESHOLD):
    w = weekly(reading)
    return bool(w and w["used"] < point and w["resets_at"] and w["resets_at"] > now
                and reading.get("state") == "ok" and (reading.get("age") or 0) <= stale
                and not reading.get("provider_refused"))


def describe(reading, now):
    parts = []
    for w in reading.get("windows", []):
        if w["id"] == "five_hour":
            continue
        reset = datetime.datetime.fromtimestamp(w["resets_at"], datetime.timezone.utc).isoformat() if w["resets_at"] else "unknown"
        parts.append("%s: %g%%, resets %s" % (w["label"], w["used"], reset))
    return "; ".join(parts) or "weekly quota: UNKNOWN (not reported)"


APP_RUNS_RESET = "the RichOS app runs any user-approved reset itself; this watcher does not tick it"


def reset_tick(engine, reading=None, at_limit=False):
    # An app watcher may run on an added account. Only the desktop monitor can
    # check or redeem against Account 1, so watchers use its published answer.
    if (os.environ.get("RICHOS_APP_STATE") or "").strip():
        saved = (reading or {}).get("reset_offers")
        if isinstance(saved, dict):
            return {"resets": saved, "status": saved.get("status"),
                    "actionError": saved.get("message") if saved.get("state") == "unknown" else None}
        return {"status": APP_RUNS_RESET}
    helper = os.environ.get("QUOTA_RESET_HELPER") or os.path.join(engine, "scripts", "quota-reset.sh")
    env = dict(os.environ, RICHOS_QUOTA_WEEKLY_LIMIT="1" if at_limit else "0")
    if at_limit and (week := weekly(reading or {})):
        env["RICHOS_QUOTA_WEEKLY_USED"] = str(week["used"])
    else:
        env.pop("RICHOS_QUOTA_WEEKLY_USED", None)
    try:
        result = subprocess.run([helper, "tick"], env=env, capture_output=True, text=True, timeout=120)
        saved = json.loads(result.stdout) if result.returncode == 0 else None
        if isinstance(saved, dict):
            return saved
    except (OSError, ValueError, subprocess.TimeoutExpired):
        pass
    return {"error": "Reset helper unavailable; weekly pause still applies."}


def reset_line(reset_status):
    return (reset_status.get("error") or reset_status.get("actionError") or reset_status.get("status")
            or "no successful reset has produced available weekly quota")


def reset_notice_key(reset_status):
    # The age changes each minute, but only a change of answer (or a refusal
    # appearing/clearing) should wake a waiting lead.
    return reset_line(reset_status).rsplit(", checked ", 1)[0]


def handle(args, reading, now, workers, reset_status, five_verdict, cached_notice=None, *, point=THRESHOLD):
    paused = workers.get("weekly_paused", [])
    if held(reading, now, point):
        w = weekly(reading)
        names = workers["working"] + workers.get("quota_paused", [])
        if names or not workers["known"]:
            print("WEEKLY-QUOTA-THRESHOLD: overall weekly usage reached %g%%; pause until allowance is confirmed." % point)
            print("  " + describe(reading, now))
            print("  Reset status: " + reset_line(reset_status))
            if cached_notice:
                print("  " + cached_notice)
            print("  Send the standard message unchanged to: " + (", ".join(names) or "every working agent"))
            reset = datetime.datetime.fromtimestamp(w["resets_at"], datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ") if w["resets_at"] and w["resets_at"] > now else None
            print("  First, for each agent, run: python3 %s --to <agent> --reason weekly-quota%s"
                  % (os.path.abspath(pause_protocol.__file__), " --reset " + reset if reset else ""))
            print("  It freezes that agent before any message and prints this same message to send.")
            print(pause_protocol.render("weekly-quota", reset))
            print("  Summary: " + pause_protocol.SUMMARY)
            print("  The watcher keeps polling; a five-hour reset does not release this weekly hold.")
            return True, True
        return True, False
    if paused:
        if fresh_below(reading, now, args.stale, point) and five_verdict() in ("below", "near-reset"):
            reset = weekly(reading)["resets_at"]
            old_resets = workers.get("weekly_pause_resets", {})
            resumable = [worker for worker in paused if not old_resets.get(worker)
                         or abs(old_resets[worker] - reset) >= 60 or reading.get("weekly_allowance_restored")]
            if not resumable:
                return True, False
            print("WEEKLY-QUOTA-RELEASE: fresh weekly allowance and five-hour policy both permit work.")
            print("  Send to: " + ", ".join(resumable))
            print("  First, for each agent, run: python3 %s --resume --to <agent> --reason weekly-quota"
                  % os.path.abspath(pause_protocol.__file__))
            print("  It releases that agent's hold before any message and prints this same message to send.")
            # The generated RESUME, unchanged: the workspace message handler releases a
            # controlled hold on that message and on nothing else.
            print(pause_protocol.render_resume("weekly-quota"))
            print("  The watcher keeps polling.")
            return False, True
        return True, False
    return False, False
