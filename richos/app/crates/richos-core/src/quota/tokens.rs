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
//!   `token-track.py`'s total tokens / total points. A model the episodes cannot tell apart from
//!   another has no figure yet, and neither has a model the episodes barely saw: besides the
//!   model that spent the most, one that spent fewer tokens over them than the cheapest whole
//!   point the account was measured at (its own few thousand tokens fit any points at all).
//! - **The speed** (`Track::speed`): over the last minute, each measured model's tokens divided
//!   by its tokens per point, summed. The quota service takes the larger of it and the
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
    /// Message id -> (stamped at, model, tokens, output tokens). The largest copy wins.
    events: HashMap<String, (u64, String, u64, u64)>,
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
        self.add(&found.id, found.at, &found.model, found.tokens, found.output);
    }

    /// One assistant message's usage. A later, larger copy of the same message replaces it.
    pub fn add(&mut self, id: &str, at: u64, model: &str, tokens: u64, output: u64) {
        if self.events.get(id).is_some_and(|old| old.3 > output) { return; }
        self.events.insert(id.to_string(), (at, model.to_string(), tokens, output));
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
                            for (stamped, model, count, _) in self.events.values() {
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
    /// tokens divided by its tokens per point. A model with no figure adds nothing.
    pub fn speed(&self, now: u64) -> f64 {
        let per_point = figures(&self.episodes);
        let points: f64 = self.events.values()
            .filter(|e| e.0 + LIVE_MS > now && e.0 <= now)
            .filter_map(|e| per_point.get(&e.1).map(|p| e.2 as f64 / p))
            .sum();
        points / LIVE_MS as f64
    }
}

/// One assistant message's usage: input + output + cache write + cache read, by model, as
/// `token-track.py` counts it, and when it was stamped. `request` keeps what that sum throws
/// away and pricing needs (the API-equivalent spend, §118, `crate::spend`): the five buckets,
/// the two cache writes apart, the effort, fast mode and US-only inference.
#[derive(Clone, Debug, PartialEq)]
pub struct Use { pub id: String, pub at: u64, pub model: String, pub tokens: u64, pub output: u64, pub request: crate::spend::Request }

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
    Some(Use { id, at, model, tokens, output, request })
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
            output: kept.buckets.output, request: kept })
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
                            let mut request = only.clone();
                            request.buckets.add(&rest);
                            out.extend(self.count(request, true));
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
        Use { id, at: request.at, model: model.to_string(), tokens: rest.total(), output: 0, request }
    }
}

/// Tokens per weekly point for each model the episodes measure (see the module comment).
pub fn figures(episodes: &[Episode]) -> BTreeMap<String, f64> {
    let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
    for episode in episodes {
        for (model, count) in &episode.tokens { *totals.entry(model).or_insert(0) += count; }
    }
    let mut models: Vec<(&str, u64)> = totals.into_iter().filter(|(_, t)| *t > 0).collect();
    models.sort_by_key(|m| std::cmp::Reverse(m.1));
    // **A model the episodes barely saw is not measured** (Work, 2026-10-28 13:05Z: Ray and Zach
    // ordered to hand off at 3% of the week). The fit is blind to how many tokens a model has:
    // with Work's first two episodes, Haiku's 1,029 and 3,151 tokens (Claude Code's own Haiku
    // calls, 0.005% of the 81.2M) fitted the two points as well as Opus's 58M did, and were given
    // 3,953 tokens a point. The next 83,446 Haiku tokens were 21 points in one minute, and the
    // week's handoff point fell to 0. So besides the model that spent the most, only a model
    // that spent at least as many tokens, over the kept episodes, as the cheapest whole point the
    // account was ever measured at is fitted: the percentage only says whole points, and fewer
    // tokens than any measured point have never been seen to move it. The rest add nothing, like
    // any model with no figure, so the percentage speed stays the floor for them.
    let cheapest = episodes.iter().filter(|e| e.points > 0.)
        .map(|e| e.tokens.values().sum::<u64>() as f64 / e.points).filter(|t| *t > 0.).fold(f64::INFINITY, f64::min);
    let most = models.first().map(|m| m.0);
    models.retain(|(model, tokens)| Some(*model) == most || *tokens as f64 >= cheapest);
    models.truncate(MAX_MODELS);
    // One row per episode with tokens: each model's tokens in millions, the points, the weight.
    let rows: Vec<(Vec<f64>, f64, f64)> = episodes.iter().filter_map(|e| {
        let all: u64 = e.tokens.values().sum();
        (all > 0 && e.points > 0.).then(|| (
            models.iter().map(|(m, _)| e.tokens.get(*m).copied().unwrap_or(0) as f64 / 1e6).collect(),
            e.points, 1e6 / all as f64))
    }).collect();
    // Non-negative least squares by trying every subset of models: the best fit whose every
    // coefficient is positive.
    let mut best: Option<(f64, Vec<usize>, Vec<f64>)> = None;
    for mask in 1u32..(1 << models.len()) {
        let pick: Vec<usize> = (0..models.len()).filter(|i| mask >> i & 1 == 1).collect();
        let k = pick.len();
        let mut system = vec![vec![0.; k + 1]; k];
        for (x, points, weight) in &rows {
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
    let Some((_, pick, per_million)) = best else { return BTreeMap::new() };
    pick.into_iter().zip(per_million).map(|(i, c)| (models[i].0.to_string(), 1e6 / c)).collect()
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
        // Always the same mix: only one of the two can be given a figure.
        let f = figures(&[episode(1., &[("opus", 1_000_000), ("sonnet", 1_000_000)]),
            episode(2., &[("opus", 2_000_000), ("sonnet", 2_000_000)])]);
        assert_eq!(f.len(), 1, "{f:?}");
    }
    /// **A model the episodes barely saw is not given a figure by fitting their noise** (Work,
    /// 2026-10-28 13:05Z: Ray and Zach ordered to hand off at 3% of the week). Work's first two
    /// episodes, exactly as `claude-token-points.json` kept them: one point each, Opus and Sonnet
    /// carrying 99.99% of the tokens and Haiku 1,029 and 3,151. Two episodes and three models: a
    /// pair fits two episodes exactly whatever the pair, so the smallest residual chose Sonnet at
    /// 24.5M and Haiku at 3,953 tokens a point (Opus, 99% of the use, no figure at all), and the
    /// 83,446 Haiku tokens a lease's result reported in the 13:00 slot (Claude Code's own Haiku
    /// calls, in no assistant frame) became 21 weekly points in one minute. The cheapest point
    /// Work was measured at is its second episode, 22,385,074 tokens; Haiku's 4,180 in all are
    /// far from it, so Haiku is not fitted. Sonnet (23,134,948) is, and the best positive fit is
    /// then Opus alone: 1 / the weighted least-squares fit of both points, 28.3M a point.
    #[test]
    fn a_model_the_episodes_barely_saw_gets_no_figure_from_their_noise() {
        for haiku in [3_151, 1_500] {
            let work = [
                episode(1., &[("claude-haiku-5-5", 1_029), ("claude-opus-5-5", 40_634_401), ("claude-sonnet-5-5", 18_154_910)]),
                episode(1., &[("claude-haiku-5-5", haiku), ("claude-opus-5-5", 17_401_885), ("claude-sonnet-5-5", 4_980_038)]),
            ];
            let f = figures(&work);
            assert!(!f.contains_key("claude-haiku-5-5"), "Haiku, 0.005% of the tokens, was given a figure: {f:?}");
            assert_eq!(f.keys().collect::<Vec<_>>(), ["claude-opus-5-5"], "{f:?}");
            assert!((f["claude-opus-5-5"] / 28_336_202. - 1.).abs() < 1e-3, "{f:?}");
            // The same two episodes with Opus and Haiku alone (Haiku is in every lease's result).
            let alone: Vec<Episode> = work.iter().map(|e| Episode { points: e.points,
                tokens: e.tokens.iter().filter(|(m, _)| !m.contains("sonnet")).map(|(m, t)| (m.clone(), *t)).collect() }).collect();
            let f = figures(&alone);
            assert_eq!(f.keys().collect::<Vec<_>>(), ["claude-opus-5-5"], "{f:?}");
        }
    }
    /// **Every model the episodes did measure keeps its figure.** Two episodes on one model each
    /// (Opus 30M = 1 point, Sonnet 60M = 1 point): each spent a whole point by itself, so both are
    /// exact. And one episode of a mix (Work's first): the model that spent the most is measured,
    /// as before, at its tokens for the point, 40,634,401.
    #[test]
    fn every_model_the_episodes_measured_keeps_its_figure() {
        let f = figures(&[episode(1., &[("opus", 30_000_000)]), episode(1., &[("sonnet", 60_000_000)])]);
        assert!((f["opus"] - 3e7).abs() < 1. && (f["sonnet"] - 6e7).abs() < 1., "{f:?}");
        let f = figures(&[episode(1., &[("claude-haiku-5-5", 1_029), ("claude-opus-5-5", 40_634_401), ("claude-sonnet-5-5", 18_154_910)])]);
        assert_eq!(f.keys().collect::<Vec<_>>(), ["claude-opus-5-5"], "{f:?}");
        assert!((f["claude-opus-5-5"] - 40_634_401.).abs() < 1., "{f:?}");
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

    #[test]
    fn a_streamed_message_counts_once_at_its_largest_copy() {
        let mut t = Track::default();
        t.add("m1", 1_000, "opus", 100, 1);
        t.add("m1", 1_000, "opus", 500, 9);
        t.add("m1", 1_000, "opus", 100, 1);
        assert_eq!(t.events["m1"].2, 500);
    }
}
