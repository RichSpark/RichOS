use regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn cargo_summary(text: &str) -> Result<Value, String> {
    let result =
        Regex::new(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;")
            .unwrap();
    let docs = Regex::new(r"^\s*Doc-tests\s+").unwrap();
    let ordinary = Regex::new(r"^\s*(?:Running|Finished)\s+").unwrap();
    let mut counts = [[0u64; 4]; 2];
    let mut kind = 0;
    for line in text.lines() {
        if docs.is_match(line) {
            kind = 1;
        } else if ordinary.is_match(line) {
            kind = 0;
        }
        if let Some(m) = result.captures(line) {
            counts[kind][3] += 1;
            for i in 0..3 {
                counts[kind][i] += m[i + 2].parse::<u64>().map_err(|e| e.to_string())?;
            }
        }
    }
    if counts[0][3] == 0 {
        return Err(
            "no ordinary test result found; an empty or compiler-error log is not a passing run"
                .into(),
        );
    }
    let row = |c: [u64; 4]| json!({"passed":c[0],"failed":c[1],"ignored":c[2],"suites":c[3]});
    let mut out = json!({"ordinary":row(counts[0]),"documentation":row(counts[1]),"process_exit_verified":false});
    out["ordinary"]["total"] = json!(counts[0][..3].iter().sum::<u64>());
    Ok(out)
}

fn string(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::String(s) => s.clone(),
        Value::Bool(v) => if *v { "True" } else { "False" }.into(),
        _ => value.to_string(),
    }
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn cut(v: &Value, full: bool) -> String {
    let text = if truth(v) { string(v) } else { String::new() }.replace('\n', " / ");
    if full || text.chars().count() <= 300 {
        text
    } else {
        format!("{} ...", text.chars().take(300).collect::<String>())
    }
}
fn at(row: &Value) -> f64 {
    row["at"]
        .as_f64()
        .unwrap_or_else(|| if row["at"] == true { 1. } else { 0. })
}
fn key(row: &Value) -> String {
    row["turn_id"].to_string()
}
fn turn(row: &Value) -> String {
    if truth(&row["turn_id"]) {
        string(&row["turn_id"])
    } else {
        String::new()
    }
}

pub fn ledger_read(path: &str) -> Result<Value, String> {
    if !std::path::Path::new(path).is_file() {
        return Ok(json!([null, format!("no ledger at {path}")]));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                return Ok(json!([
                    null,
                    format!("line {} of {path} is not JSON", n + 1)
                ]))
            }
        };
        if !row.is_object() || !(row["at"].is_number() || row["at"].is_boolean()) {
            return Ok(json!([
                null,
                format!("line {} of {path} has no `at` time", n + 1)
            ]));
        }
        rows.push(row);
    }
    Ok(if rows.is_empty() {
        json!([null, format!("{path} holds no rows")])
    } else {
        json!([rows, null])
    })
}

pub fn ledger_timeline(input: &Value) -> Result<Value, String> {
    let rows = input["rows"].as_array().ok_or("expected ledger rows")?;
    let thread = input["thread"].as_str().filter(|s| !s.is_empty());
    let rows: Vec<_> = rows
        .iter()
        .filter(|r| {
            thread.is_none() || r["thread_id"].is_null() || r["thread_id"].as_str() == thread
        })
        .collect();
    let Some(first) = rows.first() else {
        return Ok(json!([]));
    };
    let t0 = at(first);
    let full = input["full"].as_bool().unwrap_or(false);
    let mut replies: BTreeMap<String, (f64, String)> = BTreeMap::new();
    for r in &rows {
        if r["event"] == "AssistantDelta" {
            let entry = replies.entry(key(r)).or_insert((at(r), String::new()));
            entry
                .1
                .push_str(&string(r.get("text").unwrap_or(&json!(""))));
        }
    }
    let mut said = BTreeSet::new();
    let mut out = Vec::new();
    for r in rows {
        let name = turn(r);
        let suffix = name
            .chars()
            .skip(name.chars().count().saturating_sub(6))
            .collect::<String>();
        if r["event"] == "AssistantDelta" {
            if !said.insert(name) {
                continue;
            }
            let (time, text) = &replies[&key(r)];
            out.push(format!(
                "{:8.1} REPLY - {suffix} {}",
                (time - t0) / 1000.,
                cut(&json!(text), full)
            ));
        } else {
            let what = ["text", "stop_reason", "kind"]
                .iter()
                .find_map(|k| r.get(*k).filter(|v| truth(v)))
                .cloned()
                .unwrap_or(json!(""));
            let source = r
                .get("source")
                .filter(|v| truth(v))
                .map(string)
                .unwrap_or("-".into());
            out.push(format!(
                "{:8.1} {} {source} {suffix} {}",
                (at(r) - t0) / 1000.,
                string(&r["event"]),
                cut(&what, full)
            ));
        }
    }
    Ok(json!(out))
}

pub fn ledger_turns(input: &Value) -> Result<Value, String> {
    let rows = input["rows"].as_array().ok_or("expected ledger rows")?;
    let full = input["full"].as_bool().unwrap_or(false);
    let (mut sent, mut first, mut done) = (
        Vec::<(String, &Value)>::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    );
    for r in rows {
        let k = key(r);
        match r["event"].as_str().unwrap_or("") {
            "PromptReceived"
                if !["internal", "proactive"].contains(&r["source"].as_str().unwrap_or("")) =>
            {
                if let Some(old) = sent.iter_mut().find(|(id, _)| id == &k) {
                    old.1 = r;
                } else {
                    sent.push((k, r));
                }
            }
            "AssistantDelta" if sent.iter().any(|(id, _)| id == &k) => {
                first.entry(k).or_insert(at(r));
            }
            "TurnCompleted" | "TurnInterrupted" | "TurnStopped" | "TurnFailed"
                if sent.iter().any(|(id, _)| id == &k) =>
            {
                done.insert(k, (at(r), string(&r["event"])));
            }
            _ => {}
        }
    }
    Ok(json!(sent
        .iter()
        .map(|(k, r)| {
            let f = first
                .get(k)
                .map(|v| format!("{:.1}s", (v - at(r)) / 1000.))
                .unwrap_or("none".into());
            let d = done
                .get(k)
                .map(|(v, e)| format!("{:.1}s {e}", (v - at(r)) / 1000.))
                .unwrap_or("not completed".into());
            format!(
                "sent {} first reply {f} end {d} | {}",
                string(&r["at"]),
                cut(&r["text"], full)
            )
        })
        .collect::<Vec<_>>()))
}

pub fn ledger_cli(args: &[String]) -> Result<i32, String> {
    if args.first().is_some_and(|s| s == "--help" || s == "-h") {
        println!("ledger-timeline.py LEDGER.jsonl [--thread ID] [--full] [--turns]");
        return Ok(0);
    }
    let path = args
        .first()
        .ok_or("ledger-timeline.py LEDGER.jsonl [--thread ID] [--full] [--turns]: see --help")?;
    let (mut thread, mut full, mut turns) = (Value::Null, false, false);
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--thread" => {
                i += 1;
                thread = json!(args.get(i).ok_or("missing thread ID")?);
            }
            "--full" => full = true,
            "--turns" => turns = true,
            _ => {
                return Err(
                    "ledger-timeline.py LEDGER.jsonl [--thread ID] [--full] [--turns]: see --help"
                        .into(),
                )
            }
        }
        i += 1;
    }
    let read = ledger_read(path)?;
    if !read[1].is_null() {
        return Err(format!(
            "ledger-timeline.py: cannot read the ledger: {}",
            read[1].as_str().unwrap()
        ));
    }
    let input = json!({"rows":read[0],"thread":thread,"full":full});
    let result = if turns {
        ledger_turns(&input)?
    } else {
        ledger_timeline(&input)?
    };
    println!(
        "{}",
        result
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cargo_counts_do_not_verify_process_exit() {
        let out=cargo_summary("Running tests/a\ntest result: ok. 3 passed; 0 failed; 1 ignored;\nDoc-tests core\ntest result: ok. 2 passed; 0 failed; 0 ignored;\n").unwrap();
        assert_eq!(out["ordinary"]["passed"], 3);
        assert_eq!(out["documentation"]["passed"], 2);
        assert_eq!(out["process_exit_verified"], false);
        assert!(cargo_summary("error: compilation failed").is_err());
    }
    #[test]
    fn ledger_joins_deltas_and_keeps_first_timestamp() {
        let rows = json!([{"at":1000,"event":"PromptReceived","turn_id":"abcdefghi","text":"hello"},{"at":2000,"event":"AssistantDelta","turn_id":"abcdefghi","text":"a"},{"at":3000,"event":"AssistantDelta","turn_id":"abcdefghi","text":"b"},{"at":4000,"event":"TurnCompleted","turn_id":"abcdefghi"}]);
        let out = ledger_timeline(&json!({"rows":rows})).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 3);
        assert_eq!(out[1], "     1.0 REPLY - defghi ab");
        assert_eq!(
            ledger_turns(&json!({"rows":rows})).unwrap()[0],
            "sent 1000 first reply 1.0s end 3.0s TurnCompleted | hello"
        );
    }
}
