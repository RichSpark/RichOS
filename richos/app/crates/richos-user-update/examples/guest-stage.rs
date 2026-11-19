//! Test-guest fixture for `scripts/testvm/update-in-place-walk.py --scenario reinstall`, never
//! shipped (an example is not part of the app bundle). It puts a RichOS update archive in place
//! through this crate's own staging and publication, which is how an older RichOS left its copy
//! and its publication receipt in ~/Applications:
//!
//!   guest-stage HOME VERSION ARCHIVE
//!
//! Run only where no copy is at ~/Applications/RichOS.app and Applications was never chosen as
//! the install location, so the publication lands in ~/Applications. The archive's own
//! signature is not checked here: the host verified the signed fixture before handing it over.
use std::{env, fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let [home, version, archive] = args.as_slice() else {
        return Err("expected HOME VERSION ARCHIVE".into());
    };
    let home = Path::new(home);
    richos_user_update::stage_verified(home, &fs::read(archive)?, version)?;
    let mut lease = richos_user_update::StartupLease::acquire(home)?;
    let done = richos_user_update::activate_prepared(&mut lease, "0.0.0")?
        .ok_or("nothing was put in place")?;
    println!("published {}", done.entered.version);
    Ok(())
}
