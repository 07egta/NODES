"""Run with python scripts/test_release_archive.py (no Rust build required)."""
import os
from pathlib import Path
import tempfile
import unittest
import warnings
import zipfile

import release_archive as r

SHA = "a" * 40


class ReleaseArchiveTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.stage = self.root / "stage"
        self.stage.mkdir()
        for name in sorted(r.REQUIRED | {"examples/01.morph", "licenses/notice.txt", "はじめに.txt"}):
            path = self.stage / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(name.encode("utf-8"))
        self.data = r.metadata(SHA, "0.1.0", "rustc test", "cargo test")

    def pack(self, name="first.zip"):
        return r.pack(self.stage, self.root / name, self.data)

    def test_reproducible_unicode(self):
        first = self.pack()
        for path in self.stage.rglob("*"):
            os.utime(path, (1700000000, 1700000000))
        second = self.pack("second.zip")
        self.assertEqual(first.read_bytes(), second.read_bytes())
        result = r.verify(first, SHA)
        self.assertIn("はじめに.txt", result["files"])
        self.assertEqual(r.checksum_path(first).read_bytes(), r.checksum_line(first))

    def test_wrong_sha(self):
        with self.assertRaisesRegex(ValueError, "source SHA mismatch"):
            r.verify(self.pack(), "b" * 40)

    def test_missing_cli(self):
        (self.stage / "nodes-cli.exe").unlink()
        with self.assertRaisesRegex(ValueError, "missing files"):
            self.pack()
        self.assertFalse((self.root / "first.zip").exists())

    def test_stale_manifest(self):
        (self.stage / r.MANIFEST).write_text("{}")
        with self.assertRaisesRegex(ValueError, "stale manifest"):
            self.pack()

    def test_tampered_payload(self):
        source = self.pack()
        target = self.root / "tampered.zip"
        with zipfile.ZipFile(source) as old, zipfile.ZipFile(target, "w") as new:
            for name in old.namelist():
                content = b"bad" if name == "NODES.exe" else old.read(name)
                new.writestr(r.zipinfo(name), content)
        with self.assertRaisesRegex(ValueError, "hash or size mismatch"):
            r.verify(target, SHA)

    def test_duplicate_entry(self):
        source = self.pack()
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(source, "a") as z:
                z.writestr(r.zipinfo("NODES.exe"), b"duplicate")
        with self.assertRaisesRegex(ValueError, "duplicate ZIP path"):
            r.verify(source, SHA)

    def test_path_rejection(self):
        for names in (["../x"], ["/x"], ["C:/x"], ["dir/CON.txt"], ["x."],
                      ["x/y", "X/z"], ["x", "x/y"], ["x\\y"]):
            with self.subTest(names=names), self.assertRaises(ValueError):
                r.check_names(names)

    def test_output_in_staging(self):
        with self.assertRaisesRegex(ValueError, "outside staging"):
            r.pack(self.stage, self.stage / "test.zip", self.data)

    def test_no_overwrite(self):
        output = self.pack()
        before = output.read_bytes()
        with self.assertRaisesRegex(ValueError, "refusing to overwrite"):
            self.pack()
        self.assertEqual(before, output.read_bytes())


if __name__ == "__main__":
    unittest.main()
