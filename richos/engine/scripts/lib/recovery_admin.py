#!/usr/bin/env python3
"""Recovery inspection and explicit applicability routing. No bypass command."""
import argparse
import json
import sys
import math
import subprocess
from pathlib import Path
import recovery as R


def report(job=None):
    paths = [R.root()/(job+'.json')] if job else sorted(R.root().glob('*.json'))
    if len(paths)>4096:
        raise R.Unavailable('report exceeds 4096 jobs; select --job')
    if sum(p.stat().st_size for p in paths if p.exists())>R.MAX_LEDGER_BYTES:
        raise R.Unavailable('report source exceeds 16 MiB; select --job')
    events=R.rows(R.root()/"events.jsonl")
    jobs=[]
    total=0.0
    missing=[]
    for path in paths:
        record=R.read(path.stem)
        if not record:
            missing.append(dict(job=path.stem,field='record'))
            continue
        consults=[]
        for c in record['consults']:
            cost=c.get('cost') or {}
            amount=cost.get('cost_usd',cost.get('total_cost_usd'))
            if isinstance(amount,(int,float)) and not isinstance(amount,bool) and math.isfinite(amount) and amount>=0:
                total+=amount
            else:
                amount=None
                missing.append(dict(job=record['id'],consult=c['id'],field='cost_usd'))
            if not cost.get('tokens'):missing.append(dict(job=record['id'],consult=c['id'],field='tokens'))
            elapsed=c.get('ended_at',0)-c['started_at'] if c.get('ended_at') else None
            if elapsed is None:missing.append(dict(job=record['id'],consult=c['id'],field='elapsed'))
            consults.append(dict(id=c['id'],role=c['role'],phase=c['phase'],status=c['status'],elapsed_seconds=elapsed,cost_usd=amount,tokens=cost.get('tokens'),audit=c.get('audit')))
        history=[e for e in events if e.get('job')==record['id']]
        if any('wes_candidates' not in e for e in history):missing.append(dict(job=record['id'],field='historical_firing_observations'))
        wes_fired=bool(record.get('wes_candidates') or any(e.get('wes_candidates') or e.get('status')=='reviewing_premise' for e in history) or record.get('launches',{}).get('wes'))
        observations=[dict(control=c['id'],worker=w,**o) for c in record['controls'] for w,o in c.get('observations',{}).items()]
        jobs.append(dict(id=record['id'],work=record['work'],repo=record['repo'],mode=record['mode'],status=record['status'],owner=record['owner'],
            trigger_failures=len(record['failed']),failed_recoveries=record.get('recovery_failures',0),
            axel_fired=bool(record.get('axel_required')),wes_fired=wes_fired,
            axel_scheduled=bool(record.get('launches',{}).get('axel')),
            first_control_seconds=min(c['at']-record['created_at'] for c in record['controls']) if record['controls'] else None,
            wes_findings=record.get('wes_candidates',[]),created_at=record['created_at'],deadline=record['deadline'],
            controls=observations,consultations=consults,plan=record.get('plan'),escalation=record.get('escalation'),
            gaps=[record[k] for k in ('supervision_error','wes_status') if record.get(k)]))
    return dict(schema=1,jobs=jobs,axel_jobs=sum(j['axel_fired'] for j in jobs),wes_jobs=sum(j['wes_fired'] for j in jobs),axel_scheduled_jobs=sum(j['axel_scheduled'] for j in jobs),
        known_cost_usd=total,cost_complete=not any(g['field']=='cost_usd' for g in missing),missing_data=missing,
        scope='Durable host records only. Empty or incomplete data does not prove successful native cancellation or semantic efficacy.')


def contract(entity, installed):
    declared=R.mode(entity)
    engine=Path(R.__file__).resolve().parents[2]
    paths=['recovery-profiles/axel.md','recovery-profiles/wes.md','scripts/lib/recovery.py','scripts/lib/recovery_runner.py','scripts/lib/recovery_controls.py',
        'scripts/lib/pierce.py','mega-lander/workspaces.py','scripts/app-engine-hook.py']
    fingerprints={p:R.F.file_digest(str(engine/p)) for p in paths}
    mismatches=[p for p,value in fingerprints.items() if not value or R.F.file_digest(str(Path(installed)/p))!=value]
    command=[sys.executable,'-B','-c','import json,sys;sys.path.insert(0,sys.argv[1]);import recovery;print(json.dumps(recovery.mode(sys.argv[2]),sort_keys=True))',str(Path(installed)/'scripts/lib'),entity]
    result=subprocess.run(command,capture_output=True,text=True,timeout=10)
    observed=json.loads(result.stdout) if result.returncode==0 else None
    if observed!=declared:mismatches.append('installed declaration/mode')
    return dict(schema=1,status='passed' if not mismatches else 'unavailable',declared=declared,installed=observed,
        source_pins=fingerprints,mismatches=mismatches,error=result.stderr if result.returncode else '',
        scope='Mode and source contract only. Supported native tool capabilities require separate witnesses.')


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    commands = ap.add_subparsers(dest='command', required=True)
    status = commands.add_parser('status')
    status.add_argument('--job', required=True)
    metrics = commands.add_parser('report')
    metrics.add_argument('--job')
    probe = commands.add_parser('contract')
    probe.add_argument('--entity', required=True)
    probe.add_argument('--installed-engine', required=True)
    dispute = commands.add_parser('dispute')
    dispute.add_argument('--repo', required=True)
    dispute.add_argument('--work', required=True)
    dispute.add_argument('--tip', required=True)
    dispute.add_argument('--finding', action='append', required=True)
    args = ap.parse_args(argv)
    if args.command=='report':
        print(json.dumps(report(args.job),indent=2,sort_keys=True))
        return 0
    if args.command=='contract':
        result=contract(args.entity,args.installed_engine)
        print(json.dumps(result,indent=2,sort_keys=True))
        return 0 if result['status']=='passed' else 2
    if args.command=='status':
        record = R.read(args.job)
        print(json.dumps(record, indent=2, sort_keys=True))
        return 0 if record else 1
    findings = R.source_findings(R.rows(R.F.review_ledger_default()), args.repo, args.work, args.tip)
    if not all(f in findings and findings[f].get('blocks') for f in args.finding):
        raise R.Unavailable('dispute must name a current exact blocking finding at this source')
    print(json.dumps(R.route_request(args.repo, args.work, args.tip, args.finding, 'explicit-dispute'), sort_keys=True))
    return 0


if __name__=='__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print('recovery: '+str(error), file=sys.stderr)
        raise SystemExit(2)
