#!/usr/bin/env python3
"""Build RichCleaner's pinned, self-contained native/metadata resources in the build cache.
No installation on the build host and no system Perl dependency in the delivered app.
"""
import argparse
import hashlib
import fcntl
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent
APP = HERE.parent
PINS = {
    "perl": ("5.42.0", "https://www.cpan.org/src/5.0/perl-5.42.0.tar.gz", "e093ef184d7f9a1b9797e2465296f55510adb6dab8842b0c3ed53329663096dc"),
    "exiftool": ("13.43", "https://codeload.github.com/exiftool/exiftool/tar.gz/refs/tags/13.43", "3d5b3058555ffb58f80c2d416a80b4ea545cd06b3df6fcf52816b1a8a269d9ce"),
}

# Execution hang guard for compiler/configure steps, not a performance assertion.
def run(args, cwd=None):
    subprocess.run([str(a) for a in args], cwd=cwd, check=True, stdout=subprocess.DEVNULL,timeout=900)

def verify(root):
    manifest=json.loads((root / "manifest.json").read_text())
    required={"native","perl/bin/perl","exiftool/exiftool"}
    if not required.issubset(manifest["files"]):
        raise RuntimeError("The RichCleaner manifest omits a required runtime file")
    for name, digest in manifest["files"].items():
        if Path(name).is_absolute() or ".." in Path(name).parts:
            raise RuntimeError("The RichCleaner manifest contains an invalid path")
        path=root / name
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest()!=digest:
            raise RuntimeError(f"RichCleaner resource changed or is missing: {name}")
    result=subprocess.check_output([root / "perl/bin/perl", root / "exiftool/exiftool", "-ver"],env={k:v for k,v in os.environ.items() if not k.startswith("PERL")},text=True,timeout=30).strip()
    digest=subprocess.check_output([root / "perl/bin/perl","-MDigest::SHA=sha256_hex","-MDigest::MD5","-MEncode","-e","print sha256_hex('RichCleaner')"],env={k:v for k,v in os.environ.items() if not k.startswith("PERL")},text=True,timeout=30)
    if digest!=hashlib.sha256(b"RichCleaner").hexdigest():
        raise RuntimeError("Bundled Perl cannot load its relocated metadata modules")
    if result!=PINS["exiftool"][0]:
        raise RuntimeError("Bundled ExifTool did not report its pinned version")

def pdf_unicode(root):
    # ExifTool 13.43 labels PDF text as UCS2 and truncates supplementary Unicode
    # characters. Its own UTF16 codec already implements surrogate pairs. Use that
    # codec at the two PDF text boundaries; do not change the PDF container writer.
    for relative, before, after in [
        ("lib/Image/ExifTool/WritePDF.pl", "Encode($val,'UCS2','MM')", "Encode($val,'UTF16','MM')"),
        ("lib/Image/ExifTool/PDF.pm", "? 'UCS2' : 'PDFDoc'", "? 'UTF16' : 'PDFDoc'"),
    ]:
        path=root / relative
        text=path.read_text()
        if text.count(before)!=1:
            raise RuntimeError("The pinned PDF Unicode correction no longer matches its source")
        path.write_text(text.replace(before,after))

def build(cache):
    native=APP / "native/cleaner.swift"
    sdk=subprocess.check_output(["xcrun","--show-sdk-version"],text=True,timeout=30).strip()
    key=hashlib.sha256(native.read_bytes()+Path(__file__).read_bytes()+json.dumps(PINS,sort_keys=True).encode()+platform.machine().encode()+sdk.encode()).hexdigest()[:24]
    destination=cache / key
    if (destination / "manifest.json").is_file():
        verify(destination)
        return destination
    with tempfile.TemporaryDirectory(prefix="richcleaner-build-",dir=os.environ.get("TMPDIR")) as scratch:
        scratch=Path(scratch)
        stage=scratch / "resources"
        stage.mkdir()
        for name, (version, source, digest) in PINS.items():
            archive=cache / f"{name}-{version}.tar.gz"
            if not archive.is_file():
                partial=archive.with_suffix(".download")
                try:
                    with urllib.request.urlopen(source,timeout=60) as response, partial.open("wb") as output:
                        total=0
                        while chunk:=response.read(1024*1024):
                            total+=len(chunk)
                            if total>128*1024*1024:
                                raise RuntimeError("The pinned source archive exceeds its download budget")
                            output.write(chunk)
                    if hashlib.sha256(partial.read_bytes()).hexdigest()!=digest:
                        raise RuntimeError(f"The {name} source did not match its pinned hash")
                    partial.replace(archive)
                finally:
                    partial.unlink(missing_ok=True)
            if hashlib.sha256(archive.read_bytes()).hexdigest()!=digest:
                raise RuntimeError(f"The cached {name} source hash is invalid")
            with tarfile.open(archive) as bundle:
                bundle.extractall(scratch,filter="data")
            if name=="perl":
                source_dir=scratch / f"perl-{version}"
                run(["/bin/sh","Configure","-des","-Dprefix=/richcleaner-runtime","-Duserelocatableinc","-Dman1dir=none","-Dman3dir=none","-Dcc=clang","-Dccflags=-Os -mmacosx-version-min=13.0","-Dldflags=-mmacosx-version-min=13.0"],source_dir)
                # Two compiler jobs stay inside the host's ordinary worker budget. The
                # package build already holds its admission; this helper takes no new slot.
                run(["make","-j2"],source_dir)
                install=scratch / "install"
                run(["make","install",f"DESTDIR={install}"],source_dir)
                shutil.move(install / "richcleaner-runtime",stage / "perl")
                for path in (stage / "perl/bin").iterdir():
                    if path.name not in {"perl",f"perl{version}"}:
                        path.unlink()
                for path in (stage / "perl").rglob("*.pod"):
                    path.unlink()
                shutil.copyfile(source_dir / "Copying",stage / "perl/COPYING")
                shutil.copyfile(source_dir / "Artistic",stage / "perl/Artistic")
                developer=subprocess.check_output(["xcode-select","-p"],text=True,timeout=30).strip()
                for path in (stage / "perl").rglob("Config*.p*"):
                    if path.is_file():
                        text=path.read_text()
                        if developer in text:
                            mode=path.stat().st_mode
                            path.chmod(mode | 0o200)
                            path.write_text(text.replace(developer,"/usr/share/richcleaner-build-sdk"))
                            path.chmod(mode)
            else:
                source_dir=scratch / f"exiftool-{version}"
                (stage / "exiftool").mkdir()
                shutil.copyfile(source_dir / "exiftool",stage / "exiftool/exiftool")
                shutil.copytree(source_dir / "lib",stage / "exiftool/lib")
                shutil.copyfile(source_dir / "README",stage / "exiftool/README")
                pdf_unicode(stage / "exiftool")
        modules=cache / "swift-modules"
        run(["xcrun","swiftc","-O","-target",f"{platform.machine()}-apple-macosx13.0","-module-cache-path",modules,native,"-o",stage / "native"])
        files={str(p.relative_to(stage)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(stage.rglob("*")) if p.is_file()}
        (stage / "manifest.json").write_text(json.dumps({"sources":PINS,"files":files},sort_keys=True,indent=2)+"\n")
        verify(stage)
        destination.parent.mkdir(parents=True,exist_ok=True)
        if destination.exists():
            raise RuntimeError("An incomplete RichCleaner cache exists; preserve it for diagnosis")
        shutil.move(stage,destination)
    return destination

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify",type=Path)
    args=parser.parse_args()
    if args.verify:
        verify(args.verify)
        return
    cache=Path(subprocess.check_output(["python3",HERE / "../../engine/scripts/lib/machine_settings.py","cache","richcleaner/resources","--key","RICHOS_CLEANER_BUILD_CACHE"],text=True,timeout=30).strip())
    cache.mkdir(parents=True,exist_ok=True)
    with (cache / "build.lock").open("a") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        print(build(cache))

if __name__=="__main__":
    main()
