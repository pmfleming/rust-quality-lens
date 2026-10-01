import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

from check import EXPECTED, check
from profile import documents


class AcceptanceGate(unittest.TestCase):
    def test_missing_criteria_cannot_release_even_with_optimization(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory).resolve()
            (root / "acceptance").mkdir()
            (root / "acceptance/criteria.json").write_text(json.dumps({"version": 1, "criteria": []}))
            with self.assertRaisesRegex(ValueError, "missing required criteria"):
                check(root, release=True)
            script = pathlib.Path(__file__).with_name("check.py")
            result = subprocess.run([sys.executable, "-O", "-c", "import sys,pathlib;sys.path.insert(0,sys.argv[1]);from check import check;check(pathlib.Path(sys.argv[2]),True)", str(script.parent), str(root)], capture_output=True, text=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing required criteria", result.stderr)

    def test_native_profile_documents_must_be_complete_json(self):
        self.assertEqual(list(documents(' {"results": []}\n{"status":"complete"}\n')), [{"results": []}, {"status": "complete"}])
        with self.assertRaises(json.JSONDecodeError):
            list(documents('{"results":[]} trailing-corruption'))
        self.assertEqual(len(EXPECTED), 25)


if __name__ == "__main__":
    unittest.main()
