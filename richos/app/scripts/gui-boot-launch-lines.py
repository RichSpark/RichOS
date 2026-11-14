#!/usr/bin/env python3
"""gui-boot-launch-lines.py: every `[richos] ...` line the shell can print has a home in gui-boot.test.sh.

WHY. gui-boot.test.sh's B2 holds every line of a healthy Finder-condition boot to a rule (resolved,
routine or declared gap) and fails on any line no rule accounts for. B2 needs the test VM, so it ran
only in the nightly: nightly 78's boot proof failed on `[richos] Codex watch: watching 2 channel(s)
(process 17902)`, a launch line landed with no rule, at the cost of a failed proof and a retake.
This reads the same rules WITHOUT a boot, in a second, so the land that adds a line learns it.

WHAT IT CHECKS. Every `eprintln!("[richos] ...")` literal in the app's own Rust source outside its
test code is a template: every `.rs` file under src-tauri/src and under each crates/<crate>/src,
except src/bin (other programs) and test code (a `#[cfg(test)]` item, or a test module declared
`#[cfg(test)] mod x;` and kept in its own file). It once read main.rs alone, and a nightly's boot
proof then failed B2 on `[richos] SwearMeter historical sources: read (0 ms)`, printed from
swearmeter_host.rs, which the check could not see. Each template must be exactly one of:
  - ACCOUNTED: some rendering of it (its `{}` holes filled with a number, a word, a path, a dash, or
    a string literal passed among the eprintln!'s own arguments, the way
    `if partial { "partial" } else { "read" }` fills a hole) matches a `resolved`, `routine` or
    `gap` rule of gui-boot.test.sh, with the same `grep -E` its own accounting uses; or it matches
    a line the file declares `refused`;
  - NOT ON A HEALTHY BOOT, listed in gui-boot-launch-lines.unaccounted: a failure, a runtime event or
    a sibling of an accounted line. If a healthy boot ever printed one, B2 would still fail on it,
    which is how those lines are meant to stay.
A line that is neither is the defect: the exact line to add is printed. A listed line that stopped
existing, or that a rule now accounts for, is also an error, so the list cannot drift into a pile of
dead entries. A template with no words of its own (`[richos] {}`) cannot be judged here and is
counted, not checked; B2 still sees what it prints.

Exit 0 when clean, 1 on a finding, 2 when an input cannot be read (never a quiet pass over nothing).
"""
import argparse
import glob
import itertools
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_APP = os.path.normpath(os.path.join(HERE, ".."))
DEFAULT_RULES = os.path.join(HERE, "gui-boot.test.sh")
DEFAULT_LIST = os.path.join(HERE, "gui-boot-launch-lines.unaccounted")

PLACEHOLDER = re.compile(r"\{[^{}]*\}")
FILLS = ("1", "x", "/x", "-")
MAX_HOLES_PRODUCT = 5


class InputError(Exception):
    pass


def read(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except OSError as error:
        raise InputError("cannot read %s: %s" % (path, error))


def unescape(literal):
    """The Rust string literal's text: line continuations, quotes, backslashes, newlines."""
    literal = re.sub(r"\\\r?\n\s*", "", literal)
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), literal)


# What is not code: comments, strings (plain, byte and raw) and character literals. A lifetime
# (`'a`) is not a character literal, which is why the literal must close on the next quote.
NOT_CODE = re.compile(r"""//[^\n]*|/\*.*?\*/|b?r(?P<h>\#*)".*?"(?P=h)|b?"(?:[^"\\]|\\.)*"|'(?:\\.[^']*|[^\\'])'""", re.S)
CFG_TEST = re.compile(r"#\[cfg\(test\)\]")


def code_map(text):
    """(spans that are not code, as sorted (start, end)); everything else in `text` is code."""
    return [(m.start(), m.end()) for m in NOT_CODE.finditer(text)]


def in_spans(position, spans):
    import bisect
    index = bisect.bisect_right(spans, (position, float("inf"))) - 1
    return index >= 0 and spans[index][0] <= position < spans[index][1]


def test_ranges(text, spans):
    """[(start, end)] of every item behind a `#[cfg(test)]` attribute: its braces, or up to its `;`."""
    ranges = []
    skip_to = dict(spans)
    for attribute in CFG_TEST.finditer(text):
        if in_spans(attribute.start(), spans):
            continue
        depth, i, end = 0, attribute.end(), None
        while i < len(text):
            if i in skip_to:
                i = skip_to[i]
                continue
            ch = text[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth <= 0:
                    end = i + 1
                    break
            elif ch == ";" and depth == 0:
                end = i + 1
                break
            i += 1
        ranges.append((attribute.start(), end if end is not None else len(text)))
    return ranges


STRING = re.compile(r'"((?:[^"\\]|\\.)*)"', re.S)
TEST_MODULE = re.compile(r"#\[cfg\(test\)\]\s*((?:#\[[^\]]*\]\s*)*)(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")
PATH_ATTRIBUTE = re.compile(r'#\[path\s*=\s*"([^"]+)"\s*\]')


def call_arguments(text, start, spans):
    """The plain string literals among a call's arguments, read from `start` (just past its format
    string) to the parenthesis that closes the call."""
    skip_to = dict(spans)
    depth, i, literals = 0, start, []
    while i < len(text):
        if i in skip_to:
            string = STRING.match(text, i)
            if string:
                literals.append(unescape(string.group(1)).replace("\n", " "))
            i = skip_to[i]
            continue
        ch = text[i]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            if depth == 0:
                break
            depth -= 1
        i += 1
    return literals


def templates(main_text):
    """[(line number, template, string literals among its arguments)] for each `[richos]` eprintln!
    outside every test item."""
    spans = code_map(main_text)
    excluded = test_ranges(main_text, spans)
    found = []
    for match in re.finditer(r'eprintln!\(\s*"((?:[^"\\]|\\.)*)"', main_text, re.S):
        if in_spans(match.start(), spans) or any(b <= match.start() < e for (b, e) in excluded):
            continue
        literal = unescape(match.group(1))
        if literal.startswith("[richos]"):
            found.append((main_text.count("\n", 0, match.start()) + 1, literal,
                          call_arguments(main_text, match.end(), spans)))
    return found


def module_dir(path):
    """Where the files of the modules `path` declares live (Rust's rule for a non-`#[path]` `mod x;`)."""
    base = os.path.basename(path)
    return os.path.dirname(path) if base in ("main.rs", "lib.rs", "mod.rs") else os.path.splitext(path)[0]


def test_module_paths(path, text):
    """The file and directory of each test module `path` declares out of line (`#[cfg(test)] mod x;`)."""
    spans = code_map(text)
    out = []
    for match in TEST_MODULE.finditer(text):
        if in_spans(match.start(), spans):
            continue
        attribute = PATH_ATTRIBUTE.search(match.group(1) or "")
        if attribute:
            out.append(os.path.normpath(os.path.join(os.path.dirname(path), attribute.group(1))))
        else:
            out += [os.path.join(module_dir(path), match.group(2) + ".rs"), os.path.join(module_dir(path), match.group(2))]
    return out


def source_files(app, main_override=None):
    """[(text, path relative to `app`)] for every app source file: each `.rs` under src-tauri/src and
    crates/*/src, without src/bin and without test modules kept in their own files. `main_override`
    is read in place of src-tauri/src/main.rs (the tests replay a changed main.rs that way)."""
    shell = os.path.join(app, "src-tauri", "src")
    if not os.path.isdir(shell):
        raise InputError("%s is not a directory: the app source this check reads is not there" % shell)
    real_main = os.path.join(shell, "main.rs")
    files = []
    for root in [shell] + sorted(glob.glob(os.path.join(app, "crates", "*", "src"))):
        for directory, subdirs, names in os.walk(root):
            subdirs.sort()
            if directory == root and "bin" in subdirs:
                subdirs.remove("bin")
            files += [os.path.join(directory, n) for n in sorted(names) if n.endswith(".rs")]
    texts = {path: read(main_override if main_override and path == real_main else path) for path in files}
    tests = set()
    for path, text in texts.items():
        tests.update(test_module_paths(path, text))
    return [(texts[path], os.path.relpath(path, app)) for path in files
            if path not in tests and not any(path.startswith(t + os.sep) for t in tests)]


def all_templates(app, main_override=None):
    """{template's stable name: ("file:line" where it is first printed, string literals its calls pass)}."""
    found = {}
    for text, name in source_files(app, main_override):
        for number, template, literals in templates(text):
            _where, fills = found.setdefault(normal(template), ("%s:%d" % (name, number), []))
            fills.extend(literal for literal in literals if literal not in fills)
    return found


def normal(template):
    """The stable name of a template: every hole is `{}`, doubled braces are single."""
    return PLACEHOLDER.sub("\x00", template).replace("{{", "{").replace("}}", "}").replace("\x00", "{}")


def has_own_words(template):
    return bool(re.search(r"[A-Za-z]", re.sub(r"^\[richos\]", "", normal(template)).replace("{}", "")))


def renderings(template, chosen=()):
    """Candidate boot lines for a template: its holes filled in every combination of a few shapes and
    of the words its own call passes (`chosen`), which are the words that call can print there."""
    parts = normal(template).split("{}")
    holes = len(parts) - 1
    if holes == 0:
        return [parts[0]]
    fills = FILLS + tuple(word for word in chosen if word not in FILLS)
    if len(fills) ** holes > len(FILLS) ** MAX_HOLES_PRODUCT:
        combos = [(fill,) * holes for fill in fills]
    else:
        combos = itertools.product(fills, repeat=holes)
    out = []
    for combo in combos:
        text = parts[0]
        for fill, tail in zip(combo, parts[1:]):
            text += fill + tail
        out.append(text.replace("\n", " "))
    return out


def template_regex(template):
    parts = [re.escape(p) for p in normal(template).split("{}")]
    return re.compile(".*".join(parts) + r"\Z", re.S)


def load_rules(rules_path):
    """(patterns, refused lines) from gui-boot.test.sh: its declarations, run by bash, not parsed."""
    text = read(rules_path)
    start = re.search(r"^DECL_ERRORS=0$", text, re.M)
    declare = re.search(r"^declare_rules\(\) \{$", text, re.M)
    if not start or not declare:
        raise InputError("%s no longer has the shape this check reads (DECL_ERRORS=0 ... declare_rules() {...})" % rules_path)
    end = re.search(r"^\}$", text[declare.end():], re.M)
    if not end:
        raise InputError("%s: declare_rules() has no closing brace at the start of a line" % rules_path)
    chunk = text[start.start():declare.end() + end.end()]
    program = chunk + r'''
declare_rules
for i in "${!RESOLVED_RE[@]}"; do printf 'P\t%s\n' "${RESOLVED_RE[$i]}"; done
for i in "${!ROUTINE_RE[@]}";  do printf 'P\t%s\n' "${ROUTINE_RE[$i]}"; done
for i in "${!GAP_RE[@]}";      do printf 'P\t%s\n' "${GAP_RE[$i]}"; done
for i in "${!REFUSED_LINE[@]}"; do printf 'R\t%s\n' "${REFUSED_LINE[$i]}"; done
exit "$DECL_ERRORS"
'''
    run = subprocess.run(["bash", "-c", program], capture_output=True, text=True)
    if run.returncode != 0:
        raise InputError("%s: its rule declarations did not run cleanly: %s" % (rules_path, run.stderr.strip()[:300]))
    patterns, refused = [], []
    for row in run.stdout.splitlines():
        kind, _, value = row.partition("\t")
        (patterns if kind == "P" else refused).append(value)
    if not patterns:
        raise InputError("%s declares no rules at all" % rules_path)
    return patterns, refused


def literal_prefix(pattern):
    """The text an anchored ERE must start with, as far as it is plain characters; '' if unanchored."""
    if not pattern.startswith("^"):
        return ""
    body, out, i = pattern[1:], "", 0
    while i < len(body):
        char = body[i]
        if char == "\\" and i + 1 < len(body):
            if body[i + 1].isalnum():
                break
            out += body[i + 1]
            i += 2
        elif char in ".[(*+?{|$)":
            break
        else:
            out += char
            i += 1
        if i < len(body) and body[i] in "*?{":
            out = out[:-1]           # the last plain character is optional
            break
    return out


def covered_by_prefix(name, patterns):
    """A template that is mostly a hole (`engine directory: {}`) is accounted for when a rule begins
    with the same words: the hole is a whole sentence another function writes, so the sentence is
    what the rule matches at boot (B2 holds that half). Needs real words before the first hole."""
    head = normal(name).split("{}")[0]
    if "{}" not in normal(name) or len(head.replace("[richos]", "").strip()) < 8:
        return False
    return any(len(rule) >= len(head) and rule.startswith(head)
               for rule in (literal_prefix(p) for p in patterns) if rule)


def accounted_by_rule(candidates_by_template, patterns):
    """The templates some rendering of which a rule matches, by the same grep -E gui_account uses."""
    flat, owner = [], []
    for name, candidates in candidates_by_template.items():
        for candidate in candidates:
            flat.append(candidate)
            owner.append(name)
    with tempfile.TemporaryDirectory(prefix="gui-boot-lines.") as scratch:
        pattern_file = os.path.join(scratch, "patterns")
        candidate_file = os.path.join(scratch, "candidates")
        with open(pattern_file, "w", encoding="utf-8") as f:
            f.write("\n".join(patterns) + "\n")
        with open(candidate_file, "w", encoding="utf-8") as f:
            f.write("\n".join(flat) + "\n")
        run = subprocess.run(["grep", "-E", "-n", "-f", pattern_file, candidate_file], capture_output=True, text=True)
    if run.returncode not in (0, 1):
        raise InputError("grep -E could not apply the rules: %s" % run.stderr.strip()[:300])
    hit = set()
    for row in run.stdout.splitlines():
        number = row.split(":", 1)[0]
        if number.isdigit():
            hit.add(owner[int(number) - 1])
    return hit


def read_list(path):
    out = {}
    for number, row in enumerate(read(path).splitlines(), 1):
        if row.strip() and not row.startswith("#"):
            out[row] = number
    return out


def classify(app, rules_path, main_override=None):
    """(templates found, the ones with words of their own, the ones a rule, refusal or prefix accounts for, skipped)."""
    found = all_templates(app, main_override)
    names = sorted(found)
    judged = [n for n in names if has_own_words(n)]
    patterns, refused = load_rules(rules_path)
    by_rule = accounted_by_rule({n: renderings(n, found[n][1]) for n in judged}, patterns)
    by_refusal = {n for n in judged if any(template_regex(n).match(line) for line in refused)}
    by_prefix = {n for n in judged if covered_by_prefix(n, patterns)}
    return found, judged, by_rule | by_refusal | by_prefix, len(names) - len(judged)


def check(app, rules_path, list_path, main_override=None):
    """Findings as printable strings, plus (templates checked, templates skipped for having no words)."""
    found, judged, accounted, skipped = classify(app, rules_path, main_override)
    listed = read_list(list_path)
    findings = []
    for name in judged:
        if name not in accounted and name not in listed:
            findings.append(
                "UNCLASSIFIED  %s\n    %s\n"
                "    No resolved/routine/gap rule in gui-boot.test.sh accounts for it, no `refused` line is it, and\n"
                "    it is not listed in gui-boot-launch-lines.unaccounted. If a healthy boot prints it, add a rule to\n"
                "    gui-boot.test.sh (B2 would fail on it at the next nightly otherwise). If a healthy boot never\n"
                "    does (an error, a runtime event), append the line above, exactly, to the list."
                % (found[name][0], name))
    for name, number in sorted(listed.items(), key=lambda kv: kv[1]):
        if name not in found:
            findings.append("STALE  %s:%d\n    %s\n    No app source file prints it any more; delete it from the list." % (os.path.basename(list_path), number, name))
        elif name in accounted:
            findings.append("COVERED  %s:%d\n    %s\n    A rule in gui-boot.test.sh accounts for it now; delete it from the list." % (os.path.basename(list_path), number, name))
    return findings, (len(judged), skipped)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--app", default=DEFAULT_APP, help="the app directory whose src-tauri/src and crates/*/src are read")
    parser.add_argument("--main", default=None, help="a file read in place of the app's src-tauri/src/main.rs")
    parser.add_argument("--rules", default=DEFAULT_RULES)
    parser.add_argument("--list", default=DEFAULT_LIST)
    parser.add_argument("--print-unclassified", action="store_true",
                        help="print the templates no rule accounts for, in the list's format (to start or refresh the list)")
    args = parser.parse_args(argv)
    try:
        if args.print_unclassified:
            _found, judged, accounted, _skipped = classify(args.app, args.rules, args.main)
            for name in judged:
                if name not in accounted:
                    print(name)
            return 0
        findings, (judged, skipped) = check(args.app, args.rules, args.list, args.main)
    except InputError as error:
        print("gui-boot-launch-lines: %s" % error, file=sys.stderr)
        return 2
    for finding in findings:
        print(finding)
    print("gui-boot-launch-lines: %d launch line(s) checked, %d with no words of their own left to B2; %d finding(s)"
          % (judged, skipped, len(findings)))
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
