"""Keep mutable Cargo artifacts private to the physical workspace that built them."""
import hashlib
import fnmatch
import json
import os
from pathlib import Path
import shutil
import sys
import subprocess
import tempfile
import tomllib
import machine_storage as M


def cargo_executable(environment):
    paths = [p for p in environment.get("PATH", "").split(os.pathsep)
             if not (Path(p or ".").resolve().parent / "lib/cargo_identity.py").is_file()]
    cargo = shutil.which("cargo", path=os.pathsep.join(paths))
    fallback = Path(environment.get("HOME", str(Path.home()))) / ".cargo/bin/cargo"
    if not cargo and fallback.is_file():
        cargo = str(fallback)
    return cargo


def cargo_environment(environment, cwd=None):
    # Cargo already supports a relative environment target directory. Normalize
    # that tool input before the strict private-settings reader sees it.
    env = dict(environment)
    target = env.get('CARGO_TARGET_DIR')
    if target and not Path(target).is_absolute():
        env['CARGO_TARGET_DIR'] = str((Path(cwd or Path.cwd()) / target).resolve())
    return env


def enable_environment(environment):
    env = M.environment(cargo_environment(environment))
    cargo = cargo_executable(env)
    if not cargo:
        return env
    if not shutil.which("cargo", path=env.get("PATH")):
        env["PATH"] = str(Path(cargo).parent) + os.pathsep + env.get("PATH", "")
    wrapper = str(Path(__file__).resolve().parent.parent / "bin")
    env["PATH"] = wrapper + os.pathsep + env.get("PATH", "")
    if not env.get("RUSTC_WRAPPER") and not env.get("CARGO_BUILD_RUSTC_WRAPPER"):
        cache = shutil.which("sccache", path=env["PATH"])
        if cache:
            env["RUSTC_WRAPPER"] = cache
            env.setdefault("SCCACHE_DIR", str(M.cache("sccache/richos", "SCCACHE_DIR", env)))
    return env


def workspace_root(directory):
    directory = Path(directory).resolve()
    if directory.is_file():
        directory = directory.parent
    manifest = next((p / "Cargo.toml" for p in (directory, *directory.parents)
                     if (p / "Cargo.toml").is_file()), None)
    if manifest is None:
        raise ValueError(f"no Cargo.toml above {directory}")
    data = tomllib.loads(manifest.read_text())
    if "workspace" in data:
        return manifest.parent
    explicit = data.get("package", {}).get("workspace")
    if explicit:
        return (manifest.parent / explicit).resolve()
    for parent in manifest.parent.parents:
        candidate = parent / "Cargo.toml"
        if candidate.is_file():
            workspace = tomllib.loads(candidate.read_text()).get("workspace")
            if workspace is not None:
                relative = manifest.parent.relative_to(parent).as_posix()
                if any(fnmatch.fnmatchcase(relative, pattern) for pattern in workspace.get("exclude", [])):
                    return manifest.parent
                return parent
    return manifest.parent


def target_dir(directory, environment, base=None, cwd=None):
    root = workspace_root(directory)
    environment = cargo_environment(environment, cwd)
    values = M.load(environment)
    configured = base or values.get("RICHOS_CARGO_CACHE_ROOT") or values.get("CARGO_TARGET_DIR")
    if not configured:
        configured = str(M.cache("cargo-target", environment=environment))
    cache = Path(configured).expanduser()
    if not cache.is_absolute():
        cache = Path(cwd or Path.cwd()) / cache
    cache = cache.resolve()
    # An absent external volume is a refusal, never an internal-disk fallback.
    if str(cache).startswith("/Volumes/"):
        volume = Path(*cache.parts[:3])
        if not os.path.ismount(volume):
            raise ValueError(f"Cargo cache volume is not mounted: {volume}")
    key = hashlib.sha256(os.fsencode(root)).hexdigest()[:24]
    return cache, cache / "workspaces" / key


def shared_build_environment(args, env, cwd, root, cache, private):
    """Let Cargo reuse dependency units while giving local units distinct hashes.

    Cargo 1.95 hashes the workspace-wrapper path for members and the absolute
    source path for nonmembers outside the workspace. Its internal metadata
    switch also gives cdylibs hashed filenames. Without that switch a fresh
    cdylib can be another workspace's overwritten file, despite the wrapper.
    Keep the old isolation on other versions or unqualified workspace shapes.
    """
    prefix = args[:1] if args and args[0].startswith("+") else []
    command_args = args[len(prefix):]
    if not command_args or command_args[0] not in ("build", "check", "test", "run", "bench", "rustc"):
        return env
    if env.get("RUSTC_WORKSPACE_WRAPPER") or env.get("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER"):
        return env
    for directory in (Path(cwd), *Path(cwd).parents, Path(env.get("CARGO_HOME", str(Path(env.get("HOME", str(Path.home()))) / ".cargo")))):
        for config in (directory / ".cargo/config", directory / ".cargo/config.toml", directory / "config.toml"):
            if config.is_file():
                data = tomllib.loads(config.read_text())
                if data.get("include") or data.get("build", {}).get("rustc-workspace-wrapper"):
                    return env
    cargo = cargo_executable(env)
    try:
        version = subprocess.run([cargo, *prefix, "-vV"], env=env, cwd=cwd,
                                 capture_output=True, text=True, timeout=15, check=True).stdout
        if "release: 1.95.0\n" not in version:
            return env
        host = next(line.split(": ", 1)[1] for line in version.splitlines() if line.startswith("host: "))
        # Preserve resolution/configuration flags. Cargo validates features,
        # patches and path dependencies; never guess which crates are local.
        metadata_args = []
        before_separator = command_args[:command_args.index("--")] if "--" in command_args else command_args
        i = 1
        while i < len(before_separator):
            arg = before_separator[i]
            if arg in ("--manifest-path", "--features", "-F", "--config", "--target"):
                value = before_separator[i + 1]
                if arg == "--config" and (Path(cwd) / value).is_file():
                    data = tomllib.loads((Path(cwd) / value).read_text())
                    if data.get("include") or data.get("build", {}).get("rustc-workspace-wrapper"):
                        return env
                metadata_args.extend(("--filter-platform" if arg == "--target" else arg, value))
                i += 2
                continue
            if arg in ("--all-features", "--no-default-features", "--offline", "--locked", "--frozen"):
                metadata_args.append(arg)
            elif arg.startswith(("--manifest-path=", "--features=", "--config=")):
                if arg.startswith("--config=") and (Path(cwd) / arg.split("=", 1)[1]).is_file():
                    data = tomllib.loads((Path(cwd) / arg.split("=", 1)[1]).read_text())
                    if data.get("include") or data.get("build", {}).get("rustc-workspace-wrapper"):
                        return env
                metadata_args.append(arg)
            elif arg.startswith("--target="):
                metadata_args.append(arg.replace("--target=", "--filter-platform=", 1))
            i += 1
        if not any(a.startswith("--filter-platform") for a in metadata_args):
            metadata_args.extend(("--filter-platform", host))
        result = subprocess.run([cargo, *prefix, "metadata", "--format-version=1", *metadata_args],
                                env=env, cwd=cwd, capture_output=True, text=True, timeout=60, check=True)
        metadata = json.loads(result.stdout)
        members = set(metadata["workspace_members"])
        for package in metadata["packages"]:
            path = Path(package["manifest_path"]).resolve()
            if package["source"] is None and package["id"] not in members and path.is_relative_to(root):
                # An excluded path dependency inside the root gets a relative
                # Cargo hash but no workspace wrapper. It must stay isolated.
                return env
        # CLI configuration of a workspace wrapper must not be overridden.
        if any("rustc-workspace-wrapper" in arg for arg in metadata_args):
            return env
    except (OSError, ValueError, KeyError, StopIteration, IndexError, subprocess.SubprocessError):
        return env
    private.mkdir(parents=True, exist_ok=True)
    wrapper = private / "rustc-workspace-v1"
    if not wrapper.exists():
        # Identical content, distinct physical paths. Cargo hashes the path.
        fd, temporary = tempfile.mkstemp(prefix="rustc-workspace-", dir=private)
        with os.fdopen(fd, "w") as output:
            output.write('#!/bin/sh\nexec "$@"\n')
        os.chmod(temporary, 0o755)
        os.replace(temporary, wrapper)
    env["RUSTC_WORKSPACE_WRAPPER"] = str(wrapper)
    # sccache 0.18 cannot identify an arbitrary workspace wrapper as rustc.
    # Cargo itself now reuses complete dependency units (including build
    # scripts), so bypass this redundant wrapper for shared-build commands.
    if Path(env.get("RUSTC_WRAPPER") or env.get("CARGO_BUILD_RUSTC_WRAPPER") or "").name == "sccache":
        env["RUSTC_WRAPPER"] = ""
        env["CARGO_BUILD_RUSTC_WRAPPER"] = ""
    env.setdefault("__CARGO_DEFAULT_LIB_METADATA", "richos-private-workspace-v1")
    env["CARGO_BUILD_BUILD_DIR"] = str(cache / "shared-build-v1")
    return env


def command(arguments, environment, cwd):
    environment = enable_environment(environment)
    args, base, directory = [], None, Path(cwd)
    i = 0
    while i < len(arguments):
        arg = arguments[i]
        if arg == "--":
            args.extend(arguments[i:])
            break
        if arg == "--config" and i + 1 < len(arguments):
            config = Path(cwd) / arguments[i + 1]
        elif arg.startswith("--config="):
            config = Path(cwd) / arg.split("=", 1)[1]
        else:
            config = None
        if config is not None and config.is_file():
            build = tomllib.loads(config.read_text()).get("build", {})
            if "target-dir" in build or "build-dir" in build:
                raise ValueError("Cargo config cannot override private artifact output; use --target-dir for a cache root")
        if arg in ("--manifest-path", "--target-dir"):
            if i + 1 == len(arguments):
                raise ValueError(f"{arg} needs a path")
            value = arguments[i + 1]
            if arg == "--target-dir":
                base = value
            else:
                directory = Path(cwd) / value
                args.extend((arg, value))
            i += 2
            continue
        if arg.startswith("--manifest-path="):
            directory = Path(cwd) / arg.split("=", 1)[1]
        elif arg.startswith("--target-dir="):
            base = arg.split("=", 1)[1]
            i += 1
            continue
        if "build.target-dir" in arg or "build.build-dir" in arg:
            raise ValueError("use --target-dir to select a cache root; shared Cargo output config is unsafe")
        args.append(arg)
        i += 1
    # Commands that neither build nor inspect a workspace keep Cargo's normal behavior.
    cargo_args = args[:args.index("--")] if "--" in args else args
    if not args or any(a in ("--version", "-V", "--help", "-h") for a in cargo_args):
        return args, dict(environment)
    try:
        cache, private = target_dir(directory, environment, base, cwd)
    except ValueError:
        if args[0] in ("install", "uninstall", "search", "login", "logout", "new", "init") or args[:2] == ["tauri", "signer"]:
            return args, dict(environment)
        raise
    env = dict(environment, RICHOS_CARGO_CACHE_ROOT=str(cache),
               CARGO_TARGET_DIR=str(private), CARGO_BUILD_TARGET_DIR=str(private),
               CARGO_BUILD_BUILD_DIR=str(private))
    return args, shared_build_environment(args, env, cwd, workspace_root(directory), cache, private)


def main():
    if sys.argv[1:2] == ["--richos-target-dir"]:
        print(target_dir(sys.argv[2], os.environ, cwd=sys.argv[2])[1])
        return
    # PATH may contain nested wrappers. Resolve the actual Cargo, preserving fake
    # Cargo tools supplied by a fixture and the caller's chosen Rust toolchain.
    cargo = cargo_executable(os.environ)
    if not cargo:
        raise ValueError("cargo is not on PATH")
    args, env = command(sys.argv[1:], os.environ, Path.cwd())
    os.execvpe(cargo, [cargo, *args], env)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, tomllib.TOMLDecodeError) as exc:
        sys.exit(f"RichOS Cargo: {exc}")
