use richos_devtools::{caption, dialect, reports, wait_shape};

use serde_json::{json, Value};
use std::io::{self, Read};

fn run(args: &[String]) -> Result<i32, String> {
    match args.first().map(String::as_str) {
        Some("rpc") => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .map_err(|e| e.to_string())?;
            let input: Value = serde_json::from_str(&input).map_err(|e| e.to_string())?;
            let op = args.get(1).ok_or("missing operation")?;
            let result = match op.as_str() {
                "cargo-summary" => {
                    reports::cargo_summary(input.as_str().ok_or("expected log text")?)
                }
                "caption-text" => Ok(json!(
                    caption::caption(input.as_str().ok_or("expected VTT")?).0
                )),
                "caption-times" => Ok(json!(
                    caption::caption(input.as_str().ok_or("expected VTT")?).1
                )),
                "caption-compare" => caption::compare(&input),
                "caption-norm" => Ok(json!(caption::norm(input.as_str().ok_or("expected word")?))),
                "caption-close" => Ok(json!(caption::close(
                    input[0].as_str().ok_or("expected word")?,
                    input[1].as_str().ok_or("expected word")?
                ))),
                "ledger-read" => {
                    reports::ledger_read(input.as_str().ok_or("expected ledger path")?)
                }
                "ledger-timeline" => reports::ledger_timeline(&input),
                "ledger-turns" => reports::ledger_turns(&input),
                "wait-shape" => wait_shape::scan(std::path::Path::new(
                    input.as_str().ok_or("expected directory")?,
                ))
                .map(|v| json!(v)),
                _ if op.starts_with("dialect-") => dialect::rpc(op, &input),
                _ => Err(format!("unknown operation: {op}")),
            }?;
            println!("{}", json!({"result": result}));
            Ok(0)
        }
        Some("cargo-summary") => {
            let text = std::fs::read_to_string(args.get(1).ok_or("expected captured log path")?)
                .map_err(|e| e.to_string())?;
            let out = reports::cargo_summary(&text)?;
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
            Ok(i32::from(
                out["ordinary"]["failed"].as_u64().unwrap_or(0) > 0
                    || out["documentation"]["failed"].as_u64().unwrap_or(0) > 0,
            ))
        }
        Some("ledger-timeline") => reports::ledger_cli(&args[1..]),
        Some("wait-shape") => {
            let root = args.get(1).ok_or("expected UI test directory")?;
            let hits = wait_shape::scan(std::path::Path::new(root))?;
            if hits.is_empty() {
                println!("PASS  every waitForFunction deadline is in the third argument");
                Ok(0)
            } else {
                println!("FAIL  {} waitForFunction call(s) pass {{ timeout }} as the page function's argument, so the wait uses Playwright's 30 s default:", hits.len());
                for hit in hits {
                    println!("        {hit}");
                }
                println!("      The deadline is the THIRD argument: waitForFunction(fn, null, {{ timeout }}).");
                Ok(1)
            }
        }
        _ => {
            Err("usage: richos-devtools <cargo-summary|ledger-timeline|wait-shape|rpc> ...".into())
        }
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if args.first().is_some_and(|v| v == "rpc") {
                println!("{}", json!({"error": error}));
            } else {
                eprintln!("{error}");
            }
            std::process::exit(2);
        }
    }
}
