"""Load-sensitive checks in test code: the rule from the 2026-09-29 audit, section 5.

The rule (docs/verification/2026-09-29-load-sensitive-checks-audit.md): a test may bound
TIME only to catch a hang, never to decide a verdict; and a deadline measures execution,
never queueing. Every failure the audit ranks has one shape: a check turns the speed of the
Mac into a fact about the product. An idle Mac hides it and a loaded nightly exposes it.

Five blocking IDs, in app and engine test code only (`*.test.sh`, `*.test.py`, `ui/tests/**/*.js`,
Rust `tests/*.rs` and `#[cfg(test)]` code):

  wall-clock-verdict      an assertion comparing a measured duration against an upper literal
  short-deadline          a literal deadline under 30 s on a wait for real work
  sleep-then-assert       a sleep followed, before any condition wait, by an assertion
  host-sample             top / load average / vm_stat / memory_pressure / uptime executed
  env-mutation-unguarded  std::env::set_var/remove_var in a Rust test with no static guard

A site is exempt only by `load-bound: <why this cannot depend on host load>` on the line or
the line above. A bare marker, or a reason under ten characters, exempts nothing.

SITES, NOT COUNTS. The other ratchets count, and their README says why that is weak: fixing
one and adding another can leave the same count. These rules baseline each SITE (file, rule
and a hash of the whitespace-normalized line), so any new site fails even while an old one
is being removed. The sites that existed when the rule was introduced are in
`baselines/load.json` and are paid down; nothing new gets in undeclared.

These are structural checks, not a parser; parenthesized assertions may span lines. What each recognizes is written beside its
pattern below; a shape outside them is not claimed. Fixture directories are data, never
scanned, as everywhere else in this lint.
"""
import hashlib
import json
import re
from pathlib import Path

from common import APP, Refusal

IDS = ("wall-clock-verdict", "short-deadline", "sleep-then-assert", "host-sample",
       "env-mutation-unguarded")
# Registered in driver.RULES beside the count rules. The kind is not "blocking": the count
# phase must not give them a ceiling, because their enforcement is per site (this module).
RULES = {rule: "blocking-site" for rule in IDS}
BASELINE = APP + "scripts/lint/baselines/load.json"
NOTE = "Sites, not counts: every site not in this file refuses. Check never writes."
DECLARED = re.compile(r"load-bound:\s*(.*)")
LIMIT_SECONDS = 30
LIT = r"(\d[\d_]*(?:\.\d+)?(?:e\d+)?)"


def number(text):
    return float(text.replace("_", ""))


# ---------------------------------------------------------------------------------------
# Which files are test code, and which of their lines
# ---------------------------------------------------------------------------------------

def language_of(path):
    """None when the path is not test code this rule covers."""
    if not path.startswith((APP, "richos/engine/")):
        return None
    p = Path(path)
    if "fixtures" in p.parts or "fixture" in p.parts or "node_modules" in p.parts:
        return None
    if p.name.endswith(".test.sh"):
        return "shell"
    if p.name.endswith(".test.py"):
        return "python"
    if p.suffix == ".js" and (path.startswith(APP + "ui/tests/") or p.name.endswith(".test.js")):
        return "javascript"
    if p.suffix == ".rs":
        return "rust"
    return None


def targets(paths):
    return {p: language_of(p) for p in paths if language_of(p)}


def rust_test_lines(path, lines):
    """Every line of a file under `tests/`; otherwise the items under `#[cfg(test)]`."""
    if "tests" in Path(path).parts:
        return set(range(len(lines)))
    marked = set()
    i = 0
    while i < len(lines):
        if not re.match(r"\s*#\[cfg\((?:all\()?test\b", lines[i]):
            i += 1
            continue
        depth, opened, j = 0, False, i
        while j < len(lines):
            code = re.sub(r'"(?:\\.|[^"\\])*"', '""', lines[j].split("//")[0])
            depth += code.count("{") - code.count("}")
            opened = opened or "{" in code
            marked.add(j)
            if opened and depth <= 0:
                break
            if not opened and code.rstrip().endswith(";") and j > i:
                break
            j += 1
        i = j + 1
    return marked


def comment(line, language):
    s = line.lstrip()
    if language in ("shell", "python"):
        return s.startswith("#")
    return s.startswith(("//", "/*", "*"))


# ---------------------------------------------------------------------------------------
# The five rules
# ---------------------------------------------------------------------------------------

ASSERT = {
    "javascript": re.compile(r"\bassert\w*\s*\(|\bexpect\s*\(|\bthrow\s+new\s+\w*Error\b"),
    "python": re.compile(r"\bself\.assert\w+\s*\(|^\s*assert\b|\bself\.fail\s*\("),
    "shell": re.compile(r"^\s*(?:if\s+!?\s*)?(?:assert\w*|expect\w*|check\w*|ok|bad|fail|pass)\b"
                        r"|\|\|\s*(?:bad|fail|die)\b|\bthen\s+(?:ok|bad|fail|pass)\b"
                        r"|^\s*(?:if|elif)\s+(?:\[|test\b|grep\b|!)"),
    "rust": re.compile(r"\b(?:debug_)?assert(?:_eq|_ne)?!\s*\(|\bpanic!\s*\("),
}
# A wait on a CONDITION ends a sleep's scope: what follows is judged on a fact, not a bet.
WAIT = {
    "javascript": re.compile(r"\bwaitFor(?!Timeout)\w*\s*\(|\bexpect\.poll\s*\(|\.toPass\s*\(|\bpoll\w*\s*\(|\bwhile\s*\("),
    "python": re.compile(r"^\s*while\b|\bwait_for\w*\s*\(|\bpoll\w*\s*\(|\.wait\s*\(|\.join\s*\("),
    "shell": re.compile(r"^\s*(?:while|until)\b|\bwait_for\w*|^\s*wait\b"),
    "rust": re.compile(r"^\s*(?:loop|while)\b|\.recv\s*\(|\.join\s*\(|\.wait\w*\s*\("),
}
SLEEP = {
    "javascript": re.compile(r"\bwaitForTimeout\s*\("),
    "python": re.compile(r"\btime\.sleep\s*\("),
    "shell": re.compile(r"(?:^|[;&|]\s*)\s*sleep\s+[0-9.]+"),
    "rust": re.compile(r"\bthread::sleep\s*\("),
}
LOOP_HEAD = re.compile(r"^\s*(?:for|while|until|loop|do)\b|\bfor\s*\(|\bwhile\s*\(")
SCOPE_END = re.compile(r"^\s*(?:def |fn |async fn |async function\b|function\b)|\brun\.check\s*\(|^\}")

# Measured durations. Inline forms, and names assigned from them earlier in the file.
DURATION_INLINE = (r"(?:(?:Date\.now|performance\.now)\(\)\s*-\s*[\w.]+"
                   r"|time\.(?:monotonic|time|perf_counter)\(\)\s*-\s*[\w.]+"
                   r"|[\w.]+\.elapsed\(\)(?:\.as_\w+\(\))?"
                   r"|[\w.]+At\s*-\s*[\w.]+At|\$?\bSECONDS\b)")
ASSIGNED = re.compile(r"(?:\b(?:const|let|var)\s+(?:mut\s+)?)?\b([A-Za-z_]\w*)\s*=\s*\$?\(*\s*"
                      r"(?:(?:Date\.now|performance\.now)\(\)\s*-|time\.(?:monotonic|time|perf_counter)\(\)\s*-"
                      r"|[\w.]+\.elapsed\(\)|\$?SECONDS\s*-|\$\(date \+%s\)\s*-)")
DURATION_NAME = r"[\w.]*(?:\b(?:elapsed|took|duration|waited)\w*|\w(?:Ms|_ms|Millis|_secs|Seconds|_seconds|Elapsed|Duration))\b"


def upper_bound(line, duration, language):
    d = rf"(?:{duration}|Math\.abs\(\s*(?:{duration}\s*-\s*{duration}|[\w.]+\.window\s*-\s*[\w.]+\.window)\s*\))"
    upper = [rf"{d}\s*<=?\s*{LIT}", rf"{LIT}\s*>=?\s*{d}",
             rf"{d}\s*<=?\s*Duration::from_\w+\(\s*{LIT}",
             rf"assertLess(?:Equal)?\(\s*{d}\s*,\s*{LIT}",
             rf"expect\(\s*{d}\s*\)\.toBeLessThan(?:OrEqual)?\(\s*{LIT}"]
    if language == "shell":
        # `[ "$t" -lt 5 ] || bad` passes only under the bound; `[ "$t" -gt 5 ] && bad` fails
        # over it. The other two readings are lower bounds, which are not load-sensitive.
        fails_when_true = re.search(r"(?:&&|\bthen)\s*(?:bad|fail|die|exit\s+[1-9])\b", line)
        op = "(?:gt|ge)" if fails_when_true else "(?:lt|le)"
        return bool(re.search(rf"\"?{d}\"?\s+-{op}\s+\"?{LIT}", line))
    if FAILS.search(line):
        # `if (took > 2000) throw ...`: the same verdict, written as the failing side.
        upper += [rf"{d}\s*>=?\s*{LIT}", rf"{d}\s*>=?\s*Duration::from_\w+\(\s*{LIT}"]
    return any(re.search(p, line) for p in upper)


FAILS = re.compile(r"\bthrow\b|\bself\.fail\b|\bpanic!|\b(?:bad|fail|die)\b")


def assertion_text(lines, index, language):
    """Join an assertion's parenthesized lines, retaining its starting source location.

    This remains a structural check. Quoted strings/comments do not contribute delimiters.
    Bound the lookahead so malformed source cannot turn the rest of a file into one site.
    """
    if language == "shell":
        return lines[index]
    parts, depth = [], 0
    for line in lines[index:index + 40]:
        parts.append(line)
        code = re.sub(r"(?:\"(?:\\.|[^\"])*\"|'(?:\\.|[^'])*')", "''", line)
        code = re.split(r"//|#", code, maxsplit=1)[0]
        depth += code.count("(") - code.count(")")
        if depth <= 0:
            break
    return " ".join(parts)


def wall_clock(lines, code, language):
    names = set()
    for i in code:
        for m in ASSIGNED.finditer(lines[i]):
            names.add(re.escape(m[1]))
    duration = "(?:" + "|".join([DURATION_INLINE, DURATION_NAME] + [rf"\b{n}\b" for n in sorted(names)]) + ")"
    found = []
    for i in code:
        line = lines[i]
        if language == "shell":
            judged = bool(re.search(r"\[\[?|\(\(", line)) and not re.match(r"\s*(?:while|until)\b", line)
        else:
            judged = bool(ASSERT[language].search(line))
        if judged and upper_bound(assertion_text(lines, i, language), duration, language):
            found.append(i)
    return found


def seconds(unit, value):
    return {"secs": value, "millis": value / 1000, "micros": value / 1e6, "nanos": value / 1e9}.get(
        unit.split("_")[0], value)


def short_deadline(lines, code, language):
    found = []
    for i in code:
        line = lines[i]
        hits = []
        if language in ("python", "shell"):
            hits += [number(v) for v in re.findall(rf"[(,]\s*timeout\s*=\s*{LIT}", line)]
        if language == "javascript":
            hits += [number(v) / 1000 for v in re.findall(rf"\btimeout\s*:\s*{LIT}", line)]
            hits += [number(v) / 1000 for v in re.findall(rf"\bwaitForFact\s*\([^;]*,\s*{LIT}\s*\)", line)]
        if language == "rust":
            for unit, v in re.findall(rf"(?:recv_timeout\s*\(\s*|Instant::now\(\)\s*\+\s*)Duration::from_(\w+)\(\s*{LIT}", line):
                hits.append(seconds(unit, number(v)))
        if any(h < LIMIT_SECONDS for h in hits):
            found.append(i)
    return found


QUOTES = {"rust": '"', "javascript": "\"'`", "python": "\"'", "shell": "\"'"}


def in_string(line, pos, language):
    """True when `pos` is inside a string literal opened earlier on the same line: a fixture
    program handed to a child (`"import time; time.sleep(60)"`) is data, not this test's wait."""
    quote, escaped = None, False
    for ch in line[:pos]:
        if escaped:
            escaped = False
        elif ch == "\\":
            escaped = True
        elif quote:
            quote = None if ch == quote else quote
        elif ch in QUOTES[language]:
            quote = ch
    return quote is not None


def sleep_then_assert(lines, code, language):
    found = []
    ordered = sorted(code)
    for pos, i in enumerate(ordered):
        match = SLEEP[language].search(lines[i])
        if not match or in_string(lines[i], match.start(), language):
            continue
        indent = len(lines[i]) - len(lines[i].lstrip())
        # A sleep inside a loop is a poll interval: the loop's own condition is the wait.
        above = [lines[k] for k in range(max(0, i - 10), i)
                 if lines[k].strip() and len(lines[k]) - len(lines[k].lstrip()) < indent]
        rest = lines[i][match.end():]
        if any(LOOP_HEAD.search(line) for line in above) or LOOP_HEAD.search(lines[i][:match.start()]) \
                or re.search(r"\bdone\b", rest):
            continue
        if ASSERT[language].search(rest.lstrip(" ;&)")):
            found.append(i)
            continue
        for k in ordered[pos + 1:pos + 13]:
            line = lines[k]
            if not line.strip():
                continue
            if SCOPE_END.search(line) or WAIT[language].search(line):
                break
            if ASSERT[language].search(line):
                found.append(i)
                break
    return found


HOST_TOOLS = r"(?:/usr/bin/top|top|vm_stat|memory_pressure|uptime)"
HOST = {
    "javascript": re.compile(r"\bos\.loadavg\s*\(|\b(?:exec|execSync|execFile|execFileSync|spawn|spawnSync)\s*\(\s*['\"`]" + HOST_TOOLS + r"\b"),
    "python": re.compile(r"\bos\.getloadavg\s*\(|\b(?:run|check_output|check_call|call|Popen)\s*\(\s*\[?\s*['\"]" + HOST_TOOLS + r"\b"),
    "shell": re.compile(r"(?:^|[;&|(]|\$\()\s*" + HOST_TOOLS + r"(?:\s|$|\))"),
    "rust": re.compile(r"\bgetloadavg\s*\(|Command::new\(\s*\"" + HOST_TOOLS + r"\""),
}
MOCKED = re.compile(r"mock|fake|stub|fixture", re.I)


def host_sample(lines, code, language):
    return [i for i in code if HOST[language].search(lines[i]) and not MOCKED.search(lines[i])]


ENV_SET = re.compile(r"\benv::(?:set_var|remove_var)\s*\(")
ENV_GUARD = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?static\s+\w+\s*:\s*[^=]*\b(?:Mutex|RwLock)\b|#\[serial")


def env_mutation(lines, code, language):
    if language != "rust" or any(ENV_GUARD.search(line) for line in lines):
        return []
    return [i for i in code if ENV_SET.search(lines[i])]


SCANS = (("wall-clock-verdict", wall_clock), ("short-deadline", short_deadline),
         ("sleep-then-assert", sleep_then_assert), ("host-sample", host_sample),
         ("env-mutation-unguarded", env_mutation))


def declared(lines, i):
    """`load-bound: <reason>` on the line or the line above; the reason must say something."""
    for k in (i, i - 1):
        if k < 0:
            continue
        m = DECLARED.search(lines[k])
        if m and len(re.sub(r"[\s*/#-]+", "", m[1])) >= 10:
            return True
    return False


def normalized(line):
    return " ".join(line.split())


def site_hash(line):
    return hashlib.sha256(normalized(line).encode()).hexdigest()[:16]


def scan(path, text, language):
    """[(rule, 1-based line, line text)] for undeclared sites."""
    lines = text.splitlines()
    if language == "shell":
        from shell_source import statements
        visible = statements(text).splitlines()
        visible += [""] * (len(lines) - len(visible))
    else:
        visible = lines
    if language == "rust":
        scope = rust_test_lines(path, lines)
    else:
        scope = set(range(len(lines)))
    code = {i for i in scope if i < len(visible) and visible[i].strip() and not comment(visible[i], language)}
    findings = []
    for rule, fn in SCANS:
        for i in fn(visible, code, language):
            if not declared(lines, i):
                source = assertion_text(lines, i, language) if rule == "wall-clock-verdict" else lines[i]
                findings.append((rule, i + 1, source))
    return sorted(findings, key=lambda f: (f[1], f[0]))


def collect(root, paths):
    """Undeclared sites in the given repository-relative paths that are test code."""
    sites = []
    for path, language in sorted(targets(paths).items()):
        file = root / path
        if not file.is_file():
            continue
        for rule, line, text in scan(path, file.read_text(errors="surrogateescape"), language):
            sites.append(dict(path=path, rule=rule, line=line, hash=site_hash(text), text=normalized(text)[:200]))
    return sites


# ---------------------------------------------------------------------------------------
# The site baseline
# ---------------------------------------------------------------------------------------

def tally(sites):
    table = {}
    for s in sites:
        entry = table.setdefault(s["path"], {}).setdefault(s["rule"], {}).setdefault(
            s["hash"], {"count": 0, "text": s["text"]})
        entry["count"] += 1
    return table


def record(sites):
    return dict(schema=1, limitation=NOTE, rules=RULES, sites=tally(sites))


def validate(baseline):
    if not isinstance(baseline, dict) or baseline.get("schema") != 1 or baseline.get("limitation") != NOTE:
        raise Refusal("invalid load-rule baseline schema")
    if not isinstance(baseline.get("rules"), dict) or not isinstance(baseline.get("sites"), dict):
        raise Refusal("missing load-rule baseline rules or sites")
    for path, rules in baseline["sites"].items():
        for rule, hashes in rules.items():
            for digest, entry in hashes.items():
                if type(entry.get("count")) is not int or entry["count"] < 1:
                    raise Refusal(f"invalid load-rule baseline count: {path} {rule} {digest}")


def allowed(baseline, path, rule, digest):
    return baseline["sites"].get(path, {}).get(rule, {}).get(digest, {}).get("count", 0)


def check(sites, baseline, paths=None):
    """Refuse every site beyond the baseline. `paths` limits the comparison to those files
    (the commit check scans only the files it changed; sites are per file, so that is exact)."""
    validate(baseline)
    if baseline["rules"] != RULES:
        raise Refusal("load-rule set does not match the committed baseline")
    new = []
    seen = {}
    for s in sites:
        key = (s["path"], s["rule"], s["hash"])
        seen[key] = seen.get(key, 0) + 1
        if seen[key] > allowed(baseline, *key):
            new.append(s)
    if new:
        shown = "\n".join(f"  {s['path']}:{s['line']}  {s['rule']}  {s['text'][:120]}" for s in new[:25])
        more = f"\n  ... and {len(new) - 25} more" if len(new) > 25 else ""
        raise Refusal(
            f"load-rule growth: {len(new)} new load-sensitive site(s) in test code:\n{shown}{more}\n"
            "  A test may bound time only to catch a hang, never to decide a verdict "
            "(docs/verification/2026-09-29-load-sensitive-checks-audit.md, section 5).\n"
            "  Wait for the fact instead of the clock, or declare the site with "
            "`load-bound: <why this cannot depend on host load>` on the line or the line above.")


def compare(candidate, trusted):
    """A branch cannot add a site to the baseline or drop a rule; only paying down is allowed."""
    validate(candidate)
    validate(trusted)
    for rule, kind in trusted["rules"].items():
        if candidate["rules"].get(rule) != kind:
            raise Refusal(f"removed or weakened load rule: {rule}")
    for path, rules in candidate["sites"].items():
        for rule, hashes in rules.items():
            for digest, entry in hashes.items():
                if entry["count"] > allowed(trusted, path, rule, digest):
                    raise Refusal(f"load-rule baseline adds a site not on integration: {path} {rule} {entry['text'][:80]}")


def lower(sites, baseline):
    """Keep only sites still present, at no more than their present count."""
    check(sites, baseline)
    present = tally(sites)
    kept = {}
    for path, rules in baseline["sites"].items():
        for rule, hashes in rules.items():
            for digest, entry in hashes.items():
                now = present.get(path, {}).get(rule, {}).get(digest, {}).get("count", 0)
                if now:
                    kept.setdefault(path, {}).setdefault(rule, {})[digest] = dict(entry, count=min(now, entry["count"]))
    return dict(baseline, sites=kept)


def dump(baseline):
    return json.dumps(baseline, indent=2, sort_keys=True) + "\n"


def check_engine(root, paths, trusted_ref="refs/heads/main"):
    """Ratchet engine tests against committed integration source under these same rules.

    Existing engine debt is reported, not silently inserted into the app baseline. New
    sites (including duplicates) refuse. Fixing one site cannot buy room for another.
    """
    import subprocess
    before = []
    engine = [p for p in paths if p.startswith("richos/engine/") and language_of(p)]
    for path in engine:
        old = subprocess.run(["git", "show", f"{trusted_ref}:{path}"], cwd=root,
                             capture_output=True, text=True)
        if old.returncode:
            # Missing paths are new tests; a missing trusted ref is not an empty baseline.
            subprocess.run(["git", "rev-parse", "--verify", trusted_ref], cwd=root,
                           check=True, capture_output=True)
            continue
        for rule, line, text in scan(path, old.stdout, language_of(path)):
            before.append(dict(path=path, rule=rule, line=line, hash=site_hash(text), text=normalized(text)))
    sites = collect(root, engine)
    check(sites, record(before))
    return sites


if __name__ == "__main__":
    import argparse
    import subprocess
    parser = argparse.ArgumentParser(description="Refuse new load-sensitive sites in engine tests")
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--paths", nargs="+")
    parser.add_argument("--trusted-ref", default="refs/heads/main")
    args = parser.parse_args()
    paths = args.paths or subprocess.run(["git", "diff", "--cached", "--name-only", "--no-renames"],
                           cwd=args.root, capture_output=True, text=True, check=True).stdout.splitlines()
    try:
        check_engine(args.root, paths, args.trusted_ref)
    except (Refusal, subprocess.CalledProcessError) as exc:
        raise SystemExit(str(exc))
