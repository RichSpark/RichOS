//! **Settings → Terminal setup, on the Mac** (richos-hq design round 41, piece 1). The read is
//! `richos-core`'s `login_environment.rs`; this file is the window's two commands: what the last
//! read found (names only, never a value), and reading it again.

/// The Terminal setup as last read. Waits for the read the app makes at start when it is still
/// running, which is at most `login_environment::TIMEOUT`.
#[richos_command::command]
pub fn terminal_setup() -> Result<richos_core::login_environment::Setup, String> {
    Ok(richos_core::login_environment::setup())
}

/// Read the Terminal setup again now and say what changed. A read that does not finish keeps the
/// last good one, which the answer says.
#[richos_command::command]
pub fn terminal_setup_read_again() -> Result<richos_core::login_environment::Setup, String> {
    Ok(richos_core::login_environment::read_again())
}

#[cfg(test)]
mod tests {
    /// Both commands answer off Tauri's async workers (`crate::command_rule_tests`): a read can
    /// take up to ten seconds.
    #[test]
    fn its_two_commands_run_off_the_async_workers() {
        let source = include_str!("terminal_setup_host.rs");
        assert_eq!(crate::command_rule_tests::bare_tauri_attributes(source), Vec::<String>::new());
        let ours = source.lines().filter(|line| line.trim() == "#[richos_command::command]").count();
        assert_eq!(ours, 2, "terminal_setup, terminal_setup_read_again");
    }
}
