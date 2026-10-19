//! Sentence chunking of Rich's STREAMING reply, and turning written text into speakable text.
//!
//! Rich's reply arrives as `rich://chunk` deltas — a few tokens at a time, arbitrary
//! boundaries, often mid-word. Handing those straight to a synthesizer produces stutter.
//! Handing the synthesizer the WHOLE reply instead produces a long silence before Rich says
//! anything. The pilot's answer, which we keep, is **gapless sentence pipelining**: emit each
//! sentence the moment its boundary arrives, so synthesis of sentence N+1 overlaps playback
//! of sentence N and the CEO hears Rich start talking about one sentence after he starts
//! thinking (the pilot's notes; architecture 2026-08-24 §1.1).
//!
//! ## Speakable, not written
//!
//! Rich writes for a screen. Spoken aloud, `**Q3**` becomes "star star Q three star star",
//! a fenced code block becomes three minutes of punctuation, and a URL becomes an alphabet
//! recital. [`speakable`] is the CEO-facing clean-output rule applied to the EAR: markdown
//! machinery is removed, code fences are dropped whole, and links are spoken as links. This
//! is the same doctrine as the visual clean-output guarantee, not a separate one.
//!
//! Everything in this module is pure. No devices, no clock, no synthesizer.

/// A chunk shorter than this (after sanitizing, in characters) is held back and merged with
/// the next one rather than spoken on its own — prevents one-word stutter.
pub const MIN_CHUNK_CHARS: usize = 2;

/// If no sentence boundary has arrived within this many characters, the chunker cuts at the
/// last word break anyway so Rich starts talking instead of buffering a monologue.
/// ~240 characters is roughly 15 s of speech at a normal rate.
pub const MAX_CHUNK_CHARS: usize = 240;

/// Words that end in a full stop WITHOUT ending a sentence. Matched case-insensitively
/// against the token immediately before the stop.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "mt", "vs", "etc", "eg", "e.g", "ie",
    "i.e", "inc", "ltd", "llc", "co", "corp", "dept", "est", "fig", "no", "approx", "min",
    "max", "vol", "cf", "al", "am", "pm", "jan", "feb", "mar", "apr", "jun", "jul", "aug",
    "sep", "sept", "oct", "nov", "dec", "mon", "tue", "tues", "wed", "thu", "thur", "thurs",
    "fri", "sat", "sun", "q1", "q2", "q3", "q4",
];

/// Streaming sentence chunker. Feed it reply deltas; it hands back complete, speakable
/// sentences as their boundaries arrive.
#[derive(Debug)]
pub struct SentenceChunker {
    /// Raw text received but not yet emitted.
    pending: String,
    /// Inside a ``` fenced block: everything is dropped until the closing fence.
    in_code_fence: bool,
    /// Partial fence marker seen at the end of a delta (fences can split across deltas).
    max_chars: usize,
}

impl Default for SentenceChunker {
    fn default() -> Self {
        SentenceChunker::new()
    }
}

impl SentenceChunker {
    pub fn new() -> Self {
        SentenceChunker { pending: String::new(), in_code_fence: false, max_chars: MAX_CHUNK_CHARS }
    }

    /// Chunker with an explicit max chunk length (tests, and latency tuning).
    pub fn with_max_chars(max_chars: usize) -> Self {
        SentenceChunker { pending: String::new(), in_code_fence: false, max_chars }
    }

    /// Characters buffered and not yet spoken — diagnostics.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Start of a new turn: drop anything left over from the last one.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.in_code_fence = false;
    }

    /// Feed one streamed delta. Returns zero or more speakable sentences, in order.
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.pending.push_str(delta);
        let mut out = Vec::new();
        while let Some(cut) = self.next_cut() {
            let raw: String = self.pending.drain(..cut).collect();
            if let Some(s) = self.speakable_or_skip(&raw) {
                out.push(s);
            }
        }
        out
    }

    /// End of the turn: emit whatever is left, if it is worth speaking.
    pub fn flush(&mut self) -> Option<String> {
        if self.pending.trim().is_empty() {
            self.pending.clear();
            return None;
        }
        let raw: String = std::mem::take(&mut self.pending);
        self.speakable_or_skip(&raw)
    }

    fn speakable_or_skip(&mut self, raw: &str) -> Option<String> {
        let (text, fence_state) = strip_code_fences(raw, self.in_code_fence);
        self.in_code_fence = fence_state;
        let s = speakable(&text);
        // Nothing worth saying: a bullet marker, a lone `**`, an emptied code fence.
        if s.chars().filter(|c| c.is_alphanumeric()).count() < MIN_CHUNK_CHARS {
            return None;
        }
        Some(s)
    }

    /// Byte index one past the end of the next emittable chunk, or None if we should wait
    /// for more text.
    fn next_cut(&self) -> Option<usize> {
        let bytes = self.pending.as_bytes();
        let mut idx = 0usize;
        let chars: Vec<(usize, char)> = self.pending.char_indices().collect();

        while idx < chars.len() {
            let (bi, c) = chars[idx];
            // A newline always ends a chunk: list items and paragraphs are separate breaths.
            if c == '\n' {
                return Some(bi + c.len_utf8());
            }
            if matches!(c, '.' | '!' | '?') {
                // Consume a run of terminators ("?!", "...").
                let mut end = idx;
                while end + 1 < chars.len() && matches!(chars[end + 1].1, '.' | '!' | '?') {
                    end += 1;
                }
                let (last_bi, last_c) = chars[end];
                let after = chars.get(end + 1).map(|(_, c)| *c);
                match after {
                    // Boundary only when whitespace follows: "3.5" and "acme.com" are not
                    // sentence ends, and neither is a stop we have not yet seen past.
                    Some(w) if w.is_whitespace() => {
                        if self.is_real_boundary(&chars, idx) {
                            return Some(last_bi + last_c.len_utf8() + w.len_utf8());
                        }
                    }
                    // Terminator at the very end of what we have: wait — the next delta
                    // decides whether it was "3." or "3. Next".
                    None => return self.overlong_cut(bytes),
                    _ => {}
                }
                idx = end + 1;
                continue;
            }
            idx += 1;
        }
        self.overlong_cut(bytes)
    }

    /// No boundary in sight and the buffer is too long: cut at the last word break so Rich
    /// starts talking. Never cuts mid-word.
    fn overlong_cut(&self, bytes: &[u8]) -> Option<usize> {
        if self.pending.chars().count() <= self.max_chars {
            return None;
        }
        let limit = self
            .pending
            .char_indices()
            .nth(self.max_chars)
            .map(|(i, _)| i)
            .unwrap_or(bytes.len());
        // Cut just past the last word break, advancing by that character's own UTF-8 length:
        // a non-breaking or ideographic space is two or three bytes, and cutting one byte in
        // would split it (hunt part 1 finding 40).
        self.pending[..limit]
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, c)| i + c.len_utf8())
    }

    /// Is the terminator at `idx` a genuine sentence end, or an abbreviation / initial /
    /// list marker?
    fn is_real_boundary(&self, chars: &[(usize, char)], idx: usize) -> bool {
        if chars[idx].1 != '.' {
            return true; // '!' and '?' are never abbreviations
        }
        // Walk back over the word attached to the stop.
        let mut start = idx;
        while start > 0 {
            let prev = chars[start - 1].1;
            if prev.is_alphanumeric() || prev == '.' {
                start -= 1;
            } else {
                break;
            }
        }
        if start == idx {
            return true; // stop with no word in front of it
        }
        let word: String = chars[start..idx].iter().map(|(_, c)| *c).collect();

        // "J. R. R. Tolkien" — a single capital letter is an initial, not a sentence.
        if word.chars().count() == 1 && word.chars().next().unwrap().is_uppercase() {
            return false;
        }
        // "1." / "2." at the start of a line is a list marker, not a sentence.
        if word.chars().all(|c| c.is_ascii_digit()) {
            let at_line_start = start == 0
                || chars[..start].iter().rev().take_while(|(_, c)| c.is_whitespace()).any(|(_, c)| *c == '\n')
                || chars[..start].iter().all(|(_, c)| c.is_whitespace());
            if at_line_start {
                return false;
            }
        }
        let lower = word.to_lowercase();
        let lower = lower.trim_end_matches('.');
        !ABBREVIATIONS.contains(&lower)
    }
}

/// Remove ``` fenced blocks from `raw`, given whether we start inside one. Returns the
/// surviving text and the fence state afterwards. Code read aloud is machinery reaching the
/// CEO's ear — the same thing clean output forbids on screen.
pub fn strip_code_fences(raw: &str, mut inside: bool) -> (String, bool) {
    let mut out = String::with_capacity(raw.len());
    for segment in raw.split_inclusive('\n') {
        let is_fence = segment.trim_start().starts_with("```");
        if is_fence {
            inside = !inside;
            continue;
        }
        if !inside {
            out.push_str(segment);
        }
    }
    (out, inside)
}

/// Turn one written chunk into something worth hearing: markdown machinery removed,
/// whitespace collapsed, links spoken as links.
pub fn speakable(raw: &str) -> String {
    let mut s = raw.to_string();

    // Markdown links: [the Q3 memo](https://…) -> "the Q3 memo".
    s = replace_md_links(&s);
    // Bare URLs read aloud are unbearable.
    s = replace_bare_urls(&s);

    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut at_line_start = true;
    while i < chars.len() {
        let c = chars[i];
        if at_line_start {
            // Heading hashes, blockquote arrows, bullet markers, table pipes.
            if c == '#' || c == '>' {
                i += 1;
                continue;
            }
            if (c == '-' || c == '*' || c == '+')
                && chars.get(i + 1).map(|n| n.is_whitespace()).unwrap_or(false)
            {
                i += 2;
                continue;
            }
            // "1. " / "12) " list markers.
            if c.is_ascii_digit() {
                let mut j = i;
                while chars.get(j).map(|d| d.is_ascii_digit()).unwrap_or(false) {
                    j += 1;
                }
                if matches!(chars.get(j), Some('.') | Some(')'))
                    && chars.get(j + 1).map(|n| n.is_whitespace()).unwrap_or(false)
                {
                    i = j + 2;
                    continue;
                }
            }
            if c == ' ' || c == '\t' {
                i += 1;
                continue;
            }
            at_line_start = false;
        }
        match c {
            // Emphasis / inline code / strikethrough markers carry no sound.
            '*' | '_' | '`' | '~' => {
                i += 1;
                continue;
            }
            '\n' | '\r' => {
                out.push(' ');
                at_line_start = true;
                i += 1;
                continue;
            }
            _ => out.push(c),
        }
        i += 1;
    }

    // Collapse runs of whitespace; a synthesizer pauses on every one of them.
    let collapsed: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    on_the_hour(collapsed.trim())
}

/// **A time on the hour is said the way a person says it: "9:00 AM" is "9 AM", "at 10:00" is
/// "at 10".** Rich's voice cannot say ":00". Rendered by Pocket TTS on 2026-10-19 (to memory, not
/// played) and transcribed by whisper large-v3-turbo, "It resets Wednesday at
/// 9:00 AM." came out as "... at 9.70am" (twice), "at 97", "at 9 so", "at 9.30" and "at 9.00": six
/// renders, six wrong, four without the "AM". "Lunch is at 12:00 PM." came out as "12 zero" and
/// "12.00", "We meet at 10:00." as "10-7" and "ten, sir". The same sentences without the ":00"
/// were said right every time (6 of 6, 2 of 2, 2 of 2). That is the end of Rich's spoken reply on
/// nightly 65: written "It resets Wednesday at 9:00 AM.", heard "... at 9" and nothing after it.
///
/// Only ":00" after an hour of one or two digits goes, and never inside a longer time such as
/// "10:00:30"; "2:30 PM" is said correctly already and stays as it is.
fn on_the_hour(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ':'
            && chars.get(i + 1) == Some(&'0')
            && chars.get(i + 2) == Some(&'0')
            && !chars.get(i + 3).is_some_and(|c| c.is_ascii_digit() || *c == ':')
        {
            let digits = chars[..i].iter().rev().take_while(|c| c.is_ascii_digit()).count();
            let before = i.checked_sub(digits + 1).map(|j| chars[j]);
            if (1..=2).contains(&digits) && !before.is_some_and(|c| c == ':' || c == '.' || c.is_alphanumeric()) {
                i += 3;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn replace_md_links(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(close) = find_from(&chars, i + 1, ']') {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(paren) = find_from(&chars, close + 2, ')') {
                        out.extend(&chars[i + 1..close]);
                        i = paren + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn find_from(chars: &[char], from: usize, target: char) -> Option<usize> {
    chars.iter().enumerate().skip(from).find(|(_, c)| **c == target).map(|(i, _)| i)
}

fn replace_bare_urls(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            if w.starts_with("http://") || w.starts_with("https://") || w.starts_with("www.") {
                "a link"
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(chunker: &mut SentenceChunker, deltas: &[&str]) -> Vec<String> {
        let mut out = Vec::new();
        for d in deltas {
            out.extend(chunker.push(d));
        }
        if let Some(tail) = chunker.flush() {
            out.push(tail);
        }
        out
    }

    /// INVARIANT: a sentence is emitted the moment its boundary arrives, not when the reply
    /// ends — this is what makes TTS overlap generation instead of following it.
    #[test]
    fn a_sentence_is_emitted_as_soon_as_its_boundary_arrives() {
        let mut c = SentenceChunker::new();
        // No terminator yet: nothing to say.
        assert!(c.push("Good morning").is_empty());
        // The boundary arrives INSIDE this delta -> the sentence is emitted on this call,
        // not held until the reply ends. That one call is the whole point of the module.
        assert_eq!(c.push(". Here is").as_slice(), ["Good morning."]);
        // The rest is still open, so it waits.
        assert!(c.push(" where things").is_empty());
        assert_eq!(c.push(" stand. ").as_slice(), ["Here is where things stand."]);
    }

    /// INVARIANT: token-level deltas that split words and punctuation still produce whole
    /// sentences — the chunker never speaks half a word.
    #[test]
    fn deltas_that_split_words_still_produce_whole_sentences() {
        let mut c = SentenceChunker::new();
        let out = stream(&mut c, &["Ac", "me ren", "egotiat", "ed", ". We ", "close Thur", "sday."]);
        assert_eq!(out, vec!["Acme renegotiated.", "We close Thursday."]);
    }

    /// INVARIANT: concatenating every emitted chunk reproduces the whole reply's words —
    /// chunking never loses or duplicates content.
    #[test]
    fn chunking_loses_no_words_from_the_reply() {
        let reply = "Three things. Acme signed at 4.5 million. Dr. Patel wants Thursday. \
                     I'd push back — the number is soft.";
        let mut c = SentenceChunker::new();
        let mut out = Vec::new();
        for ch in reply.chars() {
            out.extend(c.push(&ch.to_string()));
        }
        if let Some(t) = c.flush() {
            out.push(t);
        }
        let spoken_words: Vec<String> =
            out.join(" ").split_whitespace().map(|w| w.to_string()).collect();
        let source_words: Vec<String> =
            reply.split_whitespace().map(|w| w.to_string()).collect();
        assert_eq!(spoken_words, source_words);
    }

    /// INVARIANT: a decimal is not a sentence end. "4.5 million" must never be spoken as
    /// "four." then "five million."
    #[test]
    fn a_decimal_number_is_not_a_sentence_boundary() {
        let mut c = SentenceChunker::new();
        let out = stream(&mut c, &["Acme signed at 4.5 million on the nose. Next."]);
        assert_eq!(out, vec!["Acme signed at 4.5 million on the nose.", "Next."]);
    }

    /// INVARIANT: abbreviations do not end sentences.
    #[test]
    fn abbreviations_do_not_end_sentences() {
        let mut c = SentenceChunker::new();
        let out = stream(&mut c, &["Dr. Patel and Mr. Yao met the Acme Inc. team. It went well."]);
        assert_eq!(out, vec!["Dr. Patel and Mr. Yao met the Acme Inc. team.", "It went well."]);
    }

    /// INVARIANT: initials do not end sentences.
    #[test]
    fn initials_do_not_end_sentences() {
        let mut c = SentenceChunker::new();
        let out = stream(&mut c, &["J. R. Yao signed it. Done."]);
        assert_eq!(out, vec!["J. R. Yao signed it.", "Done."]);
    }

    /// INVARIANT: a numbered list marker is not a sentence, and is not read aloud as "one".
    #[test]
    fn numbered_list_markers_are_neither_spoken_nor_treated_as_boundaries() {
        let mut c = SentenceChunker::new();
        let out = stream(&mut c, &["Here are two:\n1. Renegotiate Acme.\n2. Call Patel.\n"]);
        assert_eq!(out, vec!["Here are two:", "Renegotiate Acme.", "Call Patel."]);
    }

    /// INVARIANT: an unpunctuated monologue still starts speaking — the buffer is cut at a
    /// word break rather than growing until the reply ends.
    #[test]
    fn an_unpunctuated_monologue_is_cut_at_a_word_break_not_mid_word() {
        let mut c = SentenceChunker::with_max_chars(40);
        let long = "one two three four five six seven eight nine ten eleven twelve";
        let out = stream(&mut c, &[long]);
        assert!(out.len() >= 2, "should have cut: {out:?}");
        for chunk in &out {
            assert!(!chunk.is_empty());
        }
        // No word was split across chunks.
        let rejoined: Vec<&str> = out.iter().flat_map(|s| s.split(' ')).collect();
        let original: Vec<&str> = long.split(' ').collect();
        assert_eq!(rejoined, original);
    }

    /// INVARIANT (hunt part 1 finding 40): the overlong cut lands on a character boundary
    /// when the last word break is a multibyte space. A non-breaking space (U+00A0, two
    /// bytes) or an ideographic space (U+3000, three bytes) is ordinary text in a reply;
    /// cutting one byte past its start used to panic in `drain`, poisoning the chunker and
    /// controller locks so voice stayed broken until restart.
    #[test]
    fn an_overlong_cut_after_a_multibyte_space_does_not_split_the_space() {
        for space in ['\u{00A0}', '\u{3000}'] {
            let mut c = SentenceChunker::with_max_chars(40);
            // 34 characters, then the multibyte space at character 34, then 16 letters: 51
            // characters, so the 40-character budget forces a cut and the multibyte space is
            // the last word break before it.
            let head = "aaaa bbbb cccc dddd eeee ffff gggg";
            let tail = "hhhhhhhhhhhhhhhh";
            let text = format!("{head}{space}{tail}");
            assert_eq!(text.chars().count(), 51);
            let mut out = c.push(&text);
            out.extend(c.flush());
            assert_eq!(out, vec![head.to_string(), tail.to_string()], "space {space:?}");
        }
    }

    /// INVARIANT (clean output, for the ear): markdown machinery is never spoken.
    #[test]
    fn markdown_machinery_is_never_spoken() {
        assert_eq!(speakable("**Q3** is _soft_ and `flagged`"), "Q3 is soft and flagged");
        assert_eq!(speakable("## The number"), "The number");
        assert_eq!(speakable("- Renegotiate Acme"), "Renegotiate Acme");
        assert_eq!(speakable("> he said no"), "he said no");
        assert_eq!(speakable("1. First thing"), "First thing");
    }

    /// INVARIANT (clean output, for the ear): a code block is never read aloud.
    #[test]
    fn a_code_block_is_never_read_aloud() {
        let mut c = SentenceChunker::new();
        let out = stream(
            &mut c,
            &["Run this. ", "\n```sh\n", "rm -rf /tmp/x && echo $HOME\n", "```\n", "Then tell me.\n"],
        );
        assert_eq!(out, vec!["Run this.", "Then tell me."]);
        assert!(!out.join(" ").contains("rm -rf"), "shell reached the CEO's ear");
    }

    /// INVARIANT: a time on the hour reaches the synthesizer without ":00", which Rich's voice
    /// cannot say and which took the end of his reply with it (nightly 65, [`on_the_hour`]).
    #[test]
    fn a_time_on_the_hour_is_spoken_without_its_zeros() {
        let mut c = SentenceChunker::new();
        let mut said = c.push("Also, a heads-up: your Claude account has used 95% of this week's limit. It resets Wednesday at 9:00 AM.");
        said.extend(c.flush());
        assert_eq!(said.last().map(String::as_str), Some("It resets Wednesday at 9 AM."));
        assert_eq!(speakable("Lunch is at 12:00 PM."), "Lunch is at 12 PM.");
        assert_eq!(speakable("We meet at 10:00."), "We meet at 10.");
        assert_eq!(speakable("(9:00-10:00)"), "(9-10)");
        // Not on the hour, or not a clock time: unchanged.
        assert_eq!(speakable("The call is at 2:30 PM."), "The call is at 2:30 PM.");
        assert_eq!(speakable("It ran 10:00:30 and 1:10:00 and v1:00 and 123:00."), "It ran 10:00:30 and 1:10:00 and v1:00 and 123:00.");
    }

    /// INVARIANT: a URL is spoken as "a link", not letter by letter.
    #[test]
    fn urls_are_spoken_as_links_not_recited() {
        assert_eq!(speakable("See [the Q3 memo](https://x.test/a/b) now"), "See the Q3 memo now");
        assert_eq!(speakable("See https://x.test/a/b now"), "See a link now");
    }

    /// INVARIANT: nothing worth zero sound is ever sent to the synthesizer — a stray bullet
    /// or an emptied fence produces no utterance at all.
    #[test]
    fn chunks_with_nothing_to_say_are_dropped_entirely() {
        let mut c = SentenceChunker::new();
        assert!(c.push("- \n").is_empty());
        assert!(c.push("**\n").is_empty());
        assert!(c.push("\n\n\n").is_empty());
        assert!(c.flush().is_none());
    }

    /// INVARIANT: reset clears mid-turn state, so a barge-in cannot leak the abandoned
    /// half-sentence into the next reply.
    #[test]
    fn reset_drops_the_abandoned_half_sentence() {
        let mut c = SentenceChunker::new();
        c.push("I was about to say something");
        assert!(c.pending_len() > 0);
        c.reset();
        assert_eq!(c.pending_len(), 0);
        assert!(c.flush().is_none());
        assert_eq!(c.push("Fresh start. "), vec!["Fresh start."]);
    }

    /// INVARIANT: an unterminated final fragment is still spoken at end of turn — Rich never
    /// swallows his last words because he forgot a full stop.
    #[test]
    fn the_final_fragment_is_spoken_at_end_of_turn() {
        let mut c = SentenceChunker::new();
        assert!(c.push("and that is where it stands").is_empty());
        assert_eq!(c.flush().as_deref(), Some("and that is where it stands"));
    }
}
