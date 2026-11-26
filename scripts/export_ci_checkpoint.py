#!/usr/bin/env python3
"""Export bounded CI repair candidates without reading runner secrets or untracked files.

Only modified Rust blobs already tracked at the source commit and three explicitly
named generated JSON documents are eligible. Generated documents are included only
when their producer step succeeded. Nothing is pushed or written by this exporter.

Reassemble the NODES_CHECKPOINT_PART fragments in numerical order, verify the
SHA-256 in NODES_CHECKPOINT_BEGIN, then parse the resulting JSON. A checkpoint is
a review candidate, never evidence that the original source commit passed tests.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tempfile
import unittest

GENERATED = {
    "docs/compatibility/nodes_inventory.json": "registry",
    "docs/compatibility/gh2_official_inventory.json": "gh2",
    "docs/development/performance.json": "perf",
}
STEP_NAMES = frozenset({
    "checkout", "system", "toolchain", "exporter_self_test", "fmt_fix",
    "clippy_fix", "fmt_after_fix", "check", "test", "fmt_check",
    "clippy_check", "registry", "gh2", "perf",
})
OUTCOMES = frozenset({"success", "failure", "cancelled", "skipped", "unknown"})
MAX_FILE_BYTES = 2 * 1024 * 1024
MAX_TOTAL_BYTES = 8 * 1024 * 1024
MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_GIT_BYTES = 2 * 1024 * 1024
MAX_FILES = 256
CHUNK_CHARS = 8000


class ExportError(ValueError):
    """A safe, fixed diagnostic; never contains file contents or command stderr."""


def git(root: Path, *arguments: str) -> bytes:
    try:
        result = subprocess.run(
            ["git", "-C", str(root), *arguments],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            check=False, timeout=20,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ExportError("git_command_unavailable") from error
    if result.returncode:
        raise ExportError("git_command_failed")
    if len(result.stdout) > MAX_GIT_BYTES:
        raise ExportError("git_metadata_limit")
    return result.stdout


def parse_steps(arguments: list[str]) -> dict[str, str]:
    steps: dict[str, str] = {}
    for argument in arguments:
        name, separator, outcome = argument.partition("=")
        if not separator or name not in STEP_NAMES or outcome not in OUTCOMES:
            raise ExportError("invalid_step_outcome")
        if name in steps:
            raise ExportError("duplicate_step_outcome")
        steps[name] = outcome
    return dict(sorted(steps.items()))


def source_files(root: Path, revision: str) -> dict[str, tuple[str, str]]:
    entries: dict[str, tuple[str, str]] = {}
    for entry in git(root, "ls-tree", "-r", "-z", "--full-tree", revision).split(b"\0"):
        if not entry:
            continue
        metadata, separator, path = entry.partition(b"\t")
        if not separator:
            raise ExportError("invalid_git_tree")
        try:
            mode, kind, blob = metadata.decode("ascii").split()
            name = path.decode("utf-8")
        except (UnicodeError, ValueError) as error:
            raise ExportError("invalid_git_tree_encoding") from error
        if kind == "blob":
            entries[name] = (mode, blob)
    return entries


def read_regular(root: Path, name: str, limit: int) -> bytes:
    relative = PurePosixPath(name)
    if (
        relative.is_absolute() or not relative.parts
        or any(part in {".", "..", ".git"} for part in relative.parts)
        or "\\" in name
    ):
        raise ExportError("unsafe_path")
    current = root
    try:
        for part in relative.parts:
            current = current / part
            if stat.S_ISLNK(current.lstat().st_mode):
                raise ExportError("symlink_rejected")
        info = current.stat()
        if not stat.S_ISREG(info.st_mode):
            raise ExportError("not_a_regular_file")
        if info.st_size > limit:
            raise ExportError("file_size_limit")
        with current.open("rb") as stream:
            data = stream.read(limit + 1)
        if len(data) > limit:
            raise ExportError("file_size_limit")
        return data
    except OSError as error:
        raise ExportError("unreadable_or_missing_file") from error


def reject_nonfinite(_value: str) -> None:
    raise ValueError("non-finite JSON number")


def collect(
    root: Path, revision: str, steps: dict[str, str], *,
    file_limit: int = MAX_FILE_BYTES, total_limit: int = MAX_TOTAL_BYTES,
    file_count_limit: int = MAX_FILES,
) -> dict:
    if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", revision):
        raise ExportError("invalid_source_revision")
    head = git(root, "rev-parse", "--verify", "HEAD").decode("ascii").strip()
    if head != revision:
        raise ExportError("source_revision_mismatch")
    tracked = source_files(root, revision)
    changed_raw = git(
        root, "diff", "--name-only", "-z", "--no-ext-diff", "--no-renames",
        revision, "--", "*.rs",
    )
    try:
        changed = {
            name.decode("utf-8")
            for name in changed_raw.split(b"\0")
            if name and name.decode("utf-8") in tracked
        }
    except UnicodeError as error:
        raise ExportError("invalid_changed_path_encoding") from error
    candidates = {name: "tracked_rust" for name in changed if name.endswith(".rs")}
    for name, producer in GENERATED.items():
        if steps.get(producer) == "success":
            candidates[name] = "generated_json"
    files = []
    omitted = []
    used = 0
    for name, kind in sorted(candidates.items()):
        try:
            if len(files) >= file_count_limit:
                raise ExportError("file_count_limit")
            original = tracked.get(name)
            if original and original[0] not in {"100644", "100755"}:
                raise ExportError("source_is_not_a_regular_blob")
            data = read_regular(root, name, file_limit)
            try:
                content = data.decode("utf-8")
            except UnicodeError as error:
                raise ExportError("non_utf8_file") from error
            if kind == "generated_json":
                try:
                    json.loads(content, parse_constant=reject_nonfinite)
                except (ValueError, RecursionError) as error:
                    raise ExportError("invalid_generated_json") from error
            if used + len(data) > total_limit:
                raise ExportError("total_size_limit")
            files.append({
                "path": name, "kind": kind,
                "source_blob_sha": original[1] if original else None,
                "sha256": hashlib.sha256(data).hexdigest(),
                "bytes": len(data), "content": content,
            })
            used += len(data)
        except ExportError as error:
            omitted.append({"path": name, "reason": str(error)})
    return {
        "schema_version": 1,
        "repository": "07egta/NODES",
        "source_revision": revision,
        "validation_scope": (
            "CI working tree after formatter and Clippy repair; candidate files "
            "require review and a fresh committed-source quality gate"
        ),
        "step_outcomes": dict(sorted(steps.items())),
        "generated_allowlist": GENERATED,
        "limits": {
            "file_bytes": file_limit, "total_file_bytes": total_limit,
            "files": file_count_limit, "encoded_json_bytes": MAX_JSON_BYTES,
            "chunk_characters": CHUNK_CHARS,
        },
        "files": files, "omitted": omitted, "complete": not omitted,
    }


def serialize(report: dict, limit: int = MAX_JSON_BYTES) -> str:
    # ASCII escaping keeps every fragment on exactly one log line, including
    # source strings containing newlines or workflow-command-looking text.
    # The runner's legacy ##[command] parser also searches inside a line;
    # neutralize both command syntaxes while preserving decoded JSON content.
    while True:
        payload = (
            json.dumps(report, ensure_ascii=True, separators=(",", ":"), allow_nan=False)
            .replace("##[", r"\u0023\u0023[")
            .replace("::", r"\u003a\u003a")
        )
        if len(payload) <= limit:
            return payload
        if not report["files"]:
            raise ExportError("encoded_metadata_limit")
        dropped = report["files"].pop()
        report["omitted"].append({"path": dropped["path"], "reason": "encoded_json_limit"})
        report["complete"] = False


def checkpoint_lines(payload: str, revision: str, chunk_size: int = CHUNK_CHARS) -> list[str]:
    if (
        not 1 <= chunk_size <= CHUNK_CHARS or not payload.isascii()
        or "##[" in payload or "::" in payload
    ):
        raise ExportError("invalid_chunk_encoding")
    parts = [payload[index:index + chunk_size] for index in range(0, len(payload), chunk_size)]
    digest = hashlib.sha256(payload.encode("ascii")).hexdigest()
    return [
        f"NODES_CHECKPOINT_BEGIN source={revision} sha256={digest} bytes={len(payload)} parts={len(parts)}",
        *(f"NODES_CHECKPOINT_PART {index}/{len(parts)} {part}" for index, part in enumerate(parts, 1)),
        f"NODES_CHECKPOINT_END sha256={digest}",
    ]


class CheckpointTests(unittest.TestCase):
    def repository(self) -> tuple[Path, str]:
        temporary = tempfile.TemporaryDirectory(prefix="nodes-checkpoint-test-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name) / "repository"
        (root / "src").mkdir(parents=True)
        (root / "src/changed.rs").write_text("fn value() -> u32 { 1 }\n", encoding="utf-8")
        (root / "src/unchanged.rs").write_text("// unchanged\n", encoding="utf-8")
        (root / "notes.txt").write_text("tracked non-Rust\n", encoding="utf-8")
        git(root, "init", "-q")
        git(root, "add", "--", "src/changed.rs", "src/unchanged.rs", "notes.txt")
        git(
            root, "-c", "user.name=NODES exporter test",
            "-c", "user.email=exporter-test@example.invalid",
            "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
            "commit", "-qm", "fixture",
        )
        revision = git(root, "rev-parse", "HEAD").decode("ascii").strip()
        return root, revision

    def write_json(self, root: Path, name: str, content: str = '{"ok":true}\n') -> None:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def test_only_changed_source_and_successful_allowlisted_generation(self) -> None:
        root, revision = self.repository()
        text = "// 日本語\nfn value() -> u32 { 2 }\n"
        (root / "src/changed.rs").write_text(text, encoding="utf-8")
        (root / "src/untracked.rs").write_text("DO_NOT_EXPORT_UNTRACKED", encoding="utf-8")
        (root / "notes.txt").write_text("DO_NOT_EXPORT_NON_RUST", encoding="utf-8")
        (root / "secret.env").write_text("DO_NOT_EXPORT_ENV_FILE", encoding="utf-8")
        self.write_json(root, "docs/compatibility/nodes_inventory.json")
        self.write_json(root, "docs/compatibility/gh2_official_inventory.json", '{"stale":true}')
        self.write_json(root, "docs/not_allowlisted.json", '{"secret":true}')
        report = collect(root, revision, {"registry": "success", "gh2": "failure"})
        self.assertTrue(report["complete"])
        by_path = {item["path"]: item for item in report["files"]}
        self.assertEqual(set(by_path), {"src/changed.rs", "docs/compatibility/nodes_inventory.json"})
        self.assertEqual(by_path["src/changed.rs"]["content"], text)
        self.assertEqual(
            by_path["src/changed.rs"]["sha256"],
            hashlib.sha256(text.encode("utf-8")).hexdigest(),
        )
        self.assertIsNotNone(by_path["src/changed.rs"]["source_blob_sha"])
        self.assertIsNone(by_path["docs/compatibility/nodes_inventory.json"]["source_blob_sha"])

    def test_symlink_and_missing_source_contents_are_never_exported(self) -> None:
        root, revision = self.repository()
        outside = root.parent / "private"
        outside.mkdir()
        (outside / "secret.rs").write_text("DO_NOT_EXPORT_SYMLINK_TARGET", encoding="utf-8")
        (outside / "performance.json").write_text('{"private":true}', encoding="utf-8")
        (root / "src/changed.rs").unlink()
        (root / "src/changed.rs").symlink_to(outside / "secret.rs")
        (root / "src/unchanged.rs").unlink()
        (root / "docs").mkdir()
        (root / "docs/development").symlink_to(outside, target_is_directory=True)
        report = collect(root, revision, {"perf": "success"})
        self.assertFalse(report["complete"])
        self.assertEqual(report["files"], [])
        reasons = {entry["reason"] for entry in report["omitted"]}
        self.assertIn("symlink_rejected", reasons)
        self.assertIn("unreadable_or_missing_file", reasons)
        self.assertNotIn("DO_NOT_EXPORT_SYMLINK_TARGET", serialize(report))

    def test_file_total_and_json_budgets_are_explicit(self) -> None:
        root, revision = self.repository()
        (root / "src/changed.rs").write_text("x" * 200, encoding="utf-8")
        report = collect(root, revision, {}, file_limit=100)
        self.assertEqual(report["omitted"][0]["reason"], "file_size_limit")
        report = collect(root, revision, {}, total_limit=100)
        self.assertEqual(report["omitted"][0]["reason"], "total_size_limit")
        report = collect(root, revision, {}, file_count_limit=0)
        self.assertEqual(report["omitted"][0]["reason"], "file_count_limit")
        report = collect(root, revision, {})
        empty_size = len(serialize({**report, "files": []}))
        payload = serialize(report, limit=empty_size + 100)
        self.assertLessEqual(len(payload), empty_size + 100)
        self.assertFalse(report["complete"])
        self.assertEqual(report["omitted"][0]["reason"], "encoded_json_limit")

    def test_invalid_utf8_json_and_wrong_source_are_rejected(self) -> None:
        root, revision = self.repository()
        (root / "src/changed.rs").write_bytes(b"\xff")
        self.write_json(root, "docs/development/performance.json", '{"bad":NaN}')
        report = collect(root, revision, {"perf": "success"})
        self.assertEqual(report["files"], [])
        self.assertEqual(
            {item["reason"] for item in report["omitted"]},
            {"non_utf8_file", "invalid_generated_json"},
        )
        with self.assertRaisesRegex(ExportError, "source_revision_mismatch"):
            collect(root, "0" * len(revision), {})
        with self.assertRaisesRegex(ExportError, "invalid_source_revision"):
            collect(root, "main", {})

    def test_numbered_fragments_roundtrip_without_log_command_injection(self) -> None:
        original = "日本語\n::error::fake\n##[error]fake\n" + "A" * 20000
        payload = serialize({"files": [], "omitted": [], "complete": True, "text": original})
        self.assertEqual(json.loads(payload)["text"], original)
        self.assertNotIn("##[", payload)
        self.assertNotIn("::", payload)
        with self.assertRaises(ExportError):
            checkpoint_lines('{"unsafe":"##[error]fake"}', "1" * 40)
        lines = checkpoint_lines(payload, "1" * 40)
        parts = []
        expected_count = len(lines) - 2
        for index, line in enumerate(lines[1:-1], 1):
            prefix, position, fragment = line.split(" ", 2)
            self.assertEqual(prefix, "NODES_CHECKPOINT_PART")
            self.assertEqual(position, f"{index}/{expected_count}")
            self.assertLessEqual(len(fragment), CHUNK_CHARS)
            self.assertNotIn("\n", line)
            parts.append(fragment)
        self.assertEqual("".join(parts), payload)
        digest = hashlib.sha256(payload.encode("ascii")).hexdigest()
        self.assertIn(f"sha256={digest}", lines[0])
        self.assertEqual(lines[-1], f"NODES_CHECKPOINT_END sha256={digest}")

    def test_step_metadata_is_strict_and_does_not_accept_environment_names(self) -> None:
        self.assertEqual(parse_steps(["test=failure", "check=success"]), {"check": "success", "test": "failure"})
        for arguments in (["SECRET=success"], ["test=secret"], ["test=success", "test=failure"]):
            with self.assertRaises(ExportError):
                parse_steps(arguments)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-revision", help="Exact checked-out commit SHA")
    parser.add_argument("--step", action="append", default=[], help="Allowed step ID=outcome")
    parser.add_argument("--self-test", action="store_true", help="Run isolated offline exporter regressions")
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(CheckpointTests)
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    if args.source_revision is None:
        parser.error("--source-revision is required unless --self-test is used")
    try:
        root = Path(__file__).resolve().parents[1]
        report = collect(root, args.source_revision, parse_steps(args.step))
        payload = serialize(report)
        for line in checkpoint_lines(payload, args.source_revision):
            print(line, flush=True)
        return 0 if report["complete"] else 1
    except ExportError as error:
        print(f"NODES_CHECKPOINT_ERROR {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
