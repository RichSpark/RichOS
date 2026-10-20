# How to write a RichOS changelog entry

`nightly-local.py` hands you these instructions when a nightly or a stable version has been published. Below them you get the version and every merge that landed for it, on main or on a release branch merged into main: each one's subject line names what landed and its body may add detail. Write the entry for that version.

## What you write back

Only the entry's lines, one per change and nothing else: no heading, no introduction, no closing remark, no code fence. The script adds the `## <version>` heading itself.

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
- Write only what the merges say. Never guess at a change they do not describe.

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
