//! **WHERE RICHOS LIVES ON GITHUB, AND WHERE IT LIVED.** The CEO, 2026-10-27: *"I want to create
//! a new GitHub organization and then move both repos into that organization."* Then: *"OK, I
//! created https://github.com/RichSpark."* And: *"let's pretend that there currently are active
//! users. And then do this migration properly from that point of view."*
//!
//! So a release built from this code works with the repository at either address, before the
//! move, during it and after it, with nothing for the user to do:
//!
//!   - **Updates** try [`HOME`] first and [`FORMER`] second (`src-tauri/src/updates.rs`). Before
//!     the move the new address does not exist yet and the old one answers; after it the new one
//!     answers. A download on either address is fetched from the new one first and the old one
//!     second, decided when it is installed ([`download_addresses`]): an update found before the
//!     move and installed after it does not depend on GitHub's redirect from the old address.
//!   - **Reports and ideas** come back from the relay as a link on whichever repository the relay
//!     filed them on. A link on either address is the same repository ([`accepted`]); a link on any
//!     other repository is still refused.
//!
//! The steps of the move itself are in the private record:
//! `richos-hq/docs/operations/2026-10-27-github-richcore-move.md`.

/// The repository's address from the move on, exactly as the CEO capitalized it.
pub const HOME: &str = "RichSpark/RichOS";

/// The repository's address before the move. GitHub sends its old pages, downloads and API calls
/// to the new address for as long as nobody creates a repository at this one again.
pub const FORMER: &str = "WebDevBooster/richos";

/// The repositories a report link may name when the configured one is `configured`: both
/// addresses of RichOS when it is either of them, and only itself otherwise (a test relay's
/// repository stays one repository).
pub fn accepted(configured: &str) -> Vec<&str> {
    if configured == HOME || configured == FORMER {
        vec![HOME, FORMER]
    } else {
        vec![configured]
    }
}

/// The number of the GitHub page `url` when it is exactly
/// `https://github.com/<one of repositories>/<kind>/<n>` with `n` a positive number. `kind` is
/// `issues` or `discussions`.
pub fn page_number(url: &str, repositories: &[&str], kind: &str) -> Option<u64> {
    repositories.iter().find_map(|repository| {
        let rest = url.strip_prefix(&format!("https://github.com/{repository}/{kind}/"))?;
        // `parse` alone would take a leading `+`; only digits are a page number.
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        rest.parse::<u64>().ok().filter(|n| *n > 0)
    })
}

/// `url` moved to [`HOME`] when it is a page or download of [`FORMER`] on github.com, or `None`
/// when it is anything else. Used for an update download announced by a manifest published
/// before the move: the bytes are the same, and they are signature-checked either way.
pub fn at_home(url: &str) -> Option<String> {
    url.strip_prefix(&format!("https://github.com/{FORMER}/")).map(|rest| format!("https://github.com/{HOME}/{rest}"))
}

/// Where to download an update announced at `download`, in the order to try, decided when it is
/// installed: a page or download of RichOS on github.com, on either address, at [`HOME`] first and
/// [`FORMER`] second (before the move the new address has nothing; after it the new one answers);
/// anything else only as announced. The bytes are signature-checked whichever address delivers.
pub fn download_addresses(download: &str) -> Vec<String> {
    let rest = [HOME, FORMER]
        .iter()
        .find_map(|repository| download.strip_prefix(&format!("https://github.com/{repository}/")));
    match rest {
        Some(rest) => [HOME, FORMER].iter().map(|repository| format!("https://github.com/{repository}/{rest}")).collect(),
        None => vec![download.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The relay may answer with a link on the old repository (a report filed before the move,
    /// or replayed from the relay's record after it) or on the new one: both are RichOS.
    /// Nothing else is: not another repository of either owner, not a different capitalization
    /// the relay never sends, not another page.
    #[test]
    fn a_report_link_on_the_old_or_the_new_repository_is_accepted_and_nothing_else() {
        let both = accepted(HOME);
        assert_eq!(both, accepted(FORMER), "either address configured accepts both");
        assert_eq!(page_number("https://github.com/WebDevBooster/richos/issues/412", &both, "issues"), Some(412));
        assert_eq!(page_number("https://github.com/RichSpark/RichOS/issues/413", &both, "issues"), Some(413));
        assert_eq!(page_number("https://github.com/RichSpark/RichOS/discussions/9", &both, "discussions"), Some(9));
        assert_eq!(page_number("https://github.com/WebDevBooster/richos/discussions/9", &both, "discussions"), Some(9));
        for refused in [
            "https://github.com/RichSpark/RichOS-HQ/issues/1",
            "https://github.com/RichSpark/richos/issues/1",
            "https://github.com/RichCore/RichOS/issues/1",
            "https://github.com/WebDevBooster/RichOS/issues/1",
            "https://github.com/WebDevBooster/richos-hq/issues/1",
            "https://github.com/WebDevBooster/richos-report-test/issues/1",
            "https://github.com/someone/else/issues/1",
            "https://github.com/RichSpark/RichOS/pull/1",
            "https://github.com/RichSpark/RichOS/issues/0",
            "https://github.com/RichSpark/RichOS/issues/+1",
            "https://github.com/RichSpark/RichOS/issues/1/",
            "https://github.com/RichSpark/RichOS/issues/1#x",
            "https://github.com/RichSpark/RichOS/issues/",
            "http://github.com/RichSpark/RichOS/issues/1",
            "https://github.com.evil.example/RichSpark/RichOS/issues/1",
        ] {
            assert_eq!(page_number(refused, &both, "issues"), None, "{refused}");
        }
        assert_eq!(page_number("https://github.com/RichSpark/RichOS/issues/1", &both, "discussions"), None, "an issue is not an idea's post");
    }

    #[test]
    fn a_test_repository_accepts_only_itself() {
        let one = accepted("WebDevBooster/richos-report-test");
        assert_eq!(one, vec!["WebDevBooster/richos-report-test"]);
        assert_eq!(page_number("https://github.com/WebDevBooster/richos-report-test/issues/3", &one, "issues"), Some(3));
        assert_eq!(page_number("https://github.com/RichSpark/RichOS/issues/3", &one, "issues"), None);
        assert_eq!(page_number("https://github.com/WebDevBooster/richos/issues/3", &one, "issues"), None);
    }

    #[test]
    fn only_an_address_on_the_old_repository_moves_home() {
        assert_eq!(
            at_home("https://github.com/WebDevBooster/richos/releases/download/v1.2.3/RichOS.app.tar.gz").as_deref(),
            Some("https://github.com/RichSpark/RichOS/releases/download/v1.2.3/RichOS.app.tar.gz")
        );
        for unchanged in [
            "https://github.com/RichSpark/RichOS/releases/download/v1.2.3/RichOS.app.tar.gz",
            "https://github.com/WebDevBooster/richos-hq/releases/download/v1/x",
            "https://objects.githubusercontent.com/WebDevBooster/richos/x",
            "http://127.0.0.1:8973/RichOS.app.tar.gz",
        ] {
            assert_eq!(at_home(unchanged), None, "{unchanged}");
        }
    }

    /// An update announced on either address, whenever it was found, is downloaded from the new
    /// address first and the old one second (follow-up `rv-20261027T120956Z-02520365-1a35#1`: one
    /// found before the move and installed after it). Anything else only as announced.
    #[test]
    fn a_download_on_either_address_is_tried_at_the_new_one_first_and_the_old_one_second() {
        let old = "https://github.com/WebDevBooster/richos/releases/download/v1.2.3/RichOS.app.tar.gz";
        let new = "https://github.com/RichSpark/RichOS/releases/download/v1.2.3/RichOS.app.tar.gz";
        assert_eq!(download_addresses(old), [new, old], "announced on the old address before the move");
        assert_eq!(download_addresses(new), [new, old], "announced on the new address");
        for unchanged in [
            "http://127.0.0.1:8973/RichOS.app.tar.gz",
            "https://github.com/WebDevBooster/richos-hq/releases/download/v1/x",
            "https://github.com/RichSpark/RichOS-HQ/releases/download/v1/x",
            "https://objects.githubusercontent.com/WebDevBooster/richos/x",
        ] {
            assert_eq!(download_addresses(unchanged), [unchanged], "{unchanged}");
        }
    }
}
