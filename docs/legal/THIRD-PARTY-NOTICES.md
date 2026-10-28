# Third-party notices

RichOS bundles work by other authors. Each piece stays under the license it
arrived with and none of it becomes AGPL by sitting in this repository. This
file is the inventory: what is bundled, where it came from, which revision or
version, who holds the copyright and where the full terms live.

**Last verified 2026-09-04.** Every provenance claim below was checked against
the named upstream on that date rather than carried forward from a previous
document. Where a check could not be completed, this file says so instead of
rounding up.

**Nine skills were added to this table on 2026-09-04**, on the CEO's express
exception to his own standing rule that a skill carrying no license gets none
added. They are third-party, they were established as such by
`docs/legal/SKILL-PROVENANCE-2026-09-04.md` and until that day they carried no
license file at all. Each now carries one, taken from its upstream rather than
from a template. The other twelve skills in `richos/engine/skills/` are RichOS-authored
and stay bare, which is correct and deliberate.

The RichOS-authored software around all of this is AGPL-3.0-only — see
`docs/legal/LICENSING.md`. The RichOS brand material is excluded from both —
see `docs/legal/BRAND-ASSETS.md`.

## How to read the "Verified" column

- **byte-identical** — every bundled file was fetched from the named upstream
  revision and compared byte for byte on the date above.
- **modified** — the bundled copy differs from upstream and the difference is
  written down in the directory alongside the work.
- **by version** — the upstream project publishes versioned skills rather than
  a stable path history, so the bundled copy is identified by the version it
  declares in its own frontmatter. The body was compared to the nearest
  upstream revision and the differences are stated in the row.

## Bundled skills and tools — redistributed inside the engine

| Work | Upstream | Revision / version | License | Copyright | Full terms | Verified |
|---|---|---|---|---|---|---|
| `richos/engine/skills/frontend-design` | anthropics/skills, `skills/frontend-design` | `00756142ab04c82a447693cf373c4e0c554d1005` (2025-12-04) | Apache-2.0 | Anthropic | `richos/engine/skills/frontend-design/LICENSE.txt` | modified — see `richos/engine/skills/frontend-design/MODIFICATIONS.md` |
| `richos/engine/skills/android-emulator-qa` | openai/plugins, `plugins/test-android-apps/skills/android-emulator-qa` | plugin v0.1.2, `42e54808e826d2002f73d85d6538c144dd805059` (2026-03-25) | MIT | OpenAI | `richos/engine/skills/android-emulator-qa/LICENSE.txt` | byte-identical (all 3 files) |
| `richos/engine/skills/landing-page-taste` | Leonxlnx/taste-skill, `skills/taste-skill` | `72e299530e2eb31ed8da06181bc19f6c18a00821` | MIT | 2026 Leonxlnx | `richos/engine/skills/landing-page-taste/LICENSE` | pinned in `SKILL.md`, below a scope pin |
| `richos/engine/skills/landing-page-redesign` | Leonxlnx/taste-skill, `skills/redesign-skill` | `72e299530e2eb31ed8da06181bc19f6c18a00821` | MIT | 2026 Leonxlnx | `richos/engine/skills/landing-page-redesign/LICENSE` | pinned in `SKILL.md`, below a scope pin |
| `richos/engine/skills/marketing-strategy-pmm` | alirezarezvani/claude-skills, `marketing-skill/skills/marketing-strategy-pmm` | version 1.0.0 (frontmatter, updated 2025-10-20) | MIT | 2025 Alireza Rezvani | `richos/engine/skills/marketing-strategy-pmm/LICENSE` | by version — see the note below |
| `richos/engine/skills/resend` | resend/resend-skills, `skills/resend` | version 3.5.0 (frontmatter) | MIT | 2026 Resend | `richos/engine/skills/resend/LICENSE` | by version — see the note below |
| `richos/engine/skills/copywriting` | coreyhaines31/marketingskills, `skills/copywriting` | `68f5eaf64e858438db47e436d7a3bef0e9d69721` (2026-03-04) | MIT | 2025 Corey Haines | `richos/engine/skills/copywriting/LICENSE` | modified — 3 of 4 files byte-identical; the fourth differs in 4 lines of spelling, see the note below |
| `richos/engine/skills/customer-research` | coreyhaines31/marketingskills, `skills/customer-research` | `cc1a9c106b1030ae2994167d9160bc51e54e8d26` (2026-03-28) | MIT | 2025 Corey Haines | `richos/engine/skills/customer-research/LICENSE` | modified — 1 of 3 files byte-identical; the other two differ only in indentation and a trailing newline, see the note below |
| `richos/engine/skills/marketing-psychology` | coreyhaines31/marketingskills, `skills/marketing-psychology` | `68f5eaf64e858438db47e436d7a3bef0e9d69721` (2026-03-04) | MIT | 2025 Corey Haines | `richos/engine/skills/marketing-psychology/LICENSE` | byte-identical (both files) |
| `richos/engine/skills/product-marketing-context` | coreyhaines31/marketingskills, `skills/product-marketing-context` | `68f5eaf64e858438db47e436d7a3bef0e9d69721` (2026-03-04) | MIT | 2025 Corey Haines | `richos/engine/skills/product-marketing-context/LICENSE` | byte-identical (both files) |
| `richos/engine/skills/playwright-cli` | microsoft/playwright-cli, `skills/playwright-cli` | `4fafcccadd265f959b41ee8bc87eff7cc607e8e7` (2026-02-06) | Apache-2.0 | Microsoft Corporation | `richos/engine/skills/playwright-cli/LICENSE.txt` | modified — see `richos/engine/skills/playwright-cli/MODIFICATIONS.md` |
| `richos/engine/skills/playwright-e2e-testing` | bobmatnyc/claude-mpm-skills, `toolchains/javascript/testing/playwright` | `a0632b527e53d377e186e16b3b8edb8bc2892a98` (2025-12-31) | MIT | 2025 Claude MPM Contributors | `richos/engine/skills/playwright-e2e-testing/LICENSE` | modified — one line of the single bundled file, see the note below |
| `richos/engine/skills/svelte-code-writer` | sveltejs/ai-tools, `plugins/claude/svelte/skills/svelte-code-writer` | `0e55ee792d3d72fc413162ec6442a0c436ff1294` (2026-03-12) | MIT | 2026 Svelte Contributors | `richos/engine/skills/svelte-code-writer/LICENSE` | byte-identical (1 file) — but the license text is dated after the copy, see the note below |
| `richos/engine/skills/svelte-core-bestpractices` | sveltejs/ai-tools, `plugins/claude/svelte/skills/svelte-core-bestpractices` | `0e55ee792d3d72fc413162ec6442a0c436ff1294` (2026-03-12) | MIT | 2026 Svelte Contributors | `richos/engine/skills/svelte-core-bestpractices/LICENSE` | byte-identical (all 10 files) — but the license text is dated after the copy, see the note below |
| `richos/engine/skills/use-railway` | railwayapp/railway-skills, `plugins/railway/skills/use-railway` | `1b59fd592cd4413f3c325f139bf3f3afa99ee095` (2026-07-09), declaring skill version 1.3.5; `SKILL.md` plus all 13 `references/` files | MIT | 2026 Railway Corporation | `richos/engine/skills/use-railway/LICENSE` | modified — 24 lines of roughly 600, enumerated in that file |
| `richos/engine/tools/gpt-exporter` | WebDevBooster/gpt-exporter | vendored copy | MIT | 2026 Alex Booster | `richos/engine/tools/gpt-exporter/LICENSE` | the repository owner's own separately published project |

### Notes on the rows that are not byte-identical

**`frontend-design`.** One word differs from the pinned upstream revision:
`retro-futuristic,` is absent from the bundled `SKILL.md` line 15. Apache-2.0
section 4(b) requires a modified file to carry a notice that it was changed,
and `MODIFICATIONS.md` in that directory is that notice. Its `LICENSE.txt` was
missing entirely until 2026-09-04 while the skill's own frontmatter said
"Complete terms in LICENSE.txt" — a citation pointing at nothing. The file is
now present and byte-identical to upstream.

**`android-emulator-qa`.** This directory previously carried the **Apache
License 2.0**, which nobody upstream ever granted. `openai/plugins` publishes
no LICENSE file at all; the grant of record is the upstream plugin manifest
`plugins/test-android-apps/.codex-plugin/plugin.json`, which declares
`"license": "MIT"` with author `OpenAI`. The bundled `SKILL.md`,
`scripts/ui_pick.py` and `scripts/ui_tree_summarize.py` were each fetched from
the pinned revision and compared: all three byte-identical. The license file
now carries the MIT terms and records where the claim was read from.

**`marketing-strategy-pmm`.** The bundled `SKILL.md` is a single 38 KB file.
Upstream ships a 12 KB `SKILL.md` plus four files under `references/`
(`international-gtm.md`, `launch-checklists.md`, `messaging-templates.md`,
`positioning-frameworks.md`), so the bundled copy is a consolidated form of the
upstream skill rather than a byte copy of any one file. MIT permits that and
requires only that the copyright notice and permission notice travel with it,
which the restored `LICENSE` does. The directory carried **no** MIT notice at
all before 2026-09-04.

**`resend`.** The bundled copy declares version 3.5.0; upstream is now 3.7.0.
Compared against the upstream revision that also declared 3.5.0
(`3553a52065b48240c7786332fa431d4cd6f39d36`), the bundled body differs in one
line — the HTTP 429 row of the error table, where the bundled copy carries the
*newer* rate-limit wording — and in the frontmatter `metadata` block, which
upstream has since extended. The bundled copy is therefore a 3.5.0-era release
taken through a distribution channel other than that commit and is recorded by
version. The directory carried no MIT notice before 2026-09-04.

**`copywriting`.** Three of the four bundled files are byte-identical to the
pinned revision. The fourth, `references/natural-transitions.md`, differs in four
lines and in nothing but spelling: two headings carry the British `-ising` form
upstream and the American `-izing` form here, each appearing twice. The cause is
in this repository's own history — commit
`06f4a8221a61500b90790383c8359d931391fe4f` (2026-08-30), "docs(prose): US
spelling in richos/engine/scripts, richos/engine/skills and root repo comments". **The
spellings were left as they are.** MIT does not require a modified copy to
disclose the change and reverting an edit to make a table say "byte-identical"
would be the tail wagging the dog. The row says "modified" because it is.

**`customer-research`.** Word for word identical to the pinned revision; nothing
a reader would call a difference is a word. `SKILL.md` has eight lines whose
leading indentation is four spaces here and three upstream and
`evals/evals.json` is one byte shorter because the bundled copy has no trailing
newline. `references/source-guides.md` is byte-identical. It is pinned to a
different revision from its three siblings because it did not exist upstream at
theirs: it was created upstream on 2026-03-28 at 06:32 UTC and reached this
project's source repository at 10:53 UTC the same morning.

**`playwright-cli`.** The Apache-2.0 one and the only one of the nine where the
license imposes conditions beyond carrying the notice. All three that apply are
answered: the license travels (`LICENSE.txt`), the modified file carries a
notice (`MODIFICATIONS.md`, per section 4(b)) and section 4(d) — carry forward
an upstream `NOTICE` — is answered by the fact that upstream publishes none, at
the pinned revision or on `main` today, both listed and checked. The single
modification is an inserted eighteen-line "Headed browser requirement" section
that is RichOS doctrine rather than Playwright's; all seven files under
`references/` are byte-identical. The revision was pinned by fetching all
twenty-six upstream revisions and comparing: at `4fafccc` the references match
and `SKILL.md` differs only by the insertion and one revision later 57 lines
differ. Upstream's `LICENSE` is CRLF throughout and this repository stores LF,
so the bundled text is the LF form; no character of the license differs and both
hashes are recorded at the top of the file.

**`playwright-e2e-testing`.** One line differs from the pinned revision: a
deprecated `page.type(...)` call replaced by `page.locator(...).fill(...)`.
Nothing else — the frontmatter matches exactly, including the skill name, which
is upstream's own at that revision rather than a local rename. Upstream ships a
`metadata.json` beside the skill declaring `"license": "MIT"`, a second
independent statement of the same grant; it is not bundled.

**`svelte-code-writer` and `svelte-core-bestpractices`.** Both are byte-identical
to `0e55ee7` — all ten files of one, the single file of the other. The thing
worth knowing is about the license rather than the code: **at that revision
`sveltejs/ai-tools` published no license file at all**, at its root or in the
plugin directory and its root `package.json` is private with no license field.
The `LICENSE` granting MIT was added upstream seven weeks later, on 2026-04-28,
in `1ce957ac72c9b6d1d6086cc2f38254d802caebf7`. That file is what both
directories reproduce and each says so in its own header. It is upstream's only
statement of its own terms and it is not a license this project picked on
upstream's behalf. Upstream's copyright line is a Markdown link and is
reproduced as written.

**`use-railway`.** Pinned to the upstream revision that declares skill version
1.3.5 — the version the bundled copy declares in its own text — which is also
the closest by content of all thirty upstream revisions: 24 differing lines out
of roughly 600, against 34 for upstream's current `main`. Pinning it makes the
claim smaller and truer than the provenance document's, which read the skill as
substantially extended in-house on the strength of a commit message and a diff
against today's `main`. Against its actual base it is upstream's file with four
small edits — three punctuation swaps, one added CLI line, one "must" changed to
"should" and one dropped routing step with the renumbering it forced. All four
are enumerated in that directory's `LICENSE`.

**`gpt-exporter`.** Its `LICENSE` reads "Copyright (c) 2026 Alex Booster". It is
the repository owner's own separately published MIT project, vendored here — not
third-party code in the usual sense. It is listed anyway, because a reader
deciding what they may reuse needs the same answer either way and because
`richos/engine/LICENSING.md` used to describe it as the *only* exception when there are
seven.

### The reason two rows say "modified" that should not have to

`copywriting` is not byte-identical because **this project edited it** and not
on purpose. The dialect guard enforces American spelling on every file written
in this tree and it cannot tell a vendored third-party file from one we wrote.
On 2026-08-30 a sweep under that rule
(`06f4a8221a61500b90790383c8359d931391fe4f`) changed four lines in
`richos/engine/skills/copywriting/references/natural-transitions.md`.

**The same sweep changed ten lines in `richos/engine/skills/landing-page-taste/SKILL.md`,
which is also third-party** (MIT, Leonxlnx) and already in the table above. Its
row does not claim byte-identity, so nothing in this file is false — but the row
is quieter than the fact deserves and a reader comparing that directory against
`72e2995` will find ten differences with nothing here to explain them. Both are
American spelling substitutions of the same kind.

Nothing hazardous follows from either. MIT requires only that the notice travel
with the copy and it does. What follows is narrower and worth naming, in two
parts.

**The sweep manufactures "modified" rows.** It will keep doing so, one pass at a
time, for as long as vendored paths are in its scope — and each pass costs
somebody the work of establishing that the difference is only spelling.

**The guard now makes the correct repair impossible.** `dialect-guard.sh`
refuses a write whose new content carries a non-American word. It does not know
a vendored path either, so **re-vendoring `natural-transitions.md` from upstream
today would be blocked** — and so is writing upstream's spelling into a notice
that explains the difference. That is not hypothetical: while this section was
being written, an attempt to quote the two upstream words was refused and the
sentence above is phrased around the block rather than through it. A guard that
forbids restoring a file to its upstream state has turned a cosmetic drift into
a one-way door.

The remedy is the same for both and it is small: exempt vendored third-party
paths. It belongs to whoever owns the guard and it is recorded here rather than
fixed here.

Until it is fixed, the rule is the one this file already follows: **do not revert
the edits to make a row read better.** Record what the tree actually contains.

## Bundled fonts — redistributed inside the application

All four families are under the **SIL Open Font License 1.1**, whose only
material conditions are that the fonts are not sold on their own, that the
copyright and license notice travel with them and that a derived font is not
released under the reserved font name. RichOS satisfies all three: the fonts
ship as part of the application, the notices below are in the tree beside the
files and nothing is renamed or re-released.

| Family | Files | Copyright | Full terms |
|---|---|---|---|
| Inter | `Inter-Variable.woff2`, `Inter-Italic.woff2` | 2016 The Inter Project Authors (<https://github.com/rsms/inter>) | `richos/app/ui/fonts/LICENSE-Inter.txt` |
| Newsreader | `Newsreader-Regular.woff2`, `Newsreader-Italic.woff2` | 2020 The Newsreader Project Authors (<http://github.com/productiontype/Newsreader>) | `richos/app/ui/fonts/LICENSE-Newsreader.txt` |
| Noto Sans Math | `NotoSansMath-subset.woff2` | 2022 The Noto Project Authors (<https://github.com/notofonts/math>) | `richos/app/ui/fonts/LICENSE-NotoSansMath.txt` |
| Noto Sans Symbols | `NotoSansSymbols-subset.woff2`, `NotoSansSymbols2-subset.woff2` | 2022 The Noto Project Authors (<https://github.com/notofonts/symbols>) | `richos/app/ui/fonts/LICENSE-NotoSansSymbols.txt` |

The Noto files are **subsets**. Subsetting is a modification the OFL allows; the
reserved-font-name clause is not engaged because the subsets keep the original
names and are not redistributed as a separate font product.

## Bundled data compiled into the application

| Work | Upstream | Revision / version | License | Copyright | Full terms | Verified |
|---|---|---|---|---|---|---|
| `richos/engine/scripts/lib/dialect/third_party/varcon/` (`varcon.txt`, `README`) | VarCon, part of SCOWL (<http://wordlist.aspell.net/>), as vendored by crate-ci/typos at `crates/varcon/assets` | VarCon 2020.12.07; typos commit `dd3e1018f8a825a98be71048c21a9b0495aff703`; `varcon.txt` sha256 `c1e234817b526a809fa745d742fec84db88b65c429e22faf8a556b3d5a224085` | notice-only permissive terms (three notices, below) | 2000-2020 Kevin Atkinson; 2016 Benjamin Titze; 1993 Geoff Kuenning | `richos/engine/scripts/lib/dialect/third_party/varcon/README`, section "Copyright" | byte-identical (both files, 2026-09-27) |
| `richos/engine/scripts/lib/dialect/dialect-en-US.generated.dict` | generated from the row above by `richos/engine/scripts/lib/dialect/dialect-table.py` | same input, pinned by sha256 inside the generator | same three notices | same holders | this section | **modified version** — see below |

**What ships and where.** The engine asset carries all three files, because it
ships every tracked engine file. The generated table is also compiled into the
RichOS application's executable (`richos/app/crates/richos-core/src/american_spelling.rs`,
by `include_str!`), so the application bundle carries this page with it:
`richos/app/src-tauri/tauri.conf.json`'s `bundle.resources` copies it to
`RichOS.app/Contents/Resources/THIRD-PARTY-NOTICES.md` and
`richos/app/scripts/package-app.sh` refuses a bundle that lacks it or whose copy
lacks this section. The updater archive and the first-install zip are archives
of that bundle, so they carry it too. Nothing else from VarCon is used and no
VarCon script is bundled. `varcon.txt` itself is unmodified.

**The generated table is a modified version of VarCon, marked as such**, as the
Ispell notice's third condition requires of source and of binaries built from
modified source. It is a subset: only VarCon's verified clusters at SCOWL level
70 or below, only British forms with exactly one preferred American form and
no preferred-American use in any sense, reformatted to one pair per line, with
RichOS additions and removals merged in (`dialect-en-US.overrides.dict` and
`dialect-en-US.leave.tsv` beside it and `richos/engine/scripts/lib/dialect-en-US.dict`).
The table's own header says so in every copy and carries the three notices.

**No endorsement.** The names of Kevin Atkinson, Benjamin Titze and Geoff
Kuenning appear here only to give the attribution their notices require. None
of them endorses or promotes RichOS and nothing in RichOS may say or imply
otherwise.

**The three notices, verbatim from the bundled `README`:**

```text
Copyright 2000-2019 by Kevin Atkinson

Permission to use, copy, modify, distribute and sell this array, the
associated software, and its documentation for any purpose is hereby
granted without fee, provided that the above copyright notice appears
in all copies and that both that copyright notice and this permission
notice appear in supporting documentation. Kevin Atkinson makes no
representations about the suitability of this array for any
purpose. It is provided "as is" without express or implied warranty.

Copyright 2016 by Benjamin Titze

Permission to use, copy, modify, distribute and sell this array, the
associated software, and its documentation for any purpose is hereby
granted without fee, provided that the above copyright notice appears
in all copies and that both that copyright notice and this permission
notice appear in supporting documentation. Benjamin Titze makes no
representations about the suitability of this array for any
purpose. It is provided "as is" without express or implied warranty.

Since the original words lists come from the Ispell distribution:

Copyright 1993, Geoff Kuenning, Granada Hills, CA
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in the
   documentation and/or other materials provided with the distribution.
3. All modifications to the source code must be clearly marked as
   such.  Binary redistributions based on modified source code
   must be clearly marked as modified versions in the documentation
   and/or other materials provided with the distribution.
(clause 4 removed with permission from Geoff Kuenning)
5. The name of Geoff Kuenning may not be used to endorse or promote
   products derived from this software without specific prior
   written permission.

THIS SOFTWARE IS PROVIDED BY GEOFF KUENNING AND CONTRIBUTORS ``AS IS'' AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED.  IN NO EVENT SHALL GEOFF KUENNING OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
SUCH DAMAGE.
```

The README's header also reads "Copyright 2000-2020 by Kevin Atkinson ... and
Benjamin Titze"; the dates in the notices above are as the README states them.

## Authoring-time tools — used to build artifacts, not redistributed

These never reach a user. They are recorded because "we do not ship it" is a
claim worth being able to check.

| Tool | License | Where it is used |
|---|---|---|
| Pillow | MIT-CMU | `richos/app/scripts/generate-app-icons.sh` and `richos/app/scripts/lib/app_icons.py` — decode, resample, PNG and ICO writing for the application icon set |
| Apple `iconutil` | Apple system tool | `.icns` assembly, same script |
| Playwright | Apache-2.0 | `richos/app/ui/tests` — a devDependency of the browser acceptance harness. Not part of any bundle; excluded from the shipped frontend by `richos/app/src-tauri/build.rs` and gated by `richos/app/scripts/frontend-payload.test.sh`. |

## Compiled dependency inventory — GENERATED and where it lives

`docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md` is the per-package inventory of
every Rust dependency that resolves in this repository: 498 distinct
third-party packages across the three workspaces, each with its version, the
license its publisher declares and whether it reaches a macOS binary at all.

**Every one of them may be distributed as part of an AGPL-3.0-only combined
work.** Nothing in the tree is proprietary and nothing carries terms that
conflict with the AGPL.

The two things the pre-publication audit told us not to round up are answered
there in full rather than summarized here, because a second copy of a legal
finding is a second thing that can go stale:

- **MPL-2.0** — five packages. Reciprocal per file and section 3.3 expressly
  permits distributing the Larger Work under the GNU AGPL v3 provided no
  covered file carries the Exhibit B notice. None does.
- **`r-efi`** — offered as "MIT OR Apache-2.0 OR LGPL-2.1-or-later". RichOS
  takes the MIT branch and the package is a UEFI binding that is never
  compiled for either Darwin target. Its two coexisting versions are explained
  there too: the Tauri workspace is deliberately detached and resolves
  independently and inside it two semver-major lines of `getrandom` coexist.

### Why it can be trusted, which is the part that took the work

Until 2026-09-04 this section said the inventory did **not** exist and gave a
structural reason: `richos/app/.gitignore` ignored both `Cargo.lock` files, so an
inventory generated on any given morning would have been keyed to whatever
versions resolved that morning on one machine. A license list keyed to versions
nobody can reproduce is not evidence; it is a document that looks like evidence.

Both lockfiles are now tracked. The inventory is generated from them by
`richos/app/scripts/dependency-license-inventory.sh`, which:

- discovers its workspaces as every `Cargo.lock` git tracks, so a fourth Rust
  workspace appears with no edit to the tool;
- reads the **resolved graph** via `cargo metadata --locked`, which refuses to
  run at all if a lockfile would have to change;
- records the sha256 of each lockfile it read, so the document's identity is
  the lockfile rather than a date;
- **refuses to produce a document** if any package declares no license or
  declares one that has never been reviewed against AGPL-3.0-only. A new
  dependency arriving under unreviewed terms is the event this inventory exists
  to catch and rendering it as a row saying "unknown" would convert the
  finding into a line of text.

`richos/app/scripts/dependency-license-inventory.sh --check` fails if the committed
document no longer describes the committed lockfiles. Run it after any
dependency change; it is the only way this document can lie.

### What is still open before a binary ships

The audit's wording was "for the actual release bundle" and there is still no
release, tag or signed artifact. Two things therefore remain and they belong
to the release work rather than to this file:

- **Build the release with `--locked`.** `richos/app/scripts/package-app.sh` invokes
  `cargo tauri build` without it, so a release build would use the committed
  lockfile but would not be *refused* if it had to deviate from it. CI already
  passes `--locked` on both of its cargo steps.
- **Carry these notices inside the artifact.** The engine asset does this and
  has a packaging test that opens the archive and refuses it when license
  material is missing. Since 2026-09-27 the application bundle carries this
  page at `Contents/Resources/THIRD-PARTY-NOTICES.md` and `package-app.sh`
  refuses a bundle without it (see "Bundled data compiled into the
  application" above). The per-package Rust inventory
  (`docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md`) is not yet copied in beside it.

Nothing above blocks publishing the **source**. The AGPL obligations attach to
what this repository contains and every bundled work in it is named here with
its terms.

## Designs adopted from third-party source — no code bundled

Nothing in this section is redistributed. It is recorded because a design read
off somebody else's source and built to deliberately is a provenance fact and
a reader of ours should be able to find the original rather than assume the
reasoning was ours.

| What we took | Upstream | Revision | License | Copyright | Where it landed |
|---|---|---|---|---|---|
| The phone outbox's retry policy and its compare-and-set write revisions and the rule that the send path owns its own retry clock rather than borrowing the transport's | `pingdotgg/t3code` | `8ebb6112377a110afb956ac01fb44fee68f5709c` | MIT | T3 Tools Inc. | `richos/web/web-app/lib/queue.js`, `richos/web/web-app/app.js` |
| Dictation's paste: save every clipboard item, write the words, post Command-V with V's key code found by `UCKeyTranslate` over key codes 0 to 127 on the current layout and put the clipboard back after 1.0 s only if its change count is still the one the write produced | `human37/open-wispr`, `Sources/OpenWisprLib/TextInserter.swift` | `7ab4e62e8f182f3ecc2116e1094a1eb4416a248f` (v0.43.0) | MIT | human37 | `richos/app/src-tauri/src/dictation/insert.rs`, `richos/app/crates/richos-voice/src/dictation.rs` |

**What was and was not taken, precisely.** The two numbers — double from one
second, cap at sixteen — and the reasoning, from
`apps/mobile/src/state/thread-outbox-model.ts:163-165`,
`apps/mobile/src/state/thread-outbox-manager.ts:51-57, 164-177` and
`docs/internals/connection-runtime.md:5-22`. **No file, function or line of
theirs is copied**: our outbox is plain ES5-era JavaScript with a storage port,
theirs is React with Jotai atoms and Effect schemas and the two share no
shape. Each adopting site names the upstream file it was read from, in the
comment beside the code. MIT would permit copying with attribution; we did not
need to.

**open-wispr's paste, translated rather than copied.** Its `TextInserter` is Swift; dictation's
is Rust, reaching the same AppKit, CoreGraphics and Carbon calls through FFI, so no line of theirs
is in this repository. The logic is theirs, translated and its MIT notice is: *Copyright (c) 2026
human37. Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software without
restriction, including without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense and/or sell copies of the Software and to permit persons to whom the
Software is furnished to do so, subject to the following conditions: The above copyright notice
and this permission notice shall be included in all copies or substantial portions of the
Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE
AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.* Added around
it and ours: the no-text-box check, the spacing rule and the `org.nspasteboard.TransientType`
marker (dictation plan, `richos-hq/docs/plans/2026-10-08-dictation-anywhere.md`, section 5 and
section 7). The file at that commit was compared with the audited checkout on 2026-10-08 and is
the same.

Read for RichOS by Reed on 2026-09-18 and 2026-09-19 —
`richos-hq/docs/research/t3code-mobile-vs-richos-phone-2026-09-18.md` §1 item 6
and `richos-hq/docs/research/t3code-tooling-what-to-adopt-2026-09-19.md` §2.6.

## Adding something new

Vendoring anything means three things in the same commit:

1. the upstream license file next to the work,
2. a row in this table with a pinned revision or a declared version and
3. if the bundled copy differs from upstream at all, a notice in that directory
   saying how — required by Apache-2.0 section 4(b) and good manners
   everywhere else.

A dependency whose own license forbids combination with the AGPL cannot be
bundled, however convenient it is.

## Rich's voice: Pocket TTS speaking as Peter Yearsley

Rich speaks with Kyutai's Pocket TTS and its Peter Yearsley voice (CEO,
2026-10-14). The engine code and ONNX Runtime are compiled into the application.
The model and the voice recording are NOT shipped in it: the app downloads them
on first need from the pinned URLs in `richos/app/crates/richos-voice/src/pocket.rs`,
and checks each file's size and SHA-256 before using it.

| Work | Upstream | Revision / version | License | How it reaches the user |
|---|---|---|---|---|
| Pocket TTS engine in Rust (`richos/app/crates/richos-voice/src/pocket.rs`) | Buzz, <https://github.com/block/buzz>, `crates/buzz-voice/src/pocket_april.rs` | commit `8599efb6a3cfbd3d4bc4534f6de766b4481c7fca` | Apache-2.0; `richos/app/crates/richos-voice/third_party/buzz-voice/LICENSE` | compiled in; **modified version**, the changes are listed in that directory's `README.md` |
| ONNX Runtime, statically linked through the `ort` crate | Microsoft, prebuilt by pyke (`ort-sys` 2.0.0-rc.12 `dist.txt`) | 1.24.2, archive sha256 `612739f75438dc0a075461e1fb454226b4a1eb175e60a7271ba966bbbb972cd4` | MIT | compiled in |
| Pocket TTS model and the Mimi codec | Kyutai, <https://huggingface.co/kyutai/pocket-tts>; code <https://github.com/kyutai-labs/pocket-tts> | `english_2026-04` | weights CC-BY-4.0, code MIT | downloaded |
| ONNX export of that model, INT8 | KevinAHM, <https://huggingface.co/KevinAHM/pocket-tts-onnx> | `58a6d00cf13d239b6748cb0769f35c580a8f606c` | CC-BY-4.0 | downloaded |
| Voice recording `peter_yearsley.wav` | Kyutai, <https://huggingface.co/kyutai/tts-voices>, `voice-zero/`, from Voice-Zero's LibriVox selection | `323332d33f997de8394f24a193e1a76df720e01a` | CC0 | downloaded |

**Attribution, as CC-BY-4.0 asks.** Pocket TTS and Mimi are © Kyutai. The ONNX
export is by KevinAHM. Neither Kyutai, KevinAHM, Voice-Zero nor Peter Yearsley
endorses RichOS. Kyutai's terms forbid cloning a voice without consent; the
Peter Yearsley recording is a public-domain LibriVox reading released as CC0,
and it is one of Kyutai's own Pocket TTS preset voices.

## Copywriting regression test fixture

`richos/engine/scripts/hooks/fixtures/third_party/copywriting/natural-transitions.md`
contains the complete upstream copywriting reference used to verify re-vendoring.
It is test input, not a separately installed skill. Source and revision match the
copywriting entry above: Corey Haines's marketingskills at
`68f5eaf64e858438db47e436d7a3bef0e9d69721`. The adjacent `LICENSE` reproduces the MIT
notice and `README.md` records the extraction revision and SHA-256. The bytes are
unmodified and retain the upstream spellings.

## Engine 1.2.0 macOS runtime delivery

The macOS arm64 engine asset adds the runtime built by
`richos/app/scripts/build-runtimes.py`. This is generated delivery content, not
a copy of the developer's installed tools. Exact public download URLs and
SHA-256 pins are in `richos/app/scripts/runtime-sources.json`. The delivered
`runtime/delivery.json` lists every file digest and symlink.

| Work | Version | Delivered notices and source |
|---|---|---|
| CPython through python-build-standalone | 3.13.15, build 20260901 | Upstream Python distribution retained, including its `LICENSE*` files and bundled dependency notices |
| Node.js | 24.21.0 | Upstream distribution retained, including `runtime/node/LICENSE` and its dependency notices |
| jq | 1.8.2 | Upstream binary with `runtime/sources/JQ-COPYING` |
| Git | 2.55.0 | `runtime/sources/GIT-COPYING` and exact corresponding source `git-2.55.0.tar.xz` |
| GNU libiconv, statically linked into Git | 1.18 | `runtime/sources/ICONV-COPYING.LIB` and exact corresponding source `libiconv-1.18.tar.gz` |
| whisper.cpp (`whisper-cli`, with the ggml it vendors statically linked), MIT | 1.9.1 | `runtime/sources/WHISPER-CPP-LICENSE`; built from the pinned upstream source archive |
| FFmpeg (`ffmpeg`, `ffprobe`), static macOS arm64 build by Martin Riedl, GPL-3.0-or-later | 9.0.2 | `runtime/sources/FFMPEG-COPYING.GPLv3`; directions to the exact corresponding source in `runtime/sources/FFMPEG-SOURCE.txt` and a copy of every archive attached to the RichOS release |

### FFmpeg: a third party's static build and where its source is

`ffmpeg` and `ffprobe` are not built here. They are the unmodified static
macOS arm64 build of FFmpeg 9.0.2 that Martin Riedl publishes at
<https://ffmpeg.martin-riedl.de> (built 2026-09-20 19:18 UTC, signed with
Developer ID team `KU3N25YGLU`), pinned by SHA-256 like jq. That build is
configured with `--enable-gpl --enable-version3` and links about thirty
libraries statically (x264, x265, aom, dav1d, libvpx, OpenSSL, Opus and
others; `ffmpeg -version` prints the whole configuration), so as a whole it is
distributed under the GNU General Public License version 3 or later.

**Its corresponding source is not shipped inside the runtime, unlike Git's.**
It is 34 archives totaling 304,272,357 bytes (measured 2026-10-07), more than
twice the size of the whole engine asset today. Instead, under GPLv3 section
6(d), the runtime carries clear directions beside the programs:
`runtime/sources/FFMPEG-SOURCE.txt` lists FFmpeg 9.0.2, every library at the
version the build used and the build script
(`git.martin-riedl.de/ffmpeg/build-script`, Apache-2.0) at commit
`6a611e19870e197bc37c6e4c7fccddebd3715466`, each with its download URL and
SHA-256. The same list is the `corresponding_source` entry in
`richos/app/scripts/runtime-sources.json`. How the versions were established:
the build publishes its own `versions.txt` beside the zip, listing 31
libraries; 30 of those versions match the build script's `version/` files at
that commit, which is the script's "ffmpeg (version 9.0.2)" commit, made
about three hours before the build (libogg, which libvorbis and libtheora
need, is pinned there too). x264 is the one exception: the script builds it from
the `master` branch, so the listed commit is `0480cb05`, which was master at
the build time and still is. rav1e's Rust dependencies are fixed by the
`Cargo.lock` inside its archive.

**The RichOS release carries a copy of every archive.** GPLv3 6(d) leaves the
distributor responsible for keeping the source available as long as the
programs are distributed, so RichOS does not rely on the upstream hosts: every
RichOS release whose engine carries ffmpeg also carries, beside the engine
asset, a byte-identical copy of each of the 34 archives, named
`ffmpeg-source-<name>-<version>.tar.<gz|xz|bz2>` (for example
`ffmpeg-source-x264-0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee.tar.gz`), at
`https://github.com/RichSpark/RichOS/releases/download/<release>/<name>`.
`FFMPEG-SOURCE.txt` gives each archive's release name next to its upstream URL
and SHA-256. The nightly publish step attaches them (`nightly.py`
`attach_gpl_sources`, run by `finish` before the release is verified and before
any channel points at it), reading the list from the engine archive being
published, checking each archive against its SHA-256 and checking every
uploaded copy against GitHub's digest. The first release to carry ffmpeg,
`v1.2.0-nightly.20261007.45`, was published before that step existed; the same
function (`nightly.py attach-gpl-sources`) attached its copies on 2026-10-08.
Its runtime's `FFMPEG-SOURCE.txt` predates the release names and lists the
upstream URLs only; the copies on that release carry the same SHA-256s.

The Git, libiconv and whisper.cpp build instructions are the public runtime build script.
Git is built with a relocatable prefix and system libraries; libiconv is built
from the included source. No macOS system binary is copied. macOS supplies
Bash and the system libraries used by this delivery. The complete upstream
license texts govern each runtime and its dependencies. The Claude provider
is installed separately through its own installer and is not redistributed in
this runtime asset.

Runtime preparation and inventory checks do not establish installed-app
acceptance, signing or notarization. Those checks apply to the exact release
candidate separately.
