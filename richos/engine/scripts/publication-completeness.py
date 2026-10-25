#!/usr/bin/env python3
"""publication-completeness.py — the analysis behind publication-completeness.sh.

Read that script's header first; it carries the argument for why this contract
exists. This file is the predicate, and nothing else: it takes a publication
root, derives everything it needs from the tree, and prints findings.

The four checks are documented at their own functions below. What they all
share is the derivation rule this engine holds itself to:

    NOTHING HERE IS A TYPED INVENTORY OF CAPABILITIES.

The set of declarations is derived from the shipped source that reads them.
The set of documents is derived from git. The set of onboarding entry points
is derived from README's own link graph. The set of workflows is derived from
the paths GitHub Actions itself discovers. A capability added tomorrow is
checked by the next run with no edit here — which is the only reason this
check can still be true in six months.

Output is one line per finding on stdout, machine-greppable:

    <CHECK>\t<severity>\t<path>\t<message>

Exit codes: 0 no findings, 1 findings, 2 broken (see the .sh wrapper).
"""

import json
import os
import re
import subprocess
import sys
from collections import Counter

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "lib"))
from machine_paths import matches as machine_matches, in_scope as machine_scope

# ---------------------------------------------------------------------------
# Bounds. Exceeding one is BROKEN, never "checked what we could" — a checker
# that silently truncates its own scope is the defect it exists to find.
# ---------------------------------------------------------------------------
MAX_TRACKED_FILES = 100000
MAX_PRIVATE_FILES = 50000
MAX_FILE_BYTES = 4 * 1024 * 1024
# Source files are read to derive declarations and a generated data file can be
# legitimately large (richos/app/ui/home/field-data.js is 4.9 MB of tracked
# synthetic data). Skipping it is wrong (its declarations would be unread); ending
# BROKEN on it makes every commit impossible. So a source file is READ up to this
# higher bound, and only beyond it does the run end BROKEN.
MAX_SOURCE_BYTES = 64 * 1024 * 1024

FINDINGS = []


def finding(check, path, message):
    FINDINGS.append((check, path, message))


class Broken(Exception):
    pass


# ---------------------------------------------------------------------------
# The tree, as git sees it.
# ---------------------------------------------------------------------------
def git(root, *args):
    p = subprocess.run(["git", "-C", root] + list(args),
                       capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


class Tree:
    """The publication-bound tree: tracked files only.

    Tracked-and-not-ignored is not an approximation of "what an adopter
    receives" — it IS what an adopter receives. Using os.path.exists() here
    would have been the subtle version of the bug this whole file is about: a
    citation that resolves on the author's disk and dangles in the clone.
    """

    def __init__(self, root):
        self.root = root
        self.unread = {}
        rc, out, err = git(root, "ls-files", "-z")
        if rc != 0:
            raise Broken("`git ls-files` failed in %s: %s" % (root, err.strip()))
        self.files = set(x for x in out.split("\0") if x)
        if not self.files:
            raise Broken("`git ls-files` returned nothing in %s — that is not an "
                         "empty publication tree, it is a broken checkout." % root)
        if len(self.files) > MAX_TRACKED_FILES:
            raise Broken("%d tracked files exceeds the %d bound."
                         % (len(self.files), MAX_TRACKED_FILES))
        self.dirs = set()
        for f in self.files:
            d = os.path.dirname(f)
            while d:
                self.dirs.add(d)
                d = os.path.dirname(d)
        # Every directory that holds a VERSION file is a SHIPPED ROOT: a unit
        # somebody can vendor on its own. `engine/` is one; the repository root
        # is always one. This is how a doc under engine/docs/ gets to cite
        # `docs/foo.md` and mean engine/docs/foo.md — which is what its author
        # meant, and what an adopter who vendored engine/ at their root reads.
        self.shipped_roots = [""]
        for f in sorted(self.files):
            if os.path.basename(f) == "VERSION":
                d = os.path.dirname(f)
                if d and d not in self.shipped_roots:
                    self.shipped_roots.append(d)

    def has(self, rel):
        return rel in self.files or rel in self.dirs

    def read(self, rel, limit=MAX_FILE_BYTES):
        """The text of a tracked document, or None when it could not be read.

        None is NOT "nothing to check": the document is recorded in self.unread
        with the reason, and main() refuses (BROKEN) at the end of the checks
        rather than report the tree complete over documents it never read (hunt
        P5-08). The callers keep their `if text is None: continue` so one bad
        document does not hide what the rest say; the refusal comes after."""
        p = os.path.join(self.root, rel)
        try:
            size = os.path.getsize(p)
            if size > limit:
                self.unread[rel] = ("%d bytes, over the %d-byte read bound"
                                    % (size, limit))
                return None
            with open(p, encoding="utf-8", errors="replace") as fh:
                return fh.read()
        except (OSError, ValueError) as exc:
            self.unread[rel] = getattr(exc, "strerror", None) or type(exc).__name__
            return None

    def require_all_read(self):
        """Raise Broken if any document this run needed could not be read."""
        if self.unread:
            names = sorted(self.unread)
            raise Broken(
                "%d tracked document(s) could not be read, so their citations, "
                "declarations and onboarding links were NOT checked and the tree "
                "cannot be called complete: %s%s"
                % (len(names),
                   "; ".join("%s (%s)" % (n, self.unread[n]) for n in names[:5]),
                   " ..." if len(names) > 5 else ""))

    def ignored(self, candidates):
        """Which of these repo-relative paths does .gitignore exclude?

        A citation of an ignored path is not a broken claim. It is a claim
        about a file that is GENERATED (a .sha256 sidecar install.sh mints) or
        OPERATOR-LOCAL (~/.claude state). Documenting those is the correct
        thing to do, and demanding they be committed would be demanding the
        opposite of what the boundary contract requires. Measured: this one
        rule removed 6 false positives on the first run, with no exemption.
        """
        cands = sorted(set(candidates))
        if not cands:
            return set()
        p = subprocess.run(["git", "-C", self.root, "check-ignore", "--stdin", "-z"],
                           input="\0".join(cands), capture_output=True, text=True)
        # rc 0 = some ignored, 1 = none ignored, 128 = error.
        if p.returncode not in (0, 1):
            raise Broken("`git check-ignore` failed: %s" % p.stderr.strip())
        return set(x for x in p.stdout.split("\0") if x)

    def nearest_shipped_root(self, rel):
        best = ""
        for r in self.shipped_roots:
            if r and (rel == r or rel.startswith(r + "/")) and len(r) > len(best):
                best = r
        return best


# ---------------------------------------------------------------------------
# CHECK 1 — REFERENTIAL HONESTY
# ---------------------------------------------------------------------------
# Every path a public document cites must resolve IN THE PUBLIC TREE.
#
# This is the cheapest of the four and it caught the loudest defect: a README
# that says "CI runs exactly this on windows-latest
# (.github/workflows/windows-companion-ci.yml)" about a file that exists in no
# repository anywhere. It also caught five citations that dangle for a reason
# worth stating, because it is the exact shape of the CEO's complaint: the
# publication boundary correctly MOVED five documents out to the private
# record, and every reference to them stayed behind. The boundary took the
# wrong things out. Nothing put the references right. A reader with only the
# public repo hits five dead links.
#
# PRECISION IS THE CONTRACT. A checker that fires on everything gets switched
# off, so the extraction was measured against this tree rather than argued:
#
#   naive (any backticked path, any ancestor anchor)          89 findings
#   + anchored only at the citing dir / shipped root / repo root
#   + gitignored targets skipped                              16 findings
#   + engine/reference/ declared as adopter-template prose    11 findings
#
# and all 11 were real. The three narrowing rules are, in order:
#
#   ANCHORED. A citation is a claim about THIS tree only if its first path
#   segment names something that exists in this tree at one of three anchors:
#   the citing file's own directory, its nearest shipped root, or the
#   repository root. `src/lib/Foo.svelte` in an example is not a claim about
#   this tree and is not treated as one. This is what removed 73 of the 89.
#
#   NOT IGNORED. See Tree.ignored().
#
#   NOT DECLARED REFERENCE MATERIAL. engine/reference/'s own README already
#   says, in prose: "Cross-file path references still point at the original
#   layout and will not resolve here — that is expected for reference
#   material." The CITATION_EXEMPT key makes that existing declaration
#   machine-readable. It is not a new license; it is the one already granted.
# ---------------------------------------------------------------------------

CITE_TOKEN = re.compile(r"`([^`\n]{2,200})`|\]\(([^)\s]{2,200})\)")
# Anything with a glob, a placeholder, a variable, a scheme, a home-relative or
# absolute root, or a `..` is not a concrete claim about a committed path.
CITE_REJECT = re.compile(r"""[*?<>{}$|\\"'\s]|\.\.|https?:|^~|^/|^-""")
CITE_HAS_EXT = re.compile(r"\.[A-Za-z0-9]{1,8}$")
# A FILENAME TEMPLATE IS NOT A CLAIM. `ui-ux-signoffs/SIGNOFF_YYYY-MM-DD_HH.MM.md`
# names the shape of a file the adopter will create, not a file anyone shipped.
# These are the conventional stand-ins, and treating them as citations was
# measured as 2 false positives out of the first 26 — small, but the kind that
# teaches a reader the check is pedantic, which is how a check gets switched off.
CITE_PLACEHOLDER = re.compile(r"(?:^|[^A-Za-z])(?:YYYY|MM|DD|HH|SS|NN|X{3,}|N{3,})(?:[^A-Za-z]|$)")
TEXT_SUFFIXES = (".md", ".txt")


def check_citations(tree, exempt_prefixes, used):
    pending = []
    for f in sorted(tree.files):
        if not f.endswith(TEXT_SUFFIXES):
            continue
        # A CHANGELOG RECORDS PATHS THAT NO LONGER EXIST, ON PURPOSE. Its
        # entries are of the form "`.github/workflows/kit-self-verify.yml` →
        # `.github/workflows/engine-self-verify.yml`" — the left side is
        # written down precisely BECAUSE it is gone. Demanding those resolve
        # would demand never renaming anything, and would grow one permanent
        # false positive per rename, forever. Same reason it is not in the
        # onboarding set: a changelog answers "what changed", not "what is".
        if os.path.basename(f) == "CHANGELOG.md":
            continue
        hit = _match_prefix(f, exempt_prefixes)
        if hit is not None:
            used.setdefault("CITATION_EXEMPT", set()).add(hit)
            continue
        text = tree.read(f)
        if text is None:
            continue
        anchors = [os.path.dirname(f), tree.nearest_shipped_root(f), ""]
        seen = set()
        for m in CITE_TOKEN.finditer(text):
            in_backticks = m.group(1) is not None
            cite = (m.group(1) or m.group(2) or "").strip().rstrip("/")
            if "/" not in cite or CITE_REJECT.search(cite):
                continue
            # `./x` INSIDE BACKTICKS IS A RUNTIME ARGUMENT, NOT A DOCUMENTED PATH.
            # app/README.md quotes the Tauri CLI's default icon source as
            # `./app-icon.png` — a value relative to whatever directory the
            # operator runs the command in. In a markdown LINK, `./` is instead
            # an ordinary relative link and must resolve, so the two syntaxes
            # are treated as what they respectively are.
            if in_backticks and cite.startswith("./"):
                continue
            if not CITE_HAS_EXT.search(cite.split("/")[-1]):
                continue
            if CITE_PLACEHOLDER.search(cite):
                continue
            if (f, cite) in seen:
                continue
            seen.add((f, cite))
            resolved = False
            anchored_as = None
            for a in anchors:
                full = os.path.normpath(os.path.join(a, cite)) if a else cite
                if tree.has(full):
                    resolved = True
                    break
                head = cite.split("/")[0]
                first = os.path.normpath(os.path.join(a, head)) if a else head
                if anchored_as is None and tree.has(first):
                    anchored_as = full
            if not resolved and anchored_as is not None:
                pending.append((f, cite, anchored_as))

    for f, cite, full in _drop_ignored(tree, pending):
        finding("CITATION", f,
                "cites `%s`, which does not exist in the published tree "
                "(would be %s). Fix the path, or point it at the private "
                "record by name so a reader with only the public repo is not "
                "sent to a file they will never have." % (cite, full))


def _drop_ignored(tree, pending):
    ign = tree.ignored([p[2] for p in pending])
    return [p for p in pending if p[2] not in ign]


# ---------------------------------------------------------------------------
# CHECK 2 — DECLARATION REACHABILITY  ("shipped inert")
# ---------------------------------------------------------------------------
# The engine's guards are switched on by a committed declaration file at the
# governed repository's root: .publication-boundary switches on the boundary
# guards, .ceo-todos switches on the TODOs guard. Presence IS adoption. That
# design is right, and it has a failure mode that is invisible from inside:
#
#   SHIP THE ENFORCEMENT, SHIP THE TESTS, SHIP NO DECLARATION AND NO WORD OF
#   IT ANYWHERE AN ADOPTER READS — AND THE CUSTOMER RECEIVES A GUARD THAT CAN
#   NEVER FIRE.
#
# That is not a hypothetical. On 2026-08-29 the engine shipped
# ceo-todos-lint.sh, guard-ceo-todos-commits.sh, lib/ceo-todos.{sh,py} and a
# full test suite, with no .ceo-todos anywhere, no template of one, and no
# mention in ONBOARDING-RUNBOOK.md, WALKTHROUGH.md or the bootstrap-interview
# skill. Every test passed. The probe was green. The capability was inert.
#
# THE SET OF DECLARATIONS IS DERIVED, NOT LISTED. Each guard names its own
# declaration in its own source, in one form the engine already uses twice:
#
#     : "${PUBLICATION_DECLARATION:=.publication-boundary}"
#     : "${CEO_TODOS_DECLARATION:=.ceo-todos}"
#
# so the set is a grep over shipped source, and a third capability added
# tomorrow is checked the moment it follows the same convention. What that
# does NOT catch is a capability that invents a different convention; see the
# .sh wrapper's "WHAT THIS CANNOT CATCH".
#
# TWO ARMS, because the failure had two halves:
#
#   COPYABLE   Some tracked file is named <D>, <D>.example, <D>.template or
#              <D>.sample — or the repository's own live declaration sits at
#              `<declaration_dir>/<D-without-its-dot>`, which is the same file
#              in the grouped form scripts/lib/declaration-path.sh resolves.
#              Without one, the adopter must reverse-engineer the format out of
#              a parser.
#   ONBOARDED  <D> is named in the ADOPTER'S ENTRY-POINT SET — README.md, the
#              transitive closure of the markdown it links to, and every
#              shipped SKILL.md. Derived from README's own link graph, so it
#              tracks the docs rather than a list of doc names. CHANGELOG.md
#              is not in the closure and must not be: a changelog records that
#              something happened, it does not tell a new adopter to do it.
# ---------------------------------------------------------------------------

DECL_BASH = re.compile(r""":\s*"\$\{([A-Z][A-Z0-9_]*_DECLARATION):=([^}"'\s]+)\}"\s*""")
DECL_ASSIGN = re.compile(r"""^\s*([A-Z][A-Z0-9_]*_DECLARATION)\s*=\s*["']([^"'\s]+)["']""",
                         re.MULTILINE)
SOURCE_SUFFIXES = (".sh", ".py", ".mjs", ".js", ".rb", ".ps1")
# A link target may carry a #fragment; the fragment is not part of the path.
MD_LINK = re.compile(r"\]\(([^)\s#]{1,200})(?:#[^)\s]*)?\)")
# What may NOT be followed as navigation. Unlike CITE_REJECT (a filter for path
# citations in prose, which must stay conservative), a markdown link legitimately
# climbs out of its directory with `../`; the target is resolved against the
# linking file and must exist in the tree, so `..` is followed and an escape from
# the tree simply finds nothing.
MD_LINK_REJECT = re.compile(r"""[*?<>{}$|\\"'\s]|https?:|^~|^/|^-""")


# A TEST FIXTURE IS NOT A SHIPPED CAPABILITY. Suites and mutation testers build
# synthetic trees declaring synthetic gates — this file's own suite declares a
# `.widget` — and deriving from those would demand a template and an onboarding
# paragraph for a capability that exists only inside a fixture. Caught on the
# first end-to-end ci-verify run: the check reported the engine as shipping
# inert `.widget` enforcement, which is exactly the kind of confident,
# well-formatted, wrong finding that gets a checker switched off.
#
# The blind spot this opens, stated: a declaration named ONLY in a test file is
# not derived. That is the correct reading — a gate no shipped code reads is not
# a capability the tree offers.
FIXTURE_SUFFIXES = (".test.sh", ".test.py", ".mutation.sh")


def derive_declarations(tree):
    decls = {}
    for f in sorted(tree.files):
        if not f.endswith(SOURCE_SUFFIXES) or f.endswith(FIXTURE_SUFFIXES):
            continue
        text = tree.read(f, MAX_SOURCE_BYTES)
        if text is None:
            continue
        # A DEFINITION IS CODE. A COMMENT QUOTING THE CONVENTION IS NOT.
        # This file's own header quotes both real declarations verbatim to
        # explain the derivation, and that made it register as a THIRD source
        # of `.ceo-todos` — so a finding named it alongside the guard that
        # actually implements the gate, and deleting the real guard would have
        # left the declaration "alive" in a doc comment. Comment lines are
        # dropped before matching, which is both more accurate and the reason
        # a header may go on quoting the convention it documents.
        code = "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))
        for rx in (DECL_BASH, DECL_ASSIGN):
            for m in rx.finditer(code):
                decls.setdefault(m.group(2), set()).add(f)
    return decls


# A CHANGELOG IS NOT ONBOARDING, and this distinction is load-bearing rather
# than fussy. Measured: with CHANGELOG.md in the closure, BOTH declarations
# passed the ONBOARDED arm — because the changelog records the commit that
# ADDED them. `.ceo-todos` was mentioned in exactly one published document, the
# one that says it happened, and in none of the documents that tell an adopter
# to do it. A changelog answers "what changed"; onboarding answers "what do I
# do". Counting the first as the second is how a capability comes to look
# documented while being undiscoverable, which is failure #1 wearing a
# reassuring green tick.
NOT_ONBOARDING = {"CHANGELOG.md", "LICENSE", "LICENSE.md", "LICENSING.md"}


def onboarding_set(tree):
    """README.md, everything it links to transitively, and every SKILL.md."""
    frontier, seen = [], set()
    for root in tree.shipped_roots:
        r = os.path.join(root, "README.md") if root else "README.md"
        if tree.has(r):
            frontier.append(r)
    for f in sorted(tree.files):
        if os.path.basename(f) == "SKILL.md":
            frontier.append(f)
    while frontier:
        cur = frontier.pop()
        if cur in seen or os.path.basename(cur) in NOT_ONBOARDING:
            continue
        seen.add(cur)
        text = tree.read(cur)
        if text is None:
            continue
        base = os.path.dirname(cur)
        for m in MD_LINK.finditer(text):
            t = m.group(1)
            if not t.endswith(".md") or MD_LINK_REJECT.search(t):
                continue
            nxt = os.path.normpath(os.path.join(base, t)) if base else t
            if tree.has(nxt) and nxt not in seen:
                frontier.append(nxt)
    return seen


DECL_LIST_RE = re.compile(r"""^\s*(DECL_ADOPTED_STEMS|DECL_FOREIGN_STEMS)\s*=\s*"([^"]*)\"""",
                          re.MULTILINE)


def check_declaration_classification(tree, decls, by_base):
    """Every declaration this engine ships is CLASSIFIED by the resolver.

    scripts/lib/declaration-path.sh answers "where does this declaration live?"
    for some declarations and not others, and it refuses one it does not serve
    if it finds a copy in the grouped directory -- which is what stops a `git mv`
    from switching a contract off in silence.

    That refusal is driven by two hand-written lists, and a hand-written list of
    names is the exact thing this file exists to stop going quietly stale: a
    declaration added tomorrow, in neither list, is one the resolver would find
    in the grouped directory and say nothing about. So the derived set -- which
    is the truth, read out of shipped source -- is checked against them here.

    BROKEN rather than a finding, because a stale classification makes every
    other verdict in this check less trustworthy, and a checker that is unsure
    what it is checking must say so rather than grade.
    """
    hits = by_base.get("declaration-path.sh") or []
    if not hits:
        return                      # not shipped in this tree; nothing to check
    text = tree.read(sorted(hits)[0]) or ""
    classified = set()
    for m in DECL_LIST_RE.finditer(text):
        classified.update(m.group(2).split())
    if not classified:
        raise Broken("read scripts/lib/declaration-path.sh and derived NO "
                     "classified declaration stems from it. Those lists decide "
                     "which declarations the resolver refuses to find in the "
                     "grouped directory; finding none means the derivation is "
                     "broken, not that there is nothing to classify.")
    missing = sorted(d for d in decls if (d[1:] if d.startswith(".") else d) not in classified)
    if missing:
        raise Broken(
            "scripts/lib/declaration-path.sh classifies neither way: %s. Every "
            "declaration this engine ships must appear in DECL_ADOPTED_STEMS "
            "(the resolver finds it in the grouped directory) or in "
            "DECL_FOREIGN_STEMS (it does not, and a copy found there is "
            "REFUSED). An unclassified one is a declaration a `git mv` into "
            "that directory would switch off in silence, which is the failure "
            "the directory must not be able to cause."
            % ", ".join("`%s`" % d for d in missing))


def check_declarations(tree, exempt, used, explain, declaration_dir=""):
    decls = derive_declarations(tree)
    if not decls:
        raise Broken("derived NO declaration names from shipped source. The engine "
                     "has used the `${X_DECLARATION:=.name}` convention since "
                     "2026-08-29; finding none means the derivation is broken, not "
                     "that there is nothing to check.")
    onboard = onboarding_set(tree)
    if not onboard:
        raise Broken("derived an EMPTY adopter entry-point set (no README.md and no "
                     "SKILL.md anywhere in the tree). Refusing to report every "
                     "declaration as undocumented on the strength of that.")
    if explain:
        sys.stderr.write("declarations derived: %s\n" % ", ".join(sorted(decls)))
        sys.stderr.write("adopter entry-point set (%d):\n  %s\n"
                         % (len(onboard), "\n  ".join(sorted(onboard))))

    by_base = {}
    for f in tree.files:
        by_base.setdefault(os.path.basename(f), []).append(f)

    check_declaration_classification(tree, decls, by_base)

    for d in sorted(decls):
        problems = []
        # A DOTFILE'S TEMPLATE IS USUALLY SHIPPED WITHOUT THE DOT, because a
        # hidden template is a template nobody browsing the tree ever sees.
        # `.ceo-todos`'s real one shipped as reference/ceo-todos/ceo-todos.example
        # and this check called it missing — a false positive on a capability
        # that had just been fixed properly, which is how a checker earns the
        # reputation that gets it switched off. The dot-stripped form is
        # accepted only WITH an explicit template suffix, so a coincidentally
        # named file can never satisfy this arm.
        stem = d[1:] if d.startswith(".") else d
        names = [d] + ["%s.%s" % (n, sfx)
                       for n in ({d, stem} if stem else {d})
                       for sfx in ("example", "template", "sample")]
        # A LIVE DECLARATION IN THE GROUPED DIRECTORY IS ITS OWN COPYABLE
        # INSTANCE, exactly as the root form always was. `.publication-boundary`
        # had no separate template and never needed one — the repository's own
        # declaration was the thing an adopter read and copied. Moving it to
        # `.richos/publication-boundary` must not turn a working capability into
        # an INERT finding, and this is PATH-EXACT rather than basename-matched
        # so a file that merely happens to be called `publication-boundary`
        # somewhere else in the tree still satisfies nothing.
        grouped = ("%s/%s" % (declaration_dir.rstrip("/"), stem)
                   if declaration_dir and stem else None)
        if not any(by_base.get(n) for n in names) and not (grouped and tree.has(grouped)):
            looked = sorted(set(names)) + ([grouped] if grouped else [])
            problems.append("no copyable instance or template of `%s` is published "
                            "(looked for %s anywhere in the tree)"
                            % (d, ", ".join(looked)))
        if not any(d in (tree.read(p) or "") for p in sorted(onboard)):
            problems.append("`%s` is named in no document an adopter reads "
                            "(README.md, everything README links to, and every "
                            "SKILL.md)" % d)
        if not problems:
            continue
        if d in exempt:
            used.setdefault("DECLARATION_EXEMPT", set()).add(d)
            continue
        finding("INERT", ", ".join(sorted(decls[d])),
                "ships enforcement gated on `%s`, but %s. A customer receives "
                "machinery that can never fire." % (d, "; and ".join(problems)))


# ---------------------------------------------------------------------------
# CHECK 3 — WORKFLOW REACHABILITY
# ---------------------------------------------------------------------------
# GitHub Actions discovers workflows ONLY in `.github/workflows/` at the
# REPOSITORY ROOT. Until 2026-08-29 this repository's only copy of its own
# self-verification lived at engine/.github/workflows/engine-self-verify.yml.
# It was not broken and it was not misconfigured. It had never executed once —
# `gh run list` returned nothing at all — because it sat at a path the runner
# does not look at.
#
# A workflow under a subdirectory is legitimate: it is the ADOPTER'S TEMPLATE,
# and it fires at their root once they vendor the engine there. What is not
# legitimate is that being the ONLY copy, because then the repository shipping
# it has never run the thing it tells the adopter to run.
#
# So the predicate is not "no workflows in subdirectories". It is: whatever a
# subdirectory workflow INVOKES must also be invoked by a workflow at the root.
# Entry points are read out of the YAML's own `run:` lines, so the two copies
# are compared on what they actually do rather than on being byte-identical —
# which they must not be (the root copy carries `working-directory: engine`).
# ---------------------------------------------------------------------------

WF_RE = re.compile(r"(?:^|/)\.github/workflows/[^/]+\.ya?ml$")
RUN_TOKEN = re.compile(r"[\w./-]+\.(?:sh|py|ps1|bash)\b")


def _wf_entrypoints(text):
    """Every script a workflow's `run:` steps invoke: the one-line form and the
    block-scalar form (`run: |` / `run: >-`), whose commands are the indented
    lines that follow."""
    eps = set()
    lines = (text or "").splitlines()
    i = 0
    while i < len(lines):
        raw = lines[i]
        s = raw.strip()
        i += 1
        if not (s.startswith("run:") or s.startswith("- run:") or s.startswith("-run:")):
            continue
        for m in RUN_TOKEN.finditer(s):
            eps.add(os.path.basename(m.group(0)))
        value = s.split("run:", 1)[1].strip()
        if re.match(r"^[|>][+-]?[0-9]?\s*(#.*)?$", value):
            # The body is every following line indented deeper than the key.
            key_indent = len(raw) - len(raw.lstrip(" ")) + (2 if s.startswith("- ") else 0)
            while i < len(lines):
                body = lines[i]
                if body.strip() and len(body) - len(body.lstrip(" ")) <= key_indent:
                    break
                for m in RUN_TOKEN.finditer(body):
                    eps.add(os.path.basename(m.group(0)))
                i += 1
    return eps


def check_workflows(tree, exempt, used, explain):
    wfs = [f for f in sorted(tree.files) if WF_RE.search(f)]
    if not wfs:
        return
    root_wfs = [f for f in wfs if f.startswith(".github/workflows/")]
    root_eps = set()
    for f in root_wfs:
        root_eps |= _wf_entrypoints(tree.read(f))
    if explain:
        sys.stderr.write("workflows: %d total, %d at the repository root; root entry "
                         "points: %s\n" % (len(wfs), len(root_wfs),
                                           ", ".join(sorted(root_eps)) or "(none)"))
    for f in wfs:
        if f in root_wfs:
            continue
        if f in exempt:
            used.setdefault("WORKFLOW_EXEMPT", set()).add(f)
            continue
        eps = _wf_entrypoints(tree.read(f))
        if not root_wfs:
            finding("UNREACHABLE", f,
                    "is a GitHub Actions workflow outside `.github/workflows/` at the "
                    "repository root, and this repository has NO root workflow at all. "
                    "Actions discovers workflows only at the root, so this file has "
                    "never executed and cannot. A workflow that has not run verifies "
                    "nothing.")
            continue
        uncovered = eps - root_eps
        if uncovered:
            finding("UNREACHABLE", f,
                    "runs %s, which no root-level workflow runs. Actions discovers "
                    "workflows only at `.github/workflows/` in the repository root, so "
                    "this copy never executes here — it is a template for an adopter, "
                    "and nothing in THIS repository proves it works."
                    % ", ".join(sorted(uncovered)))


# ---------------------------------------------------------------------------
# CHECK 4 — MECHANISM MISPLACEMENT
# ---------------------------------------------------------------------------
# The TODOs contract shipped its guard, its lint and its parser to the public
# tree, and left the renderer — the only thing that turns the record into
# something a CEO can look at — in the private HQ repository. The customer got
# the enforcement and not the view.
#
# THE JUDGEMENT CALL IS MECHANISM vs INSTANCE DATA, and it is made
# STRUCTURALLY rather than by opinion:
#
#   INSTANCE DATA  is content. A `.ceo-todos` in the private tree is one
#                  company's declaration; CEO-TODOs.md is one company's TODOs.
#                  Correctly private, and never a candidate here, because
#                  neither has a shebang nor an executable extension.
#
#   MECHANISM      is executable. A shebang, or a .sh/.py/.mjs/.js/.rb/.ps1
#                  extension. render-ceo-todos.mjs is one.
#
# Being a mechanism is not enough — a private repo is allowed its own scripts.
# The defect is a private mechanism COUPLED TO A PUBLIC CONTRACT: it reads or
# writes an artifact whose format is defined by machinery in the public tree,
# i.e. it mentions one of the declaration names Check 2 derived. That is the
# whole test, and it is why normalize-gpt-branch-exports.mjs — a private script
# that couples to nothing public — is not flagged and should not be.
#
# WHERE the private trees are is not guessed: it is read from PRIVATE_SOURCES
# in .publication-boundary, which the operator already maintains for the
# boundary guard. One declaration, two contracts, no second copy to drift.
#
# ===========================================================================
# THE EXCUSE TRAVELS WITH THE THING IT EXCUSES
# ===========================================================================
# This is the one property of this check a reader must not get wrong, and it
# was got wrong. `.publication-completeness` carried an INSTANCE_MECHANISMS key
# naming a file inside a private sibling repository, and the result was a gate
# whose verdict on the PUBLIC tree depended on what happened to be sitting next
# to it on disk:
#
#   ON THE MACHINE THAT WROTE THE ENTRY the private root resolved, this check
#   ran, the entry suppressed a real finding, and the gate was green.
#
#   IN A CLONE OF THE PUBLIC REPOSITORY ALONE — every CI runner, every reader,
#   every contributor — no private root resolves, so this check does not run at
#   all. The entry therefore suppressed nothing, and the "an exemption that
#   suppresses nothing FAILS" rule at the bottom of this file fired over a file
#   that is not in the tree being checked.
#
# Both verdicts were sincere and only one of them was about the published tree.
# Note where the asymmetry actually lived: not in a path that failed to
# resolve, but in a CHECK THAT DID NOT RUN while its exemption was still being
# judged. The rule is not at fault and is not weakened here.
#
#       A VERDICT ON THE PUBLISHED TREE MAY NOT DEPEND ON THE OPERATOR'S
#       MACHINE.
#
# So a private mechanism that is one operator's artifact rather than a withheld
# capability now says so ON ITSELF:
#
#       instance-mechanism: <why this is one operator's artifact>
#
# anywhere in its own body. That is the discipline `dialect-exempt: <reason>`
# and `main-checkout-run: <reason>` already use in this engine, for the same
# reason — the declaration sits where the reviewer of that file cannot miss it
# — and a BARE marker with no reason after it exempts nothing.
#
# THE MARKER IS ITSELF CHECKED, so it cannot outlive its justification any more
# than a declaration entry could: a marker on a mechanism that couples to no
# public contract suppresses nothing and is reported as STALE-EXEMPTION, naming
# itself for deletion. That check runs wherever the file exists, which is the
# only place it can mean anything.
#
# WHAT THE PUBLISHED TREE GAINS is a property rather than one fixed instance:
# every key `.publication-completeness` still accepts — CITATION_EXEMPT,
# DECLARATION_EXEMPT, WORKFLOW_EXEMPT — is a function of `git ls-files` and the
# tracked bytes, so its staleness verdict is identical in every clone of that
# tree. There is no key left whose meaning depends on the host, and the retired
# one is refused BY NAME in the wrapper rather than quietly ignored.
# ---------------------------------------------------------------------------

EXEC_SUFFIXES = (".sh", ".py", ".mjs", ".js", ".rb", ".ps1", ".bash")
#
# `.worktrees` IS IN HERE FOR A MEASURED REASON, NOT FOR TIDINESS. An agent
# working under `isolation: "worktree"` gets a full second copy of the private
# tree at `<private-root>/.worktrees/<branch>/`. Every declared mechanism in it
# then appears at a SECOND path that no `.publication-completeness` entry names
# — so the checker reports a MISPLACED finding for a file that is already
# exempt at its real path, and `guard-completeness-commits.sh` refuses EVERY
# commit in the public repository until that agent's worktree goes away.
#
# Measured 2026-09-01: one isolated worktree inside the private tree re-reported
# an already-excused mechanism and blocked an unrelated commit in the public
# repository. Declaring the copy would have been worse than useless: it names an
# ephemeral path, and the "an entry that suppresses nothing FAILS" rule would
# then have fired the moment the worktree was reaped. Under the marker the copy
# carries the marker too, so it is excused either way — the skip still earns
# its place by keeping one file from being reported twice.
#
# A worktree is a checkout of content that is already being walked at its
# tracked path, so skipping it removes duplicates and never removes coverage.
SKIP_DIRS = {".git", ".worktrees", "node_modules", "target", "dist", "build",
             ".venv", "__pycache__"}

# The marker, in this engine's usual shape: at least one alphanumeric after the
# colon, so a BARE `instance-mechanism:` is not an off switch anybody can type
# into a file. dialect-guard.sh's DECLARED_EXEMPT_RE is the same regex for the
# same reason, and its mutation suite proves that the one-character difference
# between these two patterns is what stands between an escape hatch and an
# amnesty.
INSTANCE_MARKER_RE = re.compile(r"instance-mechanism:\s*[A-Za-z0-9]")
# Matched only to say something USEFUL when a marker was written without a
# reason. It never exempts anything; it changes the wording of the refusal from
# "you may declare this" to "you tried to declare this and left the reason out".
INSTANCE_MARKER_BARE_RE = re.compile(r"instance-mechanism:")


def _is_mechanism(path):
    if path.endswith(EXEC_SUFFIXES):
        return True
    try:
        with open(path, "rb") as fh:
            return fh.read(2) == b"#!"
    except OSError:
        return False


def _judge_private(key, path, body, decl_names):
    """The MISPLACED / STALE-EXEMPTION verdict on one private file's text."""
    if not (path.endswith(EXEC_SUFFIXES) or body[:2] == "#!"):
        return
    coupled = sorted(d for d in decl_names if d in body)
    marked = bool(INSTANCE_MARKER_RE.search(body))
    if not coupled:
        # A marker on a mechanism that couples to nothing public
        # excuses a finding that is not there. Same rule as a stale
        # declaration entry, evaluated here — beside the file —
        # because beside the file is the only place it can mean
        # anything.
        if marked:
            finding("STALE-EXEMPTION", key,
                    "carries an `instance-mechanism:` marker and couples to no "
                    "public contract, so the marker suppresses nothing. Either "
                    "the declaration this file used to read has been renamed — "
                    "in which case this file is broken and the marker is hiding "
                    "it — or the coupling is gone and THE MARKER SHOULD BE "
                    "DELETED. An exemption cannot outlive its own reason.")
        return
    if marked:
        return
    bare = "" if not INSTANCE_MARKER_BARE_RE.search(body) else (
        " This file already carries a BARE `instance-mechanism:` with no reason "
        "after it, which exempts nothing — write the reason and it will.")
    finding("MISPLACED", key,
            "is an executable mechanism in the PRIVATE tree that reads the "
            "public contract %s. The public tree ships the enforcement and "
            "withholds this. Move it into the published tree, or — if it is one "
            "operator's artifact rather than a capability a customer could use "
            "— write `instance-mechanism: <reason>` in the file itself. The "
            "excuse lives with the file because the published tree's verdict "
            "must not depend on which private trees this machine happens to "
            "have.%s"
            % (", ".join("`%s`" % c for c in coupled), bare))


# ---------------------------------------------------------------------------
# A PRIVATE TREE THAT IS A REPOSITORY IS JUDGED BY WHAT IT COMMITTED
# ---------------------------------------------------------------------------
# 2026-10-01: a teammate's commit in its own richos workspace was refused over
# richos-hq/docs/audits/2026-09-29-hunt/part-3-codex-evidence-v2/probes-v2.py, a
# Codex audit probe staged, not committed, in richos-hq's main checkout while
# five Codex windows worked there. The walk read whatever bytes sat in that
# working tree at that instant: another repository's work in progress, which
# nobody committing to the public tree changed or could fix. (2026-09-28 was the
# same class; richos-hq 46e9abc6 excused that one file and the class stayed.)
#
# So when the private root is the top of its own Git repository, Check 4 reads
# that repository's HEAD: its committed files, through `git grep` for the
# declaration names and the marker (fast on ten thousand files) and `git
# cat-file` for the bodies of the few that match. A misplaced mechanism is
# found once it is COMMITTED to the private tree, by every run that runs this
# check (the land's run on the branch, the push arm, CI with the private tree
# present). Uncommitted work in that checkout is nobody's finding here. A
# private root that is not a repository's top (a plain directory, or an ignored
# directory inside another repository) has no commits to read, and is walked on
# disk as before. The caller's GIT_* environment (the commit guard's scratch
# index) never reaches the private repository.

def _private_git(root, *args):
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    p = subprocess.run(["git", "-C", root] + list(args), capture_output=True, env=env)
    return p.returncode, p.stdout, p.stderr.decode("utf-8", "replace")


def _committed_repository(root):
    rc, out, _ = _private_git(root, "rev-parse", "--show-toplevel")
    return rc == 0 and os.path.realpath(out.decode("utf-8", "replace").strip()) == os.path.realpath(root)


def _committed_private_files(root, decl_names):
    """(tracked count, [(path, text)]) for HEAD's files that mention a
    declaration name or the instance-mechanism marker, under MAX_FILE_BYTES."""
    rc, _, _ = _private_git(root, "rev-parse", "--verify", "--quiet", "HEAD^{commit}")
    if rc != 0:
        return 0, []          # nothing committed yet: nothing to judge
    rc, out, err = _private_git(root, "ls-tree", "-r", "-z", "--long", "--full-tree", "HEAD")
    if rc != 0:
        raise Broken("`git ls-tree HEAD` failed in the private tree %s: %s" % (root, err.strip()))
    blobs, n = {}, 0
    for item in out.decode("utf-8", "surrogateescape").split("\0"):
        meta, _, path = item.partition("\t")
        parts = meta.split()
        if not path or len(parts) < 4 or parts[1] != "blob":
            continue
        if any(seg in SKIP_DIRS for seg in path.split("/")[:-1]):
            continue
        n += 1
        if n > MAX_PRIVATE_FILES:
            raise Broken("the declared private tree %s tracks more than the %d-file bound at HEAD. "
                         "Refusing to report a clean misplacement check over a truncated read."
                         % (root, MAX_PRIVATE_FILES))
        if parts[0] != "120000" and parts[3].isdigit() and int(parts[3]) <= MAX_FILE_BYTES:
            blobs[path] = parts[2]
    patterns = []
    for name in ["instance-mechanism:"] + sorted(decl_names):
        patterns += ["-e", name]
    rc, out, err = _private_git(root, "grep", "-l", "-z", "-F", "--no-color", *patterns, "HEAD", "--")
    if rc not in (0, 1):
        raise Broken("`git grep` over HEAD failed in the private tree %s: %s" % (root, err.strip()))
    hits = []
    for item in out.decode("utf-8", "surrogateescape").split("\0"):
        path = item[len("HEAD:"):] if item.startswith("HEAD:") else ""
        if path in blobs:
            hits.append(path)
    if not hits:
        return n, []
    batch = subprocess.run(["git", "-C", root, "cat-file", "--batch"], capture_output=True,
                           input="".join(blobs[p] + "\n" for p in hits).encode(),
                           env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
    if batch.returncode != 0:
        raise Broken("`git cat-file --batch` failed in the private tree %s" % root)
    data, at, files = batch.stdout, 0, []
    for path in hits:
        nl = data.index(b"\n", at)
        size = int(data[at:nl].split()[2])
        files.append((path, data[nl + 1:nl + 1 + size].decode("utf-8", "replace")))
        at = nl + 1 + size + 1
    return n, files


def check_misplacement(tree, private_roots, decl_names, explain):
    if not private_roots:
        return
    for label, root in private_roots:
        if _committed_repository(root):
            n, files = _committed_private_files(root, decl_names)
            for rel, body in files:
                _judge_private("%s/%s" % (label, rel), rel, body, decl_names)
            if explain:
                sys.stderr.write("private tree %s (%s): its committed HEAD, %d tracked files, %d read "
                                 "(uncommitted work there is not judged)\n" % (label, root, n, len(files)))
            continue
        n = 0
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for fn in sorted(filenames):
                n += 1
                if n > MAX_PRIVATE_FILES:
                    raise Broken("walking the declared private tree %s exceeded the "
                                 "%d-file bound. Refusing to report a clean "
                                 "misplacement check over a truncated walk."
                                 % (root, MAX_PRIVATE_FILES))
                full = os.path.join(dirpath, fn)
                if not _is_mechanism(full):
                    continue
                rel = os.path.relpath(full, root)
                try:
                    if os.path.getsize(full) > MAX_FILE_BYTES:
                        continue
                    with open(full, encoding="utf-8", errors="replace") as fh:
                        body = fh.read()
                except OSError:
                    continue
                _judge_private("%s/%s" % (label, rel), rel, body, decl_names)
        if explain:
            sys.stderr.write("private tree %s (%s): %d files walked\n" % (label, root, n))


def _match_prefix(path, prefixes):
    for p in prefixes:
        if path == p.rstrip("/") or path.startswith(p if p.endswith("/") else p + "/"):
            return p
    return None



# CHECK 5: literal machine assumptions. Baseline is temporary tracked debt,
# not a host-local override. Archived records stay out as whole trees.
def committed_machine_baseline(tree):
    # Both committed predecessor and landing base constrain migration debt.
    # After this checker is adopted, absent debt means zero, never bootstrap.
    budgets = []
    for ref in ("refs/heads/main", "HEAD"):
        rc, _out, _err = git(tree.root, "rev-parse", "--verify", ref + "^{commit}")
        if rc:
            continue
        relative = os.path.relpath(__file__, tree.root)
        rc, checker, _err = git(tree.root, "show", ref + ":" + relative)
        adopted = rc == 0 and "def check_machine_paths(" in checker
        old = None
        for declaration in (".publication-completeness", ".richos/publication-completeness"):
            rc, text, _err = git(tree.root, "show", ref + ":" + declaration)
            if rc:
                continue
            for line in text.splitlines():
                key, separator, value = line.strip().partition("=")
                if not separator or key != "MACHINE_PATH_BASELINE":
                    continue
                value = value.strip()
                if len(value) >= 2 and value[0] == value[-1] and value[0] in ("'", '"'):
                    value = value[1:-1]
                else:
                    value = value.split("#", 1)[0].strip()
                rc, body, _err = git(tree.root, "show", ref + ":" + value)
                if rc:
                    raise Broken("committed machine baseline cannot be read")
                try:
                    old = json.loads(body)
                    if not isinstance(old, dict):
                        raise ValueError("expected an object")
                except ValueError as exc:
                    raise Broken("invalid committed machine baseline: " + str(exc))
        if old is not None or adopted:
            budgets.append(old or {})
    if not budgets:
        return None  # only the reviewed first introduction can initialize debt
    allowed = budgets[0]
    for other in budgets[1:]:
        allowed = {path: {prefix: min(count, other.get(path, {}).get(prefix, 0))
                          for prefix, count in counts.items()}
                   for path, counts in allowed.items()}
    return allowed


def check_machine_paths(tree, baseline_path, previous=None):
    baseline = {}
    if baseline_path:
        if baseline_path not in tree.files:
            raise Broken("machine path baseline must name a tracked file")
        body = tree.read(baseline_path)
        try:
            baseline = json.loads(body)
            if not isinstance(baseline, dict):
                raise ValueError("expected an object")
            for path, counts in baseline.items():
                if not isinstance(counts, dict) or any(
                        not isinstance(prefix, str) or type(count) is not int or count < 1
                        for prefix, count in counts.items()):
                    raise ValueError("expected prefix counts for " + path)
        except (TypeError, ValueError) as exc:
            raise Broken("invalid machine path baseline: " + str(exc))
    if previous is None and hasattr(tree, "root"):
        previous = committed_machine_baseline(tree)
    if previous is not None:
        for path, counts in baseline.items():
            for prefix, count in counts.items():
                if count > previous.get(path, {}).get(prefix, 0):
                    finding("MACHINE-PATH-GROWTH", path,
                            "%s migration debt may only shrink relative to the committed base" % prefix)
    actual = {}
    for path in sorted(tree.files):
        if path == baseline_path or not machine_scope(path):
            continue
        text = tree.read(path, MAX_SOURCE_BYTES)
        if text is None or "\x00" in text:
            continue
        counts = machine_matches(text, path)
        if counts:
            actual[path] = dict(counts)
    for path in sorted(set(actual) | set(baseline)):
        got, old = actual.get(path, {}), baseline.get(path, {})
        for prefix in sorted(set(got) | set(old)):
            if got.get(prefix, 0) != old.get(prefix, 0):
                finding("MACHINE-PATH" if got.get(prefix, 0) else "STALE-MACHINE-PATH",
                        path, "%s: %s occurrence(s), baseline %s; resolve the path or remove stale debt" %
                        (prefix, got.get(prefix, 0), old.get(prefix, 0)))


# ---------------------------------------------------------------------------
def main():
    cfg = json.loads(sys.stdin.read())
    root = cfg["root"]
    explain = bool(cfg.get("explain"))
    exempt = cfg.get("exempt", {})
    used = {}

    try:
        tree = Tree(root)
        decls = derive_declarations(tree)
        check_citations(tree, exempt.get("CITATION_EXEMPT", []), used)
        check_declarations(tree, set(exempt.get("DECLARATION_EXEMPT", [])), used, explain,
                           cfg.get("declaration_dir", ""))
        check_workflows(tree, set(exempt.get("WORKFLOW_EXEMPT", [])), used, explain)
        check_misplacement(tree, cfg.get("private_roots", []), set(decls), explain)
        check_machine_paths(tree, cfg.get("machine_baseline", ""))
        tree.require_all_read()
    except Broken as e:
        sys.stderr.write("BROKEN: %s\n" % e)
        return 2

    # -----------------------------------------------------------------------
    # AN EXEMPTION THAT SUPPRESSES NOTHING IS A FAILURE.
    #
    # Every exemption list in this engine is a place drift hides: the line was
    # written for a real reason, the reason was fixed, and the line stayed —
    # now silently licensing the next instance of the defect. So an exemption
    # that no longer suppresses anything does not pass quietly; it FAILS, and
    # names itself for deletion. That is what makes the escape hatch safe to
    # have: it cannot outlive its own justification.
    # -----------------------------------------------------------------------
    for key in sorted(exempt):
        for entry in sorted(exempt[key]):
            if entry not in used.get(key, set()):
                finding("STALE-EXEMPTION", ".publication-completeness",
                        "%s entry `%s` suppresses nothing any more. Either the defect "
                        "it excused is fixed — in which case DELETE THIS LINE — or the "
                        "path it names has moved and the exemption is now silently "
                        "licensing something else." % (key, entry))

    for check, path, msg in sorted(FINDINGS):
        sys.stdout.write("%s\t%s\t%s\n" % (check, path, msg))
    return 1 if FINDINGS else 0


if __name__ == "__main__":
    sys.exit(main())
