import importlib.util
import unittest
from pathlib import Path

MODULE = Path(__file__).with_name("annotate_workspace_tests.py")
spec = importlib.util.spec_from_file_location("annotate", MODULE)
annotate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(annotate)


class AnnotateTests(unittest.TestCase):
    def test_strips_ansi_and_keeps_panic_context(self) -> None:
        text = "\x1b[31mthread 'x' panicked at file.rs:1\x1b[0m\nvalue mismatch\nexpected 1\nactual 2\n"
        lines = annotate.ANSI.sub("", text).splitlines()
        selected = [line for _, line in annotate.interesting_lines(lines)]
        self.assertIn("thread 'x' panicked at file.rs:1", selected)
        self.assertIn("value mismatch", selected)

    def test_escapes_github_commands(self) -> None:
        self.assertEqual(annotate.escape("a%b\r\nc"), "a%25b%0D%0Ac")


if __name__ == "__main__":
    unittest.main()
