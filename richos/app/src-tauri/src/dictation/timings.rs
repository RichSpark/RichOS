//! Numeric local milestones for one recording, including preview cleanup before
//! final recognition. No speech, target names, corpus or model output enters here.
use super::log;
use serde_json::{json, Value};
use std::time::Instant;

pub struct Timings {
    seq: u64,
    model: &'static str,
    preview: bool,
    began: Instant,
    stopped: Option<Instant>,
    updates: u64,
    first_ms: Option<u64>,
    max_decode_ms: u64,
    max_window_ms: u64,
    complete: bool,
    cpu_at_start: (Option<u64>, Option<u64>),
}

impl Timings {
    pub fn new(seq: u64, model: &str, preview: bool) -> Self {
        let model = match model { "small.en" => "small.en", "large-v3-turbo-q5_0" => "large-v3-turbo-q5_0", _ => "other" };
        let value = Self { seq, model, preview, began: Instant::now(), stopped: None,
            updates: 0, first_ms: None, max_decode_ms: 0, max_window_ms: 0, complete: false, cpu_at_start: cpu_usage() };
        value.event("capture_ready", json!({}));
        value
    }
    fn entry(&self, stage: &str, elapsed_ms: u64, since_finish_ms: Option<u64>, detail: Value) -> Value {
        json!({"schema":1,"pid":std::process::id(),"seq":self.seq,
            "version":env!("CARGO_PKG_VERSION"),"profile":if cfg!(debug_assertions) {"debug"} else {"release"},
            "model":self.model,"preview":self.preview,"stage":stage,
            "elapsed_ms":elapsed_ms,"since_finish_ms":since_finish_ms,"detail":detail})
    }
    pub fn event(&self, stage: &'static str, detail: Value) {
        log::line(&format!("dictation timing {}", self.entry(stage, self.began.elapsed().as_millis() as u64,
            self.stopped.map(|s| s.elapsed().as_millis() as u64), detail)));
    }
    pub fn preview(&mut self, decode_ms: u64, window_ms: u64) {
        self.updates += 1;
        self.max_decode_ms = self.max_decode_ms.max(decode_ms);
        self.max_window_ms = self.max_window_ms.max(window_ms);
        if self.first_ms.is_none() {
            self.first_ms = Some(self.began.elapsed().as_millis() as u64);
            self.event("first_preview_result", json!({"decode_ms":decode_ms,"sampled_window_ms":window_ms}));
        }
    }
    pub fn stop(&mut self) {
        self.stopped = Some(Instant::now());
        self.event("finish_received", json!({"updates":self.updates,"first_result_ms":self.first_ms,
            "max_decode_ms":self.max_decode_ms,"max_sampled_window_ms":self.max_window_ms}));
    }
    pub fn finished(&mut self, outcome: &'static str) {
        let current = cpu_usage();
        self.event("operation_completed", json!({"outcome":outcome,
            "process_cpu_us":current.0.zip(self.cpu_at_start.0).map(|(end, start)| end.saturating_sub(start)),
            "child_cpu_us":current.1.zip(self.cpu_at_start.1).map(|(end, start)| end.saturating_sub(start))}));
        self.complete = true;
    }
}

// Cumulative process counters sampled only at capture and completion, never an
// idle poller. The delta includes all work by this tool and its reaped children
// during the operation; it is not a machine-wide CPU or GPU measurement.
fn cpu_usage() -> (Option<u64>, Option<u64>) {
    let read = |who| {
        // SAFETY: getrusage writes the numeric rusage output and retains no pointer.
        let mut value: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(who, &mut value) } != 0 { return None; }
        let micros = |time: libc::timeval| time.tv_sec as u64 * 1_000_000 + time.tv_usec as u64;
        Some(micros(value.ru_utime) + micros(value.ru_stime))
    };
    (read(libc::RUSAGE_SELF), read(libc::RUSAGE_CHILDREN))
}

impl Drop for Timings {
    fn drop(&mut self) {
        if !self.complete { self.event("operation_ended", json!({"outcome":"cancelled_or_interrupted"})); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stage_records_preserve_distinct_monotonic_spans_and_omit_user_content() {
        let mut t = Timings::new(7, "untrusted model text", true);
        let v = t.entry("preview_stopped", 4010, Some(420), json!({}));
        assert_eq!(v["elapsed_ms"], 4010);
        assert_eq!(v["since_finish_ms"], 420);
        assert_eq!(v["model"], "other");
        assert_eq!(v["seq"], 7);
        assert_eq!(v["preview"], true);
        assert!(v.to_string().len() < 400);
        assert!(!v.to_string().contains("untrusted"));
        t.complete = true;
    }
}
