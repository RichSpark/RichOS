# Escalation: RichDoctor relay: the live screenshot test needs a one-day Cloudflare token that can also create the private R2 bucket

- id: `esc-20261022T153557Z-d8f38228`
- raised: 2026-10-22T15:35:57Z
- from: zach-opus-reportrelay1
- worktree: `/Users/alex/ab/richos-wt/zach-opus-reportrelay1` (branch `cc/zach-opus-reportrelay1`)
- head: `792866176e6b6de78ffdfa8a45f2f0807e8d834f`
- state: **proceeding**
- for: lead
- needs: **ceo-hands** (the answer needs the CEO at a device)

## The question

CEO, about two minutes in a browser. The current one-day token (cf-token.txt) can deploy the Worker and its database but not R2: creating the private screenshot bucket answered HTTP 403 (Cloudflare code 10000). (1) https://dash.cloudflare.com/profile/api-tokens : Create Token > Custom token > Get started. Name RichOS report relay test 2. Permissions: Account / Workers Scripts / Edit; Add more: Account / D1 / Edit; Add more: Account / Workers R2 Storage / Edit. Account Resources: Include / the RichOS account. TTL: end date tomorrow. Continue to summary > Create Token > copy it. (2) Open TextEdit, Format > Make Plain Text, paste the token, File > Save as cf-token-2.txt in /Volumes/E1TB/state/richos-report/credentials (the same private folder). That is all: the team creates the private test bucket richos-report-test (public access off), redeploys the test relay with screenshots and runs the live A9 screenshot steps (a maximum-size screenshot at 1 Mbit/s, a cut upload resumed, completion with every image, a link that shows nothing without staff access, retirement of an unseen ID) against the test repository only. The old token can then be deleted or left to expire. Later, for release and not for this test, Rich provisions the Cloudflare Access staff policy for report.richos.ceo and a named support engineer opens a real screenshot from an issue link (PRD v6 section 7).

## What was already tried

Everything that needs no new permission is built and green in this worktree: protocol 2 (register, upload, complete, retire), the shared PNG/JPEG profile with golden fixtures, the R2 storage written once per hash, staff-only reads that verify the Access JWT in the Worker, the per-receipt lock shared by upload, completion, retirement and cleanup, the hourly cleanup with crash resume, one-year retention with operator extension and the three handover clarifications. The workerd A9 test passes, including a 5 MiB screenshot uploaded at 1 Mbit/s in 42.5 s, a cut upload, a lost acknowledgment, a relay restart after GitHub creation, cleanup racing upload and completion and retirement. Creating the bucket with the current token: HTTP 403, code 10000.

## Proceeding meanwhile

Committing protocol 2, rerunning the proof selection and handing over. The live screenshot run starts as soon as cf-token-2.txt exists. The protocol 1 live A9 run already passed (18 of 18 steps, issues 16-18) against the test relay.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T153557Z-d8f38228`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T153557Z-d8f38228 --disposition "<what you decided or did>"
