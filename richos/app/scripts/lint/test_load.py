"""load_rules.py: each rule refuses its shape and accepts the load-safe form of it.

Positive controls are the point: a rule that also flagged the hang guard, the condition
wait or the declared site would be waived within a week.
"""
import itertools
from pathlib import Path
import re
import signal
import tempfile
import unittest

import load_rules
from common import Refusal

APP = "richos/app/"


def rules(path, text):
    return [rule for rule, _line, _text in load_rules.scan(path, text, load_rules.language_of(path))]


class WallClockVerdict(unittest.TestCase):
    def test_refuses_an_upper_bound_on_measured_time_in_every_language(self):
        cases = {
            APP + "ui/tests/home.js": "const took = Date.now() - started;\nassert(took < 2000, 'slow');\n",
            APP + "ui/tests/nav.js": "assert(b.elapsedMs >= 1500 && b.elapsedMs < 2500, 'window');\n",
            APP + "scripts/x.test.py": "elapsed = time.monotonic() - t0\nself.assertLess(elapsed, 2)\n",
            APP + "scripts/x.test.sh": "elapsed=$(( SECONDS - start ))\nif [ \"$elapsed\" -gt 10 ]; then bad 'slow'; fi\n",
            APP + "crates/c/tests/t.rs": "assert!(start.elapsed() < Duration::from_secs(1));\n",
            APP + "ui/tests/throw.js": "if (Date.now() - t0 > 2000) throw new Error('slow');\n",
        }
        for path, text in cases.items():
            with self.subTest(path=path):
                self.assertIn("wall-clock-verdict", rules(path, text))

    def test_multiline_assertions_and_measured_window_differences_are_sites(self):
        text = "assert(\n Math.abs(five.window - three.window) < 100,\n 'window');\n"
        sites = load_rules.scan(APP + "ui/tests/splash.js", text, "javascript")
        self.assertEqual([rule for rule, _, _ in sites], ["wall-clock-verdict"])
        self.assertEqual(sites[0][1], 1)
        changed = text.replace("100", "120")
        self.assertNotEqual(load_rules.site_hash(sites[0][2]),
                            load_rules.site_hash(load_rules.scan(APP + "ui/tests/splash.js", changed, "javascript")[0][2]))
        for path, source in (("richos/engine/scripts/x.test.py", "self.assertLess(\n elapsed, 2)"),
                             ("richos/engine/tests/x.test.sh", 'sleep 1\nassert_ok ready'),
                             ("richos/engine/tests/x.test.js", "assert(\n took < 20);")):
            self.assertTrue(rules(path, source), path)

    def test_shell_direction_follows_which_side_fails(self):
        upper = ["[ \"$elapsed\" -lt 5 ] || bad 'slow'\n", "if [ \"$elapsed\" -gt 5 ]; then bad 'slow'; fi\n"]
        lower = ["[ \"$waited\" -ge 3 ] && ok 'it waited'\n", "if [ \"$elapsed\" -lt 3 ]; then bad 'too fast'; fi\n"]
        for text in upper:
            with self.subTest(text=text):
                self.assertEqual(rules(APP + "scripts/x.test.sh", text), ["wall-clock-verdict"])
        for text in lower:
            with self.subTest(text=text):
                self.assertEqual(rules(APP + "scripts/x.test.sh", text), [])

    def test_accepts_a_lower_bound_and_a_named_hang_guard(self):
        text = ("const took = Date.now() - started;\nassert(took >= 1200, 'decided by the page default');\n"
                "assert(took < HANG_GUARD_MS, 'hang guard');\n")
        self.assertEqual(rules(APP + "ui/tests/home.js", text), [])

    def test_a_declared_site_is_exempt_and_a_bare_marker_is_not(self):
        declared = "// load-bound: virtual clock, page.clock installed at :40\nassert(Date.now() - t0 < 50);\n"
        bare = "// load-bound:\nassert(Date.now() - t0 < 50);\n"
        self.assertEqual(rules(APP + "ui/tests/a.js", declared), [])
        self.assertEqual(rules(APP + "ui/tests/a.js", bare), ["wall-clock-verdict"])


class ShortDeadline(unittest.TestCase):
    def test_refuses_literal_deadlines_under_thirty_seconds(self):
        cases = {
            APP + "scripts/a.test.py": "subprocess.run(cmd, timeout=15)\n",
            APP + "scripts/b.test.sh": "proc.communicate(timeout=5)\n",
            APP + "ui/tests/c.js": "await page.waitForFunction(fn, null, { timeout: 5000 });\n",
            APP + "ui/tests/d.js": "await waitForFact(page, NO_LABELS, 1500);\n",
            APP + "crates/c/tests/e.rs": "rx.recv_timeout(Duration::from_millis(40)).unwrap();\n",
            APP + "crates/c/tests/f.rs": "let deadline = Instant::now() + Duration::from_secs(3);\n",
        }
        for path, text in cases.items():
            with self.subTest(path=path):
                self.assertEqual(rules(path, text), ["short-deadline"])

    def test_accepts_hang_guards_and_named_constants(self):
        cases = {
            APP + "scripts/a.test.py": "subprocess.run(cmd, timeout=300)\nsubprocess.run(cmd, timeout=None)\n",
            APP + "ui/tests/c.js": "await page.waitForFunction(fn, null, { timeout: 60000 });\n",
            APP + "crates/c/tests/e.rs": "rx.recv_timeout(HANG_GUARD).unwrap();\n",
            APP + "scripts/b.test.sh": "timeout=5\n",
        }
        for path, text in cases.items():
            with self.subTest(path=path):
                self.assertEqual(rules(path, text), [])


class SleepThenAssert(unittest.TestCase):
    def test_refuses_a_sleep_used_as_synchronization(self):
        cases = {
            APP + "ui/tests/a.js": "await page.waitForTimeout(700);\nconst v = await read();\nassertEqual(v, 'x');\n",
            APP + "scripts/b.test.py": "time.sleep(0.3)\nself.assertFalse(alive(pid))\n",
            APP + "scripts/c.test.sh": "sleep 1\n[ -f \"$out\" ] || bad 'not written'\n",
            APP + "crates/c/tests/d.rs": "thread::sleep(Duration::from_millis(300));\nassert!(!alive);\n",
        }
        for path, text in cases.items():
            with self.subTest(path=path):
                self.assertEqual(rules(path, text), ["sleep-then-assert"])

    def test_accepts_a_poll_interval_and_a_condition_wait_before_the_assertion(self):
        cases = {
            APP + "ui/tests/a.js": "await page.waitForTimeout(100);\nawait page.waitForFunction(fn);\nassert(ok);\n",
            APP + "scripts/b.test.py": "while time.monotonic() < deadline:\n    if done():\n        break\n    time.sleep(0.1)\nself.assertTrue(done())\n",
            APP + "scripts/c.test.sh": "for i in $(seq 1 50); do [ -f x ] && break; sleep 0.1; done\n[ -f x ] || bad 'never'\n",
            APP + "crates/c/tests/d.rs": "loop {\n    if ready() { break; }\n    thread::sleep(Duration::from_millis(10));\n}\nassert!(ready());\n",
        }
        for path, text in cases.items():
            with self.subTest(path=path):
                self.assertEqual(rules(path, text), [])

    def test_a_sleep_inside_a_fixture_program_string_is_data(self):
        text = 'self.item("running", "import time; time.sleep(60)", weight=3)\nself.assertEqual(state, "running")\n'
        self.assertEqual(rules(APP + "scripts/b.test.py", text), [])

    def test_a_heredoc_body_is_data_not_a_sleep(self):
        text = "cat > \"$bin/agent\" <<'EOF'\nsleep 5\nEOF\nexpect 'the agent was stopped'\n"
        self.assertEqual(rules(APP + "scripts/c.test.sh", text), [])


class HostSample(unittest.TestCase):
    def test_refuses_executing_host_load_tools_in_test_code(self):
        cases = {
            APP + "ui/tests/lib/evidence.js": "const r = spawnSync('/usr/bin/top', ['-l', '1']);\n",
            APP + "ui/tests/b.js": "const load = os.loadavg();\n",
            APP + "scripts/c.test.py": "out = subprocess.check_output(['vm_stat'])\n",
            APP + "scripts/d.test.sh": "pressure=$(memory_pressure -Q)\n",
        }
        for path, text in cases.items():
            with self.subTest(path=path):
                self.assertEqual(rules(path, text), ["host-sample"])

    def test_accepts_a_mocked_tool(self):
        self.assertEqual(rules(APP + "scripts/d.test.sh", "fake_vm_stat=$(vm_stat_fixture)\n"), [])
        self.assertEqual(rules(APP + "ui/tests/b.js", "const stub = { loadavg: () => [1, 1, 1] }; // fake os\n"), [])


class EnvMutation(unittest.TestCase):
    def test_refuses_set_var_without_a_static_guard(self):
        text = "#[test]\nfn grace() {\n    std::env::set_var(\"RICHOS_CANCEL_GRACE_MS\", \"300\");\n}\n"
        self.assertEqual(rules(APP + "crates/c/tests/t.rs", text), ["env-mutation-unguarded"])

    def test_accepts_it_under_a_static_mutex(self):
        text = ("static ENV_GUARD: Mutex<()> = Mutex::new(());\n#[test]\nfn grace() {\n"
                "    let _g = ENV_GUARD.lock().unwrap();\n    std::env::set_var(\"X\", \"1\");\n}\n")
        self.assertEqual(rules(APP + "crates/c/tests/t.rs", text), [])


class QuotedStrings(unittest.TestCase):
    """The string stripper in assertion_text: linear on an unclosed quote, same text as before."""

    # The pattern before 2026-10-11, kept as the reference the new one must agree with.
    OLD = re.compile(r"(?:\"(?:\\.|[^\"])*\"|'(?:\\.|[^'])*')")

    def test_an_unclosed_quote_with_43_escaped_pairs_is_stripped_in_under_a_tenth_of_a_second(self):
        # The 2026-10-10 hang: a Rust string continued on the next line, 43 escape pairs on
        # this one. The old pattern roughly tripled its work per pair; this one is linear.
        lines = ['    assert!(ok, "' + "\\\\" * 43, '        tail", x);']

        def hung(_signum, _frame):
            raise TimeoutError("stripping one line took over 0.1 s of CPU")

        # load-bound: ITIMER_VIRTUAL counts this process's own CPU time, never time spent queued
        # behind other work, so host load cannot trip it; it is a hang guard, not a speed verdict.
        previous = signal.signal(signal.SIGVTALRM, hung)
        signal.setitimer(signal.ITIMER_VIRTUAL, 0.1)
        try:
            joined = load_rules.assertion_text(lines, 0, "rust")
        finally:
            signal.setitimer(signal.ITIMER_VIRTUAL, 0)
            signal.signal(signal.SIGVTALRM, previous)
        self.assertEqual(joined, " ".join(lines))

    def test_every_short_line_strips_exactly_as_the_old_pattern_did(self):
        # Every string up to six characters over the characters that matter: both quotes,
        # the backslash, a newline and one stand-in for anything else.
        for size in range(7):
            for chars in itertools.product("\"'\\a\n", repeat=size):
                line = "".join(chars)
                self.assertEqual(load_rules.QUOTED_STRING.sub("''", line), self.OLD.sub("''", line), repr(line))

    def test_strings_and_escapes_still_hide_their_delimiters(self):
        cases = {'assert!(a == "(", "x");': 'assert!(a == \'\', \'\');',
                 "assert(b, 'it\\'s (');": "assert(b, '');",
                 'f("a\\"b(") + g(\'c\')': "f('') + g('')"}
        for line, stripped in cases.items():
            with self.subTest(line=line):
                self.assertEqual(load_rules.QUOTED_STRING.sub("''", line), stripped)


class Scope(unittest.TestCase):
    def test_rust_production_code_is_not_test_code_but_its_cfg_test_module_is(self):
        text = ("pub fn run() {\n    std::thread::sleep(Duration::from_millis(5));\n    assert!(ok());\n}\n"
                "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n"
                "        std::thread::sleep(Duration::from_millis(5));\n        assert!(ok());\n    }\n}\n")
        found = load_rules.scan(APP + "crates/c/src/lib.rs", text, "rust")
        self.assertEqual([(r, line) for r, line, _ in found], [("sleep-then-assert", 9)])

    def test_fixtures_and_production_sources_are_not_scanned(self):
        self.assertIsNone(load_rules.language_of(APP + "ui/tests/fixtures/stalled-host-preload.js"))
        self.assertIsNone(load_rules.language_of(APP + "ui/main.js"))
        self.assertIsNone(load_rules.language_of(APP + "scripts/lint.sh"))
        self.assertEqual(load_rules.language_of(APP + "ui/tests/lib/harness.js"), "javascript")


class SiteBaseline(unittest.TestCase):
    SLOW = "await page.waitForTimeout(700);\nassert(seen);\n"
    OTHER = "await page.waitForTimeout(900);\nassert(other);\n"

    def collect(self, files):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for path, text in files.items():
                (root / path).parent.mkdir(parents=True, exist_ok=True)
                (root / path).write_text(text)
            return load_rules.collect(root, list(files))

    def test_a_new_site_is_refused_even_while_an_old_one_is_removed(self):
        path = APP + "ui/tests/a.js"
        baseline = load_rules.record(self.collect({path: self.SLOW}))
        load_rules.check(self.collect({path: self.SLOW}), baseline)  # acceptance: unchanged passes
        with self.assertRaisesRegex(Refusal, "1 new load-sensitive site"):
            # Same count (one), different site: a count ceiling would have let this through.
            load_rules.check(self.collect({path: self.OTHER}), baseline)

    def test_a_duplicated_line_is_a_new_site(self):
        path = APP + "ui/tests/a.js"
        baseline = load_rules.record(self.collect({path: self.SLOW}))
        with self.assertRaises(Refusal):
            load_rules.check(self.collect({path: self.SLOW + self.SLOW}), baseline)

    def test_a_branch_cannot_add_a_site_to_the_baseline_and_can_pay_one_down(self):
        path = APP + "ui/tests/a.js"
        trusted = load_rules.record(self.collect({path: self.SLOW}))
        grown = load_rules.record(self.collect({path: self.SLOW + self.OTHER}))
        with self.assertRaisesRegex(Refusal, "adds a site not on integration"):
            load_rules.compare(grown, trusted)
        paid = load_rules.lower(self.collect({path: "assert(seen);\n"}), trusted)
        self.assertEqual(paid["sites"], {})
        load_rules.compare(paid, trusted)

    def test_a_dropped_rule_is_refused(self):
        trusted = load_rules.record([])
        weakened = dict(trusted, rules={k: v for k, v in trusted["rules"].items() if k != "host-sample"})
        with self.assertRaisesRegex(Refusal, "removed or weakened load rule"):
            load_rules.compare(weakened, trusted)


class EngineRatchet(unittest.TestCase):
    def test_committed_engine_debt_is_allowed_but_new_and_duplicated_sites_refuse(self):
        import subprocess
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            def git(*args):
                return subprocess.run(["git", "-c", "core.hooksPath=/dev/null", *args], cwd=root,
                                      capture_output=True, text=True, check=True)
            git("init", "-q", "-b", "main")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            path = "richos/engine/scripts/slow.test.py"
            target = root / path
            target.parent.mkdir(parents=True)
            original = "self.assertLess(elapsed, 2)\n"
            target.write_text(original)
            git("add", ".")
            git("commit", "-qm", "existing engine timing debt")
            load_rules.check_engine(root, [path])
            target.write_text(original * 2)
            with self.assertRaisesRegex(Refusal, "load-rule growth"):
                load_rules.check_engine(root, [path])
            target.write_text("self.assertLess(\n duration, 3)\n")
            with self.assertRaisesRegex(Refusal, "load-rule growth"):
                load_rules.check_engine(root, [path])
            target.write_text("self.assertTrue(ready)\n")
            load_rules.check_engine(root, [path])
            new = "richos/engine/scripts/new.test.py"
            (root / new).write_text(original)
            with self.assertRaisesRegex(Refusal, "load-rule growth"):
                load_rules.check_engine(root, [new])


if __name__ == "__main__":
    unittest.main()
