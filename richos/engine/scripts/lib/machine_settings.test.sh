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
   home=pathlib.Path(root);source=home/'Library/Application Support/RichOS/.env'
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
 def test_named_lifetime_symlink_and_containment(self):
  with tempfile.TemporaryDirectory() as root:
   base=pathlib.Path(root);bulk=base/'bulk';bulk.mkdir();alias=base/'alias';alias.symlink_to(bulk,target_is_directory=True)
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
 def test_process_sweep_uses_job_tmp_and_never_named_root(self):
  import importlib.util
  spec=importlib.util.spec_from_file_location('fixture_reaper',pathlib.Path(sys.argv[1],'scratch-reaper.py'))
  R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)
  class Walls:
   def check(self,*args):return None
  with tempfile.TemporaryDirectory() as root:
   base=pathlib.Path(root);job=base/'nightly/home.noindex/tmp';job.mkdir(parents=True)
   allocator=job/'richos-scratch';dead=allocator/'dead';live=allocator/'live'
   dead.mkdir(parents=True);live.mkdir();named=job/'richos-agents/agent';named.mkdir(parents=True)
   cfg={'age_floor_minutes':60,'scratch_root_name':'richos-scratch','default_ttl_minutes':60}
   rows={str(dead):{'pid':101,'created_epoch':0,'ttl_minutes':60},str(live):{'pid':102,'created_epoch':0,'ttl_minutes':60}}
   reaper=R.Reaper.__new__(R.Reaper);reaper.cfg=cfg;reaper.now=10000;reaper.floor=3600;reaper.roots=[];reaper._standing=set()
   decisions=[];reaper.add=lambda path,kind,size,status,reason:decisions.append((path,status))
   reaper.owner_state=lambda pid,*args:(pid==102,'controlled generation')
   reaper.open_under_alloc=lambda:set()
   with patch.dict(os.environ,{'TMPDIR':str(job),'SCRATCH_LEDGER':str(base/'ledger')}),patch.object(R,'read_ledger',return_value=rows),patch.object(R,'measure',return_value=(1,0,False)):
    reaper.scan_scratch_root(Walls())
   self.assertIn((str(dead),R.DELETE),decisions)
   self.assertIn((str(live),R.KEEP),decisions)
   self.assertFalse(any(path==str(named) for path,status in decisions))
   # A scheduled sweep with another TMPDIR sees no nightly allocations.
   decisions.clear()
   with patch.dict(os.environ,{'TMPDIR':str(base/'scheduled')}):reaper.scan_scratch_root(Walls())
   self.assertFalse(decisions)
 def test_shell_quotes_literal_values_and_forward_array(self):
  with tempfile.TemporaryDirectory() as root:
   base=pathlib.Path(root);source=base/'settings';marker=base/'executed'
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
