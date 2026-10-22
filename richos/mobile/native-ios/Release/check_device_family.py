#!/usr/bin/env python3
"""Refuses an app the App Store Connect processing would reject for its device family.

    check_device_family.py <path to an .app, or to an .xcarchive>

RichConnect is an iPhone app: the app and every extension must declare exactly `UIDeviceFamily` [1].
On 2026-10-04 a build declared [1, 2] (XcodeGen gave each target its own "1,2" default over the
project-level "1"); Apple refused it with error 90023 (no 152x152 and no 167x167 iPad icon) and
error 90474 (iPad multitasking needs all four orientations). This reads what Apple reads, from the
built Info.plists and the compiled asset catalog, so it needs no signing and no network.

Checks, each printed as one line; exit 0 only when all pass:
  - the app and every PlugIns/*.appex have UIDeviceFamily exactly [1]
  - the app's icon: CFBundleIconName is AppIcon and Assets.car holds the 120 and 180 px iPhone
    renditions and the 1024 px App Store one (when a catalog is present)
  - the orientation set is valid for the declared devices: an app that is not iPhone-only must
    declare all four orientations (error 90474); an iPhone-only app may declare any non-empty set
  - the app was built with the iOS 26 SDK or later (DTSDKName / DTPlatformVersion in Info.plist);
    App Store Connect refuses anything older (2026-10-04: "built with the iOS 18.5 SDK")
Exit 2 on a usage error.
"""
import os
import plistlib
import re
import subprocess
import sys

ALL_FOUR = {
    "UIInterfaceOrientationPortrait",
    "UIInterfaceOrientationPortraitUpsideDown",
    "UIInterfaceOrientationLandscapeLeft",
    "UIInterfaceOrientationLandscapeRight",
}


def load(path):
    with open(path, "rb") as f:
        return plistlib.load(f)


def find_app(path):
    if path.endswith(".xcarchive"):
        apps = os.path.join(path, "Products", "Applications")
        found = [n for n in sorted(os.listdir(apps)) if n.endswith(".app")] if os.path.isdir(apps) else []
        if len(found) != 1:
            raise SystemExit(f"FAIL archive has {len(found)} apps under Products/Applications, expected 1")
        return os.path.join(apps, found[0])
    return path


def icon_widths(app):
    car = os.path.join(app, "Assets.car")
    if not os.path.exists(car):
        return None
    listing = subprocess.run(["xcrun", "assetutil", "--info", car], capture_output=True, text=True).stdout
    return {int(w) for w in re.findall(r'"PixelWidth" : (\d+)', listing)}


MIN_SDK = 26


def sdk_major(info):
    """The major iOS SDK version the app was built with, or None when the plist does not say."""
    m = re.search(r"(\d+)(?:\.\d+)*$", str(info.get("DTSDKName") or ""))
    for text in (m.group(1) if m else None, str(info.get("DTPlatformVersion") or "").split(".")[0]):
        if text and text.isdigit():
            return int(text)
    return None


def check_sdk(info):
    found = sdk_major(info)
    detail = (f"built with {info.get('DTSDKName') or 'no SDK recorded'} "
              f"(DTPlatformVersion {info.get('DTPlatformVersion')!r}); App Store Connect needs the iOS "
              f"{MIN_SDK} SDK or later. Archive with Xcode {MIN_SDK} or later, selected with xcode-select or DEVELOPER_DIR.")
    return (found is not None and found >= MIN_SDK, f"built with the iOS {MIN_SDK} SDK or later", detail)


def check(app):
    """Returns a list of (ok, name, detail) rows."""
    rows = []
    info = load(os.path.join(app, "Info.plist"))
    bundles = [("app", info)]
    plugins = os.path.join(app, "PlugIns")
    if os.path.isdir(plugins):
        for n in sorted(os.listdir(plugins)):
            p = os.path.join(plugins, n, "Info.plist")
            if n.endswith(".appex") and os.path.exists(p):
                bundles.append((n, load(p)))
    bad = [(n, b.get("UIDeviceFamily")) for n, b in bundles if b.get("UIDeviceFamily") != [1]]
    rows.append((not bad, f"UIDeviceFamily is exactly [1] (iPhone only) in {', '.join(n for n, _ in bundles)}",
                 f"found {bad}"))

    family = info.get("UIDeviceFamily") or []
    orientations = set(info.get("UISupportedInterfaceOrientations") or [])
    if family == [1]:
        valid = bool(orientations)
        why = f"iPhone-only app declares {sorted(orientations) or 'none'}"
    else:
        valid = ALL_FOUR <= orientations
        why = (f"devices {family} need all four orientations (Apple error 90474); "
               f"declared {sorted(orientations)}")
    rows.append((valid, "the orientation set is valid for the declared devices", why))

    icon_name = info.get("CFBundleIcons", {}).get("CFBundlePrimaryIcon", {}).get("CFBundleIconName")
    widths = icon_widths(app)
    needed = {120, 180, 1024}
    if 2 in family:
        needed |= {152, 167}  # the iPad icons Apple error 90023 names
    missing = sorted(needed - widths) if widths is not None else sorted(needed)
    rows.append((icon_name == "AppIcon" and not missing,
                 "the required icons are present (AppIcon; " + ", ".join(str(w) for w in sorted(needed)) + " px)",
                 f"CFBundleIconName {icon_name!r}; missing renditions {missing}"))
    rows.append(check_sdk(info))
    return rows


def main(argv):
    if len(argv) != 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        return 2
    app = find_app(argv[1])
    failed = 0
    for ok, name, detail in check(app):
        if ok:
            print(f"  ok  {name}")
        else:
            failed += 1
            print(f"  FAIL  {name}\n        {detail}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
