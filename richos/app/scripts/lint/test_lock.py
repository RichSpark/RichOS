import unittest
from lock_rules import scan


class LockRules(unittest.TestCase):
    def test_an_undeclared_non_blocking_lock_is_a_site(self):
        for source in (
            "if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } != 0 {",
            "// lock-busy:\nlibc::flock(fd, libc::LOCK_SH | libc::LOCK_NB);",
            "// lock-busy: wait\nlibc::flock(fd, libc::LOCK_SH | libc::LOCK_NB);",
            "// lock-busy: answers busy at once, so the caller can say so\n\n\n\nlibc::flock(fd, libc::LOCK_NB);",
        ):
            with self.subTest(source=source):
                self.assertEqual([r for r, _ in scan(source)], ["lock-busy-undeclared"])

    def test_a_declared_site_or_prose_is_not(self):
        for source in (
            "// lock-busy: one poll inside the 5 s PUBLICATION_WAIT loop\n"
            "if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } != 0 {",
            "libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) // lock-busy: busy means another owner, Ok(None)",
            "// lock-busy: busy is the answer: a liveness probe\n(if exclusive {\n libc::LOCK_EX\n}) | libc::LOCK_NB,",
            "/// Readers took it with ONE non-blocking `flock` (`LOCK_NB`) and no wait.",
            "    // LOCK_NB alone is never the bug; the missing wait is.",
        ):
            with self.subTest(source=source):
                self.assertEqual(scan(source), [])

    def test_the_line_number_is_the_site(self):
        self.assertEqual(scan("fn a() {}\nlibc::flock(fd, libc::LOCK_NB);"), [("lock-busy-undeclared", 2)])


if __name__ == "__main__":
    unittest.main()
