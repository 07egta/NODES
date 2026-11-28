#!/usr/bin/env python3
"""CI/developer verification only: native CLI fixture and optional real GUI screenshot."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import sys
import time
import zlib


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def check_stl(path):
    data = path.read_bytes()
    require(len(data) >= 84, "STL header missing")
    count = struct.unpack_from("<I", data, 80)[0]
    require(count == 11808 and len(data) == 84 + 50 * count, "STL triangle/size mismatch")
    minimum = [math.inf] * 3
    maximum = [-math.inf] * 3
    for offset in range(84, len(data), 50):
        values = struct.unpack_from("<12fH", data, offset)
        require(all(math.isfinite(v) for v in values[:12]), "Non-finite STL")
        for i in range(3, 12):
            axis = (i - 3) % 3
            minimum[axis] = min(minimum[axis], values[i])
            maximum[axis] = max(maximum[axis], values[i])
    require(math.isclose(maximum[2] - minimum[2], 104, abs_tol=0.0001),
            "Unexpected vase height")
    return {"triangles": count, "bounds_mm": [minimum, maximum], "sha256": digest(path)}


def png_info(path):
    # Check a complete PNG, not a stale file or a partially written screenshot.
    data = path.read_bytes()
    require(data[:8] == b"\x89PNG\r\n\x1a\n", "Invalid PNG signature")
    offset = 8
    width = height = None
    idat = bytearray()
    while offset + 12 <= len(data):
        size = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4:offset + 8]
        end = offset + 12 + size
        require(end <= len(data), "Incomplete PNG chunk")
        payload = data[offset + 8:end - 4]
        crc = struct.unpack_from(">I", data, end - 4)[0]
        require(zlib.crc32(kind + payload) == crc, "PNG CRC mismatch")
        if kind == b"IHDR":
            require(size == 13, "Invalid PNG header")
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", payload)
            require(100 <= width <= 16384 and 100 <= height <= 16384,
                    "Unexpected screenshot dimensions")
            require(depth == 8 and color == 6 and interlace == 0, "Unexpected screenshot format")
        elif kind == b"IDAT":
            idat.extend(payload)
        elif kind == b"IEND":
            require(width is not None and idat and end == len(data), "Incomplete PNG")
            raw = zlib.decompress(idat)
            require(len(raw) == height * (1 + 4 * width), "Invalid PNG pixel data length")
            return {"width": width, "height": height, "sha256": digest(path)}
        offset = end
    raise RuntimeError("PNG not yet complete")


def run_cli(cli, args, directory, env, label):
    proc = subprocess.run([str(cli), *map(str, args)], cwd=directory, env=env,
                          capture_output=True, timeout=90, check=False)
    (directory / (label + ".json")).write_bytes(proc.stdout)
    (directory / (label + ".stderr.log")).write_bytes(proc.stderr)
    require(proc.returncode == 0, label + " failed")
    response = json.loads(proc.stdout)
    require(response.get("ok") is True, label + " returned unsuccessful JSON")
    return response["result"]


def gui_smoke(gui, fixture, directory, env, timeout):
    screenshot = directory / "native-frame.png"
    start = time.monotonic()
    with (directory / "gui.log").open("wb") as log:
        proc = subprocess.Popen([str(gui), str(fixture), "--screenshot", str(screenshot)],
                                cwd=directory, env=env, stdin=subprocess.DEVNULL,
                                stdout=log, stderr=subprocess.STDOUT)
        try:
            while time.monotonic() - start < timeout:
                require(proc.poll() is None, "GUI exited before a complete rendered frame")
                if screenshot.is_file():
                    try:
                        result = png_info(screenshot)
                    except (RuntimeError, OSError, ValueError, zlib.error, struct.error):
                        pass
                    else:
                        result["elapsed_to_screenshot_seconds"] = time.monotonic() - start
                        return result
                time.sleep(0.2)
            raise RuntimeError("GUI render timeout; see gui.log")
        finally:
            # Only stop the child this invocation launched. Never kill other NODES instances.
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--gui", type=Path)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    cli = args.cli.resolve(strict=True)
    gui = args.gui.resolve(strict=True) if args.gui else None
    fixture = args.fixture.resolve(strict=True)
    # Refuse an existing directory; never remove prior evidence or developer files.
    directory = args.output_dir.absolute()
    directory.mkdir(parents=True, exist_ok=False)
    report = {"source_sha": os.environ.get("GITHUB_SHA"), "platform": platform.platform(),
              "machine": platform.machine(), "cli_sha256": digest(cli),
              "fixture_sha256": digest(fixture), "cli_status": "not_run",
              "gui_status": "not_run", "physical_gpu_verified": False,
              "ime_verified": False, "ok": False}
    try:
        project = directory / "日本語 作品.morph"
        shutil.copyfile(fixture, project)
        require(digest(project) == report["fixture_sha256"], "Fixture copy differs")
        env = os.environ.copy()
        for key in ("APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "XDG_DATA_HOME"):
            private = directory / key.lower()
            private.mkdir()
            env[key] = str(private)
        run_cli(cli, ["validate", project], directory, env, "validate")
        run_cli(cli, ["evaluate", project], directory, env, "evaluate")
        stl = directory / "日本語 出力.stl"
        result = run_cli(cli, ["export-stl", project, stl], directory, env, "export")
        require(result.get("triangles") == 11808 and result.get("vertices") == 5906,
                "Unexpected CLI mesh counts")
        require(result.get("units") == "millimeters", "Unexpected units")
        report["stl"] = check_stl(stl)
        require(digest(project) == report["fixture_sha256"], "CLI mutated source")
        report["cli_status"] = "passed"
        if gui:
            report["gui_sha256"] = digest(gui)
            report["gui_status"] = "failed"
            report["frame"] = gui_smoke(gui, project, directory, env, args.timeout)
            report["gui_status"] = "rendered_frame"
        report["ok"] = True
    except Exception as exc:
        if report["cli_status"] == "not_run":
            report["cli_status"] = "failed"
        report["error"] = str(exc)
    finally:
        (directory / "native-report.json").write_text(
            json.dumps(report, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")
        print(json.dumps(report, ensure_ascii=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
