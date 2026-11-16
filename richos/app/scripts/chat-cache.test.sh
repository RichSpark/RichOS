#!/usr/bin/env bash
# The context meter and Keep chat alive (CEO 2026-11-16, round 83), from the sessions' own
# streams to the window: the readings and the 55-minute rule, the keep-alive message at a
# session, what it does to a conversation, the window's wire shape and each chat's saved box,
# and the two surfaces in the real renderer.
# run-tests: no-host-screen: headless WebKit, stand-in claude children and core fixtures; no model, no network, no sound
# run-tests: inputs richos/app/scripts/chat-cache.test.sh richos/app/crates/richos-core/src/chat_cache.rs richos/app/crates/richos-core/src/native.rs richos/app/crates/richos-core/src/spine.rs richos/app/crates/richos-core/src/work_host.rs richos/app/crates/richos-core/src/ecs.rs richos/app/crates/richos-core/src/cognition.rs richos/app/crates/richos-core/tests/keep_chat_alive_tests.rs richos/app/src-tauri/src/chat_cache_host.rs richos/app/src-tauri/src/nav.rs richos/app/ui/context-meter.js richos/app/ui/context-meter.css richos/app/ui/keep-alive.js richos/app/ui/keep-alive.css richos/app/ui/settings-button.js richos/app/ui/mock.js richos/app/ui/tests/context-meter.js richos/app/ui/tests/keep-alive.js richos/engine/scripts/app-engine-hook.py
# run-tests: covers richos/app/src-tauri/src/chat_cache_host.rs richos/app/crates/richos-core/src/chat_cache.rs richos/app/ui/context-meter.js richos/app/ui/keep-alive.js
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo test --manifest-path "$here/../Cargo.toml" -p richos-core --lib chat_cache
cargo test --manifest-path "$here/../Cargo.toml" -p richos-core --lib a_chats_context_size_is_its_leads_last_request
cargo test --manifest-path "$here/../Cargo.toml" -p richos-core --lib the_keep_alive_message_runs_under_no_grant
cargo test --manifest-path "$here/../Cargo.toml" -p richos-core --test keep_chat_alive_tests
cargo test --manifest-path "$here/../src-tauri/Cargo.toml" chat_cache_host
cargo test --manifest-path "$here/../src-tauri/Cargo.toml" keep_chat_alive_is_off_in_every_chat
node "$here/../ui/tests/context-meter.js"
node "$here/../ui/tests/keep-alive.js"
