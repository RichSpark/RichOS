//! `richos-search-harness`: the reusable archive-search harness (search PRD v3.1, "Delivery and
//! acceptance": *"Build a small reusable archive-search harness first, using Qdrant and the two
//! retrieval modes. It exposes chunking or embedding mistakes before palette work."*).
//!
//! It drives the same [`SearchService`] the app and Rich's tool use, against a folder named on
//! the command line, with its own index folder. It never reads the app's settings and never
//! opens the app's index.
//!
//! ```text
//! richos-search-harness inventory --root DIR
//! richos-search-harness query     --root DIR --index DIR [--embedder E] [--strict] [--off] [--limit N] QUERY...
//! richos-search-harness compare   --root DIR --index DIR [--embedder E] --questions FILE --out FILE [--repeat N]
//! ```
//!
//! `--embedder` is `apple` (default), `stand-in` (the synthetic concept table, for fixtures), or
//! `gguf:<model.gguf>:<sha256>[:<pooling>]`, which runs Homebrew's `llama-embedding` in batches
//! (the PRD allows a batch command-line tool for the comparison; a persistent runtime is built
//! only if that model wins). The model file must match the SHA-256 given, so the comparison is
//! pinned.
//!
//! **Privacy.** `compare` writes excerpts of the searched files into `--out`. Point `--out` at the
//! private record (richos-hq), never at this repository; the harness refuses an `--out` inside a
//! folder that holds a `.richos/publication-boundary` file.
use richos_search::corpus;
use richos_search::embed::{normalize, ConceptEmbedder, Embedder};
use richos_search::service::{Config, SearchRequest, SearchService, Tuning};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage:\n  richos-search-harness inventory --root DIR\n  richos-search-harness query --root DIR --index DIR [--embedder E] [--strict] [--off] [--limit N] QUERY...\n  richos-search-harness compare --root DIR --index DIR [--embedder E] --questions FILE --out FILE [--repeat N]\n\nE is apple (default), stand-in, or gguf:<model.gguf>:<sha256>[:<pooling>]"
    );
    std::process::exit(2)
}

struct Args {
    command: String,
    root: Option<PathBuf>,
    index: Option<PathBuf>,
    embedder: String,
    strict: bool,
    off: bool,
    limit: usize,
    questions: Option<PathBuf>,
    out: Option<PathBuf>,
    repeat: usize,
    words: Vec<String>,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let command = it.next().unwrap_or_else(|| usage());
    let mut a = Args {
        command,
        root: None,
        index: None,
        embedder: "apple".into(),
        strict: false,
        off: false,
        limit: 5,
        questions: None,
        out: None,
        repeat: 1,
        words: Vec::new(),
    };
    while let Some(arg) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| usage());
        match arg.as_str() {
            // Relative paths are taken from the current folder; the service wants absolute ones.
            "--root" => a.root = Some(std::path::absolute(value()).unwrap_or_else(|_| usage())),
            "--index" => a.index = Some(std::path::absolute(value()).unwrap_or_else(|_| usage())),
            "--embedder" => a.embedder = value(),
            "--questions" => a.questions = Some(PathBuf::from(value())),
            "--out" => a.out = Some(PathBuf::from(value())),
            "--limit" => a.limit = value().parse().unwrap_or_else(|_| usage()),
            "--repeat" => a.repeat = value().parse().unwrap_or_else(|_| usage()),
            "--strict" => a.strict = true,
            "--off" => a.off = true,
            other if other.starts_with("--") => usage(),
            other => a.words.push(other.to_string()),
        }
    }
    a
}

// ---- the GGUF candidate, through llama-embedding in batches -----------------------------

/// Embeds with `llama-embedding`, one process per batch. Passages are embedded in large batches
/// while indexing; a query costs one process start (about half a second, Frank's V2 measurement),
/// which is acceptable for a harness and is exactly why the app would need a persistent runtime.
struct LlamaCliEmbedder {
    model: PathBuf,
    sha256: String,
    pooling: String,
    dimension: usize,
    cache: Mutex<HashMap<String, Vec<f32>>>,
}

const LLAMA_EMBEDDING: &str = "/opt/homebrew/bin/llama-embedding";
const SEPARATOR: &str = "<#richos-sep#>";

impl LlamaCliEmbedder {
    fn new(spec: &str) -> Result<Self, String> {
        let mut parts = spec.splitn(3, ':');
        let model = PathBuf::from(parts.next().ok_or("gguf:<model>:<sha256>")?);
        let sha256 = parts.next().ok_or("gguf:<model>:<sha256> needs the model's SHA-256")?.to_lowercase();
        // `model` (the default) uses the pooling the GGUF file declares.
        let pooling = parts.next().unwrap_or("model").to_string();
        let bytes = std::fs::read(&model).map_err(|e| format!("cannot read {}: {e}", model.display()))?;
        let actual = corpus::content_hash(&bytes);
        if actual != sha256 {
            return Err(format!("{} has SHA-256 {actual}, not the pinned {sha256}", model.display()));
        }
        let mut e = LlamaCliEmbedder { model, sha256, pooling, dimension: 0, cache: Mutex::new(HashMap::new()) };
        e.dimension = e.run(&["dimension probe"])?.first().map(Vec::len).ok_or("the model returned no vector")?;
        Ok(e)
    }

    fn run(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        let scratch = std::env::temp_dir().join(format!("richos-search-llama-{}.txt", std::process::id()));
        // One prompt per text; newlines inside a text become spaces so the separator is the only
        // boundary.
        let body: Vec<String> = texts.iter().map(|t| t.replace(['\n', '\r'], " ")).collect();
        std::fs::write(&scratch, body.join(SEPARATOR)).map_err(|e| e.to_string())?;
        let output = Command::new(LLAMA_EMBEDDING)
            .args(["-m"])
            .arg(&self.model)
            .args(["-f"])
            .arg(&scratch)
            .args(["--embd-separator", SEPARATOR, "--embd-output-format", "json", "--embd-normalize", "2"])
            .args(if self.pooling == "model" { vec![] } else { vec!["--pooling", self.pooling.as_str()] })
            .args(["-c", "512", "-b", "512", "-ub", "512"])
            .output()
            .map_err(|e| format!("cannot run {LLAMA_EMBEDDING}: {e}"))?;
        if let Err(error) = std::fs::remove_file(&scratch) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("harness scratch: {error}");
            }
        }
        if !output.status.success() {
            return Err(format!("llama-embedding failed: {}", String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or("")));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let start = stdout.find('{').ok_or("llama-embedding printed no JSON")?;
        let parsed: Value = serde_json::from_str(&stdout[start..]).map_err(|e| format!("llama-embedding output: {e}"))?;
        let data = parsed["data"].as_array().ok_or("llama-embedding output has no data")?;
        if data.len() != texts.len() {
            return Err(format!("llama-embedding returned {} vectors for {} texts", data.len(), texts.len()));
        }
        Ok(data
            .iter()
            .map(|d| {
                let mut v: Vec<f32> = d["embedding"].as_array().map(|a| a.iter().filter_map(Value::as_f64).map(|x| x as f32).collect()).unwrap_or_default();
                normalize(&mut v);
                v
            })
            .collect())
    }

    /// Embed every passage of the folder ahead of indexing, in batches, so indexing reads the
    /// cache instead of starting one process per passage.
    fn prime(&self, texts: Vec<String>) -> Result<(), String> {
        for chunk in texts.chunks(256) {
            let refs: Vec<&str> = chunk.iter().map(String::as_str).collect();
            let vectors = self.run(&refs)?;
            let mut cache = self.cache.lock().unwrap();
            for (t, v) in chunk.iter().zip(vectors) {
                cache.insert(t.clone(), v);
            }
        }
        Ok(())
    }
}

impl Embedder for LlamaCliEmbedder {
    fn identity(&self) -> String {
        format!("gguf:{}:{}:{}:{}", self.model.file_name().and_then(|n| n.to_str()).unwrap_or("model"), &self.sha256[..12], self.pooling, self.dimension)
    }
    fn dimension(&self) -> usize {
        self.dimension
    }
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        let missing: Vec<&str> = {
            let cache = self.cache.lock().unwrap();
            texts.iter().copied().filter(|t| !cache.contains_key(*t)).collect()
        };
        if !missing.is_empty() {
            let vectors = self.run(&missing)?;
            let mut cache = self.cache.lock().unwrap();
            for (t, v) in missing.iter().zip(vectors) {
                cache.insert(t.to_string(), v);
            }
        }
        let cache = self.cache.lock().unwrap();
        Ok(texts.iter().map(|t| cache[*t].clone()).collect())
    }
}

/// Shares one embedder between the service (which owns a box) and the harness.
struct Shared(std::sync::Arc<dyn Embedder>);
impl Embedder for Shared {
    fn identity(&self) -> String {
        self.0.identity()
    }
    fn dimension(&self) -> usize {
        self.0.dimension()
    }
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        self.0.embed(texts)
    }
}

fn make_embedder(spec: &str, root: &Path) -> Result<std::sync::Arc<dyn Embedder>, String> {
    if spec == "stand-in" {
        return Ok(std::sync::Arc::new(ConceptEmbedder::stand_in()));
    }
    if spec == "apple" {
        #[cfg(target_os = "macos")]
        return Ok(std::sync::Arc::new(richos_search::embed::AppleSentenceEmbedder::new("en")?));
        #[cfg(not(target_os = "macos"))]
        return Err("Apple's embedding exists only on macOS".into());
    }
    if let Some(rest) = spec.strip_prefix("gguf:") {
        let e = LlamaCliEmbedder::new(rest)?;
        let c = corpus::load(root)?;
        let texts: Vec<String> = c
            .docs
            .iter()
            .flat_map(|d| d.passages.iter().map(move |p| richos_search::service::embedding_text(&d.title, &p.heading, p.text(&d.text))))
            .collect();
        let started = Instant::now();
        e.prime(texts.clone())?;
        eprintln!("gguf: embedded {} passages in {:.1} s", texts.len(), started.elapsed().as_secs_f64());
        return Ok(std::sync::Arc::new(e));
    }
    Err(format!("unknown embedder {spec}"))
}

fn start(args: &Args) -> (SearchService, std::sync::Arc<dyn Embedder>, f64) {
    let root = args.root.clone().unwrap_or_else(|| usage());
    let index = args.index.clone().unwrap_or_else(|| usage());
    let embedder = make_embedder(&args.embedder, &root).unwrap_or_else(|e| {
        eprintln!("embedder: {e}");
        std::process::exit(1)
    });
    let shared = embedder.clone();
    let started = Instant::now();
    let service = SearchService::start(Config {
        root: Some(root),
        cache: Ok(index),
        make_embedder: Box::new(move || Ok(Box::new(Shared(shared.clone())) as Box<dyn Embedder>)),
        busy: None,
        tuning: Tuning::default(),
    });
    service.wait_idle();
    let seconds = started.elapsed().as_secs_f64();
    (service, embedder, seconds)
}

fn inventory(args: &Args) {
    let root = args.root.clone().unwrap_or_else(|| usage());
    let c = corpus::load(&root).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let mut sizes: Vec<(u64, &str, usize)> = c.docs.iter().map(|d| (d.size, d.rel_path.as_str(), d.passages.len())).collect();
    sizes.sort();
    let bytes: u64 = sizes.iter().map(|s| s.0).sum();
    let passages = c.passage_count();
    // Language, roughly: the share of letters outside basic Latin, and the share of words that
    // are common English function words. A screen for "is this mostly English", not a detector.
    let mut letters = 0usize;
    let mut non_latin = 0usize;
    let mut words = 0usize;
    let mut english = 0usize;
    for d in &c.docs {
        for ch in d.text.chars().filter(|c| c.is_alphabetic()) {
            letters += 1;
            if !ch.is_ascii() && !('\u{00C0}'..='\u{024F}').contains(&ch) {
                non_latin += 1;
            }
        }
        for w in richos_search::text::fold(&d.text).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
            words += 1;
            if ["the", "and", "is", "to", "of", "a", "in", "that", "it", "you"].contains(&w) {
                english += 1;
            }
        }
    }
    let pick = |i: usize| sizes.get(i).map(|s| json!({"path": s.1, "bytes": s.0, "passages": s.2}));
    let n = sizes.len();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "files": n,
            "bytes": bytes,
            "passages": passages,
            "skipped": c.skipped.iter().map(|s| json!({"path": s.rel_path, "reason": s.reason})).collect::<Vec<_>>(),
            "links_not_followed": c.links_skipped,
            "letters_outside_latin_share": if letters > 0 { non_latin as f64 / letters as f64 } else { 0.0 },
            "english_function_word_share": if words > 0 { english as f64 / words as f64 } else { 0.0 },
            "representative_files": {
                "short": if n > 0 { pick(n / 10) } else { None },
                "median": if n > 0 { pick(n / 2) } else { None },
                "long": if n > 0 { pick(n - 1) } else { None },
            }
        }))
        .unwrap()
    );
}

fn query(args: &Args) {
    let (service, _, seconds) = start(args);
    eprintln!("ready in {seconds:.1} s: {:?}", service.status().meaning);
    let response = service.search(&SearchRequest {
        query: args.words.join(" "),
        super_finder: !args.off,
        strict: args.strict,
        offset: 0,
        limit: args.limit,
    });
    println!("{}", serde_json::to_string_pretty(&response).unwrap());
    service.shutdown();
}

#[derive(Deserialize)]
struct Expected {
    file: String,
    /// A phrase the supporting passage must contain (case and spacing folded). Empty: any
    /// passage of the file counts.
    #[serde(default)]
    passage_contains: String,
}

#[derive(Deserialize)]
struct Question {
    id: String,
    query: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    strict: bool,
    /// Empty for a question with no answer in the folder.
    #[serde(default)]
    expected: Vec<Expected>,
}

#[derive(Deserialize)]
struct Questions {
    questions: Vec<Question>,
}

fn refuse_public_out(out: &Path) {
    let absolute = if out.is_absolute() { out.to_path_buf() } else { std::env::current_dir().unwrap().join(out) };
    let mut dir = absolute.parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        if d.join(".richos/publication-boundary").exists() {
            eprintln!("refusing --out inside {}: that repository is published, and compare output quotes the searched files", d.display());
            std::process::exit(2);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

fn compare(args: &Args) {
    let out = args.out.clone().unwrap_or_else(|| usage());
    refuse_public_out(&out);
    let questions: Questions = serde_json::from_str(&std::fs::read_to_string(args.questions.as_ref().unwrap_or_else(|| usage())).unwrap_or_else(|e| {
        eprintln!("questions: {e}");
        std::process::exit(1)
    }))
    .unwrap_or_else(|e| {
        eprintln!("questions: {e}");
        std::process::exit(1)
    });
    let (service, embedder, build_seconds) = start(args);
    let status = service.status();
    let mut rows = Vec::new();
    let (mut on_found, mut off_found, mut answerable) = (0, 0, 0);
    let mut recovered = Vec::new();
    let mut regressions = Vec::new();
    let mut times: HashMap<&str, Vec<f64>> = HashMap::new();
    for q in &questions.questions {
        let mut per_mode = serde_json::Map::new();
        let mut found_in = HashMap::new();
        for (mode, super_finder) in [("on", true), ("off", false)] {
            let request = SearchRequest { query: q.query.clone(), super_finder, strict: q.strict, offset: 0, limit: 5 };
            let response = service.search(&request);
            times.entry(mode).or_default().push(response.elapsed_ms);
            let hit_rank = response.hits.iter().position(|h| {
                q.expected.iter().any(|e| {
                    h.rel_path == e.file
                        && (e.passage_contains.is_empty()
                            || richos_search::text::fold(&h.passage.text).contains(&richos_search::text::fold(&e.passage_contains)))
                })
            });
            found_in.insert(mode, hit_rank.is_some());
            per_mode.insert(
                mode.into(),
                json!({
                    "effective_super_finder": response.effective.super_finder,
                    "found_at": hit_rank.map(|r| r + 1),
                    "elapsed_ms": response.elapsed_ms,
                    "top5": response.hits.iter().map(|h| json!({
                        "file": h.rel_path, "title": h.title, "why": h.why, "line": h.passage.line,
                        "excerpt": h.passage.text.chars().take(240).collect::<String>(),
                    })).collect::<Vec<_>>(),
                }),
            );
        }
        if !q.expected.is_empty() {
            answerable += 1;
            let on = found_in["on"];
            let off = found_in["off"];
            on_found += on as usize;
            off_found += off as usize;
            if on && !off {
                recovered.push(q.id.clone());
            }
            if off && !on {
                regressions.push(q.id.clone());
            }
        }
        rows.push(json!({"id": q.id, "kind": q.kind, "query": q.query, "strict": q.strict, "answerable": !q.expected.is_empty(), "modes": per_mode}));
    }
    // Further rounds for latency only: every call differs from the one before it (another
    // question or another mode), so none is answered from the one-entry result cache.
    for _ in 1..args.repeat.max(1) {
        for q in &questions.questions {
            for (mode, super_finder) in [("on", true), ("off", false)] {
                let r = service.search(&SearchRequest { query: q.query.clone(), super_finder, strict: q.strict, offset: 0, limit: 5 });
                times.entry(mode).or_default().push(r.elapsed_ms);
            }
        }
    }
    let mut latency = serde_json::Map::new();
    for (mode, mut t) in times {
        t.sort_by(|a, b| a.partial_cmp(b).unwrap());
        latency.insert(mode.into(), json!({"p50_ms": percentile(&t, 50.0), "p95_ms": percentile(&t, 95.0), "max_ms": t.last().copied().unwrap_or(0.0), "n": t.len()}));
    }
    let report = json!({
        "embedder": embedder.identity(),
        "status": status,
        "index_build_seconds": build_seconds,
        "answerable_questions": answerable,
        "found_in_first_five": {"super_finder_on": on_found, "super_finder_off": off_found},
        "recovered_text_misses": recovered,
        "regressions": regressions,
        "service_latency_ms": latency,
        "questions": rows,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap_or_else(|e| {
        eprintln!("cannot write {}: {e}", out.display());
        std::process::exit(1)
    });
    println!(
        "{}: {answerable} answerable; first five with Super Finder on {on_found}, off {off_found}; recovered {}, regressions {}; written to {}",
        embedder.identity(),
        recovered.len(),
        regressions.len(),
        out.display()
    );
    service.shutdown();
}

fn main() {
    let args = parse_args();
    match args.command.as_str() {
        "inventory" => inventory(&args),
        "query" => query(&args),
        "compare" => compare(&args),
        _ => usage(),
    }
}
