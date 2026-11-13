# The browser reader on real websites, in four browsers, and through the real app (2026-11-13)

## What was asked

> "Getting useful information from ANY website in the user's default browser where the user is
> already logged in to all their relevant social media accounts etc. And NOTE: I don't know what
> the user's default browser is or what extensions the user might or might not have installed."

The reader is `richos_browser` (`richos/app/crates/richos-core/src/browser_tools.rs`): it shows
the teammate the page with its links, buttons and tabs numbered, and presses only the one item
the teammate chooses. Before this day it had never opened a real page.

## Result

Measured at commit `230c19a49`, in a test guest, nothing on the host's screen.

| Browser | Real pages read and answered | Tab closed afterwards | Time for all ten |
|---|---|---|---|
| Safari (the guest's own) | 10 of 10 | yes | 51 s |
| Google Chrome (current download) | 10 of 10 | yes | 69 s |
| Firefox 160.0 | 10 of 10 | yes | 59 s |
| Brave 1.99.57 | 10 of 10 | yes | 71 s |

`report.json` beside this file is that run's own report, page by page.

The ten pages, each with a question whose answer is on the page:

| Page | What the reader had to do |
|---|---|
| Wikipedia, "Accessibility" (122,000 to 128,000 characters, in three parts) | read it whole, then follow the link "Assistive technology" |
| W3C accordion pattern | say which panel is open and which is closed, open "Billing Address", read its fields |
| Bootstrap accordion | open "Accordion Item #2", read its text |
| W3C tab list | show the tab "Carl Andersen", read its panel |
| Hacker News | read the table, follow its "More" to page 2 |
| infinite-scroll.com demo | scroll to the end, read what loaded |
| GitHub, `rust-lang/rust` (a web app) | read it, follow "Issues" |
| `github.com/settings/profile`, signed out | say that the site sent the browser to its sign-in page |
| the local site's `/profile` | read the sentence only a browser holding the site's cookie is shown (a plain fetch in the same guest gets the signed-out one) |
| the local site's `/links` | follow a link that opens in a new tab, read it there, close both tabs |

## Through the real app

One conversation with the app built from this branch (`conversation-walk.py --reader-site`), Safari
the guest's default browser and signed in to the local site. The user's message:

> "Our store's admin page is at http://127.0.0.1:18765/store and I'm already signed in there in
> my browser. Please have someone on the team read that page in my browser and tell me two
> things: how many orders we had this week, and where the store ships from."

The shipping sentence is not in the page until its "Shipping" button is pressed.

- Rich answered in 18 seconds that he had passed it to the team.
- Stu gave the read to Reed. Reed called `open_page` and was shown
  `## Shipping [1 button "Shipping" collapsed]`; he called `press` with item 1 and was shown
  `Ships from Rotterdam within two days.`; he closed the page. Both calls are recorded as Reed's,
  a specialist's, not Stu's and not Rich's.
- 93 seconds later Rich told the user: "Orders this week: 214" and "Ships from: Rotterdam. The
  Shipping section says 'Ships from Rotterdam within two days.' The page was read in your
  signed-in browser. Nothing on it was changed, and the tab was closed afterward."

## What the first two runs found, and what changed

Three runs of the same walk. The first two are why the code is what it is.

1. **Run 1.** Chrome 9 of 9, Brave 9 of 9, Safari 8 of 9, Firefox 0 of 9.
   - Safari gives a page's own tabs as radio buttons inside a tab group, without the tab
     subrole, so the reader took them for form options and would not press them. A radio button
     inside a tab group is now a tab.
   - Safari gives a menu button (Wikipedia's "Tools", GitHub's branch menu) as a pop-up button
     with no value. It is now numbered; a real select, which has its chosen option as its
     value, is still only read.
   - Safari, Chrome and Brave answer "not expanded" for an ordinary link or button when asked:
     2,957 of the 3,116 controls numbered in Safari's run read "collapsed" (Chrome 3,045 of
     3,204, Brave 3,039 of 3,198). A closed state now counts only for an
     element that lists the attribute as its own. Run 3 checks this on the W3C accordion: the
     open panel reads `expanded`, the closed one `collapsed`, a plain button neither.
   - Firefox: the reader refused every page with "could not tell which tab shows it" and read
     nothing. A new Firefox profile puts a "Welcome to Firefox" dialog window over its browser
     window, and that dialog was the only window Accessibility listed. A user's Firefox has
     long dismissed it; the guest's is now told not to show it, through Firefox's own policies.
2. **Run 2.** Safari, Chrome and Brave 9 of 9. Firefox still 0 of 9, refusing in the same words.
   Measured next: Firefox, asked in the background to open a link, adds a tab and keeps showing
   the tab it was on, and a browser exposes only the tab it shows. The reader now selects the
   one tab its own open added, once, the way Safari and the Chromium browsers select it by
   themselves.
3. **Run 3** is the result above, with a tenth page added (a link that opens in a new tab).

In all three runs a reader that could not tell which tab was its own read nothing and said so.

## Not measured

- **No real account anywhere.** The guest is signed in to nothing, and nobody was asked for a
  sign-in. The signed-in case is the local site and its cookie. No real signed-in LinkedIn, X,
  Facebook or store page has been read by this reader.
- **Browsers not run:** Edge, Arc, Opera, Vivaldi, Orion and any other. Edge, Opera and
  Vivaldi are Chromium, like Chrome and Brave. Arc draws its own tab list and could not be
  started in a guest without an account; if its tabs cannot be read, the reader follows the
  window instead, reads the page, and leaves the tab open and says so.
- **A desktop somebody is using.** The page opens as a tab in the user's front browser window
  and that window shows it while the teammate reads. Nobody has watched that with a person at
  the Mac.
- **The pages were chosen by the engineer who built the reader**, not by someone else.
- **A page drawn as a picture** gives almost no text. The reader says how little it got; it
  does not read pictures.
- **The LinkedIn reader and this one at the same time** in one browser window: each selects
  its own tab, so one of them can lose its page and will say so. Not run.
- **Windows.** Nothing here runs on Windows.

## Run it again

```
cd richos/app/src-tauri && cargo build
cd ../scripts/testvm
./run-walk.py --wait 3600 --no-app --home EMPTY_DIR --engine "" --report REPORT -- \
    "$PWD/browser-reader-walk.py" --out OUT --exe <target>/debug/richos-tauri --expect-sha <commit>
```

The pages are other people's websites and can change: a FAIL names the page and what was
missing, and `OUT/<browser>-rows.json` holds every answer of the tool, whole.
