#!/usr/bin/env bash
# operator-fences-fixture.sh: the shared fixture of the operator-fence suites.
# Sourced, never run. Everything lives under one scratch directory, $OFX; no
# repository, session record or lock of the operator's is read or written.
#
# WHY SESSIONS ARE SERVERS. The lease and the fence decide by ANCESTRY: a holder
# is a process that is an ancestor of the Git command, with its start time. A
# fixture that ran every command from the suite's own shell would make the suite
# an ancestor of everything, and one session's lease would authorize the other
# session's commands, which is exactly the property under test. So each fixture
# session is its own long-lived process with its own session record, and runs the
# commands it is handed as its children. The suite captures each server's pid at
# spawn and ends it by that pid, never by name.
#
#   ofx_init                     scratch root, fixture git config, lease home, entity
#   ofx_repo <name>              a main checkout with a bare origin, main pushed
#   ofx_session <name> [kind]    start a session server; kind = claude (default),
#                                codex (runs under the declared fake Codex
#                                executable, no session record) or none
#   ofx_in <name> <command...>   run a command inside that session; sets OFX_OUT, returns its rc
#   ofx_end <name>               end the server (its pid, captured at spawn)
#   ofx_cleanup                  end every server, remove the scratch root

OFX_ENGINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# The scratch root comes from the engine's one allocator (scripts/lib/scratch.sh),
# so the sweeper finds it even if a run is killed before ofx_cleanup runs.
# shellcheck source=scratch.sh
. "$OFX_ENGINE/scripts/lib/scratch.sh"

ofx_init() {
    OFX="$(scratch_new operator-fences-suite)" || return 1
    OFX="$(cd "$OFX" && pwd -P)"
    mkdir -p "$OFX/claude/sessions" "$OFX/locks" "$OFX/hooks" "$OFX/bin" "$OFX/s" "$OFX/forensics" "$OFX/entity"
    export CLAUDE_CONFIG_DIR="$OFX/claude"
    export RICHOS_LAND_LOCKS_DIR="$OFX/locks"
    export RICHOS_REF_FORENSICS_DIR="$OFX/forensics"
    export GIT_CONFIG_GLOBAL="$OFX/gitconfig"
    export GIT_CONFIG_NOSYSTEM=1
    unset CLAUDE_PID CLAUDE_CODE_ENTRYPOINT RICHOS_SESSION_PID RICHOS_ENTITY_ROOT LAND_LEASE_WAIT LAND_LEASE_TTL \
          GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE 2>/dev/null
    # An app lease exports these; inherited, they would aim the review ledger and the workspace
    # registry at the live ones (a case that wants one sets it itself).
    unset RICHOS_CLAUDE_HOME RICHOS_CLAUDE_ACCOUNT_DIR RICHOS_WORKSPACES_DIR RICHOS_APP_STATE 2>/dev/null
    # The fence suite runs through the operator's own global dispatcher when it
    # exists (spec r3 e2: "through his global dispatcher"), else a minimal
    # chaining one, so the launcher is reached exactly the way it is on his Mac.
    local dispatch="$HOME/.config/git/hooks/git-identity-guard-dispatch"
    if [ -x "$dispatch" ]; then
        ln -s "$dispatch" "$OFX/hooks/reference-transaction"
        OFX_DISPATCHER="global"
    else
        cat > "$OFX/hooks/reference-transaction" <<'D'
#!/usr/bin/env bash
COMMON="$(git rev-parse --git-common-dir 2>/dev/null)"
case "$COMMON" in /*) ;; *) COMMON="$(cd "$COMMON" && pwd)";; esac
[ -x "$COMMON/hooks/reference-transaction" ] && exec "$COMMON/hooks/reference-transaction" "$@"
exit 0
D
        chmod +x "$OFX/hooks/reference-transaction"
        OFX_DISPATCHER="fixture"
    fi
    printf '[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n[init]\n\tdefaultBranch = main\n[core]\n\thooksPath = %s\n[maintenance]\n\tauto = false\n[gc]\n\tauto = 0\n[advice]\n\tdetachedHead = false\n[commit]\n\tgpgSign = false\n' \
        "$OFX/hooks" > "$GIT_CONFIG_GLOBAL"
    # The declared non-Claude holder, standing in for Codex (the fixture has no
    # ChatGPT app, as W2 step 3b says): an executable at a fixture path that
    # STAYS ALIVE as the ancestor of what it runs, the way Codex's app-server is
    # the direct parent of its git. It cannot be a copy of /bin/bash: macOS
    # SIGKILLs a platform binary run from outside the system volume (measured,
    # "Killed: 9"). So it is compiled: fork, run bash in the child, wait.
    if command -v cc >/dev/null 2>&1; then
        cat > "$OFX/bin/fakecodex.c" <<'C'
#include <sys/wait.h>
#include <unistd.h>
int main(int argc, char **argv) {
    pid_t pid = fork();
    if (pid == 0) { argv[0] = "bash"; execv("/bin/bash", argv); _exit(127); }
    int status = 0;
    if (pid < 0 || waitpid(pid, &status, 0) < 0) return 126;
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128;
}
C
        cc -O0 -o "$OFX/bin/fakecodex" "$OFX/bin/fakecodex.c" 2>/dev/null
    fi
    [ -x "$OFX/bin/fakecodex" ] || { cp /bin/bash "$OFX/bin/fakecodex" && chmod +x "$OFX/bin/fakecodex"; }
    OFX_CODEX="$(cd "$OFX/bin" && pwd -P)/fakecodex"
    ofx_declare on
    cat > "$OFX/session-server.sh" <<'S'
#!/usr/bin/env bash
# $1 name, $2 kind. Writes its own session record (kind claude) and then runs
# each line written to its FIFO as a child, recording output and status.
name="$1"; kind="$2"; dir="$OFX/s"
if [ "$kind" = claude ]; then
    start="$(TZ=UTC0 LC_ALL=C ps -o lstart= -p $$ | awk '{$1=$1;print}')"
    printf '{"pid": %d, "sessionId": "%s", "cwd": "%s", "procStart": "%s", "kind": "interactive", "entrypoint": "cli"}\n' \
        "$$" "$name" "$OFX" "$start" > "$CLAUDE_CONFIG_DIR/sessions/$$.json"
fi
echo "$$" > "$dir/$name.pid"
n=0
while :; do
    if ! IFS= read -r line < "$dir/$name.fifo"; then continue; fi
    [ "$line" = "__exit__" ] && break
    n=$((n + 1))
    ( eval "$line" ) > "$dir/$name.out" 2>&1
    echo "$?" > "$dir/$name.rc.tmp" && mv "$dir/$name.rc.tmp" "$dir/$name.rc"
done
rm -f "$CLAUDE_CONFIG_DIR/sessions/$$.json"
S
    OFX_SERVERS=""
}

ofx_declare() { # on|off
    cat > "$OFX/entity/orchestration.config" <<C
OPERATOR_FENCES="$1"
LAND_LEASE_HOLDERS="codex=$OFX_CODEX"
OPERATOR_FENCES_REPOS=""
C
}

ofx_repo() { # <name> -> OFX_R (main checkout)
    local name="$1"
    git init -q --bare "$OFX/$name-origin.git"
    git init -q "$OFX/$name"
    OFX_R="$(cd "$OFX/$name" && pwd -P)"
    printf 'base\n' > "$OFX_R/f.txt"
    git -C "$OFX_R" add f.txt && git -C "$OFX_R" commit -q -m base
    git -C "$OFX_R" remote add origin "$OFX/$name-origin.git"
    git -C "$OFX_R" push -q -u origin main 2>/dev/null
}

ofx_session() { # <name> [claude|codex|none]
    local name="$1" kind="${2:-claude}" runner=/bin/bash
    mkfifo "$OFX/s/$name.fifo"
    rm -f "$OFX/s/$name.pid" "$OFX/s/$name.rc"
    [ "$kind" = codex ] && runner="$OFX/bin/fakecodex"
    OFX="$OFX" "$runner" "$OFX/session-server.sh" "$name" "$kind" </dev/null >/dev/null 2>&1 &
    local spawned=$!
    local i=0
    while [ ! -s "$OFX/s/$name.pid" ] && [ $i -lt 100 ]; do sleep 0.05; i=$((i + 1)); done
    OFX_SERVERS="$OFX_SERVERS $name:$spawned"
    eval "OFX_PID_$name=$spawned"
}

ofx_in() { # <name> <command words...>  (a single line)
    local name="$1"; shift
    rm -f "$OFX/s/$name.rc"
    printf '%s\n' "$*" > "$OFX/s/$name.fifo"
    local i=0 pid
    pid="$(cat "$OFX/s/$name.pid" 2>/dev/null)"
    while [ ! -f "$OFX/s/$name.rc" ]; do
        sleep 0.05; i=$((i + 1))
        if [ $((i % 20)) = 0 ] && ! kill -0 "$pid" 2>/dev/null; then
            OFX_OUT="(the fixture session $name is not running)"; return 98
        fi
        [ $i -gt 2400 ] && { OFX_OUT="(the fixture session $name did not answer in 120 s)"; return 99; }
    done
    OFX_OUT="$(cat "$OFX/s/$name.out")"
    return "$(cat "$OFX/s/$name.rc")"
}

ofx_end() { # <name>: end the server we spawned, by the pid captured at spawn
    local name="$1" entry pid
    for entry in $OFX_SERVERS; do
        [ "${entry%%:*}" = "$name" ] || continue
        pid="${entry#*:}"
        printf '__exit__\n' > "$OFX/s/$name.fifo" &
        local writer=$!
        local i=0
        while kill -0 "$pid" 2>/dev/null && [ $i -lt 60 ]; do sleep 0.05; i=$((i + 1)); done
        kill -0 "$pid" 2>/dev/null && kill "$pid" 2>/dev/null
        wait "$pid" 2>/dev/null
        kill "$writer" 2>/dev/null; wait "$writer" 2>/dev/null
        rm -f "$OFX/s/$name.fifo"
    done
    OFX_SERVERS="$(printf '%s\n' $OFX_SERVERS | grep -v "^$name:" | tr '\n' ' ')"
}

ofx_cleanup() {
    local entry
    for entry in $OFX_SERVERS; do ofx_end "${entry%%:*}"; done
    [ -n "${OFX:-}" ] && [ -d "$OFX" ] && { scratch_release "$OFX" >/dev/null 2>&1 || rm -rf "$OFX"; }
}

ofx_install() { # <repo> -- install the launcher (state off), with the fixture entity
    bash "$OFX_ENGINE/scripts/operator-fences.sh" install --repo "$1" --entity "$OFX/entity"
}

ofx_on()  { bash "$OFX_ENGINE/scripts/operator-fences.sh" on  --repo "$1" --entity "$OFX/entity"; }
ofx_off() {
    [ -s "$OFX/s/FenceAdmin.pid" ] || ofx_session FenceAdmin
    ofx_in FenceAdmin "bash '$OFX_ENGINE/scripts/operator-fences.sh' off --repo '$1' --reason 'fixture exercises off state without bypassing acceptance'"
}
ofx_lease() { echo "bash '$OFX_ENGINE/scripts/land-lease.sh' $*"; }
