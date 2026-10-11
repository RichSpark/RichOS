# host-shield-fixture.sh — the throwaway suites and the tripwire every host-shield case uses.
# Sourced by lib/host_shield.test.sh and by the app runners' own cases, so every runner is held to
# the same two suites.
#
# NOTHING HERE CAN REACH THE HOST'S REAL COMMAND, by three separate walls:
#
#   * every command name is ASSEMBLED at run time, never written whole, so this file and the
#     suites it writes carry no call a mis-parse could execute from here;
#   * the bare-name suite checks that a host-shield stand-in is what PATH finds BEFORE it calls,
#     and otherwise fails without calling (which is how it goes red on a runner with no shield);
#   * the by-path suite exits on its first line: its by-path line exists only for the source
#     check to READ, and a runner that does not refuse it runs a suite that exits 1 first;
#   * callers put the tripwire directory first on PATH and leave /sbin and /usr/sbin off it, so a
#     stand-in that wrongly handed a call on would reach a tripwire, never the host.
#
# hs_fixture <tripwire-dir> <suite-dir>
#   <tripwire-dir>/<name>        for every stand-in name: appends `TRIPWIRE <name> <args>` (and,
#                                for osascript, `STDIN <text>`) to <tripwire-dir>/trip.log, exit 0
#   <suite-dir>/hs-bare.test.sh  calls the halt-or-restart command BY BARE NAME, `<it> -h +60`
#   <suite-dir>/hs-path.test.sh  names it by absolute path; touches <tripwire-dir>/path-suite-ran
#                                if it is ever started
# HS_CMD                         the assembled command name, for the callers' assertions

HS_LIB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HS_STANDINS="$HS_LIB/host-shield"
HS_TOOL="$HS_LIB/host_shield.py"
HS_CMD="shut"; HS_CMD="${HS_CMD}down"

hs_tripwire() {  # <tripwire-dir>
    local d="$1" n
    mkdir -p "$d"
    : > "$d/trip.log"
    for n in "$HS_STANDINS"/*; do
        n="$(basename "$n")"
        cat > "$d/$n" <<TRIP
#!/bin/sh
printf 'TRIPWIRE %s %s\n' "$n" "\$*" >> "$d/trip.log"
if [ "$n" = osascript ] && [ "\$#" -eq 0 ]; then printf 'STDIN %s\n' "\$(cat)" >> "$d/trip.log"; fi
exit 0
TRIP
        chmod +x "$d/$n"
    done
}

hs_fixture() {  # <tripwire-dir> <suite-dir>
    local trip="$1" suites="$2"
    hs_tripwire "$trip"
    mkdir -p "$suites"
    {
        printf '#!/usr/bin/env bash\n'
        printf '# A throwaway suite (host-shield-fixture.sh): it calls the host halt command by bare name.\n'
        printf 'found="$(command -v %s)"\n' "$HS_CMD"
        printf 'if [ "$(basename "$(dirname "$found")")" != host-shield ]; then\n'
        printf '  echo "  FAIL  no host-shield stand-in is first on PATH for %s ($found); the call is NOT made"; exit 1\n' "$HS_CMD"
        printf 'fi\n'
        printf '%s -h +60\n' "$HS_CMD"
        printf 'echo "  PASS  carried on after the call"\n'
        printf 'echo "=== hs-bare: all 1 passed ==="\n'
        printf 'exit 0\n'
    } > "$suites/hs-bare.test.sh"
    {
        printf '#!/usr/bin/env bash\n'
        printf '# A throwaway suite (host-shield-fixture.sh): its source names the host halt command by path.\n'
        printf 'touch "%s/path-suite-ran"\n' "$trip"
        printf 'echo "  FAIL  this suite was started although its source names the host command by path"\n'
        printf 'exit 1\n'
        printf '/sbin/%s -h +60\n' "$HS_CMD"
    } > "$suites/hs-path.test.sh"
}

# hs_path <tripwire-dir> — the PATH a case runs under: the tripwire first, python3's own
# directory and the two user binary directories, and NEITHER sbin.
hs_path() {
    local py
    py="$(dirname "$(command -v python3)")"
    printf '%s:%s:/usr/bin:/bin\n' "$1" "$py"
}
