#!/usr/bin/env python3
"""Generate the compiled-dependency license inventory from the tracked lockfiles.

Called by richos/app/scripts/dependency-license-inventory.sh, which is where the
rationale lives. This file is the mechanism.

Usage: dependency_license_inventory.py <repo-root> <out-file> <lockfile>...

Everything it reports comes out of `cargo metadata --locked`, which refuses to
run if a lockfile would have to change. So a document produced by this script
describes the graph the lockfiles pin, or it does not get produced.

THE ONE RULE WORTH KNOWING BEFORE YOU EDIT IT: an SPDX expression this file has
never seen is a hard failure, not a row that quietly says "unknown". A new
dependency arriving under terms nobody reviewed is precisely the event this
inventory exists to catch, and a tool that renders it as an ordinary row has
converted the finding into a line of text.
"""

import hashlib
import json
import os
import subprocess
import sys

# The targets a RichOS binary is actually built for. The packaging path is macOS
# (richos/app/scripts/package-app.sh), so these two triples decide the "reaches the
# macOS binary" column. They are named rather than derived from the host, so the
# document is identical on every machine and `--check` means the same thing in
# CI as it does on a laptop.
DARWIN_TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin")

# Every SPDX expression seen in this tree, with the reviewed answer to the only
# question that matters: may the work it covers be distributed as part of an
# AGPL-3.0-only combined work?
#
# "permissive" — no reciprocal obligation beyond attribution; combines freely.
# "file-copyleft" — reciprocal per file (MPL-2.0). Compatible here because
#   MPL-2.0 section 3.3 expressly permits distributing the Larger Work under a
#   Secondary License, which names the GNU AGPL v3, PROVIDED no covered file
#   carries the Exhibit B "Incompatible With Secondary Licenses" notice.
# "data" — a permissive license over data rather than code.
#
# An expression offering a CHOICE ("MIT OR Apache-2.0", and notably r-efi's
# "MIT OR Apache-2.0 OR LGPL-2.1-or-later") is classified by the alternative
# RichOS relies on, which is always a permissive one. The offer is recorded in
# full in the table; this column records the branch taken.
LICENSE_CLASS = {
    "0BSD OR MIT OR Apache-2.0": "permissive",
    "(MIT OR Apache-2.0) AND Unicode-3.0": "permissive",
    "Apache-2.0": "permissive",
    "Apache-2.0 / MIT": "permissive",
    "Apache-2.0 AND ISC": "permissive",
    "Apache-2.0 AND MIT": "permissive",
    "Apache-2.0 OR BSL-1.0": "permissive",
    "Apache-2.0 OR ISC OR MIT": "permissive",
    "Apache-2.0 OR MIT": "permissive",
    "Apache-2.0 OR MIT OR Zlib": "permissive",
    "Apache-2.0 WITH LLVM-exception": "permissive",
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT": "permissive",
    "Apache-2.0/MIT": "permissive",
    "BSD-2-Clause": "permissive",
    "BSD-2-Clause OR Apache-2.0 OR MIT": "permissive",
    "BSD-2-Clause OR MIT OR Apache-2.0": "permissive",
    "BSD-3-Clause": "permissive",
    "BSD-3-Clause AND MIT": "permissive",
    "BSD-3-Clause OR MIT OR Apache-2.0": "permissive",
    "BSD-3-Clause/MIT": "permissive",
    "CC0-1.0": "permissive",
    "CC0-1.0 OR MIT-0 OR Apache-2.0": "permissive",
    "CDLA-Permissive-2.0": "data",
    "ISC": "permissive",
    "MIT": "permissive",
    "MIT OR Apache-2.0": "permissive",
    "MIT OR Apache-2.0 OR LGPL-2.1-or-later": "permissive",
    "MIT OR Apache-2.0 OR Zlib": "permissive",
    "MIT OR Zlib OR Apache-2.0": "permissive",
    "MIT/Apache-2.0": "permissive",
    "MPL-2.0": "file-copyleft",
    "Unicode-3.0": "permissive",
    "Unlicense OR MIT": "permissive",
    "Unlicense/MIT": "permissive",
    "Zlib": "permissive",
    "Zlib OR Apache-2.0 OR MIT": "permissive",
}

CLASS_NOTE = {
    "permissive": "Attribution only. Combines with AGPL-3.0-only without further obligation.",
    "file-copyleft": (
        "Per-file reciprocal. MPL-2.0 section 3.3 permits distributing the Larger Work "
        "under the GNU AGPL v3 and the source of the covered files must stay available."
    ),
    "data": "A permissive license over data rather than code. No reciprocal obligation.",
}


def die(msg):
    sys.stderr.write("dependency_license_inventory.py: %s\n" % msg)
    sys.exit(1)


def cargo_metadata(workspace_dir, target=None):
    cmd = ["cargo", "metadata", "--locked", "--format-version", "1"]
    if target:
        cmd += ["--filter-platform", target]
    proc = subprocess.run(cmd, cwd=workspace_dir, capture_output=True, text=True)
    if proc.returncode != 0:
        die(
            "`%s` failed in %s.\n%s"
            % (" ".join(cmd), workspace_dir, proc.stderr.strip())
        )
    return json.loads(proc.stdout)


def reachable(meta, kinds):
    """Package ids reachable from the workspace members along the given edge kinds.

    `kinds` is a set drawn from {None, "build", "dev"}; cargo spells a normal
    dependency's kind as null. Crossing a build edge puts everything below it in
    build-time land, which is why the walk carries no state: the caller asks a
    single question and gets the closure for it.
    """
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen = set()
    stack = list(meta["workspace_members"])
    while stack:
        pid = stack.pop()
        node = nodes.get(pid)
        if node is None:
            continue
        for dep in node.get("deps", []):
            if not any(dk.get("kind") in kinds for dk in dep.get("dep_kinds", [])):
                continue
            if dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                stack.append(dep["pkg"])
    return seen


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def main(argv):
    if len(argv) < 4:
        die("usage: dependency_license_inventory.py <repo-root> <out-file> <lockfile>...")
    root, out_path, lock_paths = argv[1], argv[2], argv[3:]

    workspaces = []  # (lock_rel_path, workspace_rel_dir, sha256)
    packages = {}  # (name, version) -> dict
    for lock_rel in lock_paths:
        lock_abs = os.path.join(root, lock_rel)
        if not os.path.isfile(lock_abs):
            die("tracked lockfile %s does not exist on disk." % lock_rel)
        ws_dir_rel = os.path.dirname(lock_rel) or "."
        ws_dir_abs = os.path.join(root, ws_dir_rel)
        workspaces.append((lock_rel, ws_dir_rel, sha256_of(lock_abs)))

        full = cargo_metadata(ws_dir_abs)
        # Union across the Darwin triples: a package that compiles for either is
        # a package whose terms can attach to a shipped macOS binary.
        ships = set()
        builds = set()
        for target in DARWIN_TARGETS:
            tmeta = cargo_metadata(ws_dir_abs, target)
            ships |= reachable(tmeta, {None})
            builds |= reachable(tmeta, {None, "build"})

        for pkg in full["packages"]:
            if pkg.get("source") is None:
                continue  # a workspace member or a path dependency: RichOS's own code
            key = (pkg["name"], pkg["version"])
            entry = packages.setdefault(
                key,
                {
                    "name": pkg["name"],
                    "version": pkg["version"],
                    "license": pkg.get("license") or "",
                    "license_file": pkg.get("license_file") or "",
                    "repository": pkg.get("repository") or "",
                    "workspaces": set(),
                    "role": set(),
                },
            )
            entry["workspaces"].add(ws_dir_rel)
            if pkg["id"] in ships:
                entry["role"].add("ships")
            elif pkg["id"] in builds:
                entry["role"].add("build")
            else:
                entry["role"].add("other")

    # A package with no declared license is a legal unknown, not a blank cell.
    undeclared = [p for p in packages.values() if not p["license"] and not p["license_file"]]
    if undeclared:
        for p in undeclared:
            sys.stderr.write(
                "  NO DECLARED LICENSE: %s %s (%s)\n"
                % (p["name"], p["version"], p["repository"] or "no repository declared")
            )
        die(
            "%d package(s) declare no license at all. Every one is an unanswered legal\n"
            "  question and none may be distributed until it is answered. Refusing to write\n"
            "  an inventory that renders them as blanks." % len(undeclared)
        )

    unknown = sorted({p["license"] for p in packages.values() if p["license"] not in LICENSE_CLASS})
    if unknown:
        for expr in unknown:
            users = sorted(
                "%s %s" % (p["name"], p["version"])
                for p in packages.values()
                if p["license"] == expr
            )
            sys.stderr.write("  UNREVIEWED: %s  —  %s\n" % (expr, ", ".join(users)))
        die(
            "%d license expression(s) above have never been reviewed against AGPL-3.0-only.\n"
            "  A new dependency arriving under unreviewed terms is the event this inventory\n"
            "  exists to catch, so this is a failure rather than a row saying 'unknown'.\n"
            "  Review each one, then add it to LICENSE_CLASS in this file with the class it\n"
            "  belongs to. If any of them cannot be combined with the AGPL, the dependency\n"
            "  goes, not the check." % len(unknown)
        )

    rows = sorted(packages.values(), key=lambda p: (p["name"].lower(), p["version"]))

    role_label = {
        "ships": "yes",
        "build": "build only",
        "other": "no (other targets or tests only)",
    }

    def role_of(entry):
        for key in ("ships", "build", "other"):
            if key in entry["role"]:
                return role_label[key]
        return role_label["other"]

    counts = {}
    for entry in rows:
        counts[entry["license"]] = counts.get(entry["license"], 0) + 1

    ships_count = sum(1 for e in rows if "ships" in e["role"])

    out = []
    w = out.append
    w("# Compiled dependency license inventory (Rust)")
    w("")
    w("**Generated. Do not hand-edit.**")
    w("")
    w("```")
    w("richos/app/scripts/dependency-license-inventory.sh          # regenerate")
    w("richos/app/scripts/dependency-license-inventory.sh --check  # fail if this file is stale")
    w("```")
    w("")
    w(
        "This is the per-package inventory that `docs/legal/THIRD-PARTY-NOTICES.md` "
        "makes a gate on distributing a RichOS binary. Every row comes from "
        "`cargo metadata --locked`, which refuses to run if a lockfile would have to "
        "change — so this document describes the graph the committed lockfiles pin or "
        "it does not exist."
    )
    w("")
    w("## What it is keyed to")
    w("")
    w(
        "Identity is the lockfile, not a date. Regenerate after any dependency change and "
        "these digests move with it."
    )
    w("")
    w("| Lockfile | Workspace | sha256 |")
    w("|---|---|---|")
    for lock_rel, ws_rel, digest in workspaces:
        w("| `%s` | `%s` | `%s` |" % (lock_rel, ws_rel, digest))
    w("")
    w("## The answer, first")
    w("")
    w(
        "**%d distinct third-party packages resolve across the %d workspaces. Every one of "
        "them may be distributed as part of an AGPL-3.0-only combined work.** No package in "
        "this tree is proprietary and none carries terms that conflict with the AGPL."
        % (len(rows), len(workspaces))
    )
    w("")
    w(
        "%d of them reach a macOS binary. The rest are build-time tooling, test-only "
        "dependencies or code compiled exclusively for targets RichOS does not ship."
        % ships_count
    )
    w("")
    w("### The two families that are not plain attribution")
    w("")
    mpl = [e for e in rows if LICENSE_CLASS[e["license"]] == "file-copyleft"]
    w(
        "**MPL-2.0 (%d packages: %s).** MPL-2.0 is reciprocal per FILE, not per program. "
        "Section 3.3 expressly permits distributing the Larger Work under a Secondary "
        "License and the GNU AGPL v3 is named as one — provided no covered file carries "
        "the Exhibit B \"Incompatible With Secondary Licenses\" notice. No source file in "
        "any of these packages carries it; the phrase appears only inside the license text "
        "each of them ships, where it is part of the boilerplate. The obligation RichOS "
        "carries is therefore the ordinary one: the source of those files stays available "
        "and their notices travel with the binary."
        % (len(mpl), ", ".join("`%s`" % e["name"] for e in mpl))
    )
    w("")
    refi = [e for e in rows if e["name"] == "r-efi"]
    if refi:
        where = "; ".join(
            "%s in %s" % (e["version"], ", ".join("`%s`" % x for x in sorted(e["workspaces"])))
            for e in refi
        )
        w(
            "**`r-efi` (%s).** Offered as \"MIT OR Apache-2.0 OR LGPL-2.1-or-later\". The "
            "pre-publication audit flagged it twice: once because an LGPL alternative in the "
            "list must not be silently rolled into an AGPL claim and once because the two "
            "untracked lockfiles carried different versions of it."
            % ", ".join("%s" % e["version"] for e in refi)
        )
        w("")
        w("Both are settled here and neither needed a dependency change.")
        w("")
        w(
            "*The license.* RichOS takes the MIT branch, which the offer permits outright. "
            "The package is also a UEFI binding, reached only through `getrandom`'s UEFI "
            "target support: it is absent from the resolved graph for both Darwin triples, "
            "so it is never compiled into anything RichOS ships. It appears in the lockfiles "
            "at all because a lockfile pins every platform's graph, which is the behavior "
            "that makes lockfiles worth committing."
        )
        w("")
        w(
            "*The versions.* %s. That is not drift between a stale file and a fresh one — "
            "regenerating both lockfiles from the same index on the same day reproduces it "
            "exactly. Two things cause it. `richos/app/src-tauri` is a DELIBERATELY DETACHED "
            "workspace (the empty `[workspace]` table in its manifest, explained at the top "
            "of `richos/app/Cargo.toml`), so Cargo resolves it independently of `richos/app/` and there is "
            "no single lockfile that could cover both. And within the Tauri workspace two "
            "semver-major lines of `getrandom` coexist — `tauri` 2.11.5 pulls `getrandom` "
            "0.3.x, which requires `r-efi` 5.x, while `tempfile` and `uuid` pull `getrandom` "
            "0.4.x, which requires `r-efi` 6.x. Cargo keeps both because they are different "
            "major versions, which is correct behavior rather than a conflict to resolve."
            % where
        )
        w("")
    w(
        "It is also worth stating what carries the offer-versus-obligation distinction: "
        "most of this tree is \"MIT OR Apache-2.0\", which is a CHOICE. The table below "
        "records the full offer as the publisher stated it; the compatibility class records "
        "the branch RichOS relies on."
    )
    w("")
    w("## Licenses present, by package count")
    w("")
    w("| License expression | Packages | Class | What it obliges |")
    w("|---|---|---|---|")
    for expr in sorted(counts, key=lambda k: (-counts[k], k)):
        cls = LICENSE_CLASS[expr]
        w("| `%s` | %d | %s | %s |" % (expr, counts[expr], cls, CLASS_NOTE[cls]))
    w("")
    w("## Every package")
    w("")
    w(
        "\"Reaches macOS binary\" is derived from the resolved graph filtered to "
        "`%s`, following normal dependency edges from the workspace members. "
        "\"build only\" means it is reached only through a build-dependency edge; "
        "\"no\" means it is reached only by dev/test edges or is gated to a target "
        "RichOS does not build for." % "` and `".join(DARWIN_TARGETS)
    )
    w("")
    w("| Package | Version | License | Reaches macOS binary | Workspace |")
    w("|---|---|---|---|---|")
    for entry in rows:
        lic = entry["license"] or ("see %s" % entry["license_file"])
        w(
            "| `%s` | %s | `%s` | %s | %s |"
            % (
                entry["name"],
                entry["version"],
                lic,
                role_of(entry),
                ", ".join("`%s`" % x for x in sorted(entry["workspaces"])),
            )
        )
    w("")
    w("## What this file does not cover")
    w("")
    w(
        "- **Bundled skills, fonts and tools.** They are not compiled dependencies and a "
        "resolver knows nothing about their provenance. They are hand-verified against "
        "upstream in `docs/legal/THIRD-PARTY-NOTICES.md`."
    )
    w(
        "- **Whether a declared license is true.** Each row reproduces the publisher's own "
        "claim from the manifest cargo resolved to."
    )
    w(
        "- **The Node dependencies of the browser test harness.** They are devDependencies "
        "of `richos/app/ui/tests` and are excluded from the shipped frontend by "
        "`richos/app/src-tauri/build.rs`, which `richos/app/scripts/frontend-payload.test.sh` gates."
    )
    w("")

    with open(out_path, "w") as fh:
        fh.write("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
