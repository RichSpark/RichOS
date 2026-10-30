#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use richos_core::runtime::EngineRuntime;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::os::unix::fs::PermissionsExt;

struct Delivery(PathBuf);
impl Delivery {
    fn new() -> Self {
        Self::at(std::env::temp_dir().join(format!("richos runtime test {}", uuid::Uuid::new_v4())))
    }

    /// A delivery in `root/runtime`, which is removed with `root` when the fixture drops.
    fn at(root: PathBuf) -> Self {
        let runtime = root.join("runtime");
        std::fs::create_dir_all(runtime.join("bin")).unwrap();
        std::fs::create_dir_all(runtime.join("git/libexec/git-core")).unwrap();
        let mut files = serde_json::Map::new();
        for name in ["bin/python3", "bin/node", "bin/git", "bin/jq", "bin/ffmpeg", "bin/ffprobe", "git/libexec/git-core/git-remote-https"] {
            let content = b"#!/bin/sh\nexit 0\n";
            std::fs::write(runtime.join(name), content).unwrap();
            std::fs::set_permissions(runtime.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
            files.insert(name.into(), json!(format!("{:x}", Sha256::digest(content))));
        }
        std::fs::write(runtime.join("delivery.json"), serde_json::to_vec(&json!({
            "schema":1,"platform":"aarch64-apple-darwin","versions":{},"files":files,"links":{}
        })).unwrap()).unwrap();
        Self(root)
    }

    /// Add one more delivered file, listed in `delivery.json` with its digest.
    fn deliver(&self, name: &str, content: &[u8]) {
        let path = self.0.join("runtime").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        let manifest = self.0.join("runtime/delivery.json");
        let mut delivery: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
        delivery["files"][name] = json!(format!("{:x}", Sha256::digest(content)));
        std::fs::write(&manifest, serde_json::to_vec(&delivery).unwrap()).unwrap();
    }

    /// Write a file the delivery does NOT list, creating its directories.
    fn stray(&self, name: &str, content: &[u8]) {
        let path = self.0.join("runtime").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
}
impl Drop for Delivery { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

/// The standard library's own caches, by the names the CEO's engines carried on 2026-10-29.
const STDLIB: &str = "python/lib/python3.13";
const DELIVERED_CACHE: &str = "python/lib/python3.13/encodings/__pycache__/aliases.cpython-313.pyc";

/// Do to a delivery exactly what one Python run from its interpreter without `-B` did to the
/// CEO's installed engine: recompile the delivered cache relocation invalidated, and write new
/// caches beside the standard library (plus importlib's mid-write temporary, which a killed run
/// leaves behind).
fn run_python_without_the_launcher(fixture: &Delivery) {
    std::fs::write(fixture.0.join("runtime").join(DELIVERED_CACHE), b"recompiled after relocation").unwrap();
    for cache in ["__pycache__/argparse.cpython-313.pyc", "__pycache__/subprocess.cpython-313.pyc",
        "json/__pycache__/decoder.cpython-313.pyc", "re/__pycache__/_parser.cpython-313.pyc",
        "__pycache__/tempfile.cpython-313.pyc.4372811920"] {
        fixture.stray(&format!("{STDLIB}/{cache}"), b"bytecode the interpreter wrote for itself");
    }
}

/// **THE CEO'S STARTUP SHEET, 2026-10-29.** An installed, pinned, working engine was refused at
/// every start with *"runtime delivery has missing or unexpected files"* because a Python run
/// had cached bytecode inside its own runtime, and the window asked him to set it up again.
/// Bytecode the interpreter writes for itself is not the delivery; a ready engine stays ready.
#[test]
fn bytecode_the_delivered_interpreter_caches_for_itself_leaves_a_ready_engine_ready() {
    let fixture = Delivery::new();
    fixture.deliver(&format!("{STDLIB}/encodings/aliases.py"), b"# the source, hashed as ever\n");
    fixture.deliver(DELIVERED_CACHE, b"upstream bytecode");
    EngineRuntime::load(&fixture.0, None).expect("the delivery as installed is ready");
    run_python_without_the_launcher(&fixture);
    EngineRuntime::load(&fixture.0, None).expect("caches the interpreter wrote must not unready the engine");
    // A cache the interpreter (or a cleanup) removed is not a missing delivery either.
    std::fs::remove_file(fixture.0.join("runtime").join(DELIVERED_CACHE)).unwrap();
    EngineRuntime::load(&fixture.0, None).expect("a removed cache must not unready the engine");
}

/// ...and NOTHING else is forgiven: only a regular `.pyc` (or importlib's numbered temporary of
/// one) directly inside a `__pycache__` under `python/`.
#[test]
fn only_the_interpreters_own_caches_are_forgiven() {
    for stray in [
        format!("{STDLIB}/argparse.pyc"),                         // a cache outside __pycache__
        format!("{STDLIB}/__pycache__/sitecustomize.py"),         // source dropped in a cache dir
        format!("{STDLIB}/__pycache__/argparse.cpython-313.pyc.tmp"), // not importlib's temporary
        format!("{STDLIB}/__pycache__/.pyc"),                     // no module name
        "bin/__pycache__/argparse.cpython-313.pyc".to_string(),   // outside the Python tree
        format!("{STDLIB}/__pycache__/deeper/argparse.cpython-313.pyc"),
    ] {
        let fixture = Delivery::new();
        fixture.stray(&stray, b"fictional only");
        let refused = EngineRuntime::load(&fixture.0, None).expect_err(&stray).to_string();
        assert!(refused.contains("unexpected"), "{stray}: {refused}");
    }
    // A symlinked cache is not a cache the interpreter wrote.
    let fixture = Delivery::new();
    let link = fixture.0.join("runtime").join(STDLIB).join("__pycache__/argparse.cpython-313.pyc");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(fixture.0.join("runtime/bin/git"), &link).unwrap();
    let refused = EngineRuntime::load(&fixture.0, None).unwrap_err().to_string();
    assert!(refused.contains("unexpected"), "{refused}");
    // The source a cache is compiled from is still hashed.
    let fixture = Delivery::new();
    fixture.deliver(&format!("{STDLIB}/encodings/aliases.py"), b"# the source\n");
    std::fs::write(fixture.0.join("runtime").join(STDLIB).join("encodings/aliases.py"), b"# changed\n").unwrap();
    let refused = EngineRuntime::load(&fixture.0, None).unwrap_err().to_string();
    assert!(refused.contains("changed"), "{refused}");
}

/// **The sheet itself.** The first-run detector, asked with the CEO's exact state: the engine
/// RichOS installed, from the asset this build pins, after a Python run cached bytecode in it.
/// It must find the engine, which is what keeps "There's a bit of setting up to do first" off
/// his screen (`SetupStatus::needs` is empty of the engine exactly when `present`).
#[test]
fn first_run_setup_does_not_ask_for_an_engine_whose_python_cached_bytecode() {
    use richos_core::setup::{engine_install_dir, find_engine_demanded, EngineDemand, EnginePin, SetupPaths};
    let home = std::env::temp_dir().join(format!("richos runtime test home {}", uuid::Uuid::new_v4()));
    let engine = engine_install_dir(&home);
    let fixture = Delivery::at(engine.clone());
    let _cleanup = Delivery(home.clone());
    std::fs::create_dir_all(engine.join("scripts/hooks")).unwrap();
    std::fs::write(engine.join("VERSION"), "1.2.0\n").unwrap();
    let pin = EnginePin {
        version: "1.2.0".into(),
        url: "https://example.invalid/richos-engine.tar.gz".into(),
        sha256: "9ddce07265cdf449f9bdc9fda55e7aff4cf107f501a5f45261b3187d6bb403d9".into(),
    };
    std::fs::write(engine.join("INSTALLED-FROM"), format!("engine 1.2.0\nsha256 {}\n", pin.sha256)).unwrap();
    fixture.deliver(DELIVERED_CACHE, b"upstream bytecode");
    let paths = SetupPaths { home: Some(home.clone()), ..SetupPaths::default() };
    // The runtime half of `setup::engine_is_usable` (`runtime::verify_engine` ends in this
    // load; its component-entry half reads nothing a Python run writes).
    let usable = |dir: &std::path::Path| EngineRuntime::load(dir, None).map(|_| ()).map_err(|e| e.to_string());
    let demand = EngineDemand::pinned(Some(&pin));
    assert!(find_engine_demanded(&paths, &[], &usable, demand).present, "the engine as installed");
    run_python_without_the_launcher(&fixture);
    let after = find_engine_demanded(&paths, &[], &usable, demand);
    assert!(after.present, "first-run setup asked again for a ready engine: {:?}", after.looked_in);
}

#[test]
fn delivery_uses_selected_engine_and_never_searches_host_path() {
    let fixture = Delivery::new();
    let loaded = EngineRuntime::load(&fixture.0, None).unwrap();
    assert_eq!(loaded.python, std::fs::canonicalize(&fixture.0).unwrap().join("runtime/bin/python3"));
    assert!(!loaded.path().contains("homebrew"));
    assert!(EngineRuntime::load(&fixture.0.join("missing"), None).is_err());
}

#[test]
fn changed_or_unlisted_content_prevents_runtime_activation() {
    let fixture = Delivery::new();
    std::fs::write(fixture.0.join("runtime/private-fixture.txt"), "fictional only").unwrap();
    assert!(EngineRuntime::load(&fixture.0, None).unwrap_err().to_string().contains("unexpected"));
    std::fs::remove_file(fixture.0.join("runtime/private-fixture.txt")).unwrap();
    std::fs::write(fixture.0.join("runtime/bin/git"), "changed").unwrap();
    assert!(EngineRuntime::load(&fixture.0, None).unwrap_err().to_string().contains("changed"));
}

#[test]
fn unreadable_or_nonexecutable_delivery_is_not_ready() {
    let fixture = Delivery::new();
    std::fs::set_permissions(fixture.0.join("runtime/bin/python3"), std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(EngineRuntime::load(&fixture.0, None).unwrap_err().to_string().contains("not executable"));
}

#[test]
fn escaping_symlink_cannot_supply_a_delivered_command() {
    let fixture = Delivery::new();
    std::fs::remove_file(fixture.0.join("runtime/bin/git")).unwrap();
    std::os::unix::fs::symlink("/usr/bin/git", fixture.0.join("runtime/bin/git")).unwrap();
    assert!(EngineRuntime::load(&fixture.0, None).unwrap_err().to_string().contains("escapes"));
}

#[test]
fn a_delivery_without_ffmpeg_or_ffprobe_is_refused_even_when_its_inventory_agrees() {
    for tool in ["bin/ffmpeg", "bin/ffprobe"] {
        let fixture = Delivery::new();
        let manifest = fixture.0.join("runtime/delivery.json");
        let mut delivery: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
        delivery["files"].as_object_mut().unwrap().remove(tool);
        std::fs::write(&manifest, serde_json::to_vec(&delivery).unwrap()).unwrap();
        std::fs::remove_file(fixture.0.join("runtime").join(tool)).unwrap();
        let refused = EngineRuntime::load(&fixture.0, None).unwrap_err().to_string();
        assert!(refused.contains(&format!("runtime is missing {tool}")), "{refused}");
    }
}

#[test]
fn a_delivery_already_verified_is_not_read_and_hashed_again_but_a_touched_file_is() {
    let fixture = Delivery::new();
    let root = std::fs::canonicalize(fixture.0.join("runtime")).unwrap();
    assert_eq!(richos_core::runtime::files_hashed_under(&root), 0);
    EngineRuntime::load(&fixture.0, None).unwrap();
    assert_eq!(richos_core::runtime::files_hashed_under(&root), 7, "the first load reads every delivered file");
    for _ in 0..3 {
        EngineRuntime::load(&fixture.0, None).unwrap();
    }
    assert_eq!(richos_core::runtime::files_hashed_under(&root), 7, "later connections reuse the verified result");
    // Everything else is still checked on every load: an unlisted file is refused...
    std::fs::write(fixture.0.join("runtime/stray.txt"), "fictional only").unwrap();
    assert!(EngineRuntime::load(&fixture.0, None).unwrap_err().to_string().contains("unexpected"));
    std::fs::remove_file(fixture.0.join("runtime/stray.txt")).unwrap();
    // ...and so is a file rewritten with different bytes of the very same length.
    std::fs::write(fixture.0.join("runtime/bin/git"), b"#!/bin/sh\nexit 1\n").unwrap();
    assert!(EngineRuntime::load(&fixture.0, None).unwrap_err().to_string().contains("changed"));
    assert_eq!(richos_core::runtime::files_hashed_under(&root), 7, "a failed read is not remembered as verified");
}
