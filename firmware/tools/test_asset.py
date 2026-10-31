import ctypes
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from test_gif import gif

ROOT = Path(__file__).resolve().parents[1]


def load(path):
    lib = ctypes.CDLL(str(path))
    lib.td_asset_upload_begin.argtypes = [ctypes.c_char_p, ctypes.c_uint32, ctypes.c_int]
    lib.td_asset_upload_write.argtypes = [ctypes.c_char_p, ctypes.c_size_t]
    lib.td_asset_present.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_uint32), ctypes.POINTER(ctypes.c_int)]
    lib.td_asset_present.restype = ctypes.c_bool
    lib.td_asset_result_reason.restype = ctypes.c_char_p
    return lib


def upload(lib, data, fmt=2):
    key = hashlib.sha256(data).hexdigest().encode()
    result = lib.td_asset_upload_begin(key, len(data), fmt)
    if result == 0:
        for at in range(0, len(data), 1021):
            block = data[at:at+1021]
            result = lib.td_asset_upload_write(block, len(block))
            if result:
                break
        if result == 0:
            result = lib.td_asset_upload_finish()
    return key, result


class AssetTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT"))
        cls.dir = Path(cls.tmp.name).resolve()
        cls.dll = cls.dir / ("asset.dll" if os.name == "nt" else "asset.so")
        host = ROOT / "tools/host"
        includes = [host / "include", ROOT / "components/td_store/include", ROOT / "components/td_update/include"]
        sources = [ROOT / "components/td_store/td_asset.c", ROOT / "components/td_store/td_gif.c",
                   ROOT / "components/td_update/td_update_state.c", host / "asset_host.c"]
        exports = ["td_asset_upload_begin", "td_asset_upload_write", "td_asset_upload_finish",
                   "td_asset_upload_abort", "td_asset_present", "td_asset_delete", "td_asset_result_reason", "host_interrupt"]
        if os.name == "nt":
            command = ["cl", "/nologo", "/LD", "/std:clatest", "/W3", "/WX", "/D_CRT_SECURE_NO_WARNINGS",
                       '/DTD_ASSET_DIR="assets"', "/FI" + str(host / "include/compat.h")]
            command += ["/I" + str(p) for p in includes] + [str(p) for p in sources]
            command += ["/link", "/OUT:" + str(cls.dll)] + ["/EXPORT:" + f for f in exports]
        else:
            command = [os.environ.get("CC", "cc"), "-shared", "-fPIC", "-O2", "-Wall", "-Wextra", "-Werror",
                       '-DTD_ASSET_DIR="assets"', "-include", str(host / "include/compat.h")]
            command += ["-I" + str(p) for p in includes] + [str(p) for p in sources] + ["-o", str(cls.dll)]
        subprocess.run(command, cwd=cls.dir, check=True)
        cls.lib = load(cls.dll)
        cls.cwd = Path.cwd()
        os.chdir(cls.dir)

    @classmethod
    def tearDownClass(cls):
        os.chdir(cls.cwd)
        if os.name == "nt":
            import _ctypes
            _ctypes.FreeLibrary(cls.lib._handle)
        cls.tmp.cleanup()

    def test_commit_and_delete(self):
        key, result = upload(self.lib, gif())
        self.assertEqual(result, 0)
        size, fmt = ctypes.c_uint32(), ctypes.c_int()
        self.assertTrue(self.lib.td_asset_present(key, ctypes.byref(size), ctypes.byref(fmt)))
        self.assertEqual((size.value, fmt.value), (len(gif()), 2))
        deleted = ctypes.c_bool()
        self.assertEqual(self.lib.td_asset_delete(key, ctypes.byref(deleted)), 0)
        self.assertTrue(deleted.value)
        self.assertFalse(self.lib.td_asset_present(key, None, None))

    def test_static_asset_remains_static(self):
        import struct
        data = b"TDI1" + struct.pack("<HHHH", 64, 64, 1, 0) + bytes(12288)
        key, result = upload(self.lib, data, fmt=1)
        self.assertEqual(result, 0)
        size, fmt = ctypes.c_uint32(), ctypes.c_int()
        self.assertTrue(self.lib.td_asset_present(key, ctypes.byref(size), ctypes.byref(fmt)))
        self.assertEqual((size.value, fmt.value), (12300, 1))

    def test_hostile_commit_preserves_previous_assets(self):
        good, result = upload(self.lib, gif())
        self.assertEqual(result, 0)
        before = {p: p.read_bytes() for p in Path("assets").glob("*/*.bin")}
        for data in [gif()[:-1], gif(width=129), gif(frames=61), gif(codes=[4, 0] * 1000 + [5])]:
            key, result = upload(self.lib, data)
            self.assertEqual(self.lib.td_asset_result_reason(result),
                             b"size" if data in [gif(width=129), gif(frames=61)] else b"format")
            self.assertFalse(self.lib.td_asset_present(key, None, None))
            self.assertFalse(Path("assets/upload.tmp").exists())
            self.assertEqual({p: p.read_bytes() for p in Path("assets").glob("*/*.bin")}, before)
        self.assertTrue(self.lib.td_asset_present(good, None, None))

    def test_process_interrupted_during_commit(self):
        data = gif(frames=60)
        key = hashlib.sha256(data).hexdigest().encode()
        before = {p: p.read_bytes() for p in Path("assets").glob("*/*.bin")}
        result = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--interrupt", str(self.dll)])
        self.assertEqual(result.returncode, 94)
        self.assertFalse(self.lib.td_asset_present(key, None, None))
        self.assertEqual({p: p.read_bytes() for p in Path("assets").glob("*/*.bin")}, before)
        self.assertTrue(Path("assets/upload.tmp").exists())
        Path("assets/upload.tmp").unlink()
        self.assertEqual(upload(self.lib, data)[1], 0)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--interrupt":
        lib = load(sys.argv[2])
        lib.host_interrupt(1)
        upload(lib, gif(frames=60))
        sys.exit(1)
    unittest.main()
