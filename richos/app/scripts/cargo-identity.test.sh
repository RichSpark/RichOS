#!/usr/bin/env bash
# run-tests: no-host-screen: builds and runs tiny Rust fixtures with no UI or network
# run-tests: inputs richos/app/scripts/lib/machine_storage.py richos/engine/scripts/lib/machine_settings.py richos/app/scripts/cargo-identity.test.sh richos/app/scripts/bin/cargo richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/cargo-cache-env.sh richos/app/scripts/testvm/reserve.py
# run-tests: covers richos/app/scripts/lib/machine_storage.py richos/app/scripts/bin/cargo richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/cargo-cache-env.sh richos/app/scripts/testvm/reserve.py
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib/cargo-cache-env.sh"
python3 -B - "$HERE" <<'PY'
import concurrent.futures
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import shutil
import hashlib

here = Path(sys.argv[1])
sys.path.insert(0, str(here / "lib"))
import cargo_identity
wrapper = here / "bin/cargo"
scratch = Path(tempfile.mkdtemp(prefix="cargo-identity.", dir=os.environ["TMPDIR"]))
try:
    settings_home = scratch / "home with spaces"
    settings_file = settings_home / "Library/Application Support/RichOS/.env"
    settings_file.parent.mkdir(parents=True)
    workspace = scratch / "settings workspace"; workspace.mkdir()
    (workspace / "Cargo.toml").write_text('[workspace]\nmembers=[]\n')
    file_cache = scratch / "file cache"; alias_cache = scratch / "alias cache"
    settings_file.write_text('CARGO_TARGET_DIR=' + str(file_cache) + '\n')
    supplied = {'HOME': str(settings_home), 'TMPDIR': str(scratch)}
    assert cargo_identity.target_dir(workspace, supplied)[0] == file_cache
    assert cargo_identity.target_dir(workspace, {**supplied, 'RICHOS_CARGO_CACHE_ROOT': str(alias_cache)})[0] == alias_cache
    snapshot = cargo_identity.M.environment(supplied)
    settings_file.write_text('UNKNOWN=/invalid\n')
    assert cargo_identity.target_dir(workspace, snapshot)[0] == file_cache
    assert cargo_identity.target_dir(workspace, snapshot, base=str(alias_cache))[0] == alias_cache
    print("PASS private Cargo settings, alias precedence, spaces and job snapshot")
    missing = dict(HOME=str(scratch / "no-cargo-home"), PATH="/usr/bin:/bin")
    assert shutil.which("cargo", path=cargo_identity.enable_environment(missing)["PATH"]) is None
    missing_shell = subprocess.run(["/bin/bash", "-c", '. "$1"; command -v cargo', "fixture",
                                   str(here / "lib/cargo-cache-env.sh")], env=missing,
                                  capture_output=True, text=True, timeout=30)
    assert missing_shell.returncode != 0 and not missing_shell.stdout
    print("PASS a missing Rust toolchain stays missing for host capability checks")
    environment = dict(os.environ, CARGO_TARGET_DIR=str(scratch / "shared"))
    environment.pop("RICHOS_CARGO_CACHE_ROOT", None)
    dependency = scratch / "shared-dependency"
    (dependency / "src").mkdir(parents=True)
    (dependency / "Cargo.toml").write_text('[package]\nname="common_fixture"\nversion="0.1.0"\nedition="2021"\n')
    (dependency / "src/lib.rs").write_text('pub fn ready() {}\n')
    # A real Cargo registry source, replaced by an immutable local fixture.
    # Includes a build script and generated Rust, so compiler-cache-only reuse
    # would not satisfy the test. No download or prepopulated registry needed.
    vendor = scratch / "vendor" / "thirdparty_fixture"
    (vendor / "src").mkdir(parents=True)
    (vendor / "Cargo.toml").write_text('[package]\nname="thirdparty_fixture"\nversion="1.0.0"\nedition="2021"\n')
    (vendor / "build.rs").write_text('fn main() { std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("ready.rs"), "pub fn ready() {}\\n").unwrap(); }\n')
    (vendor / "src/lib.rs").write_text('include!(concat!(env!("OUT_DIR"), "/ready.rs"));\n')
    (vendor / ".cargo-checksum.json").write_text(json.dumps({"package": "0" * 64,
        "files": {str(p.relative_to(vendor)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in vendor.rglob("*") if p.is_file()}}))
    roots = []
    for label in ("alpha", "beta"):
        root = scratch / label
        (root / "src").mkdir(parents=True)
        (root / ".cargo").mkdir()
        (root / ".cargo/config.toml").write_text('[source.crates-io]\nreplace-with="fixture"\n[source.fixture]\ndirectory=' + json.dumps(str(vendor.parent)) + '\n')
        (root / "Cargo.toml").write_text('[package]\nname="identity_fixture"\nversion="0.1.0"\nedition="2021"\n[lib]\ncrate-type=["rlib", "cdylib", "staticlib"]\n[dependencies]\ncommon_fixture={path="../shared-dependency"}\nthirdparty_fixture="=1.0.0"\n')
        (root / "src/lib.rs").write_text(f'pub fn identity() -> &\'static str {{ common_fixture::ready(); thirdparty_fixture::ready(); "{label}" }}\n'
            f'#[no_mangle] pub extern "C" fn fixture_identity() -> i32 {{ {1 if label == "alpha" else 2} }}\n'
            f'#[test] fn own_source() {{ assert_eq!(identity(), "{label}"); }}\n')
        (root / "src/main.rs").write_text('fn main() { println!("{}", identity_fixture::identity()); }\n')
        # The other checkout's artifact will be newer than every source here.
        for path in root.rglob("*"):
            os.utime(path, (946684800, 946684800))
        roots.append(root)

    def cargo(root, *args, env=None):
        result = subprocess.run([str(wrapper), *args], cwd=root, env=env or environment,
                                capture_output=True, text=True, timeout=120)
        if result.returncode:
            raise AssertionError(result.stdout + result.stderr)
        return result

    # This probe checks WHICH checkout Cargo runs through reserve.py, not whether the Mac is quiet. The
    # nightly runs every gate at once (93-99% CPU), so the 80% line refused it after 301 s; --low-priority
    # keeps the reserve.py route and the memory rule but skips the CPU line (the CPU rule has its own tests).
    admitted = subprocess.run([sys.executable, "-B", str(here / "testvm/reserve.py"), "--low-priority", "--wait", "300", "--",
                               "cargo", "run", "--offline", "--quiet", "--manifest-path",
                               str(roots[1] / "Cargo.toml"), "--config", str(roots[1] / ".cargo/config.toml"),
                               "--target-dir", str(scratch / "admitted")], cwd=here, env=environment,
                              capture_output=True, text=True, timeout=360)
    assert admitted.returncode == 0, admitted.stdout + admitted.stderr
    assert admitted.stdout.strip() == "beta", admitted.stdout
    print("PASS direct Cargo through reserve.py executes the requested checkout")

    executables = []
    registry_artifacts = []
    for root in (roots[0], roots[1], roots[0]):
        label = root.name
        result = cargo(root, "test", "--offline", "--lib", "--message-format=json")
        artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
        registry = [a for a in artifacts if a.get("reason") == "compiler-artifact" and a["package_id"].startswith("registry+")]
        assert registry and any(a["target"]["kind"] == ["custom-build"] for a in registry), registry
        registry_artifacts.append(registry)
        tests = [a["executable"] for a in artifacts if a.get("reason") == "compiler-artifact" and a.get("executable")]
        assert len(tests) == 1, tests
        executables.append(tests[0])
        assert cargo(root, "run", "--offline", "--quiet").stdout.strip() == label
        assert cargo(root, "run", "--offline", "--quiet", "--", "--help", "--target-dir", "ignored").stdout.strip() == label
        # Tauri-style output: Cargo normally omits cdylib filename hashes.
        # Loading it in a separate process catches an A/B/A overwritten fresh
        # library without the dynamic loader reusing a previous handle.
        built = cargo(root, "build", "--offline", "--lib", "--message-format=json")
        libraries = [json.loads(line) for line in built.stdout.splitlines() if line.startswith("{")]
        dynamic = next(p for a in libraries if a.get("reason") == "compiler-artifact"
                       and a["target"]["name"] == "identity_fixture" for p in a["filenames"]
                       if p.endswith((".dylib", ".so", ".dll")))
        loaded = subprocess.run([sys.executable, "-B", "-c", "import ctypes,sys; print(ctypes.CDLL(sys.argv[1]).fixture_identity())", dynamic],
                                capture_output=True, text=True, timeout=30, check=True)
        assert loaded.stdout.strip() == ("1" if label == "alpha" else "2"), loaded.stdout
        print("PASS old-mtime checkout builds and runs its own library and binary:", label)
    assert executables[0] != executables[1], executables
    assert executables[0] == executables[2], executables
    assert all(not a["fresh"] for a in registry_artifacts[0]), registry_artifacts[0]
    assert all(a["fresh"] for a in registry_artifacts[1]), registry_artifacts[1]
    assert [a["filenames"] for a in registry_artifacts[0]] == [a["filenames"] for a in registry_artifacts[1]]
    print("PASS second workspace's first test reuses every third-party unit including its build script and generated library")
    print("PASS Tauri-style cdylib and staticlib outputs retain A/B/A source identity")

    # Cargo retains its own locking and parallelism. Different checkout outputs
    # cannot replace one another after the build lock is released.
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        futures = [executor.submit(cargo, root, "run", "--offline", "--quiet") for root in roots]
        assert [f.result().stdout.strip() for f in futures] == ["alpha", "beta"]
    print("PASS concurrent checkouts execute distinct application binaries")
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        cold = [executor.submit(cargo, root, "test", "--offline", "--lib", "--message-format=json",
                                "--target-dir", str(scratch / "cold-concurrent")) for root in roots]
        counts = []
        for future in cold:
            rows = [json.loads(line) for line in future.result().stdout.splitlines() if line.startswith("{")]
            registry = [a for a in rows if a.get("reason") == "compiler-artifact" and a["package_id"].startswith("registry+")]
            counts.append(sum(not a["fresh"] for a in registry))
        assert sorted(counts) == [0, 2], counts
    print("PASS simultaneous cold workspaces compile the third-party library and build script once")
    changed = dict(environment, RUSTFLAGS="--cfg richos_dependency_probe")
    changed.pop("CARGO_ENCODED_RUSTFLAGS", None)
    rebuilt = cargo(roots[0], "test", "--offline", "--lib", "--message-format=json", env=changed)
    rows = [json.loads(line) for line in rebuilt.stdout.splitlines() if line.startswith("{")]
    assert all(not a["fresh"] for a in rows if a.get("reason") == "compiler-artifact" and a["package_id"].startswith("registry+"))
    print("PASS changed compiler flags rebuild dependency units instead of reusing incompatible output")

    for flag in (("--target-dir", str(scratch / "explicit")), ("--target-dir=" + str(scratch / "explicit"),)):
        targets = [json.loads(cargo(root, "metadata", "--offline", "--no-deps", "--format-version=1", *flag).stdout)["target_directory"] for root in roots]
        assert targets[0] != targets[1], targets
        assert all(str(scratch / "explicit") in p for p in targets)
    print("PASS inherited and explicit target roots retain separate workspace namespaces")

    # A relocated fixture in the same process needs a new namespace too.
    copied = scratch / "copied"
    shutil.copytree(roots[0], copied)
    assert json.loads(cargo(copied, "metadata", "--offline", "--no-deps", "--format-version=1").stdout)["target_directory"] != json.loads(cargo(roots[0], "metadata", "--offline", "--no-deps", "--format-version=1").stdout)["target_directory"]
    print("PASS copied fixture workspace gets its own artifacts")
    # A detached or excluded package is its own Cargo workspace, even when
    # nested below another manifest. It cannot inherit that parent's output.
    (roots[0] / "Cargo.toml").write_text((roots[0] / "Cargo.toml").read_text() + '\n[workspace]\nexclude=["excluded"]\n')
    excluded = roots[0] / "excluded"
    (excluded / "src").mkdir(parents=True)
    (excluded / "Cargo.toml").write_text('[package]\nname="excluded_fixture"\nversion="0.1.0"\n')
    (excluded / "src/lib.rs").write_text('pub fn ready() {}\n')
    targets = [json.loads(cargo(root, "metadata", "--offline", "--no-deps", "--format-version=1").stdout)["target_directory"] for root in (roots[0], excluded)]
    assert targets[0] != targets[1]
    unsafe = scratch / "unsafe.toml"
    unsafe.write_text('[build]\ntarget-dir="shared-again"\n')
    denied = subprocess.run([str(wrapper), "metadata", "--no-deps", "--config", str(unsafe)], cwd=roots[0], env=environment, capture_output=True, text=True, timeout=30)
    assert denied.returncode and 'cannot override private artifact output' in denied.stderr
    print("PASS excluded packages stay isolated and output config cannot bypass isolation")

finally:
    shutil.rmtree(scratch)
PY
