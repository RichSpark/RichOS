#!/usr/bin/env python3
"""proof-run.test.py — the land runner runs everything it was given, at the same time where
that is safe, one at a time where it is not, and never reports green over a check that did
not pass. Fixture commands only (sleep, exit codes, files); nothing here builds or boots.
"""
import importlib.util
import json
import os
import signal
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SPEC = importlib.util.spec_from_file_location("proof_run", os.path.join(HERE, "proof-run.py"))
pr = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pr)
pr.SETTLE_SECONDS = 0

PASSED = FAILED = 0


def check(ok, what, detail=""):
    global PASSED, FAILED
    if ok:
        PASSED += 1
        print("  ok    %s" % what)
    else:
        FAILED += 1
        print("  FAIL  %s" % what)
        if detail:
            print("        %s" % str(detail)[:600])


class Args:
    def __init__(self, **kw):
        self.capacity, self.engine_shards, self.admission_wait = 4, 4, 1800
        self.max_cpu, self.budget, self.deadline, self.sample_every = 80, 600, 1800, 0.5
        self.__dict__.update(kw)


def idle():
    return {"cpu_user_percent": 5.0, "cpu_system_percent": 2.0, "cpu_idle_percent": 93.0, "swapout_mb_per_s": 0.0,
            "memory_pressure": "normal", "memory_free_percent": 80, "swap_used_mb": 0.0}


def busy():
    s = idle()
    s["cpu_user_percent"] = 95.0
    return s


def items_from(lines, tmp, **kw):
    return pr.plan(lines, Args(**kw), tmp, {})


tmp = tempfile.mkdtemp(prefix="proof-run-test.")
os.environ["RICHOS_MACHINE_WORKERS"] = os.path.join(tmp, "machine")
# A stalled admission is recorded for the lead's turn-end gate (engine resource_waits.py); a
# test never writes one where the operator's gate reads.
os.environ["RICHOS_WAITS_DIR"] = os.path.join(tmp, "waits")
# The host-wide proof-run slots (lib/proof_slots.py): a test's runs never take the Mac's real ones.
os.environ["RICHOS_PROOF_RUN_SLOTS_DIR"] = os.path.join(tmp, "slots")
try:
    print("=== proof-run ===")

    # P1 — the plan: every family becomes checks, nothing is dropped except a cargo filter a
    # shorter filter on the same target already runs.
    lines = [
        "  cd richos/app/src-tauri && cargo test --bin richos-tauri phone::",
        "  cd richos/app/src-tauri && cargo test --bin richos-tauri phone::rows::",
        "  cd richos/app && cargo test -p richos-core --lib sessions::",
        "  cd richos/web/web-app && node --test test/api.test.js test/scroll.test.js",
        "  cd richos/app && scripts/run-tests.sh --no-host-screen --only lint.test.sh --only native-android-app.test.sh --only native-android-core.test.sh",
        "  cd richos/engine && bash scripts/ci-shard.sh --only-units scripts/stop.test.sh",
        "  cd richos/engine && bash scripts/ci-shard.sh --only-units scripts/spawn.test.sh",
    ]
    its = items_from(lines, tmp)
    labels = sorted(i.label for i in its)
    cargo = [i for i in its if i.lane == "cargo"]
    check(sorted(i.label for i in cargo) == ["cargo --bin richos-tauri phone::", "cargo -p richos-core --lib sessions::"],
          "P1a cargo: phone::rows:: is dropped only because phone:: on the same target runs it; sessions:: stays",
          [i.label for i in cargo])
    suites = [i for i in its if i.argv[:1] == ["scripts/run-tests.sh"]]
    check(sorted(i.label for i in suites) == ["lint", "native-android-app", "native-android-core"]
          and all(i.argv[1] == "--no-host-screen" and i.argv[-2] == "--only" for i in suites),
          "P1b run-tests.sh --only a --only b becomes one check per suite, flags kept", [i.argv for i in suites])
    gradle = sorted(i.label for i in its if i.lane == "gradle")
    check(gradle == ["native-android-app", "native-android-core"],
          "P1c the suites that drive bin/randroid share the gradle lane (derived from the suite, not a list)", gradle)
    eng = [i for i in its if i.label.startswith("engine ")]
    shards = [i for i in eng if i.label != "engine receipts"]
    recv = [i for i in eng if i.label == "engine receipts"]
    with open(os.path.join(tmp, "engine-units.txt")) as fh:
        units = fh.read().split()
    check(units == ["scripts/spawn.test.sh", "scripts/stop.test.sh"] and shards
          and all("--only-units" in i.argv and "--receipt" in i.argv for i in shards)
          and {i.lane for i in shards} == {"engine-shard-1", "engine-shard-2"},
          "P1d engine units retain the packer's shard lanes and acquire admission per unit",
          [i.argv for i in shards])
    check(len(recv) == 1 and recv[0].after == {i.label for i in shards} and "--verify-receipts" in recv[0].argv,
          "P1e and a receipts check runs after every shard, proving the union of units run is the set selected")
    check("web-app" in labels, "P1f every other command runs as printed", labels)
    # P1g: two checks never share a label. A dir/test/run-tests.sh harness and the run-tests.test.sh
    # suite were both "run-tests", so one identity served both and a merge refused the first as
    # INVALID (inputs changed during the check) though the tree never changed.
    its = items_from([
        "cd richos/app && bash scripts/testvm/test/run-tests.sh",
        "cd richos/app && scripts/run-tests.sh --only run-tests.test.sh",
    ], tmp)
    got = sorted(i.label for i in its)
    check(got == ["run-tests", "testvm"],
          "P1g a dir/test/run-tests.sh is labeled by its directory and never collides with the run-tests suite", got)
    # P1h: two crates with a same-named module are two checks under two labels. Both were
    # "cargo dictation::" (the filter alone), so the evidence record refused the plan as one
    # obligation twice and the merge of cc/echo-opus-dict1 crashed before running anything
    # (2026-10-08). The label carries the target, so the record accepts the plan.
    its = items_from([
        "cd richos/app && cargo test -p richos-voice --lib dictation::",
        "cd richos/app/src-tauri && cargo test --bin richos-tauri dictation::",
    ], tmp)
    got = sorted(i.label for i in its)
    try:
        pr.proof_evidence.refuse_duplicates(its)
        refused = ""
    except ValueError as error:
        refused = str(error)
    check(len(its) == 2 and len(set(got)) == 2 and not refused,
          "P1h one module name in two crates is two checks with two labels, and the plan is accepted",
          "%s %s" % (got, refused))
    # P1i: the land's own `lint.sh --changed` (autocheck land_check, about a second when little
    # changed) and the lint.test.sh suite (`lint.sh --all`, Tauri Clippy, about 100 s) were both
    # "lint", so their times shared one weights.tsv row. On 2026-10-15 the main checkout's row read
    # 1.0,98.3,82.6,1.7,96.5,0.9,0.9,78.2,0.9, median 1.7; the commit gate took lint.test.sh for a
    # fast check, capped it at 120 s and refused every richos/app/ui commit. Two labels, two rows.
    its = items_from([
        "cd richos/app && bash scripts/lint.sh --changed",
        "cd richos/app && scripts/run-tests.sh --only lint.test.sh",
    ], tmp)
    got = sorted(i.label for i in its)
    d1i = os.path.join(tmp, "p1i")
    os.makedirs(d1i)
    with open(os.path.join(d1i, "weights.tsv"), "w") as fh:
        fh.write("lint\t98.3\t98.3,82.6,96.5\n")
    for it in its:
        it.state, it.started = "passed", 1.0
        it.ended = it.started + (0.9 if it.argv[:2] == ["bash", "scripts/lint.sh"] else 101.0)
    pr.record_weights(d1i, its)
    suite_samples = pr.history_samples(d1i).get("lint")
    check(len(set(got)) == 2 and suite_samples == [98.3, 82.6, 96.5, 101.0],
          "P1i the land's lint.sh --changed is not the lint suite: its second never enters the suite's times",
          "%s %s" % (got, suite_samples))

    # P2 — concurrency: four checks run together, not one after another. Proved by a rendezvous,
    # never by a wall-clock bound a loaded host can miss: each check announces itself and waits
    # until all four have; one after another they could never meet and give up after 120 s.
    d2 = os.path.join(tmp, "p2")
    os.makedirs(d2)
    meet = ("touch %s/$$; for i in $(seq 1 1200); do [ $(ls %s | wc -l) -ge 4 ] && exit 0; sleep 0.1; done; exit 1"
            % (d2, d2))
    lines = ["cd richos/app && bash -c '%s'" % meet] * 4
    its = items_from(lines, tmp)
    wall = pr.run(its, Args(capacity=4), tmp, sampler=idle)
    check(all(i.state == "passed" for i in its),
          "P2 four checks with a capacity of 4 all ran at the same time (they met; %.1f s)" % wall,
          [(i.label, i.state) for i in its])

    # P3 — a lane is one at a time.
    probe = os.path.join(tmp, "lane")
    os.makedirs(probe, exist_ok=True)
    script = ("d=%s; if ls $d/busy.* >/dev/null 2>&1; then touch $d/overlap; fi; "
              "touch $d/busy.$$; sleep 1; rm -f $d/busy.$$" % probe)
    its = [pr.Item("g%d" % n, os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", script], "gradle", 1.0)
           for n in range(3)]
    wall = pr.run(its, Args(capacity=4), tmp, sampler=idle)
    check(not os.path.exists(os.path.join(probe, "overlap")) and wall >= 2.8,
          "P3 three checks in one lane never overlap (%.1f s for three 1 s checks)" % wall)

    # P4 — a failure is named and the exit is non-zero.
    its = items_from(["cd richos/app && bash -c 'exit 0'", "cd richos/app && bash -c 'echo broken; exit 3'"], tmp)
    its[1].label = "the-broken-one"
    pr.run(its, Args(), tmp, sampler=idle)
    rc = pr.summarize(its, 1.0, tmp)
    with open(os.path.join(tmp, "summary.json")) as fh:
        summary = json.load(fh)
    check(rc == 1 and its[1].state == "failed" and its[1].rc == 3
          and any(c["check"] == "the-broken-one" and c["result"] == "failed" for c in summary["checks"]),
          "P4 a check that exits 3 fails the run by name, in the summary and its JSON")
    check(open(its[1].log).read().splitlines()[-1] == "broken",
          "P4b and its output is in its own log", its[1].log)

    # P5 — admission: a busy machine is waited out, never ignored, and the wait has an end.
    its = items_from(["cd richos/app && bash -c 'exit 0'"], tmp)
    pr.run(its, Args(admission_wait=0), tmp, sampler=busy)
    check(its[0].state == "not-admitted" and its[0].started is None,
          "P5a at 95% user CPU a check is not started, and after its wait it is NOT-ADMITTED (a failure)",
          (its[0].state, its[0].notes))
    # P5c — waiting behind this run's OWN checks is a queue, never a refusal (2026-09-29, part-2
    # hunt section 01: checks were NOT ADMITTED after ~1,830 s of waiting, partly behind their own
    # siblings). One worker, an idle Mac, an admission limit far below the sibling's run time:
    # the second check waits for the worker, is never refused for it, and then runs; its queue
    # time is reported apart from the time the Mac refused it (none).
    app = os.path.join(pr.ROOT, "richos/app")
    its = [pr.Item("holds-the-worker", app, ["bash", "-c", "sleep 1.5"], None, 2.0),
           pr.Item("queued-behind-it", app, ["bash", "-c", "exit 0"], None, 1.0)]
    d5c = os.path.join(tmp, "p5c")
    import io
    import contextlib
    with contextlib.redirect_stdout(io.StringIO()):
        pr.run(its, Args(capacity=1, admission_wait=0.3), d5c, sampler=idle)
        pr.summarize(its, 1.0, d5c)
    with open(os.path.join(d5c, "summary.json")) as fh:
        row5c = {c["check"]: c for c in json.load(fh)["checks"]}.get("queued-behind-it", {})
    check([i.state for i in its] == ["passed", "passed"] and its[1].started >= its[0].ended
          and row5c.get("total_queue_seconds", 0) >= 1.0 and row5c.get("admission_refused_seconds") == 0
          and row5c.get("wait_seconds_by_reason", {}).get("worker", 0) > 0.3,
          "P5c a check queued behind its own run's worker for longer than --admission-wait is never refused "
          "for it: it runs, queue time and refused time reported apart",
          ([(i.label, i.state, i.notes) for i in its], row5c))
    r = subprocess.run([sys.executable, os.path.join(HERE, "proof-run.py"), "--low-priority", "--dry-run",
                        "--log-dir", os.path.join(tmp, "p5b"), "--paths", "richos/app/scripts/proof-run.py"],
                       capture_output=True, text=True)
    check(r.returncode != 0 and "--low-priority" in r.stderr,
          "P5b there is no switch that skips the CPU line: --low-priority is refused, not honored",
          (r.returncode, r.stderr[-200:]))

    # P11 — the deadline is enforced WHILE the run goes: named at the budget, stopped with its
    # whole tree at the deadline (a TERM-ignoring child and an own-session grandchild included),
    # and the run is red by name.
    d11 = os.path.join(tmp, "p11")
    os.makedirs(d11)
    script = ("python3 -c 'import subprocess, sys; p = subprocess.Popen([\"sleep\", \"60\"], start_new_session=True); "
              "open(sys.argv[1], \"w\").write(str(p.pid)); p.wait()' %s/own & "
              "bash -c 'trap \"\" TERM; echo $$ > %s/deaf; while :; do sleep 1; done' & "
              "sleep 60" % (d11, d11))
    it = pr.Item("runs-forever", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", script], None, 0.1)
    import io
    import contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        wall = pr.run([it], Args(budget=1, deadline=3), os.path.join(tmp, "p11log"), sampler=idle)
    pids = [int(open(os.path.join(d11, f)).read()) for f in ("own", "deaf") if os.path.exists(os.path.join(d11, f))]
    alive = []
    for p in pids + [it.proc.pid]:
        try:
            os.kill(p, 0)
            alive.append(p)
            os.kill(p, signal.SIGKILL)
        except ProcessLookupError:
            pass
    check("OVER BUDGET" in buf.getvalue() and it.state == "timed-out" and wall < 20,
          "P11 a check past its budget is named while running, and at its deadline it is TIMED-OUT (%.0f s)" % wall,
          (it.state, buf.getvalue()[-300:]))
    check(len(pids) == 2 and not alive,
          "P11b nothing of the timed-out check survives: its shell, a TERM-ignoring child, an own-session grandchild",
          (pids, alive))

    # P12 — the worker budget counts NESTED workers. Three checks, each with three workers that
    # follow the mutation pool's contract: a worker runs on the check's own free slot when no
    # other worker of that check holds it, otherwise on a token of the run's budget. With a
    # capacity of three, no more than three workers ever work at once, and none is stuck: the
    # first version of this case had no free slot and deadlocked once the checks held every
    # token (found by the branch's own proof run, 2026-09-23).
    d12 = os.path.join(tmp, "p12")
    os.makedirs(d12)
    # Quoted: a worktree the app makes lives under "Application Support", and the bare path split
    # at the space, so P12 and P13 failed in every app worktree and passed everywhere else.
    wt = __import__("shlex").quote(os.path.join(pr.ROOT, "richos/engine/scripts/lib/worker_tokens.py"))
    work = ("touch %s/w.$$; ls %s/w.* | wc -l >> %s/conc; sleep 0.6; rm -f %s/w.$$" % (d12, d12, d12, d12))
    nested = ("for i in 1 2 3; do python3 %s run \"$RICHOS_WORKER_TOKENS\" --free %s/free.$$ -- bash -c '%s' & done; wait"
              % (wt, d12, work.replace("'", "")))
    its = [pr.Item("nested-%d" % n, os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", nested], None, 1.0)
           for n in range(3)]
    pr.run(its, Args(capacity=3), os.path.join(tmp, "p12log"), sampler=idle)
    conc = [int(x) for x in open(os.path.join(d12, "conc")).read().split()] if os.path.exists(os.path.join(d12, "conc")) else []
    check(all(i.state == "passed" for i in its) and len(conc) == 9 and 2 <= max(conc) <= 3,
          "P12 capacity 3, three checks with three workers each: all nine ran, at most %s at once" % (max(conc) if conc else "?"),
          (conc, [(i.label, i.state) for i in its]))

    # P13 — checks not yet started are never starved by nested workers. A runs twelve nested
    # workers, as many at a time as the budget lets it, for several seconds; four light checks
    # queued behind it must all run to the end while A's workers are still going, on the token
    # nested workers may never take (worker_tokens.py's reserve). In the third full run
    # native-ios-share waited 2519 s to start behind four mutation pools.
    d13 = os.path.join(tmp, "p13")
    os.makedirs(d13)
    nested13 = ("for i in $(seq 1 12); do python3 %s run \"$RICHOS_WORKER_TOKENS\" --free %s/free.$$ -- sleep 1 & done; wait"
                % (wt, d13))
    a = pr.Item("A", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", nested13], None, 30)
    light = [pr.Item("light-%d" % k, os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "sleep 0.3"], None, 1)
             for k in range(4)]
    pr.run([a] + light, Args(capacity=4), os.path.join(tmp, "p13log"), sampler=idle)
    check(all(i.state == "passed" for i in [a] + light) and max(i.ended for i in light) < a.ended,
          "P13 four light checks all ran while a nested pool kept the rest of the budget busy "
          "(last one ended %.1f s before the pool's check)" % (a.ended - max(i.ended for i in light)),
          [(i.label, i.state) for i in [a] + light])

    # P14 — a check whose command cannot even start fails by name, and the run goes on. The third
    # full run crashed on `cargo` not being found and left two other checks running under init.
    gone = pr.Item("no-such-tool", os.path.join(pr.ROOT, "richos/app"), ["no-such-tool-7c1f", "--x"], None, 5)
    fine = pr.Item("fine", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "sleep 0.5"], None, 1)
    pr.run([gone, fine], Args(), os.path.join(tmp, "p14log"), sampler=idle)
    check(gone.state == "failed" and fine.state == "passed" and any("could not start" in n for n in gone.notes),
          "P14 a command that cannot start is a FAILED check, named, and the run carries on",
          (gone.state, gone.notes, fine.state))

    # P17 — a check that did not run is never recorded as passed. In the refused land of
    # 6ef73abf (2026-09-29) front-door and gui-boot were `passed` while their logs said "0 of 1
    # suites passed — 1 NOT RUN (no screen)". Here: the REAL run-tests.sh and the real
    # front-door suite with the no-screen flag and no guest named, which opens nothing and
    # records the suite NOT RUN in about a second, beside a check that really passes.
    import io
    import contextlib
    d17 = os.path.join(tmp, "p17")
    os.makedirs(d17)
    saved_gui = os.environ.pop("RICHOS_GUI_HOST", None)
    buf = io.StringIO()
    door_line = "cd richos/app && scripts/run-tests.sh %s --only %s" % ("--no-host-screen", "front-door.test.sh")
    try:
        its = items_from([door_line, "cd richos/app && bash -c 'exit 0'"], d17)
        with contextlib.redirect_stdout(buf):
            pr.run(its, Args(), d17, sampler=idle)
            rc = pr.summarize(its, 1.0, d17)
    finally:
        if saved_gui is not None:
            os.environ["RICHOS_GUI_HOST"] = saved_gui
    door = [i for i in its if i.label == "front-door"]
    with open(os.path.join(d17, "summary.json")) as fh:
        rows = {c["check"]: c for c in json.load(fh)["checks"]}
    with open(os.path.join(d17, "progress.json")) as fh:
        progress = {c["check"]: c for c in json.load(fh)}
    out = buf.getvalue()
    check(len(door) == 1 and door[0].state == "not-run" and rc == 3
          and rows.get("front-door", {}).get("result") == "not-run"
          and (rows.get("front-door", {}).get("not_run") or {}).get("why") == "no-screen"
          and progress.get("front-door", {}).get("state") == "not-run"
          and [rows.get(i.label, {}).get("result") for i in its if i.label != "front-door"] == ["passed"]
          and "] NOT RUN   front-door" in out
          and "1 check(s) NOT RUN, which is not a pass: front-door (no-screen)" in out,
          "P17 a suite run-tests.sh did not run is NOT RUN (no-screen) in progress.json, summary.json, the live "
          "line and the final line; the run exits 3, never 0",
          ([(i.label, i.state, i.notes) for i in its], rc, out[-600:]))
    # A run-tests.sh check that exits 0 and never wrote its results record cannot show it ran.
    silent = pr.Item("silent-suite", os.path.join(pr.ROOT, "richos/app"),
                     ["bash", "-c", "exit 0", "--results-out", os.path.join(d17, "never-written.json")], None, 1)
    with contextlib.redirect_stdout(io.StringIO()):
        pr.run([silent], Args(), os.path.join(tmp, "p17b"), sampler=idle)
    check(silent.state == "invalid" and silent.rc == 125
          and any("without a readable results record" in n for n in silent.notes),
          "P17b a run-tests.sh check that exits 0 without its results record is INVALID, not passed",
          (silent.state, silent.notes))

    # P38 — a UI suite the land runs directly (`cd richos/app/ui/tests && node <suite>.js`) is
    # read from the evidence ledger its harness writes, as run.js reads it (hunt part 2, finding
    # 18). Before 2026-09-30 only its exit code was read, so realbytes.js's skipSuite()-and-exit-0
    # was recorded `passed` over a Rust compile failure. Fixture suites write the ledger the way
    # lib/harness.js recordEvidence() does, from RICHOS_UI_TESTS_LEDGER.
    d38 = os.path.join(tmp, "p38")
    os.makedirs(d38)

    def ui_fixture(name, records, code=0):
        path = os.path.join(d38, name + ".js")
        with open(path, "w") as fh:
            fh.write("const fs = require('fs');\nconst L = process.env.RICHOS_UI_TESTS_LEDGER || '';\n"
                     "for (const r of %s) { if (L) fs.appendFileSync(L, JSON.stringify(Object.assign({suite: '%s.js'}, r)) + '\\n'); }\n"
                     "process.exit(%d);\n" % (json.dumps(records), name, code))
        return pr.Item(name, os.path.join(pr.ROOT, "richos/app/ui/tests"), ["node", path], None, 1)

    skipped = ui_fixture("skipped", [{"label": "x", "skipped": "could not run cargo\n        error[E0425]"}])
    empty = ui_fixture("empty", [])
    ran = ui_fixture("ran", [{"label": "x", "checks": 3, "failed": 0}])
    lied = ui_fixture("lied", [{"label": "x", "checks": 3, "failed": 1}])
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        pr.run([skipped, empty, ran, lied], Args(), d38, sampler=idle)
        rc38 = pr.summarize([skipped, empty, ran, lied], 1.0, d38)
    with open(os.path.join(d38, "summary.json")) as fh:
        rows38 = {c["check"]: c for c in json.load(fh)["checks"]}
    check(skipped.state == "not-run" and (skipped.not_run or {}).get("why") == "suite-skipped"
          and rows38.get("skipped", {}).get("result") == "not-run"
          and "could not run cargo" in skipped.not_run["suites"][0]["reason"],
          "P38a a UI suite whose ledger holds only a skip is NOT RUN (suite-skipped), never passed",
          (skipped.state, skipped.notes, rows38.get("skipped")))
    check(empty.state == "invalid" and any("without a single check" in n for n in empty.notes),
          "P38b a UI suite that exits 0 with no check in its ledger is INVALID: it cannot show it ran",
          (empty.state, empty.notes))
    check(ran.state == "passed", "P38c a UI suite that ran its checks and exited 0 still passes", (ran.state, ran.notes))
    check(lied.state == "invalid" and any("1 failed check" in n for n in lied.notes) and rc38 != 0,
          "P38d a UI suite whose ledger records a failed check and exits 0 is INVALID, and the run is not green",
          (lied.state, lied.notes, rc38))
    # P38f-h — recheck R26: the direct reader holds a suite to the same floor as run.js. The ledger's
    # `checks` includes the housekeeping check the harness appends, so a run with no check of its
    # own (productChecks 0) or fewer than its source declares is not evidence.
    hk_only = ui_fixture("hk-only", [{"label": "x", "checks": 1, "productChecks": 0, "declared": 3, "failed": 0}])
    hk_short = ui_fixture("hk-short", [{"label": "x", "checks": 3, "productChecks": 2, "declared": 3, "failed": 0}])
    hk_full = ui_fixture("hk-full", [{"label": "x", "checks": 4, "productChecks": 3, "declared": 3, "failed": 0}])
    hk_none = ui_fixture("hk-none", [{"label": "x", "checks": 1, "productChecks": 0, "declared": 0, "failed": 0}])
    with contextlib.redirect_stdout(io.StringIO()):
        pr.run([hk_only, hk_short, hk_full, hk_none], Args(), os.path.join(d38, "r26"), sampler=idle)
    check(hk_only.state == "invalid" and any("no check of its own" in n for n in hk_only.notes),
          "P38f a UI suite whose only passing check is the harness's housekeeping is INVALID, not passed",
          (hk_only.state, hk_only.notes))
    check(hk_short.state == "invalid" and any("stopped early" in n for n in hk_short.notes),
          "P38g a UI suite that ran fewer own checks than its source declares is INVALID",
          (hk_short.state, hk_short.notes))
    check(hk_full.state == "passed" and hk_none.state == "invalid" and any("cannot fail" in n for n in hk_none.notes),
          "P38h own checks meeting the declared count pass; a source declaring none is INVALID",
          (hk_full.state, hk_none.state, hk_none.notes))
    # P38i — a UI suite's navigation-failure bundle lands in the check's own results folder in the
    # run's evidence, never in the checkout's .shots/ (PRD step 3, T07/R9), and is no input.
    nav = os.path.join(d38, "nav.js")
    with open(nav, "w") as fh:
        fh.write("const fs = require('fs'), path = require('path');\n"
                 "const d = process.env.RICHOS_UI_NAV_EVIDENCE_DIR || '';\n"
                 "if (d) { fs.mkdirSync(d, {recursive: true}); fs.writeFileSync(path.join(d, 'bundle.txt'), 'evidence'); }\n"
                 "fs.appendFileSync(process.env.RICHOS_UI_TESTS_LEDGER, JSON.stringify("
                 "{suite: 'nav.js', label: 'x', checks: 1, failed: 1}) + '\\n');\nprocess.exit(1);\n")
    navigated = pr.Item("nav", os.path.join(pr.ROOT, "richos/app/ui/tests"), ["node", nav], None, 1)
    with contextlib.redirect_stdout(io.StringIO()):
        pr.run([navigated], Args(), os.path.join(d38, "nav"), sampler=idle)
    bundle = os.path.join(navigated.results or "", "navigation-failures", "bundle.txt")
    check(os.path.isfile(bundle) and os.path.realpath(bundle).startswith(os.path.realpath(os.path.join(d38, "nav")))
          and "RICHOS_UI_NAV_EVIDENCE_DIR" not in navigated.env,
          "P38i a UI suite's navigation-failure bundle is kept in the run's evidence, not the checkout, and is no input",
          (navigated.results, navigated.state, sorted(navigated.env)))
    real = items_from(["cd richos/app/ui/tests && node realbytes.js",
                       "cd richos/web/web-app && node --test test/api.test.js"], d38)
    check([bool(pr.ui_suite_file(i)) for i in real] == [True, False],
          "P38e proof-for.sh's UI line is recognized as a UI suite; the web app's node --test is not",
          [(i.label, i.cwd, i.argv) for i in real])

    # P39 — the REAL realbytes.js on the land's own line, both ways it can fail to render the
    # backend's bytes. A cargo that runs and fails (a compile error) is a red check; only a cargo
    # that cannot be started is a skip, and that skip is NOT RUN. Before 2026-09-30 both were
    # the same skip, exit 0, recorded `passed`. Neither case reaches the browser.
    d39 = os.path.join(tmp, "p39")
    fake = os.path.join(d39, "fakebin")
    os.makedirs(fake)
    with open(os.path.join(fake, "cargo"), "w") as fh:
        fh.write("#!/bin/sh\necho 'error[E0425]: cannot find value `payload` in this scope' >&2\nexit 101\n")
    os.chmod(os.path.join(fake, "cargo"), 0o755)
    nocargo_home = os.path.join(d39, "home")
    os.makedirs(nocargo_home)
    line39 = "cd richos/app/ui/tests && node realbytes.js"
    broken = items_from([line39], d39)[0]
    broken.env["PATH"] = fake + os.pathsep + os.environ.get("PATH", "")
    absent = items_from([line39], d39)[0]
    absent.label = "realbytes-no-cargo"
    # No cargo anywhere: the runner and the suite both add $HOME/.cargo/bin, so HOME moves too.
    absent.env["PATH"] = os.pathsep.join((os.path.dirname(sys.executable), "/usr/bin", "/bin"))
    absent.env["HOME"] = nocargo_home
    saved_home = os.environ["HOME"]
    os.environ["HOME"] = nocargo_home
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            pr.run([broken], Args(), os.path.join(d39, "broken"), sampler=idle)
            pr.run([absent], Args(), os.path.join(d39, "absent"), sampler=idle)
    finally:
        os.environ["HOME"] = saved_home
    with open(broken.log, errors="replace") as fh:
        broken_log = fh.read()
    check(broken.state == "failed" and broken.rc == 1 and "E0425" in broken_log and "FAIL" in broken_log,
          "P39a realbytes.js over a cargo that fails to compile is a FAILED check naming the error, never a skip",
          (broken.state, broken.rc, broken.notes, broken_log[-400:]))
    with open(absent.log, errors="replace") as fh:
        absent_log = fh.read()
    check(absent.state == "not-run" and (absent.not_run or {}).get("why") == "suite-skipped"
          and "SKIPPED, not passed" in absent_log,
          "P39b realbytes.js with no cargo at all still skips, loudly, and the land records it NOT RUN",
          (absent.state, absent.notes, absent_log[-400:]))

    # P18 — a proof run started by a check of a proof run, in a MAIN checkout, is part of its
    # caller's integration plan and never waits for the episode lock its caller holds. Before
    # 2026-09-29 it opened a second episode, waited its whole bound for that lock and failed
    # "integration priority exhausted" (P16h in a main checkout; the land gate stopped at 600 s).
    # Here the caller's episode is this process holding the lock, the bound is 5 s, and the
    # checkout is declared main for the run.
    import fcntl
    d18 = os.path.join(tmp, "p18")
    admission = os.path.join(os.environ["RICHOS_MACHINE_WORKERS"], "admission")
    os.makedirs(admission, exist_ok=True)
    caller_episode = open(os.path.join(admission, "integration-plan.lock"), "a")
    fcntl.flock(caller_episode, fcntl.LOCK_EX)

    class CallersSlot:
        borrowed, fds = True, ()

        def env(self):
            return {}

    saved = (pr.SLOT, pr.engine_pass.is_main_checkout, pr.engine_pass.INTEGRATION_PLAN_SECONDS)
    pr.SLOT, pr.engine_pass.is_main_checkout, pr.engine_pass.INTEGRATION_PLAN_SECONDS = \
        CallersSlot(), (lambda *a: True), 5
    inner = pr.Item("inner-run-check", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "exit 0"], None, 1)
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            pr.run([inner], Args(), d18, sampler=idle)
    finally:
        pr.SLOT, pr.engine_pass.is_main_checkout, pr.engine_pass.INTEGRATION_PLAN_SECONDS = saved
        caller_episode.close()
    check(inner.state == "passed",
          "P18 a nested run in a main checkout runs inside its caller's integration plan, never waiting for its lock",
          (inner.state, inner.notes))

    # P6 — the receipts check runs even when a shard failed: it is what names the missing units.
    a = pr.Item("engine 1/2", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "exit 1"], None, 5)
    b = pr.Item("engine receipts", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "exit 0"], None, 1,
                after=["engine 1/2"])
    pr.run([a, b], Args(), tmp, sampler=idle)
    check(a.state == "failed" and b.state == "passed" and b.started >= a.ended,
          "P6 the receipts check waits for the shards and runs even after one failed")

    # P6b — the gate path stays whole. A runner started from a teammate workspace narrows by
    # default (engine/scripts/lib/workspace_scope.py, 2026-10-01); every check this runner starts
    # (the merge gate, a land, the engine nightly) is told RICHOS_TEST_SCOPE=full, whatever the
    # caller's own shell says.
    saved = os.environ.get("RICHOS_TEST_SCOPE")
    os.environ["RICHOS_TEST_SCOPE"] = "narrow"
    try:
        scope = pr.Item("scope", os.path.join(pr.ROOT, "richos/app"),
                        ["bash", "-c", 'test "$RICHOS_TEST_SCOPE" = full'], None, 1)
        pr.run([scope], Args(), tmp, sampler=idle)
    finally:
        if saved is None:
            os.environ.pop("RICHOS_TEST_SCOPE", None)
        else:
            os.environ["RICHOS_TEST_SCOPE"] = saved
    check(scope.state == "passed", "P6b every check runs with RICHOS_TEST_SCOPE=full, so nothing it runs is narrowed",
          (scope.state, scope.notes))

    # P7 — an UNCOVERED selection runs nothing and exits 1.
    r = subprocess.run([sys.executable, os.path.join(HERE, "proof-run.py"), "--log-dir", os.path.join(tmp, "p7"),
                        "--paths", "richos/mobile/zz-uncovered.sh"], capture_output=True, text=True)
    check(r.returncode == 1 and "Nothing was run" in r.stderr and not os.path.exists(os.path.join(tmp, "p7", "summary.json")),
          "P7 proof-for.sh's UNCOVERED stops the run before anything starts (exit 1)", (r.returncode, r.stderr[-300:]))

    # P8 — an interrupt stops what the runner started, by its process group, and nothing else.
    cmds = os.path.join(tmp, "p8.txt")
    pidfile = os.path.join(tmp, "p8.pid")
    own = os.path.join(tmp, "p8.own")
    deaf = os.path.join(tmp, "p8.deaf")
    # Three ways a process escaped a group kill: a plain child, a grandchild in a session of its
    # own (reserve.py's shape), and a child that ignores SIGTERM.
    with open(cmds, "w") as fh:
        fh.write("cd richos/app && bash -c 'python3 -c \"import subprocess, sys; p = subprocess.Popen([\\\"sleep\\\", \\\"60\\\"], "
                 "start_new_session=True); open(sys.argv[1], \\\"w\\\").write(str(p.pid)); p.wait()\" %s & "
                 "bash -c \"trap \\\"\\\" TERM; echo \\$\\$ > %s; while :; do sleep 1; done\" & "
                 "sleep 60 & echo $! > %s; wait'\n" % (own, deaf, pidfile))
    # This case tests cancellation of started children. Use a deterministic
    # admission sample so a busy real Mac cannot turn it into a queueing test.
    bootstrap = ("import runpy,sys; sys.path.insert(0," + repr(os.path.join(HERE, 'testvm')) + "); "
                 "import reserve; reserve.host_sample=lambda: " + repr(idle()) + "; "
                 "sys.argv=[" + repr(os.path.join(HERE, 'proof-run.py')) + "]+sys.argv[1:]; "
                 "runpy.run_path(sys.argv[0],run_name='__main__')")
    runner = subprocess.Popen([sys.executable, '-c', bootstrap, "--commands", cmds,
                               "--log-dir", os.path.join(tmp, "p8")],
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    # No clock of this suite's own around real runners (audit R13, 2026-09-29): every wait
    # ends on a fact or on the process that would produce it ending. A hang is caught by the
    # enclosing runner's per-suite deadline, which names this suite.
    while not all(os.path.exists(f) and open(f).read().strip() for f in (pidfile, own, deaf)) \
            and runner.poll() is None:
        time.sleep(0.1)
    pids = [int(open(f).read().strip()) for f in (pidfile, own, deaf) if os.path.exists(f) and open(f).read().strip()]
    runner.send_signal(signal.SIGTERM)
    out, _ = runner.communicate()
    alive = []
    for p in pids:
        try:
            os.kill(p, 0)
            alive.append(p)
            os.kill(p, signal.SIGKILL)
        except ProcessLookupError:
            pass
    check(len(pids) == 3 and not alive and runner.returncode == 130 and "interrupted" in out,
          "P8 SIGTERM to the runner stops the whole tree of the check it started (child, own-session "
          "grandchild, TERM-ignoring child: %s all gone) and exits 130" % pids, (alive, out[-300:]))

    # P10 — --as-printed: the selection unsplit, in printed order, one at a time (the baseline).
    order = os.path.join(tmp, "order")
    lines = ["cd richos/app && bash -c 'echo %d >> %s; sleep 0.3'" % (n, order) for n in range(1, 4)]
    its = pr.as_printed(lines)
    pr.run(its, Args(capacity=4), tmp, sampler=idle)
    check(len(its) == 3 and {i.lane for i in its} == {"as-printed"}
          and open(order).read().split() == ["1", "2", "3"],
          "P10 --as-printed runs each printed line unsplit, one after another, in printed order")

    # P15 — a run with nothing of its own running, whose next check admission refuses, is a job
    # waiting on the Mac: it is RECORDED for exactly as long as that lasts (so the lead's turn end
    # is refused past ten minutes, engine guard-resource-waits.sh) and the record goes when the
    # check is admitted. The sampler is busy for the first 0.7 s, then idle.
    waits = os.environ["RICHOS_WAITS_DIR"]
    seen = []
    t15 = time.time()

    def stalled():
        try:
            names = [f for f in os.listdir(waits) if f.endswith(".json")]
        except OSError:
            names = []
        seen.extend(open(os.path.join(waits, f)).read() for f in names)
        return busy() if time.time() - t15 < 0.7 else idle()

    saved_retry = pr.reserve.MIN_RETRY_SECONDS
    pr.reserve.MIN_RETRY_SECONDS = 0.3
    try:
        its = items_from(["cd richos/app && bash -c 'exit 0'"], tmp)
        pr.run(its, Args(admission_wait=60), os.path.join(tmp, "p15log"), sampler=stalled)
    finally:
        pr.reserve.MIN_RETRY_SECONDS = saved_retry
    left = [f for f in os.listdir(waits) if f.endswith(".json")] if os.path.isdir(waits) else []
    check(its[0].state == "passed" and any('"cpu-admission"' in r and "not admitted" in r for r in seen)
          and not left,
          "P15 a stalled run is recorded as a CPU-admission wait while it stalls, and the record is gone "
          "once the check is admitted", (its[0].state, seen[:1], left))

    # P16 — RUNS share the Mac: a host-wide limit on how many proof runs go at once, counted by
    # OS-held locks (lib/proof_slots.py). 2026-09-27: several agents each saw a free Mac on one
    # instant sample and each started a full selection; one run alone already peaks at the whole
    # Mac. Real proof-run.py processes, a fixture slot directory, a stubbed idle sampler (so only
    # the run limit can make a run wait), fixture commands only. The fixture runs are background
    # runs wherever this suite runs: in a main checkout every run is otherwise a land's
    # integration plan, and one integration plan at a time is a different rule (engine_pass.py)
    # that made P16g wait 600 s there (2026-09-29, the land gate of cc/zach-opus-gatefix1).
    slots_dir = os.path.join(tmp, "slots")
    slots_tool = os.path.join(HERE, "lib", "proof_slots.py")
    runner_boot = ("import runpy,sys; sys.path.insert(0," + repr(os.path.join(HERE, 'testvm')) + "); "
                   "import reserve; reserve.host_sample=lambda: " + repr(idle()) + "; "
                   "sys.path.insert(0," + repr(os.path.join(os.path.dirname(os.path.dirname(HERE)), 'engine', 'scripts', 'lib')) + "); "
                   "import engine_pass; engine_pass.is_main_checkout=lambda *a: False; "
                   "sys.argv=[" + repr(os.path.join(HERE, 'proof-run.py')) + "]+sys.argv[1:]; "
                   "runpy.run_path(sys.argv[0],run_name='__main__')")

    def slots(*argv):
        return subprocess.run([sys.executable, slots_tool, *argv], capture_output=True, text=True,
                              env={**os.environ, "RICHOS_PROOF_RUN_SLOTS_DIR": slots_dir})

    def start_run(name, command, extra_env=None):
        cmds = os.path.join(tmp, name + ".cmds")
        with open(cmds, "w") as fh:
            fh.write("cd richos/app && bash -c %s\n" % shlex_quote(command))
        out = open(os.path.join(tmp, name + ".out"), "w")
        env = {**os.environ, "RICHOS_PROOF_RUN_SLOTS_DIR": slots_dir, "RICHOS_WAITER": name,
               "RICHOS_RUNTIME_DIR": os.path.join(tmp, "no-runtime")}
        env.pop("RICHOS_PROOF_RUN_SLOT_HELD", None)   # this suite may itself run inside a proof run
        env.update(extra_env or {})
        return subprocess.Popen([sys.executable, "-c", runner_boot, "--commands", cmds,
                                 "--log-dir", os.path.join(tmp, name + ".log")],
                                stdout=out, stderr=subprocess.STDOUT, env=env)

    def output(name):
        with open(os.path.join(tmp, name + ".out")) as fh:
            return fh.read()

    def until(predicate, *runs):
        # The fact, or the end of every run that could produce it; never a number of seconds
        # (audit R13: 10-30 s on real runners was a verdict on how busy the Mac was).
        while not predicate():
            if all(run.poll() is not None for run in runs):
                return predicate()
            time.sleep(0.1)
        return True

    def exists(path):
        return os.path.exists(path)

    def held(tag):
        # A holder run that keeps its slot until the test says so. It used to `sleep 3`, and a
        # second run slower than that to start never met a held slot at all.
        return "while [ ! -e %s/%s.release ]; do sleep 0.1; done" % (mark, tag)

    def release(tag):
        open(os.path.join(mark, tag + ".release"), "w").close()

    def gone(pid):
        # Gone, or a zombie waiting to be reaped: os.kill(pid, 0) still finds one.
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        state = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)], capture_output=True, text=True).stdout
        return not state.strip() or "Z" in state

    from shlex import quote as shlex_quote
    r = slots("set", "1")
    check(r.returncode == 0 and slots("status", "--json").returncode == 0
          and json.loads(slots("status", "--json").stdout)["limit"] == 1,
          "P16a the limit is the host's setting: `proof_slots.py set 1` is what a later run reads",
          (r.returncode, r.stdout, r.stderr))
    bad = [slots("set", v).returncode for v in ("0", "-1", "x", "1000")]
    check(all(rc == 2 for rc in bad), "P16a2 a limit below 1, above the core count or not a number is refused", bad)

    # P16b — over the limit, the second run WAITS, is recorded as waiting while it waits, is named
    # in the status with the holder, and runs once the first run is over.
    mark = os.path.join(tmp, "p16")
    os.makedirs(mark)
    a = start_run("p16-a", "touch %s/a.start; %s; touch %s/a.end" % (mark, held("a"), mark))
    check(until(lambda: exists(os.path.join(mark, "a.start")), a), "P16b0 the first run starts at once under the limit",
          output("p16-a")[-400:])
    b = start_run("p16-b", "touch %s/b.start" % mark)
    waited_seen = until(lambda: "waiting for a proof-run slot" in output("p16-b"), b)
    status = json.loads(slots("status", "--json").stdout)
    records = []
    for f in (os.listdir(os.environ["RICHOS_WAITS_DIR"]) if os.path.isdir(os.environ["RICHOS_WAITS_DIR"]) else []):
        if f.endswith(".json"):
            with open(os.path.join(os.environ["RICHOS_WAITS_DIR"], f)) as fh:
                records.append(json.load(fh))
    check(waited_seen and not exists(os.path.join(mark, "b.start")),
          "P16b a second run over the limit of 1 WAITS, and says so, while the first holds the Mac",
          output("p16-b")[-400:])
    check([h["pid"] for h in status["holders"]] == [a.pid] and [w["pid"] for w in status["waiting"]] == [b.pid]
          and status["holders"][0]["waiter"] == "p16-a" and status["waiting"][0]["waiter"] == "p16-b",
          "P16c `proof_slots.py status` names who holds the Mac (pid %d) and who waits (pid %d)" % (a.pid, b.pid),
          status)
    check(any(rec.get("pid") == b.pid and rec.get("resource") == "cpu-admission" and "proof-run slot" in rec.get("reason", "")
              and "p16-a" in rec.get("reason", "") for rec in records),
          "P16d the waiting run is RECORDED for the lead's turn-end gate, naming the run it waits for", records)
    release("a")
    a_rc, b_rc = a.wait(), b.wait()
    a_end = os.path.getmtime(os.path.join(mark, "a.end")) if exists(os.path.join(mark, "a.end")) else None
    b_start = os.path.getmtime(os.path.join(mark, "b.start")) if exists(os.path.join(mark, "b.start")) else None
    left = [f for f in os.listdir(os.environ["RICHOS_WAITS_DIR"]) if f.endswith(".json")] \
        if os.path.isdir(os.environ["RICHOS_WAITS_DIR"]) else []
    check(a_rc == 0 and b_rc == 0 and a_end is not None and b_start is not None and b_start >= a_end
          and not left and "waited" in output("p16-b"),
          "P16e ... and it runs after the first run is over, passes, reports its wait, and leaves no wait record",
          (a_rc, b_rc, a_end, b_start, left, output("p16-b")[-400:]))
    # P16e2 — the wait is SAID when it changes (who holds, place in line, the limit) or every
    # proof_slots.SAY_EVERY_SECONDS, never once a second: 2026-09-28's end-to-end run printed its
    # wait line every second (the elapsed time made each line new), 10,800 lines in a 3 h wait.
    said = [l for l in output("p16-b").splitlines() if "waiting for a proof-run slot" in l]
    check(len(said) == 1,
          "P16e2 a run waiting behind one unchanged holder says so once, not once a second (%d line(s))" % len(said),
          said)

    # P16f — a killed holder releases the Mac: SIGKILL the holding runner (its pid, captured at
    # spawn). The kernel drops its locks; its check's supervisor stops the check it was running.
    a = start_run("p16-kill-a", "echo $$ > %s/k.pid; %s" % (mark, held("k")))   # never released: killed
    check(until(lambda: exists(os.path.join(mark, "k.pid")) and open(os.path.join(mark, "k.pid")).read().strip(), a),
          "P16f0 the holder's check is running", output("p16-kill-a")[-400:])
    b = start_run("p16-kill-b", "touch %s/k.b" % mark)
    until(lambda: "waiting for a proof-run slot" in output("p16-kill-b"), b)
    a.send_signal(signal.SIGKILL)
    a.wait()
    released = until(lambda: exists(os.path.join(mark, "k.b")), b)
    b_rc = b.wait()
    sleeper = int(open(os.path.join(mark, "k.pid")).read().strip())
    survived = not gone(sleeper)
    if survived:
        os.kill(sleeper, signal.SIGKILL)
    check(released and b_rc == 0 and not survived,
          "P16f a SIGKILLed holder releases the Mac: the waiting run starts and passes, the dead run's check is gone",
          (released, b_rc, survived, output("p16-kill-b")[-400:]))

    # P16g — the positive control: under the limit, a second run starts at once.
    slots("set", "2")
    a = start_run("p16-two-a", "touch %s/t.a; %s" % (mark, held("t")))
    until(lambda: exists(os.path.join(mark, "t.a")), a)
    t_b = time.time()
    b = start_run("p16-two-b", "touch %s/t.b" % mark)
    started = until(lambda: exists(os.path.join(mark, "t.b")), b)   # the first still holds its slot
    to_start = time.time() - t_b
    b_rc = b.wait()
    release("t")
    a_rc = a.wait()
    check(started and b_rc == 0 and a_rc == 0 and "waiting for a proof-run slot" not in output("p16-two-b"),
          "P16g under a limit of 2, the second run's check starts beside the first (%.1f s after launch)" % to_start,
          output("p16-two-b")[-400:])

    # P16h — a proof run started BY a check of a proof run (a suite that runs the real runner)
    # works inside its caller's slot, never waiting for the slot its own caller holds.
    slots("set", "1")
    nested_cmds = os.path.join(tmp, "p16-inner.cmds")
    with open(nested_cmds, "w") as fh:
        fh.write("cd richos/app && bash -c 'touch %s/inner'\n" % mark)
    inner = ("%s -c %s --commands %s --log-dir %s" % (
        shlex_quote(sys.executable), shlex_quote(runner_boot), shlex_quote(nested_cmds),
        shlex_quote(os.path.join(tmp, "p16-inner.log"))))
    outer = start_run("p16-outer", inner)
    rc = outer.wait()
    check(rc == 0 and exists(os.path.join(mark, "inner")),
          "P16h a proof run inside a check of a proof run uses its caller's slot and does not deadlock",
          output("p16-outer")[-600:])
    # ... and nobody else can claim that: a run that is NOT a descendant of the holder, naming the
    # holder in the environment, still waits.
    a = start_run("p16-hold", "touch %s/h.a; %s" % (mark, held("h")))
    until(lambda: exists(os.path.join(mark, "h.a")), a)
    holder = json.loads(slots("status", "--json").stdout)["holders"]
    fake = holder[0]["holder_file"] if holder else "none"
    b = start_run("p16-fake", "touch %s/h.b" % mark, {"RICHOS_PROOF_RUN_SLOT_HELD": fake})
    waited = until(lambda: "waiting for a proof-run slot" in output("p16-fake"), b)
    release("h")
    a.wait()
    b.wait()
    check(waited, "P16i naming the holder's slot in the environment does not let an unrelated run skip the line",
          output("p16-fake")[-400:])

    # P40 — the merge gate's caps (2026-09-30). --cap is ONE number for every check, engine
    # units included, whatever a dated weight predicts (`workspace-spec-fourteen` was planned
    # at 2812 s and ran in 172-554 s); --run-cap ends the run on its own clock and ends, never
    # fails, what did not finish; a run refused a proof-run slot still writes its summary.
    ended = "cancelled"  # dialect-exempt: the runner's state value, which summary.json carries
    heavy = pr.Item("engine mega-lander/tests/workspace-spec-fourteen.test.sh", os.path.join(pr.ROOT, "richos/engine"),
                    ["bash", "scripts/ci-shard.sh", "--only-units", "mega-lander/tests/workspace-spec-fourteen.test.sh"],
                    "engine-shard-1", 2812.0)
    long_suite = pr.Item("contrast.js", os.path.join(pr.ROOT, "richos/app/ui/tests"), ["node", "contrast.js"], None, 2812.0)
    capped = [pr.deadline_for(heavy, Args(cap=600)), pr.deadline_for(long_suite, Args(cap=600))]
    uncapped = [pr.deadline_for(heavy, Args()), pr.deadline_for(long_suite, Args())]
    check(capped == [600.0, 600.0] and uncapped == [None, 3600.0],
          "P40a --cap 600 stops an engine unit and a check planned at 2812 s at 600 s; without it nothing changes",
          (capped, uncapped))

    d40 = os.path.join(tmp, "p40")
    os.makedirs(d40)
    slow = pr.Item("slow", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "sleep 300"], "one-lane", 5.0)
    behind = pr.Item("behind", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "true"], "one-lane", 1.0)
    quick = pr.Item("quick", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "true"], None, 1.0)
    # The slow check sleeps 300 s: ending well before that, not a tight clock, is what is proved.
    t40 = time.time()
    buf40 = io.StringIO()
    with contextlib.redirect_stdout(buf40):
        pr.run([slow, behind, quick], Args(run_cap=3), d40, sampler=idle)
    took40 = time.time() - t40
    check(slow.state == ended and "--run-cap" in " ".join(slow.notes)
          and behind.state == ended and "not started" in " ".join(behind.notes)
          and quick.state == "passed" and took40 < 150 and "RUN CAP" in buf40.getvalue(),
          "P40b at --run-cap 3 the running check is stopped and the waiting one never starts, both ended by "
          "name; the finished one keeps its pass (%.0f s)" % took40,
          ([(i.label, i.state, i.notes) for i in (slow, behind, quick)], buf40.getvalue()[-300:]))

    a = start_run("p40-hold", "touch %s/c.a; %s" % (mark, held("c")))
    until(lambda: exists(os.path.join(mark, "c.a")), a)
    cmds40 = os.path.join(tmp, "p40-refused.cmds")
    with open(cmds40, "w") as fh:
        fh.write("cd richos/app && bash -c 'touch %s/c.b'\n" % mark)
    summary40 = os.path.join(tmp, "p40-refused.summary.json")
    env40 = {**os.environ, "RICHOS_PROOF_RUN_SLOTS_DIR": slots_dir, "RICHOS_RUNTIME_DIR": os.path.join(tmp, "no-runtime")}
    env40.pop("RICHOS_PROOF_RUN_SLOT_HELD", None)
    r40 = subprocess.run([sys.executable, "-c", runner_boot, "--commands", cmds40, "--log-dir",
                          os.path.join(tmp, "p40-refused.log"), "--slot-wait", "0.5", "--summary-out", summary40],
                         capture_output=True, text=True, env=env40)
    release("c")
    a.wait()
    rows40 = json.load(open(summary40))["checks"] if exists(summary40) else None
    check(r40.returncode == 1 and rows40 and all(row["result"] == "not-admitted" for row in rows40)
          and not exists(os.path.join(mark, "c.b")),
          "P40c a run refused a proof-run slot writes its summary: every check NOT ADMITTED by name, none run",
          (r40.returncode, rows40, r40.stdout[-300:]))

    # P41 — under --cap a check planned past the cap is never started (2026-09-30, the merge of
    # 4e73fd89: a mutation unit planned at 1408 s ran into its 600 s cap and held the Mac while
    # the change's own checks waited). It is NOT RUN at once, with its planned weight as the
    # reason; the check beside it runs. Without --cap nothing changes.
    d41 = os.path.join(tmp, "p41")
    os.makedirs(d41)
    marker41 = os.path.join(d41, "started")
    long41 = pr.Item("long", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "touch %s" % marker41], None, 1408.0)
    short41 = pr.Item("short", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "true"], None, 5.0)
    t41 = time.time()
    with contextlib.redirect_stdout(io.StringIO()):
        pr.run([long41, short41], Args(cap=600), d41, sampler=idle)
    check(long41.state == "not-run" and not exists(marker41)
          and "planned 1408 s, over its 600 s cap" in (getattr(long41, "not_run", None) or {}).get("why", "")
          and short41.state == "passed" and time.time() - t41 < 120,
          "P41a under --cap 600 a check planned at 1408 s is NOT RUN at once and never starts; the other runs",
          [(i.label, i.state, getattr(i, "not_run", None)) for i in (long41, short41)])
    # The planner keeps such an engine unit out of the shards and out of the receipts proof: the
    # fence suite's mutation unit is planned at 1408 s in lib/ci-unit-weights.tsv.
    d41b = os.path.join(tmp, "p41b")
    os.makedirs(d41b)
    unit_line41 = "cd richos/engine && " + " ".join(["bash", "scripts/ci-shard.sh", "--only-units"]) + " "
    fence_unit41 = "scripts/" + "operator-fences" + "-mutation.test.sh"
    its41 = pr.plan([unit_line41 + fence_unit41, unit_line41 + "scripts/spawn.test.sh"], Args(cap=600), d41b, {})
    fence41 = [i for i in its41 if fence_unit41 in i.label]
    with open(os.path.join(d41b, "engine-units.txt")) as fh:
        planned41 = fh.read().split()
    check(len(fence41) == 1 and fence41[0].lane is None and planned41 == ["scripts/spawn.test.sh"],
          "P41b the planner gives a unit planned past --cap no shard and leaves it out of the receipts proof",
          ([(i.label, i.lane) for i in its41], planned41))
    uncapped41 = pr.Item("long", os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "touch %s" % marker41], None, 1408.0)
    with contextlib.redirect_stdout(io.StringIO()):
        pr.run([uncapped41], Args(), os.path.join(tmp, "p41c"), sampler=idle)
    check(uncapped41.state == "passed" and exists(marker41),
          "P41c without --cap the same check runs, whatever its planned weight", uncapped41.state)

    # P42 — longest first, with or without --cap (2026-10-05). From 2026-09-30 the merge gate
    # started cheapest first; a check planned past the cap is now never started (P41), and
    # cheapest first left the long poles to be ended at the round's cap (P43c). One lane makes
    # the start order visible.
    def start_order(args, name):
        record = os.path.join(tmp, name + ".order")
        lane = [pr.Item(label, os.path.join(pr.ROOT, "richos/app"), ["bash", "-c", "echo %s >> %s" % (label, record)],
                        "one-lane", weight) for label, weight in (("fifty", 50.0), ("five", 5.0), ("twenty", 20.0))]
        with contextlib.redirect_stdout(io.StringIO()):
            pr.run(lane, args, os.path.join(tmp, name), sampler=idle)
        return open(record).read().split() if exists(record) else None
    capped42, uncapped42 = start_order(Args(cap=600), "p42-capped"), start_order(Args(), "p42-uncapped")
    check(capped42 == ["fifty", "twenty", "five"] and uncapped42 == ["fifty", "twenty", "five"],
          "P42 checks start longest first, under --cap as without it", (capped42, uncapped42))

    # P43 — PLANNED FROM MEASURED MEDIANS, LONGEST FIRST (2026-10-05, merge-check speed fix 2).
    # merge110.log: by-reference.test.sh waited 440 s in its lane, ran 443 s, was ended at the
    # 882 s round cap and ran again; the plan came from a weight the run itself called stale.
    d43 = os.path.join(tmp, "p43")
    os.makedirs(d43)
    # P43a: what a check measured is kept as samples, and its weight is their median: one slow run
    # does not move the plan (the last run alone did, before).
    for seconds in (10.0, 12.0, 100.0):
        done = pr.Item("measured", os.path.join(pr.ROOT, "richos/app"), ["true"], None, 1.0)
        done.state, done.started, done.ended = "passed", 1000.0, 1000.0 + seconds
        pr.record_weights(d43, [done])
    check(pr.history_weights(d43).get("measured") == 12.0,
          "P43a a check's planned weight is the median of its measured runs (10, 12, 100 -> 12)",
          open(os.path.join(d43, "weights.tsv")).read())
    # P43b: an engine unit measured in this checkout is planned at its median, not at the dated
    # table's row, and the lanes are packed with it.
    unit43 = "scripts/spawn.test.sh"
    d43b = os.path.join(tmp, "p43b")
    os.makedirs(d43b)
    planned43 = [i for i in pr.plan([unit_line41 + unit43], Args(), d43b, {"engine " + unit43: 431.0})
                 if i.label == "engine " + unit43]
    measured43 = open(os.path.join(d43b, "engine-measured-weights.tsv")).read() \
        if exists(os.path.join(d43b, "engine-measured-weights.tsv")) else ""
    check(len(planned43) == 1 and planned43[0].weight == 431.0 and unit43 + "\t431.0" in measured43,
          "P43b a measured engine unit is planned at its measured median, and the packer is given it",
          ([(i.label, i.weight) for i in planned43], measured43))
    # P43c: THE MERGE GATE'S SHAPE. One lane, a round cap, and the longest check listed in the
    # stale plan as the cheapest one used to be: measured, it is planned long and starts first, so
    # it finishes inside the round; started last it was ended at the round cap.
    marks43 = os.path.join(d43, "marks")
    os.makedirs(marks43)
    def lane43(label, seconds, weight):
        return pr.Item(label, os.path.join(pr.ROOT, "richos/app"),
                       ["bash", "-c", "sleep %s; touch %s/%s" % (seconds, marks43, label)], "one-lane", weight)
    stale43 = os.path.join(d43, "stale")
    os.makedirs(stale43)
    with open(os.path.join(stale43, "weights.tsv"), "w") as fh:
        fh.write("long\t4.0\t4.0,4.0,4.0\nshort-a\t2.0\t2.0\nshort-b\t2.0\t2.0\n")
    hist43 = pr.history_weights(stale43)
    lane = [lane43("short-a", 2, hist43["short-a"]), lane43("short-b", 2, hist43["short-b"]),
            lane43("long", 4, hist43["long"])]
    with contextlib.redirect_stdout(io.StringIO()):
        pr.run(lane, Args(cap=600, run_cap=6.5), os.path.join(d43, "run"), sampler=idle)
    long43 = lane[2]
    check(long43.state == "passed" and exists(os.path.join(marks43, "long")),
          "P43c under the gate's caps the longest measured check starts first and finishes inside the round",
          [(i.label, i.state, i.notes[-1:] if i.notes else []) for i in lane])

    # P44 — A CHECK THAT DID NOT PASS CARRIES THE COMMAND THAT RERUNS IT AS THE GATE RAN IT
    # (2026-10-05). The merge gate's checks inherit LC_ALL=C (workspaces.py _git_env) where a shell
    # has en_GB.UTF-8: land-completeness.test.sh failed only in the gate and passed every hand rerun
    # (3c0383fba). A check that fails only under the gate's locale and time zone: run as the gate
    # runs it, then rerun from a shell with another locale, by hand (passes) and through the file
    # the summary names (fails, as in the gate).
    import re as re44
    import shlex as shlex44
    d44 = os.path.join(tmp, "p44")
    os.makedirs(d44)
    gate_only = pr.Item("gate-only", os.path.join(pr.ROOT, "richos/app"),
                        ["bash", "-c", '[ "${LC_ALL:-}" != C ] && [ "${TZ:-}" != UTC0 ]'], None, 1.0)
    saved44 = {k: os.environ.get(k) for k in ("LC_ALL", "LANG", "TZ")}
    os.environ.update(LC_ALL="C", LANG="C", TZ="UTC0")       # what `git merge` hands the gate
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            pr.run([gate_only], Args(), d44, sampler=idle)
    finally:
        for k, v in saved44.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
    out44 = io.StringIO()
    with contextlib.redirect_stdout(out44):
        pr.summarize([gate_only], 1.0, d44)
    shell44 = {"PATH": os.environ["PATH"], "HOME": os.environ.get("HOME", "/"), "LC_ALL": "en_US.UTF-8",
               "LANG": "en_US.UTF-8", "TZ": "Europe/London"}
    by_hand44 = subprocess.run(gate_only.argv, cwd=gate_only.cwd, env=shell44).returncode
    m44 = re44.search(r"rerun as the gate ran it: sh (\S+)", out44.getvalue())
    rerun44 = subprocess.run(["sh", shlex44.split(m44.group(1))[0]], env=shell44,
                             capture_output=True).returncode if m44 else None
    check(gate_only.state == "failed" and m44 is not None and by_hand44 == 0 and rerun44 == 1
          and "LC_ALL=C" in out44.getvalue() and "TZ=UTC0" in out44.getvalue(),
          "P44 a failed check names the file that reruns it as the gate ran it: by hand it passes, through the file "
          "it fails as in the gate, and the line shows the locale and time zone",
          (gate_only.state, by_hand44, rerun44, out44.getvalue()[-400:]))

    # P45 — THE RERUN RUNS ON THE TREE THE GATE TESTED, NOT ON WHAT THE CHECKOUT HOLDS LATER
    # (2026-10-05). The gate runs in the main checkout while the merge is in progress; a refused
    # merge is aborted, so that checkout is back on main. A front-door.js A1 failure
    # (attempt-ioj6a1_a) "passed" its rerun on main. Here: a check that fails only on the merged
    # tree (it reads a file the branch changed, through the checkout's absolute path) is run
    # mid-merge, the merge is aborted, and the rerun file still fails it, from a scratch checkout
    # of the merged tree that is gone afterwards; by hand on main it passes.
    repo45 = os.path.join(os.path.realpath(tmp), "p45-checkout")
    os.makedirs(os.path.join(repo45, "sub"))
    ident45 = [] if subprocess.run(["git", "config", "--get", "user.email"], capture_output=True).stdout.strip() \
        else ["-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid"]
    env45 = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    def git45(*args):
        return subprocess.run(["git", *ident45, "-C", repo45, *args], env=env45, capture_output=True, text=True)
    git45("init", "-q", "-b", "main")
    open(os.path.join(repo45, "value"), "w").write("main\n")
    open(os.path.join(repo45, "sub", "keep"), "w").write("x\n")
    git45("add", "-A"); git45("commit", "-q", "-m", "main")
    git45("checkout", "-q", "-b", "side")
    open(os.path.join(repo45, "value"), "w").write("branch\n")
    git45("commit", "-q", "-am", "side")
    git45("checkout", "-q", "main")
    git45("merge", "-q", "--no-ff", "--no-commit", "side")
    merged45 = open(os.path.join(repo45, "value")).read().strip()
    on_main45 = ["bash", "-c", 'test "$(cat %s/value)" = main' % repo45]
    check45 = pr.Item("merged-only", os.path.join(repo45, "sub"), on_main45, None, 1.0)
    d45 = os.path.join(tmp, "p45")
    os.makedirs(d45)
    with contextlib.redirect_stdout(io.StringIO()):
        pr.run([check45], Args(), d45, sampler=idle)
    out45 = io.StringIO()
    with contextlib.redirect_stdout(out45):
        pr.summarize([check45], 1.0, d45)
    m45 = re44.search(r"rerun as the gate ran it: sh (\S+)", out45.getvalue())
    file45 = shlex44.split(m45.group(1))[0] if m45 else ""
    shell45 = {"PATH": os.environ["PATH"], "HOME": os.environ.get("HOME", "/")}
    mid45 = subprocess.run(["sh", file45], env=shell45, capture_output=True, text=True) if m45 else None
    git45("merge", "--abort")
    by_hand45 = subprocess.run(on_main45, cwd=check45.cwd).returncode
    rerun45 = subprocess.run(["sh", file45], env=shell45, capture_output=True, text=True) if m45 else None
    trees45 = git45("worktree", "list", "--porcelain").stdout.count("worktree ")
    check(merged45 == "branch" and check45.state == "failed" and m45 is not None
          and mid45.returncode == 1 and "holds the tree the check ran on" in mid45.stderr
          and by_hand45 == 0 and rerun45.returncode == 1 and "scratch checkout" in rerun45.stderr
          and trees45 == 1,
          "P45 a check that failed on a merged tree reruns on that tree after the merge is aborted: by hand on "
          "main it passes, through the file it fails as in the gate, and the scratch checkout is gone",
          (merged45, check45.state, mid45 and (mid45.returncode, mid45.stderr[-300:]), by_hand45,
           rerun45 and (rerun45.returncode, rerun45.stderr[-400:]), trees45, out45.getvalue()[-300:]))

    # P46 — A RERUN FILE NEVER CARRIES A CREDENTIAL'S VALUE, AND STILL REPRODUCES THE CHECK
    # (2026-10-05). The file is plain text under proof-runs/, and it wrote CLAUDE_CODE_MESSAGING_TOKEN,
    # a live session token, into every rerun. Here: the gate's environment holds that token, an API
    # key, a camelCase auth token and a URL with a password in it, beside the variable the check fails
    # on. None of the four values is in the file; the rerun still fails as in the gate; the token
    # reaches the rerun only from a shell that has it.
    import uuid as uuid46
    d46 = os.path.join(tmp, "p46")
    os.makedirs(d46)
    mark46 = os.path.join(d46, "token-seen")
    secrets46 = {"CLAUDE_CODE_MESSAGING_TOKEN": "p46-" + uuid46.uuid4().hex,
                 "RICHOS_FIXTURE_API_KEY": "p46-" + uuid46.uuid4().hex,
                 "fixture46_authToken": "p46-" + uuid46.uuid4().hex,
                 "FIXTURE46_URL": "https://fixture:p46%s@example.invalid/db" % uuid46.uuid4().hex}
    plain46 = {"FIXTURE46": "gate", "FIXTURE46_MARK": mark46}
    gate46 = pr.Item("gate-secrets", os.path.join(pr.ROOT, "richos/app"),
                     ["bash", "-c", 'printf %s "${CLAUDE_CODE_MESSAGING_TOKEN:-}" > "$FIXTURE46_MARK"; '
                                    'test "${FIXTURE46:-}" != gate'], None, 1.0)
    saved46 = {k: os.environ.get(k) for k in [*secrets46, *plain46]}
    os.environ.update(secrets46)
    os.environ.update(plain46)
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            pr.run([gate46], Args(), d46, sampler=idle)
    finally:
        for k, v in saved46.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
    seen_gate46 = open(mark46).read() if exists(mark46) else None
    file46 = getattr(gate46, "rerun", None) or ""
    text46 = open(file46).read() if file46 and exists(file46) else ""
    leaked46 = sorted(k for k, v in secrets46.items() if v in text46)
    shell46 = {"PATH": os.environ["PATH"], "HOME": os.environ.get("HOME", "/")}
    os.unlink(mark46) if exists(mark46) else None
    bare46 = subprocess.run(["sh", file46], env=shell46, capture_output=True, text=True) if text46 else None
    seen_bare46 = open(mark46).read() if exists(mark46) else None
    token46 = secrets46["CLAUDE_CODE_MESSAGING_TOKEN"]
    held46 = subprocess.run(["sh", file46], env={**shell46, "CLAUDE_CODE_MESSAGING_TOKEN": token46},
                            capture_output=True, text=True) if text46 else None
    seen_held46 = open(mark46).read() if exists(mark46) else None
    check(gate46.state == "failed" and seen_gate46 == token46 and text46 and not leaked46
          and "FIXTURE46=gate" in text46 and all(k in text46 for k in secrets46)
          and bare46.returncode == 1 and seen_bare46 == ""
          and held46.returncode == 1 and seen_held46 == token46,
          "P46 a rerun file holds no credential's value (a token, an API key, a camelCase auth token, a URL "
          "password), still fails as in the gate, and passes the token on only from a shell that has it",
          (gate46.state, seen_gate46 == token46, bool(text46), leaked46,
           bare46 and (bare46.returncode, bare46.stderr[-300:]), seen_bare46 == "",
           held46 and held46.returncode, seen_held46 == token46))

    # P46b — rerun-scrub.py takes the values out of a file written before P46: the old file (the
    # values put back in, as the writer did before 2026-10-05) is rewritten in place, keeps its mode
    # and every other line, still fails as in the gate, and --check then finds nothing left.
    import importlib.util as ilu46
    spec46 = ilu46.spec_from_file_location("rerun_scrub_t", os.path.join(HERE, "rerun-scrub.py"))
    rs46 = ilu46.module_from_spec(spec46)
    spec46.loader.exec_module(rs46)
    old46 = os.path.join(d46, "old", "rerun", os.path.basename(file46) if file46 else "x.sh")
    os.makedirs(os.path.dirname(old46))
    lines46 = []
    for line in text46.splitlines(True):
        name = line.strip().split('"', 1)[0][2:-1] if line.strip().startswith("${") else ""
        if line.startswith("# Credentials the gate had"):
            continue
        if line.startswith("# that runs this file has it."):
            continue
        lines46.append("  %s \\\n" % shlex44.quote("%s=%s" % (name, secrets46[name])) if name in secrets46 else line)
    open(old46, "w").write("".join(lines46))
    os.chmod(old46, 0o755)
    planted46 = sum(v in open(old46).read() for v in secrets46.values())
    with contextlib.redirect_stdout(io.StringIO()):
        rs46.main([os.path.join(d46, "old")])
        still46 = rs46.main(["--check", os.path.join(d46, "old")])
    after46 = open(old46).read()
    os.unlink(mark46) if exists(mark46) else None
    scrubbed_run46 = subprocess.run(["sh", old46], env=shell46, capture_output=True, text=True)
    check(planted46 == 4 and not any(v in after46 for v in secrets46.values()) and still46 == 0
          and os.stat(old46).st_mode & 0o777 == 0o755 and "FIXTURE46=gate" in after46
          and all(k in after46 for k in secrets46) and scrubbed_run46.returncode == 1
          and open(mark46).read() == "",
          "P46b rerun-scrub.py takes the four values out of a file written before the fix, keeps its mode and "
          "its other lines, the file still fails as in the gate, and --check finds nothing left",
          (planted46, [k for k, v in secrets46.items() if v in after46], still46,
           oct(os.stat(old46).st_mode & 0o777), scrubbed_run46.returncode, scrubbed_run46.stderr[-300:]))

    # P37 — a pause (agent_hold.py) suspends the runner with its checks. The loop gap it leaves is
    # not the checks' time: each running check's start moves by it, so its deadline is where it was.
    class Held:
        def __init__(self, started):
            self.started = started
    now = 1000.0
    running = [Held(now - 100), Held(now - 5)]
    gap = 300.0 + pr.HELD_GAP_SECONDS               # a 5-minute pause landing in one loop turn
    shifted = pr.discount_held_gap(gap, running)
    ages = [now + gap - it.started for it in running]
    check(shifted == 300.0 and ages == [100 + pr.HELD_GAP_SECONDS, 5 + pr.HELD_GAP_SECONDS],
          "P37 a 5-minute pause ages each running check by the loop's own %.0f s, not by the pause" % pr.HELD_GAP_SECONDS,
          (shifted, ages))
    check(pr.discount_held_gap(pr.HELD_GAP_SECONDS, running) == 0.0 and running[0].started == now - 100 + 300.0,
          "P37b an ordinary slow loop turn (a stop, an admission sample) moves nothing", [it.started for it in running])

    # PHS — THE HOST SHIELD (ceo-decisions §65; engine lib/host_shield.py). Every check starts
    # through it. The two throwaway suites come from host-shield-fixture.sh, which keeps the real
    # command out of reach on a runner without the shield (the bare-name suite checks what PATH
    # finds before it calls; the by-path suite exits on line one); PATH here is the tripwire
    # first and neither sbin.
    hs_dir = os.path.join(tmp, "phs")
    trip, hs_suites = os.path.join(hs_dir, "trip"), os.path.join(hs_dir, "suites")
    fixture = os.path.join(pr.ROOT, "richos/engine/scripts/lib/host-shield-fixture.sh")
    subprocess.run(["bash", "-c", '. "$1" && hs_fixture "$2" "$3" && hs_path "$2" > "$4"', "phs", fixture,
                    trip, hs_suites, os.path.join(hs_dir, "path")], check=True)
    with open(os.path.join(hs_dir, "path")) as fh:
        case_path = fh.read().strip()
    halt_cmd = "shut" + "down"
    saved_path = os.environ["PATH"]
    os.environ["PATH"] = case_path
    try:
        app = os.path.join(pr.ROOT, "richos/app")
        its = [pr.Item("hs-bare", app, ["bash", os.path.join(hs_suites, "hs-bare.test.sh")], None, 1.0),
               pr.Item("hs-path", app, ["bash", os.path.join(hs_suites, "hs-path.test.sh")], None, 1.0)]
        pr.run(its, Args(), os.path.join(tmp, "phs-log"), sampler=idle)
    finally:
        os.environ["PATH"] = saved_path
    logs = {}
    for it in its:
        with open(it.log, errors="replace") as fh:
            logs[it.label] = fh.read()
    tripped = os.path.getsize(os.path.join(trip, "trip.log"))
    check(its[0].state == "failed" and ("  FAIL  host-shield: refused `%s -h +60`" % halt_cmd) in logs["hs-bare"]
          and not tripped,
          "PHS1 a check calling %s -h +60 by bare name FAILS with the stand-in's logged line; nothing reached "
          "the tripwire" % halt_cmd, (its[0].state, logs["hs-bare"][-600:]))
    check(its[1].state == "failed" and "REFUSED before it ran" in logs["hs-path"]
          and not os.path.exists(os.path.join(trip, "path-suite-ran")),
          "PHS2 a check whose suite names /sbin/%s is refused before it runs" % halt_cmd,
          (its[1].state, logs["hs-path"][-600:]))
    # PHS3 — the file that reruns a failed check as the gate ran it starts it through the shield
    # too (review rv-20261011T141442Z-0009fe5a-a898 #5): autocheck tells the operator to run it.
    open(os.path.join(trip, "trip.log"), "w").close()
    rerun3 = getattr(its[0], "rerun", None)
    ran3 = subprocess.run(["sh", rerun3], env={"PATH": case_path, "HOME": os.environ.get("HOME", "/")},
                          capture_output=True, text=True, timeout=120) if rerun3 else None
    said3 = (ran3.stdout + ran3.stderr) if ran3 else ""
    check(ran3 is not None and ran3.returncode != 0
          and ("  FAIL  host-shield: refused `%s -h +60`" % halt_cmd) in said3
          and not os.path.getsize(os.path.join(trip, "trip.log")),
          "PHS3 the rerun file of a check that called %s starts it through the shield: the stand-in refuses "
          "the call and the rerun fails as the gate's run did" % halt_cmd,
          (rerun3, ran3 and ran3.returncode, said3[-600:]))
    # PHS4 — with no shield in the checkout and none exported, a check is refused and never
    # started (a failed check, named), as run-tests.sh refuses: never run without the stand-ins.
    started4 = os.path.join(hs_dir, "phs4-started")
    with open(os.path.join(hs_suites, "phs4.test.sh"), "w") as fh:
        fh.write('#!/usr/bin/env bash\ntouch "%s"\necho "  PASS  started"\n' % started4)
    saved_shield, pr.HOST_SHIELD = pr.HOST_SHIELD, None
    os.environ["PATH"] = case_path
    try:
        it4 = pr.Item("phs4", app, ["bash", os.path.join(hs_suites, "phs4.test.sh")], None, 1.0)
        pr.run([it4], Args(), os.path.join(tmp, "phs4-log"), sampler=idle)
    finally:
        os.environ["PATH"] = saved_path
        pr.HOST_SHIELD = saved_shield
    with open(it4.log, errors="replace") as fh:
        log4 = fh.read()
    check(it4.state == "failed" and "REFUSED: NO HOST SHIELD" in log4 and not os.path.exists(started4),
          "PHS4 with no host shield in the checkout and none exported, a check is refused and never started",
          (it4.state, os.path.exists(started4), log4[-600:]))

    # P9 — the log directory is bounded: the last three runs of a checkout, nothing more.
    parent = os.path.join(tmp, "rot")
    for n in range(5):
        d = os.path.join(parent, "20260923T00000%dZ" % n)
        os.makedirs(d)
        json.dump({"checks": [{"result": "passed"}]}, open(os.path.join(d, "summary.json"), "w"))
    os.makedirs(os.path.join(parent, "not-a-run"))
    pr.rotate(parent)
    left = sorted(os.listdir(parent))
    check(left == ["20260923T000002Z", "20260923T000003Z", "20260923T000004Z", "not-a-run"],
          "P9 rotation keeps the last 3 runs and touches nothing it did not name", left)

    # P19 — the app's session is never a check's environment (2026-10-28: a unit's fixture wait
    # record was left in the app's LIVE registry by a proof run in an app shell). A worker shell
    # the app starts carries the live registry, the hold records, the installed engine and the
    # conversation; a check that touches the registry through its default would write them. CI
    # starts under `env -i`: none of these names exist there.
    app_shell = {"RICHOS_WORKSPACES_DIR": "/live/app-state/workspaces/thr", "RICHOS_AGENT_HOLD_DIR": "/live/agent-hold",
                 "STALL_WATCH_STATE_DIR": "/live/stall", "RICHOS_APP_STATE": "/live/app-state",
                 "RICHOS_APP_THREAD": "thr_fixture", "RICHOS_APP_ENTITY": "entity", "RICHOS_ENGINE_ROOT": "/installed/engine",
                 "RICHOS_ENGINE_DIR": "/installed/engine", "RICHOS_ENTITY_ROOT": "/live/coordination",
                 "CLAUDE_PROJECT_DIR": "/live/coordination", "RICHOS_SESSION_ID": "app-session",
                 "RICHOS_SPAWN_HOOK_SOURCES": "app=/live/profile/spawn-preflight.json",
                 "RICHOS_AGENT_OWNER": "worker-1", "RICHOS_AGENT_SESSION": "session-1",
                 "KEEP_ME": "yes", "RICHOS_CLAUDE_BIN_PATH": "/some/claude"}
    saved_env = dict(os.environ)
    os.environ.update(app_shell)
    try:
        suite = items_from(["cd richos/app && scripts/run-tests.sh --only front-door.test.sh"], tmp)[0]
        script = items_from(["cd richos/app && python3 scripts/thing-check.py"], tmp)[0]
        for it in (suite, script):
            env = pr.execution_environment(it)
            leaked = sorted(k for k in env if k in pr.APP_SESSION_NAMES or k.startswith("RICHOS_APP_"))
            check(not leaked, "P19a %s: no variable of the app's session reaches the check" % it.label, leaked)
            check(env.get("RICHOS_AGENT_OWNER") == "worker-1" and env.get("RICHOS_AGENT_SESSION") == "session-1",
                  "P19b %s: the hold's two tags stay, so a pause can still freeze the check" % it.label)
            check(env.get("RICHOS_CLAUDE_BIN_PATH") == "/some/claude" and env.get("KEEP_ME") == "yes",
                  "P19c %s: the rest of the caller's environment still pass" % it.label)
        # The recorded copy of a run read back (as_caller) is stripped the same way.
        again = pr.execution_environment(suite, caller=dict(app_shell, PATH="/usr/bin"))
        check("RICHOS_WORKSPACES_DIR" not in again and "RICHOS_APP_THREAD" not in again,
              "P19d a recorded caller environment is read back without the app's session too", sorted(again))
    finally:
        os.environ.clear()
        os.environ.update(saved_env)
    # P19e: this list is record-canary.sh's (_rc_roads_back) less the hold's two tags: the engine
    # units and every other check lose the same names, so the two cannot drift apart.
    with open(os.path.join(HERE, "../../engine/scripts/lib/record-canary.sh")) as fh:
        canary = fh.read()
    block = canary.split("APP_SESSION_NAMES = {", 1)[1].split("}", 1)[0]
    in_canary = set(__import__("re").findall(r'"([A-Z0-9_]+)"', block))
    mine = set(pr.APP_SESSION_NAMES) | set(pr.HOLD_TAGS)
    check(in_canary == mine, "P19e the proof runner strips the names record-canary.sh strips, less the hold's tags",
          sorted(in_canary ^ mine))
finally:
    subprocess.run(["rm", "-rf", tmp])

if FAILED:
    print("=== proof-run tests: %d FAILED, %d passed ===" % (FAILED, PASSED))
    sys.exit(1)
print("=== proof-run tests: all %d passed ===" % PASSED)
