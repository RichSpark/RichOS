#!/usr/bin/env python3
"""dialect-fix.py: American spelling, fixed mechanically where that is safe and refused where it is not.

The CEO, 2026-09-27 (richos-hq wiki/ceo-decisions.md section 93): "The goal is to have a guard
that is as fast as possible and eliminates the need for slow inference as much as possible i.e.
automates as much as possible on this matter." 99% is his bar, not 100%.

ONE module, two readers:
  * scripts/hooks/dialect-guard.sh, the engine's PreToolUse hook on Write|Edit|MultiEdit|
    NotebookEdit, runs `python3 dialect-fix.py hook` with the payload on stdin;
  * richos/app/scripts/lint/dialect.py imports it (by file path) to count what a file
    already carries.
The data is the generated table beside this file, dialect-en-US.generated.dict. This file
carries no vocabulary word except one declared vendor file name, so it needs no exemption
from the hook it implements.

WHAT HAPPENS TO A MATCH. Every letter run that is a key of the table, bounded by non-word
characters, is a match. Each match is SKIPPED (today's exemptions, nothing said), FIXED (the
American form goes into the write through updatedInput, and the change is reported), or
REFUSED (exit 2, as before). A match is never passed silently where the old guard refused it,
except through a skip the old guard also had: fixing replaces the refusal, it never drops it.

  SKIPPED  path exemptions (captured evidence, vendor legal text, lockfiles, run output,
           the dialect files themselves, DIALECT_EXEMPT_PATHS); a line inside a fenced
           block, a `>` line, a line declaring `dialect-exempt: <reason>`, a line holding
           a DIALECT_SCAN_ALLOWLIST substring; an inline code span; a token that is a
           path, URL, identifier or dotted name; a per-word ext-exempt (a CSS named color
           in a stylesheet).
  FIXED    by file class:
             prose (md, markdown, mdx, txt, rst): every match outside a double-quoted run;
             code (rs, swift, kt, js, ts, py, sh and kin): matches in comments, and in
             string literals that contain whitespace (sentences, which are UI copy);
           in every fixable context the word must be lowercase, all capitals, or
           capitalized at the start of a sentence (a capital mid-sentence is a name).
  REFUSED  everything else: markup, styles, config and any other file class (a fix there
           can change behavior: a class name, a wire value, a key); in code, the code
           itself and single-token string literals; a double-quoted run (a quotation is
           somebody's words); a capital mid-sentence; a hyphenated token, markup or an
           interpolation inside a code string; and the `CEO queue` collocation, which is
           a rename, not a spelling.

EDIT, MULTIEDIT AND NOTEBOOKEDIT carry a fragment, not a file. A fragment in the middle of a
fenced block or a code string reads as prose on its own, so the target file is read, the
fragment is placed where `old_string` is, and the scan runs on the file as it would be. That
costs one file read, only when the fragment has a match at all. When `old_string` is not in
the file, nothing is done: the edit fails on its own.
"""
import bisect
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
TABLE_PATH = os.path.join(HERE, "dialect-en-US.generated.dict")

# --- path classes -----------------------------------------------------------
PROSE_EXTS = frozenset(("md", "markdown", "mdx", "txt", "rst"))
CODE_LANGS = {
    "rs": "rust", "swift": "swift", "kt": "kotlin", "kts": "kotlin",
    "js": "js", "mjs": "js", "cjs": "js", "jsx": "js",
    "ts": "js", "tsx": "js", "mts": "js", "cts": "js",
    "py": "python", "sh": "shell", "bash": "shell", "zsh": "shell",
}

VENDOR_LEGAL = (
    "license",
    "licence",  # dialect-exempt: a vendor's own legal file name, matched and never written
    "notice", "copying", "copyright",
    "third-party-notices", "third_party_notices", "authors", "patents")
GENERATED = ("package-lock.json", "yarn.lock", "pnpm-lock.yaml", "cargo.lock",
             "go.sum", "composer.lock", "gemfile.lock")
EVIDENCE_SEGMENTS = ("raw", "cold-open", "transcripts", "transcript",
                     "fixtures", "fixture", "corpora", "corpus", "snapshots",
                     "snapshot", "logs", "node_modules", "vendor",
                     "third_party", "third-party", "__pycache__", ".git")
EVIDENCE_EXTS = (".log", ".jsonl", ".lock", ".min.js", ".min.css", ".patch",
                 ".diff", ".po", ".pot", ".map", ".sha256")
# The dialect files that are made of the words they govern. Self-serving on its face, so it
# is a list of exact basenames, never a pattern: a dictionary of British spellings is made of
# British spellings, a suite that proves the hook needs British inputs, and a mutation script
# names the words it mutates. Everything else here (this module, the generator, the READMEs)
# carries no vocabulary word, and dialect-guard.test.sh proves it for this module.
OWN_FILES = ("dialect-en-us.dict", "dialect-guard.sh", "dialect-guard.test.sh",
             "dialect-guard.mutation.sh", "dialect-en-us.generated.dict",
             "dialect-en-us.overrides.dict", "dialect-en-us.leave.tsv")


def path_exempt(file_path, segment_path=None, extra_exempt=()):
    """The reason a whole file is never checked, or "". `segment_path` is the path below the
    governing repository when that is known (evidence is a place INSIDE a repository, never
    a directory the repository happens to live in); the whole path otherwise."""
    if not file_path:
        return ""
    low_path = file_path.lower().replace("\\", "/")
    low_base = os.path.basename(low_path)
    segment_path = (segment_path if segment_path is not None else low_path).lower().replace("\\", "/")
    if low_base in OWN_FILES:
        return "a dialect file made of the words it governs"
    if low_base.split(".")[0] in VENDOR_LEGAL:
        return "vendor legal text (%s), not ours to edit" % os.path.basename(file_path)
    if low_base in GENERATED:
        return "generated lockfile"
    for ext in EVIDENCE_EXTS:
        if low_base.endswith(ext):
            return "generated or captured file type (%s)" % ext
    for seg in segment_path.split("/"):
        if seg in EVIDENCE_SEGMENTS:
            return "captured evidence, path segment '%s/'" % seg
        # A QUALIFIED evidence directory (`license-snapshots/`, `call-transcripts/`). Only the
        # last hyphen component counts, so `raw-notes/` is still prose.
        if "-" in seg and seg.rsplit("-", 1)[1] in EVIDENCE_SEGMENTS:
            return "captured evidence, path segment '%s/'" % seg
    for p in extra_exempt:
        if p and p.lower() in low_path:
            return "DIALECT_EXEMPT_PATHS entry '%s'" % p
    return ""


def extension(file_path):
    base = os.path.basename((file_path or "").lower())
    return base.rsplit(".", 1)[1] if "." in base else ""


def file_class(file_path):
    """("prose", None), ("code", <language>) or ("strict", None)."""
    ext = extension(file_path)
    if ext in PROSE_EXTS:
        return "prose", None
    if ext in CODE_LANGS:
        return "code", CODE_LANGS[ext]
    return "strict", None


# --- the table --------------------------------------------------------------
def load_table(path=TABLE_PATH):
    """lowercase form -> (American form, frozenset of exempt extensions). {} when unreadable."""
    table = {}
    try:
        with open(path, encoding="utf-8") as fh:
            for raw in fh:
                line = raw.rstrip("\n")
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) < 2 or not parts[0].strip() or not parts[1].strip():
                    continue
                exts = set()
                for opt in (parts[2].split(",") if len(parts) > 2 else ()):
                    opt = opt.strip()
                    if opt.startswith("ext-exempt="):
                        opt = opt[len("ext-exempt="):]
                    if opt and "=" not in opt:
                        exts.add(opt.lower())
                table[parts[0].strip().lower()] = (parts[1].strip(), frozenset(exts))
    except OSError:
        return {}
    return table


# --- patterns ---------------------------------------------------------------
LETTER_RUN = re.compile(r"[A-Za-z]+")
CEO_QUEUE_RE = re.compile(r"\bCEO(?:'s|s)?\s+queues?\b", re.IGNORECASE)
SOFT_QUEUE_RE = re.compile(r"\b(?:his|her|my|your|the)\s+queues?\b", re.IGNORECASE)
SOFT_CONTEXT_RE = re.compile(r"\b(CEO|TODO|TODOs)\b")
RENAME_CUE_RE = re.compile(
    r"(called|renamed|rename|formerly|former|pre-rename|legacy|used to be|old name|was the|previously|instead of|no longer)",
    re.IGNORECASE)
TOKEN_CHARS = frozenset("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_$./:\\-")
FENCE_RE = re.compile(r"^\s*(```|~~~)")
# The reason must START WITH A WORD CHARACTER: `<!-- dialect-exempt: -->` is a bare marker
# wearing a reason, and a bare marker exempts nothing.
DECLARED_EXEMPT_RE = re.compile(r"dialect-exempt:\s*[A-Za-z0-9]")
MARKUP_RE = re.compile(r"<[A-Za-z/!]")
CHAR_LITERAL_RE = re.compile(r"'(?:\\.|[^\\'\n])'")


def is_word_char(c):
    return c.isalnum() or c == "_"


def shape(run):
    if run.islower():
        return "lower"
    if run.isupper():
        return "upper" if len(run) > 1 else "capitalized"
    if run[0].isupper() and run[1:].islower():
        return "capitalized"
    return "mixed"


def recase(target, how):
    if how == "upper":
        return target.upper()
    if how == "capitalized":
        return target[:1].upper() + target[1:]
    return target


def enclosing_token(text, start, end):
    i = start
    while i > 0 and text[i - 1] in TOKEN_CHARS:
        i -= 1
    j = end
    while j < len(text) and text[j] in TOKEN_CHARS:
        j += 1
    return text[i:j]


def looks_like_identifier_or_path(tok):
    if "/" in tok or "\\" in tok:
        return True
    if "_" in tok or "$" in tok or "::" in tok or "--" in tok:
        return True
    return bool(re.search(r"\.[A-Za-z_]", tok))


# --- the code lexer ---------------------------------------------------------
# Comments and string literals, per language. Strings are single-line except the forms a
# language uses for multi-line text (triple quotes, JS templates); a string not closed on its
# own line is read as code, which can only refuse, never fix.
LANG_SPECS = {
    "rust":   dict(line="//", block=True, multi=(), single=('"',), chars=True),
    "swift":  dict(line="//", block=True, multi=('"""',), single=('"',), chars=False),
    "kotlin": dict(line="//", block=True, multi=('"""',), single=('"',), chars=True),
    "js":     dict(line="//", block=True, multi=("`",), single=('"', "'"), chars=False),
    "python": dict(line="#", block=False, multi=('"""', "'''"), single=('"', "'"), chars=False),
    "shell":  dict(line="#", block=False, multi=(), single=('"', "'"), chars=False),
}
for _spec in LANG_SPECS.values():
    _openers = [_spec["line"]] + (["/*"] if _spec["block"] else []) + list(_spec["multi"]) + list(_spec["single"])
    if _spec["chars"]:
        _openers.append("'")
    _spec["finder"] = re.compile("|".join(re.escape(o) for o in sorted(set(_openers), key=len, reverse=True)))


def _close(text, at, delim, escapes, multiline):
    """Index of the closing delimiter at or after `at`, or None."""
    n = len(text)
    i = at
    while i < n:
        c = text[i]
        if escapes and c == "\\":
            i += 2
            continue
        if c == "\n" and not multiline:
            return None
        if text.startswith(delim, i):
            return i
        i += 1
    return None


def lex(text, lang):
    """Sorted, non-overlapping regions (kind, start, end): kind "comment" or "string", start
    and end bounding the CONTENT (delimiters excluded). Everything outside them is code."""
    spec = LANG_SPECS[lang]
    finder = spec["finder"]
    regions = []
    i, n = 0, len(text)
    while i < n:
        m = finder.search(text, i)
        if not m:
            break
        tok, s = m.group(0), m.start()
        if tok == spec["line"]:
            if lang == "shell" and s > 0 and text[s - 1] not in " \t\n;|&(":
                i = s + 1
                continue
            e = text.find("\n", s)
            e = n if e < 0 else e
            regions.append(("comment", s + len(tok), e))
            i = e
        elif tok == "/*":
            e = text.find("*/", s + 2)
            regions.append(("comment", s + 2, n if e < 0 else e))
            i = n if e < 0 else e + 2
        elif tok in spec["multi"]:
            e = _close(text, s + len(tok), tok, True, True)
            regions.append(("string", s + len(tok), n if e is None else e))
            i = n if e is None else e + len(tok)
        elif tok in spec["single"]:
            escapes = not (lang == "shell" and tok == "'")
            e = _close(text, s + 1, tok, escapes, False)
            if e is None:
                i = s + 1
                continue
            regions.append(("string", s + 1, e))
            i = e + 1
        else:  # a character literal or a lifetime
            cm = CHAR_LITERAL_RE.match(text, s)
            i = cm.end() if cm else s + 1
    return regions


INTERPOLATIONS = (("${", "}"), ("$(", ")"), ("\\(", ")"), ("%(", ")"), ("{", "}"))


def _masked(content, rel):
    """True when offset `rel` of a string's content sits inside an interpolation or a format
    field: {...}, ${...}, $(...), \\(...), %(...). A word there is code."""
    for opener, closer in INTERPOLATIONS:
        start = 0
        while True:
            a = content.find(opener, start)
            if a < 0 or a >= rel:
                break
            b = content.find(closer, a + len(opener))
            if b < 0 or b >= rel:
                return True
            start = b + 1
    return False


# --- context ----------------------------------------------------------------
class Text:
    """A text and the line-level facts every match consults, computed once."""

    def __init__(self, text, allowlist=()):
        self.text = text
        self.lines = text.split("\n")
        self.starts = []
        pos = 0
        for line in self.lines:
            self.starts.append(pos)
            pos += len(line) + 1
        self.skip = []   # line-level skip: fence, blockquote, declared exemption, allowlist
        self.para = []   # where the paragraph each line belongs to starts
        in_fence = False
        para_start = 0
        for idx, line in enumerate(self.lines):
            if FENCE_RE.match(line):
                in_fence = not in_fence
                self.skip.append(True)
            elif in_fence:
                self.skip.append(True)
            else:
                self.skip.append(line.lstrip().startswith(">")
                                 or bool(DECLARED_EXEMPT_RE.search(line))
                                 or any(a in line for a in allowlist))
            if not line.strip():
                para_start = self.starts[idx] + len(line) + 1
            self.para.append(para_start)

    def line_of(self, pos):
        return bisect.bisect_right(self.starts, pos) - 1


def _in_code_span(text, lo, p):
    return text.count("`", lo, p) % 2 == 1


def _in_quote(text, lo, p):
    """Inside a double-quoted run that opened at or after `lo`. A quote mark inside a code
    span does not count. An unbalanced mark costs fixes up to the paragraph's end, never a
    wrong fix."""
    inside = False
    ticks = 0
    for i in range(lo, p):
        c = text[i]
        if c == "\n":
            ticks = 0
        elif c == "`":
            ticks += 1
        elif ticks % 2:
            continue
        elif c == '"':
            inside = not inside
        elif c == "“":
            inside = True
        elif c == "”":
            inside = False
    return inside


def _sentence_start(text, p, lo):
    """Is the letter run at `p` the first word of a sentence, reading back no further than `lo`?"""
    i = p - 1
    while i >= lo and text[i] in TOKEN_CHARS:
        if text[i].isalnum():
            return False
        i -= 1
    while i >= lo:
        c = text[i]
        if c == "\n" or c in "!?|":
            return True
        if c in TOKEN_CHARS:
            j = i
            while j >= lo and text[j] in TOKEN_CHARS:
                j -= 1
            tok = text[j + 1:i + 1]
            if any(ch.isalnum() for ch in tok) or tok[-1] in ".:":
                return tok[-1] in ".:"
            i = j
            continue
        i -= 1
    return True


class Finding:
    __slots__ = ("start", "end", "written", "target", "action", "why", "line")

    def __init__(self, start, end, written, target, action, why, line):
        self.start, self.end, self.written, self.target = start, end, written, target
        self.action, self.why, self.line = action, why, line

    @property
    def label(self):
        return "%s -> %s" % (self.written, self.target)


def _prose_decision(text, s, e, run, lo, quote_lo):
    """FIX, or REFUSE with a reason, for a match in a fixable context starting at `lo`."""
    if _in_quote(text, quote_lo, s):
        return "refuse", "inside a quotation: somebody's words are not ours to change"
    how = shape(run)
    if how == "mixed":
        return "refuse", "mixed capitals: probably a name"
    if how == "capitalized":
        if not _sentence_start(text, s, lo):
            return "refuse", "capitalized mid-sentence: probably a name"
        if text[e:e + 2] in ("'s", "’s"):
            return "refuse", "a capitalized possessive: probably a name"
    return "fix", ""


def scan(text, kind, lang, ext, table, allowlist=(), region=None):
    """Every finding in `text`, a file of class `kind` (and `lang` for code). `region`
    (lo, hi) limits the findings to matches that start in it; the whole text still decides
    the context. Returns (findings, soft) where soft is a list of (label, site) reports."""
    lo_r, hi_r = region if region else (0, len(text))
    window = text[lo_r:hi_r]
    candidates = [(m.start() + lo_r, m.end() + lo_r) for m in LETTER_RUN.finditer(window)
                  if m.group(0).lower() in table]
    queue_hits = [(m.start() + lo_r, m.end() + lo_r, m.group(0)) for m in CEO_QUEUE_RE.finditer(window)]
    soft_hits = [(m.start() + lo_r, m.end() + lo_r, m.group(0)) for m in SOFT_QUEUE_RE.finditer(window)]
    if not candidates and not queue_hits and not soft_hits:
        return [], []

    t = Text(text, allowlist)
    regions = lex(text, lang) if kind == "code" else []
    region_starts = [r[1] for r in regions]
    findings, soft = [], []

    for s, e in candidates:
        if s > 0 and is_word_char(text[s - 1]):
            continue
        if e < len(text) and is_word_char(text[e]):
            continue
        idx = t.line_of(s)
        if t.skip[idx]:
            continue
        ls = t.starts[idx]
        run = text[s:e]
        target, exempt_exts = table[run.lower()]
        if ext and ext in exempt_exts:
            continue
        tok = enclosing_token(text, s, e)
        if tok != run and looks_like_identifier_or_path(tok):
            continue
        how = shape(run)
        written_target = recase(target, how if how != "mixed" else "lower")
        if kind == "prose":
            if _in_code_span(text, ls, s):
                continue
            action, why = _prose_decision(text, s, e, run, ls, t.para[idx])
        elif kind == "code":
            k = bisect.bisect_right(region_starts, s) - 1
            reg = regions[k] if k >= 0 and regions[k][1] <= s < regions[k][2] else None
            if reg is None:
                action, why = "refuse", "code: renaming an identifier changes behavior"
            else:
                rkind, rs, rend = reg
                ctx_lo = max(rs, ls)
                if _in_code_span(text, ctx_lo, s):
                    continue
                content = text[rs:rend]
                if rkind == "comment":
                    action, why = _prose_decision(text, s, e, run, ctx_lo, rs)
                elif not any(ch.isspace() for ch in content):
                    action, why = "refuse", "a single-token string literal: it may be a wire value or a key"
                elif MARKUP_RE.search(content):
                    action, why = "refuse", "markup inside a string: a fix could change a class or an attribute"
                elif _masked(content, s - rs):
                    action, why = "refuse", "an interpolation or a format field: that is code"
                elif "-" in tok:
                    action, why = "refuse", "a hyphenated token inside a string: it may be a class name or a key"
                else:
                    action, why = _prose_decision(text, s, e, run, ctx_lo, rs)
        else:
            action, why = "refuse", "markup, styles or config: a fix could change behavior"
        findings.append(Finding(s, e, run, written_target, action, why, idx + 1))

    for s, e, matched in queue_hits:
        idx = t.line_of(s)
        ls, line = t.starts[idx], t.lines[idx]
        if t.skip[idx] or _in_code_span(text, ls, s):
            continue
        col = s - ls
        if RENAME_CUE_RE.search(line[max(0, col - 60):col]):
            continue
        findings.append(Finding(s, e, matched, "CEO-TODOs", "refuse",
                                "the CEO's list is CEO-TODOs (a rename, not a spelling)", idx + 1))

    for s, e, matched in soft_hits:
        idx = t.line_of(s)
        ls, line = t.starts[idx], t.lines[idx]
        if t.skip[idx] or _in_code_span(text, ls, s):
            continue
        if not SOFT_CONTEXT_RE.search(line):
            continue
        col = s - ls
        if RENAME_CUE_RE.search(line[max(0, col - 60):col]):
            continue
        soft.append((matched.strip(), line.strip()[:110]))

    findings.sort(key=lambda f: f.start)
    return findings, soft


def apply_fixes(text, findings, base=0):
    """`text` (which starts at offset `base` of the scanned text) with every FIX applied."""
    out, last = [], 0
    for f in findings:
        if f.action != "fix":
            continue
        out.append(text[last:f.start - base])
        out.append(f.target)
        last = f.end - base
    out.append(text[last:])
    return "".join(out)


def site(text, f):
    ls = text.rfind("\n", 0, f.start) + 1
    le = text.find("\n", f.start)
    return text[ls:le if le >= 0 else len(text)].strip()[:110]


# --- the hook ---------------------------------------------------------------
class Outcome:
    def __init__(self):
        self.fixes = []      # (written, target, line)
        self.refusals = []   # (label, why, site)
        self.soft = []       # (label, site)
        self.updated = None  # the whole tool_input, fixed
        self.notfound = False

    def nothing_found(self):
        self.notfound = True
        self.fixes, self.refusals, self.soft, self.updated = [], [], [], None
        return self


def _read(path):
    try:
        with open(path, encoding="utf-8") as fh:
            return fh.read()
    except (OSError, UnicodeDecodeError, ValueError):
        return None


def _occurrences(content, old, every):
    found, start = [], 0
    while True:
        k = content.find(old, start)
        if k < 0:
            return found
        found.append(k)
        if not every:
            return found
        start = k + len(old)


def _fragment(out, content, old, new, every, kind, lang, ext, table, allowlist):
    """Decide `new` placed at every place `old` is in `content`. Returns (fixed new, the
    content after the edit), or None when `old` is not there."""
    content = content or ""
    if old == "":
        if content:
            return None
        places, replace_len = [0], 0
    else:
        places, replace_len = _occurrences(content, old, every), len(old)
        if not places:
            return None
    keyed, first_soft = {}, None
    for k in places:
        # The fragment inside the file as the file will be, so its context is the file's.
        virtual, lo = content[:k] + new + content[k + replace_len:], k
        found, soft = scan(virtual, kind, lang, ext, table, allowlist, region=(lo, lo + len(new)))
        if first_soft is None:
            first_soft = soft
        for f in found:
            keyed.setdefault((f.start - lo, f.end - lo), []).append((f, virtual))
    # A word is fixed only where it is fixed at EVERY place the edit lands; anything else
    # cannot be one string, so it is refused.
    decided = []
    for (rs, rend), items in sorted(keyed.items()):
        f0, v0 = items[0]
        actions = {f.action for f, _ in items}
        if actions == {"fix"} and len(items) == len(places):
            decided.append(Finding(rs, rend, f0.written, f0.target, "fix", "", f0.line))
        else:
            why = f0.why if actions == {"refuse"} else "the same text lands where it needs different treatment"
            out.refusals.append((f0.label, why, site(v0, f0)))
    out.soft.extend(first_soft or [])
    fixed_new = apply_fixes(new, decided)
    out.fixes.extend((f.written, f.target, f.line) for f in decided)
    after = content
    for k in reversed(places):
        after = after[:k] + fixed_new + after[k + replace_len:]
    return fixed_new, after


def _word_map_apply(text, changes):
    for written, target in changes:
        text = re.sub(r"(?<![A-Za-z0-9_])%s(?![A-Za-z0-9_])" % re.escape(written), target, text)
    return text


def _notebook_cell_kind(path, cell_id):
    data = _read(path)
    try:
        nb = json.loads(data) if data else None
    except ValueError:
        return None
    for cell in (nb.get("cells", []) if isinstance(nb, dict) else []):
        if isinstance(cell, dict) and cell.get("id") == cell_id:
            return cell.get("cell_type")
    return None


def _whole(out, text, kind, lang, ext, table, allowlist):
    found, soft = scan(text, kind, lang, ext, table, allowlist)
    out.soft.extend(soft)
    for f in found:
        if f.action == "fix":
            out.fixes.append((f.written, f.target, f.line))
        else:
            out.refusals.append((f.label, f.why, site(text, f)))
    return apply_fixes(text, found)


def has_match(table, *texts):
    """The cheap first look: does any text carry a table key or a queue collocation at all?"""
    for x in texts:
        if not isinstance(x, str):
            continue
        if CEO_QUEUE_RE.search(x) or SOFT_QUEUE_RE.search(x):
            return True
        if any(m.group(0).lower() in table for m in LETTER_RUN.finditer(x)):
            return True
    return False


def decide(tool_name, ti, table, allowlist=()):
    """The outcome of one tool call. Never raises on a well-formed payload."""
    out = Outcome()
    file_path = ti.get("file_path") or ti.get("notebook_path") or ""
    if not isinstance(file_path, str):
        file_path = ""
    kind, lang = file_class(file_path)
    ext = extension(file_path)

    if tool_name == "Write":
        content = ti.get("content")
        if not has_match(table, content):
            return out
        fixed = _whole(out, content, kind, lang, ext, table, allowlist)
        if out.fixes:
            out.updated = dict(ti, content=fixed)
        return out

    if tool_name == "Edit":
        old, new = ti.get("old_string"), ti.get("new_string")
        if not isinstance(old, str) or not has_match(table, new):
            return out
        content = _read(file_path) if file_path else None
        if content is None and old != "":
            return out.nothing_found()
        got = _fragment(out, content, old, new, bool(ti.get("replace_all")), kind, lang, ext, table, allowlist)
        if got is None:
            return out.nothing_found()
        if out.fixes:
            out.updated = dict(ti, new_string=got[0])
        return out

    if tool_name == "MultiEdit":
        edits = ti.get("edits")
        if not isinstance(edits, list) or not has_match(table, *[e.get("new_string") for e in edits if isinstance(e, dict)]):
            return out
        content = _read(file_path) if file_path else None
        first_old = edits[0].get("old_string") if edits and isinstance(edits[0], dict) else None
        if content is None and first_old != "":
            return out.nothing_found()
        new_edits, made = [], []
        for ed in edits:
            if not isinstance(ed, dict) or not isinstance(ed.get("old_string"), str) \
                    or not isinstance(ed.get("new_string"), str):
                return out.nothing_found()
            old, new, every = ed["old_string"], ed["new_string"], bool(ed.get("replace_all"))
            before = len(out.fixes)
            got = _fragment(out, content, old, new, every, kind, lang, ext, table, allowlist)
            if got is None and made:
                # This edit quotes an earlier edit's text, which this hook already changed.
                old = _word_map_apply(old, made)
                got = _fragment(out, content, old, new, every, kind, lang, ext, table, allowlist)
            if got is None:
                return out.nothing_found()
            for written, target, _ in out.fixes[before:]:
                if (written, target) not in made:
                    made.append((written, target))
            new_edits.append(dict(ed, old_string=old, new_string=got[0]))
            content = got[1]
        if out.fixes:
            out.updated = dict(ti, edits=new_edits)
        return out

    if tool_name == "NotebookEdit":
        source = ti.get("new_source")
        if not has_match(table, source):
            return out
        cell_type = ti.get("cell_type")
        if cell_type not in ("code", "markdown"):
            cell_type = _notebook_cell_kind(file_path, ti.get("cell_id"))
        nkind, nlang = {"markdown": ("prose", None), "code": ("code", "python")}.get(cell_type, ("strict", None))
        fixed = _whole(out, source, nkind, nlang, "", table, allowlist)
        if out.fixes:
            out.updated = dict(ti, new_source=fixed)
        return out
    return out


def change_report(file_path, fixes):
    """What was changed, for the model: `colour -> color x3 (lines 4, 9, 12)` style."""
    grouped = {}
    for written, target, line in fixes:
        grouped.setdefault((written, target), []).append(line)
    parts = []
    for (written, target), lines in grouped.items():
        count = " ×%d" % len(lines) if len(lines) > 1 else ""
        uniq = sorted(set(lines))
        where = "line%s %s" % ("s" if len(uniq) > 1 else "", ", ".join(str(x) for x in uniq))
        parts.append("%s → %s%s (%s)" % (written, target, count, where))
    return ("dialect-guard.sh changed this %s to American spelling before it ran: %s. "
            "The file on disk has the American forms, so quote them in any later old_string."
            % ("write to " + file_path if file_path else "write", "; ".join(parts)))


def soft_report(file_path, soft):
    items = "; ".join("'%s' in: %s" % (label, where) for label, where in soft)
    return ("dialect-guard.sh note (not blocked): %s uses 'queue' in a way that MIGHT mean the "
            "CEO's list, which is CEO-TODOs (CEO ruling 2026-08-29). Reported, not blocked, because "
            "one line cannot tell it from a sentence about a data structure: %s"
            % (file_path or "this write", items))


def refusal_text(file_path, target, out):
    target = target or "en-US"
    lines = ["=== Dialect check BLOCKED ===",
             "  Refusing to write '%s': this content introduces wording that is not %s and"
             % (file_path or "<unknown file>", target),
             "  cannot be fixed mechanically where it stands (CEO ruling 2026-08-29: American",
             "  English is the language of every string a customer reads, and of the record):"]
    for label, why, where in out.refusals:
        lines.append("    - %s" % label)
        lines.append("        %s" % where)
        if why:
            lines.append("        (%s)" % why)
    for label, where in out.soft:
        lines.append("    ? %s (reported, not the reason for this block)" % label)
        lines.append("        %s" % where)
    if out.fixes:
        lines.append("  %d other spelling%s here will be fixed automatically when you retry; only"
                     % (len(out.fixes), "" if len(out.fixes) == 1 else "s"))
        lines.append("  the ones above need you.")
    lines += [
        "  FIX THE WORD. If the text is genuinely exempt (quoted external material, a protocol",
        "  value, a name you do not own) declare it where a reviewer will see it:",
        "    * put it in a code span or fenced block, or a > blockquote if it is a quotation;",
        "    * or add 'dialect-exempt: <reason>' to the line (a bare marker does nothing);",
        "    * or add the literal substring to DIALECT_SCAN_ALLOWLIST in orchestration.config.",
        "  IF THIS FILE IS SOMEBODY ELSE'S WORK, that is not an exemption to declare: it is a",
        "  fact to record. Add an entry with origin=third-party to the repository's",
        "  .richos/vendored-material registry and this guard leaves the whole path alone,",
        "  including a verbatim re-vendoring from upstream.",
        "  To turn this off for a repository whose product does not speak %s," % target,
        "  blank DIALECT_TARGET in orchestration.config; never weaken the table.",
        "(hook: scripts/hooks/dialect-guard.sh)"]
    return "\n".join(lines)


def hook_main(stdin, env):
    """The protocol dialect-guard.sh reads: line 1 is the verdict, the rest its body.
      CLEAN, NOTFOUND, PARSEFAIL  nothing to do (exit 0, silent)
      NODICT                      the table yielded no rule (fail closed)
      FIX + JSON                  the stdout envelope: updatedInput and the change report
      REPORTONLY + JSON           the stdout envelope with the soft note only
      REFUSE + text               the stderr refusal, exit 2
    """
    try:
        payload = json.loads(stdin)
    except ValueError:
        return "PARSEFAIL\n"
    if not isinstance(payload, dict):
        return "PARSEFAIL\n"
    ti = payload.get("tool_input")
    ti = ti if isinstance(ti, dict) else {}
    file_path = ti.get("file_path") or ti.get("notebook_path") or ""
    file_path = file_path if isinstance(file_path, str) else ""
    gov = env.get("DIALECT_GOV_ROOT", "").rstrip("/")
    phys = env.get("DIALECT_FILE_PHYS", "")
    segment = phys[len(gov) + 1:] if gov and phys.startswith(gov + "/") else None
    extra = [p for p in env.get("DIALECT_EXEMPT_PATHS", "").split() if p]
    if path_exempt(file_path, segment, extra):
        return "CLEAN\n"
    table = load_table(env.get("DICT") or TABLE_PATH)
    if not table:
        return "NODICT\n"
    allowlist = [a for a in env.get("DIALECT_SCAN_ALLOWLIST", "").split() if a]
    out = decide(payload.get("tool_name", ""), ti, table, allowlist)
    if out.notfound:
        return "NOTFOUND\n"
    if out.refusals:
        return "REFUSE\n" + refusal_text(file_path, env.get("DIALECT_TARGET", ""), out) + "\n"
    if out.fixes:
        context = change_report(file_path, out.fixes)
        if out.soft:
            context += " " + soft_report(file_path, out.soft)
        return "FIX\n" + json.dumps({"hookSpecificOutput": {
            "hookEventName": "PreToolUse", "updatedInput": out.updated,
            "additionalContext": context}}) + "\n"
    if out.soft:
        return "REPORTONLY\n" + json.dumps({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": soft_report(file_path, out.soft)}}) + "\n"
    return "CLEAN\n"


def findings_for_file(file_path, text, table, allowlist=(), segment=None, extra_exempt=()):
    """What the lint counts: every match a Write of `text` to `file_path` would fix or refuse,
    as `<written> -> <American>` labels. Ownership (third-party material) is the caller's."""
    if path_exempt(file_path, segment, extra_exempt):
        return []
    kind, lang = file_class(file_path)
    found, _ = scan(text, kind, lang, extension(file_path), table, allowlist)
    return [f.label for f in found]


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "hook":
        sys.stdout.write(hook_main(sys.stdin.read(), os.environ))
        sys.exit(0)
    sys.stderr.write("usage: dialect-fix.py hook < payload.json\n")
    sys.exit(64)
