//! AFTER AN UPDATE, A SESSION STARTS IN THE ENGINE THAT IS INSTALLED, AS IT IS.
//!
//! While the app installs its own build's engine, the earlier build's engine serves, and a session
//! never waits for the install (the CEO: users "will not wait"). The earlier engine may name a
//! skill differently: an earlier build's engine ships `audio-transcription` where this build names
//! it `transcribe-audio` (`skills::SKILLS`). The session must neither fail on that nor wait for
//! the install: it starts in the earlier engine with the skills that engine ships.
//!
//! Its own test binary, because `skills::earlier_engine_serves` is process-wide and the crate's
//! unit tests read `verify_present` too.

use std::path::{Path, PathBuf};

use richos_core::skills;
use richos_core::user_settings;

/// The skill names an earlier build's `skills::SKILLS` carried, before `audio-transcription` was
/// renamed `transcribe-audio`.
const EARLIER_ENGINE_SKILLS: [&str; 4] = ["american-english", "bootstrap-interview", "watch-video", "audio-transcription"];

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "richos-earlier-engine-{name}-{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The engine plugin (`<engine>/app`) as that earlier build shipped it.
fn earlier_engine_plugin(root: &Path) -> PathBuf {
    let plugin = root.join("engine/app");
    write(&plugin.join(".claude-plugin/plugin.json"), "{\"name\":\"rich-skills\"}");
    write(&plugin.join("agents/rich.md"), "---\nname: rich\n---\nRich, as the earlier build shipped him.\n");
    write(&plugin.join("agents/stu.md"), "---\nname: stu\n---\nStu, as the earlier build shipped him.\n");
    for name in EARLIER_ENGINE_SKILLS {
        write(&skills::skill_path(&plugin, name), &format!("{name}, as the earlier build shipped it\n"));
    }
    plugin
}

#[test]
fn while_the_earlier_engine_serves_a_session_starts_in_it_with_the_skills_it_ships() {
    let root = scratch("serves");
    let shipped = earlier_engine_plugin(&root);
    let folder = root.join("user");
    std::fs::create_dir_all(&folder).unwrap();
    assert!(
        !EARLIER_ENGINE_SKILLS.contains(&"transcribe-audio") && skills::SKILLS.contains(&"transcribe-audio"),
        "the fixture must be the case under test: this build names a skill the earlier engine does not ship"
    );

    // ON ANY OTHER DAY THIS BUILD'S SKILLS ARE REQUIRED, and the earlier engine does not have one:
    // the session would not start (and before this rule it then waited for the install).
    skills::earlier_engine_serves(false);
    let strict = user_settings::standing_plugin(&folder, &shipped, &root.join("standing-strict"));
    assert!(strict.is_err(), "the strict rule must still refuse an engine missing this build's skill");

    // WHILE THE EARLIER ENGINE SERVES: the session starts, at once, in that engine as it is.
    skills::earlier_engine_serves(true);
    let standing = user_settings::standing_plugin(&folder, &shipped, &root.join("standing"))
        .expect("the session starts in the earlier engine, with no wait");
    skills::verify_present(&standing).expect("the session's own preflight accepts it");
    assert!(skills::skill_path(&standing, "audio-transcription").is_file(), "its own skill came along");
    assert!(!skills::skill_path(&standing, "transcribe-audio").exists(), "nothing was made up for it");

    // AND ONCE THIS BUILD'S ENGINE IS IN PLACE, its skill names apply again, from the next session.
    skills::earlier_engine_serves(false);
    assert!(skills::verify_present(&standing).is_err(), "this build's names apply once the earlier engine retires");
    assert_eq!(skills::names_in(&standing), skills::SKILLS.iter().map(|s| s.to_string()).collect::<Vec<_>>());
}
