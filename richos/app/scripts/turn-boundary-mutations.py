#!/usr/bin/env python3
"""turn-boundary-mutations.py -- every door of the turn-boundary adoption, watched going red.

WHY: CEO ruling §88, "The user should be able to answer on their phone just as well as they can
answer on the desktop app." Every entrance that runs his turns (typed, spoken, the phone's
drain, front-desk priming, the boot's reconciliation) must adopt the work those turns wrote
down (`adopt_at_the_turn_boundary` in src-tauri/src/main.rs, `Spine::take_ended_turns`). A test
only proves an entrance if it FAILS when that entrance's door is taken away, so each mutant
below takes away ONE door, or puts back the pre-fix shape of one entrance, and the named tests
must fail. A mutant caught only by some other test, or by a compile error, does not score. The
unmutated copy must pass first.

THE CHECKOUT IS NEVER WRITTEN. The tracked sources at HEAD are exported (`git archive`) into a
private copy at a FIXED path, the untracked build input `richos/app/ui-dist` is copied beside
them, and every mutant is applied to and restored in that copy. It builds into a PRIVATE target
beside the copy, never the shared Cargo cache: cargo judges a workspace member fresh by mtime
and keys it relative to the workspace root, so a mutant compiled into the shared cache is what
the next older checkout runs (esc-20260926T113721Z-70ef679e, measured on this harness's first
version). The copy and its target are deleted at the end unless --keep is given; with --keep and
the same --copy, a rerun is incremental. (These mutants reach the Tauri shell, whose build reads
richos/web, richos/mobile and richos/engine/voice/models too, so a copy is the one way not to
touch them.)

  turn-boundary-mutations.py --check          every mutant applies exactly once, every test it
                                              names exists; no cargo (turn-boundary-mutations.test.sh)
  turn-boundary-mutations.py [--copy DIR] [--keep]
                                              the full run: one cargo test per mutant. Run by hand,
                                              from richos/app, through the admission:
                                              scripts/testvm/reserve.py --wait 600 -- python3 scripts/turn-boundary-mutations.py

Exit: 0 every mutant proven (or, with --check, applies); 1 otherwise.
"""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]  # <repo>/richos/app/scripts/this
# --check only: read the sources under another root (the suite's negative control).
CHECK_ROOT = Path(os.environ.get("TURN_BOUNDARY_CHECK_ROOT", str(ROOT)))
APP = "richos/app"

SPINE = f"{APP}/crates/richos-core/src/spine.rs"
MAIN = f"{APP}/src-tauri/src/main.rs"
BRIDGE = f"{APP}/src-tauri/src/phone/bridge.rs"

PUSH = """        if !self.thread_is_running(thread_id) && !self.ended_turns.iter().any(|known|known.thread_id()==thread_id) {
            self.ended_turns.push(binding.clone());
        }
"""

# (name, file, old text, new text, workspace, test filter, tests that must FAIL)
MUTANTS = [
    ("M1 the spine forgets which turns ended", SPINE, PUSH, "",
     "core", "", ["a_typed_turn_is_reported_once_and_the_list_is_then_empty",
                  "a_spoken_turn_is_reported_exactly_as_a_typed_one",
                  "a_phone_turn_is_reported_on_its_own_thread_and_not_on_the_active_one"]),
    ("M2 taking the list does not empty it", SPINE,
     "std::mem::take(&mut self.ended_turns)", "self.ended_turns.clone()",
     "core", "", ["a_typed_turn_is_reported_once_and_the_list_is_then_empty"]),
    ("M3 the door wakes no peer input", MAIN,
     "ended.iter().map(|binding| work.adopt_registered(binding)).sum()", "{ let _ = (&ended, work); 0 }",
     "shell", "turn_boundary_tests::", ["turn_boundary_tests::work_he_gives_aloud_starts_without_a_typed_message",
                                        "turn_boundary_tests::work_he_gives_from_the_phone_starts_without_a_typed_message",
                                        "turn_boundary_tests::the_boundary_wakes_peer_input_without_dispatching_the_record"]),
    ("M4 spoken: the pre-fix shape, no adoption after a spoken turn", MAIN,
     "    // A failed turn still reaches here: what it wrote down before failing is a receipt.\n    adopt_at_the_turn_boundary(spine, work);\n",
     "    let _ = work;\n",
     "shell", "turn_boundary_tests::", ["turn_boundary_tests::work_he_gives_aloud_starts_without_a_typed_message"]),
    ("M5 phone: the pre-fix shape, no adoption after the drain", MAIN,
     "    adopt_at_the_turn_boundary(spine, work);\n    outcome.and(driven)",
     "    drop(spine);\n    let _ = work;\n    outcome.and(driven)",
     "shell", "turn_boundary_tests::", ["turn_boundary_tests::work_he_gives_from_the_phone_starts_without_a_typed_message"]),
    ("M6 spoken: the callback runs the turn itself and skips its door", MAIN,
     "            run_the_spoken_turn(&state.spine, &state.work, &text, rich_audible);",
     "            let mut spine = state.spine.lock().unwrap();\n            let _ = spine.submit_prompt_spoken(&text, Source::Jam, rich_audible);",
     "shell", "turn_boundary_tests::", ["turn_boundary_tests::every_entrance_that_runs_his_turns_adopts_at_its_boundary",
                                        "turn_boundary_tests::the_voice_callback_and_the_phone_drain_go_through_their_doors"]),
    ("M7 phone: the drain runs the turn itself and skips its door", BRIDGE,
     "crate::dispatch_incoming_messages(&self.app, &thread)?;",
     "state.spine.lock().unwrap().poll_intake();",
     "shell", "turn_boundary_tests::", ["turn_boundary_tests::every_entrance_that_runs_his_turns_adopts_at_its_boundary",
                                        "turn_boundary_tests::the_voice_callback_and_the_phone_drain_go_through_their_doors"]),
    ("M8 typed: send_message skips its door", MAIN,
     "    adopt_at_the_turn_boundary(spine, &state.work);\n    Ok(messages)\n",
     "    drop(spine);\n    Ok(messages)\n",
     "shell", "turn_boundary_tests::", ["turn_boundary_tests::every_entrance_that_runs_his_turns_adopts_at_its_boundary"]),
    ("M9 priming: ready_the_front_desk skips its door", MAIN,
     "        adopt_at_the_turn_boundary(spine, &state.work);\n        match verdict {",
     "        drop(spine);\n        match verdict {",
     "shell", "turn_boundary_tests::", ["turn_boundary_tests::every_entrance_that_runs_his_turns_adopts_at_its_boundary"]),
    ("M10 boot: reconcile_intake skips its door", MAIN,
     "            adopt_at_the_turn_boundary(&mut spine, &work);\n",
     "",
     "shell", "turn_boundary_tests::", ["turn_boundary_tests::every_entrance_that_runs_his_turns_adopts_at_its_boundary"]),
]


def cargo_env(target):
    env = dict(os.environ)
    env["PATH"] = f"{Path.home() / '.cargo/bin'}:{env.get('PATH', '')}"
    env["CARGO_TARGET_DIR"] = str(target)
    return env


def run(copy, target, workspace, filt):
    if workspace == "core":
        cwd = copy / APP
        cmd = ["cargo", "test", "-p", "richos-core", "--test", "ended_turn_tests"]
        env = cargo_env(target / "workspace")
    else:
        cwd = copy / APP / "src-tauri"
        cmd = ["cargo", "test", "--bin", "richos-tauri", "--", filt]
        env = cargo_env(target / "src-tauri")
    done = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    out = done.stdout + done.stderr
    failed = set(re.findall(r"^test (\S+) \.\.\. FAILED$", out, re.M))
    passed = set(re.findall(r"^test (\S+) \.\.\. ok$", out, re.M))
    compiled = "could not compile" not in out
    return done.returncode, failed, passed, compiled, out


# Everything the export carries. richos-core compiles the dialect table in with `include_str!`,
# so its directory is a root too; `unexported_includes` keeps this list honest.
EXPORT_ROOTS = [APP, "richos/web", "richos/mobile", "richos/engine/voice/models",
                "richos/engine/scripts/lib/dialect",
                "richos/engine/.env.example", "richos/engine/scripts/lib/machine_settings.fixtures.json",
                "richos/engine/scripts/lib/machine_settings.py",
                # richos-core's integration tests compile in the captured upstream failures.
                "docs/verification/upstream-failure-2026-09-05"]
INCLUDE_RE = re.compile(r'include_(?:str|bytes)!\s*\(\s*"([^"]+)"')


def unexported_includes(root):
    """Every `include_str!`/`include_bytes!` in the built crates whose file lies outside EXPORT_ROOTS.
    Returns (source file, literal path, resolved path) triples."""
    found = []
    for base in (root / APP / "crates", root / APP / "src-tauri"):
        for src in sorted(base.rglob("*.rs")):
            if "target" in src.relative_to(base).parts:
                continue
            for line in src.read_text(errors="replace").splitlines():
                if line.lstrip().startswith("//"):
                    continue
                for literal in INCLUDE_RE.findall(line):
                    resolved = Path(os.path.normpath(src.parent / literal)).relative_to(root).as_posix() \
                        if Path(os.path.normpath(src.parent / literal)).is_relative_to(root) else literal
                    if not any(resolved == r or resolved.startswith(r + "/") for r in EXPORT_ROOTS):
                        found.append((src.relative_to(root).as_posix(), literal, resolved))
    return found


def report_unexported(root):
    missing = unexported_includes(root)
    for src, literal, resolved in missing:
        print(f"FAIL export misses {resolved}: {src} compiles it in with include_str!/include_bytes! "
              f"(\"{literal}\"); add its directory to EXPORT_ROOTS")
    return len(missing)


def export(copy):
    if copy.exists():
        shutil.rmtree(copy)
    copy.mkdir(parents=True)
    archive = subprocess.run(["git", "-C", str(ROOT), "archive", "HEAD", *EXPORT_ROOTS],
                             capture_output=True, check=True).stdout
    subprocess.run(["tar", "-x", "-C", str(copy)], input=archive, check=True)
    dist = ROOT / APP / "ui-dist"
    if dist.is_dir():
        shutil.copytree(dist, copy / APP / "ui-dist")


TEST_FILES = [f"{APP}/crates/richos-core/tests/ended_turn_tests.rs", f"{APP}/src-tauri/src/turn_boundary_tests.rs"]


def check():
    """Every mutant still applies exactly once to the tree, and names tests that exist."""
    defined = set()
    for rel in TEST_FILES:
        defined.update(re.findall(r"^fn ([a-z0-9_]+)\(", (CHECK_ROOT / rel).read_text(), re.M))
    unexported = report_unexported(CHECK_ROOT)
    bad = 0
    for name, rel, old, _new, _workspace, _filt, must_fail in MUTANTS:
        count = (CHECK_ROOT / rel).read_text().count(old)
        unknown = [t for t in must_fail if t.split("::")[-1] not in defined]
        if count != 1:
            print(f"FAIL {name}: its site occurs {count} times in {rel}, not once")
            bad += 1
        elif unknown:
            print(f"FAIL {name}: names tests that do not exist: {unknown}")
            bad += 1
        else:
            print(f"ok   {name}")
    print(f"{len(MUTANTS) - bad}/{len(MUTANTS)} mutants apply")
    return 1 if bad or unexported else 0


def main():
    os.environ["PATH"] = str(Path(__file__).resolve().parent / "bin") + os.pathsep + os.environ.get("PATH", "")
    parser = argparse.ArgumentParser()
    parser.add_argument("--copy", default=str(Path(os.environ.get("TMPDIR", "/tmp")) / "richos-turn-boundary-mutants"))
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.check:
        return check()
    copy = Path(args.copy)
    # NEVER the shared cache. Cargo keys a workspace member relative to its workspace root and
    # judges it fresh by mtime, so a mutant built here into the shared target is what every
    # other checkout's `cargo test` runs next if its files are older (measured 2026-09-26,
    # esc-20260926T113721Z-70ef679e). The mutants get a target of their own, beside the copy.
    target = Path(str(copy) + "-target")
    head = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True,
                          check=True).stdout.strip()
    print(f"sources: HEAD {head}, exported to {copy}")
    if report_unexported(ROOT):
        return 1
    export(copy)
    bad = 0
    try:
        for workspace, filt in (("core", ""), ("shell", "turn_boundary_tests::")):
            code, failed, passed, _, out = run(copy, target, workspace, filt)
            print(f"UNMUTATED {workspace}: exit {code}, {len(passed)} passed, {len(failed)} failed")
            if code != 0 or failed or not passed:
                print(out[-3000:])
                return 1
        for name, rel, old, new, workspace, filt, must_fail in MUTANTS:
            path = copy / rel
            original = path.read_text()
            if original.count(old) != 1:
                print(f"{name}: SITE NOT FOUND EXACTLY ONCE ({original.count(old)}) -- the mutant no longer applies")
                bad += 1
                continue
            path.write_text(original.replace(old, new))
            try:
                code, failed, passed, compiled, out = run(copy, target, workspace, filt)
            finally:
                path.write_text(original)
            missing = [t for t in must_fail if t not in failed]
            if not compiled:
                print(f"{name}: DID NOT COMPILE -- does not score")
                print(out[-2000:])
                bad += 1
            elif missing:
                print(f"{name}: SURVIVED -- expected red: {missing}; red: {sorted(failed)}")
                bad += 1
            else:
                print(f"{name}: KILLED by {', '.join(must_fail)} (exit {code}, {len(failed)} red)")
    finally:
        if not args.keep:
            shutil.rmtree(copy, ignore_errors=True)
            shutil.rmtree(target, ignore_errors=True)
    print(f"{len(MUTANTS) - bad}/{len(MUTANTS)} mutants killed")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
