# How to write a RichOS changelog entry

The changelog worker (`changelog-worker-brief.md`, beside this file) follows these instructions before publication, from the candidate request. It establishes the net user-visible changes between the pinned published source and candidate source, including work merged through release branches. Its draft stays on its branch until QA accepts that candidate and Rich lands it. Publication requires the matching approved entry on remote main.

## What the entry is

The worker writes only the entry's bullet lines. `nightly.py notes-draft` adds the version heading and its GitHub release link and saves the candidate identity beside the changelog. Rich publishes immediately after landing and keeps the same job open through release verification or removal of an abandoned entry. No introduction, closing remark or code fence belongs in the entry.

Each line is `- ` followed by one of these words, then what changed as the user sees it:

- `Added` for something new the user can do or see
- `Changed` for something that works differently now
- `Improved` for something that works better or faster
- `Fixed` for something that was wrong and is now right

Something that should have worked and did not is a Fixed line, even when the merge says it "adds" or "makes" it work.

Put the lines in that order: every Added line first, then Changed, Improved and Fixed.

## How the lines read

- Plain American English, the way a non-technical user would say it to a friend. One sentence per line, no period at the end.
- In a list, no comma before the last "and" or "or": "Cut, Copy, Paste and Select All".
- Name things by what the user sees on screen: "Talk to Rich", "the Output panel", "Settings, Updates", "question cards", "the phone page".
- Never a file name, function, branch, agent or teammate name, commit id, review id, round number, follow-up number or escalation id.
- One line per change a user would notice. Several merges that fix the same thing are one line.
- Establish the release facts from the implementation at the request's source commit before writing. Commit subjects are an index to investigate, never evidence for a claim.
- Compare the previous published source with the candidate source. Read the net changes and the relevant callers, consumers and acceptance evidence, including what the user sees and which part of the app actually does it. A function name, a nearby comment or a passing test alone does not establish the entire behavior.
- For every user-visible change, establish what worked before, what works now, who can use it and the conditions or limitations. Distinguish what was requested from what was implemented. Do not silently turn "the team" into "Rich and Stu" or a test's behavior into shipped behavior.
- Account for the whole changed-file inventory before composing prose. Group related implementation files by the user-visible change. Record included changes, changes folded into another line and exclusions with their reasons. Include deletions and check that a change was not reverted before the candidate. Internal-only groups need no public line.
- Keep the source facts and coverage account beside the request, outside the repository and sealed candidate. For each public line, record the behavior it states and the implementation and acceptance evidence that establishes it. A file:line reference by itself is not proof of the wording.
- Resolve uncertainty by tracing the implementation. If the supplied request lacks the changed-file inventory, obtain it with git diff between its pinned sources; for a first release use git ls-tree at its source commit. Do not fill gaps from memory or present an unresolved product change as "nothing you would notice". A genuinely unresolved fact is unfinished work for Rich to resolve internally, not a guess to publish or a question for the CEO.

## What you leave out

A user never sees these, so they get no line:

- tests, test tooling, screenshots kept for tests and the test VM
- the review process and its findings
- building, signing, publishing and release tooling, including version bumps and this changelog itself
- the tooling that runs the team behind the product (the engine, guards, hooks, agent definitions)
- internal documentation and records

If nothing in the merges is something a user would notice, write exactly this one line:

- Changed nothing you would notice: this build carries behind-the-scenes work only

## Examples of good lines

- Added Dictation: choose a function key, then speak to type in any app on your Mac (Settings, Dictation)
- Added right-clicking a conversation in the sidebar to rename, pin or archive it
- Changed closing the window to keep Rich's work running; Quit tells you first when work would stop
- Improved Stop so it also ends a reply Rich is speaking
- Fixed a message typed while Rich was busy being dropped
- Fixed Escape not closing the Bust a bug and Request a feature panels
