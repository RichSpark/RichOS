#!/usr/bin/env bash
# machine_paths.py / publication-completeness.py: scope, exceptions and exact debt.
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 - "$DIR" <<'PY'
import importlib.util, json, sys, unittest
from collections import Counter
sys.path.insert(0, sys.argv[1])
from machine_paths import matches, in_scope
spec=importlib.util.spec_from_file_location('completeness',sys.argv[1]+'/../publication-completeness.py')
C=importlib.util.module_from_spec(spec);spec.loader.exec_module(C)
V='/Volumes/'+'Disk'
H='/Users/'+'builder/repo'
T='~'+'/ab'
D='$'+'HOME/ab'
B='$'+'{HOME}/ab'
class Tree:
 def __init__(self, files): self.bodies=files;self.files=set(files)
 def read(self,path,*args): return self.bodies[path]
class MachinePaths(unittest.TestCase):
 def test_scope(self):
  for p in ['docs/plans/run.py','docs/verification/probe.sh','richos/app/validation/output.json']:
   self.assertFalse(in_scope(p))
  for p in ['docs/development/verification-input-qualifications.json','richos/engine/skills/tool/SKILL.md','richos/app/crates/a.rs']:
   self.assertTrue(in_scope(p))
 def test_patterns_comments_and_strings(self):
  self.assertEqual(matches(V+'/x '+H+' '+T+'/x '+D+'/x '+B+'/x'),Counter({V:1,H.rsplit('/',1)[0]:1,T:1,D:1,B:1}))
  self.assertFalse(matches('/Users/'+'you/repo'))
  self.assertFalse(matches('# '+H,'a.py'))
  self.assertEqual(matches('# '+V,'a.py'),{V:1})
  self.assertEqual(matches('"""'+H+'"""','a.py'),{H.rsplit('/',1)[0]:1})
  self.assertTrue(matches(H,'SKILL.md'))
  self.assertFalse(matches(D+'c/x'))
 def test_bare_roots_and_rust_assignment(self):
  home=H.rsplit('/',1)[0]
  self.assertEqual(matches('ROOT=Path("'+home+'") / "ab"'),{home:1})
  self.assertEqual(matches('cd '+T),{T:1})
  self.assertEqual(matches('*root = PathBuf::from("'+H+'");','a.rs'),{home:1})
 def test_local_reason(self):
  marker='machine-path-exempt: '
  self.assertFalse(matches(marker+'historical measurement\n'+V))
  self.assertTrue(matches(marker+'\n'+V))
  self.assertEqual(matches(marker+'fixture\n'+V+'\n'+V),{V:1})
 def check(self, files):
  C.FINDINGS.clear();C.check_machine_paths(Tree(files),'debt.json');return list(C.FINDINGS)
 def test_baseline_and_same_count_replacement(self):
  debt=json.dumps({'a.py':{V:1}})
  self.assertFalse(self.check({'debt.json':debt,'a.py':V}))
  self.assertTrue(self.check({'debt.json':debt,'a.py':V.replace('Disk','Other')}))
  self.assertTrue(self.check({'debt.json':debt,'a.py':''}))
 def test_scope_does_not_hide_live_config(self):
  self.assertFalse(self.check({'debt.json':'{}','docs/verification/run.py':V}))
  self.assertTrue(self.check({'debt.json':'{}','docs/development/live.json':V}))
 def test_new_skill_fails(self):
  self.assertTrue(self.check({'debt.json':'{}','richos/engine/skills/new/SKILL.md':V}))
 def test_baseline_cannot_grow(self):
  C.FINDINGS.clear()
  files={'debt.json':json.dumps({'a.py':{V:2}}),'a.py':V+' '+V}
  C.check_machine_paths(Tree(files),'debt.json',{'a.py':{V:1}})
  self.assertTrue(any(row[0]=='MACHINE-PATH-GROWTH' for row in C.FINDINGS))
  C.FINDINGS.clear()
  C.check_machine_paths(Tree(files),'debt.json',{})
  self.assertTrue(C.FINDINGS)
 def test_broken_debt_refuses(self):
  with self.assertRaises(C.Broken):self.check({'debt.json':'[]'})
  with self.assertRaises(C.Broken):self.check({'debt.json':'{"a":{"x":true}}'})
  with self.assertRaises(C.Broken):self.check({'a.py':''})
unittest.main(argv=['machine_paths'],verbosity=2)
PY
