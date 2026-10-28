#!/usr/bin/env python3
"""claude-login.mutation.py: proves the claude-login cases can fail.

Removes ONE property of testvm/claude-login.sh at a time, in a throwaway copy
of testvm/, runs `test/run-tests.sh "claude login"` there, and requires the
NAMED case to fail and the mutation to have applied. The shipped file is never
opened for writing.

    testvm/test/claude-login.mutation.py      exit 0 = every property load-bearing

About a minute: each mutant runs the login cases, two of which wait on a
one-second keeper loop. Not run by run-tests.sh, which stays fast; run it after
any change to claude-login.sh's push or keep.
"""
# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs every check with RICHOS_MUTATION_PASSES=0; this pass is run by hand (above).
# The gate still runs text_only() below: no case runs, but a mutant that no longer matches its
# source, or names a case that is gone, is refused there.
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
TESTVM = os.path.dirname(HERE)

# (name, file, old, new, the case that must fail)
MUTANTS = [
    ("no-host-renew-in-push", "claude-login.sh",
     '  host_renew || log "this Mac\'s claude did not answer the renewal request"\n  take_snapshot; SNAP_RC=$?',
     '  take_snapshot; SNAP_RC=$?',
     "renewed by THIS Mac first"),
    ("renew-even-when-long", "claude-login.sh",
     'if [ "$SNAP_RC" -ne 0 ] || [ "$LEFT" -lt "$TESTVM_CLAUDE_RENEW_BELOW_SECONDS" ]; then',
     'if true; then',
     "without asking this Mac to renew"),
    ("keep-never-pushes", "claude-login.sh",
     '      if "$0" push ${ACCOUNT_ITEM:+--account "$ACCOUNT_ITEM"} ${HOST_FOLDER:+--host-folder "$HOST_FOLDER"} ${ACCOUNT_ITEM:+--guest-home "$KC_HOME"} "$VM" "$GUEST_HOME" >/dev/null; then',
     '      if true; then',
     "keep hands the guest each token"),
    ("keep-asks-every-tick", "claude-login.sh",
     '] && [ "$ASKED_FOR" != "$EXP" ]; then',
     ']; then',
     "keep asks THIS Mac to renew once"),
    ("keep-outlives-run", "claude-login.sh",
     '    if [ ! -d "$STATE" ]; then',
     '    if false; then',
     "keep hands the guest each token"),
    ("primary-ignores-in-use", "claude-login.sh",
     '      HOST_SERVICE="$CLAUDE_KC_PREFIX-$(sha8 "$IN_USE_FOLDER")"\n',
     '',
     "the guest's primary login is the account the app has IN USE"),
    ("renew-inherits-callers-folder", "claude-login.sh",
     '  elif [ -z "$ACCOUNT_ITEM" ]; then\n    unset CLAUDE_CONFIG_DIR\n',
     '  elif false; then\n    :\n',
     "renewed under its OWN folder"),
    ("unreadable-list-guesses", "claude-login.sh",
     '  if ! IN_USE="$(in_use_account)"; then',
     '  if ! IN_USE="$(in_use_account)" && false; then',
     "never a guess at the account"),
]


def main():
    root = tempfile.mkdtemp(prefix="claude-login-mutation.")
    survived = 0
    try:
        for name, rel, old, new, case in MUTANTS:
            d = os.path.join(root, name)
            shutil.copytree(TESTVM, d, symlinks=True)
            p = os.path.join(d, rel)
            with open(p, encoding="utf-8") as fh:
                s = fh.read()
            if s.count(old) != 1:
                print("  FAIL  %s: the mutation did not apply (%d matches)" % (name, s.count(old)))
                survived += 1
                continue
            with open(p, "w", encoding="utf-8") as fh:
                fh.write(s.replace(old, new))
            r = subprocess.run(["bash", os.path.join(d, "test", "run-tests.sh"), "claude login"],
                               capture_output=True, text=True, timeout=600)
            failed = [l.strip() for l in r.stdout.splitlines() if l.startswith("  FAIL")]
            if any(case in l for l in failed):
                print("  PASS  %s: removing it turns \"%s\" red" % (name, case))
            else:
                print("  FAIL  %s: \"%s\" stayed green (failed: %s)" % (name, case, failed or "none"))
                survived += 1
            shutil.rmtree(d, ignore_errors=True)
    finally:
        shutil.rmtree(root, ignore_errors=True)
    print("=== claude-login mutation: %s ===" % (
        "all %d properties proven load-bearing" % len(MUTANTS) if not survived
        else "%d NOT proven" % survived))
    return 1 if survived else 0


def text_only():
    """What the merge gate checks while it skips this pass: each mutant's text appears exactly
    once in its file, and each case it names is in a `t "..."` line of test/run-tests.sh. A
    text search; no case runs (2026-10-05: nightly-local.mutation.py drifted from its source
    and only the nightly refused, df0feab6e; every app harness got this check)."""
    with open(os.path.join(HERE, "run-tests.sh"), encoding="utf-8") as fh:
        cases = [l for l in fh.read().splitlines() if l.startswith('t "')]
    problems = []
    for name, rel, old, _new, case in MUTANTS:
        with open(os.path.join(TESTVM, rel), encoding="utf-8") as fh:
            count = fh.read().count(old)
        if count != 1:
            problems.append("%s: the text to mutate appears %d times in %s, not once" % (name, count, rel))
        if not any(case in l for l in cases):
            problems.append("%s: no case in test/run-tests.sh is named \"%s\"" % (name, case))
    for problem in problems:
        print("  FAIL  %s" % problem)
    print("NOT RUN: %s, a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it); "
          "text check of its %d mutants: %s" % (os.path.basename(__file__), len(MUTANTS),
                                                "%d problem(s)" % len(problems) if problems else "all match"))
    return 1 if problems else 0


if __name__ == "__main__":
    if os.environ.get("RICHOS_MUTATION_PASSES") == "0":
        sys.exit(text_only())
    sys.exit(main())
