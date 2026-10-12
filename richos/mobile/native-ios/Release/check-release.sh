#!/usr/bin/env bash
# Builds the RichOS native iPhone app in Debug AND Release (simulator SDK, ad hoc signed, with both
# extensions) and proves, on the Release product, everything App Store Connect and App Review read
# from the binary. Stream I3's release check; it extends `bin/rios sim check-release` (stream I1)
# from the app's own binary to the whole product: the app and both extensions.
#
#   Release/check-release.sh <output directory in the selected storage folder>
#
# Prints one line per check ("ok …" / "FAIL …") and exits 0 only when every check passed.
#
#   R1  both extensions are embedded in the app, with identifiers under the app's
#   R2  the purpose strings are present, word for word as Release/platform.yml sets them
#   R3  ITSAppUsesNonExemptEncryption is false
#   R4  the App Group and Keychain group names reach the app and both extensions
#   R5  the privacy manifest ships in the app bundle and declares no tracking
#   R6  REQUIRED-REASON APIs: every category the Release binaries use is declared, and none is
#       declared that nothing uses (measured on the symbols each Mach-O imports)
#   R7  the icon: AppIcon is compiled into Assets.car with its 1024 App Store rendition
#   R8  no development code in any Release binary (the markers `bin/rios sim check-release` uses),
#       after proving the Debug app binary DOES carry them (a negative check needs its positive)
#   R9  nothing from the source tree that is not app content was bundled (specs, tests, scripts)
#   R10 iPhone only, as App Store Connect reads it (Release/check_device_family.py): UIDeviceFamily is
#       exactly [1] in the app and both extensions, the required icons are compiled, the orientation set
#       fits the devices; proved against a copy rewritten to [1, 2], which it must refuse
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:?usage: check-release.sh <output directory>}"
OUT="$(python3 "$HERE/../../engine/scripts/lib/machine_settings.py" validate "$OUT")" || exit 2
mkdir -p "$OUT/logs"

PROJECT="$("$HERE/Release/generate.sh" "$OUT/project")" || { echo "FAIL generate: see xcodegen output above"; exit 1; }
for CONFIG in Debug Release; do
  if ! python3 "$HERE/../../engine/scripts/lib/native-work.py" -- xcodebuild -project "$PROJECT" -scheme RichOSNative -configuration "$CONFIG" -sdk iphonesimulator \
       -destination 'generic/platform=iOS Simulator' -derivedDataPath "$OUT/DerivedData" \
       -clonedSourcePackagesDirPath "$OUT/SourcePackages" CODE_SIGN_IDENTITY=- build >"$OUT/logs/build-$CONFIG.log" 2>&1; then
    grep -E 'error:' "$OUT/logs/build-$CONFIG.log" | sort -u | head -20
    echo "FAIL build $CONFIG: full log $OUT/logs/build-$CONFIG.log"
    exit 1
  fi
done

python3 - "$OUT/DerivedData/Build/Products" "$HERE" <<'PY'
import os, plistlib, re, subprocess, sys

products, root = sys.argv[1], sys.argv[2]
release = os.path.join(products, "Release-iphonesimulator", "RichOSNative.app")
debug = os.path.join(products, "Debug-iphonesimulator", "RichOSNative.app")
failed = 0

def ok(name, detail=""):
    print(f"  ok  {name}")

def bad(name, detail=""):
    global failed
    failed += 1
    print(f"  FAIL  {name}" + (f"\n        {detail}" if detail else ""))

def plist(path):
    with open(path, "rb") as f:
        return plistlib.load(f)

def spec_value(key):
    """A setting's value exactly as Release/platform.yml writes it (quoted strings only)."""
    text = open(os.path.join(root, "Release", "platform.yml"), encoding="utf-8").read()
    m = re.search(r"^\s*" + re.escape(key) + r':\s*"(.*)"\s*$', text, re.M)
    return m.group(1) if m else None

info = plist(os.path.join(release, "Info.plist"))
bundle = info.get("CFBundleIdentifier", "")
extensions = {"RichOSShare.appex": "com.apple.share-services", "RichOSNotificationService.appex": "com.apple.usernotifications.service"}

# Simulator signatures omit APNs. Check expanded metadata here; the physical runner
# separately requires an actual matching aps-environment signing entitlement.
for config, expected in [("Debug", "development"), ("Release", "production")]:
    app = os.path.join(products, config + "-iphonesimulator", "RichOSNative.app")
    entitlements = plistlib.loads(subprocess.check_output(["codesign", "-d", "--entitlements", ":-", app], stderr=subprocess.DEVNULL))
    configured = plist(os.path.join(app, "Info.plist")).get("RichOSAPNsEnvironment")
    signed = entitlements.get("aps-environment")
    (ok if configured == expected and signed in (None, expected) else bad)(
        f"APNs {config} metadata expands correctly (simulator)", str((configured, signed)))

# R1
problems = []
for name, point in extensions.items():
    path = os.path.join(release, "PlugIns", name, "Info.plist")
    if not os.path.exists(path):
        problems.append(f"{name} missing")
        continue
    ext = plist(path)
    if not ext.get("CFBundleIdentifier", "").startswith(bundle + "."):
        problems.append(f"{name} id {ext.get('CFBundleIdentifier')} is not under {bundle}")
    if ext.get("NSExtension", {}).get("NSExtensionPointIdentifier") != point:
        problems.append(f"{name} is not a {point} extension")
(bad if problems else ok)(f"R1 both extensions embedded under {bundle}", "; ".join(problems))

# R2
keys = ["NSMicrophoneUsageDescription", "NSCameraUsageDescription", "NSPhotoLibraryUsageDescription", "NSLocalNetworkUsageDescription"]
wrong = [k for k in keys if not spec_value("INFOPLIST_KEY_" + k) or info.get(k) != spec_value("INFOPLIST_KEY_" + k)]
(bad if wrong else ok)(f"R2 {len(keys)} purpose strings present, word for word", f"missing or different: {wrong}")

# R3
(ok if info.get("ITSAppUsesNonExemptEncryption") is False else bad)(
    "R3 ITSAppUsesNonExemptEncryption is false", f"found {info.get('ITSAppUsesNonExemptEncryption')!r}")

# R4
group = info.get("RichOSAppGroup")
share = plist(os.path.join(release, "PlugIns", "RichOSShare.appex", "Info.plist"))
service = plist(os.path.join(release, "PlugIns", "RichOSNotificationService.appex", "Info.plist"))
names_ok = (group == f"group.{bundle}" and share.get("RichOSAppGroup") == group
            and info.get("RichOSKeychainGroup", "").endswith(f"{bundle}.shared")
            and service.get("RichOSKeychainGroup") == info.get("RichOSKeychainGroup"))
(ok if names_ok else bad)(f"R4 App Group {group} and the Keychain group reach the app and both extensions",
                          f"app {group}/{info.get('RichOSKeychainGroup')}, share {share.get('RichOSAppGroup')}, service {service.get('RichOSKeychainGroup')}")

# R5
manifest_path = os.path.join(release, "PrivacyInfo.xcprivacy")
manifest = plist(manifest_path) if os.path.exists(manifest_path) else None
if manifest is None:
    bad("R5 the privacy manifest ships in the app bundle", "PrivacyInfo.xcprivacy is not in the Release app")
else:
    (ok if manifest.get("NSPrivacyTracking") is False and manifest.get("NSPrivacyTrackingDomains") == [] else bad)(
        "R5 the privacy manifest ships in the app and declares no tracking", str(manifest.get("NSPrivacyTracking")))

# R6 — required-reason APIs (developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api),
# matched on what each Release Mach-O imports (undefined symbols) and the selectors it names.
CATEGORIES = {
    "NSPrivacyAccessedAPICategoryFileTimestamp": [r"^_(stat|fstat|lstat|fstatat|getattrlist|fgetattrlist|getattrlistat|getattrlistbulk)(\$INODE64)?$",
                                                  r"^_NSFile(CreationDate|ModificationDate)$", r"^_NSURL(ContentModificationDate|CreationDate)Key$"],
    "NSPrivacyAccessedAPICategorySystemBootTime": [r"^_mach_absolute_time$", r"^systemUptime$"],
    "NSPrivacyAccessedAPICategoryDiskSpace": [r"^_(statfs|statvfs|fstatfs|fstatvfs)(\$INODE64)?$", r"^_NSFileSystem(Free)?Size$",
                                              r"^_NSURLVolume(AvailableCapacity|AvailableCapacityForImportantUsage|AvailableCapacityForOpportunisticUsage|TotalCapacity)Key$"],
    "NSPrivacyAccessedAPICategoryActiveKeyboards": [r"^activeInputModes$"],
    "NSPrivacyAccessedAPICategoryUserDefaults": [r"^_OBJC_CLASS_\$_NSUserDefaults$", r"AppStorage"],
}
binaries = [os.path.join(release, "RichOSNative")] + [os.path.join(release, "PlugIns", n, n.replace(".appex", "")) for n in extensions]
used = {}
for binary in binaries:
    symbols = subprocess.run(["xcrun", "nm", "-u", binary], capture_output=True, text=True).stdout.split()
    selectors = subprocess.run(["strings", "-a", binary], capture_output=True, text=True).stdout.split()
    for category, patterns in CATEGORIES.items():
        for token in symbols + selectors:
            if any(re.search(p, token) for p in patterns):
                used.setdefault(category, set()).add(f"{os.path.basename(binary)}:{token}")
declared = {entry["NSPrivacyAccessedAPIType"] for entry in (manifest or {}).get("NSPrivacyAccessedAPITypes", [])}
undeclared = sorted(set(used) - declared)
unused = sorted(declared - set(used))
if undeclared or unused:
    bad("R6 required-reason APIs match the manifest",
        f"used but not declared: {[(c, sorted(used[c])) for c in undeclared]}; declared but unused: {unused}")
else:
    ok(f"R6 required-reason APIs match the manifest: {len(binaries)} Release binaries use {sorted(used) or 'none'}; declared {sorted(declared) or 'none'}")

# R7
car = os.path.join(release, "Assets.car")
icons = info.get("CFBundleIcons", {}).get("CFBundlePrimaryIcon", {}).get("CFBundleIconName")
listing = subprocess.run(["xcrun", "assetutil", "--info", car], capture_output=True, text=True).stdout if os.path.exists(car) else ""
has_1024 = '"PixelWidth" : 1024' in listing and '"Name" : "AppIcon"' in listing
(ok if icons == "AppIcon" and has_1024 else bad)("R7 AppIcon is the app's icon and its 1024 px App Store rendition is compiled",
                                                  f"CFBundleIconName {icons!r}; 1024 in Assets.car: {has_1024}")

# R8 — the markers are stream I1's (`Simulator.developmentMarkers`), read from its source so the two
# checks can never disagree about what counts as development code.
source = open(os.path.join(root, "Core", "Sources", "RichOSCLI", "Simulator.swift"), encoding="utf-8").read()
markers = re.findall(r'"([^"]+)"', re.search(r"developmentMarkers\s*=\s*\[(.*?)\]", source).group(1))
# Xcode 16 builds a Debug app's code into `RichOSNative.debug.dylib` beside a small launcher, so the
# positive probe reads every Mach-O in the Debug app's top level.
debug_binary = b"".join(open(os.path.join(debug, f), "rb").read() for f in os.listdir(debug)
                        if f == "RichOSNative" or f.endswith(".dylib"))
absent_from_debug = [m for m in markers if m.encode() not in debug_binary]
leaks = [f"{os.path.basename(b)}:{m}" for b in binaries for m in markers if m.encode() in open(b, "rb").read()]
if absent_from_debug:
    bad("R8 development markers", f"the Debug binary lacks {absent_from_debug}; the Release search would prove nothing")
elif leaks:
    bad("R8 no development code in any Release binary", f"found {leaks}")
else:
    ok(f"R8 no development code in the Release app or its extensions ({', '.join(markers)}; all present in Debug)")

# R9
stray = []
for dirpath, _, files in os.walk(release):
    for f in files:
        if f.endswith((".yml", ".sh", ".cjs", ".mjs", ".ts", ".md")) or f == "main.swift" or f.endswith(".entitlements"):
            stray.append(os.path.relpath(os.path.join(dirpath, f), release))
(bad if stray else ok)("R9 no specs, scripts, tests or notes were bundled", f"found {stray}")

# R10 — the 2026-10-04 upload failure (Apple errors 90023 and 90474): every target had been given
# TARGETED_DEVICE_FAMILY "1,2". The same script checks an .xcarchive before upload.
import shutil, tempfile
checker = os.path.join(root, "Release", "check_device_family.py")
run = subprocess.run([sys.executable, checker, release], capture_output=True, text=True)
copy = tempfile.mkdtemp(dir=os.path.dirname(products)) + "/RichOSNative.app"
shutil.copytree(release, copy, symlinks=True)
with open(os.path.join(copy, "Info.plist"), "rb") as f:
    spoiled = plistlib.load(f)
spoiled["UIDeviceFamily"] = [1, 2]
with open(os.path.join(copy, "Info.plist"), "wb") as f:
    plistlib.dump(spoiled, f)
negative = subprocess.run([sys.executable, checker, copy], capture_output=True, text=True)
shutil.rmtree(os.path.dirname(copy))
if run.returncode != 0:
    bad("R10 iPhone only: device family, icons, orientations", run.stdout.strip())
elif negative.returncode == 0:
    bad("R10 the device-family check refuses an app declaring iPhone and iPad", "it passed a copy with UIDeviceFamily [1, 2]")
else:
    ok("R10 iPhone only: UIDeviceFamily [1] in the app and both extensions, icons compiled, orientations valid; refuses [1, 2]")

sys.exit(1 if failed else 0)
PY
