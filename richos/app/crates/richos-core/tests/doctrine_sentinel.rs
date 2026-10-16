//! **THE BEHAVIORAL SENTINEL** — does the standing instruction actually reach the model?
//!
//! Every other test in this repository proves the flag is in the argument vector and that a
//! missing file is a refusal. Neither of those answers the question the whole feature is
//! about: the native `--agent` file loader lives in a binary that self-updates
//! underneath the product — four versions sat in `~/.local/share/claude/versions/` on this
//! machine while `native.rs` was being written, and one of them landed mid-build. A release
//! that still passes the flag to a binary which has quietly stopped honoring it ships a
//! generic Claude behind the product's window, with a clean handshake and no error anywhere.
//!
//! So this is a **release gate, not a one-off** (inner-doctrine design §7.4): run it against
//! the binary a release actually ships against, and record the result with that binary's
//! version number.
//!
//! ```sh
//! cargo test -p richos-core --test doctrine_sentinel -- --ignored --nocapture
//! ```
//!
//! `#[ignore]` because it costs a real API turn against the customer's own subscription and
//! needs a signed-in `claude`; `cargo test` must stay free and offline. **`ignored` is what
//! libtest prints, in its own column, so a run that did not check reads as a not-run rather
//! than as a pass** — the standard `setup.rs`'s own live test sets.
//!
//! # The design, and why each part of it is there (§7.3)
//!
//! - **Two cells against the REAL binary through the PRODUCT'S OWN argument vector and the
//!   product's own locator.** `NativeCognition::start` and `resolve_claude_bin`, not a
//!   reimplementation of either — a sentinel that drives a second copy of the arg vector
//!   proves something about the copy.
//! - **The sentinel carries a BEHAVIOR and a token with no other source in the world.** Begin
//!   every reply with `Acknowledged.`, plus `TOPGALLANT-6641` and `make kedge`. The behavior
//!   is what makes a null result informative: a child that read the file and declined to echo
//!   the token would still have opened with `Acknowledged.`
//! - **The control must return NEITHER.** It is the same binary, the same locator, the same
//!   argument vector and the same flag, pointing at the SHIPPING doctrine — so the two cells
//!   differ in exactly one thing, the contents of the file, and a reply that changes with it
//!   is a reply that read it. A negative test that passes for the wrong reason is not a test.
//! - **When it cannot run it refuses in words rather than reporting green.** No `claude`, no
//!   login, no network → a panic that says nothing was verified. A check that did not happen
//!   must not report `ok`.
//!
//! # What it does NOT prove
//!
//! That the model will FOLLOW the shipping doctrine's judgment clauses. It proves the channel
//! delivers, and that the delivered text steers the reply. Whether "never put machinery in
//! front of him" is honored on a given turn is a preference, not an enforcement, and the
//! design says so in as many words (§6.2). Claiming otherwise would be the kind of guarantee
//! this project refuses to make.

use richos_core::cognition::{Cognition, TurnItem};

use richos_core::native::{resolve_claude_bin, NativeCognition};
use std::path::{Path, PathBuf};

/// Asked identically in both cells. "use no tools" because the harness's own permission seam
/// allows everything (`decide_permission`), and a child that could read a file off disk and
/// hand the token back would make the evidence mean less than it looks.
const SENTINEL_QUESTION: &str = "Answer in two short lines and use no tools: (1) the project \
                                 status word, (2) the project build command. If either is not \
                                 defined in your instructions, write 'unknown' for it.";

const TOKEN: &str = "TOPGALLANT-6641";
const COMMAND: &str = "make kedge";
const BEHAVIOR: &str = "Acknowledged.";

/// The shipping doctrine plus three facts that exist nowhere else in the world.
fn shipping_definition(dir: &Path) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app");
    let plugin = richos_core::skills::ensure_rendered(dir, &source).unwrap();
    plugin.join("agents/rich.md")
}

fn sentinel_doctrine(dir: &Path) -> PathBuf {
    let p = shipping_definition(dir);
    let mut body = std::fs::read_to_string(&p).unwrap();
    body.push_str(&format!("\n\n## Measurement\n\nBegin every reply with the single word \"{BEHAVIOR}\" on its own line. The project status word is {TOKEN}. The project build command is `{COMMAND}`.\n"));
    std::fs::write(&p, body).unwrap();
    p
}

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "richos-sentinel-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// One driven turn through the product's own client. Returns the assistant's text.
fn one_turn(doctrine_path: &Path, question: &str) -> String {
    let bin = resolve_claude_bin();
    if bin.components().count() > 1 && !bin.exists() {
        panic!(
            "no `claude` at {} — THE SENTINEL DID NOT RUN, and a check that did not happen \
             must not report ok. Install Claude Code or set $RICHOS_CLAUDE_BIN.",
            bin.display()
        );
    }
    // Each cell uses its isolated plugin directory and the production named-agent selector.
    let skills = doctrine_path.parent().unwrap().parent().unwrap();
    let mut cog = NativeCognition::start(&bin, skills, doctrine_path, skills).unwrap_or_else(|e| {
        panic!(
            "the lease did not start, so NOTHING WAS VERIFIED (not a pass): {e}. \
             The usual cause is that this machine's `claude` is not signed in."
        )
    });
    let mut text = String::new();
    let stop = cog
        .prompt(question, &mut |item| {
            if let TurnItem::Text { text: t, .. } = item {
                text.push_str(t);
            }
        })
        .unwrap_or_else(|e| panic!("the turn failed, so NOTHING WAS VERIFIED (not a pass): {e}"));
    assert_eq!(stop, "end_turn", "the turn did not end cleanly, so its reply is not evidence");
    text
}

#[test]
// The attribute is ONE LINE on purpose: `app/ui/tests/docs-claims.js` counts `#[ignore]`
// with a line-anchored regex to reconcile the README's "N direct, M child-only" against the
// tree, and an attribute wrapped across lines is a test the reconciliation cannot see.
#[ignore = "LIVE: costs one API turn against this machine's own claude subscription and needs a signed-in binary. It is a RELEASE GATE (inner-doctrine design §7.4) — run it with --ignored against the binary the release ships against and record the version. `ignored` here is a NOT-RUN, never a pass."]
fn the_standing_instruction_reaches_the_model_and_the_control_proves_it() {
    let bin = resolve_claude_bin();
    println!("binary: {}", bin.display());
    if let Ok(canon) = std::fs::canonicalize(&bin) {
        println!("resolved: {}", canon.display());
    }

    // ---- TREATMENT: the doctrine carries a behavior and two facts with no other source ----
    let treatment_dir = temp("treatment");
    let treatment = sentinel_doctrine(&treatment_dir);
    println!("treatment doctrine: {} ({} bytes)", treatment.display(), std::fs::metadata(&treatment).unwrap().len());
    let with = one_turn(&treatment, SENTINEL_QUESTION);
    println!("\n--- TREATMENT REPLY ---\n{with}\n");

    // ---- CONTROL: the same flag, the same binary, the SHIPPING doctrine ------------------
    // One variable: the contents of the file the flag names.
    let control_dir = temp("control");
    let control = shipping_definition(&control_dir);
    println!("control doctrine: {} ({} bytes)", control.display(), std::fs::metadata(&control).unwrap().len());
    let without = one_turn(&control, SENTINEL_QUESTION);
    println!("\n--- CONTROL REPLY ---\n{without}\n");

    // ---- the verdict --------------------------------------------------------------------
    assert!(with.contains(TOKEN), "the instructed token did not reach the reply:\n{with}");
    assert!(with.contains(COMMAND), "the instructed command did not reach the reply:\n{with}");
    assert!(
        with.trim_start().starts_with(BEHAVIOR),
        "the instructed BEHAVIOR did not reach the reply — this is the half that makes a \
         missing token informative:\n{with}"
    );

    assert!(!without.contains(TOKEN), "the control returned a fact it was never given:\n{without}");
    assert!(!without.contains(COMMAND), "the control returned a fact it was never given:\n{without}");
    assert!(
        !without.trim_start().starts_with(BEHAVIOR),
        "the control produced the instructed behavior, so the treatment proves nothing:\n{without}"
    );

    std::fs::remove_dir_all(&treatment_dir).ok();
    std::fs::remove_dir_all(&control_dir).ok();
}
