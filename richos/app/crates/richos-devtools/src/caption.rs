//! Offline caption comparison. Matching blocks follow the legacy earliest-match
//! tie order; exact gap alignment prefers near spellings at equal edit counts.
use regex::Regex;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}
pub fn norm(word: &str) -> String {
    re(r"[^\p{L}\p{N}_']")
        .replace_all(
            &word.replace(['\u{2018}', '\u{2019}'], "'").to_lowercase(),
            "",
        )
        .trim_matches('\'')
        .into()
}
fn seconds(s: &str) -> f64 {
    s.split(':')
        .rev()
        .enumerate()
        .map(|(i, s)| s.parse::<f64>().unwrap_or(0.) * 60f64.powi(i as i32))
        .sum()
}
fn clock(t: f64) -> String {
    let t = t as i64;
    format!("{:02}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60)
}

pub fn caption(vtt: &str) -> (String, Vec<f64>) {
    let rolling = vtt.lines().any(|l| l.contains("<c>"));
    let cue = re(r"^\d\d:\d\d(:\d\d)?\.\d{3} --> ");
    let meta = re(r"^(Kind|Language|NOTE)\b");
    let tag = re(r"<[^>]+>");
    let inline = re(r"<(\d\d:\d\d:\d\d\.\d{3})>");
    let (mut output, mut times, mut last, mut start) = (Vec::new(), Vec::new(), None::<String>, 0.);
    for line in vtt.lines() {
        let s = line.trim();
        if cue.is_match(s) {
            start = seconds(s.split(" --> ").next().unwrap());
            continue;
        }
        if s.is_empty() || s == "WEBVTT" || meta.is_match(s) || s.chars().all(|c| c.is_numeric()) {
            continue;
        }
        if rolling {
            if !s.contains("<c>") {
                continue;
            }
            output.push(tag.replace_all(s, "").into_owned());
            let (mut at, mut offset) = (start, 0);
            for m in inline.captures_iter(s) {
                let whole = m.get(0).unwrap();
                times.extend(std::iter::repeat_n(
                    at,
                    tag.replace_all(&s[offset..whole.start()], "")
                        .split_whitespace()
                        .count(),
                ));
                at = seconds(&m[1]);
                offset = whole.end();
            }
            times.extend(std::iter::repeat_n(
                at,
                tag.replace_all(&s[offset..], "").split_whitespace().count(),
            ));
        } else if last.as_deref() != Some(s) {
            output.push(s.into());
            times.extend(std::iter::repeat_n(start, s.split_whitespace().count()));
        }
        last = Some(s.into());
    }
    (output.join(" "), times)
}

/// SequenceMatcher's matching blocks, without junk; popular elements may be
/// excluded as seeds for its default character-ratio operation only.
fn blocks<T: Ord + Clone>(a: &[T], b: &[T], autojunk: bool) -> Vec<(usize, usize, usize)> {
    let mut positions: BTreeMap<T, Vec<usize>> = BTreeMap::new();
    for (j, item) in b.iter().enumerate() {
        positions.entry(item.clone()).or_default().push(j);
    }
    if autojunk && b.len() >= 200 {
        let threshold = b.len() / 100 + 1;
        positions.retain(|_, js| js.len() <= threshold);
    }
    let mut queue = vec![(0, a.len(), 0, b.len())];
    let mut found = Vec::new();
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let (mut ai, mut bj, mut size) = (alo, blo, 0);
        let mut previous = BTreeMap::new();
        for (i, item) in a.iter().enumerate().take(ahi).skip(alo) {
            let mut current = BTreeMap::new();
            if let Some(js) = positions.get(item) {
                for &j in js {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    let k = previous.get(&j.wrapping_sub(1)).copied().unwrap_or(0) + 1;
                    current.insert(j, k);
                    if k > size {
                        ai = i + 1 - k;
                        bj = j + 1 - k;
                        size = k;
                    }
                }
            }
            previous = current;
        }
        while ai > alo && bj > blo && a[ai - 1] == b[bj - 1] {
            ai -= 1;
            bj -= 1;
            size += 1;
        }
        while ai + size < ahi && bj + size < bhi && a[ai + size] == b[bj + size] {
            size += 1;
        }
        if size > 0 {
            found.push((ai, bj, size));
            if alo < ai && blo < bj {
                queue.push((alo, ai, blo, bj));
            }
            if ai + size < ahi && bj + size < bhi {
                queue.push((ai + size, ahi, bj + size, bhi));
            }
        }
    }
    found.sort();
    let mut merged = Vec::new();
    for (i, j, n) in found {
        if let Some((x, y, len)) = merged.last_mut() {
            if *x + *len == i && *y + *len == j {
                *len += n;
                continue;
            }
        }
        merged.push((i, j, n));
    }
    merged.push((a.len(), b.len(), 0));
    merged
}
pub fn close(a: &str, b: &str) -> bool {
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    if a.len() + b.len() == 0 {
        return true;
    }
    let matched: usize = blocks(&a, &b, true).iter().map(|x| x.2).sum();
    2. * matched as f64 / (a.len() + b.len()) as f64 >= 0.6
}
#[derive(Clone, Debug)]
struct Word {
    raw: String,
    normal: String,
    time: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Eq,
    Sub,
    Del,
    Ins,
}
type Edit = (Op, Option<usize>, Option<usize>);
fn cost(a: &str, b: &str) -> f64 {
    if a == b {
        0.
    } else if close(a, b) {
        0.99
    } else {
        1.
    }
}
fn gap(a: &[Word], b: &[Word], ai: usize, bi: usize) -> Vec<Edit> {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0.; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i as f64;
    }
    for (j, v) in d[0].iter_mut().enumerate() {
        *v = j as f64;
    }
    for i in 1..=n {
        for j in 1..=m {
            d[i][j] = (d[i - 1][j] + 1.)
                .min(d[i][j - 1] + 1.)
                .min(d[i - 1][j - 1] + cost(&a[i - 1].normal, &b[j - 1].normal));
        }
    }
    let (mut i, mut j, mut out) = (n, m, Vec::new());
    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && (d[i][j] - (d[i - 1][j - 1] + cost(&a[i - 1].normal, &b[j - 1].normal))).abs() < 1e-9
        {
            out.push((
                if a[i - 1].normal == b[j - 1].normal {
                    Op::Eq
                } else {
                    Op::Sub
                },
                Some(ai + i - 1),
                Some(bi + j - 1),
            ));
            i -= 1;
            j -= 1;
        } else if i > 0 && (d[i][j] - (d[i - 1][j] + 1.)).abs() < 1e-9 {
            out.push((Op::Del, Some(ai + i - 1), None));
            i -= 1;
        } else {
            out.push((Op::Ins, None, Some(bi + j - 1)));
            j -= 1;
        }
    }
    out.reverse();
    out
}
fn align(a: &[Word], b: &[Word]) -> Vec<Edit> {
    let an: Vec<_> = a.iter().map(|w| w.normal.clone()).collect();
    let bn: Vec<_> = b.iter().map(|w| w.normal.clone()).collect();
    let (mut ai, mut bi, mut out) = (0, 0, Vec::new());
    for (i, j, n) in blocks(&an, &bn, false) {
        if i > ai || j > bi {
            if (i - ai) * (j - bi) <= 400 * 400 {
                out.extend(gap(&a[ai..i], &b[bi..j], ai, bi));
            } else {
                let k = (i - ai).min(j - bi);
                for x in 0..k {
                    out.push((
                        if an[ai + x] == bn[bi + x] {
                            Op::Eq
                        } else {
                            Op::Sub
                        },
                        Some(ai + x),
                        Some(bi + x),
                    ));
                }
                for x in ai + k..i {
                    out.push((Op::Del, Some(x), None));
                }
                for x in bi + k..j {
                    out.push((Op::Ins, None, Some(x)));
                }
            }
        }
        for k in 0..n {
            out.push((Op::Eq, Some(i + k), Some(j + k)));
        }
        ai = i + n;
        bi = j + n;
    }
    out
}
fn add(counts: &mut Vec<((String, String), usize)>, a: &str, b: &str) {
    if let Some((_, n)) = counts.iter_mut().find(|((x, y), _)| x == a && y == b) {
        *n += 1;
    } else {
        counts.push(((a.into(), b.into()), 1));
    }
}
fn top(counts: &[((String, String), usize)], n: usize) -> Value {
    let mut ordered = counts.to_vec();
    ordered.sort_by_key(|item| std::cmp::Reverse(item.1));
    json!(ordered
        .iter()
        .take(n)
        .map(|((a, b), n)| json!({"captions":a,"transcript":b,"count":n}))
        .collect::<Vec<_>>())
}
fn repeats(words: &[Word]) -> Value {
    let mut counts = Vec::new();
    for pair in words.windows(2) {
        add(&mut counts, &pair[0].normal, &pair[1].normal);
    }
    counts.sort_by_key(|item| std::cmp::Reverse(item.1));
    counts
        .first()
        .map(|((a, b), n)| json!({"phrase":format!("{a} {b}"),"count":n}))
        .unwrap_or(Value::Null)
}
pub fn compare(input: &Value) -> Result<Value, String> {
    let vtt = input["vtt"].as_str().ok_or("expected captions")?;
    let txt = input["txt"].as_str().ok_or("expected transcript")?;
    let top_n = input["top"].as_i64().unwrap_or(25).max(0) as usize;
    let (text, times) = caption(vtt);
    let reference: Vec<_> = text
        .split_whitespace()
        .enumerate()
        .map(|(i, s)| Word {
            raw: s.into(),
            normal: norm(s),
            time: times.get(i).copied(),
        })
        .filter(|w| !w.normal.is_empty())
        .collect();
    let hypothesis: Vec<_> = txt
        .split_whitespace()
        .map(|s| Word {
            raw: s.into(),
            normal: norm(s),
            time: None,
        })
        .filter(|w| !w.normal.is_empty())
        .collect();
    let ops = align(&reference, &hypothesis);
    let (mut sub, mut del, mut ins, mut caps, mut spell, mut stretches, mut current, mut at) = (
        0,
        0,
        0,
        Vec::new(),
        Vec::new(),
        Vec::<Value>::new(),
        None::<Value>,
        None::<f64>,
    );
    let clean = re(r"[^\p{L}\p{N}_']");
    for (op, r, h) in ops {
        let r = r.map(|i| &reference[i]);
        let h = h.map(|i| &hypothesis[i]);
        match op {
            Op::Sub => {
                sub += 1;
                let (r, h) = (r.unwrap(), h.unwrap());
                if close(&r.normal, &h.normal) {
                    add(&mut spell, &r.normal, &h.normal);
                }
            }
            Op::Del => del += 1,
            Op::Ins => ins += 1,
            Op::Eq => {
                let (r, h) = (r.unwrap(), h.unwrap());
                let rw = clean.replace_all(&r.raw, "");
                let hw = clean.replace_all(&h.raw, "");
                if rw != hw && rw.to_lowercase() == hw.to_lowercase() {
                    add(&mut caps, &rw, &hw);
                }
            }
        }
        if let Some(t) = r.and_then(|w| w.time) {
            at = Some(t);
        }
        if op == Op::Eq {
            if let Some(value) = current.take() {
                stretches.push(value);
            }
            continue;
        }
        let entry = current.get_or_insert_with(
            || json!({"at":clock(at.unwrap_or(0.)),"captions":[],"transcript":[]}),
        );
        if let Some(w) = r {
            entry["captions"].as_array_mut().unwrap().push(json!(w.raw));
        }
        if let Some(w) = h {
            entry["transcript"]
                .as_array_mut()
                .unwrap()
                .push(json!(w.raw));
        }
    }
    if let Some(value) = current {
        stretches.push(value);
    }
    let wer = if reference.is_empty() {
        Value::Null
    } else {
        json!(
            format!("{:.4}", (sub + del + ins) as f64 / reference.len() as f64)
                .parse::<f64>()
                .unwrap()
        )
    };
    let mut result = json!({"caption_words":reference.len(),"transcript_words":hypothesis.len(),"wer":wer,"substitutions":sub,"deletions":del,"insertions":ins,
        "capitalization_differences":caps.iter().map(|v|v.1).sum::<usize>(),"capitalization_top":top(&caps,top_n),"spelling_differences":spell.iter().map(|v|v.1).sum::<usize>(),"spelling_top":top(&spell,top_n),"repeats":{"transcript":repeats(&hypothesis),"captions":repeats(&reference)}});
    if input["disagreements"].as_bool().unwrap_or(false) {
        for item in &mut stretches {
            for key in ["captions", "transcript"] {
                item[key] = json!(item[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(" "));
            }
        }
        result["disagreements"] = json!(stretches);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matcher_preserves_earliest_ties_and_popular_extension() {
        assert_eq!(
            blocks(&['a', 'b', 'a'], &['b', 'a', 'b'], false),
            vec![(0, 1, 2), (3, 3, 0)]
        );
        assert!(close("claude", "cloud"));
        let repeated = "x".repeat(250);
        assert!(close(&repeated, &repeated));
    }
    #[test]
    fn gap_counts_and_spelling_survive() {
        let out=compare(&json!({"vtt":"WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nBefore we use Claude and Tailscale stop","txt":"Before we use cloud uh and Tailscale"})).unwrap();
        assert_eq!(out["substitutions"], 1);
        assert_eq!(out["deletions"], 1);
        assert_eq!(out["insertions"], 1);
    }
    #[test]
    fn unicode_and_empty_normalization() {
        assert_eq!(norm("‘Café!’"), "café");
        assert_eq!(norm("́"), "");
        assert_eq!(
            compare(&json!({"vtt":"","txt":""})).unwrap()["wer"],
            Value::Null
        );
    }
}
