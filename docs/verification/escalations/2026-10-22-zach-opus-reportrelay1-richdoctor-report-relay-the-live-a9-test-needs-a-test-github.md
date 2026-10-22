# Escalation: RichDoctor report relay: the live A9 test needs a test GitHub App and a one-day Cloudflare token from the CEO's accounts

- id: `esc-20261022T142030Z-c50b5c1c`
- raised: 2026-10-22T14:20:30Z
- from: zach-opus-reportrelay1
- worktree: `/Users/alex/ab/richos-wt/zach-opus-reportrelay1` (branch `cc/zach-opus-reportrelay1`)
- head: `edfcade6d5abdfe5068c6ada9320c54166ccdb2f`
- state: **proceeding**
- for: lead
- needs: **ceo-hands** (the answer needs the CEO at a device)

## The question

CEO, about ten minutes in a browser, nothing typed in Terminal. GITHUB: (1) https://github.com/new : owner WebDevBooster, name richos-report-test, Private, tick Add a README, Create. (2) https://github.com/WebDevBooster/richos-report-test/labels : New label, name richdoctor, Create label. (3) https://github.com/settings/apps/new : GitHub App name RichOS Reports Test; Homepage URL https://richos.ceo; under Webhook untick Active; Repository permissions > Issues: Read and write (leave everything else No access); Where can this GitHub App be installed: Only on this account; Create GitHub App. (4) On the App page that opens: note the App ID number; scroll to Private keys, Generate a private key (a .pem file downloads, leave it in Downloads). (5) Left menu Install App > Install on WebDevBooster > Only select repositories > richos-report-test > Install. CLOUDFLARE: (6) https://dash.cloudflare.com/profile/api-tokens : Create Token > Custom token > Get started. Name RichOS report relay test. Permissions: Account / Workers Scripts / Edit, then Add more: Account / D1 / Edit. Account Resources: Include / the RichOS account. TTL: end date tomorrow. Continue to summary > Create Token > copy it. (7) Open TextEdit, Format > Make Plain Text, paste the token, File > Save as cf-token in the folder /Volumes/E1TB/state/richos-report/credentials (already created, private). Then tell Rich the App ID. That is all: the team moves the .pem into that private folder, creates the test relay's own D1, deploys the test Worker on workers.dev (no DNS), runs the live A9 run against the test repository only, switches the test relay off and the token expires on its own. RICH (decision, no hands): PRD 7 says Rich names the operator and the initial global daily cap before the PRODUCTION relay is turned on; what are they? Default if unanswered: operator Rich, cap 50 reports per day. PRODUCTION, later and not needed for this test: a second App RichOS Reports installed only on WebDevBooster/richos, its label richdoctor and attaching report.richos.ceo to the Worker richos-report.

## What was already tried

Built and committed everything that does not need his accounts, on richos cc/zach-opus-reportrelay1: the relay Worker with its own D1 schema, the agreed client contract (richos/mobile/service/RICHDOCTOR-RELAY.md, accepted by echo-opus-richdoc1), the operator CLI including one-command create-database and deploy, the exact upload in workerd proving A9 under injected faults (client gone before and after acceptance, GitHub created then relay restarted, incomplete listings into operator review, refusal then Try again, cap, switch, maximum body) and the live A9 runner, which refuses a production profile and has itself been run against the local stand-in. Nothing was created on GitHub or Cloudflare, nothing touched Connect, nothing was sent to the production repository.

## Proceeding meanwhile

Finishing the branch: merge main, run the proof selection and hand over. The live A9 run is the one remaining step and starts from the restart pointer once the token file and the .pem exist.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T142030Z-c50b5c1c`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T142030Z-c50b5c1c --disposition "<what you decided or did>"
