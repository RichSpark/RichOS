#!/usr/bin/env bash
# run.sh — put the RichOS app on a screen that is NOT the CEO's, and tell the
#          caller how to drive it.
#
#   testvm/run.sh --bundle <RichOS.app.zip|RichOS.app> --home <fixture-home-dir>
#                 [--vm <name>] [--keep] [--engine <engine.tar.gz>] [--no-tailnet]
#   testvm/run.sh --no-app --home <fixture-home-dir> [--vm <name>] [--keep] [--no-tailnet]
#   testvm/run.sh --bundle <...> --home <fixture-home-dir> --new-user [--vm <name>] [--no-tailnet]
#
# --new-user: the app is launched the way a person's first double-click launches it,
# with HOME and nothing else the harness normally hands it: no RICHOS_CLAUDE_BIN, no
# CLAUDE_CONFIG_DIR, no RICHOS_ENGINE_DIR, no DISABLE_AUTOUPDATER, no Claude login pushed
# into the fixture home and no engine payload copied in. The fixture home has no
# `claude` under it, so the app finds Claude Code the way it does on a new customer's
# Mac (or installs it), fetches its engine from its own pin, and reaches Claude signed
# out. A walk signs in later with `claude-login.sh push` when it means to. Every
# harness default left out here is one a new user does not have: a first-run walk
# that inherits them proves the harness, not the product (new-user-walk.py).
#
# --no-app: the same guest (the host's claude synced in, the host's login pushed
# into the fixture home, a login keychain ready) and NO app launched, for a proof
# about `claude` itself rather than about the app (`run-walk.py --no-app`). It
# prints `pid=none` and nothing is drawn.
#
# Prints, on success:
#   vm=<name> ip=<guest ip> pid=<app pid IN THE GUEST> ssh=<user@ip> elapsed=<s>
#   tailnet=<dnsname|not-joined>
#
# Each --vm name is an INDEPENDENT guest cloned from the provisioned base, so
# two runs can each have a guest and a window, at the same time, neither touching
# the host's screen. That parallelism is the point: on-screen proofs used to be a
# queue of one behind the CEO's working day. Each of the two runs holds one guest
# slot (slots.py) for its own length only; this script refuses to boot outside one.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib.sh"

BUNDLE=""; FIXTURE_HOME=""; VM="richos-test-1"; ENGINE=""; KEEP=0; TAILNET=1; NOAPP=0; NEWUSER=0
while [ $# -gt 0 ]; do
  case "$1" in
    --bundle) BUNDLE="$2"; shift 2 ;;
    --no-app) NOAPP=1; shift ;;
    --new-user) NEWUSER=1; shift ;;
    --home)   FIXTURE_HOME="$2"; shift 2 ;;
    --vm)     VM="$2"; shift 2 ;;
    --engine) ENGINE="$2"; shift 2 ;;
    --keep)   KEEP=1; shift ;;
    # The tailnet join is ON by default because the phone path is the reason
    # this VM exists at all (CEO §61: there is one path, and it is Tailscale).
    # It is also the only step that can refuse without failing the run, so
    # --no-tailnet is for a proof that has nothing to do with the phone and
    # would rather not spend twenty seconds and a node on one.
    --no-tailnet) TAILNET=0; shift ;;
    *) die "unknown argument: $1" ;;
  esac
done
if [ "$NEWUSER" -eq 1 ]; then
  [ "$NOAPP" -eq 0 ] || die "--new-user launches the app as a new user does: it needs --bundle, not --no-app"
  # A new user has no engine payload handed to them: the app fetches its own.
  ENGINE=""
fi
if [ "$NOAPP" -eq 1 ]; then
  [ -z "$BUNDLE" ] || die "--no-app and --bundle contradict each other: say which one"
else
  [ -n "$BUNDLE" ] || die "--bundle is required"
  [ -e "$BUNDLE" ] || die "no such bundle: $BUNDLE"
fi
[ -n "$FIXTURE_HOME" ] || die "--home is required"
[ -d "$FIXTURE_HOME" ] || die "no such fixture home: $FIXTURE_HOME"
[ "$VM" != "$TESTVM_BASE_VM" ] || die "refusing to run in the base VM — it is the template every clone comes from"

# --- a guest boots only inside a slot, for exactly one run --------------------
# The CEO, 2026-09-27: *"do I want the work to be BLOCKED AND PISSED AWAY like this because
# the current worker might need the VM for a 5-second-long fart?"* A guest booted here by
# hand is a guest nobody else can see or be scheduled around, kept up while its owner
# thinks. So this boots only for a caller that holds one of the two guest slots
# (slots.py): run-walk.py, gui-proof-in-vm.sh, or `slots.py run -- ...`.
# The slot is recorded in the run state below, and a slot released with its guest still
# up stops that guest (a guest never outlives its slot).
SLOT="$(python3 "$HERE/slots.py" check 2>&1)" || die "no guest is booted outside a guest slot: $SLOT.
  Put the steps in a script that takes the VM name as its first argument and run it once:
    $HERE/run-walk.py --bundle <zip> --home <home> --engine <engine> --report <json> -- <script>
  The slot is held for that run only and is free again the moment it ends."

# --- 0. what this host can give the guest, asked BEFORE anything boots --------
# Both of these are properties of THIS Mac, so they are answered before a 25 GB
# clone exists to clean up. A guest gets the host's claude binary and the host's
# claude login at every run (see claude-sync.sh / claude-login.sh); a host that
# has neither cannot hand over either, and finding that out after a boot costs a
# minute and leaves a VM behind.
host_claude_path >/dev/null 2>&1 \
  || die "no claude binary on this host at $TESTVM_HOST_CLAUDE — the app shells out to it; install Claude Code first"
# host-check says WHY it refuses (signed out, or the app's account list unreadable), and that
# reason is the one passed on: a fixed "not logged in" sent the reader to /login for a list
# that could not be read (Tom's review of 9bbb8777).
HOST_LOGIN_REFUSAL="$("$HERE/claude-login.sh" host-check 2>&1 >/dev/null)" \
  || die "the host's Claude login cannot be handed to the guest: ${HOST_LOGIN_REFUSAL#\[testvm\] }"

START=$(date +%s)
preflight_tart
vm_exists "$TESTVM_BASE_VM" || die "base VM missing — run testvm/setup.sh first"

STATE="$TESTVM_RUN/$VM"
mkdir -p "$STATE"
printf '%s\n' "$SLOT" > "$STATE/slot"

# --- 1. an ephemeral clone ----------------------------------------------------
# Cloning is copy-on-write on APFS: a new guest costs seconds and almost no
# disk until it diverges. This is what makes a per-run disposable VM practical
# rather than a 25 GB copy each time.
CREATED_CLONE=0
if ! vm_exists "$VM"; then
  CREATED_CLONE=1
  log "cloning $TESTVM_BASE_VM -> $VM (copy-on-write)"
  # The base must be stopped to clone a consistent disk.
  if vm_running "$TESTVM_BASE_VM"; then
    log "stopping the base VM so the clone is consistent..."
    tart stop "$TESTVM_BASE_VM" 2>/dev/null || true
    # Clone the moment the base reports stopped, not after a fixed 3 s; 30 s is the ceiling.
    wait_until_vm_stopped "$TESTVM_BASE_VM" 60 0.5 || die "$TESTVM_BASE_VM did not stop within 30s; not cloning an inconsistent disk"
  fi
  tart clone "$TESTVM_BASE_VM" "$VM"
  echo ephemeral > "$STATE/ephemeral"
fi
tart set "$VM" --cpu "$TESTVM_CPU" --memory "$TESTVM_RAM_MB" --display "$TESTVM_DISPLAY"

# --- 2. boot headless ---------------------------------------------------------
if ! vm_running "$VM"; then
  log "booting $VM headless..."
  # --no-graphics: the guest keeps a full virtual display and WindowServer;
  # the host simply never renders a window for it. This one flag is the
  # difference between a proof and an interruption.
  #
  # Tests must respect the host's power and sleep policy (§65). Launch Tart
  # directly: even a system-only wake lock is forbidden by the host shield.
  # The guest has its own WindowServer and needs no host display changes.
  nohup "$TART_BIN" run "$VM" --no-graphics > "$TESTVM_LOG/$VM.log" 2>&1 &
  echo $! > "$STATE/vm.pid"
fi

log "waiting for $VM to report an IP..."
IP="$(vm_ip "$VM")" || die "$VM never reported an IP — see $TESTVM_LOG/$VM.log"
echo "$IP" > "$STATE/ip"
log "$VM is up at $IP"

for i in $(seq 1 30); do
  if guest_ssh "$VM" true 2>/dev/null; then break; fi
  [ "$i" -eq 30 ] && die "ssh into $VM never came up"
  sleep 2
done

# §54: a refusal takes this run's own garbage with it. A guest that was ALREADY
# there when this script started belongs to whoever started it and is left alone.
clean_up_this_runs_clone() {
  if [ "$CREATED_CLONE" -eq 1 ] && [ "$KEEP" -eq 0 ]; then
    log "cleaning up the clone this run created..."
    "$HERE/stop.sh" "$VM" >&2 || log "WARNING: stop.sh did not clean up $VM — remove it by hand: tart delete $VM"
  else
    log "$VM was already running before this call, so it is left alone."
  fi
}

# --- 2-. the screen-capture consent -------------------------------------------
# BEFORE anything captures: macOS 15 asks again every 30 days whether the ssh client may
# capture the screen, and the base image's approval came due on 2026-10-19, so a clone's first
# frame raised an alert that took the keyboard focus from the app (capture-consent.py says what
# was measured). Each clone's record is renewed; a failure is said, and the run goes on.
if CONSENT="$(guest_ssh "$VM" /usr/bin/python3 - < "$HERE/capture-consent.py" 2>&1)"; then
  log "$CONSENT"
else
  log "WARNING: the screen-capture consent could not be renewed (${CONSENT:-no output}); a frame may raise macOS's alert"
fi

# --- 2a. the claude binary ----------------------------------------------------
# FIRST, because it is the cheapest refusal in the file when it fires and the
# most expensive defect when it does not. The binary was copied into the base
# image once, at setup, and never looked at again — while the host's Claude Code
# auto-updated underneath it. The CEO asked what happens to it (2026-09-20);
# what happened was silent, growing drift between the `claude` the app shelled
# out to in here and the one he actually runs.
CLAUDE_LINE="$("$HERE/claude-sync.sh" "$VM")" && CLAUDE_RC=0 || CLAUDE_RC=$?
printf '%s\n' "$CLAUDE_LINE" >&2
if [ "$CLAUDE_RC" -ne 0 ]; then
  cat >&2 <<EOF

========================================================================
[testvm] REFUSED: $VM IS NOT RUNNING THE CLAUDE BINARY THIS MAC RUNS
========================================================================
  $CLAUDE_LINE

  The app shells out to \`claude\`. A guest holding a different build is not
  the Mac the CEO uses, and every model turn measured in it would be measuring
  a version nobody ships. The copy was attempted and did not take; the cause is
  above this banner.

  Ask again, without booting anything:
    $HERE/claude-sync.sh --check $VM
========================================================================
EOF
  clean_up_this_runs_clone
  exit 1
fi

# --- 2b. the tailnet ----------------------------------------------------------
# BEFORE the app launches, not after: the app asks Tailscale where it is on
# startup and caches the answer (phone/mod.rs `tailnet_now`), so a guest that
# joins afterwards shows the user "this Mac is not set up yet" until the cache
# ages out. Joining first means the first screen he reads is the true one.
#
# A JOIN THAT FAILS IS A REFUSAL, AND IT WAS NOT — CHANGED 2026-09-20.
#
# This step used to swallow its own exit status and carry on, putting
# `tailnet=not-joined` in the summary with the cause thirty lines away on
# stderr. Ray raised it on `.7`, raised it again on `.8` — *"A tester who misses
# that line walks the whole path against a Mac that cannot serve"* — and Echo
# reproduced it on `.9`. Twice in a row a walk of the phone path went ahead
# against a guest that could not serve one, and both times the harness had said
# so in a way nobody read.
#
# So the tailnet is what it says it is: ON by default because the phone path is
# the reason this VM exists (CEO §61), and `--no-tailnet` the way to say this
# proof is not about the phone. Asked for and not delivered is a stop, with the
# cause on the same screen as the refusal — never a degraded run that looks like
# a successful one.
TAILNET_NAME="not-joined"
if [ "$TAILNET" -eq 1 ]; then
  # `VAR=$(cmd)` UNDER `set -e` EXITS ON A FAILING cmd, before any `$?` can be
  # read — which is why the line this replaces ended in `&& true` and threw the
  # status away. Written as a compound list, errexit does not fire and `$?` in
  # the `||` branch is the assignment's own status. Proved by driving this file
  # with a `tailnet.sh` that exits 1: without the compound form the script dies
  # silently at this line and the refusal block below never runs at all.
  JOIN_OUT="$("$HERE/tailnet.sh" join "$VM" 2>&1)" && JOIN_RC=0 || JOIN_RC=$?
  printf '%s\n' "$JOIN_OUT" >&2
  case "$JOIN_OUT" in
    *tailnet=*) TAILNET_NAME="$(printf '%s\n' "$JOIN_OUT" | tailnet_name_from_join_output)" ;;
  esac
  [ -n "$TAILNET_NAME" ] || TAILNET_NAME="not-joined"
  if [ "$JOIN_RC" -ne 0 ] || [ "$TAILNET_NAME" = "not-joined" ]; then
    WHY="$(printf '%s\n' "$JOIN_OUT" | tailnet_cause_from_join_output)"
    # The diagnosis is taken BEFORE the guest is destroyed, because after it
    # there is nothing left to ask. It is best effort by construction: a guest
    # whose daemon never came up cannot answer most of these either.
    DOCTOR="$("$HERE/tailnet.sh" doctor "$VM" 2>&1)" || true
    cat >&2 <<EOF

========================================================================
[testvm] REFUSED: $VM IS NOT ON THE TAILNET, AND THE PHONE PATH NEEDS IT
========================================================================
  cause:  $WHY

  There is ONE phone path and it is Tailscale (CEO §61). A guest that is not
  on the tailnet cannot serve a pairing code: the app answers "This Mac is
  not set up yet" and any walk of the phone path against it is measuring the
  harness, not the product. That is what happened on nightly .7, .8 and .9,
  and it is why this is a stop rather than a line in a summary.

  WHAT THE GUEST SAID, while it was still there to ask:
$(printf '%s\n' "$DOCTOR" | sed 's/^/    /')

  IF THIS PROOF IS NOT ABOUT THE PHONE, say so and it costs nothing:
    $0 --bundle <bundle> --home <home> --vm $VM --no-tailnet
========================================================================
EOF
    # §54: this run's own garbage goes with it. A guest that was ALREADY there
    # when this script started belongs to whoever started it and is left alone.
    if [ "$CREATED_CLONE" -eq 1 ] && [ "$KEEP" -eq 0 ]; then
      log "cleaning up the clone this run created..."
      "$HERE/stop.sh" "$VM" >&2 || log "WARNING: stop.sh did not clean up $VM — remove it by hand: tart delete $VM"
    else
      log "$VM was already running before this call, so it is left alone."
      log "diagnose: $HERE/tailnet.sh doctor $VM      stop: $HERE/stop.sh $VM"
    fi
    exit 1
  fi
fi

# --- 3. stage the payload -----------------------------------------------------
PAYLOAD="/Users/$TESTVM_GUEST_USER/testvm/$VM"
# Named here rather than at the launch, because step 3b below prepares this
# home's keychain and the launch must use the same path it was prepared for.
GUEST_HOME="$PAYLOAD/home"
guest_ssh "$VM" "rm -rf '$PAYLOAD' && mkdir -p '$PAYLOAD'"

if [ "$NOAPP" -eq 0 ]; then
log "copying the bundle in..."
if [ "${BUNDLE##*.}" = "zip" ]; then
  scp "${TESTVM_SSH_OPTS[@]}" -O -i "$TESTVM_SSH_KEY" "$BUNDLE" \
    "$TESTVM_GUEST_USER@$IP:$PAYLOAD/bundle.zip" >/dev/null
  guest_ssh "$VM" "cd '$PAYLOAD' && ditto -x -k bundle.zip . && rm -f bundle.zip"
else
  scp "${TESTVM_SSH_OPTS[@]}" -O -r -i "$TESTVM_SSH_KEY" "$BUNDLE" \
    "$TESTVM_GUEST_USER@$IP:$PAYLOAD/" >/dev/null
fi
APP="$(guest_ssh "$VM" "ls -d '$PAYLOAD'/*.app 2>/dev/null | head -1")"
[ -n "$APP" ] || die "no .app found in the copied bundle"

# Strip quarantine BEFORE the first launch. A bundle that arrived over scp is
# flagged "downloaded from the internet", and the first launch would raise a
# Gatekeeper dialog that nothing in a headless guest can click — the app would
# simply never appear, with no error anywhere.
guest_ssh "$VM" "xattr -dr com.apple.quarantine '$APP' 2>/dev/null || true"
fi

log "copying the fixture home in..."
# tar over ssh, not scp -r: the fixture home contains symlinks and a Library
# tree whose structure matters, and scp -r silently follows and flattens
# symlinks. Preserving them is the difference between testing the app and
# testing a copy of somebody's home directory.
tar -C "$FIXTURE_HOME" -cf - . \
  | guest_ssh "$VM" "mkdir -p '$PAYLOAD/home' && tar -C '$PAYLOAD/home' -xf -"

# --- 3b. a login keychain that exists and is unlocked -------------------------
# AFTER the fixture home is in and BEFORE the app launches, because the keychain
# belongs to this run's home and the app reads it on its first pairing.
#
# Ray, .8 defect 5: the fixture's `Library/Keychains` is a real EMPTY directory,
# so `security` finds no keychain and `Set my phone up` raises `Keychain Not
# Found` — and .8 defect 2 measured 120 s of "Getting this Mac ready…" behind
# that dialog. On the CEO's Mac the login keychain exists and is unlocked at
# login; this makes the guest match, in the guest, never with his.
#
# NOT fatal. A guest whose keychain could not be prepared still renders, still
# screenshots, and still proves everything that is not a stored secret — but it
# says so HERE rather than letting a tester discover it 120 seconds into a
# pairing, which is the whole lesson of the tailnet line above.
KEYCHAIN_STATE="$("$HERE/keychain.sh" prepare "$VM" "$GUEST_HOME" 2>&1)" && KEYCHAIN_RC=0 || KEYCHAIN_RC=$?
printf '%s\n' "$KEYCHAIN_STATE" >&2
if [ "$KEYCHAIN_RC" -ne 0 ]; then
  cat >&2 <<EOF
[testvm] WARNING: $VM has no usable login keychain in this run's fixture home.
         'Set my phone up' will raise 'Keychain Not Found' and the sheet will
         sit on "Getting this Mac ready..." for two minutes behind it (Ray .8,
         defects 2 and 5). Everything that stores no secret still works.
         Diagnose:  $HERE/keychain.sh check $VM $GUEST_HOME
EOF
fi

# --- 3c. the claude login -----------------------------------------------------
# AFTER the keychain exists and is unlocked, BEFORE the app launches. The guest
# receives an unexpired access snapshot from this Mac at every run. Its refresh
# token stays on the host, so the guest cannot rotate the host login. The
# snapshot can expire during a long run; authenticated checks must then stop
# or provision a fresh snapshot. The value travels on stdin, never in argv
# or logs. See claude-login.sh.
#
# NOT fatal, for the same reason the keychain step is not: everything that needs
# no model turn still renders, still screenshots and still proves what it was
# asked to. It says so here rather than at the first model turn.
#
# WHOSE login: the account the app has in use on this Mac (claude-accounts.json
# inUse; claude-login.sh "WHICH ACCOUNT"), so a guest is never signed in as an
# account the app has already left for being nearly used up.
if [ "$NEWUSER" -eq 1 ]; then
  # A new user is signed out until they sign in; the walk signs in when it means to.
  CLAUDE_LOGIN_LINE="claude login: guest NOT logged in (--new-user: nothing pushed)"
else
CLAUDE_LOGIN_LINE="$("$HERE/claude-login.sh" push "$VM" "$GUEST_HOME")" || true
fi
printf '%s\n' "$CLAUDE_LOGIN_LINE" >&2
# His second sign-in (Work), the same way, with no step for him: the host folder
# is where the app keeps Account 2 on this Mac, the host item is named for it
# (sha256 of the folder, first 8 hex digits: claude-login.sh header), and the
# guest gets it under its own folder. Access token only. Skipped silently when
# this Mac has no Work item; never fatal. No keeper: an access token outlives a round.
WORK_HOST_FOLDER="${TESTVM_WORK_HOST_FOLDER:-$HOME/Library/Application Support/com.richos.app/claude-accounts/2}"
WORK_HOST_ITEM="Claude Code-credentials-$(printf '%s' "$WORK_HOST_FOLDER" | shasum -a 256 | cut -c1-8)"
WORK_GUEST_FOLDER="$GUEST_HOME/claude-accounts/2"
if [ "$NEWUSER" -eq 0 ] && /usr/bin/security find-generic-password -s "$WORK_HOST_ITEM" -a "${USER:-alex}" >/dev/null 2>&1; then
  "$HERE/claude-login.sh" push --account "$WORK_HOST_ITEM" --host-folder "$WORK_HOST_FOLDER" \
    --guest-home "$GUEST_HOME" "$VM" "$WORK_GUEST_FOLDER" >&2 || true
fi
# The keeper: for the run's lifetime, each access token THIS Mac renews is
# handed to the guest, so a run longer than one token's remaining life keeps
# working without the guest ever holding a refresh token (claude-login.sh, "A
# LONG RUN OUTLIVES ONE ACCESS TOKEN"). Its pid is recorded here and stop.sh
# ends exactly that pid; it also ends by itself once this run's state is gone.
# Nothing it prints reaches this script's output (a caller reading run.sh's
# pipe must not wait on it).
if [ "$CLAUDE_LOGIN_LINE" = "claude login: guest logged in" ]; then
  nohup "$HERE/claude-login.sh" keep "$VM" "$GUEST_HOME" \
    >>"$STATE/claude-keep.log" 2>&1 </dev/null &
  echo $! > "$STATE/claude-keep.pid"
fi

if [ -n "$ENGINE" ] && [ -e "$ENGINE" ]; then
  log "copying the engine payload in..."
  scp "${TESTVM_SSH_OPTS[@]}" -O -i "$TESTVM_SSH_KEY" "$ENGINE" \
    "$TESTVM_GUEST_USER@$IP:$PAYLOAD/engine.tar.gz" >/dev/null
  # --strip-components 1, AND A CHECK — FIXED 2026-09-20, from Ray's vm9 audit.
  #
  # `--engine` had never worked. The release tarball's only top-level member is
  # `engine/` (`tar -tzf richos-engine-1.2.0.tar.gz | head -1` → `engine/`), so
  # untarring it into `$PAYLOAD/engine` produced `$PAYLOAD/engine/engine/…`.
  # `setup::engine_looks_valid` asks for `<dir>/scripts/hooks` and `<dir>/VERSION`
  # and found neither one level up, so the app said *"the RichOS engine isn't on
  # this Mac"* — which is exactly what nightlies .7 and .8 both reported, and why
  # neither could measure Mac → phone.
  #
  # The check after the untar is the point, not the flag: a silently-wrong
  # directory shape is what cost two walks, and a flag alone would be another
  # thing believed rather than measured. Same predicate as the app's, so this
  # cannot pass while the app disagrees.
  guest_ssh "$VM" "mkdir -p '$PAYLOAD/engine' && tar -C '$PAYLOAD/engine' --strip-components 1 -xzf '$PAYLOAD/engine.tar.gz'"
  ENGINE_SHAPE="$(guest_ssh "$VM" "if [ -d '$PAYLOAD/engine/scripts/hooks' ] && [ -f '$PAYLOAD/engine/VERSION' ]; then echo ok; else echo bad; fi" | tr -d '[:space:]')"
  if [ "$ENGINE_SHAPE" != "ok" ]; then
    NESTED="$(guest_ssh "$VM" "ls '$PAYLOAD/engine' 2>/dev/null | head -5 | tr '\n' ' '")"
    cat >&2 <<EOF

========================================================================
[testvm] REFUSED: THE ENGINE PAYLOAD IS NOT AN ENGINE IN THE GUEST
========================================================================
  $PAYLOAD/engine holds: $NESTED

  The app asks for <dir>/scripts/hooks and <dir>/VERSION (setup::engine_looks_valid)
  and would answer "the RichOS engine isn't on this Mac", which is how nightlies
  .7 and .8 both lost the Mac -> phone half of the walk. The tarball is expected to
  have ONE top-level member; check it with:
    tar -tzf $ENGINE | head -3
========================================================================
EOF
    clean_up_this_runs_clone
    exit 1
  fi
  log "engine payload verified in the guest (scripts/hooks + VERSION present)"
fi

# --- 3d. --no-app stops here, with the guest ready and nothing launched -------
if [ "$NOAPP" -eq 1 ]; then
  echo "$PAYLOAD" > "$STATE/payload"
  ELAPSED=$(( $(date +%s) - START ))
  log "ready in ${ELAPSED}s (no app launched: --no-app)"
  echo "vm=$VM ip=$IP pid=none ssh=$TESTVM_GUEST_USER@$IP windows=0 elapsed=${ELAPSED}s"
  echo "tailnet=$TAILNET_NAME"
  echo "$CLAUDE_LINE"
  echo "$CLAUDE_LOGIN_LINE"
  echo "home:  $GUEST_HOME"
  echo "guest: $HERE/guest.sh $VM '<shell command>'   # --pull/--push to move a file"
  echo "stop:  $HERE/stop.sh $VM      # ALWAYS, before you report (CEO §54)"
  exit 0
fi

# --- 4. launch --------------------------------------------------------------
# RICHOS_ACTIVATION=regular asks for the front: Dock icon, key window, focus.
# On the host that is an intrusion and needs a CEO ruling (§45). In here it is
# free — the only screen it can take is the guest's.
# GUEST_HOME is set at step 3, where the payload path is named, so the keychain
# prepared at 3b and the HOME handed to the app below cannot drift apart.
LOGFILE="$PAYLOAD/app.log"

# `open -a` rather than executing the binary: a GUI app started directly from
# an ssh shell has no Aqua session and either dies or draws nothing. `open`
# routes the launch through launchd into the auto-logged-in user's GUI session,
# which is where a window can actually exist. --env carries the environment the
# host QA recipe uses, so the app under test is configured identically.
#
# TESTVM_APP_MODEL, when set (for example `sonnet`), is handed to the app as ANTHROPIC_MODEL,
# which every `claude` it starts inherits: a walk that only needs the app's machinery to run can
# spend a cheaper model. The app passes `--setting-sources ''`, so a settings file in the
# fixture home cannot do this. Unset, nothing changes.
#
# DISABLE_AUTOUPDATER=1 is on the launch, and it is not decoration: `claude`
# updates ITSELF, and a `claude` that updated itself mid-walk would silently
# stop being the version step 2a just measured and printed. The env var is the
# only pin that holds for a native install — the `autoUpdates:false` config key
# is ignored once `autoUpdatesProtectedForNative` is set, which the native
# updater sets itself. Reasoning and the quoted check: lib.sh, TESTVM_CLAUDE_PIN.
APP_MODEL_ENV=""
if [ -n "${TESTVM_APP_MODEL:-}" ]; then
  APP_MODEL_ENV="--env ANTHROPIC_MODEL='$TESTVM_APP_MODEL'"
fi
# TESTVM_APP_ENV, when set, adds the app's own test knobs to this launch (lib.sh app_env_args,
# which refuses anything but RICHOS_ names and plain values). Unset, nothing changes.
APP_EXTRA_ENV="$(app_env_args "${TESTVM_APP_ENV:-}")" || die "TESTVM_APP_ENV was refused; nothing was launched"
log "launching the app in the guest's GUI session..."
HARNESS_ENV="--env CLAUDE_CONFIG_DIR='$GUEST_HOME/.claude' \
    --env RICHOS_CLAUDE_BIN='$TESTVM_GUEST_CLAUDE' \
    --env $TESTVM_CLAUDE_PIN_VAR=$TESTVM_CLAUDE_PIN_VALUE \
    ${ENGINE:+--env RICHOS_ENGINE_DIR='$PAYLOAD/engine'} \
    $APP_MODEL_ENV"
# --new-user: HOME and the window's activation, nothing else of the harness's (see the header).
[ "$NEWUSER" -eq 0 ] || HARNESS_ENV=""
guest_ssh "$VM" "rm -f '$LOGFILE'; \
  open -n -a '$APP' \
    --env HOME='$GUEST_HOME' \
    --env RICHOS_ACTIVATION=regular \
    $HARNESS_ENV \
    $APP_EXTRA_ENV \
    --stdout '$LOGFILE' --stderr '$LOGFILE'"

# Wait for the process, then for a WINDOW. A pid is not a proof: the app can be
# running and drawing nothing, which is precisely the state a screenshot would
# photograph and call a pass.
PID=""
for i in $(seq 1 30); do
  PID="$(guest_ssh "$VM" "pgrep -n -f 'richos-tauri' 2>/dev/null || true" | tr -d '[:space:]')"
  [ -n "$PID" ] && break
  sleep 1
done
[ -n "$PID" ] || die "the app never started in the guest — log: ssh $TESTVM_GUEST_USER@$IP cat '$LOGFILE'"
echo "$PID" > "$STATE/app.pid"
echo "$PAYLOAD" > "$STATE/payload"

if ! wait_for_app_window "$VM" "$PID" 45 1; then
  # A windowless boot is a failed boot: a walk that carried on would spend its captures and
  # deadlines on an app already known not to draw. The guest's log is shown BEFORE the clone
  # is cleaned up, because the cleanup takes it with it.
  log "the app is running (pid $PID) but reports ${WINDOWS:-0} windows after 45s - refusing."
  log "guest log tail ($LOGFILE):"
  guest_ssh "$VM" "tail -n 20 '$LOGFILE' 2>/dev/null || true" 2>&1 | sed 's/^/[testvm]   /' >&2 || true
  clean_up_this_runs_clone
  exit 1
fi

ELAPSED=$(( $(date +%s) - START ))

log "ready in ${ELAPSED}s"
echo "vm=$VM ip=$IP pid=$PID ssh=$TESTVM_GUEST_USER@$IP windows=${WINDOWS:-0} elapsed=${ELAPSED}s"
echo "tailnet=$TAILNET_NAME"
# Printed whether they matched or not, and printed at EVERY run: a number
# nobody prints is a number nobody checks, and these two silently drifted apart
# for as long as the harness existed.
echo "$CLAUDE_LINE"
echo "$CLAUDE_LOGIN_LINE"
if [ "$TAILNET_NAME" != "not-joined" ]; then
  echo "phone: https://$TAILNET_NAME:8443/  (from this Mac: curl -sk https://$TAILNET_NAME:8443/)"
fi
echo "shot:  $HERE/shot.sh $VM <out.png> [--ocr]"
echo "tree:  $HERE/ax.sh $VM tree            # the screen as text, with real geometry"
echo "click: $HERE/ax.sh $VM click --title '<button>'"
echo "guest: $HERE/guest.sh $VM '<shell command>'   # --pull/--push to move a file"
echo "ax:    $HERE/ax.sh $VM '<applescript>'"
echo "stop:  $HERE/stop.sh $VM      # ALWAYS, before you report (CEO §54)"
