//! One host sampler, shared by the rail and every agent. Reads never sample or spawn commands.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const TOOL: &str = "system_resources";
pub const QUALIFIED_TOOL: &str = "mcp__richos_status__system_resources";
pub const STALE_MS: u64 = 3_000;
const HISTORY_MS: u64 = 60_000;
const MAX_SAMPLES: usize = 60;
const MAX_REPLY: u64 = 128 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub sampled_at_ms: u64,
    pub monotonic_ms: u64,
    pub interval_ms: u64,
    pub cpu_user_percent: f64,
    pub cpu_system_percent: f64,
    pub cpu_idle_percent: f64,
    pub cpu_busy_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub memory_compressed_bytes: u64,
    pub memory_pressure: String,
    pub swap_used_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub instance_id: String,
    /// available, warming_up, unavailable or stale. Only available has a current reading.
    pub status: String,
    pub age_ms: Option<u64>,
    pub observed_monotonic_ms: u64,
    pub current: Option<Sample>,
    pub history: Vec<Sample>,
    pub detail: Option<String>,
}
impl Snapshot {
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self {
            version: 1,
            instance_id: String::new(),
            status: "unavailable".into(),
            age_ms: None,
            observed_monotonic_ms: monotonic_ms(),
            current: None,
            history: Vec::new(),
            detail: Some(detail.into()),
        }
    }
    fn at(&self, now: u64) -> Self {
        let mut result = self.clone();
        result.observed_monotonic_ms = now;
        result.history.retain(|s| {
            now.checked_sub(s.monotonic_ms)
                .is_some_and(|age| age <= HISTORY_MS)
        });
        result.age_ms = self
            .current
            .as_ref()
            .and_then(|s| now.checked_sub(s.monotonic_ms));
        if self.current.is_some() && result.age_ms.is_none_or(|age| age > STALE_MS) {
            result.status = "stale".into();
            result.current = None;
            result.detail = Some("The sampler has not supplied a current reading.".into());
        }
        result
    }
}
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
/// Darwin's monotonic raw clock includes time asleep, unlike mach_absolute_time / Instant.
fn monotonic_ms() -> u64 {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut value: libc::timespec = std::mem::zeroed();
        if libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut value) == 0 {
            return value.tv_sec as u64 * 1000 + value.tv_nsec as u64 / 1_000_000;
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        wall_ms()
    }
    #[cfg(target_os = "macos")]
    0
}

/// A second clock excludes sleep. Comparing the two detects even a short sleep interval.
fn awake_ms() -> u64 {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut value: libc::timespec = std::mem::zeroed();
        if libc::clock_gettime(libc::CLOCK_UPTIME_RAW, &mut value) == 0 {
            return value.tv_sec as u64 * 1000 + value.tv_nsec as u64 / 1_000_000;
        }
    }
    monotonic_ms()
}
fn crossed_sleep(elapsed: u64, awake_elapsed: u64) -> bool {
    elapsed.abs_diff(awake_elapsed) > 100
}

#[derive(Clone)]
struct Counters {
    ticks: [u32; 4],
    memory_used_bytes: u64,
    memory_total_bytes: u64,
    memory_compressed_bytes: u64,
    memory_pressure: String,
    swap_used_bytes: u64,
}
fn sample(
    before: &Counters,
    after: &Counters,
    elapsed: u64,
    now: u64,
    wall: u64,
) -> Result<Sample, String> {
    if !(500..=STALE_MS).contains(&elapsed) {
        return Err("Starting a fresh CPU interval after a sampling gap.".into());
    }
    let delta: Vec<u64> = before
        .ticks
        .iter()
        .zip(after.ticks)
        .map(|(a, b)| b.wrapping_sub(*a) as u64)
        .collect();
    let total: u64 = delta.iter().sum();
    if total == 0 {
        return Err("The CPU counters did not advance.".into());
    }
    if after.memory_total_bytes == 0
        || after.memory_used_bytes > after.memory_total_bytes
        || after.memory_compressed_bytes > after.memory_total_bytes
    {
        return Err("macOS returned invalid memory counters.".into());
    }
    let percent = |n: u64| n as f64 * 100.0 / total as f64;
    Ok(Sample {
        sampled_at_ms: wall,
        monotonic_ms: now,
        interval_ms: elapsed,
        cpu_user_percent: percent(delta[0] + delta[3]),
        cpu_system_percent: percent(delta[1]),
        cpu_idle_percent: percent(delta[2]),
        cpu_busy_percent: percent(total - delta[2]),
        memory_used_bytes: after.memory_used_bytes,
        memory_total_bytes: after.memory_total_bytes,
        memory_compressed_bytes: after.memory_compressed_bytes,
        memory_pressure: after.memory_pressure.clone(),
        swap_used_bytes: after.swap_used_bytes,
    })
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    extern "C" {
        fn mach_port_deallocate(
            task: libc::mach_port_t,
            name: libc::mach_port_t,
        ) -> libc::kern_return_t;
    }
    struct Host(libc::mach_port_t);
    impl Drop for Host {
        #[allow(deprecated)]
        fn drop(&mut self) {
            unsafe {
                let _ = mach_port_deallocate(libc::mach_task_self(), self.0);
            }
        }
    }
    fn sysctl<T: Copy>(name: &[u8], mut value: T) -> Result<T, String> {
        let mut len = std::mem::size_of::<T>();
        let result = unsafe {
            libc::sysctlbyname(
                name.as_ptr().cast(),
                (&mut value as *mut T).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 || len != std::mem::size_of::<T>() {
            return Err("macOS resource counters are unavailable.".into());
        }
        Ok(value)
    }
    pub(super) fn counters() -> Result<Counters, String> {
        unsafe {
            #[allow(deprecated)]
            let host = Host(libc::mach_host_self());
            let mut ticks = [0u32; 4];
            let mut count = 4;
            if libc::host_statistics(
                host.0,
                libc::HOST_CPU_LOAD_INFO,
                ticks.as_mut_ptr().cast(),
                &mut count,
            ) != 0
                || count != 4
            {
                return Err("macOS CPU counters are unavailable.".into());
            }
            let mut vm: libc::vm_statistics64 = std::mem::zeroed();
            let mut count =
                (std::mem::size_of_val(&vm) / std::mem::size_of::<libc::integer_t>()) as u32;
            let required = ((std::mem::offset_of!(libc::vm_statistics64, internal_page_count)
                + std::mem::size_of::<u32>())
                / std::mem::size_of::<libc::integer_t>()) as u32;
            if libc::host_statistics64(
                host.0,
                libc::HOST_VM_INFO64,
                (&mut vm as *mut libc::vm_statistics64).cast(),
                &mut count,
            ) != 0
                || count < required
            {
                return Err("macOS memory counters are unavailable.".into());
            }
            let page = sysctl(b"hw.pagesize\0", 0u32)? as u64;
            let total = sysctl(b"hw.memsize\0", 0u64)?;
            // Anonymous pages less purgeable, plus wired and the physical compressor footprint.
            // File cache is excluded. XNU vm_statistics.h counts speculative pages within free_count.
            // https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/mach/vm_statistics.h
            let pages = (vm.internal_page_count as u64
                + vm.wire_count as u64
                + vm.compressor_page_count as u64)
                .checked_sub(vm.purgeable_count as u64)
                .ok_or("macOS returned inconsistent memory counters.")?;
            let used = pages.checked_mul(page).ok_or("Memory counter overflow.")?;
            let pressure = match sysctl(b"kern.memorystatus_vm_pressure_level\0", 0i32).ok() {
                Some(1) => "normal",
                Some(2) => "warning",
                Some(4) => "critical",
                _ => "unknown",
            };
            #[repr(C)]
            #[derive(Clone, Copy)]
            struct Swap {
                total: u64,
                avail: u64,
                used: u64,
                pagesize: u32,
                encrypted: i32,
            }
            let swap = sysctl(
                b"vm.swapusage\0",
                Swap {
                    total: 0,
                    avail: 0,
                    used: 0,
                    pagesize: 0,
                    encrypted: 0,
                },
            )?;
            Ok(Counters {
                ticks,
                memory_used_bytes: used,
                memory_total_bytes: total,
                memory_compressed_bytes: vm.compressor_page_count as u64 * page,
                memory_pressure: pressure.into(),
                swap_used_bytes: swap.used,
            })
        }
    }
}
#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;
    pub(super) fn counters() -> Result<Counters, String> {
        Err("Live host resources are supported on macOS.".into())
    }
}

pub fn endpoint(state: &Path) -> PathBuf {
    state.join("system-resources.sock")
}

/// A separate process uses this same bounded read as IPC. No probing or sampling fallback.
pub fn read(state: &Path) -> Snapshot {
    #[cfg(unix)]
    {
        use std::{
            io::{Read, Write},
            os::unix::net::UnixStream,
        };
        let answer = (|| -> Result<Snapshot, String> {
            let mut stream = UnixStream::connect(endpoint(state))
                .map_err(|_| "The resource sampler is not running.")?;
            stream
                .set_read_timeout(Some(Duration::from_millis(500)))
                .map_err(|e| format!("Could not bound the resource read: {e}"))?;
            stream
                .set_write_timeout(Some(Duration::from_millis(500)))
                .map_err(|e| e.to_string())?;
            // The server waits for this byte before replying, so it cannot close the
            // socket before Darwin has accepted our timeout options (EINVAL race).
            stream
                .write_all(b"1")
                .map_err(|_| "The resource sampler could not be reached.")?;
            let mut bytes = Vec::new();
            (&mut stream)
                .take(MAX_REPLY + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "The resource sampler did not answer promptly.")?;
            if bytes.len() as u64 > MAX_REPLY {
                return Err("The resource response is too large.".into());
            }
            let value: Snapshot = serde_json::from_slice(&bytes)
                .map_err(|_| "The resource sampler returned invalid data.")?;
            if value.version != 1 || value.history.len() > MAX_SAMPLES {
                return Err("Unsupported resource response.".into());
            }
            Ok(value.at(monotonic_ms()))
        })();
        answer.unwrap_or_else(Snapshot::unavailable)
    }
    #[cfg(not(unix))]
    Snapshot::unavailable("Live host resources are supported on macOS.")
}

/// Owns the sampler and endpoint. Dropping it stops and joins only its own threads.
pub struct Monitor {
    stop: Arc<(Mutex<bool>, Condvar)>,
    workers: Vec<std::thread::JoinHandle<()>>,
    #[cfg(unix)]
    socket: PathBuf,
}
impl Monitor {
    pub fn start(state: &Path) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::{
                io::{Read, Write},
                os::unix::{
                    fs::{FileTypeExt, PermissionsExt},
                    net::{UnixListener, UnixStream},
                },
            };
            std::fs::create_dir_all(state).map_err(|e| e.to_string())?;
            let socket = endpoint(state);
            if let Ok(meta) = std::fs::symlink_metadata(&socket) {
                if !meta.file_type().is_socket() {
                    return Err("Resource endpoint is occupied by a non-socket file.".into());
                }
                if UnixStream::connect(&socket).is_ok() {
                    return Err("A resource sampler already owns this endpoint.".into());
                }
                std::fs::remove_file(&socket).map_err(|e| e.to_string())?;
            }
            let listener = UnixListener::bind(&socket)
                .map_err(|e| format!("Could not open the resource endpoint: {e}"))?;
            let stop = Arc::new((Mutex::new(false), Condvar::new()));
            let mut monitor = Self {
                stop: stop.clone(),
                workers: Vec::new(),
                socket,
            };
            std::fs::set_permissions(&monitor.socket, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
            let mut initial = Snapshot::unavailable("Waiting for the first CPU interval.");
            initial.instance_id = uuid::Uuid::new_v4().to_string();
            initial.status = "warming_up".into();
            let cache = Arc::new(Mutex::new(initial));
            let sample_stop = stop.clone();
            let sample_cache = cache.clone();
            let sampler = std::thread::Builder::new()
                .name("host-resources".into())
                .spawn(move || {
                    let mut before = platform::counters()
                        .ok()
                        .map(|c| (c, monotonic_ms(), awake_ms()));
                    let mut history = VecDeque::new();
                    loop {
                        let (lock, wake) = &*sample_stop;
                        let stopped = lock.lock().unwrap_or_else(|e| e.into_inner());
                        let (stopped, _) = wake
                            .wait_timeout_while(stopped, Duration::from_secs(1), |stop| !*stop)
                            .unwrap_or_else(|e| e.into_inner());
                        if *stopped {
                            break;
                        }
                        drop(stopped);
                        let now = monotonic_ms();
                        let awake = awake_ms();
                        let reading = platform::counters();
                        let result = match (&before, &reading) {
                            (Some((previous, at, was_awake)), Ok(current)) => {
                                let elapsed = now.saturating_sub(*at);
                                if crossed_sleep(elapsed, awake.saturating_sub(*was_awake)) {
                                    Err("Starting a fresh CPU interval after sleep.".into())
                                } else {
                                    sample(previous, current, elapsed, now, wall_ms())
                                }
                            }
                            (_, Ok(_)) => Err("Waiting for the first CPU interval.".into()),
                            (_, Err(e)) => Err(e.clone()),
                        };
                        before = reading.ok().map(|c| (c, now, awake));
                        history.retain(|s: &Sample| {
                            now.checked_sub(s.monotonic_ms)
                                .is_some_and(|age| age <= HISTORY_MS)
                        });
                        let mut value = sample_cache.lock().unwrap_or_else(|e| e.into_inner());
                        match result {
                            Ok(s) => {
                                history.push_back(s.clone());
                                while history.len() > MAX_SAMPLES {
                                    history.pop_front();
                                }
                                value.status = "available".into();
                                value.current = Some(s);
                                value.detail = None;
                                value.age_ms = Some(0);
                            }
                            Err(error) => {
                                value.status = "unavailable".into();
                                value.current = None;
                                value.age_ms = None;
                                value.detail = Some(error);
                            }
                        }
                        value.history = history.iter().cloned().collect();
                    }
                })
                .map_err(|e| e.to_string())?;
            monitor.workers.push(sampler);
            let serve_stop = stop.clone();
            let server = std::thread::Builder::new()
                .name("resource-reads".into())
                .spawn(move || {
                    for connection in listener.incoming() {
                        if *serve_stop.0.lock().unwrap_or_else(|e| e.into_inner()) {
                            break;
                        }
                        let Ok(mut stream) = connection else { break };
                        let mut request = [0u8; 1];
                        if stream
                            .set_read_timeout(Some(Duration::from_millis(100)))
                            .is_err()
                            || stream.read_exact(&mut request).is_err()
                            || request != *b"1"
                        {
                            continue;
                        }

                        let value = cache
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .at(monotonic_ms());
                        if stream
                            .set_write_timeout(Some(Duration::from_millis(100)))
                            .is_ok()
                        {
                            if let Ok(bytes) = serde_json::to_vec(&value) {
                                // A disconnected client does not stop the shared sampler.
                                if stream.write_all(&bytes).is_err() {
                                    continue;
                                }
                            }
                        }
                    }
                })
                .map_err(|e| {
                    *stop.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
                    stop.1.notify_all();
                    e.to_string()
                })?;
            monitor.workers.push(server);
            Ok(monitor)
        }
        #[cfg(not(unix))]
        Err("Live host resources are supported on macOS.".into())
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        *self.stop.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.stop.1.notify_all();
        #[cfg(unix)]
        {
            if let Ok(wake) = std::os::unix::net::UnixStream::connect(&self.socket) {
                drop(wake);
            }
        }
        for worker in self.workers.drain(..) {
            if worker.join().is_err() {
                eprintln!("[richos] a resource monitor thread ended unexpectedly");
            }
        }
        #[cfg(unix)]
        {
            if let Err(error) = std::fs::remove_file(&self.socket) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[richos] resource endpoint cleanup: {error}");
                }
            }
        }
    }
}

/// The isolated review providers receive only this read tool, never the conversation tools.
pub fn run_stdio(state: &Path) -> std::io::Result<()> {
    use serde_json::{json, Value};
    use std::io::{BufRead, Read, Write};
    let input = std::io::stdin();
    let mut reader = input.lock();
    let output = std::io::stdout();
    let mut writer = output.lock();
    let mut initialized = false;
    loop {
        let mut line = Vec::new();
        let count = (&mut reader)
            .take(16 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if count > 16 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Oversized resource request",
            ));
        }
        let r: Value = match serde_json::from_slice(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let Some(id) = r.get("id") else { continue };
        let result = match r["method"].as_str() {
            Some("initialize") => {
                initialized = true;
                Ok(
                    json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"richos_status","version":"1"}}),
                )
            }
            Some("ping") => Ok(json!({})),
            Some("tools/list") if initialized => {
                let all = crate::status_tools::tools();
                Ok(
                    json!({"tools":all["tools"].as_array().unwrap().iter().filter(|t|t["name"]==TOOL).collect::<Vec<_>>()}),
                )
            }
            Some("tools/call")
                if initialized
                    && r["params"]["name"] == TOOL
                    && r["params"]["arguments"] == json!({}) =>
            {
                Ok(
                    json!({"content":[{"type":"text","text":serde_json::to_string(&read(state))?}],"isError":false}),
                )
            }
            _ => Err("Initialize and call the read-only system_resources tool with no arguments."),
        };
        let reply = match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(message) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":message}})
            }
        };
        serde_json::to_writer(&mut writer, &reply)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn counters(ticks: [u32; 4]) -> Counters {
        Counters {
            ticks,
            memory_used_bytes: 8,
            memory_total_bytes: 16,
            memory_compressed_bytes: 2,
            memory_pressure: "unknown".into(),
            swap_used_bytes: 0,
        }
    }
    #[test]
    fn cpu_intervals_wrap_and_memory_validation() {
        let a = counters([u32::MAX - 9, 10, 100, 0]);
        let b = counters([10, 30, 160, 0]);
        let s = sample(&a, &b, 1000, 2000, 4000).unwrap();
        assert_eq!(
            (
                s.cpu_user_percent,
                s.cpu_system_percent,
                s.cpu_idle_percent,
                s.cpu_busy_percent
            ),
            (20.0, 20.0, 60.0, 40.0)
        );
        assert_eq!(s.memory_pressure, "unknown");
        assert!(sample(&a, &b, 10_000, 2000, 4000).is_err());
        assert!(sample(&b, &b, 1000, 2000, 4000).is_err());
        let mut bad = b.clone();
        bad.memory_used_bytes = 17;
        assert!(sample(&a, &bad, 1000, 2000, 4000).is_err());
    }
    #[test]
    fn short_sleep_is_a_gap_even_with_a_sub_three_second_interval() {
        assert!(crossed_sleep(1500, 1000));
        assert!(!crossed_sleep(1001, 1000));
    }
    #[test]
    fn freshness_uses_monotonic_time_not_wall_time() {
        let s = sample(
            &counters([0; 4]),
            &counters([1, 1, 2, 0]),
            1000,
            1000,
            u64::MAX,
        )
        .unwrap();
        let value = Snapshot {
            version: 1,
            instance_id: "test".into(),
            status: "available".into(),
            age_ms: Some(0),
            observed_monotonic_ms: 1000,
            current: Some(s.clone()),
            history: vec![s],
            detail: None,
        };
        assert_eq!(value.at(2000).status, "available");
        assert_eq!(value.at(5000).status, "stale");
        assert!(value.at(5000).current.is_none());
        assert!(value.at(0).current.is_none());
        assert!(value.at(62000).history.is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn malformed_endpoint_reply_is_unavailable() {
        use std::{io::Write, os::unix::net::UnixListener};
        let root = std::env::temp_dir().join(format!(
            "rr-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        std::fs::create_dir_all(&root).unwrap();
        let listener = UnixListener::bind(endpoint(&root)).unwrap();
        let responder = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"not JSON").unwrap();
        });
        let answer = read(&root);
        assert_eq!(answer.status, "unavailable");
        assert!(answer.current.is_none());
        responder.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn endpoint_reads_share_identity_and_shutdown_is_unavailable() {
        let root = std::env::temp_dir().join(format!(
            "rr-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        let monitor = Monitor::start(&root).unwrap();
        let first = read(&root);
        let second = read(&root);
        assert!(!first.instance_id.is_empty());
        assert_eq!(first.instance_id, second.instance_id);
        for _ in 0..200 {
            let reply = read(&root);
            assert_eq!(
                reply.instance_id, first.instance_id,
                "cached read lost its server: {:?}",
                reply.detail
            );
        }
        assert!(["warming_up", "available", "unavailable"].contains(&first.status.as_str()));
        assert!(Monitor::start(&root).is_err());
        drop(monitor);
        assert_eq!(read(&root).status, "unavailable");
        std::fs::remove_dir_all(root).unwrap();
    }
}
