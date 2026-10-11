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
//!   another has no figure yet.
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
/// **Bucket by bucket, for the spend** (§118): the same sums kept per bucket, so a remainder is
/// priced as what it is (output, on every capture so far) and not as a blend. A remainder belongs
/// to requests already counted, so it takes the effort, the speed, the geography and Haiku 5.5's
/// card of the lease's latest request on that model; a cache-write remainder has no 5-minute and
/// 1-hour split in `modelUsage`, so it is priced as 5-minute writes (none was ever seen).
#[derive(Debug, Default)]
pub struct Tally {
    /// Each message counted so far, by id: its model and its largest streamed total.
    seen: HashMap<String, (String, u64)>,
    /// Per model, everything this lease has counted: frames plus earlier remainders.
    counted: BTreeMap<String, u64>,
    /// Each message's largest buckets so far, by id.
    seen_buckets: HashMap<String, crate::spend::Buckets>,
    /// Per model (without a date suffix), the buckets counted: frames plus earlier remainders.
    counted_buckets: BTreeMap<String, crate::spend::Buckets>,
    /// Per model, the lease's latest request.
    latest: BTreeMap<String, crate::spend::Request>,
    results: u64,
}

impl Tally {
    /// What to count for one frame from this lease, received at `at`.
    pub fn frame(&mut self, frame: &Value, at: u64) -> Vec<Use> {
        match frame["type"].as_str() {
            Some("assistant") => {
                let Some(found) = message_use(&frame["message"], frame, at) else { return vec![] };
                let old = self.seen.get(&found.id).map_or(0, |(_, tokens)| *tokens);
                if found.tokens > old {
                    *self.counted.entry(found.model.clone()).or_insert(0) += found.tokens - old;
                    self.seen.insert(found.id.clone(), (found.model.clone(), found.tokens));
                }
                let base = crate::spend::base_model(&found.model).to_string();
                let had = self.seen_buckets.entry(found.id.clone()).or_default();
                let grew = had.beyond(&found.request.buckets);
                had.add(&grew);
                self.counted_buckets.entry(base.clone()).or_default().add(&grew);
                self.latest.insert(base, found.request.clone());
                vec![found]
            }
            Some("result") => {
                let Some(models) = frame["modelUsage"].as_object() else { return vec![] };
                self.results += 1;
                let lease = frame["session_id"].as_str().unwrap_or("lease");
                let mut remainders = Vec::new();
                for (model, usage) in models {
                    let count = |field: &str| usage.get(field).and_then(Value::as_u64).unwrap_or(0);
                    let total = count("inputTokens") + count("outputTokens")
                        + count("cacheCreationInputTokens") + count("cacheReadInputTokens");
                    let counted = self.counted.entry(model.clone()).or_insert(0);
                    let tokens = total.saturating_sub(*counted);
                    *counted = (*counted).max(total);
                    let base = crate::spend::base_model(model).to_string();
                    let had = self.counted_buckets.entry(base.clone()).or_default();
                    let writes = count("cacheCreationInputTokens").saturating_sub(had.write_5m + had.write_1h);
                    let rest = crate::spend::Buckets {
                        input: count("inputTokens").saturating_sub(had.input),
                        output: count("outputTokens").saturating_sub(had.output),
                        cache_read: count("cacheReadInputTokens").saturating_sub(had.cache_read),
                        write_5m: writes,
                        write_1h: 0,
                    };
                    had.add(&rest);
                    if tokens == 0 && rest.total() == 0 { continue; }
                    let id = format!("{lease}:result-{}:{model}", self.results);
                    let latest = self.latest.get(&base);
                    let request = crate::spend::Request {
                        id: id.clone(), at, model: model.clone(),
                        effort: latest.and_then(|l| l.effort.clone()),
                        fast: latest.is_some_and(|l| l.fast),
                        us: latest.is_some_and(|l| l.us),
                        buckets: rest,
                        card_prompt: latest.map(|l| l.buckets.prompt()),
                    };
                    remainders.push(Use { id, at, model: model.clone(), tokens, output: 0, request });
                }
                remainders
            }
            _ => vec![],
        }
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
        let total: f64 = kept.values().map(|r| r.price().unwrap()).sum();
        assert!((total - 0.68264).abs() < 1e-12, "the lease priced to {total}, by hand $0.68264");
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
