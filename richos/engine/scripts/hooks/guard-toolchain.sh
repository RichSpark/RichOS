#!/usr/bin/env bash
#
# guard-toolchain.sh: PreToolUse[Bash] rule (a module of dispatch-pretooluse.sh).
# NO AGENT COMMAND REACHES AN APPLE DEVELOPER-TOOLS STUB WITHOUT A TOOLCHAIN BEHIND IT.
# dispatch-words: 2to3 actool agvtool ar as bison c++ c89 c99 cc clang clang++ cmpdylib codesign_allocate cpp ctags dsymutil dwarfdump flex g++ gcc gdb git git-receive-pack git-shell git-upload-archive git-upload-pack gm4 gnumake gperf ibtool indent install_name_tool ld lex libtool lipo lldb m4 make mig nm otool pip3 pydoc3 python3 python3-config ranlib rpcgen scalar segedit size strings strip svn swift swiftc unifdef unifdefall xcode-select xcode-select-tool xcodebuild xcrun xctrace xed yacc
# (every stub in scripts/lib/toolchain.py SHIMS: the dispatcher skips this rule for a call naming none
# of them, so a stub missing here would reach Apple's installer; toolchain.test.py T11 holds it)
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
# No dirname: on a Mac without developer tools a scrubbed PATH may hold nothing beyond /bin.
_gtc_src="${BASH_SOURCE[0]}"
case "$_gtc_src" in */*) _gtc_dir="${_gtc_src%/*}" ;; *) _gtc_dir=. ;; esac
_gtc_dir="$(cd "$_gtc_dir" && pwd)"
# Nothing this hook runs may itself be a stub: /usr/bin/python3 IS one, and on a Mac without
# developer tools running it opens the very dialog this rule exists to prevent. Take the first
# python3 on PATH outside /usr/bin (the app's bundled runtime, Homebrew); with none, pass. That
# python's folder goes first on PATH for the rest of the hook, so the shared library sourced below
# (which calls `python3` by name) gets it too, never the stub. Only file tests here, nothing run.
_gtc_py=""
_gtc_stubs="${RICHOS_TOOLCHAIN_ROOT:-}/usr/bin"   # the suite's fake Mac keeps its stubs under a root
_gtc_ifs="$IFS"; IFS=:
for _gtc_d in $PATH; do
    [ -n "$_gtc_d" ] && [ "$_gtc_d" != "/usr/bin" ] && [ "$_gtc_d" != "$_gtc_stubs" ] &&
        [ -x "$_gtc_d/python3" ] && { _gtc_py="$_gtc_d/python3"; break; }
done
IFS="$_gtc_ifs"
# The command is resolved with the caller's own PATH (review 2026-10-29: with the bundled python's
# folder prepended, its pip3 hid the /usr/bin stub the agent's shell would actually run).
_gtc_caller_path="$PATH"
[ -n "$_gtc_py" ] && PATH="${_gtc_py%/*}:$PATH"
# An unreadable payload is said, never passed in silence (unevaluated-payload.test.sh), and that
# comes BEFORE the cheap exit below: an empty, truncated or non-JSON payload names no stub, so the
# cheap exit used to pass it without a word. The library runs `python3` by name (to read the payload
# and to write the notice), so it runs only with the non-stub python first on PATH; with none, the
# hook passes as before (toolchain.test.py T9: a PATH without /bin even reads the payload as empty).
_UE_LIB="$_gtc_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ] && [ -n "$_gtc_py" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-toolchain.sh" "$_gtc_in" "" \
        "whether a Bash command runs an Apple developer-tools stub with no toolchain behind it"
fi
# Cheap exit: most commands name no stub at all. EVERY stub in toolchain.py SHIMS is here
# (a short name only as a word: followed by a non-word character, or last); toolchain.test.py
# T11 runs the hook once per stub.
case "$_gtc_in" in
    *2to3*|*actool*|*agvtool*|*ar[!A-Za-z0-9_-]*|*ar|*as[!A-Za-z0-9_-]*|*as|*bison*|\
    *c++[!A-Za-z0-9_-]*|*c++|*c89[!A-Za-z0-9_-]*|*c89|*c99[!A-Za-z0-9_-]*|*c99|\
    *cc[!A-Za-z0-9_-]*|*cc|*clang*|*clang++*|*cmpdylib*|*codesign_allocate*|\
    *cpp[!A-Za-z0-9_-]*|*cpp|*ctags*|*dsymutil*|*dwarfdump*|*flex*|*g++[!A-Za-z0-9_-]*|*g++|\
    *gcc[!A-Za-z0-9_-]*|*gcc|*gdb[!A-Za-z0-9_-]*|*gdb|*git[!A-Za-z0-9_-]*|*git|*git-receive-pack*|\
    *git-shell*|*git-upload-archive*|*git-upload-pack*|*gm4[!A-Za-z0-9_-]*|*gm4|*gnumake*|*gperf*|\
    *ibtool*|*indent*|*install_name_tool*|*ld[!A-Za-z0-9_-]*|*ld|*lex[!A-Za-z0-9_-]*|*lex|*libtool*|\
    *lipo*|*lldb*|*m4[!A-Za-z0-9_-]*|*m4|*make*|*mig[!A-Za-z0-9_-]*|*mig|*nm[!A-Za-z0-9_-]*|*nm|\
    *otool*|*pip3*|*pydoc3*|*python3*|*python3-config*|*ranlib*|*rpcgen*|*scalar*|*segedit*|*size*|\
    *strings*|*strip*|*svn[!A-Za-z0-9_-]*|*svn|*swift*|*swiftc*|*unifdef*|*unifdefall*|\
    *xcode-select*|*xcode-select-tool*|*xcodebuild*|*xcrun*|*xctrace*|*xed[!A-Za-z0-9_-]*|*xed|\
    *yacc*) ;;
    *) exit 0 ;;
esac
[ -n "$_gtc_py" ] || exit 0
printf '%s' "$_gtc_in" | RICHOS_TOOLCHAIN_CALLER_PATH="$_gtc_caller_path" "$_gtc_py" -B "$_gtc_dir/../lib/toolchain.py" check
_gtc_rc=$?
[ "$_gtc_rc" = 2 ] && exit 2
exit 0
