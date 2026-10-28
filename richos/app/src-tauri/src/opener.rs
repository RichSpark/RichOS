//! **OPENING ONE OF EIGHT KNOWN THINGS, AND NOTHING ELSE.**
//!
//! CEO §61.1, from his own evening: Tailscale's own screens never say that the account IS the
//! network, and he could not find the macOS download from `tailscale.com`'s home page or from the
//! admin console. So the how-to screens name an exact address for every step — and an address a
//! person has to retype into a browser is an address half of them will mistype.
//!
//! # Why this is an allowlist and not a URL opener
//!
//! A general "open this URL" command is a hole with a button on it: anything that can reach the
//! webview can then ask the Mac to open `file:///`, a `mailto:` that sends, a custom scheme
//! registered by some other application, or a link that looks like ours with a different host.
//! The screens need **eight** destinations (the changelog under two keys, its address before and
//! after the GitHub move), all of them fixed at build time, so the command takes
//! the address as a **key into a table** rather than as data to act on. Anything not in the table
//! is refused by name.
//!
//! **The table IS the set of addresses the screens print**, and a test in this file reads
//! `ui/phone.js` and proves it: a new address in the UI that nobody allowlisted fails the suite
//! rather than silently producing a control that does nothing.
//!
//! # And the address stays written out beside the control
//!
//! Urban's §2: *"a control that opens somewhere the user cannot see first is a control that asks
//! for trust it has not earned"*. The button is the convenience; the printed address is the
//! fallback for a Mac where the opener does not work, and neither replaces the other.

use std::path::Path;
use std::process::Command;

/// What one entry of the table actually opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A web page. The scheme is OURS — the table holds hosts and paths only, so nothing the
    /// screen passes can choose a scheme.
    Page(&'static str),
    /// An application already on this Mac, by name and then by bundle identifier.
    App,
}

/// **Every destination the how-to, quota, setup and update screens name.** Eight, and the key is the address exactly as the
/// screen prints it — so a reader of `phone.js` and a reader of this file see the same string.
///
/// The two Apple identifiers are different and must not be swapped: `1470499037` is the iPhone and
/// iPad app, `1475387142` is the Mac app. Sending someone to the wrong one sends them to a listing
/// their device cannot install from. (Only the phone one appears here; the Mac is installed from
/// `tailscale.com/download/mac`, which is the page Tailscale's own documentation sends people to.)
const ALLOWED: &[(&str, Target)] = &[
    ("claude.ai/new#settings/usage", Target::Page("https://claude.ai/new#settings/usage")),
    // The setup sheet's "sign up there first and pick the Max/Max 20x tier" link (CEO, 2026-10-08).
    ("claude.com/pricing", Target::Page("https://claude.com/pricing")),
    ("tailscale.com/download/mac", Target::Page("https://tailscale.com/download/mac")),
    (
        "apps.apple.com/app/tailscale/id1470499037",
        Target::Page("https://apps.apple.com/app/tailscale/id1470499037"),
    ),
    (
        "play.google.com/store/apps/details?id=com.tailscale.ipn",
        Target::Page("https://play.google.com/store/apps/details?id=com.tailscale.ipn"),
    ),
    ("console.tailscale.com/admin/dns", Target::Page("https://console.tailscale.com/admin/dns")),
    // The Updates row's "See what's new" link (CEO, 2026-10-19; round 34): the RichOS changelog.
    (
        "github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md",
        Target::Page("https://github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md"),
    ),
    // The same page under RichOS's address before the GitHub move (2026-10-28; richos-hq
    // `docs/operations/2026-10-27-github-richcore-move.md`): a link already in a user's hands keeps
    // opening. It opens exactly the address asked for, as every entry does, and GitHub sends it on
    // to the new address.
    (
        "github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md",
        Target::Page("https://github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md"),
    ),
    // NOT a URL. Screen 3 says "Open Tailscale", and what that has to do is bring the app the user
    // already installed to the front — sending them to a web page there would be telling somebody
    // who has the app to go and download it again.
    ("Tailscale.app", Target::App),
];

/// The two bundle identifiers Tailscale ships on macOS, tried in turn if the name does not
/// resolve. MEASURED on this Mac on 2026-09-19: the installed copy is `io.tailscale.ipn.macsys`,
/// the open-source/standalone variant. `io.tailscale.ipn.macos` is the Mac App Store build, which
/// is what a different user will have — <https://tailscale.com/docs/concepts/macos-variants>.
const TAILSCALE_BUNDLE_IDS: [&str; 2] = ["io.tailscale.ipn.macsys", "io.tailscale.ipn.macos"];

/// **Exact match, and that is the whole security argument.**
///
/// No trimming, no lowercasing, no prefix or suffix test, no parsing. `tailscale.com/download/mac`
/// opens; `tailscale.com.evil.example/download/mac`, `tailscale.com/download/mac?x=1`,
/// `https://tailscale.com/download/mac` and ` tailscale.com/download/mac` do not — not because
/// each was thought of, but because a table lookup cannot be talked into a near miss.
pub fn resolve(asked: &str) -> Option<Target> {
    ALLOWED.iter().find(|(key, _)| *key == asked).map(|(_, target)| *target)
}

/// Open it, or say why not.
///
/// The error is a sentence for the screen, and the screen keeps the address printed beside the
/// control either way — so a Mac where `open` does not work is a Mac where the user reads the
/// address rather than one where the step is impossible.
pub fn open(asked: &str) -> Result<(), String> {
    match resolve(asked) {
        Some(Target::Page(url)) => run(&["/usr/bin/open", url]),
        Some(Target::App) => open_tailscale(),
        None => {
            // The refusal is logged with the string, because the only way this happens is a screen
            // asking for something nobody allowlisted — a defect, not an attack, and one nobody
            // can find without knowing what was asked for.
            eprintln!("[richos] refusing to open an address that is not one of the screens': {asked}");
            Err("I can only open the pages these screens name.".into())
        }
    }
}

/// By name first, then by each bundle identifier. `open -a Tailscale` covers both variants on a
/// normal install; the identifiers are the fallback for a copy that is not in `/Applications`.
fn open_tailscale() -> Result<(), String> {
    if Path::new("/Applications/Tailscale.app").exists()
        && run(&["/usr/bin/open", "-a", "/Applications/Tailscale.app"]).is_ok()
    {
        return Ok(());
    }
    for id in TAILSCALE_BUNDLE_IDS {
        if run(&["/usr/bin/open", "-b", id]).is_ok() {
            return Ok(());
        }
    }
    Err("I could not find Tailscale on this Mac to open it.".into())
}

// ---- a file Rich put in front of the user (§138, build plan item 12) -------------------------
//
// **The one destination that is not in the table above, and the same rule holds for it.** The
// key is an attachment id, never a path: it is looked up in the conversation's own record of the
// files Rich and Stu attached (`richos-core`'s `rich_files.rs`), and only what that record holds
// for THAT conversation opens. Anything else is refused by name, exactly like an address nobody
// allowlisted. The file opens with the Mac's default app for it, as a double-click in Finder
// would; nothing here chooses an app or passes an option.

/// An id the conversation's record does not hold.
pub const NOT_ATTACHED: &str = "I don't have that file in this conversation.";
/// Nothing at the recorded path now.
pub const FILE_GONE: &str = "No longer on this Mac. It was moved or deleted.";
/// A link, a folder, or another file swapped in under the recorded path.
pub const FILE_REFUSED: &str = "This file was replaced by a link or a folder, so I won't open it from here.";

/// The file an attachment id names in this conversation, checked as it is now.
pub fn resolve_attached(root: &Path, thread_id: &str, id: &str) -> Result<std::path::PathBuf, String> {
    use richos_core::rich_files::{locate, now, Now};
    let row = locate(root, thread_id, id).ok_or_else(|| NOT_ATTACHED.to_string())?;
    match now(&row) {
        Now::Here { .. } => Ok(std::path::PathBuf::from(row.path)),
        Now::Gone => Err(FILE_GONE.into()),
        Now::Refused => Err(FILE_REFUSED.into()),
    }
}

/// Open an attached file with the Mac's default app for it.
pub fn open_attached(root: &Path, thread_id: &str, id: &str) -> Result<(), String> {
    open_attached_with(root, thread_id, id, &run)
}

/// Select an attached file in a Finder window.
pub fn reveal_attached(root: &Path, thread_id: &str, id: &str) -> Result<(), String> {
    let path = resolve_attached(root, thread_id, id)?;
    run(&["/usr/bin/open", "-R", &path.to_string_lossy()])
}

/// [`open_attached`] with the runner handed in, so a test sees the exact command.
pub fn open_attached_with(
    root: &Path,
    thread_id: &str,
    id: &str,
    runner: &dyn Fn(&[&str]) -> Result<(), String>,
) -> Result<(), String> {
    let path = resolve_attached(root, thread_id, id)?;
    // The recorded path is canonical and absolute, so it begins with `/` and `open` cannot
    // read it as an option.
    runner(&["/usr/bin/open", &path.to_string_lossy()])
}

fn run(argv: &[&str]) -> Result<(), String> {
    let status = Command::new(argv[0])
        .args(&argv[1..])
        .status()
        .map_err(|e| format!("this Mac would not open it: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("this Mac would not open it.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **THE TABLE IS THE SCREENS' OWN ADDRESSES, proven against the screens.**
    ///
    /// `ui/phone.js` holds them in one `LINKS` object. If somebody adds a sixth address to the
    /// screens and not to this table, the control they wire to it would do nothing at all — and
    /// the failure would be a button that looks fine and is dead. This reads the shipped file.
    #[test]
    fn every_address_the_screens_print_is_one_this_command_will_open() {
        let phone_js = include_str!("../../ui/phone.js");
        let start = phone_js.find("const LINKS = {").expect("ui/phone.js no longer has a LINKS table");
        let end = phone_js[start..].find("};").expect("the LINKS table is not closed") + start;
        let mut found = 0;
        for line in phone_js[start..end].lines().skip(1) {
            let Some((_, value)) = line.split_once(':') else { continue };
            let value = value.trim().trim_end_matches(',').trim_matches('"');
            if value.is_empty() || value.starts_with("//") {
                continue;
            }
            found += 1;
            assert!(
                resolve(value).is_some(),
                "the screens print {value:?} and this command would refuse to open it"
            );
        }
        assert_eq!(found, 4, "the LINKS table is not the four addresses this table was built for");
        // And the fifth entry, which is not a URL and so is not in `LINKS`.
        assert_eq!(resolve("Tailscale.app"), Some(Target::App));
        // The sixth destination belongs to quota recovery, not phone setup.
        let quota_js = include_str!("../../ui/quota.js");
        let usage = "claude.ai/new#settings/usage";
        assert!(quota_js.contains(&format!("target: \"{usage}\"")));
        assert!(quota_js.contains(&format!("title=\"{usage}\"")), "the button must disclose its destination");
        assert!(quota_js.contains(&format!("Open {usage} in your browser.")), "a failed opener must print the fallback address");
        assert_eq!(resolve(usage), Some(Target::Page("https://claude.ai/new#settings/usage")));
        // The seventh belongs to the setup sheet's account sentence.
        let main_js = include_str!("../../ui/main.js");
        assert!(main_js.contains("\"claude.com/pricing\""), "the setup sheet must ask for the pricing address");
        assert_eq!(resolve("claude.com/pricing"), Some(Target::Page("https://claude.com/pricing")));
        // The eighth belongs to the Updates row's "See what's new" link (CEO, 2026-10-19).
        let updates_js = include_str!("../../ui/updates.js");
        let changelog = "github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md";
        assert!(
            updates_js.contains(&format!("\"{changelog}\"")),
            "the Updates row must ask for the changelog by this key"
        );
        assert_eq!(
            resolve(changelog),
            Some(Target::Page("https://github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md"))
        );
        // The key it had before the GitHub move still opens it (GitHub redirects that address).
        assert_eq!(
            resolve("github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md"),
            Some(Target::Page("https://github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md"))
        );
        // Eight destinations, the changelog under both of its addresses.
        assert_eq!(ALLOWED.len(), 9, "the allowlist grew or shrank without this test being told");
    }

    /// **AN ADDRESS OUTSIDE THE LIST IS REFUSED** — the check the brief asks for, over the near
    /// misses that a prefix test, a suffix test or a `contains` would each have let through.
    #[test]
    fn anything_that_is_not_exactly_one_of_the_eight_is_refused() {
        for asked in [
            // The classic host-suffix trick: it ENDS with nothing of ours and BEGINS with all of it.
            "tailscale.com.evil.example/download/mac",
            // And the other way round.
            "evil.example/tailscale.com/download/mac",
            // Our address with something appended, which a `starts_with` would allow.
            "tailscale.com/download/mac?x=1",
            "tailscale.com/download/mac#x",
            "tailscale.com/download/mac/../../etc/passwd",
            // The scheme is ours to choose, so an address carrying one is not an address we know.
            "https://tailscale.com/download/mac",
            "javascript:alert(1)",
            "claude.ai.evil.example/new#settings/usage",
            "claude.ai/new#settings/usage?reset=true",
            "claude.com/pricing?x=1",
            "claude.com/pricing/",
            "claude.com",
            // The changelog's near misses: a scheme, another branch, a suffix, the repository.
            "https://github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md",
            "github.com/WebDevBooster/richos/blob/evil/docs/CHANGELOG.md",
            "github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md?x=1",
            "github.com/WebDevBooster/richos",
            "https://github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md",
            "github.com/RichSpark/RichOS/blob/evil/docs/CHANGELOG.md",
            "github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md?x=1",
            "github.com/RichSpark/richos/blob/main/docs/CHANGELOG.md",
            "github.com/RichSpark/RichOS",
            "file:///etc/passwd",
            "mailto:someone@example.com",
            // Whitespace and case: no normalizing, on purpose.
            " tailscale.com/download/mac",
            "tailscale.com/download/mac ",
            "Tailscale.com/Download/Mac",
            // A real Tailscale page that these screens do not name.
            "tailscale.com/download",
            "login.tailscale.com/admin",
            "",
        ] {
            assert_eq!(resolve(asked), None, "{asked:?} was allowed");
            assert!(open(asked).is_err(), "{asked:?} was opened");
        }
    }

    /// **A FILE RICH ATTACHED REACHES THE CONVERSATION AND OPENS THROUGH THIS OPENER** (§138,
    /// build plan item 12): the tool call Rich makes, the item the conversation shows, and the
    /// command the click runs, end to end through the code that ships.
    #[test]
    fn a_file_rich_attached_reaches_the_conversation_and_opens_through_the_opener() {
        use richos_core::question_tools::{write_scope, Scope};
        use richos_core::questions::AskScope;
        use richos_core::rich_files;
        use serde_json::json;
        use std::cell::RefCell;

        let dir = std::env::temp_dir().join(format!("richos-opener-attached-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.join("engine-state");
        let file = dir.join("Acme counteroffer.pdf");
        std::fs::write(&file, b"%PDF-1.4").unwrap();
        let scope = dir.join("assignments.questions.json");
        write_scope(&scope, &Scope {
            context: AskScope { root: root.clone(), entity_id: "acme".into(), thread_id: "t1".into(),
                turn_id: "turn-1".into(), asker: "front_desk".into(), obligation_id: None,
                session_id: "s".into(), engine: None, entity_root: None, app_run: None },
            actions_allowed: true, answer_method: "typed".into(), surface: "mac".into(),
        }).unwrap();

        // Rich calls the tool, as the model does through the MCP server.
        let out = rich_files::call(&scope, rich_files::TOOL, json!({"path": file.to_string_lossy()})).unwrap();
        let id = out["id"].as_str().unwrap().to_string();

        // The conversation carries it, in Rich's turn.
        let items = rich_files::items(&root, "acme", "t1");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["kind"], "rich_file");
        assert_eq!(items[0]["turnId"], "turn-1");
        assert_eq!(items[0]["file"]["id"], id.as_str());
        assert_eq!(items[0]["file"]["name"], "Acme counteroffer.pdf");

        // A click opens it through this opener, with the Mac's default app.
        let ran = RefCell::new(Vec::<String>::new());
        let runner = |argv: &[&str]| {
            *ran.borrow_mut() = argv.iter().map(|s| s.to_string()).collect();
            Ok(())
        };
        open_attached_with(&root, "t1", &id, &runner).unwrap();
        let canonical = std::fs::canonicalize(&file).unwrap();
        assert_eq!(*ran.borrow(), vec!["/usr/bin/open".to_string(), canonical.to_string_lossy().into_owned()]);

        // Only that conversation's record opens it; a made-up id and a path are refused.
        assert_eq!(open_attached_with(&root, "t2", &id, &runner), Err(NOT_ATTACHED.into()));
        assert_eq!(open_attached_with(&root, "t1", &file.to_string_lossy(), &runner), Err(NOT_ATTACHED.into()));
        // And the table's own addresses are not attachment ids, nor the other way round.
        assert_eq!(resolve(&id), None);

        // Moved or deleted: the click says so instead of opening something else.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(open_attached_with(&root, "t1", &id, &runner), Err(FILE_GONE.into()));
        // Replaced by a link: refused, never followed.
        std::os::unix::fs::symlink("/etc/hosts", &file).unwrap();
        assert_eq!(open_attached_with(&root, "t1", &id, &runner), Err(FILE_REFUSED.into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_scheme_is_ours_and_every_page_is_https() {
        for (key, target) in ALLOWED {
            if let Target::Page(url) = target {
                assert!(url.starts_with("https://"), "{key} opens over something that is not https");
                assert!(
                    url.ends_with(key) || url == &format!("https://{key}"),
                    "{key} and {url} are not the same destination, so the printed address lies"
                );
            }
        }
    }
}
