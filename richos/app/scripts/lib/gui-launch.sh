#!/usr/bin/env bash
#
# gui-launch.sh — build a complete machine, and boot the shipped binary on it the way
# LaunchServices boots it.
#
# Sourced by `gui-boot.test.sh`, and by `testvm/run-suite.sh` for `gui_prebuild` when that
# suite runs in a test-VM guest (PREBUILT MODE, below). It is a library and not a suite: everything here is
# MACHINERY, and every VERDICT lives in the suite, where a reader looking for what is
# asserted finds it in one file.
#
# ---------------------------------------------------------------------------------------
# THE CONDITION, MEASURED RATHER THAN ASSUMED
# ---------------------------------------------------------------------------------------
#
# `ps eww` on a real Finder double-click of RichOS.app on this machine carries exactly:
#
#   HOME=/Users/alex  USER=alex  PATH=/usr/bin:/bin:/usr/sbin:/sbin
#
# and the working directory is `/`. That is recorded in
# `docs/verification/loro-write-path-2026-09-01/` and in `engine.rs`'s module doc, and it is
# the whole reason three separate components resolved their configuration out of variables
# that were not there. `env -i` reproduces it exactly: nothing is inherited, so a variable
# this harness does not name cannot rescue the boot the way a developer's shell does.
#
# `/usr/bin/open` is NOT used and must not be: it forwards the CALLER's environment, which is
# how every boot log in this repository before 2026-09-01 came to be a developer's launch
# wearing a double-click's clothes (commit 8b7ca41).
#
# ---------------------------------------------------------------------------------------
# WHY THE BINARY IS COPIED OUT OF THE REPOSITORY BEFORE IT IS RUN
# ---------------------------------------------------------------------------------------
#
# `engine.rs`'s candidate 4 is "the nearest ancestor of the EXECUTABLE holding an engine/".
# A binary sitting in `app/src-tauri/target/debug/` satisfies it, so a boot from there finds
# an engine no matter how badly the resolution is broken for a customer — the dogfood
# checkout answers on the executable's behalf. Every boot log in this repository so far was
# taken that way, and each one says `via repo layout above the executable`.
#
# So the binary is copied to `<machine>/Applications/RichOS.app/Contents/MacOS/richos-tauri`
# first. Candidates 4 and 5 then miss, and the engine has to be found the way it is found on
# a customer's Mac: through `<home>/.claude/richos-engine`, candidate 6.
#
# ---------------------------------------------------------------------------------------
# AND WHY IT IS A REAL BUNDLE AND NOT A DIRECTORY SHAPED LIKE ONE
# ---------------------------------------------------------------------------------------
#
# For nine days it was the latter, and the boot died on its first line. `gui_bundle_plist`
# below carries the full diagnosis; the short version is that once `main()` began calling
# `update_startup::prepare`, a `.app` with no `Contents/Info.plist` stopped being a launch
# this product would complete. A harness that boots something the product refuses is not
# testing a launch, and — worse — its NEGATIVE cases keep passing, because a boot that never
# happens looks exactly like a boot that failed for the reason each of them broke.

set -uo pipefail

# ---------------------------------------------------------------------------------------
# PREBUILT MODE — RICHOS_GUI_PREBUILT=<directory>
# ---------------------------------------------------------------------------------------
#
# A test-VM guest is a clean macOS with no cargo and no checkout, on purpose (testvm/run.sh,
# gui-proof-in-vm.sh), so a suite that `run-tests.sh` routes there under RICHOS_GUI_HOST
# cannot build what it launches. This Mac builds all three executables first (`gui_prebuild`,
# called by testvm/run-suite.sh) and the guest run names the directory that holds them:
#
#   <dir>/richos-tauri       the binary the suite boots, built as the suite builds it
#   <dir>/gui_boot_machine   the example `gui_machine` runs to provision the machine
#   <dir>/richos-companion   the capture sidecar placed beside the app executable
#
# UNSET, NOTHING IN THIS FILE CHANGES: the suite builds from the checkout and `gui_machine`
# runs the example through `cargo run`, exactly as before this mode existed.
GUI_PREBUILT_NAMES="richos-tauri gui_boot_machine richos-companion"

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/cargo-cache-env.sh"
gui_prebuilt_mode() { [ -n "${RICHOS_GUI_PREBUILT:-}" ]; }

# gui_prebuilt_check — exit 0 when every prebuilt executable is there and runnable; otherwise
# names each one that is not, on stderr.
gui_prebuilt_check() {
  local name missing=0
  for name in $GUI_PREBUILT_NAMES; do
    if [ ! -f "$RICHOS_GUI_PREBUILT/$name" ] || [ ! -x "$RICHOS_GUI_PREBUILT/$name" ]; then
      echo "gui_prebuilt_check: no executable $RICHOS_GUI_PREBUILT/$name" >&2
      missing=$((missing + 1))
    fi
  done
  [ "$missing" -eq 0 ]
}

# gui_artifact <cargo-json-file> <target-name> <target-kind> — the executable cargo reported
# for that target, read from `--message-format=json`, never from a guessed target path: the
# target directory here is a symlink into a cache shared by every checkout on this Mac.
gui_artifact() {
  python3 - "$1" "$2" "$3" <<'PY'
import json, sys
path, name, kind = sys.argv[1:]
hit = None
with open(path, encoding="utf-8") as handle:
    for line in handle:
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        target = msg.get("target") or {}
        if (msg.get("reason") == "compiler-artifact" and target.get("name") == name
                and kind in target.get("kind", []) and msg.get("executable")):
            hit = msg["executable"]
if not hit:
    sys.exit(1)
print(hit)
PY
}

# ---------------------------------------------------------------------------------------
# gui_prebuild <out-dir>
#
# THIS MAC's HALF OF PREBUILT MODE. Builds the executables the way the suite and
# `gui_machine` build them today (the same crates, the same profile, and RICHOS_SOURCE_SHA
# set to this checkout's HEAD so the boot names its commit) and copies them into <out-dir>
# the moment each build ends.
#
# THE COPY IS CHECKED, NOT TRUSTED. `src-tauri/target` is a symlink into a cache every
# richos checkout on this Mac builds into, so another checkout's build can replace the
# binary between this build finishing and the copy. The copied `richos-tauri` must carry
# this checkout's commit (engine::source_commit compiles it in); one that does not is
# refused here rather than booted in a guest as somebody else's code.
#
# Exit 0: all are in <out-dir>. Exit 1: a build failed, which is a verdict about the CODE.
# Exit 2: nothing this Mac could vouch for came out of it (no checkout, no artifact, a copy
# that is not this commit's).
# ---------------------------------------------------------------------------------------
gui_prebuild() {
  local out="$1" app_dir="$GUI_APP_DIR" sha json exe companion
  if ! sha="$(git -C "$app_dir" rev-parse HEAD 2>/dev/null)" || [ -z "$sha" ]; then
    echo "gui_prebuild: $app_dir is not inside a git checkout, so the binary could not name its commit." >&2
    return 2
  fi
  mkdir -p "$out" || return 2
  # Found the way gui-boot.test.sh finds it: PATH, then rustup's own directory. None at all is
  # a fact about this Mac, never a verdict about the code.
  if ! command -v cargo >/dev/null 2>&1; then
    if [ -x "$HOME/.cargo/bin/cargo" ]; then
      PATH="$HOME/.cargo/bin:$PATH"; export PATH
    else
      echo "gui_prebuild: no cargo on PATH or at $HOME/.cargo/bin; nothing can be built for the guest." >&2
      return 2
    fi
  fi
  json="$(mktemp -t gui-prebuild.XXXXXX)" || return 2

  if ! ( cd "$app_dir/src-tauri" && RICHOS_SOURCE_SHA="$sha" \
         cargo build --quiet --bin richos-tauri --message-format=json-render-diagnostics ) > "$json"; then
    echo "gui_prebuild: richos-tauri did not build. That is a verdict about the CODE." >&2
    rm -f "$json"; return 1
  fi
  if ! exe="$(gui_artifact "$json" richos-tauri bin)" || ! cp "$exe" "$out/richos-tauri"; then
    echo "gui_prebuild: cargo reported no richos-tauri executable to copy." >&2
    rm -f "$json"; return 2
  fi
  if ! LC_ALL=C grep -aFq "$sha" "$out/richos-tauri"; then
    echo "gui_prebuild: the richos-tauri copied from $exe does not carry this checkout's commit" >&2
    echo "              $sha. Another checkout's build replaced it in the shared target directory" >&2
    echo "              between the build and the copy. Refusing to boot somebody else's binary." >&2
    rm -f "$json"; return 2
  fi

  if ! ( cd "$app_dir" && cargo build --quiet -p richos-core --example gui_boot_machine \
           --message-format=json-render-diagnostics ) > "$json"; then
    echo "gui_prebuild: the gui_boot_machine example did not build. That is a verdict about the CODE." >&2
    rm -f "$json"; return 1
  fi
  if ! exe="$(gui_artifact "$json" gui_boot_machine example)" || ! cp "$exe" "$out/gui_boot_machine"; then
    echo "gui_prebuild: cargo reported no gui_boot_machine executable to copy." >&2
    rm -f "$json"; return 2
  fi
  rm -f "$json"
  if ! companion="$(python3 "$app_dir/scripts/prepare-companion.py" --binary)" \
     || [ ! -x "$companion" ] || ! cp -p "$companion" "$out/richos-companion"; then
    echo "gui_prebuild: the capture companion could not be prepared for the guest." >&2
    return 1
  fi
  chmod 755 "$out/richos-tauri" "$out/gui_boot_machine" "$out/richos-companion" || return 2
  echo "prebuilt        : richos-tauri, gui_boot_machine and richos-companion at $sha, in $out"
  return 0
}

# ---------------------------------------------------------------------------------------
# gui_compiler_source
#
# Fixture code comes from the public engine under test. An explicit source is
# supported for developer probes, but private corpus pointers are never searched.
gui_compiler_source() {
  local looks_like_loro=""
  local c
  for c in "${RICHOS_LORO_SOURCE:-${GUI_ENGINE_DIR}/loro}"; do
    [ -n "$c" ] || continue
    [ "$c" = "/loro" ] && continue
    if [ -f "$c/bin/loro-context.mjs" ] && [ -f "$c/bin/loro-write.mjs" ]; then
      looks_like_loro="$c"; break
    fi
  done
  if [ -z "$looks_like_loro" ]; then
    echo "gui_compiler_source: no loro checkout on this machine to copy from. Looked at" >&2
    echo "  the explicit RICHOS_LORO_SOURCE or the public engine component." >&2
    return 1
  fi
  printf '%s\n' "$looks_like_loro"
  return 0
}

# ---------------------------------------------------------------------------------------
# gui_machine <scratch-root>
#
# Build the machine. Everything it creates is under <scratch-root>; nothing it does can
# reach the real `$HOME`. Prints what it made. Non-zero means the machine could not be
# built, which is never a verdict about the code under test.
# ---------------------------------------------------------------------------------------
gui_machine() {
  local home="$1"
  local app_dir="$GUI_APP_DIR"

  # -- the corpus, the loro tools, the pointer and the saved company ---------------------
  # Every one of them through the product's own `provision`, so this cannot describe a
  # machine RichOS does not create. See examples/gui_boot_machine.rs.
  local src
  src="$(gui_compiler_source)" || return 1
  mkdir -p "$home" || return 1
  mkdir -p "$home/.claude" "$home/FixtureDelivery/engine" || return 1
  # Each negative case owns its component copy. Removing a component must never
  # follow a symlink into the developer's engine or another fixture.
  cp -a "$GUI_ENGINE_DIR/." "$home/FixtureDelivery/engine/" || return 1
  if [ ! -f "$home/FixtureDelivery/engine/runtime/delivery.json" ]; then
    [ -n "${RICHOS_RUNTIME_DIR:-}" ] || { echo "gui_machine: a verified delivered runtime is required (RICHOS_RUNTIME_DIR)" >&2; return 1; }
    python3 "$app_dir/scripts/verify-runtime.py" "$RICHOS_RUNTIME_DIR" "$app_dir/scripts/runtime-sources.json" || return 1
    cp -a "$RICHOS_RUNTIME_DIR" "$home/FixtureDelivery/engine/runtime" || return 1
  fi
  ln -sfn ../FixtureDelivery/engine "$home/.claude/richos-engine" || return 1
  # -- the speech model: half of the video tools, a setup essential since 2026-10-07 --------
  # Present only when a PINNED model verifies (setup.rs `find_media_tools`, through
  # `stt::readiness`), so it is a real one, named by the caller (nightly-local.py and
  # proof-run.py hand lib/runtime_cache.py's; run-suite.sh pushes it into a guest). Cloned
  # (`cp -c`, no bytes copied on APFS), with a plain copy where the two are on different
  # volumes. Put where the app installs one: `~/.config/richos/models` (stt.rs).
  # BOTH models (the CEO, 2026-10-07: "Both"): RICHOS_GUI_SPEECH_MODELS is their paths joined
  # by ":", the one voice resolves and the transcription model.
  [ -n "${RICHOS_GUI_SPEECH_MODELS:-}" ] \
    || { echo "gui_machine: the pinned speech models are required (RICHOS_GUI_SPEECH_MODELS)" >&2; return 1; }
  mkdir -p "$home/.config/richos/models" || return 1
  local model model_dest models
  IFS=':' read -r -a models <<<"$RICHOS_GUI_SPEECH_MODELS"
  for model in "${models[@]}"; do
    [ -f "$model" ] || { echo "gui_machine: speech model $model is not a file" >&2; return 1; }
    model_dest="$home/.config/richos/models/$(basename "$model")"
    cp -c "$model" "$model_dest" 2>/dev/null || cp "$model" "$model_dest" || return 1
  done
  src="$home/FixtureDelivery/engine/loro"
  # The same program either way; prebuilt mode runs the copy this Mac built (see the top).
  if gui_prebuilt_mode; then
    "$RICHOS_GUI_PREBUILT/gui_boot_machine" "$home" "$src" || return 1
  else
    ( cd "$app_dir" && cargo run -q -p richos-core --example gui_boot_machine -- "$home" "$src" ) || return 1
  fi

  # -- the central folder, and the ONE thing here that is deliberately not provisioned ----
  #
  # `~/myrichos/companies/<id>/company.md` is what the CEO said about his company, and
  # `company.rs` reads it at prime time. It is written HERE, in the harness, rather than by
  # the example above, and that is the point rather than an oversight: RichOS does not create
  # this folder and must not. A reader that helpfully created its own source could never
  # report the source missing, which is how this machine ended up with four `corpus.*`
  # symlinks pointing at a directory that is not there
  # (`richos-central-folder-2026-09-06.md` §1.3, `company.rs`).
  #
  # So a COMPLETE machine has one because somebody put it there, and B3-B8 — which build
  # their machines the same way and then break one thing — inherit it too. The company id is
  # `northwind`, the same one `gui_boot_machine.rs` registers; a file under any other id
  # would leave the boot printing `nothing on file` and would look like a product failure.
  mkdir -p "$home/myrichos/companies/northwind" || return 1
  cat > "$home/myrichos/companies/northwind/company.md" <<'COMPANY'
# Northwind

Recorded from a conversation with the CEO, 2026-09-06.

## What the business is

Northwind sells chandlery to working harbors. Its customers are harbor masters, not sailors.

## Who he wants around him

Not discussed yet.
COMPANY

  # -- the engine, at the pointer an installed app reaches -------------------------------
  # `engine.rs` candidate 6: `$CLAUDE_CONFIG_DIR`(or `~/.claude`)`/richos-engine`, which is
  # the pointer `engine/scripts/hooks/install.sh` mints. A symlink, because that is what the
  # installer makes.


  # -- a `claude` binary, at the path `resolve_claude_bin` step 2 names -------------------
  # A STAND-IN, and it is honest about being one. It answers the single control request the
  # boot makes (`initialize`, native.rs::handshake) and then stays alive; it runs no model,
  # holds no credential and is never prompted, because the boot never prompts. What it
  # stands in for is the FACT that a claude binary is installed — which is a configuration,
  # and therefore something this check has to be able to have present in order for its
  # absence to mean anything.
  mkdir -p "$home/.local/bin" || return 1
  cat > "$home/.local/bin/claude" <<'STUB'
#!/usr/bin/env node
// A stand-in for the claude binary. It answers `initialize` and then idles. See
// richos/app/scripts/lib/gui-launch.sh for why a stand-in is the right thing here.
process.stdin.setEncoding("utf8");
let buf = "";
process.stdin.on("data", (chunk) => {
  buf += chunk;
  let nl;
  while ((nl = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, nl);
    buf = buf.slice(nl + 1);
    if (!line.trim()) continue;
    let msg;
    try { msg = JSON.parse(line); } catch (_) { continue; }
    if (msg.type === "control_request") {
      process.stdout.write(JSON.stringify({
        type: "control_response",
        response: { request_id: msg.request_id, subtype: "success" },
      }) + "\n");
    }
  }
});
process.stdin.on("end", () => process.exit(0));
setInterval(() => {}, 1 << 30);
STUB
  chmod +x "$home/.local/bin/claude" || return 1

  # `node` is on no GUI process's PATH, so the stand-in is given an absolute interpreter the
  # same way `resolve_node_bin` gives the compiler one. Without this the shebang fails under
  # launchd's PATH and the boot reports a lease failure that is the harness's fault.
  local node
  node="$(command -v node)"
  [ -n "$node" ] || { echo "gui_machine: no node on PATH — cannot build the stand-in" >&2; return 1; }
  sed -i '' "1s|.*|#!$node|" "$home/.local/bin/claude" || return 1

  # -- the binary, OUTSIDE the repository, IN A BUNDLE THE PRODUCT WILL ACCEPT -----------
  local ident identifier version
  ident="$(gui_identity "$GUI_BINARY")" || return 1
  identifier="${ident%% *}"; version="${ident##* }"
  gui_bundle "$home/Applications/RichOS.app" "$GUI_BINARY" "$identifier" "$version" || return 1
  # The healthy app has the same capture sidecar as the shipped bundle. A
  # prebuilt guest carries it from the host; a local fixture uses the same build.
  local companion
  if gui_prebuilt_mode; then
    companion="$RICHOS_GUI_PREBUILT/richos-companion"
  else
    companion="$(python3 "$app_dir/scripts/prepare-companion.py" --binary)" || return 1
  fi
  if [ ! -f "$companion" ] || [ ! -x "$companion" ]; then
    echo "gui_machine: no executable capture companion at $companion" >&2
    return 1
  fi
  cp -p "$companion" "$home/Applications/RichOS.app/Contents/MacOS/richos-companion" || return 1
  chmod 755 "$home/Applications/RichOS.app/Contents/MacOS/richos-companion" || return 1
  echo "bundle identity : $identifier $version (asked of the binary, not typed here)"
  echo "binary          : $home/Applications/RichOS.app/Contents/MacOS/richos-tauri"
  return 0
}

# ---------------------------------------------------------------------------------------
# gui_identity <executable>
#
# ASK THE BINARY WHO IT IS. Prints `<identifier> <version>`.
#
# `update_startup::identity_probe` answers `--richos-internal-update-identity` with
# `{"identifier": …, "version": …, "protocol": 1}` and returns, BEFORE home resolution,
# before any lease and before Tauri exists (update_startup.rs:14-27). It is the installer's
# own way of asking a candidate executable what it is, and it is the right source here for
# one reason: the Info.plist below has to carry the version the binary will COMPARE it
# against, and every other source of that number is a copy that can disagree.
#
# The number matters more than it looks. `update_startup::prepare` reads the bundle version
# and, when it differs from the compiled version, treats the running application as CHANGED
# (`loaded_bundle_changed`, update_startup.rs:74-78); with no publication receipt on the
# machine that path returns `Err("Changed application has no publication receipt.")` and the
# boot is over before it prints a line. A plist carrying a hand-typed version would fail the
# same way the missing plist did, for a different reason and with an even stranger message.
#
# No GUI, no window, no runtime: the probe is a `println!` and a return.
# ---------------------------------------------------------------------------------------
gui_identity() {
  local exe="$1" out identifier version
  if ! out="$("$exe" --richos-internal-update-identity 2>/dev/null)"; then
    echo "gui_identity: $exe did not answer --richos-internal-update-identity" >&2
    return 1
  fi
  identifier="$(printf '%s' "$out" | sed -n 's/.*"identifier"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
  version="$(printf '%s' "$out" | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
  if [ -z "$identifier" ] || [ -z "$version" ]; then
    echo "gui_identity: could not read an identifier and a version out of the probe's answer:" >&2
    printf '  %s\n' "$out" >&2
    return 1
  fi
  printf '%s %s\n' "$identifier" "$version"
  return 0
}

# ---------------------------------------------------------------------------------------
# gui_bundle_plist <identifier> <version> <executable-name>
#
# The `Info.plist` of the bundle this harness boots, on stdout.
#
# ---------------------------------------------------------------------------------------
# WHY THIS EXISTS AT ALL, AND WHAT IT COST NOT TO HAVE IT
# ---------------------------------------------------------------------------------------
#
# Until 2026-09-10 this harness copied the executable into `RichOS.app/Contents/MacOS/` and
# wrote NO `Info.plist` — a directory shaped like a bundle that was not one. That was fine
# until `01e9b8d8` put `update_startup::prepare` at the top of `main()`: it takes the bundle
# root around the executable (`activation::bundle_root`, a pure path-shape test that this
# layout satisfies) and then reads `Contents/Info.plist` (`richos_user_update::bundle_plist`,
# lib.rs:570-575). There was no such file, so `prepare` returned `Err`, `main` printed
#
#     [richos] application startup: No such file or directory (os error 2)
#
# and returned. Every boot in this suite died at line one. B1 and B2 went red — and B3-B8,
# the six negative halves, went GREEN over the same dead boot, because a machine that never
# boots is indistinguishable from a machine with its engine pointer removed. Measured on
# 2026-09-10 against `7272ecf8`: `3 FAILED, 18 passed`, six of those passes false.
#
# THE KEYS ARE NOT A LIST SOMEBODY MAINTAINS. `C2` below extracts the `Info.plist` keys the
# product's own `bundle_version` reads out of `richos-user-update/src/lib.rs` and asserts
# this function emits every one of them, so a key the product STARTS requiring makes that
# case red rather than making this file quietly insufficient. Drift lengthens the list.
#
# The file-shape rules `bundle_plist` enforces beyond parsing (a regular file, one link,
# under 1 MiB, no group- or other-write bit, no write ACL — lib.rs:576-586) are satisfied by
# `gui_bundle` writing it and chmod 644, and asserted by `C4`.
# ---------------------------------------------------------------------------------------
gui_bundle_plist() {
  local identifier="$1" version="$2" exe_name="$3"
  cat <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>$identifier</string>
	<key>CFBundleShortVersionString</key>
	<string>$version</string>
	<key>CFBundleVersion</key>
	<string>$version</string>
	<key>CFBundleExecutable</key>
	<string>$exe_name</string>
	<key>CFBundleName</key>
	<string>RichOS</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
</dict>
</plist>
PLIST
}

# ---------------------------------------------------------------------------------------
# gui_bundle <bundle-root> <executable> <identifier> <version>
#
# Lay out a `.app` the product accepts: `Contents/MacOS/<name>` and `Contents/Info.plist`.
#
# IT TAKES THE EXECUTABLE AS AN ARGUMENT, and that is not decoration. `C1`-`C4` call this
# with a stand-in executable in a scratch directory, so the bundle this harness gives the
# product is held to account on a host that can build no machine and boot nothing — which is
# every public runner, and is the only place the defect above would ever have been seen.
# A version of this that reached for `$GUI_BINARY` itself would be untestable exactly where
# the testing was needed.
#
# Directory modes are 755 rather than left to the umask: `owned_dir` (lib.rs:120-126)
# refuses any application directory carrying a group- or other-write bit.
# ---------------------------------------------------------------------------------------
gui_bundle() {
  local root="$1" exe="$2" identifier="$3" version="$4"
  local name; name="$(basename "$exe")"
  mkdir -p "$root/Contents/MacOS" || return 1
  chmod 755 "$root" "$root/Contents" "$root/Contents/MacOS" || return 1
  cp "$exe" "$root/Contents/MacOS/$name" || return 1
  chmod 755 "$root/Contents/MacOS/$name" || return 1
  gui_bundle_plist "$identifier" "$version" "$name" > "$root/Contents/Info.plist" || return 1
  chmod 644 "$root/Contents/Info.plist" || return 1
  return 0
}

# ---------------------------------------------------------------------------------------
# gui_display_counts
#
# HOW MANY DISPLAYS THE BOOT WILL BE TOLD ARE THERE — asked of the same list the product
# asks, before the product is asked.
#
# `read_displays` (main.rs:5703) calls `app.available_monitors()`, and that reaches the
# machine through
#
#     tauri-2.11.5/src/app.rs:888
#       -> tauri-runtime-wry-2.11.4/src/lib.rs:2805
#         -> tao-0.35.3/src/platform_impl/macos/monitor.rs:146
#
# where it is, in full, `CGDisplay::active_displays()` — that is, `CGGetActiveDisplayList`.
#
# ACTIVE IS NOT ONLINE, AND THAT DISTINCTION IS THE WHOLE REASON THIS FUNCTION EXISTS.
# Apple's active display is one that is connected, AWAKE, and available for drawing. A Mac
# whose screens have gone to sleep still has every display ONLINE and has NO ACTIVE DISPLAY
# AT ALL — so the boot is told, truthfully, that there is no display to place a window on,
# prints `window: the runtime reported no attached display`, and B2 goes red over the state
# of the SCREEN rather than the state of the code.
#
# MEASURED ON THIS MACHINE, 2026-09-17, because this is not a story about what could happen:
#
#   pmset -g log        displays off 02:33:47 +0100 (01:33:47Z), on 03:16:29 +0100 (02:16:29Z)
#   nightly 815e318a    ended 23:59:24Z      screens awake   ->  all 29 passed
#   nightly 61831cdb    ended 00:42:10Z      screens awake   ->  all 29 passed
#   nightly 84aa5e62    02:06:26-02:13:09Z   screens ASLEEP  ->  1 FAILED (B2), 28 passed
#   re-run by hand      02:14Z               screens ASLEEP  ->  1 FAILED (B2), 28 passed
#   re-run, 516975db    02:25:06-02:27:11Z   screens awake   ->  all 29 passed
#
# The last line is the one that settles it: the SAME source that "broke" B2 passes it, on
# this machine, with nothing changed but the power state of the screen. A nightly release
# and two hours went into hunting a commit for a defect the source never contained. That is
# what an undeclared precondition costs, which is why this one is now declared, measured and
# printed on every run.
#
# python3 + ctypes rather than a compiled probe: python3 is already required by six scripts
# in this directory, and a precondition check that needs a C compiler is a check that cannot
# run on the machine it is a claim about.
#
# Prints "<active> <online>". Prints "unknown unknown" and returns 1 when the question could
# not be put to the machine at all — which is NOT a zero and is never read as one.
# ---------------------------------------------------------------------------------------
gui_display_counts() {
  local out
  if ! out="$(python3 - <<'PY' 2>/dev/null
import ctypes
import sys

MAX = 64
try:
    cg = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
    ids = (ctypes.c_uint32 * MAX)()
    active = ctypes.c_uint32(0)
    if cg.CGGetActiveDisplayList(ctypes.c_uint32(MAX), ids, ctypes.byref(active)) != 0:
        raise OSError("CGGetActiveDisplayList refused")
    online = ctypes.c_uint32(0)
    if cg.CGGetOnlineDisplayList(ctypes.c_uint32(MAX), ids, ctypes.byref(online)) != 0:
        raise OSError("CGGetOnlineDisplayList refused")
    print(active.value, online.value)
except Exception:
    sys.exit(1)
PY
  )"; then
    printf 'unknown unknown\n'
    return 1
  fi
  case "$out" in
    ''|*[!0-9\ ]*) printf 'unknown unknown\n'; return 1 ;;
  esac
  printf '%s\n' "$out"
  return 0
}

# ---------------------------------------------------------------------------------------
# gui_display_verdict <active-count>
#
# THE DECISION, HELD APART FROM THE MEASUREMENT, so the suite can prove the decision is
# alive on numbers this run did not produce (D1-D3). It prints the sentence and answers with
# its exit code:
#
#   0  at least one display is awake — a boot may be held to a display-backed placement
#   1  none are awake — this host cannot answer the question B2 asks
#   2  the count is not a number — nothing was measured, and an unmeasured premise is not a
#      zero and not a pass
#
# The three are separated because the OPERATOR has a different thing to do in each: wake the
# screen, or install a python3 that can reach CoreGraphics. A verdict that collapsed them
# would send whoever reads it to the wrong repair.
# ---------------------------------------------------------------------------------------
gui_display_verdict() {
  local active="${1:-}"
  case "$active" in
    ''|*[!0-9]*)
      printf 'display state UNMEASURED — this host could not be asked how many displays are awake\n'
      return 2
      ;;
    0)
      printf 'NO DISPLAY IS AWAKE — CGGetActiveDisplayList reports 0, so the boot would be told there is none\n'
      return 1
      ;;
    *)
      printf '%s display(s) awake — the boot can be held to a placement backed by one of them\n' "$active"
      return 0
      ;;
  esac
}

# ---------------------------------------------------------------------------------------
# gui_boot <scratch-root> <output-file> [seconds]
#
# Boot it, capture stderr, and stop the moment the process says it has finished resolving.
#
# TERMINATION IS A FACT, NOT A SLEEP. The app prints `[richos] boot complete` as the last
# line of `setup`; this waits for THAT and kills the process the moment it arrives. A
# harness that slept for a fixed number of seconds would be reading a different amount of
# boot log on a busy machine than on an idle one, which is the precise shape of a check that
# goes red for no reason and stops being believed.
#
# Exit 0: the marker arrived. Exit 7: the timeout expired first — which is a REAL failure
# ("this build did not finish booting"), reported as one and never as a pass.
#
# ---------------------------------------------------------------------------------------
# EVERY LAUNCH IS KILLED BY THE THING THAT MADE IT, ON EVERY PATH
# ---------------------------------------------------------------------------------------
#
# THIS WAS WRONG AND IT REACHED THE CEO'S DOCK. The first version of this function wrote
#
#     ( cd / && env -i ... "$bin" >> "$out" 2>&1 ) &
#     local pid=$!
#
# and `$!` there is the SUBSHELL, not the app. `cd / && env …` is a compound list, so bash
# forks a subshell which then forks `env`, which execs the binary — and the `kill` below
# reached the first of them. The app was orphaned to PID 1 and kept running with its window.
# Measured on 2026-09-01: 157 live `richos-tauri` processes, roughly six per round across
# about twenty-six rounds, every one from a `<tmp>/…/RichOS.app` whose directory had already
# been deleted. Deleting a temp directory does not kill what is running out of it —
# CLAUDE.md's zombie-residue rule, in the exact shape the rule describes.
#
# Three changes, and each is load-bearing:
#
#   1. `exec` — the subshell REPLACES itself with the binary, so `$!` is the app's own pid.
#      One process, and the pid this function holds is the pid it needs to signal.
#   2. TERM, then a bounded wait, then KILL, then VERIFY the pid is gone. A signal sent is
#      not a process ended.
#   3. every pid is appended to `$GUI_LAUNCHED_PIDS` before the wait begins, so the caller's
#      EXIT trap can reap it even if this function is interrupted before it gets to kill it.
#      The FAILING rounds are exactly the ones that leave processes behind.
# ---------------------------------------------------------------------------------------
gui_boot() {
  local home="$1" out="$2" limit="${3:-60}"
  local bin="$home/Applications/RichOS.app/Contents/MacOS/richos-tauri"
  : > "$out"

  # cwd=/ and an empty environment, exactly as measured. `USER` is passed because launchd
  # passes it; nothing reads it, and leaving it out would make this a condition no launch
  # actually produces. `exec` so that this subshell IS the app — see the header above.
  #
  # CFFIXED_USER_HOME IS THE SAME HOME, AND HOME ALONE IS NOT ENOUGH. Foundation answers
  # NSHomeDirectory() from the account record, not from HOME, so a boot under HOME alone put
  # WebKit's store and the URL cache in the REAL ~/Library/WebKit/com.richos.app and
  # ~/Library/Caches/com.richos.app, the folders his daily driver (same bundle identifier)
  # uses. Measured in a test guest on 2026-09-24 (richos-hq
  # docs/verification/2026-09-24-nightly-launcher/, run 1 finding 3); nightly-launch.sh sets
  # both for that reason. On a real double-click the two agree, so setting both is the
  # condition a launch actually has: one home.
  ( cd / && exec env -i HOME="$home" CFFIXED_USER_HOME="$home" USER="${USER:-unknown}" \
      PATH=/usr/bin:/bin:/usr/sbin:/sbin "$bin" >> "$out" 2>&1 ) &
  local pid=$!
  [ -n "${GUI_LAUNCHED_PIDS:-}" ] && printf '%s\n' "$pid" >> "$GUI_LAUNCHED_PIDS"

  local rc=7 waited=0
  while [ "$waited" -lt "$((limit * 10))" ]; do
    if grep -q '^\[richos\] boot complete' "$out" 2>/dev/null; then
      rc=0
      break
    fi
    # The process can also DIE before finishing — a panic in `setup`, a failed window
    # build. That is a result, not a reason to wait out the clock.
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null
      echo "[gui_boot] the process exited before printing 'boot complete'" >> "$out"
      return 7
    fi
    sleep 0.1
    waited=$((waited + 1))
  done
  [ "$rc" -eq 7 ] && echo "[gui_boot] no 'boot complete' within ${limit}s" >> "$out"
  gui_kill "$pid" || echo "[gui_boot] pid $pid SURVIVED both TERM and KILL" >> "$out"
  wait "$pid" 2>/dev/null
  return "$rc"
}

# ---------------------------------------------------------------------------------------
# gui_kill <pid>
#
# End it, and PROVE it ended. Exit 0 when the pid is gone, 1 when it survived both signals —
# which the caller reports rather than swallowing, because a kill that failed quietly is how
# 157 of these accumulated.
# ---------------------------------------------------------------------------------------
gui_kill() {
  local pid="$1" i=0
  kill -0 "$pid" 2>/dev/null || return 0
  kill -TERM "$pid" 2>/dev/null
  while [ "$i" -lt 30 ]; do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.1
    i=$((i + 1))
  done
  kill -KILL "$pid" 2>/dev/null
  i=0
  while [ "$i" -lt 30 ]; do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.1
    i=$((i + 1))
  done
  return 1
}

# ---------------------------------------------------------------------------------------
# gui_reap_all
#
# The backstop, run from the suite's EXIT trap so it runs on the failing paths too. Kills
# every pid this run recorded, then prints how many survived — a NUMBER, so "there is no
# residue" is measured rather than assumed.
#
# It only ever touches pids THIS RUN launched, read out of `$GUI_LAUNCHED_PIDS`. The CEO's
# own install at ~/Applications/RichOS.app is never started here and no signal from here can
# reach it.
# ---------------------------------------------------------------------------------------
gui_reap_all() {
  local survivors=0 pid
  if [ -n "${GUI_LAUNCHED_PIDS:-}" ] && [ -f "$GUI_LAUNCHED_PIDS" ]; then
    while IFS= read -r pid; do
      [ -n "$pid" ] || continue
      gui_kill "$pid" || survivors=$((survivors + 1))
    done < "$GUI_LAUNCHED_PIDS"
  fi
  printf '%s\n' "$survivors"
}
