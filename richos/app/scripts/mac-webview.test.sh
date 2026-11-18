#!/usr/bin/env bash
# run-tests: no-host-screen: fixtures stand in for xcrun and the native WebView
# run-tests: inputs richos/app/scripts/mac-webview.test.sh richos/app/scripts/mac-webview.test.js richos/app/ui/tests/lib/mac-webview.js richos/app/ui/tests/lib/mac-webview.swift
# run-tests: covers richos/app/scripts/mac-webview.test.js
set -euo pipefail
APP="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec node --test "$APP/scripts/mac-webview.test.js"
