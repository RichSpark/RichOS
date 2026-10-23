#!/usr/bin/env bash
#
# merge-verification-pins.test.sh — two branches that each renewed verification pins merge by
# themselves (merge-verification-pins.py, registered by richos/app/scripts/autocheck/install.sh).
#
# WHY (2026-10-05). Every branch that changes a pinned reader renews its sha256 pin in
# richos/engine/scripts/lib/verification-dependencies.json or
# docs/development/verification-input-qualifications.json, so any two branches built in
# parallel conflicted there; four merges stopped on it in one night, and each time an agent took
# one side and re-ran renew-verification-pins.py by hand (a64668554, ccc3ff319).
#
# In a private repository laid out like this one, with both maps, three readers and the driver
# registered by the real install.sh:
#   V1  a branch that renewed the pins of a.sh and c.sh and added a node merges into a main that
#       renewed b.sh and c.sh and added another: no conflict, both new nodes kept, and every pin
#       in both maps equals the sha256 of the merged file on disk, including c.sh, which both
#       sides changed and git merged by content (neither side's pin is right for it).
#   V2  a real conflict (a non-pin value both sides changed differently) is still a conflict,
#       with git's markers: the driver never decides between two reviews.
#   V4  two renewals that each changed one reader, renewed its pin, added an edge and appended a
#       sentence to its evidence note merge by themselves, both sentences and both edges kept:
#       the exact entry that stopped the folderfiles2 land on 2026-10-22.
#   V5  a note one side shortened while the other appended to it is still a conflict.
#   V3  install.sh --check names the driver, and --uninstall removes it.
#
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPO_ROOT="$(cd "$ENGINE_ROOT/../.." && pwd)"
INSTALL="$REPO_ROOT/richos/app/scripts/autocheck/install.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2"; FAIL=$((FAIL + 1)); }

SANDBOX="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/merge-pins-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT
# The fixture's git sees no operator configuration: no global hooks path, no identity guard.
export GIT_CONFIG_GLOBAL="$SANDBOX/gitconfig" GIT_CONFIG_NOSYSTEM=1
printf '[user]\n\tname = fixture\n\temail = fixture@example.invalid\n[init]\n\tdefaultBranch = main\n' > "$GIT_CONFIG_GLOBAL"
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE

echo "=== merge-verification-pins tests ==="

R="$SANDBOX/repo"
mkdir -p "$R/richos/engine/scripts/lib" "$R/docs/development" "$R/richos/app/scripts/autocheck"
cp "$SCRIPT_DIR/merge-verification-pins.py" "$R/richos/engine/scripts/lib/"
cp "$SCRIPT_DIR/.gitattributes" "$R/richos/engine/scripts/lib/.gitattributes"
cp "$REPO_ROOT/docs/development/.gitattributes" "$R/docs/development/.gitattributes"
cp "$INSTALL" "$REPO_ROOT/richos/app/scripts/autocheck/shim.sh" "$R/richos/app/scripts/autocheck/"
for f in a b c; do
    printf '#!/usr/bin/env bash\n# reader %s\nline one\nline two\nline three\nline four\nline five\n' "$f" \
        > "$R/richos/engine/scripts/$f.sh"
done

# write_maps — set every pin in both maps to the bytes on disk (renew-verification-pins.py's
# result), adding the nodes named on the command line.
write_maps() {
    python3 - "$R" "$@" <<'PY'
import hashlib, json, os, sys
root, extra = sys.argv[1], sys.argv[2:]
def sha(rel):
    return hashlib.sha256(open(os.path.join(root, "richos/engine", rel), "rb").read()).hexdigest()
dep_path = os.path.join(root, "richos/engine/scripts/lib/verification-dependencies.json")
qual_path = os.path.join(root, "docs/development/verification-input-qualifications.json")
dep = json.load(open(dep_path)) if os.path.exists(dep_path) else {
    "schema": 1, "nodes": {}, "hook_readers": {"scripts/x.test.sh": {"evidence": "fixture", "sources": {}}}}
qual = json.load(open(qual_path)) if os.path.exists(qual_path) else {"schema": 1, "units": {"u1": {"review": "fixture", "sources": {}}}}
for rel in ["scripts/a.sh", "scripts/b.sh", "scripts/c.sh"] + extra:
    dep["nodes"].setdefault(rel, {"source": rel, "evidence": "fixture"})
for row in dep["nodes"].values():
    row["sha256"] = sha(row["source"])
for rel in ("scripts/a.sh", "scripts/b.sh", "scripts/c.sh"):
    dep["hook_readers"]["scripts/x.test.sh"]["sources"][rel] = sha(rel)
    qual["units"]["u1"]["sources"]["richos/engine/" + rel] = sha(rel)
for path, doc in ((dep_path, dep), (qual_path, qual)):
    open(path, "w").write(json.dumps(doc, indent=2) + "\n")
PY
}
write_maps
git -C "$R" init -q
git -C "$R" add -A && git -C "$R" commit -qm base
( cd "$R" && bash richos/app/scripts/autocheck/install.sh >"$SANDBOX/install.out" 2>&1 )

# --- V1 --------------------------------------------------------------------
git -C "$R" switch -q -c left
sed -i.bak 's/^line one$/line one, left/' "$R/richos/engine/scripts/a.sh"
sed -i.bak 's/^line one$/line one, left/' "$R/richos/engine/scripts/c.sh"
printf '#!/usr/bin/env bash\n# reader d\n' > "$R/richos/engine/scripts/d.sh"
rm -f "$R"/richos/engine/scripts/*.bak
write_maps scripts/d.sh
git -C "$R" add -A && git -C "$R" commit -qm "left: a.sh, c.sh, d.sh"
git -C "$R" switch -q main
sed -i.bak 's/^line two$/line two, main/' "$R/richos/engine/scripts/b.sh"
sed -i.bak 's/^line five$/line five, main/' "$R/richos/engine/scripts/c.sh"
printf '#!/usr/bin/env bash\n# reader e\n' > "$R/richos/engine/scripts/e.sh"
rm -f "$R"/richos/engine/scripts/*.bak
write_maps scripts/e.sh
git -C "$R" add -A && git -C "$R" commit -qm "main: b.sh, c.sh, e.sh"
( cd "$R" && git merge --no-ff --no-edit left ) >"$SANDBOX/v1.out" 2>&1
V1RC=$?
STALE="$(python3 - "$R" <<'PY'
import hashlib, json, os, sys
root = sys.argv[1]
def sha(rel):
    path = os.path.join(root, rel if rel.startswith("richos/") else "richos/engine/" + rel)
    return hashlib.sha256(open(path, "rb").read()).hexdigest()
dep = json.load(open(os.path.join(root, "richos/engine/scripts/lib/verification-dependencies.json")))
qual = json.load(open(os.path.join(root, "docs/development/verification-input-qualifications.json")))
stale = [r["source"] for r in dep["nodes"].values() if r["sha256"] != sha(r["source"])]
stale += ["hook_readers:" + k for k, v in dep["hook_readers"]["scripts/x.test.sh"]["sources"].items() if v != sha(k)]
stale += ["qual:" + k for k, v in qual["units"]["u1"]["sources"].items() if v != sha(k)]
missing = [n for n in ("scripts/d.sh", "scripts/e.sh") if n not in dep["nodes"]]
print(" ".join(stale + ["missing:" + m for m in missing]))
PY
)"
C_BOTH="$(grep -c 'left\|main' "$R/richos/engine/scripts/c.sh")"
if [ "$V1RC" = 0 ] && [ -z "$STALE" ] && [ "$C_BOTH" = 2 ]; then
    ok "V1  parallel pin renewals merge by themselves: both new nodes kept, every pin equals the merged file, c.sh (merged by content) included"
else
    bad "V1  merge rc=$V1RC stale/missing=[$STALE] c.sh-sides=$C_BOTH" "$(tail -5 "$SANDBOX/v1.out" | tr '\n' ' ')"
fi

# --- V2 --------------------------------------------------------------------
git -C "$R" switch -q -c left2
python3 - "$R/richos/engine/scripts/lib/verification-dependencies.json" left <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p)); d["hook_readers"]["scripts/x.test.sh"]["evidence"] = "reviewed by " + sys.argv[2]
open(p, "w").write(json.dumps(d, indent=2) + "\n")
PY
git -C "$R" commit -qam "left2: a review"
git -C "$R" switch -q main
python3 - "$R/richos/engine/scripts/lib/verification-dependencies.json" main <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p)); d["hook_readers"]["scripts/x.test.sh"]["evidence"] = "reviewed by " + sys.argv[2]
open(p, "w").write(json.dumps(d, indent=2) + "\n")
PY
git -C "$R" commit -qam "main: another review"
( cd "$R" && git merge --no-ff --no-edit left2 ) >"$SANDBOX/v2.out" 2>&1
V2RC=$?
if [ "$V2RC" != 0 ] && grep -q '^<<<<<<<' "$R/richos/engine/scripts/lib/verification-dependencies.json"; then
    ok "V2  a non-pin value both sides changed differently is still a conflict, with git's markers"
else
    bad "V2  merge rc=$V2RC — two reviews were decided between without a person" "$(tail -3 "$SANDBOX/v2.out" | tr '\n' ' ')"
fi
git -C "$R" merge --abort >/dev/null 2>&1

# --- V4 --------------------------------------------------------------------
# The conflict that stopped the land of cc/zach-opus-folderfiles2 on 2026-10-22, entry
# nodes/mega-lander/app.py#land-lock-fixtures: each branch changed the pinned reader, renewed its
# pin, added an edge and appended its own renewal sentence to the same evidence note. The three
# notes and edge lists below are that entry's merge base, ours and theirs, verbatim.
note() {
    python3 - "$R/richos/engine/scripts/lib/verification-dependencies.json" "$1" <<'PY'
import json, sys
BASE = ("Fixture calls only land_lock and _live_fence_lease. Full local closure reviewed: lock metadata, flock, process liveness and repository lease/history reads. Workspaces and ECS imports are bound; ECS runtime operations are not invoked. Renewed 2026-10-15 (echo-opus-hire57): the team tool's description text only; nothing read changed. Renewed 2026-10-18: retired-execution reconciliation and two desktop dispatch cases changed elsewhere in these files; this selected fixture closure and setUp are unchanged.")
OURS = BASE + (" Renewed 2026-10-21 (zach-opus-folderfiles1): module load now also loads instruction_files.py by fixed path (edge added); prepare changed, outside this fixture closure.")
THEIRS = BASE + (" Renewed 2026-10-21 (zach-opus-landgate1, plan item 6): land_lock also calls _take_fence_lease and _release_fence_lease, which load scripts/lib/operator_fences.py by fixed path and, where the fixture repository's launcher has the fence on, write and remove the lease file in the launcher's home under the same flock. No configuration read is added. Renewed 2026-10-21 (zach-opus-landgate1, review rv-20261021T072931Z-a4453faa-ffff finding 1): where the fixture repository's launcher names a lease home other than this land's lock home (another Claude account's CLAUDE_CONFIG_DIR, RICHOS_LAND_LOCKS_DIR in the fixture), _take_fence_lease also takes the launcher's own lock file and reads its lease there, within RICHOS_LAND_LOCK_TIMEOUT; both homes are private to the fixture. No configuration key is added. Renewed 2026-10-21 (zach-opus-landgate1, review f79aeabf finding 1): land_merge's Git environment changed; this closure does not reach it.")
EDGES = [{"to": "mega-lander/workspaces.py#land-record-readers"}, {"to": "mega-lander/workspaces.py#liveness-readers"},
         {"to": "ecs/core/ecs_core.py#load"}, {"to": "ecs/adapters/app.py#load"}]
side = sys.argv[2]
p = sys.argv[1]; d = json.load(open(p)); row = d["nodes"]["scripts/a.sh"]
row["evidence"] = {"base": BASE, "ours": OURS, "theirs": THEIRS, "expected": OURS + THEIRS[len(BASE):]}[side]
row["edges"] = EDGES + {"base": [], "ours": [{"to": "scripts/lib/instruction_files.py"}],
                        "theirs": [{"to": "scripts/lib/operator_fences.py#runtime"}],
                        "expected": [{"to": "scripts/lib/instruction_files.py"}, {"to": "scripts/lib/operator_fences.py#runtime"}]}[side]
if side == "expected":
    print(json.dumps({"evidence": row["evidence"], "edges": row["edges"]}, sort_keys=True))
else:
    open(p, "w").write(json.dumps(d, indent=2) + "\n")
PY
}
git -C "$R" checkout -q -f main
note base
git -C "$R" commit -qam "main: the note both branches extend"
git -C "$R" switch -q -c left4
sed -i.bak 's/^line three$/line three, left4/' "$R/richos/engine/scripts/a.sh"
rm -f "$R"/richos/engine/scripts/*.bak
write_maps; note theirs
git -C "$R" commit -qam "left4: a.sh, its pin, an edge and a renewal sentence"
git -C "$R" switch -q main
sed -i.bak 's/^line five$/line five, main4/' "$R/richos/engine/scripts/a.sh"
rm -f "$R"/richos/engine/scripts/*.bak
write_maps; note ours
git -C "$R" commit -qam "main: a.sh, its pin, another edge and another renewal sentence"
# As on 2026-10-22, the checked-out side (main here, the folderfiles2 branch then) carries the
# "ours" note and the merged-in side the "theirs" note: ours first, then theirs' addition.
WANT="$(note expected)"
( cd "$R" && git merge --no-ff --no-edit left4 ) >"$SANDBOX/v4.out" 2>&1
V4RC=$?
GOT="$(python3 - "$R" 2>/dev/null <<'PY'
import hashlib, json, os, sys
root = sys.argv[1]
row = json.load(open(os.path.join(root, "richos/engine/scripts/lib/verification-dependencies.json")))["nodes"]["scripts/a.sh"]
fresh = row["sha256"] == hashlib.sha256(open(os.path.join(root, "richos/engine/scripts/a.sh"), "rb").read()).hexdigest()
print(json.dumps({"evidence": row["evidence"], "edges": row["edges"]}, sort_keys=True) if fresh else "stale pin")
PY
)" || GOT="not JSON (conflict markers left)"
if [ "$V4RC" = 0 ] && [ -n "$WANT" ] && [ "$GOT" = "$WANT" ]; then
    ok "V4  two renewals that each appended a sentence to one note merge by themselves: both sentences and both edges kept, the pin equals the merged file (the 2026-10-22 folderfiles2 conflict)"
else
    bad "V4  merge rc=$V4RC; note, edges and pin: ${GOT:0:60}" "$(tail -3 "$SANDBOX/v4.out" | tr '\n' ' ')"
fi
git -C "$R" merge --abort >/dev/null 2>&1

# --- V5 --------------------------------------------------------------------
# One side drops the note's last sentence while the other appends one (review
# rv-20261022T111705Z-e8050296-c669): never merged, or the dropped sentence would come back.
git -C "$R" checkout -q -f main
git -C "$R" switch -q -c left5
python3 - "$R/richos/engine/scripts/lib/verification-dependencies.json" <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p)); row = d["nodes"]["scripts/a.sh"]
row["evidence"] = row["evidence"] + " Renewed 2026-10-22 (left5): appended."
open(p, "w").write(json.dumps(d, indent=2) + "\n")
PY
git -C "$R" commit -qam "left5: appends a sentence"
git -C "$R" switch -q main
python3 - "$R/richos/engine/scripts/lib/verification-dependencies.json" <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p)); row = d["nodes"]["scripts/a.sh"]
row["evidence"] = row["evidence"].rsplit(" Renewed ", 1)[0]
open(p, "w").write(json.dumps(d, indent=2) + "\n")
PY
git -C "$R" commit -qam "main: drops the last sentence"
( cd "$R" && git merge --no-ff --no-edit left5 ) >"$SANDBOX/v5.out" 2>&1
V5RC=$?
if [ "$V5RC" != 0 ] && grep -q '^<<<<<<<' "$R/richos/engine/scripts/lib/verification-dependencies.json"; then
    ok "V5  a note one side shortened and the other extended is a conflict: the dropped sentence is not brought back"
else
    bad "V5  merge rc=$V5RC — a sentence one side deleted was merged back without a person" "$(tail -3 "$SANDBOX/v5.out" | tr '\n' ' ')"
fi
git -C "$R" merge --abort >/dev/null 2>&1

# --- V3 --------------------------------------------------------------------
CHECK="$(cd "$R" && bash richos/app/scripts/autocheck/install.sh --check 2>&1)"
( cd "$R" && bash richos/app/scripts/autocheck/install.sh --uninstall >/dev/null 2>&1 )
GONE="$(git -C "$R" config --get merge.richos-verification-pins.driver || true)"
if printf '%s' "$CHECK" | grep -q 'INSTALLED  merge.richos-verification-pins.driver' && [ -z "$GONE" ]; then
    ok "V3  install.sh --check names the driver, and --uninstall removes it"
else
    bad "V3  check: $(printf '%s' "$CHECK" | grep richos-verification-pins) left after uninstall: $GONE"
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "=== merge-verification-pins tests: all $PASS passed ==="
    exit 0
fi
echo "=== merge-verification-pins tests: $PASS passed, $FAIL FAILED ===" >&2
exit 1
