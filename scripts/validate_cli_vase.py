#!/usr/bin/env python3
"""Validate the real NODES CLI vase workflow and independently inspect both STLs.

Only Python's standard library is used. --output-dir must be new or empty.
Every CLI invocation has a 30-second timeout. Source attribution is unknown
unless supplied explicitly; a source revision is never inferred from a checkout.
This checks one fixture and does not certify printability, manifoldness,
self-intersection, minimum wall thickness, GUI behavior, or printer hardware.
"""
from __future__ import annotations

import argparse
import copy
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import stat
import struct
import subprocess
import sys
import time

TIMEOUT_SECONDS = 30
MAX_JSON_BYTES = 8 * 1024 * 1024
EXPECTED_VERTICES = 5906
EXPECTED_TRIANGLES = 11808
EXPECTED_STL_BYTES = 590484
EXPECTED_HEIGHT_MM = 104.0
EXPECTED_HEADER = b"NODES binary STL | units: millimeters | mesh geometry"
VOLUME_REL_TOL = 1e-5
VOLUME_ABS_TOL_MM3 = 1e-3
BOUNDS_ABS_TOL_MM = 1e-5
EXPECTED_PARAMS = {
    "radius": 32.0, "height": 104.0, "thickness": 2.4, "lobes": 7.0,
    "amplitude": 0.18, "segments": 72.0, "layers": 40.0,
}


class ValidationError(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def finite(value: object, label: str) -> float:
    require(type(value) in {int, float} and math.isfinite(value),
            label + " must be a finite number")
    return float(value)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def reject_constant(_value: str) -> None:
    raise ValidationError("non-finite JSON constant")


def parse_json(data: bytes) -> dict:
    require(len(data) <= MAX_JSON_BYTES, "JSON exceeds validation size limit")
    try:
        result = json.loads(data, object_pairs_hook=unique_object,
                            parse_constant=reject_constant)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValidationError("invalid UTF-8 JSON document") from error
    require(isinstance(result, dict), "JSON document must be an object")
    return result


def read_json(path: Path) -> dict:
    require(path.is_file() and not path.is_symlink(), "expected a regular JSON file")
    with path.open("rb") as stream:
        return parse_json(stream.read(MAX_JSON_BYTES + 1))


def new_output_directory(path: Path) -> Path:
    require(not path.is_symlink(), "output directory must not be a symlink")
    path = path.absolute()
    if path.exists():
        info = path.stat()
        require(stat.S_ISDIR(info.st_mode), "output path is not a directory")
        if hasattr(os, "getuid"):
            require(info.st_uid == os.getuid(), "output directory is owned by another user")
        require(next(path.iterdir(), None) is None, "output directory is not empty")
    else:
        # Do not create or alter an arbitrary chain of parent directories.
        path.mkdir(mode=0o700, parents=False, exist_ok=False)
    return path.resolve(strict=True)


def unused(path: Path) -> None:
    require(not path.exists() and not path.is_symlink(),
            "refusing to replace an existing output: " + path.name)


def run_cli(cli: Path, output_dir: Path, command: str, arguments: list[Path],
            commands: list[dict]) -> dict:
    record = {
        "command": command, "arguments": [str(path) for path in arguments],
        "timeout_seconds": TIMEOUT_SECONDS, "passed": False,
    }
    commands.append(record)
    started = time.perf_counter()
    try:
        process = subprocess.run(
            [str(cli), command, *(str(path) for path in arguments)],
            cwd=output_dir, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            timeout=TIMEOUT_SECONDS, check=False,
        )
    except subprocess.TimeoutExpired as error:
        record["elapsed_seconds"] = time.perf_counter() - started
        record["timed_out"] = True
        record["stderr_tail"] = (error.stderr or b"")[-4096:].decode("utf-8", errors="replace")
        raise ValidationError(command + " exceeded the 30-second timeout") from error
    record["elapsed_seconds"] = time.perf_counter() - started
    record["exit_code"] = process.returncode
    record["stdout_bytes"] = len(process.stdout)
    record["stderr_tail"] = process.stderr[-4096:].decode("utf-8", errors="replace")
    response = parse_json(process.stdout)
    record["response"] = response
    require(process.returncode == 0, command + " returned a nonzero exit code")
    require(response.get("ok") is True, command + " did not return JSON ok=true")
    require(response.get("command") == command, "CLI command response mismatch")
    require(response.get("cli_version") == 1 and response.get("api_version") == 1,
            "unexpected CLI/API version")
    result = response.get("result")
    require(isinstance(result, dict), "CLI result must be an object")
    summary = result.get("project")
    require(isinstance(summary, dict) and summary.get("node_count") == 3,
            command + " did not report three nodes")
    require(summary.get("schema_version") == 1, "unexpected saved schema version")
    record["passed"] = True
    return result


def verify_evaluation(result: dict) -> None:
    evaluation = result.get("evaluation")
    require(isinstance(evaluation, dict) and evaluation.get("cancelled") is False,
            "evaluation missing or cancelled")
    nodes = evaluation.get("nodes")
    require(isinstance(nodes, list) and len(nodes) == 3, "expected three evaluated nodes")
    by_id = {node.get("node_id"): node for node in nodes if isinstance(node, dict)}
    require(set(by_id) == {0, 1, 2}, "unexpected or duplicate evaluated node IDs")
    for identifier, kind in [(0, "vase"), (1, "twist"), (2, "output")]:
        node = by_id[identifier]
        require(node.get("kind") == kind and node.get("disabled") is False,
                "wrong or disabled evaluated node")
        data = node.get("data")
        require(isinstance(data, dict) and data.get("item_count") == 1
                and data.get("kinds") == {"mesh": 1}, "expected one Mesh per node")
        preview = data.get("preview")
        require(isinstance(preview, list) and len(preview) == 1
                and data.get("preview_truncated") is False, "Mesh preview missing")
        mesh = preview[0]
        require(mesh.get("kind") == "mesh"
                and mesh.get("vertex_count") == EXPECTED_VERTICES
                and mesh.get("triangle_count") == EXPECTED_TRIANGLES,
                "unexpected vertex/triangle counts")


def verify_project(project: dict, radius: float) -> None:
    require(project.get("schema_version") == 1, "unexpected project schema")
    graph = project.get("graph")
    require(isinstance(graph, dict), "saved project has no graph")
    nodes = graph.get("nodes")
    require(isinstance(nodes, list) and len(nodes) == 3, "saved graph must have three nodes")
    by_id = {node.get("id"): node for node in nodes if isinstance(node, dict)}
    require(set(by_id) == {0, 1, 2}, "saved node IDs differ from the fixture")
    for identifier, kind in [(0, "vase"), (1, "twist"), (2, "output")]:
        require(by_id[identifier].get("kind") == kind, "saved node kind mismatch")
    require(by_id[1].get("inputs", {}).get("geometry") == 0
            and by_id[2].get("inputs", {}).get("geometry") == 1,
            "saved vase/twist/output connections differ from the fixture")
    expected = {**EXPECTED_PARAMS, "radius": radius}
    parameters = by_id[0].get("params", {})
    for name, value in expected.items():
        require(finite(parameters.get(name), name) == value,
                "saved vase parameter mismatch: " + name)
    require(finite(by_id[1].get("params", {}).get("angle"), "twist angle") == 110.0,
            "saved twist angle differs from the fixture")


def inspect_stl(path: Path, cli_report: dict) -> dict:
    require(path.is_file() and not path.is_symlink(), "STL is not a regular file")
    require(path.stat().st_size == EXPECTED_STL_BYTES, "unexpected binary STL file size")
    with path.open("rb") as stream:
        raw = stream.read(EXPECTED_STL_BYTES + 1)
    require(len(raw) == EXPECTED_STL_BYTES, "binary STL size changed during reading")
    header = raw[:80]
    require(header == EXPECTED_HEADER.ljust(80, b"\0"), "unexpected binary STL header")
    triangle_count = struct.unpack_from("<I", raw, 80)[0]
    require(triangle_count == EXPECTED_TRIANGLES, "wrong STL triangle count")
    require(len(raw) == 84 + 50 * triangle_count, "STL header/count/length disagree")

    minimum = [math.inf] * 3
    maximum = [-math.inf] * 3
    contributions = []
    area_minimum = math.inf
    vertices = set()
    facet = struct.Struct("<12fH")
    for index in range(triangle_count):
        values = facet.unpack_from(raw, 84 + 50 * index)
        require(all(math.isfinite(value) for value in values[:12]),
                "non-finite STL coordinate or normal")
        require(values[12] == 0, "unexpected STL attribute bytes")
        require(math.hypot(*values[:3]) > 0.0, "zero stored STL normal")
        a, b, c = values[3:6], values[6:9], values[9:12]
        for vertex in (a, b, c):
            vertices.add(vertex)
            for axis, value in enumerate(vertex):
                minimum[axis] = min(minimum[axis], value)
                maximum[axis] = max(maximum[axis], value)
        ab = tuple(b[axis] - a[axis] for axis in range(3))
        ac = tuple(c[axis] - a[axis] for axis in range(3))
        normal = (
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        )
        twice_area = math.hypot(*normal)
        require(math.isfinite(twice_area) and twice_area > 0.0,
                "zero-area or invalid STL triangle")
        area_minimum = min(area_minimum, 0.5 * twice_area)
        # Independent origin-based determinant from the exported f32 vertices.
        determinant = math.fsum((
            a[0] * b[1] * c[2], a[1] * b[2] * c[0], a[2] * b[0] * c[1],
            -a[0] * b[2] * c[1], -a[1] * b[0] * c[2], -a[2] * b[1] * c[0],
        ))
        contributions.append(determinant / 6.0)

    require(len(vertices) == EXPECTED_VERTICES, "unexpected unique STL vertex positions")
    volume = math.fsum(contributions)
    require(math.isfinite(volume) and volume > 0.0, "nonpositive signed STL volume")
    require(math.isclose(minimum[2], 0.0, rel_tol=0.0, abs_tol=BOUNDS_ABS_TOL_MM)
            and math.isclose(maximum[2], EXPECTED_HEIGHT_MM, rel_tol=0.0,
                             abs_tol=BOUNDS_ABS_TOL_MM), "STL Z bounds must be 0..104 mm")
    require(cli_report.get("output_node_ids") == [2]
            and cli_report.get("mesh_count") == 1, "STL selection must be Output 2, Mesh 1")
    require(cli_report.get("vertices") == EXPECTED_VERTICES
            and cli_report.get("triangles") == EXPECTED_TRIANGLES,
            "CLI export counts differ from expected mesh")
    require(cli_report.get("format") == "binary_stl"
            and cli_report.get("units") == "millimeters", "unexpected STL format or units")
    require(cli_report.get("skipped_values") == 0
            and cli_report.get("degenerate_triangles") == 0, "CLI omitted data or reported degeneracy")
    require(cli_report.get("printability_verified") is False,
            "CLI must not claim physical printability")
    reported_volume = finite(cli_report.get("signed_volume_mm3"), "CLI signed volume")
    require(math.isclose(volume, reported_volume,
                         rel_tol=VOLUME_REL_TOL, abs_tol=VOLUME_ABS_TOL_MM3),
            "independent STL signed volume disagrees with CLI report")
    bounds = cli_report.get("bounds")
    require(isinstance(bounds, list) and len(bounds) == 2
            and all(isinstance(point, list) and len(point) == 3 for point in bounds),
            "CLI bounds have the wrong shape")
    for expected, reported in zip((minimum, maximum), bounds):
        for expected_value, reported_value in zip(expected, reported):
            require(math.isclose(expected_value, finite(reported_value, "CLI bound"),
                                 rel_tol=1e-6, abs_tol=BOUNDS_ABS_TOL_MM),
                    "independent STL bounds disagree with CLI report")
    return {
        "file": path.name, "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw),
        "header_ascii": header.rstrip(b"\0").decode("ascii"),
        "triangle_count": triangle_count,
        "unique_exported_vertex_positions": len(vertices),
        "bounds_mm": [minimum, maximum],
        "height_mm": maximum[2] - minimum[2],
        "half_width_x_mm": (maximum[0] - minimum[0]) / 2.0,
        "half_width_y_mm": (maximum[1] - minimum[1]) / 2.0,
        "minimum_triangle_area_mm2": area_minimum,
        "all_coordinates_and_normals_finite": True,
        "all_triangle_areas_nonzero": True,
        "signed_volume_mm3": volume, "cli_signed_volume_mm3": reported_volume,
        "volume_absolute_difference_mm3": abs(volume - reported_volume),
        "volume_relative_tolerance": VOLUME_REL_TOL,
        "volume_absolute_tolerance_mm3": VOLUME_ABS_TOL_MM3,
        "volume_comparison_passed": True,
        "topology_diagnostics_from_cli": {
            key: cli_report.get(key) for key in (
                "open_edges", "non_manifold_edges", "inconsistent_edges",
                "degenerate_triangles", "topologically_closed")
        },
        "independent_topology_verification_performed": False,
        "printability_verified": False,
    }


def validate_workflow(cli: Path, request: Path, directory: Path, report: dict) -> None:
    baseline = directory / "vase-radius-32.morph"
    baseline_stl = directory / "vase-radius-32.stl"
    edited = directory / "vase-radius-36.morph"
    edited_stl = directory / "vase-radius-36.stl"
    edit_request = directory / "radius-36-request.json"
    commands = report["commands"]

    unused(baseline)
    verify_evaluation(run_cli(cli, directory, "build", [request, baseline], commands))
    original_bytes = baseline.read_bytes()
    original = read_json(baseline)
    verify_project(original, 32.0)
    validated = run_cli(cli, directory, "validate", [baseline], commands)
    require(validated.get("evaluation_performed") is False, "validate unexpectedly evaluated")
    verify_evaluation(run_cli(cli, directory, "evaluate", [baseline], commands))
    unused(baseline_stl)
    export = run_cli(cli, directory, "export-stl", [baseline, baseline_stl], commands)
    report["baseline"] = inspect_stl(baseline_stl, export)
    report["baseline"]["saved_radius_mm"] = 32.0
    report["baseline"]["project_sha256"] = hashlib.sha256(original_bytes).hexdigest()

    with edit_request.open("x", encoding="utf-8") as stream:
        json.dump({"api_version": 1, "edits": [
            {"op": "set", "node": "0", "input": "radius", "value": 36}
        ]}, stream, indent=2)
        stream.write("\n")
    unused(edited)
    verify_evaluation(run_cli(cli, directory, "apply",
                              [baseline, edit_request, edited], commands))
    updated = read_json(edited)
    verify_project(updated, 36.0)
    expected = copy.deepcopy(original)
    next(node for node in expected["graph"]["nodes"] if node["id"] == 0)["params"]["radius"] = 36
    require(updated == expected, "apply changed fields other than node 0 radius")
    require(baseline.read_bytes() == original_bytes, "apply changed the source project")
    validated = run_cli(cli, directory, "validate", [edited], commands)
    require(validated.get("evaluation_performed") is False, "validate unexpectedly evaluated")
    verify_evaluation(run_cli(cli, directory, "evaluate", [edited], commands))
    unused(edited_stl)
    export = run_cli(cli, directory, "export-stl", [edited, edited_stl], commands)
    report["edited"] = inspect_stl(edited_stl, export)
    report["edited"]["saved_radius_mm"] = 36.0
    report["edited"]["project_sha256"] = sha256_file(edited)
    require(report["baseline"]["sha256"] != report["edited"]["sha256"],
            "radius edit did not change exported STL bytes")
    require(not math.isclose(report["baseline"]["half_width_x_mm"],
                             report["edited"]["half_width_x_mm"],
                             rel_tol=1e-6, abs_tol=BOUNDS_ABS_TOL_MM),
            "radius edit did not change the measured width")
    require(not math.isclose(report["baseline"]["signed_volume_mm3"],
                             report["edited"]["signed_volume_mm3"],
                             rel_tol=VOLUME_REL_TOL, abs_tol=VOLUME_ABS_TOL_MM3),
            "radius edit did not change measured volume")
    require(baseline.read_bytes() == original_bytes, "a later command changed the source project")
    report["saved_radius_change_verified"] = [32.0, 36.0]
    report["all_other_saved_project_fields_preserved"] = True
    report["source_project_bytes_preserved"] = True
    report["geometry_changed_after_edit"] = True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--source-revision", default="unknown",
                        help="Caller-provided source revision; not independently verified")
    parser.add_argument("--source-context", default="unknown",
                        help="Caller-provided build/CI provenance; not inferred from local Git")
    args = parser.parse_args()
    try:
        cli = args.cli.resolve(strict=True)
        request = args.request.resolve(strict=True)
        require(cli.is_file() and request.is_file(), "CLI and request must be regular files")
        directory = new_output_directory(args.output_dir)
    except (OSError, ValidationError) as error:
        print("CLI vase validation setup failed: " + str(error), file=sys.stderr)
        return 2
    report = {
        "schema_version": 1, "fixture": "vase -> twist -> Output 2, radius 32 -> 36",
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "ok": False, "printability_verified": False,
        "provenance": {
            "cli_path": str(cli), "cli_sha256": sha256_file(cli),
            "request_path": str(request), "request_sha256": sha256_file(request),
            "validator_sha256": sha256_file(Path(__file__)),
            "source_revision": args.source_revision,
            "source_context": args.source_context,
            "source_attribution": ("unknown" if args.source_revision == "unknown"
                                   and args.source_context == "unknown"
                                   else "caller_provided_not_independently_verified"),
        },
        "environment": {
            "platform": platform.platform(), "system": platform.system(),
            "release": platform.release(), "machine": platform.machine(),
            "python_version": platform.python_version(),
            "execution_context": "Local real CLI subprocesses; no GUI or printer hardware test",
        },
        "expected": {
            "nodes": 3, "output_node_id": 2, "mesh_count": 1,
            "vertices": EXPECTED_VERTICES, "triangles": EXPECTED_TRIANGLES,
            "height_mm": EXPECTED_HEIGHT_MM, "stl_bytes": EXPECTED_STL_BYTES,
            "bounds_absolute_tolerance_mm": BOUNDS_ABS_TOL_MM,
            "volume_relative_tolerance": VOLUME_REL_TOL,
            "volume_absolute_tolerance_mm3": VOLUME_ABS_TOL_MM3,
        },
        "limitations": [
            "STL coordinates are f32; CLI geometry and volume use the source mesh.",
            "The independent check covers finite data, nonzero faces, bounds and determinant volume.",
            "Topology, self-intersection, wall thickness and printability are not independently certified.",
            "Metadata preservation is checked for every field of this actual build-produced project.",
        ],
        "commands": [],
    }
    report_path = directory / "cli-vase-report.json"
    try:
        # Reserve a new report before running commands; never overwrite an old result.
        with report_path.open("x", encoding="utf-8") as report_stream:
            try:
                validate_workflow(cli, request, directory, report)
                require(sha256_file(cli) == report["provenance"]["cli_sha256"],
                        "CLI binary changed during validation")
                require(sha256_file(request) == report["provenance"]["request_sha256"],
                        "input request changed during validation")
                report["ok"] = True
            except (ValidationError, OSError, KeyError, TypeError, StopIteration,
                    struct.error, OverflowError) as error:
                report["error"] = {"type": type(error).__name__, "message": str(error)}
            report["finished_utc"] = datetime.now(timezone.utc).isoformat()
            json.dump(report, report_stream, ensure_ascii=True, indent=2, allow_nan=False)
            report_stream.write("\n")
    except OSError as error:
        print("Cannot save CLI vase validation report: " + str(error), file=sys.stderr)
        return 1
    print(json.dumps({"ok": report["ok"], "report": str(report_path),
                      "commands_completed": sum(item["passed"] for item in report["commands"]),
                      "printability_verified": False}, ensure_ascii=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
