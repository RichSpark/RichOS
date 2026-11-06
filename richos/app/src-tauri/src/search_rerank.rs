//! Score each query and retrieved passage together with the bundled MiniLM model.
use ort::{session::Session, value::Tensor};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use richos_search::{embed::Embedder, service::Reranker};
use tokenizers::{Encoding, Tokenizer, TruncationParams};

const TOKEN_LIMIT: usize = 192;
const BUCKET_WIDTH: usize = 32;

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
            .with_intra_threads(2).map_err(|error| error.to_string())?
            .with_inter_threads(1).map_err(|error| error.to_string())?
            .with_intra_op_spinning(false).map_err(|error| error.to_string())?
            .commit_from_file(model).map_err(|error| error.to_string())?;
        Ok(Self { tokenizer, session })
    }

    fn scores(&mut self, query: &str, candidates: &[String]) -> Result<Vec<f32>, String> {
        let encodings = candidates.iter().map(|candidate| self.tokenizer.encode((query, candidate.as_str()), true))
            .collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())?;
        let mut buckets: [_; TOKEN_LIMIT / BUCKET_WIDTH] = std::array::from_fn(|_| Vec::new());
        for (i, encoding) in encodings.iter().enumerate() {
            let bucket = encoding.len().div_ceil(BUCKET_WIDTH).saturating_sub(1);
            buckets[bucket].push((i, encoding));
        }

        let mut scores = vec![0.0; candidates.len()];
        for bucket in buckets.iter().filter(|bucket| !bucket.is_empty()) {
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


pub(crate) fn models() -> (impl Fn() -> Result<Box<dyn Embedder>, String> + Send + Sync, Reranker) {
    let model = Arc::new(OnceLock::<Option<Mutex<Model>>>::new());
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
    let reranker: Reranker = Arc::new(move |query, candidates| {
        model.get()?.as_ref()?.lock().ok()?.scores(query, candidates).ok()
    });
    (embedder, reranker)
}

fn bundled() -> Result<Model, String> {
    let directory = crate::search_embed::resources()?.join("Ranking");
    let model = directory.join("model_qint8_arm64.onnx");
    let tokenizer = directory.join("tokenizer.json");
    Model::open(&model, &tokenizer)
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
        ]).unwrap();
        assert!(scores[1] > scores[0]);
    }
}
