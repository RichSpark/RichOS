#!/usr/bin/env python3
"""Pinned dictionary generator. Rust owns parsing, checks, composition and rendering.
The existing command and Python test interface stay available to source users.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys

HERE = Path(__file__).resolve().parent
VARCON = HERE / "third_party" / "varcon" / "varcon.txt"
VARCON_SHA256 = "c1e234817b526a809fa745d742fec84db88b65c429e22faf8a556b3d5a224085"
VARCON_REVISION = "VarCon 2020.12.07, as vendored by crate-ci/typos at dd3e1018f8a825a98be71048c21a9b0495aff703"
DICT = HERE.parent / "dialect-en-US.dict"
OVERRIDES = HERE / "dialect-en-US.overrides.dict"
LEAVE = HERE / "dialect-en-US.leave.tsv"
TABLE = HERE / "dialect-en-US.generated.dict"
MAX_LEVEL = 70


class Refused(Exception):
    """An input the generator will not resolve on its own."""


def native(operation, value):
    # An explicit binary also supports isolated engine-only verification copies.
    supplied = os.environ.get('RICHOS_DEVTOOLS_BIN')
    try:
        if supplied:
            if not Path(supplied).is_absolute() or not Path(supplied).is_file():
                raise ValueError('RICHOS_DEVTOOLS_BIN must name an existing absolute executable')
            run = subprocess.run([supplied, 'rpc', operation], input=json.dumps(value),
                                 capture_output=True, text=True)
            reply = json.loads(run.stdout)
            if run.returncode:
                message = reply.get('error', run.stderr)
                if message.startswith('IO: '):
                    raise OSError(message[4:])
                raise ValueError(message)
            return reply['result']
        sys.path.insert(0, str(HERE.parents[3] / 'app/scripts/lib'))
        from devtools_bridge import request
        return request(operation, value)
    except ValueError as error:
        raise Refused(str(error)) from None


def read_varcon(path=VARCON, expected_sha256=VARCON_SHA256):
    return native('dialect-read-varcon', {'path': str(path), 'pin': expected_sha256})


def parse_clusters(text):
    return native('dialect-parse', text)


def split_varcon(clusters, max_level=MAX_LEVEL):
    return tuple(native('dialect-split', {'clusters': clusters, 'max_level': max_level}))


def read_pairs(path):
    return {k: tuple(v) for k, v in native('dialect-read-pairs', {'path': str(path)}).items()}


def read_leave(path):
    return native('dialect-read-pairs', {'path': str(path), 'leave': True})


def assemble(verified, any_tier, ambiguous, hand, overrides, leave):
    table, stats = native('dialect-assemble', dict(verified=verified, any_tier=any_tier,
                         ambiguous=ambiguous, hand=hand, overrides=overrides, leave=leave))
    return {k: tuple(v) for k, v in table.items()}, stats


def render(table, stats):
    return native('dialect-render', dict(table=table, stats=stats, pin=VARCON_SHA256,
                                       revision=VARCON_REVISION, max_level=MAX_LEVEL))


def generate(varcon_path=VARCON, expected_sha256=VARCON_SHA256, dict_path=DICT,
             overrides_path=OVERRIDES, leave_path=LEAVE):
    text, table, stats = native('dialect-generate', dict(
        varcon_path=str(varcon_path), expected_sha256=expected_sha256,
        dict_path=str(dict_path), overrides_path=str(overrides_path), leave_path=str(leave_path),
        pin=VARCON_SHA256, revision=VARCON_REVISION, max_level=MAX_LEVEL))
    return text, {k: tuple(v) for k, v in table.items()}, stats


def deny_lists():
    """Judge the generated table with the engine's own named-person predicate and publication
    write guard, twice (the whole file, then its American forms alone), and print verdicts only.
    Never prints a matched name or a private excerpt.

    An exit 0 from a guard is not a verdict: both also exit 0 when they stand down. So the
    named-person check calls the predicate's explicit modes (--doctor, --scan-text: OK/ABSENT,
    CLEAN/FOUND), and the publication check reads the guard's scanner result from its shell
    trace (the way the app's lint rule read the dialect hook until stream B). Exit 1 unless every
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
