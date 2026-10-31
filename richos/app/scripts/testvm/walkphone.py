"""walkphone.py — the phone a VM walk pairs and sends with: the actual RichConnect client, run on the
host by walk-phone.mjs beside this file, talking to the Mac in the guest over the same API the
native apps use (the way richos/app/scripts/mobile-mac.test.sh drives it).

It replaced the guest's Safari, which drove the phone web page the Mac no longer serves (retired
2026-10-30, richos 89f4a018a): the Mac's pairing address now answers "Use the RichConnect app".
The address is the guest's tailnet name with a certificate `tailscale cert` issued, so the host
reaches it as a phone does, when the guest was booted with run-walk.py --tailnet.

The state file holds the walk's own test key and session; keep it under the walk's --out.
"""
import json
from pathlib import Path
import subprocess

PHONE = Path(__file__).resolve().parent / 'walk-phone.mjs'


class PhoneFailed(RuntimeError):
    pass


def run(verb, state, *args, timeout=90):
    """walk-phone.mjs VERB --state STATE ARGS..., its one JSON line back, or PhoneFailed with its words."""
    argv = ['node', str(PHONE), verb, '--state', str(state), *map(str, args)]
    try:
        r = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise PhoneFailed('the walk phone could not %s: %s' % (verb, exc)) from exc
    if r.returncode:
        raise PhoneFailed((r.stderr.strip() or r.stdout.strip() or 'walk-phone.mjs exited %d' % r.returncode)[-600:])
    lines = [s for s in r.stdout.splitlines() if s.startswith('{')]
    if not lines:
        raise PhoneFailed('the walk phone said nothing it could read: ' + r.stdout[-300:])
    return json.loads(lines[-1])


def pair(state, link, reject=False):
    """Pairs with the Mac's pairing address and answers its six words on the phone: "They match",
    or "They do not match" with reject=True. The Mac activates a matched phone with its own
    "They match" afterwards (Sage F1). Returns {"words", "device", "threads"}."""
    return run('pair', state, '--link', link, *(['--reject'] if reject else []))


def send(state, thread, text, seconds=60):
    """The paired phone sends TEXT on conversation THREAD from its composer and waits for the Mac's
    receipt. Returns {"thread", "text"}."""
    return run('send', state, '--thread', thread, '--text', text, '--seconds', seconds, timeout=seconds + 60)
