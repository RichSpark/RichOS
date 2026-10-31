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
/// `not_shipped` is a parameter: the one tree staged here, `app/ui`, passes `UI_NOT_SHIPPED`.
/// (A second tree, the phone web page, was staged beside it until that page was retired on
/// 2026-10-30.)
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
        // dotfile under `app/ui` is product — MEASURED at the commit that wrote this, the only
        // one in the tree is `ui/tests/.gitignore`, which `UI_NOT_SHIPPED` already excludes
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
