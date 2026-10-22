# The test VM — on-screen proofs that never touch the CEO's screen

For bounded AX searches, cached OCR, held reservations and reusable scenarios, see
[the walk harness guide](walk-harness.md).

**What it is:** a macOS virtual machine on this same Mac that acts as the host for
anything that has to put the RichOS app on a screen. The app opens on the guest's
virtual display; the host renders no window at all. Two guests can run at once, so two
agents can each have a window instead of queueing behind one another — and behind the
CEO's working day.

**Why it exists,** in his words on 2026-09-19:

> "So, every engineer will keep opening the app making me unable to do anything here or WHAT???"

> "WHEN THE FUCK WILL THERE BE A PROPER FUCKING SETUP THAT WILL ENABLE SUPER FAST
> DEVELOPMENT AND NOT PATHETICALLY SLOW SNAIL PACE DEVELOPMENT???"

After this, no test opens a window on his Mac.

---

## The commands

```sh
cd <repo>/richos/app/scripts/testvm

./run-walk.py [--wait S] --bundle <zip> --home <dir> --engine <tar> --report <json> -- <script> [args]
./slots.py   status | run [--wait S] -- <command> | check
./run.sh     --bundle <RichOS.app.zip> --home <fixture-home-dir> [--vm <name>] [--no-tailnet]   # inside a slot only
./shot.sh    <vm> <out.png> [--ocr]
./guest.sh   <vm> '<shell command>' | <cmd> <arg>... | --pull <guest> <host> | --push <host> <guest>
./ax.sh      <vm> tree  [--app <name>] [--depth N] [--window N] [--max N] [--json]
./ax.sh      <vm> find  --title <text> [--role R] [--value V] [--contains] [--json]
./ax.sh      <vm> click --title <text> [--role R] [--nth N] [--contains]
./ax.sh      <vm> click --at <x>,<y>
./ax.sh      <vm> '<applescript>' | --focused | --windows | --key <code>
./hand-file.sh <vm> paste <guest-file> | drag <guest-file> --to <x>,<y>
./stop.sh    <vm>
./tailnet.sh join|name|logout|nodes|doctor [<vm>]
./keychain.sh prepare|check <vm> <guest-home-path>
./claude-sync.sh  [--check] <vm>
./claude-login.sh host-check | push|check <vm> <guest-home-path>
./test/run-tests.sh
```

`run.sh` prints lines the caller can parse — this one is real output from the first
end-to-end run, on candidate `.5`:

```
vm=richos-test-1 ip=192.168.64.4 pid=717 ssh=admin@192.168.64.4 windows=1 elapsed=100s
tailnet=richos-test-1.tail1a2b3c.ts.net
claude: host 2.1.277 guest 2.1.277
claude login: guest logged in
```

The last two lines are printed at **every** run, matched or not — see *The claude binary and
the claude login*. Real output from `zach-claude1`, 2026-09-20.

The `tailnet=` line is the guest's own name on the tailnet and it is what makes the
**phone path** testable in here — see *The phone path in a VM*. **Since 2026-09-20 there
is no `tailnet=not-joined` on a successful run**: a join that was asked for and did not
happen stops `run.sh` with a `REFUSED` block naming the cause, because two nightlies
were walked against guests that could not serve a pairing code. `--no-tailnet` is how to
say the proof is not about the phone.

**Measured, 2026-09-19.** First VM: `run.sh` to a window on screen in **100 s**
(guest boot 11 s, bundle copy 7 s, 1.8 GB fixture home 58 s, launch 3 s). Second VM
started while the first was running: **151 s** — slower only because the two were
copying payloads at once. A capture takes **~8 s**, an accessibility read **~3 s**,
and `stop.sh` **~8 s**.

Two at once is two runs, each in its own guest slot:

```sh
./run-walk.py --wait 900 --bundle b.zip --home h --engine e.tar.gz --report a.json -- ./walk-a.sh &
./run-walk.py --wait 900 --bundle b.zip --home h --engine e.tar.gz --report b.json -- ./walk-b.sh &
```

## Guest slots: two, each held only while a run executes

The CEO, 2026-09-27: *"do I want the work to be BLOCKED AND PISSED AWAY like this because the
current worker might need the VM for a 5-second-long fart? Do I want HOURS OF DEVELOPMENT TIME
TO BE PISSED AWAY FOR EVERY 5-SECOND FART???"*

* There are **two slots** (`slots.py`): `~/.richos-testvm/guest.lock` and `guest-2.lock`.
  This Mac has 10 cores and 24 GB, a guest takes 4 cores and 5 GB and Apple's
  Virtualization framework runs at most two macOS guests at once. `TESTVM_SLOTS=1` lowers
  it; nothing raises it.
* **A slot is held only while one run executes**: boot, the run's steps, cleanup. It is
  released the instant the run ends. `run-walk.py`, `run-probes.py` and
  `gui-proof-in-vm.sh` each hold one for exactly their own run.
* **No reservations.** Nothing keeps a guest or a slot for later, across a job, a build or
  a debugging session. `hold-walk.py` is retired and `run.sh` refuses to boot a guest unless
  one of its own callers holds a slot. Steps found by hand become a script that takes the
  VM name as `$1` (`ax.sh "$1" tree`, `shot.sh "$1" out.png`, ...), run once under
  `run-walk.py`; the next attempt takes a free slot again. A walk that needs one guest for
  a long observation is one run and holds a slot for that run's length only.
* **A caller waits only while both slots are executing someone's run.** `--wait SECONDS`
  waits for a free slot (checked every 2 s) and for the CPU and memory admission (CEO ruling
  §77, one sample at least every 30 s; a refused sample releases the slot before waiting).
  The default, 0, refuses at once with exit 75.
* **A second guest must fit in memory, not just in a slot.** A guest takes its RAM as it
  boots, so a sample taken at admission cannot see it. Measured 2026-09-27 18:56Z: two guests
  admitted 1.5 s apart both passed the CPU and memory rule at 71% available; 40 s later the Mac
  was at 32% available, memory pressure warn, swapping out 120 MB/s. So a guest is admitted
  only when available memory, less what each running guest may still take (its RAM less its VM
  process's resident size) and less the new guest's whole RAM, leaves 4 GB. On this 24 GB Mac
  with its usual load that often means one guest at a time; the refusal says so and the caller
  waits (`--wait`) on that named reason, never on an idle slot.
* A slot released with its guest still recorded against it stops that guest first
  (`stop.sh`): a guest never outlives its slot.
* `./slots.py status` shows who holds which slot and for how long.

**`stop.sh` is not optional.** A test instance of the app is garbage and by CEO §54
garbage is always cleaned up. `stop.sh` quits the app, verifies `pgrep` is empty, stops
the VM, deletes the clone and removes the run state. If any of that fails it prints a
massive alert naming exactly what to delete by hand.

---

## Reading the guest's screen and driving it

**Added 2026-09-20, because every walk was writing these two by hand.** The CEO's
question that day: *"How many times does Ray build the same scripts or checks from
scratch (for the same type of job)? And how much time does that waste in every one of his
runs?"* For the guest, the answer was about fourteen accessibility scripts and eleven ssh
wrappers across three walks.

### `guest.sh` — one word into the guest

```sh
./guest.sh <vm> 'ls -la /Users/admin/testvm | wc -l'   # one string: the guest's shell runs it
./guest.sh <vm> ls -la '/a b'                          # several: each quoted, none re-split
./guest.sh <vm> --pull /Users/admin/testvm/<vm>/app.log ./app.log
./guest.sh <vm> --push ./fixture.json /Users/admin/fixture.json
printf '%s' "$SECRET" | ./guest.sh <vm> 'cat > /tmp/x' # stdin is the only place a secret goes
```

The exit code is the **guest command's**. `lib.sh` has had `guest_ssh` since the harness
was written, but it is a shell function in a file that is sourced and never run, so
nothing on a command line could call it — which is why eleven wrappers existed, each with
its own copy of the same four ssh options.

**One argument is a shell command; several are arguments.** `ssh` joins its arguments
with spaces and lets the guest re-split them, so `ssh host ls '/a b'` runs `ls` with two
arguments and the error blames the guest. One argument is passed through untouched
(pipes and redirections are the guest's, which is what `'cat > /tmp/x'` needs); several
are quoted for the guest's shell. `--` forces the literal reading for a single argument.

### `ax.sh tree` / `find` / `click` — the screen as text

```sh
./ax.sh <vm> tree                       # the app under test, every window, indented
./ax.sh <vm> tree --app Safari          # the phone half of a walk is Safari in the guest
./ax.sh <vm> find --title 'Set my phone up'
./ax.sh <vm> click --title Settings     # AXPress, by title OR description
./ax.sh <vm> click --at 974,43          # the fallback, and it refuses unless frontmost
```

A line of `tree` carries role, subrole, title, description, value, enabled and the box
as **four separate numbers**:

```
AXWindow/AXStandardWindow title='RichOS' ... pos=0 25 size=1024 700
  AXGroup title='' ... pos=0 25 size=1024 700
    AXButton title='Settings' desc='' value='' enabled=yes pos=974 43 size=32 32
```

**The four numbers are never text.** AppleScript renders a list as a string by
*concatenating* it — measured on this Mac, 2026-09-20:

```
$ osascript -e 'set sz to {1024, 700}' -e 'return (sz as string)'
1024700
```

A 1024x700 window and a 102x4700 one are the same six characters and the app derives a
**1024x700** window in this guest, so those are not hypothetical neighbors. `ax.py` (run
by `python3` in the guest) reads `AXPosition` and `AXSize` as `AXValue`s and unpacks them
into a `CGPoint` and a `CGSize`, so the numbers are never adjacent in the first place. The
renderer **refuses** a fused token anyway and exits 4: that failure is silent otherwise,
because `1024700` looks like a measurement and would be quoted in an audit.

**The guest reads the tree through the accessibility API, not System Events.** Until
2026-10-21 the reader was `ax.js` under `osascript -l JavaScript` and every attribute was
an Apple Event to System Events, which re-resolves the element from the process root each
time: 7-26 ms per attribute shallow and 15-120 ms at depth 7 in a busy guest, so one read
in most first-run walks crossed the 20 s deadline while the app sat idle in its run loop
(esc-20261021T122950Z-b8b7362a, esc-20261021T133408Z-5d70a3e4). `ax.py` calls
`AXUIElementCopyAttributeValue` on the element directly: 0.29-0.41 ms per attribute, the
whole 98-node tree in 115-161 ms in the same guests. Keystrokes (`type`'s paste) and
`click --at` still go through System Events, one Apple Event each. The guest needs no
new grant: `python3` over SSH reads AX under the same Accessibility grant `osascript` used.

**Match on the title OR the description and `click` presses the element.** An
`aria-label` on a web view's control surfaces as `AXDescription`, not `AXTitle` — the
walk that learned this wrote `axpress.js` (title), `axpress2.js` (title plus role, still
`NOTFOUND` on buttons plainly on the screen) and `axpress3.js` (description) before it
worked. `--contains` switches from an exact match to a substring; `--nth N` picks among
several; `--role`, `--subrole` and `--value` narrow further. A press goes to the
*element*, so it survives a moved window, a window that is not frontmost and anything
drawn over it. `--at x,y` exists for controls that expose no `AXPress` and it carries
the same frontmost refusal `--key` has had since a synthetic key landed in the CEO's
Terminal on 2026-09-19.

Everything takes `--json` and prints the guest's own one-object-per-line stream, which is
what to pipe into `python3 -c` when a walk needs to compute rather than read. A walk that
hits the node cap says so rather than handing back a tree that merely looks complete.

### Measured, 2026-09-20, on `v1.2.0-nightly.20260920.1`

One guest (`vmtools2`, `--no-tailnet`), ready in **111 s**, then:

```
$ ./guest.sh vmtools2 uname -a
Darwin Manageds-Virtual-Machine.local 24.6.0 ... RELEASE_ARM64_VMAPPLE arm64

$ ./ax.sh vmtools2 tree                                    # 3.6 s, 29 nodes
# app=richos-tauri pid=890 windows=1 mode=tree
AXWindow/AXStandardWindow title='RichOS' ... pos=438 92 size=1024 700
  ...
          AXGroup/AXApplicationDialog title="There's one thing I need on this Mac." ...
            AXButton title='Set it up' ... pos=757 443 size=88 35
            AXButton title='Not now'   ... pos=852 443 size=90 35

$ ./ax.sh vmtools2 click --title 'Not now'
pressed AXButton title='Not now' desc='' pos=852 443 size=90 35 (matches=1)
```

and the next `tree` showed the app had moved on to *"Where should I keep what you tell
me?"* — the press drove the app, rather than merely reporting that it had found a button.
`tree --app Dock` answered `windows=0 nodes=0` (the Dock has no `AXWindow` and saying so
is not an error); `tree --app NoSuchApp` refused with `noprocess`; `guest.sh vmtools2
'exit 42'` returned **42**; `--pull` brought the guest's `app.log` back. `stop.sh` then
reported `clean: app quit, VM stopped, clone deleted, state removed` and `tart list`
shows nothing but the base.

The window's own geometry is the reason for all of this: **`size=1024 700`**, which
AppleScript would have handed over as `1024700`.

### `hand-file.sh` — a file, handed over the way a person does it

```sh
./guest.sh <vm> --push './Screenshot invoice 4471.png' /Users/admin/walk/
./hand-file.sh <vm> paste '/Users/admin/walk/Screenshot invoice 4471.png'
./hand-file.sh <vm> drag  /Users/admin/walk/board-memo.pdf --to 950,780
```

**Added 2026-09-24**, when the Mac composer started taking files (CEO §86). `ax.sh type`
hands the app text; nothing handed it a file.

* `paste` puts the file on the guest's pasteboard the way macOS does: a `.png` as image
  data (what a screenshot to the clipboard is), anything else as a file reference (what
  Finder's Copy is). Then Command-V, to the app under test by PID and the text that was on
  the pasteboard is put back.
* `drag` places the file on the guest user's Desktop and moves it with real mouse events
  from its Finder icon to `--to x,y` (screen coordinates; take them from
  `ax.sh find --json`). The app receives an ordinary Finder drag; nothing in it is bypassed.

Both refuse unless the app's PID is frontmost, for the reason `ax.sh --key` does. The
program travels on stdin, so a guest path with a space in it never becomes part of a
command line and it runs in the guest under the same guest-side deadline `ax.sh` uses.

**Two things the first live runs taught it, 2026-09-24:**

* **It never scripts Finder.** The icon's position comes from Finder's accessibility tree,
  read through System Events, which the guest is granted. An Apple Event to Finder raises a
  *"wants access to control Finder"* consent dialog in the guest that nobody can answer.
* **The guest deadline is not optional.** Without it, a call that timed out on the host
  kept waiting in the guest and finished its drag minutes later, which put the same file
  on the composer twice.

Tests: `test/run-tests.sh hand-file` (7, against the stub guest). Each refusal, the guest
deadline and the no-Finder rule were proven by running the test against code without them.

---

## Does the Mac have to be left unlocked? No.

**Lock the screen, let the display sleep, walk away — the proofs keep running.** The
completion proof for this harness was deliberately captured with the host display
**asleep** and the frame came back normally.

The reason is structural rather than lucky: the guest runs **its own WindowServer inside
the VM**. The host's screen — lit, dark, locked or showing something else entirely — is
not connected to the guest's display in any way.

The one thing that *would* interrupt a run is the **whole Mac** going to sleep, because
that suspends the VM process along with everything else. So every VM is started under
`caffeinate -is`:

* `-i` holds off idle **system** sleep, `-s` does the same on AC power.
* **`-d` and `-u` are deliberately NOT used.** They prevent *display* sleep and declare
  the user active, which would hold the screen lit and unlocked for as long as any test
  ran — the exact intrusion this harness exists to remove.

So: **locked is fine, dark is fine, sleeping the whole Mac pauses everything.** The
assertion dies with the VM process, so nothing is left holding the machine awake after
`stop.sh`.

---

## One-time setup

```sh
./setup.sh
```

Installs Tart, pulls the guest image, provisions it and verifies that a capture comes
back non-black before it claims success. Re-running it is safe; `./setup.sh
--reprovision` re-runs only the guest provisioning.

**Measured cost, 2026-09-19:** a **25.35 GB** compressed download (read from the registry
manifest, not estimated) taking **~65 minutes** on this connection, expanding to **62 GB
on disk** — the brief's unverified "~50 GB" was low for disk and high for the download.
Everything after the download took **2 minutes 22 seconds**: guest boot 9 s, ssh key
13 s, TCC grants 2 s, Homebrew tooling 46 s, Tailscale 11 s, reboot 25 s, capture
verification 25 s.

Disk left afterwards on this Mac: 175 GiB of 460 GiB.

### What still needs a human, once

| # | Action | Why it cannot be automated |
|---|---|---|
| 1 | **Save one Tailscale auth key.** Five minutes, once and every future guest joins the tailnet by itself — full steps in *The phone path in a VM* below. | It needs the CEO's Tailscale account. No agent may hold or borrow those credentials. |

That is the whole list. It used to have a second row — *"`claude` sign-in in the guest,
only if model turns are wanted there: `ssh admin@<guest-ip> 'claude /login'`"* — and that
row is **gone since 2026-09-20**: the login is copied from this Mac at every run, like
the binary. See *The claude binary and the claude login* below.

Action 1 replaces what used to be here — *"`tailscale up` prints a URL; open it once"*.
A URL sign-in authorizes **one** guest and a guest is deleted at the end of every run,
so that was a human errand per proof. An auth key is the same five minutes spent once.
The `claude /login` row went the same way and for the same reason.

Nothing else needs a person. In particular **no host permission is required** — the
harness grants Screen Recording and Accessibility *inside the guest*, never on the host,
and the host never renders a VM window.

---

## The phone path in a VM

**The problem this solves.** The app refuses to hand out a pairing code unless it can
actually serve one: `begin_pairing` returns `PhoneError::TailnetNotReady` when
`serving_plan` is `None` (`src-tauri/src/phone/mod.rs:858`) and `serving_plan` (`:563`)
needs three things — a tailnet name, an origin derived from it and a certificate
`tailscale cert` will issue. CEO §61 deleted the LAN fallback deliberately: *"any mobile
app or PWA is utterly useless within the home network"*. So there is one path, a
signed-out guest is not on it and until this existed every phone proof was a window on
his Mac.

### His one action

Once. Nothing else about the phone path needs a person again.

1. Open <https://login.tailscale.com/admin/settings/keys> → **Generate auth key**.
2. Set it up like this and the three switches all matter:
   * **Reusable — ON.** Every run is a new guest. A single-use key works once and then
     every later run refuses.
   * **Ephemeral — ON.** The node removes itself when it stops talking to the control
     plane, so a guest that dies without cleanup does not leave a node behind (CEO §54).
   * **Pre-approved — ON.** Otherwise each new guest waits in the admin console for a
     human to approve it, which is the errand being removed.
   * **Expiry:** 90 days is the usual maximum. Put a reminder somewhere; an expired key
     refuses with the exact sentence that says so.
   * **Tags:** none needed. Add them only if the tailnet's ACLs require it.
3. Save the key — and nothing else, no quotes, no trailing note — into the file:

   ```sh
   install -m 600 /dev/null ~/.richos-testvm/tailscale.authkey
   printf '%s' 'tskey-auth-...' > ~/.richos-testvm/tailscale.authkey
   ```

   The file lives **outside every repository**, mode `0600`. The harness reads it, never
   prints it, never logs it and never puts it in a command line.

4. One switch, tailnet-wide, if it is not on already:
   <https://login.tailscale.com/admin/dns> → **HTTPS Certificates: Enable**. Without it
   the control plane will not certify any node and the app shows "not set up yet" with
   no code. Measured on this Mac 2026-09-19: it is **already on** — `tailscale status
   --json` reports `CertDomains: ["mm1.tail1a2b3c.ts.net"]`.

### What `run.sh` does with it

Nothing, until there is a guest — then, before the app launches:

```
tailnet.sh join <vm>   →   tailnet=richos-test-a.tail1a2b3c.ts.net
```

* The key is staged **as a file inside the guest** and passed as `--auth-key file:…`.
  Read off `tailscale up --help` in the guest (1.102.4) rather than assumed: *"if it
  begins with `file:`, then it's a path to a file containing the authkey"*. A key given
  as a command argument is readable by every process in the guest and lands in every log
  that records a command line. It is deleted the moment `up` returns, either way.
* `--hostname richos-test-<vm>`, so two guests are two named nodes instead of two
  machines both calling themselves `Manageds-Virtual-Machine` (the image's default,
  measured) and colliding into `…-1` names nothing can predict.
* `--operator <guest user>`, because **the app runs as the console user, not as root**,
  and `tailscale cert` is a mutating call. Without this the app's own certificate fetch
  is a permission error it renders as "not set up yet".
* Then the name is **read back** from `tailscale status --json` — never constructed. A
  name still held by a node that has not aged out becomes `<name>-1` and every URL built
  from the requested name would point at somebody else's machine. `run.sh` prints what
  the daemon said.
* Finally `tailscale cert` is warmed **as the user the app runs as, with the app's own
  arguments**, so a refusal appears here, with a sentence, rather than inside a pairing
  sheet.

**There is no certificate file to install.** `fetch_cert` (`phone/tailnet.rs:722`) runs
`tailscale cert --cert-file - --key-file -` and parses the PEM blocks off **stdout** —
deliberately, so the private key never touches a disk. What the guest needs is a
`tailscale` binary at one of the four paths the app looks at (`phone/tailnet.rs:82`) and
a daemon that answers the app's user. Measured in a guest: the Homebrew install is
`/opt/homebrew/bin/tailscale`, which is candidate 3 and a symlink `is_file()` follows.

### It waits for `tailscaled` and a join that fails STOPS the run

**Both halves are fixes for the same defect, raised by Ray on `.7`, again on `.8`
(defect 3) and reproduced by Echo on `.9`.** `run.sh` joined as soon as ssh answered —
about nine to eleven seconds after the guest reported an IP — and Homebrew's launch
daemon was not up yet:

```
failed to connect to local tailscaled … /var/run/tailscaled.socket: no such file or directory
tailnet=not-joined
```

The run then carried on. *"A tester who misses that line walks the whole path against a
Mac that cannot serve."*

* **The wait is on a fact, not a clock.** `tailnet.sh join` polls `tailscale status
  --json` until the daemon produces a `BackendState` — which is exactly the capability
  `tailscale up` needs one line later. Bounded by `TESTVM_DAEMON_POLLS` ×
  `TESTVM_DAEMON_POLL_SECONDS` (60 × 1 s). Measured in a real guest 2026-09-20: the
  daemon answered on the **second** read, so the race is live and this is what catches
  it. A fixed sleep could not: Ray's guest was ready 4 s after the failed join, Echo's
  took 37 s.
* **A failed join is now a refusal.** `run.sh` prints a `REFUSED` block naming the cause
  in one line, the guest's own `tailnet.sh doctor` output taken while there is still a
  guest to ask and then **exits 1** and deletes the clone it created. A guest that was
  already running when you called `run.sh` is left alone and named.

If the proof has nothing to do with the phone, say so and it costs nothing:

```sh
./run.sh --bundle <bundle> --home <home> --vm <name> --no-tailnet
```

### The fixture's login keychain

**Ray, `.7` defect 4 and `.8` defect 5.** The fixture home's `Library/Keychains` used to
be a symlink to the CEO's real keychain directory. That was removed and what replaced it
is an **empty directory** — which is not a keychain. `Set my phone up` then raises
`Keychain Not Found` and `.8` defect 2 measured **120 seconds of "Getting this Mac
ready…"** with that dialog sitting behind the sheet.

`run.sh` step 3b now makes one, in the guest, in **this run's** fixture home, after the
home is copied in and before the app launches:

```sh
./keychain.sh prepare <vm> /Users/admin/testvm/<vm>/home    # run.sh does this for you
./keychain.sh check   <vm> /Users/admin/testvm/<vm>/home    # if you want to ask again
```

**The unlock has to happen in the app's own session and that is measured rather than
assumed.** Keychain lock state is held by `securityd` and an ssh login is its own
security session — not the Aqua session `open -n -a` launches the app into. Three fixture
homes in one guest, 2026-09-20 (`zach-kc3`):

| what was done | what the app's session sees |
|---|---|
| nothing — the fixture as Ray met it | `UNUSABLE (no keychain at that path)` |
| `create-keychain` + unlock **over ssh** — the documented workaround, applied literally | `UNUSABLE (no answer within 20s)` — the file is there and is the default and the app still gets the password dialog |
| the same, plus an unlock through `launchctl asuser` | **`usable`** |

So the middle row is Ray's *"it then prompted for its password twice more, once per
pairing"*. `keychain.sh` does both unlocks and reports `gui-session=` as the verdict; the
`ssh-session=` line beside it is a different securityd session and is frequently
`UNUSABLE` on a perfectly healthy guest.

**Every `security` call in that script is bounded by a `perl` alarm and that is not
caution.** Against a home with no keychain, `security` does not return an error — it
raises `SecurityAgent`'s dialog inside the guest and waits for a click nothing can give
it. The first version of `check` hung for over ten minutes on exactly that case. macOS
ships no `timeout(1)`. The missing-keychain case is now answered from `test -f`, without
calling `security` at all.

It is never the host's keychain: `prepare` refuses any path that is not under
`/Users/<guest user>/`, rather than trying to normalize one.

### Reaching it

```sh
curl -sk https://richos-test-a.tail1a2b3c.ts.net:8443/     # from this Mac
```

Inside the guest, the pairing URL opens in Safari and resolves through MagicDNS — which
is why `--accept-dns=true` is part of the join rather than inherited from a default.

### The certificate cache and why one exists

Every clone is a fresh node with an empty certificate store, so every run would ask the
control plane to issue a **new** certificate for the same name. Public CAs rate-limit
exactly that — the duplicate-certificate limit for one name is single digits per week —
and the failure would land days later, on somebody else's run, looking like a broken
harness.

So a certificate that was issued is kept on the host under `~/.richos-testvm/certs/<dns
name>/`, mode `0600`, and offered back to the next clone between `up` and the first
`cert` call: the only window where the name is known and nothing has been issued yet. It
is re-used only if openssl says it is for that exact name and outlives the app's own
`TAILNET_MIN_VALIDITY` of 720h. `reap.sh` deletes entries that fail either test — a
private key with no use is garbage with a key in it.

That is a private key at rest, taken deliberately: it belongs to a throwaway node
reachable only from inside the CEO's own tailnet and it buys back the ability to run the
phone proof more than a handful of times a week.

### Cleaning up — §54 at the tailnet

`stop.sh` signs the node out **before** the guest is destroyed, then checks that it is
signed out. If the guest dies without ever reaching `stop.sh`, the node is ephemeral and
disappears on the control plane's own clock; `reap.sh` reports any `richos-test-*` node
the tailnet still lists, because that is the one piece of garbage nothing here can delete
(removing a node needs an API key with tailnet-admin rights, which no agent holds).

```sh
./tailnet.sh nodes            # what the tailnet still shows
./tailnet.sh doctor <vm>      # every reason the phone path is or is not available
```

### What is verified and what is not

| Claim | State |
|---|---|
| Refusal with no key: one action named, the guest untouched, the VM still usable | **Verified** on a live guest, 2026-09-19. A full `run.sh` with the join refusing reached a window in **43 s** (`windows=1`, capture 64.2% non-black) and printed `tailnet=not-joined` |
| Two clones at once, each refusing on its own, neither disturbing the other | **Verified** 2026-09-19 — `richos-test-a` ready in **72 s**, `richos-test-b` in **66 s**, both with a window, both stopped clean |
| `tailnet.sh doctor` against a live guest | **Verified** — reports `NeedsLogin` and names the CLI it found (`/opt/homebrew/bin/tailscale -> ../Cellar/tailscale/1.102.4/…`) |
| Every flag, the key-by-file handling, the name read-back, the refusal classifications | **Verified** by `test/run-tests.sh` (111 tests) against a stub guest |
| The certificate cache's accept/refuse rules | **Verified** with openssl against generated certificates |
| A real tailnet accepting a real key; two guests as two **nodes**; the pairing sheet in a VM | **NOT RUN** — needs the CEO's key, which no agent can produce. `tailnet.sh doctor <vm>` settles the first two in one command once the key is in place; the third is a QA walk |

---

## Tests

```sh
./test/run-tests.sh            # ~1 second, no VM booted
./test/run-tests.sh 'cert'     # only the tests whose names match
```

**111 tests** (79 before the 2026-09-20 screen-and-shell additions), against a **stub
guest**: `tailnet.sh`, `keychain.sh`, `claude-sync.sh` and `ax.sh` each reach their VM
through one indirection, so pointing that at a script exercises every decision without
booting anything. `guest.sh` is driven against a fake `ssh` and `scp` first on `PATH`,
because what matters there is the call it BUILDS. They assert the things a screenshot
cannot show — that the key never appears in a command line or in output, that a missing
key refuses exactly one step without touching the guest, that the name is read back from
the daemon, that the join waits for the daemon and does not merely sleep, that no path
`keychain.sh` touches is ever outside the guest, that `stop.sh`'s sign-out actually signs
out, that a fused geometry token is refused rather than printed and that a path with a
space in it survives the trip into the guest as one path.

They found four real defects on their first run, including a certificate cache that
could never hit (written under the daemon's FQDN, read under the short name) whose only
symptom would have been a rate-limit refusal weeks later on somebody else's run. The
2026-09-20 additions found two more **in the changes being made**: an unbounded
`security` call that would have hung and a `VAR=$(cmd)` under `set -e` that would have
killed `run.sh` silently at exactly the failure it was being taught to shout about.

**A new test here is checked against the UNFIXED code before it is trusted.** The
2026-09-20 batch was run with the new `test/` against pristine `049d8790`'s scripts:
42 passed, 7 failed and the seven were the seven that were new. A new test that passes
against unfixed code is testing nothing.

**The screen-and-shell cases were proved the same way, by defeat.** With the fused-token
refusal deleted from `ax.sh`'s renderer, *"a FUSED geometry token is refused"* fails; with
`printf '%q'` removed from `guest.sh`, *"a path with a space stays one path"* fails
(`expected [ls -la /a\ b] got [ls -la /a b]`). One of them caught itself first: the
`node --check` case passed for the wrong reason until the captured program was given a
`.js` name, because node refuses an unknown extension before it looks at any syntax.

---

## What works in the guest and what does not

All verified on 2026-09-19 against candidate `.5` unless marked otherwise.

| Capability | Status |
|---|---|
| App launches, window appears | **Yes** — `open -n -a` routes the launch through launchd into the auto-logged-in user's GUI session. `windows=1` reported and the frame shows it |
| `screencapture` over ssh | **Yes** — 1680x1050, 64.3% non-black, once the TCC grant is written |
| System Events AX reads | **Yes** — `window: RichOS / role: AXTextArea / title: Message to Rich`; geometry `RichOS 1024x700 at (438,92)` |
| `tesseract` OCR | **Yes**, in-guest via `shot.sh --ocr` — it read the offer sheet's body text back. Note it cannot read from the guest's `/tmp`; everything is staged under `$HOME` |
| Home screen + engine-offer sheet | **Yes** — rendered identically to the host, including the fixture's thread history |
| Two VMs at once | **Yes** — both booted, both rendered, both captured, 32% host memory still free |
| Works with the host screen **locked/asleep** | **Yes** — the completion proof was captured with the host display asleep. The guest runs its own WindowServer; the host's screen state is irrelevant to it |
| WebGL splash / Metal rendering | Untested in this pass — the fixture home boots straight past the splash into the home screen. See "Graphics" |
| **Model turns (`claude`)** | **Yes, since 2026-09-20, measured** — `claude -p 'Reply with the single word READY…'` in the guest answered **`READY`** (rc 0) from a login copied in at the start of that run. It was *"no, not without a one-time human sign-in"* until then; see *The claude binary and the claude login* |
| Audio | Out of scope. CEO §53 stands: the Mac's speakers cannot stand in for a person and a VM's virtual audio device does not change that. Do not test barge-in here. |
| Tailscale as its own node | **Yes, with the auth key** — the guest joins as `richos-test-<vm>`, which is what makes the phone path reachable in here. Without the key it stays signed out and only the phone path is unavailable. See *The phone path in a VM* |

**One observation worth passing on, not a harness fault:** the app derived a
**1024x700** window in the guest — its stated minimum — on a 1680x1050 screen, where on
the host it derives 1400x880. If window derivation is what you are testing, test it on
the host or treat the guest's smaller screen as the input it is.

### Model turns — the honest answer

**CHANGED 2026-09-20.** What this section used to say was:

> With a scratch `HOME`, `claude` answers *"Not logged in · Please run /login"*. The thing
> `HOME` takes away is the **login keychain** — macOS Security resolves it through the
> home directory … A guest has no such keychain to symlink. **So: no model turns in the
> guest until a human runs `claude /login` in it, once.**

The diagnosis was right and the conclusion was one step short. The missing piece was never
the *login* — it was a **login keychain to put one in** and `keychain.sh` now makes one in
the fixture home at every run. So the credential is copied into that keychain from this
Mac's own, by `claude-login.sh`, over ssh, with the value on stdin only.

That cost of it — *"no agent may copy or borrow the CEO's credentials"* — is answered by
where the copy goes and how long it lives, not by refusing to make it: a keychain inside a
disposable guest, destroyed with the clone at `stop.sh`, never written to a file, never
printed, never in a command line, never in a log. The full reasoning and the exact
mechanism are in *The claude binary and the claude login* above.

**What this cost before it was fixed:** Ray's `.8` walk of the phone path in the VM
(`docs/verification/2026-09-20-nightly-1.2.0-nightly.20260919.8-phone-path-in-the-vm-audit.md`)
could measure **phone → Mac** and not **Mac → phone**, because a desk message in the guest
was refused before it became a message at all. Half a round trip, twice in a row, for a
sign-in that is now part of the run.

Everything that needs no model turn — the home screen, the engine-offer sheet, the splash,
window geometry, focus, theme and layout — still renders without one, exactly as before.

### The claude binary and the claude login

**The CEO, 2026-09-20:** *"What happens with the Claude binary in the VM? Will it always
stay on the same version regardless of any updates?"* and *"What happens when the Claude
login expires in the VM?"*

Both used to have the same bad answer — *it is whatever was copied in once and nothing
ever looks at it again* — and both now have the same structural one: **the guest gets
this Mac's binary and this Mac's login at the start of every run and keeps neither
beyond it.**

#### The binary: compared at every run and the two versions printed

`setup.sh` copied `~/.local/bin/claude` into the base image once, when the image was
built. `run.sh` never looked at it again. Meanwhile Claude Code updates itself: **2.1.274 and
2.1.275 on the 17th, 2.1.276 and 2.1.277 on the 18th** — four versions in two days, read
from the mtimes of `~/.local/share/claude/versions/*`. So the VM drifted a little further
from his Mac every day and nothing in the output ever said so.

`claude-sync.sh`, called by `run.sh` before the app starts:

```
claude: host 2.1.277 guest 2.1.277
```

* **sha256 of the RESOLVED host binary**, never of the path. `~/.local/bin/claude` is a
  **symlink** into `~/.local/share/claude/versions/<version>` — 48 bytes of text whose
  target changes at every update. Hashing the link would compare equal forever.
* **Copied only when the bytes differ**, to a side path and then moved into place, so a
  half-written 217 MB binary is never sitting where the app launches from.
* **Re-measured after the copy.** `scp` returning 0 and the file being right are two
  different claims.
* **A run that still disagrees is REFUSED** and the clone it created is cleaned up (§54).
  A guest holding a different build is not the Mac he uses and a model turn measured in
  it would be measuring a version nobody ships.

Ask it without booting anything: `./claude-sync.sh --check <vm>`.

#### Why the auto-updater is pinned with an env var and not a setting

`claude` updates ITSELF, so a guest that started the run in sync could leave it behind
mid-walk. The pin is `DISABLE_AUTOUPDATER=1`, on the launch and in the guest's shell
environment — **not** the `autoUpdates: false` config key and that is measured rather
than preferred. The check, read out of the 2.1.277 binary, in its own order:

```js
if (DISABLE_UPDATES) …
if (DISABLE_AUTOUPDATER) …
if (CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC) …
if (config.autoUpdates === false &&
    (config.installMethod !== "native" || config.autoUpdatesProtectedForNative !== true)) …
```

The last line is the trap. The host's `claude` is a **native** install, so the copy in the
guest is one too and the native updater writes `installMethod:"native"` with
`autoUpdatesProtectedForNative:true` itself — from that moment `autoUpdates:false` is
ignored. A config pin would have **looked set and done nothing**, which is the worst shape
a pin can have.

#### The login: an access snapshot with no refresh capability

`claude-login.sh push`, called by `run.sh` after `keychain.sh prepare` and before the app
launches:

```
claude login: guest logged in
```

`claude-login.sh` now sends only `accessToken`, `expiresAt`, `scopes`,
`subscriptionType` and `rateLimitTier` from the selected host credential. It never
sends `refreshToken`, its expiry or other credential-store entries. Both guest
file stores and both keychain entries receive this same restricted snapshot.
Malformed or expired source credentials are refused before any guest write.

A single snapshot **can expire during a run**, so two things keep it current and
in both only this Mac ever renews this Mac's login (its own lock, its own store):

* **At push:** when the host's access token is within five minutes of expiry (Claude
  Code's own renewal window) or past it, `claude-login.sh` first asks this Mac's own
  `claude` to renew (a control-only `get_usage` request, no model turn), then hands over
  the fresh token. A host that cannot renew is refused as before.
* **During the run:** `run.sh` starts `claude-login.sh keep <vm> <guest-home>` after a
  successful push and records its pid in the run state (`claude-keep.pid`, log in
  `claude-keep.log`). Every 60 s it reads the host credential item's **modification
  time** (an attribute, never the value); when this Mac has renewed, it pushes the new
  access snapshot. When the guest's copy enters the renewal window and this Mac has not
  renewed (an idle Mac makes no model calls), it asks this Mac once. `stop.sh` ends the
  recorded pid; the keeper also ends by itself once the run state is gone and after
  12 hours.

This prevents a disposable VM from rotating the host login; it does not promise
unattended authentication forever (a login the provider revokes stays revoked). Do not
restore refresh-token copying. A stored item's presence or byte count proves storage
only, not successful authentication. The keeper and the renewal are proved against a
fake credential and a stub guest (`test/run-tests.sh "claude login"` and
`test/claude-login.mutation.py`), not against a live guest.

The following measurements describe the old full-copy implementation, not live
verification of the restricted snapshot:

**Measured, 2026-09-20** (`docs/verification/2026-09-20-claude-binary-and-login-in-the-test-vm.md`):
the credential file lands at **524 bytes in the guest, matching this Mac's credential byte for
byte in size** and `claude -p` in that guest answered **`READY`**. The size comparison is the
check that matters and it is not decoration — Ray's vm9 audit found that
`security add-generic-password -w` reading from a pipe with no tty stores an **empty** password
and **exits 0**, so a store's own success is not evidence that it holds anything.

**Where it goes: the file store first.** `claude` keeps `.credentials.json` in its config dir
(`CLAUDE_CONFIG_DIR` when set, `~/.claude` otherwise) and a file has no tty, no securityd and
no session to be on the wrong side of. Both paths are written — the fixture home's, which is
what the app is launched with and the guest user's own, which is what a `claude` started by
hand over ssh reads — from ONE stdin write copied inside the guest, under `umask 077`. The
keychain item is written too, by a different mechanism from the one that failed Ray (`security
-i` with `-X <hex>` on stdin, never `-w`), so either store alone is a login.

**The value is only ever on a pipe.** `security -i` reads its commands from **stdin**,
which is how Claude Code itself writes this item — from the same binary:

```js
i = `add-generic-password -U -a "${account}" -s "${service}" -X "${hex}"\n`
if (i.length <= 4032) security -i     // the command arrives on STDIN
else … argv …                          // their fallback; NOT taken here
```

Measured here on 2026-09-20: the credential is **524 bytes**, so the line is ~1.1 kB,
comfortably inside that limit. Past it this **refuses** rather than taking the argv
fallback — a secret on a command line is in the guest's process table and in every log
that records one, the same rule the Tailscale key already follows.

**Two service names are written, with one value.** `claude` builds the keychain service
name as `Claude Code` + `OAUTH_FILE_SUFFIX` + `-credentials` + a scope, where the scope is
empty when `CLAUDE_CONFIG_DIR` is unset and `-<sha256(configDir)[0:8]>` when it is set;
the account is `process.env.USER`. `run.sh` launches the app **with**
`CLAUDE_CONFIG_DIR=<fixture home>/.claude`, so the app resolves the scoped name while a
`claude` a tester starts by hand over ssh resolves the bare one. Writing two rows of a
throwaway keychain costs nothing; predicting which one the process under test will ask
for and being wrong, costs a whole walk. The account written is the **guest's** user,
never the host's — an item filed under `alex` would never be found by an app running as
`admin`.

The old implementation left cross-machine refresh rotation as an open question.
The current boundary removes that capability regardless of whether it caused a
particular outage. Do not deliberately rotate the production login to test it;
the harness tests inspect every guest payload using synthetic credentials.

### Graphics

The splash uses WebGL. Apple Virtualization gives the guest a paravirtualized GPU with
Metal, so WebGL resolves — but the guest is not the host's M4 GPU and a frame is not
guaranteed pixel-identical to `ui/tests/shots-home/home-named.png`. Treat the guest as
authoritative for **layout, text, focus and state** and confirm anything that depends on
exact GPU rasterization against a host capture.

---

## Where it all lives and how to delete it

Everything is under **one** directory:

```
~/.richos-testvm/
  bin/tart.app        the pinned Tart binary
  tart/               TART_HOME — the base image and every clone
  run/<vm>/           per-run state: ip, app.pid, vm.pid, payload path
  log/                VM console logs
  MANIFEST            what was installed, when, from which digest
  IMAGE_DIGEST        the digest actually pulled
  tailscale.authkey   the CEO's auth key, 0600 — his to create, once
  certs/<dnsname>/    a certificate already issued for a test node, 0600
```

Two of those hold secrets and neither is in any repository. `rm -rf ~/.richos-testvm`
takes them with everything else.

Disk: **62 GB** for the base image. Clones are **copy-on-write** on APFS — cloning the
base took **1 second** and near-zero disk until the guest diverges, which is what makes a
disposable per-run VM practical instead of a 62 GB copy each time.

**Delete everything:** `rm -rf ~/.richos-testvm` — and that is the whole story; nothing is
installed outside it. No Homebrew formula, no launch agent, no login item, no host TCC
change.

### Garbage and CEO §54

The engine's `scratch-reaper.sh` does **not** cover this directory, deliberately: its
first wall is containment in a declared scratch root and the 25 GB base image must
survive every sweep. So the harness cleans up after itself:

* `stop.sh` is the normal path and verifies rather than assumes.
* `reap.sh` is the backstop for a run killed before it reached `stop.sh` — a quota pause,
  a crash, a terminated session. It deletes nothing by default:

```sh
./reap.sh            # the plan
./reap.sh --apply    # do it
./reap.sh --notice   # one line, only if there is garbage
```

It will never delete the base image, a VM it did not create or a **running** VM.

---

## How it works, for the next person who has to fix it

### Nothing is executed before it is proven launchable

`preflight-binary.py` checks a binary's code signature, its Gatekeeper status and —
the part that matters — that every **non-weak** dynamic dependency actually resolves,
either on disk through the binary's own `LC_RPATH` entries or in the dyld shared cache
(tested by asking the loader, in a throwaway python process).

This exists because a bare `tart --version` on 2026-09-19 put *"tart cannot be opened
because of a problem"* on the CEO's screen. Tart 2.36.0 and 2.37.0 hard-link
`@rpath/libswiftCompatibilitySpan.dylib`, a Swift 6.2 runtime that ships only with
macOS 26 / Xcode 26; this host is macOS 15.6, so dyld aborts before `main`. **A version
check is the most innocent thing you can run and it still reached his screen** — so the
rule is not "be careful with GUI apps", it is that nothing runs until its dependencies
are proven.

The gate has a negative control: it passes 2.33.1's 71 dependencies and names the exact
missing library on 2.36.0.

### The TCC grant is the whole trick

Over ssh there is no GUI session, so the consent dialog that would grant Screen Recording
can never appear and nobody is there to click it. Without the grant `screencapture`
returns a **black frame, silently**.

`provision-guest.sh` writes the grants straight into `TCC.db`. That is only possible
because the cirruslabs base image ships with **SIP disabled** — on a stock Mac, SIP
protects those files from root itself. This is a VM-only trick and must never be done to
the host.

Two client shapes are granted, because Apple changed how the ssh session identifies
itself across releases: the bundle id `com.apple.sshd-session` and the executable path
`/usr/libexec/sshd-keygen-wrapper`. Guessing wrong costs a black frame with no error, so
both are granted. Columns are named, never positional — the `access` table has gained
columns in several macOS releases.

TCC caches decisions per process, so `setup.sh` **reboots the guest** after writing them.

### A black frame is a failure, not a picture

A missing grant, a slept display and an engaged screensaver all produce a perfectly valid
PNG full of black pixels. `shot.sh` measures every capture and refuses to return one that
is less than 2% non-black, because handing one back is how a harness reports a green run
of something it never saw. Provisioning separately disables display sleep, screensaver
and screen lock so that one of those three causes simply cannot happen.

### Quitting is by pid, never by keystroke

`stop.sh` sends `kill -TERM` to the pid `run.sh` launched. System Events addresses
whatever is **frontmost** and on 2026-09-19 at 18:10Z a Command-Q sent that way landed
in the CEO's Terminal. `ax.sh --key` carries the same discipline: it resolves the app's
pid, verifies that pid is frontmost and refuses to send otherwise.

### No third-party default is trusted

| Setting | Value | Why not the default |
|---|---|---|
| Tart version | **2.33.1** | `latest` (2.37.0) aborts in dyld on macOS 15.6 |
| Install method | release tarball | `brew install cirruslabs/cli/tart` fails — the tap's formula uses a `depends_on` form current Homebrew refuses |
| Guest image | `macos-sequoia-base`, digest recorded in `IMAGE_DIGEST` | matches the host's major version; `base` has Homebrew **and SIP off**, which the TCC grants require. ghcr publishes only a `latest` tag, so the digest is the only pin with meaning |
| Display | **1680x1050** | Tart's default is 1024x768; the app derives a 1400x880 window and would be clipped — every screenshot would be a lie |
| Per VM | **4 cpu / 5120 MB** | measured 2026-09-27 with `mem-walk.py`: a real walk (first run and a model turn) peaked at 3218 MB used with zero guest swap-outs and a 70 s boot; two 7 GB guests swapped this Mac at 120 MB/s. Two 5 GB guests need 58% of the Mac available (lib.sh has the numbers) |
| `TART_HOME` | inside `~/.richos-testvm` | tart defaults to `~/.tart`; one directory means one answer to "where is the disk" and one `rm` to remove it |
| Key install | `/usr/bin/expect` | `sshpass` is not in Homebrew core and needs a third-party tap |
| ssh known hosts | `/dev/null` | a clone's host key changes every run; the CEO's `known_hosts` is not polluted |
| `screencapture` | `-x` | the default plays a shutter sound and guest audio is mixed into the host's output — it would make a noise in his room |
| ssh | `BatchMode=yes` | without it, failed key auth falls back to a password prompt: a hang or an askpass **window** on his screen |
| `caffeinate` | `-is`, never `-dimsu` | `-d`/`-u` would keep his display lit and unlocked for the length of every test |
| Guest staging dir | `$HOME`, not `/tmp` | Homebrew's tesseract cannot read the guest's `/tmp` |
| `--engine` untar | **`--strip-components 1`, then a shape check** | the release tarball's one top-level member is `engine/`, so a plain untar produced `<payload>/engine/engine/…`; the app asks for `<dir>/scripts/hooks` and `<dir>/VERSION` and said *"the RichOS engine isn't on this Mac"* — which is why nightlies .7 and .8 could not measure Mac → phone |
| `claude` auto-updates in the guest | **`DISABLE_AUTOUPDATER=1`** | the `autoUpdates:false` CONFIG key is ignored for a native install once `autoUpdatesProtectedForNative` is set, which the native updater sets itself — a config pin would look set and do nothing |
| The guest's `claude` version | **the host's, re-checked every run** | a binary copied once into the base image is a snapshot; the host's updates four times in two days and the VM is supposed to be his Mac |
| Writing the credential into the guest | **`security -i` on stdin** | `add-generic-password -w <secret>` puts it in the process table and in every log of the command line; `security -i` is the mode Claude Code itself uses for the same item |
| `tailscale up --auth-key` | `file:<path>`, never the key inline | an inline key is in the guest's process table and in every log that records a command line |
| `tailscale up --timeout` | **90s** | the flag's own default is `0s`, documented as *"blocks forever"* — a hang with no terminal to notice it |
| `tailscale up --operator` | the guest's console user | the app is not root and `tailscale cert` is a mutating call it makes itself |
| `tailscale up --reset` | on | `up` refuses when an unspecified setting would change; a clone starts from a known state, not an inherited one |
| `tailscale up --accept-dns` | **true** (the vendor default, stated) | the pairing URL is opened in Safari *inside the guest*; only MagicDNS resolves the name there |
| `tailscale up --shields-up` | **false** (stated) | shields-up blocks inbound connections, which is exactly what a phone makes |
| `tailscale up --ssh` / `--advertise-*` | off | the guest is reached over the LAN already; a throwaway node advertises nothing |

---

## Pointing the nightly's on-screen gate at a VM

The nightly build's gates phase runs `gui-boot.test.sh` **on screen** for ~162 s, so a
build cannot currently run on this Mac without touching the CEO's screen. The guest is a
general-purpose app host, not a screenshot tool: the app is launched with the same
environment the host recipe uses, so a host-side test script can be copied in and run
over the same ssh channel.

```sh
cat > "$SCRATCH/gui-boot-in-guest.sh" <<'SH'
#!/bin/sh
./guest.sh "$1" --push gui-boot.test.sh /tmp/ && ./guest.sh "$1" 'bash /tmp/gui-boot.test.sh'
SH
chmod +x "$SCRATCH/gui-boot-in-guest.sh"
./run-walk.py --wait 900 --bundle <zip> --home <fixture> --engine <tar> --report "$SCRATCH/run.json" -- \
  "$SCRATCH/gui-boot-in-guest.sh"
```

---

## When it breaks

| Symptom | Cause and fix |
|---|---|
| `shot.sh` says **BLACK FRAME** | The TCC grant did not take. `./setup.sh --reprovision` (it reboots the guest so `tccd` reloads). |
| `osascript`: *not allowed assistive access* | The Accessibility grant did not take. Same fix. |
| `run.sh`: app started but **0 windows** | Read the app log: `ssh admin@<ip> cat ~/testvm/<vm>/app.log`. Usually the bundle kept its quarantine flag and a Gatekeeper dialog is waiting in the guest with nobody to click it. |
| `tesseract`: *image file not found*, naming a file that exists | It is reading from `/tmp`. Stage under `$HOME` — see the note in `shot.sh`. |
| `reap.sh`: *VICTIMS[@]: unbound variable* | A regression of the bash 3.2 empty-array bug; use `${arr[@]+"${arr[@]}"}`. |
| VM never reports an IP | `cat ~/.richos-testvm/log/<vm>.log`. |
| *"tart cannot be opened because of a problem"* | The pin was bypassed. Run `preflight-binary.py` against the binary and read what it names. |
| Disk filling up | `./reap.sh` — clones that outlived their agent. |
| `run.sh` says **tailnet: NOT JOINED** | Read the line after it: it names the one thing to do. Usually the auth key is missing, expired or the file has more than the key in it. Everything except the phone path still works. |
| The app says **"not set up yet"** with a joined guest | `./tailnet.sh doctor <vm>`. If `certificates: no`, the tailnet's HTTPS switch is off — <https://login.tailscale.com/admin/dns>. If `backend state` is not `Running`, the join did not take. |
| A pairing code appears but the phone cannot reach the Mac | Check the node is online from the host: `./tailnet.sh nodes`, then `curl -sk https://<name>:8443/`. A phone signed in to a *different* Tailscale account is in a different tailnet and sees nothing. |
| `tailscale cert` refuses with a rate-limit message | The certificate cache did not hit and the name has been issued too many times this week. `ls ~/.richos-testvm/certs/` — an entry is only re-used if it is for that exact name and outlives 720h. |
| A `richos-test-*` node is still listed with no VM behind it | Expected briefly: ephemeral nodes age out after the control plane stops hearing from them. If one persists, remove it in the admin console — nothing here holds the rights to. |
