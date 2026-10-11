//! **The spend ledger**: every request this Mac's Claude Code made, counted once and kept.
//!
//! Two sources, because one of them leaves no file (`report.md`, "Collection and completeness"):
//! - **Transcripts.** Each account folder's `projects/**/*.jsonl`, the whole history on the first
//!   read (9.28 GB, 3,867 files on the CEO's Mac, 2026-10-11) and from then on only the bytes
//!   added since, from offsets kept on disk.
//! - **The app's own leases**, which run with `--no-session-persistence` and write no
//!   transcript: their `assistant` frames and `result` remainders, handed over by the quota
//!   service's sink (`quota::tokens::Sink::take_for_spend`).
//!
//! **One response counts once.** A message is written as several transcript lines, one per
//! content block, and the output grows from copy to copy; a resumed session copies earlier
//! messages into a new file (328 of 311,807 message ids appear in two files). So: the largest copy
//! of a message wins while it is recent ([`RECENT_MS`]), and an all-time set of message-id hashes
//! keeps any later copy from counting again.
//!
//! **What is kept is totals, not requests**: per 15-minute UTC slot, per card ([`Card`]) and
//! effort, the request count and the five buckets. Every time zone's midnight falls on a quarter
//! hour, so a local day, a week from Monday and a month are exact sums of slots; and requests on
//! one card are priced exactly by pricing their summed buckets, because the price is linear in
//! each bucket.
use super::{Buckets, Card, Family, Request};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

/// The totals' time slot.
pub const SLOT_MS: u64 = 15 * 60_000;
/// How long a counted message can still be replaced by a larger copy of itself.
pub const RECENT_MS: u64 = 60 * 60_000;
/// How often the totals are written to disk while requests keep coming.
pub const SAVE_EVERY_MS: u64 = 30_000;

const STATE_FILE: &str = "state.json";
const SEEN_FILE: &str = "seen.bin";

/// One slot's totals on one card and effort.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// The slot's start, Unix milliseconds (a multiple of [`SLOT_MS`]).
    pub slot: u64,
    pub card: Card,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    pub requests: i64,
    pub buckets: Buckets,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    offsets: BTreeMap<PathBuf, u64>,
    recent: Vec<Request>,
    rows: Vec<Row>,
    last: Option<Request>,
    counted: u64,
}

#[derive(Default)]
struct State {
    offsets: BTreeMap<PathBuf, u64>,
    /// Every message ever counted, by id hash: the slot it was counted in and its output, so a
    /// later, larger copy of it adds what it adds (the second review of fc1d7bb31, finding 3).
    seen: HashMap<u64, (u32, u32)>,
    /// Records added since the last save (appended to `seen.bin`, 16 bytes each).
    unsaved_seen: Vec<(u64, u32, u32)>,
    recent: HashMap<String, Request>,
    totals: BTreeMap<(u64, Card, Option<String>), (i64, Buckets)>,
    /// The latest request by its own time, for the live line.
    last: Option<Request>,
    /// Requests counted, ever: the screen compares two readings to know something landed.
    counted: u64,
    dirty: bool,
    saved_at: u64,
}

/// The ledger. Reading files never holds the lock that counting takes, so a lease's frames are
/// never kept waiting by a long first read.
pub struct Ledger {
    dir: PathBuf,
    state: Mutex<State>,
    /// Held for one read of the transcripts at a time.
    reading: Mutex<()>,
    /// Whether the whole history has been read once.
    ready: std::sync::atomic::AtomicBool,
}

/// FNV-1a, 64 bits: a hash that is the same in every build (the set is kept on disk).
fn hash(id: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
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

impl State {
    /// Add (`sign` 1) or take back (`sign` -1) one record's buckets, and `requests` to the count.
    fn put(&mut self, request: &Request, sign: i64, requests: i64) {
        let slot = request.at / SLOT_MS * SLOT_MS;
        let entry = self.totals.entry((slot, request.card(), request.effort.clone())).or_default();
        entry.0 += requests;
        let b = &request.buckets;
        let apply = |total: &mut u64, n: u64| *total = if sign > 0 { *total + n } else { total.saturating_sub(n) };
        apply(&mut entry.1.input, b.input);
        apply(&mut entry.1.output, b.output);
        apply(&mut entry.1.cache_read, b.cache_read);
        apply(&mut entry.1.write_5m, b.write_5m);
        apply(&mut entry.1.write_1h, b.write_1h);
        if entry.0 <= 0 && entry.1.total() == 0 {
            self.totals.remove(&(slot, request.card(), request.effort.clone()));
        }
    }

    fn remember(&mut self, h: u64, at: u64, output: u64) {
        let record = ((at / SLOT_MS) as u32, output.min(u64::from(u32::MAX)) as u32);
        self.seen.insert(h, record);
        self.unsaved_seen.push((h, record.0, record.1));
    }

    /// Count one request: once per message, at its largest copy. While it is recent a larger copy
    /// replaces it; after that, a larger copy adds the output it adds, in the slot it was counted
    /// in. A correction (`Request::correction`) is spend with no request of its own.
    fn count(&mut self, request: Request) {
        let h = hash(&request.id);
        if request.correction {
            if self.seen.contains_key(&h) {
                return;
            }
            self.remember(h, request.at, request.buckets.output);
            self.put(&request, 1, 0);
            self.dirty = true;
            return;
        }
        // A larger copy of a recent message keeps the message's own time, so it stays in the slot
        // (and the day) it was written in, as an older one does below (the review of b80728780).
        let request = match self.recent.get(&request.id) { Some(old) => Request { at: old.at, ..request }, None => request };
        if let Some(old) = self.recent.get(&request.id) {
            if request.buckets.output <= old.buckets.output && request.buckets.total() <= old.buckets.total() {
                return;
            }
            let old = old.clone();
            self.put(&old, -1, -1);
            self.put(&request, 1, 1);
            self.remember(h, old.at, request.buckets.output);
        } else if let Some(&(slot, output)) = self.seen.get(&h) {
            if request.buckets.output <= u64::from(output) {
                return;
            }
            // Counted long ago at a smaller size: only the output this copy adds, where it was counted.
            let at = u64::from(slot) * SLOT_MS;
            let extra = Request {
                at,
                buckets: Buckets { output: request.buckets.output - u64::from(output), ..Buckets::default() },
                card_prompt: Some(request.buckets.prompt()),
                correction: true,
                ..request.clone()
            };
            self.put(&extra, 1, 0);
            self.remember(h, at, request.buckets.output);
            self.dirty = true;
            return;
        } else {
            self.remember(h, request.at, request.buckets.output);
            self.put(&request, 1, 1);
            self.counted += 1;
        }
        if self.last.as_ref().is_none_or(|l| request.at >= l.at) {
            self.last = Some(request.clone());
        }
        self.recent.insert(request.id.clone(), request);
        self.dirty = true;
    }

    fn prune(&mut self, now: u64) {
        self.recent.retain(|_, r| r.at + RECENT_MS > now);
    }
}

impl Ledger {
    /// The ledger kept in `dir` (created on the first save). A damaged state file starts the
    /// count over from the transcripts, which are the record; it is never half-read.
    pub fn open(dir: &Path) -> Ledger {
        let mut state = State::default();
        if let Some(saved) = fs::read(dir.join(STATE_FILE)).ok().and_then(|b| serde_json::from_slice::<Saved>(&b).ok()) {
            let mut seen_bytes = Vec::new();
            let seen_ok = fs::File::open(dir.join(SEEN_FILE)).and_then(|mut f| f.read_to_end(&mut seen_bytes)).is_ok();
            if seen_ok {
                // Later records of one message replace earlier ones: its latest known size.
                state.seen = seen_bytes.chunks_exact(16).map(|c| (
                    u64::from_le_bytes(c[..8].try_into().unwrap()),
                    (u32::from_le_bytes(c[8..12].try_into().unwrap()), u32::from_le_bytes(c[12..].try_into().unwrap())),
                )).collect();
                state.offsets = saved.offsets;
                state.recent = saved.recent.into_iter().map(|r| (r.id.clone(), r)).collect();
                state.totals = saved.rows.into_iter().map(|r| ((r.slot, r.card, r.effort), (r.requests, r.buckets))).collect();
                state.last = saved.last;
                state.counted = saved.counted;
            }
        }
        let ready = !state.offsets.is_empty() || state.counted > 0;
        Ledger { dir: dir.to_path_buf(), state: Mutex::new(state), reading: Mutex::new(()), ready: ready.into() }
    }

    /// Whether the whole history has been read at least once.
    pub fn ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Requests counted, ever.
    pub fn counted(&self) -> u64 {
        self.state.lock().unwrap().counted
    }

    /// The latest request.
    pub fn last(&self) -> Option<Request> {
        self.state.lock().unwrap().last.clone()
    }

    /// Requests from a lease's stream (no transcript holds them).
    pub fn add(&self, requests: Vec<Request>, now: u64) {
        if requests.is_empty() {
            return;
        }
        let mut state = self.state.lock().unwrap();
        for r in requests {
            state.count(r);
        }
        state.prune(now);
    }

    /// Read what the transcripts in `folders` (account folders; each one's `projects/`) gained
    /// since the last read, and save when due. `false` without reading when another read is under
    /// way (the first one takes a while): the caller answers from what is counted.
    pub fn update(&self, folders: &[PathBuf], now: u64) -> bool {
        let Ok(_reading) = self.reading.try_lock() else { return false };
        let mut files = Vec::new();
        for folder in folders {
            transcripts(&folder.join("projects"), &mut files);
        }
        for path in files {
            let Ok(meta) = fs::metadata(&path) else { continue };
            let len = meta.len();
            let mut offset = self.state.lock().unwrap().offsets.get(&path).copied().unwrap_or(0);
            if len < offset {
                offset = 0; // rewritten from the start
            }
            if len == offset {
                continue;
            }
            let Some((read_to, found)) = read_from(&path, offset, len) else { continue };
            let mut state = self.state.lock().unwrap();
            for r in found {
                state.count(r);
            }
            state.offsets.insert(path, read_to);
            state.dirty = true;
            // A file is read to its last complete line, so the copies of an older message are all
            // in hand: only the last hour's messages need to be held as they are.
            state.prune(now);
        }
        let first = !self.ready();
        self.ready.store(true, std::sync::atomic::Ordering::SeqCst);
        self.save(now, first);
        true
    }

    /// Write the totals when they changed and it is due (or `force`).
    pub fn save(&self, now: u64, force: bool) {
        let mut state = self.state.lock().unwrap();
        if !state.dirty || (!force && now < state.saved_at + SAVE_EVERY_MS) {
            return;
        }
        let saved = Saved {
            offsets: state.offsets.clone(),
            recent: state.recent.values().cloned().collect(),
            rows: state.totals.iter().map(|((slot, card, effort), (requests, buckets))| Row {
                slot: *slot, card: card.clone(), effort: effort.clone(), requests: *requests, buckets: *buckets,
            }).collect(),
            last: state.last.clone(),
            counted: state.counted,
        };
        let seen: Vec<u8> = state.unsaved_seen.iter()
            .flat_map(|(h, slot, output)| h.to_le_bytes().into_iter().chain(slot.to_le_bytes()).chain(output.to_le_bytes()))
            .collect();
        let written = (|| -> std::io::Result<()> {
            fs::create_dir_all(&self.dir)?;
            // The ids first: a crash between the two writes then re-reads bytes whose messages
            // are already marked seen, and nothing counts twice.
            fs::OpenOptions::new().create(true).append(true).open(self.dir.join(SEEN_FILE))?.write_all(&seen)?;
            let tmp = self.dir.join(format!("{STATE_FILE}.tmp"));
            fs::write(&tmp, serde_json::to_vec(&saved)?)?;
            fs::rename(&tmp, self.dir.join(STATE_FILE))
        })();
        match written {
            Ok(()) => {
                state.unsaved_seen.clear();
                state.dirty = false;
                state.saved_at = now;
            }
            Err(error) => eprintln!("[richos] spend: the ledger could not be saved ({error})"),
        }
    }

    /// The totals of every request at or after `from` (Unix ms; the UI's local midnight, Monday
    /// or first of the month, each a quarter hour), by family.
    pub fn totals(&self, from: u64) -> Totals {
        let state = self.state.lock().unwrap();
        let mut t = Totals { counted: state.counted, ..Totals::default() };
        let mut families: BTreeMap<Family, FamilyTotal> = BTreeMap::new();
        for ((slot, card, effort), (requests, buckets)) in &state.totals {
            if *slot < from || *requests <= 0 && buckets.total() == 0 {
                continue;
            }
            let requests = (*requests).max(0) as u64;
            t.first_at = Some(t.first_at.map_or(*slot, |f: u64| f.min(*slot)));
            t.requests += requests;
            t.tokens += buckets.total();
            t.cache_read += buckets.cache_read;
            let (Some(cost), Some((family, name))) = (super::price(card, buckets), super::family(&card.model)) else {
                t.unpriced_requests += requests;
                t.unpriced_tokens += buckets.total();
                continue;
            };
            t.cost += cost;
            let f = families.entry(family).or_insert_with(|| FamilyTotal { family, ..FamilyTotal::default() });
            f.cost += cost;
            f.requests += requests;
            *f.names.entry(name).or_default() += cost;
            match effort {
                Some(e) => *f.efforts.entry(e.clone()).or_default() += cost,
                None => f.effort_unrecorded = true,
            }
        }
        t.families = families.into_values().collect();
        t
    }
}

/// The complete lines of `path` from `offset`, and where the read stopped (the end of the last
/// complete line: a line still being written is read next time).
fn read_from(path: &Path, offset: u64, len: u64) -> Option<(u64, Vec<Request>)> {
    let mut file = fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut reader = BufReader::with_capacity(1 << 20, file.take(len - offset));
    let mut at = offset;
    let mut line = Vec::new();
    let mut found = Vec::new();
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line).ok()?;
        if n == 0 || line.last() != Some(&b'\n') {
            break;
        }
        at += n as u64;
        if let Some(r) = Request::of_transcript_line(&line) {
            found.push(r);
        }
    }
    Some((at, found))
}

/// One model mark's share of a period.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyTotal {
    pub family: Family,
    pub cost: f64,
    pub requests: u64,
    /// Each version on record ("Opus 5.5", "Opus 5") and its cost.
    pub names: BTreeMap<String, f64>,
    /// Each recorded effort and its cost.
    pub efforts: BTreeMap<String, f64>,
    /// Whether some of its requests recorded no effort.
    pub effort_unrecorded: bool,
}

/// A period's totals.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub cost: f64,
    pub requests: u64,
    pub tokens: u64,
    pub cache_read: u64,
    /// Requests on a model with no published rate: counted in the requests and tokens, not priced.
    pub unpriced_requests: u64,
    /// Tokens counted and not priced: those requests', and a lease's remainder that could not be
    /// matched to one card (`Card::unmatched`).
    pub unpriced_tokens: u64,
    /// The start of the earliest slot with a request in the period.
    pub first_at: Option<u64>,
    pub families: Vec<FamilyTotal>,
    /// Requests counted, ever, at the time of this reading.
    pub counted: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(id: &str, model: &str, ts: &str, [input, output, read, w5, w1]: [u64; 5]) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","effort":"high","uuid":"u-{id}","message":{{"id":"{id}","model":"{model}","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_read_input_tokens":{read},"cache_creation_input_tokens":{},"cache_creation":{{"ephemeral_5m_input_tokens":{w5},"ephemeral_1h_input_tokens":{w1}}}}}}}}}"#,
            w5 + w1
        ) + "\n"
    }
    /// A scratch folder with an account's `projects/p`, removed when the test ends.
    fn scratch() -> (crate::quota::tests::Scratch, PathBuf) {
        let root = crate::quota::tests::Scratch::new();
        let dir = root.path().to_path_buf();
        fs::create_dir_all(dir.join("account/projects/p")).unwrap();
        (root, dir)
    }
    const NOW: u64 = 1_791_684_000_000; // 2026-10-11T03:20:00Z

    #[test]
    fn a_message_counts_once_across_copies_files_and_reads() {
        let (_root, dir) = scratch();
        let file = dir.join("account/projects/p/a.jsonl");
        // Two copies in one read, the larger second; then a third copy in a later read.
        fs::write(&file, line("m1", "claude-opus-5-5", "2026-10-11T03:10:00Z", [2, 10, 1000, 0, 100])
            + &line("m1", "claude-opus-5-5", "2026-10-11T03:10:01Z", [2, 40, 1000, 0, 100])).unwrap();
        let ledger = Ledger::open(&dir.join("spend"));
        let folders = [dir.join("account")];
        assert!(ledger.update(&folders, NOW));
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        f.write_all(line("m1", "claude-opus-5-5", "2026-10-11T03:10:02Z", [2, 51, 1000, 0, 100]).as_bytes()).unwrap();
        // A resumed session copies m1 into a second file, and adds m2; a half-written line waits.
        fs::write(dir.join("account/projects/p/b.jsonl"), line("m1", "claude-opus-5-5", "2026-10-11T03:10:02Z", [2, 51, 1000, 0, 100])
            + &line("m2", "claude-sonnet-5-5", "2026-10-11T03:12:00Z", [1000, 2000, 50000, 4000, 0])
            + r#"{"type":"assistant","timestamp":"2026-10-11T03:13:00Z","message":{"id":"m3""#).unwrap();
        assert!(ledger.update(&folders, NOW));
        let t = ledger.totals(0);
        assert_eq!(t.requests, 2, "m1 once, m2 once, the unfinished line not yet");
        // m1 at its largest copy: 2*4 + 51*20 + 1000*0.20 + 100*8 = 2,028 -> $0.002028; m2 $0.037.
        assert!((t.cost - (0.002028 + 0.037)).abs() < 1e-12, "{}", t.cost);
        assert_eq!(t.counted, 2);
    }

    /// **The recorded-request fixture, through the whole read**: the transcript file as Claude
    /// Code writes it, read from an account folder, counted once per message and priced, comes to
    /// the hand-computed $0.5234329 of `spend::tests` (nine priced messages, one unpriced).
    #[test]
    fn the_fixture_read_from_an_account_folder_prices_to_the_hand_computed_amount() {
        let (_root, dir) = scratch();
        fs::write(dir.join("account/projects/p/fixture.jsonl"), include_str!("fixture-requests.jsonl")).unwrap();
        let ledger = Ledger::open(&dir.join("spend"));
        assert!(ledger.update(&[dir.join("account")], NOW));
        let t = ledger.totals(0);
        assert_eq!((t.requests, t.unpriced_requests), (10, 1));
        assert!((t.cost - 0.5234329).abs() < 1e-12, "the fixture read from disk prices to {}, by hand $0.5234329", t.cost);
        let haiku = t.families.iter().find(|f| f.family == Family::Haiku).unwrap();
        assert_eq!(haiku.names.keys().cloned().collect::<Vec<_>>(), vec!["Haiku 4.5".to_string(), "Haiku 5.5".to_string()]);
        assert!(haiku.effort_unrecorded, "Haiku 4.5's request recorded no effort");
    }

    /// **A growing copy stays in its message's own time slot** (the review of b80728780, its third
    /// finding, its own case): a message written at 23:59:59 and its larger copy at 00:00:01, read
    /// in two reads while the first is still recent. By hand at Opus 5.5's rates the message is
    /// 1,000 x $4 + 101 x $20 + 10,000 x $0.20 = $0.00802, and all of it is the day before: Today
    /// has no request and no dollars (the copy's later time put all $0.00802 in Today).
    #[test]
    fn a_growing_copy_across_midnight_stays_on_the_day_its_message_was_written() {
        let (_root, dir) = scratch();
        let folders = [dir.join("account")];
        let file = dir.join("account/projects/p/a.jsonl");
        fs::write(&file, line("late", "claude-opus-5-5", "2026-10-10T23:59:59Z", [1000, 1, 10_000, 0, 0])).unwrap();
        let midnight = stamp_ms("2026-10-11T00:00:00Z");
        // read as it is written, so the first copy is still recent when the larger one comes
        let ledger = Ledger::open(&dir.join("spend"));
        ledger.update(&folders, midnight + 1000);
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        f.write_all(line("late", "claude-opus-5-5", "2026-10-11T00:00:01Z", [1000, 101, 10_000, 0, 0]).as_bytes()).unwrap();
        ledger.update(&folders, midnight + 2000);
        let today = ledger.totals(midnight);
        assert!(today.cost.abs() < 1e-12, "Today read ${:.8}, by hand $0.00000000", today.cost);
        assert_eq!(today.requests, 0);
        let all = ledger.totals(0);
        assert!((all.cost - 0.00802).abs() < 1e-12, "All time read ${:.8}, by hand $0.00802000", all.cost);
        assert_eq!(all.requests, 1);
    }

    /// **A larger copy found later adds what it adds** (the second review of fc1d7bb31, finding 3,
    /// its own fixture): a message counted a day ago at 1 output token, then found again, at 101,
    /// in another transcript after a relaunch. By hand at Opus 5.5's rates: 1,000 x $4 + 101 x $20
    /// + 10,000 x $0.20 = $0.00802 (the first copy alone: $0.00602). Still one request.
    #[test]
    fn a_larger_copy_found_after_the_first_is_long_counted_adds_its_output() {
        let (_root, dir) = scratch();
        let folders = [dir.join("account")];
        fs::write(dir.join("account/projects/p/a.jsonl"), line("historical", "claude-opus-5-5", "2026-10-10T01:00:00Z", [1000, 1, 10_000, 0, 0])).unwrap();
        let ledger = Ledger::open(&dir.join("spend"));
        ledger.update(&folders, NOW);
        assert!((ledger.totals(0).cost - 0.00602).abs() < 1e-12);
        drop(ledger);
        let again = Ledger::open(&dir.join("spend"));
        fs::write(dir.join("account/projects/p/b.jsonl"), line("historical", "claude-opus-5-5", "2026-10-10T01:00:00Z", [1000, 101, 10_000, 0, 0])).unwrap();
        again.update(&folders, NOW);
        let t = again.totals(0);
        assert!((t.cost - 0.00802).abs() < 1e-12, "the larger copy reconciles: {} against $0.00802 by hand", t.cost);
        assert_eq!(t.requests, 1, "a larger copy is the same request");
        // In the slot the message was counted in, not on the day the copy was read.
        assert_eq!(again.totals(stamp_ms("2026-10-10T02:00:00Z")).requests, 0);
        assert!(again.totals(stamp_ms("2026-10-10T02:00:00Z")).cost.abs() < 1e-12);
        // And a third read of either file adds nothing.
        again.update(&folders, NOW);
        assert!((again.totals(0).cost - 0.00802).abs() < 1e-12);
    }

    /// **A correction is spend, never a request** (the second review of fc1d7bb31, finding 2): it
    /// adds its dollars, adds no request, and is never the latest request on the live line.
    #[test]
    fn a_correction_adds_spend_but_no_request_and_is_never_the_last_request() {
        let (_root, dir) = scratch();
        let ledger = Ledger::open(&dir.join("spend"));
        let request = Request::of_transcript_line(line("one", "claude-opus-5-5", "2026-10-11T03:00:00Z", [1000, 1, 10_000, 0, 0]).as_bytes()).unwrap();
        let correction = Request {
            id: "lease:result-1:claude-opus-5-5".into(),
            at: request.at + 1000,
            buckets: Buckets { output: 100, ..Buckets::default() },
            correction: true,
            ..request.clone()
        };
        ledger.add(vec![request, correction.clone(), correction], NOW);
        let t = ledger.totals(0);
        assert_eq!((t.requests, t.counted), (1, 1));
        assert!((t.cost - 0.00802).abs() < 1e-12, "{}", t.cost);
        assert_eq!(ledger.last().unwrap().id, "one");
    }

    #[test]
    fn the_totals_survive_a_reopen_and_nothing_is_read_twice() {
        let (_root, dir) = scratch();
        fs::write(dir.join("account/projects/p/a.jsonl"), line("m1", "claude-opus-5-5", "2026-10-10T03:10:00Z", [2, 51, 447_967, 0, 483])).unwrap();
        let folders = [dir.join("account")];
        let ledger = Ledger::open(&dir.join("spend"));
        assert!(!ledger.ready());
        ledger.update(&folders, NOW);
        assert!(ledger.ready());
        drop(ledger);
        let again = Ledger::open(&dir.join("spend"));
        assert!(again.ready(), "a ledger read once is ready at the next launch");
        // The same file, the copy appended again by a resume into a new file: still once.
        fs::write(dir.join("account/projects/p/b.jsonl"), line("m1", "claude-opus-5-5", "2026-10-10T03:10:00Z", [2, 51, 447_967, 0, 483])).unwrap();
        again.update(&folders, NOW);
        let t = again.totals(0);
        assert_eq!(t.requests, 1);
        assert!((t.cost - 0.0944854).abs() < 1e-12, "{}", t.cost);
    }

    #[test]
    fn a_period_is_every_slot_from_its_start() {
        let (_root, dir) = scratch();
        // 2026-10-10T23:50Z and 2026-10-11T00:05Z: a UTC day apart by fifteen minutes.
        fs::write(dir.join("account/projects/p/a.jsonl"),
            line("late", "claude-sonnet-5-5", "2026-10-10T23:50:00Z", [1000, 0, 0, 0, 0])
            + &line("early", "claude-sonnet-5-5", "2026-10-11T00:05:00Z", [2000, 0, 0, 0, 0])).unwrap();
        let ledger = Ledger::open(&dir.join("spend"));
        ledger.update(&[dir.join("account")], NOW);
        let midnight_utc = stamp_ms("2026-10-11T00:00:00Z");
        assert_eq!(ledger.totals(midnight_utc).requests, 1);
        assert!((ledger.totals(midnight_utc).cost - 0.004).abs() < 1e-12);
        // New York's midnight, 04:00Z the day before: both.
        assert_eq!(ledger.totals(stamp_ms("2026-10-10T04:00:00Z")).requests, 2);
        // India's midnight, 18:30Z: the quarter-hour slots keep it exact.
        assert_eq!(ledger.totals(stamp_ms("2026-10-10T18:30:00Z")).requests, 2);
        assert_eq!(ledger.totals(stamp_ms("2026-10-10T23:55:00Z")).requests, 1);
    }

    fn stamp_ms(s: &str) -> u64 {
        crate::spend::stamp(s).unwrap()
    }

    #[test]
    fn families_name_their_versions_and_efforts_and_unpriced_models_stay_unpriced() {
        let (_root, dir) = scratch();
        fs::write(dir.join("account/projects/p/a.jsonl"),
            line("a", "claude-opus-5-5", "2026-10-11T01:00:00Z", [1_000_000, 0, 0, 0, 0])
            + &line("b", "claude-opus-5", "2026-10-11T01:00:00Z", [1_000_000, 0, 0, 0, 0])
            + &line("c", "claude-nova-9", "2026-10-11T01:00:00Z", [1_000_000, 0, 0, 0, 0])).unwrap();
        let ledger = Ledger::open(&dir.join("spend"));
        ledger.update(&[dir.join("account")], NOW);
        let t = ledger.totals(0);
        assert_eq!((t.requests, t.unpriced_requests), (3, 1));
        assert_eq!(t.families.len(), 1);
        let opus = &t.families[0];
        assert_eq!(opus.family, Family::Opus);
        assert_eq!(opus.names.keys().cloned().collect::<Vec<_>>(), vec!["Opus 5".to_string(), "Opus 5.5".to_string()]);
        assert!((opus.cost - 9.).abs() < 1e-12, "$4 + $5 for a million input tokens each");
        assert_eq!(opus.efforts.keys().cloned().collect::<Vec<_>>(), vec!["high".to_string()]);
    }
}

/// What the screen draws for a period (`spend_view`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendView {
    /// Whether the whole history has been read once. Until then the screen says it is counting
    /// and shows no partial figure.
    pub ready: bool,
    /// When these totals were taken, Unix ms (the screen's "Updated").
    pub as_of: u64,
    /// The day the rates were read off the published table.
    pub price_date: &'static str,
    pub totals: Totals,
}

/// The live line (`spend_live`): who is working now and the latest request.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendLive {
    pub ready: bool,
    /// Requests counted, ever: more than the snapshot's means something landed since.
    pub counted: u64,
    pub agents_working: usize,
    pub last: Option<LastRequest>,
}

/// The latest request, as the live line says it: "Haiku 5.5, 58,828 tokens, $0.001".
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastRequest {
    pub name: String,
    pub tokens: u64,
    /// `None` on a model with no published rate.
    pub cost: Option<f64>,
}

impl LastRequest {
    pub fn of(request: &Request) -> LastRequest {
        LastRequest {
            name: super::family(&request.model).map_or_else(|| request.model.clone(), |(_, name)| name),
            tokens: request.buckets.total(),
            cost: request.price(),
        }
    }
}
