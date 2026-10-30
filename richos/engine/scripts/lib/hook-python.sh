#!/usr/bin/env bash
#
# scripts/lib/hook-python.sh: EVERY HOOK'S python3 IS A REAL INTERPRETER, FOUND BY PATH, NEVER
# APPLE'S INSTALL-DIALOG STUB.
#
# On a Mac without the developer tools, /usr/bin/python3 (and git, make, ...) is a stub: running it
# opens "install command line developer tools" on the user's screen. Most regular users are not
# developers and have no developer tools (CEO, 2026-10-29), and the hooks call `python3` by name in
# hundreds of places, so a hook would open that dialog whenever /usr/bin came before a real python
# on its PATH. Every hook sources this file first (hook-python.test.sh holds that), so the name
# `python3` in a hook, in the libraries it sources and in the bash scripts it starts is the
# interpreter chosen here by path. Only file tests decide; nothing is run to find out.
#
# RESOLUTION, first found wins:
#   1. <engine>/runtime/bin/python3   the desktop app's bundled runtime (runtime.rs EngineRuntime::load)
#   2. the first python3 on PATH outside the stub folder (Homebrew, pyenv, a virtual environment)
#   3. <developer dir>/usr/bin/python3 by path (DEVELOPER_DIR, the xcode-select link, Xcode, the
#      Command Line Tools), the real tool the stub would have run
# THEN:
#   - python3 is a function running that interpreter, exported to the bash scripts a hook starts;
#     RICHOS_HOOK_PYTHON names it for anything else that wants it;
#   - when PATH would reach the stub folder before the chosen interpreter's folder, that folder goes
#     first on PATH, so `exec python3`, `env python3` and `#!/usr/bin/env python3` children (and, from
#     the runtime, its git) never reach a stub either. RICHOS_HOOK_CALLER_PATH keeps the PATH the
#     hook was started with, for a rule that judges the agent's own command (guard-toolchain.sh).
#   - with no interpreter at all, python3 says so on stderr and returns 127; the stub never runs.
# RICHOS_TOOLCHAIN_ROOT prefixes the system paths, as in toolchain.py (tests build fake Macs).
# Safe under set -euo pipefail; sourcing it twice in one shell does the work once.

if [ -n "${_HOOK_PYTHON_SOURCED:-}" ]; then
    return 0 2>/dev/null || true
fi
_HOOK_PYTHON_SOURCED=1
RICHOS_HOOK_CALLER_PATH="$PATH"

_hp_root="${RICHOS_TOOLCHAIN_ROOT:-}"
_hp_stubs="$_hp_root/usr/bin"
_hp_lib="${BASH_SOURCE[0]%/*}"
[ "$_hp_lib" != "${BASH_SOURCE[0]}" ] || _hp_lib=.
# The engine folder, spelled as the app spells it on PATH (<engine>/runtime/bin), with no fork:
# a hook sources this file as <engine>/scripts/hooks/../lib/hook-python.sh.
case "$_hp_lib" in
    */scripts/hooks/../lib) _hp_engine="${_hp_lib%/scripts/hooks/../lib}" ;;
    */scripts/lib) _hp_engine="${_hp_lib%/scripts/lib}" ;;
    *) _hp_engine="$_hp_lib/../.." ;;
esac
_hp_py=""

# 1. The bundled runtime, by its place beside this engine's scripts.
if [ -f "$_hp_engine/runtime/bin/python3" ] && [ -x "$_hp_engine/runtime/bin/python3" ]; then
    _hp_py="$_hp_engine/runtime/bin/python3"
fi
# 2. The first python3 on PATH that is not in a stub folder.
if [ -z "$_hp_py" ]; then
    _hp_ifs="$IFS"; IFS=:
    for _hp_d in $PATH; do
        if [ -n "$_hp_d" ] && [ "$_hp_d" != /usr/bin ] && [ "$_hp_d" != "$_hp_stubs" ] &&
            [ -f "$_hp_d/python3" ] && [ -x "$_hp_d/python3" ]; then
            _hp_py="$_hp_d/python3"
            break
        fi
    done
    IFS="$_hp_ifs"
fi
# 3. The developer directory the stub itself would use, read from the file system.
if [ -z "$_hp_py" ]; then
    _hp_link=""
    if [ -L "$_hp_root/var/db/xcode_select_link" ]; then
        _hp_link="$(readlink "$_hp_root/var/db/xcode_select_link" 2>/dev/null)" || _hp_link=""
        case "$_hp_link" in /*) _hp_link="$_hp_root$_hp_link" ;; ?*) _hp_link="$_hp_root/var/db/$_hp_link" ;; esac
    fi
    for _hp_d in "${DEVELOPER_DIR:-}" "$_hp_link" "$_hp_root/Applications/Xcode.app/Contents/Developer" \
        "$_hp_root/Library/Developer/CommandLineTools"; do
        if [ -n "$_hp_d" ] && [ -f "$_hp_d/usr/bin/python3" ] && [ -x "$_hp_d/usr/bin/python3" ]; then
            _hp_py="$_hp_d/usr/bin/python3"
            break
        fi
    done
fi

if [ -n "$_hp_py" ]; then
    case "$_hp_py" in /*) ;; *) _hp_py="$PWD/$_hp_py" ;; esac   # a hook may cd later
    RICHOS_HOOK_PYTHON="$_hp_py"
    export RICHOS_HOOK_PYTHON
    python3() { "$RICHOS_HOOK_PYTHON" "$@"; }
    # The interpreter's folder before the first stub folder on PATH.
    _hp_first=""
    _hp_ifs="$IFS"; IFS=:
    for _hp_d in $PATH; do
        if [ "$_hp_d" = "${_hp_py%/*}" ]; then _hp_first=py; break; fi
        if [ "$_hp_d" = /usr/bin ] || [ "$_hp_d" = "$_hp_stubs" ]; then _hp_first=stub; break; fi
    done
    IFS="$_hp_ifs"
    if [ "$_hp_first" != py ]; then
        PATH="${_hp_py%/*}:$PATH"
        export PATH
    fi
else
    python3() {
        printf '%s\n' "hook-python.sh: no Python interpreter outside Apple's developer-tools stub on this Mac, so this hook could not run Python; the stub was not run." >&2
        return 127
    }
fi
export -f python3
unset _hp_root _hp_stubs _hp_lib _hp_engine _hp_py _hp_ifs _hp_d _hp_link _hp_first _hp_dir
