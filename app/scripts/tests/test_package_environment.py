import importlib.util
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('packaging', Path(__file__).parents[1] / 'package_windows.py')
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class BuildEnvironmentTests(unittest.TestCase):
    def test_build_rejects_ambient_signing_in_every_mode(self):
        for test_signing in (False, True):
            with self.subTest(test_signing=test_signing), patch.object(sys, 'platform', 'win32'), \
                    patch.dict(os.environ, {'tauri_signing_private_key_future': 'fake'}):
                with self.assertRaisesRegex(ValueError, 'must not inherit signing variables'):
                    packaging.build(SimpleNamespace(test_signing=test_signing))

    def test_children_never_receive_signing_variables(self):
        environment = dict(os.environ)
        environment.update(TAURI_SIGNING_PRIVATE_KEY='fake', TAURI_SIGNING_PRIVATE_KEY_PASSWORD='fake',
                           TAURI_SIGNING_PRIVATE_KEY_PATH='fake', tauri_signing_private_key_future='fake',
                           TOUCHDECK_UPDATER_PUBKEY='public')
        command = [sys.executable, '-c', 'import os,json; print(json.dumps(dict(os.environ)))']
        with patch.dict(os.environ, environment, clear=True):
            for env in (None, environment):
                child = json.loads(packaging.capture(command, env=env))
                self.assertFalse(any(k.upper().startswith('TAURI_SIGNING_PRIVATE_KEY') for k in child))
                self.assertEqual(child['TOUCHDECK_UPDATER_PUBKEY'], 'public')
                packaging.run([sys.executable, '-c',
                    'import os; assert not any(k.upper().startswith("TAURI_SIGNING_PRIVATE_KEY") for k in os.environ)'], env=env)


if __name__ == '__main__':
    unittest.main()
