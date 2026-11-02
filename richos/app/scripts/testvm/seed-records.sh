#!/bin/sh
# seed-records.sh — put N of a user's own records into a fixture home's loro corpus, on the host.
#
#   seed-records.sh <fixture home> <N> [company id]      (default company: northwind)
#
# WHY. `seed-team.sh` writes teammates and nothing else, and no committed tool wrote a user's RECORDS,
# so the home screen's picture of a user with few records (round 11.4 v2, "The Ghost", CEO 2026-11-02)
# could only be looked at in the browser preview's generated dataset, never in the real app. This is
# the real thing: N records through the engine's own writer (`loro/bin/loro-write.mjs append`), into
# the corpus `gui_boot_machine` provisioned at <fixture home>/RichOS/corpus, with the compiler the
# same provision installed. The home screen's `home_field_data` compiles that corpus; `objects` is
# what it draws, and this script prints what the app's own compile (`home_field.rs::field_argv`: the
# same flags, topic = the company id) surfaces, so the count is MEASURED, not assumed. It exits 1
# when that is not N.
#
# THE KINDS. One record in eight is a decision, one in eight a lesson, the rest facts, so the left
# column has something to say about decisions and lessons at every count. Every body names the
# company, which is what the compile's topic matches. Records are fictional and carry no names.
#
# Run it on a fixture home built by `cargo run -p richos-core --example gui_boot_machine -- <home>
# <loro checkout>` (that is what makes <home>/RichOS/corpus and the compiler); it refuses a home
# that has neither, and never touches anything outside <fixture home>. Re-running it with a larger N
# adds only the records that are missing.
set -eu

if [ "${1:-}" = "--one" ]; then
  # one record, run by the xargs below: --one <corpus> <tools> <company> <i>
  CORPUS="$2"; TOOLS="$3"; COMPANY="$4"; i="$5"
  case $((i % 8)) in
    1) kind=decision ;;
    5) kind=lesson ;;
    *) kind=fact ;;
  esac
  printf '%s record %s: the harbor master at berth %s orders chandlery on the %s account.' "$COMPANY" "$i" "$i" "$COMPANY" \
    | node "$TOOLS/bin/loro-write.mjs" append --corpus "$CORPUS" --id "rec-$COMPANY-$i" --kind "$kind" \
      --scope org-shared --partition "$COMPANY" --body-stdin --json > /dev/null
  exit 0
fi

HOME_DIR="${1:?usage: seed-records.sh <fixture home> <N> [company id]}"
N="${2:?usage: seed-records.sh <fixture home> <N> [company id]}"
COMPANY="${3:-northwind}"
case "$N" in '' | *[!0-9]*) echo "seed-records.sh: N must be a whole number, got '$N'" >&2; exit 2 ;; esac

CORPUS="$HOME_DIR/RichOS/corpus"
TOOLS="${RICHOS_LORO_DIR:-$HOME_DIR/Library/Application Support/RichOS/loro-tools}"
[ -d "$CORPUS/ceo" ] || { echo "seed-records.sh: $CORPUS is not a provisioned corpus (run gui_boot_machine on this home first)" >&2; exit 1; }
{ [ -f "$TOOLS/bin/loro-write.mjs" ] && [ -f "$TOOLS/bin/loro-context.mjs" ]; } || { echo "seed-records.sh: no loro compiler at $TOOLS" >&2; exit 1; }

if [ ! -d "$CORPUS/companies/$COMPANY" ]; then
  node "$TOOLS/bin/loro-write.mjs" create-company --corpus "$CORPUS" --id "$COMPANY" --name "Northwind Traders" > /dev/null
fi

START="$(date +%s)"
i=0
# four writers at a time: the writer serializes through its own lock and each id is fixed by its
# number, so the corpus is the same whichever process wins
while [ "$i" -lt "$N" ]; do
  i=$((i + 1))
  [ -e "$CORPUS/companies/$COMPANY/records/rec-$COMPANY-$i.md" ] || echo "$i"
done | xargs -P 4 -I '{}' "$0" --one "$CORPUS" "$TOOLS" "$COMPANY" '{}'
echo "records written in $(($(date +%s) - START)) s"

# What the app's own compile surfaces: the same flags as `home_field.rs::field_argv`.
COUNT="$(printf '%s' "$COMPANY" | node "$TOOLS/bin/loro-context.mjs" compile --corpus "$CORPUS" --topic-stdin --company all \
  --budget-chars 2000000 --max-items 5000 --audience rich --format json \
  | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["items"]))')"
echo "objects the app's compile surfaces: $COUNT"
[ "$COUNT" -eq "$N" ] || { echo "seed-records.sh: asked for $N records and the compile surfaces $COUNT" >&2; exit 1; }
