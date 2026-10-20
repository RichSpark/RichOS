# Two messages sent before the launch opens his conversation (handover review rv-20261020T191206Z-fa2ee4a4-778c)

`richos/app/scripts/testvm/first-send-walk.py --steps identity,first-run,two`, one fresh test
guest through `run-walk.py`, fixture data only, bundle built from `a8a82160c` (ad-hoc signed
development build, version `1.2.2-dev.a8a82160c`). Input is a person's: real keystrokes (System
Events) and the Return key, straight after leaving the home screen. "Open when sent" is read by
OCR off the frame taken right after the second Return: the Output buttons are drawn only once the
launch has opened the conversation.

| Bundle | Step | Open when sent | Result |
|---|---|---|---|
| fix `a8a82160c` | two (Return, Return) | no (`two-sent-at-once.jpg`: no Output button, rail row not selected; both messages on screen, box empty) | both received, in his order: intake 1 then 2, `PromptReceived` 7.5 s and 7.5 s after the second press; one copy of each, Rich working on both (`two-sent.jpg`) |

The run was `TESTVM_AX_TIMEOUT=35 run-walk.py --wait 3000 ...`: identity PASS (0.3 s),
first-run PASS (58.7 s), two PASS (36.5 s). Guest quit, VM stopped and clone deleted by
`run-walk.py` (`clean: app quit, VM stopped, clone deleted, state removed`; slot released after
129 s).

## The step record (fields kept from `two.json`)

```json
{"how": "two", "home_left_after_s": 12.3, "typed_at_ms": 1792526019252, "sent_at_ms": 1792526022514, "conversation_open_when_sent": false, "shown_after_send": [true, true], "received_order": ["Please send the proposal today. Reply with one word: noted.", "Correction: do not send it until I approve. Reply with one word: received."], "received": [{"turn_id": "turn_ddc8a051a3874277b113a54908a9287f", "thread_id": "thr_9a7509ac2e5543498c8ab045b9e66569", "at": 1792526030035, "intake_id": 1}, {"turn_id": "turn_4ad4a34d1236428099ac76c5baea8f20", "thread_id": "thr_9a7509ac2e5543498c8ab045b9e66569", "at": 1792526030048, "intake_id": 2}], "composer_after": ""}
```
