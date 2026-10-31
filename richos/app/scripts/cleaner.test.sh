#!/usr/bin/env bash
# Filing/recovery uses real filesystem identities in a disposable directory. Metadata
# unit checks exercise the shipping writers; full-media qualification is separate.
# run-tests: no-host-screen: a native file helper and Rust tests; no window, key tap or microphone
# run-tests: inputs richos/app/scripts/cleaner.test.sh richos/app/native/cleaner.swift richos/app/src-tauri/src/cleaner_native.rs richos/app/src-tauri/src/cleaner_metadata.rs richos/app/src-tauri/examples/cleaner_probe.rs richos/app/src-tauri/Cargo.toml richos/app/src-tauri/Cargo.lock richos/app/crates/richos-core/src/cleaner/
# run-tests: covers richos/app/native/cleaner.swift richos/app/src-tauri/src/cleaner_native.rs richos/app/src-tauri/src/cleaner_metadata.rs richos/app/src-tauri/examples/cleaner_probe.rs
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$here/lib/cargo-cache-env.sh"
[ "$(uname)" = Darwin ] || { echo 'RichCleaner native qualification requires macOS' >&2; exit 1; }
scratch="$(mktemp -d "${TMPDIR:?}richcleaner-test.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/resources" "$scratch/fixtures"
xcrun swiftc -O -module-cache-path "$scratch/modules" "$here/../native/cleaner.swift" -o "$scratch/resources/native"
printf 'ordinary original, kept as one file\n' > "$scratch/fixtures/context.txt"
# Foundation copies can change quarantine provenance and date precision even when
# bytes survive. Compare the actual native result with all original attributes.
python3 - "$scratch/resources/native" "$scratch/attributes" <<'PYATTR'
import json,os,pathlib,plistlib,subprocess,sys
helper=pathlib.Path(sys.argv[1]);root=pathlib.Path(sys.argv[2]);root.mkdir()
def native(action,**args):
    run=subprocess.run([helper],input=json.dumps({"action":action,**args}),capture_output=True,text=True,timeout=30)
    answer=json.loads(run.stdout)
    if run.returncode:raise RuntimeError(answer)
    return answer["ok"]
def attributes(path):
    names=subprocess.check_output(["/usr/bin/xattr",path],text=True).splitlines()
    attrs={name:subprocess.check_output(["/usr/bin/xattr","-px",name,path],text=True).replace(" ","").replace("\n","") for name in names}
    acl=[line for line in subprocess.check_output(["ls","-le",path],text=True).splitlines() if line.lstrip().startswith("0:")]
    stat=path.stat()
    return (attrs,acl,stat.st_mode,stat.st_flags,stat.st_mtime_ns,stat.st_birthtime)
source=root/"original.txt";source.write_bytes(b"one original")
for name,value in {"com.apple.metadata:kMDItemFinderComment":plistlib.dumps("Existing Finder context",fmt=plistlib.FMT_BINARY),"com.apple.metadata:_kMDItemUserTags":plistlib.dumps(["Fixture\n6"],fmt=plistlib.FMT_BINARY),"com.apple.quarantine":b"0081;69000000;RichCleaner fixture;00000000-0000-0000-0000-000000000001"}.items():
    subprocess.run(["/usr/bin/xattr","-wx",name,value.hex(),source],check=True)
subprocess.run(["chmod","+a","everyone allow readattr,readextattr,readsecurity",source],check=True)
subprocess.run(["chflags","nodump",source],check=True)
before=attributes(source);identity=native("inspect",path=str(source));stage=native("reserve",path=str(source),identity=identity)
native("materialize",path=str(source),identity=identity,staged=stage["path"])
pathlib.Path(stage["path"]).write_bytes(b"one original with a staged annotation")
staged=native("attributes",path=str(source),identity=identity,staged=stage["path"])
replaced=native("replace",path=str(source),identity=identity,staged=stage["path"],stagedIdentity=staged)
target=root/"filed.txt";native("rename",path=str(source),destination=str(target),identity=replaced)
assert before==attributes(target),"Native filing changed original filesystem attributes"
assert not source.exists() and target.read_bytes()==b"one original with a staged annotation"
native("cleanup",path=stage["path"],identity=staged)
native("cleanup",path=stage["path"],identity=staged)
print("Native attributes, single-original move and idempotent staging cleanup passed")
PYATTR
cargo test --manifest-path "$here/../src-tauri/Cargo.toml" --example cleaner_probe
cargo run --quiet --manifest-path "$here/../src-tauri/Cargo.toml" --example cleaner_probe -- "$scratch/resources" "$scratch/fixtures" "$scratch/report"
