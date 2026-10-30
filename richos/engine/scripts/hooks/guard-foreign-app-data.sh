#!/usr/bin/env bash
#
# guard-foreign-app-data.sh: PreToolUse[Bash] rule (a module of dispatch-pretooluse.sh).
# NEVER ANOTHER APP'S DATA, and never a walk of the whole home folder.
#
# The CEO, 2026-09-25: "a regular user of RichOS is not expected to keep clicking
# those things", about "python3.14 would like to access data from other apps". The
# cause on his Mac was the disk watchdog's `du` (fixed at the source, disk-watchdog.py).
# THIS rule is for the door a source fix cannot close: a command an agent composes at
# runtime. Inside a RichOS session that command runs as a child of RichOS.app, so
# `find ~ -name x` makes macOS ask the USER whether RichOS may access data from other
# apps, and a refusal here is the only thing standing between that habit and the prompt.
#
# It refuses exactly two shapes (the rule and its reasons are in
# scripts/lib/foreign_app_data.py, bash_verdict):
#   (a) a path under another app's Containers or Group Containers folder (our own
#       com.richos.* containers pass);
#   (b) find, du, ls -R, grep -r or rg rooted at the home folder, its Library, /Users or /
#       deep enough to open an app's container (find -maxdepth 3 from ~ passes).
# Measured against 166,915 distinct Bash commands in this Mac's transcripts: see
# guard-foreign-app-data.test.sh's header for the count and how it was taken.
#
# `# foreign-app-data-exempt: <reason>` on the command passes it and is logged to
# ${CLAUDE_CONFIG_DIR:-~/.claude}/state/foreign-app-data-acks.log. A bare marker exempts nothing.
#
# IT IS A TEXT MATCH AND IT LEAKS (a script, a variable, `sh -c "$X"`). Its job is to stop
# the habitual command, not an adversary. Any error in the check is a pass.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
_gfad_in="$(cat)"
_gfad_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_UE_LIB="$_gfad_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-foreign-app-data.sh" "$_gfad_in" "" \
        "whether a Bash command reads another app's data or walks the whole home folder"
fi
# Cheap exits first: nearly every command names neither a container folder nor a walker.
case "$_gfad_in" in
    *Containers*|*find*|*du\ *|*grep*|*rg\ *|*ls\ *) ;;
    *) exit 0 ;;
esac
_gfad_py="$(command -v python3 2>/dev/null || echo /usr/bin/python3)"
printf '%s' "$_gfad_in" | "$_gfad_py" "$_gfad_dir/../lib/foreign_app_data.py" bash-check
_gfad_rc=$?
[ "$_gfad_rc" = 2 ] && exit 2
exit 0
