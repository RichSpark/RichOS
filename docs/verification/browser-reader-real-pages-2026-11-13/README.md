# The browser reader on real websites, in five browsers, on a desktop as a user leaves it and through the real app (2026-11-13)

## What was asked

> "Getting useful information from ANY website in the user's default browser where the user is
> already logged in to all their relevant social media accounts etc. And NOTE: I don't know what
> the user's default browser is or what extensions the user might or might not have installed."

The reader is `richos_browser` (`richos/app/crates/richos-core/src/browser_tools.rs`): it shows
the teammate the page with its links, buttons and tabs numbered and presses only the one item
the teammate chooses. It reads through macOS Accessibility, so nothing is installed in the
browser and the user stays signed in. Before this day it had never opened a real page.

## Result

Measured at commit `b7f4d96fb`, in a test guest, nothing on the host's screen (run 9;
`run-9-report.txt` beside this file is that run's own report, page by page and state by state).

**Eleven real pages, each with a question whose answer is on the page.**

| Browser | Pages read and answered | Its tab closed afterwards | Time for all eleven |
|---|---|---|---|
| Safari (the guest's own) | 11 of 11 | yes | 95 s |
| Google Chrome 157 | 11 of 11 | yes | 107 s |
| Firefox 160 | 11 of 11 | yes | 88 s |
| Brave 1.99 | 11 of 11 | yes | 96 s |
| Microsoft Edge 157 | 11 of 11 | yes | 139 s |

| Page | What the reader had to do |
|---|---|
| Wikipedia, "Accessibility" (122,000 to 125,000 characters, in three parts) | read it whole, then follow the link "Assistive technology" |
| W3C accordion pattern | say which panel is open and which is closed, open "Billing Address", read its fields |
| Bootstrap accordion | open "Accordion Item #2", read its text; keep the lines and indents of its code sample |
| W3C tab list | show the tab "Carl Andersen", read its panel |
| Hacker News | read the table, follow its "More" to page 2 |
| infinite-scroll.com demo | scroll to the end, read what loaded |
| GitHub, `rust-lang/rust` (a web app) | read it, follow "Issues" |
| `github.com/settings/profile`, signed out | say that the site sent the browser to its sign-in page |
| `de.wikipedia.org/wiki/München` (450,000 to 466,000 characters, in eight parts, in German) | know its tab although the browser spells the address `M%C3%BCnchen`, not say the site sent it elsewhere and read it although Chrome and Brave offer to translate it |
| the local site's `/profile` | read the sentence only a browser holding the site's cookie is shown (a plain fetch in the same guest gets the signed-out one) |
| the local site's `/links` | follow a link that opens in a new tab, read it there, close both tabs |

**One real page (the GitHub repository) read with the browser in each state a desktop leaves it
in.** "Read" means on the first open unless said.

| Browser | Every window minimized | The app hidden | No window open | Not running |
|---|---|---|---|---|
| Safari | read, tab closed | read, tab closed | read, tab closed | read, tab closed |
| Chrome | read, tab closed | read, tab closed | read, tab closed | refused, then read on the second open (1) |
| Firefox | read, tab closed | read, tab closed | read, tab closed | read (15 s), tab closed |
| Brave | read, tab closed | read, tab closed | state not reached (2) | refused, then read on the second open (3) |
| Edge | read, tab closed | read, tab closed | read, tab closed | read (19 s), tab left open and said (4) |

Minimized or hidden, the reader brings one window back before it opens the page, says so in its
answer and leaves the window showing: Safari and Chrome do the same by themselves whenever any
app opens a link.

1. The guest's Chrome is a new install and opens its "What's New" tab in front when started, so
   the reader's tab is not the one showing. The reader refuses ("could not tell which tab shows
   it ... It showed 0 tabs before the open and 2 after") and the second open reads the page in
   5 s.
2. Brave did not close its window when its close button was pressed (not looked into: the walk
   only reports it). The page was read with the window open.
3. Brave restored its earlier tabs when started (6 tabs after the open). That is the case the
   refusal exists for: one of them could be the user's mail. The second open read the page.
4. Edge, quit by the walk, opened a "Restore pages" window when started. The page was read; the
   reader could not be sure which tab was its own, left it open and said so.

## Through the real app

Two conversations with the app built from this branch (`conversation-walk.py --reader-site`),
Safari the guest's default browser and signed in to the local site. The user's message both
times:

> "Our store's admin page is at http://127.0.0.1:18765/store and I'm already signed in there in
> my browser. Please have someone on the team read that page in my browser and tell me two
> things: how many orders we had this week and where the store ships from."

The shipping sentence is not in the page until its "Shipping" button is pressed.

- **At `fe765d4ff`** (the reader's first version). Rich answered in 18 seconds that he had
  passed it to the team. Stu gave the read to Reed. Reed called `open_page` and was shown
  `## Shipping [1 button "Shipping" collapsed]`; he called `press` with item 1 and was shown
  `Ships from Rotterdam within two days.`; he closed the page. 93 seconds later Rich told the
  user: "Orders this week: 214" and "Ships from: Rotterdam."
- **At `b7f4d96fb`** (this result's commit; `real-app-run-2-turns.txt` and
  `real-app-run-2-actors.txt` are the walk's own records). The page was read and answered the
  same way, but **Rich did the reading himself, in his own reply**: he called `open_page`, was
  shown the closed panel, pressed item 1, read the sentence, closed the tab and answered after
  57 seconds: "Orders this week: 214", "Ships from: Rotterdam. It goes out within two days. The
  shipping details were in a collapsed section. I opened it to read them, then closed the tab."
  The walk expected a specialist to make both calls and failed on that. The reader, the
  definitions and the engine are the same in both runs; which of the two Rich does is his
  instruction files' business, not the reader's and it is reported here as seen.

## What each run found and what changed

The same walk nine times. Every change to the reader after the first version came from one of
these runs on a real page, never from a page somebody made up.

1. **Run 1** (nine pages, four browsers). Chrome 9 of 9, Brave 9 of 9, Safari 8 of 9, Firefox 0
   of 9.
   - Safari gives a page's own tabs as radio buttons inside a tab group, without the tab
     subrole, so the reader took them for form options and would not press them. A radio button
     inside a tab group is now a tab.
   - Safari gives a menu button (Wikipedia's "Tools", GitHub's branch menu) as a pop-up button
     with no value. It is now numbered; a real select, which has its chosen option as its
     value, is still only read.
   - Safari, Chrome and Brave answer "not expanded" for an ordinary link or button when asked:
     2,957 of the 3,116 controls numbered in Safari's run read "collapsed" (Chrome 3,045 of
     3,204, Brave 3,039 of 3,198). A closed state now counts only for an element that lists the
     attribute as its own.
   - Firefox: a new profile puts a "Welcome to Firefox" dialog over its browser window and that
     dialog was the only window Accessibility listed. The guest's Firefox is now told not to
     show it; a user's has long dismissed it.
2. **Run 2.** Firefox still 0 of 9. Firefox, asked in the background to open a link, adds a tab
   and keeps showing the tab it was on and a browser exposes only the tab it shows. The reader
   now selects the one tab its own open added, once.
3. **Run 3** (`run-3-report.txt`). Ten pages, four browsers, 10 of 10 each.
4. **Run 4** tried joining the pieces of a page's text exactly as the browser gives them, to
   read a code sample as written. Chrome and Brave give the space between two styled runs as a
   piece of its own; Safari and Firefox leave it out, so there the page read
   `class="accordion"id="accordionExample"` and `•In`. Not kept. What is kept: the line breaks
   and indents a browser gives and one space wherever two pieces meet without any, in every
   browser, so a word is never run into the next.
5. **Run 5** (`run-5-report.txt`). Eleven pages, a fifth browser (Edge) and the four desktop
   states for the first time.
   - Chrome and Brave could not read the German page: "could not tell which tab shows it".
     Edge, Safari and Firefox read it.
   - Firefox, minimized or hidden, was asked twice and read nothing and left a tab each time.
   - Safari, minimized or hidden, was read and its tab was left open.
   - Chrome, Brave and Firefox, not running, were read only on the second open and Edge not
     at all (the guest's Edge showed its welcome window).
6. **Run 6.** A browser with no window on screen now gets one back before its tabs are counted:
   Firefox and Safari read on the first open and close their tab, minimized and hidden. The
   German page still failed in Chrome and Brave, now after 60 s.
7. **The cause of the German page**, measured in a guest (`chrome-translate-window.txt` is
   Chrome's window as Accessibility shows it with a German page open). Chrome shows "Translate
   this page?" as a window of its own and again inside the browser window, each time with two
   tab buttons, "German" and "English". The reader counted them as browser tabs and could no
   longer tell which tab was its own. It concerned every page in another language than the
   browser's, not this page. Now only a standard window is a browser window and a dialog laid
   over a window is never its tab bar or its page.
8. **Run 7** (stopped after Chrome). With the accessibility switch set again once a second
   while waiting, Chrome showed no page at all on 9 of 11 pages. It is now set again once,
   after 10 s, which is what a browser that has just started needs.
9. **Run 8** (Chrome and the states of Chrome and Firefox). Chrome 11 of 11, the German page in
   9.8 s. Firefox, not running, read on the first open.
10. **Run 9** is the result above.

In every run, a reader that could not tell which tab was its own read nothing and said so.

## Limits

- **A token split by its styling gets spaces the page did not write**: a highlighted code
  sample reads `< div class = "accordion" >` and `3<sup>rd</sup>` reads `3 rd`. Run 4 is why:
  two of five browsers do not say whether a space was there and a missing space would cost a
  name or a figure. Its lines and indents are kept.
- **A browser started by the read itself** can refuse the first open (it restored its earlier
  tabs or put another tab in front). The refusal says to open the page again and that works.
- **A page drawn as a picture** gives almost no text. The reader says how little it got; it
  does not read pictures.
- **The page opens as a tab in the user's browser** and that window shows it while the teammate
  reads; a minimized or hidden browser gets a window back and keeps it.

## Not measured

- **No real account anywhere.** The guest is signed in to nothing and nobody was asked for a
  sign-in. The signed-in case is the local site and its cookie. No real signed-in LinkedIn, X,
  Facebook or store page has been read by this reader.
- **Browsers not run:** Arc, Opera, Vivaldi, Orion and any other. Opera and Vivaldi are
  Chromium, like Chrome, Brave and Edge. Arc draws its own tab list and could not be started in
  a guest without an account; if its tabs cannot be read, the reader follows the window
  instead, reads the page and leaves the tab open and says so.
- **A desktop somebody is using**: a browser in full screen or on another desktop and a person
  at the Mac while a page is read.
- **A panel beside the page that is a web page itself** (a browser's own side panel showing a
  website). The reader takes a window's first web page at a web address.
- **The pages were chosen by the engineers who built the reader**, not by someone else.
- **The LinkedIn reader and this one at the same time** in one browser window: each selects
  its own tab, so one of them can lose its page and will say so.
- **Windows.** Nothing here runs on Windows.

## Run it again

```
cd richos/app/src-tauri && cargo build
cd ../scripts/testvm
./run-walk.py --wait 3600 --no-app --home EMPTY_DIR --engine "" --report REPORT -- \
    "$PWD/browser-reader-walk.py" --out OUT --exe <target>/debug/richos-tauri --expect-sha <commit>
```

`--steps site,chrome,tool,read-chrome,states-chrome,cleanup` walks one browser, `--pages accordion`
only the pages whose name matches. The pages are other people's websites and can change: a FAIL names the page
and what was missing and `OUT/<browser>-rows.json` holds every answer of the tool, whole.
