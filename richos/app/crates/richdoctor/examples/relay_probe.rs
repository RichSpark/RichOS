//! **DELIVERY OVER THE REAL TRANSPORT, AGAINST A LOOPBACK RELAY** (PRD §7, A9 in part).
//!
//! Starts a relay that speaks protocol 2 on 127.0.0.1 (the guest stand-in
//! `scripts/testvm/report-relay-stand-in.py` by default, or the relay owner's workerd stand-in
//! when `RICHDOCTOR_REPORT_RELAY` and `RICHDOCTOR_REPORT_REPOSITORY` are already set), and drives
//! RichDoctor's own delivery code through its own HTTPS client (`relay::Https`, plain HTTP on
//! loopback only):
//!
//!   1. a report with two screenshots: register, two uploads, complete; "Report submitted" only
//!      with both images confirmed; exactly one issue, both images stored, byte-identical;
//!   2. the issue is created and its answer lost: status reconciles the same ID, one issue;
//!   3. a definite refusal, then Try again with the same ID: one issue;
//!   4. retirement: an unseen ID retires, and a retired ID cannot be completed.
//!
//! ```sh
//! cargo run --offline --manifest-path richos/app/crates/richdoctor/Cargo.toml --example relay_probe -- <scratch dir>
//! ```
//! Exit 0 only when every check held. The relay it started is stopped by the PID it recorded.
use richdoctor::relay::Https;
use richdoctor_core::delivery::{self, Destination, NotSent, Relay, State};
use richdoctor_core::image_profile::Entry;
use std::io::BufRead;

fn png(seed: u8, w: u32, h: u32) -> Vec<u8> {
    let px: Vec<u8> = (0..w * h).flat_map(|i| [seed, (i % 251) as u8, 64, 255]).collect();
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&px).unwrap();
    }
    richdoctor_core::image_profile::finalize(&out).unwrap().0
}

fn report(dest: &Destination, images: &[Vec<u8>]) -> delivery::Approved {
    let id = delivery::new_report_id().unwrap();
    let manifest: Vec<Entry> = images.iter().enumerate().map(|(i, b)| Entry::of(i as u32 + 1, b).unwrap()).collect();
    let body = format!("## What happened\n\nRichOS closes before its window opens.\n\n{}", richdoctor_core::report::ending(&dest.relay, &id, manifest.len()));
    delivery::approve(&id, "RichOS closes before its window opens", &body, manifest, &dest.relay, richdoctor_core::util::now_millis(), "inst-probe").unwrap()
}

fn get(dest: &Destination, path: &str) -> serde_json::Value {
    let out = std::process::Command::new("/usr/bin/curl").args(["-s", &format!("{}{path}", dest.relay)]).output().unwrap();
    serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null)
}

fn main() {
    let scratch = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: relay_probe <scratch dir>"));
    std::fs::create_dir_all(&scratch).unwrap();
    let mut started: Option<std::process::Child> = None;
    let dest = match Destination::from_env(|k| std::env::var(k).ok()) {
        Ok(d) if d.relay.starts_with("http://127.0.0.1:") => d,
        _ => {
            let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/testvm/report-relay-stand-in.py");
            let state = scratch.join("relay-state.json");
            std::fs::remove_file(&state).ok();
            let mut child = std::process::Command::new("/usr/bin/python3").arg(&script).arg("8799").arg(&state).stdout(std::process::Stdio::piped()).spawn().unwrap();
            let mut line = String::new();
            std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
            assert!(line.starts_with("listening"), "{line}");
            started = Some(child);
            Destination { relay: "http://127.0.0.1:8799".into(), repository: "richos-test/report-relay-local".into() }
        }
    };
    let relay = Https::new().unwrap();
    let clock = richdoctor_core::util::now_millis;
    let sleep = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms.min(1000)));
    let mut failures = 0;
    let mut check = |name: &str, ok: bool, why: String| {
        println!("{} {name}{}", if ok { "ok  " } else { "FAIL" }, if ok { String::new() } else { format!(": {why}") });
        if !ok {
            failures += 1;
        }
    };

    // 1. Two screenshots, the whole protocol.
    let images = vec![png(10, 320, 200), png(200, 240, 160)];
    let a = report(&dest, &images);
    let loader = |e: &Entry| images.get(e.order as usize - 1).cloned();
    let mut seen = Vec::new();
    let s = delivery::attempt(&a, &loader, &dest, &relay, &clock, &sleep, &mut |s| seen.push(s.clone()), false);
    check("1 two screenshots: Report submitted", matches!(s, State::Submitted { .. }), format!("{s:?}"));
    check("1 the upload stage was shown before filing", seen.iter().any(|s| matches!(s, State::SendingScreenshots { .. })) && seen.contains(&State::Creating), format!("{seen:?}"));
    let issues = get(&dest, "/_issues");
    let issue_count = issues.as_array().map(|a| a.len()).unwrap_or(0);
    check("1 exactly one issue", issue_count == 1, format!("{issue_count}"));
    let stored = get(&dest, "/_attachments");
    let mine: Vec<String> = stored.as_array().unwrap_or(&vec![]).iter().filter(|x| x["key"].as_str().is_some_and(|k| k.starts_with(&format!("{}/", a.report_id)))).filter_map(|x| x["sha256"].as_str().map(str::to_string)).collect();
    check("1 both images stored, byte-identical to the approved ones", mine.len() == 2 && a.manifest.iter().all(|e| mine.contains(&e.sha256)), format!("{mine:?}"));
    let body = issues[0]["body"].as_str().unwrap_or("");
    check("1 the issue carries the count and the staff links", body.contains("Screenshots shared privately with RichOS support: 2") && body.contains(&format!("{}/v1/reports/{}/attachments/2", dest.relay, a.report_id)), body.to_string());

    // 2. Created, answer lost: reconciled, never created again.
    let b = report(&dest, &[]);
    relay.post(&format!("{}/_fault", dest.relay), &serde_json::json!({"next": "lost"}));
    let s = delivery::attempt(&b, &|_| None, &dest, &relay, &clock, &sleep, &mut |_| {}, false);
    check("2 a lost answer is reconciled to Report submitted", matches!(s, State::Submitted { .. }), format!("{s:?}"));
    let n = get(&dest, "/_issues").as_array().map(|a| a.len()).unwrap_or(0);
    check("2 still one issue for that report (two in all)", n == 2, format!("{n}"));

    // 3. A definite refusal, then Try again with the same ID.
    let c = report(&dest, &[]);
    relay.post(&format!("{}/_fault", dest.relay), &serde_json::json!({"next": "limited"}));
    let s = delivery::attempt(&c, &|_| None, &dest, &relay, &clock, &sleep, &mut |_| {}, false);
    check("3 busy: Not sent yet, Try again offered", matches!(s, State::NotSent { why: NotSent::Busy { .. }, .. }) && s.permits_try_again(), format!("{s:?}"));
    let s = delivery::attempt(&c, &|_| None, &dest, &relay, &clock, &sleep, &mut |_| {}, true);
    check("3 Try again, same ID: Report submitted", matches!(s, State::Submitted { .. }), format!("{s:?}"));
    let n = get(&dest, "/_issues").as_array().map(|a| a.len()).unwrap_or(0);
    check("3 one issue for it (three in all)", n == 3, format!("{n}"));

    // 4. Retirement.
    let d = report(&dest, &[]);
    check("4 an unseen ID retires", delivery::retire(&d, &dest, &relay, clock()).is_ok(), String::new());
    let s = delivery::attempt(&d, &|_| None, &dest, &relay, &clock, &sleep, &mut |_| {}, false);
    check("4 a retired ID is never filed", matches!(s, State::NotSent { why: NotSent::Retired, .. }), format!("{s:?}"));
    let n = get(&dest, "/_issues").as_array().map(|a| a.len()).unwrap_or(0);
    check("4 no issue was created for it", n == 3, format!("{n}"));

    if let Some(mut child) = started {
        // The relay this probe started, by the handle it holds, never by a name.
        child.kill().ok();
        child.wait().ok();
    }
    println!("{}", if failures == 0 { "relay_probe: passed".to_string() } else { format!("relay_probe: {failures} FAILED") });
    std::process::exit(if failures == 0 { 0 } else { 1 });
}
