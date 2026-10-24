#!/usr/bin/env bash
#
# workspaces.sh — the CEO's workspace spec, as the one command Rich runs.
#
# The page: docs/plans/worktree-spec-2026-09-11.md. The mechanism:
# mega-lander/workspaces.py. This file only finds python3 and the library.
#
#   workspaces.sh status
#       every finished agent this session must land or discard (point 5),
#       every paused and working agent, and every deletion being retried
#   workspaces.sh land <agent> [--ignored-not-needed '<why>']
#       its branches are already in main and nothing is uncommitted (point 8):
#       stop its processes and delete every workspace and branch (points 4, 9, 10)
#   workspaces.sh merge <agent> [-m '<merge message>']
#       THE LAND COMMAND: merge every branch of a finished teammate that is not
#       yet in its integration branch (git merge --no-ff, in that repository's
#       main checkout, with git's own checks), then land it as above. Ignored
#       files it left are kept under <state>/kept/, never a reason to stay.
#       In a repository listed in SECOND_REVIEW_REPOS it REFUSES, merging
#       nothing, unless a passing second review of exactly each tip exists
#       (CEO §113); the merge message names that review. A branch that
#       changes only release notes needs no review (CEO 2026-10-23).
#   workspaces.sh discard <agent> --reason '<why>' (--ceo-word '<his words>' | --not-ceo-ordered '<why>')
#       work that must not go into main: every workspace and branch deleted,
#       the reason and the branch tips recorded (point 7)
#   workspaces.sh pause <agent> --until '<what ends it>'   /  resume <agent>  /  stop <agent>
#       point 11. A SendMessage carrying a line `pause-until: <what ends it>`
#       records the same pause; a later message to the paused agent resumes it.
#   workspaces.sh wait <agent> --started '<what>' | --outside '<what>' --todo '<ref>' | --ceo '<question>' --todo '<ref>'
#       point 5's allowances for ending a turn while an item waits
#   workspaces.sh retry
#       run every due deletion retry now (point 13 retries on its own anyway)
#   workspaces.sh withdraw-cc --name <agent> [--why '<why>']
#       point 3's other half: a workspace registered and created for a spawn
#       that then COULD NOT BE MADE is a registration for work that will not
#       happen. Its workspaces, branches and record go, and the name is free
#       again. It REFUSES anything that ever ran - that is what land and
#       discard are for. scripts/spawn.sh calls it to roll itself back.
#
# THESE ARE THE ONLY THINGS IN THE ENGINE THAT DELETE A WORKSPACE.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/workspaces.py"
command -v python3 >/dev/null 2>&1 || { echo "workspaces.sh: python3 is required" >&2; exit 2; }
[ -f "$LIB" ] || { echo "workspaces.sh: $LIB is missing" >&2; exit 2; }
exec python3 "$LIB" "$@"
