#!/usr/bin/env bash
#
# branch-bundle-richdoctor.sh — give a branch-bundle.sh zip the RichDoctor payload a packaged
# RichOS carries, so richdoctor-update-walk.sh can walk the install and refresh in a guest.
#
#   testvm/branch-bundle-richdoctor.sh --in <branch.zip> --richdoctor-zip <RichDoctor-macos-aarch64.zip> --out <zip>
#
# branch-bundle.sh wraps the debug executable and carries no RichDoctor, so richdoctor_install.rs
# finds no payload and does nothing. This adds `Contents/Resources/RichDoctor/RichDoctor.app.tar.gz`
# and `.../version` (what package-app.sh puts there): the RichDoctor of the given release zip,
# stamped with the bundle's own version and re-signed AD HOC, because the bundle is ad hoc and the
# refresh requires the payload and RichOS to share one signing team (bootstrap.rs `verify`). The
# bundle is then re-signed ad hoc. Guest only, never for anyone's own Mac.
set -euo pipefail
IN=""; RD=""; OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --in) IN="$2"; shift 2 ;;
    --richdoctor-zip) RD="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "branch-bundle-richdoctor.sh: unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -f "$IN" ] && [ -f "$RD" ] && [ -n "$OUT" ] || { echo "branch-bundle-richdoctor.sh: --in, --richdoctor-zip and --out are required" >&2; exit 2; }
# Resolve the destination before entering disposable staging directories.
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac

W="$(mktemp -d "${TMPDIR:-/tmp}/bbrd.XXXXXX")"
trap 'rm -rf "$W"' EXIT
mkdir "$W/app" "$W/rd"
ditto -x -k "$IN" "$W/app"
ditto -x -k "$RD" "$W/rd"
APP="$W/app/RichOS.app"; DOC="$W/rd/RichDoctor.app"
[ -d "$APP" ] && [ -d "$DOC" ] || { echo "branch-bundle-richdoctor.sh: a zip has no RichOS.app / RichDoctor.app root" >&2; exit 1; }
V="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $V" "$DOC/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $V" "$DOC/Contents/Info.plist"
codesign --force --deep --sign - --timestamp=none "$DOC" >/dev/null 2>&1
codesign --verify --deep --strict "$DOC"
mkdir -p "$APP/Contents/Resources/RichDoctor"
( cd "$W/rd" && COPYFILE_DISABLE=1 tar -czf "$APP/Contents/Resources/RichDoctor/RichDoctor.app.tar.gz" RichDoctor.app )
printf '%s\n' "$V" > "$APP/Contents/Resources/RichDoctor/version"
codesign --force --sign - "$APP" >/dev/null 2>&1
mkdir -p "$(dirname "$OUT")"; rm -f "$OUT"
( cd "$W/app" && ditto -c -k --keepParent RichOS.app "$OUT" )
echo "bundle=$OUT richdoctor-payload-version=$V"
