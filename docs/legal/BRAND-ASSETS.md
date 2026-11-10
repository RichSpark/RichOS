# Brand assets — what the AGPL does not cover

The root `LICENSE` is the GNU AGPL v3 and it covers the **software** in this
repository. It does not cover the RichOS brand.

**The RichOS name, the RichOS mark and wordmark, the application icons, the
banner artwork and the Rich Hand avatar are not licensed under the AGPL. All
rights in them are reserved unless separately authorized in writing by the
copyright holder.**

**The same applies to RichPass: the RichPass name and the RichPass logo and
icon artwork are not licensed under the AGPL. All rights in them are reserved
unless separately authorized in writing by the copyright holder.** RichPass
itself, the software in `richos/tools/richpass/`, is RichOS-authored software
under the AGPL like the rest of this repository.

This is the ordinary shape of an open-source project with a name worth
protecting — the same position Mozilla takes with the Firefox branding and the
Chromium project takes with the Chrome marks. You may take the software; you may
not take the identity and present your build as RichOS.

Without this document a visitor reads a root AGPL and reasonably concludes the
grant covers the logo too, because a license file at the root of a repository
says nothing about scope on its own. That silence was a real gap until
2026-09-04 and this file closes it.

## What is excluded, exactly

Every path below is excluded from the AGPL grant **as brand material**. This
list is complete for the brand: if a file is not named here, it is not withheld
on brand grounds.

**It does not follow that everything else is AGPL.** This repository also carries
work by other authors, redistributed under the licenses it arrived with — that
material is neither brand nor AGPL and it is inventoried separately in
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md). An earlier version of this
page said the AGPL covered everything it did not name, which was wrong in a way
that mattered: it contradicted the notices document and two shipping legal
documents disagreeing is worse than either being vague. **Three categories, not
two** — RichOS-authored software under the AGPL, RichOS brand material listed
here and third-party work under its own terms.

### Image and vector files

| Path | What it is |
|---|---|
| `.github/images/richos-banner.jpg` | The project banner on the repository front page |
| `richos/app/icon-source/richos-icon.svg` | The application icon, vector master |
| `richos/app/icon-source/richos-icon-1024.png` | The application icon, raster master |
| `richos/app/icon-source/preview/richos-icon-1024.png` | Icon preview renders |
| `richos/app/icon-source/preview/richos-icon-128.png` | Icon preview renders |
| `richos/app/icon-source/preview/richos-icon-32.png` | Icon preview renders |
| `richos/app/icon-source/preview/richos-icon-16.png` | Icon preview renders |
| `richos/app/src-tauri/icons/icon.icns` | Generated macOS icon |
| `richos/app/src-tauri/icons/icon.ico` | Generated Windows icon |
| `richos/app/src-tauri/icons/icon.png` | Generated icon |
| `richos/app/src-tauri/icons/32x32.png` | Generated icon |
| `richos/app/src-tauri/icons/128x128.png` | Generated icon |
| `app/src-tauri/icons/128x128@2x.png` | Generated icon |
| `richos/app/ui/assets/rich-hand.png` | The Rich Hand avatar artwork |
| `richos/tools/richos-extension/icons/icon16.png` | The RichOS mark, browser-extension icon |
| `richos/tools/richos-extension/icons/icon48.png` | The RichOS mark, browser-extension icon |
| `richos/tools/richos-extension/icons/icon128.png` | The RichOS mark, browser-extension icon |
| `richos/app/ui/assets/app-icons/richpass-128.png` | The RichPass icon |
| `richos/app/ui/assets/app-icons/richpass-mark-light.svg` | The RichPass mark, for a light page |
| `richos/app/ui/assets/app-icons/richpass-mark-dark.svg` | The RichPass mark, for a dark page |

`richos/app/icon-source/README.md` is **not** excluded. It is documentation about how
the icons are produced and it stays under the AGPL like every other document
here. The exclusion is the artwork, not the prose about the artwork.

### Artwork embedded in source files

These places hold brand drawing instructions as data inside shipped source
files. **The files themselves stay under the AGPL as software.** What is
excluded is the artwork those constants and elements encode — the outline
geometry of the RichOS mark and wordmark and of the RichPass mark:

| File | Constant | What it encodes |
|---|---|---|
| `richos/app/ui/splash.js` | `LOGO` | The RichOS mark, `viewBox 0 0 744 744`, two paths |
| `richos/app/ui/splash.js` | `WORDMARK` | The v3.5 wordmark, `viewBox 0 0 3299.1 754.5`, seven paths |
| `richos/app/ui/home.js` | `MARK_SVG` | The same v3.5 wordmark, as inline SVG markup |
| `richos/app/ui/index.html` | `#rail-wordmark` | The same v3.5 wordmark again, as an inline `<svg>` element with `viewBox 0 0 3299.1 754.5` |
| `richos/tools/richpass/RichPass.html` | `#richpass-title`, `#generate-title` | The RichPass mark (the same artwork as `richpass-mark-light.svg` and `richpass-mark-dark.svg`), embedded as base64 SVG images in a `<picture>` for the light and dark themes |
| `richos/tools/richpass/RichPass.html` | `<link rel="icon">` (two) | The RichOS mark, embedded as base64 SVG browser-tab icons for the light and dark themes |
| `richos/tools/richpass/tests/RichPass.master-reference.html`, `richos/tools/richpass/tests/RichPass.setup-reference.html`, `richos/tools/richpass/tests/RichPass.states-reference.html`, `richos/tools/richpass/tests/RichPass.style-reference.html` | the same elements | The same RichPass and RichOS marks, in the four frozen test references |

They are named by constant or element id rather than by line number because line
numbers move and a legal boundary that drifts with an unrelated edit is not a
boundary. At the time of writing they are at `splash.js` 283–301, `home.js`
173–183 and `index.html` 128.

RichPass is distributed as that one `RichPass.html` file, with everything in it,
so its own license statement (the `LICENSES AND PROVENANCE` section of its
opening comment) repeats this exclusion inside the file and points here.

The `index.html` copy was missing from this table until 2026-09-04. It was found
by re-running the inventory rather than by reading the list, which is the only
way a missing entry is ever found — a list cannot report what it does not
contain. Anyone adding a further copy of the mark adds a row here in the same
commit.

**What this means in practice for a fork:** take the file, keep the renderer,
replace the path data with your own mark. Everything around the constants — the
SVG assembly, the relief filter, the animation, the accessibility handling — is
AGPL software you may use and modify freely.

### Screenshots that reproduce the brand

The committed browser-suite evidence under `app/ui/tests/shots-*` contains
rendered screenshots of the running application and those images show the mark,
the wordmark and the splash compositions. Twelve directories:

`shots-3-1`, `shots-5`, `shots-5b`, `shots-5c`, `shots-5d`, `shots-7-2`,
`shots-10-1`, `shots-26`, `shots-contrast`, `shots-home`, `shots-splash`,
`shots-updates`.

The RichPass mark appears in `shots-nav-rail/richpass-page-light.png` and
`shots-nav-rail/richpass-page-dark.png`, the RichPass page of the
application.

They are test evidence and they exist so a reviewer can check a rendering claim.
Reading them, diffing them and regenerating them are all ordinary uses of this
repository. Extracting the brand elements out of them is not.

### The name

**RichOS** is the project's name and identity. The AGPL grants no rights in it,
and neither does this document.

**RichPass** is the name of RichOS's password tool and is held the same way.
The AGPL grants no rights in it and neither does this document.

## What you may do with the excluded files

You may:

- **View them** in the repository, on GitHub or in a clone.
- **Build and run RichOS unmodified** for yourself, with its own branding
  intact. That is what the artwork is there for.
- **Fork the repository** on GitHub. A fork necessarily copies these files and
  that is fine — GitHub forks are how the AGPL's source obligations are usually
  satisfied.
- **Reproduce the mark or banner to refer to the project** — in an article, a
  talk, a comparison, a list of tools. Nominative reference is not something
  this document tries to restrict.

You may not, without separate written permission:

- **Distribute a modified RichOS carrying the RichOS name, mark, icons or
  avatar.** Rebrand it. This is the whole point of the exclusion: a user must be
  able to tell whether the thing in front of them came from here.
- **Use the name or mark for your own product, service or organization** or in
  a way that suggests endorsement by or affiliation with RichOS.
- **Register the name or a confusingly similar one** as a trademark, domain,
  package name or app-store listing.
- **Use the artwork for anything unrelated to RichOS** — merchandise, templates,
  asset packs, training corpora sold as such.

If you want to do something this document does not permit, ask. The answer is
often yes and it costs an email.

## Why the embedded artwork was not extracted into asset files

The pre-publication audit asked for embedded artwork to be moved into standalone
asset files "where practical", because a file that mixes code and art makes the
boundary ambiguous. It was considered and **not** done and the reason is
specific rather than reluctance:

- `home.js` builds `MARK_SVG` as an inline string deliberately, on the documented
  ground that the boot path must not wait on a fetch. Turning it into a fetched
  `.svg` moves brand rendering onto the network path of a cold start — a real
  regression traded for a documentation convenience.
- `splash.js` reads its geometry from `LOGO` and `WORDMARK` constants that its
  own suite treats as fixed. `splash-library.js` is checked to be pure JSON data
  and `splash.js` is checked to contain no variation-specific value, so the mark
  lives where it does because two tests require it to.

The ambiguity is therefore resolved by naming the constants here, which costs
nothing and breaks nothing. If the boot path is ever restructured for other
reasons, extraction becomes free and should be taken then.

## Where this is referenced from

- `docs/legal/LICENSING.md` — the licensing overview points here for scope.
- `richos/engine/LICENSING.md` — the engine's own terms point here for the same reason.
- `.github/README.md` — the project front page should link here alongside the
  license and the third-party notices. That link belongs to the README rewrite
  tracked as section 4 of the 2026-09-04 pre-publication audit; this file is
  written to be linked and is not itself the README's author.
