import importlib.util
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location("package_windows", Path(__file__).parents[1] / "package_windows.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ProductionCompanionTests(unittest.TestCase):
    def test_plain_companion_is_accepted(self):
        package.verify_production_companion(b"MZ/api/v1/system/shutdown")

    def test_test_hook_routes_or_arguments_are_rejected(self):
        for marker in (b"/test/pairing-delivery", b"/test/devices/", b"--emit-ts"):
            with self.subTest(marker=marker), self.assertRaisesRegex(ValueError, "Test hooks"):
                package.verify_production_companion(b"MZ" + marker)

    def test_non_executable_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "Windows executable"):
            package.verify_production_companion(b"ELF")

    def test_lifecycle_update_is_the_next_patch(self):
        self.assertEqual(package.next_patch("0.2.9"), "0.2.10")


if __name__ == "__main__":
    unittest.main()
