#!/usr/bin/env python3
"""Walk the fifteen behaviors of RichFiles round 107.1 in a disposable guest, light and dark.

Run it as the command of run-walk.py --no-app with a packaged standalone RichFiles zip:

  run-walk.py --no-app --home EMPTY_DIR --engine NONE --report REPORT.json -- \\
      richfiles-round107-walk.py --standalone-zip RichFiles.zip --out DIR [--themes daybreak,sovereign]

It installs the app in the guest, builds a fixture folder with the mockup's names (129 items, so
the List view scrolls sideways) and a seven-level folder tree on the guest's Desktop, then for
each theme opens RichFiles on the fixture and drives it with the real mouse (pointer-drag.sh)
and keys (System Events), reading the result back from the guest's file system and
accessibility tree. Every check is recorded in DIR/checks.json and every look in DIR/*.png;
a failed check is recorded and the walk goes on, so one guest gives the whole picture. The
app is quit by its own Apple Event at the end and the walk confirms the process is gone.
Nothing plays sound and nothing runs on this Mac's screen.
"""
import argparse
import json
from pathlib import Path
import shlex
import subprocess
import time

HERE = Path(__file__).resolve().parent
APP = "RichFiles"
BUNDLE = "app.richos.richfiles"
REMOTE = "/Users/admin/richfiles-round107"
FIXTURE = "/Users/admin/Home107"

FOLDERS = ["Applications", "Archive 2024", "bank statements", "Client work", "Documents", "Downloads", "Movies",
           "Music", "Pictures", "Projects", "Public", "RichOS notes", "sound-effects", "a test folder",
           "this is a test folder", "updates", "Webinars"]
FILES = [
    "_short file name.html", "_medium length file name.html", "_the longest file name in this column.html",
    "_webinar-widgets.css", "_webinar-widgets-old.css", "bootstrap-icons.min.css", "check-circle.svg",
    "custom-webinar-embed.css", "embed-sample-for-webinar.css", "fa-check-circle - Copy.svg", "fa-check-circle.svg",
    "Funnel_BG_Image.webp", "image.jpg", "image.webp", "Logo_White.png", "mock-custom-values.js", "presenter.jpg",
    "presenter.png", "test1.txt", "test2 - Copy.txt", "test2.txt", "webinar-opt-in-1.1 - draft A.html",
    "webinar-opt-in-1.1 - draft B.html", "webinar-opt-in-base-1.0.html", "webinar-opt-in-base-1.0.txt",
    "yet another test file.txt", "2026 budget draft.xlsx", "2026 goals.md", "Annual report 2025.pdf",
    "Brand colors.pdf", "call-notes-2026-10-04.txt", "call-notes-2026-10-11.txt", "changelog.md",
    "checkout-page-copy.docx", "content-calendar-q1.csv", "cover-photo.jpg", "deploy.sh", "faq-draft.md",
    "favicon.ico", "hero-banner-wide.png", "invoice-1041.pdf", "invoice-1042.pdf", "keynote-outline.md",
    "landing-page-v1.html", "landing-page-v2.html", "launch-plan.md", "lead-magnet.pdf", "logo-dark.svg",
    "logo-light.svg", "meeting-notes.txt", "newsletter-issue-14.md", "newsletter-issue-15.md", "package.json",
    "podcast-episode-12-outline.md", "product-demo.mp4", "proposal-acme-co.pdf", "readme.txt", "signup-form.html",
    "social-post-ideas.txt", "style.css", "survey-results.csv", "tagline-options.txt", "testimonials.md",
    "thank-you-page.html", "todo.txt", "useful-links.txt", "webinar-replay-notes.txt", "welcome-email.html",
] + [f"notes-{n:02d}.txt" for n in range(1, 45)]
# The Desktop tree of the round's mockup, seven levels deep, with folders that hold nothing or only files.
TREE = [
    "AI book/chapter review calls", "AI book/drafts/v1", "AI book/drafts/v2", "AI book/research",
    "AI book/cover design/frontcover hardback",
    "AI book/cover design/book cover complete set/mockups",
    "AI book/cover design/book cover complete set/print-ready/proofs",
    "AI book/cover design/book cover complete set/print-ready/spine variants/cream paper",
    "AI book/cover design/book cover complete set/print-ready/spine variants/white paper",
    "landing pages/spring launch", "landing pages/webinar funnel/assets", "to sort",
]

CONTROL, COMMAND = "control down", "command down"


def run(argv, timeout=60):
    result = subprocess.run([str(a) for a in argv], capture_output=True, text=True, timeout=timeout)
    return result.returncode, (result.stdout + result.stderr).strip()


class Walk:
    def __init__(self, vm, out):
        self.vm, self.out = vm, out
        self.checks = []

    # -- the guest ------------------------------------------------------------------------
    def guest(self, command, timeout=120):
        return run([HERE / "guest.sh", self.vm, command], timeout)

    def ls(self, folder=FIXTURE):
        return set(self.guest("ls -1 " + shlex.quote(folder))[1].splitlines())

    def until_listed(self, name, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if name in self.ls():
                return True
            time.sleep(0.5)
        return False

    def shot(self, name):
        code, text = run([HERE / "shot.sh", self.vm, self.out / f"{name}.png"], 90)
        if code:
            self.record(f"screenshot {name}", False, text[:300])

    def record(self, name, ok, detail=""):
        self.checks.append({"check": name, "ok": bool(ok), "detail": detail})
        (self.out / "checks.json").write_text(json.dumps(self.checks, indent=2) + "\n")
        print(("PASS " if ok else "FAIL ") + name + (": " + detail if detail else ""), flush=True)

    # -- the app's tree ---------------------------------------------------------------------
    def nodes(self, *matchers):
        code, text = run([HERE / "ax.sh", self.vm, "find", *matchers, "--app", APP, "--json"], 90)
        found = []
        for line in text.splitlines():
            if line.strip().startswith("{"):
                node = json.loads(line)
                if all(isinstance(node.get(k), (int, float)) for k in ("x", "y", "w", "h")) and node["w"] and node["h"]:
                    found.append(node)
        return found

    def node(self, title, *more):
        found = self.nodes("--title", title, *more)
        return found[0] if found else None

    @staticmethod
    def center(node):
        return round(node["x"] + node["w"] / 2), round(node["y"] + node["h"] / 2)

    @staticmethod
    def name_point(node):
        # The row's center: always on the row, also when its column is partly scrolled out of view.
        return round(node["x"] + node["w"] / 2), round(node["y"] + node["h"] / 2)

    # -- the real mouse and keys --------------------------------------------------------------
    def pointer(self, *args):
        return run([HERE / "pointer-drag.sh", self.vm, *args, "--app", APP], 60)

    def click(self, x, y):
        return self.pointer(f"{x},{y}", f"{x},{y}")

    def double(self, x, y):
        return self.pointer(f"{x},{y}", "--double")

    def right(self, x, y):
        return self.pointer(f"{x},{y}", f"{x},{y}", "--right")

    def hover(self, x, y):
        # A hand arrives from a little way off, so the app sees the pointer move onto the item.
        return self.pointer(f"{x - 30},{y + 3}", f"{x},{y}", "--hover")

    def wheel(self, x, y, lines):
        return self.pointer(f"{x},{y}", "--scroll", str(lines))

    def key(self, code, *modifiers):
        using = f" using {{{', '.join(modifiers)}}}" if modifiers else ""
        return self.guest("osascript -e " + shlex.quote(
            f'tell application "System Events" to key code {code}{using}'))

    def keystroke(self, text, *modifiers):
        using = f" using {{{', '.join(modifiers)}}}" if modifiers else ""
        literal = text.replace("\\", "\\\\").replace('"', '\\"')
        return self.guest("osascript -e " + shlex.quote(
            f'tell application "System Events" to keystroke "{literal}"{using}'))

    def scroll_value(self):
        bars = self.nodes("--role", "AXScrollBar")
        horizontal = [b for b in bars if b["w"] > b["h"]]
        return horizontal[0].get("value") if horizontal else None

    # -- setup ------------------------------------------------------------------------------
    def build_fixture(self):
        # Each theme starts from the same folder: the previous theme's copies and renames are gone.
        self.guest(f"rm -rf {shlex.quote(FIXTURE)} && mkdir -p {REMOTE} {shlex.quote(FIXTURE)}")
        script = "cd " + shlex.quote(FIXTURE) + " && " + " && ".join(
            ["mkdir -p " + shlex.quote(f) for f in FOLDERS]
            + ["printf 'fixture %s\\n' " + shlex.quote(f) + " > " + shlex.quote(f) for f in FILES])
        self.guest(script, 180)
        self.guest("printf 'inside\\n' > " + shlex.quote(FIXTURE + "/Projects/plan.txt"))
        # The Desktop tree is rebuilt too: the previous theme renamed a folder in it.
        desktop = ("cd /Users/admin/Desktop && rm -rf 'AI book' 'landing pages' 'to sort' && "
                   + " && ".join("mkdir -p " + shlex.quote(p) for p in TREE))
        self.guest(desktop + " && printf 'notes\\n' > 'AI book/research/notes.txt'")
        # The mockup's Music is empty; the guest's ships with a folder inside, which the walk moves
        # to the guest's Trash so the no-arrow case is the one the CEO saw (the guest is disposable).
        self.guest("mkdir -p /Users/admin/Music && find /Users/admin/Music -mindepth 1 -maxdepth 1 -type d "
                   "-exec mv {} /Users/admin/.Trash/ \\;")
        music = self.guest("find /Users/admin/Music -mindepth 1 -maxdepth 1 -type d | wc -l")[1].strip()
        self.record("guest Music folder holds no folder (the no-arrow case)", music == "0", f"{music} folders")
        self.record("fixture holds 129 items", len(self.ls()) == 129, f"{len(self.ls())} items")

    def prepare(self, archive):
        self.guest(f"mkdir -p {REMOTE}")
        code, text = run([HERE / "guest.sh", self.vm, "--push", archive, REMOTE + "/RichFiles.zip"], 300)
        self.guest(f"ditto -x -k {REMOTE}/RichFiles.zip /Applications")
        code, text = self.guest("codesign --verify --deep --strict /Applications/RichFiles.app")
        self.record("app bundle signature verifies in the guest", code == 0, text[:200])

    def launch(self, theme):
        self.build_fixture()
        self.guest(f"defaults write {BUNDLE} richfiles.theme -string {theme}")
        self.guest("open -a /Applications/RichFiles.app " + shlex.quote(FIXTURE))
        for _ in range(30):
            if self.node("yet another test file.txt"):
                break
            time.sleep(1)
        pid = self.guest(f"pgrep -x {APP}")[1].strip()
        self.record(f"{theme}: RichFiles opened the fixture", bool(self.node("yet another test file.txt")), f"pid {pid}")
        # The window fills the guest display, so every column the walk needs is in view.
        self.guest("osascript -e " + shlex.quote(
            'tell application "System Events" to tell process "RichFiles" to '
            'set position of window 1 to {0, 25}') + " -e " + shlex.quote(
            'tell application "System Events" to tell process "RichFiles" to set size of window 1 to {1600, 960}'))
        time.sleep(1)
        return pid

    def quit(self):
        script = f'tell application id "{BUNDLE}" to quit'
        self.guest(f"if pgrep -x {APP} >/dev/null; then osascript -e " + shlex.quote(script) + "; fi")
        for _ in range(10):
            if self.guest(f"pgrep -x {APP}")[0] != 0:
                return True
            time.sleep(1)
        return False

    # -- the fifteen behaviors ----------------------------------------------------------------
    def walk(self, theme):
        t = theme
        self.launch(theme)
        self.shot(f"{t}-00-open")

        # 4, 5: one click selects with the quiet look; the pane draws no ring.
        item = self.node("yet another test file.txt")
        self.click(*self.name_point(item))
        time.sleep(0.6)
        self.shot(f"{t}-04-05-selected-no-pane-ring")

        # 7: hover tint on another item.
        other = self.node("test2.txt")
        self.hover(*self.name_point(other))
        time.sleep(0.3)
        self.shot(f"{t}-07-hover")

        # 13: rest on the item: Kind, Size, Modified.
        time.sleep(1.2)
        self.shot(f"{t}-13-quick-info")
        folder = self.node("Projects")
        self.hover(*self.name_point(folder))
        time.sleep(1.6)
        self.shot(f"{t}-13-quick-info-folder")

        # 6: a click in the 12 px gap between the first two columns deselects; the outline alone stays.
        item = self.node("_short file name.html")
        self.click(*self.name_point(item))
        time.sleep(0.6)
        gap = (round(item["x"] + item["w"] + 6), round(item["y"] + item["h"] / 2))
        self.click(*gap)
        time.sleep(0.6)
        self.shot(f"{t}-06-deselected-outline")

        # 8: right-click an unselected item: it is selected and its menu opens beside it.
        html = self.node("landing-page-v1.html")
        self.right(*self.name_point(html))
        time.sleep(1.0)
        self.shot(f"{t}-08-right-click-selects")
        menus = [m for m in self.nodes("--role", "AXMenu") if m["h"] > 100]
        if html and menus:
            menu = menus[0]
            # Clear of the row: beside its end, below it or above it.
            clear = (menu["x"] >= html["x"] + html["w"] - 3 or menu["y"] >= html["y"] + html["h"] - 3
                     or menu["y"] + menu["h"] <= html["y"] + 3)
            self.record(f"{t}: the menu opens clear of the right-clicked row, its name readable", clear,
                        f"row x {html['x']}-{html['x'] + html['w']} y {html['y']}-{html['y'] + html['h']}, "
                        f"menu x {menu['x']} y {menu['y']}-{menu['y'] + menu['h']}")
        else:
            self.record(f"{t}: the right-click menu was found", False, f"row {bool(html)}, menus {len(menus)}")
        # 3: Open With lists the apps.
        open_with = self.node("Open With", "--role", "AXMenuItem")
        if open_with:
            # Menu items are reached in one move: an approach from the side would cross other items.
            self.pointer("%d,%d" % self.center(open_with), "--hover")
            time.sleep(1.2)
            self.shot(f"{t}-03-open-with-submenu")
            another = self.node("Choose Another App…", "--role", "AXMenuItem")
            self.record(f"{t}: Open With lists Choose Another App…", bool(another))
            if another:
                self.pointer("%d,%d" % (another["x"] + 10, another["y"] + another["h"] // 2), "--hover")
                time.sleep(0.4)
                self.click(*self.center(another))
                time.sleep(2.5)
                self.shot(f"{t}-03-choose-another-app")
                # The sheet's own buttons: the window behind it has no Cancel button.
                sheet = self.nodes("--title", "Cancel", "--role", "AXButton")
                self.record(f"{t}: Choose Another App opens the app chooser", bool(sheet))
                self.key(53)
                time.sleep(0.8)
        else:
            self.record(f"{t}: the right-click menu has Open With", False)
            self.key(53)

        # 9: the wheel scrolls the list sideways, down = right, up = left.
        pane = self.node("test1.txt")
        before = self.scroll_value()
        self.wheel(*self.name_point(pane), -5)
        time.sleep(1.0)
        after = self.scroll_value()
        self.shot(f"{t}-09-wheel-down-scrolled-right")
        self.record(f"{t}: wheel down moves the list right", before is not None and after is not None
                    and float(after) > float(before), f"scroll bar value {before} -> {after}")
        self.wheel(*self.name_point(self.node("test1.txt") or pane), 5)
        time.sleep(1.0)
        back = self.scroll_value()
        self.record(f"{t}: wheel up moves it back left", back is not None and after is not None
                    and float(back) < float(after), f"scroll bar value {after} -> {back}")

        # 1: Ctrl+C, Ctrl+V twice duplicates as " - Copy", " - Copy (2)"; a folder as " - Copy".
        item = self.node("yet another test file.txt")
        self.click(*self.name_point(item))
        time.sleep(0.6)
        self.keystroke("c", CONTROL)
        time.sleep(0.4)
        self.keystroke("v", CONTROL)
        # A copy is staged and verified before it is published; in a guest that can take seconds.
        self.until_listed("yet another test file - Copy.txt")
        time.sleep(0.8)
        self.shot(f"{t}-01-duplicate-copy")
        self.keystroke("v", CONTROL)
        self.until_listed("yet another test file - Copy (2).txt")
        names = self.ls()
        self.record(f"{t}: Ctrl+C, Ctrl+V, Ctrl+V makes - Copy and - Copy (2)",
                    {"yet another test file - Copy.txt", "yet another test file - Copy (2).txt"} <= names)
        # The list reloads after the file appears on disk, so its row is waited for too.
        listed = False
        for _ in range(10):
            listed = bool(self.node("yet another test file - Copy (2).txt"))
            if listed:
                break
            time.sleep(0.5)
        self.record(f"{t}: the new copy is in the list (its selected look is in the screenshot)", listed)
        self.shot(f"{t}-01-duplicate-copy-2")
        self.keystroke("z", CONTROL)
        time.sleep(1.5)
        self.record(f"{t}: Ctrl+Z undoes the last duplicate",
                    "yet another test file - Copy (2).txt" not in self.ls())
        projects = self.node("Projects")
        self.click(*self.name_point(projects))
        time.sleep(0.6)
        self.keystroke("c", CONTROL)
        time.sleep(0.4)
        self.keystroke("v", CONTROL)
        self.until_listed("Projects - Copy")
        self.record(f"{t}: a folder duplicates as - Copy", "Projects - Copy" in self.ls())

        # 2, 11: click, pause, click opens the inline box; the column follows each letter.
        short = self.node("_short file name.html")
        self.click(*self.name_point(short))
        time.sleep(1.0)
        self.click(*self.name_point(short))
        time.sleep(1.2)
        self.shot(f"{t}-02-click-pause-click-rename")
        self.keystroke("renamed short")
        time.sleep(0.4)
        self.key(36)
        time.sleep(1.5)
        self.record(f"{t}: click, pause, click renames in place", "renamed short.html" in self.ls())
        # F2 on the longest name, made longer: the next column moves right as it is typed.
        longest = self.node("_the longest file name in this column.html")
        if longest:
            self.click(*self.name_point(longest))
            time.sleep(0.6)
            marker = self.next_column_marker(longest)
            self.key(120)
            time.sleep(1.0)
            self.key(124)
            self.keystroke(" even longer than before")
            time.sleep(1.0)
            self.shot(f"{t}-11-column-grows-while-typing")
            moved = self.next_column_marker(longest)
            self.record(f"{t}: the next column moves right while the name is typed",
                        marker is not None and moved is not None and moved > marker,
                        f"next column x {marker} -> {moved}")
            self.key(53)
            time.sleep(0.8)
        self.shot(f"{t}-10-11-12-spacing-columns-no-lines")

        # 2: a drag inside the inline box selects part of the name; it never starts a file drag.
        readme = self.node("readme.txt")
        if readme:
            self.click(*self.name_point(readme))
            time.sleep(0.6)
            self.key(120)
            time.sleep(1.0)
            y = round(readme["y"] + readme["h"] / 2)
            start = round(readme["x"] + 36)
            self.shot(f"{t}-02-f2-box")
            self.pointer(f"{start},{y}", f"{start + 18},{y}")
            time.sleep(0.5)
            self.shot(f"{t}-02-drag-selects-part")
            self.keystroke("Z")
            time.sleep(0.3)
            self.shot(f"{t}-02-drag-typed")
            self.key(36)
            time.sleep(1.5)
            names = self.ls()
            renamed = [n for n in names if n.endswith(".txt") and "Z" in n and n != "Z.txt" and n.endswith("me.txt")]
            self.record(f"{t}: a drag in the rename box selects text and typing replaces it",
                        "readme.txt" not in names and bool(renamed),
                        ", ".join(renamed) or "names with Z: " + ", ".join(n for n in names if "Z" in n))

        # 15: a folder made outside RichFiles gets its arrow on Refresh (F5).
        self.record(f"{t}: Documents (no folders yet) has no arrow", self.node("Open Documents in the sidebar") is None)
        self.guest("mkdir -p '/Users/admin/Documents/Made in Finder'")
        self.key(96)
        time.sleep(2.5)
        self.record(f"{t}: after a folder is made outside and Refresh, Documents has an arrow",
                    self.node("Open Documents in the sidebar") is not None)
        self.guest("rmdir '/Users/admin/Documents/Made in Finder'")

        # 15, 16: the sidebar tree. Home opened by its arrow: Music (no folders inside) has no arrow.
        home_arrow = self.node("Open Home in the sidebar")
        if home_arrow:
            self.click(*self.center(home_arrow))
            time.sleep(1.5)
        close_home = self.node("Close Home in the sidebar")
        self.record(f"{t}: with Home open, Music (no folders inside) has no arrow",
                    close_home is not None and self.node("Open Music in the sidebar") is None,
                    "Home opened" if close_home else "Home did not open")
        self.shot(f"{t}-15-home-open-music-no-arrow")
        if close_home:
            self.click(*self.center(close_home))
            time.sleep(1.0)
        desktop_arrow = self.node("Open Desktop in the sidebar")
        self.record(f"{t}: Desktop has an arrow", bool(desktop_arrow))
        window_before = self.window_title()
        if desktop_arrow:
            self.click(*self.center(desktop_arrow))
            time.sleep(1.5)
            for name in ("AI book", "cover design", "book cover complete set", "print-ready", "spine variants"):
                arrow = self.node(f"Open {name} in the sidebar")
                if arrow:
                    self.click(*self.center(arrow))
                    time.sleep(1.2)
            self.shot(f"{t}-15-tree-seven-levels")
            self.record(f"{t}: seven levels open (white paper is shown)", bool(self.node("white paper")))
            self.record(f"{t}: to sort (empty) has no arrow", self.node("Open to sort in the sidebar") is None)
            self.record(f"{t}: the arrow left the right pane alone", self.window_title() == window_before
                        and bool(self.node("yet another test file.txt")), f"{window_before!r} -> {self.window_title()!r}")
            close = self.node("Close AI book in the sidebar")
            if close:
                self.click(*self.center(close))
                time.sleep(1.0)
                self.record(f"{t}: closing AI book hides everything under it", self.node("cover design") is None)
                self.shot(f"{t}-15-tree-closed")
                reopen = self.node("Open AI book in the sidebar")
                if reopen:
                    self.click(*self.center(reopen))
                    time.sleep(1.5)
                    self.record(f"{t}: reopening brings back what was open inside", bool(self.node("white paper")))
            landing = self.node("landing pages")
            if landing:
                self.click(*self.center(landing))
                time.sleep(2.0)
                self.shot(f"{t}-16-name-shows-files")
                self.record(f"{t}: a click on the name shows its files on the right",
                            bool(self.node("spring launch", "--role", "AXButton")) or "landing pages" in self.window_title(),
                            self.window_title())
            # Renaming an open folder keeps it and what is open inside it open in the sidebar.
            ai_book = self.node("AI book")
            if ai_book:
                self.click(*self.center(ai_book))
                time.sleep(2.0)
                rows = [n for n in self.nodes("--title", "cover design") if n["x"] > 270]
                if rows:
                    self.click(*self.name_point(rows[0]))
                    time.sleep(0.6)
                    self.key(120)
                    time.sleep(1.0)
                    self.keystroke("cover art")
                    self.key(36)
                    time.sleep(2.5)
                    self.shot(f"{t}-15-rename-keeps-branches-open")
                    self.record(f"{t}: after renaming an open folder its branches stay open in the sidebar",
                                bool(self.node("Close cover art in the sidebar")) and bool(self.node("white paper")))
                else:
                    self.record(f"{t}: cover design is listed on the right", False)
        if not self.quit():
            self.record(f"{t}: RichFiles quit", False)

    def window_title(self):
        code, text = self.guest("osascript -e " + shlex.quote(
            'tell application "System Events" to tell process "RichFiles" to get name of window 1'))
        return text if code == 0 else ""

    def next_column_marker(self, item):
        """The x of the first item that sits in the column right after ITEM's."""
        right = item["x"] + item["w"]
        candidates = [n["x"] for n in self.nodes("--role", "AXButton")
                      if n["x"] >= right and abs(n["y"] - item["y"]) < 4]
        return min(candidates) if candidates else None


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("vm")
    parser.add_argument("--standalone-zip", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--themes", default="daybreak,sovereign", help="RichFiles themes to walk, in order")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(args.vm, args.out)
    try:
        walk.prepare(args.standalone_zip)
        for theme in args.themes.split(","):
            try:
                walk.walk(theme)
            except Exception as error:  # record and go on to the next theme
                walk.record(f"{theme}: walk step raised", False, repr(error)[:300])
                walk.quit()
    finally:
        gone = walk.quit()
        walk.record("RichFiles is quit and its process is gone", gone)
    failed = [c for c in walk.checks if not c["ok"]]
    print(f"{len(walk.checks) - len(failed)} of {len(walk.checks)} checks passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
