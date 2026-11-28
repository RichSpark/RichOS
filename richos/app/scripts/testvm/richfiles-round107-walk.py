#!/usr/bin/env python3
"""Walk the fifteen behaviors of RichFiles round 107.1 in a disposable guest, light and dark.

Run it as the command of run-walk.py --no-app with a packaged standalone RichFiles zip:

  run-walk.py --no-app --home EMPTY_DIR --engine NONE --report REPORT.json -- \\
      richfiles-round107-walk.py --standalone-zip RichFiles.zip --out DIR [--themes daybreak,sovereign] [--fixes]

It installs the app in the guest, builds a fixture folder with the mockup's names (129 items, so
the List view scrolls sideways) and a seven-level folder tree on the guest's Desktop, then for
each theme opens RichFiles on the fixture and drives it with the real mouse (pointer-drag.sh)
and keys (System Events), reading the result back from the guest's file system and
accessibility tree. Every check is recorded in DIR/checks.json and every look in DIR/*.png;
a failed check is recorded and the walk goes on, so one guest gives the whole picture. The
app is quit by its own Apple Event at the end and the walk confirms the process is gone.
Nothing plays sound and nothing runs on this Mac's screen.

--fixes walks only the handover review's fixes instead (2026-11-26): clicking another file to
finish a rename leaves that file selected, Down cancels a pending click, pause, click rename,
Undo Rename keeps the open sidebar branches open, the Open With submenu shows app icons, and the
chooser's Open button and the selected line under the pointer clear their contrast floors,
measured from the guest's own screen pixels.

--sheets opens every sheet with a default button instead (Connect to Server, File Actions,
Search Filters, Create ZIP, New Folder, the Open With chooser, the new file's name sheet) and
Disk Cleanup's Move to Trash, and measures from the guest's pixels each default button's label
on its fill (4.5:1) and its fill against what is around it (3:1), and each Cancel's label. The
Change the File Extension? alert, drawn by macOS, is measured as a finding in notes.json.

--menu-position uses a single long filename to check the menu anchor and the
window and display edges in the signed guest.

--menu-corners right-clicks at all four corners of the window and of the display, on a file, a
folder and empty space (three fixture folders: only files, only folders, nothing), and judges
each menu as File Explorer on Windows places it: where there is room it opens at the pointer
(it may cross the window's edge); at the display's edge it stays wholly on the display with the
pointer at its edge (added with the nightly 123 walk; judged by corner_verdict, tested in
test/richfiles-menu-corners.test.py).

--transfers copies and pastes a file into a wide List pane, then drags an item
from the other RichFiles pane. It checks receipt, selection, viewport and refresh.
"""
import argparse
import json
from statistics import median
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
CORNERS = ("top-left", "top-right", "bottom-left", "bottom-right")


def corner_verdict(point, menu, screen, window=None):
    """Is a right-click menu where File Explorer on Windows puts it? point (x, y) of the click,
    menu and window {x, y, w, h}, screen (width, height) of the whole display. Returns
    (ok, facts). The menu must be wholly on the display and the pointer at it: at its top-left
    corner where the display leaves room, otherwise at its edge after it moved to fit. With a
    window given (a click away from the display's edges) it must open at the pointer itself;
    crossing the window's edge is allowed and reported."""
    px, py = point
    x, y, w, h = menu["x"], menu["y"], menu["w"], menu["h"]
    width, height = screen
    on_display = x >= -1 and y >= -1 and x + w <= width + 1 and y + h <= height + 1
    # The native border and the first item's inset sit between the pointer and the menu's frame.
    anchored = abs(x - px) <= 12 and abs(y - py) <= 24
    touching = x - 12 <= px <= x + w + 12 and y - 24 <= py <= y + h + 24
    facts = {"pointer": [px, py], "menu": [x, y, w, h], "on_display": on_display,
             "anchored_top_left": anchored, "pointer_at_menu": touching,
             "opened_left": x + w <= px + 12, "opened_up": y + h <= py + 24}
    if window is not None:
        facts["past_window_edge"] = (x < window["x"] or y < window["y"] or x + w > window["x"] + window["w"]
                                     or y + h > window["y"] + window["h"])
        return on_display and anchored, facts
    room_right, room_down = px + w <= width, py + h <= height
    # Where the display leaves room on a side, the menu must not have moved away from it; where
    # it does not, the menu's far edge sits at the pointer (flipped) or at the display's edge
    # (slid to fit), never anywhere else around the pointer.
    natural = (not room_right or abs(x - px) <= 12) and (not room_down or abs(y - py) <= 24)
    fitted = ((room_right or abs(x + w - px) <= 12 or width - (x + w) <= 12)
              and (room_down or abs(y + h - py) <= 24 or height - (y + h) <= 12))
    return on_display and touching and natural and fitted, facts


def menu_target(items):
    """Which kind of item a RichFiles right-click menu belongs to, read from its items: a
    folder's offers Open in New Tab, a file's Open With, empty space's begins with New Folder."""
    if "Open in New Tab" in items:
        return "folder"
    if "Open With" in items:
        return "file"
    if items[:1] == ["New Folder"]:
        return "empty"
    return None


def run(argv, timeout=60):
    result = subprocess.run([str(a) for a in argv], capture_output=True, text=True, timeout=timeout)
    return result.returncode, (result.stdout + result.stderr).strip()


class Walk:
    def __init__(self, vm, out):
        self.vm, self.out = vm, out
        self.checks = []
        self.notes = []

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

    def menu_at(self, label, point, item, check):
        code, detail = self.right(*point)
        try:
            self.shot(label)
            action = self.node(item, "--role", "AXMenuItem")
            menus = self.nodes("--role", "AXMenu")
            menu = next((m for m in menus if action and
                         m["x"] <= action["x"] < m["x"] + m["w"] and
                         m["y"] <= action["y"] < m["y"] + m["h"]), None)
            self.record(label, code == 0 and menu is not None and check(menu),
                        f"pointer {point}, menu {menu}, action {item}: {bool(action)}"
                        + (f", input error: {detail}" if code else ""))
        finally:
            self.key(53)

    def menu_position(self, theme):
        folder = "/Users/admin/Menu Placement Fixture"
        name = "Quarterly report with a deliberately long filename for the pointer test.txt"
        code, detail = self.guest(
            f"mkdir -p {shlex.quote(folder)} && touch {shlex.quote(folder + '/' + name)} && "
            f"defaults write {BUNDLE} richfiles.theme -string {shlex.quote(theme)} && "
            f"open -a /Applications/RichFiles.app {shlex.quote(folder)}")
        if code:
            raise RuntimeError(detail)
        deadline = time.monotonic() + 30
        while not self.node(name) and time.monotonic() < deadline:
            time.sleep(0.25)
        if not self.node(name):
            raise RuntimeError("RichFiles did not open the menu fixture")

        def move(x, y):
            target = 'tell application "System Events" to tell process "RichFiles" to '
            code, detail = self.guest("osascript -e " + shlex.quote(target + "set size of window 1 to {920, 560}")
                                      + " -e " + shlex.quote(target + f"set position of window 1 to {{{x}, {y}}}"))
            if code:
                raise RuntimeError(detail)
            window = self.nodes("--role", "AXWindow")
            return window[0]

        def view(mode):
            for title, match in [("File view", []), (mode, ["--role", "AXMenuItem"])]:
                code, detail = run([HERE / "ax.sh", self.vm, "click", "--app", APP,
                                    "--title", title, *match, "--first", "--json"])
                if code:
                    raise RuntimeError(f"could not choose {mode} via {title}: {detail}")
            if not self.node(name):
                raise RuntimeError(f"file absent in {mode}")

        def near(point, menu):
            # Allow the native border and the inset around the first menu item.
            return abs(menu["x"] - point[0]) <= 12 and abs(menu["y"] - point[1]) <= 24

        def at_pointer(label, point, action="Open", extra=lambda _: True):
            self.menu_at(f"{theme}-{label}", point, action, lambda menu: near(point, menu) and extra(menu))

        window = move(105, 110)
        for mode in ("List", "Columns", "Thumbnails", "Details", "Gallery"):
            view(mode)
            row = self.node(name)
            if mode == "List":
                for offset in (40, 170, 320):
                    at_pointer(f"list-name-{offset}", (round(row["x"] + offset), self.center(row)[1]))
            else:
                at_pointer(mode.lower(), (round(row["x"] + min(48, row["w"] / 2)), self.center(row)[1]))

        view("List")
        pin = self.node("Pin Current Folder")
        if not pin:
            raise RuntimeError("Pin Current Folder is absent")
        self.click(*self.center(pin))
        favorite = next((n for n in self.nodes("--title", "Menu Placement Fixture", "--role", "AXButton")
                         if n["y"] >= pin["y"] - 10), None)
        if not favorite:
            raise RuntimeError("the fixture did not appear in Quick Access")
        at_pointer("sidebar", self.center(favorite), "Remove from Quick Access")
        volume = self.node("Macintosh HD", "--role", "AXButton")
        if not volume:
            raise RuntimeError("the startup volume is absent from the sidebar")
        at_pointer("sidebar-volume", self.center(volume), "Copy Path")

        right = window["x"] + window["w"]
        bottom = window["y"] + window["h"]
        at_pointer("window-right", (round(right - 15), round(bottom - 300)), "New Folder",
                   extra=lambda m: m["x"] + m["w"] > right)
        at_pointer("window-bottom", (round(right - 345), round(bottom - 45)), "New Folder",
                   extra=lambda m: m["y"] + m["h"] > bottom)

        # run-walk.py supplies a single display. Get its bounds from the guest,
        # rather than assuming the dimensions of the default test VM.
        code, display = self.guest("osascript -l JavaScript -e " + shlex.quote(
            'ObjC.import("AppKit"); var screen = $.NSScreen.mainScreen;'
            'var full = screen.frame, usable = screen.visibleFrame;'
            'JSON.stringify([usable.origin.x + usable.size.width, full.size.height - usable.origin.y])'))
        if code:
            raise RuntimeError(display)
        screen_right, screen_bottom = json.loads(display)
        window = move(round(screen_right - 920), 110)
        right = window["x"] + window["w"]
        bottom = window["y"] + window["h"]
        for label, y, action in [
            ("display-right-empty", round(bottom - 305), "New Folder"),
            ("display-right-file", self.center(self.node(name))[1], "Open"),
        ]:
            point = (round(right - 95), y)
            self.menu_at(f"{theme}-{label}", point, action,
                         lambda m: abs(m["y"] - point[1]) <= 24 and 0 <= screen_right - m["x"] - m["w"] <= 12
                         and m["x"] < point[0] < m["x"] + m["w"])
        window = move(round(screen_right - 920), round(screen_bottom - 560))
        point = (round(window["x"] + window["w"] - 345), round(window["y"] + window["h"] - 45))
        self.menu_at(f"{theme}-display-bottom", point, "New Folder",
                     lambda m: abs(m["x"] - point[0]) <= 12 and m["y"] < point[1] - 24
                     and m["y"] + m["h"] <= screen_bottom + 1)

    def menu_corners(self, theme):
        base = "/Users/admin/Menu Corners"
        kinds = {"file": base + "/Only files", "folder": base + "/Only folders", "empty": base + "/Nothing"}
        code, detail = self.guest(
            f"rm -rf {shlex.quote(base)} && mkdir -p " + " ".join(shlex.quote(f) for f in kinds.values())
            + f" && cd {shlex.quote(kinds['file'])} && for i in $(seq -w 1 60); do printf 'x' > \"Corner file $i.txt\"; done"
            + f" && cd {shlex.quote(kinds['folder'])} && for i in $(seq -w 1 60); do mkdir \"Corner folder $i\"; done"
            + f" && defaults write {BUNDLE} richfiles.theme -string {shlex.quote(theme)}")
        if code:
            raise RuntimeError(detail)
        code, display = self.guest("osascript -l JavaScript -e " + shlex.quote(
            'ObjC.import("AppKit"); var f = $.NSScreen.mainScreen.frame;'
            'JSON.stringify([f.size.width, f.size.height])'))
        if code:
            raise RuntimeError(display)
        screen = tuple(json.loads(display))
        # The Dock spans most of the guest display's bottom edge: hidden, so a click near the
        # display's bottom corners reaches the window, as on a Mac whose Dock hides or sits aside.
        self.guest("defaults write com.apple.dock autohide -bool true && killall Dock")
        time.sleep(3)
        target = 'tell application "System Events" to tell process "RichFiles" to '

        def place(x, y, w, h):
            # A window RichFiles has just opened can still be put back where it last was: the
            # frame is read back and set again until it holds. Only the browser window is moved:
            # RichFiles can also hold a small system-dialog window, which "window 1" may name.
            window = '(first window whose subrole is "AXStandardWindow")'
            for _ in range(5):
                code, detail = self.guest("osascript -e " + shlex.quote(target + f"set size of {window} to {{{w}, {h}}}")
                                          + " -e " + shlex.quote(target + f"set position of {window} to {{{x}, {y}}}"))
                if code:
                    raise RuntimeError(detail)
                time.sleep(0.8)
                windows = [n for n in self.nodes("--role", "AXWindow") if n.get("sub") == "AXStandardWindow"]
                if windows and abs(windows[0]["x"] - x) <= 2 and abs(windows[0]["y"] - y) <= 2 \
                        and abs(windows[0]["w"] - w) <= 2 and abs(windows[0]["h"] - h) <= 2:
                    return windows[0]
            raise RuntimeError(f"RichFiles' window did not hold the frame {x},{y} {w}x{h}: {windows[:1]}")

        def show(kind):
            self.quit()
            code, detail = self.guest("open -a /Applications/RichFiles.app " + shlex.quote(kinds[kind]))
            if code:
                raise RuntimeError(detail)
            expect = {"file": "Corner file 01.txt", "folder": "Corner folder 01", "empty": None}[kind]
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if self.nodes("--role", "AXWindow") and (expect is None or self.node(expect)):
                    break
                time.sleep(0.5)
            else:
                raise RuntimeError(f"RichFiles did not open {kinds[kind]}")
            # Details: one full-width row per item, so a corner of the list is on a file or a
            # folder; List wraps into columns and its last column can leave a corner empty.
            for title, match in [("File view", []), ("Details", ["--role", "AXMenuItem"])]:
                run([HERE / "ax.sh", self.vm, "click", "--app", APP, "--title", title, *match, "--first", "--json"])
            time.sleep(0.5)

        # Where the list's first and last fully shown rows and its left edge sit in a window of a
        # given size, measured on the files folder; the same offsets serve the other two folders.
        def offsets(w, h):
            window = place(200, 60, w, h)
            rows = [n for n in self.nodes("--title", "Corner file", "--contains")
                    if n["y"] >= window["y"] and n["y"] + n["h"] <= window["y"] + window["h"] - 40]
            if len(rows) < 4:
                raise RuntimeError(f"too few list rows in view to find the corners: {len(rows)}")
            top, bottom = min(rows, key=lambda n: n["y"]), max(rows, key=lambda n: n["y"])
            return {"left": round(top["x"] + 30 - window["x"]), "right": w - 40,
                    "top": round(top["y"] + top["h"] / 2 - window["y"]),
                    "bottom": round(bottom["y"] + bottom["h"] / 2 - window["y"])}

        def at(window, off, corner):
            v, hz = corner.split("-")
            return (round(window["x"] + off[hz]), round(window["y"] + off[v]))

        def menu_after_right_click(point):
            code, detail = self.right(*point)
            time.sleep(0.8)
            menus = [m for m in self.nodes("--role", "AXMenu") if m["h"] > 60]
            menu = max(menus, key=lambda m: m["h"]) if menus else None
            items = []
            if menu:
                items = [n.get("title", "") for n in sorted(self.nodes("--role", "AXMenuItem"), key=lambda n: n["y"])
                         if menu["x"] <= n["x"] < menu["x"] + menu["w"] and menu["y"] <= n["y"] < menu["y"] + menu["h"]
                         and n.get("title")]
            return code, detail, menu, items

        def judge(label, point, window_rule):
            code, detail, menu, items = menu_after_right_click(point)
            try:
                self.shot(label)
                if menu is None:
                    self.record(label, False, f"no menu after the right-click at {point}"
                                + (f", input error: {detail}" if code else ""))
                    return
                ok, facts = corner_verdict(point, menu, screen, window_rule)
                facts["items"] = items[:6]
                self.record(label, code == 0 and ok, json.dumps(facts))
                # Placement is judged above; whether the click landed on the kind of item meant
                # (a file, a folder, empty space) is its own check, so a miss is never read as one.
                hit = menu_target(items)
                self.record(label + " (target)", hit == label.rsplit("-", 1)[1], f"menu of: {hit}")
            finally:
                self.key(53)
                time.sleep(0.4)

        show("file")
        middle = offsets(920, 480)
        edge = offsets(920, 560)
        self.record(f"{theme}-corners: display and list measured", True,
                    json.dumps({"screen": screen, "window_offsets": middle, "display_offsets": edge}))
        for kind in ("file", "folder", "empty"):
            show(kind)
            # The window's four corners, away from the display's edges.
            window = place(round((screen[0] - 920) / 2), 60, 920, 480)
            for corner in CORNERS:
                judge(f"{theme}-corners-window-{corner}-{kind}", at(window, middle, corner), window)
            # The display's four corners: the window's matching corner pushed into each.
            for corner in CORNERS:
                v, hz = corner.split("-")
                x = 0 if hz == "left" else screen[0] - 920
                y = 25 if v == "top" else screen[1] - 560
                window = place(x, y, 920, 560)
                judge(f"{theme}-corners-display-{corner}-{kind}", at(window, edge, corner), None)
        self.quit()

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

        # 8: right-click an unselected item: it is selected and its menu opens at the pointer.
        html = self.node("landing-page-v1.html")
        point = self.name_point(html)
        self.right(*point)
        time.sleep(1.0)
        self.shot(f"{t}-08-right-click-selects")
        menus = [m for m in self.nodes("--role", "AXMenu") if m["h"] > 100]
        if html and menus:
            menu = menus[0]
            anchored = abs(menu["x"] - point[0]) <= 12 and abs(menu["y"] - point[1]) <= 24
            self.record(f"{t}: the menu opens at the pointer", anchored,
                        f"pointer {point}, menu x {menu['x']} y {menu['y']}")
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

    # -- the handover review's fixes (--fixes) ------------------------------------------------
    def image(self, name):
        from PIL import Image
        with Image.open(self.out / f"{name}.png") as picture:
            picture = picture.convert("RGB")
        # The accessibility tree is in points; the capture may be at a higher scale. The screen's
        # width in points comes from AppKit (JXA), which needs no Apple Event to another app.
        code, text = self.guest("osascript -l JavaScript -e " + shlex.quote(
            'ObjC.import("AppKit"); $.NSScreen.mainScreen.frame.size.width'))
        try:
            width = float(text.strip().splitlines()[-1]) if code == 0 else picture.width
        except (ValueError, IndexError):
            width = picture.width
        return picture, picture.width / width

    @staticmethod
    def luminance(rgb):
        linear = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in (v / 255 for v in rgb)]
        return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]

    @classmethod
    def contrast(cls, a, b):
        high, low = sorted((cls.luminance(a), cls.luminance(b)), reverse=True)
        return (high + 0.05) / (low + 0.05)

    @staticmethod
    def median_color(picture, box):
        pixels = list(picture.crop(tuple(round(v) for v in box)).getdata())
        return tuple(round(median(channel)) for channel in zip(*pixels))

    def transfers(self, theme):
        root = f"{REMOTE}/transfers"
        source, destination = f"{root}/Source", f"{root}/Destination"
        paste_name, drag_name = "Z pasted.txt", "A dragged.txt"
        head, tail = "Middle entry 000.txt", "Middle entry 179.txt"
        timing = "/Users/admin/Library/Logs/RichOS/richfiles-timing.jsonl"
        setup = (
            f"rm -rf {root} && mkdir -p {source} {destination} && "
            f"printf paste > {shlex.quote(source + '/' + paste_name)} && "
            f"printf drag > {shlex.quote(source + '/' + drag_name)} && "
            f"cd {destination} && for i in $(jot -w %03d 180 0); do touch \"Middle entry $i.txt\"; done"
        )
        code, detail = self.guest(setup)
        if code:
            raise RuntimeError(detail)
        self.guest(f"defaults write {BUNDLE} richfiles.theme -string {theme}")
        self.guest("osascript -e 'set volume with output muted'")
        _, audio = run([HERE / "guest-sound.sh", self.vm])
        (self.out / f"{theme}-guest-sound.txt").write_text(audio + "\n")
        code, detail = self.guest(
            f"open -a /Applications/RichFiles.app {destination}")
        if code:
            raise RuntimeError(detail)

        def wait_for(label, read):
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                result = read()
                if result:
                    return result
                time.sleep(0.25)
            raise RuntimeError(label)

        def item(name):
            return self.node(name, "--role", "AXButton")

        wait_for("destination did not open", lambda: item(head))
        code, detail = self.guest("osascript -e " + shlex.quote(
            'tell application "System Events" to tell process "RichFiles" to '
            'set position of window 1 to {0, 25}') + " -e " + shlex.quote(
            'tell application "System Events" to tell process "RichFiles" to set size of window 1 to {1600, 960}'))
        if code:
            raise RuntimeError(detail)

        split = wait_for("the Split View control is missing", lambda: self.node("Split View", "--role", "AXButton"))
        self.click(*self.center(split))
        wait_for("the split pane did not open", lambda: self.node("Close Split View", "--role", "AXButton"))
        def split_rows():
            nodes = self.nodes("--title", head, "--role", "AXButton")
            return sorted(nodes, key=lambda node: node["x"]) if len(nodes) == 2 else None

        items = wait_for("the destination folder is missing from one of the two panes", split_rows)
        self.click(*self.center(items[0]))
        up = wait_for("the Up control is missing", lambda: self.node("Up", "--role", "AXButton"))
        self.click(*self.center(up))
        source_row = wait_for("the parent listing does not show Source", lambda: item("Source"))
        self.double(*self.center(source_row))
        wait_for("the left pane did not navigate to Source", lambda: item(paste_name))

        def panes():
            nodes = self.nodes("--title", "List view, filenames and extensions")
            return sorted(nodes, key=lambda node: node["x"]) if len(nodes) == 2 else None

        _, right = wait_for("the two List viewport frames are missing", panes)

        def in_right(node):
            if not node:
                return False
            x, y = self.center(node)
            return right["x"] <= x < right["x"] + right["w"] and right["y"] <= y < right["y"] + right["h"]

        def receive(name):
            return next((node for node in self.nodes("--title", name, "--role", "AXButton")
                         if in_right(node) and node["selected"] is True), None)

        self.record(f"{theme}: paste target starts beyond the visible List columns",
                    in_right(item(head)) and not in_right(item(tail)))
        self.shot(f"{theme}-transfer-01-before-paste")
        self.click(*self.center(item(paste_name)))
        self.keystroke("c", CONTROL)
        clipboard = "ObjC.import('AppKit'); ObjC.unwrap($.NSPasteboard.generalPasteboard.stringForType('public.file-url'))"

        def copied():
            code, value = self.guest("osascript -l JavaScript -e " + shlex.quote(clipboard))
            return code == 0 and value.endswith("/Z%20pasted.txt")

        try:
            wait_for("copy did not reach the guest file clipboard", copied)
        except RuntimeError:
            self.shot(f"{theme}-transfer-copy-failed")
            raise
        self.click(*self.center(item(head)))
        self.keystroke("v", CONTROL)
        try:
            pasted = wait_for("pasted file was not selected in the destination viewport", lambda: receive(paste_name))
        except RuntimeError:
            self.shot(f"{theme}-transfer-paste-failed")
            self.record(f"{theme}: paste file on disk", paste_name in self.ls(destination),
                        json.dumps(self.nodes("--title", paste_name, "--role", "AXButton")))
            raise
        self.record(f"{theme}: keyboard paste reveals and selects a copy at the far end",
                    paste_name in self.ls(source) and paste_name in self.ls(destination), json.dumps(pasted))
        self.shot(f"{theme}-transfer-02-paste")

        self.record(f"{theme}: drag target starts beyond the visible List columns",
                    not in_right(item(head)) and in_right(pasted))
        moving = wait_for("drag source is missing", lambda: item(drag_name))
        start_x, start_y = self.center(moving)
        end_x = round(right["x"] + right["w"] * 0.75)
        end_y = round(right["y"] + right["h"] * 0.5)
        code, detail = self.pointer(f"{start_x},{start_y}", f"{end_x},{end_y}", "--rest", "0.5")
        if code:
            raise RuntimeError(detail)
        moved = wait_for("dragged file was not selected in the destination viewport", lambda: receive(drag_name))
        self.record(f"{theme}: real mouse drag moves, selects and reveals the file at the start of name order",
                    drag_name not in self.ls(source) and drag_name in self.ls(destination) and in_right(item(head)),
                    json.dumps(moved))
        self.shot(f"{theme}-transfer-03-drag")
        def samples():
            code, raw = self.guest(f"tail -n 80 {timing}")
            return [json.loads(line) for line in raw.splitlines() if line.startswith("{")] if code == 0 else []

        # A click in the pane heading activates it without selecting a file.
        self.click(end_x, round(right["y"] - 10))
        refresh = wait_for("Refresh is missing", lambda: self.node("Refresh", "--role", "AXButton"))
        previous = {row["id"] for row in samples() if row["operation"] == "listing"}
        self.click(*self.center(refresh))
        wait_for("Refresh did not finish a new listing", lambda: any(
            row["operation"] == "listing" and row["id"] not in previous for row in samples()))
        refreshed = wait_for("refresh lost the drag's selection or viewport", lambda: receive(drag_name))
        self.record(f"{theme}: refresh keeps the moved item selected and visible", in_right(refreshed), json.dumps(refreshed))
        self.shot(f"{theme}-transfer-04-refresh")

        records = samples()
        selections = [row for row in records if row.get("operation") == "transferSelection"]
        (self.out / f"{theme}-transfer-timing.json").write_text(json.dumps(selections, indent=2) + "\n")
        operations = {row["id"]: row["operation"] for row in records if row.get("operation") in {"copy", "move"}}
        completed = {operations.get(row["id"]) for row in selections
                     if row.get("outcome") == "complete" and row.get("confirmed") == row.get("total") == 1}
        self.record(f"{theme}: transfer diagnostics correlate both selections with their worker receipts",
                    {"copy", "move"} <= completed, json.dumps(selections[-2:]))

    def selected_copy(self):
        """Ctrl+C, Ctrl+V duplicates whatever is selected: the new copy's name says which item that is."""
        before = {n for n in self.ls() if " - Copy" in n}
        self.keystroke("c", CONTROL)
        time.sleep(0.4)
        self.keystroke("v", CONTROL)
        for _ in range(30):
            made = {n for n in self.ls() if " - Copy" in n} - before
            if made:
                return made
            time.sleep(0.5)
        return set()

    def fixes(self, theme):
        t = theme
        self.launch(theme)

        # Fix 1: clicking another file finishes an inline rename and leaves THAT file selected.
        self.click(*self.name_point(self.node("test1.txt")))
        time.sleep(0.6)
        self.key(120)
        time.sleep(1.0)
        self.keystroke("renamed-one")
        time.sleep(0.4)
        self.click(*self.name_point(self.node("todo.txt")))
        time.sleep(2.0)
        self.shot(f"{t}-fix1-click-finishes-rename")
        self.record(f"{t}: clicking todo.txt saved the rename", "renamed-one.txt" in self.ls())
        made = self.selected_copy()
        self.record(f"{t}: the clicked file (todo.txt) stays selected after the rename is saved",
                    made == {"todo - Copy.txt"}, "Ctrl+C, Ctrl+V made " + (", ".join(sorted(made)) or "nothing"))

        # Fix 2: click, pause, click, then Down before the box opens: no box, the selection moved.
        # The box opens one double-click interval (0.5 s) after the second click. A key sent by a
        # separate command lands later than that, so Down is pressed 0.1 s after the button comes
        # up by the same guest process (pointer-drag.sh --then-key). An open box would take
        # Ctrl+C, Ctrl+V as text, so a file copy proves there is none.
        readme = self.node("readme.txt")
        x, y = self.name_point(readme)
        self.click(x, y)
        time.sleep(1.5)
        self.pointer(f"{x},{y}", f"{x},{y}", "--then-key", "125", "--then-after", "0.1")
        time.sleep(2.0)
        self.shot(f"{t}-fix2-down-cancels-pending-rename")
        made = self.selected_copy()
        self.record(f"{t}: Down before the name box opened canceled it and the selection stayed moved",
                    len(made) == 1 and "readme - Copy.txt" not in made,
                    "Down 0.1 s after the second click; Ctrl+C, Ctrl+V made " + (", ".join(sorted(made)) or "nothing"))

        # Fix 5: the Open With submenu shows each app's icon beside its name.
        html = self.node("landing-page-v1.html")
        self.right(*self.name_point(html))
        time.sleep(1.0)
        open_with = self.node("Open With", "--role", "AXMenuItem")
        if open_with:
            self.pointer("%d,%d" % self.center(open_with), "--hover")
            time.sleep(1.2)
            # The icons are a look, judged on this screenshot: app icons vary too much (a white
            # TextEdit page, a dark Safari compass) for a pixel rule to tell an icon from a name.
            self.shot(f"{t}-fix5-open-with-icons")
            another = self.node("Choose Another App…", "--role", "AXMenuItem")
            apps = [n.get("title") for n in self.nodes("--role", "AXMenuItem")
                    if another and abs(n["x"] - another["x"]) < 4 and n["y"] < another["y"] - 4 and n.get("title")]
            self.record(f"{t}: Open With lists the apps (their icons are in the screenshot)", bool(apps), ", ".join(apps))
            # Fix 4: the chooser's Open button clears 4.5:1 for its label.
            if another:
                self.pointer("%d,%d" % (another["x"] + 10, another["y"] + another["h"] // 2), "--hover")
                time.sleep(0.4)
                self.click(*self.center(another))
                time.sleep(2.5)
                # Open is enabled once the chooser has found the apps and selected the default
                # one; disabled, it is drawn at half strength (an exempt, inactive control).
                button = None
                for _ in range(20):
                    button = self.node("Open", "--role", "AXButton")
                    if button and button.get("enabled") is True:
                        break
                    time.sleep(1)
                self.shot(f"{t}-fix4-chooser-open-button")
                if button and button.get("enabled") is not True:
                    self.record(f"{t}: the chooser's Open button became enabled", False, repr(button.get("enabled")))
                if button:
                    picture, scale = self.image(f"{t}-fix4-chooser-open-button")
                    x, y, w, h = (button[k] * scale for k in ("x", "y", "w", "h"))
                    fill = self.median_color(picture, (x + 4, y + 4, x + 12, y + h - 4))
                    label = picture.crop(tuple(round(v) for v in (x + 14, y + 6, x + w - 14, y + h - 6))).getdata()
                    ink = max(label, key=lambda p: self.contrast(p, fill))
                    ratio = self.contrast(ink, fill)
                    self.record(f"{t}: the chooser's Open label clears 4.5:1 on its fill", ratio >= 4.5,
                                f"fill {fill}, label {ink}, {ratio:.2f}:1")
                else:
                    self.record(f"{t}: the chooser's Open button was found", False)
                self.key(53)
                time.sleep(0.8)
        else:
            self.record(f"{t}: the right-click menu has Open With", False)
            self.key(53)

        # Fix 6: the selected item's 1 px line under the pointer clears 3:1 against its fill.
        item = self.node("yet another test file.txt")
        self.click(*self.name_point(item))
        time.sleep(0.6)
        self.hover(*self.name_point(item))
        time.sleep(0.3)
        self.shot(f"{t}-fix6-selected-under-pointer")
        picture, scale = self.image(f"{t}-fix6-selected-under-pointer")
        x, y, w, h = (item[k] * scale for k in ("x", "y", "w", "h"))
        inside = x + w - 10 * scale
        fill = self.median_color(picture, (inside - 4 * scale, y + h / 2 - 2 * scale, inside, y + h / 2 + 2 * scale))
        scan = [picture.getpixel((round(inside), round(y + h + d))) for d in range(-4, 5)] \
            + [picture.getpixel((round(inside), round(y + d))) for d in range(-4, 5)]
        line = max(scan, key=lambda p: self.contrast(p, fill))
        ratio = self.contrast(line, fill)
        self.record(f"{t}: the selected line under the pointer clears 3:1 against its fill", ratio >= 3,
                    f"fill {fill}, line {line}, {ratio:.2f}:1")

        # Fix 3: Undo Rename keeps the open sidebar branches open.
        desktop = self.node("Open Desktop in the sidebar")
        if desktop:
            self.click(*self.center(desktop))
            time.sleep(1.5)
            for name in ("AI book", "cover design"):
                arrow = self.node(f"Open {name} in the sidebar")
                if arrow:
                    self.click(*self.center(arrow))
                    time.sleep(1.2)
            self.click(*self.center(self.node("AI book")))
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
                renamed = bool(self.node("Close cover art in the sidebar"))
                self.keystroke("z", CONTROL)
                time.sleep(2.5)
                self.shot(f"{t}-fix3-undo-rename-keeps-branches-open")
                self.record(f"{t}: after Undo Rename the folder and what is open inside stay open in the sidebar",
                            renamed and bool(self.node("Close cover design in the sidebar"))
                            and bool(self.node("book cover complete set")),
                            f"open after rename {renamed}")
            else:
                self.record(f"{t}: cover design is listed on the right", False)
        else:
            self.record(f"{t}: Desktop has an arrow", False)
        if not self.quit():
            self.record(f"{t}: RichFiles quit", False)

    # -- every sheet's default button (--sheets) -----------------------------------------------
    def menu(self, menu, item):
        self.guest("osascript -e " + shlex.quote(f'tell application id "{BUNDLE}" to activate'))
        time.sleep(0.4)
        return self.guest("osascript -e " + shlex.quote(
            f'tell application "System Events" to tell process "{APP}" to '
            f'click menu item "{item}" of menu "{menu}" of menu bar 1'))

    def sheet_button(self, title, seconds=15):
        """The enabled button drawn by SheetButtonStyle (at least 84 x 32 points) named TITLE."""
        deadline = time.monotonic() + seconds
        found = []
        while time.monotonic() < deadline:
            found = [n for n in self.nodes("--title", title, "--role", "AXButton") if n["w"] >= 83 and n["h"] >= 31]
            if any(n.get("enabled") is True for n in found):
                return next(n for n in found if n.get("enabled") is True)
            time.sleep(1)
        return found[0] if found else None

    def measure(self, shot, button, what, primary, note=False):
        """A button's label against its fill and, for a default button, its fill against what is
        around it, read from the guest's own screen pixels. With NOTE the reading is recorded as a
        finding (notes.json), not as a check."""
        if not button:
            self.record(f"{what}: the button was found", False)
            return
        picture, scale = self.image(shot)
        x, y, w, h = (button[k] * scale for k in ("x", "y", "w", "h"))
        fill = self.median_color(picture, (x + 4 * scale, y + 6 * scale, x + 9 * scale, y + h - 6 * scale))
        label = picture.crop(tuple(round(v) for v in (x + 12 * scale, y + 6 * scale, x + w - 12 * scale, y + h - 6 * scale))).getdata()
        ink = max(label, key=lambda p: self.contrast(p, fill))
        ratio = self.contrast(ink, fill)
        detail = f"label {ink} on fill {fill}, {ratio:.2f}:1"
        passed = ratio >= 4.5
        if primary:
            around = self.median_color(picture, (x - 7 * scale, y + h / 2 - 2 * scale, x - 3 * scale, y + h / 2 + 2 * scale))
            edge = self.contrast(fill, around)
            detail += f"; fill against {around} around it, {edge:.2f}:1"
            passed = passed and edge >= 3
        if note:
            self.notes.append({"finding": what, "detail": detail})
            (self.out / "notes.json").write_text(json.dumps(self.notes, indent=2) + "\n")
            print("NOTE " + what + ": " + detail, flush=True)
        else:
            enabled = button.get("enabled")
            self.record(f"{what} clears AA ({'label 4.5:1, fill 3:1' if primary else 'label 4.5:1'})",
                        passed and enabled is True, detail + ("" if enabled is True else f"; enabled {enabled!r}"))

    def sheet(self, t, slug, default, what, cancel="Cancel"):
        shot = f"{t}-sheet-{slug}"
        button = self.sheet_button(default)
        time.sleep(0.5)
        self.shot(shot)
        self.measure(shot, button, f"{t}: {what}'s {default}", primary=True)
        if cancel:
            self.measure(shot, self.sheet_button(cancel, 3), f"{t}: {what}'s {cancel}", primary=False)
        self.key(53)
        time.sleep(1.0)

    def sheets(self, theme):
        t = theme
        self.launch(theme)

        connect = self.node("Connect to Server", "--role", "AXButton")
        if connect:
            self.click(*self.center(connect))
            time.sleep(1.5)
            self.sheet(t, "connect", "Connect", "Connect to Server")
        else:
            self.record(f"{t}: the Connect to Server button was found", False)

        self.menu("Files", "File Actions…")
        time.sleep(1.5)
        self.sheet(t, "file-actions", "Run", "File Actions")

        scope = self.node("Search Scope")
        if scope:
            filters = None
            for _ in range(2):  # the first open of a menu can be slow to reach the tree
                self.click(*self.center(scope))
                time.sleep(1.5)
                filters = self.node("Search Filters…", "--role", "AXMenuItem")
                if filters:
                    break
                self.key(53)
                time.sleep(0.5)
            if filters:
                self.click(*self.center(filters))
                time.sleep(1.5)
                self.sheet(t, "search-filters", "Search", "Search Filters")
            else:
                self.record(f"{t}: the Search Scope menu has Search Filters…", False)
                self.key(53)
        else:
            self.record(f"{t}: the Search Scope menu was found", False)

        html = self.node("landing-page-v1.html")
        self.right(*self.name_point(html))
        time.sleep(1.0)
        zip_item = self.node("Create ZIP…", "--role", "AXMenuItem")
        if zip_item:
            self.click(*self.center(zip_item))
            time.sleep(1.5)
            self.sheet(t, "create-zip", "Create", "Create ZIP", cancel="Close")
        else:
            self.record(f"{t}: the right-click menu has Create ZIP…", False)
            self.key(53)

        self.menu("File", "New Folder")
        time.sleep(1.5)
        self.sheet(t, "new-folder", "Create", "New Folder")

        self.right(*self.name_point(self.node("landing-page-v1.html")))
        time.sleep(1.0)
        open_with = self.node("Open With", "--role", "AXMenuItem")
        another = None
        if open_with:
            self.pointer("%d,%d" % self.center(open_with), "--hover")
            time.sleep(1.2)
            another = self.node("Choose Another App…", "--role", "AXMenuItem")
        if another:
            self.pointer("%d,%d" % (another["x"] + 10, another["y"] + another["h"] // 2), "--hover")
            time.sleep(0.4)
            self.click(*self.center(another))
            time.sleep(2.5)
            self.sheet(t, "open-with", "Open", "Open With chooser")
        else:
            self.record(f"{t}: the right-click menu has Open With, Choose Another App…", False)
            self.key(53)
            self.key(53)

        # Disk Cleanup's Move to Trash is enabled once one measured item is in review.
        self.menu("Files", "Disk Usage…")
        add = []
        for _ in range(60):
            add = [n for n in self.nodes("--title", " to review", "--contains", "--role", "AXButton")
                   if (n.get("title") or n.get("desc") or "").startswith("Add ") and n.get("enabled") is True]
            if add:
                break
            time.sleep(1)
        if add:
            self.click(*self.center(add[0]))
            time.sleep(1.5)
            self.sheet(t, "disk-cleanup-move-to-trash", "Move to Trash", "Disk Cleanup", cancel=None)
        else:
            self.record(f"{t}: Disk Cleanup listed an item to add to review", False)
        # Back to the browser by Disk Usage's own back button (Esc reaches it only with focus inside).
        back = [n for n in self.nodes("--title", "Home107", "--role", "AXButton") if n["x"] < 200 and n["y"] < 120]
        if back:
            self.click(*self.center(back[0]))
        time.sleep(1.5)

        # New Text File in an empty folder opens the name sheet (Rename or Create). Changing the
        # extension there asks in a system alert, which takes no SwiftUI button style: its default
        # button is measured as a finding, not as a check of this change.
        webinars = self.node("Webinars")
        if webinars:
            self.double(*self.name_point(webinars))
            time.sleep(2.0)
            self.right(1000, 520)
            time.sleep(1.0)
            new_text = self.node("New Text File", "--role", "AXMenuItem")
            if new_text:
                self.click(*self.center(new_text))
                time.sleep(2.0)
                title = "Rename" if self.sheet_button("Rename", 5) else "Create"
                shot = f"{t}-sheet-new-file-name"
                button = self.sheet_button(title)
                self.shot(shot)
                self.measure(shot, button, f"{t}: New File's {title}", primary=True)
                self.measure(shot, self.sheet_button("Cancel", 3), f"{t}: New File's Cancel", primary=False)
                self.keystroke("a", COMMAND)
                self.keystroke("New File.md")
                self.key(36)
                time.sleep(1.5)
                alert = [n for n in self.nodes("--title", "Change Extension", "--role", "AXButton")]
                shot = f"{t}-alert-change-extension"
                self.shot(shot)
                if alert:
                    self.measure(shot, alert[0], f"{t}: the Change the File Extension? alert's default button "
                                 "(system-drawn, not changed here)", primary=False, note=True)
                else:
                    self.notes.append({"finding": f"{t}: the extension alert did not appear", "detail": ""})
                self.key(53)
                time.sleep(1.0)
                self.key(53)
                time.sleep(1.0)
            else:
                self.record(f"{t}: the empty folder's right-click menu has New Text File", False)
                self.key(53)
        else:
            self.record(f"{t}: Webinars is listed", False)
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
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--fixes", action="store_true", help="walk only the handover review's fixes")
    mode.add_argument("--transfers", action="store_true", help="walk real paste and drag selection between two panes")
    mode.add_argument("--sheets", action="store_true", help="open every sheet with a default button and measure it")
    mode.add_argument("--menu-position", action="store_true", help="check context menu placement")
    mode.add_argument("--menu-corners", action="store_true",
                      help="right-click at the four corners of the window and of the display on a file, a folder and empty space")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(args.vm, args.out)
    try:
        walk.prepare(args.standalone_zip)
        for theme in args.themes.split(","):
            try:
                if args.transfers:
                    walk.transfers(theme)
                    walk.quit()
                elif args.menu_position:
                    walk.menu_position(theme)
                elif args.menu_corners:
                    walk.menu_corners(theme)
                else:
                    (walk.fixes if args.fixes else walk.sheets if args.sheets else walk.walk)(theme)
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
