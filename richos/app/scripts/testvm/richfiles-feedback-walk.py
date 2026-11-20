#!/usr/bin/env python3
"""Time real row selection in the standalone RichFiles window and Finder.

Pass this walk to run-walk.py --no-app and give it a packaged RichFiles zip.
The same fixture and input are used for both apps. The result is a conservative
post-to-visible upper bound in the guest, with the changed pixels recorded.
"""
import argparse
import json
from pathlib import Path
import shlex
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
COMMAND = 1 << 20
SHIFT = 1 << 17


def run(argv, timeout=45):
    result = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return result.stdout


class Walk:
    def __init__(self, vm, out):
        self.vm = vm
        self.out = out
        self.remote = f"/Users/admin/testvm/{vm}/richfiles-feedback"
        self.folder = self.remote + "/Fixture"

    def guest(self, *args):
        return run([HERE / "guest.sh", self.vm, *args], 120)

    def ax(self, app, mode, *args):
        text = run([HERE / "ax.sh", self.vm, mode, "--app", app, *args, "--json"])
        return [json.loads(line) for line in text.splitlines() if line.startswith("{")]

    def node(self, app, *args):
        nodes = self.ax(app, "find", *args)
        return next(node for node in nodes if node.get("w") and node.get("h"))

    def push(self, path, name):
        self.guest("--push", str(path), self.remote + "/" + name)

    def shot(self, label):
        guest_path = self.remote + "/" + label + ".png"
        self.guest("screencapture", "-x", guest_path)
        self.guest("--pull", guest_path, str(self.out / (label + ".png")))

    def action(self, app, label, region, verb, argument, modifier=0, required=True):
        pid = self.guest("pgrep", "-x", app).strip()
        timing = self.guest("env", "RICHOS_TESTVM_CAPTURE=1", self.remote + "/timeline-window",
                            pid, csv(region), verb, argument, str(modifier))
        row = json.loads(timing)
        if required and row["visible_upper_ms"] is None:
            self.shot(label + "-missing")
            raise RuntimeError("the cropped region never changed after " + label)
        record = {"app": app, "action": label, "region": list(region), **row}
        with (self.out / "samples.jsonl").open("a") as evidence:
            evidence.write(json.dumps(record) + "\n")
        return record

    def launch(self, app):
        self.guest("open", "-a", app, self.folder)
        time.sleep(1.2)

    @staticmethod
    def click_point(node):
        return csv((node["x"] + min(14, node["w"] / 2), node["y"] + node["h"] / 2))

    @staticmethod
    def selection_patch(node):
        # Empty space within the row where the selected fill is drawn.
        return (node["x"] + node["w"] * .64, node["y"] + node["h"] * .34,
                max(3, min(50, node["w"] * .18)), max(3, node["h"] * .33))

    def richfiles(self, modes):
        self.launch("/Applications/RichFiles.app")
        results = []
        for mode in modes:
            self.ax("RichFiles", "click", "--title", "File view", "--first")
            self.ax("RichFiles", "click", "--title", mode, "--role", "AXMenuItem", "--first")
            time.sleep(.35)
            echo = self.node("RichFiles", "--title", "Echo.txt")
            self.action("RichFiles", f"{mode}-reset", self.selection_patch(echo),
                        "click", self.click_point(echo), required=False)
            alpha = self.node("RichFiles", "--title", "Alpha.txt")
            bravo = self.node("RichFiles", "--title", "Bravo Folder")
            delta = self.node("RichFiles", "--title", "Delta.txt")
            targets = [
                (alpha, "click", self.click_point(alpha), 0),
                (delta, "shift-click", self.click_point(delta), SHIFT),
                (bravo, "command-click", self.click_point(bravo), COMMAND),
            ]
            for node, name, input_point, modifiers in targets:
                results.append(self.action("RichFiles", f"{mode}-{name}", self.selection_patch(node),
                                           "click", input_point, modifiers))
            self.action("RichFiles", f"{mode}-pre-arrow", self.selection_patch(echo),
                        "click", self.click_point(echo), required=False)
            key = "123" if mode in ("Thumbnails", "Gallery") else "126"
            results.append(self.action("RichFiles", f"{mode}-arrow", self.selection_patch(delta), "key", key))
            self.shot("richfiles-" + mode.lower())
        return results

    def finder(self):
        self.launch("Finder")
        window = self.node("Finder", "--title", "Fixture", "--role", "AXWindow")
        # Finder's list starts 94 points below its standard title with the side
        # bar visible, as checked in the before/after screenshots below.
        left, top = window["x"], window["y"]
        self.action("Finder", "list-view", (left + 250, top + 82, 50, 20), "key", "19", COMMAND, required=False)
        self.shot("finder-before")
        alpha_point = csv((left + 225, top + 94))
        patches = [(left + 300, top + y, 50, 8) for y in (90, 130, 110)]
        results = []
        for label, offset, patch, flags in zip(("click", "shift-click", "command-click"),
                                               (94, 134, 114), patches, (0, SHIFT, COMMAND)):
            point = alpha_point if offset == 94 else csv((left + 225, top + offset))
            results.append(self.action("Finder", "List-" + label, patch, "click", point, flags))
        self.action("Finder", "List-pre-arrow", patches[0], "click", alpha_point, required=False)
        results.append(self.action("Finder", "List-arrow", patches[2], "key", "125"))
        self.shot("finder-after")
        return results

    def prepare(self, archive):
        self.guest("mkdir", "-p", self.folder + "/Bravo Folder")
        for name in ("Alpha.txt", "Delta.txt", "Echo.txt"):
            self.guest("touch", self.folder + "/" + name)
        self.push(archive, "RichFiles.zip")
        self.guest("ditto", "-x", "-k", self.remote + "/RichFiles.zip", "/Applications")
        self.guest("codesign", "--verify", "--deep", "--strict", "/Applications/RichFiles.app")
        with tempfile.TemporaryDirectory(prefix="richfiles-timeline-") as temp:
            binary = Path(temp) / "timeline-window"
            run(["swiftc", HERE.parent / "qa/timeline-window.swift", "-o", binary], 120)
            self.push(binary, "timeline-window")


def csv(values):
    return ",".join(f"{value:g}" for value in values)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("vm")
    parser.add_argument("--standalone-zip", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--modes", default="List", help="comma-separated RichFiles layouts")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    walk = Walk(args.vm, args.out)
    walk.prepare(args.standalone_zip)
    try:
        result = {"fixture": walk.folder}
        for app, measure in [("Finder", walk.finder), ("RichFiles", lambda: walk.richfiles(args.modes.split(",")))]:
            result[app] = measure()
            (args.out / "timings.json").write_text(json.dumps(result, indent=2) + "\n")
    finally:
        # Quit only the RichFiles instance opened by this walk. Finder is the
        # guest's shell and is already running before the walk starts.
        script = 'tell application id "app.richos.richfiles" to quit'
        walk.guest("if pgrep -x RichFiles >/dev/null; then osascript -e " + shlex.quote(script) + "; fi")
        walk.guest("! pgrep -x RichFiles")


if __name__ == "__main__":
    main()
