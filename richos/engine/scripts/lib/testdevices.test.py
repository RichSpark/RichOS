#!/usr/bin/env python3
"""testdevices.test.py — the test-device collector, TESTED BY DEFEAT.

Run by testdevices.test.sh, which allocates the sandbox. Every simulator here
is a row in a FAKE simctl's state file and every emulator is a real process
whose arguments name an AVD; the machine's own simulators are never listed
(T11 proves the module refuses to, from a sandboxed HOME). Each removal case is
paired with the case that must NOT remove, because a collector is judged as
much on what it leaves as on what it takes.
"""
import contextlib
import importlib.util
import io
import json
import os
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

HERE = os.path.dirname(os.path.abspath(__file__))
SANDBOX = os.environ["TESTDEVICES_SANDBOX"]

spec = importlib.util.spec_from_file_location("testdevices", os.path.join(HERE, "testdevices.py"))
T = importlib.util.module_from_spec(spec)
spec.loader.exec_module(T)

FAKE_SIMCTL = r'''#!/usr/bin/env python3
import json, os, sys, plistlib
state = os.environ["FAKE_SIMCTL_STATE"]
d = json.load(open(state))
with open(state + ".log", "a") as log:
    log.write(" ".join(sys.argv[1:]) + "\n")
a = sys.argv[1:]
if a[:2] == ["list", "devices"]:
    print(json.dumps({"devices": {"com.apple.CoreSimulator.SimRuntime.iOS-18-6": d["devices"]}}))
    sys.exit(0)
if a[0] == "create":
    udid = "pool-" + str(len(d["devices"]))
    d["devices"].append(dict(udid=udid, name=a[1], state="Shutdown"))
    json.dump(d, open(state, "w"))
    print(udid)
    sys.exit(0)
udid = a[1] if len(a) > 1 else ""
dev = [x for x in d["devices"] if x["udid"] == udid]
if not dev:
    sys.stderr.write("Invalid device: %s\n" % udid); sys.exit(148)
if a[0] == "listapps":
    print(plistlib.dumps({"dev.richos.connect": {"ApplicationType": "User"}, "com.apple.Preferences": {"ApplicationType": "System"}}).decode())
    sys.exit(0)
if a[0] == "boot":
    if dev[0]["state"] == "Booted":
        sys.stderr.write("Unable to boot device in current state: Booted\n"); sys.exit(149)
    dev[0]["state"] = "Booted"
if a[0] == "shutdown":
    dev[0]["state"] = "Shutdown"
elif a[0] == "delete":
    if udid in d.get("undeletable", []):
        sys.stderr.write("Unable to delete: busy\n"); sys.exit(1)
    d["devices"] = [x for x in d["devices"] if x["udid"] != udid]
json.dump(d, open(state, "w"))
'''


class Base(unittest.TestCase):
    n = 0

    def setUp(self):
        import cpu_guard
        guard_state = patch.object(cpu_guard, "STATE", __import__("pathlib").Path(SANDBOX)/"cpu-state")
        guard_state.start()
        self.addCleanup(guard_state.stop)
        admission = patch.object(T, "_device_admission", return_value=[])
        admission.start()
        self.addCleanup(admission.stop)
        Base.n += 1
        self.root = os.path.join(SANDBOX, "case-%02d" % Base.n)
        self.home = os.path.join(self.root, "home")
        self.claude = os.path.join(self.root, "claude")
        self.devdir = os.path.join(self.root, "devices")
        self.android = os.path.join(self.root, "android")
        for d in (self.home, self.claude, self.devdir, self.android):
            os.makedirs(d)
        self.simctl = os.path.join(self.root, "simctl")
        with open(self.simctl, "w") as f:
            f.write(FAKE_SIMCTL)
        os.chmod(self.simctl, 0o755)
        self.state = os.path.join(self.root, "simctl.json")
        self.write_state([])
        self.saved = dict(os.environ)
        os.environ.update({
            "HOME": self.home, "CLAUDE_CONFIG_DIR": self.claude,
            "RICHOS_WORKSPACES_DIR": os.path.join(self.claude, "state", "workspaces"),
            "RICHOS_SIMCTL": self.simctl, "FAKE_SIMCTL_STATE": self.state,
            "RICHOS_SIM_DEVICES_DIR": self.devdir, "RICHOS_ANDROID_CACHES_ROOT": self.android,
        })
        os.environ.pop("RICHOS_TEST_DEVICES_DIR", None)
        os.environ.pop("TEST_DEVICE_FAILURES_STATE", None)
        self.procs = []

    def tearDown(self):
        for p in self.procs:
            try:
                p.kill()
                p.wait()
            except OSError:
                pass
        os.environ.clear()
        os.environ.update(self.saved)

    # --- fixtures -----------------------------------------------------------
    def write_state(self, devices, undeletable=()):
        with open(self.state, "w") as f:
            json.dump({"devices": devices, "undeletable": list(undeletable)}, f)

    def devices(self):
        return json.load(open(self.state))["devices"]

    def calls(self):
        try:
            return open(self.state + ".log").read().splitlines()
        except OSError:
            return []

    def device(self, name, state="Booted", udid=None):
        udid = udid or "%08X-0000-4000-8000-%012X" % (Base.n, len(self.devices()) + 1)
        os.makedirs(os.path.join(self.devdir, udid))
        self.write_state(self.devices() + [{"udid": udid, "name": name, "state": state}],
                         json.load(open(self.state)).get("undeletable", []))
        return udid

    def proc(self, *args):
        p = subprocess.Popen(list(args) or ["sleep", "600"])
        self.procs.append(p)
        time.sleep(0.1)
        return p

    def dead_pid(self):
        p = self.proc()
        p.kill()
        p.wait()
        return p.pid

    def record_checkout(self, path):
        d = os.path.join(os.environ["RICHOS_WORKSPACES_DIR"], "done")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "agent-%d.json" % Base.n), "w") as f:
            json.dump({"key": "k", "workspaces": [{"path": path, "repo": "/nonexistent-repo"}]}, f)

    def rows(self):
        return T.read_failures()

    def ids(self, res, key):
        return sorted(d["id"] for d in res[key])


class Collector(Base):

    def test_run_finalizer_waits_past_the_collectors_lock_window(self):
        device = self.device('rios-ui-mine')
        owner = self.proc()
        with patch.dict(os.environ, {'RICHOS_TEST_DEVICE_RUN_ID': 'mine'}):
            T.register('ios-simulator', device, owner.pid)
        owner.kill(); owner.wait()
        holder = self.hold_registry_lock()
        now = [time.time()]
        real_sleep = time.sleep

        def release_on_wait(seconds):
            if holder.poll() is None:
                now[0] += 2
                holder.kill(); holder.wait()
            else:
                real_sleep(seconds)

        # A real process owns the lock. Expire the collector's window before the
        # first poll. Admission advances the controlled clock past the work budget;
        # the finalizer must still have that budget when it acquires the lock.
        with patch.object(T, 'REGISTRY_LOCK_SECONDS', 0), \
                patch.object(T.time, 'time', side_effect=lambda: now[0]), \
                patch.object(T.time, 'sleep', side_effect=release_on_wait):
            errors = T.cleanup_run_simulators('mine', budget=1)
        self.assertEqual(errors, [])
        self.assertEqual(self.devices(), [])
        self.assertEqual(self.rows(), {})

    def test_run_finalizer_still_fails_when_its_lock_budget_is_exhausted(self):
        device = self.device('rios-ui-locked')
        self.hold_registry_lock()
        with patch.object(T, 'REGISTRY_LOCK_SECONDS', 0), \
                patch.object(T, 'COLLECTOR_BUSY_ALERT_SECONDS', 0), \
                patch.object(T, 'ios_devices', side_effect=AssertionError('lock not held')):
            errors = T.cleanup_run_simulators('mine')
        self.assertTrue(errors)
        self.assertIn('collector:incomplete', self.rows())
        self.assertEqual({d['udid'] for d in self.devices()}, {device})

    def test_run_finalizer_outwaits_the_longest_legitimate_lock_hold(self):
        import inspect
        seen = []

        @contextlib.contextmanager
        def lock(wait_seconds=None):
            seen.append(wait_seconds)
            raise T.RegistryBusy('busy')
            yield

        longest_hold = inspect.signature(T.simctl).parameters['timeout'].default   # one simctl call, under the lock
        os.makedirs(T.registry_dir(), exist_ok=True)   # an absent registry returns before the lock
        with patch.object(T, 'registry_lock', lock):
            self.assertTrue(T.cleanup_run_simulators('mine'))
        self.assertEqual(len(seen), 1)
        self.assertGreater(seen[0], longest_hold)

    def test_run_finalizer_preserves_a_shared_foreign_live_owner_without_failure(self):
        device = self.device('rios-ui-shared')
        owner = self.proc()
        with patch.dict(os.environ, {'RICHOS_TEST_DEVICE_RUN_ID': 'mine'}):
            T.register('ios-simulator', device, owner.pid)
        with patch.dict(os.environ, {'RICHOS_TEST_DEVICE_RUN_ID': 'other'}):
            T.register('ios-simulator', device, os.getpid())
        owner.kill(); owner.wait()
        self.assertEqual(T.cleanup_run_simulators('mine'), [])
        self.assertEqual({d['udid'] for d in self.devices()}, {device})

    def test_run_finalizer_removes_only_its_dead_registered_device(self):
        mine = self.device('rios-ui-mine', udid='10000000-0000-4000-8000-000000000001')
        other = self.device('rios-ui-other', udid='10000000-0000-4000-8000-000000000002')
        live = self.device('rios-ui-live', udid='10000000-0000-4000-8000-000000000003')
        owner = self.proc()
        with patch.dict(os.environ, {'RICHOS_TEST_DEVICE_RUN_ID': 'mine'}):
            T.register('ios-simulator', mine, owner.pid)
            T.register('ios-simulator', live, os.getpid())
        with patch.dict(os.environ, {'RICHOS_TEST_DEVICE_RUN_ID': 'other'}):
            T.register('ios-simulator', other, owner.pid)
        owner.kill(); owner.wait()
        errors = T.cleanup_run_simulators('mine')
        self.assertEqual({d['udid'] for d in self.devices()}, {other, live})
        self.assertTrue(any(live in e for e in errors))

    def test_run_finalizer_without_records_never_queries_simulators(self):
        with patch.object(T, 'ios_devices', side_effect=AssertionError('unexpected inventory')):
            self.assertEqual(T.cleanup_run_simulators('not-a-real-run'), [])

    def test_run_finalizer_without_registry_leaves_records_untouched(self):
        os.rmdir(self.devdir)
        before = os.stat(self.root)
        with patch.object(T, 'ios_devices', side_effect=AssertionError('unexpected inventory')):
            self.assertEqual(T.cleanup_run_simulators('not-a-real-run'), [])
        self.assertFalse(os.path.lexists(self.devdir))
        after = os.stat(self.root)
        self.assertEqual((before.st_mtime_ns, before.st_ctime_ns),
                         (after.st_mtime_ns, after.st_ctime_ns))

    def test_run_finalizer_records_failed_deletion(self):
        device = self.device('rios-ui-busy')
        owner = self.proc()
        with patch.dict(os.environ, {'RICHOS_TEST_DEVICE_RUN_ID': 'busy'}):
            T.register('ios-simulator', device, owner.pid)
        owner.kill(); owner.wait()
        self.write_state(self.devices(), undeletable=[device])
        self.assertTrue(T.cleanup_run_simulators('busy'))
        self.assertTrue(T.read_failures())


    def test_T01_a_simulator_whose_creator_pid_is_gone_is_shut_down_and_deleted(self):
        u = self.device("rios-ui-%d-iPhone-SE" % self.dead_pid())
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "collected"), [u])
        self.assertEqual(self.devices(), [])
        self.assertIn("shutdown %s" % u, self.calls())
        self.assertIn("delete %s" % u, self.calls())
        self.assertEqual(self.rows(), {})

    def test_T02_its_live_creator_keeps_it(self):
        p = self.proc()
        time.sleep(1.2)                                   # the device is made AFTER its creator started
        u = self.device("rios-ui-%d-iPhone-SE" % p.pid)
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "left"), [u])
        self.assertEqual(len(self.devices()), 1)
        self.assertFalse([c for c in self.calls() if not c.startswith("list")])

    def test_T03_a_reused_pid_is_not_its_creator(self):
        u = self.device("rios-ui-99999-iPhone-SE")
        time.sleep(2.2)
        p = self.proc()                                   # started after the device existed
        self.write_state([{"udid": u, "name": "rios-ui-%d-iPhone-SE" % p.pid, "state": "Booted"}])
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "collected"), [u])
        self.assertIn("reused", res["collected"][0]["why"])

    def test_T04_a_registered_cache_is_kept_while_its_owner_runs_and_collected_after(self):
        cache = os.path.join(self.root, "caches", "60e53bf487-app")
        os.makedirs(cache)
        owner = self.proc()
        u = self.device("RichOS native-ios 60e53bf487-app")
        T.register("ios-simulator", u, owner.pid, "native-ios-app.test.sh")
        self.assertEqual(self.ids(T.collect(apply=True), "left"), [u])
        owner.kill()
        owner.wait()
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "collected"), [u])
        self.assertIn("native-ios-app.test.sh", res["collected"][0]["why"])
        self.assertEqual(T.records(), [])                 # nothing left to say: pruned

    def test_T05_a_registered_simulator_goes_with_its_owner_and_an_unregistered_one_is_alerted(self):
        reg = self.device("RichOS native-ios platform-tests simulator")
        T.register("ios-simulator", reg, self.proc().pid, "simulator-tests.sh")
        for p in self.procs:
            p.kill()
            p.wait()
        stray = self.device("RichOS native-ios platform-tests simulator")
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "collected"), [reg])
        self.assertEqual(self.ids(res, "undecided"), [stray])
        self.assertEqual([d["udid"] for d in self.devices()], [stray])     # never deleted on a guess
        row = self.rows()["ios:" + stray]
        self.assertEqual(row["verdict"], "owner cannot be proven")
        self.assertIn("xcrun simctl delete %s" % stray, row["command"])

    def test_T06_a_per_checkout_device_follows_its_recorded_checkout(self):
        co = os.path.join(self.root, "richos-wt", "isaac-opus-n1")
        os.makedirs(co)
        self.record_checkout(co)
        key = T.checkout_keys(co)["rios"]
        u = self.device("RichOS native-ios %s" % key)
        self.assertEqual(self.ids(T.collect(apply=True), "undecided"), [u])
        os.rmdir(co)
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "collected"), [u])
        self.assertIn("no longer exists", res["collected"][0]["why"])

    def test_T06b_an_unknown_checkout_is_undecided_and_alerted_only_while_it_runs(self):
        off = self.device("RichOS mobile loop 0052f60ff2", state="Shutdown")
        on = self.device("RichOS mobile loop 38180311e9", state="Booted")
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "undecided"), sorted([off, on]))
        self.assertEqual(len(self.devices()), 2)
        self.assertEqual(sorted(self.rows()), ["ios:" + on])

    def test_T07_a_land_deleting_the_checkout_takes_its_devices(self):
        co = os.path.join(self.root, "richos-wt", "andy-opus-a2")
        os.makedirs(co)
        self.record_checkout(co)
        u = self.device("RichOS mobile loop %s" % T.checkout_keys(co)["loop"], state="Shutdown")
        self.assertEqual(self.ids(T.collect(apply=True), "undecided"), [u])
        res = T.collect(apply=True, departing=[co])
        self.assertEqual(self.ids(res, "collected"), [u])
        self.assertTrue(os.path.isdir(co))               # the land deletes it, not this

    def test_T08_a_device_that_will_not_go_is_alerted_until_it_is_gone(self):
        u = self.device("rios-ui-%d-iPhone-SE" % self.dead_pid())
        st = json.load(open(self.state))
        self.write_state(st["devices"], undeletable=[u])
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "survivors"), [u])
        self.assertEqual(self.rows()["ios:" + u]["verdict"], "would not close")
        T.collect(apply=True)
        self.assertEqual(self.rows()["ios:" + u]["attempts"], 2)
        self.write_state([])                              # Rich deleted it by hand
        T.collect(apply=True)
        self.assertEqual(self.rows(), {})
        self.assertFalse(os.path.exists(T.failures_path()))

    def test_T09_a_device_outside_our_families_is_never_touched(self):
        self.device("iPhone 16 Pro", state="Booted")
        self.device("RichOS native-ios notahexkey")
        res = T.collect(apply=True)
        self.assertEqual(sum(len(res[k]) for k in ("collected", "left", "undecided", "survivors")), 0)
        self.assertFalse([c for c in self.calls() if not c.startswith("list")])

    def test_T10_an_orphaned_emulator_is_ended_by_its_recorded_pid_and_its_avd_removed(self):
        cache = os.path.join(self.android, "1a2b3c4d5e")
        os.makedirs(os.path.join(cache, "avd", "randroid-1a2b3c4d5e.avd"))
        emu = self.proc(sys.executable, "-c", "import time; time.sleep(600)",
                        "-avd", "randroid-1a2b3c4d5e", "-no-window")
        with open(os.path.join(cache, "emulator.json"), "w") as f:
            json.dump({"serial": "emulator-5580", "pid": emu.pid, "avd": "randroid-1a2b3c4d5e"}, f)
        owner = self.proc()
        T.register("android-emulator", cache, owner.pid, "native-android-app.test.sh")
        self.assertEqual(self.ids(T.collect(apply=True), "left"), [emu.pid])
        owner.kill()
        owner.wait()
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "collected"), [emu.pid])
        emu.wait(timeout=10)
        self.assertIsNotNone(emu.returncode)
        self.assertFalse(os.path.exists(os.path.join(cache, "avd")))
        self.assertFalse(os.path.exists(os.path.join(cache, "emulator.json")))

    def test_T10b_a_recorded_pid_that_is_no_longer_the_emulator_is_never_signaled(self):
        cache = os.path.join(self.android, "9f8e7d6c5b")
        os.makedirs(os.path.join(cache, "avd"))
        bystander = self.proc()                          # some other process now holds the number
        with open(os.path.join(cache, "emulator.json"), "w") as f:
            json.dump({"serial": "emulator-5582", "pid": bystander.pid, "avd": "randroid-9f8e7d6c5b"}, f)
        owner = self.proc()
        T.register("android-cache", cache, owner.pid, "native-android-app.test.sh")
        owner.kill()
        owner.wait()
        res = T.collect(apply=True)
        self.assertEqual(res["collected"] + res["survivors"] + res["undecided"], [])
        self.assertIsNone(bystander.poll())
        self.assertTrue(os.path.isdir(os.path.join(cache, "avd")))

    def test_T11_a_sandboxed_home_never_reads_the_machines_simulators(self):
        os.environ.pop("RICHOS_SIMCTL")
        os.environ.pop("RICHOS_ANDROID_CACHES_ROOT")
        self.assertFalse(T.machine_devices_allowed())
        self.assertEqual(T.ios_devices(), [])
        self.assertEqual(T.android_caches_root(), "")

    def test_T12_a_dry_run_removes_nothing(self):
        u = self.device("rios-ui-%d-iPhone-SE" % self.dead_pid())
        res = T.collect(apply=False)
        self.assertEqual(self.ids(res, "collected"), [u])
        self.assertEqual(len(self.devices()), 1)
        self.assertFalse(os.path.exists(T.failures_path()))

    def test_T13_the_registration_cli_records_the_calling_shell(self):
        owner = self.proc()
        cache = os.path.join(self.root, "c", "abcdef0123-app")
        os.makedirs(cache)
        r = subprocess.run([sys.executable, os.path.join(HERE, "testdevices.py"), "register", "--kind",
                            "ios-cache", "--id", cache, "--owner-pid", str(owner.pid), "--script", "x.sh"],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        rec = T.records()[0]
        self.assertEqual(rec["owner"]["pid"], owner.pid)
        self.assertEqual(rec["owner"]["start"], T.process_start(owner.pid))
        r = subprocess.run([sys.executable, os.path.join(HERE, "testdevices.py"), "register", "--kind",
                            "ios-cache", "--id", cache, "--owner-pid", str(self.dead_pid())],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 2)                 # a dead owner proves nothing

    def test_T14_existing_checkout_without_live_owner_alerts(self):
        co = os.path.join(self.root, "checkout")
        os.makedirs(co)
        self.record_checkout(co)
        u = self.device("RichOS native-ios " + T.checkout_keys(co)["rios"])
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "undecided"), [u])
        self.assertIn("ios:" + u, self.rows())
        self.assertEqual(len(self.devices()), 1)

    def test_T15_stale_cache_never_owns_a_new_simulator(self):
        cache = os.path.join(self.root, "deadbeef01-app")
        owner = self.proc()
        T.register("ios-cache", cache, owner.pid)
        owner.kill(); owner.wait()
        u = self.device("RichOS native-ios deadbeef01-app")
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "undecided"), [u])
        self.assertIn("ios:" + u, self.rows())

    def test_T16_inventory_failure_alerts_and_success_resolves(self):
        with patch.object(T, "ios_devices", return_value=None):
            res = T.collect(apply=True)
        self.assertTrue(res["notes"])
        self.assertIn("collector:incomplete", self.rows())
        T.collect(apply=True)
        self.assertEqual(self.rows(), {})

    def test_T17_inventory_obeys_the_entire_deadline(self):
        with open(self.simctl, "w") as f:
            f.write("#!/usr/bin/env python3\nimport time\ntime.sleep(10)\n")
        start = time.monotonic()
        res = T.collect(apply=True, deadline=time.time() + 0.2)
        self.assertLess(time.monotonic() - start, 1.2)
        self.assertTrue(res["notes"])
        self.assertIn("collector:incomplete", self.rows())

    def test_T18_collector_exception_is_durable(self):
        with patch.object(T, "records", side_effect=ValueError("corrupt registry")):
            res = T.collect(apply=True)
        self.assertIn("corrupt registry", res["notes"][0])
        self.assertIn("corrupt registry", self.rows()["collector:incomplete"]["why"])

    def test_T19_all_live_owners_are_preserved(self):
        u = self.device("RichOS native-ios platform-tests shared")
        first, second = self.proc(), self.proc()
        T.register("ios-simulator", u, first.pid)
        T.register("ios-simulator", u, second.pid)
        second.kill(); second.wait()
        self.assertEqual(self.ids(T.collect(apply=True), "left"), [u])
        first.kill(); first.wait()
        self.assertEqual(self.ids(T.collect(apply=True), "collected"), [u])

    def test_T20_changed_android_generation_never_inherits_dead_owner(self):
        cache = os.path.join(self.android, "abcdef1234")
        os.makedirs(os.path.join(cache, "avd"))
        avd = "randroid-abcdef1234"
        first = self.proc(sys.executable, "-c", "import time; time.sleep(600)", "-avd", avd)
        path = os.path.join(cache, "emulator.json")
        with open(path, "w") as f:
            json.dump({"pid": first.pid, "avd": avd}, f)
        owner = self.proc()
        T.register("android-emulator", cache, owner.pid)
        owner.kill(); owner.wait()
        second = self.proc(sys.executable, "-c", "import time; time.sleep(600)", "-avd", avd)
        with open(path, "w") as f:
            json.dump({"pid": second.pid, "avd": avd}, f)
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "undecided"), [second.pid])
        self.assertIsNone(second.poll())
        T.register("android-emulator", cache, self.proc().pid)
        self.assertEqual(self.ids(T.collect(apply=True), "left"), [second.pid])

    def test_T21_android_avd_delete_failure_is_not_reported_as_success(self):
        cache = os.path.join(self.android, "remove-failure")
        os.makedirs(os.path.join(cache, "avd"))
        T._write_json(os.path.join(cache, "emulator.json"), {"pid": 987654321, "avd": "x"})
        with patch.object(T, "process_start", return_value=""), \
             patch.object(T, "_remove_avd", side_effect=PermissionError("denied")):
            gone, why = T._android_remove({"pid": 987654321, "avd": "x", "cache": cache})
        self.assertFalse(gone)
        self.assertIn("could not be removed", why)

    def test_T22_slow_removal_preserves_deadline_and_alert(self):
        u = self.device("rios-ui-%d-test" % self.dead_pid())
        original = open(self.simctl).read()
        with open(self.simctl, "w") as f:
            f.write(original.replace('if a[0] == "shutdown":', 'if a[0] == "shutdown":\n    import time; time.sleep(10)'))
        start = time.monotonic()
        T.collect(apply=True, deadline=time.time() + 0.3)
        self.assertLess(time.monotonic() - start, 1.3)
        self.assertIn("collector:incomplete", self.rows())
        self.assertIn(u, [d["udid"] for d in self.devices()])

    def test_T23_private_device_set_is_scanned_and_removed(self):
        private = os.path.join(self.root, "private-devices")
        os.makedirs(private)
        u = "PRIVATE-SIM-ONLY"
        fake = self.state + ".private"
        with open(fake, "w") as f:
            json.dump({"devices": [{"udid": u, "name": "test private", "state": "Booted"}]}, f)
        original = open(self.simctl).read()
        original = original.replace("d = json.load(open(state))", "a = sys.argv[1:]\nif a[:1] == ['--set']:\n    state += '.private'\n    sys.argv = [sys.argv[0]] + a[2:]\nd = json.load(open(state))")
        with open(self.simctl, "w") as f:
            f.write(original)
        owner = self.proc()
        T.register("ios-simulator", u, owner.pid, device_set=private)
        owner.kill(); owner.wait()
        res = T.collect(apply=True)
        self.assertEqual(self.ids(res, "collected"), [u])
        self.assertEqual(json.load(open(fake))["devices"], [])

    def test_T24_agent_end_collects_even_when_checkout_remains(self):
        checkout = os.path.join(self.root, "checkout")
        os.makedirs(checkout)
        wd = os.environ["RICHOS_WORKSPACES_DIR"]
        parent = self.proc()
        T._write_json(os.path.join(wd, "sessions", "parent.json"),
                      {"pid": parent.pid, "pid_start": T.process_start(parent.pid)})
        path = os.path.join(wd, "agents", "parent-child.json")
        rec = {"key": "parent-child", "session_id": "parent", "agent_id": "child",
               "workspaces": [{"path": checkout}]}
        T._write_json(path, rec)
        u = self.device("RichOS native-ios " + T.checkout_keys(checkout)["rios"])
        T.register("ios-simulator", u, checkout=checkout)
        self.assertEqual(self.ids(T.collect(apply=True), "left"), [u])
        rec["end"] = {"signal": "stopped", "at": time.time()}
        T._write_json(path, rec)
        self.assertEqual(self.ids(T.collect(apply=True), "collected"), [u])
        self.assertTrue(os.path.isdir(checkout))
        self.assertIsNone(parent.poll())

    def test_T25_dead_emulator_still_has_its_avd_collected(self):
        cache = os.path.join(self.android, "dead-emulator")
        os.makedirs(os.path.join(cache, "avd"))
        avd = "randroid-dead"
        proc = self.proc(sys.executable, "-c", "import time; time.sleep(600)", "-avd", avd)
        owner = self.proc()
        T._write_json(os.path.join(cache, "emulator.json"), {"pid": proc.pid, "avd": avd})
        T.register("android-emulator", cache, owner.pid)
        proc.kill(); proc.wait(); owner.kill(); owner.wait()
        self.assertEqual(self.ids(T.collect(apply=True), "collected"), [proc.pid])
        self.assertFalse(os.path.exists(os.path.join(cache, "avd")))

    def test_T26_launch_publishes_identity_and_retains_existing_owners(self):
        cache = os.path.join(self.android, "launched")
        avd = "randroid-launched"
        owner = self.proc()
        command = [sys.executable, "-c", "import time; time.sleep(600)", "-avd", avd]
        pid = T.launch_android(cache, avd, 5556, command, owner_pid=owner.pid)
        try:
            self.assertEqual(T.launch_android(cache, avd, 5556, command, owner_pid=os.getpid()), pid)
            self.assertEqual(len(T.records()[0]["owners"]), 2)
            self.assertEqual(self.ids(T.collect(apply=True), "left"), [pid])
        finally:
            os.kill(pid, 9)
            os.waitpid(pid, 0)

    def test_T27_failed_registration_terminates_new_emulator(self):
        cache = os.path.join(self.android, "failed-launch")
        avd = "randroid-failed"
        with patch.object(T, "register", side_effect=ValueError("cannot register")):
            with self.assertRaisesRegex(ValueError, "cannot register"):
                T.launch_android(cache, avd, 5556,
                    [sys.executable, "-c", "import time; time.sleep(600)", "-avd", avd])
        pid = T._read_json(os.path.join(cache, "emulator.json"))["pid"]
        self.assertEqual(T.process_start(pid), "")

    def test_T28_registry_lock_wait_is_bounded_and_visible(self):
        os.makedirs(T.registry_dir(), exist_ok=True)
        command = "import fcntl,sys,time; f=open(sys.argv[1],'a'); fcntl.flock(f,fcntl.LOCK_EX); print('ready',flush=True); time.sleep(600)"
        proc = subprocess.Popen([sys.executable, "-c", command, os.path.join(T.registry_dir(), ".lock")], stdout=subprocess.PIPE, text=True)
        self.procs.append(proc)
        self.assertEqual(proc.stdout.readline().strip(), "ready")
        start = time.monotonic()
        res = T.collect(apply=True, deadline=time.time() + .2)
        self.assertLess(time.monotonic() - start, 1.2)
        self.assertTrue(res["notes"])
        self.assertIn("collector:incomplete", self.rows())

    def test_T29_unreadable_emulator_inventory_alerts(self):
        cache = os.path.join(self.android, "broken")
        os.makedirs(cache)
        with open(os.path.join(cache, "emulator.json"), "w") as f:
            f.write("broken json")
        res = T.collect(apply=True)
        self.assertTrue(res["notes"])
        self.assertIn("collector:incomplete", self.rows())

    def test_T30_pid_reused_during_removal_is_never_signaled(self):
        cache = os.path.join(self.android, "pid-reused")
        os.makedirs(cache)
        T._write_json(os.path.join(cache, "emulator.json"), {"pid": 123, "avd": "x"})
        with patch.object(T, "process_start", return_value="new start"), patch.object(T.os, "kill") as kill:
            gone, why = T._android_remove({"pid": 123, "avd": "x", "cache": cache, "start": "old start"})
        self.assertFalse(gone)
        kill.assert_not_called()

    def test_T31_explicit_stop_checks_generation_and_can_preserve_avd(self):
        cache = os.path.join(self.android, "explicit-stop")
        os.makedirs(os.path.join(cache, "avd"))
        avd = "randroid-explicit"
        command = [sys.executable, "-c", "import time; time.sleep(600)", "-avd", avd]
        pid = T.launch_android(cache, avd, 5556, command, owner_pid=os.getpid())
        path = os.path.join(cache, "emulator.json")
        record = T._read_json(path)
        try:
            T._write_json(path, dict(record, start="not this generation"))
            with self.assertRaisesRegex(ValueError, "unproven"):
                T.stop_android(cache)
            self.assertTrue(T.process_start(pid))
            T._write_json(path, record)
            T.stop_android(cache)
            self.assertEqual(T.process_start(pid), "")
            self.assertTrue(os.path.isdir(os.path.join(cache, "avd")))
            T.stop_android(cache, delete=True)
            self.assertFalse(os.path.exists(os.path.join(cache, "avd")))
        finally:
            if T.process_start(pid):
                os.kill(pid, 9)
            try:
                os.waitpid(pid, 0)
            except ChildProcessError:
                pass

    def test_T32_interrupted_collection_leaves_an_alert(self):
        with patch.object(T, "_collect", side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                T.collect(apply=True)
        self.assertIn("has not completed", self.rows()["collector:incomplete"]["why"])
        T.collect(apply=True)
        self.assertEqual(self.rows(), {})

    def alerts(self):
        spec = importlib.util.spec_from_file_location("device_alerts_reader", os.path.join(HERE, "testdevice_alerts.py"))
        A = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(A)
        return A

    def test_T32b_a_collection_still_running_is_not_a_failure(self):
        # 2026-10-24 23:50Z the watchdog published `test_device_failures 1` from the
        # start mark of a collection that was simply still running.
        seen = {}

        def running(*_args):
            seen["raw"] = self.rows()
            seen["standing"] = self.alerts().standing(seen["raw"])
            return {k: [] for k in ("collected", "survivors", "left", "undecided", "deferred")}
        with patch.object(T, "_collect", side_effect=running):
            T.collect(apply=True)
        self.assertIn("collector:incomplete", seen["raw"])        # the mark is still written
        self.assertEqual(seen["standing"], {})

    def test_T32c_an_abandoned_or_overdue_collection_is_still_a_failure(self):
        A = self.alerts()
        A.record_running("collect")
        row = self.rows()["collector:incomplete"]
        self.assertEqual(A.standing(self.rows()), {})
        dead = self.dead_pid()
        abandoned = A.standing({"collector:incomplete": dict(row, pid=dead)})
        self.assertEqual(abandoned["collector:incomplete"]["verdict"], "collection abandoned")
        overdue = A.standing({"collector:incomplete": row}, now=row["due"] + A.GRACE_SECONDS)
        self.assertEqual(overdue["collector:incomplete"]["verdict"], "collection timed out")

    def test_T32d_a_new_run_does_not_mask_a_standing_collector_failure(self):
        T.record_collector_failure("corrupt registry")
        seen = {}

        def running(*_args):
            seen["standing"] = self.alerts().standing(self.rows())
            raise RuntimeError("still broken")
        with patch.object(T, "_collect", side_effect=running):
            T.collect(apply=True)
        self.assertIn("corrupt registry", seen["standing"]["collector:incomplete"]["why"])

    def test_T32e_an_abandoned_or_overdue_mark_is_not_recovered_by_the_next_start(self):
        A = self.alerts()
        for case in ("abandoned", "timed out"):
            A.record_running("collect")
            row = self.rows()["collector:incomplete"]
            if case == "abandoned":
                row["pid"] = self.dead_pid()
            else:
                row["due"] -= A.GRACE_SECONDS
            T._write_json(T.failures_path(), {"collector:incomplete": row})
            seen = {}

            def running(*_args):
                seen["standing"] = A.standing(self.rows())
                return {k: [] for k in ("collected", "survivors", "left", "undecided", "deferred")}
            with patch.object(T, "_collect", side_effect=running):
                T.collect(apply=True)
            self.assertEqual(seen["standing"]["collector:incomplete"]["verdict"], "collection " + case)
            T.collect(apply=True)                                   # a real pass succeeds
            self.assertEqual(self.rows(), {})

    def test_T32f_a_long_collection_whose_operations_progress_is_not_timed_out(self):
        # Review rv-20261025T010507Z-164da3c3-7552#1: a pass with no total deadline ran six
        # simctl operations of 31 s each, read "collection timed out" at 186 s, then finished
        # clean. The bound is each operation's own, not a total.
        clock = [time.time()]
        seen = {}

        def progressing(*_args):
            A = T._MARK.get()
            for _ in range(6):
                T._timeout(120)                 # a simctl operation starts, bounded at 120 s
                clock[0] += 31                  # ...and succeeds 31 s later
            seen["progressing"] = A.standing(self.rows(), now=clock[0])
            T._timeout(120)                     # one more starts, and then nothing more happens
            seen["hung"] = A.standing(self.rows(), now=clock[0] + 120 + A.GRACE_SECONDS)
            return {k: [] for k in ("collected", "survivors", "left", "undecided", "deferred")}
        with patch.object(time, "time", lambda: clock[0]), \
                patch.object(T, "_collect", side_effect=progressing):
            T.collect(apply=True)
        self.assertEqual(seen["progressing"], {})
        self.assertEqual(seen["hung"]["collector:incomplete"]["verdict"], "collection timed out")

    def test_T33_identity_read_failure_after_signal_preserves_avd(self):
        cache = os.path.join(self.android, "unreadable-after-signal")
        os.makedirs(os.path.join(cache, "avd"))
        T._write_json(os.path.join(cache, "emulator.json"), {"pid": 123, "avd": "x"})
        with patch.object(T, "process_start", side_effect=["same", None]), \
             patch.object(T, "_names_avd", return_value=True), patch.object(T.os, "kill"):
            gone, why = T._android_remove({"pid": 123, "avd": "x", "cache": cache, "start": "same"})
        self.assertFalse(gone)
        self.assertIn("unreadable", why)
        self.assertTrue(os.path.isdir(os.path.join(cache, "avd")))
        self.assertTrue(os.path.isfile(os.path.join(cache, "emulator.json")))

    def test_T34_avd_removal_receives_the_remaining_budget(self):
        token = T._DEADLINE.set(time.time() + .2)
        try:
            with patch.object(T.subprocess, "run", side_effect=subprocess.TimeoutExpired("remove", .2)) as run:
                with self.assertRaises(subprocess.TimeoutExpired):
                    T._remove_avd(os.path.join(self.android, "avd"))
            self.assertGreater(run.call_args.kwargs["timeout"], 0)
            self.assertLessEqual(run.call_args.kwargs["timeout"], .2)
        finally:
            T._DEADLINE.reset(token)

    def test_T35_corrupt_previous_alert_cannot_hide_a_new_failure(self):
        T._write_json(T.failures_path(), {"collector:incomplete": "corrupt row"})
        with patch.object(T, "records", side_effect=ValueError("bad registry")):
            T.collect(apply=True)
        self.assertIn("bad registry", self.rows()["collector:incomplete"]["why"])

    def test_T36_registration_refuses_a_changed_recorded_generation(self):
        cache = os.path.join(self.android, "wrong-generation")
        os.makedirs(cache)
        avd = "randroid-wrong"
        proc = self.proc(sys.executable, "-c", "import time; time.sleep(600)", "-avd", avd)
        T._write_json(os.path.join(cache, "emulator.json"), {"pid": proc.pid, "avd": avd, "start": "old"})
        with self.assertRaisesRegex(ValueError, "generation changed"):
            T.register("android-emulator", cache, os.getpid())
        self.assertEqual(T.records(), [])
        self.assertIsNone(proc.poll())

    def test_T37_android_inventory_failure_does_not_block_ios_cleanup(self):
        u = self.device("rios-ui-%d-independent" % self.dead_pid())
        previous = {"android:42": {"kind": "android", "id": 42, "name": "prior emulator", "why": "prior failure"}}
        T._write_json(T.failures_path(), previous)
        with patch.object(T, "android_emulators", side_effect=PermissionError("external volume denied")):
            result = T.collect(apply=True)
        self.assertEqual(self.ids(result, "collected"), [u])
        self.assertIn("external volume denied", self.rows()["collector:incomplete"]["why"])
        self.assertIn("android:42", self.rows())
        self.assertEqual(self.devices(), [])

    def test_T38_activity_does_not_extend_maximum_lifetime(self):
        udid = self.device("rios-ui-lease")
        rec = T.register("ios-simulator", udid, os.getpid())
        created = rec["lease"]["created"]
        T.touch_lease("ios-simulator", udid)
        fresh = T.records()[0]
        self.assertEqual(fresh["lease"]["created"], created)
        self.assertGreaterEqual(fresh["lease"]["last_use"], rec["lease"]["last_use"])
        renewed = T.register("ios-simulator", udid, os.getpid())
        self.assertEqual(renewed["lease"]["created"], created)

    def test_T39_expired_live_owned_simulator_is_removed_but_personal_device_is_not(self):
        import cpu_guard
        udid = self.device("rios-ui-lease")
        personal = self.device("My iPhone")
        rec = T.register("ios-simulator", udid, os.getpid())
        rec["lease"]["last_use"] = time.time() - 301
        T._write_json(T._record_path("ios-simulator", udid), rec)
        with patch.object(cpu_guard, "event") as event:
            self.assertEqual(T.expire_leases(), 0)
            event.assert_called_once()
        self.assertEqual([d["udid"] for d in self.devices()], [personal])
        self.assertEqual(T.records(), [])

    def test_T40_pressure_sheds_registered_simulator_without_guessing_service_ownership(self):
        import cpu_guard
        udid = self.device("rios-ui-pressure")
        personal = self.device("My iPhone")
        T.register("ios-simulator", udid, os.getpid())
        with patch.object(cpu_guard, "event"):
            self.assertEqual(T.expire_leases(pressure=True), 0)
        self.assertEqual([d["udid"] for d in self.devices()], [personal])

    @unittest.skipUnless(sys.platform == 'darwin', 'Apple plist converter')
    def test_simctl_openstep_application_list_is_parsed(self):
        apps = T.simulator_apps('{ "dev.richos.connect" = { ApplicationType = User; }; }')
        self.assertEqual(apps['dev.richos.connect']['ApplicationType'], 'User')

    def test_stale_prepared_cache_cannot_use_another_run_lease(self):
        udid = T.acquire_ios("iPhone", "runtime", os.getpid())
        T.use_ios(udid, os.getpid())
        other = subprocess.Popen(["sleep", "30"])
        try:
            with self.assertRaises(ValueError): T.use_ios(udid, other.pid)
        finally:
            other.terminate(); other.wait()
        T.release_ios(udid, os.getpid())
        with self.assertRaises(ValueError): T.use_ios(udid, os.getpid())

    def test_prepared_os_is_reused_and_app_state_reset_without_erase(self):
        first = T.acquire_ios("iPhone", "runtime", os.getpid())
        path = os.path.join(T.prepared_dir(), T.records()[0]['prepared'] + '.json')
        self.assertNotIn('boot_completed_at', T._read_json(path))
        T.boot_ios(first)
        first_boot = T.records()[0]['boot']
        self.assertEqual(first_boot['phase'], 'ready')
        self.assertEqual(first_boot['deadline'] - first_boot['started'], 180)
        self.assertIn('boot_completed_at', T._read_json(path))
        T.release_ios(first, os.getpid())
        second = T.acquire_ios("iPhone", "runtime", os.getpid())
        T.boot_ios(second)
        warm_boot = T.records()[0]['boot']
        self.assertEqual(warm_boot['deadline'] - warm_boot['started'], 120)
        T.release_ios(second, os.getpid())
        self.assertEqual(first, second)
        self.assertEqual(self.devices()[0]["state"], "Shutdown")
        log = open(self.state + ".log").read().splitlines()
        self.assertEqual(sum(x.startswith("create ") for x in log), 1)
        self.assertEqual(sum(x == "uninstall " + first + " dev.richos.connect" for x in log), 2)
        self.assertEqual(sum(x == "keychain " + first + " reset" for x in log), 2)
        self.assertFalse(any(x.startswith(("erase ", "delete ")) for x in log))
        self.assertFalse(any("uninstall " + first + " com.apple" in x for x in log))

    def test_failed_first_boot_does_not_claim_prepared_os_or_reset_deadline(self):
        first = T.acquire_ios("iPhone", "runtime", os.getpid())
        path = os.path.join(T.prepared_dir(), T.records()[0]['prepared'] + '.json')
        original = T.checked_simctl
        def fail_bootstatus(*args, **kwargs):
            if args[0] == 'bootstatus':
                rec = T.records()[0]
                self.assertEqual(rec['boot']['phase'], 'starting')
                self.assertEqual(T._DEADLINE.get(), rec['boot']['deadline'])
                raise RuntimeError('boot did not complete')
            return original(*args, **kwargs)
        with patch.object(T, 'checked_simctl', side_effect=fail_bootstatus):
            with self.assertRaisesRegex(RuntimeError, 'did not complete'): T.boot_ios(first)
        self.assertNotIn('boot_completed_at', T._read_json(path))
        self.assertEqual(self.devices()[0]['state'], 'Shutdown')
        self.assertEqual(T.records()[0]['boot']['phase'], 'failed')
        self.assertIsNone(T._DEADLINE.get())

    def test_duplicate_boot_cannot_extend_startup_deadline(self):
        first = T.acquire_ios("iPhone", "runtime", os.getpid())
        path = T._record_path('ios-simulator', first)
        rec = T._read_json(path)
        rec['boot'] = dict(phase='starting', started=100, deadline=280)
        T._write_json(path, rec)
        with self.assertRaisesRegex(RuntimeError, 'already starting'): T.boot_ios(first)
        self.assertEqual(T._read_json(path)['boot']['deadline'], 280)

    def test_failed_startup_cleanup_checks_shutdown_state_before_blocking_other_runs(self):
        import cpu_guard
        for state in ('Shutdown', 'Booted', 'unreadable'):
            with self.subTest(state=state):
                udid = T.acquire_ios('iPhone', 'runtime', os.getpid())
                checked, simctl = T.checked_simctl, T.simctl
                failed = False
                def fail_boot(*args, **kwargs):
                    nonlocal failed
                    if args[0] == 'boot':
                        failed = True
                        self.write_state([dict(d, state=state) for d in self.devices()])
                        raise RuntimeError('original startup failure')
                    return checked(*args, **kwargs)
                def cleanup(*args, **kwargs):
                    if failed and args[0] == 'shutdown':
                        return 405, '', 'Unable to shutdown device in current state: Shutdown'
                    if failed and state == 'unreadable' and args[0] == 'list':
                        return 1, '', 'inventory unavailable'
                    return simctl(*args, **kwargs)
                with patch.object(T, 'checked_simctl', side_effect=fail_boot), \
                        patch.object(T, 'simctl', side_effect=cleanup), \
                        patch.object(cpu_guard, 'block_ios') as blocked:
                    with self.assertRaisesRegex(RuntimeError, 'original startup failure'):
                        T.boot_ios(udid)
                    if state == 'Shutdown':
                        blocked.assert_not_called()
                        self.assertEqual(T._read_json(T._record_path('ios-simulator', udid))['boot']['phase'], 'failed')
                    else:
                        blocked.assert_called_once()
                        self.assertIn('Failed startup cleanup', blocked.call_args.args[0])
                T.release_ios(udid, os.getpid())

    def test_collected_boot_cannot_resurrect_its_lease(self):
        first = T.acquire_ios("iPhone", "runtime", os.getpid())
        original = T.checked_simctl
        def collected(*args, **kwargs):
            result = original(*args, **kwargs)
            if args[0] == 'bootstatus':
                os.unlink(T._record_path('ios-simulator', first))
            return result
        with patch.object(T, 'checked_simctl', side_effect=collected):
            with self.assertRaisesRegex(RuntimeError, 'lease ended'): T.boot_ios(first)
        self.assertEqual(T.records(), [])
        self.assertEqual(self.devices()[0]['state'], 'Shutdown')

    def test_prepared_lease_excludes_other_owner_and_other_device_type(self):
        first = T.acquire_ios("iPhone", "runtime", os.getpid())
        other = subprocess.Popen(["sleep", "30"])
        try:
            with self.assertRaises(TimeoutError):
                T.acquire_ios("iPhone", "runtime", other.pid, timeout=0)
            with self.assertRaises(TimeoutError):
                T.acquire_ios("iPad", "runtime", os.getpid(), timeout=0)
            with self.assertRaises(ValueError):
                T.release_ios(first, other.pid)
            with self.assertRaises(ValueError):
                T.register("ios-simulator", first, other.pid)
        finally:
            other.terminate(); other.wait()
        T.release_ios(first, os.getpid())

    def test_prepared_pressure_and_dead_owner_keep_os_but_end_lease(self):
        import cpu_guard
        for pressure in (True, False):
            udid = T.acquire_ios("iPhone", "runtime", os.getpid())
            T.boot_ios(udid)
            rec = T._read_json(T._record_path("ios-simulator", udid))
            rec["lease"]["last_use"] = time.time() - 301
            T._write_json(T._record_path("ios-simulator", udid), rec)
            with patch.object(cpu_guard, "event"):
                self.assertEqual(T.expire_leases(pressure), 0)
            self.assertEqual(self.devices()[0]["state"], "Shutdown")
            self.assertEqual(T.records(), [])

    def test_lease_hold_survives_one_unreadable_process_start(self):
        ident = "emu-lease-unknown"
        path = T._record_path("android-emulator", ident)
        T._write_json(path, {"lease": {"created": 1}, "generation": {"pid": 4242, "start": "Mon Oct  4 10:00:00 2026"}})
        answers = iter([None, "Mon Oct  4 10:00:00 2026", "Mon Oct  4 10:00:00 2026"])
        seen = []

        def ps(pid):
            seen.append(pid)
            return next(answers)

        def nap(_):
            if len(seen) >= 3:
                os.unlink(path)          # only the record's removal may end the loop here

        with patch.object(T, "process_start", side_effect=ps), patch.object(T.time, "sleep", side_effect=nap):
            self.assertEqual(T.hold_lease("android-emulator", ident), 0)
        self.assertEqual(len(seen), 3, "an unknown answer ended the lease early")

    def test_pressure_shutdown_does_not_need_owner_process_listing(self):
        import cpu_guard
        udid = self.device("old-checkout-simulator")
        rec = T.register("ios-simulator", udid, os.getpid())
        rec.pop("lease")  # The exact record shape before CPU guard landed.
        T._write_json(T._record_path("ios-simulator", udid), rec)
        with patch.object(T, "process_start", side_effect=AssertionError("ps is unavailable")), patch.object(cpu_guard, "event"):
            self.assertEqual(T.expire_leases(pressure=True), 0)
        log = open(self.state + ".log").read().splitlines()
        self.assertTrue(log[0].startswith("shutdown "))
        self.assertEqual(self.devices(), [])

    def test_T41_dead_lease_holder_is_collected_even_with_live_owner(self):
        udid = self.device("rios-ui-holder")
        rec = T.register("ios-simulator", udid, os.getpid())
        rec["lease"]["holder"] = {"pid": self.dead_pid(), "start": "old birth"}
        self.assertEqual(T._registered_verdict(rec)[0], T.COLLECT)

    def test_T42_expired_emulator_is_stopped_with_live_owner(self):
        import cpu_guard
        cache = os.path.join(self.android, "lease-expiry")
        avd = "randroid-lease-expiry"
        pid = T.launch_android(cache, avd, 5556,
            [sys.executable, "-c", "import time; time.sleep(600)", "-avd", avd], owner_pid=os.getpid())
        try:
            rec = T.records()[0]
            rec["lease"]["created"] = time.time() - 901
            T._write_json(T._record_path("android-emulator", cache), rec)
            with patch.object(cpu_guard, "event"):
                self.assertEqual(T.expire_leases(), 0)
            self.assertEqual(T.process_start(pid), "")
            self.assertFalse(os.path.exists(os.path.join(cache, "emulator.json")))
        finally:
            try: os.kill(pid, 9)
            except ProcessLookupError: pass
            try: os.waitpid(pid, 0)
            except ChildProcessError: pass

    def test_T43_expired_lease_cannot_be_renewed_by_touch(self):
        udid = self.device("rios-ui-expired")
        rec = T.register("ios-simulator", udid, os.getpid())
        rec["lease"]["created"] = time.time() - 901
        T._write_json(T._record_path("ios-simulator", udid), rec)
        with self.assertRaisesRegex(ValueError, "expired"):
            T.touch_lease("ios-simulator", udid)

    # --- an owned run keeps its lease (escalation esc-20260924T220236Z-52fae3ec) ---
    def run_active(self, udid, owner_pid, *command, interval="0.3"):
        """The renewer as its callers start it: the CLI, in this sandbox."""
        p = subprocess.Popen([sys.executable, "-B", os.path.join(HERE, "testdevices.py"), "run-active",
                              "--kind", "ios-simulator", "--id", udid, "--owner-pid", str(owner_pid),
                              "--interval", interval, "--"] + list(command),
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.procs.append(p)
        return p

    def lease(self, udid):
        return (T._read_json(T._record_path("ios-simulator", udid)) or {}).get("lease")

    def test_T44_an_owned_run_holds_its_lease_past_300_s_of_cli_silence(self):
        import cpu_guard
        udid = self.device("rios-ui-active")
        rec = T.register("ios-simulator", udid, os.getpid())
        silent_since = time.time() - (T.LEASE_IDLE_SECONDS - 3)   # the CLI's last touch
        rec["lease"]["last_use"] = silent_since
        T._write_json(T._record_path("ios-simulator", udid), rec)
        runner = self.run_active(udid, os.getpid(), "sleep", "5")
        time.sleep(4)                                              # 301 s after the CLI's last touch
        self.assertGreaterEqual(time.time() - silent_since, T.LEASE_IDLE_SECONDS + 1)
        with patch.object(cpu_guard, "event") as event:
            self.assertEqual(T.expire_leases(), 0)
            event.assert_not_called()
        self.assertEqual([d["state"] for d in self.devices()], ["Booted"])
        lease = self.lease(udid)
        self.assertEqual(lease["created"], rec["lease"]["created"])
        self.assertEqual(lease["max_seconds"], T.LEASE_MAX_SECONDS)
        self.assertEqual(lease["activity"]["renewer"], runner.pid)
        self.assertEqual(runner.wait(timeout=10), 0)
        # The run is over: renewal stops with it, and silence counts again.
        ended = self.lease(udid)["last_use"]
        time.sleep(0.8)
        self.assertEqual(self.lease(udid)["last_use"], ended)
        self.assertTrue(T.lease_expired({"lease": self.lease(udid)}, ended + T.LEASE_IDLE_SECONDS))

    def test_T45_an_owned_run_whose_owner_dies_is_collected_and_no_longer_renewed(self):
        import cpu_guard
        owner = self.proc()
        udid = self.device("rios-ui-owner-dies")
        T.register("ios-simulator", udid, owner.pid)
        runner = self.run_active(udid, owner.pid, "sleep", "4")
        time.sleep(1)
        self.assertIn("activity", self.lease(udid))
        owner.kill()
        owner.wait()
        time.sleep(0.8)
        stopped = self.lease(udid)["last_use"]
        time.sleep(0.8)
        self.assertEqual(self.lease(udid)["last_use"], stopped)      # the renewer let go
        self.assertIsNone(runner.poll())                             # while its run still runs
        with patch.object(cpu_guard, "event"):
            self.assertEqual(T.expire_leases(), 0)
        self.assertEqual(self.devices(), [])
        self.assertEqual(T.records(), [])
        runner.wait(timeout=10)
        self.assertIn("not proven alive", runner.stderr.read())

    def test_T46_an_owned_run_that_renews_forever_ends_at_the_ceiling(self):
        # P5-23: the lifetime no longer cuts off a run whose renewer vouches for it; a run
        # that hangs yet stays alive is still ended, at LEASE_ACTIVE_CEILING_SECONDS.
        import cpu_guard
        udid = self.device("rios-ui-runaway")
        rec = T.register("ios-simulator", udid, os.getpid())
        rec["lease"]["created"] = time.time() - (T.LEASE_ACTIVE_CEILING_SECONDS - 1.5)
        rec["lease"]["activity"] = {"pid": 1, "renewer": 1, "renewed": time.time()}   # already renewed once
        T._write_json(T._record_path("ios-simulator", udid), rec)
        runner = self.run_active(udid, os.getpid(), "sleep", "4")
        time.sleep(0.8)
        self.assertEqual(self.lease(udid)["created"], rec["lease"]["created"])
        self.assertIn("activity", self.lease(udid))
        time.sleep(1.2)
        with patch.object(cpu_guard, "event"):
            self.assertEqual(T.expire_leases(), 0)
        self.assertEqual(self.devices(), [])
        runner.wait(timeout=10)

    def test_T46b_renewal_refuses_an_expired_lease(self):
        udid = self.device("rios-ui-expired-renewal")
        rec = T.register("ios-simulator", udid, os.getpid())
        owner = T.choose_owner(os.getpid())
        live = self.proc()
        rec["lease"]["created"] = time.time() - T.LEASE_MAX_SECONDS
        T._write_json(T._record_path("ios-simulator", udid), rec)
        renewed, why = T.renew_activity("ios-simulator", udid, owner, rec["lease"]["created"], live)
        self.assertFalse(renewed)
        self.assertIn("lifetime", why)
        self.assertNotIn("activity", self.lease(udid))

    def test_T47_a_killed_renewer_renews_nothing(self):
        udid = self.device("rios-ui-renewer-killed")
        T.register("ios-simulator", udid, os.getpid())
        runner = self.run_active(udid, os.getpid(), "sleep", "3")
        time.sleep(0.8)
        runner.kill()                    # the renewer, by the pid captured at its spawn
        runner.wait()
        stopped = self.lease(udid)["last_use"]
        time.sleep(0.8)
        self.assertEqual(self.lease(udid)["last_use"], stopped)
        time.sleep(1.5)                  # its orphaned `sleep 3` ends by itself

    def test_T48_run_active_refuses_a_foreign_missing_or_expired_lease_and_runs_nothing(self):
        marker = os.path.join(self.root, "ran")
        udid = self.device("rios-ui-refused")
        other = self.proc()
        missing = self.run_active(udid, os.getpid(), "touch", marker)
        self.assertEqual(missing.wait(timeout=10), 2)
        T.register("ios-simulator", udid, other.pid)
        foreign = self.run_active(udid, os.getpid(), "touch", marker)
        self.assertEqual(foreign.wait(timeout=10), 2)
        self.assertIn("another run", foreign.stderr.read())
        rec = T.register("ios-simulator", udid, os.getpid())
        rec["lease"]["last_use"] = time.time() - T.LEASE_IDLE_SECONDS
        T._write_json(T._record_path("ios-simulator", udid), rec)
        expired = self.run_active(udid, os.getpid(), "touch", marker)
        self.assertEqual(expired.wait(timeout=10), 2)
        self.assertFalse(os.path.exists(marker))

    def test_T49_run_active_returns_the_runs_exit_status(self):
        udid = self.device("rios-ui-status")
        T.register("ios-simulator", udid, os.getpid())
        self.assertEqual(self.run_active(udid, os.getpid(), "sh", "-c", "exit 65").wait(timeout=10), 65)
        self.assertEqual(self.run_active(udid, os.getpid(), "sh", "-c", "kill -TERM $$").wait(timeout=10), 143)

    def test_T50_a_declared_ui_suite_lease_lives_longer_and_nothing_lengthens_a_lease(self):
        ui = T.acquire_ios("iPhone", "runtime", os.getpid(), purpose="ui-suite")
        lease = self.lease(ui)
        self.assertEqual((lease["max_seconds"], lease["purpose"]), (T.LEASE_PURPOSES["ui-suite"], "ui-suite"))
        self.assertLessEqual(T.LEASE_PURPOSES["ui-suite"], 2 * T.LEASE_MAX_SECONDS)
        self.assertEqual(lease["idle_seconds"], T.LEASE_IDLE_SECONDS)
        active = dict(lease, last_use=lease["created"] + T.LEASE_MAX_SECONDS)     # renewed by its run
        self.assertFalse(T.lease_expired({"lease": active}, lease["created"] + T.LEASE_MAX_SECONDS + 1))
        self.assertTrue(T.lease_expired({"lease": dict(lease, last_use=lease["created"] + 1799)},
                                        lease["created"] + 1800))
        # Registering again resets to the default; it never keeps or grants more.
        again = T.register("ios-simulator", ui, os.getpid())
        self.assertEqual(again["lease"]["max_seconds"], T.LEASE_MAX_SECONDS)
        self.assertNotIn("purpose", again["lease"])
        T.release_ios(ui, os.getpid())
        plain = T.acquire_ios("iPad", "runtime", os.getpid())
        self.assertEqual(self.lease(plain)["max_seconds"], T.LEASE_MAX_SECONDS)
        T.acquire_ios("iPad", "runtime", os.getpid(), purpose="ui-suite")      # the same lease again
        self.assertEqual(self.lease(plain)["max_seconds"], T.LEASE_MAX_SECONDS)
        T.release_ios(plain, os.getpid())
        with self.assertRaisesRegex(ValueError, "unknown lease purpose"):
            T.acquire_ios("iPhone", "runtime", os.getpid(), purpose="forever")

    def test_T50a_an_actively_renewed_lease_outlives_its_lifetime_but_not_the_ceiling(self):
        # P5-23: the lifetime must not cut off a run whose renewer is still vouching for it.
        base = {"created": 1000.0, "max_seconds": 900, "idle_seconds": 300}
        at = 1000.0 + 901
        renewed = dict(base, last_use=at - 10, activity={"pid": 1, "renewer": 2, "renewed": at - 10})
        self.assertFalse(T.lease_expired({"lease": renewed}, at))                     # healthy, past 900 s
        # No renewer behind it (a CLI touch alone): the lifetime still ends it.
        touched = dict(base, last_use=at - 10)
        self.assertTrue(T.lease_expired({"lease": touched}, at))
        # A renewer that has gone quiet for the inactivity window: idle limit ends it.
        quiet = dict(base, last_use=at - 300, activity={"pid": 1, "renewer": 2, "renewed": at - 300})
        self.assertTrue(T.lease_expired({"lease": quiet}, at))
        # A run that renews forever is still ended, at the ceiling.
        late = 1000.0 + T.LEASE_ACTIVE_CEILING_SECONDS
        forever = dict(base, last_use=late - 10, activity={"pid": 1, "renewer": 2, "renewed": late - 10})
        self.assertTrue(T.lease_expired({"lease": forever}, late))
        self.assertFalse(T.lease_expired({"lease": forever}, late - 100))

    def test_T50c_the_default_pool_wait_outlasts_every_lease_that_is_still_valid(self):
        # P5-24 (v2 re-check): the default wait covered the declared lifetimes (1860 s), but a lease
        # its run keeps renewing stays valid to LEASE_ACTIVE_CEILING_SECONDS, so a waiter gave up on
        # a healthy holder at 1860 s that legitimately held the device at 1900 s and beyond.
        with patch.dict(os.environ, {T.POOL_WAIT_ENV: ""}):
            wait = T.pool_wait_seconds()
        now = 100000.0
        for max_seconds in (T.LEASE_MAX_SECONDS, *T.LEASE_PURPOSES.values()):
            for age in (1900, T.LEASE_ACTIVE_CEILING_SECONDS - 1):
                lease = {"created": now - age, "last_use": now - 10, "max_seconds": max_seconds,
                         "idle_seconds": T.LEASE_IDLE_SECONDS, "activity": {"pid": 1, "renewer": 2, "renewed": now - 10}}
                with self.subTest(max_seconds=max_seconds, age=age):
                    self.assertFalse(T.lease_expired({"lease": lease}, now), "the holder's lease is still valid")
                    self.assertGreater(wait, age, "a default waiter must not give up while the lease is valid")
        self.assertGreaterEqual(T.POOL_WAIT_SECONDS, T.LEASE_ACTIVE_CEILING_SECONDS + 60)

    def test_T50b_the_pool_wait_outlasts_the_longest_lease_unless_a_gate_names_a_positive_number(self):
        with patch.dict(os.environ, {T.POOL_WAIT_ENV: ""}):
            # P5-24: five minutes gave up on a healthy holder that may keep the device 15 or 30.
            self.assertGreater(T.pool_wait_seconds(), max(T.LEASE_MAX_SECONDS, *T.LEASE_PURPOSES.values()))
            self.assertGreater(T.pool_wait_seconds(), 300)
        with patch.dict(os.environ, {T.POOL_WAIT_ENV: "6900"}):
            self.assertEqual(T.pool_wait_seconds(), 6900)
            # ...and the CLI hands exactly that to acquire_ios, for rios's calls as much as a suite's.
            # _main raises the module's REGISTRY_LOCK_SECONDS to the CLI's; restore it for later tests.
            with patch.object(T, "acquire_ios", return_value="UDID") as acquire, \
                    patch.object(T, "REGISTRY_LOCK_SECONDS", T.REGISTRY_LOCK_SECONDS), \
                    patch("sys.stdout"):
                self.assertEqual(T._main(["acquire-ios", "--type", "t", "--runtime", "r"]), 0)
            self.assertEqual(acquire.call_args.kwargs["timeout"], 6900)
        for bad in ("soon", "0", "-5", "nan", "inf"):
            with self.subTest(value=bad), patch.dict(os.environ, {T.POOL_WAIT_ENV: bad}):
                with self.assertRaisesRegex(ValueError, T.POOL_WAIT_ENV):
                    T.pool_wait_seconds()
        # Positive control: a second holder really does make a short wait give up, so the
        # longer wait is what separates a queued suite from a failed one.
        other = T.acquire_ios("iPhone", "runtime", os.getpid())
        waiter = subprocess.Popen(["sleep", "30"])
        try:
            with self.assertRaisesRegex(TimeoutError, "leased by another run"):
                T.acquire_ios("iPad", "runtime", waiter.pid, timeout=1)
        finally:
            waiter.kill()
            waiter.wait()
            T.release_ios(other, os.getpid())

    def test_T52_the_lease_report_shows_age_inactivity_and_activity_without_changing_anything(self):
        udid = self.device("rios-ui-report")
        rec = T.register("ios-simulator", udid, os.getpid())
        rec["lease"]["last_use"] = rec["lease"]["created"]
        T._write_json(T._record_path("ios-simulator", udid), rec)
        before = open(T._record_path("ios-simulator", udid)).read()
        [row] = T.lease_report(now=rec["lease"]["created"] + 10)
        self.assertEqual((row["id"], row["age"], row["idle"], row["expired"]), (udid, 10, 10, False))
        self.assertEqual(row["max_seconds"], T.LEASE_MAX_SECONDS)
        self.assertEqual(open(T._record_path("ios-simulator", udid)).read(), before)
        [late] = T.lease_report(now=rec["lease"]["created"] + T.LEASE_IDLE_SECONDS)
        self.assertTrue(late["expired"])

    def test_T62_the_pool_hands_out_one_lease_unless_a_build_names_a_whole_number(self):
        # Unset is 1 -- every land and every engineer's suite keeps one simulator at a time.
        with patch.dict(os.environ, {T.POOL_LEASES_ENV: ""}):
            self.assertEqual(T.pool_leases(), 1)
        with patch.dict(os.environ, {T.POOL_LEASES_ENV: "2"}):
            self.assertEqual(T.pool_leases(), 2)
        for bad in ("two", "0", "-1", "1.5", "all"):
            with self.subTest(value=bad), patch.dict(os.environ, {T.POOL_LEASES_ENV: bad}):
                with self.assertRaisesRegex(ValueError, T.POOL_LEASES_ENV):
                    T.pool_leases()
        # And at the default a second run on ANOTHER device type still waits: one at a time.
        os.environ.pop(T.POOL_LEASES_ENV, None)
        first = T.acquire_ios("iPhone", "runtime", os.getpid())
        other = subprocess.Popen(["sleep", "30"])
        try:
            with self.assertRaisesRegex(TimeoutError, "leased by another run"):
                T.acquire_ios("iPad", "runtime", other.pid, timeout=0)
        finally:
            other.kill(); other.wait()
            T.release_ios(first, os.getpid())

    def test_T63_a_raised_lease_count_runs_other_types_side_by_side_and_never_shares_one(self):
        with patch.dict(os.environ, {T.POOL_LEASES_ENV: "2"}):
            others = [subprocess.Popen(["sleep", "30"]) for _ in range(3)]
            try:
                first = T.acquire_ios("iPhone", "runtime", os.getpid())
                # Another run, another type: admitted at once, on its own device.
                second = T.acquire_ios("iPad", "runtime", others[0].pid, timeout=0)
                self.assertNotEqual(first, second)
                self.assertEqual(len(T.records()), 2)
                # Both leases taken: a third type waits.
                with self.assertRaisesRegex(TimeoutError, "leased by another run"):
                    T.acquire_ios("iPhone-mini", "runtime", others[1].pid, timeout=0)
                T.release_ios(second, others[0].pid)
                # Room for one more, but the iPhone's one prepared simulator is the first run's:
                # a second run on the SAME type waits rather than sharing it.
                with self.assertRaisesRegex(TimeoutError, "leased by another run"):
                    T.acquire_ios("iPhone", "runtime", others[2].pid, timeout=0)
                self.assertEqual(sum(x.startswith("create ") for x in self.calls()), 2)
            finally:
                for p in others:
                    p.kill(); p.wait()
            T.release_ios(first, os.getpid())

    def hold_registry_lock(self):
        os.makedirs(T.registry_dir(), exist_ok=True)
        command = "import fcntl,sys,time; f=open(sys.argv[1],'a'); fcntl.flock(f,fcntl.LOCK_EX); print('ready',flush=True); time.sleep(600)"
        proc = subprocess.Popen([sys.executable, "-c", command, os.path.join(T.registry_dir(), ".lock")],
                                stdout=subprocess.PIPE, text=True)
        self.procs.append(proc)
        self.assertEqual(proc.stdout.readline().strip(), "ready")
        return proc

    def guard_events(self):
        import cpu_guard
        try:
            return [json.loads(l)["message"] for l in open(cpu_guard.STATE / "events.jsonl")]
        except OSError:
            return []

    def test_T53_a_busy_registry_is_a_skipped_collector_cycle_not_a_failure(self):
        import cpu_guard
        udid = self.device("rios-ui-busy")
        rec = T.register("ios-simulator", udid, os.getpid())
        rec["lease"]["last_use"] = time.time() - T.LEASE_IDLE_SECONDS - 1
        T._write_json(T._record_path("ios-simulator", udid), rec)
        before = len(self.guard_events())
        holder = self.hold_registry_lock()
        with patch.object(T, "REGISTRY_LOCK_SECONDS", 0.2), patch.object(cpu_guard, "event") as alert:
            self.assertEqual(T.expire_leases(), 0)                  # skipped, not failed
            self.assertEqual(T.expire_leases(), 0)
            alert.assert_not_called()
        self.assertNotIn("collector:incomplete", self.rows())
        notes = self.guard_events()[before:]
        self.assertEqual(sum("skipped: registry lock busy" in n for n in notes), 1)   # once per spell
        self.assertEqual(T._read_json(T._busy_path())["skipped"], 2)
        self.assertEqual([d["state"] for d in self.devices()], ["Booted"])
        holder.kill()
        holder.wait()
        with patch.object(cpu_guard, "event"):
            self.assertEqual(T.expire_leases(), 0)                  # the next free cycle collects
        self.assertEqual(self.devices(), [])
        self.assertFalse(os.path.exists(T._busy_path()))
        self.assertIn("Device lease collection resumed after a busy registry", self.guard_events()[before:])

    def test_T54_a_registry_busy_past_the_bound_is_a_collector_failure(self):
        self.hold_registry_lock()
        T._write_json(T._busy_path(), {"since": time.time() - T.COLLECTOR_BUSY_ALERT_SECONDS, "skipped": 90})
        with patch.object(T, "REGISTRY_LOCK_SECONDS", 0.2):
            self.assertEqual(T.expire_leases(), 1)
        self.assertIn("registry lock busy", self.rows()["collector:incomplete"]["why"])

    def test_T55_a_prepared_simulator_left_running_without_a_lease_is_shut_down_then_booted(self):
        udid = T.acquire_ios("iPhone", "runtime", os.getpid())
        self.write_state([dict(d, state="Booted") for d in self.devices()])     # an orphan's xcodebuild
        before = len(self.guard_events())
        T.boot_ios(udid)
        self.assertEqual(T.records()[0]["boot"]["phase"], "ready")
        log = self.calls()
        self.assertLess(log.index("shutdown " + udid), log.index("boot " + udid))
        self.assertIn("Prepared simulator was running without a lease; shut down before boot",
                      self.guard_events()[before:])
        T.release_ios(udid, os.getpid())

    def test_T56_a_cleanup_release_leaves_a_device_another_run_now_leases(self):
        other = self.proc()
        udid = T.acquire_ios("iPhone", "runtime", other.pid)
        T.boot_ios(udid)
        with self.assertRaisesRegex(ValueError, "another run"):
            T.release_ios(udid, os.getpid())                       # the default stays strict
        T.release_ios(udid, os.getpid(), if_ours=True)              # a cleanup path: nothing to do
        self.assertEqual(self.devices()[0]["state"], "Booted")
        self.assertTrue(T.same_owner(T.records()[0]["owner"], T.choose_owner(other.pid)))
        T.release_ios(udid, other.pid)

    def run_tree(self):
        """A run with a child of its own, as xcodebuild has: prints both pids, then waits."""
        return ["sh", "-c", "sleep 120 & echo $$ $!; wait"]

    def test_T57_a_run_whose_lease_is_handed_to_another_run_stops_at_once_and_is_not_run(self):
        # esc-20260925T014934Z-0a4bf206: the lease ended mid-run, another run leased the same
        # prepared simulator, and the first run went on executing the other checkout's bundle.
        import cpu_guard
        lost = os.path.join(self.root, "test-0.lost")
        udid = T.acquire_ios("iPhone", "runtime", os.getpid(), purpose="ui-suite")
        T.boot_ios(udid)
        p = subprocess.Popen([sys.executable, "-B", os.path.join(HERE, "testdevices.py"), "run-active",
                              "--kind", "ios-simulator", "--id", udid, "--owner-pid", str(os.getpid()),
                              "--interval", "0.3", "--check", "0.2", "--lost-file", lost, "--"] + self.run_tree(),
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.procs.append(p)
        leader, grandchild = (int(x) for x in p.stdout.readline().split())
        # The handover: the lease ends (as the collector ends one), and another run takes it.
        rec = T._read_json(T._record_path("ios-simulator", udid))
        rec["lease"]["last_use"] = time.time() - T.LEASE_IDLE_SECONDS - 1
        T._write_json(T._record_path("ios-simulator", udid), rec)
        with patch.object(cpu_guard, "event"):
            self.assertEqual(T.expire_leases(), 0)
        other = self.proc()
        taken = T.acquire_ios("iPhone", "runtime", other.pid, timeout=5)
        self.assertEqual(taken, udid)
        start = time.monotonic()
        self.assertEqual(p.wait(timeout=20), T.LEASE_LOST_EXIT)       # requirement 1: it stops
        self.assertLess(time.monotonic() - start, 15)
        self.assertIn("LEASE LOST", p.stderr.read())                   # loudly
        self.assertEqual(T.process_start(leader), "")                  # the run and everything
        self.assertEqual(T.process_start(grandchild), "")              # it started are gone
        self.assertIn("why", T._read_json(lost))                       # and it is recorded
        # The other run's lease is untouched by the first run's exit.
        self.assertTrue(T.same_owner(T.records()[0]["owner"], T.choose_owner(other.pid)))
        T.release_ios(udid, other.pid)

    def test_T58_a_signal_to_the_renewer_reaches_the_whole_run(self):
        udid = self.device("rios-ui-signal")
        T.register("ios-simulator", udid, os.getpid())
        p = subprocess.Popen([sys.executable, "-B", os.path.join(HERE, "testdevices.py"), "run-active",
                              "--kind", "ios-simulator", "--id", udid, "--owner-pid", str(os.getpid()),
                              "--"] + self.run_tree(), stdout=subprocess.PIPE, text=True)
        self.procs.append(p)
        leader, grandchild = (int(x) for x in p.stdout.readline().split())
        p.terminate()                                                  # the pid captured at spawn
        self.assertEqual(p.wait(timeout=15), 143)
        time.sleep(0.3)
        self.assertEqual((T.process_start(leader), T.process_start(grandchild)), ("", ""))

    def test_T59_a_cli_acquire_outwaits_a_registry_lock_busy_past_the_collectors_five_seconds(self):
        command = ("import fcntl,sys,time; f=open(sys.argv[1],'a'); fcntl.flock(f,fcntl.LOCK_EX); "
                   "print('ready',flush=True); time.sleep(7)")
        os.makedirs(T.registry_dir(), exist_ok=True)
        holder = subprocess.Popen([sys.executable, "-c", command, os.path.join(T.registry_dir(), ".lock")],
                                  stdout=subprocess.PIPE, text=True)
        self.procs.append(holder)
        self.assertEqual(holder.stdout.readline().strip(), "ready")
        start = time.monotonic()
        r = subprocess.run([sys.executable, "-B", os.path.join(HERE, "testdevices.py"), "acquire-ios",
                            "--type", "iPhone", "--runtime", "runtime", "--owner-pid", str(os.getpid())],
                           capture_output=True, text=True, timeout=120)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertGreater(time.monotonic() - start, T.REGISTRY_LOCK_SECONDS)   # it really waited
        self.assertEqual(T.records()[0]["id"], r.stdout.strip())
        T.release_ios(r.stdout.strip(), os.getpid())

    def test_T60_device_admission_waits_one_shared_bound_not_sixty_seconds_per_step(self):
        import types
        seen = []

        class Token:
            def __init__(self, what):
                self.what = what
            def release(self):
                seen.append(("release", self.what))

        class Budget:
            def __init__(self, *a, **k):
                pass
            def acquire(self, timeout):
                seen.append(("worker", timeout))
                return Token("worker")
        fake_tokens = types.SimpleNamespace(Budget=Budget, machine_directory=lambda: "/nonexistent")

        def sim_acquire(kind, timeout):
            seen.append((kind, timeout))
            if kind == "boot" and fail[0]:
                raise TimeoutError("simulator boot admission exceeded %gs" % timeout)
            return Token(kind)
        fake_budget = types.SimpleNamespace(acquire=sim_acquire)
        fail = [False]
        import cpu_guard
        mods = {"worker_tokens": fake_tokens, "simulator_budget": fake_budget}
        with patch.dict(sys.modules, mods), patch.object(cpu_guard, "healthy", return_value=True):
            # setUp replaced T._device_admission; load the module's own copy of it.
            source = importlib.util.spec_from_file_location("td_real", os.path.join(HERE, "testdevices.py"))
            td = importlib.util.module_from_spec(source)
            source.loader.exec_module(td)
            held = td._device_admission()
            self.assertEqual([w for w, _ in seen], ["worker", "live", "boot"])
            self.assertTrue(all(t > 60 for _, t in seen))                           # not 60 s each
            self.assertTrue(all(t <= td.DEVICE_ADMISSION_SECONDS for _, t in seen))  # one shared bound
            self.assertEqual(len(held), 3)
            seen.clear()
            fail[0] = True
            with self.assertRaises(TimeoutError):
                td._device_admission()
            self.assertIn(("release", "worker"), seen)                             # nothing leaks
            self.assertIn(("release", "live"), seen)

    def test_T61_a_lease_is_kept_alive_while_its_boot_waits_for_admission(self):
        udid = T.acquire_ios("iPhone", "runtime", os.getpid())
        rec = T._read_json(T._record_path("ios-simulator", udid))
        rec["lease"]["last_use"] = time.time() - (T.LEASE_IDLE_SECONDS - 1)     # 1 s of idle left
        T._write_json(T._record_path("ios-simulator", udid), rec)
        waited = []

        def slow_admission():
            time.sleep(1.6)                                   # longer than the idle left
            waited.append(T.lease_expired(T._read_json(T._record_path("ios-simulator", udid))))
            return []
        with patch.object(T, "_device_admission", side_effect=slow_admission):
            T._admitted_keeping_lease("ios-simulator", udid, every=0.3)
        self.assertEqual(waited, [False])                      # renewed while it waited
        created = rec["lease"]["created"]
        self.assertEqual(T._read_json(T._record_path("ios-simulator", udid))["lease"]["created"], created)
        stopped = T._read_json(T._record_path("ios-simulator", udid))["lease"]["last_use"]
        time.sleep(0.8)                                        # and not after it
        self.assertEqual(T._read_json(T._record_path("ios-simulator", udid))["lease"]["last_use"], stopped)
        T.release_ios(udid, os.getpid())

    def test_T51b_one_failed_renewal_attempt_does_not_end_renewal_for_good(self):
        # P5-25: a registry-lock timeout is a failed attempt, not a lost lease. The run
        # keeps renewing on the next interval; before, one error stopped renewal forever.
        udid = self.device("rios-ui-transient-renewal")
        T.register("ios-simulator", udid, os.getpid())
        real = T.renew_activity
        calls = []

        def flaky(*args):
            calls.append(time.monotonic())
            if len(calls) == 1:
                raise TimeoutError("registry lock busy")
            return real(*args)
        err = io.StringIO()
        with patch.object(T, "renew_activity", flaky), patch("sys.stderr", err):
            code = T.run_active("ios-simulator", udid, ["sleep", "1.5"], os.getpid(), interval=0.2, check=0.05)
        self.assertEqual(code, 0)
        self.assertGreaterEqual(len(calls), 3)                    # it went on after the failure
        self.assertIn("trying again", err.getvalue())
        self.assertNotIn("stopped renewing", err.getvalue())

    def test_T51_renewal_stops_when_the_owned_run_ends(self):
        udid = self.device("rios-ui-run-ended")
        rec = T.register("ios-simulator", udid, os.getpid())
        ended = self.proc("true")
        ended.wait()
        renewed, why = T.renew_activity("ios-simulator", udid, T.choose_owner(os.getpid()),
                                        rec["lease"]["created"], ended)
        self.assertFalse(renewed)
        self.assertIn("run ended", why)
        self.assertNotIn("activity", self.lease(udid))



class _Result(unittest.TextTestResult):
    def addSuccess(self, test):
        super().addSuccess(test)
        self.stream.write("  PASS  %s\n" % test._testMethodName)

    def addFailure(self, test, err):
        super().addFailure(test, err)
        self.stream.write("  FAIL  %s\n" % test._testMethodName)

    def addError(self, test, err):
        super().addError(test, err)
        self.stream.write("  FAIL  %s (error)\n" % test._testMethodName)


if __name__ == "__main__":
    runner = unittest.TextTestRunner(stream=sys.stdout, verbosity=0, resultclass=_Result)
    names = [a for a in sys.argv[1:] if a]
    loader = unittest.defaultTestLoader
    suite = (loader.loadTestsFromNames(names, sys.modules[__name__]) if names
             else loader.loadTestsFromModule(sys.modules[__name__]))
    result = runner.run(suite)
    print("=== testdevices tests: %d run, %d failed ===" % (
        result.testsRun, len(result.failures) + len(result.errors)))
    sys.exit(0 if result.wasSuccessful() else 1)
