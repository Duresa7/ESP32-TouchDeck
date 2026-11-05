import base64
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_windows", Path(__file__).parents[1] / "package_windows.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def unwrap(text):
    return base64.b64decode(text, validate=True).decode("utf-8").split("\n")


@unittest.skipUnless(shutil.which("node"), "node required")
class DisposableSigningTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.dir = Path(temp.name)
        self.key = self.dir / "disposable-test-signing.key"
        self.env = {k: v for k, v in os.environ.items() if not k.upper().startswith("TAURI_")}
        self.env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = ""

    def test_generated_key_is_an_empty_password_minisign_key(self):
        public = unwrap(package.generate_test_key(self.key))
        raw = base64.b64decode(unwrap(self.key.read_text(encoding="utf-8"))[1], validate=True)
        self.assertEqual(len(raw), 158)
        self.assertEqual(raw[:6], b"EdScB2")
        self.assertEqual(int.from_bytes(raw[38:46], "little"), 1_048_576)
        self.assertEqual(int.from_bytes(raw[46:54], "little"), 33_554_432)
        stream = hashlib.scrypt(b"", salt=raw[6:38], n=32768, r=8, p=1, maxmem=64 * 1024 * 1024, dklen=104)
        plain = bytes(a ^ b for a, b in zip(raw[54:], stream))
        key_id, secret, checksum = plain[:8], plain[8:72], plain[72:]
        self.assertEqual(hashlib.blake2b(b"Ed" + key_id + secret, digest_size=32).digest(), checksum)
        self.assertEqual(public[0], f"untrusted comment: minisign public key: {key_id[::-1].hex().upper()}")
        self.assertEqual(base64.b64decode(public[1]), b"Ed" + key_id + secret[32:])
        with self.assertRaises(package.subprocess.CalledProcessError):
            package.generate_test_key(self.key)

    def test_sign_binds_file_and_version_to_the_generated_key(self):
        public = base64.b64decode(unwrap(package.generate_test_key(self.key))[1])
        installer = self.dir / "OpenTouchDeck-windows-x64-TEST-ONLY-setup.exe"
        installer.write_bytes(b"synthetic installer")
        lines = unwrap(package.sign(installer, "1.2.3", self.env, self.key))
        self.assertEqual(lines[0], "untrusted comment: signature from tauri secret key")
        packet = base64.b64decode(lines[1], validate=True)
        self.assertEqual((packet[:2], packet[2:10], len(packet)), (b"ED", public[2:10], 74))
        self.assertRegex(lines[2], r"^trusted comment: timestamp:\d+\tfile:OpenTouchDeck-windows-x64-TEST-ONLY-setup\.exe\tversion:1\.2\.3$")
        self.assertEqual(len(base64.b64decode(lines[3], validate=True)), 64)
        self.assertEqual(lines[4], "")

    def test_sign_fails_on_a_wrong_password(self):
        package.generate_test_key(self.key)
        installer = self.dir / "setup.exe"
        installer.write_bytes(b"synthetic installer")
        with self.assertRaises(package.subprocess.CalledProcessError):
            package.sign(installer, "1.2.3", dict(self.env, TAURI_SIGNING_PRIVATE_KEY_PASSWORD="wrong"), self.key)
        self.assertFalse(installer.with_name("setup.exe.sig").exists())


if __name__ == "__main__":
    unittest.main()
