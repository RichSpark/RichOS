#!/usr/bin/env python3
"""Recovery inspection and explicit applicability routing. No bypass command."""
import argparse
import json
import sys
import recovery as R


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    commands = ap.add_subparsers(dest='command', required=True)
    status = commands.add_parser('status')
    status.add_argument('--job', required=True)
    dispute = commands.add_parser('dispute')
    dispute.add_argument('--repo', required=True)
    dispute.add_argument('--work', required=True)
    dispute.add_argument('--tip', required=True)
    dispute.add_argument('--finding', action='append', required=True)
    args = ap.parse_args(argv)
    if args.command=='status':
        record = R.read(args.job)
        print(json.dumps(record, indent=2, sort_keys=True))
        return 0 if record else 1
    rows = [r for r in R.rows(R.F.review_ledger_default()) if r.get('work')==args.work
            and r.get('tip')==args.tip and R.F.review_row_belongs(r, R.F.review_repo_identity(args.repo))]
    findings = R.F.open_review_findings(rows)
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
