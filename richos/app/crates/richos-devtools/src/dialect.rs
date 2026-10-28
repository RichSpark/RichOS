//! Reproducible dictionary generation from pinned data. No runtime spelling
//! policy or checked-in dictionary bytes change when this developer tool moves.
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
type Pairs = BTreeMap<String, (String, String)>;
type Words = BTreeMap<String, String>;
type Ambiguous = BTreeMap<String, Vec<String>>;

fn re(s: &str) -> Regex {
    Regex::new(s).unwrap()
}
fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("IO: {e}"))
}
fn path<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input[key].as_str().ok_or_else(|| format!("missing {key}"))
}
fn decode<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}

pub fn read_varcon(path: &str, pin: &str) -> Result<String, String> {
    let raw = std::fs::read(path).map_err(|e| format!("IO: {e}"))?;
    let hash = format!("{:x}", Sha256::digest(&raw));
    if hash != pin {
        return Err(format!("{path} has sha256 {hash}, but the pin is {pin}. The pinned input changed; re-vendor deliberately and update VARCON_SHA256, the registry row and the notices together."));
    }
    Ok(raw.into_iter().map(char::from).collect())
}
pub fn parse_clusters(text: &str) -> Value {
    let head = re(r"^#\s*(\S+)\s*(<verified>)?\s*\(level (\d+)\)");
    let tags = re(r"^([ABZCD_])([.vV\-x]?)$");
    let braces = re(r"\s*\{[^}]*\}");
    let suffix = re(r"\s*<[^>]*>$");
    let mut clusters = Vec::<Value>::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('#') {
            if let Some(m) = head.captures(line) {
                clusters.push(json!({"head":&m[1],"verified":m.get(2).is_some(),"level":m[3].parse::<u64>().unwrap(),"lines":[]}));
            }
            continue;
        }
        let Some(current) = clusters.last_mut() else {
            continue;
        };
        let body = line.split(" | ").next().unwrap();
        let body = braces.replace_all(body, "");
        let mut variants = Vec::new();
        for part in body.split(" / ") {
            let Some((cats, word)) = part.split_once(": ") else {
                continue;
            };
            let word = suffix.replace(word.trim(), "");
            let ts: Vec<_> = cats
                .split_whitespace()
                .filter_map(|t| tags.captures(t))
                .map(|m| json!([&m[1], &m[2]]))
                .collect();
            variants.push(json!([word, ts]));
        }
        current["lines"]
            .as_array_mut()
            .unwrap()
            .push(json!(variants));
    }
    json!(clusters)
}
pub fn split_varcon(clusters: &Value, max_level: u64) -> Result<(Words, Words, Ambiguous), String> {
    let clusters = clusters.as_array().ok_or("expected clusters")?;
    let (mut preferred, mut candidates, mut metadata) = (
        BTreeSet::<String>::new(),
        BTreeMap::<String, BTreeSet<String>>::new(),
        BTreeMap::<String, (bool, u64)>::new(),
    );
    for cluster in clusters {
        for variants in cluster["lines"]
            .as_array()
            .ok_or("expected variant lines")?
        {
            let variants = variants.as_array().ok_or("expected variants")?;
            let has = |v: &Value, category: &str, levels: Option<&[&str]>| {
                v[1].as_array().is_some_and(|tags| {
                    tags.iter().any(|t| {
                        t[0] == category && levels.is_none_or(|ls| ls.iter().any(|l| t[1] == *l))
                    })
                })
            };
            let preferred_a: Vec<_> = variants
                .iter()
                .filter(|v| has(v, "A", Some(&[""])))
                .filter_map(|v| v[0].as_str())
                .collect();
            for v in variants {
                let word = v[0].as_str().ok_or("expected word")?;
                if has(v, "A", Some(&["", "."])) {
                    preferred.insert(word.into());
                }
                let british = v[1]
                    .as_array()
                    .is_some_and(|ts| ts.iter().any(|t| t[0] == "B" && t[1] != "x"));
                if british && !preferred_a.is_empty() {
                    for a in &preferred_a {
                        if *a != word {
                            candidates
                                .entry(word.into())
                                .or_default()
                                .insert((*a).into());
                        }
                    }
                    let meta = metadata.entry(word.into()).or_insert((true, 0));
                    meta.0 &= cluster["verified"].as_bool().unwrap_or(false);
                    meta.1 = meta.1.max(cluster["level"].as_u64().unwrap_or(0));
                }
            }
        }
    }
    let (mut verified, mut any, mut ambiguous) = (Words::new(), Words::new(), Ambiguous::new());
    for (word, targets) in candidates {
        if word.contains('\'') || word.contains(' ') {
            continue;
        }
        if preferred.contains(&word) || targets.len() != 1 {
            ambiguous.insert(word, targets.into_iter().collect());
            continue;
        }
        let target = targets.into_iter().next().unwrap();
        any.insert(word.clone(), target.clone());
        let meta = metadata[&word];
        if meta.0 && meta.1 <= max_level && word == word.to_lowercase() {
            verified.insert(word, target);
        }
    }
    Ok((verified, any, ambiguous))
}
pub fn read_pairs(file: &str, leave: bool) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    for (n, raw) in read(file)?.lines().enumerate() {
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        let parts: Vec<_> = raw.split('\t').collect();
        if (if leave {
            parts.len() != 2
        } else {
            parts.len() < 2
        }) || parts[0].trim().is_empty()
            || parts[1].trim().is_empty()
        {
            return Err(format!(
                "{file}:{} is not {}",
                n + 1,
                if leave {
                    "<form><TAB><why it stays>"
                } else {
                    "<form><TAB><target>[<TAB>options]"
                }
            ));
        }
        let form = parts[0].trim().to_lowercase();
        if out.contains_key(&form) {
            return Err(format!("{file}:{} repeats '{form}'", n + 1));
        }
        out.insert(
            form,
            if leave {
                json!(parts[1].trim())
            } else {
                json!([
                    parts[1].trim(),
                    parts.get(2).map(|s| s.trim()).unwrap_or("")
                ])
            },
        );
    }
    Ok(json!(out))
}
pub fn assemble(
    verified: &Words,
    any: &Words,
    ambiguous: &Ambiguous,
    hand: &Pairs,
    overrides: &Pairs,
    leave: &Words,
) -> Result<(Pairs, Value), String> {
    let mut problems = Vec::new();
    for (form, (target, _)) in hand {
        if let Some(options) = ambiguous.get(form) {
            problems.push(format!("../dialect-en-US.dict carries '{form}', which VarCon marks as a preferred American spelling in some sense ({})",options.join(", ")));
        } else if let Some(actual) = verified.get(form) {
            if actual != target {
                problems.push(format!("../dialect-en-US.dict maps '{form}' to '{target}'; VarCon's verified form is '{actual}'"));
            }
        }
    }
    for (form, (target, _)) in overrides {
        if hand.contains_key(form) {
            problems.push(format!(
                "override '{form}' is already in ../dialect-en-US.dict; keep one source"
            ));
        } else if verified.contains_key(form) {
            problems.push(format!(
                "override '{form}' is already in VarCon's verified table; it is not an override"
            ));
        } else if let Some(options) = ambiguous.get(form) {
            problems.push(format!(
                "override '{form}' is a preferred American spelling in some sense ({})",
                options.join(", ")
            ));
        } else if let Some(actual) = any.get(form) {
            if actual != target {
                problems.push(format!("override maps '{form}' to '{target}'; VarCon's (unverified) form is '{actual}'"));
            }
        }
    }
    for form in leave.keys() {
        if hand.contains_key(form) || overrides.contains_key(form) {
            problems.push(format!(
                "'{form}' is both left alone and added by a hand-kept list"
            ));
        } else if !verified.contains_key(form) {
            problems.push(format!(
                "leave entry '{form}' removes nothing: the table would not carry it"
            ));
        }
    }
    if !problems.is_empty() {
        return Err(format!(
            "the RichOS lists disagree with VarCon or with each other:\n  - {}",
            problems.join("\n  - ")
        ));
    }
    let mut table: Pairs = verified
        .iter()
        .filter(|(form, _)| !leave.contains_key(*form))
        .map(|(f, t)| (f.clone(), (t.clone(), String::new())))
        .collect();
    table.extend(overrides.clone());
    table.extend(hand.clone());
    let word = re(r"^[a-z]+$");
    let hex = re(r"^[a-f0-9]+$");
    for (form, (target, _)) in &table {
        if !word.is_match(form) || !word.is_match(target) {
            problems.push(format!(
                "'{form}' -> '{target}' is not lowercase ASCII letters on both sides"
            ));
        }
        if hex.is_match(form) {
            problems.push(format!(
                "key '{form}' is hex-only, so it could match part of a commit SHA"
            ));
        }
        if form == target {
            problems.push(format!("'{form}' maps to itself"));
        }
        if table.contains_key(target) {
            problems.push(format!(
                "'{form}' -> '{target}', and '{target}' is itself a key (a chain)"
            ));
        }
    }
    if !problems.is_empty() {
        return Err(format!(
            "the assembled table fails its invariants:\n  - {}",
            problems.join("\n  - ")
        ));
    }
    let same = hand
        .iter()
        .filter(|(f, (t, _))| verified.get(*f) == Some(t))
        .count();
    let stats = json!({"verified":verified.len(),"ambiguous":ambiguous.len(),"left":leave.len(),"overrides":overrides.len(),"hand":hand.len(),"hand_same":same,"hand_only":hand.len()-same,"table":table.len()});
    Ok((table, stats))
}
pub fn render(table: &Pairs, stats: &Value, pin: &str, revision: &str, max: u64) -> String {
    let mut head = vec![
        "dialect-en-US.generated.dict - GENERATED. DO NOT EDIT BY HAND.".into(),
        "".into(),
        "Regenerate: python3 richos/engine/scripts/lib/dialect/dialect-table.py".into(),
        "Verify:     python3 richos/engine/scripts/lib/dialect/dialect-table.py --check".into(),
        "".into(),
        "British-to-American spelling pairs, one per line, TAB-separated, in the format".into(),
        "../dialect-en-US.dict uses: <form><TAB><American form>[<TAB>ext-exempt=...].".into(),
        "Keys are lowercase ASCII; a reader matches case-insensitively and re-cases.".into(),
        "".into(),
        "THIS IS A MODIFIED VERSION OF VARCON. It is a generated subset of".into(),
        format!("{revision},"),
        format!("input sha256 {pin}:"),
        format!("only verified clusters at SCOWL level {max} or below, only British forms with"),
        "exactly one preferred American form and no preferred-American use in any".into(),
        "sense, reformatted to one pair per line, with RichOS additions and removals".into(),
        "merged in from dialect-en-US.overrides.dict, dialect-en-US.leave.tsv and".into(),
        "../dialect-en-US.dict. The unmodified data and its README are in".into(),
        "third_party/varcon/; the full notices are also in".into(),
        "docs/legal/THIRD-PARTY-NOTICES.md.".into(),
        "".into(),
        format!(
            "{} pairs = {} from VarCon's verified clusters - {} left alone",
            stats["table"], stats["verified"], stats["left"]
        ),
        format!(
            "  + {} overrides + {} from ../dialect-en-US.dict not already in VarCon's set",
            stats["overrides"], stats["hand_only"]
        ),
        format!(
            "  ({} of that dictionary's {} lines match VarCon exactly; none disagree).",
            stats["hand_same"], stats["hand"]
        ),
        "".into(),
        "The copyright notices VarCon's README requires in all copies:".into(),
        "".into(),
    ];
    head.extend(
        include_str!("varcon_notice.txt")
            .trim_end_matches('\n')
            .split('\n')
            .map(String::from),
    );
    let mut lines: Vec<_> = head
        .iter()
        .map(|s| format!("# {s}").trim_end().to_string())
        .collect();
    lines.push(String::new());
    for (form, (target, options)) in table {
        lines.push(format!(
            "{form}\t{target}{}",
            if options.is_empty() {
                String::new()
            } else {
                format!("\t{options}")
            }
        ));
    }
    lines.join("\n") + "\n"
}
pub fn rpc(op: &str, input: &Value) -> Result<Value, String> {
    match op {
        "dialect-read-varcon" => {
            read_varcon(path(input, "path")?, path(input, "pin")?).map(|s| json!(s))
        }
        "dialect-parse" => Ok(parse_clusters(
            input.as_str().ok_or("expected VarCon text")?,
        )),
        "dialect-split" => {
            let (a, b, c) = split_varcon(
                &input["clusters"],
                input["max_level"].as_u64().unwrap_or(70),
            )?;
            Ok(json!([a, b, c]))
        }
        "dialect-read-pairs" => read_pairs(
            path(input, "path")?,
            input["leave"].as_bool().unwrap_or(false),
        ),
        "dialect-assemble" => {
            let (table, stats) = assemble(
                &decode(&input["verified"])?,
                &decode(&input["any_tier"])?,
                &decode(&input["ambiguous"])?,
                &decode(&input["hand"])?,
                &decode(&input["overrides"])?,
                &decode(&input["leave"])?,
            )?;
            Ok(json!([table, stats]))
        }
        "dialect-render" => Ok(json!(render(
            &decode(&input["table"])?,
            &input["stats"],
            path(input, "pin")?,
            path(input, "revision")?,
            input["max_level"].as_u64().unwrap_or(70)
        ))),
        "dialect-generate" => {
            let clusters = parse_clusters(&read_varcon(
                path(input, "varcon_path")?,
                path(input, "expected_sha256")?,
            )?);
            let max = input["max_level"].as_u64().unwrap_or(70);
            let (verified, any, ambiguous) = split_varcon(&clusters, max)?;
            let hand = decode(&read_pairs(path(input, "dict_path")?, false)?)?;
            let overrides = decode(&read_pairs(path(input, "overrides_path")?, false)?)?;
            let leave = decode(&read_pairs(path(input, "leave_path")?, true)?)?;
            let (table, mut stats) =
                assemble(&verified, &any, &ambiguous, &hand, &overrides, &leave)?;
            let cs = clusters.as_array().unwrap();
            stats["clusters"] = json!(cs.len());
            stats["verified_clusters"] = json!(cs.iter().filter(|c| c["verified"] == true).count());
            let text = render(
                &table,
                &stats,
                path(input, "pin")?,
                path(input, "revision")?,
                max,
            );
            Ok(json!([text, table, stats]))
        }
        _ => Err("unknown dictionary operation".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_and_unverified_forms_are_separate() {
        let c=parse_clusters("# one <verified> (level 50)\nA: token / B: variant\n# two (level 80)\nA: second / B: unproven\n# three <verified> (level 20)\nA: variant / B: other\n");
        let (verified, any, ambiguous) = split_varcon(&c, 70).unwrap();
        assert!(!verified.contains_key("variant"));
        assert!(ambiguous.contains_key("variant"));
        assert_eq!(any["unproven"], "second");
        assert!(!verified.contains_key("unproven"));
    }
    #[test]
    fn duplicates_and_stale_removals_refuse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input");
        std::fs::write(&path, "token\tother\ntoken\tother\n").unwrap();
        assert!(read_pairs(path.to_str().unwrap(), false)
            .unwrap_err()
            .contains("repeats"));
        assert!(assemble(
            &Words::new(),
            &Words::new(),
            &Ambiguous::new(),
            &Pairs::new(),
            &Pairs::new(),
            &BTreeMap::from([("token".into(), "reason".into())])
        )
        .unwrap_err()
        .contains("removes nothing"));
    }
}
