#!/usr/bin/env python3
"""Rust and shell lint entry point. Run lint.sh --help for modes."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import json
import hashlib
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

import advisory_rules
from common import APP, Refusal, checked, classify, inventory, select, shellcheck, tracked
import dialect
import load_rules
import process_rules
import ratchet
import rust
import suite_rules
import timeout_rules

ROOT = Path(__file__).resolve().parents[4]
BASE = APP + "scripts/lint/baselines/"
# Seconds of Tauri Clippy's OWN work; waiting for Cargo's lock is not counted (rust.LOCK_WAIT).
TAURI_CAP = 180
RULES = {**process_rules.RULES,**timeout_rules.RULES, **advisory_rules.RULES, **dialect.RULES, **suite_rules.RULES,
         **load_rules.RULES}


def versions(root, cargo=False):
    result = {"python": f"{sys.version_info.major}.{sys.version_info.minor}"}
    shell_version = checked(["shellcheck", "--version"], root)
    if "version: 0.11.0" not in shell_version:
        raise Refusal("ShellCheck 0.11.0 is required")
    result["shellcheck"] = "0.11.0"
    if cargo:
        result["clippy"] = checked(["cargo", "clippy", "--version"], root).strip()
        result["rustc"] = checked(["rustc", "-Vv"], root).strip()
    return result


def record(commands, tool_versions, rules, rows, counts):
    return dict(schema=1, limitation=ratchet.NOTE, commands=commands, versions=tool_versions,
                rules=rules, inventory=rows, counts=counts or {"no-diagnostics": 0})


def summarize(name, counts, diagnostics):
    print(name + ": " + (", ".join(f"{key}={value}" for key, value in sorted(counts.items()) if value) or "no blocking diagnostics"), flush=True)
    advisory = Counter(d["rule"] for d in diagnostics if d.get("classification") == "advisory")
    if advisory:
        print("Advisory candidates (review required): " + ", ".join(f"{key}={value}" for key, value in sorted(advisory.items())), flush=True)


def custom(root, rows, read=None):
    """`read` supplies each file's text; by default the working tree. The dialect hook always
    runs from `root`'s engine with the file's real path, whatever text it is handed."""
    read = read or (lambda path: (root / path).read_text())
    findings = []
    counts = {rule: 0 for rule, kind in RULES.items() if kind == "blocking"}
    paths = select(rows, "shell") + select(rows, "rust")
    texts = {path: read(path) for path in paths}
    for path in paths:
        text = texts[path]
        row = rows[path]
        found = advisory_rules.scan(text, row["role"], row["language"])
        if row["language"] == "shell":
            found += process_rules.scan(text) + timeout_rules.scan(text)
            found += suite_rules.scan(path, text)
        for rule, line in found:
            findings.append(dict(path=path, line=line, rule=rule, classification=RULES[rule]))
            if RULES[rule] == "blocking":
                counts[rule] += 1
    def scan_path(path):
        return path, dialect.scan(root, path, texts[path])
    # Each hook receives its real file path, preserving ownership and exemptions.
    with ThreadPoolExecutor(max_workers=4) as pool:
        for path, labels in pool.map(scan_path, paths):
            counts["dialect"] += len(labels)
            findings.extend(dict(path=path, rule="dialect", classification="blocking", message=label) for label in labels)
    return counts, findings


def enforce(root, name, actual, args):
    path = BASE + name + ".json"
    baseline_path = root / path
    trusted = ratchet.trusted_record(root, args.trusted_ref, path)
    if args.bootstrap and not baseline_path.exists():
        if trusted is not None:
            raise Refusal(f"bootstrap refuses a baseline already on integration: {name}")
        ratchet.validate(actual)
        baseline_path.parent.mkdir(parents=True, exist_ok=True)
        baseline_path.write_text(json.dumps(actual, indent=2, sort_keys=True) + "\n")
        return
    try:
        baseline = json.loads(baseline_path.read_text())
    except (OSError, ValueError) as exc:
        raise Refusal(f"missing or malformed baseline: {name}") from exc
    # A scanned path deleted from the tree leaves the inventory; one still present never does.
    gone = ratchet.gone_from(root)
    if trusted is not None:
        ratchet.compare(baseline, trusted, gone)
    ratchet.check(actual, baseline, gone)
    if args.lower:
        baseline_path.write_text(json.dumps(ratchet.lower(actual, baseline, gone), indent=2, sort_keys=True) + "\n")


def load_phase(root, args, report, paths=None):
    """Load-sensitive test code (load_rules.py): per-SITE baseline, not a count ceiling.
    `paths` limits the scan to those files; sites are per file, so that is exact for them."""
    tick = time.monotonic()
    scanned = tracked(root) if paths is None else paths
    engine_sites = load_rules.check_engine(root, scanned, args.trusted_ref)
    sites = load_rules.collect(root, [p for p in scanned if p.startswith(APP)])
    report["load"] = dict(sites=sites, engine_sites=engine_sites, files=len(load_rules.targets(scanned)))
    baseline_path = root / load_rules.BASELINE
    trusted = ratchet.trusted_record(root, args.trusted_ref, load_rules.BASELINE)
    if args.bootstrap and not baseline_path.exists():
        if trusted is not None:
            raise Refusal("bootstrap refuses a baseline already on integration: load")
        baseline_path.write_text(load_rules.dump(load_rules.record(sites)))
    else:
        try:
            baseline = json.loads(baseline_path.read_text())
        except (OSError, ValueError) as exc:
            raise Refusal("missing or malformed baseline: load") from exc
        if trusted is not None:
            load_rules.compare(baseline, trusted)
        load_rules.check(sites, baseline)
        if args.lower:
            baseline_path.write_text(load_rules.dump(load_rules.lower(sites, baseline)))
    report["load"]["seconds"] = time.monotonic() - tick
    print(f"Load rules: {report['load']['files']} file(s) scanned for test code, no new load-sensitive site "
          f"({len(sites)} baselined) in {report['load']['seconds']:.2f}s", flush=True)


def shell_record(rows, tool_versions, counts):
    shell_rows = {p: r for p, r in rows.items() if r["language"] == "shell"}
    return record({"shellcheck": ["shellcheck", "--format=json", "--rcfile=" + APP + ".shellcheckrc", "<shell-inventory>"],
                   "configuration_sha256": hashlib.sha256((ROOT / APP / ".shellcheckrc").read_bytes()).hexdigest()},
                  {"shellcheck": tool_versions["shellcheck"]}, {"shellcheck-diagnostics": "blocking"}, shell_rows, counts)


def custom_record(rows, tool_versions, counts):
    return record({"custom": ["python3", APP + "scripts/lint/driver.py", "<rust-and-shell-inventory>"]},
                  {"python": tool_versions["python"]}, RULES,
                  {p: r for p, r in rows.items() if r["language"] != "javascript"}, counts)


def static_full(args, rows, tool_versions, report):
    tick = time.monotonic()
    print("ShellCheck and custom rules running", flush=True)
    counts, diagnostics = shellcheck(ROOT, rows)
    report["shell"] = dict(counts=counts, diagnostics=diagnostics)
    summarize("ShellCheck", counts, diagnostics)
    enforce(ROOT, "shell", shell_record(rows, tool_versions, counts), args)
    counts, diagnostics = custom(ROOT, rows)
    report["custom"] = dict(counts=counts, diagnostics=diagnostics, seconds=time.monotonic() - tick)
    summarize("Project rules", counts, diagnostics)
    enforce(ROOT, "custom", custom_record(rows, tool_versions, counts), args)
    load_phase(ROOT, args, report)
    print(f"Static checks complete in {time.monotonic() - tick:.2f}s", flush=True)


def rust_fast(args, rows, tool_versions, report):
    tick = time.monotonic()
    print("Rust fast set running", flush=True)
    counts, diagnostics = rust.collect(ROOT, rust.FAST)
    report["rust-fast"] = dict(counts=counts, diagnostics=diagnostics, seconds=time.monotonic() - tick)
    summarize("Rust fast set", counts, diagnostics)
    remember_counts(ROOT, "fast", tool_versions, counts)
    enforce(ROOT, "rust-fast", record({"clippy": rust.FAST}, tool_versions,
            rust.lint_rules(ROOT), {p: r for p, r in rows.items() if r["language"] == "rust"}, counts), args)
    return counts


# ---------------------------------------------------------------------------------------
# Clippy counts by content, so a commit can be compared with its parent without building it
# ---------------------------------------------------------------------------------------
# Every Clippy run records its counts under a digest of what that set reads: the blob of every
# Rust input (.rs, manifests, lockfile, build scripts) plus the commands and tool versions.
# The digest is a content address, so the record is shared by every worktree of the
# repository (in <git-common-dir>/richos-lint-cache/) and a land's run serves every branch
# cut from that main. Inputs that are not Rust (a file read by include_str!) are outside it.
RUST_CACHE = "richos-lint-cache"


def in_set(path, which):
    return rust_input(path) and (which == "tauri" or not path.startswith(APP + "src-tauri/"))


def rust_digest(root, which, tool_versions, source):
    """`source` "HEAD" digests HEAD's blobs; "working" the working tree, untracked inputs included."""
    blobs = {}
    for line in checked(["git", "ls-tree", "-r", "HEAD", "--", APP], root).splitlines():
        meta, path = line.split("\t", 1)
        if in_set(path, which):
            blobs[path] = meta.split()[2]
    if source == "working":
        untracked = checked(["git", "ls-files", "--others", "--exclude-standard", "--", APP], root).splitlines()
        present = []
        for path in changed_paths(root) + untracked:
            if not in_set(path, which):
                continue
            if (root / path).is_file():
                present.append(path)
            else:
                blobs.pop(path, None)
        if present:
            hashes = checked(["git", "hash-object", "--", *present], root).split()
            blobs.update(zip(present, hashes))
    digest = hashlib.sha256(json.dumps({"set": which, "commands": rust.TAURI if which == "tauri" else rust.FAST,
                                        "clippy": tool_versions.get("clippy"), "rustc": tool_versions.get("rustc")},
                                       sort_keys=True).encode())
    for path in sorted(blobs):
        digest.update(f"{path} {blobs[path]}\n".encode())
    return digest.hexdigest()


def cache_file(root, which, digest):
    common = Path(checked(["git", "rev-parse", "--git-common-dir"], root).strip())
    common = common if common.is_absolute() else root / common
    return common / RUST_CACHE / which / (digest + ".json")


def remember_counts(root, which, tool_versions, counts):
    try:
        path = cache_file(root, which, rust_digest(root, which, tool_versions, "working"))
        path.parent.mkdir(parents=True, exist_ok=True)
        scratch = path.with_suffix(f".{os.getpid()}.tmp")
        scratch.write_text(json.dumps(counts, sort_keys=True) + "\n")
        scratch.replace(path)
    except (Refusal, OSError, ValueError) as exc:
        print(f"Clippy counts not recorded for later comparison: {exc}", flush=True)


def recalled_counts(root, which, tool_versions):
    try:
        path = cache_file(root, which, rust_digest(root, which, tool_versions, "HEAD"))
        return json.loads(path.read_text()) if path.is_file() else None
    except (Refusal, OSError, ValueError):
        return None


def refuse_growth(what, growth):
    grown = ", ".join(f"{rule} +{n}" for rule, n in sorted(growth.items()))
    raise Refusal(
        f"this change adds {what} diagnostics: {grown}. A commit may not grow a lint count, whatever room "
        "the ceiling still has (--strict). Fix it, or make the exception explicit where a reviewer sees it "
        "(# shellcheck disable=SCnnnn, #[allow(clippy::...)] with the reason beside it).")


# ---------------------------------------------------------------------------------------
# --changed: the commit check. Same ceilings, same refusals, a fraction of the work.
# ---------------------------------------------------------------------------------------
# A full static pass is ~35 s here (ShellCheck ~9 s over 95 scripts, the dialect hook
# ~0.2 s per file over 378), which is the cost that gets a hook skipped. Every static rule
# is a per-file count, so the change's effect on each count is the change's files counted
# before and after; files that `source` a changed script are counted with it, because a
# ShellCheck result can depend on what a script sources. If no count grows, no ceiling can
# be newly exceeded by this change and the check passes. If one grows, the exact full pass
# decides, so the verdict is never looser than the full lint's: a change that adds a
# diagnostic still passes when the tree has room under the ceiling, and is refused when not.
# Everything that is not a count (tool versions, rule set, shrunk inventory, a baseline
# weakened against main) is checked in full every time; it is cheap.
LINT_MACHINERY = (APP + "scripts/lint/", APP + ".shellcheckrc", APP + "scripts/lint.sh")
RUST_INPUT = ("Cargo.toml", "Cargo.lock", "clippy.toml", ".clippy.toml", "rust-toolchain", "rust-toolchain.toml", "build.rs")


def changed_paths(root):
    out = checked(["git", "diff", "--name-only", "--no-renames", "HEAD", "--"], root)
    return sorted({p for p in out.splitlines() if p})


def head_text(root, path):
    result = subprocess.run(["git", "show", f"HEAD:{path}"], cwd=root, capture_output=True)
    return result.stdout.decode(errors="surrogateescape") if result.returncode == 0 else None


def rust_input(path):
    return path.startswith(APP) and (path.endswith(".rs") or Path(path).name in RUST_INPUT)


def changed(args, rows, tool_versions, report):
    started = time.monotonic()
    paths = changed_paths(ROOT)
    report["changed"] = paths
    app_paths = [p for p in paths if p.startswith(APP)]
    if not app_paths:
        load_phase(ROOT, args, report, paths=paths)
        print("No change under " + APP + "; nothing this lint scans changed", flush=True)
        return
    if any(p.startswith(LINT_MACHINERY) for p in app_paths):
        # The lint or its baselines changed: only the full check can say what they mean now.
        print("The lint itself changed: running the full static check and the Rust fast set", flush=True)
        static_full(args, rows, tool_versions, report)
        rust_fast(args, rows, tool_versions, report)
        if any(p.startswith(BASE + "tauri") or (rust_input(p) and p.startswith(APP + "src-tauri/")) for p in app_paths):
            tauri(ROOT, args, rows, tool_versions, report)
        return
    # Non-count parts of the contract, in full: versions, rules, commands, inventory, and the
    # committed baselines against integration.
    gone = ratchet.gone_from(ROOT)
    for name, rec in (("shell", shell_record(rows, tool_versions, {})), ("custom", custom_record(rows, tool_versions, {}))):
        baseline = json.loads((ROOT / BASE / (name + ".json")).read_text())
        trusted = ratchet.trusted_record(ROOT, args.trusted_ref, BASE + name + ".json")
        if trusted is not None:
            ratchet.compare(baseline, trusted, gone)
        ratchet.check(rec, baseline, gone)
    # Counts, before and after, over the changed files plus the scripts that source them.
    shell_paths = select(rows, "shell")
    touched = {p for p in app_paths if classify(p) and classify(p)["role"] != "fixture"
               and classify(p)["language"] in ("shell", "rust")}
    names = {Path(p).name for p in touched if p.endswith(".sh")}
    if names:
        for p in shell_paths:
            text = (ROOT / p).read_text(errors="surrogateescape")
            if any(n in text for n in names):
                touched.add(p)
    now_rows = {p: rows[p] for p in touched if p in rows}
    before = {p: head_text(ROOT, p) for p in touched}
    then_rows = {p: classify(p) for p, text in before.items() if text is not None}
    growth = {}
    if now_rows or then_rows:
        after_counts = count_static(ROOT, now_rows, None)
        before_counts = count_static(ROOT, then_rows, before)
        for rule in set(after_counts) | set(before_counts):
            delta = after_counts.get(rule, 0) - before_counts.get(rule, 0)
            if delta > 0:
                growth[rule] = delta
    report["changed-growth"] = growth
    if growth and args.strict:
        refuse_growth("static", growth)
    if growth:
        print("This change adds " + ", ".join(f"{k}+{v}" for k, v in sorted(growth.items()))
              + "; the full static check decides whether the tree still fits its ceilings", flush=True)
        static_full(args, rows, tool_versions, report)
    else:
        load_phase(ROOT, args, report, paths=paths)
        print(f"Static checks: {len(touched)} changed or dependent file(s), no count grew "
              f"({time.monotonic() - started:.2f}s)", flush=True)
    for which, wanted in (("fast", lambda p: in_set(p, "fast")), ("tauri", lambda p: p.startswith(APP + "src-tauri/"))):
        if not any(rust_input(p) and wanted(p) for p in app_paths):
            continue
        before = recalled_counts(ROOT, which, tool_versions) if args.strict else None
        if which == "fast":
            after = rust_fast(args, rows, tool_versions, report)
        else:
            tauri(ROOT, args, rows, tool_versions, report)
            after = report["tauri"]["counts"]
        if not args.strict:
            continue
        if before is None:
            print(f"No Clippy counts are recorded for HEAD's Rust ({which} set), so this commit is held "
                  "to the ceiling only; this run's counts are recorded for the next one", flush=True)
            continue
        grown = {rule: n - before.get(rule, 0) for rule, n in after.items() if n > before.get(rule, 0)}
        if grown:
            refuse_growth(f"Clippy ({which} set)", grown)
        print(f"Clippy ({which} set): no count grew against HEAD", flush=True)


def count_static(root, rows, texts):
    """ShellCheck plus project-rule counts for exactly these rows. `texts` None means the
    working tree; otherwise HEAD's text per path, checked in a scratch copy of the scripts
    tree at HEAD so that what a script sources is HEAD's too."""
    counts = {}
    shell_rows = {p: r for p, r in rows.items() if r["language"] == "shell" and r["role"] != "fixture"}
    if shell_rows:
        if texts is None:
            found, _ = shellcheck(root, shell_rows)
        else:
            with tempfile.TemporaryDirectory(prefix="lint-head-") as scratch:
                archive = subprocess.run(["git", "archive", "--format=tar", "HEAD", "--", APP + "scripts", APP + ".shellcheckrc"],
                                         cwd=root, capture_output=True, check=True).stdout
                subprocess.run(["tar", "-x", "-C", scratch], input=archive, check=True)
                found, _ = shellcheck(Path(scratch), shell_rows)
        for rule, n in found.items():
            counts[rule] = counts.get(rule, 0) + n
    if rows:
        found, _ = custom(root, rows, None if texts is None else (lambda p: texts[p]))
        for rule, n in found.items():
            counts[rule] = counts.get(rule, 0) + n
    return counts


def js_report(root, rows):
    findings = []
    for path in select(rows, "javascript"):
        row = rows[path]
        findings.extend(dict(path=path, line=line, rule=rule, classification="advisory")
                        for rule, line in advisory_rules.scan((root / path).read_text(), row["role"], "javascript"))
        findings.extend(dict(path=path, rule="dialect", classification="structural-match", message=label)
                        for label in dialect.scan(root, path, (root / path).read_text()))
    return {"status": "report-only; advisory candidates require review; dialect matches come from the existing hook",
            "files": len(select(rows, "javascript")), "findings": findings,
            "unsupported": ["process-pattern-kill (shell syntax only)", "process-self-wait (shell syntax only)", "timeout-success (shell syntax only)"],
            "counts": {rule: sum(f["rule"] == rule for f in findings) for rule in (*advisory_rules.RULES, "dialect")}}


def fast_was_run(path, run_id):
    """A script-suite skip cannot bypass the every-build fast lint requirement."""
    if path is None or not run_id:
        return False
    try:
        data = json.loads(path.read_text())
        if data.get("run_id") != run_id:
            return False
        matches = [s for s in data["suites"] if s["name"] == "lint.test.sh"]
        return len(matches) == 1 and matches[0]["state"] == "passed"
    except (OSError, ValueError, KeyError, TypeError):
        return False


def tauri(root, args, rows, tools, report):
    started = time.monotonic()
    deadline = started + TAURI_CAP
    clock = {"lock_wait": 0.0}
    print(f"Tauri Clippy running ({TAURI_CAP}-second cap on its own work; waiting for Cargo's "
          "lock is reported, not counted)", flush=True)
    counts, diagnostics = rust.collect(root, rust.TAURI, deadline, clock)
    report["tauri"] = dict(seconds=time.monotonic() - started, lock_wait_seconds=clock["lock_wait"],
                           counts=counts, diagnostics=diagnostics)
    if clock["lock_wait"]:
        print(f"Tauri Clippy waited {clock['lock_wait']:.1f}s for Cargo's lock (not counted in its cap)",
              flush=True)
    summarize("Tauri Clippy", counts, diagnostics)
    actual = record({"clippy": rust.TAURI}, tools, rust.lint_rules(root),
                    {p: r for p, r in rows.items() if r["language"] == "rust"}, counts)
    enforce(root, "tauri", actual, args)
    if time.monotonic() >= deadline + clock["lock_wait"]:
        raise TimeoutError("Tauri Clippy deadline expired")
    report["tauri"] = dict(seconds=time.monotonic() - started, lock_wait_seconds=clock["lock_wait"],
                           counts=counts, diagnostics=diagnostics)
    remember_counts(root, "tauri", tools, counts)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--fast", action="store_true", help="Rust fast set, shell and custom checks (default)")
    modes.add_argument("--all", action="store_true", help="fast set plus unconditional Tauri Clippy")
    modes.add_argument("--static", action="store_true", help="shell and custom checks only; not the full build gate")
    modes.add_argument("--js-report", action="store_true", help="report advisory JavaScript candidates only")
    modes.add_argument("--changed", action="store_true",
                       help="the commit check: static and load rules for what differs from HEAD, "
                            "Clippy only for the Rust sets whose inputs changed")
    updates = parser.add_mutually_exclusive_group()
    updates.add_argument("--lower", action="store_true", help="propose lower baselines as a working-tree diff")
    updates.add_argument("--bootstrap", action="store_true", help="create initial baselines only when absent on integration")
    parser.add_argument("--strict", action="store_true",
                        help="with --changed: refuse any count this change grows, whatever room the ceiling has")
    parser.add_argument("--trusted-ref", default="refs/heads/main")
    parser.add_argument("--suite-results", type=Path, help="current nightly script-suite receipt")
    parser.add_argument("--json-out", type=Path, help="write detailed measurement output to the named file")
    args = parser.parse_args(argv)
    started = time.monotonic()
    report = {}
    try:
        # All tool invocations use the same controlled profiles as the nightly.
        os.environ["CARGO_PROFILE_DEV_DEBUG"] = "0"
        os.environ["CARGO_PROFILE_TEST_DEBUG"] = "0"
        rows = inventory(ROOT)
        report["source"] = {
            "revision": checked(["git", "rev-parse", "HEAD"], ROOT).strip(),
            "working_diff_sha256": hashlib.sha256(checked(["git", "diff", "HEAD", "--binary"], ROOT).encode()).hexdigest(),
        }
        report["target_directory"] = os.environ.get("CARGO_TARGET_DIR", "Cargo default")
        if args.js_report:
            report.update(js_report(ROOT, rows))
            print(json.dumps(report, indent=2))
            return 0
        # Standalone measurement scheduling belongs to the operator. The lint
        # never probes release.lock or host load, including inside a nightly
        # that already owns that lock. Cargo arbitrates its own cache lock.
        if args.strict and not args.changed:
            raise Refusal("--strict applies to --changed only")
        if args.changed:
            if args.lower or args.bootstrap:
                raise Refusal("--changed only checks; --lower and --bootstrap need a full mode")
            changed_rust = [p for p in changed_paths(ROOT) if rust_input(p) or p.startswith(LINT_MACHINERY)]
            tool_versions = versions(ROOT, cargo=bool(changed_rust))
            report["versions"] = tool_versions
            changed(args, rows, tool_versions, report)
            print(f"Lint passed in {time.monotonic() - started:.2f}s", flush=True)
            return 0
        tool_versions = versions(ROOT, cargo=not args.static)
        report["versions"] = tool_versions
        # lint.test.sh runs `lint.sh --all` (fast AND Tauri) inside the script suites. When this
        # run's receipt shows it passed, the same source already passed every check `--all`
        # makes here, Tauri Clippy included, so none is repeated (hunt part 2 v3, R47: only
        # the fast phase was reused, and the nightly ran Tauri Clippy twice).
        if args.all and fast_was_run(args.suite_results, os.environ.get("RICHOS_NIGHTLY_RUN_ID")):
            print("lint.test.sh passed `lint.sh --all` in this run's script suites: its fast and "
                  "Tauri lint are not run again", flush=True)
            report["reused"] = "lint.test.sh"
        else:
            static_full(args, rows, tool_versions, report)
            if not args.static:
                rust_fast(args, rows, tool_versions, report)
            if args.all:
                tauri(ROOT, args, rows, tool_versions, report)
        print(f"Lint passed in {time.monotonic() - started:.2f}s", flush=True)
        return 0
    except (Refusal, OSError, ValueError, subprocess.TimeoutExpired, subprocess.CalledProcessError, TimeoutError) as exc:
        if isinstance(exc, TimeoutError) and "Cargo's lock" in str(exc):
            message = f"lint deadline refused: {exc}; launched work stopped"
        elif isinstance(exc, (TimeoutError, subprocess.TimeoutExpired)):
            message = (f"lint deadline refused: Tauri cap {TAURI_CAP} seconds of Clippy's own work "
                       "(waiting for Cargo's lock not counted); launched work stopped"
                       if args.all else "lint command deadline expired; launched work stopped")
        else:
            message = str(exc)
        report["refusal"] = message
        print("Lint refused: " + message, file=sys.stderr)
        return 1
    finally:
        report["total_seconds"] = time.monotonic() - started
        if args.json_out:
            args.json_out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    def interrupted(_signum, _frame):
        raise KeyboardInterrupt("lint interrupted; cleaning up owned commands")
    signal.signal(signal.SIGTERM, interrupted)
    sys.exit(main())
