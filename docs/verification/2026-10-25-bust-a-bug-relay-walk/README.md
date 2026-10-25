# Bust a bug and Request a feature through RichOS's relay: the walk on the real app (2026-10-25)

The CEO, 2026-10-24: "So, if Bust a Bug was broken, then the feature suggestion is also broken.
Am I right?" Both now send through RichOS's reporting relay, with no token on the Mac. This is
the one real-app check of that work (`cc/echo-opus-bugrelay1`, tip `ba59286b0`), merged into
`cc/echo-opus-bugrelay2` as `727f24582`. It ran once, in the test VM, on fixture data only (an
empty fixture home), against `report-relay-stand-in.py` on the guest's own `127.0.0.1:8765`,
whose test repository is `richos-test/report-relay-local`. It never reached the real relay or
WebDevBooster/richos. Nothing ran on the host's screen.

## Build identity

| What | Identity |
|---|---|
| Commit the bundle and the engine were built from | `727f24582e06f74f0897f44b17887098f4d3d3ab` |
| Bundle | `testvm/branch-bundle.sh` output, `RichOS.zip`, sha256 `82b07472360ef05d12d80138b23f37dea71fca1b26c76024452f4da01b347ee1`, identity `com.richos.app 1.2.3`, ad-hoc signed, guest only |
| Engine | `make-engine-asset.sh --runtime-dir <nightly state>/runtime-eb026316eb08` (the runtime cache for `runtime-sources.json` sha256 `eb026316eb08…`), `richos-engine-1.2.0.tar.gz`, sha256 `16a00dcf764533aa1778a38937d7d0323b3609794323df9c7156b99c7a04b7bf`, pinned into the bundle and handed to the guest with `--engine` |
| Guest | `walk-91443896cd44`, 4 CPUs, 5120 MB (`run-report.json.txt`) |
| App environment | relaunched with `RICHOS_CLAUDE_BIN` = `fake-claude-bug-report.py` (Rich's write-up), `RICHDOCTOR_REPORT_RELAY=http://127.0.0.1:8765`, `RICHDOCTOR_REPORT_REPOSITORY=richos-test/report-relay-local` |

## Command

```
run-walk.py --wait 1200 --bundle RichOS.zip --home <empty dir> \
  --engine richos-engine-1.2.0.tar.gz --report run-report.json -- \
  richos/app/scripts/testvm/steps-walk.py \
  --steps docs/verification/2026-10-25-bust-a-bug-relay-walk/steps/walk.json --out <out>
```

Then, on the host, the relay's own checks on the pulled request log:

```
node richos/app/scripts/testvm/relay-log-check.mjs <out>/stand-in.log richdoctor-report/2 richos-idea/1
```

Run once. `run-report.json.txt`: boot 62 s, scenario 80 s, `scenario_exit` 0,
`cleanup_complete` true (app pid 1454 quit, the stand-in stopped by its recorded pid, VM stopped,
clone deleted, CEO §54), 145 s in all. All 46 steps exited 0 (`steps.json.txt`), including the
one marked `allow_fail`.

## What it showed

| Step | What the app did | Evidence |
|---|---|---|
| Settings → Bust a bug! | Rich asked "What went wrong? Tell me in your own words" | step 13 |
| the answer, typed in the Rich panel's own box | the report card, "Not sent yet", with one person's name left out and a stand-in in its place | step 16, `2-report-card.jpg` |
| before Send | nothing had reached the relay (`stand-in.log` empty) | step 23 |
| Send report | "Sent · #412"; Rich: "Sent. It's issue #412 on GitHub, filed from the RichOS reporting account…"; `bug-reports/sent.jsonl` holds number 412, url `https://github.com/richos-test/report-relay-local/issues/412`, account `reporting`; 0 waiting | steps 25 and 28, `3-report-sent.jpg` |
| Settings → Request a feature | Rich asked "What would you like RichOS to do?" | step 32 |
| the idea | the idea card, "Not posted yet", the same left-out line for GitHub Discussions | step 35, `4-idea-card.jpg` |
| Post idea | "Posted · in Ideas"; `feature-requests/sent.jsonl` holds number 413, url `https://github.com/richos-test/report-relay-local/discussions/413` | steps 39 and 42, `5-idea-posted.jpg` |

## The relay's side

`stand-in.log.txt` holds 4 requests, all from the app to the stand-in: `POST /v1/reports`
(`richdoctor-report/2`), `POST /v1/reports/RD-9W87-3XBN/complete`, `POST /v1/reports`
(`richos-idea/1`), `POST /v1/reports/RD-2WVX-RD9C/complete`. None carries an Authorization
header: no token was on the Mac. `relay.json.txt`: RD-9W87-3XBN `delivered` as issue 412 and
RD-2WVX-RD9C `delivered` as discussion 413, both in `richos-test/report-relay-local`.

`relay-checks.txt`: both registrations go through the real relay's own `checks.mjs`
(`checkCreate`, then `contentProblem` with the footer): the bug report as `richdoctor-report/2`
and the idea as `richos-idea/1`, each once, `ok`, exit 0.

So the answer to his question at this commit: neither is broken. Bust a bug files an issue and
Request a feature posts an idea, both through the relay, with no token on the Mac.

## Seen and not part of this job

- Under the test override, Rich's line (and the notice shown when the Mac cannot open the link) name
  `github.com/WebDevBooster/richos/issues/412` (and `/discussions/413`), while the relay's answer
  says `richos-test/report-relay-local`: the UI writes the repository into the text
  (`richos/app/ui/bug-report.js:1375`, `:1394`, `:1656`-`1657`) instead of reading the URL the
  relay returned. A user never sets the override and for every user the relay files in
  WebDevBooster/richos, so the link is right for them. Recorded as a follow-up.
- "Northwind Traders" stays in the filed words: in this fixture home it is not one of the user's
  companies (no company was added; the fake writer names it), so there is nothing for the app to
  replace. The company stand-in is `bug-report-walk.sh`'s case, not this walk's.
- The known follow-ups were left alone: the duplicate on a lost relay answer and ideas being
  refused by the deployed relay until it is redeployed.

## Files

`steps.json.txt` (every step, its exit and output; the host output directory written as `<out>`),
`run-report.json.txt` (the guest lock written as `<TESTVM_ROOT>`), `stand-in.log.txt`,
`relay.json.txt`, `relay-checks.txt` and four screenshots of the guest's screen, resized to 1200
px. Guest paths only.
