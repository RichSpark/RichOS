#!/usr/bin/env bash
# run-tests: no-host-screen: changes only links in private test fixtures
# run-tests: inputs richos/app/scripts/shared-build-cache.test.sh richos/app/scripts/shared-build-cache.sh richos/app/scripts/bin/cargo richos/app/scripts/lib/cargo_identity.py richos/engine/scripts/lib/machine-settings.sh richos/engine/scripts/lib/machine_settings.py
# run-tests: covers richos/app/scripts/shared-build-cache.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(cd "$(mktemp -d "${TMPDIR:?}/cargo-cache-links.XXXXXX")" && pwd -P)"
trap 'rm -rf "$WORK"' EXIT
for checkout in alpha beta; do
  for workspace in app app/src-tauri; do
    mkdir -p "$WORK/$checkout/richos/$workspace"
    printf '[package]\nname="fixture"\nversion="0.1.0"\n[workspace]\n' > "$WORK/$checkout/richos/$workspace/Cargo.toml"
  done
done
mkdir -p "$WORK/cache/old-shared"
touch "$WORK/cache/old-shared/preserve-me"
ln -s "$WORK/cache/old-shared" "$WORK/alpha/richos/app/target"
if RICHOS_BUILD_CACHE="$WORK/cache" bash "$HERE/shared-build-cache.sh" --check "$WORK/alpha"; then
  echo 'FAIL old shared target was accepted'; exit 1
fi
for checkout in alpha beta; do
  RICHOS_BUILD_CACHE="$WORK/cache" bash "$HERE/shared-build-cache.sh" "$WORK/$checkout"
  RICHOS_BUILD_CACHE="$WORK/cache" bash "$HERE/shared-build-cache.sh" --check "$WORK/$checkout"
done
[ -f "$WORK/cache/old-shared/preserve-me" ]
count="$(find "$WORK/alpha" "$WORK/beta" -type l -name target -exec readlink '{}' \; | sort -u | wc -l | tr -d ' ')"
[ "$count" = 4 ]
echo 'PASS two checkouts and detached workspaces have four private outputs'
echo 'PASS legacy shared link migrated without deleting its cache'

# A different spelling of the same private output is still the same cache.
ln -s "$WORK/cache" "$WORK/cache-alias"
DEST="$(readlink "$WORK/alpha/richos/app/target")"
ALIAS_DEST="$WORK/cache-alias/${DEST#"$WORK/cache/"}"
rm "$WORK/alpha/richos/app/target"
ln -s "$ALIAS_DEST" "$WORK/alpha/richos/app/target"
RICHOS_BUILD_CACHE="$WORK/cache-alias" bash "$HERE/shared-build-cache.sh" --check "$WORK/alpha"
[ "$(readlink "$WORK/alpha/richos/app/target")" = "$ALIAS_DEST" ]
echo 'PASS an equivalent cache alias is accepted without replacing its link'

# A private component override is read on the first invocation; environment wins.
printf 'RICHOS_BUILD_CACHE=%s\n' "$WORK/private-cache" > "$WORK/private.env"
for selected in private-cache environment-cache; do
  if [ "$selected" = private-cache ]; then
    env -u RICHOS_BUILD_CACHE -u RICHOS_MACHINE_SETTINGS_RESOLVED \
      RICHOS_ENV_FILE="$WORK/private.env" bash "$HERE/shared-build-cache.sh" "$WORK/alpha"
  else
    env -u RICHOS_MACHINE_SETTINGS_RESOLVED RICHOS_BUILD_CACHE="$WORK/$selected" \
      RICHOS_ENV_FILE="$WORK/private.env" bash "$HERE/shared-build-cache.sh" "$WORK/alpha"
  fi
  for workspace in app app/src-tauri; do
    case "$(readlink "$WORK/alpha/richos/$workspace/target")" in
      "$WORK/$selected/"*) ;;
      *) echo "FAIL $selected was ignored"; exit 1 ;;
    esac
  done
done
echo 'PASS private build cache and environment override both select their own outputs'
