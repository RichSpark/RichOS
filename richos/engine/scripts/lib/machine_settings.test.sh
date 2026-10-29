#!/usr/bin/env bash
# machine_settings.py, machine-settings.sh and shared fixtures: real reader/storage seams.
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd -P)"
PYTHONDONTWRITEBYTECODE=1 python3 - "$DIR" <<'PY'
import json, os, pathlib, subprocess, sys, tempfile, unittest
from unittest.mock import patch
sys.path.insert(0,sys.argv[1]);import machine_settings as M
class Settings(unittest.TestCase):
 def test_shared_complete_key_contract(self):
  fixture=json.loads(pathlib.Path(sys.argv[1],'machine_settings.fixtures.json').read_text())
  self.assertEqual(list(M.KEYS),fixture['keys'])
  for case in fixture['cases']:
   with self.subTest(case=case['name']):
    if case.get('error'):
     with self.assertRaises(ValueError):M.parse(case['text'])
    else:self.assertEqual(M.parse(case['text']),case['values'])
 def test_selector_precedence_and_snapshot(self):
  with tempfile.TemporaryDirectory() as root:
   home=pathlib.Path(root).resolve();source=home/'Library/Application Support/RichOS/.env'
   env={'HOME':str(home),'TMPDIR':str(home)}
   self.assertEqual(M.load(env),{})
   source.parent.mkdir(parents=True);source.write_text('CARGO_TARGET_DIR=/file cache\n')
   env['RICHOS_BULK_ROOT']=str(home)
   self.assertEqual(M.load(env)['CARGO_TARGET_DIR'],'/file cache')
   self.assertEqual(M.storage('cache',key='CARGO_TARGET_DIR',environment=env),pathlib.Path('/file cache'))
   env['CARGO_TARGET_DIR']='/environment cache'
   frozen=M.environment(env)
   source.write_text('UNKNOWN=/invalid\n')
   self.assertEqual(M.load(frozen)['CARGO_TARGET_DIR'],'/environment cache')
   with self.assertRaises(ValueError):M.load(env)
   with self.assertRaises(ValueError):M.load({'HOME':str(home),'RICHOS_ENV_FILE':str(home/'missing')})
   with self.assertRaises(ValueError):M.load({'HOME':str(home),'RICHOS_ENV_FILE':'relative'})
 def test_mount_refusal_before_writes_and_mount_component(self):
  volume=pathlib.Path('/Volumes/'+'Fixture')
  env={'HOME':'/fixture home','RICHOS_BULK_ROOT':str(volume/'nested')}
  with patch.object(pathlib.Path,'is_mount',return_value=False),patch.object(pathlib.Path,'mkdir') as mkdir:
   with self.assertRaisesRegex(ValueError,'not mounted'):M.scratch('agent',env)
   mkdir.assert_not_called()
  with patch.object(pathlib.Path,'is_mount',autospec=True,side_effect=lambda p:p==volume),patch.object(pathlib.Path,'is_dir',return_value=True):
   self.assertEqual(M.agent_root(env),volume/'nested/tmp/claude')
 def test_legacy_android_key_forwarding_and_current_name_priority(self):
  with tempfile.TemporaryDirectory() as root:
   home=pathlib.Path(root).resolve();source=home/'settings'
   source.write_text('RICHOS_NATIVE_ANDROID_CACHE=/old cache\n')
   env={'HOME':str(home),'RICHOS_ENV_FILE':str(source)}
   frozen=M.environment(env)
   self.assertEqual(frozen['RANDROID_CACHE'],'/old cache')
   legacy={**env,'RICHOS_NATIVE_ANDROID_CACHE':'/legacy environment'}
   self.assertEqual(M.load(legacy)['RANDROID_CACHE'],'/legacy environment')
   source.write_text('RICHOS_NATIVE_ANDROID_CACHE=/old cache\nRANDROID_CACHE=/current cache\n')
   self.assertEqual(M.load(legacy)['RANDROID_CACHE'],'/current cache')
   self.assertEqual(M.load(frozen)['RANDROID_CACHE'],'/old cache')
   child_env={k:v for k,v in os.environ.items() if k not in M.KEYS and k!=M.RESOLVED}
   child_env.update(env)
   source.write_text('RICHOS_NATIVE_ANDROID_CACHE=/old cache\n')
   command='source "$1/machine-settings.sh"; richos_machine_settings || exit; printf "%s\\n" "$RANDROID_CACHE" "${RICHOS_MACHINE_ENV[@]}"'
   result=subprocess.run(['bash','-c',command,'fixture',sys.argv[1]],env=child_env,capture_output=True,text=True,check=True)
   self.assertEqual(result.stdout.splitlines()[0],'/old cache')
   self.assertIn('RANDROID_CACHE=/old cache',result.stdout.splitlines())
 def test_named_lifetime_symlink_and_containment(self):
  with tempfile.TemporaryDirectory() as root:
   base=pathlib.Path(root).resolve();bulk=base/'bulk';bulk.mkdir();alias=base/'alias';alias.symlink_to(bulk,target_is_directory=True)
   env={'HOME':str(base),'TMPDIR':str(base),'RICHOS_BULK_ROOT':str(alias)}
   target=M.scratch('agent',env);(target/'active').write_text('still working')
   self.assertEqual(target,bulk/'tmp/claude/agent')
   self.assertEqual(M.scratch('agent',env),target)
   self.assertTrue((target/'active').exists())
   for name in ('../other','a/b','', '..'):
    with self.assertRaises(ValueError):M.scratch(name,env)
   (bulk/'tmp/claude/escape').symlink_to(base,target_is_directory=True)
   with self.assertRaises(ValueError):M.scratch('escape',env)
   nested=base/'richos-scratch';nested.mkdir()
   with self.assertRaises(ValueError):M.agent_root({**env,'RICHOS_BULK_ROOT':str(nested)})
 def test_each_worker_of_one_teammate_gets_its_own_scratch_folder(self):
  import shutil
  with tempfile.TemporaryDirectory() as root:
   base=pathlib.Path(root).resolve();bulk=base/'bulk';bulk.mkdir()
   env={'HOME':str(base),'TMPDIR':str(base),'RICHOS_BULK_ROOT':str(bulk)}
   first=M.scratch('zach',{**env,'RICHOS_AGENT_OWNER':'a1f684fad4d37879f'})
   second=M.scratch('zach',{**env,'RICHOS_AGENT_OWNER':'b2e795be5e48980aa'})
   self.assertNotEqual(first,second)
   self.assertEqual(first,bulk/'tmp/claude/zach/a1f684fad4d37879f')
   self.assertEqual(M.scratch('zach',{**env,'RICHOS_AGENT_OWNER':'a1f684fad4d37879f'}),first)
   (first/'home').mkdir();(second/'home').mkdir();(second/'home'/'work').write_text('still working')
   # One worker's cleanup of the folder it was given leaves the other worker's files alone.
   shutil.rmtree(first)
   self.assertTrue((second/'home'/'work').exists())
   # A caller with no worker id (the lead, a script) keeps the plain per-name folder.
   self.assertEqual(M.scratch('zach',env),bulk/'tmp/claude/zach')
   for bad in ('../x','a/b','..','.'):
    with self.assertRaises(ValueError):M.scratch('zach',{**env,'RICHOS_AGENT_OWNER':bad})
   # The command line reads the same variable from the caller's environment.
   cli=subprocess.run([sys.executable,str(pathlib.Path(sys.argv[1],'machine_settings.py')),'scratch','zach'],capture_output=True,text=True,check=True,
                      env={**{k:v for k,v in os.environ.items() if not k.startswith('RICHOS_')},**env,'RICHOS_AGENT_OWNER':'c3d8a6cf6f59a91bb'})
   self.assertEqual(pathlib.Path(cli.stdout.strip()),bulk/'tmp/claude/zach/c3d8a6cf6f59a91bb')
 def test_process_sweep_uses_job_tmp_and_never_named_root(self):
  import importlib.util
  spec=importlib.util.spec_from_file_location('fixture_reaper',pathlib.Path(sys.argv[1],'scratch-reaper.py'))
  R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)
  class Walls:
   def check(self,*args,**kwargs):return None
  with tempfile.TemporaryDirectory() as root:
   base=pathlib.Path(root).resolve();job=base/'nightly/home.noindex/tmp';job.mkdir(parents=True)
   allocator=job/'richos-scratch';dead=allocator/'dead';live=allocator/'live'
   dead.mkdir(parents=True);live.mkdir();named=job/'richos-agents/agent';named.mkdir(parents=True)
   cfg={'age_floor_minutes':60,'scratch_root_name':'richos-scratch','default_ttl_minutes':60}
   rows={str(dead):{'pid':101,'created_epoch':0,'ttl_minutes':60},str(live):{'pid':102,'created_epoch':0,'ttl_minutes':60}}
   reaper=R.Reaper.__new__(R.Reaper);reaper.cfg=cfg;reaper.now=10000;reaper.floor=3600;reaper.roots=[];reaper._standing=set()
   decisions=[];reaper.add=lambda path,kind,size,status,reason:decisions.append((path,status))
   reaper.owner_state=lambda pid,*args:(pid==102,'controlled generation')
   reaper.open_under_alloc=lambda *root:set()
   with patch.dict(os.environ,{'TMPDIR':str(job),'SCRATCH_LEDGER':str(base/'ledger')}),patch.object(R,'read_ledger',return_value=rows),patch.object(R,'measure',return_value=(1,0,False)):
    reaper.scan_scratch_root(Walls())
   self.assertIn((str(dead),R.DELETE),decisions)
   self.assertIn((str(live),R.KEEP),decisions)
   self.assertFalse(any(path==str(named) for path,status in decisions))
   # A scheduled sweep with another TMPDIR sees no nightly allocations.
   decisions.clear()
   with patch.dict(os.environ,{'TMPDIR':str(base/'scheduled')}):reaper.scan_scratch_root(Walls())
   self.assertFalse(decisions)
 def test_scheduled_temp_scan_does_not_nominate_named_scratch(self):
  import importlib.util
  spec=importlib.util.spec_from_file_location('scheduled_reaper',pathlib.Path(sys.argv[1],'scratch-reaper.py'))
  R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)
  with tempfile.TemporaryDirectory() as directory:
   base=pathlib.Path(directory).resolve().resolve();named=base/'richos-agents';(named/'worker').mkdir(parents=True)
   reaper=R.Reaper.__new__(R.Reaper)
   reaper.cfg={'legacy_tmp_patterns':[], 'legacy_age_hours':0,'scratch_root_name':'richos-scratch','tmp_patterns':['*'],'deny_by_default':True,'agent_roots':[str(named)],'claude_roots':[],'shared_tmp_roots':[str(base)]}
   reaper.roots=[];reaper._unknown=[];reaper.floor=0;reaper.now=10000
   with patch.dict(os.environ,{'TMPDIR':str(base)}),patch.object(R,'measure') as measure:
    reaper.scan_tmp(None);reaper.scan_shared_tmp(None)
   measure.assert_not_called();self.assertFalse(reaper._unknown)
   self.assertTrue(reaper.named_scratch(named/'worker'))
   self.assertTrue(reaper.named_scratch(base))
 def test_process_apply_has_no_global_collection_or_state_write(self):
  import contextlib, importlib.util, io
  from types import SimpleNamespace
  spec=importlib.util.spec_from_file_location('apply_reaper',pathlib.Path(sys.argv[1],'scratch-reaper.py'))
  R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)
  with patch.object(R,'Liveness',return_value=SimpleNamespace(notes=[],running={})):
   reaper=R.Reaper({'age_floor_minutes':60},[])
  with patch.object(R,'config_from_env',return_value={}), patch.object(R,'load_workspaces',return_value=[]), patch.object(R,'Reaper',return_value=reaper), patch.object(R,'Walls'), patch.object(reaper,'scan_scratch_root'), patch.object(reaper,'collect_test_instances',return_value=[]), patch.object(reaper,'collect_test_devices',return_value=[]) as devices, patch.object(reaper,'prune_docker',return_value=[]) as docker, patch.object(reaper,'_record_failures'), patch.object(R,'write_log'), patch.object(R,'write_state') as state, contextlib.redirect_stdout(io.StringIO()):
   self.assertEqual(R.main(['--process-only','--apply','--json']),0)
  devices.assert_not_called();docker.assert_not_called();state.assert_not_called()
 def test_shell_quotes_literal_values_and_forward_array(self):
  with tempfile.TemporaryDirectory() as root:
   base=pathlib.Path(root).resolve();source=base/'settings';marker=base/'executed'
   value='/cache space/$(touch '+str(marker)+')'
   source.write_text('CARGO_TARGET_DIR='+value+'\n')
   env={key:value for key,value in os.environ.items() if key not in M.KEYS}
   env.update(HOME=str(base),RICHOS_ENV_FILE=str(source))
   env.pop(M.RESOLVED,None)
   command='source "$1/machine-settings.sh"; richos_machine_settings || exit; printf "%s\\n" "$CARGO_TARGET_DIR" "${RICHOS_MACHINE_ENV[@]}"'
   result=subprocess.run(['bash','-c',command,'fixture',sys.argv[1]],env=env,capture_output=True,text=True,check=True)
   self.assertEqual(result.stdout.splitlines()[0],value)
   self.assertIn(M.RESOLVED+'=1',result.stdout.splitlines())
   self.assertFalse(marker.exists())
unittest.main(argv=['machine-settings'],verbosity=2)
PY
