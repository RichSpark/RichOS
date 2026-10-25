#!/usr/bin/env bash
#
# hook-registration-completeness.sh — THE PREDICATE: is a newly registered hook
#                                      named in every inventory that has to
#                                      carry it?
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
# On 2026-09-14 CI went red on FIVE units at once, on `main`, and every one of
# them was self-inflicted by the same night's landings. Three hooks landed
# within two hours — handoff-facts-annotate.sh, notice-protected-ref-moves.sh,
# ref-transaction-forensics.sh. Each was correctly registered in
# hooks/hooks.json. Each was missing from a DIFFERENT subset of the other
# inventories that other suites assert against. Three competent engineers hit
# it independently the same night. The record is
# richos-hq/docs/verification/lifecycle-failure-record-2026-09-13.md §10h,
# type X.
#
# The reason all three got it wrong is not carelessness and it is worth being
# precise about, because it decides where the fix has to sit:
#
#   THE REGISTRATION WORKS THE MOMENT hooks.json HAS IT. The hook loads, fires,
#   and does its job. NOTHING IS BROKEN FROM THE AUTHOR'S SEAT. The inventories
#   are assertions made by OTHER suites about a set the author never sees, and
#   the only thing that tells anybody is CI — after the land, on `main`.
#
# So the check has to fire when the registration is WRITTEN. That is
# scripts/hooks/guard-hook-registration-commits.sh; this file is the predicate
# it runs, kept separate for the reason guard-completeness-commits.sh keeps
# its own: ONE PREDICATE, MANY CALLERS. A hook-shaped reimplementation beside a
# CI-shaped one is the defect class this engine keeps finding in itself.
#
# ===========================================================================
# THE INVENTORY LIST IS DERIVED, NOT TYPED — AND THAT IS THE WHOLE DESIGN
# ===========================================================================
# The obvious implementation is a list: "add your hook to these five files".
# It was rejected, and the reason is the failure record's own sentence:
#
#   "satisfying one requirement means editing an unknown number of unrelated
#    files, and NOTHING ENUMERATES THEM."
#
# A typed list in here would be an N+1th copy of that unknown number, drifting
# the day somebody adds a sixth inventory — which is the SAME defect one level
# up, and this engine has already watched a typed guard count go stale twice in
# two days (13/13 and 14/14 over a wired, firing guard). See
# scripts/lib/registered-hooks.sh for that story.
#
# So the inventories are derived from the tree, by UNANIMITY:
#
#   A FILE THAT NAMES EVERY ONE OF THE OTHER REGISTERED HOOKS IS, BY
#   CONSTRUCTION, AN INVENTORY OF REGISTERED HOOKS.
#
# That predicate is binary — a basename is in a file or it is not — and it
# needs no threshold, no reading of intent, and no guess about what a file is
# for. Measured on richos @ f5babcb7, 68 registered hook scripts, 674 tracked
# files in engine/:
#
#   68/68  hooks/hooks.json                          <- the registration itself
#   68/68  .claude/settings.local.json               <- hook-staleness case 11
#   68/68  scripts/hooks/contract-integrity-probe.sh <- by-reference BR2
#   68/68  scripts/hooks/engine-status.test.sh       <- engine-status case 1b
#
# OF THOSE FOUR, ONLY TWO ARE STILL TYPED BY A HUMAN. The first is the
# registration, which is the trigger rather than a demand. The second is
# GENERATED from the first by install.sh since 2026-09-15, so its demand is
# satisfied by running a command; the reader stays because the generated file
# is committed and a commit made without running the generator still reaches
# `main` with the surfaces disagreeing. The remaining two are typed on purpose
# and deriving them is a different piece of work (contract-integrity-probe.sh
# L393-402 argues why BR_EXPECTED must NOT be derived: a table derived from the
# thing it checks asserts nothing). Anyone reading this to size a retirement of
# the guard above should start there and not here.
#   ----------------------------------------- unanimity cliff -----------
#   41/68  scripts/hooks/contract-integrity.test.sh
#   41/68  README.md
#   33/68  CHANGELOG.md
#   20/68  scripts/hooks/install.sh
#
# Four files reach unanimity and the next candidate is 27 hits below them. The
# derivation reproduces, with no typed list, exactly the four places a hand
# audit found — and it reproduces the two NEGATIVE findings as well, which is
# the better evidence:
#
#   * README.md is NOT an inventory. Its table is per-SYSTEM, not per-hook, so
#     it lands at 41/68 and is never demanded. Asserting that by hand was a
#     call somebody made; here it is a measurement.
#   * install.sh is NOT an inventory. Its hashed set used to be sixteen typed
#     paths and is now DERIVED from hooks.json (see its own header: "wiring a
#     seventeenth guard mints its sidecar with no edit here"), so it names only
#     the 20 hooks its prose happens to discuss, and demanding membership would
#     be demanding a pointless edit.
#
# When a sixth inventory is added, this predicate picks it up on the run after
# the one that made it unanimous. Nothing here has to be told.
#
# ===========================================================================
# WHAT IS STILL TYPED, AND WHY THAT IS SAFE
# ===========================================================================
# Unanimity is a FILE-level test, and two of the four inventories hold their
# membership in a specific STRUCTURE inside the file. A hook named only in a
# prose comment in contract-integrity-probe.sh would satisfy a substring test
# and leave BR2 red — a green tick over the exact failure this exists to catch.
#
# So three structural readers are typed (the BR_EXPECTED table, the
# ACKNOWLEDGED_SCRIPTS heredoc, the settings.local.json command list), and the
# typing is made safe by being CHECKED AGAINST THE DERIVATION: every typed
# reader must name a file the unanimity pass independently found. If one of
# them ever stops being an inventory, this predicate says so and fails rather
# than going on quietly asserting against a file nobody enforces any more. A
# typed list that cannot notice its own staleness is the thing being replaced;
# a typed list that is re-proved against a derivation on every run is not.
#
# A file that reaches unanimity and has NO structural reader gets the substring
# test. That is the weaker check, and it is the right weaker check: it is what
# can be said about a file whose shape is not known, and it still catches the
# whole of the failure class — a name that is nowhere in the file.
#
# ===========================================================================
# THE CONDITIONAL INVENTORIES, AND WHY GETTING THIS WRONG IS WORSE THAN NOTHING
# ===========================================================================
# Two inventories are conditional on a property of the hook, and demanding them
# unconditionally would force an engineer to WRITE SOMETHING FALSE:
#
#   R_ROOTLESS_HOOKS (contract-integrity-probe.sh Layer R) — only for a hook
#     that does NOT source scripts/lib/resolve-roots.sh. INVERTED 2026-09-14
#     with Layer R itself: that layer used to hold a typed R_ROOTED_HOOKS and
#     walk only the hooks it named, which let five rooted hooks accumulate
#     unchecked, one of them already carrying a divergent bootstrap. It now
#     DERIVES the hooks it walks from hooks/hooks.json, so a rooted hook owes
#     this file nothing and a rootless one must declare itself or fail R2 for
#     not sourcing a library it has no reason to source. The condition is still
#     tested with the SAME grep Layer R itself uses.
#
#     handoff-facts-annotate.sh is a live instance. notice-protected-ref-moves.sh
#     was named here as a second one and that was WRONG — measured 2026-09-14, it
#     both sources resolve-roots.sh and assigns ENGINE_ROOT, and it has been in
#     the rooted set the whole time. The same sentence is repeated in
#     engine-status.test.sh's comment above ACKNOWLEDGED_SCRIPTS; it is wrong
#     there too.
#
# That leaves ONE conditional inventory. There were two until 2026-09-14:
#
#   CANONICAL_AGENT_CHAIN (contract-integrity-probe.sh Layer C) — GONE, and its
#     removal is the reason this paragraph is worth reading. It was an ORDERED
#     typed chain that an Agent-matcher hook had to be added to by hand, in the
#     right position, and this file dutifully DEMANDED it: the ninth Agent hook's
#     report named it as one of four owed places. A demand is the right answer
#     to a list that must exist. It is the wrong answer to a list that should
#     not. That chain is now derived from hooks/hooks.json through
#     scripts/lib/registered-hooks.sh, so the registration IS the entry and
#     there is nothing left to owe — the only demand this file can make of a
#     tenth Agent hook is the one it makes of every hook.
#
# The surviving condition is read off the artifacts (the hook's own source),
# never off intent.
#
# ===========================================================================
# WHAT THIS DELIBERATELY DOES NOT CHECK
# ===========================================================================
# REMOVALS. A hook that leaves hooks.json has to leave the same inventories,
# and that is genuinely half of this rule — but the only available test, "no
# inventory still names it", has a real false-positive class: this engine's
# inventories carry PROSE that names removed hooks on purpose.
# engine-status.test.sh's own comment names record-subagent-start.sh,
# terminalize-agent-worktrees.sh, session-start-reap-worktrees.sh and
# notice-land-disposition.sh, all four removed on 2026-09-11, all four
# correctly still written down. A blocking check would refuse that comment.
# Since the whole argument for blocking here is that the predicate is
# unambiguous with no prose false-positives, the removal half does not qualify
# and is left to engine-status case 1b, which catches it on the next CI run.
# This is disclosed rather than quietly scoped: the rule is enforced in one
# direction and that direction is named.
#
# LAYER M's double-registration list (CANON) and README.md's guard table. No
# suite asserts membership of either, so demanding them would be enforcing a
# preference while calling it a contract. They are printed as ADVICE when a
# registration adds a Bash-matcher hook, and they never block.
#
# A NEW INVENTORY'S FIRST HOOK. A file becomes unanimous only once it names
# every registered hook; the commit that CREATES an inventory is therefore not
# checked against itself. That is correct and not a gap: until it is unanimous
# it is not yet an inventory of anything.
#
# ===========================================================================
# USAGE
# ===========================================================================
#   hook-registration-completeness.sh --root <repo-root> [--baseline <rev>]
#                                     [--explain]
#
#   --root      the repository to audit. The engine may be the repository
#               itself or a subdirectory of it (richos/engine); both are found.
#   --baseline  the rev the registration surface is compared against
#               (default HEAD). The SUBJECTS are the hooks registered in the
#               about-to-exist tree and not in the baseline.
#   --explain   also print what was checked and found complete.
#
# Exit codes:
#   0  nothing newly registered, or everything newly registered is complete
#   1  INCOMPLETE — one or more inventories do not name a new hook
#   2  BROKEN, or NOT APPLICABLE (prints "NOT APPLICABLE" in that case)

set -eo pipefail

usage() { sed -n '/^# USAGE/,/^#   2  BROKEN/p' "$0" | sed 's/^# \{0,1\}//'; }

ROOT=""
BASELINE="HEAD"
EXPLAIN=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --root)     ROOT="${2:-}"; shift 2 ;;
        --baseline) BASELINE="${2:-}"; shift 2 ;;
        --explain)  EXPLAIN=1; shift ;;
        -h|--help)  usage; exit 0 ;;
        *) echo "ERROR: hook-registration-completeness.sh: unrecognized argument '$1'" >&2; exit 2 ;;
    esac
done

[ -n "$ROOT" ] || { echo "ERROR: hook-registration-completeness.sh: --root is required" >&2; exit 2; }
[ -d "$ROOT" ] || { echo "ERROR: hook-registration-completeness.sh: --root is not a directory: $ROOT" >&2; exit 2; }

command -v python3 >/dev/null 2>&1 || { echo "ERROR: hook-registration-completeness.sh: python3 is required — refusing (fail-closed)" >&2; exit 2; }

# --- WHERE IS THE ENGINE? --------------------------------------------------
# The engine is either the repository (an engine checkout) or a subdirectory of
# it (richos/engine). A repository with neither registers no hooks and is NOT
# APPLICABLE — which is every repository on this machine but one, at the cost
# of two file tests.
ENGINE_DIR=""
for _c in "$ROOT/richos/engine" "$ROOT/engine" "$ROOT"; do
    if [ -f "$_c/hooks/hooks.json" ] && [ -f "$_c/scripts/lib/registered-hooks.sh" ]; then
        ENGINE_DIR="$_c"; break
    fi
done
if [ -z "$ENGINE_DIR" ]; then
    echo "NOT APPLICABLE: $ROOT registers no hooks (no hooks/hooks.json beside scripts/lib/registered-hooks.sh)"
    exit 2
fi

exec python3 - "$ROOT" "$ENGINE_DIR" "$BASELINE" "$EXPLAIN" <<'PYEOF'
import json, os, re, subprocess, sys

ROOT, ENGINE_DIR, BASELINE, EXPLAIN = sys.argv[1:5]
EXPLAIN = EXPLAIN == "1"
ENG_REL = os.path.relpath(ENGINE_DIR, ROOT)
ENG_REL = "" if ENG_REL == "." else ENG_REL + "/"


def die_broken(msg):
    sys.stdout.write("BROKEN: %s\n" % msg)
    raise SystemExit(2)


def git(*args):
    """Run git in ROOT; return (rc, stdout). Never raises on a non-zero rc."""
    p = subprocess.run(("git",) + args, cwd=ROOT, capture_output=True, text=True)
    return p.returncode, p.stdout


# --- THE REGISTRATION SURFACE, PARSED THE WAY THE HOST READS IT ------------
# basename -> set of events. The matcher is kept alongside because the Agent
# chain condition is a property of the matcher, not of the event.
def parse_hooks_json(text, where):
    try:
        data = json.loads(text)
    except Exception as e:
        die_broken("could not parse %s: %s" % (where, e))
    if not isinstance(data, dict):
        die_broken("%s is not a JSON object" % where)
    out = {}
    for event, entries in (data.get("hooks") or {}).items():
        if not isinstance(entries, list):
            continue
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            matcher = entry.get("matcher", "") or ""
            for h in (entry.get("hooks") or []):
                cmd = (h or {}).get("command", "") or ""
                if not cmd:
                    continue
                # "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/x.sh" and
                # '"$CLAUDE_PROJECT_DIR/scripts/hooks/x.sh"' both reduce here.
                m = re.findall(r"[A-Za-z0-9._-]+\.(?:sh|py)", cmd)
                if not m:
                    continue
                base = m[-1]
                rec = out.setdefault(base, {"events": set(), "matchers": set()})
                rec["events"].add(event)
                rec["matchers"].add(matcher)
    return out


HJ_REL = ENG_REL + "hooks/hooks.json"
HJ_ABS = os.path.join(ENGINE_DIR, "hooks/hooks.json")

# THE TREE THAT IS ABOUT TO EXIST. Read the WORKTREE bytes, not the index and
# not HEAD, for guard-completeness-commits.sh's reason: a fix made with an edit
# has to count immediately, or the guard is one you cannot commit your way out
# of. Over-inclusive by construction (an unstaged registration is judged too);
# over-inclusive can cost an argument, never coverage.
try:
    with open(HJ_ABS, "r", encoding="utf-8") as f:
        cand_text = f.read()
except Exception as e:
    die_broken("could not read %s: %s" % (HJ_ABS, e))
CAND = parse_hooks_json(cand_text, HJ_ABS)

rc, base_text = git("show", "%s:%s" % (BASELINE, HJ_REL))
if rc != 0:
    # No baseline (a first commit, an unborn branch, a rev that has no such
    # path). Everything registered would read as NEW, which would refuse the
    # initial import of the whole engine. Nothing is known about what CHANGED,
    # so nothing is asserted.
    print("NOT APPLICABLE: %s:%s does not resolve — no baseline to diff the "
          "registration surface against" % (BASELINE, HJ_REL))
    raise SystemExit(2)
BASE = parse_hooks_json(base_text, "%s:%s" % (BASELINE, HJ_REL))

NEW_REGISTERED = sorted(set(CAND) - set(BASE))
PEERS = sorted(set(CAND) & set(BASE))

# --- NEW HOOK SCRIPTS THAT ARE NOT REGISTERED AT ALL -----------------------
# ci-affected-units case A5 walks EVERY *.sh in scripts/hooks/, registered or
# not, and refuses a push whose diff selects no suite. The sharpest instance in
# the failure record is exactly this shape and a hooks.json-only trigger would
# MISS it: ref-transaction-forensics.sh is a git reference-transaction hook, it
# appears nowhere in hooks.json, and it landed named by no suite at all — "the
# thing watching the refs had nothing watching it". Re-derived here rather than
# taken on trust: `grep -c ref-transaction-forensics hooks/hooks.json` is 0.
HOOKS_DIR_REL = ENG_REL + "scripts/hooks"
rc, base_tree = git("ls-tree", "-r", "--name-only", BASELINE, "--", HOOKS_DIR_REL)
base_scripts = set()
if rc == 0:
    for line in base_tree.splitlines():
        b = os.path.basename(line.strip())
        if b.endswith(".sh") and not b.endswith(".test.sh"):
            base_scripts.add(b)

cand_scripts = set()
hooks_dir_abs = os.path.join(ENGINE_DIR, "scripts/hooks")
if os.path.isdir(hooks_dir_abs):
    for b in os.listdir(hooks_dir_abs):
        if b.endswith(".sh") and not b.endswith(".test.sh"):
            cand_scripts.add(b)
NEW_SCRIPTS = sorted(cand_scripts - base_scripts)

SUBJECTS = sorted(set(NEW_REGISTERED) | set(NEW_SCRIPTS))
if not SUBJECTS:
    if EXPLAIN:
        print("COMPLETE: no hook was newly registered and no new hook script "
              "appeared against %s — nothing to check." % BASELINE)
    raise SystemExit(0)

if not PEERS:
    die_broken("the baseline registers no hooks that survive into the candidate "
               "tree, so there is no peer set to derive the inventories from")

# --- DERIVE THE INVENTORIES -----------------------------------------------
# A file that names EVERY peer is an inventory of registered hooks. The
# candidate set comes from one grep for a single peer (unanimity requires all
# of them, so any one is a sound prefilter) and is then verified in full.
PROBE_PEER = PEERS[0]
rc, grep_out = git("grep", "-lF", PROBE_PEER, "--", ENG_REL or ".")
if rc not in (0, 1):
    die_broken("`git grep` failed while looking for the inventory candidates "
               "(rc %d)" % rc)
candidates = [p.strip() for p in grep_out.splitlines() if p.strip()]

# `git grep` SEES ONLY TRACKED FILES, and one of the four inventories is a
# `.claude/` path that a global gitignore on this machine excludes by default
# (probe Layer N exists for exactly that stranding trap). The typed reader
# paths are therefore added to the candidate set directly — they are still put
# through the same unanimity test as everything else, so this widens what is
# LOOKED AT and changes nothing about what COUNTS. Found by this file's own
# suite, whose fixture repository tracked eight files and not the ninth.
for _typed in (".claude/settings.local.json",
               "scripts/hooks/contract-integrity-probe.sh",
               "scripts/hooks/engine-status.test.sh"):
    rel = ENG_REL + _typed
    if rel not in candidates and os.path.exists(os.path.join(ROOT, rel)):
        candidates.append(rel)

# The subjects' OWN files are never inventories of themselves.
subject_files = set()
for s in SUBJECTS:
    stem = s[:-3]
    subject_files.add(ENG_REL + "scripts/hooks/" + s)
    subject_files.add(ENG_REL + "scripts/hooks/" + stem + ".test.sh")

file_cache = {}


def read_file(rel):
    if rel in file_cache:
        return file_cache[rel]
    p = os.path.join(ROOT, rel)
    try:
        with open(p, "rb") as f:
            b = f.read()
        t = "" if b"\0" in b[:4096] else b.decode("utf-8", "replace")
    except Exception:
        t = ""
    file_cache[rel] = t
    return t


# A DATED RECORD IS NEVER AN INVENTORY. A file under a docs/ directory whose
# name carries a YYYY-MM-DD date (an audit, a verification record, a review)
# describes the hooks that existed on that day, and it keeps their names
# (the 2026-10-24 lead's order for the dialect-guard.sh rename). Read as an
# inventory, one such file named every hook and so demanded every new or
# renamed hook be written into a frozen record, or a hook-inventory-ack at
# every registration change. Derived, like everything else here: no list of
# exempt files to keep, and the four real inventories carry no date.
DATED_RECORD = re.compile(r"(^|/)docs/(.+/)?[^/]*\d{4}-\d{2}-\d{2}[^/]*$")

INVENTORIES = []
for rel in candidates:
    if rel in subject_files:
        continue
    if DATED_RECORD.search(rel):
        continue
    if rel == HJ_REL:          # place #1: the registration itself is the trigger
        continue
    t = read_file(rel)
    if not t:
        continue
    if all(p in t for p in PEERS):
        INVENTORIES.append(rel)
INVENTORIES.sort()

if not INVENTORIES:
    die_broken("no file in this tree names all %d registered hooks, so no "
               "inventory could be derived. Either the registration surface "
               "was just rewritten wholesale, or the inventories have been "
               "removed — both are worth a human look, and neither is "
               "something to pass silently." % len(PEERS))

# --- THE STRUCTURAL READERS ------------------------------------------------
# Typed, and re-proved against the derivation on every run: a reader whose file
# is not in INVENTORIES is a FAILURE of this predicate, never a silent skip.
SETTINGS_REL = ENG_REL + ".claude/settings.local.json"
PROBE_REL = ENG_REL + "scripts/hooks/contract-integrity-probe.sh"
ESTATUS_REL = ENG_REL + "scripts/hooks/engine-status.test.sh"


def block(text, pattern, what, where):
    m = re.search(pattern, text, re.S)
    if not m:
        die_broken("could not find %s in %s. Its shape changed, and a "
                   "membership test against a block this file can no longer "
                   "locate would pass everything. Refusing instead." % (what, where))
    return m.group(1)


structural = {}

if SETTINGS_REL in INVENTORIES:
    seated = parse_hooks_json(read_file(SETTINGS_REL), SETTINGS_REL)

    # THE FIX FOR THIS ONE IS A COMMAND, NOT AN EDIT — since 2026-09-15 the
    # seated surface is GENERATED from hooks/hooks.json by install.sh (see its
    # "GENERATE the seated surface" step), so the only correct way to satisfy
    # this reader is to run the generator. Telling an author to hand-add the
    # entry would be asking them to re-type what a script derives, and the next
    # install.sh run would overwrite it anyway. The demand stays because the
    # generated file is COMMITTED: a hook registered and committed without
    # running install.sh still reaches `main` with the surfaces disagreeing,
    # which is the red hook-staleness case 11 has always produced.
    def settings_has(name, rec):
        got = seated.get(name)
        if not got:
            return False, ("run scripts/hooks/install.sh and commit the result — "
                           "%s is GENERATED from hooks/hooks.json and does not yet "
                           "carry %s on %s. Do not hand-edit it; hook-staleness "
                           "case 11 parses BOTH surfaces and fails on 'surfaces "
                           "disagree: only-plugin=[%s]'"
                           % (SETTINGS_REL, name, "|".join(sorted(rec["events"])), name))
        missing_ev = sorted(rec["events"] - got["events"])
        if missing_ev:
            return False, ("it is seated in %s but NOT on %s — case 11 compares "
                           "(script, event) PAIRS, not names. Re-run "
                           "scripts/hooks/install.sh rather than editing the file."
                           % (SETTINGS_REL, "|".join(missing_ev)))
        return True, ""
    structural[SETTINGS_REL] = settings_has

probe_text = ""
if PROBE_REL in INVENTORIES:
    probe_text = read_file(PROBE_REL)
    br_expected = block(probe_text, r'BR_EXPECTED="\\\n(.*?)"\n', "the BR_EXPECTED table", PROBE_REL)
    br_lines = set(l.strip() for l in br_expected.splitlines() if l.strip())

    def probe_has(name, rec):
        missing = [ev for ev in sorted(rec["events"]) if "%s|%s" % (name, ev) not in br_lines]
        if missing:
            return False, ("add %s to BR_EXPECTED in %s — the probe's BR2 walks "
                           "that table in BOTH directions, so a registered hook "
                           "it does not name fails with 'plugin hook table "
                           "registers script(s) the managed set above does not "
                           "name'. session-evidence derives its set by parsing "
                           "the probe, so it fails with it."
                           % (", ".join("%s|%s" % (name, ev) for ev in missing), PROBE_REL))
        return True, ""
    structural[PROBE_REL] = probe_has

if ESTATUS_REL in INVENTORIES:
    est_text = read_file(ESTATUS_REL)
    ack = block(est_text, r"ACKNOWLEDGED_SCRIPTS=\"\$\(LC_ALL=C sort <<'ACK'\n(.*?)\nACK\n",
                "the ACKNOWLEDGED_SCRIPTS heredoc", ESTATUS_REL)
    ack_names = set(l.strip() for l in ack.splitlines() if l.strip())
    ack_start = est_text.index(ack)
    comment_lines = [l for l in est_text[:ack_start].splitlines() if l.lstrip().startswith("#")]

    def estatus_has(name, rec):
        if name not in ack_names:
            return False, ("add %s to ACKNOWLEDGED_SCRIPTS in %s AND write one "
                           "line above it saying what the hook is and why it "
                           "exists — case 1b's own fix text asks for both, and "
                           "it says 'Never silence this case'." % (name, ESTATUS_REL))
        if not any(name in l for l in comment_lines):
            return False, ("%s is in ACKNOWLEDGED_SCRIPTS in %s but NO comment "
                           "line above the list says what it is. The set is "
                           "acknowledged by NAME and explained by PROSE; a name "
                           "with no rationale is the next reader's puzzle."
                           % (name, ESTATUS_REL))
        return True, ""
    structural[ESTATUS_REL] = estatus_has

for rel in (SETTINGS_REL, PROBE_REL, ESTATUS_REL):
    if rel not in INVENTORIES and os.path.exists(os.path.join(ROOT, rel)):
        die_broken("%s is a typed structural reader of this predicate, and it "
                   "did NOT reach unanimity over the %d registered hooks — so "
                   "it has stopped being an inventory, or the peer set is "
                   "wrong, or it is missing a hook it should already name. A "
                   "typed reader that cannot notice its own staleness is the "
                   "defect this file exists to remove, so this refuses rather "
                   "than asserting against it." % (rel, len(PEERS)))

# --- THE SUITE INVENTORY (ci-affected-units case A5) -----------------------
# Not one file but "at least one of the discovered suites", so it gets its own
# shape. The suite list is discovered from disk by ci-units.sh, never typed.
_p = subprocess.run(["bash", os.path.join(ENGINE_DIR, "scripts/ci-units.sh"), "suites"],
                    capture_output=True, text=True)
SUITES = [l.strip() for l in _p.stdout.splitlines() if l.strip()] if _p.returncode == 0 else None


def named_by_a_suite(name):
    """The first suite naming this hook, preferring its OWN suite if one exists.

    A5's predicate is a substring test, so a PROSE MENTION satisfies it — this
    guard's own rationale line in engine-status.test.sh does, and that suite
    executes none of it. That is A5's rule and it is not this file's to
    tighten; what this file can honestly do is report WHICH suite the name was
    found in, and say so when it is not a suite about that hook. A demand would
    be inventing a contract; a named fact lets the author decide.
    """
    if SUITES is None:
        return None
    own = name[:-3] + ".test.sh"
    ordered = [s for s in SUITES if os.path.basename(s) == own] + \
              [s for s in SUITES if os.path.basename(s) != own]
    for s in ordered:
        if name in read_file(ENG_REL + s):
            return s
    return ""


# --- THE CONDITIONAL INVENTORIES ------------------------------------------
ROOTLESS_LIST = None
AGENT_CHAIN = None
CANON_LIST = ""
if PROBE_REL in INVENTORIES:
    # THE DEMAND INVERTED WITH LAYER R, 2026-09-14. Layer R used to hold a typed
    # R_ROOTED_HOOKS and this file demanded a rooted hook be added to it. Layer R
    # now DERIVES that membership from hooks/hooks.json and types only the
    # rootless exemption, so a rooted hook owes nothing here — it is walked the
    # moment it is registered — and the demand moves to the hook that resolves NO
    # root, which must be declared or R2 will name it for not sourcing a library
    # it has no reason to source.
    ROOTLESS_LIST = block(probe_text, r'R_ROOTLESS_HOOKS="(.*?)"\n',
                          "the R_ROOTLESS_HOOKS list", PROBE_REL)
    ROOTLESS_LIST = ROOTLESS_LIST.split()
    # AGENT_CHAIN stays None on purpose. Layer C's chain is DERIVED from
    # hooks/hooks.json now, so a hook on PreToolUse[Agent] owes it nothing and
    # the demand below is skipped. Left as an explicit None with this note
    # rather than deleted, because the next reader's question is "was this
    # forgotten?" and a silence cannot answer it.
    # Layer M's double-registration list. NOT a requirement — no suite asserts
    # membership — so it is read only to keep the advisory from telling
    # somebody to do a thing they have already done.
    _m = re.search(r"\nCANON = \[\n(.*?)\n\]\n", probe_text, re.S)
    CANON_LIST = _m.group(1) if _m else ""


def sources_roots_lib(name):
    """The SAME grep Layer R uses to decide a hook carries the root bootstrap."""
    return '. "$_RR_LIB"' in read_file(ENG_REL + "scripts/hooks/" + name)


# --- THE VERDICT ----------------------------------------------------------
failures = []   # (subject, place, how to fix)
unchecked = []  # subjects whose "named by a suite" inventory could NOT be read at all
advisories = []
checked = []

for s in SUBJECTS:
    rec = CAND.get(s)
    stem = s[:-3]

    if rec is not None:
        for rel in INVENTORIES:
            reader = structural.get(rel)
            if reader is not None:
                ok, why = reader(s, rec)
            else:
                ok = s in read_file(rel)
                why = ("add %s to %s. This file names every one of the other %d "
                       "registered hooks, which is what makes it an inventory; "
                       "no structural reader is typed for it here, so all that "
                       "can be said is that the name is absent."
                       % (s, rel, len(PEERS)))
            if ok:
                checked.append("%s: named in %s" % (s, rel))
            else:
                failures.append((s, rel, why))

        if ROOTLESS_LIST is not None:
            rooted = sources_roots_lib(s)
            exempt = stem in ROOTLESS_LIST
            if not rooted and not exempt:
                failures.append((s, PROBE_REL + " :: R_ROOTLESS_HOOKS",
                                 "this hook does NOT source scripts/lib/resolve-roots.sh, so "
                                 "add '%s' to R_ROOTLESS_HOOKS in %s. Layer R derives the "
                                 "hooks it walks from hooks/hooks.json, so a registered hook "
                                 "is walked whether or not anyone remembered it — and one "
                                 "that resolves no root fails R2 with '%s' in "
                                 "R_MISSING_SOURCE until the exemption is declared. Write "
                                 "one line above it saying why this hook needs no root."
                                 % (stem, PROBE_REL, stem)))
            elif exempt and rooted:
                # Not a failure and deliberately not one: guard-ci-red-lands.sh
                # sources the library for its broken-install banner and never
                # calls resolve_engine_root. Sourcing is not the same claim as
                # resolving, so an exemption that still sources is legitimate and
                # is reported rather than demanded away.
                checked.append("%s: R_ROOTLESS_HOOKS exempts it though it sources the "
                               "library — legitimate if it never calls resolve_engine_root; "
                               "check that it does not" % s)
            else:
                checked.append("%s: R_ROOTLESS_HOOKS correctly %s (the hook %s resolve a root)"
                               % (s, "exempts it" if exempt else "does not name it",
                                  "does not" if not rooted else "does"))

        on_agent = any("Agent" in m for m in rec["matchers"]) and "PreToolUse" in rec["events"]
        if on_agent:
            checked.append("%s: the PreToolUse[Agent] chain owes nothing — Layer C and BR2 "
                           "both DERIVE it from hooks/hooks.json, so this hook's position is "
                           "its registration and there is no list to add it to" % s)

        if not on_agent and any("Bash" in m for m in rec["matchers"]) and s not in CANON_LIST:
            advisories.append(
                "%s is a Bash-matcher hook. Layer M's CANON list in %s catches a "
                "DOUBLE registration, and no suite asserts membership of it, so it is "
                "not demanded here — but every other blocking Bash guard is in it."
                % (s, PROBE_REL))

    hit = named_by_a_suite(s)
    if hit is None:
        unchecked.append(s)
        advisories.append("could not run ci-units.sh, so the 'named by a suite' "
                          "inventory (ci-affected-units case A5) was NOT checked for %s" % s)
    elif hit == "":
        failures.append((s, "any suite (ci-affected-units case A5)",
                         "NO suite names %s. Write %sscripts/hooks/%s.test.sh, or make an "
                         "existing suite name it. A5 walks every *.sh in scripts/hooks/ "
                         "whether it is registered or not, and fails with '1 of N hook(s) "
                         "are named by NO suite: %s' — a diff touching it would select no "
                         "unit, so the push gate would certify it against an empty set."
                         % (s, ENG_REL, stem, s)))
    else:
        checked.append("%s: named by %s" % (s, hit))
        if os.path.basename(hit) != "%s.test.sh" % stem:
            # STATED, NOT ASSERTED. A5's test is a substring, so a prose mention
            # satisfies it — and whether the naming suite EXERCISES the hook is
            # not something a substring can answer either way. Saying "nothing
            # executes this hook" would be a claim this file cannot make. So it
            # reports which suite matched and leaves the judgment where the
            # evidence is.
            advisories.append(
                "%s satisfies ci-affected-units A5 through %s, which is not a suite named "
                "after it. A5's test is a substring, so a prose mention counts; whether "
                "that suite exercises this hook is not something this check can tell you. "
                "Nothing more is demanded, because no suite asserts more than A5 does."
                % (s, hit))

# --- REPORT ---------------------------------------------------------------
print("hook registration completeness — %d subject(s) against %s, %d peer(s), "
      "%d derived inventor%s"
      % (len(SUBJECTS), BASELINE, len(PEERS), len(INVENTORIES),
         "y" if len(INVENTORIES) == 1 else "ies"))
print("  subjects   : %s" % ", ".join(
    "%s%s" % (s, "" if s in CAND else " (unregistered script)") for s in SUBJECTS))
print("  inventories: %s" % ", ".join(INVENTORIES))
print("")

if failures:
    by_subject = {}
    for s, place, why in failures:
        by_subject.setdefault(s, []).append((place, why))
    for s in sorted(by_subject):
        n = len(by_subject[s])
        print("  %s is MISSING from %d place%s:" % (s, n, "" if n == 1 else "s"))
        for i, (place, why) in enumerate(by_subject[s], 1):
            print("    %d. %s" % (i, place))
            for line in re.findall(r".{1,66}(?:\s|$)", why):
                if line.strip():
                    print("       %s" % line.strip())
        print("")

for a in advisories:
    print("  NOTE: %s" % a)
    print("")

if EXPLAIN:
    for c in checked:
        print("  ok  %s" % c)
    print("")

if failures:
    print("  %d place%s owing. Each one is a file that already names every other "
          "registered hook; none of them is optional, and none of them will tell "
          "you it is owed until CI does, on main."
          % (len(failures), "" if len(failures) == 1 else "s"))
    raise SystemExit(1)

# AN INVENTORY THAT COULD NOT BE READ IS NOT AN INVENTORY THAT NAMES EVERY
# SUBJECT (hunt P5-08). Suite discovery failing used to be an advisory printed
# above and then "COMPLETE", so the push gate certified a hook against a suite
# list nobody had read -- the exact "certify it against an empty set" A5 was
# written to stop. The advisory above still names each subject; the verdict is
# now BROKEN (exit 2), decided AFTER the failures so a real owing place keeps
# its own exit 1.
if unchecked:
    die_broken("the suite inventory (ci-affected-units case A5) could not be read -- "
               "`bash %sscripts/ci-units.sh suites` failed -- so %d subject(s) were NOT "
               "checked against any suite: %s. This is not COMPLETE."
               % (ENG_REL, len(unchecked), ", ".join(unchecked)))

print("  COMPLETE: every derived inventory names every subject.")
raise SystemExit(0)
PYEOF
