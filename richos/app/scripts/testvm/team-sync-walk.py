#!/usr/bin/env python3
"""team-sync-walk.py — a change to a governed repository's `.claude/agents/<name>.md` reaches the
app's team at the next start, with nobody copying files (CEO 2026-11-02, "where and why only
there?"; `team_sync.rs`).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      team-sync-walk.py --out DIR --expect-sha SHA

It reuses hire-walk.py's class for the first run and the relaunch; only these are this walk's:
  seed     in the guest, the company folder (`~/Acme`) becomes a governed repository
           (`orchestration.config`, `.claude/agents/ray.md` and `kai.md`), a SECOND company
           (`Decoy`, listed before Acme in `entities.json`) owns another governed repository that
           also defines ray.md (review rv-20261102T050131Z-8c5e6fce-f873: a name defined in two
           repositories), and the app's team folder
           gets what his install had on 2026-11-02: a stale hand copy of ray.md (the old rule) and
           `mine.md`, a teammate made only in the app
  start    the app stopped and started again (a lease start runs the sync)
  synced   within --within seconds: team/ray.md carries Acme's new text (never the Decoy's, which
           comes first in company order) with the engine
           path rewritten for the app, team/kai.md was added, team/mine.md is byte-for-byte as it
           was, the lease's roster (`coordination/.claude/agents/ray.md`) carries the new text, and
           the stale copy is kept in `team-replaced/`
  edit     the repository's ray.md is changed again, in the guest, by a plain write
  restart  the app stopped and started again
  changed  team/ray.md carries the second text: a change in Acme's repository reaches the app
           although another company's repository defines the same name
  kept     a copy edited in the app after a sync is the user's: the repository changes again, the
           app restarts, and team/kai.md is still the user's edit
  rosters  (review rv-20261102T053802Z-c66f511d) a running lease's dispatch reads ITS OWN roster: the
           real lease's environment carries RICHOS_AGENT_ROSTER, the folder its plugin registers;
           then what a later lease for another company does to the folder every lease shares
           (`coordination/.claude/agents/ray.md` written with another model) is done in the guest,
           and the shipped engine's own `job_model('ray')`, run with that lease's real
           environment, still answers the lease's model, not the other company's; and the same for
           every other reader of the roster: guard-pierce's instruction text, the shell guards'
           resolver, and the Stop hooks' role lists (review rv-20261102T062434Z-a3c7a28e)

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
"""
import argparse
import importlib.util
import json
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('hire_walk', HERE / 'hire-walk.py')
hire_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(hire_walk)
_env_spec = importlib.util.spec_from_file_location('lease_env_walk', HERE / 'lease-env-walk.py')
lease_env_walk = importlib.util.module_from_spec(_env_spec)
_env_spec.loader.exec_module(lease_env_walk)
StepFailed = hire_walk.StepFailed

STEPS = ['identity', 'first-run', 'stock', 'seed', 'start', 'synced', 'edit', 'restart', 'changed', 'kept', 'rosters']
OLD, NEW1, NEW2, USER_EDIT = 'WALK-OLD-RULE wait for proceed', 'WALK-NEW-RULE-ONE start at once', 'WALK-NEW-RULE-TWO', 'WALK-USER-EDIT'
DECOY = 'WALK-DECOY-RULE from the other company'
ESCALATE = '~/.claude/richos-engine/scripts/escalate.sh raise --fields esc.json'


def definition(name, rule, escalate=ESCALATE):
    return (f'---\nname: {name}\ndescription: {name}, a fixture teammate of team-sync-walk.py\nmodel: sonnet\n---\n'
            f'# {name}\n\n{rule}\n\n{escalate}\n')


class TeamSyncWalk(hire_walk.HireWalk):
    def put(self, path, text):
        guest(self.vm, 'mkdir -p ' + shlex.quote(str(Path(path).parent)) + ' && printf %s ' + shlex.quote(text)
              + ' > ' + shlex.quote(path))

    def read(self, path):
        return guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true', 60)

    def team_file(self, name):
        return self.read(f'{self.data}/team/{name}.md')

    def seed(self):
        agents = self.company + '/.claude/agents'
        self.put(self.company + '/orchestration.config', 'SESSION_TEAMS_DIR=none\n')
        self.put(agents + '/ray.md', definition('ray', NEW1))
        self.put(agents + '/kai.md', definition('kai', NEW1))
        # A second company, first in company order, whose governed repository also defines ray.
        decoy = str(Path(self.company).parent / 'Decoy')
        self.put(decoy + '/orchestration.config', 'SESSION_TEAMS_DIR=none\n')
        self.put(decoy + '/.claude/agents/ray.md', definition('ray', DECOY))
        registry = f'{self.data}/entities.json'
        guest(self.vm, '/usr/bin/python3 -c ' + shlex.quote(
            'import json,sys;p=sys.argv[1];d=json.load(open(p));'
            'd["entities"].insert(0,{"id":"decoy","display_name":"Decoy","status":"active","roots":[sys.argv[2]]});'
            'json.dump(d,open(p,"w"))') + ' ' + shlex.quote(registry) + ' ' + shlex.quote(decoy), 60)
        self.put(f'{self.data}/team/ray.md', definition('ray', OLD, '"$RICHOS_ENGINE_ROOT/scripts/escalate.sh" raise'))
        self.put(f'{self.data}/team/mine.md', definition('mine', 'made in the app only'))
        self.facts['mine'] = self.team_file('mine')
        self.save()
        self.facts['decoy'] = decoy
        self.save()
        return {'governed': self.company, 'decoy': decoy, 'stale': 'team/ray.md', 'app_only': 'team/mine.md'}

    def start(self):
        return {'relaunched': self.relaunch()}

    def wait_text(self, name, marker, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if marker in self.team_file(name):
                return True
            time.sleep(3)
        return False

    def synced(self):
        if not self.wait_text('ray', NEW1, self.a.within):
            raise StepFailed('team/ray.md never carried the repository\'s new text within %d s: %r' % (
                self.a.within, self.team_file('ray')[:400]))
        ray, kai = self.team_file('ray'), self.team_file('kai')
        roster = self.read(f'{self.data}/coordination/.claude/agents/ray.md')
        kept = guest(self.vm, 'cat ' + shlex.quote(self.data) + '/team-replaced/ray.*.md 2>/dev/null || true', 60)
        evidence = {'ray': ray, 'kai_added': NEW1 in kai, 'roster_has_new_text': NEW1 in roster, 'stale_kept': OLD in kept,
                    'mine_untouched': self.team_file('mine') == self.facts['mine'],
                    'decoy_company_still_listed_first': '"decoy"' in self.read(f'{self.data}/entities.json').replace(' ', ''),
                    'decoy_rule_not_in_team': DECOY not in ray}
        (self.out / 'synced-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        problems = []
        if '"$RICHOS_ENGINE_ROOT/scripts/escalate.sh"' not in ray or '~/.claude/richos-engine' in ray:
            problems.append('the engine path was not rewritten for the app')
        problems += [k for k in ('kai_added', 'roster_has_new_text', 'stale_kept', 'mine_untouched', 'decoy_company_still_listed_first', 'decoy_rule_not_in_team') if not evidence[k]]
        if OLD in ray:
            problems.append('the stale rule is still in team/ray.md')
        if problems:
            raise StepFailed('; '.join(problems))
        return evidence

    def edit(self):
        self.put(self.company + '/.claude/agents/ray.md', definition('ray', NEW2))
        return {'ray': 'changed in the guest repository'}

    def restart(self):
        return {'relaunched': self.relaunch()}

    def changed(self):
        if not self.wait_text('ray', NEW2, self.a.within):
            raise StepFailed('team/ray.md never carried the second text within %d s: %r' % (self.a.within, self.team_file('ray')[:400]))
        return {'ray_has_second_text': True, 'first_text_gone': NEW1 not in self.team_file('ray'),
                'decoy_rule_absent': DECOY not in self.team_file('ray')}

    def kept(self):
        self.put(f'{self.data}/team/kai.md', definition('kai', USER_EDIT, '"$RICHOS_ENGINE_ROOT/scripts/escalate.sh" raise'))
        self.put(self.company + '/.claude/agents/kai.md', definition('kai', NEW2))
        self.relaunch()
        time.sleep(30)
        kai = self.team_file('kai')
        if USER_EDIT not in kai or NEW2 in kai:
            raise StepFailed('an edit made in the app was overwritten: %r' % kai[:300])
        return {'kai_is_the_users_edit': True}

    def rosters(self):
        end, leases = time.monotonic() + self.a.within, []
        while time.monotonic() < end and not leases:
            leases = [l for l in self.leases() if l.get('plugin')]
            if not leases:
                time.sleep(3)
        if not leases:
            raise StepFailed('no running lease was found in the guest within %d s' % self.a.within)
        lease = leases[0]
        env = {}
        for line in lease_env_walk.process_env(self.vm, lease['pid']).splitlines():
            key, _, value = line.partition('=')
            env[key] = value
        roster, entity, engine = env.get('RICHOS_AGENT_ROSTER'), env.get('RICHOS_ENTITY_ROOT'), env.get('RICHOS_ENGINE_ROOT')
        evidence = {'lease_plugin': lease['plugin'], 'roster_env': roster, 'entity_root': entity}
        if roster != lease['plugin'] + '/agents' or not entity or not engine:
            raise StepFailed("the lease's environment does not name its own roster: %r" % evidence)
        own = front_model(self.read(roster + '/ray.md'))
        # What a lease started later, for another company, renders into the shared folder.
        other = 'opus' if own != 'opus' else 'sonnet'
        shared = entity + '/.claude/agents/ray.md'
        self.put(shared, definition('ray', DECOY).replace('model: sonnet', 'model: ' + other))
        script = ('import importlib.util,sys\n'
                  's=importlib.util.spec_from_file_location("app",sys.argv[1]);m=importlib.util.module_from_spec(s);'
                  's.loader.exec_module(m)\nprint(m.job_model("ray",None))')
        command = (f'RICHOS_ENTITY_ROOT={shlex.quote(entity)} RICHOS_AGENT_ROSTER={shlex.quote(roster)} '
                   f'python3 -c {shlex.quote(script)} {shlex.quote(engine + "/mega-lander/app.py")} 2>&1')
        answered = guest(self.vm, command, 120).strip().splitlines()[-1:] or ['']
        evidence.update({'own_model': own, 'other_company_model_in_shared_folder': other, 'dispatch_model': answered[0]})
        # The other readers of the roster (review rv-20261102T062434Z-a3c7a28e): guard-pierce's
        # instruction text, the shell guards' resolver and the Stop hooks' role lists, each run with
        # the lease's real environment while the shared folder holds the other company's text.
        own_text, setenv = self.read(roster + '/ray.md'), (
            f'RICHOS_ENTITY_ROOT={shlex.quote(entity)} RICHOS_AGENT_ROSTER={shlex.quote(roster)} '
            f'CLAUDE_PROJECT_DIR={shlex.quote(entity)} RICHOS_APP_STATE=/tmp/team-sync-walk-state ')
        pierce_script = ('import sys\nsys.path.insert(0,sys.argv[1])\nimport pierce\n'
                         'print(pierce.instruction_text({"tool_input":{"subagent_type":"richos-app-engine:ray","prompt":"x"},'
                         '"cwd":sys.argv[2]}))')
        pierce = guest(self.vm, f'{setenv}python3 -c {shlex.quote(pierce_script)} '
                       f'{shlex.quote(engine + "/scripts/lib")} {shlex.quote(entity)} 2>&1', 120)
        resolve_script = '. "$1/scripts/lib/resolve-roots.sh"; resolve_agent_def "$2" "$1" richos-app-engine:ray; agent_roster_dir "$2"'
        resolved = guest(self.vm, setenv + 'bash -c ' + shlex.quote(resolve_script)
                         + f' x {shlex.quote(engine)} {shlex.quote(entity)} 2>&1', 60).strip().splitlines()
        own_roster = '/tmp/team-sync-walk-roster'
        self.put(own_roster + '/onlyroster.md', definition('onlyroster', 'x'))
        roles = {}
        for label, rel in (('ass-kicker', 'ass-kicker/ass-kicker.py'), ('guard-unresolved-claims', 'scripts/hooks/guard-unresolved-claims.py')):
            roles_script = ('import importlib.util,sys\n'
                            's=importlib.util.spec_from_file_location("m",sys.argv[1]);m=importlib.util.module_from_spec(s);'
                            's.loader.exec_module(m)\nprint(" ".join(sorted(m.roster_roles(sys.argv[2],None,None))))')
            roles[label] = guest(self.vm, f'RICHOS_AGENT_ROSTER={own_roster} python3 -c {shlex.quote(roles_script)} '
                                 f'{shlex.quote(engine + "/" + rel)} {shlex.quote(entity)} 2>&1', 60).strip()
        evidence.update({'pierce_has_own_text': own_text.strip() in pierce, 'pierce_has_other_text': DECOY in pierce,
                         'resolver_and_roster_dir': resolved, 'role_lists': roles})
        (self.out / 'rosters-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if DECOY not in self.read(shared):
            raise StepFailed('the shared folder was not changed, so this step proved nothing')
        if answered[0] != own:
            raise StepFailed("the dispatch ran on %r, not the lease's own %r" % (answered[0], own))
        if own_text.strip() not in pierce or DECOY in pierce:
            raise StepFailed("Pierce inspected the assignment against another definition than the lease's own: %r" % pierce[:400])
        if resolved != [roster + '/ray.md', roster]:
            raise StepFailed("the shell guards' resolver did not take the lease's roster: %r" % resolved)
        if set(roles.values()) != {'onlyroster'}:
            raise StepFailed("a Stop hook's role list did not come from the lease's roster: %r" % roles)
        return evidence


def front_model(text):
    for line in text.split('\n')[1:]:
        if line.strip() == '---':
            break
        if line.lower().startswith('model:'):
            return line.partition(':')[2].strip()
    return ''


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=180, help='seconds for the app to bring its team up to date')
    p.add_argument('--relaunch-within', type=float, default=120, help='seconds for the composer to come back')
    p.add_argument('--settle-within', type=float, default=300)
    p.add_argument('--after-relaunch-deadline', type=float, default=60)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = TeamSyncWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError, KeyError) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
