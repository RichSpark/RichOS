use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Gate on the app icons BEFORE Tauri's own build step, so a placeholder icon set is
/// caught with a clear, actionable message instead of quietly shipping a blank/wrong
/// icon (macOS: `.app` with no icon at any size; Windows: exe/installer with no icon at
/// all — both launch-blocking for a product the CEO installs on his own machine).
///
/// THE DEFECT THIS REPLACES: all four files under `icons/` were byte-identical 2200-byte
/// placeholders, each internally a 512x512 PNG regardless of what its filename claimed.
///
/// THE DEFECT *THIS REWRITE* REPLACES: the previous version of this gate hand-typed a
/// list of four filenames while `tauri.conf.json` declared six, so `icon.icns` and
/// `icon.ico` — the only two artefacts that actually carry the icon on macOS and Windows
/// — were never checked at all. A check that describes something narrower than it claims
/// to is worse than no check, because it buys false confidence. So this version derives
/// the required set from `tauri.conf.json`'s `bundle.icon` array: Tauri's own list of
/// what it bundles. Adding a size there cannot leave this check silently passing, and an
/// entry we cannot decode is a failure rather than a skip.
///
/// `richos/app/scripts/lib/app_icons.py` derives the SAME requirements INDEPENDENTLY from the
/// SAME config. That duplication is deliberate — two independent readers agreeing is a
/// stronger guarantee than one shared helper, and if the generator's derivation ever
/// drifts from this one, the build says so.
fn main() {
    println!("cargo::rerun-if-changed=tauri.conf.json");
    println!("cargo::rerun-if-changed=icons");
    println!("cargo::rerun-if-changed=../ui");
    println!("cargo::rerun-if-changed=../../web/web-app");
    println!("cargo::rerun-if-env-changed=RICHOS_REQUIRE_REAL_ICONS");

    // THE SOURCE-COMMIT STAMP, so a changed commit actually reaches the next build.
    //
    // `option_env!("RICHOS_SOURCE_SHA")` (`engine::source_commit`) is read when THIS crate is
    // compiled, and cargo does not know that unless it is told — without this line a cached
    // build would carry the previous commit's stamp and nothing would say so, which is the
    // exact failure the stamp exists to prevent.
    //
    // SCOPED TO THIS CRATE, deliberately, because that is all it can do. The engine pin's three
    // variables (`setup::engine_pin`) are read in `richos-core`, which has no build.rs, so
    // naming them here would declare a dependency for a crate this file cannot speak for. That
    // exposure is real and it is NOT fixed here: `make-release.sh:459` catches it after the
    // fact, by grepping the digest out of the produced executable and refusing to publish.
    // Whether richos-core should gain a build.rs of its own is a question about that crate's
    // deliberate no-build-script shape, not something to answer silently from here.
    println!("cargo::rerun-if-env-changed=RICHOS_SOURCE_SHA");

    check_icons(Path::new("tauri.conf.json"), Path::new("icons"));
    stage_frontend(Path::new("tauri.conf.json"), Path::new("../ui"));
    embed_phone(Path::new("../../web/web-app"));
    tauri_build::build();
}

/// Directory names under `app/ui/` that must never reach a customer's machine.
///
/// `tests` holds the committed Playwright screenshots — MEASURED 2026-08-31 at
/// 22,788,572 bytes across 125 files, against 2,701,315 bytes for everything that
/// actually renders. They are evidence and they stay in the repository; they are not
/// product and they do not ship.
///
/// `node_modules` is here because `app/ui/tests/README.md` tells a developer to run
/// `npm install` in that directory to get Playwright, and `app/.gitignore`'s
/// `ui/node_modules/` rule is anchored one level too high to match it. Untracked and
/// unshipped are different questions; this list answers the second one.
const UI_NOT_SHIPPED: &[&str] = &["tests", "node_modules"];

/// Copy `app/ui` to the directory `tauri.conf.json` names as `build.frontendDist`,
/// leaving `UI_NOT_SHIPPED` behind.
///
/// WHY THIS EXISTS AT ALL. `frontendDist` used to point straight at `../ui`, and
/// `tauri-codegen` walks that directory with `WalkDir::new(&path)` filtered on nothing but
/// `is_dir()` — so every file beneath it, at any depth, is brotli-compressed and embedded
/// into the executable. Not copied into `Contents/Resources`, as was assumed: embedded in
/// the binary itself, hashed into the signature, downloaded by every customer. MEASURED on
/// a release build of `99d508b`: the binary was 35,278,784 bytes and `strings` found asset
/// keys `/tests/shots-splash/material-v13.png` and 75 more inside it.
///
/// WHY IN build.rs AND NOT IN A `beforeBuildCommand`. Only the Tauri CLI runs
/// `beforeBuildCommand`; a plain `cargo build` does not. That would leave the ordinary
/// developer build pointing at a directory nothing had created, and `tauri-codegen`
/// panics on a missing `frontendDist`. build.rs runs for both, before the proc-macro that
/// reads the directory — so there is one staging path and it cannot be bypassed by
/// choosing a different command.
///
/// WHY IT REFUSES RATHER THAN WARNS. A wrong icon is cosmetic, which is why `check_icons`
/// only warns by default. Shipping 21 MB of test evidence to a customer is not cosmetic
/// and there is no artwork to wait on, so every failure below is a panic.
fn stage_frontend(conf_path: &Path, source: &Path) {
    let dist = match frontend_dist(conf_path) {
        Ok(d) => d,
        Err(e) => panic!("\n\nrichos-tauri: {e}\n\n"),
    };

    let dist = Path::new(&dist);

    // Pointing `frontendDist` back at the source tree is the one regression that would
    // silently undo all of this, and it is a one-character edit away. Name it.
    if same_tree(dist, source) {
        panic!(
            "\n\nrichos-tauri: tauri.conf.json sets frontendDist to {dist:?}, which IS the \
             source tree {source:?}. Tauri embeds every file under frontendDist into the \
             binary, and {source:?} carries {:?} — {} of committed test evidence that would \
             then ship to every customer. Point frontendDist at a staged directory; build.rs \
             fills it.\n\n",
            UI_NOT_SHIPPED, "roughly 21 MB"
        );
    }

    if let Err(e) = sync_tree(source, dist, true, UI_NOT_SHIPPED) {
        panic!("\n\nrichos-tauri: could not stage {source:?} into {dist:?}: {e}\n\n");
    }

    // The staging is the guarantee, so it verifies itself rather than trusting its own
    // recursion. An excluded name surviving into the shipped tree is a build failure.
    for name in UI_NOT_SHIPPED {
        let stowaway = dist.join(name);
        if stowaway.exists() {
            panic!(
                "\n\nrichos-tauri: {stowaway:?} exists in the staged frontend after staging \
                 excluded {name:?}. Everything under frontendDist is embedded in the binary \
                 and signed with it. Refusing to build.\n\n"
            );
        }
    }
}

/// Read `build.frontendDist` out of `tauri.conf.json`, relative to this crate's directory.
///
/// Read from Tauri's OWN config for the same reason `derive_requirements` does: the value
/// this function returns has to be the value Tauri will walk, or the staging lands
/// somewhere Tauri never looks and the check passes over a bundle nobody inspected.
fn frontend_dist(conf_path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(conf_path)
        .map_err(|e| format!("{} could not be read: {e}", conf_path.display()))?;
    let conf: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", conf_path.display()))?;

    match conf.get("build").and_then(|b| b.get("frontendDist")) {
        Some(serde_json::Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(format!(
            "{}: build.frontendDist is {other}, not a directory path. This build stages a \
             directory; an array or a dev-server URL needs different handling and must not \
             be silently accepted.",
            conf_path.display()
        )),
        None => Err(format!(
            "{}: build.frontendDist is missing — nothing tells Tauri what to embed.",
            conf_path.display()
        )),
    }
}

/// Whether two paths name the same directory, resolving `..` and symlinks where they
/// exist. A string compare would miss `../ui` against `../ui/`, and `./../ui`.
fn same_tree(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        // `a` not existing yet is the normal first-build case and means it is not `b`,
        // which does exist. Falling back to a lexical compare keeps this honest if
        // neither resolves.
        _ => a == b,
    }
}

/// Mirror `source` into `dest`: copy what differs, delete what no longer belongs, and skip
/// `not_shipped` at the top level.
///
/// Content-compared rather than copied wholesale so an unchanged build does not touch
/// mtimes — `tauri-codegen`'s asset cache is keyed on them, and rewriting 2.6 MB every
/// build would recompile the whole shell every build.
///
/// `not_shipped` is a PARAMETER rather than the one constant it used to read, because two
/// trees are staged now and they exclude different things: `app/ui` leaves its Playwright
/// evidence behind, `web/web-app` leaves its own test suite and its icon generator behind.
/// One list covering both would have to be the union, and a union quietly excludes a name
/// from a tree that never asked for it.
fn sync_tree(source: &Path, dest: &Path, top: bool, not_shipped: &[&str]) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;

    let mut keep: BTreeSet<std::ffi::OsString> = BTreeSet::new();

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();

        if top && not_shipped.iter().any(|x| name == std::ffi::OsStr::new(x)) {
            continue;
        }

        // `.DS_Store` is the one that actually happens: Finder writes it into any directory
        // somebody opens, it is untracked, and it would otherwise be staged and shipped. No
        // dotfile under `app/ui` or `web/web-app` is product — MEASURED at this commit, the only
        // one in either tree is `ui/tests/.gitignore`, which `UI_NOT_SHIPPED` already excludes
        // with the rest of `tests/`.
        if name.to_string_lossy().starts_with('.') {
            continue;
        }

        let from = entry.path();
        let to = dest.join(&name);
        keep.insert(name);

        if entry.file_type()?.is_dir() {
            sync_tree(&from, &to, false, not_shipped)?;
        } else {
            let bytes = fs::read(&from)?;
            // `!=` on the whole file rather than a length or mtime check: the files are
            // small, and a same-size edit is exactly the change a cheaper test misses.
            if fs::read(&to).ok().as_deref() != Some(bytes.as_slice()) {
                fs::write(&to, &bytes)?;
            }
        }
    }

    // Deleting a file from `app/ui` must delete it from the shipped tree. Without this,
    // the staged directory only ever grows, and a removed asset keeps shipping.
    for entry in fs::read_dir(dest)? {
        let entry = entry?;
        if keep.contains(&entry.file_name()) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }

    Ok(())
}

// =====================================================================================
// THE PHONE APP — embedded in this executable, not copied next to it.
// =====================================================================================

/// Names under `web/web-app/` that are the phone app's WORKSHOP and not the phone app.
///
/// `test` is the phone's own node suite plus `stub-mac.js` and `desktop-verify.js`.
/// `bin/make-icons.js` regenerates the four PNGs from source; the PNGs ship, the generator
/// does not. `package.json` exists to give `npm test` a script — the app has no
/// dependencies by design, so it is not a manifest the product reads. The two `.md` files
/// are documentation for whoever works on it.
///
/// A DENYLIST AND NOT AN ALLOWLIST, deliberately, because of which way each one fails. A
/// forgotten denylist entry ships a README nobody reads. A forgotten allowlist entry drops
/// a file the app fetches, and the app breaks on his phone. The first failure is untidy;
/// the second is the defect this whole change exists to end.
/// Files the phone app gets from the desktop app's own tree instead of holding a copy:
/// `(url path, source relative to this crate)`. ONE today, the Oxford comma rule (CEO,
/// 2026-10-19): the desktop and the phone draw what Claude writes through the same filter, and a
/// second copy would be a second rule that drifts. Staged under `$OUT_DIR/phone-shared/`, never
/// inside `$OUT_DIR/phone/`, whose sync deletes whatever `web/web-app` does not hold.
const SHARED_WITH_PHONE: &[(&str, &str)] = &[("lib/oxford-comma.js", "../ui/oxford-comma.js")];
const PHONE_SHARED_DIR: &str = "phone-shared";

const PHONE_NOT_SHIPPED: &[&str] =
    &["test", "bin", "node_modules", "package.json", "README.md", "CONTRACT-STUB.md"];

/// Stage `web/web-app` into `$OUT_DIR/phone` and generate the table `phone::assets` includes.
///
/// # Why the phone app is EMBEDDED rather than declared as a Tauri resource
///
/// Both would put the app inside the shipped `.app`, and `bundle.resources` is the shorter
/// diff — `tauri_build::build()` copies declared resources into the cargo output directory
/// on every build (`tauri-build-2.6.3/src/lib.rs:555-572`) and `resource_dir` returns that
/// same directory on a developer run (`tauri-utils-2.9.3/src/platform.rs:297-302`), so that
/// route would have one lookup too. Both were checked before this one was chosen; the
/// reason is not effort.
///
/// It is that these bytes are **program data served on a socket by our own HTTP stack** —
/// not an operating-system resource, and never a file anything needs to open. Embedding
/// makes three things true by construction that a copy step can only promise:
///
///  1. **The developer run and the shipped bundle serve the SAME BYTES**, because they are
///     the same bytes: one array, compiled in. There is no copy to be stale, absent or
///     modified after signing.
///  2. **There is nothing to find at runtime**, so no path — compile-time or otherwise —
///     is needed to find it. The `env!("CARGO_MANIFEST_DIR")` this replaces put the build
///     machine's home directory into the shipped executable
///     (`/Users/<builder>/…/richos/app/src-tauri`, read out of the 2026-09-18 nightly's
///     release binary with `strings`), and `--remap-path-prefix` could not touch it,
///     because a macro's output is program data rather than compiler metadata.
///  3. **Path traversal stops being a question.** A request resolves by exact key lookup
///     against a fixed table; there is no directory to escape from and no symbolic link to
///     follow.
///
/// And one thing it is NOT: a size problem. The staged tree is about 300 KB against an
/// executable already north of 30 MB.
///
/// # Why in build.rs, and not in `beforeBuildCommand`
///
/// The same reason `stage_frontend` gives: only the Tauri CLI runs `beforeBuildCommand`,
/// and a plain `cargo build` does not. One staging path, and no command that bypasses it.
fn embed_phone(source: &Path) {
    let out_dir = std::env::var_os("OUT_DIR")
        .unwrap_or_else(|| panic!("\n\nrichos-tauri: OUT_DIR is not set; cargo always sets it\n\n"));
    let out_dir = Path::new(&out_dir);
    let staged = out_dir.join("phone");

    if !source.join("index.html").is_file() {
        panic!(
            "\n\nrichos-tauri: {source:?} does not hold the phone app (no index.html). The \
             phone channel serves what this build embeds and nothing else, so a build \
             without it would ship an app whose `/` is a 404 — which is exactly the defect \
             found in the 2026-09-18 nightly. Refusing to build.\n\n"
        );
    }

    if let Err(e) = sync_tree(source, &staged, true, PHONE_NOT_SHIPPED) {
        panic!("\n\nrichos-tauri: could not stage {source:?} into {staged:?}: {e}\n\n");
    }

    // The staging verifies itself rather than trusting its own recursion — the same posture
    // `stage_frontend` takes, and for the same reason: everything staged here is compiled
    // into the binary and signed with it.
    for name in PHONE_NOT_SHIPPED {
        let stowaway = staged.join(name);
        if stowaway.exists() {
            panic!(
                "\n\nrichos-tauri: {stowaway:?} exists in the staged phone app after staging \
                 excluded {name:?}. Refusing to build.\n\n"
            );
        }
    }

    let mut files: Vec<String> = Vec::new();
    if let Err(e) = collect_files(&staged, "", &mut files) {
        panic!("\n\nrichos-tauri: could not walk the staged phone app at {staged:?}: {e}\n\n");
    }
    let shared = out_dir.join(PHONE_SHARED_DIR);
    for (rel, from) in SHARED_WITH_PHONE {
        if files.iter().any(|f| f == rel) {
            panic!(
                "\n\nrichos-tauri: web/web-app holds its own {rel}, which the phone app gets from \
                 {from}. Two copies of one rule are two rules; delete the phone's copy.\n\n"
            );
        }
        let bytes = fs::read(from).unwrap_or_else(|e| {
            panic!("\n\nrichos-tauri: could not read {from}, which the phone app serves as {rel}: {e}\n\n")
        });
        let to = shared.join(rel);
        // Written only when it differs, for the reason `phone_app.rs` is: an unchanged file must
        // not force a recompile of everything that includes it.
        if fs::read(&to).ok().as_deref() != Some(bytes.as_slice()) {
            if let Err(e) = to.parent().map(fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| fs::write(&to, &bytes)) {
                panic!("\n\nrichos-tauri: could not stage {from} as {to:?}: {e}\n\n");
            }
        }
        files.push((*rel).to_string());
    }
    files.sort();

    if files.is_empty() {
        panic!(
            "\n\nrichos-tauri: the staged phone app at {staged:?} is empty. Refusing to \
             build.\n\n"
        );
    }

    check_phone_shell(&staged, &files);

    let mut generated = String::new();
    generated.push_str(
        "// @generated by app/src-tauri/build.rs — the phone app, embedded. Do not edit.\n\
         //\n\
         // `include_bytes!` takes its path as a MACRO ARGUMENT, so no path reaches the\n\
         // binary as data; the bytes do. `env!(\"OUT_DIR\")` is read at compile time and\n\
         // consumed by the same macro, for the same reason.\n\
         pub static FILES: &[(&str, &[u8])] = &[\n",
    );
    for rel in &files {
        // A name that needed escaping would produce a generated literal that is not the
        // file's name. Refuse rather than emit it.
        if !rel
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
        {
            panic!(
                "\n\nrichos-tauri: the phone app carries {rel:?}, whose name is not plain \
                 ASCII `[A-Za-z0-9._-]` and `/`. Every name here becomes a URL the phone \
                 requests and a string literal in generated code. Rename it.\n\n"
            );
        }
        let dir = if SHARED_WITH_PHONE.iter().any(|(r, _)| r == rel) { PHONE_SHARED_DIR } else { "phone" };
        generated.push_str(&format!(
            "    (\"{rel}\", include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{dir}/{rel}\"))),\n"
        ));
    }
    generated.push_str("];\n");

    let table = out_dir.join("phone_app.rs");
    // Written only when it differs, so an unchanged phone app does not force a recompile of
    // everything that includes it.
    if fs::read_to_string(&table).ok().as_deref() != Some(generated.as_str()) {
        if let Err(e) = fs::write(&table, &generated) {
            panic!("\n\nrichos-tauri: could not write {table:?}: {e}\n\n");
        }
    }
}

/// Every file under `root`, as a `/`-separated path relative to it.
fn collect_files(root: &Path, prefix: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(root.join(prefix))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if entry.file_type()?.is_dir() {
            collect_files(root, &rel, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

/// Hold the embedded set to the phone app's OWN list of what it needs.
///
/// `web/web-app/sw.js` names every file the service worker pre-caches, and its comment says
/// why that list is written out rather than discovered: *"a service worker that caches
/// whatever it happens to see is a service worker that serves yesterday's JavaScript to a
/// page that expects today's."* It is therefore the app's own statement of what it is,
/// maintained by whoever edits the app — so this reads it and refuses a build that embeds
/// less than it names.
///
/// This is the check that would have caught the defect it was written for: the 2026-09-18
/// bundle carried NO phone file at all, so every entry below would have been missing.
fn check_phone_shell(staged: &Path, files: &[String]) {
    let sw = staged.join("sw.js");
    let source = match fs::read_to_string(&sw) {
        Ok(s) => s,
        Err(e) => panic!(
            "\n\nrichos-tauri: the staged phone app has no readable service worker at \
             {sw:?}: {e}. It is what makes the app open away from his Mac. Refusing to \
             build.\n\n"
        ),
    };

    let array = source
        .split_once("const SHELL = [")
        .unwrap_or_else(|| {
            panic!(
                "\n\nrichos-tauri: {sw:?} no longer declares `const SHELL = [`. That list is \
                 what this build checks the embedded phone app against; if it moved, point \
                 `check_phone_shell` at where it went rather than deleting the check.\n\n"
            )
        })
        .1
        .split_once("];")
        .unwrap_or_else(|| panic!("\n\nrichos-tauri: {sw:?} has an unterminated SHELL array\n\n"))
        .0;

    // The single-quoted tokens, exactly as `phone/ca.rs` reads the phone's own word list.
    let declared: Vec<&str> = array.split('\'').skip(1).step_by(2).collect();
    if declared.is_empty() {
        panic!("\n\nrichos-tauri: {sw:?} declares an empty SHELL array\n\n");
    }

    let mut missing: Vec<String> = Vec::new();
    for entry in &declared {
        // `/` is the shell itself. Everything else is a root-relative URL.
        let rel = if *entry == "/" { "index.html" } else { entry.trim_start_matches('/') };
        if !files.iter().any(|f| f == rel) {
            missing.push((*entry).to_string());
        }
    }

    // The service worker is fetched by the registration rather than pre-cached, so it names
    // itself nowhere in its own list.
    if !files.iter().any(|f| f == "sw.js") {
        missing.push("/sw.js".to_string());
    }

    if !missing.is_empty() {
        panic!(
            "\n\nrichos-tauri: the phone app embedded in this build is missing {} file(s) \
             that web/web-app/sw.js pre-caches:\n\n{}\n\nEither the file left web/web-app and \
             sw.js still names it, or PHONE_NOT_SHIPPED is excluding something the app \
             needs. Refusing to build.\n\n",
            missing.len(),
            missing.iter().map(|m| format!("  - {m}")).collect::<Vec<_>>().join("\n")
        );
    }
}

/// Windows `.ico` layers Tauri's icon guide specifies.
/// Ref: https://v1.tauri.app/v1/guides/features/icons/ (format unchanged in v2).
/// Verified against `cargo tauri icon` 2.11.4 output: exactly [16, 24, 32, 48, 64, 256].
const ICO_LAYERS: &[u32] = &[16, 24, 32, 48, 64, 256];

/// macOS `.icns` PIXEL sizes (not fourccs — see `icns_sizes` for why).
/// Verified against `cargo tauri icon` 2.11.4 AND Apple's `iconutil`: both cover exactly
/// this set, using different chunk types to do it.
const ICNS_SIZES: &[u32] = &[16, 32, 64, 128, 256, 512, 1024];

/// `tauri icon` emits the unsized `icon.png` at 512x512. The one filename whose required
/// size cannot be read off the name. Ref: https://v2.tauri.app/develop/icons/
const UNSIZED_PNG: &[(&str, u32)] = &[("icon.png", 512)];

enum Kind {
    Png(u32),
    Icns,
    Ico,
}

struct Requirement {
    rel: String,
    kind: Kind,
}

fn check_icons(conf_path: &Path, icons_dir: &Path) {
    let mut failures: Vec<String> = Vec::new();

    let reqs = match derive_requirements(conf_path) {
        Ok(r) => r,
        Err(e) => {
            report(vec![e]);
            return;
        }
    };

    let mut blobs: Vec<(String, Vec<u8>)> = Vec::new();

    for req in &reqs {
        let name = Path::new(&req.rel)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| req.rel.clone());
        let path = icons_dir.join(&name);

        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => {
                failures.push(format!(
                    "{} is declared in tauri.conf.json bundle.icon but does not exist on disk",
                    req.rel
                ));
                continue;
            }
        };

        match req.kind {
            Kind::Png(want) => match png_dimensions(&bytes) {
                Some((w, h)) if w == want && h == want => {}
                Some((w, h)) => failures.push(format!(
                    "{} is named for {want}x{want} but is actually {w}x{h} — Tauri ships \
                     bundle icons as-is, it does not resize them to match their filename",
                    req.rel
                )),
                None => failures.push(format!("{} is not a readable PNG (bad IHDR)", req.rel)),
            },
            Kind::Icns => {
                let got = icns_sizes(&bytes);
                if got.is_empty() {
                    failures.push(format!("{} is not a readable .icns archive", req.rel));
                } else {
                    let missing: Vec<u32> = ICNS_SIZES
                        .iter()
                        .copied()
                        .filter(|s| !got.contains(s))
                        .collect();
                    if !missing.is_empty() {
                        failures.push(format!(
                            "{} is missing layer size(s) {missing:?}px (has {:?})",
                            req.rel,
                            got.iter().copied().collect::<Vec<_>>()
                        ));
                    }
                }
            }
            Kind::Ico => match ico_sizes(&bytes) {
                None => failures.push(format!("{} is not a readable .ico", req.rel)),
                Some(got) => {
                    let missing: Vec<u32> = ICO_LAYERS
                        .iter()
                        .copied()
                        .filter(|s| !got.contains(s))
                        .collect();
                    if !missing.is_empty() {
                        failures.push(format!(
                            "{} is missing layer size(s) {missing:?}px (has {:?})",
                            req.rel,
                            got.iter().copied().collect::<Vec<_>>()
                        ));
                    }
                }
            },
        }

        blobs.push((req.rel.clone(), bytes));
    }

    // The exact defect this gate was written against: every required file was the same
    // file. Independently rendered artefacts are never byte-identical.
    for i in 0..blobs.len() {
        for j in (i + 1)..blobs.len() {
            if blobs[i].1 == blobs[j].1 {
                failures.push(format!(
                    "{} is byte-identical to {} — these are supposed to be independently \
                     rendered artefacts, not copies of one placeholder",
                    blobs[j].0, blobs[i].0
                ));
            }
        }
    }

    report(failures);
}

fn report(failures: Vec<String>) {
    if failures.is_empty() {
        return;
    }

    // Compilation must not be held hostage to a cosmetic bundling asset. A missing icon
    // cannot make the binary wrong, and the icons are blocked on artwork the CEO supplies
    // (open-items 3.12) — so a hard failure here would freeze all app development for an
    // indefinite wait. Warn on every build; fail hard only where the icon actually ships,
    // which callers signal with RICHOS_REQUIRE_REAL_ICONS=1.
    //
    // THE CALLER NOW EXISTS (2026-08-30). `richos/app/scripts/package-app.sh` exports
    // RICHOS_REQUIRE_REAL_ICONS=1 before it bundles, and refuses rather than warns.
    // It also runs the generator's own `verify` first, purely so a placeholder set
    // costs two seconds instead of a full release compile — that pre-flight is
    // convenience; THIS is the guarantee, and the difference was proven rather than
    // asserted: with the pre-flight deleted from the script and `icons/32x32.png`
    // resized to 16x16, the packaging run still stopped at the panic below and
    // exited 4 with nothing packaged.
    //
    // STILL TRUE, and still why this is only a warning by default: no CI job builds
    // this crate. `.github/workflows/` holds THREE workflows — `app-spine-ci.yml`,
    // `engine-self-verify.yml`, `windows-companion-ci.yml` — and `app-spine-ci.yml`'s
    // own header excludes `app/src-tauri` by name, as "a deliberately detached
    // workspace with the whole webview dependency tree behind it". (The sentence this
    // replaces said the only workflow was engine-self-verify.yml. That had been wrong
    // since the other two landed, and its CONCLUSION — nothing builds the app —
    // survived the correction.) So an ordinary `cargo build` here must stay
    // non-fatal.
    let strict = std::env::var("RICHOS_REQUIRE_REAL_ICONS").as_deref() == Ok("1");

    if !strict {
        for line in &failures {
            println!("cargo::warning={}", line);
        }
        println!(
            "cargo::warning=app icons are still placeholders — the binary builds, but a \
             bundle produced now would ship no real icon. Fix: \
             richos/app/scripts/generate-app-icons.sh <artwork.png>. Set \
             RICHOS_REQUIRE_REAL_ICONS=1 to make this fatal, which is what \
             richos/app/scripts/package-app.sh does before it bundles."
        );
        return;
    }

    panic!(
        "\n\nrichos-tauri: app icons are not shippable — refusing to build.\n\n{}\n\n\
         Fix: run\n\n    richos/app/scripts/generate-app-icons.sh /path/to/artwork.png\n\n\
         which generates every artefact tauri.conf.json declares from ONE square PNG of \
         at least 1024x1024 with a transparent background. See richos/app/README.md's \"App \
         icon\" section for the full source spec. This is not something to paper over \
         with new placeholder art.\n\n",
        failures
            .iter()
            .map(|f| format!("  - {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Read Tauri's OWN `bundle.icon` list and turn it into checkable requirements.
fn derive_requirements(conf_path: &Path) -> Result<Vec<Requirement>, String> {
    let text = fs::read_to_string(conf_path)
        .map_err(|e| format!("{} could not be read: {e}", conf_path.display()))?;
    let conf: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", conf_path.display()))?;

    let entries = conf
        .get("bundle")
        .and_then(|b| b.get("icon"))
        .and_then(|i| i.as_array())
        .ok_or_else(|| {
            format!(
                "{}: bundle.icon is missing or not an array. Tauri bundles exactly what \
                 this array lists, so its absence means the app ships with no icon.",
                conf_path.display()
            )
        })?;

    if entries.is_empty() {
        return Err(format!(
            "{}: bundle.icon is empty — the app would ship with no icon.",
            conf_path.display()
        ));
    }

    let mut reqs = Vec::new();
    for entry in entries {
        let rel = entry
            .as_str()
            .ok_or_else(|| format!("bundle.icon entry is not a string: {entry}"))?;
        let name = Path::new(rel)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let lower = name.to_ascii_lowercase();

        if lower.ends_with(".png") {
            if let Some(size) = png_size_from_name(&name) {
                reqs.push(Requirement {
                    rel: rel.to_string(),
                    kind: Kind::Png(size),
                });
            } else if let Some((_, size)) = UNSIZED_PNG.iter().find(|(n, _)| *n == lower) {
                reqs.push(Requirement {
                    rel: rel.to_string(),
                    kind: Kind::Png(*size),
                });
            } else {
                return Err(format!(
                    "bundle.icon entry {rel:?}: cannot derive a required pixel size from \
                     this filename. Name it `<N>x<N>.png` or `<N>x<N>@<M>x.png`. This \
                     gate will not guess a size and let a wrong icon ship."
                ));
            }
        } else if lower.ends_with(".icns") {
            reqs.push(Requirement {
                rel: rel.to_string(),
                kind: Kind::Icns,
            });
        } else if lower.ends_with(".ico") {
            reqs.push(Requirement {
                rel: rel.to_string(),
                kind: Kind::Ico,
            });
        } else {
            return Err(format!(
                "bundle.icon entry {rel:?}: unsupported extension. This gate knows .png, \
                 .icns and .ico. An unknown type must stop the build rather than be skipped."
            ));
        }
    }
    Ok(reqs)
}

/// `32x32.png` -> 32, `128x128@2x.png` -> 256. Returns None if the name does not encode
/// a square size, so the caller can refuse to guess rather than invent one.
fn png_size_from_name(name: &str) -> Option<u32> {
    let stem = name.strip_suffix(".png").or_else(|| name.strip_suffix(".PNG"))?;
    let (dims, mult) = match stem.split_once('@') {
        Some((d, m)) => (d, m.strip_suffix('x').or_else(|| m.strip_suffix('X'))?),
        None => (stem, "1"),
    };
    let (w, h) = dims.split_once('x')?;
    let w: u32 = w.parse().ok()?;
    let h: u32 = h.parse().ok()?;
    let mult: u32 = mult.parse().ok()?;
    if w != h || w == 0 || mult == 0 {
        return None;
    }
    w.checked_mul(mult)
}

/// Minimal PNG width/height reader: no `image` crate needed for a build-time check.
/// PNG layout: 8-byte signature, then the first chunk is always IHDR (length:4,
/// "IHDR":4, width:4 big-endian, height:4 big-endian, ...).
/// Ref: PNG spec section 11.2.2 (https://www.w3.org/TR/png/#11IHDR).
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || bytes[0..8] != SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

/// Pixel sizes present in an `.icns`, keyed by chunk fourcc.
///
/// We deliberately check PIXEL SIZES, not fourccs. Several fourccs encode the same size,
/// and encoders legitimately differ on which they pick: `cargo tauri icon` writes
/// `is32`/`il32` (the legacy 24-bit RLE variants) for 16/32px, while Apple's own
/// `iconutil` writes the ARGB `ic04`/`ic05` instead. Both cover 16 and 32px. Requiring a
/// literal fourcc list would reject output from Apple's own tool — a checker bug, not an
/// artwork bug. Layout: "icns", total length, then [fourcc:4][length:4][data] chunks.
fn icns_sizes(bytes: &[u8]) -> BTreeSet<u32> {
    let mut found = BTreeSet::new();
    if bytes.len() < 8 || &bytes[0..4] != b"icns" {
        return found;
    }
    let mut off = 8usize;
    while off + 8 <= bytes.len() {
        let fourcc = &bytes[off..off + 4];
        let len = u32::from_be_bytes(bytes[off + 4..off + 8].try_into().unwrap()) as usize;
        if len < 8 {
            break;
        }
        let size = match fourcc {
            b"is32" | b"s8mk" | b"ic04" | b"icp4" => Some(16),
            b"il32" | b"l8mk" | b"ic05" | b"ic11" | b"icp5" => Some(32),
            b"ic12" | b"icp6" => Some(64),
            b"ih32" | b"h8mk" => Some(48),
            b"ic07" | b"it32" | b"t8mk" => Some(128),
            b"ic08" | b"ic13" => Some(256),
            b"ic09" | b"ic14" => Some(512),
            b"ic10" => Some(1024),
            _ => None,
        };
        if let Some(s) = size {
            found.insert(s);
        }
        off += len;
    }
    found
}

/// Layer widths in a Windows `.ico` directory.
/// Layout: reserved:2 (0), type:2 (1 = icon), count:2, then `count` 16-byte entries whose
/// first byte is the width — 0 meaning 256, since the field is a single byte.
/// Ref: https://learn.microsoft.com/en-us/previous-versions/ms997538(v=msdn.10)
fn ico_sizes(bytes: &[u8]) -> Option<BTreeSet<u32>> {
    if bytes.len() < 6 {
        return None;
    }
    let reserved = u16::from_le_bytes(bytes[0..2].try_into().ok()?);
    let kind = u16::from_le_bytes(bytes[2..4].try_into().ok()?);
    let count = u16::from_le_bytes(bytes[4..6].try_into().ok()?) as usize;
    if reserved != 0 || kind != 1 || count == 0 {
        return None;
    }
    if bytes.len() < 6 + count * 16 {
        return None;
    }
    let mut sizes = BTreeSet::new();
    for i in 0..count {
        let w = bytes[6 + i * 16];
        sizes.insert(if w == 0 { 256 } else { w as u32 });
    }
    Some(sizes)
}
