#!/usr/bin/env python3
"""publication-boundary.py — the content half of the publication-boundary guard.

Why this is a separate file rather than a heredoc inside the shell library:
scan-secrets.sh embeds its scanner because exactly one hook uses it. This
predicate is shared by TWO hooks — the write-time guard and the commit-time
guard — and a predicate that exists in two copies is the defect this whole
mechanism was built to stop. One implementation, two callers.

Contract with the caller (scripts/lib/publication-boundary.sh):

  argv[1] is a JSON job file:

      {
        "min_speech_lines": 8,
        "min_quote_words": 8,
        "corpus_max_files": 4000,
        "corpus_max_bytes": 67108864,
        "items_max_files": 5000,
        "corpus_may_be_empty": false,
        "sources": ["/abs/path/to/private/tree", ...],
        "private_files": ["<64-hex-digest>:<name>", ...],
        "items":   [{"label": "docs/x.md", "path": "/tmp/blob"},
                    {"label": "docs/y.md", "text": "inline content"},
                    {"label": "docs/notes/", "path": "/repo/docs/notes"},
                    {"label": "docs/z.png", "path": "/tmp/blob", "identity_only": true}]
      }

  An item may carry "published_path": the file's text as ALREADY PUBLISHED (the
  commit guard passes the same path at the public remote's main). A word-run
  that also appears verbatim there is not a new disclosure, and the
  derived-from-private detector does not count it; see ALREADY PUBLISHED below.

  "private_files" is material that is private BY IDENTITY rather than by
  resembling a recording — see the identity section below. An item marked
  "identity_only" is judged by that rule ALONE: it is how a binary blob, and a
  write to a gitignored destination, get an identity verdict without being
  handed to the text detectors. When EVERY item is identity_only the corpus is
  never built at all, so that path costs nothing and cannot fail on a corpus
  it did not need.

  An item whose "path" is a DIRECTORY is expanded to the files beneath it and
  every one is scanned. It is spelled out in the contract because the opposite
  behavior is what a scanner does by accident: `open()` on a directory raises,
  the unreadable-path branch skips it, and the run reports CLEAN having read
  ZERO BYTES. See expand_items for the shape that walked past this.

  stdout is one line per finding, tab-separated, and nothing else:

      BLOCK <TAB> label <TAB> detector <TAB> evidence
      BROKEN <TAB> reason
      CLEAN

  plus, as the LAST line of every completed analysis, what the run examined:

      CORPUS <TAB> member-files <TAB> corpus-words

  That trailer is the oracle for "this verdict was reached by looking". A CLEAN
  from a scanner whose corpus was empty is indistinguishable, from the outside,
  from a CLEAN that examined 130,000 words — and a regression test can pass for
  precisely the reason the scanner failed. It is emitted last because both
  shell callers dispatch on `head -1 | cut -f1` and pb_refusal skips non-BLOCK
  lines, so it extends the contract without altering it.

  An EMPTY corpus is not a CLEAN. When sources were declared and resolved and
  none of their files qualified as private speech, the answer is BROKEN — see
  the vacuity floor in main(). CORPUS_MAY_BE_EMPTY in the declaration is the
  committed way to state that a repository has no private corpus yet.

  Any other output shape is a contract violation the caller treats as
  fail-closed. Exit status is 0 for a completed analysis (clean or blocking),
  2 for BROKEN — but the caller reads the LINES, not the status, because a
  scanner that says CLEAN on stdout and 2 on exit is ambiguous and ambiguity
  here is what gets content published.
"""

import array
import bisect
import hashlib
import json
import marshal
import mmap
import os
import re
import shutil
import sys
import subprocess
import tempfile
import time
import uuid

SOURCE_MAX_BYTES = 8_000_000
import urllib.parse

# --- the recorded-speech shape ---------------------------------------------
#
# Three real-world transcript layouts. Between them they cover whisper/
# whisper-cli output, SRT/WebVTT captions, and the Otter/Zoom/Descript
# "Name (0:12):" export style.
#
# The near-miss these are built against is a LOG line — `[12:34:56] ERROR:
# connection refused` matches the timestamp+colon skeleton exactly. Two
# independent filters separate them: a denylist of log-level and log-field
# tokens in the speaker slot, and a prose test on the text that requires real
# words and rejects dense code punctuation. Measured across 57,034 files in
# eleven repositories: 17 flagged, all 17 genuine transcripts, zero
# non-transcript false positives.

"""
A note on the leading class, because it was measured and then narrowed.

It began as `[\\s>*-]*` — leading whitespace, blockquote markers, and markdown
list bullets. The bullet was the mistake. A CHANGELOG line

    - 12:03 Dana: rewrote the loader so it no longer reads the whole file

is a list item, not speech, and eight of them tripped the detector. So `*` and
`-` are gone and only `>` survives: a transcript pasted into a blockquote is a
real shape, a transcript pasted as a bullet list is not one anybody produces.
The cost is stated rather than hidden — a quote inside a markdown LIST is now
invisible to this detector — and it is covered by the corpus detector, which
does not care about line shape at all.
"""

TS_SPEAKER = re.compile(
    r'^[\s>]*'
    r'[\[\(]?\s*(?:\d{1,2}:)?\d{1,2}:\d{2}(?:[.,]\d{1,3})?\s*[\]\)]?'
    r'\s*[-–—]?\s*'
    r'(?P<who>[A-Za-z][A-Za-z0-9_.\'’-]{0,24}'
    r'(?:\s+[A-Za-z][A-Za-z0-9_.\'’-]{0,24}){0,2})'
    r'\s*:\s*'
    r'(?P<text>\S.*)$'
)

PAREN_SPEAKER = re.compile(
    r'^[\s>]*'
    r'(?P<who>[A-Z][A-Za-z0-9_.\'’-]{0,24}'
    r'(?:\s+[A-Z][A-Za-z0-9_.\'’-]{0,24}){0,2})'
    r'\s*[\[\(]\s*(?:\d{1,2}:)?\d{1,2}:\d{2}(?:[.,]\d{1,3})?\s*[\]\)]\s*:\s*'
    r'(?P<text>\S.*)$'
)

CUE = re.compile(
    r'^\s*\d{1,2}:\d{2}:\d{2}[,.]\d{1,3}\s*-->\s*\d{1,2}:\d{2}:\d{2}[,.]\d{1,3}'
)

# Tokens that occupy the speaker slot in machine output, never in a transcript.
LOGISH = set("""
info warn warning error debug trace fatal notice verbose log stdout stderr
ok pass fail failed todo note out in get post put patch delete http https
usage example tip exit rc pid cmd run step time took elapsed duration
""".split())

# Punctuation that appears in code and configuration and effectively never in
# a spoken sentence.
CODEY = set('{}<>=;|&$#`\\')


def _is_prose(text):
    if len(text.split()) < 4:
        return False
    if sum(text.count(c) for c in CODEY) > 2:
        return False
    return True


def speech_lines(blob, cap=None):
    """Count transcript-shaped lines. Stops early once `cap` is reached, so a
    92-minute transcript costs the same as an eight-line one."""
    n = 0
    for line in blob.splitlines():
        if CUE.match(line):
            n += 1
        else:
            m = TS_SPEAKER.match(line) or PAREN_SPEAKER.match(line)
            if not m:
                continue
            if m.group('who').strip().lower() in LOGISH:
                continue
            if not _is_prose(m.group('text')):
                continue
            n += 1
        if cap is not None and n >= cap:
            return n
    return n


# --- the derived-from-private corpus ---------------------------------------

WORD = re.compile(r"[a-z0-9]+")

TEXT_EXT = {
    '.md', '.txt', '.json', '.jsonl', '.vtt', '.srt', '.csv', '.log', '.tsv',
    '.rst', '.text', '',
}

SKIP_DIRS = {'.git', 'node_modules', '.venv', 'venv', 'dist', 'build',
             '__pycache__', '.next', 'target', 'Pods', '.gradle'}

# --- media provenance: the third way into the corpus ------------------------
#
# THE HOLE THIS CLOSES, measured 2026-08-30. The two routes above are (1) the
# recorded-speech SHAPE and (2) the closure, which admits another RENDERING of
# something already admitted. Between them they cannot admit the FIRST
# rendering of a recording that was only ever transcribed to plain text.
#
# whisper's plain `.txt` output carries no timestamps and no speaker labels, so
# it has zero transcript-shaped lines and route 1 rejects it. Route 2 can only
# EXTEND a seed, never create one — so a recording with no timestamped
# rendering anywhere is invisible to the corpus entirely, whole.
#
# That was not hypothetical. Three real private recording transcripts sat in a
# declared PRIVATE_SOURCES tree — 5,713, 6,424 and 22,375 words — and the corpus
# contained NONE of them: it held ten files, every one a rendering of a single
# other recording, admitted because ONE timestamped rendering of that recording
# happened to exist. A 6,000-character extract of the first of those transcripts
# was written into the publication-bound repository and BOTH guards returned
# exit 0, silently. The write guard and the commit
# guard share this predicate, so the commit chokepoint — the backstop that
# exists precisely to catch what the write hook misses — missed it identically.
#
# THE SIGNAL, and why it is provenance rather than content. There is no
# reliable content shape for plain whisper output: it is prose, and every
# content-side widening tried against the real trees was rejected with numbers
# (see the closure constants below). But the tree knows something the bytes do
# not — the transcript is sitting NEXT TO THE RECORDING IT CAME FROM, under a
# name derived from it:
#
#     002 Jane Marsh podcast.mp3          <- the recording
#     002 Jane Marsh podcast transcript.txt   <- a rendering of it
#
# A text file in a declared-private directory whose stem extends the stem of a
# media file in that same directory is a rendering of that recording. That is a
# fact about the tree, not a heuristic about words, and it is the least
# ambiguous case there is: the whole incident this mechanism exists for was
# "the audio was correctly gitignored, the transcript was not".
#
# MEASURED, both directions, across 5,353 tracked text files in eleven
# repositories:
#
#   admits              exactly the 3 recording transcripts. The closure then
#                       pulls in a fourth file on its own merits — a reference
#                       worksheet 80.9% covered by them (763 of 943 windows),
#                       i.e. genuinely another rendering. Corpus:
#                       10 files / 83,793 words -> 14 files / 130,466 words.
#   costs               ONE new colliding phrase across all eleven trees: a
#                       single 10-word run, 8 of its 10 words function words,
#                       sitting at the MIN_QUOTE_WORDS floor and extending no
#                       further, which appears in 4 files in one repository that
#                       declares no publication boundary, so no guard ever runs
#                       there.
#   where it counts    ZERO, before and after, in the only repository on that
#                       machine that declares a boundary: the count of legitimate
#                       files it would block is unchanged.
#
# THE WIDER RULE WAS REJECTED. "Any text file in a directory that holds media"
# needs no stem match and reaches the SAME 14-file corpus — but it gets there by
# admitting a 51 KB mixed reference worksheet DIRECTLY, on the strength of a
# coincidence of directory rather than of derivation. Admitting mixed documents
# on weak evidence is the exact move that put engineering boilerplate into the
# "private" corpus and blocked LICENSE files (see below). The narrow rule
# reaches the same place by the principled route and leaves the coincidence
# unused, so the narrow rule is what ships.
MEDIA_EXT = {'.mp3', '.mp4', '.m4a', '.wav', '.aac', '.flac', '.ogg', '.opus',
             '.mov', '.mkv', '.webm', '.avi', '.aiff', '.wma', '.m4v'}

# The shorter of the two stems must be at least this many alphanumeric
# characters. Below it a stem is a generic word — `notes`, `readme`, `audio`,
# `part1` — that matches by coincidence rather than by naming, and a coincidence
# is not provenance. Conservative rather than measured: on the real tree the
# stems in play are 19 characters ("002janemarshpodcast"), so every value from 1
# to 19 admits exactly the same files and the measurement above cannot
# distinguish them. Said plainly rather than dressed up as a finding.
MEDIA_STEM_MIN_CHARS = 8


def _stem_key(filename):
    """A filename's stem, reduced to lowercase alphanumerics.

    Reduced rather than compared raw so that `002 Jane Marsh podcast.mp3` and
    `002_jane_marsh_podcast.txt` are recognized as the same name — separators
    are an export-tool detail, not a difference in provenance.
    """
    return ''.join(c for c in os.path.splitext(filename)[0].lower() if c.isalnum())


def _renders_media(path, media_stems_by_dir):
    """True when `path` is named as a rendering of a media file beside it.

    The relation is prefix, in either direction, because both naming habits are
    real: `recording.mp3` -> `recording.transcript.txt` extends the media stem,
    and `interview.txt` -> `interview-part1.mp3` extends the text stem. Equality
    is the degenerate prefix and is included.
    """
    stems = media_stems_by_dir.get(os.path.dirname(path))
    if not stems:
        return False
    tk = _stem_key(os.path.basename(path))
    if not tk:
        return False
    for mk in stems:
        short, long_ = (tk, mk) if len(tk) <= len(mk) else (mk, tk)
        if len(short) >= MEDIA_STEM_MIN_CHARS and long_.startswith(short):
            return True
    return False


def normalise(text):
    """Lowercase, drop apostrophes, keep only alphanumeric word tokens.

    Matching on this form is what lets a quote survive re-punctuation: prose
    that wrote `"we shouldn't ship it like that"` and a transcript that has
    `we shouldnt ship it like that` both normalise to the same token run.

    The illustration is invented on purpose. An earlier draft used a real
    phrase from the incident that prompted this guard — which put a sample of
    the material inside the thing built to keep it out."""
    return WORD.findall(text.lower().replace('’', '').replace("'", ''))


# --- private BY IDENTITY, not by resemblance --------------------------------
#
# WHY THIS EXISTS BESIDE THE OTHER TWO, AND NOT INSTEAD OF THEM.
# The corpus and shape detectors above answer "does this LOOK like private
# speech?", and they were measured against the material that leaked:
# two-channel transcripts, and prose quoting them. They are sharp for that and
# they are BLIND to a small named file that is private for a reason no scoring
# function can see — a short note about a design decision, resembling nothing.
#
# Measured, both halves: that file's exact bytes were written into the
# publication-bound tree and staged, and both guards returned 0. The
# mechanism was sound for what it was aimed at and had nothing to say about
# identity. This is the addition; the scanners above are untouched.
#
# IDENTITY IS DECLARED, AND THE DECLARATION IS SELF-CONTAINED.
# Each PRIVATE_FILES entry is `<64-hex-digest>:<name>`, committed in
# .publication-boundary. Nothing here reads the private repository at guard
# time — deliberately. A guard whose coverage depends on a sibling checkout
# existing is a guard that does NOTHING on a fresh clone, which is the machine
# that matters: the one where somebody who has never seen the rule is about to
# break it. The digest and the name travel with the repository they protect.
#
# WHAT A DIGEST OF PRIVATE CONTENT DISCLOSES: nothing. sha256 is one-way, and
# recovering a document from it means already having the document. The NAME is
# in the clear, because a refusal that cannot say which file it means teaches
# nobody anything, and a refusal nobody understands gets worked around. That
# trade is stated in .publication-boundary's own header so an operator declaring
# the next file makes it knowingly.
#
# WHAT DEFEATS THIS, said plainly rather than discovered later:
#
#   * A REWRITE. Add or remove a word and the content digest changes. Identity
#     is exact by construction — the alternative is a similarity score, and a
#     probabilistic accusation is what index_corpus refuses to make.
#   * A REWRITE **AND** A RENAME, together. Either one alone is caught: the
#     digest survives renaming, and the name rule survives rewriting. Both at
#     once is a different file wearing a different name, and no rule short of
#     reading the private repository can call that.
#   * A PARTIAL EXCERPT under a different name. Half the file is not the file.
#     The corpus detector covers excerpts only for material that qualifies as
#     private SPEECH, which is precisely what this class of file is not.
#
# WHAT DOES NOT DEFEAT IT — and this is the point of hashing the WORD SEQUENCE
# rather than only the bytes: re-encoding (the seeded file is UTF-16 with a BOM,
# because it came out of a Mac text editor), re-wrapping, re-indenting, changing
# markdown emphasis, changing case, or pasting the words into a differently
# formatted document. All of those produce different bytes and the same words.
# Both digests are compared, so an operator may declare either flavor and a
# binary file — which has no words — is still covered by its bytes.

# A file larger than this is not content-hashed: the name rule still applies,
# and reading half a gigabyte inside a PreToolUse hook to compare it against a
# note is a guard that times out, which is a guard that did not run.
IDENTITY_MAX_BYTES = 64 * 1024 * 1024

_BOM_UTF8 = b'\xef\xbb\xbf'
_BOM_UTF16 = (b'\xff\xfe', b'\xfe\xff')


def decode_best(raw):
    """Bytes to text, BOM-aware.

    NOT decoration. The file this mechanism was seeded with is UTF-16BE with a
    BOM, so `raw.decode('utf-8', 'ignore')` turns every character into its own
    NUL-separated token and the word sequence it yields is a sequence of single
    letters — a digest that matches nothing a person would ever write. The same
    file re-saved as UTF-8 must hash identically to the original, or "reformat
    does not defeat it" is a claim with a counter-example in the seed itself."""
    if raw.startswith(_BOM_UTF8):
        return raw[len(_BOM_UTF8):].decode('utf-8', 'ignore')
    if raw[:2] in _BOM_UTF16:
        try:
            return raw.decode('utf-16')
        except (UnicodeDecodeError, ValueError):
            pass
    return raw.decode('utf-8', 'ignore')


def content_digests(raw=None, text=None):
    """Every digest this content could legitimately be declared under.

    A declared digest matches when it equals ANY of them, so the operator may
    commit whichever one their tooling produced:

      bytes  sha256 of the file exactly as it sits on disk;
      words  sha256 of the word sequence `normalise` produces, which is what
             survives re-encoding and reformatting.

    A `text` item (a Write payload) has no bytes on disk yet, so the bytes it
    WOULD have — its UTF-8 encoding — is hashed instead."""
    out = set()
    if raw is not None and len(raw) <= IDENTITY_MAX_BYTES:
        out.add(hashlib.sha256(raw).hexdigest())
        if text is None:
            text = decode_best(raw)
    if text is not None:
        if raw is None:
            out.add(hashlib.sha256(text.encode('utf-8')).hexdigest())
        words = normalise(text)  # dialect-exempt: existing identifier in this file
        out.add(hashlib.sha256(' '.join(words).encode('utf-8')).hexdigest())
        if '\x00' in text:
            # UTF-16 BYTES ARRIVING AS A STRING. A tool payload carries text,
            # not bytes, so a caller that hands over UTF-16 content read as
            # UTF-8 delivers every character NUL-separated — and the word
            # sequence of that is a run of single letters, which matches
            # nothing. Measured here, on the seeded file's own encoding. The
            # NUL-stripped form is hashed as well, so the same words reach the
            # same digest whichever way they were carried.
            stripped = normalise(text.replace('\x00', ''))  # dialect-exempt: existing identifier in this file
            out.add(hashlib.sha256(' '.join(stripped).encode('utf-8')).hexdigest())
    return out


def parse_private_files(entries):
    """`<digest>:<name>` tokens to (digest, name, lowercased basename).

    The FORMAT is validated in the shell library, at declaration-parse time,
    where a malformed entry becomes a BROKEN declaration and blocks. This is
    the defensive half: a token that does not split is dropped rather than
    mis-parsed, and the IDENTITY trailer counts what actually loaded, so a run
    can never report that it checked identities it never held."""
    out = []
    for entry in entries or []:
        if not isinstance(entry, str) or ':' not in entry:
            continue
        digest, name = entry.split(':', 1)
        digest = digest.strip().lower()
        # The minter writes whitespace (and '%') in a name as %XX, because an
        # entry is one whitespace-delimited token; undo that here.
        name = urllib.parse.unquote(name.strip())
        if len(digest) != 64 or not name:
            continue
        out.append((digest, name, os.path.basename(name.rstrip('/')).lower()))
    return out


def identity_findings(label, private_files, raw=None, text=None):
    """The declared-private verdicts for one item, strongest first.

    CONTENT before NAME: "these are the file's bytes" is a fact about the
    material, and "this is the file's name" is a fact about the destination.
    When both hold, the author is told the stronger one.

    The evidence NEVER quotes the content. A refusal that reproduced the
    private material in order to say it must not be reproduced would be this
    mechanism's own defect, executed inside its error message."""
    if not private_files:
        return []
    mine = content_digests(raw=raw, text=text)
    for digest, name, base in private_files:
        if digest in mine:
            return [('BLOCK', label, 'declared-private-file',
                     "content is the declared private file '%s' (identity by "
                     "digest; a rename does not change it)" % name)]
    itsbase = os.path.basename(str(label).rstrip('/')).lower()
    for digest, name, base in private_files:
        if base and itsbase == base:
            return [('BLOCK', label, 'declared-private-name',
                     "'%s' is declared private by name; a file with this name "
                     "does not belong in this repository whatever it contains"
                     % name)]
    return []


class BrokenCorpus(Exception):
    pass


# --- the derived-rendering closure -----------------------------------------
#
# WHY THE SHAPE FILTER ALONE IS NOT THE CORPUS.
#
# Measured on the real private record on 2026-08-30: 481 candidate text files
# under the declared PRIVATE_SOURCES, and the shape filter kept TWO. The
# verbatim-quote detector — the half that catches speech quoted inside ordinary
# prose, which is how quotes reached the public tree in the first place — was
# matching against 26,339 words, one recording, while the same private tree
# held SEVEN MORE two-channel transcripts of real recordings that the
# shape filter cannot see: whisper `.txt` output carries no timestamps and no
# speaker labels, so it has zero transcript-shaped lines. 3,764 to 13,880 words
# of private speech each, invisible.
#
# THE RULE. A private file joins the corpus when it is ANOTHER RENDERING of
# speech already in the corpus — not when it merely quotes it. Two conditions,
# both required, because each alone fails in a direction that was measured:
#
#   CLOSURE_MIN_WINDOWS    at least this many DISTINCT non-overlapping
#                          min_quote_words-windows of the file appear in the
#                          corpus. Absolute, so a document that quotes one
#                          sentence cannot join; distinct, so a file that repeats
#                          the same quoted line — which is exactly what a
#                          transcript with a hallucination loop looks like —
#                          cannot accumulate its way in on one phrase.
#   CLOSURE_MIN_COVERAGE   and that many as a FRACTION of the file's own
#                          windows. Scale-free, so a large document cannot
#                          accumulate its way in on boilerplate either.
#
# NON-OVERLAPPING is a cost decision, stated because it is a real approximation.
# Hashing every position of every candidate is 10x the work and measured at
# +0.8s on every single Write, Edit and commit in the repository — a guard that
# taxes every keystroke is a guard someone turns off, which is the same failure
# as one that blocks ordinary work. Every tenth position is an unbiased estimate
# of the same ratio, it is deterministic rather than sampled, and it left the
# admitted set and the margins where they were.
#
# MEASURED, not chosen. Against the real private record:
#
#   admitted (8 files)   73 - 269 shared windows, 25% - 45% coverage. Every one
#                        a genuine transcript of a real recording: two channels
#                        x two recordings x two model runs.
#   nearest EXCLUDED     14 windows / 2.8% coverage — an engineering brief that
#                        QUOTES the transcript. 2.9x below the gate; the lowest
#                        admission is 1.8x above it, and has to clear the
#                        coverage condition as well.
#
# and the reason that margin matters is the direction this fails in. An earlier
# draft admitted any file sharing ONE 10-word run. That pulled 251 private
# engineering documents into the "private" corpus, whose ordinary boilerplate
# then blocked 206 of 5,333 files across the real public trees — including
# LICENSE files, .gitignore and package.json. Admitting one mixed brief is
# enough to do damage: at a 40-word inbound threshold the corpus took in a
# single brief and its boilerplate header line and a scratchpad PATH promptly
# blocked five legitimate public files,
# one of them the technology evaluation .publication-boundary names as
# deliberately public. Under the rule above, both of those documents are
# excluded and the false-positive count across those same 5,333 public files is
# ZERO — unchanged from the narrow corpus.
#
# WHAT WAS REJECTED, with its numbers, so nobody re-proposes it as an
# improvement: harvesting every quoted prose run out of every private file
# (1,339 runs) raises recall on the one shape this cannot see — typed words in
# a private wiki page, quoted nowhere else — and blocks 98 public
# files, 23 of them in the publication-bound repository itself, including its
# README, its WALKTHROUGH and two agent definitions. Doctrine sentences live in
# both trees on purpose. That widening is not available at any threshold and
# the gap it leaves is stated in the header of publication-boundary.sh.
CLOSURE_MIN_WINDOWS = 40
CLOSURE_MIN_COVERAGE = 0.08
# Rounds, not one pass: on the real tree a fourth transcript only crossed the
# gate once the three renderings of ITS recording were themselves in the corpus
# (round 2). The cap is a bound on work, and it is never a silent truncation —
# the loop stops when a round admits nothing, which is what happened at round 3.
CLOSURE_MAX_ROUNDS = 3


def _gram_hashes(words, n, stride=1):
    """The n-word windows of `words`, as a set of hashes.

    stride=1 for the corpus side, where every position must be present or a
    genuine reproduction could slip between two windows. stride=n for the
    candidate side, where the question is a RATIO and every tenth window
    estimates it at a tenth of the cost.

    Same fast-path-not-verdict discipline as index_corpus: a collision here can
    only over-count a file's overlap by one window out of dozens, and admission
    needs CLOSURE_MIN_WINDOWS of them and a coverage fraction besides.
    """
    return {hash(tuple(words[i:i + n])) for i in range(0, len(words) - n + 1, stride)}


# --- THE BUILT CORPUS IS KEPT ON DISK (2026-10-21) ----------------------------
#
# WHY. Built from scratch, the corpus cost ~40 CPU-s and 1.3 GB on EVERY Write
# and commit to the public repository: 26,956 private files read, 32 s of
# speech-shape regex, 12.6 s of word splitting, 4.2 s of indexing (profiled
# 2026-10-21 against the real private record, 1,031 members, 10.07 M words).
# With agents writing all day that was ~9% of the whole Mac by itself, and the
# private record barely changes between two writes.
#
# WHAT IS KEPT. Per private file, keyed by (size, mtime, ctime, inode, mode):
# its speech-line count and its normalized words. Per corpus: which files are
# members, the corpus text and its sorted window index. A file whose key is
# unchanged is not read again; a corpus whose every file key and every
# admission decision is unchanged is not rebuilt at all. Anything else rebuilds
# from the kept per-file data plus the files that changed, through the SAME
# closure code below, so every verdict is the verdict a cold build gives.
#
# WHERE. Outside every repository: RICHOS_PUBLICATION_CACHE_DIR when set (the
# tests point it at their own scratch), otherwise the machine's cache root
# (`machine_settings.py cache publication-boundary`), mode 0700, because it
# holds derived private text exactly as sensitive as the sources it was read
# from. A corpus whose sources sit under the system temp directory is scratch
# and is never cached unless RICHOS_PUBLICATION_CACHE_DIR asks for it, so a
# test run leaves nothing behind. A cache entry unused for a day is removed.
#
# THE ONE NEW REQUIREMENT. The kept index stores Python hash() values, which
# are only stable across processes under a fixed PYTHONHASHSEED; main()
# re-executes itself with PYTHONHASHSEED=0 when it will use the cache. A hash
# is the fast path and never the verdict (see index_corpus), so the seed value
# changes no decision.
CACHE_VERSION = 1
CACHE_IDLE_SECONDS = 24 * 3600


def _under_temp(path):
    real = os.path.realpath(path)
    for root in {os.path.realpath(tempfile.gettempdir()), '/private/tmp', '/private/var/folders'}:
        if real == root or real.startswith(root.rstrip('/') + '/'):
            return True
    return False


def cache_root(sources):
    """The cache directory to use for these sources, or None for no cache."""
    forced = os.environ.get('RICHOS_PUBLICATION_CACHE_DIR')
    if forced:
        return forced
    if not sources or any(_under_temp(s) for s in sources):
        return None
    try:
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        import machine_settings
        return str(machine_settings.cache('publication-boundary'))
    except Exception:  # noqa: BLE001 — no cache root means no cache, never a failure
        return None


class CachedCorpus:
    """The corpus as main() and verbatim_run use it: truthiness, `in`, a word
    count and its window index. The text is a str, or a read-only mapping of
    the kept ASCII text file (normalise only ever yields [a-z0-9] and spaces)."""

    def __init__(self, text, index, words):
        self.text, self.index, self.words = text, index, words

    def __bool__(self):
        return len(self.text) > 0

    def __contains__(self, s):
        if isinstance(self.text, str):
            return s in self.text
        return self.text.find(s.encode('ascii')) != -1


class SortedIndex:
    """A sorted array of int64 window hashes, memory-mapped, searched by bisect.
    Same membership answers as the set index_corpus builds, without rebuilding
    or loading ten million entries per call."""

    def __init__(self, path):
        with open(path, 'rb') as fh:
            size = os.fstat(fh.fileno()).st_size
            self._map = mmap.mmap(fh.fileno(), 0, prot=mmap.PROT_READ) if size else b''
        self._view = memoryview(self._map).cast('q') if size else []

    def __len__(self):
        return len(self._view)

    def __contains__(self, h):
        i = bisect.bisect_left(self._view, h)
        return i < len(self._view) and self._view[i] == h


class CorpusCache:
    """One cache entry: the per-file records and the last built corpus for one
    set of sources and parameters. Every load failure is a cache miss."""

    def __init__(self, root, sources, params):
        ident = json.dumps([CACHE_VERSION, sys.version, _code_digest(), list(sources), params])
        self.dir = os.path.join(root, hashlib.sha256(ident.encode()).hexdigest()[:24])
        self.files, self.words, self.result = {}, None, None
        try:
            with open(os.path.join(self.dir, 'meta'), 'rb') as fh:
                meta = marshal.load(fh)
            self.files, self.result = meta['files'], meta['result']
        except Exception:  # noqa: BLE001 — unreadable cache: start empty
            self.files, self.result = {}, None

    def record(self, path, key):
        rec = self.files.get(path)
        return rec[1] if rec and rec[0] == key else None

    def word_text(self, path):
        """The kept normalized words of an unchanged file, or None."""
        if self.words is None:
            self.words = {}
            gen = (self.result or {}).get('gen')
            if gen:
                try:
                    with open(os.path.join(self.dir, '%s.words' % gen), 'rb') as fh:
                        self.words = marshal.load(fh)
                except Exception:  # noqa: BLE001 — missing words: the file is read again
                    self.words = {}
        return self.words.get(path)

    def load_result(self, fingerprint):
        r = self.result
        if not r or r.get('fingerprint') != fingerprint:
            return None
        try:
            index = SortedIndex(os.path.join(self.dir, '%s.index' % r['gen']))
            with open(os.path.join(self.dir, '%s.corpus' % r['gen']), 'rb') as fh:
                size = os.fstat(fh.fileno()).st_size
                text = mmap.mmap(fh.fileno(), 0, prot=mmap.PROT_READ) if size else ''
        except Exception:  # noqa: BLE001 — a missing generation is a rebuild
            return None
        os.utime(os.path.join(self.dir, 'meta'))
        return CachedCorpus(text, index, r['corpus_words']), list(r['members'])

    def save(self, records, words, fingerprint, members, corpus, index):
        """Write a new generation, then point meta at it in one rename."""
        try:
            os.makedirs(self.dir, mode=0o700, exist_ok=True)
            gen = uuid.uuid4().hex[:12]
            base = os.path.join(self.dir, gen)
            with open(base + '.words', 'wb') as fh:
                marshal.dump(words, fh)
            with open(base + '.corpus', 'wb') as fh:
                fh.write(corpus.encode('ascii'))
            with open(base + '.index', 'wb') as fh:
                array.array('q', sorted(index)).tofile(fh)
            meta = {'files': records, 'result': {
                'gen': gen, 'fingerprint': fingerprint, 'members': members,
                'corpus_words': len(corpus.split())}}
            tmp = os.path.join(self.dir, 'meta.%s' % gen)
            with open(tmp, 'wb') as fh:
                marshal.dump(meta, fh)
            os.replace(tmp, os.path.join(self.dir, 'meta'))
            for fn in os.listdir(self.dir):
                if fn != 'meta' and not fn.startswith(gen + '.'):
                    try:
                        os.unlink(os.path.join(self.dir, fn))
                    except OSError:
                        pass
            _prune(os.path.dirname(self.dir), keep=self.dir)
        except Exception:  # noqa: BLE001 — a cache that cannot be written is only slower
            pass


def _prune(root, keep):
    now = time.time()
    for name in os.listdir(root):
        entry = os.path.join(root, name)
        if entry == keep:
            continue
        try:
            if now - os.stat(os.path.join(entry, 'meta')).st_mtime > CACHE_IDLE_SECONDS:
                shutil.rmtree(entry, ignore_errors=True)
        except OSError:
            continue


def _code_digest():
    with open(os.path.abspath(__file__), 'rb') as fh:
        return hashlib.sha256(fh.read()).hexdigest()


def _file_key(st):
    return (st.st_size, st.st_mtime_ns, st.st_ctime_ns, st.st_ino, st.st_mode)


def build_corpus(sources, min_speech_lines, max_files, max_bytes,
                 min_quote_words=10, cache=None):
    """Walk the declared private trees and keep the files that ARE private
    speech: the ones that look like a recording, plus the ones that are another
    rendering of a recording already kept.

    This is the composition that buys the precision. The operator declares
    WHERE private material lives — a fact only they know. The machine works out
    WHICH files it is — a fact it can check. A dated list of transcript paths
    would be stale by the next recording; this cannot be. And because a file
    enters only by looking like speech or by REPRODUCING speech already in the
    corpus, a boilerplate sentence shared between two ordinary engineering
    documents can never collide with it — see the closure constants above for
    the measurement that pins that claim.

    Hitting a bound is BROKEN, never a quiet truncation: a corpus that silently
    stopped short would let the guard pass content it simply never looked at,
    which is the original "no media committed" failure wearing a different hat.
    """
    # Per candidate: its file key and (speech lines, word count); the files that
    # take part, in walk order, as members or closure candidates; and the
    # normalized words of every file read in this run. With a cache, an
    # unchanged file's record comes from the cache and the file is not read.
    records = {}
    order = []
    fresh = {}
    # A file is a possible rendering only if it is long enough to reach
    # CLOSURE_MIN_WINDOWS at all, so the eligibility test is arithmetic rather
    # than a guess, and short files cost nothing.
    eligible_words = (CLOSURE_MIN_WINDOWS + 1) * min_quote_words
    files = 0
    total = 0
    # Media stems per directory, collected in the SAME walk as the candidates —
    # this costs no extra file reads and no extra stat calls, only the names
    # os.walk already handed us. See _renders_media for what it is for.
    media_stems = {}
    for src in sources:
        if os.path.isfile(src):
            candidates = [src]
            d = os.path.dirname(src)
            try:
                siblings = os.listdir(d)
            except OSError:
                siblings = []
            stems = {_stem_key(fn) for fn in siblings
                     if os.path.splitext(fn)[1].lower() in MEDIA_EXT}
            if stems:
                media_stems.setdefault(d, set()).update(stems)
        else:
            candidates = []
            for dirpath, dirnames, filenames in os.walk(src):
                dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
                for fn in filenames:
                    ext = os.path.splitext(fn)[1].lower()
                    if ext in TEXT_EXT:
                        candidates.append(os.path.join(dirpath, fn))
                    elif ext in MEDIA_EXT:
                        media_stems.setdefault(dirpath, set()).add(_stem_key(fn))
        for p in candidates:
            files += 1
            if files > max_files:
                raise BrokenCorpus(
                    "the declared PRIVATE_SOURCES contain more than "
                    "CORPUS_MAX_FILES=%d candidate text files. Narrow "
                    "PRIVATE_SOURCES to the trees that actually hold recordings, "
                    "or raise CORPUS_MAX_FILES deliberately. Refusing to scan a "
                    "truncated corpus and report the result as clean." % max_files)
            # A DECLARED PRIVATE SOURCE THAT CANNOT BE READ WHOLE IS NOT A SOURCE
            # THAT WAS CHECKED (hunt P5-08). It used to be skipped, so a private
            # recording this run could not read was simply absent from the corpus
            # and a published quotation of it came back CLEAN. Only a path that
            # is no longer there (a dangling link, a file removed mid-walk) has
            # nothing to compare against and is skipped.
            try:
                st = os.stat(p)
                size = st.st_size
            except FileNotFoundError:
                continue
            except OSError as exc:
                raise BrokenCorpus(
                    "the private source %s could not be examined (%s). A corpus missing "
                    "a file it was declared to hold cannot vouch that published text "
                    "does not quote it, so this refuses instead of reporting CLEAN. "
                    "Make it readable, or narrow PRIVATE_SOURCES."
                    % (p, exc.strerror or exc))
            if size > SOURCE_MAX_BYTES:
                raise BrokenCorpus(
                    "the private source %s is %d bytes, over the 8000000-byte per-file "
                    "read limit, so it cannot be held in the corpus and a quotation of "
                    "it would come back CLEAN. Refusing instead. Move it out of "
                    "PRIVATE_SOURCES or split it." % (p, size))
            total += size
            if total > max_bytes:
                raise BrokenCorpus(
                    "the declared PRIVATE_SOURCES exceed CORPUS_MAX_BYTES=%d. "
                    "Narrow PRIVATE_SOURCES, or raise the bound deliberately. "
                    "Refusing to scan a truncated corpus and report the result "
                    "as clean." % max_bytes)
            key = _file_key(st)
            rec = cache.record(p, key) if cache is not None else None
            if rec is None:
                try:
                    with open(p, encoding='utf-8', errors='ignore') as fh:
                        blob = fh.read(SOURCE_MAX_BYTES)
                except FileNotFoundError:
                    continue
                except OSError as exc:
                    raise BrokenCorpus(
                        "the private source %s could not be read (%s), so it is missing "
                        "from the corpus and a quotation of it would come back CLEAN. "
                        "Refusing instead. Make it readable, or narrow PRIVATE_SOURCES."
                        % (p, exc.strerror or exc))
                text = ' '.join(normalise(blob))
                rec = (speech_lines(blob, cap=min_speech_lines),
                       text.count(' ') + 1 if text else 0)
                fresh[p] = text
            records[p] = (key, rec)
            lines, nwords = rec
            # Two independent ways to be a SEED: it looks like a recording, or
            # it is named as a rendering of a recording sitting beside it. The
            # second exists because whisper's plain .txt output satisfies
            # neither the shape filter nor the closure, and a recording whose
            # only rendering is plain text was therefore invisible to the corpus
            # entirely — see MEDIA_EXT above for the three real transcripts this
            # was measured against.
            if lines < min_speech_lines and not _renders_media(p, media_stems):
                # Arithmetic, not a guess: below this length the file cannot
                # reach CLOSURE_MIN_WINDOWS non-overlapping windows even if
                # every one of them matched, so it never needs hashing.
                if nwords >= eligible_words:
                    order.append((p, 'pending'))
                else:
                    fresh.pop(p, None)
                continue
            order.append((p, 'member'))

    # The result depends only on which files take part, in what role and in
    # what order, and on their contents: that is the fingerprint. Unchanged,
    # the kept corpus and index are the answer and nothing below runs.
    fingerprint = hashlib.sha256(marshal.dumps(
        [(p, records[p][0], role) for p, role in order])).hexdigest()
    if cache is not None:
        kept = cache.load_result(fingerprint)
        if kept is not None:
            return kept

    def text_of(path):
        text = fresh.get(path)
        if text is None and cache is not None:
            text = cache.word_text(path)
        if text is None:
            try:
                with open(path, encoding='utf-8', errors='ignore') as fh:
                    text = ' '.join(normalise(fh.read(SOURCE_MAX_BYTES)))
            except OSError:
                return None
        return text

    parts = []
    members = []
    # Non-member candidates, kept as gram hashes only.
    pending = {}
    kept_words = {}
    for p, role in order:
        text = text_of(p)
        if text is None:
            continue
        kept_words[p] = text
        if role == 'pending':
            pending[p] = _gram_hashes(text.split(), min_quote_words,
                                      stride=min_quote_words)
        else:
            members.append(p)
            parts.append(text)
    fresh.clear()

    # --- the closure ------------------------------------------------------
    # Nothing to be derived FROM means nothing to derive: with no shape-detected
    # speech the corpus stays empty rather than bootstrapping itself out of
    # ordinary documents.
    if parts and pending:
        index = set()
        for part in parts:
            index |= _gram_hashes(part.split(), min_quote_words)
        for _round in range(CLOSURE_MAX_ROUNDS):
            admitted = []
            for p, grams in pending.items():
                if not grams:
                    continue
                shared = len(grams & index)
                if shared >= CLOSURE_MIN_WINDOWS and \
                        shared / len(grams) >= CLOSURE_MIN_COVERAGE:
                    admitted.append(p)
            if not admitted:
                break
            for p in admitted:
                index |= pending.pop(p)
                # The words were already read (or kept) for this run, from the
                # same version of the file the stat above saw.
                members.append(p)
                parts.append(kept_words[p])

    # The double space is a barrier token: it stops a run from being matched
    # across the seam between two unrelated transcripts.
    corpus = '  '.join(parts)
    if cache is None:
        return corpus, members
    corpus_index = index_corpus(corpus, min_quote_words) if corpus else set()
    cache.save(records, kept_words, fingerprint, members, corpus, corpus_index)
    return CachedCorpus(corpus, corpus_index, len(corpus.split())), members


# --- items: what the caller asked to be scanned ----------------------------

def expand_items(items, max_files, identity_declared=False):
    """Resolve every job item to something with BYTES behind it.

    THE WALK-PAST THIS EXISTS TO CLOSE. `git status --porcelain` reports a
    wholly-new directory as ONE entry — `?? docs/session-notes/` — and a caller
    that passes that entry through hands this scanner a DIRECTORY. `open()` on a
    directory raises IsADirectoryError, the unreadable-path branch in main()
    treats it exactly like a deleted or binary file, and the scan reports CLEAN
    having examined zero bytes. Every leak this was built after was a file inside a
    directory; a new directory of transcripts arriving in one go would have been
    waved through by a guard reporting, on its own terms, the truth.

    So a directory item is EXPANDED here, once, for every caller — the write
    guard, the commit guard, and any by-hand run over a diff.

    Binary files are skipped by a NUL test rather than by extension: an image or
    an audio file carries no reproducible speech TEXT, and media was already
    covered by the check this mechanism replaced. Oversized files are read up to
    the same 8 MB bound the corpus uses.

    WITH IDENTITY DECLARED, a binary file is kept as an identity_only item
    instead of being dropped. It has to be: the first file this mechanism was
    seeded with is UTF-16, so it CONTAINS NUL BYTES, and every text-shaped
    filter in this pipeline calls it binary. A rule that says "this exact file
    never enters the repository" and then hands the file to a filter that
    discards it would be enforcement wearing the shape of enforcement. The cost
    is opt-in: with no PRIVATE_FILES declared nothing changes, and with them
    declared a binary blob is hashed and never text-scanned.

    Exceeding max_files is BROKEN, never a quiet truncation — half a directory
    scanned and reported clean is the defect this whole file exists to end.
    """
    out = []
    seen = 0
    for item in items:
        path = item.get('path')
        if not path or not os.path.isdir(path):
            out.append(item)
            seen += 1
            if seen > max_files:
                raise BrokenCorpus(
                    "the scan job names more than ITEMS_MAX_FILES=%d files. "
                    "Refusing to scan part of it and report the result as "
                    "clean." % max_files)
            continue
        label = item.get('label') or path
        base = label.rstrip('/')
        for dirpath, dirnames, filenames in os.walk(path):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for fn in sorted(filenames):
                fp = os.path.join(dirpath, fn)
                if os.path.islink(fp):
                    # A symlink's target is scanned in its own right if it is
                    # inside the scanned set, and following it here would let one
                    # directory item wander out of the tree it names.
                    continue
                try:
                    with open(fp, 'rb') as fh:
                        head = fh.read(8192)
                except OSError:
                    continue
                binary = b'\0' in head
                if binary and not identity_declared:
                    continue
                seen += 1
                if seen > max_files:
                    raise BrokenCorpus(
                        "expanding the directory items in this scan job passed "
                        "ITEMS_MAX_FILES=%d files. Refusing to scan part of a "
                        "directory and report the result as clean." % max_files)
                rel = os.path.relpath(fp, path)
                entry = {'label': '%s/%s' % (base, rel), 'path': fp}
                if binary:
                    entry['identity_only'] = True
                out.append(entry)
    return out


def index_corpus(corpus, n):
    """Hash every n-word window of the corpus once.

    Substring-searching the corpus for every window of the incoming content is
    O(content x corpus) and goes quadratic on a large file — a guard that times
    out is a guard that did not run. Hashing the corpus once turns the scan into
    one set lookup per word.

    The hash is a FAST PATH, never the verdict: a hit is re-verified against the
    corpus text below, so a hash collision can produce a wasted comparison and
    never a wrong block. Precision is the contract; a probabilistic accusation
    would not honour it."""
    words = normalise(corpus)
    return {hash(tuple(words[i:i + n])) for i in range(0, len(words) - n + 1)}


# --- ALREADY PUBLISHED (2026-09-25) -------------------------------------------
#
# The detector asks "are these words in a private record?". That is the wrong
# question for words the public repository ALREADY carries: a private record
# that quotes a public line (a console log quoting proof-run.py's output, a note
# quoting a suite's NOT RUN message) does not make that line private, and a
# commit that keeps it discloses nothing new. The rule refused exactly that
# twice: native-ios-app.test.sh line 118 (quoted whole in a private record) and
# proof-run.py (esc-20260924T234750Z-6bbc3ae9), so neither file could be
# changed at all.
#
# So a window that also appears verbatim in the item's published text (the same
# path at the public remote's main, supplied by the caller) is not counted. It
# is narrow on purpose: EXACT windows of the SAME file as published, compared as
# word tuples rather than hashes, so a collision can never excuse a private
# phrase; any window with a new word in it is still judged, so a new private
# phrase added to a public file is still refused. No published text, no
# exemption.
def published_windows(item, n):
    """The exact n-word windows of the item's already-published text, or an
    empty set when it has none."""
    path = item.get('published_path')
    if not path:
        return set()
    try:
        with open(path, 'rb') as fh:
            text = decode_best(fh.read(SOURCE_MAX_BYTES))
    except OSError:
        return set()
    words = normalise(text)  # dialect-exempt: existing identifier in this file
    return {tuple(words[i:i + n]) for i in range(0, len(words) - n + 1)}


def verbatim_run(content, corpus, index, n, published=frozenset()):
    """First run of >= n consecutive words reproduced verbatim from the corpus
    and NOT already in the published text (ALREADY PUBLISHED above).

    Short-circuits on the first hit, then extends it only far enough to report
    honest evidence — the author needs to know WHAT was reproduced, not every
    place it appears."""
    if not corpus or not index:
        return None
    words = normalise(content)  # dialect-exempt: existing identifier in this file
    for i in range(0, len(words) - n + 1):
        window = tuple(words[i:i + n])
        if hash(window) not in index:
            continue
        if window in published:     # already public: not a new disclosure
            continue
        cand = ' '.join(window)
        if cand not in corpus:      # collision — not a match, keep going
            continue
        j = i + n
        # Bounded extension: enough evidence to be convincing, never a re-scan.
        while j < len(words) and (j - i) < 40 and ' '.join(words[i:j + 1]) in corpus:
            j += 1
        return words[i:j]
    return None


def mint(path):
    """`--digest <file>` — print the PRIVATE_FILES entry for a file.

    THE MINTER LIVES HERE, IN THE SCANNER, and not in a helper script beside
    it. The digest an operator commits and the digest the guard computes have
    to be the same number, and the only way to guarantee that is for there to
    be one implementation of it. A second copy in a convenience wrapper would
    be a recipe that drifts, and a declaration minted by a drifted recipe
    protects nothing while looking exactly like one that does.

    Run it against the file IN THE PRIVATE RECORD, where it lives. Nothing is
    copied and nothing is printed but hashes and the file's own name."""
    try:
        with open(path, 'rb') as fh:
            raw = fh.read(IDENTITY_MAX_BYTES + 1)
    except OSError as exc:
        print("cannot read %s: %s" % (path, exc), file=sys.stderr)
        return 2
    if len(raw) > IDENTITY_MAX_BYTES:
        print("%s is larger than IDENTITY_MAX_BYTES=%d; content identity does "
              "not apply to it and only the name rule would fire."
              % (path, IDENTITY_MAX_BYTES), file=sys.stderr)
        return 2
    text = decode_best(raw)
    name = os.path.basename(path)
    # An entry is one whitespace-delimited token in the declaration, so a name
    # with whitespace is written percent-encoded (parse_private_files decodes).
    # The header line below shows the real name.
    declared = urllib.parse.quote(name, safe="/:@!$&'()*+,;=-._~") if re.search(r'[\s%]', name) else name
    by_bytes = hashlib.sha256(raw).hexdigest()
    words = normalise(text)  # dialect-exempt: existing identifier in this file
    by_words = hashlib.sha256(' '.join(words).encode('utf-8')).hexdigest()
    print("# %s — %d bytes, %d words" % (name, len(raw), len(words)))
    print("# bytes-only identity (a re-save under a different encoding "
          "defeats it):")
    print("#   PRIVATE_FILES=\"%s:%s\"" % (by_bytes, declared))
    print("# word-sequence identity — RECOMMENDED for text; survives "
          "re-encoding,")
    print("# re-wrapping and reformatting. Both are accepted; declare one:")
    print("PRIVATE_FILES=\"%s:%s\"" % (by_words, declared))
    return 0


def check_staged_private_source_sizes():
    """Refuse an oversized staged text blob before it can break the private corpus."""
    try:
        root = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], stderr=subprocess.STDOUT).removesuffix(b"\n")
        def git(*args, data=None):
            return subprocess.check_output(["git", "-C", os.fsdecode(root), *args], input=data, stderr=subprocess.STDOUT)
        changed = set(git("diff", "--cached", "--name-only", "-z", "--diff-filter=ACMRT").split(b"\0"))
        candidates = []
        for entry in git("ls-files", "--stage", "-z").split(b"\0"):
            if not entry:
                continue
            header, path = entry.split(b"\t", 1)
            mode, oid, stage = header.split()
            if path in changed and stage == b"0" and mode in (b"100644", b"100755") \
                    and os.path.splitext(os.fsdecode(path))[1].lower() in TEXT_EXT:
                candidates.append((oid, path))
        if not candidates:
            return 0
        sizes = git("cat-file", "--batch-check=%(objectname) %(objecttype) %(objectsize)",
                    data=b"".join(oid + b"\n" for oid, _ in candidates)).splitlines()
        if len(sizes) != len(candidates):
            raise ValueError("Git did not return every staged object")
        oversized = []
        for (oid, path), row in zip(candidates, sizes):
            actual, kind, size = row.split()
            if actual != oid or kind != b"blob":
                raise ValueError("Git did not return the staged blob")
            if int(size) > SOURCE_MAX_BYTES:
                oversized.append((path, int(size)))
        for path, size in oversized:
            print("Private source commit refused: %r is %d bytes; the existing per-file limit is %d. Compress or split it before committing."
                  % (os.fsdecode(path), size, SOURCE_MAX_BYTES), file=sys.stderr)
        return 1 if oversized else 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("Private source commit check could not complete: %s" % error, file=sys.stderr)
        return 2


def main():
    if sys.argv[1:] == ["--check-staged-private-sizes"]:
        return check_staged_private_source_sizes()
    # `--digest` before anything else: it is a tool for the operator writing the
    # declaration, not a scan, and it must not need a job file to exist.
    if len(sys.argv) >= 3 and sys.argv[1] == '--digest':
        return mint(sys.argv[2])
    try:
        with open(sys.argv[1], encoding='utf-8') as fh:
            job = json.load(fh)
    except Exception as exc:  # noqa: BLE001 — any failure here is fail-closed
        print("BROKEN\tcould not read the scan job: %s" % exc)
        return 2

    min_speech = int(job.get('min_speech_lines', 8))
    min_quote = int(job.get('min_quote_words', 10))
    sources = [s for s in job.get('sources', []) if s]
    items = job.get('items', [])
    private_files = parse_private_files(job.get('private_files', []))

    try:
        items = expand_items(items, int(job.get('items_max_files', 5000)),
                             identity_declared=bool(private_files))
    except BrokenCorpus as exc:
        print("BROKEN\t%s" % exc)
        return 2

    # An item marked identity_only is judged by the declaration alone, so when
    # EVERY item is one the corpus is never walked. That is what makes the two
    # identity-only paths cheap enough to run where they run — a write to a
    # gitignored destination, and a binary blob in a commit — and it is also why
    # neither can fail on an empty corpus it never needed. The condition is
    # DERIVED from the items rather than passed as a flag: a flag is a second
    # thing to keep true.
    need_corpus = any(not it.get('identity_only') for it in items)

    corpus, members = '', []
    if need_corpus:
        max_files = int(job.get('corpus_max_files', 4000))
        max_bytes = int(job.get('corpus_max_bytes', 67108864))
        cache = None
        root = cache_root(sources)
        if root is not None:
            # The kept index holds hash() values: see THE BUILT CORPUS IS KEPT
            # ON DISK for why this process must run under a fixed seed.
            if os.environ.get('PYTHONHASHSEED') != '0':
                os.execve(sys.executable, [sys.executable] + sys.argv,
                          dict(os.environ, PYTHONHASHSEED='0'))
            cache = CorpusCache(root, sources, [min_speech, min_quote, max_files, max_bytes])
        try:
            corpus, members = build_corpus(
                sources, min_speech, max_files, max_bytes, min_quote, cache=cache,
            )
        except BrokenCorpus as exc:
            print("BROKEN\t%s" % exc)
            return 2

    # --- THE VACUITY FLOOR ---------------------------------------------------
    # A scan that read NOTHING must never report CLEAN.
    #
    # Everything below this line is conditional on the corpus. `corpus_index` is
    # an empty set when the corpus is empty, verbatim_run returns None on an
    # empty index, and the run prints CLEAN — a guard reporting that it found no
    # private material when it never had any private material to compare
    # against. That is the same shape as the "no media committed" check this
    # whole mechanism replaced, and as the "18/18 suites" tally that described a
    # glob instead of an inventory: a claim whose scope quietly excluded the
    # thing it was supposed to cover.
    #
    # It has already bitten once, silently. pb_resolve_sources documents it: in
    # a linked worktree `../richos-hq` resolved to a path that does not exist,
    # the corpus detector went inert in exactly the place all the work happens,
    # and the only symptom was one honest line in a message nobody reads on a
    # PASS. That fix made the path resolve. This makes the SILENCE impossible.
    #
    # The condition is derived, not chosen: sources were declared AND they
    # resolved to trees that exist AND not one file in them qualified as private
    # speech. A declared source that is simply not on this machine is skipped
    # upstream and never reaches here, which is deliberate and documented.
    #
    # Deliberately NOT a size threshold. "Unexpectedly small" cannot be derived
    # from anything — any word count would be a magic number that either never
    # fires or fires on a legitimate small private record — and this file's own
    # rule is that a number nobody measured does not ship. Empty is the one
    # threshold that means something on its own.
    #
    # The way through is CORPUS_MAY_BE_EMPTY in the declaration: committed,
    # diffable, reviewed by whoever lands it — the same affordance ALLOWLIST is,
    # for the same reason, and pointedly not an in-the-moment override.
    if need_corpus and sources and not members and not job.get('corpus_may_be_empty'):
        print("BROKEN\tthe declared PRIVATE_SOURCES resolved to %d tree(s) on "
              "this machine, and NOT ONE file in them qualified as private "
              "speech. The derived-from-private detector — the only one that "
              "catches speech quoted inside ordinary prose — therefore had "
              "nothing to compare against and would have reported this content "
              "clean without examining a single private word. Refusing to scan "
              "an empty corpus and report the result as clean. Either point "
              "PRIVATE_SOURCES at the trees that actually hold recordings, or "
              "set CORPUS_MAY_BE_EMPTY=1 in the declaration to state on the "
              "record that this repository has no private corpus yet."
              % len(sources))
        return 2

    if isinstance(corpus, CachedCorpus):
        corpus_index, corpus_words = corpus.index, corpus.words
    else:
        corpus_index = index_corpus(corpus, min_quote) if corpus else set()
        corpus_words = len(corpus.split())

    findings = []
    unread = []
    for item in items:
        label = item.get('label') or item.get('path') or '<unknown>'
        raw = None
        if 'text' in item:
            blob = item['text'] or ''
        else:
            try:
                with open(item['path'], 'rb') as fh:
                    raw = fh.read(IDENTITY_MAX_BYTES + 1)
            except FileNotFoundError:
                # Deleted: there are no bytes left to publish.
                continue
            except Exception as exc:
                # A blob that cannot be read cannot be cleared either, but an
                # unreadable path is far more likely a binary/deleted file than
                # a smuggled transcript, and blocking on it would make the guard
                # fire on ordinary work. It is reported, not blocked -- and this
                # now DOES report: it used to `continue` and say nothing (hunt
                # P5-08). The UNREAD trailer below names each path, and both
                # guards print it beside their CLEAN.
                unread.append((label, getattr(exc, 'strerror', None) or type(exc).__name__))
                continue
            # Read as BYTES and decoded here, rather than opened as text: the
            # identity detector hashes the bytes, and decode_best is what makes
            # a UTF-16 file yield the words a person wrote instead of a run of
            # single letters.
            blob = decode_best(raw[:SOURCE_MAX_BYTES])

        # Detector 0 — declared private BY IDENTITY. First, because it is the
        # only one that is exact: a digest match is not an opinion about what
        # the content resembles.
        idf = identity_findings(label, private_files,
                                raw=raw, text=None if raw is not None else blob)
        if idf:
            findings.extend(idf)
            continue
        if item.get('identity_only'):
            # Judged by the declaration alone, and it said nothing. Handing it
            # to the text detectors is exactly what this flag exists to prevent.
            continue

        # Detector 1 — derived from private. The sharpest signal that needs no
        # declaration naming the file: no heuristic, no threshold on shape, just
        # "these exact words are already in a file we declared private".
        run = verbatim_run(blob, corpus, corpus_index, min_quote,
                           published_windows(item, min_quote))
        if run:
            findings.append((
                'BLOCK', label, 'derived-from-private',
                '%d words reproduced verbatim: "%s"' % (len(run), ' '.join(run)[:160]),
            ))
            continue

        # Detector 2 — the shape of a recording, for material with no declared
        # corpus to match against.
        n = speech_lines(blob, cap=min_speech)
        if n >= min_speech:
            findings.append((
                'BLOCK', label, 'recorded-speech',
                '%d+ timestamped speaker lines / caption cues' % n,
            ))

    # --- THE ORACLE ---------------------------------------------------------
    # What this run actually examined, emitted on every completed analysis so a
    # caller — or a test — can assert that a CLEAN verdict was reached by
    # LOOKING rather than by reading nothing.
    #
    # A regression test for a scanner can pass for the same reason the scanner
    # can fail: because the corpus was empty and there was nothing to find. That
    # is the exact defect this file is about, and a suite with no way to tell the
    # two apart would be the defect reproduced one level up — which has happened
    # here before, in this suite's own tally and its own fixture filenames.
    #
    # Emitted LAST, never first: both shell callers dispatch on `head -1 | cut
    # -f1`, and pb_refusal skips every line that is not a BLOCK, so a trailing
    # line is additive to the contract rather than a change to it.
    # IDENTITY is on its own line for the same reason CORPUS is: an
    # identity-only run reads no corpus at all, so CORPUS 0 0 would be the only
    # thing a caller saw and "this run compared against nothing" and "this run
    # compared against four declared files" would look identical. The count is
    # of entries that PARSED, never of entries written down.
    trailer = "CORPUS\t%d\t%d" % (len(members), corpus_words)
    identity_trailer = "IDENTITY\t%d" % len(private_files)
    # One line per path this run could NOT read, after the verdict like the other
    # trailers (hunt P5-08): the verdict is unchanged, the omission is visible.
    unread_trailers = ["UNREAD\t%s\t%s" % (path, why) for path, why in unread]
    if not findings:
        print("CLEAN")
        print(trailer)
        print(identity_trailer)
        for line in unread_trailers:
            print(line)
        return 0
    for row in findings:
        print('\t'.join(row))
    print(trailer)
    print(identity_trailer)
    for line in unread_trailers:
        print(line)
    return 0


if __name__ == '__main__':
    sys.exit(main())
