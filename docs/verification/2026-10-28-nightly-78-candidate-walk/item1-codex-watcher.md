# Item 1 evidence: Codex watcher walk, guest walk-6f77dcb76a39 (earlier run, copied verbatim)

## report.json

```json
{
  "vm": "walk-6f77dcb76a39",
  "expect_sha": "f1de705bab2ea4d366c0dff996f635bba9599280",
  "steps": [
    {
      "step": "identity",
      "evidence": {
        "built_from": "f1de705bab2ea4d366c0dff996f635bba9599280"
      },
      "outcome": "PASS",
      "seconds": 0.2
    },
    {
      "step": "first-run",
      "evidence": {
        "thread": "thr_870dcd56279442749d4c35da5f618b0d",
        "title": "Onboarding"
      },
      "outcome": "PASS",
      "seconds": 22.1
    },
    {
      "step": "channel",
      "evidence": {
        "mark": "codexwalk-first-d3ac01dafd",
        "row": {
          "file": "/Users/admin/testvm/walk-6f77dcb76a39/home/Library/Application Support/com.richos.app/engine-state/messages/acme/thr_870dcd56279442749d4c35da5f618b0d/rich.jsonl",
          "text": "CODEX-WATCH 12:30Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines 1-4 of /Users/admin/testvm/walk-6f77dcb76a39/home/.richos-coordination/rich-codex/to-rich.md); every new line follows:\n## 2026-10-28 12:30Z \u2014 WALK FIRST\ncodexwalk-first-d3ac01dafd\nSecond line of the first entry."
        }
      },
      "outcome": "PASS",
      "seconds": 42.7
    },
    {
      "step": "relaunch",
      "evidence": {
        "composer_back_after_s": 24.1,
        "home_screen_door_pressed_at_s": null,
        "recorded_pid": "1834",
        "windows": 1,
        "log": "/Users/admin/testvm/walk-6f77dcb76a39/relaunch-1793190678376825000.log"
      },
      "outcome": "PASS",
      "seconds": 27.1
    },
    {
      "step": "append",
      "evidence": {
        "mark": "codexwalk-second-9d79b95a74",
        "turn_id": "turn_041bb12c5acd476e9eb4fce5525dc882",
        "source": "agent",
        "reply_characters": 194
      },
      "outcome": "PASS",
      "seconds": 46.0
    },
    {
      "step": "answer",
      "evidence": {
        "line": "walk-ack codexwalk-second-9d79b95a74",
        "to_codex_bytes": 36
      },
      "outcome": "PASS",
      "seconds": 13.9
    }
  ]
}

```

## facts.json

```json
{
  "marks": {
    "first": "codexwalk-first-d3ac01dafd",
    "second": "codexwalk-second-9d79b95a74"
  },
  "built_from": "f1de705bab2ea4d366c0dff996f635bba9599280",
  "thread": "thr_870dcd56279442749d4c35da5f618b0d",
  "thread_title": "Onboarding"
}

```

## channel.json

```json
{
  "mark": "codexwalk-first-d3ac01dafd",
  "rows": [
    {
      "file": "/Users/admin/testvm/walk-6f77dcb76a39/home/Library/Application Support/com.richos.app/engine-state/messages/acme/thr_870dcd56279442749d4c35da5f618b0d/rich.jsonl",
      "text": "CODEX-WATCH 12:30Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines 1-4 of /Users/admin/testvm/walk-6f77dcb76a39/home/.richos-coordination/rich-codex/to-rich.md); every new line follows:\n## 2026-10-28 12:30Z \u2014 WALK FIRST\ncodexwalk-first-d3ac01dafd\nSecond line of the first entry."
    }
  ]
}

```

## relaunch-observed.json

```json
{
  "before": {
    "recorded_pid": "1073",
    "rows": [
      {
        "pid": 1073,
        "ppid": 1,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1454,
        "ppid": 1437,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1456,
        "ppid": 1437,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1458,
        "ppid": 1437,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1459,
        "ppid": 1437,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1460,
        "ppid": 1437,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1556,
        "ppid": 1536,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1558,
        "ppid": 1536,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1560,
        "ppid": 1536,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1561,
        "ppid": 1536,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      },
      {
        "pid": 1562,
        "ppid": 1536,
        "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
      }
    ]
  },
  "relaunched": {
    "pid": 1834,
    "log": "/Users/admin/testvm/walk-6f77dcb76a39/relaunch-1793190678376825000.log",
    "app": "/Users/admin/testvm/walk-6f77dcb76a39/RichOS.app"
  },
  "settled": {
    "home_on_screen_after_s": 18.1,
    "entered_with": "Enter"
  },
  "samples": [
    {
      "t": 24.1,
      "recorded_pid": "1834",
      "rows": [
        {
          "pid": 1834,
          "ppid": 1,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2001,
          "ppid": 1960,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2005,
          "ppid": 1960,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2012,
          "ppid": 1960,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2021,
          "ppid": 1960,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2022,
          "ppid": 1960,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2132,
          "ppid": 2115,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2134,
          "ppid": 2115,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2142,
          "ppid": 2115,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2147,
          "ppid": 2115,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        },
        {
          "pid": 2148,
          "ppid": 2115,
          "exe": "/RichOS.app/Contents/MacOS/richos-tauri"
        }
      ],
      "windows": 1,
      "composer": true
    }
  ],
  "home_screen_door_pressed_at_s": null
}

```

## append.json

```json
{
  "mark": "codexwalk-second-9d79b95a74",
  "rows": [
    {
      "file": "/Users/admin/testvm/walk-6f77dcb76a39/home/Library/Application Support/com.richos.app/engine-state/messages/acme/thr_870dcd56279442749d4c35da5f618b0d/rich.jsonl",
      "text": "CODEX-WATCH 12:32Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines 5-8 of /Users/admin/testvm/walk-6f77dcb76a39/home/.richos-coordination/rich-codex/to-rich.md); every new line follows:\n## 2026-10-28 12:31Z \u2014 WALK SECOND\ncodexwalk-second-9d79b95a74\nSecond line of the second entry."
    }
  ],
  "first_entry_rows_after_relaunch": [
    {
      "file": "/Users/admin/testvm/walk-6f77dcb76a39/home/Library/Application Support/com.richos.app/engine-state/messages/acme/thr_870dcd56279442749d4c35da5f618b0d/rich.jsonl",
      "text": "CODEX-WATCH 12:30Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines 1-4 of /Users/admin/testvm/walk-6f77dcb76a39/home/.richos-coordination/rich-codex/to-rich.md); every new line follows:\n## 2026-10-28 12:30Z \u2014 WALK FIRST\ncodexwalk-first-d3ac01dafd\nSecond line of the first entry."
    }
  ],
  "rich_turn": {
    "turn_id": "turn_041bb12c5acd476e9eb4fce5525dc882",
    "source": "agent",
    "prompt": "{\"from\":\"codex-watch\",\"text\":\"CODEX-WATCH 12:32Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines 5-8 of /Users/admin/testvm/walk-6f77dcb76a39/home/.richos-coordination/rich-codex/to-rich.md); every new line follows:\\n## 2026-10-28 12:31Z \u2014 WALK SECOND\\ncodexwalk-second-9d79b95a74\\nSecond line of the second entry.\",\"instruction\":null}"
  },
  "turn_ended": "TurnCompleted",
  "reply": "Codex left a second note at 12:31Z titled \"WALK SECOND\" (reference `codexwalk-second-9d79b95a74`). Like the first one, it's just two lines and asks for nothing, so there's nothing for you to do."
}

```

## to-codex.md

```
walk-ack codexwalk-second-9d79b95a74
```
