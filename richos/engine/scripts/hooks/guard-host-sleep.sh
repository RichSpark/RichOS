#!/usr/bin/env bash
#
# guard-host-sleep.sh: PreToolUse[Bash] rule (a module of dispatch-pretooluse.sh).
# NO AGENT HOLDS THE MAC AWAKE OR CHANGES ITS SLEEP SETTINGS.
# dispatch-words: caffeinate pmset systemsetup PowerManagement IOPMAssertion Amphetamine Caffeine KeepingYouAwake Lungo Theine
#
# ceo-decisions §65: nothing an agent runs changes the host Mac's display, sleep, lock, session or
# input state. On 2026-10-29 an agent held `caffeinate -i -w <pid>` on the CEO's Mac for about seven
# minutes; guard-host-display-power.sh refuses only the display forms (-d, -u), and the CEO's answer
# was that a hook refuses caffeinate from agents. This rule refuses every caffeinate, every pmset
# write and the other sleep-assertion changes, and its refusal says what to do instead. It runs in
# the desktop app's leases too (engine_profile.rs APP_BASH_RULES): a user's Mac is the user's.
#
# The rule and its shapes are in scripts/lib/host_sleep.py. It is a text match and leaks; its job
# is to stop the habitual command. Any error in the check is a pass. Suite: guard-host-sleep.test.sh.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
_ghs_in="$(cat)"
_ghs_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_UE_LIB="$_ghs_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-host-sleep.sh" "$_ghs_in" "" \
        "whether a Bash command holds this Mac awake or changes its sleep settings"
fi
# Cheap exit first: nearly every command names none of these.
case "$_ghs_in" in
    *caffeinate*|*pmset*|*systemsetup*|*PowerManagement*|*IOPMAssertion*|*Amphetamine*|*Caffeine*|*KeepingYouAwake*|*Lungo*|*Theine*) ;;
    *) exit 0 ;;
esac
_ghs_py="$(command -v python3 2>/dev/null || echo /usr/bin/python3)"
printf '%s' "$_ghs_in" | "$_ghs_py" -B "$_ghs_dir/../lib/host_sleep.py" check
_ghs_rc=$?
[ "$_ghs_rc" = 2 ] && exit 2
exit 0
