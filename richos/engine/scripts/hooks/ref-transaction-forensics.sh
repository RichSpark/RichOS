#!/usr/bin/env bash
#
# ref-transaction-forensics.sh — git `reference-transaction` hook.
#
# WHY THIS EXISTS
# ---------------
# On 2026-09-13 refs/heads/main in /Users/alex/ab/richos moved three times
# leaving an EMPTY reflog message (18:09:09, 18:23:13, 23:41:29). Every other
# entry on that branch carries "merge", "commit", "reset" or "pull". An empty
# message means the ref was written by something that supplied no reflog
# message — plumbing (update-ref with no -m, commit-tree + update-ref), a
# non-git library (libgit2/dulwich), or a direct write to .git/refs/heads/main
# or packed-refs. The reflog records the RESULT and nothing about the writer,
# so the writer was unattributable.
#
# Git invokes `reference-transaction` for EVERY ref write, including plumbing
# and including writes that supply no reflog message. This hook therefore sees
# exactly the class of event that was evading the reflog, and records WHO did
# it: the writing process, its full command line, and its whole parent chain.
#
# CONTRACT (git's, not ours)
#   argv[1]  = transaction state: "prepared" | "committed" | "aborted"
#   stdin    = lines of "<old-oid> SP <new-oid> SP <refname>"
#   exit !=0 during "prepared" ABORTS the transaction and fails the command.
#
# THEREFORE THIS HOOK IS NEVER ALLOWED TO FAIL. It runs without `set -e`, it
# swallows every error, and its last statement is an unconditional `exit 0`.
# A forensic recorder that can break a merge is worse than no recorder.
#
# The log lives OUTSIDE every repository (default
# ~/.claude/state/ref-forensics/) so it survives a reset, a worktree removal,
# a reclone, and anything done to the repository it is watching.
#
# Append-only by construction: one `printf` per ref, `>>` only; the file is
# never truncated, rewritten or rotated by this hook.

# Deliberately NO `set -e` and NO `set -o pipefail`. See above.
_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -u 2>/dev/null || true

RICHOS_REF_FORENSICS_DIR="${RICHOS_REF_FORENSICS_DIR:-$HOME/.claude/state/ref-forensics}"
RICHOS_REF_FORENSICS_LOG="${RICHOS_REF_FORENSICS_LOG:-$RICHOS_REF_FORENSICS_DIR/ref-transactions.jsonl}"

# Everything below is best-effort. Any failure must still end in exit 0.
{
    mkdir -p "$RICHOS_REF_FORENSICS_DIR" 2>/dev/null

    PHASE="${1:-unknown}"

    # --- JSON string escaping, without spawning an interpreter ---------------
    # CAP, and why there is one: a `git commit -m` with a long body puts the
    # whole message on the command line, and Claude Code's shell wrapper puts a
    # ~1.5 kB preamble in front of every command. Uncapped, one ordinary commit
    # wrote ~24 kB to this log. 1000 bytes per command comfortably contains the
    # subcommand, its flags and the refs it names — which is the whole forensic
    # payload — and a truncation is MARKED with the number of bytes dropped, so
    # it can never be mistaken for the end of a command.
    CAP=1000
    cap() {
        local s="${1-}"
        if [ "${#s}" -gt "$CAP" ]; then
            printf '%s...[+%d bytes truncated]' "${s:0:$CAP}" "$(( ${#s} - CAP ))"
        else
            printf '%s' "$s"
        fi
    }

    jesc() {
        local s="${1-}"
        s="${s//\\/\\\\}"
        s="${s//\"/\\\"}"
        s="${s//$'\n'/\\n}"
        s="${s//$'\r'/\\r}"
        s="${s//$'\t'/\\t}"
        # Strip any remaining control bytes rather than emit invalid JSON.
        s="$(printf '%s' "$s" | tr -d '\000-\037')"
        printf '%s' "$s"
    }

    TS="$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null)"
    # Ordering matters: two of the three events on 2026-09-13 were one second
    # apart from the operation they followed.
    EPOCH="$(date '+%s' 2>/dev/null)"

    # --- Which repository ----------------------------------------------------
    # The COMMON dir, so every linked worktree reports the ONE shared store.
    COMMON="$(command git rev-parse --git-common-dir 2>/dev/null)"
    case "$COMMON" in
        /*) ;;
        "") COMMON="${GIT_DIR:-unknown}" ;;
        *)  COMMON="$(cd "$COMMON" 2>/dev/null && pwd)" || COMMON="unknown" ;;
    esac

    # --- Who is writing ------------------------------------------------------
    # This hook is a child of the process that opened the transaction, so the
    # parent chain IS the answer to "who ran it". Walk it to the top and
    # capture each ancestor's full command line before any of them exits.
    #
    # UNDER THE OPERATOR-FENCE LAUNCHER this hook is not Git's child but the
    # launcher's (scripts/lib/operator-fence-launcher.sh runs its chain as
    # children), so its own parent is the launcher and every row would name
    # "bash .../reference-transaction" as the writer. The launcher hands its own
    # parent, the Git process, down as RICHOS_REF_HOOK_PARENT, and the walk
    # starts there, so a record reads the same with or without the launcher.
    CHAIN=""
    p="${RICHOS_REF_HOOK_PARENT:-${PPID:-0}}"
    depth=0
    WRITER_PID=""
    WRITER_CMD=""
    while [ "$depth" -lt 12 ] && [ -n "$p" ] && [ "$p" != "0" ] && [ "$p" != "1" ]; do
        line="$(ps -o ppid=,command= -p "$p" 2>/dev/null)"
        [ -n "$line" ] || break
        # BSD ps: leading whitespace, then ppid, then the whole command line.
        line="${line#"${line%%[![:space:]]*}"}"
        parent="${line%% *}"
        cmd="${line#* }"
        if [ -z "$WRITER_PID" ]; then
            WRITER_PID="$p"
            WRITER_CMD="$cmd"
        fi
        [ -n "$CHAIN" ] && CHAIN="$CHAIN,"
        CHAIN="$CHAIN{\"pid\":$p,\"cmd\":\"$(jesc "$(cap "$cmd")")\"}"
        p="$parent"
        depth=$((depth + 1))
    done

    # --- The environment that decides the reflog message ---------------------
    # GIT_REFLOG_ACTION is what porcelain sets to produce "merge ...", "pull
    # ...", "reset: ...". Its ABSENCE is the signature of the writes being
    # hunted, so record present-but-empty and unset as DIFFERENT values.
    if [ "${GIT_REFLOG_ACTION+set}" = "set" ]; then
        RLA="\"$(jesc "${GIT_REFLOG_ACTION}")\""
    else
        RLA="null"
    fi

    HINTS=""
    for v in GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_AUTHOR_NAME \
             CLAUDE_SESSION_ID CLAUDECODE CODEX_SANDBOX TERM_PROGRAM; do
        eval "present=\${$v+set}"
        [ "${present:-}" = "set" ] || continue
        eval "val=\${$v}"
        [ -n "$HINTS" ] && HINTS="$HINTS,"
        HINTS="$HINTS\"$v\":\"$(jesc "${val}")\""
    done

    # --- One line per ref in the transaction ---------------------------------
    # Read stdin fully; never let a malformed line stop the loop.
    while IFS=' ' read -r OLD NEW REF; do
        [ -n "${REF:-}" ] || continue
        printf '{"ts":"%s","epoch":%s,"phase":"%s","repo":"%s","ref":"%s","old":"%s","new":"%s","hook_pid":%s,"writer_pid":%s,"writer_cmd":"%s","cwd":"%s","reflog_action":%s,"chain":[%s],"env":{%s}}\n' \
            "$TS" "${EPOCH:-0}" "$(jesc "$PHASE")" "$(jesc "$COMMON")" \
            "$(jesc "$REF")" "$(jesc "$OLD")" "$(jesc "$NEW")" \
            "$$" "${WRITER_PID:-0}" "$(jesc "$(cap "${WRITER_CMD:-}")")" "$(jesc "${PWD:-}")" \
            "$RLA" "$CHAIN" "$HINTS" \
            >> "$RICHOS_REF_FORENSICS_LOG" 2>/dev/null
    done
} 2>/dev/null

# UNCONDITIONAL. A non-zero exit here would abort the caller's ref transaction.
exit 0
