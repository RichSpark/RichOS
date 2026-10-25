#!/usr/bin/env bash
# launchd-install.test.sh — `--install` of the scratch reaper and the disk
# watchdog leaves an unchanged job alone, replaces a changed one, and never
# kills a run in progress.
#
# 2026-10-25: install.sh runs both `--install`s on every land, and each one
# used to bootout + bootstrap unconditionally. The land at 04:42:24 booted out
# the 04:40 cleanup two minutes into its scan, and that scheduled cleanup never
# happened. Every case below runs both jobs against a FAKE launchctl that keeps
# its own loaded/running state in the sandbox and records every call; the real
# launchd is never touched (RICHOS_LAUNCH_AGENTS_DIR is unset only so the code
# path that calls launchctl is the one under test, and PATH puts the fake first).
#
#   U  an unchanged, loaded job is not booted out or bootstrapped again
#   C  a changed job is booted out and bootstrapped once, from the new plist,
#      and the install after that recognizes it as current
#   R  a changed job that is RUNNING is not booted out; exit 3, old plist kept
#   S  an unchanged job that is running is left alone, exit 0
#   L  CONTROL: a job that is not loaded is bootstrapped
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --install refuses from a temp dir or worktree, so the copies under test live
# outside TMPDIR, /tmp, /private/var/folders and .claude/worktrees (the same
# base search as disk-watchdog-install.test.sh; no bypass in the scripts).
BASE=""
for _c in "${DW_INSTALL_TEST_BASE:-}" "$HOME/.cache" "/Volumes/E1TB/tmp/claude" "/Users/Shared"; do
    [ -n "$_c" ] || continue
    mkdir -p "$_c" 2>/dev/null || continue
    _r="$(cd "$_c" && pwd -P)"; _t="${TMPDIR:-/tmp}"; _t="${_t%/}"
    case "$_r" in /tmp/*|/private/tmp/*|/private/var/folders/*|*/.claude/worktrees/*|"$_t"/*) continue ;; esac
    [ -w "$_r" ] && { BASE="$_r"; break; }
done
[ -n "$BASE" ] || { echo "no usable base outside temp dirs"; exit 2; }
SB="$(cd "$(mktemp -d "$BASE/launchd-install.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SB"' EXIT
mkdir -p "$SB/bin" "$SB/ld" "$SB/home/Library/LaunchAgents" "$SB/engine/scripts/lib"
cp "$SCRIPT_DIR/scratch-reaper.sh" "$SCRIPT_DIR/disk-watchdog.sh" "$SB/engine/scripts/"
[ -f "$SCRIPT_DIR/lib/launchd-install.sh" ] && cp "$SCRIPT_DIR/lib/launchd-install.sh" "$SB/engine/scripts/lib/"
cp "$SCRIPT_DIR/../orchestration.config" "$SB/engine/"

# The fake launchctl. $SB/ld/<label>.loaded exists while loaded; .running holds
# the pid of a run in progress. Every call is appended to $SB/ld/calls.
cat >"$SB/bin/launchctl" <<'FAKE'
#!/bin/sh
LD="${FAKE_LAUNCHD_DIR:?}"
echo "$*" >>"$LD/calls"
cmd="$1"; shift
case "$cmd" in
    print)
        label="${1##*/}"
        [ -f "$LD/$label.loaded" ] || { echo "Could not find service" >&2; exit 113; }
        echo "$1 = {"
        if [ -f "$LD/$label.running" ]; then
            printf '\tstate = running\n\tpid = %s\n' "$(cat "$LD/$label.running")"
        else
            printf '\tstate = not running\n'
        fi
        printf '\truns = 1\n}\n'
        exit 0 ;;
    bootout)
        label="${1##*/}"
        [ -f "$LD/$label.loaded" ] || exit 3
        [ -f "$LD/$label.running" ] && echo "KILLED $label pid $(cat "$LD/$label.running")" >>"$LD/calls"
        rm -f "$LD/$label.loaded" "$LD/$label.running"
        exit 0 ;;
    bootstrap|load)
        plist="$2"; [ "$cmd" = load ] && plist="$1"
        label="$(basename "$plist" .plist)"
        [ -f "$LD/$label.loaded" ] && exit 5
        touch "$LD/$label.loaded"
        exit 0 ;;
esac
exit 0
FAKE
chmod +x "$SB/bin/launchctl"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS+1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL+1)); }
run() {
    env -u RICHOS_LAUNCH_AGENTS_DIR -u CLAUDE_CONFIG_DIR FAKE_LAUNCHD_DIR="$SB/ld" \
        PATH="$SB/bin:$PATH" HOME="$SB/home" bash "$SB/engine/scripts/$1.sh" --install 2>&1
}
reset() { rm -f "$SB/ld/"* "$SB/home/Library/LaunchAgents/"*; }
mutations() { grep -cE '^(bootout|bootstrap|load) ' "$SB/ld/calls" 2>/dev/null || true; }
killed()    { grep -c '^KILLED ' "$SB/ld/calls" 2>/dev/null || true; }

for job in scratch-reaper disk-watchdog; do
    label="com.richos.$job"
    plist="$SB/home/Library/LaunchAgents/$label.plist"
    echo "== $job"

    # L — control: nothing loaded, so it is bootstrapped.
    reset
    OUT="$(run "$job")"; RC=$?
    if [ "$RC" = 0 ] && [ -f "$SB/ld/$label.loaded" ] && [ -f "$plist" ] \
        && grep -q "^bootstrap gui/$(id -u) $plist\$" "$SB/ld/calls"; then
        ok "L  $job: a job that is not loaded is bootstrapped"
    else bad "L  $job: (rc=$RC) $OUT"; fi

    # U — installed and unchanged: the next install touches nothing.
    : >"$SB/ld/calls"
    OUT="$(run "$job")"; RC=$?
    M="$(mutations)"
    if [ "$RC" = 0 ] && [ "$M" = 0 ] && [ -f "$SB/ld/$label.loaded" ]; then
        ok "U  $job: an unchanged, loaded job is not booted out or reloaded"
    else bad "U  $job: rc=$RC, $M bootout/bootstrap call(s): $(tr '\n' '|' <"$SB/ld/calls") $OUT"; fi

    # S — unchanged and running: left alone, nothing killed.
    echo 4242 >"$SB/ld/$label.running"; : >"$SB/ld/calls"
    OUT="$(run "$job")"; RC=$?
    if [ "$RC" = 0 ] && [ "$(mutations)" = 0 ] && [ "$(killed)" = 0 ] && [ -f "$SB/ld/$label.running" ]; then
        ok "S  $job: an unchanged job that is running is left running"
    else bad "S  $job: rc=$RC, calls: $(tr '\n' '|' <"$SB/ld/calls") $OUT"; fi

    # C — loaded from an OLD plist and not running: replaced once, then current.
    reset
    printf '<plist version="1.0"><dict><key>Label</key><string>%s</string></dict></plist>\n' "$label" >"$plist"
    cp "$plist" "$SB/old.plist"
    touch "$SB/ld/$label.loaded"
    OUT="$(run "$job")"; RC=$?
    first="$(grep -nE '^bootout ' "$SB/ld/calls" | head -1 | cut -d: -f1)"
    second="$(grep -nE '^bootstrap ' "$SB/ld/calls" | head -1 | cut -d: -f1)"
    if [ "$RC" = 0 ] && [ "$(mutations)" = 2 ] && [ -n "$first" ] && [ -n "$second" ] \
        && [ "$first" -lt "$second" ] && ! cmp -s "$plist" "$SB/old.plist" \
        && grep -q "$job.sh" "$plist" && [ -f "$SB/ld/$label.loaded" ]; then
        : >"$SB/ld/calls"
        OUT2="$(run "$job")"; RC2=$?
        if [ "$RC2" = 0 ] && [ "$(mutations)" = 0 ]; then
            ok "C  $job: a changed job is replaced once from the new plist, then left alone"
        else bad "C  $job: replaced, but the next install reloaded it again (rc=$RC2): $(tr '\n' '|' <"$SB/ld/calls") $OUT2"; fi
    else bad "C  $job: not replaced as expected (rc=$RC): $(tr '\n' '|' <"$SB/ld/calls") $OUT"; fi

    # R — loaded from an OLD plist and RUNNING: not booted out, exit 3, says so.
    reset
    cp "$SB/old.plist" "$plist"
    touch "$SB/ld/$label.loaded"; echo 4242 >"$SB/ld/$label.running"
    OUT="$(run "$job")"; RC=$?
    if [ "$RC" = 3 ] && [ "$(mutations)" = 0 ] && [ "$(killed)" = 0 ] \
        && [ -f "$SB/ld/$label.running" ] && cmp -s "$plist" "$SB/old.plist" \
        && grep -q 'NOT REPLACED' <<<"$OUT" && grep -q 'pid 4242' <<<"$OUT" \
        && [ -z "$(find "$SB/home/Library/LaunchAgents" -name '*.new.*')" ]; then
        ok "R  $job: a changed job that is running is not killed; exit 3 says so"
    else bad "R  $job: rc=$RC, calls: $(tr '\n' '|' <"$SB/ld/calls") $OUT"; fi
done

printf '\n  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
