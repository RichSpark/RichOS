//! Numeric local milestones for one recording, including preview cleanup before
//! final recognition. No speech, target names, corpus or model output enters here.
use super::log;
use richos_voice::stt::SttError;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub(super) fn decoder_failure_reason(error: &SttError) -> &'static str {
    match error {
        // These messages originate in the bounded decoder, not in its stderr.
        SttError::Io(message) if message == "decoder deadline" => "decoder_deadline",
        SttError::Io(message) if message == "decoder stopped by its owner" => "decoder_stopped",
        SttError::Io(message) if message == "decoder output limit" => "decoder_output_limit",
        SttError::Io(_) => "decoder_io",
        _ => "decoder_failed",
    }
}

pub(super) fn recognition_failure_detail(model: &str, elapsed: Duration, bound: Duration, error: &SttError) -> Value {
    json!({"model":model,"recognition_ms":elapsed.as_millis(),"deadline_ms":bound.as_millis(),
        "reason":decoder_failure_reason(error)})
}

fn preview_pause_detail(reason: &'static str, input_ms: Option<u64>) -> Value {
    json!({"reason":reason,"input_ms":input_ms})
}

pub struct Timings {
    seq: u64,
    version: String,
    model: &'static str,
    preview: bool,
    began: Instant,
    stopped: Option<Instant>,
    updates: u64,
    quiet_ticks: u64,
    first_ms: Option<u64>,
    max_decode_ms: u64,
    max_window_ms: u64,
    complete: bool,
    cpu_at_start: (Option<u64>, Option<u64>),
}

impl Timings {
    pub fn new(seq: u64, model: &str, preview: bool, version: &str) -> Self {
        let model = match model { "small.en" => "small.en", "large-v3-turbo-q5_0" => "large-v3-turbo-q5_0", _ => "other" };
        let value = Self { seq, version: version.chars().take(80).collect(), model, preview, began: Instant::now(), stopped: None,
            updates: 0, quiet_ticks: 0, first_ms: None, max_decode_ms: 0, max_window_ms: 0, complete: false, cpu_at_start: cpu_usage() };
        value.event("capture_ready", json!({}));
        value
    }
    fn entry(&self, stage: &str, elapsed_ms: u64, since_finish_ms: Option<u64>, detail: Value) -> Value {
        json!({"schema":1,"pid":std::process::id(),"seq":self.seq,
            "version":self.version,"source":crate::engine::source_commit(),"profile":if cfg!(debug_assertions) {"debug"} else {"release"},
            "selected_model":self.model,"preview_enabled":self.preview,"stage":stage,
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
        self.event("finish_received", self.finish_detail());
    }
    fn finish_detail(&self) -> Value {
        json!({"updates":self.updates,"first_result_ms":self.first_ms,
            "max_decode_ms":self.max_decode_ms,"max_sampled_window_ms":self.max_window_ms,
            "preview_quiet_ticks":self.quiet_ticks})
    }
    pub fn preview_quiet(&mut self) {
        self.quiet_ticks += 1;
    }
    pub fn preview_paused(&self, reason: &'static str, input_ms: Option<u64>) {
        self.event("preview_paused", preview_pause_detail(reason, input_ms));
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
    fn richdoctor_collects_the_produced_timing_record_without_losing_its_spans() {
        use richdoctor_core::{collect::{collect, Category, Scope, Window}, scrub::Context, evidence::Status};
        let home = std::env::temp_dir().join(format!("dictation-timing-collection-{}", std::process::id()));
        let data = home.join("Library/Application Support/com.richos.app");
        std::fs::create_dir_all(&data).unwrap();
        let mut t = Timings::new(7, "large-v3-turbo-q5_0", true, "1.2.9-dev.fixture");
        let at = 1_798_784_000_000;
        let entry = t.entry("recognition_completed", 4010, Some(420), json!({"model":"small.en","recognition_ms":300}));
        t.preview_quiet();
        let finish = t.entry("finish_received", 3590, Some(0), t.finish_detail());
        let timeout = recognition_failure_detail("small.en", Duration::from_millis(69012),
            Duration::from_secs(69), &SttError::Io("decoder deadline".into()));
        let failure = t.entry("recognition_failed", 73000, Some(69012), timeout);
        let pause = t.entry("preview_paused", 3200, None, preview_pause_detail("decoder_deadline", Some(16000)));
        let stamp = richdoctor_core::util::utc_stamp(at);
        std::fs::write(data.join("dictation.log"), format!("{stamp} dictation timing {entry}\n{stamp} dictation timing {finish}\n{stamp} dictation timing {failure}\n{stamp} dictation timing {pause}\n")).unwrap();
        let scope = Scope { home: home.clone(), window: Window::around(at), claude_bin: None, diagnostics: true };
        let collected = collect(Category::Richscribe, &scope, &Context::default(), 0);
        assert_eq!(collected[0].status, Status::Collected);
        let mut records = collected[0].text.lines()
            .filter_map(|line| line.split_once("dictation timing "))
            .map(|(_, raw)| serde_json::from_str::<Value>(raw).unwrap());
        let record = records.next().unwrap();
        assert_eq!(record["version"], "1.2.9-dev.fixture");
        assert_eq!(record["selected_model"], "large-v3-turbo-q5_0");
        assert_eq!(record["detail"]["model"], "small.en", "fallback must not masquerade as the selected model");
        assert_eq!(record["since_finish_ms"], 420);
        assert_eq!(record["detail"]["recognition_ms"], 300);
        let finish = records.next().unwrap();
        assert_eq!(finish["detail"]["preview_quiet_ticks"], 1);
        assert_eq!(finish["detail"]["updates"], 0);
        let failure = records.next().unwrap();
        assert_eq!(failure["stage"], "recognition_failed");
        assert_eq!(failure["detail"]["reason"], "decoder_deadline");
        assert_eq!(failure["detail"]["recognition_ms"], 69012);
        assert_eq!(failure["detail"]["deadline_ms"], 69000);
        let pause = records.next().unwrap();
        assert_eq!(pause["stage"], "preview_paused");
        assert_eq!(pause["detail"], json!({"reason":"decoder_deadline","input_ms":16000}));
        assert_eq!(record["preview_enabled"], true);
        t.complete = true;
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn decoder_failure_detail_never_includes_untrusted_error_content() {
        let error = SttError::Failed { status: "exit status: 1".into(), stderr: "decoder deadline: private text".into() };
        let detail = recognition_failure_detail("small.en", Duration::from_millis(27), Duration::from_secs(69), &error);
        assert_eq!(detail, json!({"model":"small.en","recognition_ms":27,"deadline_ms":69000,"reason":"decoder_failed"}));
        assert_eq!(decoder_failure_reason(&SttError::Io("private file path".into())), "decoder_io");
        assert_eq!(decoder_failure_reason(&SttError::Io("decoder stopped by its owner".into())), "decoder_stopped");
        assert_eq!(decoder_failure_reason(&SttError::Io("decoder output limit".into())), "decoder_output_limit");
    }

    #[test]
    fn stage_records_preserve_distinct_monotonic_spans_and_omit_user_content() {
        let mut t = Timings::new(7, "untrusted model text", true, "1.2.9-dev.fixture");
        let v = t.entry("preview_stopped", 4010, Some(420), json!({}));
        assert_eq!(v["elapsed_ms"], 4010);
        assert_eq!(v["since_finish_ms"], 420);
        assert_eq!(v["selected_model"], "other");
        assert_eq!(v["seq"], 7);
        assert_eq!(v["preview_enabled"], true);
        assert!(v.to_string().len() < 400);
        assert!(!v.to_string().contains("untrusted"));
        t.complete = true;
    }
}
