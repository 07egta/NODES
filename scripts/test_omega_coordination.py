"""Offline regression tests for the read-only coordination helper."""
import datetime as dt
import json
import unittest
from unittest.mock import patch
import omega_coordination as oc

def example():
    return dict(version=1, mission="OMEGA 49", task="audit", base_sha="a"*40,
                priority="P1", state="active", issues=[42], depends_on=[],
                scope=["src/mesh/"], checked_at="2026-11-28T05:07:00Z",
                verified_sha=None, handoff="resume")

def body(c):
    return "```nodes-coordination\n"+json.dumps(c)+"\n```"

def row(n, files, c=None):
    return dict(number=n, files=files, claim=c)

class CoordinationTests(unittest.TestCase):
    def test_claim(self):
        c = example()
        self.assertEqual(oc.claim(body(c)), (c, None))
        self.assertIsNone(oc.claim(body(c)+"\n"+body(c))[0])
        for k,v in [("scope", ["../src"]), ("scope", ["src/*"]), ("issues", [True]),
                    ("checked_at", "2026-11-28T05:07:00"), ("base_sha", "abc"),
                    ("version", 2), ("priority", "urgent")]:
            with self.subTest(key=k):
                self.assertIsNone(oc.claim(body(dict(c, **{k:v})))[0])

    def test_actual_overlap_even_without_claim(self):
        result = oc.conflicts([row(1, ["src/a.rs"]), row(2, ["src/a.rs"])])
        self.assertEqual(result[0]["files"], ["src/a.rs"])
        self.assertFalse(result[0]["declared_dependency"])

    def test_planned_and_actual(self):
        c = example()
        result = oc.conflicts([row(1, [], c), row(2, ["src/mesh/a.rs"])])
        self.assertEqual(result[0]["scope_overlap"], [("src/mesh/", "src/mesh/a.rs")])
        self.assertEqual(oc.conflicts([row(1, [], c), row(2, ["src/mesh_other/a.rs"])]), [])

    def test_dependency_and_issue(self):
        c = example()
        d = dict(c, depends_on=[1], scope=["other.rs"])
        result = oc.conflicts([row(1, [], c), row(2, [], d)])
        self.assertEqual(result[0]["issues"], [42])
        self.assertTrue(result[0]["declared_dependency"])

    def test_changed_main_incomplete(self):
        with patch.object(oc, "api", side_effect=[
                {"object":{"sha":"a"*40}}, [], [], {"object":{"sha":"b"*40}}, []]):
            self.assertFalse(oc.snapshot("owner/repo")["complete"])

    def test_empty_stable_snapshot(self):
        ref = {"object":{"sha":"a"*40}}
        with patch.object(oc, "api", side_effect=[ref, [], [], ref, []]):
            result = oc.snapshot("owner/repo")
            self.assertTrue(result["complete"])
            self.assertEqual(result["overlaps"], [])

if __name__ == "__main__":
    unittest.main()
