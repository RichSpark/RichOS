#!/usr/bin/env bash
#
# handoff-facts-annotate.sh — PostToolUse. THE RESTART NOTE CARRIES MEASUREMENTS BECAUSE THE
#                             MACHINE PUT THEM THERE, not because anybody remembered to.
#
# ===========================================================================
# WHY THIS IS A HOOK AND NOT AN INSTRUCTION
# ===========================================================================
# Type U (richos-hq docs/verification/lifecycle-failure-record-2026-09-13.md 10e): the note a
# session writes to save the next one time is produced at the moment of least capacity to
# verify and highest confidence that verification is unnecessary. `CLAUDE.md` already carries
# the rule that would have prevented it — a number carries the command that produced it or it
# carries the word `unverified` — and the rule was never pointed at the handoff, by nobody's
# exemption. A second written copy of a rule its own author breaks changes nothing.
#
# So the measurement is taken by whatever writes the note, at the moment of writing, with no
# step for anybody to skip. The session that wrote the defective note ran NONE of the commands
# that would have settled its three headline figures; every one of them was one command away.
#
# ===========================================================================
# WHAT IT DOES, AND THE TWO LINES IT WILL NOT CROSS
# ===========================================================================
#   1. IT NEVER EDITS A WORD THE AUTHOR WROTE. It appends one delimited section, and refreshes
#      that same section on a later write. Everything outside the delimiters is byte-identical.
#   2. IT NEVER BLOCKS AND NEVER FAILS A TOOL CALL. Exit 0 on every path. It fires after the
#      write has already happened; a fact it could not measure must not look like a failed
#      write of the note.
#
# IT WRITES INTO THE OPERATOR'S OWN MEMORY DIRECTORY, and that is said plainly rather than
# buried: `~/.claude/projects/<project>/memory/` belongs to the operator, this appends to a
# file there, the addition is delimited and removable, and deleting the block is a complete
# opt-out for that note.
#
# ===========================================================================
# WHAT COUNTS AS A RESTART NOTE — measured, not assumed
# ===========================================================================
# All fifteen session-handoff notes in the operator's memory directory on 2026-09-14 match
# `project_*restart*.md`, and no other file there does. The match is therefore on that shape,
# under a `memory/` directory, and nowhere else.
#
# THE FAST PATH IS ONE GREP. This is registered against every write-shaped tool, so the
# overwhelmingly common answer — "this call has nothing to do with a restart note" — is
# reached before python3 is started at all.
#
# NOTE: hooks are snapshotted at session start, so this is inert until the next session.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FACTS="$SCRIPT_DIR/../handoff-facts.py"

INPUT=""
IFS= read -r -t "${RICHOS_HOOK_STDIN_TIMEOUT:-3}" -d '' INPUT || true

# THE FAST PATH. A memory-directory restart note is named in this payload, or there is
# nothing here for this hook.
if ! printf '%s' "$INPUT" | grep -qE 'memory/[^"]*restart[^"]*\.md'; then
    exit 0
fi
[ -f "$FACTS" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

export HANDOFF_FACTS="$FACTS"
# Keep the payload out of argv and the environment; descriptor 3 carries its bytes, the same
# way every other payload-reading hook in this engine does it.
python3 - 3<<< "$INPUT" <<'PY'
import json
import os
import re
import subprocess
import sys

facts = os.environ.get("HANDOFF_FACTS", "")
try:
    with os.fdopen(3, "r", encoding="utf-8") as fh:
        payload = json.load(fh)
except Exception:
    sys.exit(0)

ti = payload.get("tool_input") or {}
blob = " ".join(str(v) for v in ti.values() if isinstance(v, (str, int, float)))

# Every path in this call that LOOKS like a restart note. A heredoc write puts the path in a
# command string, a Write tool call puts it in file_path, and both are read the same way —
# the session that produced the defect used the heredoc form.
NOTE = re.compile(r"(/[^\s\"'<>|]*?/memory/[^\s\"'<>|]*restart[^\s\"'<>|]*\.md)")
seen, notes = set(), []
for m in NOTE.finditer(blob):
    p = m.group(1)
    if p not in seen and os.path.isfile(p):
        seen.add(p)
        notes.append(p)
if not notes:
    sys.exit(0)

lines = []
for note in notes:
    try:
        r = subprocess.run([sys.executable, facts, note, "--in-place"],
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True, timeout=25)
        out = (r.stdout or "").strip().split("\n")[-1]
    except subprocess.TimeoutExpired:
        out = "handoff-facts: measurement timed out; %s carries NO measured block" % note
    except Exception as e:                       # never a failed tool call, whatever happens
        out = "handoff-facts: %s (%s)" % (e, note)
    lines.append(out)

# The operator is told, because a file changing under a person with no word said about it is
# its own kind of failure. The MODEL is deliberately not told: the value of this block is that
# the NEXT session reads it, and a lead who is told "it is handled" is a lead who stops reading
# it — which is the attention-with-extra-steps shape this mechanism exists to avoid.
print(json.dumps({"systemMessage": "\n".join(lines)}))
PY
exit 0
