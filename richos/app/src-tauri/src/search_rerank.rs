//! Score each query and retrieved passage together with the bundled relevance models.
use ort::{environment::ThreadManager, session::Session, value::Tensor};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use richos_search::{embed::Embedder, service::Reranker};
use tokenizers::{Encoding, Tokenizer, TruncationParams};

const TOKEN_LIMIT: usize = 192;
const BUCKET_WIDTH: usize = 32;
const USER_INITIATED: u32 = 0x19;

unsafe extern "C" {
    fn pthread_override_qos_class_start_np(
        thread: libc::pthread_t, qos_class: u32, relative_priority: i32,
    ) -> *mut libc::c_void;
    fn pthread_override_qos_class_end_np(override_token: *mut libc::c_void) -> i32;
}

/// Lift only the work the open palette is waiting on. The Tauri worker's
/// original priority resumes even when a request is superseded or fails.
pub(crate) struct QueryPriority(*mut libc::c_void);

impl QueryPriority {
    pub(crate) fn begin() -> Self {
        Self(unsafe { pthread_override_qos_class_start_np(libc::pthread_self(), USER_INITIATED, 0) })
    }
}

impl Drop for QueryPriority {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _status = unsafe { pthread_override_qos_class_end_np(self.0) };
        }
    }
}


struct RankingThreads;

impl ThreadManager for RankingThreads {
    type Thread = JoinHandle<()>;

    fn create(&self, work: impl FnOnce() + Send + 'static) -> ort::Result<Self::Thread> {
        thread::Builder::new().name("richfinder-ranking".into()).spawn(move || {
            // The session is opened on the utility-priority indexing thread,
            // but inference serves an interactive search.
            unsafe extern "C" {
                fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
            }
            if unsafe { pthread_set_qos_class_self_np(USER_INITIATED, 0) } != 0 {
                eprintln!("[richos] document search: could not set relevance worker priority");
            }
            work();
        }).map_err(|error| ort::Error::new(error.to_string()))
    }

    fn join(thread: Self::Thread) -> ort::Result<()> {
        thread.join().map_err(|_| ort::Error::new("Relevance worker stopped unexpectedly."))
    }
}

struct Model {
    tokenizer: Tokenizer,
    session: Session,
}

impl Model {
    fn open(model: &Path, tokenizer: &Path) -> Result<Self, String> {
        let mut tokenizer = Tokenizer::from_file(tokenizer).map_err(|error| error.to_string())?;
        tokenizer
            .with_truncation(Some(TruncationParams { max_length: TOKEN_LIMIT, ..Default::default() }))
            .map_err(|error| error.to_string())?;
        tokenizer.with_padding(None);
        let session = Session::builder().map_err(|error| error.to_string())?
            .with_intra_threads(4).map_err(|error| error.to_string())?
            .with_inter_threads(1).map_err(|error| error.to_string())?
            .with_thread_manager(RankingThreads).map_err(|error| error.to_string())?
            .with_intra_op_spinning(true).map_err(|error| error.to_string())?
            // Keep workers available inside a scoring call, then let them sleep.
            .with_config_entry("session.force_spinning_stop", "1").map_err(|error| error.to_string())?
            .commit_from_file(model).map_err(|error| error.to_string())?;
        Ok(Self { tokenizer, session })
    }

    fn scores(&mut self, query: &str, candidates: &[String], obsolete: &dyn Fn() -> bool) -> Result<Vec<f32>, String> {
        let encodings = candidates.iter().map(|candidate| self.tokenizer.encode((query, candidate.as_str()), true))
            .collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())?;
        let mut buckets: [_; TOKEN_LIMIT / BUCKET_WIDTH] = std::array::from_fn(|_| Vec::new());
        for (i, encoding) in encodings.iter().enumerate() {
            let bucket = encoding.len().div_ceil(BUCKET_WIDTH).saturating_sub(1);
            buckets[bucket].push((i, encoding));
        }

        let mut scores = vec![0.0; candidates.len()];
        for bucket in buckets.iter().filter(|bucket| !bucket.is_empty()) {
            if obsolete() { return Ok(Vec::new()); }
            let width = bucket.iter().map(|(_, encoding)| encoding.len()).max().unwrap_or(0);
            let tensor = |values: fn(&Encoding) -> &[u32]| {
                let mut data = Vec::with_capacity(bucket.len() * width);
                for (_, encoding) in bucket {
                    data.extend(values(encoding).iter().copied().map(i64::from));
                    data.resize(data.len() + width - encoding.len(), 0);
                }
                Tensor::from_array(([bucket.len(), width], data)).map_err(|error| error.to_string())
            };
            let outputs = self.session.run(ort::inputs! {
                "input_ids" => tensor(Encoding::get_ids)?,
                "attention_mask" => tensor(Encoding::get_attention_mask)?,
                "token_type_ids" => tensor(Encoding::get_type_ids)?,
            }).map_err(|error| error.to_string())?;
            let (_, logits) = outputs.get("logits").ok_or("The relevance model returned no logits.")?
                .try_extract_tensor::<f32>().map_err(|error| error.to_string())?;
            if logits.len() != bucket.len() || logits.iter().any(|score| !score.is_finite()) {
                return Err("Unexpected relevance scores.".into());
            }
            for ((i, _), score) in bucket.iter().zip(logits) {
                scores[*i] = *score;
            }
        }
        Ok(scores)
    }
}


struct Ranking {
    strong: Model,
    first: Option<Model>,
}

impl Ranking {
    fn scores(&mut self, query: &str, candidates: &[String], obsolete: &dyn Fn() -> bool) -> Result<Vec<f32>, String> {
        const PRELIMINARY: usize = 8;
        const RETAIN_FROM_HYBRID: usize = 2;
        let Some(first) = self.first.as_mut().filter(|_| candidates.len() > PRELIMINARY + RETAIN_FROM_HYBRID) else {
            return self.strong.scores(query, candidates, obsolete);
        };
        let early = first.scores(query, candidates, obsolete);
        if obsolete() { return Ok(Vec::new()); }
        let Ok(early) = early else {
            return self.strong.scores(query, candidates, obsolete);
        };
        let mut order: Vec<_> = (0..candidates.len()).collect();
        order.sort_by(|&a, &b| early[b].total_cmp(&early[a]));
        order.truncate(PRELIMINARY);
        for position in 0..RETAIN_FROM_HYBRID {
            if !order.contains(&position) { order.push(position); }
        }
        // Give the best hybrid candidates a second look even if the smaller model
        // ranks them low. The main model still decides the final order.
        let finalists: Vec<_> = order.iter().map(|&i| candidates[i].clone()).collect();
        let final_scores = self.strong.scores(query, &finalists, obsolete)?;
        if obsolete() { return Ok(Vec::new()); }
        let mut scores = vec![f32::MIN; candidates.len()];
        for (i, score) in order.into_iter().zip(final_scores) {
            scores[i] = score;
        }
        Ok(scores)
    }
}

pub(crate) fn models() -> (impl Fn() -> Result<Box<dyn Embedder>, String> + Send + Sync, Reranker) {
    let model = Arc::new(OnceLock::<Option<Mutex<Ranking>>>::new());
    let start_model = model.clone();
    let embedder = move || {
        let embedder = Box::new(crate::search_embed::ResidentEmbedder::bundled()?) as Box<dyn Embedder>;
        // The search worker has already published lexical results. Load once before any
        // semantic response, then share the model across folder changes.
        start_model.get_or_init(|| match bundled() {
            Ok(model) => Some(Mutex::new(model)),
            Err(_) => {
                eprintln!("[richos] document search: the relevance model is unavailable");
                None
            }
        });
        Ok(embedder)
    };
    let reranker: Reranker = Arc::new(move |query, candidates, obsolete| {
        if obsolete() { return None; }
        let model = model.get()?.as_ref()?;
        let scores = model.lock().ok().and_then(|mut model| model.scores(query, candidates, obsolete).ok());
        if scores.is_none() && !obsolete() {
            eprintln!("[richos] document search: relevance scoring failed");
        }
        scores
    });
    (embedder, reranker)
}

fn bundled() -> Result<Ranking, String> {
    let directory = crate::search_embed::resources()?.join("Ranking");
    let tokenizer = directory.join("tokenizer.json");
    let strong = Model::open(&directory.join("model_qint8_arm64.onnx"), &tokenizer)?;
    let first = Model::open(&directory.join("tinybert_qint8_arm64.onnx"), &tokenizer)
        .inspect_err(|_| eprintln!("[richos] document search: preliminary relevance scoring is unavailable"))
        .ok();
    Ok(Ranking { strong, first })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the pinned RichFinder ranking resources"]
    fn bundled_relevance_model_understands_query_and_passage_together() {
        let path = std::env::var_os("RICHOS_SEARCH_RANKING_RESOURCES").expect("set the ranking resource directory");
        let directory = Path::new(&path);
        let mut model = Model::open(&directory.join("model_qint8_arm64.onnx"), &directory.join("tokenizer.json")).unwrap();
        let scores = model.scores("How do I get my money back?", &[
            "Street guide\nThe nearest bus stops on every corner.".into(),
            "Cancel a purchase\nWe issue a refund to the original payment method.".into(),
        ], &|| false).unwrap();
        assert!(scores[1] > scores[0]);
    }

    #[test]
    #[ignore = "requires the pinned RichFinder ranking resources"]
    fn bundled_shortlist_keeps_the_relevant_passage_for_final_scoring() {
        let path = std::env::var_os("RICHOS_SEARCH_RANKING_RESOURCES").expect("set the ranking resource directory");
        let directory = Path::new(&path);
        let tokenizer = directory.join("tokenizer.json");
        let mut ranking = Ranking {
            strong: Model::open(&directory.join("model_qint8_arm64.onnx"), &tokenizer).unwrap(),
            first: Some(Model::open(&directory.join("tinybert_qint8_arm64.onnx"), &tokenizer).unwrap()),
        };
        let mut passages: Vec<String> = (0..15)
            .map(|i| format!("Street guide {i}\nThe nearest bus stops on every corner."))
            .collect();
        passages.push("Cancel a purchase\nWe issue a refund to the original payment method.".into());
        let scores = ranking.scores("How do I get my money back?", &passages, &|| false).unwrap();
        assert_eq!(scores.len(), passages.len());
        assert!(scores.contains(&f32::MIN));
        assert_eq!(scores.iter().enumerate().max_by(|(_, a), (_, b)| a.total_cmp(b)).unwrap().0, 15);
    }

}
