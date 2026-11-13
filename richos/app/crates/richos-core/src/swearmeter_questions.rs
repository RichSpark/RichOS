//! Counts of original question requests, independent of every delivery sink.
use crate::{questions::AnswerOrigin, swearmeter, swearmeter_periods::Dates};
use std::collections::HashSet;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    count: u64, unknown_inputs: u64, available: bool, known_composites: HashSet<String>, dates: Dates,
}

#[derive(Default)]
pub struct Projection {
    seen: HashSet<String>,
    pub known_composites: HashSet<String>,
    pub count: u64,
    pub dates: Dates,
    pub unknown_inputs: u64,
    pub available: bool,
    pub revision: u64,
}

/// A failed statistics cache must never reject a durable human input. Keep the
/// numerical state moving but qualify it until an explicit source rebuild.
pub fn lock(cache: &std::sync::Mutex<Projection>) -> std::sync::MutexGuard<'_, Projection> {
    cache.lock().unwrap_or_else(|poison| {
        let mut projection = poison.into_inner();
        projection.available = false;
        projection
    })
}

pub fn rebuild(cache: &std::sync::Mutex<Projection>, read: impl FnOnce() -> Result<Projection, String>) -> Result<(), String> {
    let mut projection = lock(cache);
    let rebuilt = read()?;
    if !rebuilt.available { return Err("Some saved card inputs could not be read".into()); }
    *projection = rebuilt;
    cache.clear_poison();
    Ok(())
}

impl Projection {
    pub fn empty() -> Self { Self { available: true, ..Self::default() } }

    pub fn summary(&self) -> Summary {
        Summary { count: self.count, unknown_inputs: self.unknown_inputs, available: self.available, known_composites: self.known_composites.clone(), dates: self.dates.clone() }
    }
    pub fn from_summary(summary: &Summary) -> Self {
        Self { count: summary.count, unknown_inputs: summary.unknown_inputs, available: summary.available,
            known_composites: summary.known_composites.clone(), dates: summary.dates.clone(), ..Self::empty() }
    }
    fn fold_text(&mut self, origin: Option<AnswerOrigin>, text: &str, at: Option<u64>) {
        self.revision += 1;
        match origin {
            Some(AnswerOrigin::Direct) => {
                let count = swearmeter::count(text);
                self.count += count;
                self.dates.add(at, count);
            }
            Some(AnswerOrigin::Conversation) => {}
            _ if !text.is_empty() => self.unknown_inputs += 1,
            _ => {}
        }
    }

    /// The original operation key is retained; no submitted text or excerpt is.
    pub fn record(&mut self, key: &str, origin: Option<AnswerOrigin>, text: &str, at: Option<u64>) {
        if !self.seen.insert(key.to_string()) { return; }
        self.fold_text(origin, text, at);
    }
}

/// Startup needs original input requests, not generated result copies. Fold each
/// request while reading the existing operation map and release its text at once.
/// Duplicate keys make coverage unavailable rather than publishing an overcount.
pub(crate) struct OperationInputs {
    pub projection: Projection,
    pub represented: HashSet<String>,
}
impl Default for OperationInputs {
    fn default() -> Self { Self { projection: Projection::empty(), represented: HashSet::new() } }
}
impl<'de> serde::Deserialize<'de> for OperationInputs {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Inputs;
        impl<'de> serde::de::Visitor<'de> for Inputs {
            type Value = OperationInputs;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("saved input operations") }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                #[derive(serde::Deserialize)]
                struct Input {
                    request: String,
                    #[serde(default)] origin: Option<AnswerOrigin>,
                    #[serde(default)] submitted_at: Option<u64>,
                    #[serde(default)] result: serde_json::Value,
                }
                #[derive(serde::Deserialize)]
                struct SavedResult { outcome: String, question: Option<SavedQuestion> }
                #[derive(serde::Deserialize)]
                struct SavedQuestion { id: String, answer: Option<crate::questions::Answer> }
                let mut inputs = OperationInputs::default();
                while let Some((key, operation)) = map.next_entry::<String, Input>()? {
                    if !inputs.projection.seen.insert(key) { inputs.projection.available = false; }
                    match serde_json::from_str::<crate::questions::AnswerRequest>(&operation.request) {
                        Ok(request) => {
                            let at = operation.submitted_at.or_else(|| {
                                let saved = serde_json::from_value::<SavedResult>(operation.result).ok()?;
                                if saved.outcome != "accepted" { return None; }
                                let question = saved.question?;
                                if question.id != request.question_id { return None; }
                                let answer = question.answer?;
                                let mut options = request.option_ids.clone();
                                options.sort();
                                (answer.text == request.text.trim() && answer.option_ids == options).then_some(answer.at)
                            });
                            inputs.represented.insert(request.question_id);
                            inputs.projection.fold_text(operation.origin, &request.text, at);
                        }
                        Err(_) => inputs.projection.available = false,
                    }
                }
                Ok(inputs)
            }
        }
        deserializer.deserialize_map(Inputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_old_operation_recovers_its_own_answer_date() {
        let request = serde_json::json!({"question_id":"q1","client_id":"req","text":"shit"}).to_string();
        let operation = |outcome| serde_json::json!({"origin":"direct", "request":request,
            "result":{"outcome":outcome,"question":{"id":"q1","answer":{
                "option_ids":[],"text":"shit","method":"typed","surface":"mac","at":1000}}}});
        let input: OperationInputs = serde_json::from_value(serde_json::json!({
            "accepted":operation("accepted"), "unchanged":operation("already_answered")})).unwrap();
        let periods = Dates::report(&[&input.projection.dates], 2000, |_| Some(202611));
        assert_eq!(input.projection.count, 2);
        assert_eq!(periods.last_30_days, 1);
        assert_eq!(periods.months[0].count, 1);
        assert_eq!(periods.undated, 1);
    }

    #[test]
    fn retry_cannot_change_the_original_identity_or_origin() {
        let mut counts = Projection::empty();
        counts.record("entity:thread:request", Some(AnswerOrigin::Direct), "shit", Some(1));
        counts.record("entity:thread:request", Some(AnswerOrigin::Conversation), "fuck fuck", Some(2));
        assert_eq!(counts.count, 1);
        assert_eq!(counts.revision, 1);
        counts.record("entity:thread:new-send", Some(AnswerOrigin::Direct), "shit", Some(3));
        assert_eq!(counts.count, 2);
    }

    #[test]
    fn empty_legacy_requests_are_verified_zero_and_model_text_is_never_human() {
        let mut counts = Projection::empty();
        counts.record("legacy-option", None, "", None);
        counts.record("model", Some(AnswerOrigin::Conversation), "fucking shit", None);
        assert_eq!(counts.count, 0);
        assert_eq!(counts.unknown_inputs, 0);
        counts.record("legacy-text", None, "shit", None);
        counts.record("unknown-future-origin", Some(AnswerOrigin::Unknown), "fuck", None);
        assert_eq!(counts.count, 0);
        assert_eq!(counts.unknown_inputs, 2);
    }
}

