#!/usr/bin/env python3
"""Independently inspect the two small STLs written by the Omega native GUI run.

No NODES geometry implementation is imported. This fixture checks the actual
split-file output, not arbitrary mesh printability or self-intersection.
"""

import collections
import hashlib
import json
import math
from pathlib import Path
import struct


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0])


def inspect(path, expected_bounds):
    data = path.read_bytes()
    assert len(data) >= 84, path
    count = struct.unpack_from("<I", data, 80)[0]
    assert len(data) == 84 + 50 * count, (path, len(data), count)
    assert count == 12, (path, count)
    positions, edges, volumes, areas = set(), collections.defaultdict(list), [], []
    for i in range(count):
        values = struct.unpack_from("<12fH", data, 84 + 50 * i)
        assert all(math.isfinite(x) for x in values[:12]), (path, i)
        assert values[12] == 0, (path, i, "unexpected attribute bytes")
        a, b, c = values[3:6], values[6:9], values[9:12]
        normal = cross(tuple(y - x for x, y in zip(a, b)),
                       tuple(y - x for x, y in zip(a, c)))
        area = math.sqrt(sum(x * x for x in normal)) / 2
        assert area > 0, (path, i, "degenerate triangle")
        assert sum(x * y for x, y in zip(normal, values[:3])) > 0, (path, i)
        areas.append(area)
        volumes.append(sum(x * y for x, y in zip(a, cross(b, c))) / 6)
        positions.update((a, b, c))
        for u, v in ((a, b), (b, c), (c, a)):
            edges[tuple(sorted((u, v)))].append((u, v))
    assert len(positions) == 8, (path, len(positions))
    assert len(edges) == 18, (path, len(edges))
    assert all(len(v) == 2 and v[0] == v[1][::-1] for v in edges.values()), path
    bounds = [[min(p[k] for p in positions) for k in range(3)],
              [max(p[k] for p in positions) for k in range(3)]]
    assert bounds == expected_bounds, (path, bounds)
    volume, area = math.fsum(volumes), math.fsum(areas)
    assert math.isclose(volume, 8000, rel_tol=0, abs_tol=1e-9), (path, volume)
    assert area == 2400, (path, area)
    return {"path": path.name, "bytes": len(data), "triangles": count,
            "unique_positions": len(positions), "edges": len(edges),
            "bounds_mm": bounds, "signed_volume_mm3": volume,
            "surface_area_mm2": area, "closed_oriented_edges": True,
            "sha256": hashlib.sha256(data).hexdigest()}


def main():
    root = Path(__file__).resolve().parents[1] / "docs/development/gui-exports"
    results = [inspect(root / "omega-contact_01.stl", [[-20, -20, 0], [0, 0, 20]]),
               inspect(root / "omega-contact_02.stl", [[0, 0, 20], [20, 20, 40]])]
    print(json.dumps({"files": results, "passed": 2, "failed": 0,
                      "limits": "Fixture-only geometric checks; no physical printing, "
                                "general self-intersection or minimum-wall-thickness proof."},
                     indent=2))


if __name__ == "__main__":
    main()
