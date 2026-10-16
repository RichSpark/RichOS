#!/usr/bin/env python3
"""Mutation checks for the operator back end's app side: one mutant per rule, each must turn
its named test red (operator back-end spec r3 §6 verification 2, r4 §5's added mutant).

  operator-mutations.py [--only ID[,ID...]] [--list]

Each mutant replaces ONE exact snippet in one file of richos-core (the snippet must occur
exactly once, or the harness refuses: a mutant that does not apply proves nothing), runs
`cargo test -p richos-core --lib -- <its tests>`, and restores the file byte for byte however
the run ends. A test named `test:<file>::<function>` is an integration test in
`crates/richos-core/tests/<file>.rs`, run as `cargo test -p richos-core --test <file> --
<function>` (the spine's rule is held by one). A mutant is PROVEN when cargo fails, SURVIVED when the tests still pass (the rule
is not held by any test), and BROKEN when the build itself failed to start. First the same
tests run on the unmutated tree: if they fail there, nothing below it can be read.

Run it through the committed admission, from richos/app:
  scripts/testvm/reserve.py --wait 1800 -- python3 scripts/operator-mutations.py
with CARGO_TARGET_DIR set to the shared target (host CPU policy, CEO ruling §77).
Exit: 0 every mutant proven; 1 any survived or broke; 2 the baseline failed or a mutant did not
apply.
"""
import argparse
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

APP = Path(__file__).resolve().parents[1]
SRC = APP / 'crates' / 'richos-core' / 'src'

# (id, file, the exact snippet, its replacement, the tests that must go red, the rule)
MUTANTS = [
    ('lead-affordance', 'operator_lead.rs',
     '"request": {"subtype": "initialize", "hooks": {}, "perTaskStopAffordance": true}',
     '"request": {"subtype": "initialize", "hooks": {}}',
     ['operator_lead::tests::the_handshake_declares_the_per_task_stop_affordance',
      'operator_lead::tests::the_bytes_on_the_wire_declare_the_affordance_and_carry_no_priority'],
     'r4 §1.2 and §5: the handshake declares perTaskStopAffordance'),
    ('lead-priority', 'operator_lead.rs',
     'json!({"type": "user", "uuid": uuid,',
     'json!({"type": "user", "uuid": uuid, "priority": "now",',
     ['operator_lead::tests::nothing_the_client_writes_ever_carries_priority'],
     'r4 §1.3: nothing written to a lead carries priority'),
    ('lead-close-input', 'operator_lead.rs',
     '        if alive > 0 {\n            return Err(LeadError::AgentsAlive(alive));\n        }',
     '',
     ['operator_lead::tests::input_is_never_closed_while_an_agent_is_alive'],
     'r4 §1.2: input is never closed while an agent is alive'),
    ('lead-sigterm-first', 'operator_lead.rs',
     '        terminate_one(self.pid);\n        while began.elapsed() < grace {',
     '        fence_group(self.pid);\n        while began.elapsed() < grace {',
     ['operator_lead::tests::quit_sends_sigterm_first_and_kills_the_group_only_after_the_grace'],
     'r3 (q) item 2: quit is SIGTERM to the supervisor first'),
    ('lead-keep-on-error', 'operator_lead.rs',
     '                sink.event(LeadEvent::Protocol(format!("a line that is not JSON was skipped ({e})")));\n                continue;',
     '                sink.event(LeadEvent::Protocol(format!("a line that is not JSON was skipped ({e})")));\n                break;',
     ['operator_lead::tests::a_bad_frame_is_reported_and_the_lead_is_kept'],
     'r3 (q) item 1: a bad frame is reported and the lead is kept'),
    ('host-origin-rule', 'operator_host.rs',
     '        if !self.declaration.origins.iter().any(|o| o == declared) {',
     '        if false && !self.declaration.origins.iter().any(|o| o == declared) {',
     ['operator_host::tests::a_phone_assignment_is_not_relayed_and_a_turn_with_no_origin_relays_nothing'],
     '(s) rule 1: an assignment from an undeclared origin is not relayed'),
    ('host-no-origin', 'operator_host.rs',
     '            Self::NoOrigin => None,',
     '            Self::NoOrigin => Some("desk-typed"),',
     ['operator_host::tests::a_phone_assignment_is_not_relayed_and_a_turn_with_no_origin_relays_nothing'],
     '(s) rule 4: a turn with no origin relays nothing'),
    ('host-b5-edge', 'operator_host.rs',
     'let deliver = end.text.filter(|t| last_report.as_deref().map(str::trim) != Some(t.trim()));',
     'let deliver = end.text.filter(|_| last_report.is_none());',
     ['operator_host::tests::a_turn_s_final_text_is_delivered_unless_it_is_the_last_report_s_text'],
     '(c) B5 edge: a turn that reports and then says more delivers both'),
    ('host-unconfirmed-land', 'operator_host.rs',
     '        if !failed && record.lands.iter().any(|l| !l.landed) {',
     '        if false && !failed && record.lands.iter().any(|l| !l.landed) {',
     ['operator_host::tests::an_outcome_naming_an_unconfirmed_land_closes_nothing_and_the_corrected_report_closes_it'],
     '(c), Frank Q3 (c): an outcome naming a land Git could not confirm closes nothing'),
    ('host-already-closed', 'operator_host.rs',
     '        if matches!(item.state, AssignmentState::Settled | AssignmentState::Failed) {',
     '        if false && matches!(item.state, AssignmentState::Settled | AssignmentState::Failed) {',
     ['operator_host::tests::a_report_on_an_assignment_already_closed_says_so_and_never_that_it_stays_open'],
     'Frank F5: a report on a closed assignment is never "It stays open"'),
    ('host-engine-not-open', 'operator_host.rs',
     '            Err(why) if why.contains(ENGINE_NOT_OPEN) => {',
     '            Err(why) if false && why.contains(ENGINE_NOT_OPEN) => {',
     ['operator_host::tests::a_report_on_an_assignment_already_closed_says_so_and_never_that_it_stays_open'],
     'Frank F5: the engine\'s "not open" refusal is said as already closed'),
    ('host-failed-no-land', 'operator_host.rs',
     'let mut evidence: Vec<String> = if failed { Vec::new() } else {',
     'let mut evidence: Vec<String> = if false { Vec::new() } else {',
     ['operator_host::tests::a_failed_report_that_names_a_land_closes_as_withdrawn_on_its_answer_alone'],
     '(c): a failure closes on its answer alone, never on a land'),
    ('report-stays-open', 'operator_report.rs',
     '    if record.kind == "outcome" && record.handle.is_some() && record.lands.iter().any(|l| !l.landed) {',
     '    if false && record.kind == "outcome" && record.handle.is_some() && record.lands.iter().any(|l| !l.landed) {',
     ['operator_report::tests::an_outcome_on_a_handle_with_an_unconfirmed_land_tells_the_lead_it_stays_open'],
     'Frank Q3 (c): the lead is told an unconfirmed land keeps the assignment open'),
    ('lead-permission-refusal', 'operator_lead.rs',
     'Some("can_use_tool") => Ok(json!({"behavior": "deny", "message": PERMISSION_REFUSED})),',
     'Some("can_use_tool") => Ok(json!({"behavior": "deny", "message": "Nobody is at the Mac to approve this. Use richos_operator.report with kind question to ask him."})),',
     ['operator_lead::tests::the_permission_refusal_says_nobody_approves_here_and_never_to_ask_him'],
     'Frank F17 / Q3 (a): the refusal never tells the lead to ask him'),
    ('lead-permission-seen', 'operator_lead.rs',
     '                    sink.event(LeadEvent::PermissionAsked(tool.to_string()));',
     '                    let _ = tool;',
     ['operator_lead::tests::a_permission_question_from_the_lead_is_answered_on_the_route'],
     'Frank F17 / Q3 (a): every permission request reaches the host'),
    ('host-permission-warning', 'operator_host.rs',
     '                self.log(&format!("WARNING: permission request for {tool} from {}/{}, refused; P14 measured none in \\\n',
     '                self.log(&format!("permission request for {tool} from {}/{}, refused; P14 measured none in \\\n',
     ['operator_host::tests::a_permission_request_is_logged_as_a_warning_every_time_and_said_once'],
     'Frank F17 / Q3 (a): each arrival is logged as a warning that the CLI changed'),
    ('host-permission-once', 'operator_host.rs',
     '                if !c.told_permission {',
     '                if true {',
     ['operator_host::tests::a_permission_request_is_logged_as_a_warning_every_time_and_said_once'],
     'Frank F17 / Q3 (a): he hears it once per conversation'),
    ('host-stop-holds', 'operator_host.rs',
     '        self.hold(key, handle, Some(&measured));',
     '        let _ = &measured;',
     ['operator_host::tests::the_assignment_stop_tells_the_lead_at_once_and_a_resume_never_calls_it_open'],
     'Frank F8: the per-assignment Stop tells the lead and holds the handle'),
    ('host-esc-holds', 'operator_host.rs',
     '            self.hold(key, &handle, None);',
     '            let _ = handle;',
     ['operator_host::tests::his_esc_holds_the_turn_s_assignment_and_the_next_message_names_it_once'],
     'Frank F8: his Esc holds the assignment whose turn it ended'),
    ('host-resume-names-held', 'operator_host.rs',
     '                lines.push(held_line(&c.record.held));\n',
     '',
     ['operator_host::tests::the_assignment_stop_tells_the_lead_at_once_and_a_resume_never_calls_it_open'],
     'Frank F8: a resume names the held handles'),
    ('host-untold-named', 'operator_host.rs',
     '            lines.push(held_line(&c.record.untold));\n',
     '',
     ['operator_host::tests::his_esc_holds_the_turn_s_assignment_and_the_next_message_names_it_once'],
     'Frank F8: the next message names what the lead was not yet told'),
    ('host-untold-once', 'operator_host.rs',
     '        c.record.untold.clear();\n',
     '',
     ['operator_host::tests::his_esc_holds_the_turn_s_assignment_and_the_next_message_names_it_once'],
     'Frank F8: a held handle is named once'),
    ('host-words-release', 'operator_host.rs',
     '            c.record.held.remove(h);\n',
     '',
     ['operator_host::tests::his_esc_holds_the_turn_s_assignment_and_the_next_message_names_it_once'],
     'Frank F8: his words on a held handle release it'),
    ('host-record-compatible', 'operator_host.rs',
     '    #[serde(default)]\n    held: BTreeSet<String>,',
     '    held: BTreeSet<String>,',
     ['operator_host::tests::a_lead_record_written_before_held_handles_existed_still_resumes'],
     'a lead record written before held handles still resumes'),
    ('host-report-mid-turn', 'operator_host.rs',
     '            LeadEvent::Reported => self.take_reports(&conversation),',
     '            LeadEvent::Reported => {}',
     ['operator_host::tests::a_question_asked_mid_turn_reaches_him_before_the_turn_ends'],
     '(c): a report mid-turn reaches him as it arrives, not at the turn\'s end'),
    ('host-registry-needs-stopped', 'operator_host.rs',
     '            if stopped {\n                break;\n            }',
     '            if stopped || true {\n                break;\n            }',
     ['operator_host::tests::no_registry_step_without_the_stream_saying_stopped'],
     'r4 §2.1: the registry is told only after the stream says stopped'),
    ('host-liveness-by-name', 'operator_host.rs',
     '                match self.engine.liveness(&task_id) {',
     '                match self.engine.liveness(name) {',
     ['operator_host::tests::a_named_stop_stops_that_task_tells_the_registry_and_leaves_the_other'],
     'liveness is asked by agent id, never by name (measured 2026-09-25)'),
    ('host-stop-every-channel', 'operator_host.rs',
     '        if names.len() <= 1 {\n            return names.iter()',
     '        if origin == Origin::Phone { return Vec::new(); }\n        if names.len() <= 1 {\n            return names.iter()',
     ['operator_host::tests::a_named_stop_stops_that_task_tells_the_registry_and_leaves_the_other'],
     '(d) item 7: a stop is accepted from every channel'),
    ('host-alarm-once', 'operator_host.rs',
     '        if self.alarms.lock().unwrap().admit(&alarm, Instant::now()) {',
     '        if self.alarms.lock().unwrap().admit(&alarm, Instant::now()) || true {',
     ['operator_host::tests::an_alarm_from_three_leads_reaches_him_once'],
     '(r): an alarm reaches him once across every lead'),
    ('host-idle-needs-snapshot', 'operator_host.rs',
     '            if agents_busy || descendants != Some(0) || lease {',
     '            if agents_busy || descendants.unwrap_or(0) != 0 || lease {',
     ['operator_host::tests::idle_retirement_waits_for_nothing_running_and_the_next_message_resumes'],
     '(q) item 4: no supervisor snapshot is never "nothing running"'),
    # His answers to his team, crash-safe (richos-hq docs/plans/2026-09-27-answer-delivery-crash-dedup-design.md §2.2).
    ('host-answer-once', 'operator_host.rs',
     '            if relay.taken {',
     '            if false && relay.taken {',
     ['operator_host::tests::an_answer_is_taken_only_when_the_cli_echoes_its_uuid'],
     '§88 seam and design §2.2: an answer his team has taken is never sent again, across a relaunch'),
    ('host-answer-in-flight', 'operator_host.rs',
     '            if c.awaiting.contains(&relay.uuid) {',
     '            if false && c.awaiting.contains(&relay.uuid) {',
     ['operator_host::tests::an_answer_in_flight_is_never_sent_twice_by_one_process',
      'operator_host::tests::a_question_goes_to_the_sink_and_an_answer_reaches_the_lead_once_from_any_channel'],
     'design §2.2: nothing is sent while the answer\'s uuid is awaited'),
    ('host-answer-same-uuid', 'operator_host.rs',
     '            Some(relay) if relay.session == session => relay.uuid.clone(),',
     '            Some(relay) if false && relay.session == session => relay.uuid.clone(),',
     ['operator_host::tests::after_a_crash_before_the_echo_the_answer_is_resent_under_its_first_uuid_with_the_note'],
     'design §2.2, C3: a retry into the same session reuses the first uuid, so the provider can drop it'),
    ('host-answer-intent-first', 'operator_host.rs',
     '        if let Err(e) = write_record(&c.paths.record, &c.record) {\n            c.record.answers = before;',
     '        if let Err(e) = Ok::<(), String>(()) {\n            c.record.answers = before;',
     ['operator_host::tests::after_a_crash_before_the_echo_the_answer_is_resent_under_its_first_uuid_with_the_note',
      'operator_host::tests::an_intent_that_could_not_be_saved_sends_nothing'],
     'design §2.2, C4: the intent is on disk before any byte is sent'),
    ('host-answer-taken-on-echo', 'operator_host.rs',
     '        let relay = c.record.answers.iter_mut().find(|a| a.uuid == uuid && !a.taken)?;',
     '        let relay = c.record.answers.iter_mut().find(|a| a.uuid == uuid && !a.taken && false)?;',
     ['operator_host::tests::an_answer_is_taken_only_when_the_cli_echoes_its_uuid'],
     'design §2.2, C1: the CLI\'s echo is what makes an answer taken'),
    ('host-ended-releases', 'operator_host.rs',
     '            c.awaiting.clear();\n            c.in_turn = false;',
     '            c.in_turn = false;',
     ['operator_host::tests::the_lead_s_own_crash_releases_its_queue_and_the_answer_is_resent_once',
      'operator_desk::tests::the_idle_timer_resends_an_answer_his_team_s_crash_lost'],
     'design C6: an ended lead holds nothing, so an answer it had queued is resent'),
    ('host-retry-starts-bounded', 'operator_host.rs',
     '        if prior.is_some() && starting && c.retry_starts >= RETRY_STARTS {',
     '        if false && prior.is_some() && starting && c.retry_starts >= RETRY_STARTS {',
     ['operator_host::tests::a_lead_that_keeps_ending_is_started_for_an_answer_at_most_twice'],
     'a lead that ends every time it starts is started for an answer at most twice with no turn completing'),
    ('lead-drop-needs-no-start', 'operator_lead.rs',
     '            Some("completed") if !self.started.contains(uuid) && self.taken.iter().any(|u| u == uuid) => {',
     '            Some("completed") if self.taken.iter().any(|u| u == uuid) => {',
     ['operator_lead::tests::a_duplicate_the_cli_drops_is_told_apart_from_a_message_it_runs'],
     'C5 (P18): only a completed with no started is a drop; a message the CLI ran is never one'),
    ('host-dropped-releases', 'operator_host.rs',
     '                if c.opened_by.as_deref() == Some(uuid.as_str()) {',
     '                if false && c.opened_by.as_deref() == Some(uuid.as_str()) {',
     ['operator_host::tests::a_dropped_duplicate_leaves_the_conversation_idle_and_taken'],
     'C5 (P18): a dropped duplicate leaves the conversation idle, never stuck working'),
    # The crash that kills the turn which took his answer (crash matrix cell W3): continued once.
    ('host-taken-opens-its-turn', 'operator_host.rs',
     '        relay.turn_open = true;\n',
     '',
     ['operator_host::tests::a_turn_killed_after_taking_his_answer_is_continued_once_at_relaunch'],
     'W3: an answer taken into a turn is recorded, in the same save, as a turn not yet seen to end'),
    ('host-continue-not-while-a-turn-runs', 'operator_host.rs',
     '        if c.in_turn || !c.awaiting.is_empty() || !self.release_held_turns(c) {',
     '        if !self.release_held_turns(c) {',
     ['operator_host::tests::a_turn_killed_after_taking_his_answer_is_continued_once_at_relaunch'],
     'W3: a turn running (or a message waiting for one) in this process is never continued, and never twice'),
    ('host-continue-once', 'operator_host.rs',
     '            if relay.continued.as_ref().is_some_and(|n| n.uuid == uuid) {',
     '            if false && relay.continued.as_ref().is_some_and(|n| n.uuid == uuid) {',
     ['operator_host::tests::a_turn_killed_after_taking_his_answer_is_continued_once_at_relaunch'],
     'W3: once the lead took the continuation, no later launch continues that turn again'),
    ('host-continue-intent-first', 'operator_host.rs',
     '        let intent = write_record(&c.paths.record, &c.record);',
     '        let intent = Ok::<(), String>(());',
     ['operator_host::tests::a_continuation_lost_with_the_app_is_resent_under_its_first_uuid',
      'operator_host::tests::a_turn_killed_after_taking_his_answer_is_continued_once_at_relaunch'],
     'W3: the continuation\'s intent is on disk before it is sent'),
    ('host-continue-same-uuid', 'operator_host.rs',
     '            Some(earlier) if earlier.session == session => earlier.uuid.clone(),',
     '            Some(earlier) if false && earlier.session == session => earlier.uuid.clone(),',
     ['operator_host::tests::a_continuation_lost_with_the_app_is_resent_under_its_first_uuid'],
     'W3: a continuation lost with the app is resent under its first uuid, so the provider can drop it'),
    ('host-turn-end-closes', 'operator_host.rs',
     '        // A turn completed after every answer it had taken: the lead has acted on them.\n        self.close_answer_turns(&mut c);',
     '',
     ['operator_host::tests::a_turn_that_ended_or_that_he_ended_is_never_continued'],
     'W3: a turn that completed after the answer leaves nothing to continue'),
    ('host-esc-closes', 'operator_host.rs',
     '                self.close_answer_turns(&mut conversation.lock().unwrap());',
     '                drop(conversation.lock().unwrap());',
     ['operator_host::tests::a_turn_that_ended_or_that_he_ended_is_never_continued'],
     'W3: a turn he ended with his Esc is not carried on without his words'),
    ('host-ended-closes', 'operator_host.rs',
     '        // A turn completed after every answer it had taken: the lead has acted on them.\n        self.close_answer_turns(&mut c);',
     '',
     ['operator_host::tests::a_turn_that_ended_or_that_he_ended_is_never_continued'],
     'W3: a turn that completed after the answers it took closes them, so it is not continued'),
    ('host-quit-closes', 'operator_host.rs',
     '            if c.in_turn || !c.awaiting.is_empty() {\n                self.close_answer_turns(&mut c);',
     '            if false {\n                self.close_answer_turns(&mut c);',
     ['operator_host::tests::a_turn_that_ended_or_that_he_ended_is_never_continued'],
     'W3: a quit he chose while his team worked stops that work; the next launch does not carry it on'),
    ('host-held-not-continued', 'operator_host.rs',
     '            if relay.answer_to.as_ref().is_some_and(|h| held.contains(h)) {',
     '            if false && relay.answer_to.as_ref().is_some_and(|h| held.contains(h)) {',
     ['operator_host::tests::a_turn_that_ended_or_that_he_ended_is_never_continued'],
     'W3 and F8: an answer on a handle he stopped is not carried on; his next words decide'),
    ('host-continue-bounded', 'operator_host.rs',
     '        if starting && c.retry_starts >= RETRY_STARTS {',
     '        if false && starting && c.retry_starts >= RETRY_STARTS {',
     ['operator_host::tests::a_continuation_never_starts_a_lead_past_the_retry_bound'],
     'W3: the continuation\'s lead start counts against the retry bound'),
    ('host-dropped-answer-continued', 'operator_host.rs',
     '(answer, c.in_turn, c.awaiting.is_empty(), lead)',
     '(false, c.in_turn, c.awaiting.is_empty(), lead)',
     ['operator_host::tests::a_dropped_resend_of_his_answer_is_continued_at_once'],
     'W3 via P18 case B: a resent answer the lead drops is continued at once, once'),
    ('desk-flush-continues', 'operator_desk.rs',
     '        self.host.continue_interrupted_answers(&|thread| self.origins.title(thread).unwrap_or_default());\n        flushed',
     '        flushed',
     ['operator_desk::tests::an_answer_survives_every_crash_window_and_reaches_his_team_once'],
     'W3: the launch\'s answer flush continues a turn that died with the app, with no words from him'),
    ('desk-inbox-kept-until-taken', 'operator_desk.rs',
     '                Ok(AnswerOutcome::InFlight) => {}',
     '                Ok(AnswerOutcome::InFlight) => { drop(crate::question_work::acknowledge(&self.state_root, &delivery.id)); }',
     ['operator_desk::tests::an_answer_survives_every_crash_window_and_reaches_his_team_once'],
     'design §2.2, W1b/W2/W4: the inbox lets an answer go only once his team has it'),
    ('host-resume', 'operator_host.rs',
     '            Some(session) => LeadStart::Resume(session.clone()),',
     '            Some(_session) => LeadStart::New(uuid::Uuid::new_v4().to_string()),',
     ['operator_host::tests::after_a_relaunch_the_first_message_resumes_and_lists_the_open_handles'],
     '(l): a lead is resumed, never replaced by a new session'),
    ('claim-sdk-not-terminal', 'operator_claim.rs',
     '    if entry.is_empty() || entry.starts_with("sdk-") {',
     '    if entry.is_empty() {',
     ['operator_claim::tests::the_terminal_test_is_g11_s_and_never_kind'],
     'F6/G11: a print-mode session is never the terminal'),
    ('claim-unreadable-held', 'operator_claim.rs',
     '            ClaimState::Unreadable(why) => return Err(ClaimRefusal(unreadable_sentence(place, why))),',
     '            ClaimState::Unreadable(_) => {}',
     ['operator_claim::tests::a_live_terminal_claim_refuses_and_an_unreadable_one_is_held_with_the_way_through'],
     '(e) item 6: unreadable is held'),
    ('claim-start-time', 'operator_claim.rs',
     '            (Some(recorded), Some(kernel)) => recorded.abs_diff(kernel) <= 1,',
     '            (Some(_), Some(_)) => true,',
     ['operator_claim::tests::a_live_terminal_refuses_the_app_and_an_ended_one_does_not'],
     '(e) item 1: a recycled pid is never his terminal'),
    ('gate-team-unknown', 'work_gate.rs',
     '        return (Liveness::Unknown, Some("RichOS could not tell whether your team had finished.".into()));',
     '        return (Liveness::Clear, None);',
     ['work_gate::tests::what_cannot_be_decided_waits_and_only_a_read_clear_team_is_clear'],
     '(m): what cannot be decided waits'),
    ('profile-artifact', 'operator_profile.rs',
     '        environment.insert(ARTIFACT_ENV.0.into(), ARTIFACT_ENV.1.into());\n',
     '',
     ['operator_profile::tests::the_environment_is_exactly_the_allowlist_plus_the_four_the_app_supplies'],
     'r4 §2.3: CLAUDE_CODE_ARTIFACT=1 once P17 passed'),
    # ---- the shell wiring (echo-opus-opshell1, the record's §7 items 1-3) ----
    ('spine-keeps-mouth', 'spine.rs',
     '        if self.keep_intake_channel {',
     '        if false && self.keep_intake_channel {',
     ['test:phone_intake_tests::a_keeping_spine_records_every_prompt_s_mouth_and_a_product_spine_records_none'],
     'item 3: an operator install keeps the mouth on the prompt record'),
    ('origin-unrecorded', 'operator_host.rs',
     '            (Source::Internal | Source::Proactive | Source::Agent, _) | (_, None) => Origin::NoOrigin,',
     '            (Source::Internal | Source::Proactive | Source::Agent, _) => Origin::NoOrigin,\n            (Source::Text | Source::Jam, None) => Origin::DeskTyped,',
     ['operator_host::tests::a_turn_s_origin_is_its_recorded_mouth_and_an_unrecorded_one_is_no_origin',
      'operator_desk::tests::an_assignment_whose_mouth_was_never_recorded_is_not_handed_over'],
     'item 3: an unrecorded mouth is no origin, never the desk'),
    ('origin-phone', 'operator_host.rs',
     '            (_, Some("phone" | "phone_typed" | "phone_voice")) => Origin::Phone,',
     '            (_, Some("phone" | "phone_typed" | "phone_voice")) => Origin::DeskTyped,',
     ['operator_host::tests::a_turn_s_origin_is_its_recorded_mouth_and_an_unrecorded_one_is_no_origin',
      'operator_desk::tests::a_phone_assignment_is_closed_with_the_sentence_and_its_obligation_withdrawn'],
     '(s) rule 1: phone words are the phone, and never become work'),
    ('lead-taking', 'operator_lead.rs',
     '(self.sent.contains(uuid) && !self.taken.iter().any(|u| u == uuid)).then(|| uuid.to_string())',
     '(false && self.sent.contains(uuid) && !self.taken.iter().any(|u| u == uuid)).then(|| uuid.to_string())',
     ['operator_lead::tests::the_moment_the_cli_takes_a_message_is_seen_before_the_turn_ends_and_only_once'],
     '(d) item 5: the moment the CLI takes a message is seen while the turn runs'),
    ('host-took-handle', 'operator_host.rs',
     '                    c.turn_handle = c.sent.get(&uuid).cloned().flatten();',
     '                    c.turn_handle = None;',
     ['operator_host::tests::an_agent_started_in_a_turn_the_cli_took_on_a_handle_is_that_assignment_s',
      'operator_desk::tests::the_assignment_stop_stops_its_agents_says_so_and_marks_it_stopped_only_when_all_are_gone'],
     '(d) item 5: an agent started on a handle\'s turn is that assignment\'s'),
    ('host-working', 'operator_host.rs',
     '            if busy && !lead.exited() {',
     '            if false && busy && !lead.exited() {',
     ['operator_host::tests::a_lead_in_its_turn_counts_as_working_until_the_turn_ends'],
     '(m) gap: a lead in its turn is his team working'),
    ('host-stream-no-script', 'operator_host.rs',
     '        self.team_reading(|agent| if agent.status.is_running() { AgentLiveness::Alive } else { AgentLiveness::NotAlive })',
     '        self.team_reading(|agent| self.engine.liveness(&agent.task_id))',
     ['operator_host::tests::the_stream_reading_asks_no_script_and_counts_what_the_stream_has_not_seen_end'],
     '§2.5a: the exit callback runs no script'),
    ('gate-working', 'work_gate.rs',
     '    if !team.working.is_empty() {',
     '    if false && !team.working.is_empty() {',
     ['work_gate::tests::a_lead_in_its_turn_blocks_with_no_agent_and_no_command_running'],
     '(m) gap: a lead in its turn blocks the update and keeps the app'),
    ('settle-seat', 'operator_runtime.rs',
     '        match call(command, crate::ecs::seated_request(seat.as_deref(), body(&binding))) {',
     '        match call(command, crate::ecs::seated_request(None, body(&binding))) {',
     ['operator_runtime::tests::the_settlement_names_the_conversation_s_seat_and_closes_at_its_live_binding'],
     'contract §2.5: the close is on the conversation\'s own seat'),
    ('settle-retry-once', 'operator_runtime.rs',
     '            Err(why) if attempt == 0 && why.contains("stale app binding") => continue,\n',
     '',
     ['operator_runtime::tests::a_rebind_between_the_read_and_the_close_is_retried_once_at_the_new_binding'],
     'a rebind between the read and the close is retried once'),
    ('desk-attested', 'operator_desk.rs',
     'self.origins.turn(&record.instruction_ledger_ref).filter(|f| attested(record, f)) else {',
     'self.origins.turn(&record.instruction_ledger_ref) else {',
     ['operator_desk::tests::words_the_register_did_not_attest_or_a_turn_the_ledger_lacks_are_never_relayed'],
     'item 1: words the register did not attest never reach his team'),
    ('desk-title-label', 'operator_desk.rs',
     'format!("{TITLE_LABEL} {}\\n\\nHis words, exactly as he said them:\\n{}"',
     'format!("{}\\n\\nHis words, exactly as he said them:\\n{}"',
     ['operator_desk::tests::an_assignment_given_at_the_desk_goes_to_the_lead_with_its_handle_and_his_words'],
     'Frank F1: the title line says it is the front desk\'s label, not his words'),
    ('desk-gate-per-assignment', 'operator_desk.rs',
     '        if let Some(sentence) = self.gate_refusal() {',
     '        if let Some(sentence) = None::<String> {',
     ['operator_desk::tests::a_gate_that_changed_broke_or_went_away_since_launch_refuses_and_never_falls_back'],
     '(f), B8: the gate is read per assignment and never falls back'),
    ('desk-withdraws', 'operator_desk.rs',
     '        if let Err(why) = self.settle.complete(&key_of(record), &record.obligation_id, &format!("operator-refused:{}", record.id),\n                                               ECS_WITHDRAWN, &evidence, sentence) {',
     '        if let Err(why) = Err::<(), String>(format!("{:?}", (&evidence, ECS_WITHDRAWN))) {',
     ['operator_desk::tests::a_phone_assignment_is_closed_with_the_sentence_and_its_obligation_withdrawn'],
     'item 1: an assignment not handed over has its obligation withdrawn'),
    ('desk-stop-all-gone', 'operator_desk.rs',
     'if results.iter().all(|r| matches!(r, StopResult::Stopped { .. })) {\n            if let Err(e) = assignment::advance',
     'if true {\n            if let Err(e) = assignment::advance',
     ['operator_desk::tests::a_stop_that_reached_only_some_of_its_agents_leaves_the_assignment_open'],
     '(d) item 5: marked stopped only when every agent is NOT-ALIVE'),
    ('desk-timer-silent', 'operator_desk.rs',
     '        if self.host.running_leads() == 0 {\n            return Vec::new();\n        }\n',
     '',
     ['operator_desk::tests::the_idle_timer_retires_a_lead_with_nothing_running_and_spends_nothing_with_no_lead'],
     '(q) item 4: the idle timer runs no script while no lead runs'),
    ('tools-token', 'operator_desk_tools.rs',
     '    if !desk && !lead_token.is_some_and(|lead| same_token(given, lead)) {',
     '    if false && !desk && !lead_token.is_some_and(|lead| same_token(given, lead)) {',
     ['operator_desk_tools::tests::a_request_without_the_launch_s_token_or_without_names_and_words_changes_nothing'],
     'item 2: the socket answers only the app\'s own scope'),
    ('tools-read-waiting', 'operator_desk_tools.rs',
     'words, the questions it is still waiting on (asked, and no answer relayed yet), and land leases it holds.',
     'words, questions it asked him, and land leases it holds.',
     ['operator_desk_tools::tests::the_read_tool_says_it_lists_the_questions_still_waiting'],
     'Frank F16: the read says it lists only the questions still waiting'),
    ('front-desk-operator-only', 'native.rs',
     '        if profile.and_then(|p| p.operator_desk.as_ref()).is_some() {',
     '        if true {',
     ['native::native_driver_tests::rich_has_the_register_and_full_evidence_while_stu_has_execution',
      'native::native_driver_tests::an_operator_install_gives_the_front_desk_his_team_s_tools_and_the_back_end_none'],
     'N1: no operator tools on a front desk without the desk\'s access'),
    ('work-host-intake', 'work_host.rs',
     '            operator.take(record);\n            return true;',
     '            let _ = operator;',
     ['work_host::tests::an_operator_install_hands_every_assignment_to_his_team_and_opens_no_work_lease'],
     'item 1: an operator assignment never reaches a work lease'),
    ('register-answers-unlisted', 'assignment_tools.rs',
     '        if !origin.declared().is_some_and(|mouth| listed.iter().any(|l| l == mouth)) {',
     '        if false && !origin.declared().is_some_and(|mouth| listed.iter().any(|l| l == mouth)) {',
     ['assignment_tools::tests::on_an_operator_install_the_register_answers_phone_or_unrecorded_work_and_writes_nothing'],
     '(s) rule 1: the front desk answers phone work in his turn, nothing written'),
    ('work-host-register-silent', 'work_host.rs',
     '        if self.operator.lock().unwrap().is_some() {',
     '        if false && self.operator.lock().unwrap().is_some() {',
     ['work_host::tests::an_operator_install_hands_every_assignment_to_his_team_and_opens_no_work_lease'],
     '(m): on an operator install the register does not speak for his team'),
]


def cargo(tests):
    """Run the library tests named, then each integration file's; the first failure decides."""
    lib = [t for t in tests if not t.startswith('test:')]
    files = {}
    for t in tests:
        if t.startswith('test:'):
            file, _, function = t[len('test:'):].partition('::')
            files.setdefault(file, []).append(function)
    commands = []
    if lib:
        commands.append(['cargo', 'test', '-p', 'richos-core', '--lib', '--'] + lib)
    for file, functions in sorted(files.items()):
        commands.append(['cargo', 'test', '-p', 'richos-core', '--test', file, '--'] + functions)
    began = time.monotonic()
    code, out = 0, ''
    for command in commands:
        done = subprocess.run(command, cwd=str(APP), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        out += done.stdout
        if done.returncode != 0:
            code = done.returncode
            break
    return code, out, time.monotonic() - began


def test_exists(test):
    """Does the named test function exist where the name says it is?"""
    if test.startswith('test:'):
        file, _, function = test[len('test:'):].partition('::')
        path = SRC.parent / 'tests' / (file + '.rs')
    else:
        # `<file>::<test module>::<function>`: the first segment names the source file and the
        # last the function, whatever the test module is called (`tests`, `native_driver_tests`).
        parts = test.split('::')
        path, function = SRC / (parts[0] + '.rs'), parts[-1]
    return path.is_file() and ('fn %s(' % function) in path.read_text()


def summary(out):
    lines = [l for l in out.splitlines() if l.startswith('test result:') or 'error[' in l or 'panicked' in l]
    return (lines[-1] if lines else out.strip().splitlines()[-1] if out.strip() else '')[:200]


def main():
    os.environ["PATH"] = str(Path(__file__).resolve().parent / "bin") + os.pathsep + os.environ.get("PATH", "")
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--only', default='')
    ap.add_argument('--list', action='store_true')
    ap.add_argument('--check', action='store_true',
                    help='no cargo: every mutant applies exactly once and names tests that exist')
    a = ap.parse_args()
    wanted = [x for x in a.only.split(',') if x]
    # A name that matches no mutant selected nothing, and "0 of 0 proven" with exit 0 read as a
    # pass. An unknown name is refused before anything runs, whether or not others are valid.
    unknown = [x for x in wanted if x not in [m[0] for m in MUTANTS]]
    if unknown:
        print('NOT A MUTANT: %s (known: %s)' % (', '.join(unknown), ', '.join(m[0] for m in MUTANTS)))
        return 2
    mutants = [m for m in MUTANTS if not wanted or m[0] in wanted]
    if a.list:
        for m in mutants:
            print('%-28s %-22s %s' % (m[0], m[1], m[5]))
        return 0
    for mid, name, old, new, tests, rule in mutants:
        count = (SRC / name).read_text().count(old)
        if count != 1:
            print('DOES NOT APPLY %s: the snippet occurs %d times in %s' % (mid, count, name))
            return 2
        for test in tests:
            if not test_exists(test):
                print('NAMES A MISSING TEST %s: %s' % (mid, test))
                return 2
    if a.check:
        print('%d mutants apply exactly once and name tests that exist' % len(mutants))
        return 0
    tests = sorted(set(t for m in mutants for t in m[4]))
    code, out, seconds = cargo(tests)
    print('baseline (%d tests, %.0f s): exit %d, %s' % (len(tests), seconds, code, summary(out)), flush=True)
    if code != 0:
        print(out[-3000:])
        return 2
    results = []
    current = {}

    def restore(*_):
        for path, text in current.items():
            path.write_text(text)
        current.clear()
        if _:
            raise SystemExit('interrupted; every mutated file was restored')
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, restore)
    for mid, name, old, new, tests, rule in mutants:
        path = SRC / name
        original = path.read_text()
        current[path] = original
        try:
            path.write_text(original.replace(old, new, 1))
            code, out, seconds = cargo(tests)
        finally:
            restore()
        compiled = 'error[' not in out and 'could not compile' not in out
        verdict = 'PROVEN' if code != 0 and compiled else ('BROKEN' if not compiled else 'SURVIVED')
        results.append((mid, verdict))
        print('%-8s %-28s %.0f s  %s  (%s)' % (verdict, mid, seconds, summary(out), rule), flush=True)
    proven = sum(1 for _, v in results if v == 'PROVEN')
    print('%d of %d proven' % (proven, len(results)))
    return 0 if proven == len(results) else 1


if __name__ == '__main__':
    sys.exit(main())
