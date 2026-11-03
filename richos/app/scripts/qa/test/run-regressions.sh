#!/usr/bin/env bash
# Offline regression fixtures for QA helpers. No provider, browser or guest is started.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
for test in "$HERE"/*.test.py; do
  python3 -B "$test"
done
