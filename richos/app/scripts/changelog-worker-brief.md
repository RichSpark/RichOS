# Changelog worker: <TAG>

Prepare the notes for candidate <TAG> from the request at `<REQUEST>`. The candidate is not published. Write on your own branch only, then hand over to Rich. Never commit or push main, publish a build, touch the CEO's app or phones, launch an app or change the host display.

## Draft before publication

1. Read the request and `richos/app/scripts/changelog-instructions.md`. The request pins the candidate's source commit, build commit, channel and the previous successfully published release of that channel. Its `changes` field contains every commit in that exact range, including merges from release branches. Read more of that range when needed; never include changes after its source commit. An empty range is valid and uses the behind-the-scenes fallback.
2. Write the user-facing bullet lines to a file in your SSD scratch directory. Consolidate related commits and exclude internal work. A nightly compares with the previous published nightly; a stable compares with the previous published stable, covering all intervening nightlies. Do not copy only the promoted nightly's incremental entry.
3. In your work branch, run `python3 richos/app/scripts/nightly.py notes-draft --request <REQUEST> --entry <your-bullet-file>`. This writes the exact entry and `docs/release-notes/<TAG>.json`. It refuses stale requests and main. Do not edit the record by hand. A stale baseline means the lead regenerates the request against the current published channel and you revise the notes.
4. Inspect the diff. It must contain only the corresponding changelog and that candidate's record. Do not run application tests for prose. Commit the two files, merge local main into your branch once and hand over the branch, commit, candidate identity, baseline and complete entry.

## Lead: finish this job without involving the CEO

Start this worker as soon as the build completes, while QA walks it. Track the job through writing, independent second review, landing and publication. A usage hold delays this work and publication; it must never leave a public release without its notes. A dispatch error is unresolved work, not a successful release. The request and dispatch receipt survive in the nightly state directory outside the sealed candidate.

Keep the draft on its branch until QA accepts this exact candidate. If QA rejects it, discard that draft and close its escalation. Never land a rejected candidate's entry. The replacement gets a new request from the last successfully published release, so rejected changes are not lost or marked announced.

Once QA accepts the candidate, independently review the prose, its full source range and the exact build identity. Land and push the approved entry and record together through the ordinary gate. Only Rich lands main. Then run `nightly-local.py publish --run <the request's candidate run>` yourself. Publication reads remote main and refuses absent, mismatched or stale notes before creating a public tag or release. The GitHub release body carries the same approved prose from its first public appearance. Do not ask the CEO to approve changelog mechanics or remind you to resume.

A failed upload keeps the same candidate and approved notes. Retry publication; never rebuild, allocate another version, rewrite the body or rerun QA merely because an upload failed. If the candidate is deliberately abandoned after its notes landed, remove its prepared entry and record through the normal gate. Changelog entries link to the GitHub release, which is the authority on publication; a prepared entry alone does not claim publication. Confirm the release body and installable channel before closing the job.

After a stable is published, the publisher dispatches the separate next-version worker. That job writes no changelog.
