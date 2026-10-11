#!/usr/bin/env bash
#
# ocr-gate.sh — no frame goes into the record carrying a real person's details.
#
#   ocr-gate.sh <dir-or-png> [more...] [--control <png>] [--no-control <reason>]
#   ocr-gate.sh --text <file-or-dir> [--text ...] [<dir-or-png> ...]
#   ocr-gate.sh --help
#
# Exit 0 CLEAN, 1 a hit (named, with the frame), 2 the gate could not run.
#
# =============================================================================
# THE GATE'S OWN FAILURE MODE IS "0 HITS"
# =============================================================================
# A broken reader and a clean frame produce the identical answer, and it is the
# answer the gate is hoping for. So this refuses to report clean until it has
# PROVED the reader works, on a committed control image whose text is known.
# Without that proof the run exits 2 and says nothing about the frames.
#
# The control lives at fixtures/ocr-control.png in this directory. Its address
# is `qa-fixture@example.invalid` — `.invalid` is reserved by RFC 2606 and can
# never belong to anyone, so the control itself is not a thing to redact.
#
# =============================================================================
# THE LIST OF PEOPLE IS NOT IN THIS REPOSITORY, AND THIS SCRIPT DOES NOT HAVE ONE
# =============================================================================
# Five walks each wrote their own gate with its own inline list of the
# operator's names, company and handle — in a scratch script, so the roster was
# retyped from memory every time and was different every time. Moving that list
# into this file would publish it, which is worse.
#
# The project already answers this: `engine/scripts/lib/named-persons.py` holds
# the predicate, and the roster lives at `~/.richos-privacy/named-persons`,
# operator scope, refused at load if it ever resolves inside a work tree. This
# gate hands it the OCR text and adopts its four verdicts, including the one
# that matters here:
#
#   ABSENT is not clean. It means nothing was checked.
#
# On ABSENT this gate exits 2 and says so. A QA walk publishing frames is the
# release-shaped case, not the stranger-with-a-clone case: it happens on the
# owner's machine, where the list must exist.
#
# What this script owns on its own is the SHAPE patterns — an address, a
# `/Users/<name>` home path, a phone-shaped run of digits — which need no
# roster and catch the thing that actually leaked on every walk so far.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTROL="$HERE/fixtures/ocr-control.png"
NO_CONTROL=""
TARGETS=()
TEXTS=()

# Address-shaped, home-path-shaped, phone-shaped. Deliberately broad: a false
# positive costs one look, a false negative is published.
SHAPES='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}|/Users/[A-Za-z][A-Za-z0-9._-]+|\+?[0-9][0-9 ()-]{8,}[0-9]'

usage() {
    sed -n '3,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help)     usage; exit 0 ;;
        --control)     CONTROL="${2:-}"; shift 2 || { echo "ocr-gate.sh: $1 needs a value" >&2; exit 2; } ;;
        --no-control)  NO_CONTROL="${2:-}"; shift 2 || { echo "ocr-gate.sh: $1 needs a value" >&2; exit 2; } ;;
        --text)        TEXTS+=("${2:-}"); shift 2 || { echo "ocr-gate.sh: $1 needs a value" >&2; exit 2; } ;;
        -*)            echo "ocr-gate.sh: unknown option '$1'. --help" >&2; exit 2 ;;
        *)             TARGETS+=("$1"); shift ;;
    esac
done

if [ "${#TARGETS[@]}" -eq 0 ] && [ "${#TEXTS[@]}" -eq 0 ]; then
    echo "ocr-gate.sh: name a directory of frames or one or more PNGs. --help" >&2
    exit 2
fi

TESS="${RICHOS_QA_TESSERACT:-}"
if [ "${#TARGETS[@]}" -gt 0 ]; then   # text-only runs need no reader
if [ -z "$TESS" ]; then
    TESS="$(command -v tesseract || true)"
fi
for c in /opt/homebrew/bin/tesseract /usr/local/bin/tesseract; do
    [ -n "$TESS" ] && break
    [ -x "$c" ] && TESS="$c"
done
if [ -z "$TESS" ]; then
    echo "ocr-gate.sh: no tesseract on this machine (\`brew install tesseract\`)." >&2
    echo "             NOTHING WAS READ. That is not the same as nothing being there." >&2
    exit 2
fi

# --- the positive control ----------------------------------------------------
if [ -n "$NO_CONTROL" ]; then
    echo "ocr-gate.sh: CONTROL WAIVED — $NO_CONTROL"
    echo "             A clean result below proves nothing about the reader."
elif [ ! -f "$CONTROL" ]; then
    echo "ocr-gate.sh: the control image is missing ($CONTROL)." >&2
    echo "             Regenerate it with fixtures/make-fixtures.py, or waive the" >&2
    echo "             control with --no-control '<why>' and accept a weaker result." >&2
    exit 2
else
    CTL="$(RICHOS_QA_TESSERACT="$TESS" python3 "$HERE/ocr-read.py" --fresh "$CONTROL")" || exit 2
    if ! printf '%s' "$CTL" | grep -qiE 'qa-fixture@example\.invalid'; then
        echo "ocr-gate.sh: POSITIVE CONTROL FAILED." >&2
        echo "             The reader did not find the known address in the control" >&2
        echo "             image, so every '0 hits' it produces is meaningless." >&2
        echo "             It read: $(printf '%s' "$CTL" | tr '\n' ' ' | cut -c1-160)" >&2
        exit 2
    fi
    echo "positive control: the reader found the known address. 0 hits below means 0."
fi
fi

# --- the named-person predicate ---------------------------------------------
# Resolved from this checkout, so the gate and the write guards ask the same
# question of the same roster.
NP="$HERE/../../../engine/scripts/lib/named-persons.py"
NP_STATE="unavailable"
if [ -f "$NP" ]; then
    if printf '' | python3 "$NP" --scan-text "probe" >/dev/null 2>&1; then
        NP_STATE="ready"
    else
        NP_STATE="$(printf '' | python3 "$NP" --scan-text "probe" 2>/dev/null \
                     | head -1 | cut -f1 || true)"
        [ -n "$NP_STATE" ] || NP_STATE="unavailable"
    fi
fi
case "$NP_STATE" in
    ready)   echo "named-person predicate: ready" ;;
    ABSENT)
        echo "ocr-gate.sh: there is no named-person list on this machine." >&2
        echo "             That is NOT 'no names to check', it is 'nothing was" >&2
        echo "             checked'. Frames are about to be published from this" >&2
        echo "             machine, so the list must exist here." >&2
        echo "             See engine/scripts/lib/named-persons.py --doctor." >&2
        exit 2 ;;
    BROKEN)
        echo "ocr-gate.sh: the named-person list is unusable (BROKEN)." >&2
        echo "             See engine/scripts/lib/named-persons.py --doctor." >&2
        exit 2 ;;
    *)
        echo "ocr-gate.sh: the named-person predicate is not reachable at $NP." >&2
        echo "             Shape patterns below still run, but no ROSTER was checked." >&2
        NP="" ;;
esac

# --- a walk record's text files ---------------------------------------------
# Step lists and the record itself are text, so the frame reader never sees them. A home path
# that is not the guest's own /Users/admin, an address, or a listed name in one of them is
# published exactly as a frame would be. A synthetic line proves the scan first.
TEXT_HITS=0
if [ "${#TEXTS[@]}" -gt 0 ]; then
    TSHAPES='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}|/Users/[A-Za-z][A-Za-z0-9._-]+'
    text_hits() { grep -oiE "$TSHAPES" | grep -vxE '/Users/admin' | sort -u | head -5 || true; }
    CTLTXT="$(printf 'x /Users/qafixture/run y qa-fixture@example.invalid' | text_hits)"
    if [ "$(printf '%s\n' "$CTLTXT" | grep -c .)" -lt 2 ]; then
        echo "ocr-gate.sh: TEXT POSITIVE CONTROL FAILED: the scan did not flag a synthetic home path and address." >&2
        exit 2
    fi
    echo "text positive control: a synthetic home path and address were flagged."
    TFILES=()
    for t in "${TEXTS[@]}"; do
        if [ -d "$t" ]; then
            while IFS= read -r f; do TFILES+=("$f"); done \
                < <(find "$t" -type f \( -name '*.json' -o -name '*.md' -o -name '*.txt' -o -name '*.tree' -o -name '*.sh' -o -name '*.py' \) | LC_ALL=C sort)
        elif [ -f "$t" ]; then
            TFILES+=("$t")
        else
            echo "ocr-gate.sh: no such file or directory: $t" >&2
            exit 2
        fi
    done
    if [ "${#TFILES[@]}" -eq 0 ]; then
        echo "ocr-gate.sh: found NO text files under the --text paths. Refusing to call an empty set clean." >&2
        exit 2
    fi
    for f in "${TFILES[@]}"; do
        TXT="$(cat "$f")"
        FOUND=""
        HIT="$(printf '%s' "$TXT" | text_hits)"
        [ -n "$HIT" ] && FOUND="shape"
        if [ -n "$NP" ]; then
            case "$(printf '%s' "$TXT" | python3 "$NP" --scan-text "$(basename "$f")" 2>/dev/null || true)" in
                FOUND*) FOUND="${FOUND:+$FOUND+}roster" ;;
            esac
        fi
        if [ -n "$FOUND" ]; then
            TEXT_HITS=$((TEXT_HITS + 1))
            echo "HIT  $f  [$FOUND]"
            while IFS= read -r m; do
                [ -n "$m" ] && echo "       shape match, ${#m} characters, starting '${m:0:2}...'"
            done <<< "$HIT"
            case "$FOUND" in *roster*) echo "       a listed person's name is in this file" ;; esac
        fi
    done
    echo "--- TEXT GATE: $TEXT_HITS of ${#TFILES[@]} text file(s) carry something that must not ship ---"
    if [ "${#TARGETS[@]}" -eq 0 ]; then
        if [ "$TEXT_HITS" -gt 0 ]; then
            echo "Remove them from the file and run this gate again." >&2
            exit 1
        fi
        exit 0
    fi
fi

# --- collect the frames ------------------------------------------------------
FRAMES=()
for t in "${TARGETS[@]}"; do
    if [ -d "$t" ]; then
        while IFS= read -r f; do FRAMES+=("$f"); done \
            < <(find "$t" -type f \( -name '*.png' -o -name '*.PNG' \) | LC_ALL=C sort)
    elif [ -f "$t" ]; then
        FRAMES+=("$t")
    else
        echo "ocr-gate.sh: no such file or directory: $t" >&2
        exit 2
    fi
done

if [ "${#FRAMES[@]}" -eq 0 ]; then
    echo "ocr-gate.sh: found NO frames under the paths given. Refusing to call an" >&2
    echo "             empty set clean — say which directory you meant." >&2
    exit 2
fi

# Read each image once. Every invocation still evaluates the current privacy rules.
OCR_TEXTS="$(mktemp -d "${TMPDIR:-/tmp}/ocr-gate.XXXXXX")" || exit 2
trap 'rm -rf "$OCR_TEXTS"' EXIT
RICHOS_QA_TESSERACT="$TESS" python3 "$HERE/ocr-read.py" --export "$OCR_TEXTS" "${FRAMES[@]}" || exit 2
HITS=0
FRAME_INDEX=0
for f in "${FRAMES[@]}"; do
    TXT="$(cat "$OCR_TEXTS/$FRAME_INDEX.txt")"
    FRAME_INDEX=$((FRAME_INDEX + 1))
    FOUND=""

    SHAPE_HIT="$(printf '%s' "$TXT" | grep -oiE "$SHAPES" | sort -u | head -5 || true)"
    [ -n "$SHAPE_HIT" ] && FOUND="shape"

    ROSTER_HIT=""
    if [ -n "$NP" ]; then
        NPOUT="$(printf '%s' "$TXT" | python3 "$NP" --scan-text "$(basename "$f")" 2>/dev/null || true)"
        case "$NPOUT" in
            FOUND*) ROSTER_HIT="yes"; FOUND="${FOUND:+$FOUND+}roster" ;;
        esac
    fi

    if [ -n "$FOUND" ]; then
        HITS=$((HITS + 1))
        echo "HIT  $f  [$FOUND]"
        if [ -n "$SHAPE_HIT" ]; then
            # The match itself is the thing being protected, so it is reported
            # by its SHAPE and its length, never echoed into the log.
            while IFS= read -r m; do
                [ -n "$m" ] || echo
                echo "       shape match, ${#m} characters, starting '${m:0:2}...'"
            done <<< "$SHAPE_HIT"
        fi
        if [ -n "$ROSTER_HIT" ]; then
            echo "       a listed person's name is legible in this frame"
        fi
    fi
done

echo "--- OCR GATE: $HITS of ${#FRAMES[@]} frame(s) carry something that must not ship ---"
if [ "$HITS" -gt 0 ] || [ "$TEXT_HITS" -gt 0 ]; then
    echo "Redact them with redact.py and run this gate again on the OUTPUT." >&2
    exit 1
fi
exit 0
