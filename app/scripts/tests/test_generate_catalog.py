"""Offline catalog generation contract."""
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("catalog", Path(__file__).parents[1] / "generate_catalog.py")
catalog = importlib.util.module_from_spec(spec)
spec.loader.exec_module(catalog)

class CatalogTest(unittest.TestCase):
    def test_deterministic_metadata_and_order(self):
        simple = [dict(title="Zed", slug="zed", hex="123456", source="https://example.org", aliases={"aka": ["Z", "Z"]}, license={"type": "CC-BY-NC-4.0"})]
        selfhst = [dict(Name="Alpha", Reference="alpha", Tags="server, home", Category="Tools", SVG="No", Light="Yes", Dark="No")]
        first = catalog.generate(simple, selfhst)
        self.assertEqual(first, catalog.generate(simple, selfhst))
        data = json.loads(first)
        self.assertEqual(data["simple_icons_version"], catalog.SIMPLE)
        self.assertEqual(data["selfhst_commit"], catalog.SELFHST)
        a, z = data["entries"]
        self.assertEqual(a["source"], "selfhst")
        self.assertEqual(a["tags"], ["home", "server"])
        self.assertEqual(a["license"], "CC-BY-4.0")
        self.assertNotIn("svg", a)
        self.assertEqual(z["aliases"], ["Z"])
        self.assertEqual(z["license"], "CC-BY-NC-4.0")
        self.assertNotIn(b"\r", first)
