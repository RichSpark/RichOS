//! Turning text into vectors: the meaning half of Super Finder.
//!
//! Search PRD v3.1, "Qdrant and the embedding model": the harness compares Apple's
//! `NLEmbedding.sentenceEmbedding` with one pinned local GGUF model on the CEO's questions and
//! the higher count of answers in the first five wins. This module holds the app-side
//! implementations:
//!
//! - [`AppleSentenceEmbedder`]: Apple's on-device sentence embedding, reached through objc2's
//!   runtime and `msg_send!` (no framework crate, no helper process, no download). It loads once
//!   and stays loaded; a query is one call.
//! - [`ConceptEmbedder`]: a deterministic stand-in for tests and synthetic fixtures. It maps
//!   words to a small table of subjects, so a test can show a document found by what it is
//!   about without any model. Nothing about search quality is claimed for it.
//!
//! The GGUF candidate is driven by the harness through its batch command-line tool (the PRD
//! allows that for the comparison); an in-app runtime for it is built only if it wins.
//!
//! **Identity.** Every embedder names itself, model revision and dimension included. The index
//! is stored under that identity, so vectors from two models are never mixed (*"never mix
//! language-specific spaces"*, *"never … mix incompatible vectors"*).

pub trait Embedder: Send + Sync {
    /// Model, language, revision and dimension, e.g. `apple-sentence:en:r1:512`.
    fn identity(&self) -> String;
    fn dimension(&self) -> usize;
    /// One vector per text, in order. A text the model cannot represent is an error for the
    /// whole call; the caller decides whether that file is skipped.
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String>;
}

/// Scale a vector to unit length (cosine similarity then equals the dot product). A zero
/// vector stays zero.
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

// ---- Apple ------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod apple {
    use super::{normalize, Embedder};
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use std::sync::Mutex;

    #[link(name = "NaturalLanguage", kind = "framework")]
    extern "C" {}
    #[link(name = "Foundation", kind = "framework")]
    extern "C" {}

    struct Handle(*mut AnyObject);
    // SAFETY: the NLEmbedding object is retained by this handle and only ever messaged while the
    // surrounding Mutex is held, so no two threads use it at once.
    unsafe impl Send for Handle {}

    /// Apple's sentence embedding for one language, loaded once.
    pub struct AppleSentenceEmbedder {
        handle: Mutex<Handle>,
        language: String,
        dimension: usize,
        revision: usize,
    }

    fn ns_string(text: &str) -> *mut AnyObject {
        // NSString's C-string constructor stops at a NUL; a NUL inside a Markdown file is not
        // text anyone searches for, so it becomes a space rather than truncating the passage.
        let mut bytes: Vec<u8> = text.bytes().map(|b| if b == 0 { b' ' } else { b }).collect();
        bytes.push(0);
        let Some(class) = AnyClass::get(c"NSString") else { return std::ptr::null_mut() };
        unsafe { msg_send![class, stringWithUTF8String: bytes.as_ptr() as *const std::ffi::c_char] }
    }

    impl AppleSentenceEmbedder {
        /// Load the sentence embedding for `language` (an `NLLanguage` code such as `en`), or say
        /// why it is unavailable on this Mac.
        pub fn new(language: &str) -> Result<Self, String> {
            objc2::rc::autoreleasepool(|_| {
                let class = AnyClass::get(c"NLEmbedding").ok_or("Apple's NaturalLanguage framework is not available on this Mac.")?;
                let lang = ns_string(language);
                if lang.is_null() {
                    return Err("Could not name the embedding language.".to_string());
                }
                let embedding: *mut AnyObject = unsafe { msg_send![class, sentenceEmbeddingForLanguage: lang] };
                if embedding.is_null() {
                    return Err(format!("Apple's sentence embedding for '{language}' is not available on this Mac."));
                }
                let _: *mut AnyObject = unsafe { msg_send![embedding, retain] };
                let dimension: usize = unsafe { msg_send![embedding, dimension] };
                let revision: usize = unsafe { msg_send![embedding, revision] };
                if dimension == 0 {
                    let _: () = unsafe { msg_send![embedding, release] };
                    return Err("Apple's sentence embedding reported no dimension.".to_string());
                }
                Ok(AppleSentenceEmbedder { handle: Mutex::new(Handle(embedding)), language: language.to_string(), dimension, revision })
            })
        }
    }

    impl Drop for AppleSentenceEmbedder {
        fn drop(&mut self) {
            if let Ok(handle) = self.handle.lock() {
                let _: () = unsafe { msg_send![handle.0, release] };
            }
        }
    }

    impl Embedder for AppleSentenceEmbedder {
        fn identity(&self) -> String {
            format!("apple-sentence:{}:r{}:{}", self.language, self.revision, self.dimension)
        }
        fn dimension(&self) -> usize {
            self.dimension
        }
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
            let handle = self.handle.lock().map_err(|_| "The embedding model is unusable after an earlier failure.".to_string())?;
            let mut out = Vec::with_capacity(texts.len());
            for text in texts {
                let vector = objc2::rc::autoreleasepool(|_| {
                    let s = ns_string(text);
                    if s.is_null() {
                        return Err("A passage could not be handed to the embedding model.".to_string());
                    }
                    let mut buffer = vec![0f32; self.dimension];
                    let ok: bool = unsafe { msg_send![handle.0, getVector: buffer.as_mut_ptr(), forString: s] };
                    if !ok {
                        return Err("The embedding model returned no vector for a passage.".to_string());
                    }
                    let mut v: Vec<f32> = buffer;
                    normalize(&mut v);
                    Ok(v)
                })?;
                out.push(vector);
            }
            Ok(out)
        }
    }
}

#[cfg(target_os = "macos")]
pub use apple::AppleSentenceEmbedder;

// ---- the stand-in -----------------------------------------------------------------------

/// A deterministic stand-in: each subject in the table is one dimension, and a text's vector
/// counts the subject words it contains. For tests and synthetic fixtures only.
pub struct ConceptEmbedder {
    concepts: Vec<(&'static str, &'static [&'static str])>,
}

/// Synthetic subjects used by the public tests and the stand-in folder. Each word belongs to
/// one subject.
pub const STAND_IN_CONCEPTS: &[(&str, &[&str])] = &[
    ("startup", &["slow", "sluggish", "lag", "launch", "launches", "startup", "seconds", "loading", "spinner", "blank", "open", "opens", "opening", "cold"]),
    ("mobile", &["app", "iphone", "android", "phone", "mobile", "ios"]),
    ("refunds", &["refund", "refunds", "cancel", "cancellation", "chargeback", "money"]),
    ("pricing", &["pricing", "price", "prices", "charge", "cost", "costs", "seat", "tier", "fee"]),
    ("voice", &["voice", "speech", "microphone", "whisper", "dictation", "audio"]),
    ("travel", &["trip", "flight", "hotel", "travel", "visa", "conference"]),
];

impl ConceptEmbedder {
    pub fn stand_in() -> Self {
        ConceptEmbedder { concepts: STAND_IN_CONCEPTS.to_vec() }
    }
}

impl Embedder for ConceptEmbedder {
    fn identity(&self) -> String {
        format!("stand-in-concepts:v1:{}", self.concepts.len())
    }
    fn dimension(&self) -> usize {
        self.concepts.len()
    }
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        Ok(texts
            .iter()
            .map(|t| {
                let words = crate::text::words_of(&crate::text::fold(t));
                let mut v: Vec<f32> =
                    self.concepts.iter().map(|(_, cue)| words.iter().filter(|w| cue.contains(&w.as_str())).count() as f32).collect();
                normalize(&mut v);
                v
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stand_in_puts_texts_about_the_same_thing_together() {
        let e = ConceptEmbedder::stand_in();
        let v = e.embed(&["the app felt slow on first open", "Why the iPhone launch takes three seconds", "refund within 30 days"]).unwrap();
        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        assert!(dot(&v[0], &v[1]) > 0.5);
        assert_eq!(dot(&v[0], &v[2]), 0.0);
        assert_eq!(e.dimension(), v[0].len());
    }

    /// Apple's model on this Mac: loads, has the dimension it reports, and puts a question
    /// nearer the passage that answers it than one that does not. Skipped (and said so) on a
    /// Mac without the asset, so the claim is never made from absence.
    #[cfg(target_os = "macos")]
    #[test]
    fn apple_sentence_embedding_loads_and_ranks_a_paraphrase_first() {
        let e = match AppleSentenceEmbedder::new("en") {
            Ok(e) => e,
            Err(why) => {
                eprintln!("NOT RUN: {why}");
                return;
            }
        };
        assert!(e.identity().starts_with("apple-sentence:en:r"));
        let v = e
            .embed(&[
                "The app is slow when it first opens",
                "Why the iPhone build takes three seconds to show anything at launch",
                "Our refund policy for annual subscriptions",
            ])
            .unwrap();
        assert_eq!(v[0].len(), e.dimension());
        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        assert!(dot(&v[0], &v[1]) > dot(&v[0], &v[2]), "{} vs {}", dot(&v[0], &v[1]), dot(&v[0], &v[2]));
    }
}
