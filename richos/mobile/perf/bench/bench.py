#!/usr/bin/env python3
"""bench.py — hardware benchmarks of a physical Android phone, behind `randroid device bench`.

    randroid device --serial S bench [chip|coremark|empty-start|all] [--runs 3] [--trials 20]
                                     [--ndk DIR] [--out FILE.json]

  chip         the SoC, cores and max clock per cluster, and RAM, read from the phone
  coremark     EEMBC CoreMark (vendored at ./coremark, v1.01) cross-compiled with the NDK for arm64 at -O2,
               pushed to /data/local/tmp/richos-bench, run once on one thread and once on every core,
               --runs times each, valid runs only (CoreMark's own run is at least 10 s); the binary is
               removed from the phone afterwards
  empty-start  cold starts of an app that does nothing (./empty-app, package dev.richos.bench.empty,
               one Activity with one TextView), installed beside RichConnect and removed again by that
               exact package name; `am start -W` TotalTime (the first frame), median and p95

Only through `randroid device` (physical.require_verb). The phone must hold the release RichConnect
(physical.gate); RichConnect is never touched, and nothing is left on the phone.
"""
import argparse
import glob
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
import machine_storage as M

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1]))
sys.path.insert(0, str(HERE.parent))
import physical  # noqa: E402
from physical import CannotAnswer, Refused  # noqa: E402

EMPTY_PACKAGE = "dev.richos.bench.empty"
EMPTY_ACTIVITY = f"{EMPTY_PACKAGE}/.MainActivity"
REMOTE_DIR = "/data/local/tmp/richos-bench"
SOURCES = ["core_list_join.c", "core_main.c", "core_matrix.c", "core_state.c", "core_util.c", "linux64/core_portme.c"]
COREMARK_ARGS = "0x0 0x0 0x66 0 7 1 2000"  # the performance run: iterations 0 = sized to run at least 10 s
MIN_RUN_SECONDS = 10.0
WHAT = ("chip", "coremark", "empty-start", "all")


# -- pure parsers (tested without a phone) --------------------------------------------------------

def parse_chip(props, freqs, meminfo, cpuinfo):
    """props: {name: value}; freqs: {cpu index: kHz}; meminfo, cpuinfo: the files' text."""
    clusters = {}
    for cpu, khz in sorted(freqs.items()):
        clusters.setdefault(khz, []).append(cpu)
    mem = re.search(r"MemTotal:\s+(\d+) kB", meminfo)
    parts = {}
    for p in re.findall(r"CPU part\s*:\s*(0x[0-9a-fA-F]+)", cpuinfo):
        parts[p.lower()] = parts.get(p.lower(), 0) + 1
    return {"soc_model": props.get("ro.soc.model") or None, "board_platform": props.get("ro.board.platform") or None,
            "hardware": props.get("ro.hardware") or None, "model": props.get("ro.product.model") or None,
            "android": props.get("ro.build.version.release") or None, "abi": props.get("ro.product.cpu.abi") or None,
            "cores": len(freqs),
            "clusters": [{"max_mhz": khz / 1000, "cores": len(cpus), "cpus": cpus} for khz, cpus in sorted(clusters.items())],
            "cpu_parts": parts, "ram_mib": round(int(mem.group(1)) / 1024) if mem else None}


def parse_coremark(text):
    """CoreMark's report -> {"iterations_per_sec", "total_seconds", "valid"}; raises ValueError when unreadable."""
    ips = re.search(r"Iterations/Sec\s*:\s*([0-9.]+)", text)
    secs = re.search(r"Total time \(secs\)\s*:\s*([0-9.]+)", text)
    if not ips or not secs:
        raise ValueError("no Iterations/Sec and Total time in the CoreMark output: " + " ".join(text.split())[:200])
    valid = "Correct operation validated" in text and "ERROR" not in text and float(secs.group(1)) >= MIN_RUN_SECONDS
    return {"iterations_per_sec": float(ips.group(1)), "total_seconds": float(secs.group(1)), "valid": valid}


def percentile(values, q):
    """Nearest-rank percentile, q in (0, 100]."""
    if not values:
        raise ValueError("no values")
    ordered = sorted(values)
    rank = max(1, -(-len(ordered) * q // 100))
    return ordered[int(rank) - 1]


def summarize(values):
    return {"n": len(values), "median": statistics.median(values), "p95": percentile(values, 95),
            "min": min(values), "max": max(values)}


def parse_args(argv):
    p = argparse.ArgumentParser(prog="randroid device bench", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("what", nargs="?", default="all", choices=WHAT)
    p.add_argument("--adb", default="adb")
    p.add_argument("--serial", required=True)
    p.add_argument("--runs", type=int, default=3)
    p.add_argument("--trials", type=int, default=20)
    p.add_argument("--ndk")
    p.add_argument("--out")
    a = p.parse_args(argv)
    if a.runs < 1 or a.trials < 1:
        p.error("--runs and --trials must be at least 1")
    return a


# -- phone ------------------------------------------------------------------------------------

def sh(a, command, timeout=120):
    p = physical.adb(a.adb, a.serial, "shell", command, timeout=timeout)
    return p.returncode, p.stdout


def read_chip(a):
    rc, out = sh(a, "for k in ro.soc.model ro.board.platform ro.hardware ro.product.model ro.build.version.release "
                    "ro.product.cpu.abi; do echo \"$k=$(getprop $k)\"; done; echo --freqs--; "
                    "for c in /sys/devices/system/cpu/cpu[0-9]*; do echo \"${c##*cpu} $(cat $c/cpufreq/cpuinfo_max_freq)\"; done; "
                    "echo --mem--; grep MemTotal /proc/meminfo; echo --cpuinfo--; grep 'CPU part' /proc/cpuinfo")
    props, freqs = {}, {}
    head, _, rest = out.partition("--freqs--")
    for line in head.splitlines():
        k, _, v = line.partition("=")
        props[k.strip()] = v.strip()
    fr, _, rest = rest.partition("--mem--")
    for line in fr.splitlines():
        m = re.match(r"(\d+) (\d+)$", line.strip())
        if m:
            freqs[int(m.group(1))] = int(m.group(2))
    mem, _, cpuinfo = rest.partition("--cpuinfo--")
    return parse_chip(props, freqs, mem, cpuinfo)


def battery_temp(a):
    _, out = sh(a, "dumpsys battery | grep -i temperature")
    m = re.search(r"temperature:\s*(\d+)", out)
    return int(m.group(1)) / 10 if m else None


def find_ndk(a):
    candidates = [a.ndk] if a.ndk else []
    candidates += [os.environ.get("ANDROID_NDK_HOME")] + sorted(glob.glob(os.path.join(os.environ.get("ANDROID_SDK_ROOT", os.path.expanduser("~/Library/Android/sdk")), "ndk/*")), reverse=True)
    for c in candidates:
        if c and glob.glob(f"{c}/toolchains/llvm/prebuilt/*/bin/aarch64-linux-android29-clang"):
            return c
    raise CannotAnswer("no Android NDK: pass --ndk DIR or set ANDROID_NDK_HOME (sdkmanager --sdk_root=<dir> 'ndk;29.0.14206865')")


def build_coremark(ndk, work, threads):
    clang = glob.glob(f"{ndk}/toolchains/llvm/prebuilt/*/bin/aarch64-linux-android29-clang")[0]
    out = Path(work) / f"coremark-{threads}"
    src = HERE / "coremark"
    flags = ["-O2", "-DPERFORMANCE_RUN=1", f"-DMULTITHREAD={threads}", "-I", str(src), "-I", str(src / "linux64"),
             '-DFLAGS_STR="-O2"']
    if threads > 1:
        flags.append("-DUSE_PTHREAD")
    cmd = [clang, *flags, *[str(src / s) for s in SOURCES], "-o", str(out)]
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        raise CannotAnswer("CoreMark did not compile: " + p.stderr.strip()[-400:])
    return out


def run_coremark(a, work, ndk, cores):
    results = {}
    sh(a, f"mkdir -p {REMOTE_DIR}")
    try:
        for label, threads in (("single", 1), ("multi", cores)):
            binary = build_coremark(ndk, work, threads)
            remote = f"{REMOTE_DIR}/{binary.name}"
            q = physical.adb(a.adb, a.serial, "push", str(binary), remote, timeout=120)
            if q.returncode != 0:
                raise CannotAnswer("could not push the CoreMark binary: " + (q.stderr or q.stdout).strip()[:200])
            sh(a, f"chmod 755 {remote}")
            runs = []
            for i in range(a.runs):
                temp = battery_temp(a)
                rc, out = sh(a, f"{remote} {COREMARK_ARGS}", timeout=300)
                try:
                    r = parse_coremark(out)
                except ValueError as e:
                    r = {"valid": False, "error": str(e)}
                r["battery_temp_c_before"] = temp
                runs.append(r)
                print(f"coremark {label} {i + 1}/{a.runs}: {r}", file=sys.stderr, flush=True)
                time.sleep(20)  # let the SoC cool between runs
            good = [r["iterations_per_sec"] for r in runs if r.get("valid")]
            results[label] = {"threads": threads, "runs": runs, "valid_runs": len(good),
                              "iterations_per_sec": summarize(good) if good else None}
    finally:
        sh(a, f"rm -rf {REMOTE_DIR}")
    return results


def build_empty(work):
    home = os.environ.get("ANDROID_HOME", "/opt/homebrew/share/android-commandlinetools")
    bt = sorted(glob.glob(f"{home}/build-tools/*"))[-1]
    jar = sorted(glob.glob(f"{home}/platforms/android-34/android.jar") or glob.glob(f"{home}/platforms/android-*/android.jar"))[-1]
    keystore = os.environ.get("RICHOS_BENCH_KEYSTORE") or str(M.cache("android/debug.keystore"))
    w, src = Path(work), HERE / "empty-app"
    classes = w / "classes"
    classes.mkdir()

    def run(*cmd):
        p = subprocess.run(cmd, capture_output=True, text=True, cwd=w)
        if p.returncode != 0:
            raise CannotAnswer(f"{Path(cmd[0]).name} failed: {(p.stderr or p.stdout).strip()[-400:]}")

    run(f"{bt}/aapt2", "link", "-o", "base.apk", "--manifest", str(src / "AndroidManifest.xml"), "-I", jar,
        "--min-sdk-version", "28", "--target-sdk-version", "34")
    run("javac", "--release", "8", "-cp", jar, "-d", str(classes), str(src / "src/dev/richos/bench/empty/MainActivity.java"))
    run(f"{bt}/d8", "--release", "--lib", jar, "--output", str(w), *glob.glob(f"{classes}/**/*.class", recursive=True))
    run("zip", "-q", "-j", "base.apk", "classes.dex")
    run(f"{bt}/zipalign", "-f", "4", "base.apk", "aligned.apk")
    run(f"{bt}/apksigner", "sign", "--ks", keystore, "--ks-pass", "pass:android", "--out", "empty.apk", "aligned.apk")
    return w / "empty.apk"


def list_richos(a):
    _, out = sh(a, "pm list packages | grep richos")
    return sorted(line.strip() for line in out.splitlines() if line.strip())


def run_empty_start(a, work):
    from android import parse_am_start
    from perfcore import Unmeasurable
    apk = build_empty(work)
    before = list_richos(a)
    if f"package:{EMPTY_PACKAGE}" in before:
        raise Refused(f"{EMPTY_PACKAGE} is already installed; remove it first (nothing else is touched)")
    installed = False
    first, rejected = [], []
    try:
        p = physical.adb(a.adb, a.serial, "install", str(apk), timeout=300)
        if p.returncode != 0 or "Success" not in p.stdout + p.stderr:
            raise CannotAnswer("the phone refused the benchmark app: " + (p.stderr or p.stdout).strip()[-300:])
        installed = True
        during = list_richos(a)
        for i in range(a.trials):
            sh(a, f"am force-stop {EMPTY_PACKAGE}")
            time.sleep(1.0)
            _, out = sh(a, f"am start -W -n {EMPTY_ACTIVITY}")
            try:
                launch = parse_am_start(out)
            except Unmeasurable as e:
                rejected.append({"trial": i + 1, "why": str(e)})
                continue
            if launch["launchState"] != "COLD":
                rejected.append({"trial": i + 1, "why": f"LaunchState {launch['launchState']}, not COLD"})
                continue
            first.append(launch["totalMs"])
            print(f"empty cold {i + 1}/{a.trials}: {launch['totalMs']} ms", file=sys.stderr, flush=True)
            time.sleep(1.0)
    finally:
        sh(a, f"am force-stop {EMPTY_PACKAGE}")
        sh(a, "input keyevent KEYCODE_HOME")
        if installed:
            physical.adb(a.adb, a.serial, "uninstall", EMPTY_PACKAGE, timeout=120)  # device-cli-exempt: only this benchmark's own package, named exactly, never RichConnect
    after = list_richos(a)
    if not first:
        raise CannotAnswer("no cold start could be measured: " + json.dumps(rejected)[:300])
    return {"package": EMPTY_PACKAGE, "metric": "am start -W TotalTime (first frame), ms", "cold_ms": first,
            "summary": summarize(first), "rejected": rejected,
            "packages_before": before, "packages_during": during, "packages_after": after}


def main(argv):
    a = parse_args(argv)
    work = tempfile.mkdtemp(prefix="richos-bench-", dir=os.environ.get("TMPDIR") or None)
    try:
        physical.require_verb("randroid")
        build = physical.gate(a.adb, a.serial)
        record = {"serial": a.serial, "richconnect": {"version": build["versionName"], "configuration": build["configuration"]}}
        chip = read_chip(a)
        if a.what in ("chip", "all"):
            record["chip"] = chip
        if a.what in ("coremark", "all"):
            record["coremark"] = run_coremark(a, work, find_ndk(a), chip["cores"])
        if a.what in ("empty-start", "all"):
            record["empty_start"] = run_empty_start(a, work)
        text = json.dumps(record, indent=2)
        if a.out:
            Path(a.out).parent.mkdir(parents=True, exist_ok=True)
            Path(a.out).write_text(text + "\n")
        print(text)
        return 0
    except Refused as e:
        print(json.dumps({"ok": False, "refused": str(e)}), file=sys.stderr)
        return 3
    except CannotAnswer as e:
        print(json.dumps({"ok": False, "error": str(e)}), file=sys.stderr)
        return 2
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
