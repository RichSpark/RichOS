#!/usr/bin/env python3
"""A failed main with a passing control is measured once more on the same phone before it is called a
code regression (CEO 2026-10-04). Stand-in measurements; no phone is touched."""
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import watch  # noqa: E402

fails = []


def check(name, cond):
    if not cond:
        fails.append(name)
        print(f"FAIL {name}")


def judged(verdict, tag):
    return {"data": {}, "info": "i", "blanks": [], "failed": ["cold FAIL"] if verdict == "slower" else [],
            "verdict": verdict, "report": f"{tag} {verdict}"}


def run(second, busy_again=False):
    """first main: slower; control: good; second main: `second`."""
    esc, good, meas, calls = [], [], [], []
    judges = iter([judged("slower", "first"), judged("good", "control"), judged(second, "second")])
    free = iter([(True, 0), (False, [(1, "other")]) if busy_again else (True, 0)])
    watch.Guard = lambda work: None
    watch.last_good_base = lambda *a: "b" * 40
    watch.range_text = lambda *a: "span"
    watch.mobile_commits = lambda *a: []
    watch.last_measurement = lambda *a: None
    watch.find_phone = lambda *a: ("phone", ["id"])
    watch.wait_until_free = lambda *a: next(free)
    watch.max_wait_s = lambda: 5
    watch.measure = lambda platform, checkout, sha, phone, guard, log, work, tag="": calls.append(tag) or f"/r{tag}.json"
    watch.judge_record = lambda *a: next(judges)
    watch.prepare_checkout = lambda *a, **k: "control-dir"
    watch.verb = lambda *a, **k: None
    watch.blank_screen_status = lambda *a: "ok"
    watch.set_good = lambda p, e: good.append(e)
    watch.set_measured = lambda p, e: meas.append(e)
    watch.escalate = lambda repo, title, *a: esc.append(title)
    watch.record = lambda *a, **k: None
    with tempfile.TemporaryDirectory() as d:
        out = watch.run_platform("repo", "r1", "a" * 40, "android", d, "checkout")
    return out, esc, good, meas, calls


out, esc, good, meas, calls = run("good")
check("pass again: good", out["verdict"] == "good")
check("pass again: no escalation", not esc)
check("pass again: ledger good, first failure kept", len(good) == 1 and meas[-1]["firstFailure"] == "first slower")

out, esc, good, meas, calls = run("slower")
check("fail again: escalated as a code regression", len(esc) == 1 and "code regression" in esc[0])
check("fail again: verdict slower", out["verdict"] == "slower" and not good)

out, esc, good, meas, calls = run("good", busy_again=True)
check("busy: could not tell", "could not tell: the phone was busy" in out["diagnosis"])
check("busy: never a code regression", len(esc) == 1 and "code regression" not in esc[0] and not good)
check("busy: no second measurement made", "-again" not in calls)

# A last measurement that FAILED, with no app code changed since, is measured again, not re-judged.
out, esc, good, meas, calls = run("good")  # installs the stand-ins
for failed_last, expect_measure in (("slower", True), ("good", False)):
    calls.clear()
    judges = iter([judged("good", "x")])
    watch.last_measurement = lambda *a, v=failed_last: {"commit": "c" * 40, "record": "/old.json", "verdict": v}
    watch.app_changed = lambda *a: False
    watch.judge_record = lambda *a: next(judges)
    watch.wait_until_free = lambda *a: (True, 0)
    with tempfile.TemporaryDirectory() as d:
        watch.run_platform("repo", "r2", "a" * 40, "android", d, "checkout")
    check(f"last {failed_last}, no app change: phone measured={expect_measure}", bool(calls) == expect_measure)

if fails:
    sys.exit(1)
print("ok")
