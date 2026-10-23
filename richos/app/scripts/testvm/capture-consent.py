#!/usr/bin/python3
"""capture-consent.py — RUNS INSIDE THE GUEST (run.sh pipes it to the guest's /usr/bin/python3).

macOS 15 asks again, every kScreenCapturePrivacyHintPolicy seconds (30 days), whether a client
that captures the screen may "bypass the system private window picker", and the alert takes the
keyboard focus. The base image's own record of its one approval (2026-09-19, for
/usr/libexec/sshd-keygen-wrapper, the client every ssh capture runs as) came due on 2026-10-19
18:49Z, so from then on the first frame a walk takes in a fresh clone raises it, and whatever the
walk types next goes to the alert instead of the app (two walks on 2026-10-20: the task was typed
while it was up, so no lead ever started).

Each clone is fresh from that image, so each clone's record is renewed here, before anything
captures: every recorded client is marked as alerted and used now, its next hint one policy
period from now, as replayd records it after a real approval. Measured in guests on 2026-10-20:
as the image has it, the first capture raises the alert; with only the hint date and the
last-alerted date renewed, it still does (replayd goes by the last use, 2026-09-19); with the
last use renewed as well, two captures 6 s apart raise nothing and replayd records no alert.
The TCC grants themselves stay provision-guest.sh's. Nothing outside the guest is touched.
"""
import datetime
import os
import plistlib

PATH = os.path.expanduser('~/Library/Group Containers/group.com.apple.replayd/ScreenCaptureApprovals.plist')  # foreign-app-data-exempt: executed only inside the disposable test VM guest (run.sh pipes it to the guest's python over ssh), never on a host
DEFAULT_POLICY = 30 * 24 * 3600

try:
    with open(PATH, 'rb') as fh:
        record = plistlib.load(fh)
except FileNotFoundError:
    print('screen-capture consent: nothing recorded, nothing to renew')
    raise SystemExit(0)
now = datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None, microsecond=0)
renewed = []
for client, entry in record.items():
    if isinstance(entry, dict):
        policy = entry.get('kScreenCapturePrivacyHintPolicy') or DEFAULT_POLICY
        entry['kScreenCaptureApprovalLastAlerted'] = now
        entry['kScreenCaptureApprovalLastUsed'] = now
        entry['kScreenCapturePrivacyHintDate'] = now + datetime.timedelta(seconds=int(policy))
        renewed.append('%s until %s' % (client, entry['kScreenCapturePrivacyHintDate'].isoformat() + 'Z'))
with open(PATH, 'wb') as fh:
    plistlib.dump(record, fh)
print('screen-capture consent renewed: ' + ('; '.join(renewed) or 'no client recorded'))
