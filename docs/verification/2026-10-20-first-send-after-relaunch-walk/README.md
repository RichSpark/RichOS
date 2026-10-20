# First send after a relaunch, sent the way a person sends it (esc-20261020T171021Z-8fd0fca7)

`richos/app/scripts/testvm/first-send-walk.py`, one fresh test guest per run through `run-walk.py`,
fixture data only. Input is a person's: real keystrokes (System Events) and the real mouse
(`pointer-drag.sh`); no accessibility paste and no AXPress. Each send is judged by a
`PromptReceived` row in the guest's `conversation-ledger.jsonl` carrying the exact words.
"Open when typed" is read by OCR off the frame taken right after typing: the Output buttons
are drawn only once the launch has opened the conversation.

| Bundle | Step | Open when typed | Result |
|---|---|---|---|
| main `a7db7f70b` | Return key | no (`return-typed.jpg`: no Output button, rail row not selected) | **SWALLOWED**: composer emptied, no message on screen, no `PromptReceived` row in 30 s (`return-sent.jpg`) |
| main `a7db7f70b` | click on Send | yes | received |
| fix `da423ed82` (main.js as committed in `0c1a05ad6`) | Return key | no | message on screen at once; received 68.8 s after the press, when the launch opened the conversation; one copy, Rich answered (`return-sent.jpg`) |
| fix `da423ed82` | click on Send | yes | received 9.6 s after the click |

The main-bundle run predates the OCR read; "open when typed" for it was read afterwards with
`qa/ocr-find.sh Output` on its kept `*-typed` frames (return: no hit; click: hit). Its "no
PromptReceived row in 30 s" is that run's `--within` default, raised to 120 s since.

Runs not shown: on the first fixed bundle (`dac1a8a01`, before the order fix in `0c1a05ad6`) two
runs ended in `first-run` on an accessibility read deadline (`ax.sh find --title "Add this
company"`, exit 124, host CPU 98%) and one in `return` on the same kind of read (its `click` was
received). A first run of `da423ed82` passed both steps but typed after the conversation had
opened, so it did not test the window; the walk now types straight after leaving the home screen.
The fix runs used `TESTVM_AX_TIMEOUT=35`.

## The step records (fields kept from each `<step>.json`)

`main-a7db7f70b`: identity PASS, first-run PASS, return FAIL, click PASS

```json
{"how": "return", "home_left_after_s": 17.5, "typed_at_ms": 1792517275862, "typed": "First send after a relaunch, sent with the Return key. Reply with one word: received.", "sent_at_ms": 1792517307475, "composer_after": ""}
{"how": "click", "home_left_after_s": 24.6, "typed_at_ms": 1792517404355, "typed": "First send after a relaunch, sent with a click on Send. Reply with one word: received.", "sent_at_ms": 1792517430575, "received": {"turn_id": "turn_0666ea9cdb0e4ab6b9dd65ef00d91073", "thread_id": "thr_f3905e41da7044769690ad31167a197d", "at": 1792517442303}, "composer_after": "", "send_button": {"x": 1143, "y": 878, "w": 41, "h": 47}}
```

`fix-da423ed82`: identity PASS, first-run PASS, return PASS, click PASS

```json
{"how": "return", "home_left_after_s": 29.0, "typed_at_ms": 1792522715780, "typed": "First send after a relaunch, sent with the Return key. Reply with one word: received.", "sent_at_ms": 1792522752416, "shown_after_send": true, "conversation_open_when_typed": false, "received": {"turn_id": "turn_6c06c41146384845a984845968cfc375", "thread_id": "thr_3378e569f7944fbc87f006c434ac291a", "at": 1792522821209, "after_send_s": 68.8}, "composer_after": ""}
{"how": "click", "home_left_after_s": 11.4, "typed_at_ms": 1792522846195, "typed": "First send after a relaunch, sent with a click on Send. Reply with one word: received.", "sent_at_ms": 1792522859154, "shown_after_send": true, "conversation_open_when_typed": true, "received": {"turn_id": "turn_5e91d3f3dc3b4701b52a63d585695b86", "thread_id": "thr_3378e569f7944fbc87f006c434ac291a", "at": 1792522868766, "after_send_s": 9.6}, "composer_after": "", "send_button": {"x": 1143, "y": 878, "w": 41, "h": 47}}
```
