//! Which model this machine can actually carry — asked, not assumed.
//!
//! # Why this module exists
//!
//! Until 2026-09-10 `stt.rs:42` read `pub const DEFAULT_MODEL_ID: &str = "small.en";` — a
//! compile-time constant, identical on every machine, overridable only by an environment variable
//! a non-technical CEO will never set, and pinned there by a test that asserted the string. A
//! repo-wide grep for `sysctl`, `hw.memsize`, `total_memory` or `sysinfo` across `app/crates`
//! returned nothing: **the app never asked the machine anything.** The CEO put it as
//! *"WHY IS THE SHITTIEST POSSIBLE HARDWARE HARDCODED INTO THE APP???"*
//!
//! # What it turned out to be, which is not what anyone expected
//!
//! Physical RAM is the obvious input and it is the wrong one. Measured at utterance length on the
//! CEO's own M4 (`docs/measurements/hardware-model-resolution-2026-09-10/`):
//!
//! | model | peak RSS | one utterance |
//! |---|---|---|
//! | `tiny.en` | 248,545,280 B | 0.202 s |
//! | `base.en` | 367,149,056 B | 0.248 s |
//! | `small.en` | **867,516,416 B** | 0.512 s |
//! | `large-v3-turbo-q5_0` | **858,570,752 B** | 1.302 s |
//! | `large-v3-turbo` | 1,989,836,800 B | 1.522 s |
//!
//! **`q5_0` costs 8,945,664 B LESS memory than `small.en`** — the quantized turbo is cheaper
//! resident than the model `config.js`'s tier table calls the fallback for *"low-RAM hosts"*.
//! `small.en` is fp16 and `q5_0` is 5-bit quantized; the turbo is larger on DISK (574,041,195 B
//! against 487,614,201 B) and that is the figure the constant was defended with. Disk size is not
//! memory size. Every model on that ladder, largest included, fits several times over in what a
//! working 24 GB Mac has spare (5,873,516,544 B measured under heavy load), so **memory does not
//! discriminate between these models on any Mac Apple ships.**
//!
//! Decode TIME does: 0.202 s to 2.108 s, a 10.4x span, and it is the axis that varies across
//! machines. So this module gates on measured time and keeps memory as a guard that should never
//! fire.
//!
//! # The two candidates that were measured and thrown away
//!
//! Both are the obvious thing to reach for and both are disqualified by
//! `measurements/machine-state.txt` rather than by opinion:
//!
//! - **`kern.memorystatus_vm_pressure_level` reads WARN on the CEO's machine in all six samples**
//!   during ordinary work. Not a spike — the steady state, on 24 GB with 5.8 GB spare. A resolver
//!   that demoted on elevated pressure would demote him permanently and would have looked
//!   completely principled in review. It is read here for the record and it decides nothing.
//! - **Available memory swung 315,375,616 B across six samples in six seconds** — a third of the
//!   entire ladder's memory span. Resolution keyed to it would depend on the second voice mode
//!   started, and the model id is written into every transcript's provenance line, so two
//!   transcripts from one machine on one afternoon would stop being comparable. It is kept as a
//!   guard only, where both sides of the comparison are measured bytes and no constant is chosen.
//!
//! # The ceiling is the record's, not this module's
//!
//! `stt.rs`'s module docs are the only place in either repository that says what a spoken
//! conversation may cost: *"a second of dead air after every sentence is the difference between
//! talking to Rich and operating him"*, against a promise of *"~0.5 s"*. So the ceiling is
//! **1.000 s** and the target is 0.5 s, both quoted rather than invented, both carried in
//! `model-costs.json` beside the figure they gate.
//!
//! # What this means for the live path, stated plainly
//!
//! **On every Mac measured today the live ladder can only step DOWN from `small.en`.** `q5_0`
//! medians 1.549 s busy and 1.331 s quiet on the CEO's M4 — 55% and 33% over the ceiling — and
//! his M4 is the fastest machine in play, so no slower machine makes it cheaper. The honest live
//! rule is therefore not "give the good machine the better model"; it is **"stop giving a slow
//! machine a model it cannot decode in time."** An M1 Air at 2x this cost would sit near 1.6 s per
//! utterance and the conversation would be unusable — and today it gets `small.en` regardless,
//! because a constant does not know what machine it is on.
//!
//! `q5_0` stays at the TOP of the live ladder anyway. The door opens by itself, with no code
//! change and no env var, on the first machine that decodes it under the ceiling. Recording that
//! no such machine exists yet is better than pretending the ladder has a rung it can reach.
//!
//! # The env override survives and stops being the only control
//!
//! `RICHOS_VOICE_WHISPER_MODEL_ID` still wins outright — engineers need it and reproducing a
//! measurement needs it. What changes is that someone who will never set one now gets an answer
//! read off their own machine instead of a hand-picked constant.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The measured cost table and the ladders, from the ONE place they live.
///
/// `include_str!` for the same reason `toolchain.rs` compiles in `model-pins.json`: the Node call
/// path reads this identical file, and a table this crate re-typed would be a table free to drift.
const MODEL_COSTS_JSON: &str = include_str!("../../../../engine/voice/models/model-costs.json");

// -------------------------------------------------------------------------------------------
// What the machine says about itself.
// -------------------------------------------------------------------------------------------

/// macOS's own view of how squeezed it is. Read and recorded; it decides NOTHING — see the module
/// docs for the six samples that disqualified it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressure {
    Normal,
    Warn,
    Critical,
    /// The sysctl was not readable. Distinct from `Normal` on purpose: "the machine says it is
    /// fine" and "the machine did not answer" are different facts and only one of them is good
    /// news.
    Unknown,
}

impl Pressure {
    fn from_level(level: i32) -> Pressure {
        match level {
            1 => Pressure::Normal,
            2 => Pressure::Warn,
            4 => Pressure::Critical,
            _ => Pressure::Unknown,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Pressure::Normal => "normal",
            Pressure::Warn => "warn",
            Pressure::Critical => "critical",
            Pressure::Unknown => "unreadable",
        }
    }
}

/// Everything the resolver is allowed to know about the host.
///
/// A PLAIN STRUCT WITH PUBLIC FIELDS, so every test constructs the machine it wants to test
/// against instead of mocking a syscall. That is the whole reason the rule below is a pure
/// function over this type: a forced 4 GB machine is a struct literal, not an environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    /// `hw.memsize`. Deterministic, and the only memory figure worth putting in a record.
    pub total_bytes: u64,
    /// free + inactive + speculative + purgeable, at the moment of reading. Volatile by ~315 MB
    /// over six seconds on a real desktop, which is why it guards and never discriminates.
    pub available_bytes: u64,
    /// Recorded, never acted on.
    pub pressure: Pressure,
    /// `hw.perflevel0.logicalcpu` + `hw.perflevel1.logicalcpu`, falling back to `hw.ncpu`.
    /// Carried for the record and for the speed cache's fingerprint. It does NOT gate anything:
    /// a core count predicts latency only through a constant nobody has measured on a second
    /// machine, and inventing that constant is the defect this module exists to remove.
    pub cores: u32,
}

impl Machine {
    /// Read the host. Never fails: an unreadable sysctl yields a conservative machine rather than
    /// an error, because "we could not tell" must degrade to the safe rung, not to no voice mode.
    pub fn read() -> Machine {
        // The two test seams. They force the RESOLVER'S INPUT rather than faking a decision, which
        // is what makes "a forced low-memory condition resolves to the smaller model" a thing
        // anyone can demonstrate on a machine that has plenty of memory.
        let forced_total = env_u64("RICHOS_HW_TOTAL_MEMORY_BYTES");
        let forced_avail = env_u64("RICHOS_HW_AVAILABLE_MEMORY_BYTES");

        let total = forced_total.or_else(|| sysctl_u64("hw.memsize")).unwrap_or(0);
        let page = sysctl_u64("hw.pagesize").unwrap_or(16384);
        let available = forced_avail.or_else(|| available_pages().map(|p| p * page)).unwrap_or(0);
        let pressure = sysctl_i32("kern.memorystatus_vm_pressure_level")
            .map(Pressure::from_level)
            .unwrap_or(Pressure::Unknown);
        let cores = match (sysctl_u64("hw.perflevel0.logicalcpu"), sysctl_u64("hw.perflevel1.logicalcpu")) {
            (Some(p), Some(e)) => (p + e) as u32,
            _ => sysctl_u64("hw.ncpu").unwrap_or(1) as u32,
        };
        Machine { total_bytes: total, available_bytes: available, pressure, cores }
    }

    /// The memory a candidate has to fit inside.
    ///
    /// `available` when it was readable; otherwise `total`, which is the conservative answer only
    /// because a zero would refuse every model including the smallest and leave the CEO with no
    /// voice mode at all. A guard that fails closed onto silence is worse than the constant.
    fn budget_bytes(&self) -> u64 {
        if self.available_bytes > 0 {
            self.available_bytes
        } else {
            self.total_bytes
        }
    }
}

// -------------------------------------------------------------------------------------------
// The cost table.
// -------------------------------------------------------------------------------------------

/// One model's measured price, as the registry records it.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCost {
    pub id: String,
    /// Peak RSS of the `whisper-cli` process decoding one utterance, `/usr/bin/time -l`, bytes.
    pub utterance_peak_rss: u64,
    /// What one utterance cost on the reference machine. Used for exactly one thing: dividing
    /// this machine's measurement by it to get a speed factor.
    pub reference_utterance_secs: f64,
    /// Seconds of compute per second of audio at 92 minutes. Only the batch models carry it.
    pub long_form_rate: Option<f64>,
    pub long_form_peak_rss: Option<u64>,
    /// What this model costs to DOWNLOAD, in bytes on disk.
    ///
    /// Carried here so the sentence that asks the CEO for half a gigabyte of his disk gets its
    /// number from the cost table rather than from a literal somebody typed into a UI string.
    /// `model-costs.json` sources every one of these from `model-pins.json` in its own
    /// `provenance` field, and `the_two_tables_agree_on_every_size` asserts they still do — so the
    /// number a person is shown and the number the transfer is checked against are one number with
    /// two readers, not two numbers that happen to match today.
    pub disk_bytes: u64,
}

/// The parsed registry: costs, ladders and the two gates.
#[derive(Debug, Clone)]
pub struct Costs {
    pub models: BTreeMap<String, ModelCost>,
    pub live_ladder: Vec<String>,
    pub batch_ladder: Vec<String>,
    /// 1.000 s — `stt.rs`'s stated failure point, not this module's choice.
    pub live_ceiling_secs: f64,
    /// 0.5 s — `stt.rs`'s stated promise, used in what the user is told.
    pub live_target_secs: f64,
    /// 1.0x — the real-time boundary for a batch decode.
    pub batch_real_time_multiple: f64,
    /// Where an UNMEASURABLE machine lands. Not the bottom rung — see the registry's own note:
    /// the bottom is where a measured-and-slow machine goes, and giving it to a machine nobody
    /// managed to time would be a downgrade justified by an absence of evidence.
    pub safe_rung: String,
    pub probe_text: String,
    pub probe_duration_secs: f64,
}

impl Costs {
    /// Parse the compiled-in registry. Panics only on a malformed registry, which is a source
    /// file in this repository and therefore a build-time mistake rather than a runtime condition.
    pub fn load() -> Costs {
        let v: Value = serde_json::from_str(MODEL_COSTS_JSON).expect("model-costs.json is malformed");
        let mut models = BTreeMap::new();
        for m in v["models"].as_array().expect("model-costs.json: models must be an array") {
            let id = m["id"].as_str().expect("model-costs.json: every model needs an id").to_string();
            models.insert(
                id.clone(),
                ModelCost {
                    id,
                    utterance_peak_rss: m["utterancePeakRssBytes"].as_u64().unwrap_or(0),
                    reference_utterance_secs: m["referenceUtteranceSeconds"].as_f64().unwrap_or(0.0),
                    long_form_rate: m["longFormSecondsPerAudioSecond"].as_f64(),
                    long_form_peak_rss: m["longFormPeakRssBytes"].as_u64(),
                    disk_bytes: m["diskBytes"].as_u64().unwrap_or(0),
                },
            );
        }
        let ladder = |k: &str| -> Vec<String> {
            v["ladders"][k]
                .as_array()
                .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
                .unwrap_or_default()
        };
        Costs {
            models,
            live_ladder: ladder("live"),
            batch_ladder: ladder("batch"),
            live_ceiling_secs: v["liveUtteranceCeilingSeconds"]["value"].as_f64().unwrap_or(1.0),
            live_target_secs: v["liveUtteranceCeilingSeconds"]["targetSeconds"].as_f64().unwrap_or(0.5),
            batch_real_time_multiple: v["batchRealTimeMultiple"]["value"].as_f64().unwrap_or(1.0),
            safe_rung: v["safeRung"]["value"].as_str().unwrap_or("small.en").to_string(),
            probe_text: v["probe"]["text"].as_str().unwrap_or("").to_string(),
            probe_duration_secs: v["probe"]["durationSeconds"].as_f64().unwrap_or(3.095),
        }
    }

    /// Drop every rung whose weights are not on this machine.
    ///
    /// WITHOUT THIS THE LADDER IS FICTION. A rung names a model id, and a model id is only a
    /// model if `ggml-<id>.bin` is somewhere `resolve_model` looks — a machine carrying only
    /// `small.en` has no `tiny.en` to fall back to and no `q5_0` to be promoted to. Filtering
    /// here rather than inside the walk keeps the rule a pure function of the ladder it is
    /// given, and makes "what could this machine have used?" answerable in one place.
    ///
    /// The safe rung is NOT protected from filtering: if it is absent too, the caller is about to
    /// meet `SttError::ModelNotFound`, which is already a calm sentence at the toggle and is the
    /// correct outcome for a machine with no weights at all.
    pub fn retain_installed(&mut self, present: impl Fn(&str) -> bool) {
        self.live_ladder.retain(|id| present(id));
        self.batch_ladder.retain(|id| present(id));
    }
}

// -------------------------------------------------------------------------------------------
// The answer.
// -------------------------------------------------------------------------------------------

/// Why a rung was taken, in the vocabulary the record and the CEO both need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// An engineer named the model outright. The resolver did not run.
    Override,
    /// The person chose this model himself, in Dictation's Accuracy setting (More accurate or
    /// Faster). The resolver did not run: dictation is not bound by voice mode's one-second live
    /// ceiling, and he picks speed against accuracy himself (dictation plan section 2).
    Chosen,
    /// This machine decoded the top rung in time and had the memory for it.
    TopRung,
    /// A better rung exists and this machine decodes it too slowly.
    TooSlow,
    /// A better rung exists and does not fit this machine's memory. Has never fired on a real Mac.
    TooLarge,
    /// Nothing could be measured — no probe, no binary, no cache. The safe rung, said out loud.
    Unmeasured,
}

/// What the resolver decided, why, and everything needed to say it to a person or to a record.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub model_id: String,
    pub basis: Basis,
    /// The rung above this one that was rejected, and what it measured. `None` at the top.
    pub rejected: Option<(String, f64)>,
    /// This machine's measured cost for the chosen model, when it was measured.
    pub measured_secs: Option<f64>,
    pub ceiling_secs: f64,
    pub machine: Machine,
}

impl Resolution {
    /// The fragment that joins the toolchain's provenance line, so every transcript records not
    /// just WHICH weights heard it but WHY those weights were the ones on this machine.
    pub fn provenance(&self) -> String {
        let how = match self.basis {
            Basis::Override => "env-override".to_string(),
            Basis::Chosen => "chosen in Dictation settings".to_string(),
            Basis::TopRung => match self.measured_secs {
                Some(s) => format!("hw-resolved top rung, {s:.3}s/utt <= {:.3}s", self.ceiling_secs),
                None => "hw-resolved top rung".to_string(),
            },
            Basis::TooSlow => match (&self.rejected, self.measured_secs) {
                (Some((r, rs)), Some(s)) => format!(
                    "hw-resolved, {r} too slow here at {rs:.3}s/utt > {:.3}s; this {s:.3}s/utt",
                    self.ceiling_secs
                ),
                (Some((r, rs)), None) => format!("hw-resolved, {r} too slow here at {rs:.3}s/utt"),
                _ => "hw-resolved, better rung too slow".to_string(),
            },
            Basis::TooLarge => match &self.rejected {
                Some((r, _)) => {
                    format!("hw-resolved, {r} does not fit {} B available", self.machine.available_bytes)
                }
                None => "hw-resolved on memory".to_string(),
            },
            Basis::Unmeasured => "hw-unmeasured, safe rung".to_string(),
        };
        format!(
            "model:{} ({how}) mem:{}B/{}B pressure:{}",
            self.model_id, self.machine.available_bytes, self.machine.total_bytes, self.machine.pressure.as_str()
        )
    }

    /// Is this worth telling the user about at all?
    ///
    /// The top rung on a healthy machine is the product working as designed and does not deserve a
    /// notice. Anything else is the product having quietly made a choice on the CEO's behalf, and
    /// the brief for this work is explicit that such a choice is NOT a routine notification to be
    /// suppressed — it is the product explaining itself.
    pub fn is_noteworthy(&self) -> bool {
        !matches!(self.basis, Basis::TopRung | Basis::Override)
    }

    /// What the CEO reads. Plain language, no paths, no model filenames, no byte counts — the same
    /// register `SttError::ceo_message` already uses, and for the same reason: he cannot act on a
    /// path and should not have to read one.
    ///
    /// It says WHAT changed and WHY, because a degradation nobody is told about is the defect this
    /// module was built to remove, and a silent one in the other direction would be the same defect
    /// wearing better manners.
    pub fn ceo_message(&self) -> Option<String> {
        match self.basis {
            Basis::Override | Basis::Chosen | Basis::TopRung => None,
            Basis::TooSlow => Some(
                "I'm using my faster hearing on this machine. The more accurate one takes long \
                 enough here that you'd be waiting after every sentence, and a conversation with \
                 pauses in it isn't a conversation. You can still say anything you like. I just \
                 might misread an unusual name now and then."
                    .into(),
            ),
            Basis::TooLarge => Some(
                "I'm using my lighter hearing on this machine: there isn't enough free memory \
                 right now for the more accurate one, and I'd rather not slow everything else \
                 down. You can still say anything you like."
                    .into(),
            ),
            Basis::Unmeasured => Some(
                "I couldn't work out how fast this machine hears, so I've started with the \
                 setting that works everywhere. I'll check again next time you turn voice on."
                    .into(),
            ),
        }
    }
}

// -------------------------------------------------------------------------------------------
// The rule.
// -------------------------------------------------------------------------------------------

/// Walk the live ladder best-first and take the first rung this machine clears on BOTH gates.
///
/// `probe` measures one model on THIS machine and returns seconds, or `None` when it could not be
/// measured. It is a closure so the rule is a pure function of (machine, costs, measurements):
/// every test below decides an outcome without a decoder, a WAV or a subprocess, which is the only
/// way a resolution rule gets tested at all.
///
/// THE SHAPE OF THE GATES MATTERS. Time first, memory second, and memory is `<=` against what the
/// machine has spare. Neither gate carries a multiplier, a headroom fraction or a reserve: both
/// sides of both comparisons are measured quantities. A "reserve N GB for the system" rule was
/// drafted and thrown away when `machine-state.txt` showed this machine holding 11,463,786,496 B
/// of compressed pages — a working set scales with the machine, so a reserve derived on one
/// machine does not transfer to another.
pub fn resolve_live<F>(costs: &Costs, machine: &Machine, mut probe: F) -> Resolution
where
    F: FnMut(&str) -> Option<f64>,
{
    let budget = machine.budget_bytes();
    let mut rejected: Option<(String, f64)> = None;
    let ladder_len = costs.live_ladder.len();

    for (i, id) in costs.live_ladder.iter().enumerate() {
        let Some(cost) = costs.models.get(id) else { continue };
        let last = i + 1 == ladder_len;

        // MEMORY GATE. The bottom rung is exempt: refusing it leaves the CEO with no voice mode,
        // and "we have too little memory to hear you at all" is not an outcome this product gets
        // to have. On every Mac measured this gate is silent anyway.
        if !last && cost.utterance_peak_rss > budget {
            rejected = Some((id.clone(), 0.0));
            continue;
        }

        // TIME GATE. The bottom rung is exempt for the same reason.
        match probe(id) {
            Some(secs) if secs <= costs.live_ceiling_secs || last => {
                let basis = if i == 0 {
                    Basis::TopRung
                } else if rejected.as_ref().map(|(_, s)| *s > 0.0).unwrap_or(false) {
                    Basis::TooSlow
                } else {
                    Basis::TooLarge
                };
                return Resolution {
                    model_id: id.clone(),
                    basis,
                    rejected,
                    measured_secs: Some(secs),
                    ceiling_secs: costs.live_ceiling_secs,
                    machine: *machine,
                };
            }
            Some(secs) => {
                rejected = Some((id.clone(), secs));
            }
            None => continue,
        }
    }

    // NOTHING ON THE LADDER COULD BE MEASURED — no probe voice, no decoder, an empty ladder after
    // filtering. The safe rung, NOT the bottom rung: the bottom is where a machine goes once it
    // has been measured and found slow, and handing it to a machine nobody managed to time would
    // be a silent downgrade justified by an absence of evidence. Said out loud either way.
    Resolution {
        model_id: costs.safe_rung.clone(),
        basis: Basis::Unmeasured,
        rejected,
        measured_secs: None,
        ceiling_secs: costs.live_ceiling_secs,
        machine: *machine,
    }
}

/// This machine's speed relative to the reference host, from one measured model.
///
/// 1.0 means "as fast as the M4 every reference figure was taken on"; 2.0 means half the speed.
/// STATED AS AN ASSUMPTION rather than a result: that utterance-level speed carries over to
/// long-form throughput is untested, because no machine other than the reference host has been
/// measured at all. It is used only by the batch gate, which sits 21x clear of its threshold on
/// the reference machine, so the assumption would have to be wrong by more than an order of
/// magnitude before it changed an outcome.
pub fn speed_factor(costs: &Costs, model_id: &str, measured_secs: f64) -> Option<f64> {
    let cost = costs.models.get(model_id)?;
    if cost.reference_utterance_secs <= 0.0 {
        return None;
    }
    Some(measured_secs / cost.reference_utterance_secs)
}

/// The batch rule: never promoted above the CEO decision page §10 ruling, demoted when this
/// machine could not keep up with the recording in real time.
///
/// Kept in Rust as the SPECIFICATION and as the tested article; the shipping batch decoder is
/// `tools/richos-service/lib/config.js` and reads the same registry. Both are checked against the
/// same numbers, which is the point of the registry.
pub fn resolve_batch(costs: &Costs, machine: &Machine, speed_factor: f64) -> Resolution {
    let budget = machine.budget_bytes();
    let mut rejected: Option<(String, f64)> = None;
    let ladder_len = costs.batch_ladder.len();

    for (i, id) in costs.batch_ladder.iter().enumerate() {
        let Some(cost) = costs.models.get(id) else { continue };
        let last = i + 1 == ladder_len;

        if !last {
            if let Some(rss) = cost.long_form_peak_rss {
                if rss > budget {
                    rejected = Some((id.clone(), 0.0));
                    continue;
                }
            }
            if let Some(rate) = cost.long_form_rate {
                let projected = rate * speed_factor;
                if projected >= costs.batch_real_time_multiple {
                    rejected = Some((id.clone(), projected));
                    continue;
                }
            }
        }

        let basis = if i == 0 {
            Basis::TopRung
        } else if rejected.as_ref().map(|(_, s)| *s > 0.0).unwrap_or(false) {
            Basis::TooSlow
        } else {
            Basis::TooLarge
        };
        return Resolution {
            model_id: id.clone(),
            basis,
            rejected,
            measured_secs: cost.long_form_rate.map(|r| r * speed_factor),
            ceiling_secs: costs.batch_real_time_multiple,
            machine: *machine,
        };
    }

    Resolution {
        model_id: "small.en".into(),
        basis: Basis::Unmeasured,
        rejected,
        measured_secs: None,
        ceiling_secs: costs.batch_real_time_multiple,
        machine: *machine,
    }
}

// -------------------------------------------------------------------------------------------
// Reading the host.
// -------------------------------------------------------------------------------------------

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.trim().parse::<u64>().ok()
}

fn sysctl_u64(name: &str) -> Option<u64> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut out: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let rc = unsafe {
        libc::sysctlbyname(c.as_ptr(), &mut out as *mut u64 as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0)
    };
    // A sysctl that answered with the wrong width answered a different question. `hw.ncpu` is
    // 32-bit and `hw.memsize` is 64-bit, and reading one as the other silently yields a plausible
    // number, which is the worst failure available here.
    if rc == 0 && len == std::mem::size_of::<u64>() {
        return Some(out);
    }
    let mut out32: u32 = 0;
    let mut len32 = std::mem::size_of::<u32>();
    let rc = unsafe {
        libc::sysctlbyname(c.as_ptr(), &mut out32 as *mut u32 as *mut libc::c_void, &mut len32, std::ptr::null_mut(), 0)
    };
    if rc == 0 && len32 == std::mem::size_of::<u32>() {
        Some(out32 as u64)
    } else {
        None
    }
}

fn sysctl_i32(name: &str) -> Option<i32> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut out: i32 = 0;
    let mut len = std::mem::size_of::<i32>();
    let rc = unsafe {
        libc::sysctlbyname(c.as_ptr(), &mut out as *mut i32 as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0)
    };
    if rc == 0 {
        Some(out)
    } else {
        None
    }
}

/// free + inactive + speculative + purgeable, in PAGES.
///
/// The same four buckets `vm_stat` reports and `tools/machine-state.py` sums, so the number the
/// resolver acts on and the number in the measurement record are the same number.
///
/// `mach_host_self` is deprecated in `libc`, which points at the `mach2` crate instead. It is used
/// anyway, with the deprecation allowed at the call site rather than crate-wide: `mach2` is not in
/// this workspace's lock, and taking a new supply-chain dependency for one function call is a
/// worse trade than one annotated line. If `mach2` ever arrives for another reason, this is three
/// lines.
fn available_pages() -> Option<u64> {
    unsafe {
        #[allow(deprecated)]
        let port = libc::mach_host_self();
        let mut st: libc::vm_statistics64 = std::mem::zeroed();
        let mut count =
            (std::mem::size_of::<libc::vm_statistics64>() / std::mem::size_of::<libc::integer_t>()) as u32;
        let rc = libc::host_statistics64(
            port,
            libc::HOST_VM_INFO64,
            &mut st as *mut _ as *mut libc::integer_t,
            &mut count,
        );
        if rc != 0 {
            return None;
        }
        Some(
            st.free_count as u64
                + st.inactive_count as u64
                + st.speculative_count as u64
                + st.purgeable_count as u64,
        )
    }
}

// -------------------------------------------------------------------------------------------
// The speed cache — measure once per machine, not once per launch.
// -------------------------------------------------------------------------------------------

/// Where the measured decode speeds live, beside the toolchain lock the two consumers already
/// share. `RICHOS_WHISPER_SPEED_CACHE` overrides, which is how the tests get a private one.
pub fn speed_cache_path() -> PathBuf {
    if let Ok(p) = std::env::var("RICHOS_WHISPER_SPEED_CACHE") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/richos/whisper-speed.json")
}

/// The cache key. A measurement belongs to a machine AND to the binary that produced it: a
/// `brew upgrade` can change decode cost, and a cached speed attributed to the wrong build is
/// exactly the "settings are deliberate and the binary they are handed to can change with no
/// trace" hole `toolchain.rs` was written to close.
pub fn cache_key(bin_sha: &str, machine: &Machine) -> String {
    format!("{}|{}|{}", bin_sha.chars().take(12).collect::<String>(), machine.total_bytes, machine.cores)
}

/// Measured seconds per model for this key, or an empty map.
pub fn load_speeds(key: &str) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(speed_cache_path()) else { return out };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return out };
    if let Some(obj) = v.get(key).and_then(|e| e.get("models")).and_then(|m| m.as_object()) {
        for (k, val) in obj {
            if let Some(f) = val.as_f64() {
                out.insert(k.clone(), f);
            }
        }
    }
    out
}

/// Record a measurement, keeping the MINIMUM ever seen for that model on this machine.
///
/// The minimum, not the latest and not a mean, and the measurement record is why: the same M4
/// decoded ~50% slower while busy (median 0.800 s against 0.521 s for `small.en`). A sample taken
/// while something else was running under-rates the machine, and a mean drags the estimate toward
/// whatever else the CEO happened to have open. The minimum converges on what the machine can
/// actually do and cannot be dragged down by load.
pub fn record_speed(key: &str, model_id: &str, secs: f64) {
    update_entry(key, |entry| {
        if let Some(models) = entry["models"].as_object_mut() {
            let keep = match models.get(model_id).and_then(|v| v.as_f64()) {
                Some(prev) if prev <= secs => prev,
                _ => secs,
            };
            models.insert(model_id.to_string(), serde_json::json!(keep));
        }
        // Settled: the reps that led here are no longer anything to resume.
        if let Some(unfinished) = entry.get_mut("unfinished").and_then(|u| u.as_object_mut()) {
            unfinished.remove(model_id);
        }
    });
}

/// **A REJECTION STILL BEING CORROBORATED**: the best of the reps taken so far for `model_id`,
/// and how many, when a launch ended in the middle of them. `None` when there is nothing to
/// resume.
///
/// Why it is kept at all (Ray, test55): a measurement that a quit cut short used to be thrown
/// away whole, so the next launch measured from the beginning, and a test guest that is quit
/// within half a minute of starting measured on every launch. Each rep is now kept as it is
/// taken, so the reps of one rung add up across launches to the same at most
/// [`crate::stt`]'s three, and a quit costs only the rep it interrupted.
pub fn load_unfinished(key: &str, model_id: &str) -> Option<(f64, usize)> {
    let text = std::fs::read_to_string(speed_cache_path()).ok()?;
    let v = serde_json::from_str::<Value>(&text).ok()?;
    let u = v.get(key)?.get("unfinished")?.get(model_id)?;
    Some((u.get("best")?.as_f64()?, u.get("reps")?.as_u64()? as usize))
}

/// Keep an inconclusive rep: see [`load_unfinished`].
pub fn record_unfinished(key: &str, model_id: &str, best: f64, reps: usize) {
    update_entry(key, |entry| {
        let unfinished = entry.entry("unfinished").or_insert_with(|| serde_json::json!({}));
        if let Some(u) = unfinished.as_object_mut() {
            u.insert(model_id.to_string(), serde_json::json!({ "best": best, "reps": reps }));
        }
    });
}

/// Read the cache, change this key's entry, write it back.
///
/// THE WRITE IS A RENAME, so a reader never meets half a file and a quit in the middle of a
/// write leaves the previous cache rather than an unreadable one, which would cost the machine
/// every measurement it had.
fn update_entry(key: &str, edit: impl FnOnce(&mut serde_json::Map<String, Value>)) {
    let path = speed_cache_path();
    let mut root: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !root.is_object() {
        root = serde_json::json!({});
    }
    let entry = root.as_object_mut().unwrap().entry(key.to_string()).or_insert_with(|| {
        serde_json::json!({
            "_comment": "measured decode seconds for one 3.095 s utterance, MINIMUM ever observed on this machine for this whisper binary",
            "models": {}
        })
    });
    let Some(entry) = entry.as_object_mut() else { return };
    edit(entry);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    // Best effort by design. A machine whose config dir is unwritable re-measures every launch,
    // which is a cost; refusing to open voice mode over it would be a defect.
    let staged = path.with_extension(format!("json.{}", std::process::id()));
    if std::fs::write(&staged, serde_json::to_string_pretty(&root).unwrap_or_default()).is_ok()
        && std::fs::rename(&staged, &path).is_err()
    {
        std::fs::remove_file(&staged).ok();
    }
}

/// **ONE MEASUREMENT AT A TIME ON THIS MACHINE**, held from reading the cache to recording what
/// was measured; dropping it lets the next caller in.
///
/// THE DEFECT IT CLOSES (Ray, test55, nightly 54 and 55): a launch asks which model this machine
/// uses from several places at once (the window's readiness, the phone's voice desk, a lease's
/// video model), and every one of them found the cache empty and ran its own measurement, two
/// to five `whisper-cli` decodes side by side that held a test guest's CPU for over a minute.
/// With the turn, the first caller measures and every other one waits, then reads what it wrote.
///
/// A file lock beside the cache, so a second RichOS process on the machine waits too, plus a lock
/// in this process, so the threads of one launch still take turns on a machine whose config
/// folder cannot be written.
pub struct MeasuringTurn {
    _machine: Option<std::fs::File>,
    _process: std::sync::MutexGuard<'static, ()>,
}

pub fn measuring_turn() -> MeasuringTurn {
    static PROCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let process = PROCESS.lock().unwrap_or_else(|e| e.into_inner());
    let path = speed_cache_path().with_extension("lock");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let machine = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .ok()
        .filter(|f| f.lock().is_ok());
    MeasuringTurn { _machine: machine, _process: process }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine with room for anything, so a test about TIME is only about time.
    fn roomy() -> Machine {
        Machine {
            total_bytes: 25_769_803_776,
            available_bytes: 5_873_516_544,
            pressure: Pressure::Warn,
            cores: 10,
        }
    }

    /// INVARIANT: the registry the product compiles in is the registry these rules were written
    /// against. Every other test here would pass against a registry that had quietly lost its
    /// ladders, and would prove nothing.
    #[test]
    fn the_compiled_in_registry_carries_both_ladders_and_both_gates() {
        let c = Costs::load();
        assert_eq!(c.live_ladder, vec!["large-v3-turbo-q5_0", "small.en", "base.en", "tiny.en"]);
        assert_eq!(c.batch_ladder, vec!["large-v3-turbo-q5_0", "small.en"]);
        assert_eq!(c.live_ceiling_secs, 1.0, "the ceiling is stt.rs's stated failure point");
        assert_eq!(c.batch_real_time_multiple, 1.0, "the batch gate is the real-time boundary");
        for id in c.live_ladder.iter().chain(c.batch_ladder.iter()) {
            assert!(c.models.contains_key(id), "ladder names {id}, which has no measured cost");
        }
    }

    /// INVARIANT — THE FINDING THAT REDIRECTED THIS WHOLE DESIGN, pinned so it cannot be quietly
    /// edited back to the assumption it replaced.
    ///
    /// `large-v3-turbo-q5_0` costs LESS resident memory than `small.en`, so "small.en is what a
    /// low-RAM host gets" is measurably false. Anyone who changes these figures without re-running
    /// `docs/measurements/hardware-model-resolution-2026-09-10/tools/utterance-sweep.sh` will fail
    /// here, which is the intent: the numbers are measurements, not preferences.
    #[test]
    fn the_quantized_turbo_costs_less_memory_than_the_model_kept_for_weak_hosts() {
        let c = Costs::load();
        let q5 = c.models["large-v3-turbo-q5_0"].utterance_peak_rss;
        let small = c.models["small.en"].utterance_peak_rss;
        assert!(q5 < small, "q5_0 {q5} B is not below small.en {small} B — re-run the sweep before editing this");
        assert_eq!(small - q5, 8_945_664, "the measured gap, utterance-sweep.txt 2026-09-10");
    }

    /// INVARIANT: this is what replaced `assert_eq!(DEFAULT_MODEL_ID, "small.en")`.
    ///
    /// The old test pinned a STRING, so it passed on every machine in the world and would have gone
    /// on passing if the constant had been right for none of them. This pins the RULE: on a machine
    /// that decodes the top rung in time, the top rung is what gets used — whatever it is called.
    #[test]
    fn a_machine_fast_enough_for_the_top_rung_is_given_the_top_rung() {
        let c = Costs::load();
        let r = resolve_live(&c, &roomy(), |_| Some(0.400));
        assert_eq!(r.model_id, "large-v3-turbo-q5_0");
        assert_eq!(r.basis, Basis::TopRung);
        assert!(!r.is_noteworthy(), "the product working as designed is not a notification");
        assert_eq!(r.ceo_message(), None);
    }

    /// INVARIANT: the CEO's own M4, at the speeds actually measured on it, resolves to `small.en`
    /// — the same id the constant held, now for a reason read off the machine.
    ///
    /// The figures are `warm-reps.txt` under the LIGHTER of the two load conditions, i.e. the
    /// machine at its best. Even at its best the better rung is over the ceiling, which is what
    /// makes the verdict robust rather than marginal.
    #[test]
    fn the_ceos_own_m4_resolves_to_small_en_and_says_why() {
        let c = Costs::load();
        let r = resolve_live(&c, &roomy(), |id| match id {
            "large-v3-turbo-q5_0" => Some(1.302),
            "small.en" => Some(0.512),
            _ => Some(0.202),
        });
        assert_eq!(r.model_id, "small.en");
        assert_eq!(r.basis, Basis::TooSlow);
        assert_eq!(r.rejected, Some(("large-v3-turbo-q5_0".to_string(), 1.302)));
        assert!(r.is_noteworthy(), "a machine that could not have the better model is told so");
        let msg = r.ceo_message().expect("a demotion always has words for him");
        assert!(!msg.contains('/') && !msg.contains("small.en"), "no paths and no model filenames: {msg}");
        assert!(r.provenance().contains("too slow here at 1.302s/utt"), "{}", r.provenance());
    }

    /// INVARIANT: a slower machine keeps stepping down. This is the case the constant could never
    /// serve and the reason the whole module exists — an M1-class machine at ~2.4x this M4's cost
    /// cannot hold a conversation on `small.en` either, and today it is handed `small.en` anyway.
    #[test]
    fn a_machine_too_slow_for_small_en_steps_down_again_rather_than_stalling_the_conversation() {
        let c = Costs::load();
        let r = resolve_live(&c, &roomy(), |id| match id {
            "large-v3-turbo-q5_0" => Some(3.100),
            "small.en" => Some(1.240),
            "base.en" => Some(0.590),
            _ => Some(0.480),
        });
        assert_eq!(r.model_id, "base.en");
        assert_eq!(r.basis, Basis::TooSlow);
        assert!(r.is_noteworthy());
    }

    /// INVARIANT: the bottom rung is never refused. A machine that clears no gate still gets a
    /// recognizer, because "too slow to hear you at all" is not an outcome this product may have.
    #[test]
    fn the_bottom_rung_is_taken_even_when_it_misses_the_ceiling_too() {
        let c = Costs::load();
        let r = resolve_live(&c, &roomy(), |_| Some(9.000));
        assert_eq!(r.model_id, "tiny.en", "the last rung always fits");
        assert!(r.is_noteworthy(), "and the CEO is told he is on it");
    }

    /// INVARIANT: the memory gate can force a demotion, and it says memory rather than speed when
    /// it does. Forced through the machine struct, which is the seam that makes a low-memory
    /// condition demonstrable on a machine that has plenty.
    #[test]
    fn a_machine_without_the_memory_demotes_on_memory_and_says_memory() {
        let c = Costs::load();
        let squeezed = Machine {
            total_bytes: 8_589_934_592,
            available_bytes: 500_000_000,
            pressure: Pressure::Critical,
            cores: 8,
        };
        // Fast enough for anything; only memory is in play.
        let r = resolve_live(&c, &squeezed, |_| Some(0.300));
        assert_eq!(r.model_id, "base.en", "q5_0 and small.en both exceed 500,000,000 B available");
        assert_eq!(r.basis, Basis::TooLarge);
        let msg = r.ceo_message().expect("a memory demotion has words for him too");
        assert!(msg.contains("memory"), "he is told which constraint it was: {msg}");
        assert!(r.provenance().contains("does not fit 500000000 B available"), "{}", r.provenance());
    }

    /// INVARIANT: pressure is READ and never ACTED ON.
    ///
    /// The whole reason: `kern.memorystatus_vm_pressure_level` reads WARN on the CEO's machine in
    /// all six samples during ordinary work, so a resolver that demoted on it would demote him
    /// permanently. Two machines identical but for pressure must resolve identically.
    #[test]
    fn memory_pressure_is_recorded_and_changes_no_decision() {
        let c = Costs::load();
        let mut critical = roomy();
        critical.pressure = Pressure::Critical;
        let calm = resolve_live(&c, &roomy(), |_| Some(0.400));
        let squeezed = resolve_live(&c, &critical, |_| Some(0.400));
        assert_eq!(calm.model_id, squeezed.model_id, "pressure must not move the outcome");
        assert_eq!(calm.basis, squeezed.basis);
        assert!(squeezed.provenance().contains("pressure:critical"), "but it IS recorded: {}", squeezed.provenance());
    }

    /// INVARIANT: nothing measurable still yields a working recognizer, on the SAFE rung rather
    /// than the bottom one, and the honesty is in the basis rather than in a silent fallback that
    /// looks like a decision.
    ///
    /// The distinction is the point: `tiny.en` is where a machine goes once it has been measured
    /// and found slow. Giving it to a machine nobody managed to time would be a downgrade
    /// justified by an absence of evidence, which is the shape of the defect this module removes.
    #[test]
    fn a_machine_that_cannot_be_measured_takes_the_safe_rung_and_admits_it() {
        let c = Costs::load();
        let r = resolve_live(&c, &roomy(), |_| None);
        assert_eq!(r.model_id, "small.en", "the safe rung — the one with field history");
        assert_ne!(r.model_id, *c.live_ladder.last().unwrap(), "NOT the bottom rung");
        assert_eq!(r.basis, Basis::Unmeasured);
        assert_eq!(r.measured_secs, None);
        assert!(r.is_noteworthy(), "an unmeasured machine is exactly when to say so");
        assert!(r.provenance().contains("hw-unmeasured"), "{}", r.provenance());
    }

    /// INVARIANT: a rung whose weights are not installed is not a rung.
    ///
    /// Without the filter the ladder is fiction — a machine carrying only `small.en` would be
    /// "promoted" to a `q5_0` that is not there, and `resolve_model` would then fail at the
    /// toggle with a missing-model message for a model the CEO never asked for.
    #[test]
    fn a_rung_whose_weights_are_absent_drops_out_of_the_ladder_entirely() {
        let mut c = Costs::load();
        c.retain_installed(|id| id == "small.en");
        assert_eq!(c.live_ladder, vec!["small.en"]);
        assert_eq!(c.batch_ladder, vec!["small.en"]);
        // Fast enough for anything: with only one rung installed, that rung is the answer, and it
        // is the TOP of what this machine has rather than a demotion it should be told about.
        let r = resolve_live(&c, &roomy(), |_| Some(0.300));
        assert_eq!(r.model_id, "small.en");
        assert_eq!(r.basis, Basis::TopRung);
        assert!(!r.is_noteworthy(), "using the only model installed is not a degradation");
    }

    /// INVARIANT: the batch surface is NEVER promoted above the CEO decision page §10 ruling, no
    /// matter how much machine is available. §10 decided on WER, fabrication and download size,
    /// none of which this module measured; promoting on hardware would re-decide it by the back
    /// door.
    #[test]
    fn a_huge_machine_is_not_promoted_above_the_ruling_on_the_batch_path() {
        let c = Costs::load();
        let huge = Machine {
            total_bytes: 137_438_953_472,
            available_bytes: 100_000_000_000,
            pressure: Pressure::Normal,
            cores: 24,
        };
        let r = resolve_batch(&c, &huge, 0.5);
        assert_eq!(r.model_id, "large-v3-turbo-q5_0", "the ruling is the ceiling");
        assert_eq!(r.basis, Basis::TopRung);
        assert!(!c.batch_ladder.contains(&"large-v3-turbo".to_string()), "full turbo is in no ladder");
    }

    /// INVARIANT: the batch surface DOES demote, which it has never done — `config.js` carries a
    /// `low-resource` tier that nothing in the product has ever selected.
    ///
    /// The trigger is the real-time boundary: 0.0473 s of compute per second of audio on the
    /// reference machine, so a machine ~21x slower stops keeping up with the recording.
    #[test]
    fn a_machine_that_cannot_keep_up_with_the_recording_falls_to_the_low_resource_tier() {
        let c = Costs::load();
        let r = resolve_batch(&c, &roomy(), 25.0);
        assert_eq!(r.model_id, "small.en");
        assert_eq!(r.basis, Basis::TooSlow);
        assert!(r.is_noteworthy());
        // And the reference machine itself is nowhere near it — a guard, not a discriminator.
        let here = resolve_batch(&c, &roomy(), 1.0);
        assert_eq!(here.model_id, "large-v3-turbo-q5_0");
        assert_eq!(here.basis, Basis::TopRung);
    }

    /// INVARIANT: the headroom the batch gate has on the reference machine, recomputed here from
    /// the registry rather than quoted, because a claim of "21x clear" that nobody recomputes is
    /// how a stale margin survives a model change.
    #[test]
    fn the_batch_gate_has_the_headroom_the_record_claims_on_the_reference_machine() {
        let c = Costs::load();
        let rate = c.models["large-v3-turbo-q5_0"].long_form_rate.expect("the shipping batch model has a rate");
        let headroom = c.batch_real_time_multiple / rate;
        assert!((21.0..=22.0).contains(&headroom), "expected ~21x real time, got {headroom:.1}x");
    }

    /// INVARIANT: the speed factor is a ratio against the reference host and 1.0 means "as fast".
    #[test]
    fn the_speed_factor_is_one_when_this_machine_matches_the_reference_host() {
        let c = Costs::load();
        let same = speed_factor(&c, "small.en", 0.512).unwrap();
        assert!((same - 1.0).abs() < 1e-9, "0.512 s against a 0.512 s reference is 1.0, got {same}");
        let half = speed_factor(&c, "small.en", 1.024).unwrap();
        assert!((half - 2.0).abs() < 1e-9, "twice the time is a factor of 2, got {half}");
        assert_eq!(speed_factor(&c, "not-a-model", 1.0), None);
    }

    /// INVARIANT: the cache keeps the MINIMUM, because the same machine measured ~50% slower while
    /// busy and a sample taken under load must not be able to permanently under-rate it.
    /// `RICHOS_WHISPER_SPEED_CACHE` is process-wide and libtest runs this crate's tests side by
    /// side: any test that sets it holds this for as long as its value is in place (audit
    /// `docs/verification/2026-09-29-load-sensitive-checks-audit.md`, `env-mutation-unguarded`).
    static SPEED_CACHE_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn the_speed_cache_keeps_the_best_measurement_not_the_latest() {
        let _env = SPEED_CACHE_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("richos-speed-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("speed.json");
        std::env::set_var("RICHOS_WHISPER_SPEED_CACHE", &path);
        let key = cache_key("abcdef0123456789", &roomy());

        record_speed(&key, "small.en", 0.800); // measured while busy
        assert_eq!(load_speeds(&key).get("small.en").copied(), Some(0.800));
        record_speed(&key, "small.en", 0.512); // measured while quiet
        assert_eq!(load_speeds(&key).get("small.en").copied(), Some(0.512));
        record_speed(&key, "small.en", 0.900); // busy again — must not undo the good one
        assert_eq!(load_speeds(&key).get("small.en").copied(), Some(0.512));

        // A different binary is a different key: a brew upgrade can change decode cost, and a
        // speed attributed to the wrong build is the hole toolchain.rs exists to close.
        assert!(load_speeds(&cache_key("ffffffffffffffff", &roomy())).is_empty());

        std::env::remove_var("RICHOS_WHISPER_SPEED_CACHE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// INVARIANT: whatever the resolver decides, the line a transcript carries names the model,
    /// the reason, the memory it saw and the pressure it ignored. "Which weights heard this?" was
    /// already answerable; "and why those?" is what this adds.
    #[test]
    fn every_resolution_produces_a_provenance_line_that_answers_why_these_weights() {
        let c = Costs::load();
        for probe_secs in [0.300_f64, 1.302, 9.000] {
            let r = resolve_live(&c, &roomy(), |_| Some(probe_secs));
            let p = r.provenance();
            assert!(p.contains(&format!("model:{}", r.model_id)), "{p}");
            assert!(p.contains("mem:") && p.contains("pressure:"), "{p}");
        }
    }

    /// INVARIANT: the machine reads without panicking and reports something usable on the host the
    /// tests are running on. Asserts the CONTRACT, not this machine's numbers, so it stays true on
    /// CI and on a laptop — the same discipline `resolution_returns_a_result_and_never_panics` uses.
    #[test]
    fn reading_the_host_yields_a_usable_machine_and_never_panics() {
        let m = Machine::read();
        assert!(m.total_bytes > 0, "hw.memsize must be readable on macOS");
        assert!(m.cores >= 1);
        assert!(m.budget_bytes() > 0, "the budget must never be zero — a zero refuses every model");
    }
}
