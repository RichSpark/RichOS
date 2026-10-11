# The host display/power guard, measured before it was wired

Every number in `guard-host-display-power.sh` comes from here, and everything here
comes from driving real payloads through **that file's own python3 block** — not
through a model of it. Re-run it before changing any pattern: build a `Bash`,
`Write`, `Agent` or `SendMessage` payload per item and pipe it in.

Three corpora, 1,647 payloads, **21 refusals, zero false positives**.

| corpus | payloads | refused |
|---|---|---|
| every `Bash` tool call in the session that ordered this guard | 1,097 | 19 |
| all 248 committed briefs, `ceo-decisions.md`, `CLAUDE.md`, every memory note | 535 | 2 |
| the 15 build and `testvm/` scripts that landed at `321f1076` mid-build | 15 | 0 |

---

## 1. The Bash corpus — 1,097 commands, 19 refused

Source: the `Bash` `tool_use` inputs of session `f5aeaea1-80fa-4210-b51f-720f89942a80`.

### 10 × `caffeinate -u` — the nightly candidate build

```
caffeinate -dimsu python3 scripts/nightly-local.py build --runtime-dir ~/.richos-nightly/runtime
```

**Right, and it is the highest-value catch in the measurement.** `-dimsu` contains
`-u` — *"if the display is off, this option turns the display ON and prevents the
display from going into idle sleep"* — and `-d`, which holds the display awake.
The build runs about twenty minutes. Nothing anywhere was watching for it, and
`caffeinate -is` does the job without touching the display.

It was already known to be wrong, by hand, once: at 18:45:03Z `zach-opus-testvm1`
was told to run the VM host under `caffeinate -dimsu` and corrected it itself —
*"-d (prevent DISPLAY sleep) and -u (declare user active) … would keep the CEO's
screen lit and awake permanently"* — shipping `-is` instead. This encodes what it
worked out, so it does not have to be worked out again.

### 9 × `osascript` modifier keystroke — the Command-Q quit

```
osascript -e 'tell application "System Events" to keystroke "q" using command down'
```

**Right.** `ceo-decisions.md` §65 bans it by name, and
`feedback_never_quit_by_keystroke.md` records why: at ~18:10Z one of these was
addressed at a specific unix id, the target had already exited, System Events
delivered to whatever was FRONTMOST, and the CEO's Terminal offered to terminate
the session. Three of the nine are Rich's own relaunch script.

**42 such calls exist across this project's transcripts** and every one is an agent
quitting its own on-screen test instance — the procedure §54 addendum 4 requires.
They are not collateral; they are the practice §65 ends. That is why the refusal
carries the replacement (`kill -TERM <pid>`, confirm with `pgrep`) rather than a
pointer to a rule.

### 0 × anything else

`pmset`, `killall`, `shutdown`, `defaults`, `nvram`, `systemsetup`, `open`,
`launchctl` never fired on this corpus. The two real `pmset displaysleepnow`
executions are in a SUBAGENT transcript (`agent-acb0a69e78f52de0b.jsonl`, lines 499
and 503) and are driven directly as suite fixtures A1 and A2.

### Four that fired before the heredoc rule and should not have

`cat > <file> <<'EOF' … EOF` is how briefs, memory notes and commit messages are
written here. Read as a command chain, four of them were refused — including the two
documents that BAN the practice:

* `feedback_never_quit_by_keystroke.md` — its body says *"never any key with
  `command down` through System Events"*.
* `feedback_never_touch_host_display_or_power.md`.
* `zach-brief-guard-host-display-and-power-2026-09-19.md` — the brief that ordered
  this guard.
* `echo-brief-front-door-suite-in-the-build-2026-09-18.md`.

That is the *"a guard that forbids the record of what it forbids"* shape, arriving
through the Bash door rather than the Write door. A heredoc body is now lifted out
and judged as a document; the rest of the command line is still judged as shell. The
redirect target picks the scope, so a heredoc writing a **script** is still judged
per line.

---

## 2. The document corpus — 535 documents, 2 refused

All 248 files in `richos-hq/docs/briefs`, the whole of `richos-hq/wiki/ceo-decisions.md`,
`femcboost/CLAUDE.md`, and every file in the session memory directory.

**`echo-brief-wait-for-screen-unlock-2026-09-18.md`, line 47** — the sentence that
put the command in a teammate's hands:

> …(you may lock the screen with `pmset displaysleepnow` only when Rich says the
> screen is free — ask first; a walk is on it now).

Right. **Written on 2026-09-18, the day before §65 was ruled** — the rule is right
and the document predates it. Stated rather than glossed.

**`zach-brief-candidate-build-time-2026-09-19.md`, line 5** — `caffeinate -dimsu` in
a brief. Right.

**Nothing else.** `ceo-decisions.md` §65 passes **on merit**, with no path exemption,
though it quotes `pmset displaysleepnow`, `` `caffeinate -d`/`-u` `` and
`killall loginwindow` — a ruling has to name what it rules on.

### The one known miss

`echo-brief-front-door-suite-in-the-build-2026-09-18.md` carries `caffeinate -dimsu`
as an instruction in two blocks, and both are exempted, because each block also
quotes a test runner: *"REFUSED: --bundle must name a…"* and *"the runner must
refuse"*. The block-level cue test cannot tell a refusal ABOUT this command from a
refusal about something else, and the available fix is not acceptable: `refus…` is
the central verb of this whole record, and dropping it would refuse §65 itself.

**The miss is affordable and the reason is the design.** The prose layer is defense
in depth, tuned toward permissiveness on purpose, because its failure mode —
forbidding the record — is unrecoverable. The **shell layer** is the airtight one: it
applies no cue test to a real command, and it refused that same brief's
`caffeinate -dimsu` four separate times when somebody actually ran it.

### Measured and NOT load-bearing

Removing `\bfail(?:s|ed|ure)?\b` from the cue vocabulary changes **no verdict on any
of the 1,632 payloads** in corpora 1 and 2. It is retained as vocabulary, not as a
proven property, and is named here so a later reader does not mistake it for one.

---

## 3. The 15 scripts that landed mid-build — `321f1076`, 0 refused

This corpus exists because the lead sent the input while the guard was being written,
and it was worth more than the two corpora above: **three defects fell out of it, and
two of them would have blocked an engineer on every save.**

| defect | what it did |
|---|---|
| word-scan on the code surface | `brew install tesseract displayplacer` read as an invocation of `displayplacer`. `provision-guest.sh` refused. |
| line continuations not joined | `testvm/setup.sh` reboots the GUEST with an `ssh` whose argument is on the next line; per physical line the continuation loses its command word. |
| `testvm/` not a file-surface boundary | `provision-guest.sh` runs `pmset -a displaysleep 0` and three `defaults write com.apple.screensaver` lines INSIDE the guest, because the whole file is copied there and executed. Nothing in the text says so. |

The code surface now requires **command position**, the same test the Bash surface
always applied; continuations are joined; and `testvm/` is a declared guest-side
boundary on the file surface as well as the command surface.

What that boundary costs is stated in the guard rather than buried: a host-changing
command added to a `testvm/` script is not caught. `testvm/setup.sh` genuinely does
run host-side, so the boundary is **declared** — one path segment, reviewed as a
unit — and is not derived from anything the guard can check.

---

## 4. What this measurement cost, recorded because it is the point

**The mutation harness ran `brew install` on the CEO's Mac.**

Each mutant's rationale is passed as a double-quoted bash argument, and a backtick
inside double quotes is command substitution. One rationale read *"…makes
`brew install tesseract displayplacer` an invocation of displayplacer…"*. Bash ran it.
58 Homebrew formulae changed at 20:56 on 2026-09-19, including **`displayplacer`
itself — a display-reconfiguration tool, the exact class §65 exists to keep off this
machine.** A guard against changing the host, whose own harness changed the host.

Cleanup and its limit, attributed from `INSTALL_RECEIPT.json` timestamps rather than
directory mtimes (linking touches those):

* `displayplacer` — absent at 19:12, present at 20:56, so mine. Uninstalled, verified
  gone.
* `tesseract` — mine, but `ffmpeg-full` now depends on it. **Not removed**; removing
  it would break `ffmpeg-full`.
* `whisper.cpp` relinked 1.9.1 → 1.9.4; 1.9.1 is still on disk. Not a product risk —
  `voice_provision.rs:32` says RichOS uses *"no binary, ever"* and fetches pinned
  weights.
* The uninstall auto-removed `unbound`, `libevent`, `mbedtls@3`. `brew missing`
  afterwards reports only a pre-existing `gcloud-cli: python@3.13` gap.

Raised as `esc-20260919T200721Z-123c950d`, since one decision there is the lead's.

**The mechanism, not the care.** Every backtick is gone from
`host-display-power.mutation.sh`, and suite case **Z3** asserts there are none.
Beside it, **Z1** asserts the guard parses as bash and **Z2** asserts its embedded
python block contains no apostrophe — because a single apostrophe closes the bash
quote the classifier lives in and reparses the rest of the file as shell. That
happened three times while this guard was written, and each time the file died with
an EOF error a hundred lines from the cause. Z2 caught the fourth one immediately and
named the line.
