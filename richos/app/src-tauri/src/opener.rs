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
//! The screens need **eight** destinations, all of them fixed at build time, so the command takes
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
        let changelog = "github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md";
        assert!(
            updates_js.contains(&format!("\"{changelog}\"")),
            "the Updates row must ask for the changelog by this key"
        );
        assert_eq!(
            resolve(changelog),
            Some(Target::Page("https://github.com/WebDevBooster/richos/blob/main/docs/CHANGELOG.md"))
        );
        assert_eq!(ALLOWED.len(), 8, "the allowlist grew or shrank without this test being told");
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
