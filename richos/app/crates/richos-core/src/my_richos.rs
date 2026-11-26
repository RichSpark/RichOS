//! **My RichOS**, the activity monitor under My Mac on the navigation rail (design round 109; the
//! CEO, 2026-11-26: *"I need a way to track or see at a glance 1) whether or not tokens are being
//! consumed now 2) the velocity with which they've been being consumed in the last say 1 minute
//! and last 5 minutes and last 15 minutes 3) see more detail on those stats if desired 4) also
//! see how much memory/RAM is being used now by each app of the RichOS suite individuall and all
//! combined and what was the average RAM usage in the last 1-2 minutes or so"*; on the build:
//! *"I really hope that those graphs can be drawn without using much CPU/RAM resources"* and
//! *"we previously had some work done for logging memory usage across our apps. I hope we can use
//! or re-use as much of that as possible"*).
//!
//! - **Tokens come from the turns the app ran, nothing else.** Every lease's reader already hands
//!   each `assistant`, `stream_event` and `result` frame to its token tally
//!   (`quota::tokens::Tally`, the same counts the quota service and the spend ledger take); while
//!   this is on, the reader also gives the tally's answer here ([`record`]), with the chat and
//!   the seat (Rich talking, or the team) the lease belongs to. A message counted in several
//!   copies adds only what each copy adds. No model call, no polling of any usage endpoint.
//! - **Memory is RichDoctor's per-app reading** (`suite_memory::Live`, the capture its minute
//!   recorder makes), taken every [`MEMORY_EVERY_MS`] by one thread that exists only while this is
//!   on. RichDoctor keeps one row a minute for its report; this keeps the last hour of two-second
//!   readings in memory for the 2-minute figures and graphs, and writes nothing.
//! - **Off is off.** The switch is off by default. While it is off no frame is counted (one atomic
//!   load per frame decides that), no sampler thread exists and nothing is held: turning it off
//!   drops what was kept, so the history starts when it is turned on.
use crate::quota::tokens::Use;
use crate::spend::{Buckets, Family};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

/// "Being used now": a request landed in the last ten seconds. Requests land whole, seconds apart,
/// so a shorter look would flicker between yes and no during ordinary work (round 109's README).
pub const ACTIVE_MS: u64 = 10_000;
/// How often each suite app's memory is read while the monitor is on.
pub const MEMORY_EVERY_MS: u64 = 2_000;
/// Every request of the last 15 minutes is kept as it landed: the speeds and the 15-minute graphs.
const RECENT_MS: u64 = 15 * 60_000;
/// Older use is kept per minute, per chat, seat and model, for a day: the 1-hour and 24-hour views.
const DAY_MS: u64 = 24 * 60 * 60_000;
const MINUTE_MS: u64 = 60_000;
/// A message can grow for as long as its lease streams it; ids older than this are forgotten.
const SEEN_MS: u64 = 60 * 60_000;
/// One hour of memory readings at one every two seconds.
const MEMORY_KEEP_MS: u64 = 60 * 60_000;
/// The 2-minute figures (average, highest, the small graphs).
const MEMORY_WINDOW_MS: u64 = 2 * 60_000;

static ON: AtomicBool = AtomicBool::new(false);

/// Whether the monitor is on. A lease's reader asks this once per frame before doing anything.
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

// ---- the lease's side ------------------------------------------------------------------------

/// Which chat and seat a lease's tokens are spent for. Built once when the lease starts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Source {
    pub entity: Option<String>,
    pub thread: Option<String>,
    /// The team's session (Stu and his workers), not Rich talking with the user.
    pub team: bool,
}

/// Count what one frame's tally found (`quota::tokens::Tally::frame`), for `source`: each use's
/// `grew`, what that copy added (so turning this on mid-message counts only what arrives after).
/// `worker` is the frame's `parent_tool_use_id`: a helper's frames carry their own, the lead's
/// none. A `result` frame (`from_result`) names no worker: what it adds to a message stays with the
/// worker that message came from, and a remainder no message owns is nobody's (it counts as
/// tokens, never as one more worker).
pub fn record(source: &Source, worker: Option<&str>, from_result: bool, uses: &[Use]) {
    if uses.is_empty() || !on() {
        return;
    }
    let now = crate::util::now_millis();
    lock(&shared().tokens).record(source, worker, from_result, uses, now);
}

/// The suite apps that run inside RichOS's own processes, by pid, as the host last found them
/// (`suite_memory::Hosted`): read with the rest every two seconds while this is on.
pub fn set_hosted(hosted: Vec<crate::suite_memory::Hosted>) {
    if on() {
        *lock(&shared().hosted) = hosted;
    }
}

// ---- turning it on and off -------------------------------------------------------------------

struct Shared {
    tokens: Mutex<Tokens>,
    memory: Mutex<Memory>,
    hosted: Mutex<Vec<crate::suite_memory::Hosted>>,
    /// The sampler's generation: a thread whose generation is no longer this one ends.
    sampler: Mutex<u64>,
    wake: Condvar,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared {
        tokens: Mutex::new(Tokens::default()),
        memory: Mutex::new(Memory::default()),
        hosted: Mutex::new(Vec::new()),
        sampler: Mutex::new(0),
        wake: Condvar::new(),
    })
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Turn the monitor on or off. On starts counting from now and starts the memory sampler for
/// `state_root` (the engine state, where RichScribe publishes its WebKit services); off stops the
/// sampler, which ends within one wake, and drops everything kept. Idempotent.
pub fn set_on(next: bool, state_root: &Path) {
    let s = shared();
    let mut generation = lock(&s.sampler);
    let was = ON.swap(next, Ordering::SeqCst);
    if next == was {
        return;
    }
    *generation += 1;
    let now = crate::util::now_millis();
    *lock(&s.tokens) = Tokens { since: now, ..Tokens::default() };
    *lock(&s.memory) = Memory::default();
    lock(&s.hosted).clear();
    s.wake.notify_all();
    if next {
        start_sampler(*generation, state_root.to_path_buf());
    }
}

/// Memory reader threads alive now: one while the monitor is on, none while it is off.
static SAMPLERS: AtomicUsize = AtomicUsize::new(0);

struct Alive;
impl Drop for Alive {
    fn drop(&mut self) {
        SAMPLERS.fetch_sub(1, Ordering::SeqCst);
    }
}

fn start_sampler(generation: u64, state_root: PathBuf) {
    SAMPLERS.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new().name("my-richos-memory".into()).spawn(move || {
        let _alive = Alive;
        let s = shared();
        let mut live = crate::suite_memory::Live::new(&state_root);
        let total = crate::system_resources::memory_total_bytes();
        loop {
            let now = crate::util::now_millis();
            let hosted = lock(&s.hosted).clone();
            let reading = live.sample(now as i64, &hosted);
            {
                // Published only by the generation that took it: a reading taken just before the
                // switch went off must not reappear in a fresh history.
                let current = lock(&s.sampler);
                if *current != generation {
                    return;
                }
                lock(&s.memory).push(now, reading, total);
            }
            let current = lock(&s.sampler);
            let (current, _) = s
                .wake
                .wait_timeout_while(current, Duration::from_millis(MEMORY_EVERY_MS), |g| *g == generation)
                .unwrap_or_else(|poison| poison.into_inner());
            if *current != generation {
                return;
            }
        }
    });
    if let Err(error) = spawned {
        SAMPLERS.fetch_sub(1, Ordering::SeqCst);
        eprintln!("[richos] My RichOS could not start its memory reader: {error}");
    }
}

// ---- what the window reads -------------------------------------------------------------------

/// What the open details ask for, besides the rail and the card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Detail {
    /// The token graph: this many seconds in `token_bars` bars (15 minutes, an hour or a day).
    pub token_span_s: Option<u64>,
    pub token_bars: Option<u64>,
    /// The memory graph: this many seconds in `memory_points` points.
    pub memory_span_s: Option<u64>,
    pub memory_points: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct View {
    pub on: bool,
    pub now_ms: u64,
    pub tokens: Option<TokensView>,
    pub memory: Option<MemoryView>,
}

/// Everything the window shows, read once (the window asks once a second while the monitor is on
/// and the window is visible): the rail's figures always, the card's graph and the memory only
/// while the card (`card`) or the details (`detail`) are open. Off answers `on: false` and
/// nothing else.
pub fn view(card: bool, detail: Option<Detail>) -> View {
    let now = crate::util::now_millis();
    if !on() {
        return View { on: false, now_ms: now, tokens: None, memory: None };
    }
    let s = shared();
    let tokens = lock(&s.tokens).view(now, card, detail.as_ref());
    let memory = (card || detail.is_some()).then(|| lock(&s.memory).view(now, detail.as_ref()));
    View { on: true, now_ms: now, tokens: Some(tokens), memory }
}

// ---- tokens ------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    source: Source,
    model: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    at: u64,
    key: u32,
    worker: u64,
    buckets: Buckets,
}

#[derive(Clone, Copy, Debug)]
struct MinuteRow {
    minute: u64,
    key: u32,
    buckets: Buckets,
}

#[derive(Default)]
struct Tokens {
    /// When counting began: the setting was turned on (or the app started with it on).
    since: u64,
    keys: Vec<Key>,
    index: HashMap<Key, u32>,
    recent: VecDeque<Event>,
    minutes: VecDeque<MinuteRow>,
    /// Each message counted so far, by id: when it was last seen and the worker it came from.
    workers: HashMap<String, (u64, u64)>,
    workers_pruned: u64,
    last: Option<u64>,
}

/// Tokens a `result` frame added that no message owns: counted, never one more worker.
const NOBODY: u64 = u64::MAX;

/// The model's family as the screen says it.
fn family_word(model: &str) -> &'static str {
    match crate::spend::family(model).map(|(family, _)| family) {
        Some(Family::Fable) => "Fable",
        Some(Family::Opus) => "Opus",
        Some(Family::Sonnet) => "Sonnet",
        Some(Family::Haiku) => "Haiku",
        None => "Other",
    }
}
const FAMILIES: [&str; 5] = ["Fable", "Opus", "Sonnet", "Haiku", "Other"];

/// The lead is 0; a helper is a hash of its `parent_tool_use_id` (never 0, never [`NOBODY`]).
fn worker_id(worker: Option<&str>) -> u64 {
    worker.map_or(0, |w| {
        let mut h = DefaultHasher::new();
        w.hash(&mut h);
        (h.finish() >> 1) | 1
    })
}

impl Tokens {
    fn key(&mut self, source: &Source, model: &str) -> u32 {
        let key = Key { source: source.clone(), model: family_word(model) };
        if let Some(&at) = self.index.get(&key) {
            return at;
        }
        let at = self.keys.len() as u32;
        self.keys.push(key.clone());
        self.index.insert(key, at);
        at
    }

    fn record(&mut self, source: &Source, frame_worker: Option<&str>, from_result: bool, uses: &[Use], now: u64) {
        for found in uses {
            // What this copy added, as the tally measured it against everything it had counted,
            // also before this was turned on.
            let grew = found.grew;
            if grew.total() == 0 {
                continue;
            }
            let worker = if from_result {
                self.workers.get(&found.id).map_or(NOBODY, |(_, w)| *w)
            } else {
                worker_id(frame_worker)
            };
            self.workers.insert(found.id.clone(), (now, worker));
            let key = self.key(source, &found.request.model);
            self.recent.push_back(Event { at: now, key, worker, buckets: grew });
            let minute = now / MINUTE_MS;
            match self.minutes.iter_mut().rev().take_while(|row| row.minute == minute).find(|row| row.key == key) {
                Some(row) => row.buckets.add(&grew),
                None => self.minutes.push_back(MinuteRow { minute, key, buckets: grew }),
            }
            self.last = Some(self.last.map_or(now, |last| last.max(now)));
        }
        self.prune(now);
    }

    fn prune(&mut self, now: u64) {
        while self.recent.front().is_some_and(|e| now.saturating_sub(e.at) >= RECENT_MS) {
            self.recent.pop_front();
        }
        while self.minutes.front().is_some_and(|r| (r.minute + 1) * MINUTE_MS + DAY_MS <= now) {
            self.minutes.pop_front();
        }
        if now.saturating_sub(self.workers_pruned) >= MINUTE_MS {
            self.workers_pruned = now;
            self.workers.retain(|_, (at, _)| now.saturating_sub(*at) < SEEN_MS);
        }
    }

    /// The rail's ten bars: the last minute exactly, in six-second slices counted back from now,
    /// oldest first, so they always add up to the speed under them (the second review, at
    /// b7f443a6c: clock-aligned slices left part of the minute out of the bars).
    fn last_minute(&self, now: u64) -> Vec<u64> {
        let mut bars = vec![0u64; 10];
        for e in self.recent.iter().rev().take_while(|e| now.saturating_sub(e.at) < 60_000) {
            let back = now.saturating_sub(e.at) / 6_000;
            bars[9 - back.min(9) as usize] += e.buckets.total();
        }
        bars
    }

    /// Tokens that landed in the last `span_ms` (at most 15 minutes).
    fn used(&self, now: u64, span_ms: u64) -> u64 {
        self.recent.iter().rev().take_while(|e| now.saturating_sub(e.at) < span_ms).map(|e| e.buckets.total()).sum()
    }

    /// Bars covering the last `span_s` seconds, oldest first, each the tokens in its slice of
    /// `span_s / n` seconds. The slices are aligned to the clock, so a bar never changes once its
    /// slice is over and the last one holds the current second; so the stretch is `n` bars, or
    /// `n + 1` when it starts inside a slice: the oldest, partial one is kept, and the bars add up
    /// to exactly what [`Tokens::sources`] counts over the same stretch. Up to 15 minutes from
    /// each request as it landed; longer from the per-minute rows (every longer view's slice is
    /// whole minutes).
    fn series(&self, now: u64, span_s: u64, n: u64) -> Series {
        let n = n.clamp(1, 400);
        let w = (span_s / n).max(1);
        let span_ms = span_s * 1000;
        let edge = (now / 1000 / w + 1) * w;
        let first = (now.saturating_sub(span_ms) / 1000 / w) * w;
        let mut bars = vec![0u64; ((edge - first) / w) as usize];
        let mut put = |sec: u64, tokens: u64| {
            if sec >= first && sec < edge {
                bars[((sec - first) / w) as usize] += tokens;
            }
        };
        if span_ms <= RECENT_MS {
            for e in self.recent.iter().filter(|e| now.saturating_sub(e.at) < span_ms) {
                put(e.at / 1000, e.buckets.total());
            }
        } else {
            let from = now.saturating_sub(span_ms) / MINUTE_MS;
            for r in self.minutes.iter().filter(|r| r.minute >= from) {
                put(r.minute * 60, r.buckets.total());
            }
        }
        Series { bars, bar_ms: w * 1000, ends_at_ms: edge * 1000 }
    }

    /// Each chat and seat's tokens over the last `span_s`, its speed over the last minute, the
    /// helpers working in it in that minute, and the model it spent most on.
    fn sources(&self, now: u64, span_s: u64) -> (Vec<SourceRow>, Vec<Split>, Vec<Split>) {
        #[derive(Default)]
        struct Acc {
            tokens: u64,
            per_min: u64,
            workers: HashSet<u64>,
            by_model: HashMap<&'static str, u64>,
            minute_by_model: HashMap<&'static str, u64>,
        }
        let mut chats: HashMap<&Source, Acc> = HashMap::new();
        let mut models: HashMap<&'static str, u64> = HashMap::new();
        let mut kinds = Buckets::default();
        let span_ms = span_s * 1000;
        let mut take = |key: u32, buckets: &Buckets| {
            let k = &self.keys[key as usize];
            let acc = chats.entry(&k.source).or_default();
            acc.tokens += buckets.total();
            *acc.by_model.entry(k.model).or_default() += buckets.total();
            *models.entry(k.model).or_default() += buckets.total();
            kinds.add(buckets);
        };
        if span_ms <= RECENT_MS {
            for e in self.recent.iter().filter(|e| now.saturating_sub(e.at) < span_ms) {
                take(e.key, &e.buckets);
            }
        } else {
            let from = now.saturating_sub(span_ms) / MINUTE_MS;
            for r in self.minutes.iter().filter(|r| r.minute >= from) {
                take(r.key, &r.buckets);
            }
        }
        for e in self.recent.iter().filter(|e| now.saturating_sub(e.at) < MINUTE_MS) {
            let k = &self.keys[e.key as usize];
            let acc = chats.entry(&k.source).or_default();
            acc.per_min += e.buckets.total();
            if e.worker != NOBODY {
                acc.workers.insert(e.worker);
            }
            *acc.minute_by_model.entry(k.model).or_default() += e.buckets.total();
        }
        let top = |m: &HashMap<&'static str, u64>| m.iter().max_by_key(|(name, n)| (**n, std::cmp::Reverse(**name))).map(|(name, _)| *name);
        let mut rows: Vec<SourceRow> = chats
            .into_iter()
            .filter(|(_, acc)| acc.tokens > 0 || acc.per_min > 0)
            .map(|(source, acc)| SourceRow {
                entity: source.entity.clone(),
                thread: source.thread.clone(),
                team: source.team,
                working: if acc.per_min > 0 { acc.workers.len() as u64 } else { 0 },
                model: top(&acc.minute_by_model).or_else(|| top(&acc.by_model)).unwrap_or("Other"),
                tokens: acc.tokens,
                per_min: acc.per_min,
            })
            .collect();
        rows.sort_by(|a, b| b.per_min.cmp(&a.per_min).then(b.tokens.cmp(&a.tokens)).then(a.thread.cmp(&b.thread)).then(a.team.cmp(&b.team)));
        let models = FAMILIES
            .iter()
            .filter_map(|name| models.get(name).filter(|n| **n > 0).map(|n| Split { name, tokens: *n }))
            .collect();
        let kinds = [
            ("Read from cache", kinds.cache_read),
            ("Saved to cache", kinds.write_5m + kinds.write_1h),
            ("New input", kinds.input),
            ("Written output", kinds.output),
        ]
        .into_iter()
        .map(|(name, tokens)| Split { name, tokens })
        .collect();
        (rows, models, kinds)
    }

    fn view(&self, now: u64, card: bool, detail: Option<&Detail>) -> TokensView {
        let last = self.last;
        let detail = detail.and_then(|d| Some((d.token_span_s?, d.token_bars?))).map(|(span, bars)| {
            let span = span.clamp(60, DAY_MS / 1000);
            let series = self.series(now, span, bars);
            let (sources, models, kinds) = self.sources(now, span);
            TokensDetail { span_s: span, series, sources, models, kinds }
        });
        let tile = self.last_minute(now);
        TokensView {
            since_ms: self.since,
            last_ms: last,
            active: last.is_some_and(|at| now.saturating_sub(at) < ACTIVE_MS),
            m1: self.used(now, 60_000),
            m5: self.used(now, 300_000),
            m15: self.used(now, 900_000),
            scale: self.series(now, 900, 150).bars.into_iter().chain(tile.iter().copied()).max().unwrap_or(0),
            tile,
            card: if card { self.series(now, 900, 60).bars } else { Vec::new() },
            detail,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Series {
    pub bars: Vec<u64>,
    pub bar_ms: u64,
    pub ends_at_ms: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SourceRow {
    pub entity: Option<String>,
    pub thread: Option<String>,
    pub team: bool,
    /// Rich's lead and helpers that used tokens in the last minute (0 when the chat is idle).
    pub working: u64,
    pub model: &'static str,
    pub tokens: u64,
    pub per_min: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Split {
    pub name: &'static str,
    pub tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct TokensDetail {
    pub span_s: u64,
    pub series: Series,
    pub sources: Vec<SourceRow>,
    pub models: Vec<Split>,
    pub kinds: Vec<Split>,
}

#[derive(Debug, Serialize)]
pub struct TokensView {
    pub since_ms: u64,
    pub last_ms: Option<u64>,
    /// A request landed in the last [`ACTIVE_MS`].
    pub active: bool,
    /// Tokens in the last minute, 5 minutes and 15 minutes.
    pub m1: u64,
    pub m5: u64,
    pub m15: u64,
    /// The rail: the last minute in ten six-second bars counted back from now.
    pub tile: Vec<u64>,
    /// The rail's scale: the tallest six-second bar of the last 15 minutes.
    pub scale: u64,
    /// The card: the last 15 minutes in sixty 15-second bars (empty while the card is closed).
    pub card: Vec<u64>,
    pub detail: Option<TokensDetail>,
}

// ---- memory ------------------------------------------------------------------------------------

const APPS: usize = crate::suite_memory::LIVE_APPS;

#[derive(Clone, Copy)]
struct Reading {
    at: u64,
    /// Bytes per app; -1 where it could not be read.
    bytes: [i64; APPS],
    processes: [u16; APPS],
}

#[derive(Default)]
struct Memory {
    readings: VecDeque<Reading>,
    total: Option<u64>,
    failure: Option<String>,
}

impl Memory {
    fn push(&mut self, now: u64, reading: Result<Vec<crate::suite_memory::LiveApp>, String>, total: Option<u64>) {
        self.total = total;
        match reading {
            Ok(apps) => {
                self.failure = None;
                let mut r = Reading { at: now, bytes: [-1; APPS], processes: [0; APPS] };
                for (i, app) in apps.iter().take(APPS).enumerate() {
                    r.bytes[i] = app.bytes.unwrap_or(-1).max(-1);
                    r.processes[i] = app.processes.clamp(0, u16::MAX as i64) as u16;
                }
                self.readings.push_back(r);
            }
            Err(error) => self.failure = Some(error),
        }
        while self.readings.front().is_some_and(|r| now.saturating_sub(r.at) >= MEMORY_KEEP_MS) {
            self.readings.pop_front();
        }
    }

    /// Every suite app together: unknown when an open app could not be read.
    fn all(r: &Reading) -> Option<i64> {
        let mut sum = 0i64;
        for i in 0..APPS {
            if r.processes[i] > 0 {
                if r.bytes[i] < 0 {
                    return None;
                }
                sum += r.bytes[i];
            }
        }
        Some(sum)
    }

    fn line(&self, now: u64, value: impl Fn(&Reading) -> Option<i64>) -> AppLine {
        let hist: Vec<Option<i64>> = self.readings.iter().filter(|r| now.saturating_sub(r.at) < MEMORY_WINDOW_MS).map(&value).collect();
        let known: Vec<i64> = hist.iter().flatten().copied().collect();
        AppLine {
            now: self.readings.back().and_then(&value),
            avg: (!known.is_empty()).then(|| known.iter().sum::<i64>() / known.len() as i64),
            peak: known.iter().copied().max(),
            hist,
        }
    }

    /// `points` averages of every app together, over the last `span_s`, oldest first. A stretch
    /// with no reading is `None`, never 0.
    fn series(&self, now: u64, span_s: u64, points: u64) -> Vec<Option<i64>> {
        let points = points.clamp(2, 400);
        let span = span_s.clamp(10, MEMORY_KEEP_MS / 1000) * 1000;
        let step = span as f64 / points as f64;
        let start = now.saturating_sub(span);
        let mut sums = vec![(0i64, 0i64); points as usize];
        for r in self.readings.iter().filter(|r| r.at > start) {
            if let Some(v) = Self::all(r) {
                let i = (((r.at - start) as f64 / step) as usize).min(points as usize - 1);
                sums[i].0 += v;
                sums[i].1 += 1;
            }
        }
        sums.into_iter().map(|(s, n)| (n > 0).then(|| s / n)).collect()
    }

    fn view(&self, now: u64, detail: Option<&Detail>) -> MemoryView {
        let latest = self.readings.back();
        let status = match (&self.failure, latest) {
            (Some(_), _) => "unavailable",
            (None, None) => "measuring",
            (None, Some(_)) => "available",
        };
        let apps = crate::suite_memory::apps()
            .iter()
            .enumerate()
            .map(|(i, (id, name))| {
                let line = self.line(now, |r| (r.bytes[i] >= 0).then_some(r.bytes[i]));
                AppView { id, name, processes: latest.map_or(0, |r| r.processes[i] as i64), within: crate::suite_memory::within(id), line }
            })
            .collect();
        MemoryView {
            status,
            detail: self.failure.clone().unwrap_or_default(),
            at_ms: latest.map(|r| r.at),
            mac_total_bytes: self.total,
            apps,
            all: self.line(now, Self::all),
            series: detail.and_then(|d| Some((d.memory_span_s?, d.memory_points?))).map(|(span, points)| MemorySeries {
                span_s: span,
                list: self.series(now, span, points),
            }),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AppLine {
    pub now: Option<i64>,
    /// The average and highest of the last two minutes.
    pub avg: Option<i64>,
    pub peak: Option<i64>,
    /// The last two minutes' readings, oldest first.
    pub hist: Vec<Option<i64>>,
}

#[derive(Debug, Serialize)]
pub struct AppView {
    pub id: &'static str,
    pub name: &'static str,
    /// Its processes now: 0 is not open.
    pub processes: i64,
    /// What holds this one's memory when it has no process of its own: `richos` or `browser`.
    pub within: Option<&'static str>,
    #[serde(flatten)]
    pub line: AppLine,
}

#[derive(Debug, Serialize)]
pub struct MemorySeries {
    pub span_s: u64,
    pub list: Vec<Option<i64>>,
}

#[derive(Debug, Serialize)]
pub struct MemoryView {
    /// `measuring` before the first reading, `unavailable` when the last one failed.
    pub status: &'static str,
    pub detail: String,
    pub at_ms: Option<u64>,
    pub mac_total_bytes: Option<u64>,
    pub apps: Vec<AppView>,
    pub all: AppLine,
    pub series: Option<MemorySeries>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spend::Request;

    fn use_of(id: &str, model: &str, input: u64, output: u64, cache_read: u64) -> Use {
        let buckets = Buckets { input, output, cache_read, write_5m: 0, write_1h: 0 };
        Use {
            id: id.into(),
            at: 0,
            model: model.into(),
            tokens: buckets.total(),
            output,
            request: Request { id: id.into(), model: model.into(), buckets, ..Request::default() },
            late: 0,
            grew: buckets,
        }
    }

    fn chat(thread: &str, team: bool) -> Source {
        Source { entity: Some("acme".into()), thread: Some(thread.into()), team }
    }

    const T0: u64 = 1_794_996_000_000; // a whole quarter hour

    // ---- frames as a lease streams them, through the lease's own tally ----------------------
    fn assistant(id: &str, model: &str, input: u64, output: u64, cache_read: u64, helper: Option<&str>) -> serde_json::Value {
        serde_json::json!({"type": "assistant", "parent_tool_use_id": helper,
            "message": {"id": id, "model": model, "usage": {"input_tokens": input, "output_tokens": output, "cache_read_input_tokens": cache_read}}})
    }
    fn result(model: &str, input: u64, output: u64, cache_read: u64) -> serde_json::Value {
        serde_json::json!({"type": "result", "session_id": "lease", "modelUsage": {model: {
            "inputTokens": input, "outputTokens": output, "cacheReadInputTokens": cache_read, "cacheCreationInputTokens": 0}}})
    }
    /// What native.rs does with one frame while the monitor is on.
    fn feed(t: &mut Tokens, tally: &mut crate::quota::tokens::Tally, source: &Source, frame: serde_json::Value, now: u64) {
        let found = tally.frame(&frame, now);
        t.record(source, frame["parent_tool_use_id"].as_str(), frame["type"] == "result", &found, now);
    }

    #[test]
    fn counting_from_turns_adds_what_each_copy_of_a_message_adds() {
        let mut t = Tokens::default();
        let mut lead = crate::quota::tokens::Tally::default();
        let rich = chat("general", false);
        // a message streamed at its start, then again larger, then once more unchanged
        feed(&mut t, &mut lead, &rich, assistant("msg_1", "claude-fable-5-1", 10, 4, 20_000, None), T0);
        feed(&mut t, &mut lead, &rich, assistant("msg_1", "claude-fable-5-1", 10, 152, 20_000, None), T0 + 2_000);
        feed(&mut t, &mut lead, &rich, assistant("msg_1", "claude-fable-5-1", 10, 152, 20_000, None), T0 + 2_500);
        let now = T0 + 3_000;
        assert_eq!(t.used(now, 60_000), 20_162, "the message counts once, at its final size");
        // a second message on another chat's team, by a helper
        let mut team = crate::quota::tokens::Tally::default();
        feed(&mut t, &mut team, &chat("acme-deal", true), assistant("msg_2", "claude-opus-5-5", 100, 50, 0, Some("toolu_a")), now);
        assert_eq!(t.used(now, 60_000), 20_312);
        let (rows, models, kinds) = t.sources(now, 900);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].thread.as_deref(), rows[0].team, rows[0].model, rows[0].tokens), (Some("general"), false, "Fable", 20_162));
        assert_eq!((rows[1].thread.as_deref(), rows[1].team, rows[1].working, rows[1].model), (Some("acme-deal"), true, 1, "Opus"));
        assert_eq!(models, vec![Split { name: "Fable", tokens: 20_162 }, Split { name: "Opus", tokens: 150 }]);
        assert_eq!(kinds.iter().map(|k| k.tokens).collect::<Vec<_>>(), vec![20_000, 0, 110, 202]);
    }

    /// The review of 1f3073bad, finding 1: turned on during a turn, the next copy of a message the
    /// lease was already streaming adds only what that copy added, not the whole message again.
    #[test]
    fn turned_on_mid_turn_counts_only_what_arrives_after() {
        let mut lead = crate::quota::tokens::Tally::default();
        let rich = chat("general", false);
        // streamed while the monitor was off: the tally counted it, the monitor did not exist
        let off = lead.frame(&assistant("msg_1", "claude-fable-5-1", 50, 10, 200_000, None), T0);
        assert_eq!(off.len(), 1);
        // turned on 70 s later; the same message grows by 40 output tokens
        let mut t = Tokens { since: T0 + 70_000, ..Tokens::default() };
        feed(&mut t, &mut lead, &rich, assistant("msg_1", "claude-fable-5-1", 50, 50, 200_000, None), T0 + 71_000);
        assert_eq!(t.used(T0 + 71_000, 60_000), 40, "the tokens from before the switch were counted");
    }

    /// The review of 1f3073bad, finding 3: what a `result` adds to a helper's message stays that
    /// helper's, and a remainder no message owns adds tokens but no worker.
    #[test]
    fn a_result_keeps_the_worker_a_message_came_from() {
        let mut team = crate::quota::tokens::Tally::default();
        let mut t = Tokens::default();
        let s = chat("acme-deal", true);
        feed(&mut t, &mut team, &s, assistant("msg_h", "claude-opus-5-5", 100, 4, 0, Some("toolu_a")), T0);
        // the lease's running total shows 5 more output tokens than the frames did
        feed(&mut t, &mut team, &s, result("claude-opus-5-5", 100, 9, 0), T0 + 1_000);
        let (rows, ..) = t.sources(T0 + 2_000, 900);
        assert_eq!((rows[0].tokens, rows[0].per_min, rows[0].working), (109, 109, 1), "the result made a second worker");
        // two open messages: the next remainder belongs to neither
        feed(&mut t, &mut team, &s, assistant("msg_x", "claude-opus-5-5", 10, 1, 0, Some("toolu_b")), T0 + 3_000);
        feed(&mut t, &mut team, &s, assistant("msg_y", "claude-opus-5-5", 10, 1, 0, Some("toolu_b")), T0 + 3_100);
        feed(&mut t, &mut team, &s, result("claude-opus-5-5", 120, 20, 0), T0 + 4_000);
        let (rows, ..) = t.sources(T0 + 5_000, 900);
        assert_eq!((rows[0].tokens, rows[0].working), (140, 2), "an unowned remainder counted as a worker");
    }

    /// The review of 1f3073bad, finding 4: the graph covers the whole stretch picked, so its bars
    /// add up to what the details say was used over it.
    #[test]
    fn the_graph_plots_the_whole_stretch_picked() {
        let mut t = Tokens::default();
        let s = chat("general", true);
        let now = T0 + 7_200_000 + 29_000; // mid-minute
        let burst = now - (59 * 60 + 31) * 1000;
        t.record(&s, None, false, &[use_of("burst", "claude-opus-5-5", 300_000, 0, 0)], burst);
        t.record(&s, None, false, &[use_of("late", "claude-opus-5-5", 2_000, 0, 0)], now - 5_000);
        for (span, n) in [(900, 60), (3_600, 60), (86_400, 96)] {
            let series = t.series(now, span, n);
            let (rows, ..) = t.sources(now, span);
            let used: u64 = rows.iter().map(|r| r.tokens).sum();
            assert_eq!(series.bars.iter().sum::<u64>(), used, "{span} s: the bars and Used disagree");
            assert!(series.bars.len() as u64 == n || series.bars.len() as u64 == n + 1, "{span} s: {} bars", series.bars.len());
        }
        assert_eq!(t.series(now, 3_600, 60).bars.iter().sum::<u64>(), 302_000, "the hour's oldest burst is plotted");
        // fifteen minutes from the same moment, a stretch that starts inside a 15-second slice
        let recent = now - 899_500;
        let mut t = Tokens::default();
        t.record(&s, None, false, &[use_of("edge", "claude-opus-5-5", 500, 0, 0)], recent);
        assert_eq!(t.series(now, 900, 60).bars.iter().sum::<u64>(), 500);
        assert_eq!(t.view(now, true, None).m15, 500);
    }

    #[test]
    fn the_speeds_over_one_five_and_fifteen_minutes_are_their_own_windows() {
        let mut t = Tokens::default();
        let s = chat("general", true);
        // 1,000 tokens 14 minutes ago, 2,000 four minutes ago, 3,000 thirty seconds ago
        t.record(&s, None, false, &[use_of("a", "claude-opus-5-5", 1_000, 0, 0)], T0);
        t.record(&s, None, false, &[use_of("b", "claude-opus-5-5", 2_000, 0, 0)], T0 + 600_000);
        t.record(&s, None, false, &[use_of("c", "claude-opus-5-5", 3_000, 0, 0)], T0 + 810_000);
        let now = T0 + 840_000;
        let v = t.view(now, true, None);
        assert_eq!((v.m1, v.m5, v.m15), (3_000, 5_000, 6_000));
        // the card's graph is the same 15 minutes in sixty bars, the rail's the last minute in ten
        assert_eq!(v.card.iter().sum::<u64>(), 6_000);
        assert!(v.card.len() == 60 || v.card.len() == 61, "{}", v.card.len());
        assert_eq!(v.tile.iter().sum::<u64>(), 3_000);
        assert_eq!(v.tile.len(), 10);
        assert_eq!(v.scale, 3_000);
        // sixteen minutes on, the first request has left the 15-minute window
        let later = T0 + 960_000;
        t.prune(later);
        assert_eq!(t.view(later, true, None).m15, 5_000);
        // ...and is still in the day's per-minute rows, for the hour and day views
        let hour = t.series(later, 3600, 60);
        assert_eq!(hour.bars.iter().sum::<u64>(), 6_000);
        assert_eq!(hour.bar_ms, 60_000);
        let day = t.series(later, 86_400, 96);
        assert_eq!((day.bars.iter().sum::<u64>(), day.bar_ms), (6_000, 900_000));
        let (rows, ..) = t.sources(later, 3600);
        assert_eq!(rows[0].tokens, 6_000);
    }

    /// The second review, at b7f443a6c: the rail's ten bars add up to the speed under them.
    #[test]
    fn the_rails_bars_add_up_to_the_last_minute() {
        let mut t = Tokens::default();
        let s = chat("general", false);
        let now = T0 + 120_000 + 3_000;
        t.record(&s, None, false, &[use_of("burst", "claude-fable-5-1", 300_000, 0, 0)], now - 59_500);
        t.record(&s, None, false, &[use_of("now", "claude-fable-5-1", 20, 0, 0)], now);
        let v = t.view(now, false, None);
        assert_eq!(v.m1, 300_020);
        assert_eq!((v.tile.len(), v.tile.iter().sum::<u64>(), v.tile[0], v.tile[9]), (10, 300_020, 300_000, 20));
        assert!(v.scale >= 300_000, "a bar taller than the scale");
    }

    #[test]
    fn idle_ten_seconds_after_the_last_request() {
        let mut t = Tokens::default();
        assert!(!t.view(T0, true, None).active, "nothing used yet is idle");
        assert_eq!(t.view(T0, true, None).last_ms, None);
        t.record(&chat("general", false), None, false, &[use_of("a", "claude-fable-5-1", 500, 20, 0)], T0);
        assert!(t.view(T0 + 9_999, true, None).active, "a request 9.999 s ago is being used now");
        let v = t.view(T0 + 10_000, true, None);
        assert!(!v.active, "ten seconds after the last request the rail says Idle");
        assert_eq!(v.last_ms, Some(T0));
        assert_eq!(v.m1, 520, "the last minute still shows what it used");
    }

    #[test]
    fn a_bar_is_aligned_to_the_clock_and_holds_the_current_second() {
        let mut t = Tokens::default();
        t.record(&chat("general", false), None, false, &[use_of("a", "claude-fable-5-1", 7, 0, 0)], T0 + 59_400);
        let s = t.series(T0 + 59_900, 60, 10);
        assert_eq!(s.ends_at_ms, T0 + 60_000);
        assert_eq!(s.bars.last(), Some(&7));
        assert_eq!(s.bar_ms, 6_000);
    }

    #[test]
    fn memory_keeps_two_minute_figures_and_never_sums_an_unreadable_app() {
        let mut m = Memory::default();
        let apps = |richos: Option<i64>, files: i64| -> Vec<crate::suite_memory::LiveApp> {
            crate::suite_memory::apps()
                .iter()
                .map(|(id, app)| match *id {
                    "richos" => crate::suite_memory::LiveApp { id, app, bytes: richos, processes: 3, within: None },
                    "richfiles" => crate::suite_memory::LiveApp { id, app, bytes: Some(files), processes: 1, within: None },
                    _ => crate::suite_memory::LiveApp { id, app, bytes: Some(0), processes: 0, within: None },
                })
                .collect()
        };
        assert_eq!(m.view(T0, None).status, "measuring");
        m.push(T0, Ok(apps(Some(1_000), 100)), Some(36 << 30));
        m.push(T0 + 2_000, Ok(apps(Some(2_000), 100)), Some(36 << 30));
        m.push(T0 + 4_000, Ok(apps(None, 100)), Some(36 << 30));
        let v = m.view(T0 + 4_000, Some(&Detail { memory_span_s: Some(120), memory_points: Some(60), ..Detail::default() }));
        assert_eq!(v.status, "available");
        assert_eq!(v.all.now, None, "RichOS unreadable: the total is unknown, not too low");
        assert_eq!((v.all.avg, v.all.peak), (Some(1_600), Some(2_100)));
        let richos = &v.apps[0];
        assert_eq!((richos.id, richos.processes, richos.line.now, richos.line.avg), ("richos", 3, None, Some(1_500)));
        assert_eq!(v.apps.iter().find(|a| a.id == "richglance").unwrap().processes, 0, "RichGlance is not open");
        assert_eq!(v.apps.iter().find(|a| a.id == "richfinder").unwrap().within, Some("richos"));
        assert_eq!(v.series.as_ref().unwrap().list.iter().flatten().count(), 2);
        // three minutes on, the 2-minute figures hold only the last two minutes
        m.push(T0 + 184_000, Ok(apps(Some(4_000), 200)), None);
        let v = m.view(T0 + 184_000, None);
        assert_eq!((v.all.now, v.all.avg, v.all.hist.len()), (Some(4_200), Some(4_200), 1));
        m.push(T0 + 186_000, Err("process list unavailable".into()), None);
        assert_eq!(m.view(T0 + 186_000, None).status, "unavailable");
    }

    /// The one test of the process-wide switch (the others use their own stores).
    #[test]
    fn off_means_nothing_is_counted_sampled_or_kept() {
        let root = std::env::temp_dir().join(format!("my-richos-{}", uuid::Uuid::new_v4()));
        let source = chat("general", false);
        assert!(!on(), "the monitor is off by default");
        record(&source, None, false, &[use_of("off_1", "claude-fable-5-1", 100, 0, 0)]);
        let off = view(true, Some(Detail { token_span_s: Some(900), token_bars: Some(60), memory_span_s: Some(120), memory_points: Some(60) }));
        assert!(!off.on && off.tokens.is_none() && off.memory.is_none());
        assert_eq!(*lock(&shared().sampler), 0, "no sampler was ever started while off");
        assert!(lock(&shared().tokens).recent.is_empty(), "a frame was counted while off");

        set_on(true, &root);
        record(&source, None, false, &[use_of("on_1", "claude-fable-5-1", 100, 0, 0)]);
        let v = view(true, None);
        assert!(v.on);
        // At least this test's own request: a lease test running alongside may add its own.
        assert!(v.tokens.as_ref().unwrap().m1 >= 100);
        // the sampler takes its first reading at once: waited for as a fact, never a time
        wait_until(|| view(true, None).memory.unwrap().status != "measuring");
        assert_eq!(SAMPLERS.load(Ordering::SeqCst), 1, "one memory reader while on");

        set_on(false, &root);
        assert!(!view(true, None).on);
        assert!(lock(&shared().tokens).recent.is_empty(), "turning off kept the tokens");
        assert!(lock(&shared().memory).readings.is_empty(), "turning off kept the memory readings");
        record(&source, None, false, &[use_of("off_2", "claude-fable-5-1", 100, 0, 0)]);
        assert!(lock(&shared().tokens).recent.is_empty());
        // the sampler thread ends, and nothing it read after off was kept
        wait_until(|| SAMPLERS.load(Ordering::SeqCst) == 0);
        assert!(lock(&shared().memory).readings.is_empty(), "the sampler kept a reading after off");
        assert!(!root.exists(), "the monitor wrote to the engine state");
    }

    /// Wait for a fact. The bound only stops a hang from holding the run forever.
    fn wait_until(fact: impl Fn() -> bool) {
        // load-bound: a hang guard only; every verdict above is the fact, never this time
        let hang = std::time::Instant::now() + Duration::from_secs(300);
        while !fact() {
            assert!(std::time::Instant::now() < hang, "hung waiting for a fact");
            std::thread::yield_now();
        }
    }
}
