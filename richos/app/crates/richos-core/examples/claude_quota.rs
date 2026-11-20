//! Read-only smoke test: cargo run -p richos-core --example claude_quota -- --live /path/to/claude
//! The same executable also exposes the desktop's hook entrypoint for integration checks.
use richos_core::quota::{self, Service};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|s| s.to_str()) {
        Some("--claude-quota-gate") => std::process::exit(quota::gate::run_cli()),
        Some("--live") => {}
        Some("--measure") => return measure(args),
        _ => return Err("Pass --live and the Claude Code executable path.".into()),
    }
    let bin = args.next().ok_or("Missing Claude Code executable path.")?;
    let root = std::env::temp_dir().join(format!("richos-quota-smoke-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let service = Service::open_shared_reader(&root)?;
        let view = service.refresh(std::path::Path::new(&bin), true);
        service.shutdown();
        println!(
            "Quota state: {:?}; recognized windows: {}; no model turn requested.",
            view.state,
            view.windows.len()
        );
        if view.windows.is_empty() {
            return Err("No subscription windows returned.".into());
        }
        Ok(())
    })();
    std::fs::remove_dir_all(root)?;
    result
}

/// Time N attempts to read the shared quota answer and report any resident probe's resources.
/// An attempt uses the saved answer until the same five-minute slot the app uses is free.
fn measure(mut args: impl Iterator<Item = std::ffi::OsString>) -> Result<(), Box<dyn std::error::Error>> {
    let bin = args.next().ok_or("Missing Claude Code executable path.")?;
    let reads: usize = args.next().and_then(|n| n.to_str()?.parse().ok()).unwrap_or(3);
    // The gap between attempts, including attempts served from the saved answer.
    let gap: u64 = args.next().and_then(|n| n.to_str()?.parse().ok()).unwrap_or(6).max(6);
    let root = std::env::temp_dir().join(format!("richos-quota-measure-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let service = Service::open_shared_reader(&root)?;
        for read in 1..=reads {
            let started = std::time::Instant::now();
            let view = service.refresh(std::path::Path::new(&bin), true);
            let ms = started.elapsed().as_millis();
            let me = std::process::id().to_string();
            let ps = std::process::Command::new("/bin/ps").args(["-A", "-o", "ppid=,rss=,time="]).output()?;
            let child: Vec<String> = String::from_utf8_lossy(&ps.stdout).lines()
                .map(|l| l.split_whitespace().map(str::to_string).collect::<Vec<_>>())
                .filter(|f| f.len() == 3 && f[0] == me).map(|f| format!("rss {} KiB, cpu {}", f[1], f[2])).collect();
            println!("read {read}: {ms} ms, state {:?}, windows {}; child: {}", view.state, view.windows.len(),
                if child.is_empty() { "none".to_string() } else { child.join("; ") });
            if read < reads { std::thread::sleep(std::time::Duration::from_secs(gap)); }
        }
        service.shutdown();
        Ok(())
    })();
    std::fs::remove_dir_all(root)?;
    result
}
