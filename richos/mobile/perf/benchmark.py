"""benchmark — a new perf record against the start times RichConnect has already achieved.

The private benchmark file (found through RICHOS_MOBILE_PERF_BENCHMARKS, else the default in `DEFAULT`;
this public tree holds no phone-derived number) holds, per class, the best p95 measured so far for
cold launch and warm resume, each with the record, the build commit and the date it came from, and
the run-to-run noise allowance derived from the repeated series of that class. A class is a device
(`match`: platform, device kind, model, build configuration) AND a condition (`condition`: the
seeded conversation, the Mac's state and the build type; condition.py). A record is compared only
with a class whose device matches and whose condition is equal on every condition.KEYS field;
anything else is NOT COMPARED with the reason "different conditions", which never passes and never
fails. A class that was never measured as a distribution says so in the file and carries no
number. Classes taken out of use stay in the file under `retired`, with the reason, and are never
compared against.

    compare(records, bench)  every metric, each FASTER, WITHIN NOISE, SLOWER or NOT COMPARED (why)
    update(bench, records)   the only way a number in the file changes; the caller commits the file

The noise rule. A p95 from n trials is itself a random number: run the same build again and it
moves. For each repeated series of a class, `p95_noise` resamples the series' own samples with
replacement (2000 resamples, seeded, so the same samples always give the same answer), takes the
nearest-rank p95 of each resample, and calls the standard deviation of those p95s the series'
standard error (SE). The difference of two independent runs' p95s has standard error sqrt(2) x SE,
so 1.96 x sqrt(2) x SE is its 95% bound: a rerun of the same build lands above the benchmark by more
than that about one time in forty. That is the series' two-run bound, as a percent of its p95. A class's allowance for a metric is the MEDIAN of its
series' bounds: one series dominated by a single long-tail trial cannot widen it for everyone.
A record is SLOWER when its p95 exceeds benchmark x (1 + allowance). Nothing else loosens it.

Why not the spread between the series: series measured on different builds differ by real code
changes, and that is the kind of slowdown this check exists to catch, so it cannot also be the allowance.
"""
import hashlib
import json
import math
import os
import random
import statistics
import subprocess
from pathlib import Path

import condition
import perfcore
from perfcore import Refused

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ENV_VAR = "RICHOS_MOBILE_PERF_BENCHMARKS"
# Where the benchmark file lives by default on this Mac: the private record repository, never this one.
_common = subprocess.run(["git", "-C", REPO, "rev-parse", "--path-format=absolute", "--git-common-dir"], capture_output=True, text=True, timeout=30)
DEFAULT = str((Path(_common.stdout.strip()).parent if _common.returncode == 0 else Path(REPO)).parent / "richos-hq/docs/mobile-perf/benchmarks.json")
NO_FILE = "no private benchmark file"
SCHEMA = "richos-mobile-perf-benchmarks/2"
METRICS = ("coldLaunch", "warmResume")
BOOTSTRAP_RESAMPLES = 2000
BOOTSTRAP_SEED = 95
Z = 1.96

FASTER, WITHIN, SLOWER, NOT_COMPARED = "FASTER", "WITHIN NOISE", "SLOWER", "NOT COMPARED"
VERDICT_SLOWER = "SLOWER THAN THE ESTABLISHED BENCHMARK"
VERDICT_OK = "NOT SLOWER"
VERDICT_NONE = "NOT COMPARED"


def lookup(record, dotted):
    node = record
    for part in dotted.split("."):
        if not isinstance(node, dict):
            return None
        node = node.get(part)
    return node


def p95_noise(samples):
    """The bootstrap standard error of a series' nearest-rank p95 and its two-run bound (module doc).
    None below 20 samples, where there is no p95 to be noisy."""
    n = len(samples)
    if n < 20:
        return None
    rng = random.Random(BOOTSTRAP_SEED)
    boots = [perfcore.percentile([rng.choice(samples) for _ in samples], 95) for _ in range(BOOTSTRAP_RESAMPLES)]
    se = statistics.pstdev(boots)
    base = perfcore.percentile(samples, 95)
    bound = Z * math.sqrt(2) * se
    return {"n": n, "p95Ms": base, "bootstrapSeMs": round(se, 2), "twoRunBoundMs": round(bound, 2),
            "twoRunBoundPercent": round(100.0 * bound / base, 2) if base else None}


def allowance(series):
    bounds = [s["twoRunBoundPercent"] for s in series if s.get("twoRunBoundPercent") is not None]
    return round(statistics.median(bounds), 2) if bounds else None


class NoBenchmarkFile(Refused):
    """The private benchmark file is absent (a public clone): nothing can be compared."""


def resolve(path=None):
    """The benchmark file: the explicit path, else the environment variable, else DEFAULT."""
    return path or os.environ.get(ENV_VAR) or DEFAULT


def load(path=None):
    path = resolve(path)
    if not os.path.isfile(path):
        raise NoBenchmarkFile(f"{NO_FILE} (looked at {path}; set {ENV_VAR} or pass --benchmark)")
    try:
        with open(path) as f:
            bench = json.load(f)
    except (OSError, ValueError) as e:
        raise Refused(f"the benchmark file {path} is unreadable: {e}")
    if bench.get("schema") != SCHEMA:
        raise Refused(f"{path}: schema is {bench.get('schema')!r}, not {SCHEMA} (since 2026-10-02 every class "
                      "names the condition it was measured under)")
    for old in bench.get("retired") or []:
        if not old.get("name") or not old.get("retiredBecause"):
            raise Refused(f"{path}: a retired class lacks name or retiredBecause: {old.get('name')!r}")
    for cls in bench.get("classes") or []:
        if not cls.get("name") or not isinstance(cls.get("match"), dict) or not isinstance(cls.get("metrics"), dict):
            raise Refused(f"{path}: a class lacks name, match or metrics: {cls.get('name')!r}")
        # A class with numbers names the condition they were taken under; a class still waiting for its
        # first distribution may not have one yet, and nothing is compared with it until it does.
        problems = condition.check(cls.get("condition")) if cls["metrics"] or cls.get("condition") else []
        if problems:
            raise Refused(f"{path}: class {cls['name']} {problems[0]}")
        if not cls["metrics"] and not cls.get("neverEstablished"):
            raise Refused(f"{path}: class {cls['name']} has no numbers and does not say why (neverEstablished)")
        for name, m in cls["metrics"].items():
            for key in ("p95Ms", "allowancePercent", "source", "noiseSeries"):
                if m.get(key) in (None, [], {}):
                    raise Refused(f"{path}: {cls['name']} {name} has no {key}")
            for key in ("record", "commit", "date"):
                if not m["source"].get(key):
                    raise Refused(f"{path}: {cls['name']} {name}'s source names no {key}")
    return bench


def find_class(record, bench, cond=None):
    """The class whose every match key equals the record's AND whose condition equals `cond` (the
    record's condition); else (None, why). A device match under another condition answers
    "different conditions", never the nearest class."""
    near, same_device = [], []
    for cls in bench.get("classes") or []:
        diffs = [f"{k} is {lookup(record, k)!r}, the benchmark's is {v!r}"
                 for k, v in cls["match"].items() if lookup(record, k) != v]
        if not diffs:
            same_device.append(cls)
            continue
        same_kind = all(lookup(record, k) == cls["match"].get(k) for k in ("platform", "device.kind") if k in cls["match"])
        if same_kind:
            near.append(f"{cls['name']}: " + "; ".join(diffs))
    if not same_device:
        what = f"{record.get('platform')} {lookup(record, 'device.kind')} {lookup(record, 'device.model')!r}"
        return None, (f"no benchmark class for {what}" + (f" ({' | '.join(near)})" if near else ""))
    reasons = []
    for cls in same_device:
        why = condition.why_not_comparable(cond, cls.get("condition"))
        if why is None:
            return cls, None
        if why not in [r for _, r in reasons]:
            reasons.append((cls["name"], why))
    if len(reasons) == 1:
        return None, reasons[0][1]
    lead = condition.DIFFERENT + ": "
    return None, lead + " | ".join(f"{name}: {why[len(lead):]}" for name, why in reasons)


def _identity(record):
    return (record.get("platform"), lookup(record, "device.kind"), lookup(record, "device.model"),
            lookup(record, "build.configuration"))


def _metrics(records):
    """One metric table from one or more records of ONE build (a split iOS series is a pair)."""
    first = records[0]
    table = {}
    for r in records:
        if _identity(r) != _identity(first):
            raise Refused(f"cannot compare together: {_identity(r)} is not {_identity(first)}")
        built = lookup(r, "build.builtSha256") or lookup(r, "build.installedSha256")
        if built != (lookup(first, "build.builtSha256") or lookup(first, "build.installedSha256")):
            raise Refused("cannot compare together: the records measured different build bytes")
        for name in METRICS:
            if name in (r.get("metrics") or {}):
                if name in table:
                    raise Refused(f"cannot compare together: {name} appears in two records")
                table[name] = r["metrics"][name]
    return table


def compare(records, bench, bench_path=None, conditions=None):
    """Every metric in METRICS, compared or said why not. Never only a pass. `conditions` is each
    record's condition when it is not the record's own (a declaration, condition.apply)."""
    if not records:
        raise Refused("no record to compare")
    table = _metrics(records)
    conds = list(conditions) if conditions is not None else [r.get("condition") for r in records]
    if any(not condition.same(c, conds[0]) for c in conds[1:]):
        raise Refused(f"cannot compare together: the records were measured under {condition.DIFFERENT}")
    cls, why = find_class(records[0], bench, conds[0])
    out = {"benchmarkFile": _display(resolve(bench_path)), "class": cls["name"] if cls else None,
           "condition": (conds[0] or {}).get("name"),
           "rule": "SLOWER when p95 > benchmark p95 x (1 + allowancePercent/100); allowance = median over "
                   "the class's repeated series of 1.96 x sqrt(2) x bootstrap SE of p95 (benchmark.py)",
           "metrics": {}}
    for name in METRICS:
        row = {"status": NOT_COMPARED}
        metric = table.get(name)
        summary = (metric or {}).get("stats") or {}
        p95 = summary.get("p95")
        if metric is not None:
            row.update({"p95Ms": p95, "n": summary.get("n")})
        if cls is None:
            row["why"] = why
        elif name not in cls["metrics"]:
            row["why"] = cls.get("neverEstablished") or f"{cls['name']} has no established {name}"
        elif metric is None:
            row["why"] = f"the record has no {name}"
        elif p95 is None:
            row["why"] = summary.get("why") or f"{name} has no p95"
        else:
            b = cls["metrics"][name]
            limit = round(b["p95Ms"] * (1 + b["allowancePercent"] / 100.0), 2)
            delta = round(p95 - b["p95Ms"], 2)
            row.update({"benchmarkP95Ms": b["p95Ms"], "allowancePercent": b["allowancePercent"], "limitMs": limit,
                        "deltaMs": delta, "deltaPercent": round(100.0 * delta / b["p95Ms"], 2),
                        "benchmarkSource": {k: b["source"][k] for k in ("record", "commit", "date")}})
            if p95 > limit:
                row["status"] = SLOWER
            elif p95 < b["p95Ms"]:
                row["status"] = FASTER
                row["note"] = ("faster than the established benchmark; it raises the benchmark only through "
                               "`perf.py benchmark-update` and a commit of benchmarks.json")
            else:
                row["status"] = WITHIN
        out["metrics"][name] = row
    statuses = [r["status"] for r in out["metrics"].values()]
    out["verdict"] = (VERDICT_SLOWER if SLOWER in statuses
                      else VERDICT_NONE if all(s == NOT_COMPARED for s in statuses) else VERDICT_OK)
    return out


def lines(result):
    """The comparison as text: one line per metric, then the verdict."""
    rows = []
    for name, r in result["metrics"].items():
        if r["status"] == NOT_COMPARED:
            rows.append(f"{name:11s} {r['status']:12s} {r['why']}")
            continue
        src = r["benchmarkSource"]
        rows.append(f"{name:11s} {r['status']:12s} p95 {r['p95Ms']} ms (n {r['n']}) vs benchmark {r['benchmarkP95Ms']} ms "
                    f"({r['deltaMs']:+} ms, {r['deltaPercent']:+}%; allowance {r['allowancePercent']}% -> limit "
                    f"{r['limitMs']} ms) [benchmark {src['commit'][:8]} {src['date']}]")
    rows.append(f"verdict: {result['verdict']} (class {result['class']}, condition {result.get('condition')}, "
                f"{result['benchmarkFile']})")
    return rows


def _display(path):
    """A path as the benchmark file names it: repository-relative inside this repository,
    `richos-hq/...` for the private record, otherwise as given."""
    full = os.path.realpath(path)
    if full.startswith(os.path.realpath(REPO) + os.sep):
        return os.path.relpath(full, os.path.realpath(REPO))
    marker = os.sep + "richos-hq" + os.sep
    if marker in full:
        return "richos-hq" + os.sep + full.split(marker, 1)[1]
    top = _worktree_of(full, "richos-hq")
    if top:  # a richos-hq worktree: name the file as it will be in the repository
        return "richos-hq" + os.sep + os.path.relpath(full, top)
    return path


def _worktree_of(full, repo_name):
    """The top of the git worktree holding `full` when that worktree belongs to a repository named
    `repo_name` (its .git file points into <repo_name>/.git/worktrees/); else None."""
    here = os.path.dirname(full)
    while here and here != os.path.dirname(here):
        dot_git = os.path.join(here, ".git")
        if os.path.isfile(dot_git):
            with open(dot_git) as f:
                line = f.read().strip()
            gitdir = line.split("gitdir:", 1)[1].strip() if line.startswith("gitdir:") else ""
            main = gitdir.split(os.sep + ".git" + os.sep + "worktrees" + os.sep, 1)[0]
            return here if os.path.basename(main) == repo_name else None
        if os.path.isdir(dot_git):
            return None
        here = os.path.dirname(here)
    return None


def _sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def record_condition(record, path, declarations=None):
    """(condition, declaration source): the record's own condition, or a declaration's for exactly
    this file; Refused when a declaration contradicts the record."""
    try:
        return condition.apply(record, path, declarations)
    except condition.ConditionError as e:
        raise Refused(str(e))


def update(bench, paths, allow_slower=None, declarations=None):
    """Bring each record's cold and warm series into its class: established when the class had no
    number, raised when faster, kept when not faster (unless `allow_slower` gives the reason, which
    is written beside the number). Every series joins the class's noise series and the allowance is
    recomputed. A record joins only a class of its own device AND condition. Returns one sentence
    per change; the caller writes and commits the file."""
    changes = []
    for path in paths:
        with open(path) as f:
            record = json.load(f)
        problems = perfcore.check_record(record)
        if problems:
            raise Refused(f"{path} is not a sound record: {problems[0]}")
        if lookup(record, "build.dirty"):
            raise Refused(f"{path} measured a build made from uncommitted changes; a benchmark names a commit")
        cond, declared = record_condition(record, path, declarations)
        if cond is None:
            raise Refused(f"{path} names no condition; a benchmark is taken only under a stated condition "
                          "(a record measured before perf.py wrote one needs a --condition-declaration)")
        cls, why = find_class(record, bench, cond)
        if cls is None:
            raise Refused(f"{path}: {why}; add the class (device and condition) to benchmarks.json first")
        label, digest = _display(path), _sha256(path)
        source = {"record": label, "recordSha256": digest, "commit": lookup(record, "build.commit"),
                  "date": (record.get("startedAt") or "")[:10]}
        if declared:
            source["conditionDeclaration"] = {"file": _display(declared["declaration"]),
                                              "sha256": declared["declarationSha256"]}
        for name in METRICS:
            samples = ((record.get("metrics") or {}).get(name) or {}).get("samplesMs") or []
            noise = p95_noise(samples)
            if noise is None:
                if samples:
                    changes.append(f"{cls['name']} {name}: {label} has {len(samples)} samples, no p95; not used")
                continue
            current = cls["metrics"].get(name)
            series = [s for s in (current or {}).get("noiseSeries", []) if s.get("recordSha256") != digest]
            series.append(dict(source, **noise))
            p95 = noise["p95Ms"]
            if not current:
                current = {"p95Ms": p95, "source": source}
                changes.append(f"{cls['name']} {name}: established at {p95} ms from {label}")
            elif p95 < current["p95Ms"]:
                changes.append(f"{cls['name']} {name}: raised {current['p95Ms']} -> {p95} ms (faster) from {label}")
                current = {"p95Ms": p95, "source": source}
            elif p95 > current["p95Ms"] and allow_slower:
                changes.append(f"{cls['name']} {name}: LOWERED {current['p95Ms']} -> {p95} ms from {label}: {allow_slower}")
                current = {"p95Ms": p95, "source": source, "loweredBecause": allow_slower}
            else:
                changes.append(f"{cls['name']} {name}: kept {current['p95Ms']} ms; {label} measured {p95} ms "
                               "(not faster; it joins the noise series only)")
                current = {k: v for k, v in current.items() if k not in ("allowancePercent", "noiseSeries")}
            current["noiseSeries"] = series
            current["allowancePercent"] = allowance(series)
            cls["metrics"][name] = {k: current[k] for k in ("p95Ms", "allowancePercent", "source", "loweredBecause", "noiseSeries")
                                    if k in current}
            cls.pop("neverEstablished", None)
    return changes


def dump(bench):
    return json.dumps(bench, indent=2, ensure_ascii=False) + "\n"
