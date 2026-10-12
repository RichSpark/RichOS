#!/usr/bin/env python3
"""dependency-pins.test.py — a branch commit that changes a file the engine's
verification-input selector pins (richos/engine/scripts/lib/verification-dependencies.json)
is refused unless the same change renews that pin.

Every commit case is a throwaway repository with the real shim, installer, autocheck.py and
dependency-pins.py, installed as git hooks, and a small declaration that pins a node source, a
hook-reader source and a repository-rooted external reader. Nothing touches this repository or
its hooks, and no change is under richos/app, so autocheck's lint is never reached; a stand-in
proof-for.sh that selects nothing answers the coverage lookup every commit makes.

  REFUSED  a changed node source, hook-reader source or external reader whose pin was not
           renewed; the same through `commit -a`; one committed earlier without the hooks;
           a pin typed wrong by hand
  PASSES   the same change with the pin renewed by the command the refusal names; a change to
           a file nothing pins
  PARITY   the digests the renewal writes are the ones verification_inputs.py accepts, for a
           CRLF node source, a hook-reader source and an external reader
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
AUTOCHECK = Path(os.environ.get("AUTOCHECK_UNDER_TEST", HERE / "autocheck"))
ENGINE_LIB = HERE.parents[1] / "engine" / "scripts" / "lib"
HANG_GUARD = 600

DECLARATION = "richos/engine/scripts/lib/verification-dependencies.json"
NODE = "richos/engine/scripts/reader.sh"
HOOK = "richos/engine/scripts/lib/helper.sh"
EXTERNAL = "richos/tools/external.py"
FREE = "richos/engine/scripts/unpinned.sh"
RENEW = "python3 richos/app/scripts/autocheck/dependency-pins.py --renew"
SELECTOR = "richos/app/scripts/proof-for.sh"


class Fixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="dependency-pins-")
        self.base = Path(self.tmp.name)
        (self.base / "gitconfig").write_text("[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n"
                                              "[init]\n\tdefaultBranch = main\n")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith(("GIT_", "RICHOS_AUTOCHECK"))}
        self.env.update(GIT_CONFIG_GLOBAL=str(self.base / "gitconfig"), GIT_CONFIG_NOSYSTEM="1",
                        RICHOS_ESCALATION_LEDGER=str(self.base / "ledger.jsonl"),
                        RICHOS_AUTOCHECK_PROOF_ROOT=str(self.base / "proof-runs"))
        self.repo = self.base / "repo"

    def tearDown(self):
        self.tmp.cleanup()

    def run_(self, argv, env=None):
        return subprocess.run(argv, cwd=self.repo, env=env or self.env, capture_output=True, text=True,
                              timeout=HANG_GUARD)

    def git(self, *args, expect=0):
        result = self.run_(["git", *args])
        if expect is not None:
            self.assertEqual(result.returncode, expect, f"git {' '.join(args)}\n{result.stdout}{result.stderr}")
        return result

    def write(self, rel, text, newline=None):
        path = self.repo / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "w", newline=newline) as handle:
            handle.write(text)

    def head(self):
        return self.git("rev-parse", "HEAD").stdout.strip()

    def renew(self, *paths):
        out = self.run_(["python3", "richos/app/scripts/autocheck/dependency-pins.py", "--renew", *paths])
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        return out

    def make(self, node_newline=None):
        """A repository whose declaration pins NODE, HOOK and EXTERNAL by their current bytes,
        committed on main with the hooks installed, and a branch checked out."""
        self.repo.mkdir()
        self.write(NODE, "echo reader v1\n", newline=node_newline)
        self.write(HOOK, "helper() { :; }\n")
        self.write(EXTERNAL, "print('external v1')\n")
        self.write(FREE, "echo nobody pins me\n")
        # the commit check runs the verifier from the commit; a tree without it refuses
        self.write("richos/engine/scripts/lib/verification_inputs.py",
                   (ENGINE_LIB / "verification_inputs.py").read_text())
        self.write("richos/engine/scripts/lib/machine_settings.py",
                   (ENGINE_LIB / "machine_settings.py").read_text())
        self.write(DECLARATION, json.dumps({
            "schema": 1, "status": "fixture", "config_keys": [],
            "nodes": {"scripts/reader.sh": {
                "source": "scripts/reader.sh", "sha256": "0" * 64, "evidence": "fixture reader",
                "keys": [], "edges": [],
                "external": [{"root": "repository", "path": "richos/tools/external.py",
                              "sha256": "0" * 64, "evidence": "fixture external reader"}]}},
            "units": {},
            "hook_readers": {"scripts/hook-reader.test.sh": {
                "commands": [], "evidence": "fixture hook reader",
                "sources": {"scripts/lib/helper.sh": "0" * 64}}},
        }, indent=2) + "\n")
        dest = self.repo / "richos/app/scripts/autocheck"
        dest.mkdir(parents=True)
        for name in ("autocheck.py", "shim.sh", "install.sh", "dependency-pins.py"):
            if (AUTOCHECK / name).is_file():
                shutil.copy(AUTOCHECK / name, dest / name)
        # A commit with no app change still asks the land's selector (autocheck branch_selection,
        # 1b25b64d1) and refuses when there is none. This one maps every change and selects
        # nothing, so what passes or refuses here is the pin check alone.
        self.write(SELECTOR, "#!/usr/bin/env bash\nexit 0\n")
        self.git("init", "-q")
        self.renew(NODE, HOOK, EXTERNAL)
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "base")
        out = self.run_(["bash", str(AUTOCHECK / "install.sh"), str(self.repo)])
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.git("checkout", "-q", "-b", "feature")

    def refused(self, out, path):
        text = out.stdout + out.stderr
        self.assertIn("COMMIT REFUSED: a changed file is pinned in verification-dependencies.json", text)
        self.assertIn(path, text)
        self.assertIn(f"{RENEW} {path}", text)
        return text


class Refused(Fixture):
    def test_a_changed_node_source_without_its_pin_renewed_is_refused(self):
        # 2026-09-30: 108 stale pins reached main from ten branches whose every commit passed.
        self.make()
        before = self.head()
        self.write(NODE, "echo reader v2\n")
        self.git("add", "-A")
        text = self.refused(self.git("commit", "-m", "edit a pinned reader", expect=1), NODE)
        self.assertIn('node "scripts/reader.sh"', text)
        self.assertIn("added lines for a new config key", text)
        self.assertEqual(self.head(), before)

    def test_a_changed_hook_reader_source_is_refused(self):
        self.make()
        self.write(HOOK, "helper() { echo changed; }\n")
        self.git("add", "-A")
        text = self.refused(self.git("commit", "-m", "edit a hook reader", expect=1), HOOK)
        self.assertIn('hook reader "scripts/hook-reader.test.sh"', text)

    def test_a_changed_external_reader_is_refused(self):
        self.make()
        self.write(EXTERNAL, "print('external v2')\n")
        self.git("add", "-A")
        text = self.refused(self.git("commit", "-m", "edit an external reader", expect=1), EXTERNAL)
        self.assertIn("external reader", text)

    def test_commit_all_is_checked_on_the_index_it_commits(self):
        self.make()
        before = self.head()
        self.write(NODE, "echo reader via -a\n")
        self.refused(self.git("commit", "-a", "-m", "edit through -a", expect=1), NODE)
        self.assertEqual(self.head(), before)

    def test_a_stale_pin_committed_earlier_without_the_hooks_refuses_the_next_commit(self):
        # The land diffs the whole branch, so the whole branch is asked, not only the staged files.
        self.make()
        self.write(NODE, "echo reader v2\n")
        self.git("add", "-A")
        self.git("-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", "skipped the hooks")
        self.write(FREE, "echo an innocent change\n")
        self.git("add", "-A")
        self.refused(self.git("commit", "-m", "innocent", expect=1), NODE)

    def test_a_pin_typed_wrong_by_hand_is_refused(self):
        self.make()
        path = self.repo / DECLARATION
        path.write_text(path.read_text().replace(
            json.loads(path.read_text())["hook_readers"]["scripts/hook-reader.test.sh"]["sources"]
            ["scripts/lib/helper.sh"], "f" * 64))
        self.git("add", "-A")
        self.refused(self.git("commit", "-m", "retype a pin", expect=1), HOOK)


class Passes(Fixture):
    def test_the_same_change_with_its_pin_renewed_by_the_named_command_passes(self):
        self.make()
        self.write(NODE, "echo reader v2\n")
        self.write(HOOK, "helper() { echo changed; }\n")
        self.write(EXTERNAL, "print('external v2')\n")
        out = self.renew(NODE, HOOK, EXTERNAL)
        self.assertIn("3 pin(s) renewed", out.stdout)
        self.git("add", "-A")
        before = self.head()
        self.git("commit", "-q", "-m", "edit and renew")
        self.assertNotEqual(self.head(), before)

    def test_a_change_nothing_pins_passes(self):
        self.make()
        self.write(FREE, "echo still nobody pins me\n")
        self.git("add", "-A")
        out = self.git("commit", "-m", "unpinned")
        self.assertNotIn("COMMIT REFUSED", out.stdout + out.stderr)


class Floor(Fixture):
    """The selector's own floor refusals, taken at the commit: `omitted known key reads` and
    `unqualified reader`, for a reader the commit changed (2026-10-09: two branches reached the
    land's merge gate and were refused there for them)."""
    LIB = "richos/engine/scripts/lib/verification_inputs.py"

    def declare(self, **row):
        path = self.repo / DECLARATION
        declaration = json.loads(path.read_text())
        declaration["config_keys"] = ["TOKEN"]
        declaration["nodes"]["scripts/reader.sh"].update(row)
        path.write_text(json.dumps(declaration, indent=2) + "\n")

    def start(self):
        self.make()
        self.write(self.LIB, (ENGINE_LIB / "verification_inputs.py").read_text())
        self.declare()
        self.renew(NODE)
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "the selector joins the fixture")

    def test_an_undeclared_known_key_read_is_refused_and_the_declaration_passes(self):
        self.start()
        before = self.head()
        self.write(NODE, 'echo "$TOKEN"\n')
        self.renew(NODE)
        self.git("add", "-A")
        text = self.git("commit", "-m", "read TOKEN", expect=1)
        text = text.stdout + text.stderr
        self.assertIn("omitted known key reads in scripts/reader.sh: TOKEN", text)
        self.assertEqual(self.head(), before)
        self.declare(keys=["TOKEN"])
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "read TOKEN, declared")
        self.assertNotEqual(self.head(), before)

    def test_an_edge_to_a_reader_with_no_node_is_refused_and_the_node_passes(self):
        self.start()
        before = self.head()
        self.declare(edges=[{"to": "scripts/new-helper.py"}])
        self.git("add", "-A")
        text = self.git("commit", "-m", "edge to nothing", expect=1)
        self.assertIn("unqualified reader scripts/new-helper.py", text.stdout + text.stderr)
        self.assertEqual(self.head(), before)
        self.write("richos/engine/scripts/new-helper.py", "print(1)\n")
        path = self.repo / DECLARATION
        declaration = json.loads(path.read_text())
        declaration["nodes"]["scripts/new-helper.py"] = {
            "source": "scripts/new-helper.py", "sha256": "0" * 64, "evidence": "fixture helper", "keys": []}
        path.write_text(json.dumps(declaration, indent=2) + "\n")
        self.renew("richos/engine/scripts/new-helper.py")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "edge and node")
        self.assertNotEqual(self.head(), before)


    def mutate(self, edit):
        path = self.repo / DECLARATION
        declaration = json.loads(path.read_text())
        edit(declaration)
        path.write_text(json.dumps(declaration, indent=2) + "\n")

    def land_on_main(self):
        self.git("checkout", "-q", "main")
        self.git("merge", "-q", "--ff-only", "feature")
        self.git("checkout", "-q", "feature")

    def helper_node(self, declaration):
        self.write("richos/engine/scripts/new-helper.py", "print(1)\n")
        declaration["nodes"]["scripts/new-helper.py"] = {
            "source": "scripts/new-helper.py", "sha256": "0" * 64, "evidence": "fixture helper", "keys": []}
        declaration["nodes"]["scripts/reader.sh"]["edges"] = [{"to": "scripts/new-helper.py"}]

    def test_deleting_the_node_of_a_qualified_edge_target_is_refused(self):
        # review rv-20261009T063406Z-17691477: the edge existed at the merge-base, qualified.
        self.start()
        self.mutate(self.helper_node)
        self.renew("richos/engine/scripts/new-helper.py")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "edge and node")
        self.land_on_main()
        before = self.head()
        self.mutate(lambda d: (d["nodes"].pop("scripts/new-helper.py"),
                               d["nodes"]["scripts/reader.sh"].update(evidence="edited parent")))
        self.git("add", "-A")
        text = self.git("commit", "-m", "drop the helper node", expect=1)
        self.assertIn("unqualified reader scripts/new-helper.py", text.stdout + text.stderr)
        self.assertEqual(self.head(), before)

    def test_a_new_unit_root_without_a_node_is_refused(self):
        self.start()
        before = self.head()
        self.mutate(lambda d: d["units"].update({"scripts/new-unit.test.sh": "scripts/new-helper.py"}))
        self.git("add", "-A")
        text = self.git("commit", "-m", "unit root with no node", expect=1)
        self.assertIn("unqualified reader scripts/new-helper.py", text.stdout + text.stderr)
        self.assertEqual(self.head(), before)

    def test_a_crlf_reader_with_an_undeclared_key_is_refused(self):
        self.start()
        before = self.head()
        self.write(NODE, 'echo "$TOKEN"\n', newline="\r\n")
        self.renew(NODE)
        self.git("add", "-A")
        text = self.git("commit", "-m", "read TOKEN in CRLF", expect=1)
        self.assertIn("omitted known key reads in scripts/reader.sh: TOKEN", text.stdout + text.stderr)
        self.assertEqual(self.head(), before)

    def test_an_edge_already_unqualified_at_the_merge_base_is_not_this_commits(self):
        self.start()
        self.mutate(lambda d: d["nodes"]["scripts/reader.sh"].update(edges=[{"to": "scripts/gone.py"}]))
        self.git("add", "-A")
        self.git("commit", "--no-verify", "-q", "-m", "already broken")
        self.land_on_main()
        self.write(FREE, "echo unrelated\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "unrelated")

    def refused_text(self, message, expect, stage=True):
        if stage:
            self.git("add", "-A")
        out = self.git("commit", "-m", message, expect=1)
        text = out.stdout + out.stderr
        self.assertIn(expect, text)
        return text

    # review rv-20261009T064208Z-fdb53114: three more places the commit's own logic differed from
    # the verifier's. Each is refused here by the verifier's own code, run on the staged tree.
    def test_an_edge_target_emptied_to_a_blank_row_is_refused(self):
        self.start()
        self.mutate(self.helper_node)
        self.renew("richos/engine/scripts/new-helper.py")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "edge and qualified node")
        self.mutate(lambda d: d["nodes"].update({"scripts/new-helper.py": {}}))
        self.refused_text("blank the helper row", "unqualified reader scripts/new-helper.py")

    def test_a_unit_root_pointing_at_a_blank_row_is_refused(self):
        self.start()
        self.mutate(lambda d: (d["nodes"].update({"scripts/new-helper.py": {}}),
                               d["units"].update({"scripts/new-unit.test.sh": "scripts/new-helper.py"})))
        self.refused_text("unit root on a blank row", "unqualified reader scripts/new-helper.py")

    def test_an_unstaged_edit_to_an_external_reader_cannot_hide_an_undeclared_key_read(self):
        # The verifier reads externals from the checkout it is given; here that is the staged tree,
        # so the working copy (edited, unstaged) must not stop the key check from running.
        self.start()
        self.mutate(lambda d: d["nodes"]["scripts/reader.sh"].update(external=[{
            "root": "repository", "path": EXTERNAL, "evidence": "fixture external reader",
            "sha256": "0" * 64}]))
        self.renew(EXTERNAL)
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "declare the external reader")
        self.write(NODE, 'echo "$TOKEN"\n')
        self.renew(NODE)
        self.git("add", "-A")
        self.write(EXTERNAL, "print('edited and not staged')\n")
        self.refused_text("read TOKEN", "omitted known key reads in scripts/reader.sh: TOKEN",
                          stage=False)

    def test_a_check_the_verifier_cannot_finish_refuses_instead_of_passing(self):
        # Without qualification evidence the verifier stops before its key check; that is a refusal
        # of its own (new against the merge-base), never a pass by silence.
        self.start()
        self.write(NODE, 'echo "$TOKEN"\n')
        self.mutate(lambda d: d["nodes"]["scripts/reader.sh"].update(evidence=""))
        self.renew(NODE)
        self.refused_text("read TOKEN, no evidence", "reader has no qualification evidence")

    def test_a_finding_the_merge_base_has_through_another_parent_is_not_this_commits(self):
        # Two nodes that reach the same unqualified target: the finding is the merge-base's whether
        # the node that reaches it sorts first or last.
        self.start()
        self.mutate(lambda d: d["nodes"]["scripts/reader.sh"].update(edges=[{"to": "scripts/gone.py"}]))
        self.git("add", "-A")
        self.git("commit", "--no-verify", "-q", "-m", "already broken")
        self.land_on_main()
        self.mutate(lambda d: d["nodes"].update({"scripts/a-reader.sh": {
            "source": "scripts/reader.sh", "sha256": "0" * 64, "evidence": "fixture", "keys": [],
            "edges": [{"to": "scripts/gone.py"}]}}))
        self.renew(NODE)
        self.git("add", "-A")
        before = self.head()
        self.git("commit", "-q", "-m", "another reader of the same missing target")
        self.assertNotEqual(self.head(), before)


    # review rv-20261009T065245Z-5a176a41: three defects in the run-the-verifier check.
    def test_a_staged_tree_without_the_verifier_refuses(self):
        self.start()
        self.write(NODE, 'echo "$TOKEN"\n')
        self.renew(NODE)
        self.git("rm", "-q", "--cached", self.LIB)
        self.refused_text("read TOKEN, no verifier", "the reader check could not run", stage=False)

    def test_deleting_the_declaration_refuses_when_the_merge_base_had_one(self):
        self.start()
        self.land_on_main()
        self.write(NODE, 'echo "$TOKEN"\n')
        self.git("add", "-A")
        self.git("rm", "-q", "--cached", DECLARATION)
        self.refused_text("delete the declaration", "the reader check could not run", stage=False)

    def test_a_declaration_that_does_not_parse_refuses(self):
        self.start()
        self.write(DECLARATION, "{not valid json\n")
        self.refused_text("break the declaration", "the reader check could not run")

    def test_archive_attributes_cannot_hide_the_verifier_from_the_check(self):
        self.start()
        self.write(NODE, 'echo "$TOKEN"\n')
        self.renew(NODE)
        self.write("richos/engine/scripts/lib/.gitattributes", "verification_inputs.py export-ignore\n")
        self.refused_text("read TOKEN, export-ignore", "omitted known key reads in scripts/reader.sh: TOKEN")

    CALL = 'echo reader v2\nbash "$ENGINE_ROOT/scripts/new-helper.sh"\n'   # the call form the verifier recognizes

    def test_the_complete_tree_reports_the_omitted_execute_edge(self):
        # review rv-20261009T072745Z-24609581-d2f5: the control. With the helper a real file, the
        # recognized call without an execute edge is refused, so the symlink cases below stand for it.
        self.start()
        self.write("richos/engine/scripts/new-helper.sh", "echo $TOKEN\n")
        self.write(NODE, self.CALL)
        self.renew(NODE)
        self.refused_text("call without an edge", "omitted known execute edges in scripts/reader.sh: scripts/new-helper.sh")

    def test_a_symlink_to_a_tracked_file_outside_the_export_refuses(self):
        # review rv-20261009T070921Z-22b69def-73c4: the link would dangle in the export and the
        # verifier would skip the call it carries.
        self.start()
        self.write("richos/tools/new-helper.sh", "echo $TOKEN\n")
        os.symlink("../../tools/new-helper.sh", self.repo / "richos/engine/scripts/new-helper.sh")
        self.write(NODE, self.CALL)
        self.renew(NODE)
        self.refused_text("link out of the export", "the reader check could not run")

    def test_a_symlink_through_a_directory_link_and_dotdot_refuses(self):
        # review rv-20261009T072745Z-24609581-d2f5: scripts/dir -> ../../mobile, then a link
        # through dir/.. passed a lexical containment check. Any tracked symlink refuses.
        self.start()
        self.write("richos/tools/new-helper.sh", "echo $TOKEN\n")
        os.symlink("../../mobile", self.repo / "richos/engine/scripts/dir")
        os.symlink("dir/../tools/new-helper.sh", self.repo / "richos/engine/scripts/new-helper.sh")
        self.write(NODE, self.CALL)
        self.renew(NODE)
        text = self.refused_text("link through a directory link", "the reader check could not run")
        self.assertIn("tracked symlink", text)

    def test_a_commit_adding_only_a_symlink_refuses(self):
        # review rv-20261009T073546Z-36c1f8d7-e14d: no declared path is touched, so the early
        # return for unmentioned paths used to pass it.
        self.start()
        self.write("richos/tools/new-helper.sh", "echo $TOKEN\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "target")
        # land that, so the branch's own change is the symlink alone (no declaration, no reader)
        self.git("checkout", "-q", "main")
        self.git("merge", "-q", "--ff-only", "feature")
        self.git("checkout", "-q", "-b", "symlink-only")
        os.symlink("../../tools/new-helper.sh", self.repo / "richos/engine/scripts/new-helper.sh")
        text = self.refused_text("only a symlink", "the reader check could not run")
        self.assertIn("tracked symlink", text)

    def test_an_inherited_external_refusal_is_not_new_because_the_export_folder_differs(self):
        self.start()
        self.mutate(lambda d: d["nodes"]["scripts/reader.sh"]["external"].append(
            {"root": "repository", "path": "richos/tools/missing.py", "sha256": "0" * 64,
             "evidence": "fixture, missing at the merge-base"}))
        self.git("add", "-A")
        self.git("commit", "--no-verify", "-q", "-m", "already broken")
        self.land_on_main()
        self.mutate(lambda d: d["nodes"]["scripts/reader.sh"].update(evidence="edited evidence only"))
        self.git("add", "-A")
        before = self.head()
        self.git("commit", "-q", "-m", "evidence only")
        self.assertNotEqual(self.head(), before)


class Parity(Fixture):
    """The digests --renew writes are the ones the selector itself checks."""

    def test_renewed_pins_satisfy_verification_inputs(self):
        self.make(node_newline="\r\n")               # a CRLF source: the selector reads it as text
        sys.path.insert(0, str(ENGINE_LIB))
        try:
            import verification_inputs as vi
        finally:
            sys.path.pop(0)
        engine = self.repo / "richos/engine"
        declaration = json.loads((self.repo / DECLARATION).read_text())
        vi.Dependencies(engine, declaration).node("scripts/reader.sh")   # raises Unsupported on a mismatch
        snapshot = vi.Snapshot(engine, "WORKTREE")
        vi.hook_reader(declaration["hook_readers"]["scripts/hook-reader.test.sh"], snapshot.read)
        self.write(EXTERNAL, "print('changed without renewal')\n")
        with self.assertRaisesRegex(vi.Unsupported, "changed external reader"):
            vi.Dependencies(engine, declaration).node("scripts/reader.sh")


if __name__ == "__main__":
    unittest.main(verbosity=2)
