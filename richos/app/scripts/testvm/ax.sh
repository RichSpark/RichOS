#!/usr/bin/env bash
# ax.sh — the guest's screen, as text: read the accessibility tree, find a node
#         in it, press one, or send a key to the app under test.
#
#   testvm/ax.sh <vm> tree  [--app <name>] [--depth N] [--window N] [--json]
#   testvm/ax.sh <vm> find  --title <text> | --role <role> | --value <text>
#                           [--contains] [--app <name>] [--depth N] [--json]
#   testvm/ax.sh <vm> click --title <text> [--role <role>] [--nth N] [--contains]
#   testvm/ax.sh <vm> click --at <x>,<y>
#   testvm/ax.sh <vm> --focused              # what has focus right now
#   testvm/ax.sh <vm> --windows              # window names and sizes
#   testvm/ax.sh <vm> --key <keycode>        # a special key, to the app's pid
#   testvm/ax.sh <vm> '<applescript>'        # anything else, by hand
#
# ===========================================================================
# WHY tree / find / click ARE COMMITTED AND NOT WRITTEN PER WALK
# ===========================================================================
# The CEO, 2026-09-20: *"How many times does Ray build the same scripts or
# checks from scratch (for the same type of job)? And how much time does that
# waste in every one of his runs?"*
#
# For the guest's screen the answer was: roughly fourteen times across three
# walks — `axdump.js`, `axpress.js`, `axpress2.js`, `axpress3.js`, `axproc.js`,
# `axsafari.js`, `axmac.js`, `axdialog.js`, `axunlock.js`, `findsafari.sh`,
# `findmac.sh`, `findphone.sh`, and more. Every one of them was the same walk of
# the same tree, and the SEQUENCE of them is the cost: `axpress.js` matched on
# AXTitle, `axpress2.js` added a role filter, and `axpress3.js` — the third
# rewrite, after the second had reported NOTFOUND on buttons that were plainly
# on the screen — added AXDescription, because an aria-label surfaces there and
# not in the title. That lesson is now in ax.py's matcher, once, where the next
# walk gets it for free instead of rediscovering it.
#
# ===========================================================================
# EVERY SYNTHETIC KEY IS ADDRESSED TO A PID, AND REFUSED IF IT IS NOT FRONTMOST
# ===========================================================================
# System Events sends keystrokes to whatever is frontmost. On 2026-09-19 that
# behavior sent a Command-Q into the CEO's Terminal. Inside a guest the blast
# radius is small, but the discipline is the same and it costs nothing: resolve
# the app's pid, verify THAT pid is frontmost, and only then send. A key that
# lands somewhere unintended invalidates the test it was part of.
#
# `click --title` needs none of that, and that is the point of preferring it:
# an AXPress goes to the ELEMENT, so it works on a window that is not frontmost,
# that has moved, or that something is drawn over. `click --at x,y` is the
# fallback for controls that expose no AXPress, and it carries the same
# frontmost refusal as --key, for the same reason.
#
# Note --key sends a KEYCODE, never Command-Q. Quitting is stop.sh's job and it
# does it by pid (see that script's header for why).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Bound preflight, SSH transport, guest work, rendering and cleanup together.
if [ "${TESTVM_AX_SUPERVISED:-}" != 1 ]; then
  exec env TESTVM_AX_SUPERVISED=1 python3 "$HERE/ax-deadline.py" --host "${TESTVM_AX_TIMEOUT:-20}" bash "$0" "$@" </dev/null
fi
. "$HERE/lib.sh"

VM="${1:-}"; shift || true
[ -n "$VM" ] || die "usage: ax.sh <vm> tree|find|click|--focused|--windows|--key <code>|'<applescript>'"

# ONE indirection to the guest, the same shape `tailnet.sh`, `keychain.sh` and
# `claude-sync.sh` use, so every decision in here is testable against a guest
# that is a text file rather than a 25 GB image.
ag() {  # ag <shell command string>   — stdin is passed through untouched
  if [ -n "${TESTVM_GUEST_EXEC:-}" ]; then
    "$TESTVM_GUEST_EXEC" "$VM" "$@"
  else
    guest_ssh "$VM" "$@"
  fi
}

# tart's opinion about a VM of this name is not the authority on whether there
# is a guest to talk to when the caller has supplied its own reach.
if [ -z "${TESTVM_GUEST_EXEC:-}" ]; then
  preflight_tart
  require_vm_running "$VM"
fi

# Host timestamps for phase attribution; guest duration is reported on its own clock.
ax_phase() {
  [ -n "${TESTVM_AX_PHASE_FILE:-}" ] || return 0
  python3 - "$1" <<'PHASE'
import json, os, sys, time
with open(os.environ["TESTVM_AX_PHASE_FILE"], "a") as out:
    out.write(json.dumps({"phase": sys.argv[1], "at": time.monotonic()}) + "\n")
PHASE
}

PID="$(cat "$TESTVM_RUN/$VM/app.pid" 2>/dev/null || true)"

# The deadline on every accessibility read. A tree walk of a wedged app is the
# command must also reap any helper it starts when its deadline expires.
AX_TIMEOUT="${TESTVM_AX_TIMEOUT:-20}"
# REMOTE is the supervised prefix; each caller below appends its interpreter:
# `python3 -` for ax.py (tree/find/click/focus/type), `osascript -` for the raw scripts.
REMOTE="$(python3 - "$HERE/ax-deadline.py" "$AX_TIMEOUT" "${PID:-0}" <<'DEADLINE'
import json, os, pathlib, shlex, sys, time
try:
    seconds = float(sys.argv[2])
    assert 1 <= seconds <= 300
except (ValueError, AssertionError):
    raise SystemExit("TESTVM_AX_TIMEOUT must be between 1 and 300 seconds")
# Leave two seconds for diagnostics, SSH delivery and the host renderer.
remaining = float(os.environ['TESTVM_AX_DEADLINE']) - time.monotonic() - 2
if remaining < 1:
    print(json.dumps({"error": "preflight_deadline", "phase": "preflight", "detail": "preflight consumed the AX deadline; no guest command sent"}))
    raise SystemExit(124)
seconds = min(seconds, remaining)
# --pid names the app under test, so a read still running at SLOW_SECONDS in ax-deadline.py
# is sampled while it is overdue (esc-20261021T122950Z-b8b7362a).
pid = sys.argv[3] if sys.argv[3].isdigit() else "0"
print("python3 -c " + shlex.quote(pathlib.Path(sys.argv[1]).read_text()) +
      " --pid " + pid + " " + shlex.quote(str(seconds)))
DEADLINE
)" || { rc=$?; printf '%s\n' "$REMOTE"; exit "$rc"; }

# ===========================================================================
# tree / find / click / focus / type — ax.py, the accessibility API in the guest
# ===========================================================================
ax_walk() {  # ax_walk <mode> <args...>
  local mode="$1"; shift
  local app="" depth="" window="" maxn="" role="" subrole="" text="" value=""
  local domid="" expect_args=()
  local nth="" contains="" atx="" aty="" json=0 first="" scope="" window_title="" input="" replace=""
  if [ "$mode" = "type" ]; then input="${1:?type requires text}"; shift; fi

  while [ $# -gt 0 ]; do
    case "$1" in
      --app)      app="${2:-}";      shift 2 ;;
      --depth)    depth="${2:-}";    shift 2 ;;
      --window)   window="${2:-}";   shift 2 ;;
      --max)      maxn="${2:-}";     shift 2 ;;
      --role)     role="${2:-}";     shift 2 ;;
      --subrole)  subrole="${2:-}";  shift 2 ;;
      --id)       domid="${2:-}"; shift 2 ;;
      --expect)   expect_args+=("${2:-}"); shift 2 ;;
      --title)    text="${2:-}";     shift 2 ;;
      --value)    value="${2:-}";    shift 2 ;;
      --first)    first=1; shift ;;
      --replace)  replace=1; shift ;;
      --in)
        scope="${2:-}"; shift 2
        case "$scope" in
          window) window_title="${1:?--in window needs a title}"; shift ;;
          dialog|sidebar|composer|menubar) ;;
          *) die "--in needs dialog, sidebar, composer, menubar or window <title>" ;;
        esac ;;
      --nth)      nth="${2:-}";      shift 2 ;;
      --contains) contains=1;        shift ;;
      --json)     json=1;            shift ;;
      --at)
        case "${2:-}" in
          *,*) atx="${2%%,*}"; aty="${2##*,}" ;;
          *)   die "--at takes x,y (for example --at 1200,80)" ;;
        esac
        shift 2 ;;
      *) die "unknown argument to $mode: $1" ;;
    esac
  done

  if [ -n "$atx" ]; then
    [ "$mode" = "click" ] || die "--at belongs to click"
    mode="clickat"
  fi

  case "$mode" in
    find|click|focus|type)
      [ -n "$text$role$value$subrole$domid" ] \
        || die "$mode needs something to match: --title, --role, --value or --subrole" ;;
  esac

  # The target is the app under test unless a name is given. A named process is
  # how the PHONE half of a walk is read — the phone is Safari in the guest.
  if [ -z "$app" ] && [ -z "$PID" ]; then
    die "no app pid recorded for $VM — was it started by run.sh? (or name a process with --app)"
  fi

  # The parameter block is built by python3 so a title with a quote, a brace or
  # a newline in it cannot become part of the program. It is PREPENDED to ax.py as
  # one assignment of a Python string literal (repr of the JSON) and travels on
  # stdin; nothing is interpolated into a remote command line.
  local params
  params="$(AX_MODE="$mode" AX_APP="$app" AX_PID="${PID:-0}" AX_WINDOW="$window" \
            AX_DEPTH="$depth" AX_MAX="$maxn" AX_ROLE="$role" AX_SUBROLE="$subrole" \
            AX_TEXT="$text" AX_VALUE="$value" AX_ID="$domid" AX_NTH="$nth" AX_CONTAINS="$contains" \
            AX_ATX="$atx" AX_ATY="$aty" AX_FIRST="$first" AX_SCOPE="$scope" AX_WINDOW_TITLE="$window_title" \
            AX_INPUT="$input" AX_REPLACE="$replace" \
            python3 -c '
import json, os, sys

def opt(name):
    v = os.environ.get(name, "")
    return v if v != "" else None

def num(name, default=None):
    v = os.environ.get(name, "")
    if v == "":
        return default
    try:
        return int(v)
    except ValueError:
        sys.stderr.write("[testvm] ERROR: %s must be a whole number, got %r\n" % (name, v))
        raise SystemExit(2)

p = {
    "mode":     os.environ["AX_MODE"],
    "app":      opt("AX_APP"),
    "pid":      num("AX_PID", 0),
    "window":   num("AX_WINDOW"),
    "depth":    num("AX_DEPTH", 16),
    "max":      num("AX_MAX", 4000),
    "role":     opt("AX_ROLE"),
    "sub":      opt("AX_SUBROLE"),
    "id":       opt("AX_ID"),
    "text":     opt("AX_TEXT"),
    "value":    opt("AX_VALUE"),
    "nth":      num("AX_NTH"),
    "first": os.environ.get("AX_FIRST") == "1",
    "scope": opt("AX_SCOPE"),
    "windowTitle": opt("AX_WINDOW_TITLE"),
    "input": os.environ.get("AX_INPUT", ""),
    "replace": os.environ.get("AX_REPLACE") == "1",
    "contains": os.environ.get("AX_CONTAINS") == "1",
    "atx":      num("AX_ATX"),
    "aty":      num("AX_ATY"),
}
expect = {}
for condition in sys.argv[1:]:
    key, sep, value = condition.partition("=")
    if not sep or key not in ("value", "enabled", "selected", "current", "title", "description") or key in expect:
        raise SystemExit("--expect needs a unique value|enabled|selected|current|title|description=VALUE")
    expect[key] = value
if expect and p["mode"] != "click":
    raise SystemExit("--expect requires a selector-based click")
p["expect"] = expect or None
if p["first"] and p["nth"] is not None:
    raise SystemExit("--first and --nth are mutually exclusive")
for key in ("depth", "max", "window"):
    if p[key] is not None and p[key] < 1: raise SystemExit(key + " must be positive")
if p["nth"] is not None and p["nth"] < 0: raise SystemExit("--nth must be nonnegative")
print("AX_PARAMS_JSON = %r" % json.dumps(p))
' ${expect_args[@]+"${expect_args[@]}"})" || die "could not build the parameter block (see above)"

  local errf raw rc=0
  # §54: this scratch file goes however this function ends.
  errf="${TESTVM_AX_LOG_FILE:-$(mktemp "${TMPDIR:-/tmp}/testvm-ax.XXXXXX")}"
  ax_phase transport
  raw="$( { printf '%s\n' "$params"; cat "$HERE/ax.py"; } | ag "$REMOTE python3 -" 2>"$errf" )" || rc=$?
  ax_phase render
  if [ "$rc" -ne 0 ]; then
    printf '%s\n' "$raw"
    cat "$errf" >&2
    rm -f "$errf"
    echo "[testvm] the accessibility read failed (exit $rc)." >&2
    if [ "$rc" -eq 124 ]; then
      echo "         Exit 124 is the read's own deadline, NOT a TCC failure (a missing grant says" >&2
      echo "         'not allowed assistive access'). On a busy host or a heavy app tree, retry" >&2
      echo "         with a narrower read (--window N, --depth N) before suspecting the grant." >&2
    else
      echo "         If the error mentions 'not allowed assistive access', the Accessibility" >&2
      echo "         TCC grant did not take — re-run testvm/setup.sh --reprovision." >&2
    fi
    echo "         If it produced nothing at all, the read hit its ${AX_TIMEOUT}s deadline" >&2
    echo "         Check the target process and any SecurityAgent dialog before retrying." >&2
    return "$rc"
  fi
  cat "$errf" >&2
  rm -f "$errf"

  printf '%s\n' "$raw" | AX_RENDER_JSON="$json" python3 -c '
import json, os, sys

# ===========================================================================
# THE RENDERER, AND THE ONE THING IT REFUSES TO PRINT
# ===========================================================================
# ax.py emits real numbers (AXPosition/AXSize unpacked as CGPoint/CGSize), so a
# fused geometry token cannot arrive from it.
# This check is here for the day something else feeds this renderer — an
# AppleScript dump, a hand-made fixture, a future guest-side rewrite — because
# the failure it catches is SILENT: "1024700" is a plausible-looking number and
# a walk would quote it. A measurement that cannot be trusted is not printed
# and not reported as a success.
# ===========================================================================

as_json = os.environ.get("AX_RENDER_JSON") == "1"
fused = []
err = None
invalid = False
truncated = False

def geom(v, where):
    if v is None:
        return "-"
    if isinstance(v, bool):
        fused.append((where, repr(v)))
        return "?"
    if isinstance(v, (int, float)):
        return str(int(v)) if float(v).is_integer() else str(v)
    fused.append((where, str(v)))
    return "?"

lines = []
for raw_line in sys.stdin.read().splitlines():
    line = raw_line.strip()
    if not line:
        continue
    try:
        rec = json.loads(line)
    except ValueError:
        invalid = True
        sys.stderr.write("[testvm] ax: unparseable line from the guest: %s\n" % line[:200])
        continue
    if not isinstance(rec, dict):
        continue

    if rec.get("meta"):
        truncated = bool(rec.get("truncated"))
        if not as_json:
            bits = ["# app=%s" % rec.get("app", "?"), "pid=%s" % rec.get("pid", "?"),
                    "windows=%s" % rec.get("windows", "?"), "nodes=%s" % rec.get("nodes", "?"),
                    "mode=%s" % rec.get("mode", "?")]
            if rec.get("matches") is not None:
                bits.append("matches=%s" % rec["matches"])
            lines.append(" ".join(bits))
        else:
            lines.append(line)
        continue

    if rec.get("error"):
        err = rec
        if as_json:
            lines.insert(0, line)
        else:
            lines.insert(0, "[testvm] ax: %s: %s" % (err.get("error"), err.get("detail", "")))
            if err.get("near_matches"):
                lines.insert(1, "role-mismatch hints (not clicked): " + json.dumps(err["near_matches"]))
        continue

    if rec.get("clicked"):
        if as_json:
            lines.append(line)
        elif rec.get("at"):
            lines.append("clicked at %s %s" % (rec.get("x"), rec.get("y")))
        else:
            n = rec.get("node") or {}
            lines.append("pressed %s title=%r desc=%r pos=%s %s size=%s %s (matches=%s)" % (
                n.get("role", "?"), n.get("title", ""), n.get("desc", ""),
                geom(n.get("x"), "pos.x"), geom(n.get("y"), "pos.y"),
                geom(n.get("w"), "size.w"), geom(n.get("h"), "size.h"),
                rec.get("matches")))
        continue

    if rec.get("action"):
        lines.append(line if as_json else "%s verified=%s" % (rec["action"], rec.get("verified")))
        continue

    # a node
    if as_json:
        lines.append(line)
        for k, where in (("x", "pos.x"), ("y", "pos.y"), ("w", "size.w"), ("h", "size.h")):
            geom(rec.get(k), where)
        continue

    depth = rec.get("d", 0)
    if not isinstance(depth, int) or depth < 0:
        depth = 0
    enabled = rec.get("enabled")
    enabled = "-" if enabled is None else ("yes" if enabled else "no")
    sub = rec.get("sub") or ""
    lines.append("%s%s%s title=%r desc=%r value=%r enabled=%s pos=%s %s size=%s %s" % (
        "  " * depth,
        rec.get("role", "?"),
        ("/" + sub) if sub else "",
        rec.get("title", ""), rec.get("desc", ""), rec.get("value", ""),
        enabled,
        geom(rec.get("x"), "pos.x"), geom(rec.get("y"), "pos.y"),
        geom(rec.get("w"), "size.w"), geom(rec.get("h"), "size.h"),
    ))

for line in lines:
    print(line)

if truncated:
    sys.stderr.write("[testvm] ax: the walk hit its node cap and STOPPED — this tree is "
                     "incomplete. Narrow it (--window N, --role, --depth) or raise --max.\n")

if (not lines and not err) or invalid:
    sys.stderr.write("[testvm] ax: empty or invalid guest response\n")
    raise SystemExit(3)

if fused:
    where, value = fused[0]
    sys.stderr.write(
        "[testvm] REFUSED to print a geometry value: %s arrived as the text %r, not as a "
        "number.\n"
        "         That is the AppleScript list-coercion shape: {1024, 700} renders as "
        "\"1024700\", so a 1024x700 box and a 102x4700 one are the same six characters.\n"
        "         %d value(s) in this read. The numbers above marked ? were NOT measured.\n"
        % (where, value, len(fused)))
    raise SystemExit(4)

if err:
    sys.stderr.write("[testvm] ax: %s — %s\n" % (err.get("error"), err.get("detail", "")))
    if err.get("actions") is not None:
        sys.stderr.write("         the element does offer: %s\n" %
                         (", ".join(err["actions"]) or "no actions at all"))
    raise SystemExit(1)
if truncated:
    raise SystemExit(5)
'
}

case "${1:-}" in
  --help|-h)
    echo 'ax.sh VM tree|find|click|focus|type TEXT [--title TEXT|--role ROLE|--value TEXT]'
    echo '  --in dialog|sidebar|composer|menubar|window TITLE  --first | --nth N (zero based)'
    echo '  --id DOM_ID --expect value=VALUE (click; also enabled, selected, current, title, description)'
    echo '  --replace (type) --contains --app NAME --window N --depth N --max N --json'
    echo '  find defaults to exhaustive; actions require uniqueness unless --first/--nth.'
    exit 0 ;;
  tree|find|click|focus|type)
    SUB="$1"; shift
    ax_walk "$SUB" "$@"
    exit $?
    ;;
esac

case "${1:-}" in
  --focused)
    [ -n "$PID" ] || die "no app pid recorded for $VM — was it started by run.sh?"
    SCRIPT="tell application \"System Events\"
      set p to first process whose unix id is $PID
      set out to \"\"
      try
        set fw to value of attribute \"AXFocusedWindow\" of p
        set out to out & \"window: \" & (name of fw) & linefeed
      end try
      try
        set fe to value of attribute \"AXFocusedUIElement\" of p
        set out to out & \"role: \" & (value of attribute \"AXRole\" of fe) & linefeed
        try
          set out to out & \"title: \" & (value of attribute \"AXTitle\" of fe) & linefeed
        end try
        try
          set out to out & \"description: \" & (value of attribute \"AXDescription\" of fe) & linefeed
        end try
        try
          set out to out & \"value: \" & (value of attribute \"AXValue\" of fe) & linefeed
        end try
      end try
      return out
    end tell"
    ;;
  --windows)
    [ -n "$PID" ] || die "no app pid recorded for $VM"
    SCRIPT="tell application \"System Events\"
      set p to first process whose unix id is $PID
      set out to \"\"
      repeat with w in windows of p
        -- AppleScript renders a list as a string by CONCATENATING it, so
        -- {1024, 700} becomes \"1024700\" and a 1024x700 window is
        -- indistinguishable from a 102x4700 one. The items are pulled out and
        -- punctuated by hand for that reason. (\`tree\` sidesteps this
        -- entirely by reading AXPosition/AXSize as numbers — see ax.py's header.)
        set sz to size of w
        set ps to position of w
        set out to out & (name of w) & \" \" & (item 1 of sz) & \"x\" & (item 2 of sz) & \" at (\" & (item 1 of ps) & \",\" & (item 2 of ps) & \")\" & linefeed
      end repeat
      return out
    end tell"
    ;;
  --key)
    CODE="${2:-}"; [ -n "$CODE" ] || die "--key needs a keycode"
    [ -n "$PID" ] || die "no app pid recorded for $VM"
    case "$CODE" in *[!0-9]*) die "keycode must be numeric" ;; esac
    SCRIPT="tell application \"System Events\"
      set frontProcess to first process whose frontmost is true
      if (unix id of frontProcess) is not $PID then error \"refusing to send a key to another process\"
      key code $CODE
    end tell"
    ;;
  "") die "nothing to run" ;;
  *) SCRIPT="$1" ;;
esac

# The script goes in over stdin, never interpolated into a remote command line:
# AppleScript is full of quotes and newlines and a shell would shred it.
ax_phase transport
printf '%s' "$SCRIPT" | ag "$REMOTE osascript -" 2>&1 || {
  rc=$?
  echo "[testvm] osascript failed. If the error mentions 'not allowed assistive access',"  >&2
  echo "         the Accessibility TCC grant did not take — re-run testvm/setup.sh --reprovision." >&2
  exit "$rc"
}
