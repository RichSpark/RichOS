//! **Rich's voice: Kyutai Pocket TTS speaking as Peter Yearsley.**
//!
//! CEO, 2026-10-14: *"Pocket TTS: Peter Yearsley"*, after *"why the fuck is a shitty robot voice
//! being used when much better options are freely available???"*. This replaces macOS `say` behind
//! the same [`SpeechSynth`] trait, so turn-taking, barge-in, playout and sentence pipelining do not
//! change. `say` stays only as the fallback while these files are not on the machine yet
//! ([`crate::tts::RichVoice`]).
//!
//! # Where every piece comes from, and why each may ship in a paid app
//!
//! | Piece | Source (pinned) | License |
//! |---|---|---|
//! | Model (100M params, Mimi codec) | Kyutai Pocket TTS | weights CC-BY-4.0, code MIT |
//! | ONNX export, `english_2026-04` INT8 | `KevinAHM/pocket-tts-onnx` @ `58a6d00c` | CC-BY-4.0 |
//! | Voice: `peter_yearsley.wav` | `kyutai/tts-voices` @ `323332d3`, `voice-zero/` | CC0 (LibriVox via Voice-Zero) |
//! | This engine (ONNX loop, tokenizer, state) | Buzz `crates/buzz-voice/src/pocket_april.rs` (block/buzz) | Apache-2.0 |
//! | ONNX Runtime 1.24.2, linked statically by `ort` | Microsoft, binaries pinned by sha256 in `ort-sys` | MIT |
//!
//! Peter Yearsley is one of Kyutai's own Pocket TTS presets (`kyutai/pocket-tts`
//! `embeddings_v3/peter_yearsley.safetensors`); the WAV above is the recording that preset is made
//! from. Kyutai's terms forbid cloning a voice without consent; a CC0 public-domain reading is
//! consented by its license. Attribution lives in `docs/legal/THIRD-PARTY-NOTICES.md`.
//!
//! # Delivered like the speech-recognition model
//!
//! Nine files, 165,756,868 bytes, downloaded on first need into
//! `<install_dir>/pocket-tts-english_2026-04/` and each checked against the size and sha256 in
//! [`FILES`] before it gets its real name. The transport is the shell's
//! (`src-tauri/src/voice_provision.rs`); every judgment is here.
//!
//! # Measured on this Mac
//!
//! See [`PocketSynth`] for the real-time factor and time-to-first-audio this was built against.

use crate::tts::{Speech, SpeechSynth, TtsError};
use crate::wav;
use ort::session::{Session, SessionInputValue};
use ort::value::{DynValue, Tensor};
use sentencepiece_model::SentencePieceModel;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::f32::consts::TAU;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;
use tokenizers::models::unigram::Unigram;
use tokenizers::pre_tokenizers::metaspace::{Metaspace, PrependScheme};
use tokenizers::Tokenizer;

/// The directory under [`crate::provision::install_dir`] that holds the bundle and the voice.
pub const BUNDLE_DIR: &str = "pocket-tts-english_2026-04";

/// The reference recording Rich speaks as.
pub const VOICE_FILE: &str = "peter_yearsley.wav";

/// One pinned file: where it comes from and the exact bytes RichOS means by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedFile {
    pub name: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

#[cfg(test)]
const MODEL_BASE: &str =
    "https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04";

/// Every file the voice needs. Sizes and hashes are Buzz's pins for the same revision
/// (`pocket_models.rs`), re-measured with `shasum -a 256` over a fresh download on 2026-10-14 and
/// identical; the voice's hash is also HuggingFace's own LFS oid for that file.
pub const FILES: [PinnedFile; 9] = [
    PinnedFile { name: "bundle.json", url: concat!("https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04", "/bundle.json"), bytes: 24_381, sha256: "bab643150f437f37df080a710520ff39ed9ebd9a339f8ebdc739f7eddfc28b3f" },
    PinnedFile { name: "bos_before_voice.npy", url: concat!("https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04", "/bos_before_voice.npy"), bytes: 4_224, sha256: "f46edf4f7007b7ba4ea58831f49d003e59e167b4641c44bb3addfe9231a780b1" },
    PinnedFile { name: "tokenizer.model", url: concat!("https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04", "/tokenizer.model"), bytes: 59_339, sha256: "d461765ae179566678c93091c5fa6f2984c31bbe990bf1aa62d92c64d91bc3f6" },
    PinnedFile { name: "flow_lm_main_int8.onnx", url: concat!("https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04", "/flow_lm_main_int8.onnx"), bytes: 76_341_079, sha256: "f9bd8106b79a0192c1c43399ab938fb24900a95c1c599870d75a884e99000116" },
    PinnedFile { name: "flow_lm_flow_int8.onnx", url: concat!("https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04", "/flow_lm_flow_int8.onnx"), bytes: 9_962_530, sha256: "3dd781ee5abee9e195320bf0106bebd6372a852b3b36352524ee78b40554635d" },
    PinnedFile { name: "mimi_decoder_int8.onnx", url: concat!("https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04", "/mimi_decoder_int8.onnx"), bytes: 22_684_077, sha256: "3630450a3297a101792a6ac66619ebc70ab916b265e6220c2afaef8b1673f925" },
    PinnedFile { name: "mimi_encoder.onnx", url: concat!("https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04", "/mimi_encoder.onnx"), bytes: 39_768_446, sha256: "853e2ca623b8782d94c3745ec6133bfdff7ce33d9b11128bd29ea03f28d76e3d" },
    PinnedFile { name: "text_conditioner.onnx", url: concat!("https://huggingface.co/KevinAHM/pocket-tts-onnx/resolve/58a6d00cf13d239b6748cb0769f35c580a8f606c/onnx/english_2026-04", "/text_conditioner.onnx"), bytes: 16_388_344, sha256: "4ecee995fb69f85c7a7493d11f7b5ee15d9950facc7ab3f5c9c49ef1e03847bb" },
    PinnedFile { name: VOICE_FILE, url: "https://huggingface.co/kyutai/tts-voices/resolve/323332d33f997de8394f24a193e1a76df720e01a/voice-zero/peter_yearsley.wav", bytes: 524_448, sha256: "fbb3920fda7ae26a5a8b317ffcae1d55c0bd5d89d075205f5a52b1e924b83f51" },
];

/// Total download, for logs and the record.
pub fn total_bytes() -> u64 {
    FILES.iter().map(|f| f.bytes).sum()
}

/// Where the bundle lives on this machine (`RICHOS_MODEL_DIR` honored through `install_dir`).
pub fn bundle_dir() -> Option<PathBuf> {
    crate::provision::install_dir().map(|d| d.join(BUNDLE_DIR))
}

/// sha256 of a whole file, streamed.
fn sha256_file(path: &Path) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// Is the file at `path` exactly the pinned bytes? `Err` says what is wrong, for stderr.
pub fn verify_file(path: &Path, pin: &PinnedFile) -> Result<(), String> {
    let len = fs::metadata(path).map(|m| m.len()).map_err(|_| format!("{} is not on disk", pin.name))?;
    if len != pin.bytes {
        return Err(format!("{} is {len} bytes where {} are pinned", pin.name, pin.bytes));
    }
    let sha = sha256_file(path).ok_or_else(|| format!("{} could not be read", pin.name))?;
    if sha != pin.sha256 {
        return Err(format!("{} hashes to {sha}, not the pinned {}", pin.name, pin.sha256));
    }
    Ok(())
}

/// The pinned files not yet in `dir` with the right size. Cheap (one stat each): the full hash is
/// taken when a file is installed and again by [`PocketSynth::load`] before anything is read.
pub fn missing_files(dir: &Path) -> Vec<PinnedFile> {
    FILES
        .iter()
        .filter(|f| fs::metadata(dir.join(f.name)).map(|m| m.len() != f.bytes).unwrap_or(true))
        .copied()
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The engine. Lifted from Buzz `pocket_april.rs` (Apache-2.0); changes: no sherpa-onnx (the voice
// WAV is read and resampled with this crate's `wav`), no `rand` (a local PRNG draws the flow
// noise), and PCM delivery while the rest of the sentence is still being generated.
// ---------------------------------------------------------------------------------------------

const FILE_BUNDLE: &str = "bundle.json";
const FILE_MIMI_ENCODER: &str = "mimi_encoder.onnx";
const FILE_TEXT_CONDITIONER: &str = "text_conditioner.onnx";
const FILE_FLOW_MAIN_INT8: &str = "flow_lm_main_int8.onnx";
const FILE_FLOW_INT8: &str = "flow_lm_flow_int8.onnx";
const FILE_MIMI_DECODER_INT8: &str = "mimi_decoder_int8.onnx";

const MODEL_LANGUAGE: &str = "english_2026-04";
const DEFAULT_TEMPERATURE: f32 = 0.7;
const EOS_LOGIT_THRESHOLD: f32 = -4.0;
const DECODER_CHUNK_FRAMES: usize = 12;
const TOKENS_PER_SECOND_ESTIMATE: f32 = 3.0;
const GENERATION_SECONDS_PADDING: f32 = 2.0;

/// Pocket TTS emits 24 kHz mono.
pub const SAMPLE_RATE: u32 = 24_000;

#[derive(Debug, Deserialize)]
struct Bundle {
    schema_version: u32,
    language: String,
    sample_rate: usize,
    frame_rate: f32,
    samples_per_frame: usize,
    latent_dim: usize,
    conditioning_dim: usize,
    insert_bos_before_voice: bool,
    pad_with_spaces_for_short_inputs: bool,
    remove_semicolons: bool,
    model_recommended_frames_after_eos: Option<usize>,
    max_token_per_chunk: usize,
    tokenizer_file: String,
    bos_before_voice_file: String,
    flow_lm_state_manifest: Vec<StateSpec>,
    mimi_state_manifest: Vec<StateSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct StateSpec {
    input_name: String,
    output_name: String,
    dtype: StateDtype,
    shape: Vec<i64>,
    fill: StateFill,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StateDtype {
    #[serde(rename = "float32")]
    Float32,
    #[serde(rename = "int64")]
    Int64,
    Bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StateFill {
    Empty,
    Nan,
    Ones,
    Zeros,
}

struct StateValue {
    spec: StateSpec,
    value: DynValue,
}

enum SnapshotTensor {
    F32(Vec<i64>, Vec<f32>),
    I64(Vec<i64>, Vec<i64>),
    Bool(Vec<i64>, Vec<bool>),
}

fn ort_error(context: &'static str) -> impl FnOnce(ort::Error) -> String {
    move |err| format!("{context}: {err}")
}

fn empty_or_filled<T>(shape: &[i64], data: Vec<T>) -> Result<DynValue, String>
where
    T: ort::value::PrimitiveTensorElementType + std::fmt::Debug + Clone + 'static,
{
    if data.is_empty() {
        Ok(Tensor::<T>::new(&ort::memory::Allocator::default(), shape.to_vec())
            .map_err(ort_error("create empty state tensor"))?
            .into_dyn())
    } else {
        Ok(Tensor::from_array((shape.to_vec(), data.into_boxed_slice()))
            .map_err(ort_error("create state tensor"))?
            .into_dyn())
    }
}

fn snapshot_state(state: &[StateValue]) -> Result<Vec<(StateSpec, SnapshotTensor)>, String> {
    state
        .iter()
        .map(|v| {
            let t = match v.spec.dtype {
                StateDtype::Float32 => {
                    let (s, d) = v.value.try_extract_tensor::<f32>().map_err(ort_error("snapshot f32 state"))?;
                    SnapshotTensor::F32(s.to_vec(), d.to_vec())
                }
                StateDtype::Int64 => {
                    let (s, d) = v.value.try_extract_tensor::<i64>().map_err(ort_error("snapshot i64 state"))?;
                    SnapshotTensor::I64(s.to_vec(), d.to_vec())
                }
                StateDtype::Bool => {
                    let (s, d) = v.value.try_extract_tensor::<bool>().map_err(ort_error("snapshot bool state"))?;
                    SnapshotTensor::Bool(s.to_vec(), d.to_vec())
                }
            };
            Ok((v.spec.clone(), t))
        })
        .collect()
}

fn restore_state(snapshot: &[(StateSpec, SnapshotTensor)]) -> Result<Vec<StateValue>, String> {
    snapshot
        .iter()
        .map(|(spec, t)| {
            let value = match t {
                SnapshotTensor::F32(s, d) => empty_or_filled(s, d.clone())?,
                SnapshotTensor::I64(s, d) => empty_or_filled(s, d.clone())?,
                SnapshotTensor::Bool(s, d) => empty_or_filled(s, d.clone())?,
            };
            Ok(StateValue { spec: spec.clone(), value })
        })
        .collect()
}

fn initialize_state(specs: &[StateSpec]) -> Result<Vec<StateValue>, String> {
    specs
        .iter()
        .cloned()
        .map(|spec| {
            let len = shape_len(&spec.shape)?;
            let value = match spec.dtype {
                StateDtype::Float32 => {
                    let fill = match spec.fill {
                        StateFill::Nan => f32::NAN,
                        StateFill::Empty | StateFill::Zeros => 0.0,
                        StateFill::Ones => 1.0,
                    };
                    empty_or_filled(&spec.shape, vec![fill; len])?
                }
                StateDtype::Int64 => empty_or_filled(&spec.shape, vec![i64::from(matches!(spec.fill, StateFill::Ones)); len])?,
                StateDtype::Bool => empty_or_filled(&spec.shape, vec![matches!(spec.fill, StateFill::Ones); len])?,
            };
            Ok(StateValue { spec, value })
        })
        .collect()
}

fn append_state_inputs<'a>(inputs: &mut Vec<(Cow<'a, str>, SessionInputValue<'a>)>, state: &'a [StateValue]) {
    for v in state {
        inputs.push((Cow::Borrowed(v.spec.input_name.as_str()), SessionInputValue::from(&v.value)));
    }
}

fn replace_state_from_outputs(state: &mut [StateValue], outputs: &mut ort::session::SessionOutputs<'_>) -> Result<(), String> {
    for v in state {
        v.value = outputs.remove(&v.spec.output_name).ok_or_else(|| format!("missing state output {}", v.spec.output_name))?;
    }
    Ok(())
}

fn shape_len(shape: &[i64]) -> Result<usize, String> {
    shape.iter().try_fold(1_usize, |len, &dim| {
        let dim = usize::try_from(dim).map_err(|_| format!("negative state dimension {dim}"))?;
        len.checked_mul(dim).ok_or_else(|| format!("state shape overflows usize: {shape:?}"))
    })
}

fn estimate_max_frames(token_count: usize, frame_rate: f32) -> usize {
    ((token_count as f32 / TOKENS_PER_SECOND_ESTIMATE + GENERATION_SECONDS_PADDING) * frame_rate).ceil() as usize
}

/// SplitMix64: the flow's Gaussian noise needs uniform draws, not cryptography, and a ten-line
/// generator keeps `rand` out of the dependency tree.
struct Noise(u64);

impl Noise {
    fn seeded() -> Noise {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x9E37_79B9);
        Noise(t ^ (std::process::id() as u64).rotate_left(32))
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// Box-Muller, as Buzz's `normal_noise`.
    fn normal(&mut self, len: usize, std_dev: f32) -> Vec<f32> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let u1 = self.unit().max(f32::MIN_POSITIVE);
            let u2 = self.unit();
            let r = (-2.0_f32 * u1.ln()).sqrt() * std_dev;
            out.push(r * (TAU * u2).cos());
            if out.len() < len {
                out.push(r * (TAU * u2).sin());
            }
        }
        out
    }
}

fn read_npy_f32(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if bytes.len() < 10 || &bytes[..6] != b"\x93NUMPY" {
        return Err(format!("{} is not a NumPy array", path.display()));
    }
    let header_len_bytes = match bytes[6] {
        1 => 2,
        2 | 3 => 4,
        v => return Err(format!("unsupported NumPy version {v} in {}", path.display())),
    };
    let header_start = 8 + header_len_bytes;
    if bytes.len() < header_start {
        return Err(format!("truncated NumPy header in {}", path.display()));
    }
    let header_len = if header_len_bytes == 2 {
        u16::from_le_bytes([bytes[8], bytes[9]]) as usize
    } else {
        u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize
    };
    let data_start = header_start + header_len;
    if data_start > bytes.len() {
        return Err(format!("truncated NumPy data in {}", path.display()));
    }
    let header = std::str::from_utf8(&bytes[header_start..data_start]).map_err(|e| format!("invalid NumPy header: {e}"))?;
    if !(header.contains("'descr': '<f4'") || header.contains("\"descr\": \"<f4\"")) || header.contains("fortran_order': True") {
        return Err(format!("{} must be a little-endian, C-order float32 array", path.display()));
    }
    let data = &bytes[data_start..];
    if !data.len().is_multiple_of(4) {
        return Err(format!("misaligned float32 data in {}", path.display()));
    }
    Ok(data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

fn load_session(path: PathBuf, threads: usize) -> Result<Session, String> {
    Session::builder()
        .map_err(ort_error("create ONNX session builder"))?
        .with_intra_threads(threads)
        .map_err(|e| format!("configure ONNX intra-op threads: {e}"))?
        .with_inter_threads(1)
        .map_err(|e| format!("configure ONNX inter-op threads: {e}"))?
        .commit_from_file(&path)
        .map_err(|e| format!("load {}: {e}", path.display()))
}

fn load_tokenizer(path: &Path) -> Result<Tokenizer, String> {
    let sp = SentencePieceModel::from_file(path).map_err(|e| format!("load {}: {e}", path.display()))?;
    let trainer = sp.trainer().ok_or_else(|| format!("{} has no SentencePiece trainer metadata", path.display()))?;
    let normalizer = sp.normalizer().ok_or_else(|| format!("{} has no SentencePiece normalizer metadata", path.display()))?;
    if normalizer.name() != "identity" {
        return Err(format!("{} uses unsupported SentencePiece normalizer {:?}", path.display(), normalizer.name()));
    }
    let vocab = sp.pieces().iter().map(|p| (p.piece().to_owned(), f64::from(p.score()))).collect();
    let mut tokenizer = Tokenizer::new(
        Unigram::from(vocab, Some(trainer.unk_id() as usize), trainer.byte_fallback())
            .map_err(|e| format!("construct tokenizer from {}: {e}", path.display()))?,
    );
    // SentencePiece's identity normalizer still escapes spaces as U+2581 and prepends one marker.
    tokenizer.with_pre_tokenizer(Some(Metaspace::new('▁', PrependScheme::Always, false)));
    Ok(tokenizer)
}

/// Buzz's prompt preparation: collapse whitespace, capitalize the first letter, end on
/// punctuation, and keep a few frames after end-of-speech so the last word is not clipped.
fn prepare_prompt(input: &str) -> Option<(String, usize)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut cleaned = String::with_capacity(trimmed.len());
    let mut last_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !last_space {
                cleaned.push(' ');
            }
            last_space = true;
        } else {
            cleaned.push(ch);
            last_space = false;
        }
    }
    let first = cleaned.chars().next()?;
    if first.is_lowercase() {
        let upper: String = first.to_uppercase().collect();
        cleaned = upper + &cleaned[first.len_utf8()..];
    }
    if cleaned.chars().next_back().is_some_and(char::is_alphanumeric) {
        cleaned.push('.');
    }
    let frames_after_eos = if cleaned.split_whitespace().count() <= 4 { 5 } else { 3 };
    Some((cleaned, frames_after_eos))
}

/// Split a prepared prompt into pieces of at most `max_tokens`, preferring sentence, then clause,
/// then word boundaries. Buzz's `split_at_natural_boundaries` with packing (no first-sentence
/// isolation: RichOS hands this one sentence at a time already).
fn split_at_natural_boundaries<F>(text: &str, max_tokens: usize, mut token_count: F) -> Result<Vec<String>, String>
where
    F: FnMut(&str) -> Result<usize, String>,
{
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        while text[start..].chars().next().is_some_and(char::is_whitespace) {
            start += text[start..].chars().next().map(char::len_utf8).unwrap_or(1);
        }
        if start == text.len() {
            break;
        }
        let (mut sentence_end, mut clause_end, mut word_end) = (None, None, None);
        for (offset, ch) in text[start..].char_indices() {
            let end = start + offset + ch.len_utf8();
            let at_word_end = end == text.len() || text[end..].chars().next().is_some_and(char::is_whitespace);
            let at_clause_end = matches!(ch, '—' | '–') && !text[end..].chars().next().is_some_and(is_closing_punctuation);
            if !at_word_end && !at_clause_end {
                continue;
            }
            // Token counts are monotonic in prefix length: once one overflows, none longer fits.
            if token_count(&text[start..end])? > max_tokens {
                break;
            }
            word_end = Some(end);
            match natural_boundary(&text[start..end], end == text.len()) {
                Boundary::Sentence => sentence_end = Some(end),
                Boundary::Clause => clause_end = Some(end),
                Boundary::Word => {}
            }
        }
        let end = match sentence_end.or(clause_end).or(word_end) {
            Some(end) => end,
            None => {
                // One word over the limit: cut at a character boundary rather than lose it.
                let mut scalar_end = None;
                for (offset, ch) in text[start..].char_indices() {
                    if ch.is_whitespace() {
                        break;
                    }
                    let end = start + offset + ch.len_utf8();
                    if token_count(&text[start..end])? <= max_tokens {
                        scalar_end = Some(end);
                    }
                }
                scalar_end.ok_or_else(|| format!("cannot fit one character within {max_tokens} tokens"))?
            }
        };
        let mut next = end;
        while text[next..].chars().next().is_some_and(char::is_whitespace) {
            next += text[next..].chars().next().map(char::len_utf8).unwrap_or(1);
        }
        chunks.push(text[start..next].to_string());
        start = next;
    }
    Ok(chunks)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    Sentence,
    Clause,
    Word,
}

fn natural_boundary(candidate: &str, is_end: bool) -> Boundary {
    if is_end {
        return Boundary::Sentence;
    }
    let mut chars = candidate.chars().rev();
    let mut last = chars.next();
    while last.is_some_and(is_closing_punctuation) {
        last = chars.next();
    }
    match last {
        Some('.' | '!' | '?') if !looks_like_abbreviation(candidate) => Boundary::Sentence,
        Some(',' | ';' | ':' | '—' | '–') => Boundary::Clause,
        _ => Boundary::Word,
    }
}

fn is_closing_punctuation(ch: char) -> bool {
    matches!(ch, '"' | '\'' | '”' | '’' | ')' | ']' | '}')
}

fn looks_like_abbreviation(candidate: &str) -> bool {
    const ABBREVIATIONS: &[&str] = &[
        "Dr.", "Mr.", "Mrs.", "Ms.", "Prof.", "Sr.", "Jr.", "St.", "Ave.", "Rd.", "Blvd.", "Dept.", "Inc.", "Ltd.", "Co.",
        "Corp.", "etc.", "vs.", "i.e.", "e.g.", "Ph.D.",
    ];
    let candidate = candidate.trim_end_matches(is_closing_punctuation);
    let last = candidate.rsplit_once(char::is_whitespace).map_or(candidate, |(_, w)| w);
    ABBREVIATIONS.contains(&last) || (last.ends_with('.') && last[..last.len() - 1].chars().all(|c| c.is_ascii_digit()))
}

/// The loaded graphs and the voice, conditioned once.
struct Engine {
    bundle: Bundle,
    tokenizer: Tokenizer,
    mimi_encoder: Session,
    text_conditioner: Session,
    flow_main: Session,
    flow: Session,
    mimi_decoder: Session,
    /// The Flow LM state right after conditioning on Peter Yearsley's voice. Restored per chunk so
    /// the conditioning pass runs once per process, not once per sentence.
    voice_state: Vec<(StateSpec, SnapshotTensor)>,
    noise: Noise,
}

impl Engine {
    fn load(dir: &Path, threads: usize) -> Result<Engine, String> {
        let bundle_path = dir.join(FILE_BUNDLE);
        let bundle: Bundle = serde_json::from_slice(&fs::read(&bundle_path).map_err(|e| format!("read {}: {e}", bundle_path.display()))?)
            .map_err(|e| format!("parse {}: {e}", bundle_path.display()))?;
        if bundle.schema_version != 2 || bundle.language != MODEL_LANGUAGE {
            return Err(format!("unexpected bundle: schema {} language {}", bundle.schema_version, bundle.language));
        }
        if bundle.sample_rate != SAMPLE_RATE as usize
            || bundle.frame_rate != 12.5
            || bundle.samples_per_frame != 1_920
            || bundle.latent_dim != 32
            || bundle.conditioning_dim != 1024
            || !bundle.insert_bos_before_voice
            || bundle.pad_with_spaces_for_short_inputs
            || bundle.remove_semicolons
            || bundle.model_recommended_frames_after_eos.is_some()
            || bundle.max_token_per_chunk != 50
        {
            return Err("unexpected Pocket TTS bundle dimensions or prompt policy".into());
        }
        let tokenizer = load_tokenizer(&dir.join(&bundle.tokenizer_file))?;
        let bos = read_npy_f32(&dir.join(&bundle.bos_before_voice_file))?;
        if bos.len() != bundle.conditioning_dim {
            return Err(format!("BOS embedding has {} values; expected {}", bos.len(), bundle.conditioning_dim));
        }
        let mut engine = Engine {
            mimi_encoder: load_session(dir.join(FILE_MIMI_ENCODER), threads)?,
            text_conditioner: load_session(dir.join(FILE_TEXT_CONDITIONER), threads)?,
            flow_main: load_session(dir.join(FILE_FLOW_MAIN_INT8), threads)?,
            flow: load_session(dir.join(FILE_FLOW_INT8), threads)?,
            mimi_decoder: load_session(dir.join(FILE_MIMI_DECODER_INT8), threads)?,
            bundle,
            tokenizer,
            voice_state: Vec::new(),
            noise: Noise::seeded(),
        };
        let voice = read_voice(&dir.join(VOICE_FILE))?;
        let mut embeddings = bos;
        embeddings.extend(engine.voice_embeddings(voice)?);
        let state = engine.condition_voice(&embeddings)?;
        engine.voice_state = snapshot_state(&state)?;
        Ok(engine)
    }

    fn token_ids(&self, text: &str) -> Result<Vec<i64>, String> {
        Ok(self
            .tokenizer
            .encode(text, false)
            .map_err(|e| format!("tokenize Pocket TTS prompt: {e}"))?
            .get_ids()
            .iter()
            .map(|&id| i64::from(id))
            .collect())
    }

    fn prepared_token_count(&self, text: &str) -> Result<usize, String> {
        let (prepared, _) = prepare_prompt(text).ok_or("Pocket TTS prompt chunk became empty")?;
        Ok(self.token_ids(&prepared)?.len())
    }

    /// One sentence (any length) to 24 kHz audio.
    fn synthesize(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let mut audio = Vec::new();
        self.synthesize_stream(text, &mut |chunk| { audio.extend_from_slice(chunk); true })?;
        Ok(audio)
    }

    /// Decode and deliver each codec block before generating the rest of the sentence.
    /// A false return from the sink cancels the remaining generation.
    fn synthesize_stream(&mut self, text: &str, emit: &mut dyn FnMut(&[f32]) -> bool) -> Result<bool, String> {
        let Some((prepared, _)) = prepare_prompt(text) else { return Ok(true) };
        let max = self.bundle.max_token_per_chunk;
        let chunks = if self.prepared_token_count(&prepared)? <= max {
            vec![prepared]
        } else {
            split_at_natural_boundaries(&prepared, max, |t| self.prepared_token_count(t))?
        };
        for chunk in chunks {
            let Some((chunk, frames_after_eos)) = prepare_prompt(&chunk) else { continue };
            if !self.synth_chunk_stream(&chunk, frames_after_eos, emit)? { return Ok(false); }
        }
        Ok(true)
    }

    fn synth_chunk_stream(&mut self, prepared: &str, frames_after_eos: usize, emit: &mut dyn FnMut(&[f32]) -> bool) -> Result<bool, String> {
        let token_ids = self.token_ids(prepared)?;
        if token_ids.is_empty() {
            return Ok(true);
        }
        if token_ids.len() > self.bundle.max_token_per_chunk {
            return Err(format!("prompt has {} tokens; the limit is {}", token_ids.len(), self.bundle.max_token_per_chunk));
        }
        let mut state = restore_state(&self.voice_state)?;
        let token_count = token_ids.len();
        let text = self.text_embeddings(token_ids)?;
        self.run_flow_main_prefix(&text, &mut state)?;
        let max_frames = estimate_max_frames(token_count, self.bundle.frame_rate);
        self.generate_audio(max_frames, frames_after_eos, &mut state, emit)
    }

    fn voice_embeddings(&mut self, samples: Vec<f32>) -> Result<Vec<f32>, String> {
        let audio = Tensor::from_array((vec![1_i64, 1, samples.len() as i64], samples.into_boxed_slice()))
            .map_err(ort_error("create voice audio tensor"))?;
        let outputs = self.mimi_encoder.run(ort::inputs!["audio" => audio]).map_err(ort_error("run Mimi encoder"))?;
        let (_, encoded) = outputs[0].try_extract_tensor::<f32>().map_err(ort_error("extract Mimi encoder output"))?;
        if !encoded.len().is_multiple_of(self.bundle.conditioning_dim) {
            return Err(format!("Mimi encoder returned {} values, not divisible by {}", encoded.len(), self.bundle.conditioning_dim));
        }
        Ok(encoded.to_vec())
    }

    fn condition_voice(&mut self, embeddings: &[f32]) -> Result<Vec<StateValue>, String> {
        let frames = embeddings.len() / self.bundle.conditioning_dim;
        let sequence = Tensor::<f32>::new(&ort::memory::Allocator::default(), [1_i64, 0, self.bundle.latent_dim as i64])
            .map_err(ort_error("create empty voice sequence"))?;
        let text = Tensor::from_array((vec![1_i64, frames as i64, self.bundle.conditioning_dim as i64], embeddings.to_vec().into_boxed_slice()))
            .map_err(ort_error("create voice embedding tensor"))?;
        let mut state = initialize_state(&self.bundle.flow_lm_state_manifest)?;
        let mut inputs = vec![
            (Cow::Borrowed("sequence"), SessionInputValue::from(sequence)),
            (Cow::Borrowed("text_embeddings"), SessionInputValue::from(text)),
        ];
        append_state_inputs(&mut inputs, &state);
        let mut outputs = self.flow_main.run(inputs).map_err(ort_error("condition Pocket TTS voice"))?;
        replace_state_from_outputs(&mut state, &mut outputs)?;
        drop(outputs);
        Ok(state)
    }

    fn text_embeddings(&mut self, token_ids: Vec<i64>) -> Result<Vec<f32>, String> {
        let tokens = Tensor::from_array((vec![1_i64, token_ids.len() as i64], token_ids.into_boxed_slice()))
            .map_err(ort_error("create token tensor"))?;
        let outputs = self.text_conditioner.run(ort::inputs!["token_ids" => tokens]).map_err(ort_error("run text conditioner"))?;
        let (_, e) = outputs[0].try_extract_tensor::<f32>().map_err(ort_error("extract text embeddings"))?;
        Ok(e.to_vec())
    }

    fn run_flow_main_prefix(&mut self, text: &[f32], state: &mut [StateValue]) -> Result<(), String> {
        if !text.len().is_multiple_of(self.bundle.conditioning_dim) {
            return Err(format!("text conditioner returned {} values", text.len()));
        }
        let frames = text.len() / self.bundle.conditioning_dim;
        let sequence = Tensor::<f32>::new(&ort::memory::Allocator::default(), [1_i64, 0, self.bundle.latent_dim as i64])
            .map_err(ort_error("create empty text sequence"))?;
        let text = Tensor::from_array((vec![1_i64, frames as i64, self.bundle.conditioning_dim as i64], text.to_vec().into_boxed_slice()))
            .map_err(ort_error("create text embedding tensor"))?;
        let mut inputs = vec![
            (Cow::Borrowed("sequence"), SessionInputValue::from(sequence)),
            (Cow::Borrowed("text_embeddings"), SessionInputValue::from(text)),
        ];
        append_state_inputs(&mut inputs, state);
        let mut outputs = self.flow_main.run(inputs).map_err(ort_error("prime Pocket TTS text state"))?;
        replace_state_from_outputs(state, &mut outputs)
    }

    fn generate_audio(&mut self, max_frames: usize, frames_after_eos: usize, state: &mut [StateValue], emit: &mut dyn FnMut(&[f32]) -> bool) -> Result<bool, String> {
        let dim = self.bundle.latent_dim;
        let mut current = vec![f32::NAN; dim];
        let mut latents = Vec::with_capacity(DECODER_CHUNK_FRAMES * dim);
        let mut decoder_state = initialize_state(&self.bundle.mimi_state_manifest)?;
        let mut eos_step = None;
        for step in 0..max_frames {
            let sequence = Tensor::from_array((vec![1_i64, 1, dim as i64], current.clone().into_boxed_slice()))
                .map_err(ort_error("create latent input"))?;
            let empty = Tensor::<f32>::new(&ort::memory::Allocator::default(), [1_i64, 0, self.bundle.conditioning_dim as i64])
                .map_err(ort_error("create empty text input"))?;
            let mut inputs = vec![
                (Cow::Borrowed("sequence"), SessionInputValue::from(sequence)),
                (Cow::Borrowed("text_embeddings"), SessionInputValue::from(empty)),
            ];
            append_state_inputs(&mut inputs, state);
            let mut outputs = self.flow_main.run(inputs).map_err(ort_error("run Pocket TTS Flow LM"))?;
            let conditioning = outputs[0].try_extract_tensor::<f32>().map_err(ort_error("extract Flow LM conditioning"))?.1.to_vec();
            let eos_logit = outputs[1]
                .try_extract_tensor::<f32>()
                .map_err(ort_error("extract Flow LM EOS logit"))?
                .1
                .first()
                .copied()
                .ok_or("Flow LM returned an empty EOS logit")?;
            replace_state_from_outputs(state, &mut outputs)?;
            drop(outputs);
            if eos_logit > EOS_LOGIT_THRESHOLD && eos_step.is_none() {
                eos_step = Some(step);
            }
            if eos_step.is_some_and(|eos| step >= eos + frames_after_eos) {
                break;
            }
            let mut noise = self.noise.normal(dim, DEFAULT_TEMPERATURE.sqrt());
            let c = Tensor::from_array((vec![1_i64, self.bundle.conditioning_dim as i64], conditioning.into_boxed_slice()))
                .map_err(ort_error("create flow conditioning"))?;
            let s = Tensor::from_array((vec![1_i64, 1], vec![0.0_f32].into_boxed_slice())).map_err(ort_error("create flow start"))?;
            let t = Tensor::from_array((vec![1_i64, 1], vec![1.0_f32].into_boxed_slice())).map_err(ort_error("create flow end"))?;
            let x = Tensor::from_array((vec![1_i64, dim as i64], noise.clone().into_boxed_slice())).map_err(ort_error("create flow noise"))?;
            let outputs = self.flow.run(ort::inputs!["c" => c, "s" => s, "t" => t, "x" => x]).map_err(ort_error("run Pocket TTS flow"))?;
            let flow = outputs[0].try_extract_tensor::<f32>().map_err(ort_error("extract Pocket TTS flow"))?.1;
            if flow.len() != noise.len() {
                return Err(format!("flow returned {} values; expected {}", flow.len(), noise.len()));
            }
            for (sample, delta) in noise.iter_mut().zip(flow) {
                *sample += *delta;
            }
            drop(outputs);
            current.clone_from(&noise);
            latents.extend_from_slice(&noise);
            if latents.len() == DECODER_CHUNK_FRAMES * dim {
                let audio = self.decode_block(&latents, &mut decoder_state)?;
                if !emit(&audio) { return Ok(false); }
                latents.clear();
            }
        }
        if !latents.is_empty() {
            let audio = self.decode_block(&latents, &mut decoder_state)?;
            if !emit(&audio) { return Ok(false); }
        }
        Ok(true)
    }

    fn decode_block(&mut self, latents: &[f32], state: &mut [StateValue]) -> Result<Vec<f32>, String> {
        let dim = self.bundle.latent_dim;
        let frames = latents.len() / dim;
        let latent = Tensor::from_array((vec![1_i64, frames as i64, dim as i64], latents.to_vec().into_boxed_slice()))
            .map_err(ort_error("create Mimi latent tensor"))?;
        let mut inputs = vec![(Cow::Borrowed("latent"), SessionInputValue::from(latent))];
        append_state_inputs(&mut inputs, state);
        let mut outputs = self.mimi_decoder.run(inputs).map_err(ort_error("run Mimi decoder"))?;
        let audio = outputs[0].try_extract_tensor::<f32>().map_err(ort_error("extract Mimi audio"))?.1.to_vec();
        replace_state_from_outputs(state, &mut outputs)?;
        Ok(audio)
    }
}

/// The voice recording as mono 24 kHz, whatever rate it was published at (44.1 kHz today).
fn read_voice(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let (mono, rate) = wav::read_pcm16(&bytes)?.into_mono();
    if mono.is_empty() {
        return Err(format!("{} holds no audio", path.display()));
    }
    Ok(wav::resample(&mono, rate, SAMPLE_RATE))
}

/// ONNX intra-op threads per graph. Two: the generation loop is many small sequential matmuls, and
/// Kyutai's own figure is "uses only 2 CPU cores". `RICHOS_TTS_THREADS` overrides for measuring.
pub fn threads() -> usize {
    std::env::var("RICHOS_TTS_THREADS").ok().and_then(|v| v.parse().ok()).filter(|&n: &usize| n >= 1).unwrap_or(2)
}

/// Rich speaking as Peter Yearsley through Pocket TTS.
pub struct PocketSynth {
    engine: Mutex<Engine>,
    /// Wall-clock cost of [`PocketSynth::load`], for the log line.
    pub load_ms: u64,
}

impl PocketSynth {
    /// Verify every pinned file's sha256 in `dir`, then load the graphs and condition the voice.
    /// Seconds of work, so callers run it off the audio path ([`crate::tts::RichVoice`] does).
    pub fn load(dir: &Path) -> Result<PocketSynth, String> {
        let started = Instant::now();
        for pin in &FILES {
            verify_file(&dir.join(pin.name), pin)?;
        }
        let engine = Engine::load(dir, threads())?;
        Ok(PocketSynth { engine: Mutex::new(engine), load_ms: started.elapsed().as_millis() as u64 })
    }
}

impl SpeechSynth for PocketSynth {
    fn synthesize(&self, text: &str, target_rate: u32, _scratch_dir: &Path) -> Result<Speech, TtsError> {
        let started = Instant::now();
        let audio = self
            .engine
            .lock()
            .map_err(|_| TtsError::Io("Pocket TTS engine lock poisoned".into()))?
            .synthesize(text)
            .map_err(TtsError::Decode)?;
        let samples = if target_rate == SAMPLE_RATE { audio } else { wav::resample(&audio, SAMPLE_RATE, target_rate) };
        Ok(Speech { samples, sample_rate: target_rate, synth_ms: started.elapsed().as_millis() as u64 })
    }

    fn synthesize_stream(&self, text: &str, target_rate: u32, _scratch_dir: &Path, emit: &mut dyn FnMut(&[f32]) -> bool) -> Result<(), TtsError> {
        let started = Instant::now();
        let mut converter = wav::RateConverter::new(SAMPLE_RATE, target_rate);
        let mut first = true;
        let mut samples = 0;
        self.engine.lock().map_err(|_| TtsError::Io("Pocket TTS engine lock poisoned".into()))?
            .synthesize_stream(text, &mut |audio| {
                let mut pcm = Vec::new();
                converter.push(audio, &mut pcm);
                if pcm.is_empty() { return true; }
                if first {
                    first = false;
                    eprintln!("[richos-voice] Pocket TTS first PCM in {} ms", started.elapsed().as_millis());
                }
                samples += pcm.len();
                emit(&pcm)
            }).map_err(TtsError::Decode)?;
        eprintln!("[richos-voice] tts: {:.2} s of speech streamed in {} ms by {}", samples as f64 / target_rate as f64, started.elapsed().as_millis(), self.voice_label());
        Ok(())
    }

    fn voice_label(&self) -> String {
        format!("Pocket TTS {MODEL_LANGUAGE} INT8 · Peter Yearsley · {} threads", threads())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real pinned inference, silent. Opt in with RICHOS_POCKET_TEST_DIR and --ignored.
    #[test]
    #[ignore = "requires the nine pinned Pocket TTS files; produces PCM without using an audio device"]
    fn pinned_voice_streams_audio_before_sentence_completion_and_cancels_generation() {
        let dir = std::path::PathBuf::from(std::env::var("RICHOS_POCKET_TEST_DIR").expect("set the pinned model directory"));
        let synth = PocketSynth::load(&dir).expect("pinned voice loads");
        let text = "Start with the dental practices you already serve. Ask their owners which nearby practices have the same accounting problems, then offer those practices a short demonstration.";
        let started = Instant::now();
        let mut first_ms = None;
        let mut blocks = Vec::new();
        synth.synthesize_stream(text, SAMPLE_RATE, &dir, &mut |pcm| {
            first_ms.get_or_insert_with(|| started.elapsed().as_millis());
            blocks.push(pcm.to_vec());
            true
        }).unwrap();
        assert!(blocks.len() > 2, "a long sentence was buffered instead of streamed");
        assert!(blocks.iter().all(|pcm| !pcm.is_empty() && pcm.iter().all(|v| v.is_finite())));
        assert!(blocks.iter().flatten().any(|v| v.abs() > 0.01), "voice must contain actual speech");
        let total_ms = started.elapsed().as_millis();
        // The same seeded inference must preserve the waveform across 48 kHz PCM blocks.
        synth.engine.lock().unwrap().noise = Noise(42);
        let buffered = synth.synthesize(text, 48_000, &dir).unwrap();
        synth.engine.lock().unwrap().noise = Noise(42);
        let mut converted = Vec::new();
        synth.synthesize_stream(text, 48_000, &dir, &mut |pcm| { converted.extend_from_slice(pcm); true }).unwrap();
        assert_eq!(converted, buffered.samples[..converted.len()], "block boundaries must not change the speech waveform");
        assert!(buffered.samples.len() - converted.len() <= 4, "only converter lookahead at the silent tail can be withheld");
        let mut accepted = 0;
        synth.synthesize_stream(text, SAMPLE_RATE, &dir, &mut |_| { accepted += 1; false }).unwrap();
        assert_eq!(accepted, 1, "generation must stop as soon as playback rejects an obsolete answer");
        let after_cancel = synth.synthesize("On it!", SAMPLE_RATE, &dir).unwrap();
        assert!(!after_cancel.samples.is_empty(), "a canceled sentence must not poison the next answer");
        eprintln!("POCKET_STREAM_PROOF first_pcm_ms={} complete_ms={total_ms} blocks={} samples={} canceled_blocks={accepted}", first_ms.unwrap(), blocks.len(), blocks.iter().map(Vec::len).sum::<usize>());
    }


    /// INVARIANT: the download is exactly the nine pinned files, the voice is Peter Yearsley's,
    /// and every hash is a full sha256. A tenth file or a different voice is a commit, not drift.
    #[test]
    fn the_pins_are_the_bundle_and_the_peter_yearsley_voice() {
        assert_eq!(total_bytes(), 165_756_868);
        assert!(FILES.iter().any(|f| f.name == VOICE_FILE && f.url.ends_with("voice-zero/peter_yearsley.wav")));
        for f in &FILES {
            assert_eq!(f.sha256.len(), 64, "{}", f.name);
            assert!(f.url.starts_with("https://huggingface.co/"), "{}", f.name);
            assert!(f.url.contains("/resolve/") && !f.url.contains("/resolve/main/"), "{} must be pinned to a revision", f.name);
        }
        assert!(FILES.iter().filter(|f| f.name != VOICE_FILE).all(|f| f.url.starts_with(MODEL_BASE)));
    }

    #[test]
    fn a_file_that_is_not_the_pinned_bytes_is_refused() {
        let dir = std::env::temp_dir().join(format!("richos-pocket-pin-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let pin = PinnedFile { name: "x.bin", url: "https://huggingface.co/a/resolve/1/x.bin", bytes: 3, sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" };
        fs::write(dir.join("x.bin"), b"abc").unwrap();
        assert!(verify_file(&dir.join("x.bin"), &pin).is_ok());
        fs::write(dir.join("x.bin"), b"abd").unwrap();
        assert!(verify_file(&dir.join("x.bin"), &pin).unwrap_err().contains("hashes to"));
        fs::write(dir.join("x.bin"), b"ab").unwrap();
        assert!(verify_file(&dir.join("x.bin"), &pin).unwrap_err().contains("2 bytes"));
        assert_eq!(missing_files(&dir).len(), FILES.len());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prompts_are_prepared_as_the_bundle_expects() {
        assert_eq!(prepare_prompt("  hello   there "), Some(("Hello there.".into(), 5)));
        assert_eq!(prepare_prompt("Acme signed at four and a half million!"), Some(("Acme signed at four and a half million!".into(), 3)));
        assert_eq!(prepare_prompt("   "), None);
    }

    #[test]
    fn long_text_splits_at_sentences_within_the_limit() {
        let words = |t: &str| Ok(t.split_whitespace().count());
        let parts = split_at_natural_boundaries("One two three. Four five six. Seven.", 4, words).unwrap();
        assert_eq!(parts, vec!["One two three. ", "Four five six. Seven."], "sentences are packed while they fit");
        assert_eq!(parts.concat(), "One two three. Four five six. Seven.");
        let parts = split_at_natural_boundaries("Dr. Smith went home, then slept well tonight.", 5, words).unwrap();
        assert_eq!(parts[0], "Dr. Smith went home, ");
    }

    #[test]
    fn noise_is_standard_normal_shaped() {
        let mut n = Noise(42);
        let v = n.normal(20_000, 1.0);
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / v.len() as f32;
        assert!(mean.abs() < 0.05, "mean {mean}");
        assert!((var - 1.0).abs() < 0.05, "variance {var}");
    }
}
