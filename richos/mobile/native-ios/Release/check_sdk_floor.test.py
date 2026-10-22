#!/usr/bin/env python3
"""check_device_family.py refuses an archive built with an SDK older than iOS 26. Run: python3 check_sdk_floor.test.py"""
import os
import plistlib
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))


def run(sdk, platform):
    with tempfile.TemporaryDirectory() as d:
        archive = os.path.join(d, "A.xcarchive")
        app = os.path.join(archive, "Products", "Applications", "A.app")
        os.makedirs(app)
        info = {"UIDeviceFamily": [1], "UISupportedInterfaceOrientations": ["UIInterfaceOrientationPortrait"],
                "CFBundleIcons": {"CFBundlePrimaryIcon": {"CFBundleIconName": "AppIcon"}},
                "DTSDKName": sdk, "DTPlatformVersion": platform}
        with open(os.path.join(app, "Info.plist"), "wb") as f:
            plistlib.dump(info, f)
        return subprocess.run([sys.executable, os.path.join(HERE, "check_device_family.py"), archive],
                              capture_output=True, text=True)


old = run("iphoneos18.5", "18.5")
assert old.returncode == 1, old.stdout
assert "FAIL  built with the iOS 26 SDK" in old.stdout and "iphoneos18.5" in old.stdout and "Xcode 26" in old.stdout, old.stdout
new = run("iphoneos26.2", "26.2")
assert "FAIL  built with" not in new.stdout, new.stdout
print("ok: iphoneos18.5 refused naming the SDK and Xcode; iphoneos26.2 passes the SDK check")
