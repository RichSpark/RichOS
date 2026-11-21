//! Reply attribution and the confirmed prefix at the first audio cutoff.

use crate::chunk::SentenceChunker;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplyId {
    pub thread_id: String,
    pub turn_id: String,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ReplyCutoff {
    pub reply: ReplyId,
    pub confirmed_chunks: usize,
    pub last_confirmed: Option<String>,
    pub first_unconfirmed: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ChunkId {
    pub reply: ReplyId,
    index: usize,
}

struct Chunk {
    text: String,
    end: Option<u64>,
}

struct Reply {
    generation: u64,
    chunker: SentenceChunker,
    chunks: Vec<Chunk>,
    ended: bool,
    stopped: bool,
    recorded: bool,
}

#[derive(Default)]
pub(crate) struct Playback {
    replies: HashMap<ReplyId, Reply>,
    pub last_started: Option<(u64, ReplyId)>,
}

impl Playback {
    pub fn start(&mut self, id: &ReplyId, generation: u64) {
        self.replies.entry(id.clone()).or_insert_with(|| Reply {
            generation,
            chunker: SentenceChunker::new(),
            chunks: Vec::new(),
            ended: false,
            stopped: false,
            recorded: false,
        });
    }

    pub fn delta(
        &mut self,
        id: &ReplyId,
        generation: u64,
        delta: &str,
    ) -> Vec<(u64, ChunkId, String)> {
        self.start(id, generation); // Voice mode can be opened during a reply.
        let reply = self.replies.get_mut(id).unwrap();
        if reply.stopped || reply.ended {
            return Vec::new();
        }
        let sentences = reply.chunker.push(delta);
        Self::add(id, reply, sentences)
    }

    pub fn end(&mut self, id: &ReplyId, flush: bool) -> Vec<(u64, ChunkId, String)> {
        let Some(reply) = self.replies.get_mut(id) else {
            return Vec::new();
        };
        reply.ended = true;
        let tail = if flush && !reply.stopped {
            reply.chunker.flush()
        } else {
            None
        };
        reply.chunker.reset();
        Self::add(id, reply, tail)
    }

    fn add(
        id: &ReplyId,
        reply: &mut Reply,
        chunks: impl IntoIterator<Item = String>,
    ) -> Vec<(u64, ChunkId, String)> {
        chunks
            .into_iter()
            .map(|text| {
                let chunk = ChunkId {
                    reply: id.clone(),
                    index: reply.chunks.len(),
                };
                reply.chunks.push(Chunk {
                    text: text.clone(),
                    end: None,
                });
                (reply.generation, chunk, text)
            })
            .collect()
    }

    pub fn complete_chunk(&mut self, id: &ChunkId, end: u64) {
        if let Some(chunk) = self
            .replies
            .get_mut(&id.reply)
            .and_then(|reply| reply.chunks.get_mut(id.index))
        {
            chunk.end = Some(end);
        }
    }

    pub fn cut(&mut self, generation: u64, confirmed: u64) -> Vec<ReplyCutoff> {
        self.replies
            .iter_mut()
            .filter(|(_, reply)| reply.generation == generation)
            .filter_map(|(id, reply)| {
                reply.stopped = true;
                Self::snapshot(id, reply, confirmed)
            })
            .collect()
    }

    pub fn failed_chunk(&mut self, id: &ChunkId, confirmed: u64) -> Option<ReplyCutoff> {
        Self::snapshot(&id.reply, self.replies.get_mut(&id.reply)?, confirmed)
    }

    fn snapshot(id: &ReplyId, reply: &mut Reply, confirmed: u64) -> Option<ReplyCutoff> {
        if reply.recorded || (reply.ended && Self::played_through(reply, confirmed)) {
            return None;
        }
        reply.recorded = true;
        // The device counter only certifies a contiguous prefix. Later chunks
        // cannot cover a gap left by a cut or a synthesis failure.
        let confirmed_chunks = reply
            .chunks
            .iter()
            .take_while(|chunk| chunk.end.is_some_and(|end| end <= confirmed))
            .count();
        let last = reply.chunks[..confirmed_chunks].last();
        let first_unconfirmed = reply.chunks.get(confirmed_chunks);
        Some(ReplyCutoff {
            reply: id.clone(),
            confirmed_chunks,
            last_confirmed: last.map(|chunk| chunk.text.clone()),
            first_unconfirmed: first_unconfirmed.map(|chunk| chunk.text.clone()),
        })
    }

    fn played_through(reply: &Reply, confirmed: u64) -> bool {
        reply
            .chunks
            .iter()
            .all(|chunk| chunk.end.is_some_and(|end| end <= confirmed))
    }

    pub fn retire(&mut self, confirmed: u64) {
        self.replies.retain(|_, reply| {
            !reply.ended
                || !(reply.stopped || reply.recorded || Self::played_through(reply, confirmed))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(thread: &str, turn: &str) -> ReplyId {
        ReplyId {
            thread_id: thread.into(),
            turn_id: turn.into(),
        }
    }

    #[test]
    fn a_cut_records_the_first_unconfirmed_chunk_under_each_originating_reply() {
        let a = id("a", "one");
        let b = id("b", "two");
        let mut playback = Playback::default();
        let first = playback.delta(&a, 0, "The build passed. The release is pending. ");
        let second = playback.delta(&b, 0, "I found the draft. ");
        playback.complete_chunk(&first[0].1, 100);
        playback.complete_chunk(&first[1].1, 200);
        playback.complete_chunk(&second[0].1, 300);
        playback.end(&a, true);
        let mut snapshots = playback.cut(0, 100);
        snapshots.sort_by(|a, b| a.reply.thread_id.cmp(&b.reply.thread_id));
        assert_eq!(
            snapshots,
            [
                ReplyCutoff {
                    reply: a.clone(),
                    confirmed_chunks: 1,
                    last_confirmed: Some("The build passed.".into()),
                    first_unconfirmed: Some("The release is pending.".into()),
                },
                ReplyCutoff {
                    reply: b,
                    confirmed_chunks: 0,
                    last_confirmed: None,
                    first_unconfirmed: Some("I found the draft.".into()),
                },
            ]
        );
        assert!(playback.cut(0, 300).is_empty());
        assert!(playback.delta(&a, 1, "Late words. ").is_empty());
    }

    #[test]
    fn an_ended_reply_has_no_fact_but_a_silent_gap_in_a_live_reply_does() {
        let mut playback = Playback::default();
        let done = id("a", "done");
        let writing = id("a", "writing");
        let first = playback.delta(&done, 0, "That is all. ");
        let second = playback.delta(&writing, 0, "The first point. ");
        playback.complete_chunk(&first[0].1, 100);
        playback.complete_chunk(&second[0].1, 200);
        playback.end(&done, true);
        assert_eq!(
            playback.cut(0, 200),
            [ReplyCutoff {
                reply: writing,
                confirmed_chunks: 1,
                last_confirmed: Some("The first point.".into()),
                first_unconfirmed: None,
            }]
        );
    }

    #[test]
    fn the_confirmed_count_disambiguates_repeated_speech_chunks() {
        let mut playback = Playback::default();
        let reply = id("a", "one");
        let chunks = playback.delta(
            &reply,
            0,
            "Wait for the bell. Wait for the bell. Start the timer. ",
        );
        playback.complete_chunk(&chunks[0].1, 100);
        playback.complete_chunk(&chunks[1].1, 200);
        let cuts = playback.cut(0, 200);
        assert_eq!(cuts[0].confirmed_chunks, 2);
        assert_eq!(
            cuts[0].last_confirmed.as_deref(),
            Some("Wait for the bell.")
        );
        assert_eq!(
            cuts[0].first_unconfirmed.as_deref(),
            Some("Start the timer.")
        );
    }

    #[test]
    fn a_failure_never_confirms_speech_after_the_failed_chunk() {
        let mut playback = Playback::default();
        let reply = id("a", "one");
        let chunks = playback.delta(&reply, 0, "First point. Second point. Third point. ");
        playback.complete_chunk(&chunks[0].1, 100);
        playback.complete_chunk(&chunks[2].1, 200);
        let cutoff = playback.failed_chunk(&chunks[1].1, 200).unwrap();
        assert_eq!(cutoff.last_confirmed.as_deref(), Some("First point."));
        assert_eq!(cutoff.first_unconfirmed.as_deref(), Some("Second point."));
        assert!(playback.cut(0, 200).is_empty());
    }
}
