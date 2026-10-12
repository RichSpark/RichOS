#!/usr/bin/env python3
"""rerun-scrub.py — take credential values out of rerun files proof-run already wrote.

Until 2026-10-05 proof-run wrote the gate's whole environment into <run>/rerun/<nn>-<check>.sh,
CLAUDE_CODE_MESSAGING_TOKEN (a live session token) included. proof-run no longer writes one
(rerun_credential, the same rule used here); this rewrites the files already on disk: each line
that sets a credential becomes ${NAME+"NAME=$NAME"}, exactly what proof-run writes now, so the
file still reruns the check and takes the credential only from the shell that runs it. Every
other line, the file itself, its mode and its run are left as they are. Values are never printed.

    rerun-scrub.py [--check] [ROOT ...]     (default ROOT: resolved proof-runs state)

--check changes nothing and exits 1 while any rerun file under ROOT still sets a credential.
"""
import argparse
import importlib.util
import os
import shlex
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "lib"))
import machine_storage as M
DEFAULT_ROOT = os.path.join(os.path.expanduser("~"), ".richos-nightly", "proof-runs")
NOTE = ("# Credentials the gate had, never written here: %s. Each is passed on only when the shell\n"
        "# that runs this file has it.\n")


def proof_run():
    spec = importlib.util.spec_from_file_location("rerun_scrub_proof_run", os.path.join(HERE, "proof-run.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def credential_line(pr, line):
    """The variable's name when `line` is an `env -i` argument that sets a credential, else None."""
    if not line.startswith("  ") or not line.endswith(" \\\n") or line.startswith("  ${"):
        return None    # ${NAME+"NAME=$NAME"} is what a scrubbed or a new file holds: a name, never a value
    try:
        words = shlex.split(line[:-3])
    except ValueError:
        return None
    if len(words) != 1 or "=" not in words[0]:
        return None
    name, value = words[0].split("=", 1)
    if not name or not pr.rerun_credential(name, value):
        return None
    return name


def scrub(pr, path, write):
    """The names whose values `path` held (rewritten when `write`)."""
    with open(path) as fh:
        lines = fh.readlines()
    names, out = [], []
    for line in lines:
        name = credential_line(pr, line)
        if name is None:
            out.append(line)
            continue
        names.append(name)
        if pr.RERUN_SHELL_NAME.match(name):
            out.append('  ${%s+"%s=$%s"} \\\n' % (name, name, name))
    if names and write:
        if len(out) > 1 and not any(l.startswith("# Credentials the gate had") for l in out):
            out.insert(2 if out[1].startswith("#") else 1, NOTE % " ".join(names))
        mode = os.stat(path).st_mode & 0o7777
        fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), prefix=".scrub-")
        try:
            with os.fdopen(fd, "w") as fh:
                fh.writelines(out)
            os.chmod(tmp, mode)
            os.replace(tmp, path)
        except BaseException:
            if os.path.exists(tmp):
                os.unlink(tmp)
            raise
    return names


def rerun_files(root):
    for top, dirs, files in os.walk(root):
        if os.path.basename(top) == "rerun":
            for name in sorted(files):
                if name.endswith(".sh"):
                    yield os.path.join(top, name)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--check", action="store_true", help="change nothing; exit 1 while a credential is left")
    parser.add_argument("roots", nargs="*", default=[])
    args = parser.parse_args(argv)
    pr = proof_run()
    scanned, holding, counts = 0, 0, {}
    args.roots = args.roots or [str(M.state("proof-runs", "RICHOS_PROOF_RUN_DIR"))]
    for root in args.roots:
        for path in rerun_files(root):
            scanned += 1
            names = scrub(pr, path, write=not args.check)
            if names:
                holding += 1
                for name in names:
                    counts[name] = counts.get(name, 0) + 1
    print("rerun-scrub: %d rerun file(s) scanned under %s; %d %s" % (
        scanned, ", ".join(args.roots), holding,
        "still set a credential" if args.check else "had a credential's value taken out"))
    for name in sorted(counts):
        print("  %-40s %d file(s)" % (name, counts[name]))
    return 1 if args.check and holding else 0


if __name__ == "__main__":
    sys.exit(main())
