#!/usr/bin/env bash
#
# guard-toolchain.sh: PreToolUse[Bash] rule (a module of dispatch-pretooluse.sh).
# NO AGENT COMMAND REACHES AN APPLE DEVELOPER-TOOLS STUB WITHOUT A TOOLCHAIN BEHIND IT.
# dispatch-words: make git cc clang python3 swift xcrun xcodebuild xcode-select
#
# On 2026-10-29 an /usr/bin stub (make) ran where no developer directory was usable and macOS put
# "The make command requires the command line developer tools" on the CEO's screen (§65). This
# rule resolves the toolchain from the file system WITHOUT running the stub
# (scripts/lib/toolchain.py): a command whose tool is there passes; one whose tool is genuinely
# absent is stopped before the stub runs, with a plain "missing tool" result that names the tool,
# why it was needed and what installing it involves, asked about once (the lead asks the user).
# It runs in the desktop app's leases too (engine_profile.rs APP_BASH_RULES): a regular user's Mac
# usually has no developer tools at all, and that is exactly the Mac where the dialog would open.
# Any error in the check is a pass. Suite: scripts/lib/toolchain.test.sh.

_gtc_in="$(cat)"
# Cheap exit first: most commands name no stub at all.
case "$_gtc_in" in
    *make*|*git*|*cc*|*clang*|*gcc*|*python3*|*pip3*|*swift*|*xcrun*|*xcodebuild*|*xcode-select*|\
    *lldb*|*ld\ *|*ar\ *|*nm\ *|*strip*|*lipo*|*otool*|*bison*|*flex*|*m4*|*svn*|*ctags*|*c++*) ;;
    *) exit 0 ;;
esac
# No dirname: on a Mac without developer tools a scrubbed PATH may hold nothing beyond /bin.
_gtc_src="${BASH_SOURCE[0]}"
case "$_gtc_src" in */*) _gtc_dir="${_gtc_src%/*}" ;; *) _gtc_dir=. ;; esac
_gtc_dir="$(cd "$_gtc_dir" && pwd)"
# Nothing this hook runs may itself be a stub: /usr/bin/python3 IS one, and on a Mac without
# developer tools running it opens the very dialog this rule exists to prevent. Take the first
# python3 on PATH outside /usr/bin (the app's bundled runtime, Homebrew); with none, pass. That
# python's folder goes first on PATH for the rest of the hook, so the shared library sourced below
# (which calls `python3` by name) gets it too, never the stub.
_gtc_py=""
_gtc_stubs="${RICHOS_TOOLCHAIN_ROOT:-}/usr/bin"   # the suite's fake Mac keeps its stubs under a root
_gtc_ifs="$IFS"; IFS=:
for _gtc_d in $PATH; do
    [ -n "$_gtc_d" ] && [ "$_gtc_d" != "/usr/bin" ] && [ "$_gtc_d" != "$_gtc_stubs" ] &&
        [ -x "$_gtc_d/python3" ] && { _gtc_py="$_gtc_d/python3"; break; }
done
IFS="$_gtc_ifs"
[ -n "$_gtc_py" ] || exit 0
PATH="${_gtc_py%/*}:$PATH"
_UE_LIB="$_gtc_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-toolchain.sh" "$_gtc_in" "" \
        "whether a Bash command runs an Apple developer-tools stub with no toolchain behind it"
fi
printf '%s' "$_gtc_in" | "$_gtc_py" -B "$_gtc_dir/../lib/toolchain.py" check
_gtc_rc=$?
[ "$_gtc_rc" = 2 ] && exit 2
exit 0
