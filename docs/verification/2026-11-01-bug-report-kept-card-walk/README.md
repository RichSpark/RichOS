# Bust a bug: a written-up card survives a relaunch and a change by Rich sends with one Send (2026-11-01)

The real-app check for the CEO's ruling §156: nothing in the bug-report path drops what the user
already did, asks him again or leaves him a step the app can do itself. Two kinds of extra work
were fixed (a written-up card kept only in the window; Send asked Rich again after a change Rich
himself made). It ran once, in the test VM, on fixture data only (an empty fixture home), against
`report-relay-stand-in.py` on the guest's own `127.0.0.1:8765`, whose test repository is
`richos-test/report-relay-local`. Nothing reached the real relay or the real repository and
nothing ran on the host's screen.

## Build identity

| What | Identity |
|---|---|
| Commit | `25707a0896c1efd337f37e1fe68b9816be571a5d` (this branch, both kinds committed) |
| Bundle | `testvm/branch-bundle.sh` output, `RichOS.zip`, sha256 `686157b77bc49d62bdfee5bfca39317e529667ca036f47ea10e76df7c1c2296c`, identity `com.richos.app 1.2.6`, ad-hoc signed, guest only |
| Engine | the v1.2.5 release's `richos-engine-1.2.0.tar.gz`, sha256 `80d40c80…4f9e30`, pinned into the bundle and handed to the guest with `--engine` |
| Guest | `walk-c599e00466f1`, 4 CPUs, 5120 MB |
| App environment | relaunched with `RICHOS_CLAUDE_BIN` = `fake-claude-bug-report.py` (Rich's write-up and his last pass), `RICHDOCTOR_REPORT_RELAY=http://127.0.0.1:8765`, `RICHDOCTOR_REPORT_REPOSITORY=richos-test/report-relay-local` |

## Command

```
run-walk.py --wait 1800 --bundle RichOS.zip --home <empty dir> \
  --engine richos-engine-1.2.0.tar.gz --report run-report.json -- \
  richos/app/scripts/testvm/steps-walk.py \
  --steps docs/verification/2026-11-01-bug-report-kept-card-walk/steps/walk.json.txt --out <out>
```

(The step list is kept as `.json.txt`, a record of what ran like the other `.json.txt` files here;
`steps-walk.py` reads it as the JSON it is.)

Run once. `run-report.json.txt`: boot 100 s, scenario 172 s, `scenario_exit` 0, `cleanup_complete`
true (app pid 1820 quit by its recorded pid, VM stopped, clone deleted, CEO §54), 275 s in all. All
41 steps exited 0 (`steps.json.txt`); the steps that assert (disk contents, counts of Rich's passes,
completions at the relay) end in a `test` that fails the step.

## What it showed

| Step | What the app did |
|---|---|
| answer typed in the Rich panel | the report card, "Not sent yet"; `bug-reports/unchecked/` holds exactly ONE file (the kept card) and nothing had reached the relay; Rich's last pass had run once |
| the app quit and started again (pid 1437 to 1820) | the panel opened on the same card, "Not sent yet" (`2-card-after-relaunch.jpg`, Send report / Change it / Cancel), with no new pass of Rich's (still 1) |
| "Also say it happens in the light theme too." | Rich: "Added that to What happened, above. It still waits for you to send it."; Rich's last passes: 2 (his change's own, after the scanner) |
| Send report, pressed once | "Sent · #412" (`4-sent-with-one-send.jpg`); Rich's last passes after Send: still 2, so Send asked him nothing; `sent.jsonl` holds number 412; waiting 0, kept cards 0; ONE completion at the relay |

Before this change a Send after a change by Rich ran a third pass of his (up to 90 s more) and could
bring the card back to be sent again; and a quit lost the card.

## What this does not cover

- A card the user changed by hand still goes through the scanner and then Rich at Send, by design
  (§115). The browser suite covers that path (`ui/tests/bug-report.js`); this walk did not repeat it.
- The fake `claude` answers Rich's passes by keeping the scanner's words, so this walk shows the
  number of passes and the card's survival, not the quality of a real Rich's rewrite.
- A Windows or non-test relay path is not applicable here: posting was pointed at the test
  repository.
