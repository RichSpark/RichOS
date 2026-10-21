# Failure class: lost input reported without a next step

**The class.** RichOS tells someone that what they said or typed did not get through ("it
didn't reach me", "I haven't sent anything", "your answer was not sent") and stops there. The
sentence is true and calm and it is a dead end: the reader learns about a loss and is given
nothing to do about it, so the natural reading is that the app is broken.

**The check that guards it:** the check "every line saying his words did not get through says
what to do next" in `richos/app/ui/tests/affordances.js`. It runs over the state inventory that
`lib/state-strings.js` scrapes from the shipped source every run (every UI file, the Tauri
command layer and every `ceo_message()`), so a new sentence of this kind cannot be added
without meeting it. Every line matching `LOST_INPUT` (`haven't sent anything`, `didn't reach
me`, `answer/message/words was/were not sent`) must be classified ACTIONABLE in
`lib/state-registry.js`, which makes the suite's PART 3 prove its control is on screen in its
fixture and, when it is a whole sentence, must say the step in words (`NEXT_STEP`: say it
again, send it again, try again, copy it, press, tap, click, retry). A lead-in ending in ":"
introduces the recovery drawn right after it. The check carries its own positive control: the
incident's sentence must still be caught.

## The incident that named it (2026-10-21, RichOS 1.2.1)

A new user in voice mode had an answer fail. RichOS spoke its cut-off notice through the Mac's
speakers ("I was cut off partway through that answer...") and then showed:

> While I was speaking, I couldn't tell your voice from my own, so if you said something just
> then, it didn't reach me and I haven't sent anything. I'm listening now.

What made the app say it (`richos-voice` `controller.rs`): while Rich is audible the
microphone stays open and the capture brain marks any utterance recorded while Rich is
audible as tainted unless the echo canceller is confident or a barge-in fired
(`push_residual`). On ordinary Mac speakers the canceller cannot become confident (measured on
2026-09-17, `docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`), so Rich's own voice
reaching the microphone is enough to open an utterance, the utterance is discarded
(`CapMsg::Discarded { tainted: true }`) and `supervise` raises this notice once per voice
session (`RefusalNotices::half_duplex`). Rich was speaking at that moment: the notice sits right
after the spoken cut-off notice in the conversation. Whether the person also spoke cannot be
known on this path, which is why every clause about him is conditional. The sentence then
ended with a status, a deliberate earlier choice, so it explained a loss and gave no step.

The fix: the sentence now ends "If you did, please say it again now. I'm listening." and
it is shown only once Rich is no longer audible (`HalfDuplexDelivery`): shown at the discard,
it invited a reply into the window that had just lost one (the first discard on the
committed recording comes 2.592 s into an answer that is audible until 24.960 s). A next step
the user cannot take yet is a dead end of the same kind; test
`the_half_duplex_line_is_shown_only_once_rich_can_take_the_reply`. Its two
siblings in the same family got the same treatment, because the check found them:
`HeardNoVoice` ("...If you said something, please say it again. I'm still listening.") and
`DidNotCatchThat` ("...Please say it again. I'm still listening."). The open microphone is the
control (`#talk-toggle`, on while voice is on). Tests:
`the_half_duplex_line_states_the_consequence_and_what_to_do`,
`the_two_refusal_paths_do_not_say_the_same_thing`, `the_two_voice_notices_say_different_things`
(`richos-voice` `controller.rs`) and the check above.

## Every member of the class in the app, as of 2026-10-21

| Line | Where | Next step |
| --- | --- | --- |
| "While I was speaking, I couldn't tell your voice from my own..." | `VoiceNotice::CouldNotListenWhileSpeaking` | "If you did, please say it again now." (fixed) |
| "That didn't come through as speech, so I haven't sent anything..." | `VoiceNotice::HeardNoVoice` | "If you said something, please say it again." (fixed) |
| "I didn't catch that, so I haven't sent anything..." | `VoiceNotice::DidNotCatchThat` | "Please say it again." (fixed) |
| "This conversation was deleted. Your answer was not sent. You can copy it below." | question card | copy it, with the recovery control |
| "This conversation was deleted. This answer was not sent:" | question card | lead-in to the copyable answer and its control |

Lines that report something not sent BY CHOICE ("Canceled. Nothing was sent.") or a status
("Not sent yet") are not losses and are outside the class.

## A known limit, stated plainly

On speakers, the half-duplex notice is raised by Rich's own echo whether or not the person
spoke. Measured on 2026-10-21 by `richs_own_echo_raises_the_half_duplex_line_and_it_says_what_to_do`
(`richos-voice/tests/barge_in_on_the_ceos_rig.rs`) over the committed echo-only recordings,
nobody speaking: the 2026-09-18 recording (output volume 60) raises it at all six residual
levels, the 2026-09-17 recording at none, so 6 of 12 conditions. Its companion
`a_short_word_of_his_over_rich_is_lost_and_the_line_tells_him_to_say_it_again` sums 0.192 s of a
real recorded voice into the quiet recording while Rich speaks: too short to stop him, the word
is discarded and the line tells him to say it again. The sentence is conditional for that reason and now says what to do if he did speak.
Raising it only when the canceller's near-end verdict shows more than echo would make it rarer
and is a separate, measured change on the committed echo-path fixtures; it is not part of this
fix.
