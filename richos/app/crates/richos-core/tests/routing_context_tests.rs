use richos_core::{cognition::MockCognition, entity::EntityId, ledger::{Ledger,Source}};
mod support;

#[test]
fn classification_gets_the_scoped_visible_dialogue_including_actual_replies() {
    let root=std::env::temp_dir().join(format!("richos-routing-context-{}",uuid::Uuid::new_v4()));
    let mut ledger=Ledger::open(root.join("ledger.jsonl")).unwrap();
    let thread=ledger.create_thread("Current",&EntityId::parse("femcboost").unwrap()).unwrap();
    let other=ledger.create_thread("Other company",&EntityId::parse("deeply").unwrap()).unwrap();
    for (target,source,text,answer) in [(&thread,Source::Text,"Compare the proposals","Proposal B needs vendor checks."),
        (&thread,Source::Internal,"PRIVATE preparation","PRIVATE internal reply"),
        (&thread,Source::Agent,"PRIVATE peer-input body",""),
        (&other,Source::Text,"PRIVATE other company","PRIVATE other answer")] {
        let binding=ledger.thread_binding(target).unwrap();
        let turn=ledger.record_prompt_received(&binding,text,source).unwrap();
        ledger.mark_turn_started(&turn,"previous-native").unwrap();
        if !answer.is_empty() {ledger.append_assistant_delta(&turn,answer,0).unwrap();}
        ledger.complete_turn(&turn,"end_turn").unwrap();
    }
    let mock=MockCognition::new("current-native",vec!["I will carry that through."]);
    let contexts=mock.conversation_contexts.clone();
    let mut spine=support::spine(ledger);spine.switch_thread(&thread).unwrap();
    spine.attach_lease(Box::new(mock));spine.submit_prompt("Go ahead",Source::Text).unwrap();
    let contexts=contexts.lock().unwrap();let context=contexts.last().unwrap();
    assert!(context.contains("Compare the proposals"));assert!(context.contains("Proposal B needs vendor checks."));
    assert!(!context.contains("PRIVATE"));assert!(!context.contains("Go ahead"),"the unchanged original request is supplied separately");
    drop(contexts);drop(spine);std::fs::remove_dir_all(root).unwrap();
}
