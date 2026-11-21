//! Test-guest fixture, never shipped (an example is not part of the app bundle). It puts a
//! RichOS update archive in place through this crate's own staging and publication.
//!
//!   guest-stage HOME VERSION ARCHIVE
//!
//! `scripts/testvm/update-in-place-walk.py --scenario reinstall` uses it this way: that is how
//! an older RichOS left its copy and its publication receipt in ~/Applications. Run only where
//! no copy is at ~/Applications/RichOS.app and Applications was never chosen as the install
//! location, so the publication lands in ~/Applications.
//!
//!   guest-stage HOME VERSION ARCHIVE --prepare-only
//!
//! Stages the archive as a prepared update and stops there, as a finished download does. The
//! installed RichOS then puts it in place itself at its next launch, with its own code: a
//! published nightly updating to a candidate that is not published yet (a published build
//! refuses a manifest that is not served over https, so its download cannot be pointed at the
//! guest). Prints `prepared <version>`.
//!
//! The archive's own signature is not checked here: the host verified the signed fixture
//! before handing it over.
use std::{env, fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let (home, version, archive, prepare_only) = match args.as_slice() {
        [home, version, archive] => (home, version, archive, false),
        [home, version, archive, flag] if flag == "--prepare-only" => (home, version, archive, true),
        _ => return Err("expected HOME VERSION ARCHIVE [--prepare-only]".into()),
    };
    let home = Path::new(home);
    let staged = richos_user_update::stage_verified(home, &fs::read(archive)?, version)?;
    if prepare_only {
        // The same archive staged again after it was put in place returns the published
        // receipt and prepares nothing: say so instead of claiming a queued update.
        if !richos_user_update::has_prepared(home) {
            return Err(format!("nothing prepared: {} is already in place", staged.version).into());
        }
        println!("prepared {}", staged.version);
        return Ok(());
    }
    let mut lease = richos_user_update::StartupLease::acquire(home)?;
    let done = richos_user_update::activate_prepared(&mut lease, "0.0.0")?
        .ok_or("nothing was put in place")?;
    println!("published {}", done.entered.version);
    Ok(())
}
