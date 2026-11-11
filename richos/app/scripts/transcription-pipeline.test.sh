#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/transcription-pipeline.test.sh richos/tools/richos-service/lib/merge.js richos/tools/richos-service/lib/config.js richos/tools/richos-service/lib/transcribe.js richos/tools/richos-service/lib/pipeline.js richos/tools/richos-service/lib/live-transcription.js richos/tools/richos-service/lib/restyle.js richos/tools/richos-service/test/run.js richos/tools/richos-service/test/transcript-style.test.mjs richos/tools/richos-service/test/live-transcription.test.mjs
# run-tests: covers richos/tools/richos-service/lib/merge.js richos/tools/richos-service/lib/config.js richos/tools/richos-service/lib/transcribe.js richos/tools/richos-service/lib/pipeline.js richos/tools/richos-service/lib/live-transcription.js richos/tools/richos-service/lib/restyle.js richos/tools/richos-service/test/run.js richos/tools/richos-service/test/transcript-style.test.mjs richos/tools/richos-service/test/live-transcription.test.mjs
#
# The transcription pipeline's own pure-logic suite (`tools/richos-service/test/run.js`: no ffmpeg,
# no whisper), given a seat in the proof selector. Until 2026-10-04 no script suite claimed the
# pipeline's library, so a change to how transcripts are merged, rendered or decoded was UNCOVERED
# at commit even though this suite asserts on every file declared above.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"
node richos/tools/richos-service/test/run.js
# Transcript readability (CEO report 2026-10-04): paragraphs, punctuation, capitalization, names.
node richos/tools/richos-service/test/transcript-style.test.mjs
node --test richos/tools/richos-service/test/live-transcription.test.mjs
