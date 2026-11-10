#!/usr/bin/env python3
"""first-party-license.test.py — RichOS's own material is under the root AGPL-3.0 and no other
license, refused at the commit, the land and the push backstop.

Every case is a throwaway repository with the real shim, installer, autocheck.py and
first-party-license.py installed as git hooks, holding copies of this repository's own registry
(.richos/vendored-material), RichPass.html, brand page, licensing page and Gradle wrapper. A
stand-in proof-for.sh that selects nothing answers the coverage lookup every commit makes, and no
change is under richos/app, so autocheck's lint is never reached. Nothing touches this repository
or its hooks. The strings that trigger the check are assembled from pieces so this file does not
itself declare anything.

  REFUSED  the exact RichPass case of 2026-11-09: richos/tools/richpass/LICENSE (MIT) and the line
           in RichPass.html putting its first-party source under MIT, also inside a vendored
           region; an SPDX identifier, a manifest field, a license header and a public-domain
           dedication in RichOS files; a root LICENSE that is no longer the AGPL; a Gradle
           wrapper nobody registered; AGPL-3.0-or-later; a file whose name holds a space;
           `commit -a`; a land of a commit that skipped the hooks,
           before main moves; the push backstop of a merge that skipped them
  PASSES   registered third-party material with its own license (the RichPass vendor packages);
           a Gradle wrapper registered in the same change; the brand exclusion ("not licensed",
           "All rights ... reserved") and AGPL statements; third-party notices reproduced in a
           RichOS file the three declared ways; the notices page; descriptive mentions
  TREE     --all over the copied real files passes (no false refusal on them)
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
AUTOCHECK = Path(os.environ.get("AUTOCHECK_UNDER_TEST", HERE / "autocheck"))
HANG_GUARD = 600

CHECK = "richos/app/scripts/autocheck/first-party-license.py"
RICHPASS = "richos/tools/richpass/RichPass.html"
RICHPASS_LICENSE = "richos/tools/richpass/LICENSE"
REAL = (".richos/vendored-material", RICHPASS, "docs/legal/BRAND-ASSETS.md", "docs/legal/LICENSING.md",
        "richos/mobile/native-android/gradlew", "richos/tools/richpass/vendor/tldts/LICENSE")

MIT = "MIT " + "License"
OLD_LICENSE = (f"{MIT}\n\nCopyright (c) 2025 Alex Booster\n\nPermission is hereby granted, free of charge, to any "
               "person obtaining a copy\nof this software and associated documentation files (the \"Software\"), "
               "to deal\nin the Software without restriction.\n")
OLD_LINE = ("  <!-- RichPass first-party source is " + "MIT licensed. See LICENSE and vendor/ for original\n"
            "       third-party sources, exact versions and notices. No build or network is needed. -->\n")
REFUSAL = "REFUSED: first-party material under a license other than RichOS's AGPL-3.0"
SPDX = "SPDX-License-" + "Identifier: "
LICENSED_UNDER = "Licensed under the " + "Apache License, Version 2.0"
TAG = "@lic" + "ense " + "MIT"
OLD_NEEDLE = "source is " + "MIT"


class Fixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="first-party-license-")
        self.base = Path(self.tmp.name)
        (self.base / "gitconfig").write_text("[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n"
                                              "[init]\n\tdefaultBranch = main\n")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith(("GIT_", "RICHOS_AUTOCHECK"))}
        self.env.update(GIT_CONFIG_GLOBAL=str(self.base / "gitconfig"), GIT_CONFIG_NOSYSTEM="1",
                        RICHOS_ESCALATION_LEDGER=str(self.base / "ledger.jsonl"),
                        RICHOS_AUTOCHECK_PROOF_ROOT=str(self.base / "proof-runs"), PYTHONDONTWRITEBYTECODE="1")
        self.repo = self.base / "repo"

    def tearDown(self):
        self.tmp.cleanup()

    def run_(self, argv):
        return subprocess.run(argv, cwd=self.repo, env=self.env, capture_output=True, text=True, timeout=HANG_GUARD)

    def git(self, *args, expect=0):
        result = self.run_(["git", *args])
        if expect is not None:
            self.assertEqual(result.returncode, expect, f"git {' '.join(args)}\n{result.stdout}{result.stderr}")
        return result

    def write(self, rel, text):
        path = self.repo / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def read(self, rel):
        return (self.repo / rel).read_text()

    def head(self):
        return self.git("rev-parse", "HEAD").stdout.strip()

    def make(self):
        """main holds the real files and the hooks; a branch is checked out."""
        self.repo.mkdir()
        for rel in REAL:
            target = self.repo / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / rel, target)
        self.write("LICENSE", "                    GNU AFFERO GENERAL PUBLIC LICENSE\n"
                              "                       Version 3, 19 November 2007\n")
        dest = self.repo / "richos/app/scripts/autocheck"
        dest.mkdir(parents=True)
        for name in ("autocheck.py", "shim.sh", "install.sh", "first-party-license.py"):
            if (AUTOCHECK / name).is_file():
                shutil.copy(AUTOCHECK / name, dest / name)
        self.write("richos/app/scripts/proof-for.sh", "#!/usr/bin/env bash\nexit 0\n")
        # the commit check reads the machine's storage settings for its fast-check durations
        lib = self.repo / "richos/engine/scripts/lib"
        lib.mkdir(parents=True)
        shutil.copy(ROOT / "richos/engine/scripts/lib/machine_settings.py", lib / "machine_settings.py")
        self.git("init", "-q")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "base")
        out = self.run_(["bash", str(AUTOCHECK / "install.sh"), str(self.repo)])
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.git("checkout", "-q", "-b", "feature")

    def commit(self, message, *extra, expect=0):
        self.git("add", "-A")
        return self.git("commit", "-q", *extra, "-m", message, expect=expect)

    def refused(self, out, *named):
        text = out.stdout + out.stderr
        self.assertIn(REFUSAL, text)
        for where in named:
            self.assertIn(where, text)
        return text

    def line_of(self, rel, needle):
        return next(n for n, line in enumerate(self.read(rel).splitlines(), 1) if needle in line)

    def insert_after(self, rel, anchor, text):
        body = self.read(rel)
        at = body.index(anchor)
        at = body.index("\n", at) + 1
        self.write(rel, body[:at] + text + body[at:])


class Refused(Fixture):
    def test_the_richpass_license_file_is_refused(self):
        # 776b2feef: RichPass arrived with its own MIT LICENSE, copied from PassBooster.
        self.make()
        before = self.head()
        self.write(RICHPASS_LICENSE, OLD_LICENSE)
        out = self.commit("add a license file", expect=1)
        text = self.refused(out, f"{RICHPASS_LICENSE}: a license file outside registered third-party material")
        self.assertIn(".richos/vendored-material", text)  # it says what to do
        self.assertIn("root LICENSE", text)
        self.assertEqual(self.head(), before)

    def test_the_richpass_line_is_refused(self):
        # 776b2feef RichPass.html around line 587: the comment putting its own source under MIT.
        self.make()
        self.insert_after(RICHPASS, '<meta name="application-version"', OLD_LINE)
        line = self.line_of(RICHPASS, OLD_NEEDLE)
        out = self.commit("restore the old comment", expect=1)
        self.refused(out, f"{RICHPASS}:{line}: a license declaration other than AGPL-3.0")

    def test_the_line_is_refused_even_inside_a_vendored_region(self):
        # A region holds other people's notices; a line about RichOS's own source is still judged.
        self.make()
        self.insert_after(RICHPASS, "BEGIN VENDORED ARGON2ID", OLD_LINE)
        line = self.line_of(RICHPASS, OLD_NEEDLE)
        self.refused(self.commit("hide it in a vendored block", expect=1), f"{RICHPASS}:{line}:")

    def test_identifiers_fields_headers_and_dedications_are_refused(self):
        self.make()
        self.write("richos/tools/one/tool.sh", f"#!/bin/sh\n# {SPDX}MIT\necho hi\n")
        self.write("richos/tools/two/package.json", '{\n  "name": "two",\n  "lic' + 'ense": "MIT"\n}\n')
        self.write("richos/tools/three/main.js", f"/*\n * {LICENSED_UNDER}\n */\n")
        self.write("richos/tools/four/README.md", "# Four\n\nThis tool is released into the public " + "domain.\n")
        self.write("richos/tools/five/index.js", f"/*! {TAG} */\n")
        self.write("richos/tools/six/Cargo.toml", '[package]\nname = "six"\nlic' + 'ense = "BSD-3-Clause"\n')
        out = self.commit("four ways to say it", expect=1)
        self.refused(out, "richos/tools/one/tool.sh:2: an SPDX identifier other than AGPL-3.0-only (MIT)",
                     "richos/tools/two/package.json:3: a package.json license field other than AGPL-3.0-only (MIT)",
                     "richos/tools/three/main.js:2:", "richos/tools/four/README.md:3:",
                     "richos/tools/five/index.js:1:",
                     "richos/tools/six/Cargo.toml:3: a Cargo.toml license field other than AGPL-3.0-only (BSD-3-Clause)")

    def test_agpl_or_later_is_refused(self):
        # Review P2: LICENSING.md grants version 3 and no later version; only AGPL-3.0-only is ours.
        self.make()
        self.write("richos/tools/example/main.js", f"// {SPDX}AGPL-3.0-or-later\n")
        self.write("richos/tools/example/package.json", '{\n  "lic' + 'ense": "AGPL-3.0-or-later"\n}\n')
        out = self.commit("a later version", expect=1)
        self.refused(out, "richos/tools/example/main.js:1: an SPDX identifier other than AGPL-3.0-only (AGPL-3.0-or-later)",
                     "richos/tools/example/package.json:2: a package.json license field other than AGPL-3.0-only "
                     "(AGPL-3.0-or-later)")

    def test_a_file_whose_name_has_a_space_is_judged(self):
        # Review P2: git ends the +++ header of such a path with a tab, which hid the file.
        self.make()
        self.write("richos/tools/example/My Tool.js", f"// {SPDX}MIT\n")
        self.refused(self.commit("a spaced name", expect=1),
                     "richos/tools/example/My Tool.js:1: an SPDX identifier other than AGPL-3.0-only (MIT)")
        self.commit("a spaced name, unchecked", "--no-verify")
        out = self.run_(["python3", CHECK, "--range", "HEAD~1..HEAD", "--what", "check"])
        self.assertEqual(out.returncode, 1, out.stdout + out.stderr)
        self.refused(out, "richos/tools/example/My Tool.js:1:")

    def test_a_root_license_that_is_no_longer_the_agpl_is_refused(self):
        self.make()
        self.write("LICENSE", OLD_LICENSE)
        self.refused(self.commit("swap the root license", expect=1), "LICENSE:1: the root LICENSE is no longer")

    def test_an_unregistered_gradle_wrapper_and_a_bare_mark_are_refused(self):
        self.make()
        (self.repo / "richos/tools/sample").mkdir(parents=True)
        shutil.copy(self.repo / "richos/mobile/native-android/gradlew", self.repo / "richos/tools/sample/gradlew")
        self.write("richos/tools/seven/notice.py", f'TEXT = "jq is distributed under the {MIT}"  # third-party-notice:\n')
        out = self.commit("an unregistered wrapper", expect=1)
        self.refused(out, "richos/tools/sample/gradlew:18: an SPDX identifier other than AGPL-3.0-only (Apache-2.0)",
                     "richos/tools/seven/notice.py:1:")

    def test_commit_a_is_judged_on_what_it_commits(self):
        self.make()
        self.insert_after(RICHPASS, '<meta name="application-version"', OLD_LINE)
        out = self.git("commit", "-q", "-a", "-m", "commit -a", expect=1)
        self.refused(out, f"{RICHPASS}:")

    def test_a_land_of_a_commit_that_skipped_the_hooks_is_refused_before_main_moves(self):
        self.make()
        self.write(RICHPASS_LICENSE, OLD_LICENSE)
        self.commit("skip the check", "--no-verify")
        self.git("checkout", "-q", "main")
        before = self.head()
        out = self.git("merge", "--no-ff", "-m", "land it", "feature", expect=None)
        self.assertNotEqual(out.returncode, 0, out.stdout + out.stderr)
        self.refused(out, "MERGE INTO MAIN REFUSED", f"{RICHPASS_LICENSE}: a license file")
        self.assertEqual(self.head(), before)
        self.git("merge", "--abort", expect=None)

    def test_the_push_backstop_refuses_a_merge_that_skipped_the_hooks(self):
        self.make()
        remote = self.base / "remote.git"
        subprocess.run(["git", "init", "-q", "--bare", str(remote)], env=self.env, check=True)
        self.git("remote", "add", "origin", str(remote))
        self.git("push", "-q", "--no-verify", "origin", "main")
        self.insert_after(RICHPASS, '<meta name="application-version"', OLD_LINE)
        self.commit("skip the check", "--no-verify")
        self.git("checkout", "-q", "main")
        self.git("merge", "-q", "--no-verify", "--no-ff", "-m", "land it unchecked", "feature")
        out = self.git("push", "origin", "main", expect=None)
        self.assertNotEqual(out.returncode, 0, out.stdout + out.stderr)
        self.refused(out, "PUSH REFUSED", f"{RICHPASS}:")


class Passes(Fixture):
    def test_registered_third_party_keeps_its_own_license(self):
        self.make()
        self.write("richos/tools/richpass/vendor/tldts-core/LICENSE", OLD_LICENSE)
        self.write("richos/tools/richpass/vendor/tldts/dist/index.js", f"/*! {TAG} */\n// {SPDX}MIT\n")
        self.commit("re-vendor tldts")

    def test_a_wrapper_registered_in_the_same_change_passes(self):
        self.make()
        (self.repo / "richos/tools/sample").mkdir(parents=True)
        shutil.copy(self.repo / "richos/mobile/native-android/gradlew", self.repo / "richos/tools/sample/gradlew")
        row = ["richos/tools/sample/gradlew", "third-party", "Apache-2.0", "the original authors (Gradle)",
               "gradle/gradle", "Gradle 9.7.1", "2026-11-10 fixture", "high", "verbatim", "fixture"]
        with open(self.repo / ".richos/vendored-material", "a") as registry:
            registry.write("\t".join(row) + "\n")
        self.commit("vendor a wrapper and say whose it is")

    def test_the_brand_exclusion_and_agpl_statements_pass(self):
        self.make()
        self.write("docs/legal/BRAND-ASSETS.md", self.read("docs/legal/BRAND-ASSETS.md") +
                   "\nThe RichPass name, logo and artwork are not licensed under the AGPL. All rights in them are\n"
                   "reserved. They are not licensed under the " + MIT + " either.\n"
                   "RichPass is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0-only).\n"
                   f"// {SPDX}AGPL-3.0-only\n")
        self.write("richos/tools/eight/package.json", '{\n  "lic' + 'ense": "AGPL-3.0-only"\n}\n')
        self.commit("brand wording")

    def test_notices_reproduced_in_a_richos_file_pass(self):
        self.make()
        self.write("richos/tools/nine/page.html",
                   "<!--\nThe RichOS page.\n\nThe third-party code embedded in this file keeps its own license,\n"
                   f"reproduced below.\n\nsome-lib 1.0\n{MIT}\nThis Font Software is licensed under the SIL "
                   "Open Font License, Version 1.1.\n-->\n<!-- BEGIN VENDORED SOME-LIB -->\n"
                   f"<script>/*! {TAG} */</script>\n<!-- END VENDORED SOME-LIB -->\n")
        self.write("richos/tools/nine/notes.py",
                   f'NOTE = "jq is distributed under the {MIT}"  # third-party-notice: jq\'s license, in its notice\n')
        self.commit("embed a notice")

    def test_the_notices_page_and_descriptive_mentions_pass(self):
        self.make()
        self.write("docs/legal/THIRD-PARTY-NOTICES.md", f"# Notices\n\nsome-lib is licensed under the {MIT}.\n")
        self.write("richos/tools/ten/README.md", "Two vendored MIT-licensed skills were rewritten by mistake.\n"
                   "Each bundled work keeps the license its notice names.\n")
        self.commit("describe licenses")


class Tree(Fixture):
    def test_all_passes_over_the_real_files(self):
        # The copied registry, RichPass.html (its embedded notices), the brand and licensing
        # pages and the registered Gradle wrapper: nothing in them is refused.
        self.make()
        out = self.run_(["python3", CHECK, "--all"])
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertIn("nothing refused", out.stderr)

    def test_all_names_an_unregistered_license_file(self):
        self.make()
        self.write(RICHPASS_LICENSE, OLD_LICENSE)
        self.git("add", "-A")
        out = self.run_(["python3", CHECK, "--all", "--what", "check"])
        self.assertEqual(out.returncode, 1, out.stdout + out.stderr)
        self.refused(out, "CHECK REFUSED", RICHPASS_LICENSE)


if __name__ == "__main__":
    unittest.main(verbosity=2)
