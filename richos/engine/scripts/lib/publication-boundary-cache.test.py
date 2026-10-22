#!/usr/bin/env python3
"""The kept corpus (THE BUILT CORPUS IS KEPT ON DISK, publication-boundary.py)
must give the verdict a cold build gives, and must notice every change to the
private tree: an edited file, a removed one, one made unreadable, a new one.

Every scan here runs three ways (no cache, cold cache, warm cache) and the three
outputs must be identical. Invented content only.
"""
import importlib.util
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCANNER = os.path.join(HERE, "publication-boundary.py")
ROOT = os.geteuid() == 0 if hasattr(os, "geteuid") else False

SPEECH_A = [
    "we should move the blue boxes before the rain arrives tonight",
    "the second truck can take the chairs and the long tables",
    "nobody remembered where the spare keys for the shed were kept",
    "my sister thinks the garden party should start a little later",
    "if the bakery opens early we can collect the bread ourselves",
    "the old radio in the kitchen still plays the morning news",
    "please write the names of every guest on the paper list",
    "we ran out of candles last year so buy three more boxes",
    "the neighbors offered their ladder for hanging the paper lanterns",
    "someone has to call the band about the time they arrive",
]
SPEECH_B = [
    "the library closes at noon on the first friday of every month",
    "a green bicycle was left leaning against the post office wall",
    "our cousin is bringing two jars of honey from the hill farm",
    "the ferry schedule changed again and nobody told the passengers",
    "every lamp on the pier was painted yellow during the summer",
    "the choir practices in the old school hall on wednesday nights",
    "a parcel of wool socks arrived for the hikers this morning",
    "the river was high enough to cover the lower stepping stones",
]


def transcript(lines, speaker="Alice"):
    return "".join("[00:%02d:%02d] %s: %s\n" % (i // 60, i % 60, speaker, s) for i, s in enumerate(lines))


class CorpusCache(unittest.TestCase):
    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="pb-cache."))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.private = os.path.join(self.tmp, "private")
        os.makedirs(self.private)
        self.cache = os.path.join(self.tmp, "cache")
        self.write(self.private, "call-a.txt", transcript(SPEECH_A))
        self.write(self.private, "notes.md", "a short private note about nothing in particular\n")

    def write(self, d, name, text):
        p = os.path.join(d, name)
        with open(p, "w") as fh:
            fh.write(text)
        return p

    def run_scan(self, text, cache):
        job = {"min_speech_lines": 8, "min_quote_words": 10, "corpus_max_files": 4000,
               "corpus_max_bytes": 67108864, "corpus_may_be_empty": True,
               "sources": [self.private], "private_files": [],
               "items": [{"label": "docs/published.md", "text": text}]}
        jp = os.path.join(self.tmp, "job.json")
        with open(jp, "w") as fh:
            json.dump(job, fh)
        env = {k: v for k, v in os.environ.items() if k not in ("RICHOS_PUBLICATION_CACHE_DIR", "PYTHONHASHSEED")}
        if cache:
            env["RICHOS_PUBLICATION_CACHE_DIR"] = self.cache
        res = subprocess.run([sys.executable, SCANNER, jp], capture_output=True, text=True, env=env)
        return res.stdout.splitlines()

    def scan(self, text):
        """No cache, then the cache as it stands, then the cache again: one answer."""
        plain = self.run_scan(text, cache=False)
        first = self.run_scan(text, cache=True)
        second = self.run_scan(text, cache=True)
        self.assertEqual(first, plain)
        self.assertEqual(second, plain)
        return plain

    def verdict(self, out):
        return out[0].split("\t")[0]

    def test_1_a_quotation_blocks_and_a_harmless_text_is_clean_cold_and_warm(self):
        self.assertEqual(self.verdict(self.scan("He said " + SPEECH_A[3] + " and left.")), "BLOCK")
        out = self.scan("Nothing here comes from the private tree at all.")
        self.assertEqual(out[0], "CLEAN")
        self.assertEqual(out[1].split("\t")[:2], ["CORPUS", "1"])

    def test_2_an_edited_private_file_is_read_again(self):
        self.assertEqual(self.verdict(self.scan(SPEECH_A[0])), "BLOCK")
        p = os.path.join(self.private, "call-a.txt")
        st = os.stat(p)
        self.write(self.private, "call-a.txt", transcript(SPEECH_B))
        os.utime(p, ns=(st.st_atime_ns, st.st_mtime_ns))       # same mtime: the size and ctime still differ
        self.assertEqual(self.scan(SPEECH_A[0])[0], "CLEAN")
        self.assertEqual(self.verdict(self.scan(SPEECH_B[2])), "BLOCK")

    def test_3_a_removed_private_file_no_longer_counts(self):
        self.assertEqual(self.verdict(self.scan(SPEECH_A[5])), "BLOCK")
        os.unlink(os.path.join(self.private, "call-a.txt"))
        out = self.scan(SPEECH_A[5])
        self.assertEqual(out[0], "CLEAN")
        self.assertEqual(out[1].split("\t")[:2], ["CORPUS", "0"])

    def test_4_a_new_private_file_counts_at_once(self):
        self.assertEqual(self.scan(SPEECH_B[1])[0], "CLEAN")
        self.write(self.private, "call-b.txt", transcript(SPEECH_B, "Bruno"))
        self.assertEqual(self.verdict(self.scan(SPEECH_B[1])), "BLOCK")

    @unittest.skipIf(ROOT, "root reads everything")
    def test_5_a_cached_file_made_unreadable_is_broken_not_clean(self):
        self.assertEqual(self.verdict(self.scan(SPEECH_A[1])), "BLOCK")
        p = os.path.join(self.private, "call-a.txt")
        os.chmod(p, 0)
        self.addCleanup(os.chmod, p, stat.S_IRUSR | stat.S_IWUSR)
        out = self.scan(SPEECH_A[1])
        self.assertEqual(self.verdict(out), "BROKEN")
        self.assertIn("call-a.txt", out[0])

    def test_6_a_rendering_admitted_by_the_closure_survives_the_cache(self):
        # A plain-text rendering (no timestamps) that repeats the transcript and
        # adds one line of its own: the closure admits it, so its own line blocks.
        own = "the lighthouse keeper counted forty seven gulls on the roof today"
        lines = ["%s and %s while %s" % (SPEECH_A[i % 10], SPEECH_B[i % 8], SPEECH_A[(i + 3) % 10])
                 for i in range(40)]
        self.write(self.private, "call-c.txt", transcript(lines, "Chen"))
        self.assertEqual(self.scan(own)[0], "CLEAN")
        self.write(self.private, "call-c plain.txt", "\n".join(lines) + "\n" + own + "\n")
        out = self.scan(own)
        self.assertEqual(self.verdict(out), "BLOCK")
        self.assertEqual(out[-2].split("\t")[:2], ["CORPUS", "3"])     # a, c and the admitted rendering

    def test_7_the_cache_is_private_and_scratch_sources_are_never_cached(self):
        self.scan(SPEECH_A[2])
        entries = [os.path.join(self.cache, e) for e in os.listdir(self.cache)]
        self.assertEqual(len(entries), 1)
        self.assertEqual(stat.S_IMODE(os.stat(entries[0]).st_mode), 0o700)
        spec = importlib.util.spec_from_file_location("pb", SCANNER)
        pb = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(pb)
        saved = os.environ.pop("RICHOS_PUBLICATION_CACHE_DIR", None)
        try:
            self.assertIsNone(pb.cache_root([self.private]))
        finally:
            if saved is not None:
                os.environ["RICHOS_PUBLICATION_CACHE_DIR"] = saved


if __name__ == "__main__":
    unittest.main(verbosity=2)
