#!/usr/bin/env python3
"""gui-boot-launch-lines.py: every `[richos] ...` line the shell can print has a home in gui-boot.test.sh.

WHY. gui-boot.test.sh's B2 holds every line of a healthy Finder-condition boot to a rule (resolved,
routine or declared gap) and fails on any line no rule accounts for. B2 needs the test VM, so it ran
only in the nightly: nightly 78's boot proof failed on `[richos] Codex watch: watching 2 channel(s)
(process 17902)`, a launch line landed with no rule, at the cost of a failed proof and a retake.
This reads the same rules WITHOUT a boot, in a second, so the land that adds a line learns it.

WHAT IT CHECKS. Every `eprintln!("[richos] ...")` literal in src-tauri/src/main.rs outside its test
module is a template. Each template must be exactly one of:
  - ACCOUNTED: some rendering of it (its `{}` holes filled with a number, a word, a path or a dash)
    matches a `resolved`, `routine` or `gap` rule of gui-boot.test.sh, with the same `grep -E` its
    own accounting uses; or it matches a line the file declares `refused`;
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
import itertools
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_MAIN = os.path.join(HERE, "..", "src-tauri", "src", "main.rs")
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


def templates(main_text):
    """[(line number, template)] for each `[richos]` eprintln! outside every test item."""
    spans = code_map(main_text)
    excluded = test_ranges(main_text, spans)
    found = []
    for match in re.finditer(r'eprintln!\(\s*"((?:[^"\\]|\\.)*)"', main_text, re.S):
        if in_spans(match.start(), spans) or any(b <= match.start() < e for (b, e) in excluded):
            continue
        literal = unescape(match.group(1))
        if literal.startswith("[richos]"):
            found.append((main_text.count("\n", 0, match.start()) + 1, literal))
    return found


def normal(template):
    """The stable name of a template: every hole is `{}`, doubled braces are single."""
    return PLACEHOLDER.sub("\x00", template).replace("{{", "{").replace("}}", "}").replace("\x00", "{}")


def has_own_words(template):
    return bool(re.search(r"[A-Za-z]", re.sub(r"^\[richos\]", "", normal(template)).replace("{}", "")))


def renderings(template):
    """Candidate boot lines for a template: its holes filled in every combination of a few shapes."""
    parts = normal(template).split("{}")
    holes = len(parts) - 1
    if holes == 0:
        return [parts[0]]
    if len(FILLS) ** holes > len(FILLS) ** MAX_HOLES_PRODUCT:
        combos = [(fill,) * holes for fill in FILLS]
    else:
        combos = itertools.product(FILLS, repeat=holes)
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


def check(main_path, rules_path, list_path):
    """Findings as printable strings, plus (templates checked, templates skipped for having no words)."""
    found = templates(read(main_path))
    first_line = {}
    for number, template in found:
        first_line.setdefault(normal(template), number)
    names = sorted(first_line)
    judged = [n for n in names if has_own_words(n)]
    skipped = len(names) - len(judged)
    patterns, refused = load_rules(rules_path)
    by_rule = accounted_by_rule({n: renderings(n) for n in judged}, patterns)
    by_refusal = {n for n in judged if any(template_regex(n).match(line) for line in refused)}
    by_prefix = {n for n in judged if covered_by_prefix(n, patterns)}
    accounted = by_rule | by_refusal | by_prefix
    listed = read_list(list_path)
    findings = []
    for name in judged:
        if name not in accounted and name not in listed:
            findings.append(
                "UNCLASSIFIED  %s:%d\n    %s\n"
                "    No resolved/routine/gap rule in gui-boot.test.sh accounts for it, no `refused` line is it, and\n"
                "    it is not listed in gui-boot-launch-lines.unaccounted. If a healthy boot prints it, add a rule to\n"
                "    gui-boot.test.sh (B2 would fail on it at the next nightly otherwise). If a healthy boot never\n"
                "    does (an error, a runtime event), append the line above, exactly, to the list."
                % (os.path.basename(main_path), first_line[name], name))
    for name, number in sorted(listed.items(), key=lambda kv: kv[1]):
        if name not in first_line:
            findings.append("STALE  %s:%d\n    %s\n    main.rs no longer prints it; delete it from the list." % (os.path.basename(list_path), number, name))
        elif name in accounted:
            findings.append("COVERED  %s:%d\n    %s\n    A rule in gui-boot.test.sh accounts for it now; delete it from the list." % (os.path.basename(list_path), number, name))
    return findings, (len(judged), skipped)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--main", default=DEFAULT_MAIN)
    parser.add_argument("--rules", default=DEFAULT_RULES)
    parser.add_argument("--list", default=DEFAULT_LIST)
    parser.add_argument("--print-unclassified", action="store_true",
                        help="print the templates no rule accounts for, in the list's format (to start or refresh the list)")
    args = parser.parse_args(argv)
    try:
        if args.print_unclassified:
            names = sorted({normal(t) for _n, t in templates(read(args.main))})
            judged = [n for n in names if has_own_words(n)]
            patterns, refused = load_rules(args.rules)
            by_rule = accounted_by_rule({n: renderings(n) for n in judged}, patterns)
            for name in judged:
                if (name not in by_rule and not covered_by_prefix(name, patterns)
                        and not any(template_regex(name).match(line) for line in refused)):
                    print(name)
            return 0
        findings, (judged, skipped) = check(args.main, args.rules, args.list)
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
