#!/usr/bin/env bash
# launchd-install.test.sh — `--install` of the scratch reaper and the disk
# watchdog leaves an unchanged job alone, replaces a changed one, never kills a
# run in progress, and applies a change that had to wait for a run BY ITSELF
# when that run ends.
#
# 2026-10-25: install.sh runs both `--install`s on every land, and each one
# used to bootout + bootstrap unconditionally; a land at 04:42:24 booted the
# reaper out two minutes after the 04:40 cleanup was due, and that cleanup left
# no trace (inferred to have been killed, not proven). The first fix left a
# changed, running job untouched, but then nothing applied the change: it
# waited for some later, unrelated install. Every case below runs both jobs
# against a FAKE launchctl that keeps its own loaded/running state in the
# sandbox and records every call; the real launchd is never touched
# (RICHOS_LAUNCH_AGENTS_DIR is unset only so the code path that calls launchctl
# is the one under test, and PATH puts the fake first).
#
#   U  an unchanged, loaded job is not booted out or bootstrapped again
#   C  a changed job is booted out and bootstrapped once, from the new plist,
#      and the install after that recognizes it as current
#   R  a changed job that is RUNNING is not booted out; exit 3 says the change
#      is pending and applies when the run ends, old plist kept, one waiter
#   E  ...and when that run ends, the new plist is loaded with NO second
#      install: one bootout and one bootstrap, nothing killed, logged APPLIED
#   G  a run that outlasts the declared wait: the waiter gives up visibly
#      (GAVE UP in the job's log, the give-up file), the run is not touched,
#      the old plist stays, and the disk watchdog's cleanup_health reports it
#   H  ...and the next install that succeeds clears it from the health check
#   K  the health check: a missing give-up file is nothing, an unreadable one
#      is reported as unknown
#   D  the health check: a pending change whose waiter has ended (crashed or
#      killed before it could give up), or whose pid is now another process
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
STATE="$SB/home/.claude/state"

# A WAITER THIS TEST STARTED IS STOPPED BEFORE ITS SANDBOX GOES: with the fake
# launchctl deleted under it, it would read the real one. Only a pid from this
# sandbox's own pid files, and only when its command line names this sandbox.
stop_waiters() {
    for _f in "$STATE"/launchd-pending-*.pid; do
        [ -f "$_f" ] || continue
        _p="$(tr -cd '0-9' <"$_f")"; [ -n "$_p" ] || continue
        case "$(ps -p "$_p" -o command= 2>/dev/null || true)" in
            *"$SB/"*launchd-install-after-run.sh*) kill -TERM "$_p" 2>/dev/null || true ;;
        esac
        rm -f "$_f"
    done
}
trap 'stop_waiters; rm -rf "$SB"' EXIT
mkdir -p "$SB/bin" "$SB/ld" "$SB/home/Library/LaunchAgents" "$SB/engine/scripts/lib" "$STATE"
cp "$SCRIPT_DIR/scratch-reaper.sh" "$SCRIPT_DIR/disk-watchdog.sh" "$SB/engine/scripts/"
for _l in launchd-install.sh launchd-install-after-run.sh; do
    [ -f "$SCRIPT_DIR/lib/$_l" ] && cp "$SCRIPT_DIR/lib/$_l" "$SB/engine/scripts/lib/"
done
cp "$SCRIPT_DIR/../orchestration.config" "$SB/config.orig"
# The declared wait, shortened for the test: the last assignment wins.
set_wait() {
    cp "$SB/config.orig" "$SB/engine/orchestration.config"
    printf 'LAUNCHD_APPLY_WAIT_MAX_SECONDS="%s"\nLAUNCHD_APPLY_WAIT_POLL_SECONDS="1"\n' "$1" \
        >>"$SB/engine/orchestration.config"
}

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
reset() { stop_waiters; rm -f "$SB/ld/"* "$SB/home/Library/LaunchAgents/"* "$STATE/"*; }
mutations() { grep -cE '^(bootout|bootstrap|load) ' "$SB/ld/calls" 2>/dev/null || true; }
count()     { grep -cE "^$1 " "$SB/ld/calls" 2>/dev/null || true; }
killed()    { grep -c '^KILLED ' "$SB/ld/calls" 2>/dev/null || true; }
# The waiter for LABEL, read from its pid file; empty when there is none.
waiter() { tr -cd '0-9' 2>/dev/null <"$STATE/launchd-pending-$1.pid" || true; }
# Wait (bounded) until PID has exited. 0 when it did.
gone_within() {
    _i=0
    while kill -0 "$1" 2>/dev/null; do
        _i=$((_i + 1)); [ "$_i" -le "$(($2 * 5))" ] || return 1
        sleep 0.2
    done
    return 0
}
# Wait (bounded) until FILE holds PATTERN. 0 when it does.
logged_within() {
    _i=0
    until grep -q "$1" "$2" 2>/dev/null; do
        _i=$((_i + 1)); [ "$_i" -le "$(($3 * 5))" ] || return 1
        sleep 0.2
    done
    return 0
}
# The disk watchdog's existing health check, called the way its tick calls it.
health() {
    python3 - "$SCRIPT_DIR/lib" "$SB/home/Library/LaunchAgents" "$STATE" <<'PY' 2>&1
import importlib.util, sys, time
sys.path.insert(0, sys.argv[1])
spec = importlib.util.spec_from_file_location("dw", sys.argv[1] + "/disk-watchdog.py")
dw = importlib.util.module_from_spec(spec); spec.loader.exec_module(dw)
for p in dw.cleanup_health({}, "/nonexistent", 12, time.time(), sys.argv[2], sys.argv[3]):
    print(p)
print("HEALTH-CHECK-RAN")
PY
}

for job in scratch-reaper disk-watchdog; do
    label="com.richos.$job"
    plist="$SB/home/Library/LaunchAgents/$label.plist"
    case "$job" in
        scratch-reaper) joblog="$STATE/scratch-reaper-launchd.log" ;;
        disk-watchdog)  joblog="$STATE/disk-watchdog.launchd.log" ;;
    esac
    echo "== $job"
    set_wait 30

    # L — control: nothing loaded, so it is bootstrapped.
    reset
    OUT="$(run "$job")"; RC=$?
    if [ "$RC" = 0 ] && [ -f "$SB/ld/$label.loaded" ] && [ -f "$plist" ] \
        && grep -q "^bootstrap gui/$(id -u) $plist\$" "$SB/ld/calls"; then
        ok "L  $job: a job that is not loaded is bootstrapped"
    else bad "L  $job: (rc=$RC) $OUT"; fi
    cp "$plist" "$SB/current.plist"

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

    # R — loaded from an OLD plist and RUNNING: not booted out, exit 3, the
    # change is pending, and exactly one waiter holds it.
    reset
    cp "$SB/old.plist" "$plist"
    touch "$SB/ld/$label.loaded"; echo 4242 >"$SB/ld/$label.running"
    OUT="$(run "$job")"; RC=$?
    W="$(waiter "$label")"
    if [ "$RC" = 3 ] && [ "$(mutations)" = 0 ] && [ "$(killed)" = 0 ] \
        && [ -f "$SB/ld/$label.running" ] && cmp -s "$plist" "$SB/old.plist" \
        && grep -q 'applies by itself when that' <<<"$OUT" && grep -q 'pid 4242' <<<"$OUT" \
        && ! grep -qiE 'run this again|install again' <<<"$OUT" \
        && [ -n "$W" ] && kill -0 "$W" 2>/dev/null \
        && [ -z "$(find "$SB/home/Library/LaunchAgents" -name '*.new.*')" ]; then
        ok "R  $job: a changed running job is not killed; exit 3 says the change is pending"
    else bad "R  $job: rc=$RC, waiter=${W:-none}, calls: $(tr '\n' '|' <"$SB/ld/calls") $OUT"; fi

    # E — the run ends once the waiter is waiting on it. Nobody installs
    # again; the waiter applies the change.
    logged_within 'CHANGE PENDING' "$joblog" 20 || true
    rm -f "$SB/ld/$label.running"
    if [ -n "$W" ] && gone_within "$W" 20 && [ -z "$(waiter "$label")" ] \
        && [ -f "$SB/ld/$label.loaded" ] && cmp -s "$plist" "$SB/current.plist" \
        && [ "$(count bootout)" = 1 ] && [ "$(count bootstrap)" = 1 ] && [ "$(killed)" = 0 ] \
        && grep -q 'APPLIED' "$joblog" 2>/dev/null \
        && [ ! -e "$STATE/launchd-pending-$label.gave-up" ]; then
        ok "E  $job: when the run ends the new plist is loaded, with no second install"
    else bad "E  $job: waiter=${W:-none} still=$(waiter "$label"), calls: $(tr '\n' '|' <"$SB/ld/calls") log: $(tr '\n' '|' <"$joblog" 2>/dev/null)"; fi

    # G — the run outlasts the declared wait (2 s here): the waiter gives up
    # where the health check sees it, and the run is never touched.
    reset; set_wait 2
    cp "$SB/old.plist" "$plist"
    touch "$SB/ld/$label.loaded"; echo 4242 >"$SB/ld/$label.running"
    OUT="$(run "$job")"; RC=$?
    W="$(waiter "$label")"
    HEALTH=""
    if [ "$RC" = 3 ] && [ -n "$W" ] && gone_within "$W" 20; then HEALTH="$(health)"; fi
    if [ -n "$HEALTH" ] && [ "$(mutations)" = 0 ] && [ "$(killed)" = 0 ] \
        && [ -f "$SB/ld/$label.running" ] && cmp -s "$plist" "$SB/old.plist" \
        && grep -q 'GAVE UP' "$joblog" 2>/dev/null \
        && grep -q "^$label: .*GAVE UP.*pid 4242" <<<"$HEALTH"; then
        ok "G  $job: a run past the cap is left alone; the give-up is in the log and the health check"
    else bad "G  $job: rc=$RC waiter=${W:-none}, calls: $(tr '\n' '|' <"$SB/ld/calls") health: $HEALTH log: $(tr '\n' '|' <"$joblog" 2>/dev/null) $OUT"; fi

    # H — the run ends later and the next install applies it: the health
    # check no longer reports the give-up.
    rm -f "$SB/ld/$label.running"
    OUT="$(run "$job")"; RC=$?
    HEALTH="$(health)"
    if [ "$RC" = 0 ] && cmp -s "$plist" "$SB/current.plist" \
        && grep -q '^HEALTH-CHECK-RAN$' <<<"$HEALTH" && ! grep -q "^$label: " <<<"$HEALTH"; then
        ok "H  $job: the next successful install clears the give-up from the health check"
    else bad "H  $job: rc=$RC, health: $HEALTH $OUT"; fi
done

# K and D — the health check's own reads of the two pending-change files, once
# (the label makes no difference to them).
label=com.richos.scratch-reaper
reset
gu="$STATE/launchd-pending-$label.gave-up"; pf="$STATE/launchd-pending-$label.pid"

# K — a missing give-up file is nothing to report; one that cannot be read is.
H0="$(health)"
: >"$gu"; chmod 000 "$gu"
H1="$(health)"
chmod 600 "$gu"; rm -f "$gu"
if grep -q '^HEALTH-CHECK-RAN$' <<<"$H0" && ! grep -q "^$label: " <<<"$H0" \
    && grep -q "^$label: a give-up record exists but cannot be read" <<<"$H1"; then
    ok "K  a missing give-up file reports nothing; an unreadable one reports unknown"
else bad "K  missing: $H0 | unreadable: $H1"; fi

# D — a pending change whose waiter has ended, or whose pid is now another
# process (this test's own shell), is reported; neither ever reached give_up().
true & dead=$!; wait "$dead"
echo "$dead" >"$pf"
H2="$(health)"
echo "$$" >"$pf"
H3="$(health)"
rm -f "$pf"
if grep -q "^$label: a changed job is still pending, but its waiter (pid $dead) has ended" <<<"$H2" \
    && grep -q "^$label: a changed job is still pending, but its waiter's pid $$ is now another process" <<<"$H3"; then
    ok "D  a pending change whose waiter ended, or whose pid was reused, is reported"
else bad "D  ended: $H2 | reused: $H3"; fi

printf '\n  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
