# Pocket TTS engine from Buzz (Apache-2.0)

`../../src/pocket.rs` contains code adapted from Buzz,
<https://github.com/block/buzz>, file `crates/buzz-voice/src/pocket_april.rs`
at commit `8599efb6a3cfbd3d4bc4534f6de766b4481c7fca`, licensed under the
Apache License 2.0 (`LICENSE` in this directory, unmodified).

## How the RichOS copy differs (Apache-2.0 section 4(b))

- Reformatted and merged into one module with RichOS's download pins.
- The `sherpa-onnx` dependency is removed: the voice recording is read and
  resampled with RichOS's own `wav.rs`.
- The `rand` dependency is removed: a local SplitMix64 generator draws the
  flow-matching noise (the same Box-Muller transform as upstream).
- The streaming and first-sentence-isolating paths are removed, because RichOS
  already pipelines one sentence at a time.
- The voice is conditioned once when the engine loads, instead of being cached
  on first use.

Nothing else about the model loop, the tokenizer setup, the state handling or
the text splitting was changed.
