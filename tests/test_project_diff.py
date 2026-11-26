import copy
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "scripts"))
import project_diff as diff


def project():
    return {"schema_version": 1, "name": "example", "graph": {"next_id": 2, "nodes": [
        {"id": 0, "kind": "sphere", "name": "球", "position": [0, 0],
         "params": {"radius": 2}, "inputs": {}, "preview": True},
        {"id": 1, "kind": "output", "name": "出力", "position": [1, 0],
         "params": {}, "inputs": {"geometry": 0}, "preview": True},
    ]}}


class ProjectDiffTests(unittest.TestCase):
    def test_numeric_spelling_and_primary_wire(self):
        old = project()
        new = copy.deepcopy(old)
        new["graph"]["nodes"][0]["params"]["radius"] = 2.0
        node = new["graph"]["nodes"][1]
        node["inputs"] = {}
        node["port_links"] = {"geometry": [{"node": 0, "port": 0}]}
        self.assertFalse(diff.semantic_diff(old, new)["changed"])

    def test_parameter_change(self):
        old = project()
        new = copy.deepcopy(old)
        new["graph"]["nodes"][0]["params"]["radius"] = 3
        self.assertEqual(diff.semantic_diff(old, new)["changes"], [
            {"scope": "node", "field": ["params", "radius"], "node_id": 0,
             "op": "change", "before": 2, "after": 3}])

    def test_ordered_multiport_links(self):
        old = project()
        node = old["graph"]["nodes"][1]
        node["inputs"] = {}
        node["port_links"] = {"geometry": [{"node": 0, "port": 0}, {"node": 0, "port": 1}]}
        new = copy.deepcopy(old)
        new["graph"]["nodes"][1]["port_links"]["geometry"].reverse()
        self.assertEqual(diff.semantic_diff(old, new)["changes"][0]["field"],
                         ["wires", "geometry"])

    def test_layout_only(self):
        old = project()
        new = copy.deepcopy(old)
        new["graph"]["nodes"][0]["position"] = [5, 6]
        new["graph"]["nodes"].reverse()
        new["zoom"] = 2
        self.assertEqual(diff.semantic_diff(old, new)["change_count"], 3)
        self.assertFalse(diff.semantic_diff(old, new, True)["changed"])

    def test_node_add_remove(self):
        old = project()
        new = copy.deepcopy(old)
        new["graph"]["nodes"].pop()
        changes = diff.semantic_diff(old, new)["changes"]
        self.assertEqual(len(changes), 1)
        self.assertEqual(changes[0]["op"], "remove")
        self.assertEqual(diff.semantic_diff(new, old)["changes"][0]["op"], "add")

    def test_duplicate_ids(self):
        old = project()
        old["graph"]["nodes"].append(copy.deepcopy(old["graph"]["nodes"][0]))
        with self.assertRaises(ValueError):
            diff.semantic_diff(old, old)

    def test_ambiguous_wires(self):
        old = project()
        old["graph"]["nodes"][1]["port_links"] = {"geometry": [{"node": 0, "port": 0}]}
        with self.assertRaises(ValueError):
            diff.semantic_diff(old, old)

    def test_unknown_field_and_boolean(self):
        old = project()
        new = copy.deepcopy(old)
        old["graph"]["nodes"][0]["future"] = True
        new["graph"]["nodes"][0]["future"] = 1
        self.assertTrue(diff.semantic_diff(old, new)["changed"])

    def test_invalid_json(self):
        with tempfile.TemporaryDirectory() as directory:
            p = pathlib.Path(directory) / "bad.morph"
            for raw in ('{"x": 1, "x": 2}', '{"x": NaN}', '{"x": 1e999}'):
                p.write_text(raw, encoding="utf-8")
                with self.assertRaises(ValueError):
                    diff.read_project(p)

    def test_cli_status(self):
        with tempfile.TemporaryDirectory() as directory:
            a = pathlib.Path(directory) / "a.morph"
            b = pathlib.Path(directory) / "b.morph"
            a.write_text(json.dumps(project()), encoding="utf-8")
            b.write_text(json.dumps(project()), encoding="utf-8")
            script = pathlib.Path(diff.__file__)
            def run():
                return subprocess.run([sys.executable, str(script), str(a), str(b)],
                                      capture_output=True, text=True)
            self.assertEqual(run().returncode, 0)
            changed = project()
            changed["name"] = "different"
            b.write_text(json.dumps(changed), encoding="utf-8")
            result = run()
            self.assertEqual(result.returncode, 1)
            self.assertEqual(json.loads(result.stdout)["change_count"], 1)
            b.write_text("invalid", encoding="utf-8")
            self.assertEqual(run().returncode, 2)


if __name__ == "__main__":
    unittest.main()
