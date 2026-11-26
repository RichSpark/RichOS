//! **My RichOS** (round 109): its switch and its one read. Everything it counts and reads is in
//! `richos_core::my_richos`; nothing of it runs while the switch is off.
use richos_core::config::ConfigStore;
use richos_core::my_richos::{self, Detail, View};
use std::path::Path;
use std::sync::Mutex;
use tauri::State;

/// Whether Show My RichOS is on. The page asks this once as it starts, so the monitor, when the
/// switch was left on, starts here: after the page, never in front of the first frame.
#[richos_command::command]
pub fn get_richos_monitor(state: State<crate::AppState>) -> bool {
    read(&state.config, &state.data_dir.join("engine-state"))
}

/// Save the switch and start or stop the monitor with it. A choice that could not be saved
/// changes nothing.
#[richos_command::command]
pub fn set_richos_monitor(state: State<crate::AppState>, show: bool) -> Result<(), String> {
    write(&state.config, &state.data_dir.join("engine-state"), show)
}

/// What the square, and the card (`card`) and the details (`detail`) when open, show now (asked
/// once a second while it is on and the window is visible). Answers `on: false` and nothing else
/// while it is off.
#[richos_command::command]
pub fn richos_monitor(card: Option<bool>, detail: Option<Detail>) -> View {
    my_richos::view(card.unwrap_or(false), detail)
}

fn read(config: &Mutex<ConfigStore>, state_root: &Path) -> bool {
    let on = config.lock().unwrap_or_else(|p| p.into_inner()).show_richos_monitor();
    if on {
        my_richos::set_on(true, state_root);
    }
    on
}

fn write(config: &Mutex<ConfigStore>, state_root: &Path, show: bool) -> Result<(), String> {
    config.lock().unwrap_or_else(|p| p.into_inner()).set_show_richos_monitor(show).map_err(|e| e.to_string())?;
    my_richos::set_on(show, state_root);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off by default and nothing running; on is saved and starts it; a later start with the
    /// switch left on starts it again from the saved choice; off is saved and stops it.
    #[test]
    fn the_switch_is_off_by_default_saved_and_starts_and_stops_my_richos() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = root.join("config.json");
        let state = root.join("engine-state");
        let config = Mutex::new(ConfigStore::open(&path).unwrap());
        assert!(!read(&config, &state), "the switch is on by default");
        assert!(!my_richos::on() && !my_richos::view(true, None).on, "My RichOS runs while the switch is off");

        write(&config, &state, true).unwrap();
        assert!(my_richos::on() && my_richos::view(true, None).on, "switching on did not start it");
        assert!(ConfigStore::open(&path).unwrap().show_richos_monitor(), "the choice was not saved");

        // The next start: off in memory, the saved switch on, the page asks.
        my_richos::set_on(false, &state);
        let reopened = Mutex::new(ConfigStore::open(&path).unwrap());
        assert!(read(&reopened, &state));
        assert!(my_richos::on(), "a switch left on did not start My RichOS on the page's first ask");

        write(&reopened, &state, false).unwrap();
        assert!(!my_richos::on() && !my_richos::view(true, None).on, "switching off did not stop it");
        assert!(!ConfigStore::open(&path).unwrap().show_richos_monitor(), "the off choice was not saved");
    }
}
