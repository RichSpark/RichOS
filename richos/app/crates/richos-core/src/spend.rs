//! **API-equivalent token spend** (the CEO, §118, 2026-10-10: *"I want to add this feature that
//! will allow the user to see and brag about (via screenshots and screen recording videos) about
//! their "API-equivalent token spend""*).
//!
//! What every request the user's Claude Code made would have cost at Claude's published API
//! prices. The measurement is richos-hq `docs/research/2026-10-10-claude-token-cost-accounting/`
//! (`report.md`): per request the exact model, its effort, and FIVE token buckets, each priced on
//! its own; never a total multiplied by one rate, because cache reads and the two kinds of cache
//! write differ in price by up to 80 times.
//!
//! ```text
//! cost = (input * input_rate + output * output_rate + cache_read * cache_read_rate
//!         + write_5m * write_5m_rate + write_1h * write_1h_rate) / 1,000,000
//! ```
//!
//! Rates: platform.claude.com/docs/en/about-claude/pricing, read 2026-10-11 ([`PRICE_DATE`]).
//! Request-level rules, applied per request before anything is summed:
//! - **Haiku 5.5 has two price cards**: a request whose prompt is OVER 100,000 tokens (input,
//!   cache reads and both cache writes counted) pays the higher one, cache hits included.
//! - **Fast mode** (`usage.speed == "fast"`) on Opus 5.5 ($8/$40), Opus 5 and Opus 4.8 ($10/$50),
//!   with the cache multipliers applied on top. On any other model it runs and is billed standard.
//! - **US-only inference** (`usage.inference_geo == "us"`) is 1.1 times every category.
//!
//! A model with no published rate here is NOT guessed: its requests stay unpriced and counted.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Every request counted once, kept as totals, from the transcripts and the app's own leases.
pub mod ledger;

/// When the rates below were read off the published table. The screen says it under the figure.
pub const PRICE_DATE: &str = "October 11, 2026";

/// The prompt length above which Haiku 5.5 pays its higher card ("over 100,000 tokens").
pub const HAIKU_LONG_PROMPT: u64 = 100_000;

/// One request's five chargeable token buckets. The two cache writes are separate buckets:
/// together they are `cache_creation_input_tokens`, which is a cross-check, never a sixth bucket.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buckets {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub write_5m: u64,
    pub write_1h: u64,
}

impl Buckets {
    /// Every token of the request's prompt: what Haiku 5.5's 100,000 line is counted on.
    pub fn prompt(&self) -> u64 {
        self.input + self.cache_read + self.write_5m + self.write_1h
    }
    pub fn total(&self) -> u64 {
        self.prompt() + self.output
    }
    pub fn add(&mut self, other: &Buckets) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.write_5m += other.write_5m;
        self.write_1h += other.write_1h;
    }
    /// What `other` holds beyond `self`, bucket by bucket (never below zero).
    pub fn beyond(&self, other: &Buckets) -> Buckets {
        Buckets {
            input: other.input.saturating_sub(self.input),
            output: other.output.saturating_sub(self.output),
            cache_read: other.cache_read.saturating_sub(self.cache_read),
            write_5m: other.write_5m.saturating_sub(self.write_5m),
            write_1h: other.write_1h.saturating_sub(self.write_1h),
        }
    }

    /// The buckets of one `usage` object (a transcript row's, a stream frame's). The two cache
    /// writes come from `cache_creation`; where a row has no split, its whole write total is the
    /// 5-minute bucket (Claude Code's own default). In the 311,807 messages on this Mac on
    /// 2026-10-11 every row had the split, and in every row it added up to the total.
    pub fn of_usage(usage: &Value) -> Buckets {
        let count = |v: &Value, field: &str| v.get(field).and_then(Value::as_u64).unwrap_or(0);
        let total_writes = count(usage, "cache_creation_input_tokens");
        let (write_5m, write_1h) = match usage.get("cache_creation").filter(|c| c.is_object()) {
            Some(split) => (count(split, "ephemeral_5m_input_tokens"), count(split, "ephemeral_1h_input_tokens")),
            None => (total_writes, 0),
        };
        Buckets {
            input: count(usage, "input_tokens"),
            output: count(usage, "output_tokens"),
            cache_read: count(usage, "cache_read_input_tokens"),
            write_5m,
            write_1h,
        }
    }
}

/// What decides a request's rates besides its buckets: the exact model, Haiku 5.5's card, fast
/// mode and US-only inference. Requests on the same card can be summed and priced once, exactly.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Card {
    pub model: String,
    /// Haiku 5.5's higher card (its prompt was over [`HAIKU_LONG_PROMPT`]). False on every other model.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub long: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fast: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub us: bool,
}

/// One request, as the ledger keeps it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// The API message id (`message.id`): one response, however many transcript lines carry it.
    pub id: String,
    /// When it was recorded, Unix milliseconds.
    pub at: u64,
    pub model: String,
    /// The effort applied (`effort` on the transcript row). `None` where nothing recorded it.
    pub effort: Option<String>,
    pub fast: bool,
    pub us: bool,
    pub buckets: Buckets,
    /// The prompt length that decides Haiku 5.5's card when it is not this record's own: a lease's
    /// `result` remainder is output its frames never showed, and it belongs to requests already
    /// counted, so it takes the card of the lease's latest request on that model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_prompt: Option<u64>,
}

impl Request {
    /// The request one `message` describes (a transcript row's `message`, or a stream frame's),
    /// with the row's own fields beside it (`row`: `effort`, `uuid`). `None` without usage,
    /// without a single token (Claude Code's `<synthetic>` rows) or without an id.
    pub fn of_message(message: &Value, row: &Value, at: u64) -> Option<Request> {
        let usage = message.get("usage").filter(|u| u.is_object())?;
        let id = message.get("id").and_then(Value::as_str).or_else(|| row.get("uuid").and_then(Value::as_str))?;
        let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
        Request::build(id.to_string(), text(message.get("model")), text(row.get("effort")), at, usage)
    }

    /// One transcript line, if it is an assistant message with usage. Only the fields pricing
    /// needs are decoded (the message content, most of a 9 GB history, is skipped unread).
    pub fn of_transcript_line(line: &[u8]) -> Option<Request> {
        if !line.windows(7).any(|w| w == b"\"usage\"") {
            return None;
        }
        #[derive(Deserialize)]
        struct Message {
            id: Option<String>,
            model: Option<String>,
            usage: Option<Value>,
        }
        #[derive(Deserialize)]
        struct Row {
            #[serde(rename = "type")]
            kind: Option<String>,
            timestamp: Option<String>,
            uuid: Option<String>,
            effort: Option<String>,
            message: Option<Message>,
        }
        let row: Row = serde_json::from_slice(line).ok()?;
        if row.kind.as_deref() != Some("assistant") {
            return None;
        }
        let at = stamp(row.timestamp.as_deref()?)?;
        let message = row.message?;
        let usage = message.usage.filter(Value::is_object)?;
        let id = message.id.or(row.uuid)?;
        Request::build(id, message.model, row.effort, at, &usage)
    }

    fn build(id: String, model: Option<String>, effort: Option<String>, at: u64, usage: &Value) -> Option<Request> {
        let buckets = Buckets::of_usage(usage);
        if buckets.total() == 0 {
            return None;
        }
        Some(Request {
            id,
            at,
            model: model.unwrap_or_else(|| "unknown".into()),
            effort: effort.filter(|e| !e.is_empty()),
            fast: usage.get("speed").and_then(Value::as_str) == Some("fast"),
            us: usage.get("inference_geo").and_then(Value::as_str) == Some("us"),
            buckets,
            card_prompt: None,
        })
    }

    /// The card this request is priced on.
    pub fn card(&self) -> Card {
        let model = base_model(&self.model).to_string();
        let long = model == "claude-haiku-5-5" && self.card_prompt.unwrap_or(self.buckets.prompt()) > HAIKU_LONG_PROMPT;
        Card { model, long, fast: self.fast, us: self.us }
    }

    /// Its API-equivalent cost in US dollars, or `None` on a model with no published rate.
    pub fn price(&self) -> Option<f64> {
        price(&self.card(), &self.buckets)
    }
}

/// RFC 3339 to Unix milliseconds.
pub fn stamp(text: &str) -> Option<u64> {
    let parsed = time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).ok()?;
    u64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).ok()
}

/// The model id without a date suffix (`claude-haiku-4-5-20251001` is Haiku 4.5) or a context
/// tag (`claude-opus-4-6[1m]`).
pub fn base_model(model: &str) -> &str {
    let model = model.split('[').next().unwrap_or(model);
    match model.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => model,
    }
}

/// USD per million tokens, one card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub write_5m: f64,
    pub write_1h: f64,
}

const fn r(input: f64, output: f64, cache_read: f64, write_5m: f64, write_1h: f64) -> Rates {
    Rates { input, output, cache_read, write_5m, write_1h }
}

/// The published standard rates, by exact model id (pricing page, read 2026-10-11): base input,
/// output, cache hits and refreshes, 5-minute writes, 1-hour writes. Cache hits are 0.025x input
/// on Fable 5.1 and Mythos 5.1, 0.05x on Opus 5.5 and Sonnet 5.5, 0.1x everywhere else.
const STANDARD: &[(&str, Rates)] = &[
    ("claude-fable-5-1", r(10., 50., 0.25, 12.5, 20.)),
    ("claude-mythos-5-1", r(10., 50., 0.25, 12.5, 20.)),
    ("claude-fable-5", r(10., 50., 1., 12.5, 20.)),
    ("claude-mythos-5", r(10., 50., 1., 12.5, 20.)),
    ("claude-opus-5-5", r(4., 20., 0.20, 5., 8.)),
    ("claude-opus-5", r(5., 25., 0.50, 6.25, 10.)),
    ("claude-opus-4-8", r(5., 25., 0.50, 6.25, 10.)),
    ("claude-opus-4-7", r(5., 25., 0.50, 6.25, 10.)),
    ("claude-opus-4-6", r(5., 25., 0.50, 6.25, 10.)),
    ("claude-opus-4-5", r(5., 25., 0.50, 6.25, 10.)),
    ("claude-opus-4-1", r(15., 75., 1.50, 18.75, 30.)),
    ("claude-opus-4", r(15., 75., 1.50, 18.75, 30.)),
    ("claude-sonnet-5-5", r(2., 10., 0.10, 2.5, 4.)),
    ("claude-sonnet-5", r(2., 10., 0.20, 2.5, 4.)),
    ("claude-sonnet-4-6", r(3., 15., 0.30, 3.75, 6.)),
    ("claude-sonnet-4-5", r(3., 15., 0.30, 3.75, 6.)),
    ("claude-sonnet-4", r(3., 15., 0.30, 3.75, 6.)),
    ("claude-haiku-5-5", r(0.10, 0.50, 0.01, 0.125, 0.20)),
    ("claude-haiku-4-5", r(1., 5., 0.10, 1.25, 2.)),
    ("claude-3-5-haiku", r(0.80, 4., 0.08, 1., 1.6)),
];

/// Haiku 5.5 for prompts over 100,000 tokens.
const HAIKU_5_5_LONG: Rates = r(0.50, 2.50, 0.05, 0.625, 1.);

/// Fast mode's input and output rates, where the model has fast mode.
const FAST: &[(&str, f64, f64)] = &[
    ("claude-opus-5-5", 8., 40.),
    ("claude-opus-5", 10., 50.),
    ("claude-opus-4-8", 10., 50.),
];

/// The rates of one card, or `None` for a model with no published rate.
pub fn rates(card: &Card) -> Option<Rates> {
    let standard = if card.model == "claude-haiku-5-5" && card.long {
        HAIKU_5_5_LONG
    } else {
        STANDARD.iter().find(|(id, _)| *id == card.model)?.1
    };
    let mut rates = standard;
    if card.fast {
        if let Some((_, input, output)) = FAST.iter().find(|(id, ..)| *id == card.model) {
            // "Prompt caching multipliers apply on top of fast mode pricing": the cache rates keep
            // their ratio to input.
            let scale = input / standard.input;
            rates = r(*input, *output, standard.cache_read * scale, standard.write_5m * scale, standard.write_1h * scale);
        }
    }
    if card.us {
        rates = r(rates.input * 1.1, rates.output * 1.1, rates.cache_read * 1.1, rates.write_5m * 1.1, rates.write_1h * 1.1);
    }
    Some(rates)
}

/// The API-equivalent cost in US dollars of `buckets` on `card`, or `None` with no published rate.
pub fn price(card: &Card, buckets: &Buckets) -> Option<f64> {
    let r = rates(card)?;
    Some((buckets.input as f64 * r.input
        + buckets.output as f64 * r.output
        + buckets.cache_read as f64 * r.cache_read
        + buckets.write_5m as f64 * r.write_5m
        + buckets.write_1h as f64 * r.write_1h)
        / 1_000_000.)
}

/// The four model marks the screen draws (round 22.1), in its one fixed order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Fable,
    #[default]
    Opus,
    Sonnet,
    Haiku,
}

/// Which mark a model is drawn under, and its name as the screen says it ("Opus 5.5",
/// "Haiku 4.5"). `None` for a model that is none of the four.
pub fn family(model: &str) -> Option<(Family, String)> {
    let base = base_model(model);
    let rest = base.strip_prefix("claude-")?;
    // The old naming put the version first: claude-3-5-haiku.
    if let Some(version) = rest.strip_suffix("-haiku") {
        return Some((Family::Haiku, format!("Haiku {}", version.replace('-', "."))));
    }
    let (name, version) = rest.split_once('-')?;
    let (family, word) = match name {
        "fable" => (Family::Fable, "Fable"),
        "mythos" => (Family::Fable, "Mythos"),
        "opus" => (Family::Opus, "Opus"),
        "sonnet" => (Family::Sonnet, "Sonnet"),
        "haiku" => (Family::Haiku, "Haiku"),
        _ => return None,
    };
    if version.is_empty() || !version.split('-').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    Some((family, format!("{word} {}", version.replace('-', "."))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// **The totals, proven on recorded requests** (the brief's proof: "a fixture of recorded
    /// requests (including duplicates and both cache-write kinds) prices to the hand-computed
    /// amount"). `spend/fixture-requests.jsonl` is transcript rows in Claude Code 2.1.296's own
    /// shape. Each message, priced by hand at the published rates (USD per million tokens):
    ///
    /// | message | model, card | input, output, cache read, 5m write, 1h write | dollars |
    /// |---|---|---|---|
    /// | msg_A (2 copies, output 10 then 51) | Opus 5.5 | 2, 51, 447,967, 0, 483 | 8+1,020+89,593.4+0+3,864 = 94,485.4 → 0.0944854 |
    /// | msg_B (2 copies, the second from a resumed session) | Sonnet 5.5 | 1,000, 2,000, 50,000, 4,000, 0 | 2,000+20,000+5,000+10,000 = 37,000 → 0.037 |
    /// | msg_C | Haiku 5.5, prompt 45,000 (low card) | 3,000, 400, 40,000, 500, 1,500 | 300+200+400+62.5+300 = 1,262.5 → 0.0012625 |
    /// | msg_D | Haiku 5.5, prompt 122,010 (over 100K: high card) | 10, 1,000, 120,000, 0, 2,000 | 5+2,500+6,000+0+2,000 = 10,505 → 0.010505 |
    /// | msg_E | Haiku 5.5, prompt exactly 100,000 (low card) | 0, 100, 100,000, 0, 0 | 50+1,000 = 1,050 → 0.00105 |
    /// | msg_F | Fable 5.1 | 1,800, 1,400, 280,000, 1,200, 6,000 | 18,000+70,000+70,000+15,000+120,000 = 293,000 → 0.293 |
    /// | msg_G | Opus 5.5 fast, US-only | 100, 1,000, 10,000, 1,000, 1,000 | (800+40,000+4,000+10,000+16,000) x 1.1 = 77,880 → 0.07788 |
    /// | msg_H | Haiku 4.5 (dated id) | 500, 50, 0, 0, 0 | 500+250 = 750 → 0.00075 |
    /// | msg_I | Opus 5, no cache split (all 5-minute) | 0, 100, 0, 800, 0 | 2,500+5,000 = 7,500 → 0.0075 |
    /// | `<synthetic>` | no tokens | — | not a request |
    /// | msg_U | an unpublished model | 1,000, 0, 0, 0, 0 | unpriced, counted |
    ///
    /// Sum of the nine priced: **$0.5234329**.
    #[test]
    fn a_fixture_of_recorded_requests_prices_to_the_hand_computed_amount() {
        let mut kept: HashMap<String, Request> = HashMap::new();
        for line in include_str!("spend/fixture-requests.jsonl").lines() {
            let Some(found) = Request::of_transcript_line(line.as_bytes()) else { continue };
            // The ledger's rule: one response counts once, at its largest copy.
            if kept.get(&found.id).is_some_and(|old| old.buckets.output >= found.buckets.output) { continue; }
            kept.insert(found.id.clone(), found);
        }
        assert_eq!(kept.len(), 10, "nine priced messages and one unpriced, each once: {:?}", kept.keys());
        assert_eq!(kept["msg_A"].buckets.output, 51, "the largest copy is the one kept");
        assert_eq!(kept["msg_A"].buckets.write_1h, 483);
        assert_eq!(kept["msg_A"].effort.as_deref(), Some("high"));
        assert_eq!(kept["msg_H"].effort, None, "no effort recorded stays unrecorded");
        assert_eq!(kept["msg_H"].card().model, "claude-haiku-4-5");
        assert!(kept["msg_D"].card().long && !kept["msg_C"].card().long && !kept["msg_E"].card().long);
        let priced: Vec<f64> = kept.values().filter_map(Request::price).collect();
        assert_eq!(priced.len(), 9);
        assert!(kept["msg_U"].price().is_none(), "a model with no published rate is not guessed");
        let each = |id: &str| kept[id].price().unwrap();
        for (id, dollars) in [("msg_A", 0.0944854), ("msg_B", 0.037), ("msg_C", 0.0012625), ("msg_D", 0.010505),
            ("msg_E", 0.00105), ("msg_F", 0.293), ("msg_G", 0.07788), ("msg_H", 0.00075), ("msg_I", 0.0075)]
        {
            assert!((each(id) - dollars).abs() < 1e-12, "{id}: {} against {dollars} by hand", each(id));
        }
        let total: f64 = priced.iter().sum();
        assert!((total - 0.5234329).abs() < 1e-12, "the fixture prices to {total}, by hand $0.5234329");
    }

    #[test]
    fn the_report_worked_example_prices_as_the_report_does() {
        // report.md: the Opus 5.5 response at 04:46:13.800 UTC, $0.0944854.
        let card = Card { model: "claude-opus-5-5".into(), ..Card::default() };
        let b = Buckets { input: 2, output: 51, cache_read: 447_967, write_5m: 0, write_1h: 483 };
        assert!((price(&card, &b).unwrap() - 0.0944854).abs() < 1e-12);
    }

    #[test]
    fn models_are_named_and_marked_as_the_screen_draws_them() {
        assert_eq!(family("claude-opus-5-5"), Some((Family::Opus, "Opus 5.5".into())));
        assert_eq!(family("claude-opus-5"), Some((Family::Opus, "Opus 5".into())));
        assert_eq!(family("claude-haiku-4-5-20251001"), Some((Family::Haiku, "Haiku 4.5".into())));
        assert_eq!(family("claude-3-5-haiku-20241022"), Some((Family::Haiku, "Haiku 3.5".into())));
        assert_eq!(family("claude-fable-5-1"), Some((Family::Fable, "Fable 5.1".into())));
        assert_eq!(family("<synthetic>"), None);
        assert_eq!(base_model("claude-opus-4-6[1m]"), "claude-opus-4-6");
    }

    #[test]
    fn fast_mode_is_standard_where_a_model_has_none() {
        let fast = Card { model: "claude-sonnet-5-5".into(), fast: true, ..Card::default() };
        let standard = Card { model: "claude-sonnet-5-5".into(), ..Card::default() };
        assert_eq!(rates(&fast), rates(&standard));
        let opus5 = rates(&Card { model: "claude-opus-5".into(), fast: true, ..Card::default() }).unwrap();
        assert_eq!((opus5.input, opus5.output, opus5.cache_read, opus5.write_5m, opus5.write_1h), (10., 50., 1., 12.5, 20.));
    }
}
