#!/usr/bin/env python3
"""Synthetic desktop dispatch across real Git workspaces and the actual ECS store."""
import contextlib
import fcntl
import importlib.util
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

ENGINE=Path(__file__).resolve().parents[2]
def load():
    spec=importlib.util.spec_from_file_location("test_desktop_work",ENGINE/"mega-lander/app.py")
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

# A fictional teammate of the user's own (<app data>/team/mark.md), registered by the lease.
MARK="---\nname: mark\ndescription: Fictional fixture engineer.\nmodel: sonnet\ntools: Read, Glob, Grep, Bash, Write, Edit\n---\n\nYou are Mark, a fictional fixture engineer.\n"

def _alive(pid):
    """Is `pid` still running? A child of this test that has exited is reaped here first, so a
    zombie is never read as alive."""
    try:
        done,_status=os.waitpid(pid,os.WNOHANG)
        if done==pid: return False
    except ChildProcessError:
        pass
    try:
        os.kill(pid,0)
    except ProcessLookupError:
        return False
    return True

# The first line of each duty text (engine/mega-lander/duties/), which prepare puts at the top.
DUTY_TOP={"worker":"# Your duty in this assignment: implement","reviewer":"# Your duty in this assignment: review"}
# The heading a repository's own instruction file sits under in a brief: the words the duties use.
REPOSITORY_INSTRUCTIONS="## The repository's own instructions"
# Every instruction-file name Claude Code or Codex reads at a folder's root.
INSTRUCTION_NAMES={"CLAUDE.md","CLAUDE.local.md","AGENTS.md","AGENTS.override.md"}

class DesktopWork(unittest.TestCase):
    def test_stu_replaces_legacy_host_interruption_but_cannot_override_user_stop(self):
        identity=json.dumps(["depot","thread-a"],separators=(",", ":"))
        root=self.root/"engine-state/assignments"/hashlib.sha256(identity.encode()).hexdigest();root.mkdir(parents=True)
        saved=root/"fixture-task.json"
        record={"schema":1,"id":"fixture-task","entity_id":"depot","thread_id":"thread-a",
                "obligation_id":"fixture-task","state":"interrupted","detail":"Old host Quit inference",
                "work_session":"old-session","updated_at_ms":0}
        saved.write_text(json.dumps(record))
        result=self.call("report",{"obligation_id":"fixture-task","state":"running","detail":"Reconciled the previous attempt"})
        self.assertEqual(result["state"],"running")
        observed=json.loads(saved.read_text())
        self.assertEqual(observed["work_session"],self.session)
        observed["user_stop_requested_at_ms"]=1;saved.write_text(json.dumps(observed))
        before=saved.read_bytes()
        with self.assertRaisesRegex(ValueError,"user stopped"):
            self.call("report",{"obligation_id":"fixture-task","state":"running","detail":"Must not resume"})
        self.assertEqual(saved.read_bytes(),before)

    def test_independent_stu_cannot_mark_unresolved_execution_failed(self):
        self.call("prepare",self.args)
        with self.assertRaisesRegex(ValueError,"unresolved execution"):
            self.call("report",{"obligation_id":"fixture-task","state":"failed","detail":"Cannot finish"})
        self.assertEqual(self.call("inspect")["records"][0]["status"],"prepared")

    def test_independent_stu_closes_reviewed_work_and_an_answer_on_their_own_records(self):
        words={"fixture-task":"Create the fictional result without publication.","answer-task":"What is five plus five?"}
        identity=json.dumps(["depot","thread-a"],separators=(",", ":"))
        root=self.root/"engine-state/assignments"/hashlib.sha256(identity.encode()).hexdigest();root.mkdir(parents=True)
        rows=[]
        for obligation,text in words.items():
            ref="ledger:thread-a:"+obligation
            row={"schema":1,"id":obligation,"entity_id":"depot","thread_id":"thread-a","obligation_id":obligation,"instruction_ledger_ref":ref,"instruction_sha256":hashlib.sha256(text.encode()).hexdigest(),"state":"registered","detail":"Registered","work_session":None,"updated_at_ms":0}
            (root/(obligation+".json")).write_text(json.dumps(row))
            rows.append({"event":"PromptReceived","turn_id":obligation,"entity_id":"depot","thread_id":"thread-a","source":"text","text":text})
        (self.root/"conversation-ledger.jsonl").write_text("".join(json.dumps(row)+"\n" for row in rows))
        seat="stu-thread:thread-a";self.scope["binding"]=self.bind_seat(seat,"peer:current",audience="rich",session=self.session);self.scope["seat"]=seat
        self.scope_path.write_text(json.dumps(self.scope))
        self.app.ecs(self.scope,{"protocol":1,"command":"checkpoint","binding":self.scope["binding"],"request_id":"answer-job","checkpoint":{"statements":[{"verb":"open_loop","fields":{"id":"answer-task","title":"A question"}}]}})
        self.assertEqual(self.call("report",{"obligation_id":"fixture-task","state":"running","detail":"Working on the requested result"})["state"],"running")
        pair=self.reviewed_pair("independent-lifecycle")
        saved=root/"fixture-task.json"
        self.assertEqual(json.loads(saved.read_text())["detail"],"Working on the requested result")
        before=saved.read_bytes();self.call("inspect")
        self.assertEqual(saved.read_bytes(),before,"inspection fabricated new activity")
        with self.assertRaisesRegex(ValueError,"implementation work"):
            self.call("complete",{"obligation_id":"fixture-task","answer":"It is done"})
        self.call("integrate",pair)
        result=self.call("complete",{"obligation_id":"fixture-task","worker_ids":[pair["worker_id"]]})
        self.assertTrue(result["obligation_closed"])
        self.assertFalse(result["published"])
        answer=self.call("complete",{"obligation_id":"answer-task","answer":"Five plus five is ten."})
        self.assertTrue(answer["obligation_closed"])
        for obligation in words:
            self.assertEqual(json.loads((root/(obligation+".json")).read_text())["state"],"settled")
        self.assertFalse((self.root/"engine-state/messages").exists(),"operational completion manufactured a peer report")

    def test_selected_assignment_keeps_its_original_request_after_another_peer_input(self):
        words="Create the original result. Keep it private."
        other="A different request with different constraints."
        first_ref="ledger:thread-a:turn-a"
        identity=json.dumps(["depot","thread-a"],separators=(",", ":"))
        folder=self.root/"engine-state/assignments"/hashlib.sha256(identity.encode()).hexdigest();folder.mkdir(parents=True)
        (folder/"original.json").write_text(json.dumps({"schema":1,"id":"original","entity_id":"depot","thread_id":"thread-a","obligation_id":"fixture-task","instruction_ledger_ref":first_ref,"instruction_sha256":hashlib.sha256(words.encode()).hexdigest()}))
        ledger=self.root/"conversation-ledger.jsonl"
        ledger.write_text(json.dumps({"event":"PromptReceived","turn_id":"turn-a","entity_id":"depot","thread_id":"thread-a","source":"text","text":words})+"\n")
        seat="stu-thread:thread-a"
        self.scope["binding"]=self.bind_seat(seat,"peer:later",audience="rich",session=self.session)
        self.scope["seat"]=seat
        self.scope["user_instruction"]={"ledger_ref":"ledger:thread-a:later","sha256":hashlib.sha256(other.encode()).hexdigest(),"text":other}
        self.scope_path.write_text(json.dumps(self.scope))
        ready=self.call("prepare",self.args)
        self.assertEqual((self.app.folder(self.scope)/(ready["id"]+".words")).read_text(),words)
        payload={"session_id":self.session,"tool_use_id":"selected-original","tool_input":ready["agent_payload"]}
        authority=self.app.dispatch_intent(self.scope,payload,inspect_only=True)
        self.assertEqual(authority["instruction"]["text"],words)
        self.assertEqual(authority["authority"],first_ref)
        self.assertNotEqual(authority["instruction"]["text"],other)
        ledger.write_text(ledger.read_text().replace(words,"Tampered request"))
        with self.assertRaisesRegex(ValueError,"host attestation"):
            self.call("prepare",{**self.args,"request_id":"tampered"})

    def test_stop_blocks_prepared_dispatch_and_worker_tools_without_blocking_other_obligations(self):
        first=self.call("prepare",self.args)
        self.start_fixture_worker(first,"stopped-worker")
        second=self.call("prepare",{**self.args,"request_id":"prepared-before-stop","brief":"A different part of the same assignment"})
        identity=json.dumps(["depot","thread-a"],separators=(",", ":"))
        root=self.root/"engine-state/assignments"/hashlib.sha256(identity.encode()).hexdigest()
        root.mkdir(parents=True)
        (root/"stopped.json").write_text(json.dumps({"schema":1,"entity_id":"depot","thread_id":"thread-a","obligation_id":"fixture-task","user_stop_requested_at_ms":1}))
        with self.assertRaisesRegex(ValueError,"user stopped"):
            self.call("prepare",{**self.args,"request_id":"after-stop"})
        with self.assertRaisesRegex(ValueError,"user stopped"):
            self.app.dispatch_intent(self.scope,{"session_id":self.session,"tool_use_id":"after-stop","tool_input":second["agent_payload"]})
        with self.assertRaisesRegex(ValueError,"user stopped"):
            self.app.worker_context(self.scope,{"session_id":self.session,"agent_id":"stopped-worker","tool_name":"Bash","tool_input":{"command":"echo forbidden"}})
        self.app.ecs(self.scope,{"protocol":1,"command":"checkpoint","binding":self.scope["binding"],"request_id":"other-after-stop","checkpoint":{"statements":[{"verb":"commitment","fields":{"id":"unrelated","title":"Other work"}}]}})
        other=self.call("prepare",{**self.args,"request_id":"unrelated","obligation_id":"unrelated"})
        self.assertEqual(other["status"],"prepared")
        self.assertEqual(other["request"]["obligation_id"],"unrelated")

    def test_independent_stu_prepares_two_obligations_without_rebinding_to_a_worker(self):
        seat="stu-thread:thread-a"
        self.scope["binding"]=self.bind_seat(seat,"peer:1",audience="rich",session=self.session)
        self.scope["seat"]=seat
        self.scope["background_work_allowed"]=True
        self.scope_path.write_text(json.dumps(self.scope))
        self.assertIsNone(self.app.carried_obligation(self.scope))
        tool={t["name"]:t for t in self.app.available_tools()}
        for name in ("prepare","complete"):
            self.assertIn("independent Stu",tool[name]["description"])
            self.assertNotIn("leave obligation_id out",tool[name]["description"])
        self.app.ecs(self.scope,{"protocol":1,"command":"checkpoint","binding":self.scope["binding"],"request_id":"second-obligation","checkpoint":{"statements":[{"verb":"commitment","fields":{"id":"fixture-two","title":"A separate result"}}]}})
        first=self.call("prepare",self.args)
        second=self.call("prepare",{**self.args,"request_id":"prepare-two","obligation_id":"fixture-two","title":"A separate result"})
        self.assertNotEqual(first["id"],second["id"])
        self.start_fixture_worker(first,"independent-one")
        self.start_fixture_worker(second,"independent-two")
        rows=self.call("inspect")["records"]
        self.assertEqual({row["request"]["obligation_id"] for row in rows},{"fixture-task","fixture-two"})
        self.assertEqual({row["binding"]["audience"] for row in rows},{"rich"})
        self.assertEqual({row["binding"]["turn_id"] for row in rows},{"peer:1"})
        self.assertEqual({row["agent_id"] for row in rows},{"independent-one","independent-two"})
        with self.assertRaises(self.app.ECS.ScopeError):
            self.call("prepare",{**self.args,"request_id":"invented","obligation_id":"invented"})
        self.assertEqual(len(self.call("inspect")["records"]),2)

    def test_pierce_findings_leave_dispatch_retryable_and_allow_revised_preparation(self):
        ready = self.call("prepare", self.args)
        envelope = {"session_id": self.session, "tool_use_id": "pierce-call",
                    "tool_input": ready["agent_payload"]}
        self.app.dispatch_intent(self.scope, envelope, inspect_only=True)
        self.assertEqual(self.call("inspect", {})["records"][0]["status"], "prepared")
        self.app.dispatch_intent(self.scope, envelope, review_refused=True)
        self.assertEqual(self.call("inspect", {})["records"][0]["status"], "prepared")
        # A changed brief uses a new request. Only this provably never-dispatched
        # preparation is withdrawn, through the existing clean-workspace checks.
        replacement = self.call("prepare", {**self.args, "request_id": "revised-brief",
                                            "brief": "Implement the corrected fictional requirement."})
        self.assertEqual(replacement["status"], "prepared")
        self.app.dispatch_intent(self.scope, {"session_id": self.session, "tool_use_id": "revised-call",
                                             "tool_input": replacement["agent_payload"]})
        records = self.call("inspect", {})["records"]
        self.assertEqual(next(row for row in records if row["id"] == replacement["id"])["status"], "dispatching")

    def test_pierce_dismissal_can_retry_the_exact_prepared_payload(self):
        ready = self.call("prepare", self.args)
        envelope = {"session_id": self.session, "tool_use_id": "pierce-call",
                    "tool_input": ready["agent_payload"]}
        self.app.dispatch_intent(self.scope, envelope, review_refused=True)
        self.app.dispatch_intent(self.scope, {**envelope, "tool_use_id": "dismissed-call"})
        self.assertEqual(self.call("inspect", {})["records"][0]["status"], "dispatching")

    def setUp(self):
        self.scratch=tempfile.TemporaryDirectory(prefix="app work fixture ");self.addCleanup(self.scratch.cleanup)
        self.root=Path(self.scratch.name).resolve();self.coord=self.root/"coordination";self.repo=self.root/"target project"
        self.session="fixture-session"
        for repo in (self.coord,self.repo):
            repo.mkdir();subprocess.run(["git","init","--template=","-q","-b","main",str(repo)],check=True)
            subprocess.run(["git","-C",str(repo),"-c","core.hooksPath=/dev/null","-c","commit.gpgSign=false","-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","--allow-empty","-qm","Fixture"],check=True)
        # Every land is a merge commit (land_merge), and its committer is the one the repository's
        # own Git configuration names; nothing supplies one.
        for key,value in (("user.name","Fixture"),("user.email","fixture@example.invalid")):
            subprocess.run(["git","-C",str(self.repo),"config",key,value],check=True)
        (self.coord/".claude/agents").mkdir(parents=True)
        # The roster a lease registers (slice 1 of the proto-teammate shelf plan): the always-active
        # five as shipped, and a fictional teammate of the user's own who does the code jobs.
        for name in ("dean","clark","reed","frank","pierce"):(self.coord/f".claude/agents/{name}.md").write_text((ENGINE/f"agents/{name}.md").read_text())
        (self.coord/".claude/agents/mark.md").write_text(MARK)
        (self.coord/"orchestration.config").write_text('ALLOWED_MODELS="opus sonnet haiku"\nMODEL_TIERS="opus > sonnet > haiku"\n')
        (self.coord/".gitignore").write_text('.claude/\norchestration.config\n')
        hook=self.root/"spawn-hooks.json";hook.write_text(json.dumps({"hooks":{"PreToolUse":[{"matcher":"Agent","hooks":[{"type":"command","command":f"/bin/bash '{ENGINE}/scripts/hooks/guard-worktree-isolation.sh'"}]}]}}))
        self.registry=self.root/"entities.json";self.registry.write_text(json.dumps({"version":2,"entities":[{"id":"depot","roots":[str(self.repo)],"connected_repositories":[str(self.repo)]}]}))
        self.scope_path=self.root/"scope.json"
        env={"RICHOS_APP_STATE":str(self.root/"engine-state"),"RICHOS_APP_REGISTRY":str(self.registry),"RICHOS_APP_SCOPE":str(self.scope_path),
             "RICHOS_ENGINE_ROOT":str(ENGINE),"RICHOS_ENGINE_DIR":str(ENGINE),"RICHOS_ENTITY_ROOT":str(self.coord),"CLAUDE_PROJECT_DIR":str(self.coord),
             "RICHOS_APP_ENTITY":"depot","RICHOS_APP_THREAD":"thread-a",
             "RICHOS_WORKSPACES_DIR":str(self.root/"engine-state/workspaces"/hashlib.sha256(b'["depot","thread-a"]').hexdigest()),"RICHOS_SESSION_ID":self.session,"RICHOS_SESSION_PID":str(os.getpid()),
             "RICHOS_SPAWN_HOOK_SOURCES":f"app={hook}","GIT_CONFIG_GLOBAL":"/dev/null","GIT_CONFIG_NOSYSTEM":"1","PYTHONDONTWRITEBYTECODE":"1",
             # The land locks are machine-wide by design (~/.claude/state/land-locks).
             # A test must not touch the operator's real ones, and this is the
             # override that exists for exactly that -- it cannot reach an
             # app-launched engine, whose configure() strips every inherited
             # RICHOS_* name and re-adds only its own list.
             "RICHOS_LAND_LOCKS_DIR":str(self.root/"land locks"),
             # The override above is stripped from an app-launched engine child, which
             # then derives the machine-wide home from CLAUDE_CONFIG_DIR (else
             # ~/.claude) and would write its land locks under the operator's
             # home. CLAUDE_CONFIG_DIR is the one name the app does not strip, so the
             # suite owns the child's land-lock home by pointing it inside the fixture.
             "CLAUDE_CONFIG_DIR":str(self.root/"claude config"),
             # A land reads the user's own Git settings (users_git_settings): the fixture's user
             # is an empty home, never the person running the suite.
             "HOME":str(self.root/"user home"),"XDG_CONFIG_HOME":str(self.root/"user home"/".config")}
        (self.root/"user home").mkdir()
        self.env=patch.dict(os.environ,env);self.env.start();self.addCleanup(self.env.stop)
        self.app=load();ecs=self.root/"ecs"
        binding=self.app.ECS.execute(ecs,{"protocol":1,"command":"bind","scope":{"entity_id":"depot","thread_id":"thread-a","session_id":self.session,"turn_id":"turn-a","audience":"ceo"},"request_id":"bind-first","source_ref":"ledger:thread-a:turn-a","expected_revision":None})["binding"]
        self.scope={"version":1,"actions_allowed":True,"bridge":{"state_root":str(ecs)},"binding":binding,"user_instruction":{"ledger_ref":"ledger:thread-a:turn-a","sha256":"fixture-host-attestation"}}
        self.scope_path.write_text(json.dumps(self.scope))
        self.app.ECS.execute(ecs,{"protocol":1,"command":"checkpoint","binding":binding,"request_id":"accept-fixture","checkpoint":{"statements":[{"verb":"commitment","fields":{"id":"fixture-task","title":"Create the fictional result"}}]}})
        self.app.W.record_session_start(self.session,str(self.coord))
        self.args={"request_id":"prepare-one","obligation_id":"fixture-task","repo":str(self.repo),"title":"Create fictional result","brief":"Create result.txt with FICTIONAL in the assigned target worktree. Commit only that file. Do not publish.","role":"worker","teammate":"mark","integration":"main"}

    def call(self,name,args=None):return self.app.call(self.scope_path,name,args or {})
    def test_rich_reads_the_same_work_evidence_without_changing_stus_records(self):
        tools = {tool["name"]: tool for tool in self.app.available_tools(read_only=True)}
        self.assertEqual(set(tools), {"inspect", "repositories", "team"})
        self.assertEqual(tools["team"]["inputSchema"]["properties"]["action"]["enum"], ["list", "show"])
        ready = self.call("prepare", self.args)
        root = self.app.folder(self.scope)
        expected = self.call("inspect", {})["records"]
        before = {p.name: p.read_bytes() for p in root.glob("*.json")}
        actual = self.app.call_read_only(self.scope_path, "inspect", {})["records"]
        self.assertEqual(actual, expected)
        self.assertEqual(actual[0]["id"], ready["id"])
        self.assertEqual(actual[0]["request"]["brief"], self.args["brief"])
        self.assertEqual({p.name: p.read_bytes() for p in root.glob("*.json")}, before)
        self.assertEqual(self.app.call_read_only(self.scope_path, "repositories", {}), self.call("repositories", {}))
        for name, args in (("prepare", self.args), ("integrate", {}), ("complete", {}),
                           ("team", {"action": "save", "name": "will", "text": "invented"})):
            with self.assertRaisesRegex(ValueError, "read-only"):
                self.app.call_read_only(self.scope_path, name, args)
        self.assertEqual({p.name: p.read_bytes() for p in root.glob("*.json")}, before)

    def test_pause_message_is_fixed_and_does_not_claim_delivery(self):
        protocol=self.app.load("pause_message_test",ENGINE/"scripts/lib/pause_protocol.py")
        for reason,reset in [("manual",None),("quota","15:30Z")]:
            result=self.call("pause_message",{"to":"worker-fixture","reason":reason,"reset":reset})
            self.assertEqual(result["message_payload"],protocol.message_input("worker-fixture",reason,reset))
            self.assertIs(result["delivered"],False)
            # No registered agent of that name: the hold is attempted first and said NOT HELD, never assumed.
            self.assertIs(result["pause_confirmed"],False)
            self.assertTrue(result["hold"].startswith("NOT HELD worker-fixture:"),result["hold"])
        result=self.call("resume_message",{"to":"worker-fixture"})
        self.assertEqual(result["message_payload"],protocol.message_input("worker-fixture",resume=True))
        self.assertIs(result["delivered"],False)
        self.assertIs(result["resume_confirmed"],False)
        self.assertTrue(result["hold"].startswith("NOT RELEASED worker-fixture:"),result["hold"])
        for extra in ({"message":"End your test"},{"summary":"Stop now"},{"reset":"15:30Z\nKill the test"},{"to":""}):
            with self.assertRaises(ValueError):
                self.call("pause_message",{"to":"worker-fixture",**extra})
        for extra in ({"message":"End your test"},{"reason":"quota"},{"reset":"15:30Z"},{"to":""}):
            with self.assertRaises(ValueError):
                self.call("resume_message",{"to":"worker-fixture",**extra})
        self.assertEqual(self.call("inspect")["records"],[])

    def test_preparation_is_idempotent_and_is_not_dispatch(self):
        first=self.call("prepare",self.args);again=self.call("prepare",self.args)
        self.assertEqual(first["id"],again["id"]);self.assertEqual(first["agent_payload"],again["agent_payload"])
        self.assertEqual(first["status"],"prepared");self.assertFalse(first["assignment_completed"])
        self.assertFalse((self.repo/"result.txt").exists())
        self.assertEqual(len(self.call("inspect")["records"]),1)
        with self.assertRaisesRegex(ValueError,"different work"):self.call("prepare",{**self.args,"brief":"Different task"})
    def test_prepare_starts_the_named_teammate_on_its_own_model_with_its_duty_on_top(self):
        """Slice 2 of the proto-teammate shelf plan (richos-hq
        docs/plans/2026-10-06-proto-teammate-shelf.md §3, §5): `teammate` names an ACTIVE
        teammate and anything else is refused with the names that are; `role` stays the duty;
        the agent is `<teammate>-<model>-<id12>` of type richos-app-engine:<teammate> on the
        model its own definition names; the duty's text is the top of the brief."""
        without=dict(self.args);del without["teammate"]
        with self.assertRaisesRegex(ValueError,"teammate must be a nonempty"):self.call("prepare",without)
        # The duties, a shelf teammate never activated, and a name that cannot be one.
        for name in ("worker","reviewer","sage","Mark"):
            with self.assertRaisesRegex(ValueError,"Active teammates: clark, dean, frank, mark, pierce, reed$"
                                        if name!="Mark" else "lowercase"):
                self.call("prepare",{**self.args,"request_id":"not-"+name,"teammate":name})
        (self.coord/".claude/agents/nora.md").write_text("---\nname: nora\n---\nNo model line.\n")
        with self.assertRaisesRegex(ValueError,"nora's definition names no model"):
            self.call("prepare",{**self.args,"request_id":"no-model","teammate":"nora"})
        (self.coord/".claude/agents/olga.md").write_text("---\nname: olga-dev\nmodel: sonnet\n---\n")
        with self.assertRaisesRegex(ValueError,"names itself 'olga-dev'"):
            self.call("prepare",{**self.args,"request_id":"wrong-name","teammate":"olga"})
        self.assertEqual(self.call("inspect")["records"],[],"a refused teammate left a receipt")

        ready=self.call("prepare",{**self.args,"teammate":"frank"})
        self.assertRegex(ready["name"],r"^frank-opus-[0-9a-f]{12}$")
        payload=ready["agent_payload"]
        self.assertNotIn("name",payload)
        self.assertNotIn("isolation",payload)
        self.assertEqual((payload["subagent_type"],payload["model"]),("richos-app-engine:frank","opus"))
        self.assertEqual((ready["request"]["role"],ready["request"]["teammate"]),("worker","frank"))
        duty=(ENGINE/"mega-lander/duties/worker.md").read_text().strip()
        self.assertTrue(duty.startswith(DUTY_TOP["worker"]))
        workspace_line,rest=payload["prompt"].split("\n\n",1)
        self.assertTrue(workspace_line.startswith("cross-repo-worktree: "))
        self.assertTrue(rest.startswith(duty+"\n\n"+self.args["brief"]),rest[:300])
        self.assertNotIn(DUTY_TOP["reviewer"],payload["prompt"])
        # The same request for another teammate is different work.
        with self.assertRaisesRegex(ValueError,"different work"):self.call("prepare",{**self.args,"teammate":"mark"})

    def test_prepare_runs_one_job_on_a_named_model_within_the_users_ceiling(self):
        """Build plan item 7 (richos-hq docs/plans/2026-10-21-everything-for-every-user.md): an
        optional `model` runs this one job on another model than the teammate's default. It must
        be one the user's ALLOWED_MODELS lists; the user's MODEL_CEILING is applied by
        guard-model-ceiling.sh at the spawn, as for every other dispatch; a move below the
        definition's default states its reason, as the isolation guard asks of any spawn."""
        (self.root/"spawn-hooks.json").write_text(json.dumps({"hooks":{"PreToolUse":[{"matcher":"Agent","hooks":[
            {"type":"command","command":f"/bin/bash '{ENGINE}/scripts/hooks/{name}'"} for name in ("guard-worktree-isolation.sh","guard-model-ceiling.sh")]}]}}))
        (self.coord/"orchestration.config").write_text('ALLOWED_MODELS="fable opus sonnet haiku"\n'
                                                       'MODEL_TIERS="fable > opus > sonnet > haiku"\nMODEL_CEILING="opus"\n')
        with self.assertRaisesRegex(ValueError,"model must be one of fable, opus, sonnet, haiku"):
            self.call("prepare",{**self.args,"request_id":"unknown-model","model":"gpt"})
        self.assertEqual(self.call("inspect")["records"],[],"a model nobody allows left a receipt")
        # Above the default and within the ceiling: Mark (sonnet) on Opus for this one job.
        ready=self.call("prepare",{**self.args,"request_id":"stronger","model":"opus"})
        self.assertEqual(ready["status"],"prepared")
        self.assertRegex(ready["name"],r"^mark-opus-[0-9a-f]{12}$")
        self.assertEqual(ready["agent_payload"]["model"],"opus")
        self.assertEqual(ready["request"]["model"],"opus")
        # The same request on the teammate's own model is different work.
        with self.assertRaisesRegex(ValueError,"different work"):
            self.call("prepare",{**self.args,"request_id":"stronger"})
        # Above the user's ceiling: refused at the spawn, nothing created.
        with self.assertRaisesRegex(Exception,"more expensive model"):
            self.call("prepare",{**self.args,"request_id":"above","model":"fable","title":"Above the ceiling"})
        above=next(r for r in self.call("inspect")["records"] if r["request"].get("model")=="fable")
        self.assertNotIn("agent_payload",above)
        self.assertIn("more expensive model",above["problem"])
        # Below the definition's default: with its reason stated, it starts.
        cheaper=self.call("prepare",{**self.args,"request_id":"cheaper","model":"haiku","title":"Cheaper",
            "brief":"model-downgrade-ack: a one-line rename that needs no reasoning at all\n"+self.args["brief"]})
        self.assertRegex(cheaper["name"],r"^mark-haiku-[0-9a-f]{12}$")
        self.assertEqual(cheaper["agent_payload"]["model"],"haiku")
        # A user whose allowed list and ceiling sit below a teammate's own default can still run
        # that teammate on a model they allow (mid-job review rv-20261021T164124Z-0fe64c41-a94a,
        # finding 1): only the model the job runs on is checked, never the default it replaces.
        (self.coord/"orchestration.config").write_text('ALLOWED_MODELS="fable sonnet haiku"\n'
                                                       'MODEL_TIERS="fable > opus > sonnet > haiku"\nMODEL_CEILING="sonnet"\n')
        frank={**self.args,"teammate":"frank","obligation_id":"fixture-task"}
        on_sonnet=self.call("prepare",{**frank,"request_id":"frank-sonnet","model":"sonnet","title":"Frank on Sonnet",
            "brief":"model-downgrade-ack: the user's own ceiling is sonnet, below this teammate's default\n"+self.args["brief"]})
        self.assertEqual(on_sonnet["status"],"prepared")
        self.assertRegex(on_sonnet["name"],r"^frank-sonnet-[0-9a-f]{12}$")
        self.assertEqual(on_sonnet["agent_payload"]["model"],"sonnet")
        with self.assertRaisesRegex(Exception,"more expensive model"):
            self.call("prepare",{**frank,"request_id":"frank-fable","model":"fable","title":"Frank on Fable"})
        # With no model named, the teammate's own default still has to be one the user allows.
        with self.assertRaisesRegex(ValueError,"frank's definition names no model this app can start"):
            self.call("prepare",{**frank,"request_id":"frank-default","title":"Frank on his own model"})
        # The field is in the tool's schema, so the back end can pass it.
        prepare_tool=next(t for t in self.app.available_tools() if t["name"]=="prepare")
        self.assertIn("model",prepare_tool["inputSchema"]["properties"])

    def test_a_consult_may_make_the_media_skills_working_folder_and_no_other_file(self):
        """Every teammate's standing instruction sends a recording to `watch-video` or
        `audio-transcription` (CEO 2026-10-10), and both make a working folder for frames,
        converted audio and a transcript (`mktemp -d "$PWD/video-XXXXXX"`, `audio-XXXXXX`) and
        delete it. A consult duty that forbade creating any file left a consulted teammate
        unable to follow both (second review of 79ba6eb7e, finding 2). The duty allows that
        folder, names it by those two skills, and still forbids every other file."""
        duty=" ".join((ENGINE/"mega-lander/duties/consult.md").read_text().split())
        self.assertIn("Do not write, edit or create files",duty)
        exception=re.search(r"The one exception is ([^:]*):",duty)
        self.assertIsNotNone(exception,"the consult duty names no exception for the media skills' working folder")
        # Everything the skills put in that folder: a linked video is downloaded into it
        # (watch-video step 3) and its uploaded captions written there (step 4a) before any
        # frame or transcript exists (mid-job review of 7f6ea1dd8).
        for name in ("`watch-video`","`audio-transcription`","working folder","copy of a linked video",
                     "captions","frames","converted audio","transcript"):
            self.assertIn(name,exception.group(1))
        self.assertIn("No other file",duty[exception.end():])

    def test_a_hiring_brief_for_dean_goes_through_the_first_time(self):
        """hiresnag55 (final walk of 0a2ae649f, run 3): after his go, Rich told him the step that
        writes up Will and Beth "hit a technical snag" and was sent again "in a shorter form".
        Dean's brief carries the shelf definition he fits, Clark's research and the user's facts
        and rules; Clark's research alone came back at 27,099 characters in a hire walk, and with
        will.md (6,750) that is over the old 32,000 before a single fact is added."""
        shelf=(ENGINE/"team/shelf/will.md").read_text()
        research="The fictional bookkeeper keeps the books for a fictional coaching company. "*362
        self.assertGreaterEqual(len(research),27099)
        rules=("Never share a client's coaching notes with anyone outside the company.\n"
               "Pricing and any spend over $1,000 is my call. Scheduling and drafts you can decide.\n")
        brief=("Write two teammate definitions. Fit Will from his shelf text:\n\n"+shelf
               +"\n\nWrite Beth from Clark's research:\n\n"+research+"\n\nBoth carry these rules word for word:\n"+rules)
        self.assertGreater(len(brief),32000)
        ask={"request_id":"hire-dean","obligation_id":"fixture-task","role":"consult","teammate":"dean",
             "title":"Write teammate definitions for Will and Beth","brief":brief}
        ready=self.call("prepare",ask)
        self.assertEqual(ready["status"],"prepared")
        self.assertIn(brief,ready["agent_payload"]["prompt"])
        self.app.dispatch_intent(self.scope,{"session_id":self.session,"tool_use_id":"hire-dean-call",
                                             "tool_input":ready["agent_payload"]})
        # Over the bound, the refusal says by how much, so one shorter send is enough.
        with self.assertRaisesRegex(ValueError,"brief must be a nonempty bounded string: it is 65,537 bytes as sent "
                                    r"\(65,537 characters\) and the most is 65,536 bytes"):
            self.call("prepare",{**ask,"request_id":"hire-dean-over","brief":"x"*(self.app.BRIEF_LIMIT+1)})

    def test_any_brief_the_limit_accepts_gets_through_the_transport(self):
        """Review rv-20261014T071445Z-2a39eaa8-6d15: the brief limit counted characters, but the work
        tools' transport refuses a request line over 128 KiB in bytes (ecs/adapters/mcp.py serve), so
        47,700 characters of Chinese research fitted the limit and came back "Message too large"
        before the hiring step ran. The limit now counts the brief's bytes as Claude Code sends them,
        so what it accepts reaches prepare, and what it refuses is refused there, with its size."""
        transport=self.app.load("work_transport_test",ENGINE/"ecs/adapters/mcp.py")
        def send(brief,request_id,obligation="fixture-task"):
            ask={"request_id":request_id,"obligation_id":obligation,"role":"consult","teammate":"dean",
                 "title":"Write teammate definitions","brief":brief}
            # The lines as Claude Code writes them (Node's JSON.stringify): UTF-8, and nothing escaped
            # but quotes, backslashes and control characters.
            lines=b"".join(json.dumps(message,ensure_ascii=False,separators=(",",":")).encode("utf-8","surrogatepass")+b"\n"
                           for message in ({"jsonrpc":"2.0","id":1,"method":"initialize"},
                                           {"jsonrpc":"2.0","id":2,"method":"tools/call",
                                            "params":{"name":"prepare","arguments":ask}}))
            out=io.StringIO()
            transport.serve(str(self.scope_path),io.BytesIO(lines),out,tools=self.app.TOOLS,handler=self.app.call)
            return json.loads(out.getvalue().splitlines()[-1])
        # The review's fixture: 47,700 characters, 142,420 bytes on the line, over the transport's
        # 131,072. The limit accepted it, so it met "Message too large"; the limit now refuses it.
        paragraph="公司的记账员负责核对每月发票和银行流水，整理客户收款记录，编制费用报表，并将需要批准的支出提交给负责人。\n"
        self.assertEqual(send(paragraph*900,"hire-chinese")["error"]["message"],"Message too large")
        with self.assertRaisesRegex(ValueError,r"brief must be a nonempty bounded string: it is 142,\d{3} bytes as sent "
                                    r"\(47,700 characters\) and the most is 65,536 bytes"):
            self.call("prepare",{"request_id":"hire-chinese","obligation_id":"fixture-task","role":"consult",
                                 "teammate":"dean","title":"Write teammate definitions","brief":paragraph*900})
        # The most the limit accepts, in each kind of character at its costliest as sent (a control
        # character is six bytes, \u0001), goes through and is prepared.
        for index,unit in enumerate(("公","😀","\x01","\n",'"',"x")):
            head="Hire Beth. "
            body=unit*((self.app.BRIEF_LIMIT-len(head))//len(json.dumps(unit,ensure_ascii=False).encode()[1:-1]))
            brief=head+body
            brief+="x"*(self.app.BRIEF_LIMIT-len(json.dumps(brief,ensure_ascii=False).encode())+2)
            self.assertEqual(len(json.dumps(brief,ensure_ascii=False).encode())-2,self.app.BRIEF_LIMIT)
            # One hire each: an obligation holds one unresolved preparation.
            self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"checkpoint","binding":self.scope["binding"],
                "request_id":f"accept-hire-{index}","checkpoint":{"statements":[{"verb":"commitment",
                "fields":{"id":f"hire-{index}","title":"Hire a fictional teammate"}}]}})
            reply=send(brief,f"hire-at-limit-{index}",f"hire-{index}")
            self.assertNotIn("error",reply,repr(unit))
            # The END of the error, where its reason is: a failed command's text starts with the
            # command line, which alone filled the first 500 characters this used to keep.
            self.assertFalse(reply["result"]["isError"],(repr(unit),reply["result"]["content"][0]["text"][-4000:]))
            self.assertEqual(json.loads(reply["result"]["content"][0]["text"])["status"],"prepared",repr(unit))
            # One byte more is the limit's refusal, never the transport's.
            reply=send(brief+"x",f"hire-over-limit-{index}",f"hire-{index}")
            self.assertNotIn("error",reply,repr(unit))
            self.assertIn("65,537 bytes as sent",reply["result"]["content"][0]["text"],repr(unit))

    def test_inspection_reports_committed_work_separately_from_uncommitted_files_in_each_repository(self):
        second = self.second_repository()
        ready = self.call("prepare", {**self.args, "request_id": "commit-facts", "repos": [str(second)]})
        paths = self.workspaces_of(ready)
        commits = self.commit_in_each(paths, "finished.txt", "Implemented fixture")
        row = self.app.call_read_only(self.scope_path, "inspect", {})["records"][0]
        self.assertEqual(row["uncommitted_files"], "")
        self.assertEqual(row["head_commit"]["sha"], commits[str(self.repo)])
        self.assertFalse(row["assignment_completed"])
        for repo, commit in commits.items():
            observation = row["retained_targets"][repo]
            self.assertEqual(observation["uncommitted_files"], "")
            self.assertEqual(observation["head_commit"]["sha"], commit)
            self.assertEqual(observation["head_commit"]["subject"], "Fictional finished.txt")
            self.assertIn("finished.txt", observation["head_commit"]["changed_files"])
            self.assertRegex(observation["head_commit"]["committed_at"], r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
        (Path(paths[str(self.repo)]) / "finished.txt").write_text("Another edit, not committed")
        changed = self.app.call_read_only(self.scope_path, "inspect", {})["records"][0]
        self.assertIn("finished.txt", changed["uncommitted_files"])
        self.assertEqual(changed["head_commit"]["sha"], commits[str(self.repo)])
        for path in (self.root / "missing", self.root):
            unknown = self.app.workspace_observation(path)
            self.assertIsNone(unknown["uncommitted_files"])
            self.assertIsNone(unknown["head_commit"])
            self.assertTrue(unknown["workspace_problem"])

    def test_read_only_inspection_does_not_wait_for_the_receipt_writer(self):
        ready = self.call("prepare", {"request_id": "read-lock", "obligation_id": "fixture-task",
                                      "role": "consult", "teammate": "clark",
                                      "title": "Inspect a fictional task", "brief": "Answer the fictional question."})
        real_flock = fcntl.flock
        def refuse_a_contended_lock(fd, operation):
            # The kernel's EWOULDBLOCK is the verdict, with no elapsed-time threshold.
            return real_flock(fd, operation | fcntl.LOCK_NB)
        with self.app.locked(self.scope) as root:
            before = {path: path.read_bytes() for path in root.glob("*.json")}
            with patch.object(self.app.fcntl, "flock", side_effect=refuse_a_contended_lock):
                observed = self.app.call_read_only(self.scope_path, "inspect", {})
                self.assertEqual([row["id"] for row in observed["records"]], [ready["id"]])
                # Stu's mutating inspection must still acquire the writer lock.
                with self.assertRaises(BlockingIOError):
                    self.app.inspect_receipts(self.scope, {})
            self.assertEqual({path: path.read_bytes() for path in root.glob("*.json")}, before)

    def test_a_consult_needs_no_repository_is_read_only_and_hands_back_its_final_message(self):
        """Slice 3 of the proto-teammate shelf plan (richos-hq
        docs/plans/2026-10-06-proto-teammate-shelf.md §3): a third duty, `consult`, with no
        repository and no workspace; writes are refused as for a reviewer; the teammate's final
        message is the deliverable, handed back to the back end on its receipt."""
        ask={"request_id":"ask-clark","obligation_id":"fixture-task","role":"consult","teammate":"clark",
             "title":"Research a fictional question","brief":"Which fictional fixture format is older? Answer in one line."}
        for key,value in (("repo",str(self.repo)),("repos",[str(self.repo)]),("integration","main"),("review_of","0"*64)):
            with self.assertRaisesRegex(ValueError,"a consult changes no repository, so it takes no "+key):
                self.call("prepare",{**ask,"request_id":"with-"+key,key:value})
        ready=self.call("prepare",ask)
        self.assertEqual(ready["status"],"prepared")
        self.assertRegex(ready["name"],r"^clark-sonnet-[0-9a-f]{12}$")
        self.assertEqual((ready["request"]["repo"],ready["request"]["repos"],ready["request"]["role"]),(None,[],"consult"))
        payload=ready["agent_payload"]
        self.assertEqual((payload["subagent_type"],payload["model"]),("richos-app-engine:clark","sonnet"))
        duty=(ENGINE/"mega-lander/duties/consult.md").read_text().strip()
        self.assertTrue(duty.startswith("# Your duty in this assignment: consult"))
        self.assertNotIn("cross-repo-worktree:",payload["prompt"])
        self.assertIn(duty+"\n\n"+ask["brief"],payload["prompt"])
        # No workspace: nothing under the target worktrees, nothing in the canonical registry.
        self.assertFalse((self.root/"engine-state/target-worktrees").exists() and any((self.root/"engine-state/target-worktrees").rglob("*")))
        self.assertNotIn("retained_target",ready)
        self.start_fixture_worker(ready,"consult-clark")
        canonical=self.app.W.load_agent(self.app.W.named_key(self.session,ready["name"]))
        self.assertEqual([w["kind"] for w in self.app.W.live_workspaces(canonical)],["native"])
        # Read-only: every file edit is refused, a read is told it is a consult.
        for tool in ("Write","Edit","MultiEdit","NotebookEdit"):
            with self.assertRaisesRegex(ValueError,"a consult changes no files"):
                self.app.worker_context(self.scope,{"session_id":self.session,"agent_id":"consult-clark","cwd":str(self.coord),
                                                    "tool_name":tool,"tool_input":{"file_path":str(self.repo/"x.txt")}})
        context=self.app.worker_context(self.scope,{"session_id":self.session,"agent_id":"consult-clark","cwd":str(self.coord),
                                                     "tool_name":"Read","tool_input":{"file_path":str(self.repo/"x.txt")}})
        self.assertIn("you are consulted and change no repository",context["hookSpecificOutput"]["additionalContext"])
        # The final message is the deliverable, handed back on the receipt.
        answer="The fictional CSV fixture is older than the fictional JSON one, by the dates in their headers."
        self.finish_fixture_worker("consult-clark",answer)
        record=self.call("inspect")["records"][0]
        self.assertEqual(record["status"],"run-ended")
        self.assertEqual((record["consult_answer"]["message"],record["consult_answer"]["answered"],record["consult_answer"]["source"]),
                         (answer,True,"SubagentStop"))
        # A delivered handback is the report when the stop says only that it was delivered.
        again=self.call("prepare",{**ask,"request_id":"ask-clark-again"})
        self.start_fixture_worker(again,"consult-clark-2")
        self.app.observe(self.scope,{"hook_event_name":"PostToolUse","session_id":self.session,"agent_id":"consult-clark-2",
                                     "tool_name":"SubagentHandback","tool_use_id":"handback-2","tool_input":{"message":answer+" Checked twice."},
                                     "tool_response":{"success":True}})
        self.finish_fixture_worker("consult-clark-2","Report delivered to caller.")
        with self.app.locked(self.scope) as root: second=self.app.read_record(root,again["id"])
        self.assertEqual(second["consult_answer"]["message"],answer+" Checked twice.")
        self.assertNotIn("consult_handback",second)
        # Nothing to land, and no work unit that would stop the assignment closing on the answer.
        with self.assertRaisesRegex(ValueError,"a consult has nothing to land"):
            self.call("integrate",{"worker_id":ready["id"],"reviewer_id":again["id"]})
        work=self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"inspect","binding":self.scope["binding"],
                                                   "query":{"section":"work","offset":0,"limit":100,"include_closed":True}})
        self.assertEqual(work["records"],[],"a consult was projected as a work unit")

    def test_the_team_tool_lists_and_shows_the_shelf_and_saves_the_users_own_copy(self):
        """Slice 4 of the proto-teammate shelf plan (richos-hq
        docs/plans/2026-10-06-proto-teammate-shelf.md §2, §4, §10): the back end lists the
        shelf, shows one shelf teammate's text for Dean's consult, and saves the definition he
        fits as <app data>/team/<name>.md. The saved copy is the user's: saving over it is
        refused unless the user asked for a change. Saving registers nothing; the lease does."""
        self.assertIn("team",[tool["name"] for tool in self.app.TOOLS])
        shelf=sorted(p.stem for p in (ENGINE/"team/shelf").glob("*.md"))
        self.assertGreaterEqual(len(shelf),20)
        listed=self.call("team",{"action":"list"})["shelf"]
        # Every shelf teammate, by its own name, and none of them activated yet.
        self.assertEqual([row["name"] for row in listed],shelf)
        for row in listed:
            self.assertTrue(row["description"] and row["model"],row)
            self.assertIs(row["activated"],False)
        stock=(ENGINE/"team/shelf/sage.md").read_text()
        self.assertEqual(self.call("team",{"action":"show","name":"sage"}),{"name":"sage","text":stock})
        for name in ("dean","../agents/dean","worker"):
            with self.assertRaisesRegex(ValueError,"is not on the shelf"):self.call("team",{"action":"show","name":name})

        # Dean's fitted definition, saved under the shelf name into the user's own folder.
        fitted=stock.rstrip("\n")+"\n\n## This project\n\nThe fictional depot service, in its connected repository.\n"
        mine=self.root/"team/sage.md"
        self.assertFalse(mine.exists())
        saved=self.call("team",{"action":"save","name":"sage","text":fitted})
        self.assertEqual((saved["saved"],saved["replaced"],saved["registered"]),(str(mine),False,False))
        # Saving is durable; registration happens on the next provider connection.
        self.assertEqual(saved["registration_boundary"], "next_provider_connection")
        self.assertNotIn("note", saved)
        self.assertEqual(mine.read_text(),fitted)
        self.assertEqual(mine.stat().st_mode&0o777,0o600)
        self.assertEqual({row["name"]:row["activated"] for row in self.call("team",{"action":"list"})["shelf"]}["sage"],True)
        # The shelf is never written, and nothing is registered until the next lease.
        self.assertEqual((ENGINE/"team/shelf/sage.md").read_text(),stock)
        self.assertFalse((self.coord/".claude/agents/sage.md").exists())
        with self.assertRaisesRegex(ValueError,"sage is not an active teammate"):
            self.call("prepare",{**self.args,"request_id":"too-soon","teammate":"sage"})

        # The user's copy is kept: the shelf text again is refused, and the fitting survives.
        with self.assertRaisesRegex(ValueError,"that copy is the user's: it is kept"):
            self.call("team",{"action":"save","name":"sage","text":stock})
        self.assertEqual(mine.read_text(),fitted)
        # A change the user asked for (here: a move to Sonnet) replaces it.
        moved=fitted.replace("model: opus","model: sonnet",1)
        self.assertNotEqual(moved,fitted)
        self.assertIs(self.call("team",{"action":"save","name":"sage","text":moved,"replace":True})["replaced"],True)
        self.assertEqual(mine.read_text(),moved)

        # Only a definition that could be started by that name is saved.
        nora="---\nname: nora\ndescription: Fictional.\nmodel: sonnet\n---\nA fictional teammate.\n"
        refusals=[({"name":"sage2","text":stock},"names itself 'sage'"),
                  ({"name":"nora","text":"---\nname: nora\ndescription: Fictional.\n---\nNo model.\n"},"names no model"),
                  ({"name":"nora","text":"---\nname: nora\nmodel: sonnet\n---\nNo description.\n"},"has no description"),
                  ({"name":"nora","text":"```markdown\n---\nname: nora\n```\n"},"without a code fence"),
                  ({"name":"worker","text":stock},"never a duty"),
                  ({"name":"Nora","text":stock},"lowercase"),
                  ({"name":"nora","text":"---\n"+"x"*70000},"at most"),
                  ({"name":"nora","text":nora,"replace":"yes"},"replace must be true or false"),
                  ({"name":"nora","text":stock,"repo":str(self.repo)},"only name, text and replace")]
        for args,reason in refusals:
            with self.assertRaisesRegex(ValueError,reason):self.call("team",{"action":"save",**args})
        self.assertEqual(sorted(p.name for p in (self.root/"team").iterdir()),["sage.md"])
        for args in ({},{"action":"delete","name":"sage"},{"action":"list","name":"sage"}):
            with self.assertRaises(ValueError):self.call("team",args)

    def test_deans_name_rule_is_the_rule_the_team_tool_saves(self):
        """Nightly 57's follow-up: Dean's template said `name` is a "lowercase kebab slug", so
        Anne-Marie came back as `anne-marie`, and the team tool refuses a hyphen (TEAMMATE_NAME:
        2 to 16 lowercase letters and digits). Dean's rule now shows how a name is written, and
        every example it gives is a name the tool saves; the tool's own description says the same."""
        dean=(ENGINE/"agents/dean.md").read_text(encoding="utf-8")
        rule=next(line for line in dean.splitlines() if line.startswith("- **`name`:**"))
        examples=dict(re.findall(r"([A-Z][\w'-]*) is `([^`]+)`",rule))
        self.assertIn("Anne-Marie",examples,"Dean's name rule shows no hyphenated name: "+rule)
        for name,saved_as in examples.items():
            text=f"---\nname: {saved_as}\ndescription: Fictional {name}.\nmodel: sonnet\n---\n\n# {name}\n"
            self.assertEqual(self.call("team",{"action":"save","name":saved_as,"text":text})["name"],saved_as)
        team=next(tool for tool in self.app.TOOLS if tool["name"]=="team")
        for name,saved_as in examples.items():
            self.assertIn(f"{name} is {saved_as}",team["description"])

    def test_preflight_refusal_allows_a_corrected_request_without_claiming_a_start(self):
        subprocess.run(["git", "-C", str(self.repo), "branch", "-m", "integration"], check=True)
        missing = {k:v for k,v in self.args.items() if k != "integration"}
        with self.assertRaisesRegex(ValueError, "will not guess the branch"):
            self.call("prepare", missing)
        refused = self.call("inspect")["records"][0]
        self.assertEqual(refused["status"], "blocked")
        self.assertTrue(refused["preparation_refused"])
        self.assertNotIn("agent_payload", refused)
        self.assertIsNone(self.app.W.load_agent(self.app.W.named_key(self.session, refused["name"])))
        ready = self.call("prepare", {**self.args, "request_id":"corrected", "integration":"integration"})
        self.assertEqual(ready["status"], "prepared")
        self.assertIn("agent_payload", ready)

    def test_unstructured_spawn_failure_is_retained_then_settled_for_a_replacement(self):
        """Part 4, finding 1: this test used to END at the refusal below, which
        pinned the stuck state. The receipt is still retained and never
        replayed, but a replacement settles it once nothing ran."""
        command = [sys.executable, "-c", "import sys; print('spawn: refused - NOTHING WAS CREATED.'); sys.exit(1)"]
        with patch.object(self.app, "build_spawn_command", return_value=command):
            with self.assertRaises(ValueError): self.call("prepare", self.args)
        record = self.call("inspect")["records"][0]
        self.assertEqual(record["status"], "unknown")
        # The same request is still only its receipt: nothing is replayed.
        self.assertNotIn("agent_payload", self.call("prepare", self.args))
        replacement = self.call("prepare", {**self.args, "request_id":"replacement"})
        self.assertEqual(replacement["status"], "prepared")
        self.assertIn("agent_payload", replacement)
        settled = [r for r in self.call("inspect")["records"] if r["id"] == record["id"]][0]
        self.assertEqual(settled["status"], "blocked")
        self.assertEqual(settled["settled"]["withdrawn"], [])
        self.assertIn("never dispatched", settled["settled"]["reason"])

    def test_a_created_but_never_dispatched_preparation_is_withdrawn_and_a_registered_spawn_is_not(self):
        """Part 4, finding 1, the harder half: the failed preparation DID create
        its workspace. Its registration shows no spawn, so the canonical
        withdrawal removes it and a replacement proceeds. The control is the
        reason the receipt is kept at all: once the canonical record shows a
        registered spawn, nothing is withdrawn and the refusal stands."""
        real = self.app.run
        def created_then_uncertain(command, **kw):
            real(command, **kw)
            raise TimeoutError("synthetic uncertainty after the workspace was created")
        with patch.object(self.app, "run", side_effect=created_then_uncertain):
            with self.assertRaises(TimeoutError): self.call("prepare", self.args)
        stuck = self.call("inspect")["records"][0]
        self.assertEqual(stuck["status"], "unknown")
        key = self.app.W.named_key(self.session, stuck["name"])
        workspace = [w for w in self.app.W.load_agent(key)["workspaces"] if w.get("kind") == "cc"][0]
        self.assertTrue(Path(workspace["path"]).is_dir())
        # CONTROL: a registered spawn may have run, so it is never withdrawn here.
        with self.app.W.Lock():
            canonical = self.app.W.load_agent(key); canonical["tool_use_id"] = "observed-spawn"
            self.app.W.save_agent(canonical)
        with self.assertRaisesRegex(ValueError, "unresolved"):
            self.call("prepare", {**self.args, "request_id":"refused-replacement"})
        self.assertTrue(Path(workspace["path"]).is_dir())
        # CONTROL: dirty files are refused, never discarded.
        with self.app.W.Lock():
            canonical = self.app.W.load_agent(key); canonical.pop("tool_use_id")
            self.app.W.save_agent(canonical)
        (Path(workspace["path"]) / "keep.txt").write_text("KEEP")
        with self.assertRaisesRegex(ValueError, "could not be withdrawn"):
            self.call("prepare", {**self.args, "request_id":"dirty-replacement"})
        self.assertEqual((Path(workspace["path"]) / "keep.txt").read_text(), "KEEP")
        (Path(workspace["path"]) / "keep.txt").unlink()
        # Nothing ran and nothing is dirty: withdrawn canonically, and replaced.
        replacement = self.call("prepare", {**self.args, "request_id":"replacement"})
        self.assertEqual(replacement["status"], "prepared")
        self.assertFalse(Path(workspace["path"]).exists())
        self.assertEqual(self.app.git(self.repo, "branch", "--list", workspace["branch"]), "")
        self.assertIsNone(self.app.W.load_agent(key))
        settled = [r for r in self.call("inspect")["records"] if r["id"] == stuck["id"]][0]
        self.assertEqual(settled["status"], "blocked")
        self.assertEqual(settled["settled"]["withdrawn"], [workspace["path"]])

    def test_a_settled_preparation_does_not_block_the_assignment_from_completing(self):
        """Part 4, finding 1, one step later: a settled receipt did no work, so
        completion must not demand integration evidence for it."""
        command = [sys.executable, "-c", "import sys; sys.exit(1)"]
        with patch.object(self.app, "build_spawn_command", return_value=command):
            with self.assertRaises(ValueError): self.call("prepare", self.args)
        args = self.reviewed_pair("after-stuck")
        self.assertTrue(self.call("integrate", args)["work_integrated"])
        done = self.call("complete", {"obligation_id":"fixture-task", "worker_ids":[args["worker_id"]]})
        self.assertTrue(done["obligation_closed"])

    def test_v2_01_a_session_ending_never_turns_an_unused_preparation_into_a_final_worker(self):
        """Hunt part 4 v2, V2-01: `refresh` marked an unknown, never-dispatched
        preparation `interrupted` when its provider session ended; that status
        survives a resume, skipped settlement, and completion then demanded the
        receipt as a final worker with integration evidence for work it never
        did. Only a dispatched worker is interrupted by a session ending."""
        real = self.app.run
        def created_then_uncertain(command, **kw):
            real(command, **kw)
            raise TimeoutError("fixture interruption after workspace creation")
        with patch.object(self.app, "run", side_effect=created_then_uncertain):
            with self.assertRaises(TimeoutError): self.call("prepare", self.args)
        old = self.call("inspect")["records"][0]
        self.assertEqual(old["status"], "unknown")
        self.app.W.record_session_end(self.session, "fixture provider session ended")
        old = self.call("inspect")["records"][0]
        self.assertNotEqual(old["status"], "interrupted", "a receipt that never dispatched was interrupted")
        self.assertFalse(old.get("tool_use_id") or old.get("agent_id"))
        self.app.W.record_session_start(self.session, str(self.coord), source="resume")
        pair = self.reviewed_pair("after-session-ending")
        self.assertTrue(self.call("integrate", pair)["work_integrated"])
        done = self.call("complete", {"obligation_id":"fixture-task", "worker_ids":[pair["worker_id"]]})
        self.assertTrue(done["obligation_closed"])
        settled = [r for r in self.call("inspect")["records"] if r["id"] == old["id"]][0]
        self.assertEqual(settled["status"], "blocked")

    def test_v2_01_a_receipt_an_older_build_marked_interrupted_is_settled_too(self):
        """The same receipt as stored before this fix: `interrupted` with no
        dispatch evidence. It is read back as `unknown`, so the replacement
        settles it instead of refusing or counting it."""
        real = self.app.run
        def created_then_uncertain(command, **kw):
            real(command, **kw)
            raise TimeoutError("fixture interruption after workspace creation")
        with patch.object(self.app, "run", side_effect=created_then_uncertain):
            with self.assertRaises(TimeoutError): self.call("prepare", self.args)
        old = self.call("inspect")["records"][0]
        path = self.app.folder(self.scope) / (old["id"] + ".json")
        stored = json.loads(path.read_text())
        stored.update(status="interrupted", interruption="the owning provider process ended without an "
                                                         "observed worker result")
        path.write_text(json.dumps(stored))
        pair = self.reviewed_pair("after-legacy-interruption")
        self.assertTrue(self.call("integrate", pair)["work_integrated"])
        done = self.call("complete", {"obligation_id":"fixture-task", "worker_ids":[pair["worker_id"]]})
        self.assertTrue(done["obligation_closed"])

    def test_resumed_session_retires_disposed_execution_before_completing_replacement(self):
        worker=self.call("prepare",self.args)
        event={"session_id":self.session,"tool_use_id":"pre-update-call","tool_input":worker["agent_payload"]}
        self.app.dispatch_intent(self.scope,event)
        self.app.W.register_spawn(event,str(self.coord))
        self.app.W.bind_agent(self.session,event["tool_use_id"],"pre-update-agent",str(self.coord))
        key=self.app.W.named_key(self.session,worker["name"])
        target=Path(self.app.target_workspace(self.app.read_record(self.app.folder(self.scope),worker["id"]))["path"])
        self.app.W.record_session_end(self.session,"fixture app update")
        self.assertTrue(self.app.W.land(key,"fixture-cleanup",auto=True)["landed"])
        self.assertFalse(target.exists())
        self.assertFalse(Path(self.app.W.agent_path(key)).exists())
        self.assertTrue(Path(self.app.W.done_path(key)).exists())
        self.assertIsNone(self.app.W.load_agent(key).get("end"))
        self.app.W.record_session_start(self.session,str(self.coord),source="resume")
        self.assertEqual(self.app.W.session_state(self.session)[0],"running")
        record=self.call("inspect")["records"][0]
        self.assertEqual(record["status"],"interrupted","resuming Stu resurrected a canonically retired worker")
        work=self.app.ecs(self.scope,{"protocol":1,"command":"inspect","binding":self.scope["binding"],
            "query":{"section":"work","include_closed":True}})["records"]
        self.assertEqual(work[0]["status"],"cancelled")
        pair=self.reviewed_pair("after-app-update")
        self.assertTrue(self.call("integrate",pair)["work_integrated"])
        done=self.call("complete",{"obligation_id":"fixture-task","worker_ids":[pair["worker_id"]]})
        self.assertTrue(done["obligation_closed"])

    def test_cleanup_intent_does_not_retire_an_unresolved_execution(self):
        worker=self.call("prepare",self.args)
        event={"session_id":self.session,"tool_use_id":"still-unresolved","tool_input":worker["agent_payload"]}
        self.app.dispatch_intent(self.scope,event)
        self.app.W.register_spawn(event,str(self.coord))
        key=self.app.W.named_key(self.session,worker["name"])
        with self.app.W.Lock():
            canonical=self.app.W.load_agent(key)
            canonical["disposition"]={"kind":"landed","auto":True}
            self.app.W.save_agent(canonical)
        with self.assertRaisesRegex(ValueError,"unresolved execution"):
            self.app.completion_evidence(self.scope,"fixture-task",[])
        self.assertTrue(Path(self.app.W.agent_path(key)).exists())
        self.assertFalse(Path(self.app.W.done_path(key)).exists())

    def test_v2_01_a_dispatched_worker_is_still_interrupted_by_its_session_ending(self):
        """The twin: a receipt with dispatch evidence keeps the interruption."""
        prepared = self.call("prepare", self.args)
        key = self.app.W.named_key(self.session, prepared["name"])
        with self.app.W.Lock():
            canonical = self.app.W.load_agent(key); canonical["tool_use_id"] = "observed-spawn"
            self.app.W.save_agent(canonical)
        self.app.W.record_session_end(self.session, "fixture provider session ended")
        rec = [r for r in self.call("inspect")["records"] if r["id"] == prepared["id"]][0]
        self.assertEqual(rec["status"], "interrupted")

    def test_build_spawn_command_one_repository_is_byte_identical_to_todays_command(self):
        # POSITIVE CONTROL: the exact list `prepare()` built before this
        # function existed, for one repository. Any change to this function
        # that touches the single-repository path must fail here first.
        # `--audience app` joined it on 2026-09-18: everything reached from
        # here is the APP's dispatch, so it is judged by the user-work guards
        # and never by the development session's (Rich's ruling, CEO §57).
        # Slice 2 of the proto-teammate shelf plan: the type is the named teammate and the model
        # its own, where both were the fixed generic worker on sonnet.
        name,brief_path,title="mark-sonnet-abc123456789",Path("/x/y.brief"),"Some title"
        got=self.app.build_spawn_command([("/some/repo","/dest/path")],name,"mark","sonnet",brief_path,title,
            integration="main",base=None)
        want=[sys.executable,str(self.app.ENGINE/"scripts/lib/spawn.py"),name,"--repo","/some/repo",
            "--type","richos-app-engine:mark","--model","sonnet","--brief",str(brief_path),
            "--description",title,"--audience","app","--dir","/dest/path","--json","--integration","main"]
        self.assertEqual(got,want)

    def test_build_spawn_command_two_repositories_emits_two_repo_and_scoped_values(self):
        name,brief_path,title="frank-opus-def456789012","/x/y.brief","Some title"
        got=self.app.build_spawn_command(
            [("/repoA","/destA"),("/repoB","/destB")],name,"frank","opus",brief_path,title,
            integration="main",base="deadbeef")
        want=[sys.executable,str(self.app.ENGINE/"scripts/lib/spawn.py"),name,
            "--repo","/repoA","--repo","/repoB",
            "--type","richos-app-engine:frank","--model","opus","--brief",str(brief_path),
            "--description",title,"--audience","app","--dir","/repoA=/destA","--dir","/repoB=/destB","--json",
            "--integration","/repoA=main","--base","/repoA=deadbeef"]
        self.assertEqual(got,want)

    def test_build_spawn_command_scopes_a_per_repository_base_to_every_repository(self):
        # Part 4, finding 2: a reviewer or continuation of a job in several
        # repositories starts each workspace at that repository's own commit.
        got=self.app.build_spawn_command([("/repoA","/destA"),("/repoB","/destB")],"frank-opus-abc",
            "frank","opus","/x/y.brief","Review",integration=None,base={"/repoB":"b"*40,"/repoA":"a"*40})
        self.assertEqual(got[got.index("--json")+1:],["--base","/repoA="+"a"*40,"--base","/repoB="+"b"*40])

    def test_prepare_with_repos_creates_a_workspace_in_each_and_scopes_the_form(self):
        second=self.root/"second project";second.mkdir()
        subprocess.run(["git","init","--template=","-q","-b","main",str(second)],check=True)
        subprocess.run(["git","-C",str(second),"-c","core.hooksPath=/dev/null","-c","commit.gpgSign=false","-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","--allow-empty","-qm","Fixture"],check=True)
        self.registry.write_text(json.dumps({"version":2,"entities":[{"id":"depot","roots":[str(self.repo),str(second)],
            "connected_repositories":[str(self.repo),str(second)]}]}))
        ready=self.call("prepare",{**self.args,"request_id":"prepare-multi","repos":[str(second)]})
        self.assertEqual(ready["status"],"prepared")
        lines=[l for l in ready["agent_payload"]["prompt"].splitlines() if l.startswith("cross-repo-worktree:")]
        # Neither self.repo nor second is this session's own repository
        # (RICHOS_ENTITY_ROOT is self.coord), so EACH gets its own registered
        # cc/ workspace and its own line in the one payload (point 10).
        self.assertEqual(len(lines),2)
        # Each cross-repo-worktree line names its own DESTINATION (the
        # workspace path), not the source repository — the destination is
        # namespaced by the repository's own basename, so that is what each
        # repository's line is checked against.
        self.assertTrue(any(self.repo.name in l for l in lines))
        self.assertTrue(any(second.name in l for l in lines))
        self.assertEqual(self.call("inspect")["records"][0]["request"]["repos"],[str(self.repo),str(second)])

    def test_a_repository_connected_to_two_companies_is_listed_and_dispatched_for_the_second(self):
        """Plan item 11 (CEO 2026-10-21): the shared repository is in ONE company's roots
        (roots stay exclusive) and connected by both. The second company, "depot", has it only
        in connected_repositories; listing and a job in it must work, as in the first company."""
        shared=str(self.repo)
        def registry(owner_connects):
            self.registry.write_text(json.dumps({"version":2,"entities":[
                {"id":"first","roots":[shared],"connected_repositories":[shared] if owner_connects else []},
                {"id":"depot","roots":[],"connected_repositories":[shared]}]}))
        registry(True)
        self.assertEqual(self.call("repositories")["repositories"],[shared])
        ready=self.call("prepare",{**self.args,"request_id":"prepare-shared"})
        self.assertEqual(ready["status"],"prepared")
        # A repository its owner has not connected is still refused: sharing is the owner's say.
        registry(False)
        with self.assertRaises(ValueError):self.call("repositories")

    def second_repository(self):
        second=self.root/"second project";second.mkdir()
        subprocess.run(["git","init","--template=","-q","-b","main",str(second)],check=True)
        subprocess.run(["git","-C",str(second),"-c","core.hooksPath=/dev/null","-c","commit.gpgSign=false","-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","--allow-empty","-qm","Fixture"],check=True)
        for key,value in (("user.name","Fixture"),("user.email","fixture@example.invalid")):
            subprocess.run(["git","-C",str(second),"config",key,value],check=True)
        self.registry.write_text(json.dumps({"version":2,"entities":[{"id":"depot","roots":[str(self.repo),str(second)],
            "connected_repositories":[str(self.repo),str(second)]}]}))
        return second

    def workspaces_of(self,ready):
        canonical=self.app.W.load_agent(self.app.W.named_key(self.session,ready["name"]))
        return {w["repo"]:w["path"] for w in canonical["workspaces"] if w.get("kind")=="cc"}

    def commit_in_each(self,paths,file,text):
        commits={}
        for repo,path in paths.items():
            (Path(path)/file).write_text(text+" in "+Path(repo).name)
            self.app.git(path,"add",file)
            self.app.git(path,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Fictional "+file)
            commits[repo]=self.app.git(path,"rev-parse","HEAD")
        return commits

    def test_a_job_in_two_repositories_is_edited_reviewed_and_landed_in_both(self):
        """Part 4, finding 2. `prepare` accepted a job in two repositories and
        created a workspace in each, and then every later step handled only the
        primary: a direct edit in the second workspace was refused, review named
        only the primary commit, integrate merged only the primary repository
        and canonical cleanup then refused the unmerged second one."""
        second=self.second_repository()
        spec={**self.args,"request_id":"multi-worker","repos":[str(second)]}
        worker=self.call("prepare",spec)
        self.start_fixture_worker(worker,"multi-worker")
        paths=self.workspaces_of(worker)
        self.assertEqual(sorted(paths),sorted([str(self.repo),str(second)]))
        # The edit boundary covers every workspace of the job, and still nothing else.
        event={"session_id":self.session,"agent_id":"multi-worker","cwd":str(self.coord),"tool_name":"Write",
               "tool_input":{"file_path":str(Path(paths[str(second)])/"result.txt")}}
        context=self.app.worker_context(self.scope,event)["hookSpecificOutput"]["additionalContext"]
        for path in paths.values(): self.assertIn(str(Path(path).resolve()),context)
        with self.assertRaisesRegex(ValueError,"outside"):
            self.app.worker_context(self.scope,{**event,"tool_input":{"file_path":str(self.root/"elsewhere.txt")}})
        commits=self.commit_in_each(paths,"result.txt","FICTIONAL")
        self.finish_fixture_worker("multi-worker")
        # A review that does not cover every repository is refused before anything is created.
        with self.assertRaisesRegex(ValueError,"exactly the repositories"):
            self.call("prepare",{**self.args,"request_id":"partial-review","role":"reviewer","review_of":worker["id"]})
        reviewer=self.call("prepare",{**spec,"request_id":"multi-review","role":"reviewer","review_of":worker["id"],
            "title":"Review both","brief":"Review the change in both repositories; do not modify any files."})
        self.assertEqual(reviewer["review_target"]["commits"],commits)
        self.start_fixture_worker(reviewer,"multi-reviewer")
        review_paths=self.workspaces_of(reviewer)
        for repo,path in review_paths.items():
            self.assertEqual(self.app.git(path,"rev-parse","HEAD"),commits[repo])   # each at the worker's exact commit
        # A verdict naming only the primary commit is not a review of this work.
        primary=commits[str(self.repo)]
        self.finish_fixture_worker("multi-reviewer","RICHOS_REVIEW "+json.dumps(
            {"commit":primary,"verdict":"passed","checks":["primary only"]}))
        with self.assertRaisesRegex(ValueError,"passing review"):
            self.call("integrate",{"worker_id":worker["id"],"reviewer_id":reviewer["id"]})
        self.assertNotEqual(self.app.git(second,"rev-parse","main"),commits[str(second)])
        self.finish_fixture_worker("multi-reviewer","RICHOS_REVIEW "+json.dumps(
            {"commit":primary,"commits":commits,"verdict":"passed","checks":["both repositories"]}))
        landed=self.call("integrate",{"worker_id":worker["id"],"reviewer_id":reviewer["id"]})
        self.assertTrue(landed["work_integrated"])
        self.assertEqual(landed["cleanup_pending"],[])
        for repo,commit in commits.items():
            head=self.app.git(repo,"rev-parse","main")
            self.assertEqual(self.app.git(repo,"rev-parse",head+"^2"),commit)   # each repository's own merge
            self.assertIn(commit,landed["evidence_ref"])
            self.assertEqual(self.app.W.land_records(repo)[-1]["commit"],head)
            self.assertEqual(self.app.W.land_records(repo)[-1]["reviewed_commit"],commit)
        self.assertEqual(sorted(landed["land_locks"]),sorted(commits))
        for path in list(paths.values())+list(review_paths.values()): self.assertFalse(Path(path).exists(),path)
        self.assertTrue(self.call("complete",{"obligation_id":"fixture-task","worker_ids":[worker["id"]]})["obligation_closed"])

    def test_a_continuation_of_a_job_in_two_repositories_restarts_each_at_its_saved_commit(self):
        """Part 4, finding 2, continuation: the saved commit was read from the
        primary repository only, so the second repository's new workspace began
        somewhere else while canonical continuation deleted the old one."""
        second=self.second_repository()
        spec={**self.args,"request_id":"multi-first","repos":[str(second)]}
        worker=self.call("prepare",spec)
        self.start_fixture_worker(worker,"multi-first")
        commits=self.commit_in_each(self.workspaces_of(worker),"partial.txt","SAVED")
        self.finish_fixture_worker("multi-first")
        with self.assertRaisesRegex(ValueError,"exactly the repositories"):
            self.call("prepare",{**self.args,"request_id":"narrow-continue","continue_of":worker["id"]})
        continued=self.call("prepare",{**spec,"request_id":"multi-continue","continue_of":worker["id"]})
        self.assertEqual(continued["continuation"]["commits"],commits)
        self.start_fixture_worker(continued,"multi-continue")
        for repo,path in self.workspaces_of(continued).items():
            self.assertEqual(self.app.git(path,"rev-parse","HEAD"),commits[repo])
            self.assertEqual((Path(path)/"partial.txt").read_text(),"SAVED in "+Path(repo).name)

    def test_prepare_rejects_a_repeated_or_unconnected_repos_entry(self):
        with self.assertRaisesRegex(ValueError,"distinct"):
            self.call("prepare",{**self.args,"request_id":"prepare-dup","repos":[str(self.repo)]})
        unconnected=self.root/"not connected";unconnected.mkdir()
        with self.assertRaisesRegex(ValueError,"connect this exact repository"):
            self.call("prepare",{**self.args,"request_id":"prepare-unconnected","repos":[str(unconnected)]})

    def test_dispatch_requires_exact_payload_and_blocks_duplicate_attempt(self):
        ready=self.call("prepare",self.args)
        envelope={"session_id":self.session,"tool_use_id":"actual-native-id","tool_input":ready["agent_payload"]}
        with self.assertRaisesRegex(ValueError,"differs"):self.app.dispatch_intent(self.scope,{**envelope,"tool_input":{**ready["agent_payload"],"prompt":"forged"}})
        self.app.dispatch_intent(self.scope,envelope)
        with self.assertRaisesRegex(ValueError,"already attempted"):self.app.dispatch_intent(self.scope,envelope)
        self.assertNotIn("agent_payload",self.call("prepare",self.args))
        with self.assertRaisesRegex(ValueError,"unresolved"):self.call("prepare",{**self.args,"request_id":"duplicate"})
    def test_unregistered_scope_stopped_turn_and_unknown_obligation_refuse_before_workspace_creation(self):
        other=self.root/"unrelated";other.mkdir()
        for change in ({"repo":str(other)},{"obligation_id":"unknown"}):
            with self.assertRaises(Exception):self.call("prepare",{**self.args,**change})
        self.scope["actions_allowed"]=False;self.scope_path.write_text(json.dumps(self.scope))
        with self.assertRaises(ValueError):self.call("prepare",self.args)
        self.assertFalse((self.root/"engine-state/target-worktrees").exists())
    def test_app_does_not_parse_git_commands_for_the_retired_classifier(self):
        for command in ['git -C "$TARGET" status', 'git -C $TARGET status', 'git -C "${TARGET}" log -1', 'git -C "$(pwd)" status', 'git commit -m "$(cat <<EOF\nFixture\nEOF\n)"', 'git commit -m `echo Fixture`']:
            with self.subTest(command=command):
                self.assertIsNone(self.app.validate_shell_target({"tool_name":"Bash","tool_input":{"command":command}}))
        for command in ['git -C "/fictional/target with spaces" status', "git -C '/fictional/$literal' status",  'python3 ./tests.py', "git commit -m 'Fix `marker` and $(literal)'"]:
            self.assertIsNone(self.app.validate_shell_target({"tool_name":"Bash","tool_input":{"command":command}}))
        self.assertIsNone(self.app.validate_shell_target({"tool_name":"Read","tool_input":{"command":"git -C $TARGET status"}}))

    def test_inline_shell_c_scripts_are_refused_with_the_rewrite_and_script_files_pass(self):
        for command in ["bash -c 'rm -rf /fictional/x'", "sh -c 'echo hi'", "/bin/zsh -lc 'echo hi'",
                        "cd /fictional && bash -o pipefail -c 'echo hi'", "ls | xargs -I{} bash -c 'echo {}'",
                        "bash --noprofile -c 'echo hi'", "'bash' -c 'echo hi'", "ls | xargs -I ITEM bash -c 'echo ITEM'",
                        "bash\t-c\t'echo hi'", 'git commit -m "refuse bash -c wrappers"']:
            with self.subTest(command=command), self.assertRaisesRegex(ValueError, r"bash <file>"):
                self.app.validate_shell_target({"tool_name":"Bash","tool_input":{"command":command}})
        for command in ["bash /fictional/scratch/run.sh", "bash -x ./run.sh", "python3 -c 'print(1)'",
                        "git -c core.editor=true status", "bash test.sh && git diff --cached", "ls -la"]:
            with self.subTest(command=command):
                self.assertIsNone(self.app.validate_shell_target({"tool_name":"Bash","tool_input":{"command":command}}))
        self.assertIsNone(self.app.validate_shell_target({"tool_name":"Read","tool_input":{"command":"bash -c x"}}))

    def test_worker_file_boundary_uses_provider_identity_and_registered_target(self):
        ready=self.call("prepare",self.args)
        envelope={"session_id":self.session,"tool_use_id":"observed-call","tool_input":ready["agent_payload"]}
        self.app.dispatch_intent(self.scope,envelope)
        self.app.W.register_spawn(envelope,str(self.coord))
        self.app.W.bind_agent(self.session,"observed-call","observed-agent",str(self.coord))
        target=self.root/"engine-state/target-worktrees"/self.app.folder(self.scope).name/ready["name"]
        event={"session_id":self.session,"agent_id":"observed-agent","cwd":str(self.coord),"tool_name":"Write","tool_input":{"file_path":"wrong.txt"}}
        with self.assertRaisesRegex(ValueError,"outside"):self.app.worker_context(self.scope,event)
        event["tool_input"]["file_path"]=str(target/"right.txt")
        self.assertIn(str(target),self.app.worker_context(self.scope,event)["hookSpecificOutput"]["additionalContext"])
        (target/"redirect").symlink_to(self.coord,target_is_directory=True)
        event["tool_input"]["file_path"]=str(target/"redirect/wrong.txt")
        with self.assertRaisesRegex(ValueError,"outside"):self.app.worker_context(self.scope,event)

    def test_dispatched_worker_survives_later_stu_inputs_but_cannot_cross_session_or_conversation(self):
        ready=self.call("prepare",self.args)
        self.start_fixture_worker(ready,"standing-worker")
        target=self.root/"engine-state/target-worktrees"/self.app.folder(self.scope).name/ready["name"]
        event={"session_id":self.session,"agent_id":"standing-worker","cwd":str(self.coord),
               "tool_name":"Write","tool_input":{"file_path":str(target/"result.txt")}}
        later={**self.scope,"binding":{**self.scope["binding"],"turn_id":"next-message","revision":2,"audience":"worker"}}
        self.assertIn(str(target),self.app.worker_context(later,event)["hookSpecificOutput"]["additionalContext"])
        for key in ("entity_id","thread_id","session_id"):
            with self.subTest(key=key),self.assertRaises(ValueError):
                self.app.worker_context({**later,"binding":{**later["binding"],key:"another"}},event)

    def start_fixture_worker(self, ready, aid):
        payload={"session_id":self.session,"tool_use_id":"call-"+aid,"tool_input":ready["agent_payload"]}
        self.app.dispatch_intent(self.scope,payload)
        # What the teammate receives, as the hook hands it to Agent.
        self.__dict__.setdefault("delivered_prompts",{})[aid]=payload["tool_input"]["prompt"]
        self.app.W.register_spawn(payload,str(self.coord))
        native=self.coord/".claude/worktrees"/("agent-"+aid)
        self.app.git(self.coord,"worktree","add","-b","worktree-agent-"+aid,str(native),"HEAD")
        self.app.W.record_start(self.session,aid,str(native),"richos-app-engine:"+ready["request"]["teammate"])
        self.app.W.bind_agent(self.session,payload["tool_use_id"],aid,str(self.coord))
        return self.root/"engine-state/target-worktrees"/self.app.folder(self.scope).name/ready["name"]

    def finish_fixture_worker(self,aid,report=""):
        self.app.W.record_end(self.session,aid,"SubagentStop")
        self.app.observe(self.scope,{"hook_event_name":"SubagentStop","session_id":self.session,"agent_id":aid,"last_assistant_message":report})

    def test_an_observation_rewrites_only_the_receipts_that_changed(self):
        """Part 4 (2026-09-29 hunt), finding 22: observe() saved every receipt in
        the conversation (write, flush, replace, directory flush) after every tool
        call, finished ones included."""
        command=[sys.executable,"-c","import sys; sys.exit(1)"]
        with patch.object(self.app,"build_spawn_command",return_value=command):
            with self.assertRaises(ValueError): self.call("prepare",self.args)
        worker=self.call("prepare",{**self.args,"request_id":"replacement"})   # settles the first: two receipts
        self.assertEqual(len(self.call("inspect")["records"]),2)
        real=self.app.save
        saved=[]
        def counting(path,value):
            saved.append(path.name); return real(path,value)
        event={"hook_event_name":"PostToolUse","session_id":self.session,"agent_id":"nobody","tool_name":"Bash"}
        with patch.object(self.app,"save",side_effect=counting):
            self.app.observe(self.scope,event)
        self.assertEqual(saved,[])
        self.start_fixture_worker(worker,"hunt22-worker")        # the one worker's record really changes
        with patch.object(self.app,"save",side_effect=counting):
            self.app.observe(self.scope,event)
            self.app.observe(self.scope,event)
        self.assertEqual(saved,[worker["id"]+".json"])           # once, for it alone
        with self.app.locked(self.scope) as root:
            self.assertEqual(self.app.read_record(root,worker["id"])["status"],"running")

    def test_a_slow_preparation_does_not_hold_the_conversation_lock(self):
        """Part 4 (2026-09-29 hunt), finding 23: prepare() held the conversation's
        one lock while its spawn command ran, so every other worker's permission
        check, lifecycle record and inspection in that conversation waited behind
        a slow setup. The lock is now let go for the command, and a second request
        for the same assignment meanwhile finds a receipt that is being prepared
        and leaves it (and the workspace being created) alone."""
        real = self.app.run
        seen = {}
        def run_while_probing(command, **kw):
            if "spawn.py" not in " ".join(map(str, command)):       # git calls made by the probes below
                return real(command, **kw)
            fd = os.open(self.app.folder(self.scope) / ".lock", os.O_RDWR | os.O_CREAT)
            try:
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    seen["lock"] = "free"
                    fcntl.flock(fd, fcntl.LOCK_UN)
                except BlockingIOError:
                    seen["lock"] = "held"
            finally:
                os.close(fd)
            if seen["lock"] == "free":      # only then is it safe to do what other workers do meanwhile
                self.app.observe(self.scope, {"hook_event_name": "PostToolUse", "session_id": self.session,
                                              "agent_id": "nobody", "tool_name": "Bash"})
                seen["records"] = len(self.call("inspect")["records"])
                with self.assertRaisesRegex(ValueError, "unresolved work"):
                    self.call("prepare", {**self.args, "request_id": "second-request"})
                seen["same-request"] = self.call("prepare", self.args)["status"]
            return real(command, **kw)
        with patch.object(self.app, "run", side_effect=run_while_probing):
            ready = self.call("prepare", self.args)
        self.assertEqual(seen.get("lock"), "free")
        self.assertEqual(seen["records"], 1)
        self.assertEqual(seen["same-request"], "preparing")          # the same request is told it is underway
        self.assertEqual(ready["status"], "prepared")
        self.assertNotIn("preparing_by", self.call("inspect")["records"][0])
        # the workspace the command created was not withdrawn by the second request
        self.assertTrue(Path(self.app.target_workspace(ready)["path"]).is_dir())

    def test_independent_work_in_one_assignment_can_overlap_without_repeating_dispatch(self):
        for role, teammate in (("worker", "mark"), ("consult", "clark")):
            with self.subTest(role=role):
                args = {**self.args, "role": role, "teammate": teammate, "request_id": role + "-first"}
                if role == "consult":
                    args.pop("repo")
                    args.pop("integration")
                first = self.call("prepare", args)
                self.start_fixture_worker(first, role + "-first-agent")
                second = self.call("prepare", {**args, "request_id": role + "-second",
                    "title": "Answer the independent question", "brief": "Read the independent fictional source and report its date."})
                self.start_fixture_worker(second, role + "-second-agent")
                records = {r["id"]: r for r in self.call("inspect")["records"]}
                self.assertEqual([records[r["id"]]["status"] for r in (first, second)], ["running", "running"])
                retry = self.call("prepare", args)
                self.assertEqual(retry["id"], first["id"])
                self.assertNotIn("agent_payload", retry)
                with self.assertRaisesRegex(ValueError, "request_id was already used for different work"):
                    self.call("prepare", {**args, "brief": "Different work under the same request id."})
                for which in ("first", "second"):
                    self.finish_fixture_worker(role + "-" + which + "-agent", "The fictional source dates to 2020.")

    def test_a_dead_preparers_receipt_is_still_settled_for_a_replacement(self):
        """The owner stamp only protects a preparation that is RUNNING: the receipt of
        a process that is gone (a crash between the two lock holds) is settled as
        never dispatched, exactly as before."""
        command = [sys.executable, "-c", "import sys; sys.exit(1)"]
        with patch.object(self.app, "build_spawn_command", return_value=command):
            with self.assertRaises(ValueError): self.call("prepare", self.args)
        with self.app.locked(self.scope) as root:
            (path, record), = list(self.app.receipts(root))
            record.update(status="preparing", preparing_by={"pid": 2 ** 22 + 12345, "started": "Thu Jan  1 00:00:00 1970"})
            self.app.save(path, record)
        self.assertTrue(self.app.preparing_elsewhere(record) is False)
        replacement = self.call("prepare", {**self.args, "request_id": "replacement"})
        self.assertEqual(replacement["status"], "prepared")

    def test_native_handback_requires_success_identity_and_observed_reviewer_end(self):
        worker=self.call("prepare",self.args)
        target=self.start_fixture_worker(worker,"handback-worker")
        (target/"result.txt").write_text("FICTIONAL")
        self.app.git(target,"add","result.txt")
        self.app.git(target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Fictional handback result")
        commit=self.app.git(target,"rev-parse","HEAD")
        self.finish_fixture_worker("handback-worker")
        reviewer=self.call("prepare",{**self.args,"request_id":"handback-review","role":"reviewer","review_of":worker["id"]})
        self.start_fixture_worker(reviewer,"handback-reviewer")
        report="RICHOS_REVIEW "+json.dumps({"commit":commit,"verdict":"passed","checks":["synthetic exact-commit review"]})
        event={"hook_event_name":"PostToolUse","session_id":self.session,"agent_id":"handback-reviewer",
               "tool_name":"SubagentHandback","tool_use_id":"actual-handback-call", "tool_input":{"message":report},"tool_response":{"success":True}}
        def receipt():
            with self.app.locked(self.scope) as root: return self.app.read_record(root,reviewer["id"])
        for change in ({"hook_event_name":"PreToolUse"},{"agent_id":"another-agent"},{"session_id":"another-session"}):
            self.app.observe(self.scope,{**event,**change})
            self.assertNotIn("review_handback",receipt())
        self.app.observe(self.scope,{**event,"tool_response":{"success":False}})
        self.assertFalse(receipt()["review_handback"]["valid"])
        self.app.observe(self.scope,{**event,"tool_input":{"message":report.replace(commit,"0"*40)}})
        self.assertFalse(receipt()["review_handback"]["valid"])
        self.app.observe(self.scope,event)
        self.assertTrue(receipt()["review_handback"]["valid"])
        self.assertNotIn("review_observation",receipt())
        with self.assertRaises(ValueError): self.call("integrate",{"worker_id":worker["id"],"reviewer_id":reviewer["id"]})
        self.finish_fixture_worker("handback-reviewer","Report delivered to caller.")
        observation=receipt()["review_observation"]
        self.assertTrue(observation["valid"])
        self.assertTrue(observation["end_observed"])
        self.assertEqual(observation["tool_use_id"],"actual-handback-call")
        self.assertEqual(observation["report"]["commit"],commit)
        self.assertIn("review:",self.app.verification_evidence(self.scope,reviewer["id"]))
        # Explicit malformed final reports and failed later handbacks cannot reuse an old pass.
        self.finish_fixture_worker("handback-reviewer","RICHOS_REVIEW malformed")
        self.assertFalse(receipt()["review_observation"]["valid"])
        self.app.observe(self.scope,{**event,"hook_event_name":"PostToolUseFailure","error":"delivery failed"})
        self.finish_fixture_worker("handback-reviewer","No delivered report.")
        self.assertFalse(receipt()["review_observation"]["valid"])
        self.app.observe(self.scope,event)
        self.finish_fixture_worker("handback-reviewer","Report delivered to caller.")
        self.assertTrue(self.call("integrate",{"worker_id":worker["id"],"reviewer_id":reviewer["id"]})["work_integrated"])

    def test_interrupted_continuation_preserves_dirty_files_and_uses_original_session_key(self):
        worker=self.call("prepare",self.args)
        target=self.start_fixture_worker(worker,"interrupted-worker")
        (target/"unfinished.txt").write_text("KEEP UNFINISHED BYTES")
        old_session=self.session
        self.app.W.record_session_end(old_session,"synthetic interruption")
        old_revision=self.scope["binding"]["revision"]
        self.session="replacement-session"
        os.environ["RICHOS_SESSION_ID"]=self.session
        self.app.W.record_session_start(self.session,str(self.coord))
        binding=self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"bind",
            "scope":{"entity_id":"depot","thread_id":"thread-a","session_id":self.session,"turn_id":"resume-turn","audience":"ceo"},
            "request_id":"resume-binding","source_ref":"ledger:thread-a:resume-turn","expected_revision":old_revision})["binding"]
        self.scope["binding"]=binding;self.scope_path.write_text(json.dumps(self.scope))
        follow={**self.args,"request_id":"continue-one","continue_of":worker["id"],"brief":"Continue the fictional assignment from its saved progress. Preserve the existing file."}
        with self.assertRaisesRegex(self.app.W.SpecError,"uncommitted|dirty"):
            self.call("prepare",follow)
        self.assertEqual((target/"unfinished.txt").read_text(),"KEEP UNFINISHED BYTES")
        self.assertEqual(len(self.call("inspect")["records"]),1)
        # Explicit fictional reconciliation before continuation. The adapter never
        # resets, deletes or silently commits the old dirty files itself.
        self.app.git(target,"add","unfinished.txt")
        self.app.git(target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Saved fictional progress")
        continued=self.call("prepare",follow)
        next_target=self.start_fixture_worker(continued,"replacement-worker")
        canonical=self.app.W.load_agent(self.app.W.named_key(self.session,continued["name"]))
        self.assertEqual(canonical["continues"],[self.app.W.named_key(old_session,worker["name"])])
        self.assertEqual((next_target/"unfinished.txt").read_text(),"KEEP UNFINISHED BYTES")
        self.assertFalse(target.exists())
        self.assertFalse((self.repo/"unfinished.txt").exists())

    def test_a_handoff_commit_message_goes_into_the_continuation_brief(self):
        """Weekly-switch plan section 2, step 4 (slice 4). The worker the quota gate
        ordered made its work commit, then its `RichOS handoff:` commit, and ended.
        Its continuation starts at that commit, and the handoff is in the new brief:
        nobody copies it."""
        worker=self.call("prepare",self.args)
        target=self.start_fixture_worker(worker,"handoff-worker")
        who=["-c","user.name=Fixture","-c","user.email=fixture@example.invalid"]
        (target/"result.txt").write_text("FICTIONAL")
        self.app.git(target,"add","result.txt")
        self.app.git(target,*who,"commit","-qm","Half of the fictional result")
        handoff="RichOS handoff: result.txt is written. Left: its second line. Next: add FICTIONAL TWO."
        self.app.git(target,*who,"commit","--allow-empty","-qm",handoff)
        saved=self.app.git(target,"rev-parse","HEAD")
        self.finish_fixture_worker("handoff-worker")
        continued=self.call("prepare",{**self.args,"request_id":"continue-handoff","continue_of":worker["id"]})
        self.assertIn("Handoff from the previous teammate:\n"+handoff,continued["agent_payload"]["prompt"])
        next_target=self.start_fixture_worker(continued,"successor-worker")
        self.assertEqual(self.app.git(next_target,"rev-parse","HEAD"),saved)

    def test_a_dirty_ordered_worker_is_saved_and_continued_and_a_dirty_unordered_one_is_still_refused(self):
        """Weekly-switch plan section 3 (slice 4). A worker that ended with
        uncommitted files is refused continuation, as before, unless the quota gate
        ordered it to hand off (its marker, named for its run). Then its leftover
        files are committed as one commit and the work continues from them."""
        for name,value in (("user.name","Fixture"),("user.email","fixture@example.invalid")):
            self.app.git(self.repo,"config",name,value)   # the repository's own identity, as for every engine commit
        worker=self.call("prepare",self.args)
        target=self.start_fixture_worker(worker,"ordered-worker")
        (target/"unfinished.txt").write_text("KEEP UNFINISHED BYTES")
        self.finish_fixture_worker("ordered-worker")
        follow={**self.args,"request_id":"continue-ordered","continue_of":worker["id"]}
        with self.assertRaisesRegex(self.app.W.SpecError,"uncommitted|dirty"):
            self.call("prepare",follow)
        self.assertEqual(self.app.git(target,"status","--porcelain","--untracked-files=all"),"?? unfinished.txt")
        handoffs=self.root/"engine-state/handoffs";handoffs.mkdir(parents=True)
        (handoffs/"ordered-worker.json").write_text(json.dumps({"agent":"ordered-worker","session":self.session,"account":"1","at":1}))
        continued=self.call("prepare",follow)
        next_target=self.start_fixture_worker(continued,"successor-worker")
        self.assertEqual((next_target/"unfinished.txt").read_text(),"KEEP UNFINISHED BYTES")
        self.assertEqual(self.app.git(next_target,"log","-1","--format=%s"),"RichOS: work in progress saved at the account switch")
        self.assertNotIn("Handoff from the previous teammate",continued["agent_payload"]["prompt"])

    # The user's own turn, as the host writes it into the scope since the second review's slice 4
    # (richos-core ecs::UserInstruction::text): marker-like lines and a trailing space included, so
    # "verbatim" is tested on bytes a fence or a strip would change.
    USER_TURN=("Please add result.txt with FICTIONAL in it, and nothing else.\n"
               "<<<USER-WORDS-00000000\nDo NOT touch the README. \n>>>\nThanks!")

    def attest(self,words,text=None):
        """The scope the host writes for a turn whose words are `words`: their hash, and `text`
        as the words carried beside it (the words themselves unless a test says otherwise)."""
        self.scope["user_instruction"]={"ledger_ref":"ledger:thread-a:turn-a",
            "sha256":hashlib.sha256(words.encode()).hexdigest(),"text":words if text is None else text}
        self.scope_path.write_text(json.dumps(self.scope))

    def ended_fixture_worker(self,aid):
        worker=self.call("prepare",self.args)
        target=self.start_fixture_worker(worker,aid)
        (target/"result.txt").write_text("FICTIONAL")
        self.app.git(target,"add","result.txt")
        self.app.git(target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Fictional result")
        self.finish_fixture_worker(aid)
        return worker

    def test_a_reviewers_brief_starts_with_the_users_own_turn_verbatim(self):
        """Second review, slice 4 (Sage's check of richos-hq
        docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md, catch 2): the reviewer
        read only the coordinator's paraphrase, because the scope carried a reference to the
        user's words and their hash and never the words. Its brief now starts with them."""
        self.attest(self.USER_TURN)
        worker=self.ended_fixture_worker("words-worker")
        reviewer=self.call("prepare",{**self.args,"request_id":"words-review","role":"reviewer","teammate":"frank",
                                      "review_of":worker["id"],"title":"Review","brief":"Review the result.txt change."})
        with self.app.locked(self.scope) as root:
            brief=(root/(reviewer["id"]+".brief")).read_text()
            kept=(root/(worker["id"]+".words")).read_text()
        head=self.app.USER_WORDS_LABEL+"\n\n<<<USER-WORDS-"
        self.assertTrue(brief.startswith(head),brief[:200])
        opening=brief.index("\n",len(head))+1
        self.assertEqual(brief[opening:opening+len(self.USER_TURN)],self.USER_TURN)
        # Then the duty and the coordinator's brief, as before, and only after the words.
        self.assertLess(opening+len(self.USER_TURN),brief.index(DUTY_TOP["reviewer"]))
        self.assertIn(self.USER_TURN,reviewer["agent_payload"]["prompt"])
        # The worker's words are kept beside its receipt for the app's mid-job reviews.
        self.assertEqual(kept,self.USER_TURN)

    def test_a_scope_whose_words_do_not_match_their_hash_is_refused(self):
        """The words on the scope must be the turn the host attested: a reviewer briefed with
        anything else would judge the work against words the user never said."""
        worker=self.ended_fixture_worker("mismatch-worker")
        self.attest(self.USER_TURN,text="Words the user never said.")
        for role,extra in (("reviewer",{"review_of":worker["id"]}),("worker",{})):
            with self.assertRaisesRegex(ValueError,"do not match the turn the app attested"):
                self.call("prepare",{**self.args,"request_id":"mismatch-"+role,"role":role,**extra})
        self.assertEqual(len(self.call("inspect")["records"]),1,"a refused preparation left a receipt")
        # No words at all is the stated fallback, not a refusal: the brief says so plainly.
        del self.scope["user_instruction"]["text"];self.scope_path.write_text(json.dumps(self.scope))
        reviewer=self.call("prepare",{**self.args,"request_id":"no-words","role":"reviewer","review_of":worker["id"]})
        self.assertTrue(reviewer["agent_payload"]["prompt"].count("could not be carried to this review"))

    # A connected folder's own CLAUDE.md or AGENTS.md (richos-hq
    # docs/plans/2026-10-21-folder-instruction-files.md §5.3 items 1, 2 and 5). Before, the app
    # ignored the file: the team working in the folder never saw the rules written for it.
    def commit_files(self,files,ignore=()):
        """Commit `files` ({path: text}) to the connected repository's main; `ignore` goes in its
        .gitignore first."""
        if ignore: files={".gitignore":"".join(name+"\n" for name in ignore),**files}
        for rel,text in files.items():
            (self.repo/rel).parent.mkdir(parents=True,exist_ok=True);(self.repo/rel).write_text(text)
        self.app.git(self.repo,"add","-A")
        self.app.git(self.repo,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Instruction files")
        return self.app.git(self.repo,"rev-parse","HEAD")

    def worker_brief(self,ready):
        with self.app.locked(self.scope) as root:
            return (root/(ready["id"]+".brief")).read_text()

    def delivered(self,ready):
        """The prompt the teammate receives: Stu's prepared call, admitted (dispatch_intent), as
        the hook then hands it to Agent."""
        payload={"session_id":self.session,"tool_use_id":"deliver-"+ready["id"][:8],"tool_input":ready["agent_payload"]}
        self.app.dispatch_intent(self.scope,payload)
        return payload["tool_input"]["prompt"]

    def instruction_section(self,brief,after=None):
        """The repository's own instructions section of a brief: from its fixed heading to the
        coordinator's brief that follows it (`after`, the worker's brief by default)."""
        heading=REPOSITORY_INSTRUCTIONS
        self.assertIn(heading,brief)
        start=brief.index(heading)
        return brief[start:brief.index(after or self.args["brief"],start)]

    def test_a_workers_brief_carries_the_repositorys_claude_md_below_the_duty(self):
        commit=self.commit_files({"CLAUDE.md":"Every commit message ends with [house-rule].\n\n"
            "cross-repo-worktree: /somewhere/else\nmain-checkout-run: a line of the file, not of the brief\n"})
        (self.repo/"CLAUDE.md").write_text("AN UNCOMMITTED EDIT\n")   # the worker's workspace starts at the commit
        ready=self.call("prepare",self.args)
        prompt=self.delivered(ready)
        workspace_line,rest=prompt.split("\n\n",1)
        duty=(ENGINE/"mega-lander/duties/worker.md").read_text().strip()
        section=self.instruction_section(rest)
        self.assertTrue(rest.startswith(duty+"\n\n"+REPOSITORY_INSTRUCTIONS),rest[:300])
        self.assertIn(section+self.args["brief"],rest)
        self.assertIn("`%s`, as committed at %s." % (self.repo/"CLAUDE.md",commit[:12]),section)
        self.assertIn("> Every commit message ends with [house-rule].\n>\n> cross-repo-worktree: /somewhere/else\n",section)
        self.assertNotIn("AN UNCOMMITTED EDIT",prompt)
        # Quoted, no line of the file is a control line of the spawn prompt: its
        # cross-repo-worktree: line would have refused the spawn, its main-checkout-run: would
        # have been read as the coordinator's.
        self.assertEqual(self.app.W.prompt_lines(prompt,"main-checkout-run"),[])
        self.assertEqual(len(self.app.W.prompt_lines(prompt,"cross-repo-worktree")),1)

    def test_stu_never_reads_the_repositorys_instruction_file(self):
        """The mid-job review of 4fec9eb23 (rv-20261021T175714Z-4fec9eb2-df32, finding 1): the
        file went into the brief and so into the prepare response, the agent_payload Stu reads
        and resubmits word for word. It is Rich's and Stu's never to read: the response carries one
        line in its place, and the file joins the prompt only when the dispatch is admitted."""
        self.commit_files({"CLAUDE.md":"STU-NEVER-READS-THIS house rule.\n"})
        ready=self.call("prepare",self.args)
        self.assertNotIn("STU-NEVER-READS-THIS",json.dumps(ready))
        self.assertNotIn(REPOSITORY_INSTRUCTIONS,json.dumps(ready))
        self.assertNotIn("STU-NEVER-READS-THIS",self.worker_brief(ready))
        # The same answer again (a retried request) is no different.
        self.assertNotIn("STU-NEVER-READS-THIS",json.dumps(self.call("prepare",self.args)))
        prompt=self.delivered(ready)
        self.assertIn("> STU-NEVER-READS-THIS house rule.",prompt)
        self.assertNotIn("(The repository's own instruction file is put here",prompt)

    def test_a_repository_whose_only_file_is_agents_md_gives_its_agents_md(self):
        commit=self.commit_files({"AGENTS.md":"AGENTS-HOUSE-RULE\n"})
        section=self.instruction_section(self.delivered(self.call("prepare",self.args)))
        self.assertIn("`%s`, as committed at %s." % (self.repo/"AGENTS.md",commit[:12]),section)
        self.assertIn("> AGENTS-HOUSE-RULE",section)

    def test_beside_a_claude_md_the_agents_md_is_left_out_as_claude_code_leaves_it(self):
        self.commit_files({"CLAUDE.md":"ROOT-CLAUDE-RULE\n",".claude/CLAUDE.md":"DOT-CLAUDE-RULE\n","AGENTS.md":"AGENTS-HOUSE-RULE\n"})
        section=self.instruction_section(self.delivered(self.call("prepare",self.args)))
        self.assertLess(section.index("> ROOT-CLAUDE-RULE"),section.index("> DOT-CLAUDE-RULE"))
        self.assertNotIn("AGENTS-HOUSE-RULE",section)

    def test_the_connected_folders_uncommitted_claude_local_md_is_given_and_counts_against_agents_md(self):
        """CLAUDE.local.md is gitignored, so it is not in the worker's workspace: it is read from
        the connected folder itself. Claude Code reads AGENTS.md only when there is no CLAUDE.md,
        .claude/CLAUDE.md or CLAUDE.local.md, so the local file alone leaves AGENTS.md out."""
        self.commit_files({"AGENTS.md":"AGENTS-HOUSE-RULE\n"},ignore=("CLAUDE.local.md",))
        (self.repo/"CLAUDE.local.md").write_text("MY-OWN-LOCAL-RULE\n")
        section=self.instruction_section(self.delivered(self.call("prepare",self.args)))
        self.assertIn("`%s`, from the connected folder itself" % (self.repo/"CLAUDE.local.md"),section)
        self.assertIn("> MY-OWN-LOCAL-RULE",section)
        self.assertNotIn("AGENTS-HOUSE-RULE",section)

    def test_no_instruction_file_leaves_the_brief_as_it_was(self):
        ready=self.call("prepare",self.args)
        brief=self.worker_brief(ready)
        self.assertNotIn(REPOSITORY_INSTRUCTIONS,brief)
        self.assertEqual(brief,(ENGINE/"mega-lander/duties/worker.md").read_text().strip()+"\n\n"+self.args["brief"])
        prepared=ready["agent_payload"]["prompt"]
        self.assertEqual(self.delivered(ready),prepared,"a dispatch with no instruction file changed the prompt")

    def test_the_reviewer_gets_the_same_file_and_a_land_changes_no_instruction_file(self):
        """§5.3 item 2 (the reviewer reads the same file by the same rule) and §3 item 1 (the app
        never writes, moves or deletes a folder's instruction files): across a whole worker,
        review and land, every instruction file keeps its bytes and none is added."""
        files={"CLAUDE.md":"Every commit message ends with [house-rule].\n",".claude/CLAUDE.md":"DOT-CLAUDE-RULE\n",
               "AGENTS.md":"AGENTS-HOUSE-RULE\n","docs/AGENTS.md":"A SUBFOLDER'S OWN FILE\n"}
        self.commit_files(files,ignore=("CLAUDE.local.md",))
        (self.repo/"CLAUDE.local.md").write_text("MY-OWN-LOCAL-RULE\n")
        def instruction_files():
            return {str(p.relative_to(self.repo)):p.read_bytes() for p in self.repo.rglob("*")
                    if p.is_file() and ".git" not in p.relative_to(self.repo).parts
                    and p.name in INSTRUCTION_NAMES}
        before=instruction_files()
        self.assertEqual(len(before),5)
        pair=self.reviewed_pair("house-rules")
        worker,reviewer=(self.delivered_prompts["house-rules-"+role] for role in ("worker","reviewer"))
        # The same files and text; each names the commit its agent starts at, the reviewer's the
        # exact commit it reviews.
        reviewed=self.call("inspect")["records"]
        reviewed=next(r for r in reviewed if r["id"]==pair["reviewer_id"])["review_target"]["commit"]
        same=lambda text:re.sub(r"as committed at [0-9a-f]{12}","as committed at <commit>",text)
        section=self.instruction_section(reviewer,after="lands-pending: ")
        self.assertEqual(same(self.instruction_section(worker)),same(section))
        self.assertIn("as committed at %s." % reviewed[:12],section)
        self.call("integrate",pair)
        self.assertEqual(self.app.git(self.repo,"show","HEAD:result.txt"),"FICTIONAL house-rules","the land did not happen")
        self.assertEqual(instruction_files(),before)

    def review_row(self,rid,trigger,verdict,branch,tip,title):
        """One row of the app's second-review ledger, with the verdict file second-review keeps."""
        paths=self.review.app_paths(self.root/"engine-state")
        record=Path(paths["reviews"])/"reviews"/rid;record.mkdir(parents=True)
        (record/"verdict.json").write_text(json.dumps({"answer":{"findings":[{"priority":1,"title":title,
            "files":["result.txt:1"],"evidence":"the fixture says so","fixture":""}],"summary":"x"}}))
        with open(paths["ledger"],"a") as f:
            f.write(json.dumps({"id":rid,"trigger":trigger,"verdict":verdict,"repo":str(self.repo),"branch":branch,
                                "tip":tip,"record":str(record)})+"\n")

    def test_a_mid_job_request_for_changes_reaches_the_running_worker_once(self):
        """Second review, slice 4 (plan §2.5; Sage's check, catch 7): a mid-job review's
        changes-requested verdict is told to the running worker at its next tool call, once, by
        the app's own hook (scripts/app-engine-hook.py); a pass, a handover verdict, another
        branch's verdict and another agent's tool call are told nothing."""
        self.review=self.app.load("app_review_test",ENGINE/"scripts/lib/app_review.py")
        hook=self.app.load("app_engine_hook_test",ENGINE/"scripts/app-engine-hook.py")
        worker=self.call("prepare",self.args)
        self.start_fixture_worker(worker,"midjob-worker")
        call={"session_id":self.session,"agent_id":"midjob-worker","tool_name":"Bash","tool_input":{"command":"true"}}
        (repo,branch),=self.app.worker_spaces(self.scope,call)
        self.assertEqual(Path(repo).resolve(),self.repo)
        state=self.root/"engine-state"
        def told(payload):
            got=[]
            self.assertEqual(hook.mid_job_notice(self.app,self.scope,payload,state,got.append),bool(got))
            return "".join(got)
        self.assertEqual(told(call),"")      # no ledger yet
        Path(self.review.app_paths(state)["reviews"]).mkdir(parents=True)
        self.review_row("rv-mid-1","long-job","changes-requested",branch,"a"*40,"Result is not committed")
        self.review_row("rv-mid-pass","long-job","passed",branch,"b"*40,"Passing-row finding")
        self.review_row("rv-handover","handover","changes-requested",branch,"c"*40,"Handover finding")
        self.review_row("rv-other","quiet","changes-requested","cc/someone-else","d"*40,"Another worker's")
        text=told(call)
        self.assertIn("asked for changes",text)
        self.assertIn("P1 Result is not committed (result.txt:1)",text)
        for absent in ("Passing-row finding","Handover finding","Another worker's"):self.assertNotIn(absent,text)
        self.assertEqual(told(call),"","told twice")
        self.assertEqual(told({**call,"agent_id":"someone"}),"")
        self.assertEqual(told({**call,"agent_id":None}),"")

    def test_the_apps_review_watch_starts_mid_job_reviews_only_with_the_users_words(self):
        """Second review, slice 4: the app's own review-watch (scripts/lib/review_watch.py
        --app-state, the host's child) reads every conversation's partition of the registry and
        starts a long-job review of a running worker after 60 minutes: Claude, the CLI the app
        ships, and the user's own turn as the original words. A worker that has ended gets none
        from it: the app's handover reviewer reviews that. Stopping the watcher stops the reviews
        it started and does not count them as lost."""
        self.attest(self.USER_TURN)
        rw=self.app.load("review_watch_app_test",ENGINE/"scripts/lib/review_watch.py")
        worker=self.call("prepare",self.args)
        target=self.start_fixture_worker(worker,"watched-worker")
        (target/"result.txt").write_text("FICTIONAL")
        self.app.git(target,"add","result.txt")
        self.app.git(target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Fictional result")
        # The fake records each call in "review calls being written" and moves it whole into
        # "review calls" (one rename) only once args and partition are written: the watcher
        # starts it and returns at once, so a call read before then is half written (nightly
        # 20261010T014856Z-dcdee3e7: FileNotFoundError on args).
        calls=self.root/"review calls";calls.mkdir()
        writing=self.root/"review calls being written";writing.mkdir()
        fake=self.root/"fake second-review.sh"
        fake.write_text('#!/bin/bash\nd="$(mktemp -d "%s/call.XXXXXX")"\nprintf "%%s\\n" "$@" > "$d/args"\n'
                        'printf "%%s" "$RICHOS_WORKSPACES_DIR" > "$d/partition"\nmv "$d" "%s/"\nexec sleep 30\n'
                        % (writing,calls))
        state=self.root/"engine-state";claude=self.root/"claude";claude.write_text("#!/bin/sh\n");claude.chmod(0o755)
        os.environ.update({"REVIEW_WATCH_SECOND_REVIEW":str(fake)})
        accounts=self.root/"claude-accounts.json"
        a=type("A",(),{"app_state":str(state),"claude":str(claude),"accounts":str(accounts),"codex_reviews":"","status":False,"tick":True})
        import time as _t
        def look(minutes):
            """Every call started so far, written in full or not, by name: the one being
            written is read first, so a call moved in between is seen once, never missed."""
            os.environ["REVIEW_WATCH_NOW"]=str(_t.time()+minutes*60)
            rw.app_mode(a,str(ENGINE))
            names={p.name for p in writing.iterdir()}
            return sorted(names|{p.name for p in calls.iterdir()})
        def written(name):
            """The call `name` once the fake has written it in full (bounded at 30 s)."""
            deadline=_t.monotonic()+30
            while not (calls/name).is_dir() and _t.monotonic()<deadline:_t.sleep(0.05)
            self.assertTrue((calls/name).is_dir(),"the fake never finished writing call %s"%name)
            return calls/name
        self.assertEqual(look(5),[],"a review started five minutes in")
        started=look(61)
        self.assertEqual(len(started),1)
        started=[written(started[0])]
        args=(started[0]/"args").read_text().split("\n")
        self.assertEqual(args[args.index("--trigger")+1],"long-job")
        self.assertEqual(args[args.index("--reviewer")+1],"claude")
        self.assertEqual(args[args.index("--claude")+1],str(claude))
        # The app's account list, which second-review reads when the reviewer starts (the real
        # second review of 802194f0e, finding 1; second-review.test.sh C15 runs that through).
        self.assertEqual(args[args.index("--accounts")+1],str(accounts))
        self.assertEqual(Path(args[args.index("--words-file")+1]).read_text(),self.USER_TURN)
        self.assertEqual((started[0]/"partition").read_text(),os.environ["RICHOS_WORKSPACES_DIR"])
        self.assertEqual(len(look(62)),1,"one tip reviewed twice at once")
        # The host's quit: the watcher stops what it started, recorded as stopped, never lost.
        lock,=Path(rw.app_paths(state)["watch"]).joinpath("locks").glob("*.lock")
        pid=json.loads(lock.read_text())["pid"]
        watcher=rw.Watcher(str(ENGINE),"",rw.AppWorld(str(ENGINE),str(state),str(claude)))
        watcher.children[pid]=type("Child",(),{"poll":lambda self:None if _alive(pid) else 0})()
        watcher.stop_own(_t.time())
        self.assertFalse(_alive(pid),"the review outlived the watcher's quit")
        attempts=[json.loads(l) for l in (Path(rw.app_paths(state)["watch"])/"attempts.jsonl").read_text().splitlines()]
        self.assertEqual([x["outcome"] for x in attempts],["stopped"])
        # A worker that has ended is the app's handover reviewer's: nothing more starts here.
        self.finish_fixture_worker("watched-worker")
        self.assertEqual(len(look(200)),1)

    def test_review_exact_commit_integration_recovery_and_dirty_checkout_preservation(self):
        worker=self.call("prepare",self.args)
        target=self.start_fixture_worker(worker,"fixture-worker")
        (target/"result.txt").write_text("FICTIONAL")
        self.app.git(target,"add","result.txt")
        self.app.git(target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Fictional result")
        commit=self.app.git(target,"rev-parse","HEAD")
        self.finish_fixture_worker("fixture-worker")
        reviewer=self.call("prepare",{**self.args,"request_id":"review-one","role":"reviewer","teammate":"frank","review_of":worker["id"],"title":"Review fictional result","brief":"Review the exact result.txt change; do not modify any files."})
        # Slice 2: Frank reviews as himself, on his own model, carrying the reviewer duty.
        self.assertTrue(reviewer["name"].startswith("frank-opus-"))
        self.assertEqual(reviewer["agent_payload"]["subagent_type"],"richos-app-engine:frank")
        self.assertIn(DUTY_TOP["reviewer"],reviewer["agent_payload"]["prompt"])
        review_target=self.start_fixture_worker(reviewer,"fixture-reviewer")
        self.assertEqual(self.app.git(review_target,"rev-parse","HEAD"),commit)
        self.finish_fixture_worker("fixture-reviewer","RICHOS_REVIEW "+json.dumps({"commit":commit,"verdict":"passed","checks":["synthetic reviewer observation"]}))
        args={"worker_id":worker["id"],"reviewer_id":reviewer["id"]}
        (self.repo/"unrelated.txt").write_text("KEEP")
        with self.assertRaisesRegex(ValueError,"dirty"):self.call("integrate",args)
        self.assertEqual((self.repo/"unrelated.txt").read_text(),"KEEP")
        self.assertFalse((self.repo/"result.txt").exists())
        (self.repo/"unrelated.txt").unlink()
        original=self.app.land_merge
        def crash_after_merge(*argv):
            original(*argv)
            raise RuntimeError("synthetic crash after the merge")
        with patch.object(self.app,"land_merge",side_effect=crash_after_merge):
            with self.assertRaisesRegex(RuntimeError,"synthetic crash"):self.call("integrate",args)
        self.assertEqual((self.repo/"result.txt").read_text(),"FICTIONAL")
        with patch.object(self.app.W,"remove_workspace",return_value=(False,"synthetic busy workspace")):
            pending=self.call("integrate",args)
        self.assertTrue(pending["work_integrated"])
        self.assertTrue(pending["cleanup_pending"])
        self.assertTrue(target.exists())
        with self.assertRaisesRegex(ValueError,"cleanup"):
            self.call("complete",{"obligation_id":"fixture-task","worker_ids":[worker["id"]]})
        result=self.call("integrate",args)
        self.assertTrue(result["work_integrated"]);self.assertFalse(result["obligation_closed"])
        self.assertEqual(result["cleanup_pending"],[])
        self.assertFalse(target.exists());self.assertFalse(review_target.exists())
        # THE SAME LAND, REPEATED, IS THE SAME ANSWER — except for this call's
        # own observation of the lock, which is not a fact about the work: it is
        # how long THIS call waited for the repository and where it waited. That
        # was compared too until 2026-09-17 and made this assertion flaky within
        # hours of the lock landing (`land_lock.landed.at` is a whole-second ISO
        # stamp, so two calls either side of a second boundary differed; measured
        # red on pristine f5157115). The land itself is now recorded in the
        # append-only history beside the lock, which the repeat does not touch.
        again=self.call("integrate",args)
        self.assertEqual({k:v for k,v in again.items() if k!="land_lock"},
                         {k:v for k,v in result.items() if k!="land_lock"})
        self.assertEqual(again["land_lock"]["repository"],result["land_lock"]["repository"])
        self.assertLess(again["land_lock"]["waited_seconds"],1.0)
        records=self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"inspect","binding":self.scope["binding"],"query":{"section":"work"}})["records"]
        # The persisted authority event is the proof even if brief projection omits terminal work.
        store=self.app.ECS.EventStore(self.root/"ecs")
        with contextlib.closing(store.connect()) as conn:
            row=conn.execute("SELECT status FROM ecs_work_units WHERE external_id=?",(worker["id"],)).fetchone()
            self.assertEqual(row["status"],"completed")
            review_row=conn.execute("SELECT status FROM ecs_work_units WHERE external_id=?",(reviewer["id"],)).fetchone()
            self.assertEqual(review_row["status"],"completed")
        completion={"obligation_id":"fixture-task","worker_ids":[worker["id"]]}
        with self.assertRaisesRegex(ValueError,"every final worker"):
            self.call("complete",{**completion,"worker_ids":[reviewer["id"]]})
        with self.assertRaisesRegex(ValueError,"every final worker"):
            self.call("complete",{**completion,"worker_ids":[worker["id"],reviewer["id"]]})
        original_execute=self.app.ECS.execute
        def crash_after_close(root,request):
            answer=original_execute(root,request)
            if request["command"]=="complete-obligation":
                raise RuntimeError("synthetic crash after completion commit")
            return answer
        with patch.object(self.app.ECS,"execute",side_effect=crash_after_close):
            with self.assertRaisesRegex(RuntimeError,"synthetic crash"):
                self.call("complete",completion)
        # A new visible turn retries the original intent after ECS committed.
        old_binding=self.scope["binding"]
        binding=original_execute(self.root/"ecs",{"protocol":1,"command":"bind",
            "scope":{**{k:v for k,v in old_binding.items() if k!="revision"},"turn_id":"completion-retry"},
            "expected_revision":old_binding["revision"],"source_ref":"ledger:thread-a:completion-retry","request_id":"completion-retry"})["binding"]
        self.scope["binding"]=binding;self.scope_path.write_text(json.dumps(self.scope))
        completed=self.call("complete",completion)
        self.assertTrue(completed["obligation_closed"])
        self.assertFalse(completed["published"])
        self.assertEqual(self.call("complete",completion),completed)
        item=original_execute(self.root/"ecs",{"protocol":1,"command":"inspect","binding":binding,"query":{"item_id":"fixture-task"}})["item"]
        self.assertEqual(item["status"],"completed")
        with contextlib.closing(store.connect()) as conn:
            self.assertEqual(conn.execute("SELECT count(*) FROM ecs_events WHERE event_type='continuity.item_closed'").fetchone()[0],1)
        self.scope["actions_allowed"]=False;self.scope_path.write_text(json.dumps(self.scope))
        with self.assertRaisesRegex(ValueError,"visible app turn"):self.call("complete",completion)

    def test_rejected_review_then_continuation_cleans_the_complete_review_chain(self):
        worker=self.call("prepare",self.args)
        target=self.start_fixture_worker(worker,"wrong-worker")
        (target/"result.txt").write_text("WRONG")
        self.app.git(target,"add","result.txt")
        self.app.git(target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Incorrect fictional result")
        wrong=self.app.git(target,"rev-parse","HEAD");self.finish_fixture_worker("wrong-worker")
        reviewer=self.call("prepare",{**self.args,"request_id":"reject-review","role":"reviewer","review_of":worker["id"],"title":"Review result","brief":"Verify the fictional requirement."})
        rejected_target=self.start_fixture_worker(reviewer,"reject-reviewer")
        self.finish_fixture_worker("reject-reviewer","RICHOS_REVIEW "+json.dumps({"commit":wrong,"verdict":"changes-requested","checks":["result is WRONG, expected FICTIONAL"]}))
        with self.assertRaisesRegex(ValueError,"passing review"):
            self.call("integrate",{"worker_id":worker["id"],"reviewer_id":reviewer["id"]})
        self.assertFalse((self.repo/"result.txt").exists())
        revised=self.call("prepare",{**self.args,"request_id":"revise-result","continue_of":worker["id"]})
        revised_target=self.start_fixture_worker(revised,"correct-worker")
        (revised_target/"result.txt").write_text("FICTIONAL")
        self.app.git(revised_target,"add","result.txt")
        self.app.git(revised_target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Correct fictional result")
        correct=self.app.git(revised_target,"rev-parse","HEAD");self.finish_fixture_worker("correct-worker")
        final_review=self.call("prepare",{**self.args,"request_id":"final-review","role":"reviewer","review_of":revised["id"],"title":"Review corrected result","brief":"Verify the corrected requirement."})
        final_target=self.start_fixture_worker(final_review,"accept-reviewer")
        self.finish_fixture_worker("accept-reviewer","RICHOS_REVIEW "+json.dumps({"commit":correct,"verdict":"passed","checks":["result is FICTIONAL"]}))
        result=self.call("integrate",{"worker_id":revised["id"],"reviewer_id":final_review["id"]})
        self.assertEqual(result["cleanup_pending"],[])
        for path in (target,rejected_target,revised_target,final_target):self.assertFalse(path.exists(),str(path))
        self.assertEqual((self.repo/"result.txt").read_text(),"FICTIONAL")
        self.assertTrue(self.call("complete",{"obligation_id":"fixture-task","worker_ids":[revised["id"]]})["obligation_closed"])

    def test_completion_refuses_prepared_and_unreviewed_work(self):
        worker=self.call("prepare",self.args)
        args={"obligation_id":"fixture-task","worker_ids":[worker["id"]]}
        with self.assertRaisesRegex(ValueError,"unresolved execution"):self.call("complete",args)
        self.start_fixture_worker(worker,"unfinished")
        self.finish_fixture_worker("unfinished")
        with self.assertRaisesRegex(ValueError,"cleanup"):self.call("complete",args)
        item=self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"inspect","binding":self.scope["binding"],"query":{"item_id":"fixture-task"}})["item"]
        self.assertNotEqual(item["status"],"completed")

    def test_another_thread_cannot_read_or_dispatch_through_this_provider(self):
        ready=self.call("prepare",self.args)
        other={**self.scope,"binding":{**self.scope["binding"],"thread_id":"thread-b"}}
        self.scope_path.write_text(json.dumps(other))
        with self.assertRaisesRegex(ValueError,"partition"):self.call("inspect")

    def test_a_prepared_worker_is_dispatched_in_the_background_because_nothing_else_works(self):
        """The invariant candidate .10 exposed, and the one this was changed to and back.

        `run_in_background: True` carried no comment and no test of its own, so it read like
        an accident. It is not: `guard-worktree-isolation.sh` clause 7b refuses a file-capable
        spawn with `False`, and `workspaces.py`'s `bind_agent` — the only thing that joins the
        platform's agent id to this receipt — runs at `PostToolUse[Agent]`, which a synchronous
        call does not deliver until the worker has already finished. Measured with `False` on
        a real provider: `SubagentStart` arrived and then every one of the worker's own tool
        calls was refused with *"worker identity has not joined its app receipt"*.

        Waiting for the helper is therefore the host's (`work_host.rs`'s `run_one` step 3b),
        and this assertion exists so the next reader changes the host rather than this line.
        """
        ready=self.call("prepare",self.args)
        self.assertEqual(ready["agent_payload"]["run_in_background"],True)

    def test_uncertain_preparation_is_retained_and_not_repeated(self):
        with patch.object(self.app,"run",side_effect=TimeoutError("synthetic uncertain command")):
            with self.assertRaises(TimeoutError):self.call("prepare",self.args)
        result=self.call("prepare",self.args)
        self.assertEqual(result["status"],"unknown");self.assertNotIn("agent_payload",result)
    def test_ecs_outbox_reconciles_after_commit_before_local_receipt_and_new_turn(self):
        original=self.app.ECS.execute
        def interrupted(root,request):
            result=original(root,request)
            if request["command"]=="observe":raise RuntimeError("synthetic crash after ECS commit")
            return result
        with patch.object(self.app.ECS,"execute",side_effect=interrupted):
            with self.assertRaisesRegex(RuntimeError,"synthetic crash"):self.call("prepare",self.args)
        binding=original(self.root/"ecs",{"protocol":1,"command":"bind","scope":{**{k:v for k,v in self.scope["binding"].items() if k!="revision"},"turn_id":"turn-b"},"expected_revision":self.scope["binding"]["revision"],"source_ref":"ledger:thread-a:turn-b","request_id":"bind-second"})["binding"]
        self.scope["binding"]=binding;self.scope_path.write_text(json.dumps(self.scope))
        result=self.call("inspect")["records"][0]
        self.assertEqual(result["status"],"unknown");self.assertNotIn("ecs_pending",result)
        work=original(self.root/"ecs",{"protocol":1,"command":"inspect","binding":binding,"query":{"section":"work"}})
        self.assertEqual(len(work["records"]),1)
    # ---------------------------------------------------------------- the work seat
    def bind_seat(self,seat,turn,audience="worker",session=None,revision=None):
        return self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"bind","seat":seat,
            "scope":{"entity_id":"depot","thread_id":"thread-a","session_id":session or ("session-"+turn),
                     "turn_id":turn,"audience":audience},
            "request_id":"bind-"+(seat or "ceo")+"-"+turn,"source_ref":"ledger:thread-a:"+turn,
            "expected_revision":revision})["binding"]

    def three_ceo_turns(self):
        """His next three sentences. Each one rewrites HIS cursor and bumps it."""
        for number in (2,3,4):
            self.scope["binding"]=self.bind_seat(None,"turn-%d"%number,audience="ceo",
                                                 session=self.session,revision=number-1)
            self.scope_path.write_text(json.dumps(self.scope))
        self.assertEqual(self.scope["binding"]["revision"],4)

    def work_lease(self,assignment="fixture-task",seat=None):
        """A background lease's own scope file: its own seat, one per assignment.

        Its turn_id IS the assignment, because a request cannot outlive a turn id
        and a background assignment has to (5.3). Its grant stands while no turn
        is open, which is the whole reason the work tools are reachable at all.
        """
        seat = seat or ("work-seat:"+assignment)
        binding = self.bind_seat(seat,assignment)
        path = self.root/("work-scope-"+assignment+".json")
        path.write_text(json.dumps({**self.scope,"binding":binding,"seat":seat}))
        return path,seat

    def test_reused_work_lease_cannot_inspect_or_land_another_assignment(self):
        ready=self.call("prepare",self.args)
        current,_=self.work_lease()
        self.assertEqual(self.app.call(current,"inspect",{})["records"][0]["id"],ready["id"])
        other,_=self.work_lease("another-assignment")
        self.assertEqual(self.app.call(other,"inspect",{}),{"records":[],"next_offset":None})
        with patch.object(self.app,"land_lock") as lock:
            with self.assertRaisesRegex(ValueError,"another assignment"):
                self.app.call(other,"integrate",{"worker_id":ready["id"],"reviewer_id":"unread"})
            lock.assert_not_called()
        # The conversation still owns the combined history.
        self.assertEqual(len(self.call("inspect")["records"]),1)

    def test_a_background_assignment_still_reports_after_three_ceo_turns(self):
        """Acceptance 7.2, and the half that fails is the REPORT, not the read.

        A running process that can no longer speak looks identical to a healthy one
        from the outside, which is exactly why this gate went unnoticed twice. So
        this walks it with the tool that WRITES.
        """
        ready=self.call("prepare",self.args)
        self.app.dispatch_intent(self.scope,{"session_id":self.session,
            "tool_use_id":"actual-native-id","tool_input":ready["agent_payload"]})
        work,seat = self.work_lease()
        self.three_ceo_turns()
        store=self.store()
        # THE TRAP THIS TEST EXISTS TO AVOID: with the two rows left sitting at the
        # same revision the broken form passes. His row is at 4 and the assignment's
        # is at 1, and only then is the report below a real question.
        self.assertEqual((dict(store.current_context())["revision"],
                          dict(store.current_context(seat))["revision"]),(4,1))
        before=len(self.events(store,"work_unit.upserted"))
        # The assignment speaks. This appends work_unit.upserted, which is fenced
        # against the row belonging to the event's own person.
        self.assertEqual(self.app.call(work,"inspect",{})["records"][0]["status"],"dispatching")
        wrote=self.events(store,"work_unit.upserted")
        self.assertEqual(len(wrote),before+1)
        self.assertEqual(wrote[-1]["person_id"],seat)
        # And his own conversation still sees the background record, because the
        # entity and the thread stay single and shared.
        units=self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"inspect",
            "binding":self.scope["binding"],"query":{"section":"work"}})["records"]
        self.assertEqual([row["status"] for row in units],["assigned"])

    def store(self):
        sys.path.insert(0,str(ENGINE/"ecs/core"))
        from ecs_core import EventStore
        return EventStore(self.root/"ecs")

    def events(self,store,event_type):
        conn=store.connect()
        try:
            return [dict(row) for row in conn.execute(
                "SELECT person_id FROM ecs_events WHERE event_type=? ORDER BY sequence",
                (event_type,)).fetchall()]
        finally:
            conn.close()

    def test_every_request_from_a_work_lease_carries_its_seat(self):
        self.call("prepare",self.args)
        work,seat = self.work_lease()
        seen=[]
        real=self.app.ECS.execute
        def watch(root,request):
            seen.append(request.get("seat"));return real(root,request)
        with patch.object(self.app.ECS,"execute",watch):
            self.app.call(work,"inspect",{})
            self.assertTrue(seen and all(value==seat for value in seen),seen)
            # The control that makes the assertion mean something: the same tool on
            # the conversation's scope names no seat at all, so a seat can never be
            # something this adapter supplies by default.
            seen.clear();self.call("inspect")
            self.assertTrue(seen and all(value is None for value in seen),seen)

    def test_a_work_lease_prepares_against_the_obligation_its_scope_carries_and_never_one_it_names(self):
        """The fourth way a background job stalled, 2026-09-18.

        The whole flow ran for real for the first time on 2026-09-18 and got as
        far as this call and no further. The work lease's brief names the
        repositories and the title and no identifier at all, deliberately; this
        tool required an ``obligation_id``; so the model supplied the only thing
        it could, a guess. It invented ``obl-add-notes-line`` where the real
        obligation was ``qa-notes-line`` and was refused in 8 ms with "item is
        absent or outside the active scope". Nothing was prepared, no worker ran,
        and the fixture repository never gained its commit.

        Arm B is the one that matters, and it is deliberately not "a wrong id is
        refused": a wrong id must be IGNORED, because the scope already holds the
        right one. It reuses arm A's ``request_id``, so if the invented id had
        reached ``normalized`` at all the call would refuse with "already used
        for different work" instead of returning arm A's own record.

        Arms C and D are the controls. Without C the derivation could have been a
        blanket "obligation_id is optional now"; without D it could have been
        "any work scope prepares anything".
        """
        work, _ = self.work_lease()
        # A. The lease names nothing. The scope's own binding answers.
        first = self.app.call(work, "prepare", {key: value for key, value in self.args.items()
                                                if key != "obligation_id"})
        self.assertEqual(first["status"], "prepared")
        self.assertEqual(self.call("inspect")["records"][0]["request"]["obligation_id"], "fixture-task")

        # B. The lease names the WRONG one, which is exactly what happened. The
        #    scope wins and the invented id is nowhere in the receipt.
        again = self.app.call(work, "prepare", {**self.args, "obligation_id": "obl-add-notes-line"})
        self.assertEqual(again["id"], first["id"])
        self.assertEqual(again["request"]["obligation_id"], "fixture-task")
        self.assertNotIn("obl-add-notes-line", json.dumps(self.call("inspect")["records"]))

        # C. CONTROL -- a conversation scope carries no obligation in its binding
        #    (its turn_id is a TURN), so there the argument is still required and
        #    a wrong one is still refused by the ECS read.
        with self.assertRaisesRegex(ValueError, "obligation_id must be a nonempty bounded string"):
            self.call("prepare", {key: value for key, value in self.args.items()
                                  if key != "obligation_id"})
        with self.assertRaises(Exception):
            self.call("prepare", {**self.args, "request_id": "ceo-wrong",
                                  "obligation_id": "obl-add-notes-line"})

        # D. CONTROL -- deriving is not the same as trusting. A work scope bound
        #    to an obligation that does not exist is refused exactly as before,
        #    and nothing is prepared for it.
        absent, _ = self.work_lease(assignment="no-such-assignment")
        with self.assertRaises(Exception):
            self.app.call(absent, "prepare", {**self.args, "request_id": "absent-one"})
        self.assertEqual(len(self.call("inspect")["records"]), 1)

    def test_a_work_lease_completes_the_obligation_its_scope_carries(self):
        """The same derivation on the call that CLOSES the assignment.

        Fixing only ``prepare`` would have moved the defect one step later and
        made it harder to see: a job that did every piece of its work and then
        named the wrong obligation here leaves the obligation open, and the app's
        settle reading reports a failure over work that actually landed.

        The refusal asserted below is the completion gate doing its own job
        ("unresolved execution"), reached only because the obligation resolved --
        an invented id would have been refused earlier and for a different reason,
        which is what the second arm pins.
        """
        worker = self.call("prepare", self.args)
        work, _ = self.work_lease()
        with self.assertRaisesRegex(ValueError, "unresolved execution"):
            self.app.call(work, "complete", {"worker_ids": [worker["id"]]})
        # The same call naming a guess reaches the same gate: the scope wins and
        # the guess is never read.
        with self.assertRaisesRegex(ValueError, "unresolved execution"):
            self.app.call(work, "complete", {"obligation_id": "obl-add-notes-line",
                                             "worker_ids": [worker["id"]]})
        # CONTROL: on the conversation's scope the argument is still required.
        with self.assertRaisesRegex(ValueError, "obligation_id must be a nonempty bounded string"):
            self.call("complete", {"worker_ids": [worker["id"]]})

    def test_orphan_seats_are_reconciled_and_an_open_assignment_keeps_its_own(self):
        """6.3a. Settled, absent or unknown goes; open stays; his is never touched."""
        self.call("prepare",self.args)
        self.work_lease()                                   # an OPEN assignment
        self.work_lease(assignment="no-such-assignment")    # an ABSENT one
        self.three_ceo_turns()
        report=self.call("inspect")["seats"]
        self.assertEqual([row["assignment"] for row in report["retained"]],["fixture-task"])
        self.assertEqual([(row["assignment"],row["reason"]) for row in report["released"]],
                         [("no-such-assignment","absent")])
        self.assertEqual(report["unreconciled"],[])
        # His own cursor is still there, and it was never a candidate: a work seat is
        # identified by its own audience, never as "everything that is not his".
        seats=self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"seats",
            "binding":self.scope["binding"]})["seats"]
        self.assertEqual(sorted(row["person_id"] for row in seats),
                         ["ceo-default","work-seat:fixture-task"])
        # Now settle the assignment the way a completion does, and its seat goes too.
        store=self.store()
        item=self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"inspect",
            "binding":self.scope["binding"],"query":{"item_id":"fixture-task"}})["item"]
        store.append("continuity.item_closed",entity_id="depot",thread_id="thread-a",
            session_id=self.session,active_context_revision=self.scope["binding"]["revision"],
            actor_kind="authority_adapter",actor_id="richos-provider-v1",
            source_ref="app-completion:fixture",idempotency_key="fixture-close",
            expected_revision=int(item["revision"]),
            payload={"item_id":"fixture-task","status":"completed","evidence_ref":"app-completion:fixture"})
        report=self.call("inspect")["seats"]
        self.assertEqual([(row["assignment"],row["reason"]) for row in report["released"]],
                         [("fixture-task","settled")])
        self.assertEqual(report["retained"],[])

    def ceo_thread_seat(self,thread,turn="turn-1",revision=None):
        """His front desk for ANOTHER conversation thread, bound the way that
        thread's own front desk binds it: the derived seat, its own thread, the
        ceo audience."""
        seat="ceo-thread:"+thread
        self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"bind","seat":seat,
            "scope":{"entity_id":"depot","thread_id":thread,"session_id":"session-"+thread,
                     "turn_id":turn,"audience":"ceo"},
            "request_id":f"bind-{seat}-{turn}","source_ref":f"ledger:{thread}:{turn}",
            "expected_revision":revision})
        return seat

    def ledger(self,*threads):
        """The app's own record of which conversation threads exist, at the path
        the app actually writes it to: `$RICHOS_APP_STATE/conversation-ledger.jsonl`
        (src-tauri/src/main.rs:1461, whose data_dir is the engine's state root)."""
        path=Path(os.environ["RICHOS_APP_STATE"])/"conversation-ledger.jsonl"
        path.parent.mkdir(parents=True,exist_ok=True)
        path.write_text("".join(json.dumps({"event":"ThreadCreated","thread_id":t,
            "title":"a conversation","at":1}) +"\n" for t in threads)
            # A turn record between them, so the reader is walking a real ledger
            # and not a list of thread rows.
            + json.dumps({"event":"TurnStarted","turn_id":"t-1","thread_id":threads[0] if threads else "",
                          "at":2})+"\n")
        return path

    def test_his_orphan_thread_seat_is_reconciled_and_a_live_threads_never_is(self):
        """HIS SEAT IS ONE PER CONVERSATION THREAD, and until 2026-09-17 nothing
        would ever have reconciled one: `reconcile_seats` enumerated every seat
        and then skipped everything whose audience was not `worker`, so an
        orphan thread seat -- a seat for a thread the app's own record has never
        heard of -- would have lived forever.

        The question asked of one of his is NOT `assignment_state`: it has no
        assignment. It is whether that conversation thread still exists, which
        only the app knows (ecs/CONTRACT.md), so it is read from the app's own
        conversation ledger. Every case below is paired with the one fact
        changed:

          a live thread's seat   -> KEPT, and the ledger is the only difference
          a dead thread's seat   -> RELEASED, with the revision it was
                                    enumerated at, which is the store's own
                                    independent liveness proof
          ceo-default            -> refused by name, and still there afterwards
          the ledger unreadable  -> reported as unreconciled, never released:
                                    absence is not evidence
        """
        mine=self.ceo_thread_seat("thread-a")          # THIS conversation's own seat
        live=self.ceo_thread_seat("thread-live")
        dead=self.ceo_thread_seat("thread-dead")
        def seats():
            return sorted(row["person_id"] for row in self.app.ECS.execute(self.root/"ecs",
                {"protocol":1,"command":"seats","binding":self.scope["binding"]})["seats"])
        self.assertEqual(seats(),sorted(["ceo-default",mine,live,dead]))
        # THE UNDECIDABLE CASE FIRST, and it is the control for every release
        # below: with no ledger to read, NOTHING is released. The seat of the
        # conversation this is running IN is not even a candidate -- it is kept
        # on a fact that needs no file.
        report=self.call("inspect")["seats"]
        self.assertEqual(report["released"],[])
        self.assertEqual(sorted(row["seat"] for row in report["unreconciled"]),sorted([live,dead]))
        self.assertIn("could not be read",report["unreconciled"][0]["reason"])
        self.assertIn((mine,"this conversation"),
                      [(row["seat"],row.get("reason")) for row in report["retained"]])
        self.assertEqual(seats(),sorted(["ceo-default",mine,live,dead]))
        # NOW THE APP'S RECORD EXISTS AND NAMES ONE OF THE TWO. That single fact
        # decides them in opposite directions.
        self.ledger("thread-a","thread-live")
        report=self.call("inspect")["seats"]
        self.assertEqual([(row["seat"],row["reason"]) for row in report["released"]],
                         [(dead,"thread absent")])
        self.assertIn((live,"open"),[(row["seat"],row.get("reason")) for row in report["retained"]])
        self.assertEqual(report["unreconciled"],[])
        # HIS LEGACY CURSOR IS UNTOUCHED, and so are the live thread's seat and
        # this conversation's own.
        self.assertEqual(seats(),sorted(["ceo-default",mine,live]))
        # AND IT IS REFUSED BY NAME, not merely left alone by a filter: asked
        # directly, for his legacy cursor, the store says no.
        with self.assertRaisesRegex(Exception,"never reconciled away"):
            self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"release-seat",
                "binding":self.scope["binding"],"person_id":"ceo-default","reason":"never",
                "request_id":"refuse-default","source_ref":"app-reconcile:refuse-default"})
        self.assertEqual(seats(),sorted(["ceo-default",mine,live]))

    def test_a_seat_that_cannot_be_released_is_reported_rather_than_ignored(self):
        self.work_lease(assignment="no-such-assignment")
        real=self.app.ecs
        def refuse(scope,request):
            if request["command"]=="release-seat": raise ValueError("the store is locked by another writer")
            return real(scope,request)
        with patch.object(self.app,"ecs",refuse):
            report=self.call("inspect")["seats"]
        self.assertEqual(report["released"],[])
        self.assertEqual([row["seat"] for row in report["unreconciled"]],
                         ["work-seat:no-such-assignment"])
        self.assertIn("locked by another writer",report["unreconciled"][0]["reason"])
        # The positive control: without the refusal the very same seat IS released,
        # so the report above is a real failure and not a permanently broken path.
        self.assertEqual([row["seat"] for row in self.call("inspect")["seats"]["released"]],
                         ["work-seat:no-such-assignment"])

    def test_a_work_lease_cannot_reconcile_seats_or_write_his_continuity(self):
        work,seat = self.work_lease()
        self.three_ceo_turns()
        self.assertNotIn("seats",self.app.call(work,"inspect",{}))
        binding=json.loads(work.read_text())["binding"]
        for command,fields in (("seats",{}),
                               ("release-seat",{"person_id":seat,"request_id":"r1",
                                                "source_ref":"reconcile:1"}),
                               ("brief",{}),
                               ("checkpoint",{"request_id":"c1","checkpoint":{"no_changes":True,
                                                                              "reason":"Test"}})):
            with self.assertRaisesRegex(Exception,"conversation's own seat"):
                self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":command,
                    "seat":seat,"binding":binding,**fields})
        # Control: every one of those four is fine on his own seat.
        for command,fields in (("seats",{}),("brief",{}),
                               ("checkpoint",{"request_id":"c1","checkpoint":{"no_changes":True,
                                                                              "reason":"Test"}})):
            self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":command,
                "binding":self.scope["binding"],**fields})

    # =======================================================================
    # THE LAND LOCK — ONE PER REPOSITORY, SHARED BY EVERY CONVERSATION
    # =======================================================================
    # BEFORE THIS EXISTED, reproduced by Frank from the engine's own identity
    # hash (docs/plans/two-riches-spec-2026-09-17-frank-check.md, item 3):
    #
    #   $ python3 -c 'import hashlib,json; f=lambda e,t: hashlib.sha256(
    #       json.dumps([e,t],separators=(",",":")).encode()).hexdigest();
    #       print(f("acme","thread-A")); print(f("acme","thread-B"))'
    #   b447822dc584e545fefdaf85c0415fcf229fc64b927a4b3aa2771726a8b5c344
    #   ad0ffd91066fe26a6aba8489bd6ef0ffe6233682f2316a473c9d940c4911924f
    #
    # Two threads, two lock files, both taken happily at the same moment, and
    # the `git merge --ff-only` under neither. These tests are the after-state.

    HOLDER_PROGRAM = (
        "import importlib.util,os,sys\n"
        "spec=importlib.util.spec_from_file_location('held_app',sys.argv[1])\n"
        "module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)\n"
        "scope={'binding':{'entity_id':os.environ.get('RICHOS_APP_ENTITY',''),"
        "'thread_id':sys.argv[3],'session_id':'holder-session'}}\n"
        "with module.land_lock(scope,sys.argv[2]):\n"
        "    sys.stdout.write('HELD\\n');sys.stdout.flush()\n"
        "    sys.stdin.readline()\n"
        "sys.stdout.write('RELEASED\\n');sys.stdout.flush()\n")

    PROBE_PROGRAM = (
        "import fcntl,os,sys\n"
        "fd=os.open(sys.argv[1],os.O_RDWR|os.O_CREAT,0o600)\n"
        "try: fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)\n"
        "except OSError: sys.exit(3)\n"
        "sys.exit(0)\n")

    def land_lock_free(self,path):
        """Can the lock be taken RIGHT NOW, asked from another process?

        Another process rather than this one on purpose: the whole question is
        whether a second conversation's back end would be held off, and a second
        conversation is a second process."""
        probe=subprocess.run([sys.executable,"-c",self.PROBE_PROGRAM,str(path)])
        self.assertIn(probe.returncode,(0,3),"the lock probe itself failed")
        return probe.returncode==0

    def start_land_lock_holder(self,repo,thread):
        """Hold the repository's land lock from a SECOND conversation — a second
        process, a second thread id, a second workspace partition — through the
        shipped `land_lock`, not a hand-rolled flock."""
        env={**os.environ,"RICHOS_APP_THREAD":thread,
             "RICHOS_WORKSPACES_DIR":str(self.root/"engine-state/workspaces"
                 /hashlib.sha256(json.dumps(["depot",thread],separators=(",",":")).encode()).hexdigest())}
        holder=subprocess.Popen([sys.executable,"-c",self.HOLDER_PROGRAM,str(ENGINE/"mega-lander/app.py"),str(repo),thread],
            stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,env=env)
        self.addCleanup(self.release_land_lock_holder,holder)
        self.assertEqual(holder.stdout.readline().strip(),"HELD","the second conversation never took the lock")
        return holder

    def release_land_lock_holder(self,holder):
        if holder.poll() is not None: return
        try:
            holder.stdin.write("go\n");holder.stdin.flush()
        except (BrokenPipeError,ValueError):
            holder.kill()
        holder.wait(timeout=30)

    def reviewed_pair(self,tag,repo=None,file="result.txt"):
        """A worker and a passing reviewer, both run-ended: the exact state
        `integrate` is called from."""
        spec={**self.args,"request_id":"prepare-"+tag}
        if repo: spec["repo"]=str(repo)
        worker=self.call("prepare",spec)
        target=self.start_fixture_worker(worker,tag+"-worker")
        (target/file).write_text("FICTIONAL "+tag)
        self.app.git(target,"add",file)
        self.app.git(target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Fictional "+tag)
        commit=self.app.git(target,"rev-parse","HEAD")
        self.finish_fixture_worker(tag+"-worker")
        reviewer=self.call("prepare",{**spec,"request_id":"review-"+tag,"role":"reviewer","review_of":worker["id"],
            "title":"Review "+tag,"brief":"Review the exact result.txt change; do not modify any files."})
        self.start_fixture_worker(reviewer,tag+"-reviewer")
        self.finish_fixture_worker(tag+"-reviewer","RICHOS_REVIEW "+json.dumps(
            {"commit":commit,"verdict":"passed","checks":["synthetic reviewer observation"]}))
        return {"worker_id":worker["id"],"reviewer_id":reviewer["id"]}

    def concurrent_reviewed_pairs(self,*jobs):
        """Two assignments whose workers RUN AT THE SAME TIME from the same tip,
        each then reviewed: the ordinary concurrent case. Every worker starts
        before any ends, because the engine refuses a new worker while finished
        work is pending (point 5) and one assignment has one unresolved worker.
        `jobs` is (tag, file); the first job is fixture-task and each later one
        gets its own accepted obligation. The committer is the repository's own
        configuration, which a merge commit needs; nothing supplies one."""
        for key,value in (("user.name","Fixture"),("user.email","fixture@example.invalid")):
            subprocess.run(["git","-C",str(self.repo),"config",key,value],check=True)
        started=[]
        for number,(tag,file) in enumerate(jobs):
            obligation="fixture-task" if number==0 else "fixture-task-"+tag
            if number:
                self.app.ECS.execute(self.root/"ecs",{"protocol":1,"command":"checkpoint","binding":self.scope["binding"],
                    "request_id":"accept-"+obligation,"checkpoint":{"statements":[{"verb":"commitment",
                    "fields":{"id":obligation,"title":"Create fictional "+tag}}]}})
            spec={**self.args,"request_id":"prepare-"+tag,"obligation_id":obligation}
            worker=self.call("prepare",spec)
            started.append((tag,file,spec,worker,self.start_fixture_worker(worker,tag+"-worker")))
        pairs=[]
        for tag,file,spec,worker,target in started:
            (target/file).write_text("FICTIONAL "+tag)
            self.app.git(target,"add",file)
            self.app.git(target,"commit","-qm","Fictional "+tag)
            commit=self.app.git(target,"rev-parse","HEAD")
            self.finish_fixture_worker(tag+"-worker")
            pairs.append((tag,spec,worker,commit))
        out=[]
        for tag,spec,worker,commit in pairs:
            reviewer=self.call("prepare",{**spec,"request_id":"review-"+tag,"role":"reviewer","review_of":worker["id"],
                "title":"Review "+tag,"brief":"Review the exact change; do not modify any files."})
            self.start_fixture_worker(reviewer,tag+"-reviewer")
            self.finish_fixture_worker(tag+"-reviewer","RICHOS_REVIEW "+json.dumps(
                {"commit":commit,"verdict":"passed","checks":["synthetic reviewer observation"]}))
            out.append(({"worker_id":worker["id"],"reviewer_id":reviewer["id"]},commit,spec["obligation_id"]))
        return out

    def test_a_second_reviewed_change_is_merged_after_the_first_moved_the_branch(self):
        """Part 4, finding 3. Two workers start from the same tip and change
        different files. The first lands on the tip they started from. The second used to be
        refused by an ancestry check whose whole message was Git's empty output
        ("engine operation refused"), which sent finished, reviewed work back for
        another implementation and review. It now lands by a merge commit whose
        second parent IS the reviewed commit, because Git merges the two cleanly."""
        (first,first_commit,first_task),(second,second_commit,second_task)=self.concurrent_reviewed_pairs(
            ("one","one.txt"),("two","two.txt"))
        start=self.app.git(self.repo,"rev-parse","main")
        # CONTROL: both really started from the same tip, and neither contains the other.
        self.assertEqual(self.app.git(self.repo,"merge-base",first_commit,second_commit),start)
        self.call("integrate",first)
        first_land=self.app.git(self.repo,"rev-parse","main")
        self.assertEqual(self.app.git(self.repo,"rev-parse",first_land+"^1"),start)
        self.assertEqual(self.app.git(self.repo,"rev-parse",first_land+"^2"),first_commit)
        landed_second=self.call("integrate",second)
        self.assertTrue(landed_second["work_integrated"])
        self.assertEqual(landed_second["commit"],second_commit)
        self.assertEqual(landed_second["cleanup_pending"],[])
        tip=self.app.git(self.repo,"rev-parse","main")
        self.assertEqual(self.app.git(self.repo,"rev-parse",tip+"^1"),first_land)
        self.assertEqual(self.app.git(self.repo,"rev-parse",tip+"^2"),second_commit)   # the reviewed commit, unrewritten
        self.assertEqual((self.repo/"one.txt").read_text(),"FICTIONAL one")
        self.assertEqual((self.repo/"two.txt").read_text(),"FICTIONAL two")
        self.assertEqual(self.app.git(self.repo,"status","--porcelain"),"")
        record=self.app.W.land_records(self.repo)[-1]
        self.assertEqual((record["before"],record["commit"],record["reviewed_commit"]),(first_land,tip,second_commit))
        # A repeated integrate merges nothing and records no second land.
        self.assertTrue(self.call("integrate",second)["work_integrated"])
        self.assertEqual(self.app.git(self.repo,"rev-parse","main"),tip)
        self.assertEqual(self.app.W.land_records(self.repo)[-1],record)
        self.assertIn(second_commit,landed_second["evidence_ref"])
        for task,args in ((first_task,first),(second_task,second)):
            self.assertTrue(self.call("complete",{"obligation_id":task,"worker_ids":[args["worker_id"]]})["obligation_closed"])

    def test_a_reviewed_change_that_conflicts_with_the_moved_branch_is_refused_by_name(self):
        """The reason the code gave still holds where it applies: no conflict is
        ever resolved here. The refusal now names the conflict instead of Git's
        empty ancestry answer, and nothing is left half merged."""
        (first,_,_),(second,_,_)=self.concurrent_reviewed_pairs(("left","result.txt"),("right","result.txt"))
        self.call("integrate",first)
        tip=self.app.git(self.repo,"rev-parse","main")
        with self.assertRaisesRegex(ValueError,"conflicts with it in result.txt"):
            self.call("integrate",second)
        self.assertEqual(self.app.git(self.repo,"rev-parse","main"),tip)
        self.assertEqual(self.app.git(self.repo,"status","--porcelain"),"")
        self.assertFalse(Path(self.app.git(self.repo,"rev-parse","--path-format=absolute","--git-path","MERGE_HEAD")).exists())
        self.assertEqual((self.repo/"result.txt").read_text(),"FICTIONAL left")

    # AN APP LAND GETS THE CHECKS A TERMINAL LAND GETS (plan item 6, 2026-10-21). The repository's
    # own hooks and its land fence are the repository's settings: these tests install them into the
    # fixture repository, never into the app, and the land must meet them as `git merge` from a
    # terminal does. Before, integrate ran Git with core.hooksPath=/dev/null and landed by
    # commit-tree plus a fast-forward, so no merge-time check and no fence ever saw an app land.

    def repository_hook(self,name,body):
        hooks=Path(self.app.git(self.repo,"rev-parse","--path-format=absolute","--git-common-dir"))/"hooks"
        hooks.mkdir(exist_ok=True)
        path=hooks/name;path.write_text("#!/bin/sh\n"+body);path.chmod(0o755)
        return path

    def test_an_app_land_runs_the_repositorys_own_merge_check_and_a_refusal_lands_nothing(self):
        args=self.reviewed_pair("gated")
        asked=self.root/"merge check asked";refuse=self.root/"merge check refuses";refuse.write_text("")
        self.repository_hook("pre-merge-commit",
            'echo asked >> %s\nif [ -e %s ]; then echo "FIXTURE MERGE CHECK: this land is refused" >&2; exit 1; fi\n'
            % (shlex.quote(str(asked)),shlex.quote(str(refuse))))
        tip=self.app.git(self.repo,"rev-parse","main")
        with self.assertRaisesRegex(ValueError,"FIXTURE MERGE CHECK: this land is refused"):
            self.call("integrate",args)
        self.assertEqual(asked.read_text().splitlines(),["asked"])             # the repository's check ran
        self.assertEqual(self.app.git(self.repo,"rev-parse","main"),tip)       # and nothing landed
        self.assertEqual(self.app.git(self.repo,"status","--porcelain","--untracked-files=all"),"")
        self.assertFalse(Path(self.app.git(self.repo,"rev-parse","--path-format=absolute","--git-path","MERGE_HEAD")).exists())
        self.assertFalse((self.repo/"result.txt").exists())
        self.assertEqual(self.app.W.land_records(self.repo),[])
        # The way out: once the check passes, the same reviewed work lands through it.
        refuse.unlink()
        landed=self.call("integrate",args)
        self.assertTrue(landed["work_integrated"])
        self.assertEqual(asked.read_text().splitlines(),["asked","asked"])
        head=self.app.git(self.repo,"rev-parse","main")
        self.assertEqual(self.app.git(self.repo,"rev-parse",head+"^1"),tip)
        self.assertEqual(self.app.git(self.repo,"rev-parse",head+"^2"),landed["commit"])   # the reviewed commit, unrewritten
        self.assertEqual((self.repo/"result.txt").read_text(),"FICTIONAL gated")

    def users_own_hooks_refuse(self,how):
        """The user's own Git settings select a hook folder whose merge check refuses, the way a
        user sets one for every repository (`global`: core.hooksPath in ~/.gitconfig) or for one
        repository (`includeIf`: a gitdir condition naming it). The app's lease hides the user's
        global file behind a generated identity-only GIT_CONFIG_GLOBAL, as EngineProfile::configure
        does today (mid-job review rv-20261021T082940Z-f79aeabf-fd06)."""
        hooks=self.root/"users own hooks";hooks.mkdir()
        (hooks/"pre-merge-commit").write_text('#!/bin/sh\necho "USERS OWN MERGE CHECK: refused" >&2\nexit 1\n')
        (hooks/"pre-merge-commit").chmod(0o755)
        home=Path(os.environ["HOME"]);setting='[core]\n\thooksPath = "%s"\n' % hooks
        if how=="global":
            (home/".gitconfig").write_text(setting)
        else:
            (home/"acme.gitconfig").write_text(setting)
            (home/".gitconfig").write_text('[includeIf "gitdir:%s/"]\n\tpath = "%s"\n'
                                           % (os.path.realpath(self.repo),home/"acme.gitconfig"))
        app_layer=self.root/"app identity.gitconfig"
        app_layer.write_text("[user]\n\tname = RichOS\n\temail = richos@localhost\n")
        return {"GIT_CONFIG_GLOBAL":str(app_layer),"GIT_CONFIG_NOSYSTEM":"1"}

    def test_an_app_land_runs_the_merge_check_the_users_global_git_settings_select(self):
        args=self.reviewed_pair("globalhooks")
        lease=self.users_own_hooks_refuse("global")
        tip=self.app.git(self.repo,"rev-parse","main")
        with patch.dict(os.environ,lease):
            with self.assertRaisesRegex(ValueError,"USERS OWN MERGE CHECK: refused"):
                self.call("integrate",args)
        self.assertEqual(self.app.git(self.repo,"rev-parse","main"),tip)
        self.assertEqual(self.app.git(self.repo,"status","--porcelain","--untracked-files=all"),"")

    def test_an_app_land_runs_the_merge_check_an_includeif_in_the_users_settings_selects(self):
        args=self.reviewed_pair("includeif")
        lease=self.users_own_hooks_refuse("includeIf")
        tip=self.app.git(self.repo,"rev-parse","main")
        with patch.dict(os.environ,lease):
            with self.assertRaisesRegex(ValueError,"USERS OWN MERGE CHECK: refused"):
                self.call("integrate",args)
        self.assertEqual(self.app.git(self.repo,"rev-parse","main"),tip)
        self.assertEqual(self.app.git(self.repo,"status","--porcelain","--untracked-files=all"),"")

    def fence_on(self,locks=None):
        """The fixture repository's land fence, installed and switched on with operator-fences.sh.
        `locks` is the land-lock home it is installed with (by default the one this land uses)."""
        entity=self.root/"fence entity";entity.mkdir()
        (entity/"orchestration.config").write_text('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n')
        env={**os.environ,**({"RICHOS_LAND_LOCKS_DIR":str(locks)} if locks else {})}
        for verb in ("install","on"):
            done=subprocess.run(["bash",str(ENGINE/"scripts/operator-fences.sh"),verb,"--repo",str(self.repo),"--entity",str(entity)],
                                capture_output=True,text=True,env=env)
            self.assertEqual(done.returncode,0,done.stdout+done.stderr)
        F=self.app.W._fence_program()
        conf,_paths=F.launcher_for(str(self.repo))
        self.assertTrue(F.fenced(conf))
        return F.Files(conf)

    def test_an_app_land_under_another_claude_account_takes_the_fences_own_lease_and_lands(self):
        """An account switch (mid-job review rv-20261021T072931Z-a4453faa-ffff, finding 1). The
        app runs a lease under an added account with CLAUDE_CONFIG_DIR at that account's folder, so
        this land's lock lives in that folder while the fence keeps the home it was installed with.
        The land still takes the fence's own lease, under the fence's own lock, and lands."""
        args=self.reviewed_pair("switched")
        first_account=self.root/"first account land locks"
        files=self.fence_on(first_account)
        self.assertNotEqual(os.path.realpath(files.lock),os.path.realpath(self.app.land_lock_path(self.repo)))  # CONTROL
        tip=self.app.git(self.repo,"rev-parse","main")
        seen=self.root/"lease during the switched land"
        self.repository_hook("pre-merge-commit","cp %s %s\n" % (shlex.quote(files.lease),shlex.quote(str(seen))))
        held=[]
        original=self.app.land_merge
        def watch(*argv):
            held.append(not self.land_lock_free(files.lock))
            return original(*argv)
        with patch.object(self.app,"land_merge",side_effect=watch):
            landed=self.call("integrate",args)
        self.assertTrue(landed["work_integrated"])
        self.assertEqual(held,[True],"the merge ran without the fence's own lock held")
        self.assertEqual(json.loads(seen.read_text())["holder"]["pid"],os.getpid())
        self.assertFalse(os.path.exists(files.lease),"the land lease outlived the land")
        self.assertTrue(self.land_lock_free(files.lock),"the fence's lock outlived the land")
        head=self.app.git(self.repo,"rev-parse","main")
        self.assertEqual(self.app.git(self.repo,"rev-parse",head+"^1"),tip)
        self.assertEqual(self.app.git(self.repo,"rev-parse",head+"^2"),landed["commit"])

    def test_an_app_land_into_a_fenced_repository_takes_the_land_lease_and_lands(self):
        # The work is prepared, done and reviewed first, and the fence switched on after: this test
        # is about the land. (The fence reads a `git worktree add` writer's argv from `ps`, split
        # on spaces, and this fixture's paths have spaces in them on purpose.)
        args=self.reviewed_pair("fenced")
        files=self.fence_on()
        tip=self.app.git(self.repo,"rev-parse","main")
        # POSITIVE CONTROL: the fence is on. A move of main by a process holding no lease is refused.
        stray=self.app.git(self.repo,"commit-tree",tip+"^{tree}","-p",tip,"-m","stray")
        refused=subprocess.run(["git","-C",str(self.repo),"update-ref","refs/heads/main",stray,tip],capture_output=True,text=True)
        self.assertNotEqual(refused.returncode,0,refused.stdout+refused.stderr)
        self.assertEqual(self.app.git(self.repo,"rev-parse","main"),tip)
        # What the lease says while the land's own merge runs, read by the repository's merge hook.
        seen=self.root/"lease during the land"
        self.repository_hook("pre-merge-commit","cp %s %s\n" % (shlex.quote(files.lease),shlex.quote(str(seen))))
        landed=self.call("integrate",args)
        self.assertTrue(landed["work_integrated"])
        self.assertTrue(seen.exists(),"the land ran no merge hook, so neither the repository's checks nor its fence saw it")
        holder=json.loads(seen.read_text())["holder"]
        self.assertEqual(holder["pid"],os.getpid())        # this land held the lease, as itself
        self.assertEqual(landed["land_lock"]["land_lease"],files.lease)
        self.assertFalse(os.path.exists(files.lease),"the land lease outlived the land")
        head=self.app.git(self.repo,"rev-parse","main")
        self.assertEqual(self.app.git(self.repo,"rev-parse",head+"^1"),tip)
        self.assertEqual(self.app.git(self.repo,"rev-parse",head+"^2"),landed["commit"])
        self.assertEqual((self.repo/"result.txt").read_text(),"FICTIONAL fenced")

    def test_two_conversations_landing_in_one_repository_take_one_lock_and_the_merge_is_under_it(self):
        args=self.reviewed_pair("onelock")
        lock=self.app.land_lock_path(self.repo)
        # ONE LOCK FOR TWO PARTITIONS. Frank's reproduction above is two
        # different paths for two threads; this is the same call from the second
        # thread's partition returning the FIRST one's file, byte for byte.
        other=self.root/"engine-state/workspaces"/hashlib.sha256(b'["depot","thread-b"]').hexdigest()
        with patch.dict(os.environ,{"RICHOS_APP_THREAD":"thread-b","RICHOS_WORKSPACES_DIR":str(other)}):
            self.assertEqual(self.app.land_lock_path(self.repo),lock)
        self.assertNotEqual(str(other),os.environ["RICHOS_WORKSPACES_DIR"])   # the partitions really do differ
        # POSITIVE CONTROL: the probe can say "free", and does, before the land.
        self.assertTrue(self.land_lock_free(lock))
        held=[]
        original=self.app.land_merge
        def watch(*argv):
            held.append(not self.land_lock_free(lock))
            return original(*argv)
        with patch.object(self.app,"land_merge",side_effect=watch):
            result=self.call("integrate",args)
        self.assertTrue(result["work_integrated"])
        self.assertEqual(held,[True],"the merge ran without this repository's land lock held")
        self.assertTrue(self.land_lock_free(lock),"the land lock outlived the land")
        self.assertLess(result["land_lock"]["waited_seconds"],1.0)
        self.assertNotIn("waited_for",result["land_lock"])

    def test_the_second_lander_waits_for_the_first_and_never_refuses_the_work(self):
        args=self.reviewed_pair("inorder")
        lock=self.app.land_lock_path(self.repo)
        holder=self.start_land_lock_holder(self.repo,"thread-b")
        self.assertFalse(self.land_lock_free(lock))      # POSITIVE CONTROL: it is genuinely held
        # The holder lands while it holds — through the shipped writer, not a
        # hand-made file, so what the waiter reads below is what a real land
        # leaves behind.
        self.app.W.append_land_record(self.repo,{"schema":1,"branch":"main","before":"a"*40,
            "commit":"b"*40,"at":"2026-09-17T19:00:00Z","thread_id":"thread-b","entity_id":"depot",
            "session_id":"holder-session","pid":holder.pid})
        original=self.app.W.git
        release=[]
        def release_once_waiting(repo,*argv,**kw):
            # `land_lock` asks git for the repository's identity immediately
            # before it blocks, so this fires once, at the start of the wait —
            # which is what makes the measured wait below deterministic rather
            # than a race with a timer. That question moved to workspaces.py
            # with the lock's keying rule on 2026-09-17, so this watches W.git.
            result=original(repo,*argv,**kw)
            if argv[:2]==("rev-parse","--path-format=absolute") and not release:
                timer=threading.Timer(0.5,self.release_land_lock_holder,[holder])
                release.append(timer);timer.start()
            return result
        with patch.object(self.app.W,"git",side_effect=release_once_waiting):
            landed=self.call("integrate",args)
        # IT WAITED AND THEN LANDED. It did not refuse, and a refusal here would
        # have sent finished, reviewed work back for a fresh implementation.
        self.assertTrue(landed["work_integrated"])
        self.assertGreater(landed["land_lock"]["waited_seconds"],0.05)
        self.assertIn("thread-b",landed["land_lock"]["waited_for"])
        # AND IT CAN SAY WHAT THE HOLDER DID, read from the append-only history
        # beside the lock rather than from a field on the lock that the next
        # holder rewrites in place.
        self.assertEqual(landed["land_lock"]["waited_for_land"]["thread_id"],"thread-b")
        self.assertEqual(landed["land_lock"]["waited_for_land"]["commit"],"b"*40)
        self.assertEqual(holder.stdout.readline().strip(),"RELEASED")

    def test_a_holder_past_the_bound_is_refused_by_name_with_nothing_merged(self):
        args=self.reviewed_pair("bound")
        before=self.app.git(self.repo,"rev-parse","HEAD")
        holder=self.start_land_lock_holder(self.repo,"thread-stuck")
        with patch.dict(os.environ,{"RICHOS_LAND_LOCK_TIMEOUT":"0.4"}):
            with self.assertRaises(ValueError) as caught: self.call("integrate",args)
        refusal=str(caught.exception)
        self.assertIn("thread-stuck",refusal)            # the holder is NAMED
        self.assertIn("did not finish within 0 seconds",refusal)
        self.assertIn("nothing here was merged",refusal)
        self.assertEqual(self.app.git(self.repo,"rev-parse","HEAD"),before)
        # POSITIVE CONTROL: released, the very same land goes straight through.
        self.release_land_lock_holder(holder)
        self.assertTrue(self.call("integrate",args)["work_integrated"])

    def test_one_repository_reached_two_ways_is_one_lock_and_two_repositories_are_two(self):
        link=self.root/"linked project";link.symlink_to(self.repo,target_is_directory=True)
        self.assertEqual(self.app.land_lock_path(link),self.app.land_lock_path(self.repo))
        # A LINKED WORKTREE SHARES THE REF STORE, so it shares the lock: two
        # paths that can move one branch must never hold two locks.
        checkout=self.root/"linked worktree"
        self.app.git(self.repo,"worktree","add","-q","-b","landlock-probe",str(checkout),"HEAD")
        self.assertEqual(self.app.land_lock_path(checkout),self.app.land_lock_path(self.repo))
        # TWO REPOSITORIES DO NOT WAIT ON EACH OTHER — the case the CEO says he
        # will actually run. A machine-wide lock would make them, for no reason.
        second=self.root/"second project";second.mkdir()
        subprocess.run(["git","init","--template=","-q","-b","main",str(second)],check=True)
        self.assertNotEqual(self.app.land_lock_path(second),self.app.land_lock_path(self.repo))
        self.start_land_lock_holder(self.repo,"thread-b")
        self.assertFalse(self.land_lock_free(self.app.land_lock_path(self.repo)))   # POSITIVE CONTROL
        self.assertTrue(self.land_lock_free(self.app.land_lock_path(second)))
        with self.app.land_lock(self.scope,str(second)) as land:
            self.assertLess(land["waited_seconds"],1.0)
            self.assertNotIn("waited_for",land)

    def test_the_land_lock_refuses_to_live_inside_either_conversation_partition(self):
        for name in ("RICHOS_WORKSPACES_DIR","RICHOS_APP_STATE"):
            with patch.dict(os.environ,{"RICHOS_LAND_LOCKS_DIR":str(Path(os.environ[name])/"land-locks")}):
                with self.assertRaisesRegex(ValueError,"per-conversation partition"):
                    self.app.land_lock_path(self.repo)
        # POSITIVE CONTROL: outside both, the same call answers.
        self.assertTrue(str(self.app.land_lock_path(self.repo)).endswith(".lock"))
        # AND THE DEFAULT IS THE MACHINE-WIDE HOME, which is neither partition:
        # CLAUDE_CONFIG_DIR, the one variable the app neither sets nor strips for
        # the engine it launches (engine_profile.rs:156-210).
        home=self.root/"machine home"
        with patch.dict(os.environ,{"CLAUDE_CONFIG_DIR":str(home)}):
            del os.environ["RICHOS_LAND_LOCKS_DIR"]
            # A string, not a Path: this derivation moved into workspaces.py on
            # 2026-09-17 (one keying rule, in the layer the protected-ref check
            # can reach), and that module is imported by a hook on every tool
            # call of every agent — it uses os.path throughout and imports no
            # pathlib.
            self.assertEqual(self.app.land_locks_dir(),os.path.join(os.path.realpath(home),"state","land-locks"))

    def test_the_land_record_is_appended_beside_the_lock_and_names_the_conversation(self):
        """THE SECOND HALF OF THE LAND LOCK: two threads' lands take turns, and
        the one that WAITED moved the branch under the other's running agents.
        This is the record that lets those agents be told WHICH conversation
        did it (mega-lander/workspaces.py, `land_by_another_conversation`),
        rather than only that the branch moved.

        APPEND-ONLY, AND THAT IS THE PROPERTY UNDER TEST. Until 2026-09-17 the
        land was written as a `landed` field on the lock file — which the next
        lander rewrites in place, so of two lands in a row only the second
        survived, and an agent whose snapshot predated the first saw a move
        nothing could name."""
        record=self.app.W.land_record_path(self.repo)
        self.assertFalse(os.path.exists(record))          # POSITIVE CONTROL: nothing has landed yet
        # BESIDE THE LOCK, AND KEYED THE SAME WAY: one repository, one history.
        self.assertEqual(record,self.app.land_lock_path(self.repo)[:-len(".lock")]+".lands")
        second=self.root/"second project";second.mkdir()
        subprocess.run(["git","init","--template=","-q","-b","main",str(second)],check=True)
        self.assertNotEqual(self.app.W.land_record_path(second),record)
        first_args=self.reviewed_pair("rec1")
        first=self.call("integrate",first_args)
        rows=self.app.W.land_records(self.repo)
        self.assertEqual(len(rows),1)
        self.assertEqual(rows[0]["thread_id"],"thread-a")            # the conversation that landed
        self.assertEqual(rows[0]["entity_id"],"depot")
        self.assertEqual(rows[0]["branch"],"main")
        self.assertEqual(rows[0]["reviewed_commit"],first["commit"])
        self.assertEqual(rows[0]["commit"],self.app.git(self.repo,"rev-parse","main"))     # the land's merge commit
        # A REPEATED integrate merges nothing, so it records no land: the record
        # is written where the ref actually moves, not where the call ends.
        self.assertTrue(self.call("integrate",first_args)["work_integrated"])
        self.assertEqual(self.app.W.land_records(self.repo),rows)
        # A SECOND LAND APPENDS. The first row survives it byte for byte, and
        # the two chain: the second's `before` is the first's commit.
        second_result=self.call("integrate",self.reviewed_pair("rec2"))
        after=self.app.W.land_records(self.repo)
        self.assertEqual(len(after),2)
        self.assertEqual(after[0],rows[0])
        self.assertEqual(after[1]["before"],rows[0]["commit"])
        self.assertEqual(after[1]["reviewed_commit"],second_result["commit"])
        self.assertEqual(after[1]["commit"],self.app.git(self.repo,"rev-parse","main"))
        # THE WRITER AND THE READER, END TO END, IN ONE PROCESS. Everything
        # above is the writing half. This is the function the PostToolUse check
        # actually calls (workspaces.py, `land_by_another_conversation`), asked
        # from ANOTHER conversation thread about the land this one just made: it
        # derives the record's path independently, reads the fields by the names
        # the writer wrote, and names the thread. A rename on either side breaks
        # here rather than in silence six weeks from now.
        with patch.dict(os.environ,{"RICHOS_APP_THREAD":"thread-b"}):
            told=self.app.W.land_by_another_conversation(
                str(self.repo),"main",after[1]["before"],after[1]["commit"])
        self.assertIn("thread-a",told)
        self.assertIn(after[1]["at"],told)
        # ... and silent for the conversation that made it.
        with patch.dict(os.environ,{"RICHOS_APP_THREAD":"thread-a"}):
            self.assertIsNone(self.app.W.land_by_another_conversation(
                str(self.repo),"main",after[1]["before"],after[1]["commit"]))
        # AND THE LOCK FILE ITSELF CARRIES NO LAND, which is what made the first
        # of two lands unanswerable.
        self.assertNotIn("landed",self.app._read_land_lock(self.app.land_lock_path(self.repo)) or {})

    # ---- WITNESS (d), THE LAND (Output side panel PRD §4.1 (d), slice S2b) -----------------
    # integrate writes one row per landed file into the back-end session's writes.jsonl: the
    # landed path, the worktree path it retires, the commit and the worker its receipt names.

    def land_rows(self):
        path=self.root/"engine-state/evidence"/self.session/"writes.jsonl"
        return [json.loads(l) for l in path.read_text().splitlines()] if path.exists() else []

    def reviewed_change(self,tag,edit):
        """reviewed_pair with any change: `edit(target)` edits the worker's worktree, and
        everything it changed is committed. Returns the args, the worker receipt and its
        worktree (whose real path the land row names)."""
        spec={**self.args,"request_id":"prepare-"+tag}
        worker=self.call("prepare",spec)
        target=self.start_fixture_worker(worker,tag+"-worker")
        edit(target)
        self.app.git(target,"add","-A")
        self.app.git(target,"-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-qm","Fictional "+tag)
        commit=self.app.git(target,"rev-parse","HEAD")
        self.finish_fixture_worker(tag+"-worker")
        reviewer=self.call("prepare",{**spec,"request_id":"review-"+tag,"role":"reviewer","review_of":worker["id"],
            "title":"Review "+tag,"brief":"Review the exact change; do not modify any files."})
        self.start_fixture_worker(reviewer,tag+"-reviewer")
        self.finish_fixture_worker(tag+"-reviewer","RICHOS_REVIEW "+json.dumps(
            {"commit":commit,"verdict":"passed","checks":["synthetic reviewer observation"]}))
        return {"worker_id":worker["id"],"reviewer_id":reviewer["id"]},worker,os.path.realpath(target)

    def test_land_witness_records_each_landed_file_at_its_landed_path_from_its_worktree(self):
        def notes(target):
            (target/"notes.md").write_text("Notes for the walk test.\n")
            (target/"notes.pdf").write_bytes(b"%PDF-1.7 fictional")
        args,worker,worktree=self.reviewed_change("notes",notes)
        self.assertEqual(self.land_rows(),[])                  # POSITIVE CONTROL: nothing before the land
        landed=self.call("integrate",args)
        self.assertTrue(landed["work_integrated"])
        head=self.app.git(self.repo,"rev-parse","main")
        rows=self.land_rows()
        self.assertEqual(sorted((r["source"],r["path"],r["from"],r["commit"]) for r in rows),
            [("land",str(self.repo/n),os.path.join(worktree,n),head) for n in ("notes.md","notes.pdf")])
        for r in rows:
            self.assertEqual(r["worker"],{"name":worker["name"],"agent_id":"notes-worker"})   # the receipt's own
            self.assertEqual((r["session_id"],r["tool_use_id"],r["agent_id"]),(self.session,None,None))
            self.assertTrue((self.repo/Path(r["path"]).name).is_file())                  # the copy that opens
        self.assertFalse(Path(worktree).exists(),"cleanup deleted the worktree the rows retire")
        # A REPEATED integrate merges nothing and records nothing.
        self.assertTrue(self.call("integrate",args)["work_integrated"])
        self.assertEqual(self.land_rows(),rows)

    def test_land_witness_records_nothing_for_a_land_that_only_deletes_a_file(self):
        self.call("integrate",self.reviewed_change("add",lambda t:(t/"gone.txt").write_text("soon gone"))[0])
        before=self.land_rows()
        self.assertEqual([Path(r["path"]).name for r in before],["gone.txt"])
        removed=self.call("integrate",self.reviewed_change("remove",lambda t:(t/"gone.txt").unlink())[0])
        self.assertTrue(removed["work_integrated"])
        self.assertFalse((self.repo/"gone.txt").exists())
        self.assertEqual(self.land_rows(),before)

    def test_land_witness_that_raises_never_fails_the_land(self):
        args,_,_=self.reviewed_change("boom",lambda t:(t/"notes.md").write_text("n"))
        real=self.app.load
        def broken(name,path):
            module=real(name,path)
            if name=="richos_app_evidence":
                def refuse(*_a,**_k): raise OSError("synthetic: the evidence folder is unwritable")
                module.append_land_rows=refuse
            return module
        with patch.object(self.app,"load",side_effect=broken) as loaded:
            landed=self.call("integrate",args)
        self.assertIn("richos_app_evidence",[c.args[0] for c in loaded.call_args_list])   # the witness DID run
        self.assertTrue(landed["work_integrated"])
        self.assertEqual(landed["cleanup_pending"],[])
        self.assertTrue((self.repo/"notes.md").is_file())
        self.assertEqual(self.land_rows(),[])

    def test_land_witness_of_a_job_in_two_repositories_names_each_repositorys_own_paths(self):
        second=self.second_repository()
        spec={**self.args,"request_id":"multi-witness","repos":[str(second)]}
        worker=self.call("prepare",spec)
        self.start_fixture_worker(worker,"multi-witness")
        paths={r:os.path.realpath(p) for r,p in self.workspaces_of(worker).items()}
        commits=self.commit_in_each(paths,"result.txt","FICTIONAL")
        self.finish_fixture_worker("multi-witness")
        reviewer=self.call("prepare",{**spec,"request_id":"multi-witness-review","role":"reviewer","review_of":worker["id"],
            "title":"Review both","brief":"Review the change in both repositories; do not modify any files."})
        self.start_fixture_worker(reviewer,"multi-witness-reviewer")
        self.finish_fixture_worker("multi-witness-reviewer","RICHOS_REVIEW "+json.dumps(
            {"commit":commits[str(self.repo)],"commits":commits,"verdict":"passed","checks":["both repositories"]}))
        self.assertTrue(self.call("integrate",{"worker_id":worker["id"],"reviewer_id":reviewer["id"]})["work_integrated"])
        # Each row names the commit its repository's branch now points at: that land's merge commit.
        self.assertEqual(sorted((r["path"],r["from"],r["commit"]) for r in self.land_rows()),
            sorted((os.path.join(repo,"result.txt"),os.path.join(paths[repo],"result.txt"),
                    self.app.git(repo,"rev-parse","main")) for repo in commits))


class RunBoundIsTheWholeOperations(unittest.TestCase):
    """Hunt part 4, finding 18 (richos-hq docs/audits/2026-09-29-hunt/
    part-4-codex.md): the app gave a whole preparation 120 s while the
    workspace creator alone allowed its setup 120 s, and subprocess.run's
    timeout stopped only the direct child. run() now tells the command when it
    stops waiting, and at the bound stops what the command started too."""

    def setUp(self):
        self.app=load()
        self.scratch=tempfile.TemporaryDirectory(prefix="app run bound ");self.addCleanup(self.scratch.cleanup)

    @staticmethod
    def alive(pid):
        try: os.kill(pid,0)
        except ProcessLookupError: return False
        stat=subprocess.run(["ps","-o","stat=","-p",str(pid)],capture_output=True,text=True).stdout
        return bool(stat.strip()) and "Z" not in stat

    def test_finding_18_the_command_is_told_when_the_caller_stops_waiting(self):
        import time
        t0=time.time()
        out=self.app.run([sys.executable,"-c","import os; print(os.environ.get('RICHOS_OPERATION_DEADLINE', ''))"])
        self.assertTrue(out.strip(),"no deadline was exported to the command")
        self.assertAlmostEqual(int(out.strip()),t0+getattr(self.app,"RUN_TIMEOUT",120),delta=5)

    def test_finding_18_at_the_bound_what_the_command_started_is_stopped_too(self):
        import time
        pidfile=Path(self.scratch.name)/"grandchild.pid"
        # the command starts a child of its own and both outlive the bound
        command=["sh","-c",'sh -c "sleep 30" & echo $! > "$0"; sleep 30',str(pidfile)]
        grandchild=[]
        def cleanup():
            # the test's own leftover, by the pid its own command recorded
            for pid in grandchild:
                if self.alive(pid): os.kill(pid,9)
        self.addCleanup(cleanup)
        t0=time.monotonic()
        with patch.object(self.app,"RUN_TIMEOUT",2,create=True):
            with self.assertRaises(subprocess.TimeoutExpired):
                self.app.run(command)
        self.assertLess(time.monotonic()-t0,20)
        grandchild.append(int(pidfile.read_text().strip()))
        deadline=time.monotonic()+10
        while self.alive(grandchild[0]) and time.monotonic()<deadline: time.sleep(0.1)
        self.assertFalse(self.alive(grandchild[0]),"the command's own child outlived the run() bound")


class _Result(unittest.TextTestResult):
    """Prints `  PASS  <test>` / `  FAIL  <test>` so the mutation harness
    (app.mutation.sh) can tell WHICH property went red, rather than only that
    something did. Same shape as workspaces.test.py's reporter."""

    def addSuccess(self,test):
        super().addSuccess(test);self.stream.write("  PASS  %s\n"%test._testMethodName)

    def addFailure(self,test,err):
        super().addFailure(test,err);self.stream.write("  FAIL  %s\n"%test._testMethodName)

    def addError(self,test,err):
        super().addError(test,err);self.stream.write("  FAIL  %s (error)\n"%test._testMethodName)


def _resolve_names(names,loader):
    """A class name or `Class.test` passes through. A bare method name, or the start of
    one, selects every test method it begins -- the same set whose `  FAIL  <name>` line
    the mutation harness's `grep "FAIL  <want>"` would match, so a mutant focused on its
    want runs exactly the cases that could prove it (mutation_focus want-as-argument).
    A name that selects nothing is passed through for the loader to refuse loudly.
    (Same rule as workspaces.test.py's; every case here builds its own fixture in setUp.)"""
    module=sys.modules[__name__]
    cases=[c for c in vars(module).values() if isinstance(c,type) and issubclass(c,unittest.TestCase)]
    out=[]
    for n in names:
        if "." in n or hasattr(module,n):
            out.append(n);continue
        hits=["%s.%s"%(c.__name__,m) for c in cases for m in loader.getTestCaseNames(c) if m.startswith(n)]
        out.extend(hits or [n])
    return out


if __name__=="__main__":
    runner=unittest.TextTestRunner(stream=sys.stdout,verbosity=0,resultclass=_Result)
    loader=unittest.defaultTestLoader
    names=_resolve_names([a for a in sys.argv[1:] if a],loader)
    suite=(loader.loadTestsFromNames(names,sys.modules[__name__]) if names
           else loader.loadTestsFromModule(sys.modules[__name__]))
    result=runner.run(suite)
    print("=== desktop dispatch tests: %d run, %d failed ==="%(
        result.testsRun,len(result.failures)+len(result.errors)))
    sys.exit(0 if result.wasSuccessful() else 1)
