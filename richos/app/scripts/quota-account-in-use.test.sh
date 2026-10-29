#!/usr/bin/env bash
# Every usage reading, hold and deferral follows the Claude account IN USE (richos-hq
# docs/plans/follow-ups.md, 7fed0bef; the CEO, 2026-10-28: "This idiotic shit shouldn't need any
# manual checking. That should all be automated."). The regression itself is the Rust test
# quota::tests::every_usage_consumer_follows_the_account_in_use: it switches accounts the app's
# own way and drives the published file, the gate, the quota tool, quota-watch.sh --once and
# second review's quota hold. This suite exists so a change to any of those engine consumers
# selects it (proof-for.sh: a covered file always selects the suite that covers it), which a
# change to an engine file alone would never do for a crate's unit test.
# run-tests: inputs richos/app/scripts/quota-account-in-use.test.sh richos/app/Cargo.toml richos/app/Cargo.lock richos/app/crates/richos-core richos/engine/scripts/quota-watch.sh richos/engine/scripts/lib
# run-tests: covers richos/engine/scripts/lib/quota_watch.py richos/engine/scripts/lib/quota_weekly.py richos/engine/scripts/lib/second_review.py richos/engine/scripts/quota-watch.sh
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$DIR/.."
# The test builds its own app state and environment; nothing here may point it at the live one.
unset RICHOS_APP_STATE RICHOS_WORKSPACES_DIR CLAUDE_CONFIG_DIR
# And the second review's hold is the desktop gate's own decision over 600 published files
# (quota::tests::the_review_hold_is_the_gates_own_decision; second review of 2097df51c).
cargo test --quiet -p richos-core --lib -- --exact quota::tests::every_usage_consumer_follows_the_account_in_use \
    quota::tests::the_review_hold_is_the_gates_own_decision
