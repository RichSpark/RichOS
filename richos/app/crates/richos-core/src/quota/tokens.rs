//! **The weekly speed from live token use** (the CEO, 2026-10-07: *"the percentage for the
//! switch needs to be adjusted dynamically so that this doesn't happen"*, an account reaching
//! 100% before the switch; *"if we keep tracking Claude's current weekly percentage vs token
//! consumption for different models"*).
//!
//! The weekly percentage moves in whole points, so a burst is invisible to the percentage speed
//! (`Snapshot::measure`) until a whole point has gone. Token use is visible at once: every
//! assistant message's `usage` is in the stream the app reads from each of its own leases
//! ([`Sink`]: they write no transcript), and, for Claude Code used outside the app, in the
//! account's transcripts (`<folder>/projects/**/*.jsonl`), read exactly as
//! `engine/scripts/token-track.py` reads them (input + output + cache write + cache read, by
//! model; a message streamed in several copies counts once, its largest copy).
//!
//! - **Learning** (`Track::reading`): each weekly percentage value is an episode from the
//!   reading that first shows it (after a lower one, so the rise itself was seen) to the reading
//!   that first shows a higher one; its tokens are those stamped in between. That is
//!   `token-track.py report`'s episode, start to start.
//! - **Tokens per point, per model** (`figures`): episodes mix models, so each model's figure is
//!   the non-negative weighted least-squares fit of `points = sum(tokens_m / per_point_m)` over
//!   the kept episodes, each weighted by 1 / its tokens. With one model that is exactly
//!   `token-track.py`'s total tokens / total points. A model with a published card is measured
//!   against it: the episodes move it off its card (priced at the account's dearest point) only
//!   as far as they measure it, so a few thousand tokens cannot fit any points at all, and every
//!   carded model is read at its own price. A model with no card that the episodes cannot tell
//!   apart from another has no figure yet.
//! - **The speed** (`Track::speed`): over the last minute, each measured model's tokens divided
//!   by its tokens per point, summed; what only a lease's `result` frame showed counts at most
//!   `LATE_POINTS_MAX` a message. The quota service takes the larger of it and the
//!   percentage speed (`Snapshot::reading_with`), so a model with no figure yet, or use this Mac
//!   cannot see (claude.ai, another computer), never makes the speed lower than the percentage
//!   speed.
use super::Window;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

/// The live window: tokens stamped in the last minute are the live tokens per minute.
pub const LIVE_MS: u64 = 60_000;
/// **What only a lease's `result` frame showed counts at most one weekly point in the live
/// minute** (2026-10-28 13:05Z: Work at 3% of its week was left, Ray and Zach ordered to hand
/// off). A `result` frame's remainder ([`Tally`], [`Use::late`]) is what the lease spent over
/// its whole turn, its helpers' unstreamed output included, all of it stamped at one moment. It
/// is a lump, not a rate. With only two episodes learned the fit gave Haiku 3,953 tokens a
/// point, so one lease's 83,446 late Haiku tokens read as 83,446 / 3,953 = 21.1 points in the
/// live minute: 21.1 points a minute is fast, so I = 1 minute, H the default 5, and the handoff
/// point was 99 - (ceil(21.1 x (1 + 5)) - 1) = 99 - 126, clamped to 0.
///
/// One point a minute is the app's own line for fast use (`FAST_POINTS_PER_CHECK`, 5 points a
/// 5-minute check). So one late remainder alone can at most make the week read as fast, checked
/// every minute: with nothing else in the minute and the default 5-minute handoff it moves the
/// point from 99 to about 94 (99 - (ceil(1 x 6) - 1)), for the one minute it stays in the live
/// window. Tokens the frames streamed are never capped, so a sustained burst (many messages)
/// reads exactly as before, and every model keeps its figure: the cap is in points, after it.
/// The late tokens are still counted in full everywhere else (`points_since`, the episodes, a
/// handoff's points, the spend).
pub const LATE_POINTS_MAX: f64 = 1.0;
/// Episodes kept per account (newest last). 50 weekly points is half a week of steady use.
pub const KEEP_EPISODES: usize = 50;
/// The most models fitted at once; any others have no figure (2^6 subsets at most).
const MAX_MODELS: usize = 6;

/// One weekly percentage value from its start to the next value's start: the points it rose by
/// and the tokens each model used in between.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub points: f64,
    pub tokens: BTreeMap<String, u64>,
}

/// One account's token reader and what it has learned.
#[derive(Debug, Default)]
pub struct Track {
    /// Each transcript's read position: the end of its last complete line.
    offsets: HashMap<PathBuf, u64>,
    scanned: bool,
    /// Message id -> (stamped at, model, tokens, output tokens, late tokens). The largest copy
    /// wins. Late tokens: those only a `result` frame showed ([`Use::late`]).
    events: HashMap<String, (u64, String, u64, u64, u64)>,
    /// The weekly value being watched: (used, its start if its rise was seen, its reset time).
    current: Option<(f64, Option<u64>, Option<u64>)>,
    last_reading: Option<u64>,
    pub episodes: Vec<Episode>,
    /// Keep every message since this moment too: an order whose handoff commit has not come
    /// yet (`Service::track_tokens` counts its points when it does).
    pub hold_from: Option<u64>,
}

fn stamp(text: &str) -> Option<u64> {
    let parsed = time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).ok()?;
    u64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).ok()
}

fn transcripts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            transcripts(&path, out);
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

impl Track {
    /// A track that starts from episodes learned before (kept across launches).
    pub fn with_episodes(episodes: Vec<Episode>) -> Self {
        Self { episodes, ..Self::default() }
    }

    /// Read what the account's transcripts gained since the last scan. The first scan skips
    /// every transcript untouched for the live window: nothing older is needed yet.
    pub fn scan(&mut self, folder: &Path, now: u64) {
        let mut files = Vec::new();
        transcripts(&folder.join("projects"), &mut files);
        for path in files {
            let Ok(meta) = fs::metadata(&path) else { continue };
            let len = meta.len();
            if !self.scanned && !self.offsets.contains_key(&path) {
                let modified = meta.modified().ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as u64);
                if modified + LIVE_MS < now {
                    self.offsets.insert(path, len);
                    continue;
                }
            }
            let mut offset = self.offsets.get(&path).copied().unwrap_or(0);
            if len < offset { offset = 0; } // rewritten
            if len == offset { continue; }
            let Ok(mut file) = fs::File::open(&path) else { continue };
            let mut bytes = Vec::new();
            if file.seek(SeekFrom::Start(offset)).is_err() || file.read_to_end(&mut bytes).is_err() { continue; }
            // Only complete lines; a line still being written is read next time.
            let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { continue };
            for line in bytes[..end].split(|b| *b == b'\n') {
                self.line(line);
            }
            self.offsets.insert(path, offset + end as u64 + 1);
        }
        self.scanned = true;
    }

    fn line(&mut self, line: &[u8]) {
        if !line.windows(7).any(|w| w == b"\"usage\"") { return; }
        let Ok(row) = serde_json::from_slice::<Value>(line) else { return };
        let Some(at) = row["timestamp"].as_str().and_then(stamp) else { return };
        if let Some(found) = message_use(&row["message"], &row, at) { self.add_use(&found); }
    }

    /// One message's usage, from a transcript or from a lease's stream ([`Sink`]).
    pub fn add_use(&mut self, found: &Use) {
        self.put(&found.id, found.at, &found.model, found.tokens, found.output, found.late);
    }

    /// One assistant message's usage. A later, larger copy of the same message replaces it.
    pub fn add(&mut self, id: &str, at: u64, model: &str, tokens: u64, output: u64) {
        self.put(id, at, model, tokens, output, 0);
    }

    fn put(&mut self, id: &str, at: u64, model: &str, tokens: u64, output: u64, late: u64) {
        if self.events.get(id).is_some_and(|old| old.3 > output) { return; }
        self.events.insert(id.to_string(), (at, model.to_string(), tokens, output, late.min(tokens)));
    }

    /// A reading of the weekly window, taken at `at`. Returns true when it closed an episode
    /// (the caller keeps the episodes across launches).
    pub fn reading(&mut self, window: &Window, at: u64, now: u64) -> bool {
        let mut closed = false;
        if self.last_reading != Some(at) {
            self.last_reading = Some(at);
            let used = window.used_percent;
            // The SAME window when the reset times agree within a minute (`Snapshot::measure`).
            let same = |a: Option<u64>, b: Option<u64>| match (a, b) {
                (Some(a), Some(b)) => a.abs_diff(b) < 60_000,
                (a, b) => a == b,
            };
            self.current = match self.current {
                Some((value, start, resets)) if same(resets, window.resets_at) => {
                    if used > value {
                        if let Some(start) = start {
                            let mut tokens = BTreeMap::new();
                            for (stamped, model, count, ..) in self.events.values() {
                                if start < *stamped && *stamped <= at { *tokens.entry(model.clone()).or_insert(0) += count; }
                            }
                            self.episodes.push(Episode { points: used - value, tokens });
                            let extra = self.episodes.len().saturating_sub(KEEP_EPISODES);
                            self.episodes.drain(..extra);
                            closed = true;
                        }
                        Some((used, Some(at), window.resets_at))
                    } else if used < value {
                        Some((used, None, window.resets_at))
                    } else {
                        Some((value, start, resets))
                    }
                }
                _ => Some((used, None, window.resets_at)),
            };
        }
        // Keep only what an episode still open, the live window, the tokens since the last good
        // reading (`points_since`) or a handoff still under way (`hold_from`) can use.
        let keep = [self.current.and_then(|c| c.1), Some(now.saturating_sub(LIVE_MS)), self.last_reading, self.hold_from]
            .into_iter().flatten().min().unwrap_or(0);
        self.events.retain(|_, e| e.0 > keep);
        closed
    }

    /// **Weekly points spent since `since`** (the last good reading), each measured model's
    /// tokens divided by its tokens per point. A model with no figure adds nothing: its tokens
    /// cannot be turned into points honestly.
    pub fn points_since(&self, since: u64, now: u64) -> f64 {
        let per_point = figures(&self.episodes);
        self.events.values()
            .filter(|e| e.0 > since && e.0 <= now)
            .filter_map(|e| per_point.get(&e.1).map(|p| e.2 as f64 / p))
            .sum()
    }

    /// Weekly points per millisecond from the last minute's tokens (`LIVE_MS`), each model's
    /// tokens divided by its tokens per point. A model with no figure adds nothing. What only a
    /// `result` frame showed adds at most [`LATE_POINTS_MAX`] per message.
    pub fn speed(&self, now: u64) -> f64 {
        let per_point = figures(&self.episodes);
        let points: f64 = self.events.values()
            .filter(|e| e.0 + LIVE_MS > now && e.0 <= now)
            .filter_map(|e| per_point.get(&e.1).map(|p| (e.2 - e.4) as f64 / p + (e.4 as f64 / p).min(LATE_POINTS_MAX)))
            .sum();
        points / LIVE_MS as f64
    }
}

/// One assistant message's usage: input + output + cache write + cache read, by model, as
/// `token-track.py` counts it, and when it was stamped. `request` keeps what that sum throws
/// away and pricing needs (the API-equivalent spend, §118, `crate::spend`): the five buckets,
/// the two cache writes apart, the effort, fast mode and US-only inference. `late`: how many of
/// `tokens` only a lease's `result` frame showed ([`Tally`]; 0 for anything a frame or a
/// transcript row showed), which the live speed counts at most [`LATE_POINTS_MAX`] of.
#[derive(Clone, Debug, PartialEq)]
pub struct Use { pub id: String, pub at: u64, pub model: String, pub tokens: u64, pub output: u64, pub request: crate::spend::Request, pub late: u64 }

/// The usage of one assistant `message` (a transcript row's or a stream frame's), stamped
/// `at`; `row` is the frame or transcript row around it (its `effort`). `None` without usage,
/// without tokens or without an id.
pub fn message_use(message: &Value, row: &Value, at: u64) -> Option<Use> {
    let usage = message["usage"].as_object()?;
    let count = |field: &str| usage.get(field).and_then(Value::as_u64).unwrap_or(0);
    let output = count("output_tokens");
    let tokens = count("input_tokens") + output + count("cache_creation_input_tokens") + count("cache_read_input_tokens");
    if tokens == 0 { return None; }
    let id = message["id"].as_str().or(row["uuid"].as_str())?.to_string();
    let model = message["model"].as_str().unwrap_or("unknown").to_string();
    let request = crate::spend::Request::of_message(message, row, at)?;
    Some(Use { id, at, model, tokens, output, request, late: 0 })
}

/// The most messages a sink holds before the quota service takes them (it takes them every
/// `TOKEN_SCAN_MS` and before every turn); past it the oldest go first.
const SINK_KEEP: usize = 20_000;

/// **Token use the app's own leases streamed** (the CEO, 2026-10-07; handoff round 2, run 10:
/// the leases run with `--no-session-persistence`, so they write no transcript and
/// `Track::scan` never saw the app's own teammates). Every lease's reader puts each
/// `assistant` frame's usage here under the account the lease runs on, its helpers' nested
/// frames included; the quota service takes them into that account's `Track`. A message
/// streamed in several frames counts once, at its largest copy, exactly as a transcript does.
///
/// **And the spend ledger takes them too** (§118): those leases' requests exist nowhere else, so
/// each one also waits in a second queue that only the ledger drains ([`Sink::take_for_spend`]),
/// whichever of the two readers comes first.
#[derive(Debug, Default)]
pub struct Sink { pending: std::sync::Mutex<Vec<(String, Use)>>, spend: std::sync::Mutex<Vec<crate::spend::Request>> }

impl Sink {
    /// An `assistant` frame from a lease on `account`, received at `at`. Anything else, or an
    /// assistant frame with no usage, adds nothing.
    pub fn frame(&self, account: &str, frame: &Value, at: u64) {
        if frame["type"] != "assistant" { return; }
        let Some(found) = message_use(&frame["message"], frame, at) else { return };
        self.push(account, found);
    }
    /// One message's usage (or a [`Tally`] remainder) from a lease on `account`.
    pub fn push(&self, account: &str, found: Use) {
        {
            let mut spend = self.spend.lock().unwrap();
            if spend.len() >= SINK_KEEP { spend.remove(0); }
            spend.push(found.request.clone());
        }
        let mut pending = self.pending.lock().unwrap();
        if pending.len() >= SINK_KEEP { pending.remove(0); }
        pending.push((account.to_string(), found));
    }
    /// Everything put here since the last take, oldest first.
    pub fn take(&self) -> Vec<(String, Use)> {
        std::mem::take(&mut *self.pending.lock().unwrap())
    }
    /// Every request put here since the spend ledger last took them, oldest first.
    pub fn take_for_spend(&self) -> Vec<crate::spend::Request> {
        std::mem::take(&mut *self.spend.lock().unwrap())
    }
}

/// **One lease's tokens, the ones no `assistant` frame shows included** (2.1.292, measured
/// 2026-10-07 with the app's own flags). An `assistant` frame carries the usage of its
/// message's START: the lead's Agent call streamed `output_tokens: 16` and finished at 152,
/// and a background helper's one message streamed 4 and finished at 5 (its
/// `task_notification`: 10,152 in all). What was written after the start is only in the
/// `result` frame's `modelUsage`, the lease's running total per model, the helpers' tokens
/// included. So the frames count at once (the live speed), and each `result` adds, per model,
/// whatever its running total holds beyond what the frames and earlier results counted.
///
/// **For the spend, each request's own final usage** (§118; the second review of `fc1d7bb31`,
/// rv-20261011T034701Z: a remainder priced at the latest request's card mispriced a long-context
/// Haiku lead, counted as a request of its own, and landed on the result's day). So:
/// - **A `message_delta` finalizes its own message.** With `--include-partial-messages` the lead's
///   messages stream `message_start` (its id) and `message_delta` (its final `output_tokens`);
///   the message is re-emitted at its final size, with its own id, time and card.
/// - **A remainder with one message to belong to IS that message.** When, since the last
///   `result`, exactly one message on that model was not finalized, the remainder is added to it
///   (same id, time and card), so it is priced and dated exactly and is no new request.
/// - **Otherwise it is a correction, never a request** (`Request::correction`): priced on the
///   card those messages share, dated at the earliest of them (so it is counted only in the
///   periods all of them fall in; the second review of `6c21269e6`); and when they span cards (Haiku 5.5
///   either side of 100,000), it is counted and left unpriced (`Request::unmatched`), an
///   explicit gap rather than a guess. A cache-write remainder has no 5-minute and 1-hour split in
///   `modelUsage`, so it is a 5-minute write (none was ever seen).
#[derive(Debug, Default)]
pub struct Tally {
    /// Each message counted so far, by id: its model and its largest streamed total.
    seen: HashMap<String, (String, u64)>,
    /// Per model, everything this lease has counted: frames plus earlier remainders.
    counted: BTreeMap<String, u64>,
    /// Each message's largest request so far, by id (its buckets, time and card).
    requests: HashMap<String, crate::spend::Request>,
    /// Per model (without a date suffix), the buckets counted: frames plus earlier remainders.
    counted_buckets: BTreeMap<String, crate::spend::Buckets>,
    /// Per model, the messages since the last `result` that no `message_delta` finalized.
    open: BTreeMap<String, Vec<String>>,
    /// The message each stream is writing (`message_start`), by its `parent_tool_use_id`.
    streaming: HashMap<Option<String>, String>,
    results: u64,
}

impl Tally {
    /// Count `request` (a message, or a larger copy of one) and say what to report.
    fn count(&mut self, request: crate::spend::Request, finalized: bool) -> Option<Use> {
        let base = crate::spend::base_model(&request.model).to_string();
        let old = self.requests.get(&request.id).map(|r| r.buckets).unwrap_or_default();
        let grew = old.beyond(&request.buckets);
        if grew.total() == 0 && self.requests.contains_key(&request.id) {
            if finalized { self.finalize(&base, &request.id); }
            return None;
        }
        self.counted_buckets.entry(base.clone()).or_default().add(&grew);
        *self.counted.entry(request.model.clone()).or_insert(0) += grew.total();
        // Its first copy as it is; a later one grows the copy already kept by what it adds.
        let kept = match self.requests.get(&request.id) {
            Some(had) => {
                let mut kept = had.clone();
                kept.buckets.add(&grew);
                kept
            }
            None => request,
        };
        self.seen.insert(kept.id.clone(), (kept.model.clone(), kept.buckets.total()));
        self.requests.insert(kept.id.clone(), kept.clone());
        if finalized {
            self.finalize(&base, &kept.id);
        } else {
            let open = self.open.entry(base).or_default();
            if !open.contains(&kept.id) { open.push(kept.id.clone()); }
        }
        Some(Use { id: kept.id.clone(), at: kept.at, model: kept.model.clone(), tokens: kept.buckets.total(),
            output: kept.buckets.output, request: kept, late: 0 })
    }

    fn finalize(&mut self, base: &str, id: &str) {
        if let Some(open) = self.open.get_mut(base) { open.retain(|o| o != id); }
    }

    /// What to count for one frame from this lease, received at `at`.
    pub fn frame(&mut self, frame: &Value, at: u64) -> Vec<Use> {
        match frame["type"].as_str() {
            Some("assistant") => {
                let Some(found) = message_use(&frame["message"], frame, at) else { return vec![] };
                self.count(found.request, false).into_iter().collect()
            }
            Some("stream_event") => {
                let event = &frame["event"];
                let parent = frame["parent_tool_use_id"].as_str().map(str::to_string);
                match event["type"].as_str() {
                    Some("message_start") => {
                        if let Some(id) = event["message"]["id"].as_str() { self.streaming.insert(parent, id.to_string()); }
                        vec![]
                    }
                    Some("message_delta") => {
                        let (Some(id), Some(output)) = (self.streaming.get(&parent).cloned(), event["usage"]["output_tokens"].as_u64())
                            else { return vec![] };
                        let Some(mut request) = self.requests.get(&id).cloned() else { return vec![] };
                        request.buckets.output = request.buckets.output.max(output);
                        self.count(request, true).into_iter().collect()
                    }
                    _ => vec![],
                }
            }
            Some("result") => {
                let Some(models) = frame["modelUsage"].as_object() else { return vec![] };
                self.results += 1;
                let lease = frame["session_id"].as_str().unwrap_or("lease").to_string();
                let mut out = Vec::new();
                for (model, usage) in models {
                    let count = |field: &str| usage.get(field).and_then(Value::as_u64).unwrap_or(0);
                    let total = count("inputTokens") + count("outputTokens")
                        + count("cacheCreationInputTokens") + count("cacheReadInputTokens");
                    let base = crate::spend::base_model(model).to_string();
                    let had = self.counted_buckets.get(&base).copied().unwrap_or_default();
                    let writes = count("cacheCreationInputTokens").saturating_sub(had.write_5m + had.write_1h);
                    let rest = crate::spend::Buckets {
                        input: count("inputTokens").saturating_sub(had.input),
                        output: count("outputTokens").saturating_sub(had.output),
                        cache_read: count("cacheReadInputTokens").saturating_sub(had.cache_read),
                        write_5m: writes,
                        write_1h: 0,
                    };
                    let open: Vec<crate::spend::Request> = std::mem::take(self.open.entry(base.clone()).or_default())
                        .iter().filter_map(|id| self.requests.get(id).cloned()).collect();
                    if rest.total() > 0 {
                        if let [only] = open.as_slice() {
                            // The one message it can belong to: that message, at its final size.
                            // What the frames streamed of it stays live; the rest is late.
                            let mut request = only.clone();
                            request.buckets.add(&rest);
                            out.extend(self.count(request, true).map(|found| Use { late: rest.total(), ..found }));
                        } else {
                            out.push(self.correction(&lease, model, &rest, &open, at));
                        }
                    }
                    // Whatever the frames counted, the lease's running total is now this.
                    let counted = self.counted.entry(model.clone()).or_insert(0);
                    *counted = (*counted).max(total);
                }
                out
            }
            _ => vec![],
        }
    }

    /// A remainder that cannot be put on one message: spend with no request of its own.
    fn correction(&mut self, lease: &str, model: &str, rest: &crate::spend::Buckets, open: &[crate::spend::Request], at: u64) -> Use {
        let base = crate::spend::base_model(model).to_string();
        self.counted_buckets.entry(base).or_default().add(rest);
        // The messages it can belong to: those still open, else every one on this model.
        let pool: Vec<&crate::spend::Request> = if open.is_empty() {
            self.requests.values().filter(|r| r.model == model).collect()
        } else {
            open.iter().collect()
        };
        let latest = pool.iter().max_by_key(|r| r.at).copied();
        // Dated by its own requests: at the earliest of them. Which of them it belongs to is not
        // recorded, and the periods are nested back from now, so a remainder dated at its earliest
        // request is counted in exactly the periods every one of them falls in (the second review
        // of 6c21269e6: dated at the latest, a turn crossing midnight moved yesterday's output
        // into Today). All time always has it.
        let earliest = pool.iter().map(|r| r.at).min();
        let cards: std::collections::BTreeSet<crate::spend::Card> = pool.iter().map(|r| r.card()).collect();
        let id = format!("{lease}:result-{}:{model}", self.results);
        let request = crate::spend::Request {
            id: id.clone(),
            at: earliest.unwrap_or(at),
            model: model.to_string(),
            effort: latest.and_then(|l| l.effort.clone()),
            fast: latest.is_some_and(|l| l.fast),
            us: latest.is_some_and(|l| l.us),
            buckets: *rest,
            card_prompt: latest.map(|l| l.buckets.prompt()),
            correction: true,
            unmatched: cards.len() > 1,
        };
        Use { id, at: request.at, model: model.to_string(), tokens: rest.total(), output: 0, request, late: rest.total() }
    }
}

/// A model's published standard input rate (USD per million tokens, `crate::spend`), the price its
/// card says one of its tokens costs against another model's. Every standard card prices output at
/// 5 times input, so input and output give the same ratio between models. `None` without a card.
fn card_price(model: &str) -> Option<f64> {
    let card = crate::spend::Card { model: crate::spend::base_model(model).to_string(), ..Default::default() };
    crate::spend::rates(&card).map(|r| r.input).filter(|p| *p > 0.)
}

/// The variance, in points squared, of one episode's points when the episodes cannot measure it
/// themselves (no more episodes than fitted models): a fifth of a point either way. An episode
/// runs from one whole-point rise to the next, so its points are off only by the minutes before a
/// reading sees a rise and by a late `result` landing in the next episode. Half a point let the
/// card pull a model with an episode of its own (the second review of 0e2a10356: 40M Opus cache
/// reads for one point and 4M Sonnet input for the next, $8 each, read Sonnet at 5.97M, not
/// 4M); a hundredth let one tiny model's share of two noisy episodes move it to a fraction of its
/// card (the third review's finding 1 at point 97).
const UNMEASURED_NOISE: f64 = 0.04;
/// The least variance taken from the episodes' own residuals: precise episodes are trusted
/// almost entirely, but two models always spent in the same mix still read at their cards.
const LEAST_NOISE: f64 = 1e-6;
/// How far from its card a model's price a token is expected to be, relative: one standard
/// deviation is ten times its card. The card is one bucket's price, and a token's price depends on
/// its bucket: Opus 5.5's cache read is $0.20 a million, its output $20, a span of 100 and ten
/// its geometric middle. So episodes that measure a model well move it off its card (the second
/// review of d22fc43f8: Sonnet measured at 4M a point beside an Opus-and-Fable pair the episodes
/// cannot separate, where its card at the dearest point says 18M), while a model with a few
/// thousand tokens still cannot move.
const CARD_SPREAD: f64 = 10.;

/// The non-negative weighted least-squares fit of `points = sum(tokens_g x per_million_g)` over
/// `rows` (each column's tokens in millions, the points, the weight), by trying every subset of
/// columns: the best fit whose every coefficient is positive, as (columns, points per million).
fn fit(rows: &[(Vec<f64>, f64, f64)], columns: usize) -> Option<(Vec<usize>, Vec<f64>)> {
    let mut best: Option<(f64, Vec<usize>, Vec<f64>)> = None;
    for mask in 1u32..(1 << columns) {
        let pick: Vec<usize> = (0..columns).filter(|i| mask >> i & 1 == 1).collect();
        let k = pick.len();
        let mut system = vec![vec![0.; k + 1]; k];
        for (x, points, weight) in rows {
            for (row, i) in system.iter_mut().zip(&pick) {
                for (cell, j) in row.iter_mut().zip(&pick) { *cell += weight * x[*i] * x[*j]; }
                row[k] += weight * x[*i] * points;
            }
        }
        let Some(per_million) = solve(system) else { continue };
        if per_million.iter().any(|c| c.is_nan() || *c <= 0.) { continue; }
        let residual: f64 = rows.iter().map(|(x, points, weight)| {
            let fit: f64 = pick.iter().zip(&per_million).map(|(i, c)| x[*i] * c).sum();
            weight * (points - fit).powi(2)
        }).sum();
        if best.as_ref().is_none_or(|(r, ..)| residual < *r) { best = Some((residual, pick, per_million)); }
    }
    best.map(|(_, pick, per_million)| (pick, per_million))
}

/// Tokens per weekly point for every model the episodes saw (see the module comment).
///
/// **Main's fit, with each model's published card as what the episodes are measured against**
/// (the third review of the late-frame fix, rv-20261029T115538Z: a worst-case bound is not a
/// price, and a model read with another is read at the other's price).
///
/// - **The card, at the scale the episodes measured for the model's own spend.** Every carded
///   model's tokens are priced at its card (`card_price`). For the share of the episodes' card
///   dollars that is the model's own, the scale is the one the episodes measure all together (a
///   model that is the whole spend is read exactly as main reads it, total tokens over total
///   points: the second review of dc6918a1d, Opus alone at 10M and 50M a point is 30M, not the
///   dearest 50M). For the rest, a model the episodes barely saw, the scale is the dearest whole
///   point, not the average: a model read too dear hands an account off with room left and
///   nothing corrects it, while one read too cheap is caught by the percentage speed, the floor
///   of the speed, within a point. Work, 2026-10-28 13:05Z (Ray and Zach ordered to hand off at
///   3% of the week): Haiku 5.5 is 40 times cheaper than Opus 5.5 by its card, so at Work's
///   dearest point (40,634,401 Opus and 18,154,910 Sonnet tokens, $198.8 of input for one point)
///   it is 1.99 billion tokens a point and the 83,446 tokens of one lease's `result` are 0.00004
///   points, where main's fit gave Haiku 3,953 tokens a point and the week's point fell to 0.
/// - **The episodes move a model off its card as far as they measure it** (a ridge fit: one more
///   row per carded model, saying it costs its card, weighted by how precisely the episodes
///   measure points). The noise is the fit's own residual per spare episode; with no spare episode
///   (two episodes, three models: any pair fits both exactly) it is `UNMEASURED_NOISE`. So a
///   model with a few thousand tokens stays at its card, while six exact episodes measure Fable
///   at twice its card exactly. Every carded model is read, at its own price (never another
///   model's), and the card rows only add to main's rows, so a mix the episodes explain is read
///   exactly as they explain it.
/// - **A model with no card is fitted as main fits it**: one model is token-track's total tokens
///   over total points, and a model the episodes cannot tell apart from another has no figure.
/// - **Past `MAX_MODELS`**, a carded model is read at its card and its points taken out of every
///   episode before the fit.
pub fn figures(episodes: &[Episode]) -> BTreeMap<String, f64> {
    let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
    for episode in episodes {
        for (model, count) in &episode.tokens { *totals.entry(model).or_insert(0) += count; }
    }
    let mut models: Vec<(&str, u64)> = totals.into_iter().filter(|(_, t)| *t > 0).collect();
    models.sort_by_key(|m| std::cmp::Reverse(m.1));
    // Each episode's spend in card dollars.
    let dollars = |e: &Episode| e.tokens.iter().filter_map(|(m, t)| card_price(m).map(|p| *t as f64 / 1e6 * p)).sum::<f64>();
    // Card dollars a point at the dearest whole point (an episode under half a point is a
    // reading's rounding, not a rise).
    let dearest = episodes.iter().filter(|e| e.points >= 0.5).map(|e| dollars(e) / e.points).fold(0., f64::max);
    // Card dollars a point as the episodes measure them all together: the fit below with one
    // column, the card dollars (with one model, its total tokens over total points exactly).
    let (gained, spent) = episodes.iter().filter(|e| e.points > 0.).fold((0., 0.), |(gained, spent), e| {
        let all = e.tokens.values().sum::<u64>() as f64 / 1e6;
        let x = dollars(e);
        if all > 0. { (gained + e.points * x / all, spent + x * x / all) } else { (gained, spent) }
    });
    let scale = if gained > 0. { spent / gained } else { dearest };
    let total: f64 = episodes.iter().filter(|e| e.points > 0.).map(dollars).sum();
    // Points per million tokens at the card, at the scale the episodes measured for as much of the
    // spend as the model's own and at the dearest point for the rest.
    let card = |model: &str| card_price(model).filter(|_| dearest > 0. && total > 0.).map(|p| {
        let own = episodes.iter().filter(|e| e.points > 0.)
            .map(|e| e.tokens.get(model).map_or(0., |t| *t as f64 / 1e6 * p)).sum::<f64>() / total;
        p / (scale * own + dearest * (1. - own))
    });
    let (fitted, past) = models.split_at(models.len().min(MAX_MODELS));
    let mut figures: BTreeMap<String, f64> = past.iter()
        .filter_map(|(m, _)| card(m).map(|c| (m.to_string(), 1e6 / c))).collect();
    // One row per episode with tokens: each fitted model's tokens in millions, the points the
    // models past `MAX_MODELS` leave, the weight.
    let mut rows: Vec<(Vec<f64>, f64, f64)> = episodes.iter().filter_map(|e| {
        let all: u64 = e.tokens.values().sum();
        (all > 0 && e.points > 0.).then(|| {
            let past_points: f64 = past.iter().filter_map(|(m, _)| figures.get(*m).map(|f| e.tokens.get(*m).copied().unwrap_or(0) as f64 / f)).sum();
            (fitted.iter().map(|(m, _)| e.tokens.get(*m).copied().unwrap_or(0) as f64 / 1e6).collect(),
                (e.points - past_points).max(0.), 1e6 / all as f64)
        })
    }).collect();
    let measured = rows.len();
    let Some(free) = fit(&rows, fitted.len()) else { return figures };
    let carded: Vec<(usize, f64)> = fitted.iter().enumerate().filter_map(|(i, (m, _))| card(m).map(|c| (i, c))).collect();
    let (pick, per_million) = if carded.is_empty() { free } else {
        let weight = rows.iter().map(|r| r.2).sum::<f64>() / measured as f64;
        let spare = measured.saturating_sub(free.0.len());
        let noise = if spare == 0 { UNMEASURED_NOISE } else {
            let residual: f64 = rows.iter().map(|(x, points, w)| {
                w * (points - free.0.iter().zip(&free.1).map(|(i, c)| x[*i] * c).sum::<f64>()).powi(2)
            }).sum();
            (residual / spare as f64 / weight).max(LEAST_NOISE)
        };
        for (i, c) in &carded {
            let mut x = vec![0.; fitted.len()];
            x[*i] = 1.;
            rows.push((x, *c, noise * weight / (CARD_SPREAD * c).powi(2)));
        }
        fit(&rows, fitted.len()).unwrap_or(free)
    };
    figures.extend(pick.iter().zip(&per_million).map(|(i, c)| (fitted[*i].0.to_string(), 1e6 / c)));
    // A carded model the fit sets to nothing (the episodes say less than nothing) is still read at
    // its card.
    for (i, c) in carded {
        figures.entry(fitted[i].0.to_string()).or_insert(1e6 / c);
    }
    figures
}

/// Gaussian elimination with partial pivoting on an augmented `k x (k + 1)` system. `None` when
/// it is singular (the episodes cannot tell these models apart).
fn solve(mut a: Vec<Vec<f64>>) -> Option<Vec<f64>> {
    let k = a.len();
    let scale = (0..k).map(|i| a[i][i].abs()).fold(0., f64::max);
    if scale.is_nan() || scale <= 0. { return None; }
    for col in 0..k {
        let pivot = (col..k).max_by(|x, y| a[*x][col].abs().total_cmp(&a[*y][col].abs()))?;
        if a[pivot][col].abs() <= scale * 1e-9 { return None; }
        a.swap(col, pivot);
        let lead = a[col].clone();
        for (r, row) in a.iter_mut().enumerate() {
            if r != col {
                let factor = row[col] / lead[col];
                for (cell, l) in row.iter_mut().zip(&lead).skip(col) { *cell -= factor * l; }
            }
        }
    }
    Some((0..k).map(|i| a[i][k] / a[i][i]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn episode(points: f64, tokens: &[(&str, u64)]) -> Episode {
        Episode { points, tokens: tokens.iter().map(|(m, t)| (m.to_string(), *t)).collect() }
    }
    #[test]
    fn one_model_is_total_tokens_over_total_points_as_token_track_reports_it() {
        let f = figures(&[episode(1., &[("opus", 3_000_000)]), episode(2., &[("opus", 5_000_000)])]);
        assert!((f["opus"] - 8_000_000. / 3.).abs() < 1.);
    }
    /// **A carded model alone is total tokens over total points too** (the second review of
    /// dc6918a1d, its own fixture): Opus 5.5 alone, 10M tokens for one point and 50M for the next
    /// (whole-point readings of a true 30M a point). The card at the dearest point (50M) pulled
    /// it to 41.4M; it is the whole spend, so it is read at the scale the episodes measure: 30M.
    #[test]
    fn a_carded_model_alone_is_total_tokens_over_total_points_too() {
        let f = figures(&[episode(1., &[("claude-opus-5-5", 10_000_000)]), episode(1., &[("claude-opus-5-5", 50_000_000)])]);
        assert!((f["claude-opus-5-5"] / 30e6 - 1.).abs() < 1e-9, "{f:?}");
    }
    #[test]
    fn mixed_episodes_separate_the_models() {
        // Opus 2M tokens a point, Sonnet 10M a point, in changing mixes.
        let eps = [
            episode(1., &[("opus", 1_000_000), ("sonnet", 5_000_000)]),
            episode(1., &[("opus", 1_500_000), ("sonnet", 2_500_000)]),
            episode(2., &[("opus", 3_000_000), ("sonnet", 5_000_000)]),
        ];
        let f = figures(&eps);
        assert!((f["opus"] - 2e6).abs() < 1e3, "{f:?}");
        assert!((f["sonnet"] - 1e7).abs() < 1e4, "{f:?}");
    }
    #[test]
    fn a_model_the_episodes_cannot_separate_has_no_figure() {
        // Always the same mix, and neither name has a card: only one of the two can be given a
        // figure (main's rule for a model with no card).
        let f = figures(&[episode(1., &[("opus", 1_000_000), ("sonnet", 1_000_000)]),
            episode(2., &[("opus", 2_000_000), ("sonnet", 2_000_000)])]);
        assert_eq!(f.len(), 1, "{f:?}");
    }
    /// The incident's two episodes (Work's first two, exactly as `claude-token-points.json` kept
    /// them), the tiny third model `minor` and its second count `count`.
    fn work_first_two_with(minor: &str, count: u64) -> Vec<Episode> {
        vec![
            episode(1., &[(minor, 1_029), ("claude-opus-5-5", 40_634_401), ("claude-sonnet-5-5", 18_154_910)]),
            episode(1., &[(minor, count), ("claude-opus-5-5", 17_401_885), ("claude-sonnet-5-5", 4_980_038)]),
        ]
    }
    fn work_first_two(haiku: u64) -> Vec<Episode> {
        work_first_two_with("claude-haiku-5-5", haiku)
    }
    /// Card input dollars a point in an episode: Opus 5.5 $4, Sonnet 5.5 $2, Fable 5.1 $10 and
    /// Haiku 5.5 $0.10 a million (`crate::spend`).
    fn card_dollars(e: &Episode) -> f64 {
        e.tokens.iter().map(|(m, t)| *t as f64 / 1e6 * card_price(m).unwrap()).sum::<f64>() / e.points
    }
    /// **A model the episodes barely saw is read at its card** (Work, 2026-10-28 13:05Z: Ray and
    /// Zach ordered to hand off at 3% of the week). One point each, Opus and Sonnet carrying
    /// 99.99% of the tokens and Haiku 1,029 and 3,151. Two episodes and three models: a pair fits
    /// two episodes exactly whatever the pair, so main's smallest residual chose Sonnet at 24.5M
    /// and Haiku at 3,953 tokens a point, and the 83,446 Haiku tokens a lease's result reported in
    /// the 13:00 slot became 21 weekly points in one minute. Here the dearest point is the first
    /// ($162.54 of Opus and $36.31 of Sonnet input, $198.85), so Haiku at $0.10 a million is
    /// 1.99 billion tokens a point and the 83,446 are 0.00004 points.
    #[test]
    fn a_model_the_episodes_barely_saw_is_read_at_its_card() {
        for haiku in [3_151, 1_500] {
            let eps = work_first_two(haiku);
            let f = figures(&eps);
            assert_eq!(f.keys().collect::<Vec<_>>(), ["claude-haiku-5-5", "claude-opus-5-5", "claude-sonnet-5-5"], "{f:?}");
            let at_card = card_dollars(&eps[0]) / 0.10 * 1e6;
            assert!((f["claude-haiku-5-5"] / at_card - 1.).abs() < 0.05, "Haiku, 0.005% of the tokens, is not at its card {at_card}: {f:?}");
            assert!(83_446. / f["claude-haiku-5-5"] < 1e-4, "{f:?}");
        }
    }
    /// **A switch of model does not take the card away** (the second review of 8e6574d53: a
    /// rule read off the cheapest episode alone fell to 0 when that episode was Sonnet alone,
    /// and Haiku went back to 4,233 tokens a point). The incident's two episodes, then a point of
    /// Sonnet alone (20M tokens, with and without 1,000 Opus): Haiku stays at its card.
    #[test]
    fn a_switch_of_model_does_not_take_the_card_away() {
        for extra in [vec![("claude-sonnet-5-5", 20_000_000)], vec![("claude-sonnet-5-5", 20_000_000), ("claude-opus-5-5", 1_000)]] {
            let mut eps = work_first_two(3_151);
            eps.push(episode(1., &extra));
            let f = figures(&eps);
            let at_card = card_dollars(&eps[0]) / 0.10 * 1e6;
            assert!((f["claude-haiku-5-5"] / at_card - 1.).abs() < 0.05, "{f:?}");
            assert!(83_446. / f["claude-haiku-5-5"] < 1e-4, "{f:?}");
        }
    }
    /// **A model the episodes barely saw is read at its card, not at the worst case its card
    /// allows** (the third review, rv-20261029T115538Z, finding 1, its own fixture). The incident's
    /// two episodes with Fable 5.1 in Haiku's place. d22fc43f8 held Fable at its worst-case bound,
    /// 40,634,401 / 275 = 147,761 tokens a point (its dearest bucket on its dearest card over Opus
    /// 5.5's cache reads), so a late 3M-token Fable `result` was 20.3 points a minute. At $10 a
    /// million against the dearest point's $198.85, its card is 19.89M tokens a point (its list
    /// ratio to Opus, 2.5, puts it at 16M at a true 40M Opus point), and the two episodes move it
    /// to 16.70M: the 3M are 0.180 points, under the 0.2 of the true point 98.
    #[test]
    fn a_model_the_episodes_barely_saw_is_read_at_its_card_not_its_worst_case() {
        let eps = work_first_two_with("claude-fable-5-1", 3_151);
        let f = figures(&eps);
        let at_card = card_dollars(&eps[0]) / 10. * 1e6;
        assert!(f["claude-fable-5-1"] <= at_card * (1. + 1e-9) && f["claude-fable-5-1"] > 15e6, "card {at_card}: {f:?}");
        assert!(3e6 / f["claude-fable-5-1"] < 0.2, "{f:?}");
    }
    /// **A model with an episode of its own keeps what it measured, whatever its bucket mix** (the
    /// second review of 0e2a10356, its own fixture): 40M Opus cache reads for one point and 4M
    /// Sonnet input for the next, $8 of card each. Sonnet is 4M a point, as main measures it,
    /// though its card at the dearest point says 80M; Opus is 40M.
    #[test]
    fn a_model_with_an_episode_of_its_own_keeps_what_it_measured() {
        let f = figures(&[episode(1., &[("claude-opus-5-5", 40_000_000)]), episode(1., &[("claude-sonnet-5-5", 4_000_000)])]);
        assert!((f["claude-sonnet-5-5"] / 4e6 - 1.).abs() < 0.1, "{f:?}");
        assert!(8e6 / f["claude-sonnet-5-5"] > 11. / 6., "8M Sonnet in a minute must be over 1.83 a minute (point 88): {f:?}");
        assert!((f["claude-opus-5-5"] / 40e6 - 1.).abs() < 0.1, "{f:?}");
    }
    /// **A model the episodes measure keeps its figure beside a pair they cannot separate** (the
    /// second review of d22fc43f8, its own fixture). Opus and Fable always in the same ratio (100
    /// to 1), Sonnet apart: (800,000, 8,000, 3,720,000) and (8,400,000, 84,000, 1,060,000), one
    /// point each, Sonnet at 4M a point. d22fc43f8 put the Opus it could not fit with Sonnet and
    /// read all three at 7.04M; at a card spread of 1 the card at the dearest point (18.28M) pulled
    /// Sonnet to 11.3M. Here Sonnet stays within 5% of 4M, as main measures it.
    #[test]
    fn a_model_the_episodes_measure_keeps_its_figure_beside_a_pair_they_cannot_separate() {
        let f = figures(&[
            episode(1., &[("claude-opus-5-5", 800_000), ("claude-fable-5-1", 8_000), ("claude-sonnet-5-5", 3_720_000)]),
            episode(1., &[("claude-opus-5-5", 8_400_000), ("claude-fable-5-1", 84_000), ("claude-sonnet-5-5", 1_060_000)]),
        ]);
        assert!((f["claude-sonnet-5-5"] / 4e6 - 1.).abs() < 0.05, "{f:?}");
        assert_eq!(f.len(), 3, "{f:?}");
    }
    /// **A dear minority is read at its own price, never at a cheaper model's** (the third review,
    /// finding 2, its own fixture): four one-point mixes of Opus 5.5, Sonnet 5.5, Fable 5.1 and
    /// Haiku 5.5 at their list ratios (40M, 80M, 16M and 1,600M tokens a point), each a whole
    /// point after rounding. d22fc43f8 dropped Fable from the fit and read it with Opus, at
    /// 111.8M a point, seven times its true 16M. Here it is near its card at the dearest point
    /// (the second, $225.01: 22.5M), dearer than Opus.
    #[test]
    fn a_dear_minority_is_read_at_its_own_price() {
        let eps = four_mixes();
        let f = figures(&eps);
        assert_eq!(f.len(), 4, "{f:?}");
        assert!(f["claude-fable-5-1"] < 33e6, "20M Fable tokens must be more than 0.6 points: {f:?}");
        assert!(f["claude-fable-5-1"] < f["claude-opus-5-5"] / 1.5, "Fable read about as cheap as Opus: {f:?}");
    }
    /// The third review's four mixes (finding 2): Opus, Sonnet, Fable and Haiku tokens, one
    /// point each.
    fn four_mixes() -> Vec<Episode> {
        [(34_000_000, 14_500_000, 700_000, 100_000), (40_000_000, 21_500_000, 2_200_000, 90_000),
            (18_000_000, 19_500_000, 1_900_000, 110_000), (18_500_000, 22_500_000, 1_000_000, 160_000)]
            .iter().map(|(o, s, fable, h)| episode(1., &[("claude-opus-5-5", *o), ("claude-sonnet-5-5", *s),
                ("claude-fable-5-1", *fable), ("claude-haiku-5-5", *h)])).collect()
    }
    /// **A mix the fit explains is not charged twice** (the second review of 8e6574d53: a
    /// fallback figure added on top of a fit that already explained every point). 30M Opus and
    /// 30M Sonnet for one point, then 60M and 60M for two: the same mix again is one point.
    #[test]
    fn a_mix_the_fit_explains_is_not_charged_twice() {
        let f = figures(&[episode(1., &[("claude-opus-5-5", 30_000_000), ("claude-sonnet-5-5", 30_000_000)]),
            episode(2., &[("claude-opus-5-5", 60_000_000), ("claude-sonnet-5-5", 60_000_000)])]);
        let point = 30e6 / f["claude-opus-5-5"] + 30e6 / f["claude-sonnet-5-5"];
        assert!((point - 1.).abs() < 1e-9, "{point}: {f:?}");
    }
    /// **A model the account used only a little is still read** (Tom's review of 1ff534f44, the
    /// second review of 3cdd051c8). Work's first six episodes: Fable is in three, 8,760,602
    /// tokens in all. Main's fit alone gives it about 7.98M a point; its card at the dearest point
    /// (the sixth, 54,593,932 Opus and 291,147 Sonnet tokens, $218.96) is 21.9M. It is read
    /// between the two, and Haiku, at its card.
    #[test]
    fn a_minority_model_is_read_between_what_the_episodes_measured_and_its_card() {
        let mut work = work_first_two(3_151);
        work.extend([
            episode(1., &[("claude-fable-5-1", 2_939_271), ("claude-haiku-5-5", 83_446), ("claude-opus-5-5", 27_210_374), ("claude-sonnet-5-5", 3_477_888)]),
            episode(1., &[("claude-fable-5-1", 1_667_801), ("claude-opus-5-5", 13_890_036), ("claude-sonnet-5-5", 2_311_295)]),
            episode(1., &[("claude-fable-5-1", 4_153_530), ("claude-opus-5-5", 36_103_747), ("claude-sonnet-5-5", 5_854_694)]),
            episode(1., &[("claude-opus-5-5", 54_593_932), ("claude-sonnet-5-5", 291_147)]),
        ]);
        let f = figures(&work);
        assert_eq!(f.len(), 4, "{f:?}");
        let dearest = work.iter().map(card_dollars).fold(0., f64::max);
        assert!((dearest - 218.958).abs() < 1e-3, "{dearest}");
        assert!((7.9e6..dearest / 10. * 1e6).contains(&f["claude-fable-5-1"]), "{f:?}");
        assert!((f["claude-haiku-5-5"] / (dearest / 0.10 * 1e6) - 1.).abs() < 1e-3, "{f:?}");
        // No episode at all: nothing to price anything at.
        assert!(figures(&[]).is_empty());
    }
    /// **A dear minority the episodes measure is measured exactly** (the second reviews of
    /// 3cdd051c8 and 8e6574d53). Opus 5.5 at 40M a point and Fable 5.1 at 16M (its list: 2.5
    /// times as dear) or 8M (5 times), Fable 2% to 4% of each of six one-point episodes.
    #[test]
    fn a_dear_minority_the_episodes_measure_keeps_its_figure() {
        for fable in [16e6, 8e6] {
            let eps: Vec<Episode> = [0.02_f64, 0.025, 0.03, 0.035, 0.04, 0.03].iter().map(|share| {
                let all = 1. / ((1. - share) / 40e6 + share / fable);
                episode(1., &[("claude-opus-5-5", (all * (1. - share)).round() as u64), ("claude-fable-5-1", (all * share).round() as u64)])
            }).collect();
            let f = figures(&eps);
            assert!((f["claude-fable-5-1"] / fable - 1.).abs() < 1e-3, "{f:?}");
            assert!((f["claude-opus-5-5"] / 40e6 - 1.).abs() < 1e-3, "{f:?}");
        }
    }
    /// **Every model the episodes did measure keeps its figure.** Two episodes on one model each
    /// (Opus 30M = 1 point, Sonnet 60M = 1 point): each spent a whole point by itself, so both are
    /// exact. And one episode of a mix (Work's first): one episode cannot tell the three apart,
    /// so each is read at its card, and together they are exactly the episode's one point.
    #[test]
    fn every_model_the_episodes_measured_keeps_its_figure() {
        let f = figures(&[episode(1., &[("opus", 30_000_000)]), episode(1., &[("sonnet", 60_000_000)])]);
        assert!((f["opus"] - 3e7).abs() < 1. && (f["sonnet"] - 6e7).abs() < 1., "{f:?}");
        let first = &work_first_two(3_151)[..1];
        let f = figures(first);
        assert_eq!(f.len(), 3, "{f:?}");
        let dollars = card_dollars(&first[0]);
        for (model, price) in [("claude-opus-5-5", 4.), ("claude-sonnet-5-5", 2.), ("claude-haiku-5-5", 0.10)] {
            assert!((f[model] / (dollars / price * 1e6) - 1.).abs() < 1e-9, "{model}: {f:?}");
        }
        let point: f64 = first[0].tokens.iter().map(|(m, t)| *t as f64 / f[m]).sum();
        assert!((point - 1.).abs() < 1e-9, "{point}: {f:?}");
    }
    /// **The spend of a lease, its unseen output included** (§118). The frames and `result` the
    /// app captured on 2026-10-07 (`native.rs`, `a_leases_result_totals_count_the_output_its_
    /// assistant_frames_never_show`): two Opus 5.5 messages streamed with 1 output token each,
    /// and `modelUsage` saying 1,990. Each request keeps its own buckets (the helper's cache write
    /// has no split, so it is a 5-minute write), and the remainder is the 1,988 output tokens.
    /// By hand at Opus 5.5's rates: input 10 x $4 + output 1,990 x $20 + cache reads 2,989,000 x
    /// $0.20 + 5-minute writes 9,000 x $5 = 40 + 39,800 + 597,800 + 45,000 = $0.68264.
    #[test]
    fn a_lease_prices_its_frames_and_the_output_only_its_result_shows() {
        let frames = [
            r#"{"type":"assistant","message":{"id":"msg_rich","model":"claude-opus-5-5","usage":{"input_tokens":0,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":1999000}},"parent_tool_use_id":null}"#,
            r#"{"type":"assistant","message":{"id":"msg_helper","model":"claude-opus-5-5","usage":{"input_tokens":10,"output_tokens":1,"cache_creation_input_tokens":9000,"cache_read_input_tokens":990000}},"parent_tool_use_id":"toolu_1"}"#,
            r#"{"type":"assistant","message":{"id":"msg_helper","model":"claude-opus-5-5","usage":{"input_tokens":10,"output_tokens":1,"cache_creation_input_tokens":9000,"cache_read_input_tokens":990000}},"parent_tool_use_id":"toolu_1"}"#,
            r#"{"type":"result","session_id":"s1","modelUsage":{"claude-opus-5-5":{"inputTokens":10,"outputTokens":1990,"cacheReadInputTokens":2989000,"cacheCreationInputTokens":9000,"contextWindow":1000000}}}"#,
            // A second result with nothing new adds nothing.
            r#"{"type":"result","session_id":"s1","modelUsage":{"claude-opus-5-5":{"inputTokens":10,"outputTokens":1990,"cacheReadInputTokens":2989000,"cacheCreationInputTokens":9000,"contextWindow":1000000}}}"#,
        ];
        let mut tally = Tally::default();
        let mut kept: HashMap<String, crate::spend::Request> = HashMap::new();
        for f in frames {
            for u in tally.frame(&serde_json::from_str(f).unwrap(), 1_000) { kept.insert(u.id.clone(), u.request); }
        }
        assert_eq!(kept.len(), 3, "two messages and one remainder: {:?}", kept.keys());
        let rest = &kept["s1:result-1:claude-opus-5-5"];
        assert_eq!(rest.buckets, crate::spend::Buckets { output: 1988, ..Default::default() });
        // Two messages were open, on one card: the remainder is a correction on that card.
        assert!(rest.correction && !rest.unmatched);
        let total: f64 = kept.values().map(|r| r.price().unwrap()).sum();
        assert!((total - 0.68264).abs() < 1e-12, "the lease priced to {total}, by hand $0.68264");
    }

    fn frame(id: &str, model: &str, input: u64, output: u64, read: u64, parent: Option<&str>) -> Value {
        serde_json::json!({"type": "assistant", "parent_tool_use_id": parent, "message": {"id": id, "model": model,
            "usage": {"input_tokens": input, "output_tokens": output, "cache_read_input_tokens": read,
                "cache_creation_input_tokens": 0, "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 0}}}})
    }
    fn result(model: &str, input: u64, output: u64, read: u64) -> Value {
        serde_json::json!({"type": "result", "session_id": "s1", "modelUsage": {model: {"inputTokens": input,
            "outputTokens": output, "cacheReadInputTokens": read, "cacheCreationInputTokens": 0}}})
    }
    /// Frames through one lease's `Tally` into a fresh spend ledger, as the app does.
    fn spend_of(frames: &[(Value, u64)]) -> (crate::quota::tests::Scratch, crate::spend::ledger::Ledger) {
        let root = crate::quota::tests::Scratch::new();
        let ledger = crate::spend::ledger::Ledger::open(&root.path().join("spend"));
        let mut tally = Tally::default();
        for (f, at) in frames {
            ledger.add(tally.frame(f, *at).into_iter().map(|u| u.request).collect(), *at);
        }
        (root, ledger)
    }

    /// **A lease's output is priced on its own request** (the second review of fc1d7bb31, findings
    /// 1 and 2, its own fixture; each case by hand at the published rates):
    /// - ONE REQUEST: Opus 5.5, 1,000 input, 10,000 cache reads; the frame says 1 output, the result
    ///   101. The remainder IS that request: 1,000 x $4 + 101 x $20 + 10,000 x $0.20 = $0.00802, one
    ///   request, and it is the last request (11,101 tokens), not a 100-token correction.
    /// - MIDNIGHT: the same request starting a second before midnight, its result a second after:
    ///   all of it is yesterday's (today: no request, no dollars).
    /// - MIXED HAIKU, the lead streamed: a long-context Haiku 5.5 lead (120,000 input, so the higher
    ///   card) whose `message_delta` says 10,001 output, and a short helper (1,000 input, 1 output).
    ///   The lead at its final size: 120,000 x $0.50 + 10,001 x $2.50 = $0.0850025, the helper
    ///   1,000 x $0.10 + 1 x $0.50 = $0.0001005: $0.085103, nothing left unpriced.
    /// - MIXED HAIKU, nothing streamed: the result's 10,000 more output cannot be put on either
    ///   card, so it is counted and left unpriced (an explicit gap), and priced is only what the
    ///   frames showed: $0.0600025 + $0.0001005 = $0.060103.
    #[test]
    fn a_leases_output_is_priced_on_the_request_it_came_from() {
        let at = crate::spend::stamp("2026-10-10T01:00:00Z").unwrap();
        let opus = "claude-opus-5-5";
        let (_a, one) = spend_of(&[(frame("one", opus, 1000, 1, 10_000, None), at), (result(opus, 1000, 101, 10_000), at + 1000)]);
        let t = one.totals(0);
        assert_eq!(t.requests, 1, "a remainder with one message to belong to is that message");
        assert!((t.cost - 0.00802).abs() < 1e-12, "{}", t.cost);
        let last = crate::spend::ledger::LastRequest::of(&one.last().unwrap());
        assert_eq!(last.tokens, 11_101);
        assert!((last.cost.unwrap() - 0.00802).abs() < 1e-12);

        let midnight = crate::spend::stamp("2026-10-11T00:00:00Z").unwrap();
        let (_b, crossing) = spend_of(&[(frame("late", opus, 1000, 1, 10_000, None), midnight - 1000),
            (result(opus, 1000, 101, 10_000), midnight + 1000)]);
        assert_eq!(crossing.totals(midnight).requests, 0);
        assert!(crossing.totals(midnight).cost.abs() < 1e-12, "{}", crossing.totals(midnight).cost);

        let haiku = "claude-haiku-5-5";
        let lead_start = serde_json::json!({"type": "stream_event", "parent_tool_use_id": null,
            "event": {"type": "message_start", "message": {"id": "lead", "model": haiku}}});
        let lead_end = serde_json::json!({"type": "stream_event", "parent_tool_use_id": null,
            "event": {"type": "message_delta", "usage": {"output_tokens": 10_001}}});
        let (_c, streamed) = spend_of(&[(lead_start, at), (frame("lead", haiku, 120_000, 1, 0, None), at),
            (lead_end, at + 500), (frame("helper", haiku, 1000, 1, 0, Some("toolu_1")), at + 600),
            (result(haiku, 121_000, 10_002, 0), at + 1000)]);
        let t = streamed.totals(0);
        assert_eq!((t.requests, t.unpriced_tokens), (2, 0));
        assert!((t.cost - 0.085103).abs() < 1e-12, "{}", t.cost);

        let (_d, unstreamed) = spend_of(&[(frame("lead", haiku, 120_000, 1, 0, None), at),
            (frame("helper", haiku, 1000, 1, 0, Some("toolu_1")), at + 1), (result(haiku, 121_000, 10_002, 0), at + 1000)]);
        let t = unstreamed.totals(0);
        assert_eq!((t.requests, t.unpriced_tokens, t.unpriced_requests), (2, 10_000, 0));
        assert!((t.cost - 0.060103).abs() < 1e-12, "{}", t.cost);
    }

    /// **A remainder two requests share is dated by its own requests, never moved into a later
    /// period** (the second review of 6c21269e6, earlier finding 1 still open, its own fixture). An
    /// Opus 5.5 lead a few seconds before midnight and a helper a second after, each 1,000 input,
    /// 1 output, 10,000 cache reads; the result says 10,002 output. The 10,000 more belong to one
    /// or both, which nothing says, so the remainder is counted only in the periods both requests
    /// fall in: All time has it, Today does not. By hand: Today is the helper alone, 1,000 x $4 +
    /// 1 x $20 + 10,000 x $0.20 = $0.00602 (dated at the latest request, Today read $0.20602, the
    /// 10,000 x $20 = $0.20 of a remainder that may be yesterday's moved into today); All time is
    /// both requests and the remainder, 2 x $0.00602 + $0.20 = $0.21204.
    #[test]
    fn a_shared_remainder_is_never_dated_after_its_earliest_request() {
        let opus = "claude-opus-5-5";
        let midnight = crate::spend::stamp("2026-10-11T00:00:00Z").unwrap();
        let (_a, crossing) = spend_of(&[(frame("yesterday-lead", opus, 1000, 1, 10_000, None), midnight - 10_000),
            (frame("today-helper", opus, 1000, 1, 10_000, Some("toolu_1")), midnight + 1000),
            (result(opus, 2000, 10_002, 20_000), midnight + 2000)]);
        let today = crossing.totals(midnight);
        assert!((today.cost - 0.00602).abs() < 1e-12, "Today read ${:.8}, by hand $0.00602000", today.cost);
        assert_eq!((today.requests, today.unpriced_tokens), (1, 0));
        let all = crossing.totals(0);
        assert!((all.cost - 0.21204).abs() < 1e-12, "All time read ${:.8}, by hand $0.21204000", all.cost);
        assert_eq!(all.requests, 2);
    }

    /// **A late lump counts at most `LATE_POINTS_MAX` in the live minute** (main's late-frame
    /// step bound, where the card prior does not make it moot). Opus alone at 30M tokens a point;
    /// in the live minute one streamed message of 30M (1 point) and one `result` remainder of
    /// 90M Opus tokens no frame showed (3 points by its figure): 1 + min(3, 1) = 2 points a minute.
    #[test]
    fn a_late_lump_counts_at_most_late_points_max_in_the_live_minute() {
        let mut t = Track::with_episodes(vec![episode(1., &[("claude-opus-5-5", 30_000_000)]), episode(1., &[("claude-opus-5-5", 30_000_000)])]);
        let now = 1_000_000;
        let use_of = |id: &str, tokens: u64, late: u64| Use { id: id.into(), at: now - 10_000, model: "claude-opus-5-5".into(),
            tokens, output: 0, request: crate::spend::Request::default(), late };
        t.add_use(&use_of("streamed", 30_000_000, 0));
        t.add_use(&use_of("lease:result-1:claude-opus-5-5", 90_000_000, 90_000_000));
        let per_minute = t.speed(now) * LIVE_MS as f64;
        assert!((per_minute - (1. + LATE_POINTS_MAX)).abs() < 1e-9, "{per_minute} points a minute");
    }
    #[test]
    fn a_streamed_message_counts_once_at_its_largest_copy() {
        let mut t = Track::default();
        t.add("m1", 1_000, "opus", 100, 1);
        t.add("m1", 1_000, "opus", 500, 9);
        t.add("m1", 1_000, "opus", 100, 1);
        assert_eq!(t.events["m1"].2, 500);
    }
}
