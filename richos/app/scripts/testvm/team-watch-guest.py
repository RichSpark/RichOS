#!/usr/bin/env python3
"""team-watch-guest.py — inside a test VM guest: his team's fixture, switched on for the app.

Run by team-watch-walk.py over ssh, with the guest's /usr/bin/python3 (3.9), never on a host:

  team-watch-guest.py setup  PAYLOAD   the fixture and its declaration (prints one JSON line)
  team-watch-guest.py ledger PAYLOAD   the escalation ledger path his engine reads

`setup` builds the operator probes' own fixture (operator-probes/guest_probes.py setup_fixture,
pushed beside this file): a fixture entity at <home>/ab/femcboost, his engine from
probes/engine.tar as a directory marketplace and installed as the plugin, onboarding and trust
accepted. Then, as the W2 walk's fixture does (w2_fixture, step 0), and only that: the land
fences declared on and installed in the entity, the stored environment enable.sh would write
(the guest's login allowlist, the harness's claude pin, the guest's claude first on PATH), a
fixture contract, and the declaration, written to the app's own data folder (<home>/Library/
Application Support/com.richos.app/operator.json), which the app reads at its next launch.
Two additions for this walk: QUOTA_PAUSE_PERCENT=1 in the entity's orchestration.config, so
quota-watch's threshold is crossed at the first reading, and empty Codex channels, so
codex-watch's first look settles where they end. The leads run on Sonnet (his settings decide
the model; the fixture's settings.json says sonnet, as W2's does).

Everything is in the guest's payload; nothing here touches a host.
"""
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import guest_probes as g  # noqa: E402


def setup(payload):
    p = g.Paths(payload)
    for d in (p.results, p.work):
        d.mkdir(parents=True, exist_ok=True)
    record = {}
    g.setup_fixture(p, record)
    ctx = g.Context(p, 60)
    settings = json.loads((p.claude_dir / 'settings.json').read_text())
    settings['model'] = g.PROBE_MODEL
    (p.claude_dir / 'settings.json').write_text(json.dumps(settings, indent=1) + '\n')
    config = p.entity / 'orchestration.config'
    config.write_text(config.read_text() + 'OPERATOR_FENCES="on"\nOPERATOR_FENCES_REPOS="%s"\n'
                      'QUOTA_PAUSE_PERCENT=1\n' % p.entity)
    g.git(p.entity, 'add', 'orchestration.config')
    g.git(p.entity, 'commit', '-q', '-m', 'Team-watch walk fixture: the operator switch on, threshold 1')
    g.git(p.entity, 'push', '-q', 'origin', 'main')
    home_env = dict(ctx.stored, HOME=str(p.home), **g.HARNESS_PIN)
    fences = p.engine / 'scripts' / 'operator-fences.sh'
    record['fence_install'] = g.run(['/bin/bash', str(fences), 'install', '--entity', str(p.entity)], env=home_env,
                                    cwd=str(p.entity), timeout=120)[:2]
    record['fence_on'] = g.run(['/bin/bash', str(fences), 'on', '--entity', str(p.entity)], env=home_env,
                               cwd=str(p.entity), timeout=120)[:2]
    record['fence_status'] = g.run(['/bin/bash', str(fences), 'status', '--entity', str(p.entity)], env=home_env,
                                   cwd=str(p.entity), timeout=120)[:2]
    stored = dict(ctx.stored, **g.HARNESS_PIN)
    stored['PATH'] = '/Users/admin/.local/bin:' + stored.get('PATH', '/usr/bin:/bin')
    decl = {'schema': 1, 'entity_root': str(p.entity), 'engine_root': str(p.engine), 'home': str(p.home),
            'claim': {'file': str(p.claude_dir / 'state' / 'operator-lead.json'),
                      'lock': str(p.claude_dir / 'state' / 'operator-lead.lock')},
            'permission_mode': 'bypassPermissions', 'environment': stored,
            'origins': ['desk-typed', 'desk-voice', 'desk-file'], 'file_roots': [str(p.ab)],
            'contract': {'path': str(p.contract), 'sha256': hashlib.sha256(p.contract.read_bytes()).hexdigest()}}
    data = p.home / 'Library' / 'Application Support' / 'com.richos.app'
    data.mkdir(parents=True, exist_ok=True)
    (data / 'operator.json').write_text(json.dumps(decl, indent=1) + '\n')
    channels = []
    for name in ('rich-codex', 'rich-codex-questions'):
        channel = p.home / '.richos-coordination' / name / 'to-rich.md'
        channel.parent.mkdir(parents=True, exist_ok=True)
        channel.touch()
        channels.append(str(channel))
    record.update({'entity': str(p.entity), 'engine': str(p.engine), 'home': str(p.home), 'data': str(data),
                   'channels': channels, 'environment_names': sorted(stored)})
    print(json.dumps(record))


def teammate(payload, session):
    """One working teammate of the lead of `session`, registered as spawn.sh registers one
    (workspaces.new_record, written where every watcher's registry reads, under the declaration's
    own environment): quota-watch pauses only somebody working, so at the threshold with nobody
    working it rightly says nothing (run 7, 2026-10-20: "at the threshold with nothing working").
    Prints the record's path."""
    p = g.Paths(payload)
    decl = json.loads((p.home / 'Library' / 'Application Support' / 'com.richos.app' / 'operator.json').read_text())
    env = dict(decl['environment'], HOME=decl['home'])
    # The declaration's own PATH picks python3, as it does for the watchers that read the record.
    code, out, err = g.run(['python3', '-c',
                            'import sys; sys.path.insert(0, sys.argv[1]); import workspaces as w; '
                            'w._ensure_dirs(); key = sys.argv[2] + "--fixture-mate"; '
                            'w.write_json(w.agent_path(key), w.new_record(key, name="fixture-mate", session_id=sys.argv[2])); '
                            'print(w.agent_path(key))', str(p.engine / 'mega-lander'), session],
                           env=env, timeout=60)
    if code != 0:
        raise SystemExit('the teammate could not be registered: ' + err)
    print(out.strip())


def ledger(payload):
    p = g.Paths(payload)
    code, out, err = g.run(['/usr/bin/python3', '-c', 'import sys; sys.path.insert(0, sys.argv[1]); import escalations; '
                            'print(escalations.ledger_path())', str(p.engine / 'scripts' / 'lib')],
                           env={'HOME': str(p.home), 'PATH': '/usr/bin:/bin'}, timeout=60)
    if code != 0:
        raise SystemExit('the ledger path could not be read: ' + err)
    print(out.strip())


if __name__ == '__main__':
    {'setup': setup, 'ledger': ledger, 'teammate': teammate}[sys.argv[1]](*sys.argv[2:])
