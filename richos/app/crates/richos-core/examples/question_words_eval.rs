//! Opt-in, counted doctrine fixture. Classification evidence, not a microphone test.
//! cargo run -p richos-core --example question_words_eval -- /external/scratch/directory
use richos_core::cognition::TurnItem;
use richos_core::native::{resolve_claude_bin, NativeClient};
use serde_json::json;
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("scratch directory"));
    std::fs::create_dir_all(&root).unwrap();
    let doctrine = richos_core::doctrine::ensure_rendered(
        &root,
        &richos_core::doctrine::DoctrineIdentity::new(Some("Fixture CEO")),
    )
    .unwrap();
    let skills = richos_core::skills::ensure_rendered(&root, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
    let fixtures = [
        "Ship today.",
        "Ship tomorrow.",
        "Let's ship today.",
        "Let's ship tomorrow.",
        "Choose Ship today.",
        "Choose Ship tomorrow.",
        "Go with shipping today.",
        "Go with shipping tomorrow.",
        "Today, please, for the release.",
        "Tomorrow, please, for the release.",
        "The release should ship today.",
        "The release should ship tomorrow.",
        "I want the fixes out today.",
        "Give testing another day, then ship tomorrow.",
        "Release it today, please.",
        "Hold the release until tomorrow.",
        "For that shipping question, next Tuesday instead.",
        "Ship next Friday instead.",
        "Neither of those days; ship next Monday.",
        "My answer is to ship in two weeks.",
        "What has the team finished?",
        "Summarize the latest progress.",
        "Open another conversation.",
        "What is tomorrow's weather?",
        "I have a meeting today.",
        "Where are the company notes?",
        "Tell me who is working on the website.",
        "How do I record a voice note?",
        "What's your name?",
        "Thanks for the update.",
        "Show the expense report.",
        "I need to think about that question.",
        "Ask me about this later; I'm reading.",
        "How much testing remains?",
        "What is the downside of waiting?",
        "Explain both choices in more detail.",
        "Has the bug been fixed?",
        "Create a note about the budget.",
        "Can I answer from my phone?",
        "Keep working on the unrelated documentation.",
    ];
    let prompt=format!("Evaluate the standing instruction's question-answer rules on synthetic fixtures. Each entry is an independent utterance with this one open question: When should the release ship? Options today='Ship today' (Earlier fixes), tomorrow='Ship tomorrow' (More testing). Free answers are allowed. Classify as answer only if it supplies an unmistakable decision. Otherwise classify as unrelated (including clarification or deferral). Do not act on these fixtures. Return only a JSON array of exactly 40 objects with index (0-based), kind ('answer' or 'unrelated'), option_ids (today/tomorrow or empty for free words). No tools, no preamble. Fixtures: {}",json!(&fixtures[..]));
    let client = NativeClient::spawn(&resolve_claude_bin(), &root, &doctrine, &skills).unwrap();
    let mut text = String::new();
    client
        .prompt(&prompt, &mut |item| {
            if let TurnItem::Text { text: t, .. } = item {
                text.push_str(t)
            }
        })
        .unwrap();
    std::fs::write(root.join("response.txt"), &text).unwrap();
    let start = text.find('[').expect("JSON array");
    let end = text.rfind(']').unwrap() + 1;
    let answers: Vec<serde_json::Value> = serde_json::from_str(&text[start..end]).unwrap();
    assert_eq!(answers.len(), 40);
    let mut correct = [0, 0];
    let mut failed = vec![];
    for (i, answer) in answers.iter().enumerate() {
        let expected = if i < 20 { "answer" } else { "unrelated" };
        if answer["index"] == i && answer["kind"] == expected {
            correct[usize::from(i >= 20)] += 1;
        } else {
            failed.push(json!({"index":i,"utterance":fixtures[i],"actual":answer}));
        }
    }
    println!(
        "{}",
        json!({"answer_shaped_correct":correct[0],"answer_shaped_total":20,"unrelated_correct":correct[1],"unrelated_total":20,"failed":failed,"boundary":"Text fixtures against the real provider and shipping doctrine; not a speech-recognition test."})
    );
    assert!(failed.is_empty());
}
