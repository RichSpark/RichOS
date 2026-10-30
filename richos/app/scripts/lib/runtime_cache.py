"""Where a nightly state directory keeps the runtime built from one runtime recipe.

The directory name carries the first 12 hex digits of the recipe file's SHA-256, so a changed
`runtime-sources.json` names a directory that does not exist yet and the nightly builds it,
instead of finding the previous recipe's runtime at a fixed path and refusing it at
`verify-runtime.py` (Frank's media-tools review M3, 2026-10-07). A runtime built from an older
recipe is simply no longer used; nothing deletes it.

It also names the speech model gui-boot's fixture needs (`gui_speech_model`, below).

Read by `nightly-local.py` (`Runner.runtime`, which builds it) and `proof-run.py`
(`supply_runtime`), so both name the same directory for the same recipe.
"""
import fcntl
import hashlib
import os
import shlex
import subprocess
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent       # richos/app/scripts: the recipe and its builder live here


def cache_path(state, recipe):
    """`<state>/runtime-<sha256(recipe bytes)[:12]>`. Raises if the recipe cannot be read."""
    digest = hashlib.sha256(Path(recipe).read_bytes()).hexdigest()[:12]
    return Path(state) / f"runtime-{digest}"


# THE SPEECH MODELS gui-boot.test.sh's healthy machine carries (RICHOS_GUI_SPEECH_MODELS, the
# paths joined by ":"). Since 2026-10-07 the video tools are a setup essential (media-tools plan,
# slice 3), present only when BOTH pinned speech models verify (the one voice resolves, and the
# transcription model; the CEO: "Both, in this nightly, yes."), so a fixture without them boots to
# "media-tools is NOT installed" instead of "nothing missing". These are the pinned small.en
# (487,614,201 B) and large-v3-turbo-q5_0 (574,041,195 B) already on the Mac the nightlies run on
# (engine/voice/models/model-pins.json); nothing is downloaded for them. Read by
# `nightly-local.py` (`Runner.runtime`) and `proof-run.py` (`supply_speech_models`), so the
# build and the checks before it hand gui-boot the same files.
GUI_SPEECH_MODELS = (
    Path.home() / "Models" / "Whisper" / "ggml-small.en.bin",
    Path.home() / "Models" / "Whisper" / "ggml-large-v3-turbo-q5_0.bin",
)


def gui_speech_models():
    """GUI_SPEECH_MODELS joined by ":" when every one is a file on this Mac, else None (gui-boot
    then refuses and names the input it needs)."""
    if all(model.is_file() for model in GUI_SPEECH_MODELS):
        return ":".join(str(model) for model in GUI_SPEECH_MODELS)
    return None


class RuntimeNotReady(Exception):
    """The runtime for this recipe is absent or unusable; the message says what to run."""


def builder_path(scripts=None):
    """The script that builds a runtime: the checkout's own build-runtimes.py. RICHOS_RUNTIME_BUILDER
    names another script with the same command line (a test stand-in); nothing else reads it."""
    override = os.environ.get("RICHOS_RUNTIME_BUILDER")
    return Path(override) if override else Path(scripts or SCRIPTS) / "build-runtimes.py"


def build_command(path, scripts=None):
    """The one command that builds the runtime at `path`, as text for a person to run."""
    return "python3 %s %s" % (shlex.quote(str(builder_path(scripts))), shlex.quote(str(path)))


def verifies(path, recipe, scripts=None):
    """True when verify-runtime.py accepts the runtime at `path` against `recipe`."""
    result = subprocess.run([sys.executable, str(Path(scripts or SCRIPTS) / "verify-runtime.py"), str(path), str(recipe)],
                            capture_output=True, text=True)
    return result.returncode == 0


def ensure_runtime(state, recipe, scripts=None, build=True, say=None, stdout=None):
    """The verified runtime for `recipe` in `state`, built here when it is not there. Returns
    (path, "verified" | "built"). Nobody sets RICHOS_RUNTIME_DIR by hand: a worktree has no extracted
    runtime (it is gitignored), and a proof run or a boot-proof retake used to stop at "set
    RICHOS_RUNTIME_DIR to a prepared, verified runtime" until someone ran build-runtimes.py and
    verify-runtime.py themselves (2026-10-28, twice).

    Raises RuntimeNotReady, with the command to run, when:
      - `build` is false and the runtime is absent;
      - the directory exists but does not verify. build-runtimes.py never overwrites a delivery
        and neither does this: move the directory aside and it is rebuilt;
      - the build or its verification fails.
    Two runs that want the same runtime take turns on a lock beside it, so one builds and the
    other finds it built. `stdout` is where the builder's progress goes (default: this process's)."""
    path = cache_path(state, recipe)
    say = say or (lambda line: None)
    command = build_command(path, scripts)
    if path.is_dir():
        if verifies(path, recipe, scripts):
            return path, "verified"
        raise RuntimeNotReady("the runtime at %s does not verify against %s. A delivered runtime is never "
                              "overwritten: move %s aside and it is rebuilt, or rebuild it elsewhere with: %s"
                              % (path, recipe, path, command))
    if not build:
        raise RuntimeNotReady("there is no runtime at %s for this checkout's recipe. Build it once with: %s"
                              % (path, command))
    Path(state).mkdir(parents=True, exist_ok=True)
    with open(str(path) + ".lock", "a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if path.is_dir():                                   # another run built it while this one waited
            if verifies(path, recipe, scripts):
                return path, "verified"
            raise RuntimeNotReady("the runtime at %s does not verify against %s (another run built it)" % (path, recipe))
        say("building the runtime for this recipe into %s (downloads and compiles; several minutes)" % path)
        built = subprocess.run([sys.executable, str(builder_path(scripts)), str(path)],
                               stdout=stdout if stdout is not None else None)
        if built.returncode != 0:
            raise RuntimeNotReady("building the runtime at %s failed (exit %d). Run it again to see why: %s"
                                  % (path, built.returncode, command))
        if not verifies(path, recipe, scripts):
            raise RuntimeNotReady("the runtime built at %s does not verify against %s" % (path, recipe))
    return path, "built"


def fetch_models_command():
    """The command that puts both speech models where GUI_SPEECH_MODELS names them."""
    script = SCRIPTS.parents[1] / "tools" / "richos-hud" / "fetch-dictation-models.sh"
    return "bash %s %s" % (shlex.quote(str(script)), shlex.quote(str(GUI_SPEECH_MODELS[0].parent)))


def speech_models_refusal():
    """None when both models are on this Mac, else the refusal: which are missing and the command
    that fetches them (so a run that needs them stops at once, not later inside gui-boot)."""
    missing = [str(model) for model in GUI_SPEECH_MODELS if not model.is_file()]
    if not missing:
        return None
    return ("the speech models gui-boot's healthy machine needs are not on this Mac (the video tools are a "
            "setup essential and need both): missing %s. Fetch them with: %s"
            % (", ".join(missing), fetch_models_command()))


def main(argv):
    """The same two answers for a shell script, so gui-proof-in-vm.sh names the nightly's runtime
    and models without a second copy of the rule (2026-10-28: a boot-proof retake from a worker's
    worktree had to be given both by hand).

        runtime_cache.py path <state> <recipe>   print cache_path(state, recipe); 2 when the recipe is unreadable
        runtime_cache.py models                  print gui_speech_models(); 1 when a model is absent
        runtime_cache.py ensure <state> <recipe> [--no-build]
                                                 print the verified runtime's path, building it first when it is
                                                 absent (progress on stderr); 1 with the command to run when it
                                                 cannot be had
        runtime_cache.py require-models          print gui_speech_models(); 1 with the command that fetches them
    """
    if len(argv) == 3 and argv[0] == "path":
        try:
            print(cache_path(argv[1], argv[2]))
        except OSError as error:
            print("runtime_cache.py: cannot read %s: %s" % (argv[2], error.strerror or error), file=sys.stderr)
            return 2
        return 0
    if argv == ["models"]:
        models = gui_speech_models()
        if models is None:
            return 1
        print(models)
        return 0
    if len(argv) in (3, 4) and argv[0] == "ensure" and (len(argv) == 3 or argv[3] == "--no-build"):
        try:
            path, _how = ensure_runtime(argv[1], argv[2], build=len(argv) == 3,
                                        say=lambda line: print("runtime_cache.py: " + line, file=sys.stderr, flush=True),
                                        stdout=sys.stderr)
        except RuntimeNotReady as error:
            print("runtime_cache.py: " + str(error), file=sys.stderr)
            return 1
        except OSError as error:
            print("runtime_cache.py: cannot read %s: %s" % (argv[2], error.strerror or error), file=sys.stderr)
            return 2
        print(path)
        return 0
    if argv == ["require-models"]:
        refusal = speech_models_refusal()
        if refusal:
            print("runtime_cache.py: " + refusal, file=sys.stderr)
            return 1
        print(gui_speech_models())
        return 0
    print("usage: runtime_cache.py path <state> <recipe> | models | ensure <state> <recipe> [--no-build] | require-models",
          file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
