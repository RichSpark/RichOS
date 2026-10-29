# Modifications to the bundled playwright-cli skill

Apache License 2.0, section 4(b): *"You must cause any modified files to carry
prominent notices stating that You changed the files."* This file is that
notice. It exists because the bundled `SKILL.md` is **not** byte-identical to
the upstream revision it came from and a reader deserves to know that before
they treat it as Microsoft's text.

## Provenance

| | |
|---|---|
| Upstream | <https://github.com/microsoft/playwright-cli>, `skills/playwright-cli` |
| Revision | `4fafcccadd265f959b41ee8bc87eff7cc607e8e7` (2026-02-06) |
| License | Apache-2.0, text in `LICENSE.txt` beside this file |
| Copyright | Microsoft Corporation |

## How that revision was established, since nothing recorded it

The bundled copy arrived with no note of where it came from. All twenty-six
upstream revisions of `skills/playwright-cli/SKILL.md` and all six of
`references/session-management.md` were fetched on 2026-09-04 and compared
against the bundled files. The result is not close:

- At `4fafcccadd26`, **all seven** bundled reference files are byte-identical
  to upstream and `SKILL.md` differs by one inserted section and nothing else.
- At the next revision, `1c9b20e0c617` (2026-02-14), 57 lines differ and
  `references/session-management.md` stops matching — upstream renamed the
  session flag from `-b` to `-s` there and the bundled copy still says `-b`.
- Every later revision is further away, monotonically, out to 230 differing
  lines against today's `main`.

So the bundled copy is upstream's 2026-02-06 text. That also settles a detail
the provenance audit left open: the differences it saw against later revisions
— an absent "Snapshots" section, an absent "Local installation" section, an
`## Open parameters` heading split into `### Install` and `### Configuration` —
are **upstream's own later edits**, not local ones. Nobody here deleted
anything.

## The change

**One change, in `SKILL.md` only.** A section titled "Headless on this Mac; a real
window only in the test VM" is inserted after the Quick start block and before
`## Commands`. It is present here and absent upstream at every revision. It is
reproduced here exactly as it appears in the bundled file, so that a reader can
see the whole of what was added without diffing anything:

<!-- markdownlint-disable -->

    ## Headless on this Mac; a real window only in the test VM

    On the user's own Mac the browser always runs headless (the default). A visible window would open
    in front of the person using the Mac, so RichOS refuses `--headed` there, along with every other
    way of opening a browser window from automation.

    When a task requires live visual verification in a real window, run it in the test VM, whose
    screen is the test screen:

    ```bash
    ssh <vm> 'playwright-cli open --headed --browser=chrome'
    ```

    Do not assume the browser is visibly open just because the command succeeded. Verify that a real browser window appeared in the VM.

    If no test VM is available, stop and report the blocker. Do not claim live visual verification from headless execution.

<!-- markdownlint-enable -->

That is RichOS doctrine, not Playwright's. It encodes two of this project's
rules: a visual verdict may never be claimed from a headless run (the same rule
the QA pipeline enforces elsewhere) and nothing an agent runs opens a window on
the user's own Mac, which the engine enforces itself
(`scripts/hooks/guard-headed-browser.sh` refuses `--headed` on the host). It
documents no behavior of the tool that upstream does not already document;
`--headed` and `--browser` are upstream flags.

History: the section was added to this project's source repository at commit
`88ee99d4c` (2026-04-01, per `docs/legal/SKILL-PROVENANCE-2026-09-04.md`,
roughly two months after the skill was vendored) under the title "Headed browser
requirement", where it told the agent to open a headed browser on whatever
machine it ran on. It was rewritten on 2026-10-29, when a headed browser opened
by an agent appeared on the user's screen; a real window now belongs to the test
VM. That source repository is not this one, so the first date is carried from
the provenance record rather than re-verified here; what **is** verified here is
that the section exists in no upstream revision.

**Nothing else differs.** Not the frontmatter, not the command reference, not
the examples and not one byte of any of the seven files under `references/`.

## Section 4(d), which is the other one people forget

Apache-2.0 section 4(d) obliges a redistributor to carry forward the contents of
an upstream `NOTICE` file, if the upstream distributes one.
**`microsoft/playwright-cli` distributes none** — no `NOTICE`, no `NOTICE.txt`,
no `COPYING`, neither at `4fafcccadd26` nor on `main` today. Both were listed
and checked on 2026-09-04. There is therefore nothing to carry forward and this
paragraph is the record that the obligation was examined rather than skipped.

## Line terminators and why they are not a modification

The provenance audit recorded that the copy **as acquired** had CRLF line
terminators — the signature of a downloaded package rather than hand-authored
text. The bundled files today are LF, which is upstream's own form, so the
normalization moved the files toward upstream rather than away from it. No file
differs from upstream by a line terminator now and the byte comparisons above
were run against the files as they stand.

`LICENSE.txt` is the one file where the arrow points the other way: upstream's
`LICENSE` is CRLF throughout, this repository stores LF and the bundled copy is
therefore LF. Not one character of the license text differs. Both hashes — the
CRLF form as upstream publishes it and the LF form as stored here — are recorded
at the top of `LICENSE.txt` so either comparison can be reproduced.

Verified 2026-09-04 against the upstream revision named above.
