#!/usr/bin/env python3
"""Read-only semantic diff of NODES v1 .morph projects (stdlib only)."""
import argparse
import json
import math
import stat
import sys
from pathlib import Path

MAX_BYTES = 8 * 1024 * 1024
MISSING = object()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key!r}")
        result[key] = value
    return result


def finite_float(text):
    value = float(text)
    if not math.isfinite(value):
        raise ValueError("non-finite JSON number")
    return value


def reject_constant(text):
    raise ValueError(f"invalid JSON constant: {text}")


def object_value(value, label):
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    return value


def uint(value, bits, label):
    if type(value) is not int or not 0 <= value < 2 ** bits:
        raise ValueError(f"{label} must be an unsigned {bits}-bit integer")
    return value


def read_project(path):
    path = Path(path)
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError("project must be a regular file")
    with path.open("rb") as stream:
        raw = stream.read(MAX_BYTES + 1)
    if len(raw) > MAX_BYTES:
        raise ValueError("project exceeds 8 MiB")
    return json.loads(
        raw.decode("utf-8"),
        object_pairs_hook=unique_object,
        parse_float=finite_float,
        parse_constant=reject_constant,
    )


def normalize_node(node):
    node = dict(object_value(node, "node"))
    node_id = uint(node.pop("id", None), 64, "node.id")
    if not isinstance(node.get("kind"), str):
        raise ValueError(f"node {node_id}: kind must be a string")
    params = object_value(node.get("params"), "node.params")
    node["params"] = params
    legacy = object_value(node.pop("inputs", None), "node.inputs")
    explicit = object_value(node.pop("port_links", {}), "node.port_links")
    wires = {}
    for key, source in legacy.items():
        wires[key] = [{"node": uint(source, 64, "input source"), "port": 0}]
    for key, sources in explicit.items():
        if key in wires:
            raise ValueError(f"node {node_id}: ambiguous input {key!r}")
        if not isinstance(sources, list) or not sources:
            raise ValueError(f"node {node_id}: invalid port links for {key!r}")
        for source in sources:
            source = object_value(source, "port reference")
            if set(source) != {"node", "port"}:
                raise ValueError("port reference requires node and port")
            uint(source["node"], 64, "source node")
            uint(source["port"], 32, "source port")
        wires[key] = sources
    node["wires"] = wires
    # Missing optional fields are equivalent to the Rust serde defaults.
    node.setdefault("script", "")
    node.setdefault("cluster", None)
    node.setdefault("off", False)
    layout = dict(object_value(node.get("port_layout", {}), "port_layout"))
    for key, default in {
        "input_order": [], "output_order": [], "input_labels": {},
        "output_labels": {}, "next_input": 0, "next_output": 0,
    }.items():
        layout.setdefault(key, default)
    node["port_layout"] = layout
    return node_id, node


def normalize_project(project):
    project = dict(object_value(project, "project"))
    if type(project.get("schema_version")) is not int or project["schema_version"] != 1:
        raise ValueError("only NODES schema_version 1 is supported")
    graph = dict(object_value(project.pop("graph", None), "graph"))
    raw_nodes = graph.pop("nodes", None)
    if not isinstance(raw_nodes, list):
        raise ValueError("graph.nodes must be an array")
    nodes = {}
    for raw in raw_nodes:
        node_id, node = normalize_node(raw)
        if node_id in nodes:
            raise ValueError(f"duplicate node ID: {node_id}")
        nodes[node_id] = node
    for key in ("clusters", "groups", "notes"):
        graph.setdefault(key, [])
    project.setdefault("pan", [0, 0])
    project.setdefault("zoom", 1)
    return project, graph, nodes


def equal(a, b):
    # JSON booleans are not numbers; integer/float spellings are equivalent.
    if type(a) is bool or type(b) is bool:
        return type(a) is type(b) and a == b
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(equal(a[k], b[k]) for k in a)
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(equal(x, y) for x, y in zip(a, b))
    return a == b


def semantic_diff(before, after, ignore_layout=False):
    old_project, old_graph, old_nodes = normalize_project(before)
    new_project, new_graph, new_nodes = normalize_project(after)
    changes = []

    def record(scope, field, old=MISSING, new=MISSING, node_id=None):
        if old is not MISSING and new is not MISSING and equal(old, new):
            return
        change = {"scope": scope, "field": field}
        if node_id is not None:
            change["node_id"] = node_id
        change["op"] = ("add" if old is MISSING else
                        "remove" if new is MISSING else "change")
        if old is not MISSING:
            change["before"] = old
        if new is not MISSING:
            change["after"] = new
        changes.append(change)

    def fields(scope, old, new, skip=(), node_id=None, prefix=()):
        for key in sorted(old.keys() | new.keys()):
            if key not in skip:
                record(scope, list(prefix) + [key], old.get(key, MISSING),
                       new.get(key, MISSING), node_id)

    fields("project", old_project, new_project,
           ("pan", "zoom") if ignore_layout else ())
    fields("graph", old_graph, new_graph)
    common = old_nodes.keys() & new_nodes.keys()
    if not ignore_layout:
        record("graph", ["nodes", "relative_order"],
               [k for k in old_nodes if k in common],
               [k for k in new_nodes if k in common])
    for node_id in sorted(old_nodes.keys() | new_nodes.keys()):
        old = old_nodes.get(node_id, MISSING)
        new = new_nodes.get(node_id, MISSING)
        if old is MISSING or new is MISSING:
            record("node", [], old, new, node_id)
            continue
        skip = {"params", "wires"}
        if ignore_layout:
            skip.add("position")
        fields("node", old, new, skip, node_id)
        for key in ("params", "wires"):
            fields("node", old[key], new[key], node_id=node_id, prefix=(key,))
    return {
        "changed": bool(changes),
        "change_count": len(changes),
        "changes": changes,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", help="original .morph file")
    parser.add_argument("after", help="modified .morph file")
    parser.add_argument("--ignore-layout", action="store_true",
                        help="ignore pan, zoom, node positions and relative canvas order")
    args = parser.parse_args(argv)
    try:
        before = read_project(args.before)
        after = read_project(args.after)
        result = semantic_diff(before, after, args.ignore_layout)
        # ASCII escaping works with redirected output on Windows as well.
        print(json.dumps(result, indent=2, ensure_ascii=True, allow_nan=False))
    except (OSError, ValueError, RecursionError) as error:
        print(f"project diff: {error}", file=sys.stderr)
        return 2
    return int(result["changed"])


if __name__ == "__main__":
    sys.exit(main())
