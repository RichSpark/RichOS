#!/usr/bin/env python3
"""dialect-table.py - generate the British-to-American spelling table from pinned VarCon data.

    dialect-table.py              regenerate dialect-en-US.generated.dict in place
    dialect-table.py --check      exit 1 when the committed table differs from a fresh generation
    dialect-table.py --stdout     print the generated table instead of writing it
    dialect-table.py --report     print how the table was assembled, with every count
    dialect-table.py --deny-lists one-time check: run the engine's named-person and publication
                                  write guards over the generated table (prints verdicts only)

Exit: 0 done; 1 a check failed or an input was refused; 2 usage error or an unreadable or
changed pinned input.

WHY A GENERATED TABLE (step 0 of the American spelling auto-fix, 2026-09-27). The CEO's goal:
"a guard that is as fast as possible and eliminates the need for slow inference as much as
possible". A fixed British-to-American lookup is milliseconds; the work is deciding which pairs
are safe to apply without looking at context, and VarCon already records that. The method is
Clark's, measured in richos-hq docs/research/2026-09-27-american-spelling-autofix.md ("How the
safe and ambiguous sets are produced"), and this file reproduces its varcon_split.py rules
exactly, then applies the RichOS lists:

  1. CANDIDATES. Every form VarCon tags B (British "ise" spelling) at any variant level except
     x (improper) on a line that also has a preferred American form (tag A, no variant mark).
     Possessives and multi-word forms are dropped; the base word covers them.
  2. AMBIGUOUS, and never emitted: the form is a PREFERRED American spelling in some sense
     (tag A or A. anywhere in VarCon), or it maps to more than one American form. Those words
     are correct American where they stand, so leaving them is the right default.
  3. VERIFIED ONLY, SCOWL LEVEL 70 OR BELOW. A form that appears in any unverified cluster is
     out: the unverified clusters hold nonsense pairs that an automatic fix must never apply.
     Forms American dictionaries merely tolerate as variants (tags Av, AV, A-) are IN, by the
     one product rule "always the preferred American form".
  4. LOWERCASE KEYS ONLY. Capitalized VarCon entries are names; a reader re-cases on its own.
  5. THE RICHOS LISTS, beside this file and in ../:
       dialect-en-US.leave.tsv       removals (the Harper review list's leave column, names,
                                     editorial calls); each must remove something
       dialect-en-US.overrides.dict  additions VarCon's verified data lacks
       ../dialect-en-US.dict         the hand-kept dictionary dialect-guard.sh reads today;
                                     every line of it is carried, with its per-file-type options

Every disagreement between those lists and VarCon is REFUSED rather than resolved here: a
generator that quietly picked a winner would be the second vocabulary the engine forbids.

THE OUTPUT FORMAT is the one dialect-guard.sh already parses, so both readers can use it:

    <lowercase British form><TAB><American form>[<TAB>ext-exempt=css,...]

Comment lines start with '#'. The engine hook reads it with python3 at run time; the RichOS app
compiles it in with include_str! at build time (the precedent is richos-voice's model pins).
The file is the product; this script only makes it again, identically, from pinned bytes.

This file carries no word the table would change, so no dialect exemption is needed for it;
dialect-table.test.py proves that on every run.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
VARCON = HERE / "third_party" / "varcon" / "varcon.txt"
VARCON_SHA256 = "c1e234817b526a809fa745d742fec84db88b65c429e22faf8a556b3d5a224085"
VARCON_REVISION = "VarCon 2020.12.07, as vendored by crate-ci/typos at dd3e1018f8a825a98be71048c21a9b0495aff703"
DICT = HERE.parent / "dialect-en-US.dict"
OVERRIDES = HERE / "dialect-en-US.overrides.dict"
LEAVE = HERE / "dialect-en-US.leave.tsv"
TABLE = HERE / "dialect-en-US.generated.dict"
MAX_LEVEL = 70

WORD_RE = re.compile(r"^[a-z]+$")
HEX_RE = re.compile(r"^[a-f0-9]+$")
TAG_RE = re.compile(r"^([ABZCD_])([.vV\-x]?)$")
HEAD_RE = re.compile(r"#\s*(\S+)\s*(<verified>)?\s*\(level (\d+)\)")


class Refused(Exception):
    """An input the generator will not resolve on its own."""


# ----------------------------------------------------------------------------------------------
# VarCon
# ----------------------------------------------------------------------------------------------
def read_varcon(path=VARCON, expected_sha256=VARCON_SHA256):
    raw = path.read_bytes()
    got = hashlib.sha256(raw).hexdigest()
    if got != expected_sha256:
        raise Refused("%s has sha256 %s, but the pin is %s. The pinned input changed; re-vendor "
                      "deliberately and update VARCON_SHA256, the registry row and the notices "
                      "together." % (path, got, expected_sha256))
    # VarCon is Latin-1 (seven bytes outside ASCII, all in forms no British candidate uses).
    return raw.decode("latin-1")


def parse_clusters(text):
    clusters, cur = [], None
    for line in text.splitlines():
        if not line.strip():
            continue
        if line.startswith("#"):
            m = HEAD_RE.match(line)
            if m:
                cur = {"head": m.group(1), "verified": bool(m.group(2)),
                       "level": int(m.group(3)), "lines": []}
                clusters.append(cur)
            continue
        if cur is None:
            continue
        body = line.split(" | ")[0]
        body = re.sub(r"\s*\{[^}]*\}", "", body)
        variants = []
        for part in body.split(" / "):
            if ": " not in part:
                continue
            tags, word = part.split(": ", 1)
            word = re.sub(r"\s*<[^>]*>$", "", word.strip())
            tset = []
            for t in tags.split():
                m = TAG_RE.match(t)
                if m:
                    tset.append((m.group(1), m.group(2)))
            variants.append((word, tset))
        cur["lines"].append(variants)
    return clusters


def split_varcon(clusters, max_level=MAX_LEVEL):
    """Return (verified, any_tier, ambiguous): dicts of British form -> American form (the first
    two) and British form -> sorted American forms (the third). Rules in the module docstring."""
    american_pref = set()
    pref_american = defaultdict(set)
    meta = {}
    for c in clusters:
        for variants in c["lines"]:
            pref_a = [w for w, ts in variants if ("A", "") in ts]
            for w, ts in variants:
                if any(cat == "A" and lvl in ("", ".") for cat, lvl in ts):
                    american_pref.add(w)
                if any(cat == "B" and lvl != "x" for cat, lvl in ts) and pref_a:
                    for a in pref_a:
                        if a != w:
                            pref_american[w].add(a)
                    m = meta.setdefault(w, {"verified": True, "level": 0})
                    m["verified"] = m["verified"] and c["verified"]
                    m["level"] = max(m["level"], c["level"])
    verified, any_tier, ambiguous = {}, {}, {}
    for w, targets in pref_american.items():
        if "'" in w or " " in w:
            continue
        if w in american_pref or len(targets) != 1:
            ambiguous[w] = sorted(targets)
            continue
        to = next(iter(targets))
        any_tier[w] = to
        if meta[w]["verified"] and meta[w]["level"] <= max_level and w == w.lower():
            verified[w] = to
    return verified, any_tier, ambiguous


# ----------------------------------------------------------------------------------------------
# The RichOS lists
# ----------------------------------------------------------------------------------------------
def read_pairs(path):
    """<form><TAB><target>[<TAB>options] -> {form: (target, options-string)}; refuses duplicates."""
    out = {}
    for n, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        parts = raw.split("\t")
        if len(parts) < 2 or not parts[0].strip() or not parts[1].strip():
            raise Refused("%s:%d is not <form><TAB><target>[<TAB>options]" % (path, n))
        form = parts[0].strip().lower()
        if form in out:
            raise Refused("%s:%d repeats '%s'" % (path, n, form))
        out[form] = (parts[1].strip(), parts[2].strip() if len(parts) > 2 else "")
    return out


def read_leave(path):
    out = {}
    for n, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        parts = raw.split("\t")
        if len(parts) != 2 or not parts[0].strip() or not parts[1].strip():
            raise Refused("%s:%d is not <form><TAB><why it stays>" % (path, n))
        form = parts[0].strip().lower()
        if form in out:
            raise Refused("%s:%d repeats '%s'" % (path, n, form))
        out[form] = parts[1].strip()
    return out


# ----------------------------------------------------------------------------------------------
# Assembly
# ----------------------------------------------------------------------------------------------
def assemble(verified, any_tier, ambiguous, hand, overrides, leave):
    """Return (table, stats). table: {form: (target, options)}. Refuses every disagreement."""
    problems = []
    for form, (target, _) in sorted(hand.items()):
        if form in ambiguous:
            problems.append("../dialect-en-US.dict carries '%s', which VarCon marks as a preferred "
                            "American spelling in some sense (%s)" % (form, ", ".join(ambiguous[form])))
        elif form in verified and verified[form] != target:
            problems.append("../dialect-en-US.dict maps '%s' to '%s'; VarCon's verified form is '%s'"
                            % (form, target, verified[form]))
    for form, (target, _) in sorted(overrides.items()):
        if form in hand:
            problems.append("override '%s' is already in ../dialect-en-US.dict; keep one source" % form)
        elif form in verified:
            problems.append("override '%s' is already in VarCon's verified table; it is not an "
                            "override" % form)
        elif form in ambiguous:
            problems.append("override '%s' is a preferred American spelling in some sense (%s)"
                            % (form, ", ".join(ambiguous[form])))
        elif form in any_tier and any_tier[form] != target:
            problems.append("override maps '%s' to '%s'; VarCon's (unverified) form is '%s'"
                            % (form, target, any_tier[form]))
    for form in sorted(leave):
        if form in hand or form in overrides:
            problems.append("'%s' is both left alone and added by a hand-kept list" % form)
        elif form not in verified:
            problems.append("leave entry '%s' removes nothing: the table would not carry it" % form)
    if problems:
        raise Refused("the RichOS lists disagree with VarCon or with each other:\n  - "
                      + "\n  - ".join(problems))

    table = {f: (t, "") for f, t in verified.items() if f not in leave}
    table.update(overrides)
    table.update(hand)

    for form, (target, _) in sorted(table.items()):
        if not WORD_RE.match(form) or not WORD_RE.match(target):
            problems.append("'%s' -> '%s' is not lowercase ASCII letters on both sides" % (form, target))
        if HEX_RE.match(form):
            problems.append("key '%s' is hex-only, so it could match part of a commit SHA" % form)
        if form == target:
            problems.append("'%s' maps to itself" % form)
        if target in table:
            problems.append("'%s' -> '%s', and '%s' is itself a key (a chain)" % (form, target, target))
    if problems:
        raise Refused("the assembled table fails its invariants:\n  - " + "\n  - ".join(problems))

    hand_same = sum(1 for f, (t, _) in hand.items() if verified.get(f) == t)
    stats = {
        "verified": len(verified), "ambiguous": len(ambiguous), "left": len(leave),
        "overrides": len(overrides), "hand": len(hand), "hand_same": hand_same,
        "hand_only": len(hand) - hand_same, "table": len(table),
    }
    return table, stats


NOTICES = """\
Copyright 2000-2019 by Kevin Atkinson

Permission to use, copy, modify, distribute and sell this array, the
associated software, and its documentation for any purpose is hereby
granted without fee, provided that the above copyright notice appears
in all copies and that both that copyright notice and this permission
notice appear in supporting documentation. Kevin Atkinson makes no
representations about the suitability of this array for any
purpose. It is provided "as is" without express or implied warranty.

Copyright 2016 by Benjamin Titze

Permission to use, copy, modify, distribute and sell this array, the
associated software, and its documentation for any purpose is hereby
granted without fee, provided that the above copyright notice appears
in all copies and that both that copyright notice and this permission
notice appear in supporting documentation. Benjamin Titze makes no
representations about the suitability of this array for any
purpose. It is provided "as is" without express or implied warranty.

Since the original words lists come from the Ispell distribution:

Copyright 1993, Geoff Kuenning, Granada Hills, CA
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.
3. All modifications to the source code must be clearly marked as
   such.  Binary redistributions based on modified source code
   must be clearly marked as modified versions in the documentation
   and/or other materials provided with the distribution.
(clause 4 removed with permission from Geoff Kuenning)
5. The name of Geoff Kuenning may not be used to endorse or promote
   products derived from this software without specific prior
   written permission.

THIS SOFTWARE IS PROVIDED BY GEOFF KUENNING AND CONTRIBUTORS ``AS IS'' AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED.  IN NO EVENT SHALL GEOFF KUENNING OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
SUCH DAMAGE.
"""


def render(table, stats):
    head = [
        "dialect-en-US.generated.dict - GENERATED. DO NOT EDIT BY HAND.",
        "",
        "Regenerate: python3 richos/engine/scripts/lib/dialect/dialect-table.py",
        "Verify:     python3 richos/engine/scripts/lib/dialect/dialect-table.py --check",
        "",
        "British-to-American spelling pairs, one per line, TAB-separated, in the format",
        "../dialect-en-US.dict uses: <form><TAB><American form>[<TAB>ext-exempt=...].",
        "Keys are lowercase ASCII; a reader matches case-insensitively and re-cases.",
        "",
        "THIS IS A MODIFIED VERSION OF VARCON. It is a generated subset of",
        VARCON_REVISION + ",",
        "input sha256 " + VARCON_SHA256 + ":",
        "only verified clusters at SCOWL level %d or below, only British forms with" % MAX_LEVEL,
        "exactly one preferred American form and no preferred-American use in any",
        "sense, reformatted to one pair per line, with RichOS additions and removals",
        "merged in from dialect-en-US.overrides.dict, dialect-en-US.leave.tsv and",
        "../dialect-en-US.dict. The unmodified data and its README are in",
        "third_party/varcon/; the full notices are also in",
        "docs/legal/THIRD-PARTY-NOTICES.md.",
        "",
        "%d pairs = %d from VarCon's verified clusters - %d left alone"
        % (stats["table"], stats["verified"], stats["left"]),
        "  + %d overrides + %d from ../dialect-en-US.dict not already in VarCon's set"
        % (stats["overrides"], stats["hand_only"]),
        "  (%d of that dictionary's %d lines match VarCon exactly; none disagree)."
        % (stats["hand_same"], stats["hand"]),
        "",
        "The copyright notices VarCon's README requires in all copies:",
        "",
    ] + NOTICES.rstrip("\n").split("\n")
    lines = [("# " + h).rstrip() for h in head]
    lines.append("")
    for form in sorted(table):
        target, opts = table[form]
        lines.append(form + "\t" + target + ("\t" + opts if opts else ""))
    return "\n".join(lines) + "\n"


def generate(varcon_path=VARCON, expected_sha256=VARCON_SHA256, dict_path=DICT,
             overrides_path=OVERRIDES, leave_path=LEAVE):
    clusters = parse_clusters(read_varcon(varcon_path, expected_sha256))
    verified, any_tier, ambiguous = split_varcon(clusters)
    table, stats = assemble(verified, any_tier, ambiguous, read_pairs(dict_path),
                            read_pairs(overrides_path), read_leave(leave_path))
    stats["clusters"] = len(clusters)
    stats["verified_clusters"] = sum(1 for c in clusters if c["verified"])
    return render(table, stats), table, stats


# ----------------------------------------------------------------------------------------------
# One-time deny-list check (plan check catch C16)
# ----------------------------------------------------------------------------------------------
def deny_lists():
    """Judge the generated table with the engine's own named-person predicate and publication
    write guard, twice (the whole file, then its American forms alone), and print verdicts only.
    Never prints a matched name or a private excerpt.

    An exit 0 from a guard is not a verdict: both also exit 0 when they stand down. So the
    named-person check calls the predicate's explicit modes (--doctor, --scan-text: OK/ABSENT,
    CLEAN/FOUND), and the publication check reads the guard's scanner result from its shell
    trace, the way richos/app/scripts/lint/dialect.py reads dialect-guard.sh. Exit 1 unless every
    verdict is an evaluated CLEAN."""
    engine = HERE.parents[2]
    lib = engine / "scripts" / "lib"
    content = TABLE.read_text(encoding="utf-8")
    targets = "\n".join(sorted({line.split("\t")[1] for line in content.splitlines()
                                if line and not line.startswith("#")})) + "\n"
    ok = True
    doctor = subprocess.run([sys.executable, str(lib / "named-persons.py"), "--doctor"],
                            text=True, capture_output=True, timeout=60)
    first = (doctor.stdout.splitlines() or ["?"])[0].split("\t")[0]
    print("named-person deny-list     %s" % ("present" if first == "OK" else first + " - NOT CHECKED"))
    ok = ok and first == "OK"
    for label, text in (("whole table", content), ("American forms only", targets)):
        r = subprocess.run([sys.executable, str(lib / "named-persons.py"), "--scan-text", label],
                           input=text, text=True, capture_output=True, timeout=120)
        verdict = (r.stdout.splitlines() or ["?"])[0].split("\t")[0]
        print("named-person predicate     %-20s %s" % (label, verdict))
        ok = ok and verdict == "CLEAN"
    for label, text in (("whole table", content), ("American forms only", targets)):
        payload = json.dumps({"tool_name": "Write", "cwd": str(engine),
                              "tool_input": {"file_path": str(TABLE), "content": text}})
        env = dict(os.environ, RICHOS_ENGINE_ROOT=str(engine), RICHOS_ENTITY_ROOT=str(engine),
                   PS4="+ ")
        r = subprocess.run(["bash", "-x", str(engine / "scripts" / "hooks" / "guard-publication-writes.sh")],
                           input=payload, text=True, capture_output=True, env=env, timeout=600)
        result = re.search(r"^\+ RESULT=['\"]?([A-Z]+)", r.stderr, re.M)
        sources = re.search(r"^\+ PB_SOURCES_JSON=['\"]?(\[.*?\])['\"]?$", r.stderr, re.M)
        n_sources = len(json.loads(sources.group(1))) if sources else 0
        verdict = result.group(1) if result else "NOT EVALUATED"
        print("publication write guard    %-20s exit %d, scanner %s against %d private source(s)"
              % (label, r.returncode, verdict, n_sources))
        ok = ok and r.returncode == 0 and verdict == "CLEAN" and n_sources > 0
    return 0 if ok else 1


def main(argv):
    mode = argv[0] if argv else "--write"
    if mode in ("-h", "--help"):
        print(__doc__)
        return 0
    if mode not in ("--write", "--check", "--stdout", "--report", "--deny-lists") or len(argv) > 1:
        sys.stderr.write("usage: dialect-table.py [--check|--stdout|--report|--deny-lists]\n")
        return 2
    if mode == "--deny-lists":
        return deny_lists()
    try:
        text, _, stats = generate()
    except Refused as exc:
        sys.stderr.write("dialect-table.py: REFUSED: %s\n" % exc)
        return 1
    except OSError as exc:
        sys.stderr.write("dialect-table.py: cannot read an input: %s\n" % exc)
        return 2
    if mode == "--stdout":
        sys.stdout.write(text)
    elif mode == "--report":
        for k in ("clusters", "verified_clusters", "verified", "ambiguous", "left", "overrides",
                  "hand", "hand_same", "hand_only", "table"):
            print("%-18s %d" % (k, stats[k]))
    elif mode == "--check":
        current = TABLE.read_text(encoding="utf-8") if TABLE.exists() else ""
        if current != text:
            sys.stderr.write("dialect-table.py: %s is STALE or hand-edited; run dialect-table.py "
                             "and commit the result\n" % TABLE)
            return 1
        print("dialect-table.py: %s matches a fresh generation (%d pairs)" % (TABLE.name, stats["table"]))
    else:
        TABLE.write_text(text, encoding="utf-8")
        print("dialect-table.py: wrote %s (%d pairs)" % (TABLE, stats["table"]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
