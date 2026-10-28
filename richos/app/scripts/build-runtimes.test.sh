#!/usr/bin/env bash
# run-tests: no-host-screen: Python closure and isolated shell launchers; no screen or downloads
# run-tests: inputs richos/app/scripts/runtime-launchers.test.py richos/app/scripts/lib/probe_packaged.py richos/app/scripts/runtime-sources.json richos/app/scripts/lib/no_host_paths.py richos/app/scripts/build-runtimes.test.sh richos/app/scripts/build-runtimes.py
# run-tests: covers richos/app/scripts/build-runtimes.py richos/app/scripts/runtime-launchers.test.py
#
# build-runtimes.test.sh — the delivered Python's dependency closure, as build-runtimes.py
# checks it inside every fresh runtime build.
#
# A runtime is built once and reused (`nightly-local.py`'s `runtime()`), so the closure line in
# build-runtimes.py runs only when a runtime is rebuilt, which is rare. This suite runs THE SAME
# snippet (the module's CLOSURE_CHECK, read from the file, not a copy) now:
#   1. under the delivered runtime's Python when RICHOS_RUNTIME_DIR names one, else under the
#      Python 3 running this suite, so the check itself is proven to pass on a real interpreter;
#   2. it names ctypes and libproc, which provider-supervisor.py needs to reap a lease's tool
#      commands (the product reap gap design C8);
#   3. a positive probe: the same check with a module that does not exist FAILS, so a pass
#      above is not a snippet that cannot fail.
#   4. ffmpeg and ffprobe arrive as upstream zips: a zip holding exactly its one named file is
#      installed executable, and a zip with anything else in it is refused (synthetic zips only).
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON="python3"
if [ -n "${RICHOS_RUNTIME_DIR:-}" ] && [ -x "$RICHOS_RUNTIME_DIR/bin/python3" ]; then
  PYTHON="$RICHOS_RUNTIME_DIR/bin/python3"
fi
fail=0
check=$(python3 - "$HERE/build-runtimes.py" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("build_runtimes", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
print(module.CLOSURE_CHECK)
PY
) || { echo "  FAIL  B0 build-runtimes.py has no CLOSURE_CHECK"; exit 1; }

if out=$("$PYTHON" -c "$check" 2>&1); then
  echo "  PASS  B1 the closure check passes under $PYTHON ($out)"
else
  echo "  FAIL  B1 the closure check failed under $PYTHON: $out"; fail=1
fi
case "$check" in
  *ctypes*libproc.dylib*) echo "  PASS  B2 the closure check loads ctypes and libproc (reap gap C8)" ;;
  *) echo "  FAIL  B2 the closure check no longer loads ctypes and libproc: $check"; fail=1 ;;
esac
if "$PYTHON" -c "${check/import sqlite3/import sqlite3, richos_no_such_module}" >/dev/null 2>&1; then
  echo "  FAIL  B3 a closure check with a missing module still passed"; fail=1
else
  echo "  PASS  B3 a closure check with a missing module fails"
fi
if out=$(python3 - "$HERE/build-runtimes.py" 2>&1 <<'PY'
import importlib.util, os, sys, tempfile, zipfile
from pathlib import Path
spec = importlib.util.spec_from_file_location("build_runtimes", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
with tempfile.TemporaryDirectory(prefix="richos zipped tool ") as temporary:
    root = Path(temporary)
    good = root / "ffmpeg.zip"
    with zipfile.ZipFile(good, "w") as bundle:
        bundle.writestr("ffmpeg", "#!/bin/sh\necho fictional\n")
    module.install_zipped_executable(good, "ffmpeg", root / "ffmpeg")
    assert (root / "ffmpeg").read_text() == "#!/bin/sh\necho fictional\n"
    assert os.stat(root / "ffmpeg").st_mode & 0o777 == 0o755
    for members in (["ffmpeg", "extra"], ["ffprobe"], ["../ffmpeg"]):
        bad = root / "bad.zip"
        with zipfile.ZipFile(bad, "w") as bundle:
            for member in members:
                bundle.writestr(member, "x")
        try:
            module.install_zipped_executable(bad, "ffmpeg", root / "refused")
        except RuntimeError:
            assert not (root / "refused").exists(), members
        else:
            raise SystemExit(f"a zip holding {members} was installed")
print("ok")
PY
); then
  echo "  PASS  B4 a one-file ffmpeg zip installs executable; extra, misnamed or escaping members are refused"
else
  echo "  FAIL  B4 the zipped-executable install: $out"; fail=1
fi

# 5. The delivered git in an ordinary Mac repository: core.precomposeunicode=true (what `git
#    init` writes on macOS) and a non-ASCII file name. A git linked against stock GNU libiconv
#    dies there ("iconv_open(UTF-8,UTF-8-MAC) failed"). The same check_git_precompose the
#    builder runs on every fresh runtime, run here against RICHOS_RUNTIME_DIR's git.
# 6. Positive probes, always run: the check refuses a git that dies with that error and a git
#    that exits 0 but reports the name decomposed, so a pass in 5 is not a check that cannot fail.
if [ -n "${RICHOS_RUNTIME_DIR:-}" ] && [ -x "$RICHOS_RUNTIME_DIR/bin/git" ]; then
  if out=$(python3 - "$HERE/build-runtimes.py" "$RICHOS_RUNTIME_DIR/bin/git" 2>&1 <<'PY'
import importlib.util, os, sys
spec = importlib.util.spec_from_file_location("build_runtimes", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
env = {**os.environ, "PATH": os.path.dirname(sys.argv[2]) + ":/usr/bin:/bin:/usr/sbin:/sbin"}
try:
    print(module.check_git_precompose(sys.argv[2], env))
except RuntimeError as error:
    raise SystemExit(str(error))
PY
); then
    echo "  PASS  B5 $RICHOS_RUNTIME_DIR/bin/git status exits 0 in a core.precomposeunicode=true repository ($out)"
  else
    echo "  FAIL  B5 $RICHOS_RUNTIME_DIR/bin/git in a core.precomposeunicode=true repository: $out"; fail=1
  fi
else
  echo "  NOT RUN  B5 the delivered git's precomposed-unicode check: RICHOS_RUNTIME_DIR names no runtime with bin/git; build-runtimes.py ran it on every runtime it built"  # not-a-subcheck: with no runtime named this suite checks the builder only, as B1 falls back to the host Python; no runtime can exist untested, because build() runs check_git_precompose before it writes delivery.json, and verify-runtime.py refuses a runtime from any other recipe
fi
if out=$(python3 - "$HERE/build-runtimes.py" 2>&1 <<'PY'
import importlib.util, sys, tempfile, unicodedata
from pathlib import Path
spec = importlib.util.spec_from_file_location("build_runtimes", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
decomposed = unicodedata.normalize("NFD", module.PRECOMPOSE_NAME)
stubs = {
    "dies": "echo 'fatal: iconv_open(UTF-8,UTF-8-MAC) failed, but needed:' >&2; exit 128",
    "decomposed": "printf '?? %s\\0' '" + decomposed + "'",
}
with tempfile.TemporaryDirectory(prefix="richos fictional git ") as temporary:
    for label, status in stubs.items():
        stub = Path(temporary) / label
        stub.write_text('#!/bin/sh\ncase "$1" in\n  init) mkdir -p "$3" ;;\n  -C) case "$3" in status) '
                        + status + ' ;; esac ;;\nesac\n')
        stub.chmod(0o755)
        try:
            module.check_git_precompose(stub)
        except RuntimeError:
            continue
        raise SystemExit(f"a git that {label} passed the check")
print("ok")
PY
); then
  echo "  PASS  B6 the precomposed-unicode check refuses a git that dies on UTF-8-MAC and one that reports the name decomposed"
else
  echo "  FAIL  B6 the precomposed-unicode check's positive probes: $out"; fail=1
fi

python3 -B "$HERE/runtime-launchers.test.py" || fail=1
exit "$fail"
