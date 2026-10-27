"""Production packages must refuse both WebDriver and the scripted updater."""
import importlib.util
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location("package_windows", Path(__file__).parents[1] / "package_windows.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ProductionAppTests(unittest.TestCase):
    def test_plain_app_is_accepted(self):
        package.verify_production_app(b"MZproduction", "touchdeck v0.1.0")

    def test_webdriver_dependency_or_binary_is_rejected(self):
        for binary, graph in [(b"MZ", "tauri-plugin-wdio-webdriver"), (b"MZtauri_plugin_wdio_webdriver", "touchdeck")]:
            with self.subTest(binary=binary, graph=graph), self.assertRaisesRegex(ValueError, "WebDriver"):
                package.verify_production_app(binary, graph)

    def test_scripted_updater_marker_or_command_is_rejected(self):
        for marker in ("touchdeck-scripted-app-updater-e2e", "e2e_app_update"):
            for encoding in ("utf-8", "utf-16-le"):
                with self.subTest(marker=marker, encoding=encoding), self.assertRaisesRegex(ValueError, "Scripted app updater"):
                    package.verify_production_app(b"MZ" + marker.encode(encoding), "touchdeck")


if __name__ == "__main__":
    unittest.main()
