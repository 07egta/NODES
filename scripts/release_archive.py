#!/usr/bin/env python3
"""Build-only Windows ZIP provenance tool; no application runtime dependency."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tempfile
import zipfile
import zlib

MANIFEST = "BUILD-MANIFEST.json"
REQUIRED = {"NODES.exe", "nodes-cli.exe", "Cargo.lock", "README.md",
            "LICENSE-MIT", "LICENSE-APACHE", "BUILD-INFO.txt",
            "docs/USER_GUIDE_JA.md", "docs/DEPENDENCIES.md"}
MAX_TOTAL = 2 * 1024**3
MAX_MANIFEST = 8 * 1024**2
CHUNK = 1024**2


def check_names(names):
    seen = set()
    spelling = {}
    for name in names:
        parts = name.split("/")
        for part in parts:
            device = part.split(".")[0].upper()
            if (not part or part in {".", ".."} or part.endswith((".", " "))
                    or any(ord(c) < 32 or c in '\\:*?"<>|' for c in part)
                    or device in {"CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"}
                    or re.fullmatch(r"(?:COM|LPT)[1-9¹²³]", device)):
                raise ValueError(f"unsafe Windows ZIP path: {name}")
        for i in range(1, len(parts) + 1):
            prefix = "/".join(parts[:i])
            key = prefix.casefold()
            if spelling.setdefault(key, prefix) != prefix:
                raise ValueError(f"case-colliding ZIP path: {name}")
        if name in seen:
            raise ValueError(f"duplicate ZIP path: {name}")
        seen.add(name)
    if any(str(parent) in seen for n in seen for parent in PurePosixPath(n).parents):
        raise ValueError("file/directory collision")


def check_payload(names):
    check_names(names)
    if MANIFEST in names or REQUIRED.difference(names):
        raise ValueError(f"stale manifest or missing files: {sorted(REQUIRED.difference(names))}")
    if not any(n.startswith("examples/") and n.endswith(".morph") for n in names):
        raise ValueError("no example project")
    if not any(n.startswith("licenses/") for n in names):
        raise ValueError("no dependency license files")
    check_names([*names, MANIFEST])


def regular(path, directory=False):
    info = path.lstat()
    expected = stat.S_ISDIR if directory else stat.S_ISREG
    if not expected(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
        raise ValueError(f"not a regular {'directory' if directory else 'file'}: {path}")
    return info


def inventory(root):
    regular(root, directory=True)
    files = {}
    total = 0
    for base, dirs, names in os.walk(root, onerror=lambda e: (_ for _ in ()).throw(e)):
        for name in dirs:
            regular(Path(base, name), directory=True)
        for name in names:
            path = Path(base, name)
            info = regular(path)
            total += info.st_size
            if total > MAX_TOTAL:
                raise ValueError("staging exceeds 2 GiB")
            files[path.relative_to(root).as_posix()] = path
    check_payload(list(files))
    return files


def digest(source, destination=None):
    h = hashlib.sha256()
    size = 0
    while chunk := source.read(CHUNK):
        size += len(chunk)
        if size > MAX_TOTAL:
            raise ValueError("file exceeds 2 GiB")
        h.update(chunk)
        if destination is not None:
            destination.write(chunk)
    return {"sha256": h.hexdigest(), "bytes": size}


def metadata(source, version, rustc, cargo):
    if not re.fullmatch(r"[0-9a-f]{40}", source):
        raise ValueError("source must be a full lowercase Git SHA")
    if not all(isinstance(v, str) and v.strip() for v in (version, rustc, cargo)):
        raise ValueError("missing version or compiler metadata")
    return {"schema": 1, "product": "NODES", "channel": "development",
            "target": "x86_64-pc-windows-msvc", "source_sha": source,
            "version": version, "rustc": rustc, "cargo": cargo,
            "zlib": zlib.ZLIB_RUNTIME_VERSION}


def zipinfo(name):
    info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.external_attr = (stat.S_IFREG | (0o755 if name.endswith(".exe") else 0o644)) << 16
    # ZIP_STORED would be independent of zlib but much larger. Pin compression
    # settings and record the runtime zlib version instead.
    info.compress_type = zipfile.ZIP_DEFLATED
    # ZipFile.open uses the ZipInfo compression level, not the archive default.
    info._compresslevel = 9
    return info


def manifest_bytes(data):
    return (json.dumps(data, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def verify(archive, expected_source=None):
    with zipfile.ZipFile(archive) as z:
        entries = z.infolist()
        names = [e.filename for e in entries]
        check_names(names)
        if sum(e.file_size for e in entries) > MAX_TOTAL + MAX_MANIFEST:
            raise ValueError("archive exceeds size budget")
        if any(e.is_dir() or not stat.S_ISREG(e.external_attr >> 16) or e.flag_bits & 1
               for e in entries):
            raise ValueError("archive contains non-regular or encrypted entries")
        if MANIFEST not in names or z.getinfo(MANIFEST).file_size > MAX_MANIFEST:
            raise ValueError("missing or oversized manifest")
        payload = [n for n in names if n != MANIFEST]
        check_payload(payload)
        data = json.loads(z.read(MANIFEST))
        expected = metadata(data["source_sha"], data["version"], data["rustc"], data["cargo"])
        expected["zlib"] = data["zlib"]  # Verification can use another zlib version.
        records = data["files"]
        if (data != {**expected, "files": records} or set(records) != set(payload)
                or not isinstance(data["zlib"], str) or not data["zlib"]):
            raise ValueError("manifest metadata or file inventory mismatch")
        if expected_source is not None and data["source_sha"] != expected_source:
            raise ValueError("source SHA mismatch")
        for name in payload:
            with z.open(name) as stream:
                if digest(stream) != records[name]:
                    raise ValueError(f"file hash or size mismatch: {name}")
    return data


def archive_hash(path):
    with path.open("rb") as stream:
        return digest(stream)["sha256"]


def checksum_path(path):
    return path.with_name(path.name + ".sha256")


def checksum_line(path):
    return f"{archive_hash(path)}  {path.name}\n".encode("utf-8")


def pack(root, output, data):
    files = inventory(root)
    root = root.resolve()
    output = output.resolve()
    if output.is_relative_to(root) or checksum_path(output).is_relative_to(root):
        raise ValueError("output must be outside staging")
    if output.exists() or checksum_path(output).exists():
        raise ValueError("refusing to overwrite existing archive or checksum")
    with tempfile.TemporaryDirectory(dir=output.parent) as tmp:
        candidate = Path(tmp, output.name)
        records = {}
        with zipfile.ZipFile(candidate, "w", allowZip64=True) as z:
            for name, path in sorted(files.items()):
                regular(path)
                with path.open("rb") as source, z.open(zipinfo(name), "w", force_zip64=True) as dest:
                    records[name] = digest(source, dest)
            encoded = manifest_bytes({**data, "files": records})
            if len(encoded) > MAX_MANIFEST:
                raise ValueError("manifest exceeds size budget")
            z.writestr(zipinfo(MANIFEST), encoded)
        verify(candidate, data["source_sha"])
        sidecar = checksum_path(candidate)
        sidecar.write_bytes(checksum_line(candidate))
        # Publish only after internal validation. No upload or release mutation.
        candidate.replace(output)
        sidecar.replace(checksum_path(output))
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("pack")
    p.add_argument("staging", type=Path)
    p.add_argument("output", type=Path)
    for flag in ("source-sha", "version", "rustc", "cargo"):
        p.add_argument("--" + flag, required=True)
    v = sub.add_parser("verify")
    v.add_argument("archive", type=Path)
    v.add_argument("--source-sha", required=True)
    args = parser.parse_args()
    try:
        if args.command == "pack":
            data = metadata(args.source_sha, args.version, args.rustc, args.cargo)
            print(pack(args.staging, args.output, data))
        else:
            if checksum_path(args.archive).read_bytes() != checksum_line(args.archive):
                raise ValueError("archive checksum mismatch")
            data = verify(args.archive, args.source_sha)
            print(f"Verified {len(data['files'])} files; source {data['source_sha']}")
    except (OSError, ValueError, KeyError, TypeError, AttributeError, RuntimeError, zipfile.BadZipFile) as error:
        parser.exit(1, f"release archive: {error}\n")


if __name__ == "__main__":
    main()
