#!/usr/bin/env bash
#
# guard-headed-browser.sh: PreToolUse rule on TWO surfaces. On Bash it is a module of
# dispatch-pretooluse.sh; on mcp__* tools it is registered directly (hooks/hooks.json).
# BROWSER AUTOMATION ON THE HOST RUNS HEADLESS. NO AGENT OPENS A BROWSER WINDOW ON THE SCREEN.
# dispatch-words: headless headed HEADED PWDEBUG playwright puppeteer webdriver selenium chrome Chrome chromium Chromium firefox Firefox msedge Edge Brave npm pnpm yarn npx bunx node python tsx ts-node deno bun
#
# 2026-10-28: a reviewer ran a Playwright script with `chromium.launch({ headless: false })` on the
# CEO's Mac and a Chrome window with test pages appeared in front of him. Nothing refused it.
# ceo-decisions §65: nothing appears on the host's screen; the test VM is the screen. This rule
# refuses --headed, `headless: false` in the code or script a command runs, the Playwright
# commands that always open a window, a browser binary started without --headless, and a
# browser MCP server configured without --headless. Its refusal says what to do instead. It runs
# in the desktop app's leases too (engine_profile.rs APP_BASH_RULES): a user's screen is the user's.
# 2026-10-29: headless is not enough for the FULL Chrome app (Chrome, Chrome for Testing,
# Chromium): at startup its macOS notification bridge registers with Notification Center and macOS
# asks the user to allow its notifications. A full-app launch also needs
# --disable-features=NativeNotifications,SystemNotifications, --disable-notifications and a
# throwaway profile; Puppeteer's default launch, Playwright's chromium channel and Selenium's
# Chrome are refused without them. Suites get the same rule from ui/tests/lib/quiet-browser.js.
#
# The rule and its shapes are in scripts/lib/headed_browser.py. It is a text match and leaks; its
# job is to stop the habitual launch. Any error in the check is a pass.
# Suite: guard-headed-browser.test.sh.

_ghb_in="$(cat)"
_ghb_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_UE_LIB="$_ghb_dir/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "guard-headed-browser.sh" "$_ghb_in" "" \
        "whether a command or MCP tool opens a visible browser window on this Mac"
fi
# Cheap exit first: a call that names no browser, no runner and no interpreter cannot open one.
case "$_ghb_in" in
    *'"mcp__'*|*headless*|*headed*|*HEADED*|*PWDEBUG*|*playwright*|*puppeteer*|*webdriver*|*selenium*|*chrome*|*Chrome*|*chromium*|*Chromium*|*firefox*|*Firefox*|*msedge*|*Edge*|*Brave*|*npm*|*pnpm*|*yarn*|*npx*|*bunx*|*node*|*python*|*tsx*|*ts-node*|*deno*|*bun*) ;;
    *) exit 0 ;;
esac
# Never /usr/bin/python3: on a Mac without developer tools it is Apple's stub, and running it puts
# the install dialog on the screen this rule protects (guard-toolchain.sh, §65). Take the first
# python3 on PATH outside /usr/bin (the app's bundled runtime, Homebrew); with none, pass.
_ghb_py=""
_ghb_ifs="$IFS"; IFS=:
for _ghb_d in $PATH; do
    [ -n "$_ghb_d" ] && [ "$_ghb_d" != "/usr/bin" ] && [ -x "$_ghb_d/python3" ] && { _ghb_py="$_ghb_d/python3"; break; }
done
IFS="$_ghb_ifs"
[ -n "$_ghb_py" ] || exit 0
printf '%s' "$_ghb_in" | "$_ghb_py" -B "$_ghb_dir/../lib/headed_browser.py" check
_ghb_rc=$?
[ "$_ghb_rc" = 2 ] && exit 2
exit 0
