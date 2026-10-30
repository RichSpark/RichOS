//! Bounded native probe for shared host resources, IPC latency and sampler cost.
use richos_core::system_resources::{self, Monitor};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    let state = args
        .get(2)
        .map(PathBuf::from)
        .expect("mode and absolute state directory required");
    assert!(state.is_absolute());
    if mode == "--resources-mcp" {
        system_resources::run_stdio(&state).unwrap();
        return;
    }
    if mode == "--system-resources" {
        println!(
            "{}",
            serde_json::to_string(&system_resources::read(&state)).unwrap()
        );
        return;
    }
    assert_eq!(mode, "serve");
    let seconds = args
        .get(3)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(65)
        .min(120);
    let monitor = Monitor::start(&state).unwrap();
    let start = Instant::now();
    #[cfg(unix)]
    let cpu_start = usage();
    let mut peak_read_us = 0;
    let mut previous = None;
    let mut samples = 0;
    let mut last_status = String::new();
    let mut unavailable_reads = 0;
    while start.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_millis(100));
        let at = Instant::now();
        let snapshot = system_resources::read(&state);
        peak_read_us = peak_read_us.max(at.elapsed().as_micros());
        if samples > 0 && snapshot.status != "available" {
            unavailable_reads += 1;
        }
        if snapshot.status != last_status {
            eprintln!("status={} detail={:?}", snapshot.status, snapshot.detail);
            last_status = snapshot.status.clone();
        }
        if let Some(s) = &snapshot.current {
            assert!(s.cpu_busy_percent.is_finite() && (0.0..=100.0).contains(&s.cpu_busy_percent));
            assert!(s.memory_used_bytes <= s.memory_total_bytes && s.memory_total_bytes > 0);
            assert!(snapshot.history.len() <= 60);
            if previous != Some(s.sampled_at_ms) {
                samples += 1;
                previous = Some(s.sampled_at_ms);
                println!("{}", serde_json::to_string(&snapshot).unwrap());
            }
        }
    }
    #[cfg(unix)]
    {
        let (cpu_secs, rss_bytes) = usage();
        eprintln!(
            "{}",
            serde_json::json!({"elapsed_secs":start.elapsed().as_secs_f64(),"samples":samples,"unavailable_reads":unavailable_reads,
            "cpu_secs":cpu_secs-cpu_start.0,"cpu_percent_one_core":(cpu_secs-cpu_start.0)/start.elapsed().as_secs_f64()*100.0,
            "max_rss_bytes":rss_bytes,"peak_cached_read_us":peak_read_us})
        );
    }
    assert_eq!(
        unavailable_reads, 0,
        "resource reads were lost after warmup"
    );
    assert!(samples > 0, "native sampler produced no valid samples");
    drop(monitor);
    assert_eq!(system_resources::read(&state).status, "unavailable");
}
#[cfg(unix)]
fn usage() -> (f64, u64) {
    unsafe {
        let mut r: libc::rusage = std::mem::zeroed();
        assert_eq!(libc::getrusage(libc::RUSAGE_SELF, &mut r), 0);
        let seconds = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
        let rss = r.ru_maxrss as u64;
        #[cfg(not(target_os = "macos"))]
        let rss = rss * 1024;
        (seconds(r.ru_utime) + seconds(r.ru_stime), rss)
    }
}
