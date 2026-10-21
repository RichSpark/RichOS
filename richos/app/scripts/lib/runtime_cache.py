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
import hashlib
from pathlib import Path


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
