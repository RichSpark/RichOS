#!/usr/bin/env python3
"""hid-key.py - run IN THE GUEST: press one key the way a hardware keyboard does.

  hid-key.py KEYCODE [--fn] [--keypad] [--char HEX] [--cmd] [--shift] [--ctrl] [--opt] [--times N]

Posts the key's down and up as CoreGraphics keyboard events at the HID event tap
(kCGHIDEventTap), the point where a real keyboard's events enter the system. Everything the
system routes after that (the session tap, the annotated session tap that names the receiving
app, the app itself) sees it as it sees a key from a hand.

Why this exists: a walk that judges how a product handles a key should press that key the way a
keyboard does. System Events' `key code` is synthesized by another process with its own flags and
no character of the keyboard's choosing, so a result read from it is a result about System Events.
Forward Delete from a Mac keyboard is key code 117 with the Fn and keypad flags and the character
U+F728: `hid-key.py 117 --fn --keypad --char f728`.

--fn, --keypad, --cmd, --shift, --ctrl and --opt set the event flags (SecondaryFn, NumericPad,
Command, Shift, Control, Alternate) on both events; --char sets the event's Unicode character
(hexadecimal); --times presses it N times (1..20), 0.08 s apart. Prints one JSON line naming what
was posted. Exit 0 once posted; 2 when it cannot post (not a virtual machine, no CoreGraphics, a bad
argument, no permission to post events), with a sentence. It refuses to post anywhere but a virtual machine. Added with the Desktop
Forward Delete walk.
"""
import argparse
import ctypes
import ctypes.util
import json
import sys
import time

FLAGS = {'fn': 1 << 23, 'keypad': 1 << 21, 'cmd': 1 << 20, 'opt': 1 << 19, 'ctrl': 1 << 18, 'shift': 1 << 17}
HID_TAP = 0


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('keycode', type=int)
    for name in FLAGS:
        p.add_argument('--' + name, action='store_true')
    p.add_argument('--char', metavar='HEX')
    p.add_argument('--times', type=int, default=1)
    p.add_argument('--dry-run', action='store_true', help='check the arguments and print what would be posted')
    a = p.parse_args(argv)
    if not 0 <= a.keycode <= 127:
        print('hid-key: a key code is 0..127', file=sys.stderr)
        return 2
    if not 1 <= a.times <= 20:
        print('hid-key: --times is 1..20', file=sys.stderr)
        return 2
    char = None
    if a.char is not None:
        try:
            char = int(a.char, 16)
            assert 0 < char <= 0xFFFF
        except (ValueError, AssertionError):
            print('hid-key: --char is one hexadecimal character code, such as f728', file=sys.stderr)
            return 2
    mask = sum(v for k, v in FLAGS.items() if getattr(a, k))
    said = {'posted': not a.dry_run, 'tap': 'hid', 'keycode': a.keycode, 'flags': mask,
            'char': ('%04x' % char) if char is not None else None, 'times': a.times}
    if a.dry_run:
        print(json.dumps(said))
        return 0
    # Only ever inside the test guest: a key posted here on the operator's Mac would land in
    # whatever he is typing into (CEO ruling 65). kern.hv_vmm_present is 1 in a virtual machine.
    import subprocess
    vm = subprocess.run(['sysctl', '-n', 'kern.hv_vmm_present'], capture_output=True, text=True).stdout.strip()
    if vm != '1':
        print('hid-key: refusing to post a key outside a virtual machine (kern.hv_vmm_present=%s)' % (vm or 'unknown'),
              file=sys.stderr)
        return 2
    path = ctypes.util.find_library('ApplicationServices')
    if not path:
        print('hid-key: CoreGraphics is not available here (run it in the macOS guest)', file=sys.stderr)
        return 2
    cg = ctypes.CDLL(path)
    vp = ctypes.c_void_p
    cg.CGEventCreateKeyboardEvent.restype = vp
    cg.CGEventCreateKeyboardEvent.argtypes = [vp, ctypes.c_uint16, ctypes.c_bool]
    cg.CGEventSetFlags.argtypes = [vp, ctypes.c_uint64]
    cg.CGEventKeyboardSetUnicodeString.argtypes = [vp, ctypes.c_ulong, ctypes.POINTER(ctypes.c_uint16)]
    cg.CGEventPost.argtypes = [ctypes.c_uint32, vp]
    # CGEventPost returns nothing: without the permission to post events the key is dropped silently,
    # and a walk would read an undelivered key as the product's answer. Ask first.
    cg.CGPreflightPostEventAccess.restype = ctypes.c_bool
    if not cg.CGPreflightPostEventAccess():
        print('hid-key: this process may not post events (CGPreflightPostEventAccess is false); nothing was posted',
              file=sys.stderr)
        return 2
    cf = ctypes.CDLL(ctypes.util.find_library('CoreFoundation'))
    cf.CFRelease.argtypes = [vp]
    for n in range(a.times):
        for down in (True, False):
            ev = cg.CGEventCreateKeyboardEvent(None, a.keycode, down)
            if not ev:
                print('hid-key: CoreGraphics made no keyboard event', file=sys.stderr)
                return 2
            cg.CGEventSetFlags(ev, mask)
            if char is not None:
                c = ctypes.c_uint16(char)
                cg.CGEventKeyboardSetUnicodeString(ev, 1, ctypes.byref(c))
            cg.CGEventPost(HID_TAP, ev)
            cf.CFRelease(ev)
            time.sleep(0.02)
        if n + 1 < a.times:
            time.sleep(0.08)
    print(json.dumps(said))
    return 0


if __name__ == '__main__':
    sys.exit(main())
