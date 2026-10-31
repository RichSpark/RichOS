#!/usr/bin/env python3
"""app_review.py: WHERE THE APP'S SECOND REVIEWS LIVE.

Slice 4 of richos-hq docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md, with Sage's
check beside it (...-sage-check.md §2 catches 2 and 7, §4 row 4). His words (ruling §113): "A
regular RichOS user can never be expected anything even remotely close to that. So, this all
must be completely automated."

Every reader takes the app's folders from here, so none can disagree about which ledger holds a
worker's verdicts:
  * review_watch.py in app mode (AppWorld), the host's own child, which starts mid-job reviews
    and writes their verdicts through second-review into app_paths()["ledger"];
  * scripts/app-engine-hook.py, which runs the terminal's own delivery
    (scripts/hooks/deliver-review-verdict.sh) at every tool call of the app's workers, with this
    ledger and this watch folder, so a worker is told a verdict on its own work once (CEO §154,
    2026-10-31: the app's copy of that delivery is gone);
  * mega-lander/app.py, whose integrate reads a handover verdict from this ledger.

Deliberately small and import-light: the hook loads it on a worker's tool calls.
"""

import os


def app_paths(app_state):
    """Under the app's own engine state, never ~/.claude (plan §2.4: "In the app, the same
    ledger under the app's own data folder")."""
    root = os.path.realpath(app_state)
    return {"watch": os.path.join(root, "review-watch"),
            "reviews": os.path.join(root, "second-review"),
            "ledger": os.path.join(root, "second-review", "reviews.jsonl"),
            "delivered": os.path.join(root, "review-watch", "delivered")}
