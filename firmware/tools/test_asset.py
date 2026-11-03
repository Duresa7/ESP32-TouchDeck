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
    lib.td_asset_gif_canvas.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_uint16), ctypes.POINTER(ctypes.c_uint16)]
    lib.td_asset_gif_canvas.restype = ctypes.c_bool
    lib.td_asset_result_reason.restype = ctypes.c_char_p
    lib.td_asset_read.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)]
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
    littlefs = False

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT"))
        cls.dir = Path(cls.tmp.name).resolve()
        cls.dll = cls.dir / ("asset.dll" if os.name == "nt" else "asset.so")
        host = ROOT / "tools/host"
        includes = [host / "include", host / "heap", ROOT / "components/td_store/include",
                    ROOT / "components/td_update/include"]
        sources = [ROOT / "components/td_store/td_asset.c", ROOT / "components/td_store/td_gif.c",
                   ROOT / "components/td_update/td_update_state.c", host / "asset_host.c", host / "heap/heap_caps_host.c"]
        decoder = ROOT / "managed_components/lvgl__lvgl/src/libs/tjpgd"
        for name in ["tjpgd.c", "tjpgd.h", "tjpgdcnf.h"]:
            text = (decoder / name).read_text().replace("#define JD_FASTDECODE   1", "#define JD_FASTDECODE   0")
            text = text.replace('#include "../../lvgl_public.h"', '#include <stdint.h>\n#define LV_USE_TJPGD 1\n#define LV_STDINT_INCLUDE <stdint.h>')
            if name == "tjpgd.c":
                text = text.replace("unsigned int n, i, ofs;", "unsigned int n, i; size_t ofs;")
            (cls.dir / name).write_text(text, newline="\n")
        shim = cls.dir / "esp32s3/rom"
        shim.mkdir(parents=True)
        (shim / "tjpgd.h").write_text(
            '#include "../../tjpgd.h"\n'
            'typedef size_t UINT;\ntypedef uint8_t BYTE;\n#define TD_JPEG_OUTPUT int\n'
            'extern void host_note_decode(void);\n'
            'static inline JRESULT host_prepare(JDEC *d, size_t (*f)(JDEC *, uint8_t *, size_t), void *p, size_t n, void *c) { '
            'host_note_decode(); JRESULT r = jd_prepare(d, f, p, n, c); return r == JDR_OK && d->ncomp != 3 ? JDR_FMT3 : r; }\n'
            '#define jd_prepare host_prepare\n', newline="\n")
        includes.insert(0, cls.dir)
        sources.append(cls.dir / "tjpgd.c")
        if cls.littlefs:
            lfs = ROOT / "managed_components/joltwallet__littlefs/src/littlefs"
            if not (lfs / "lfs.c").exists():
                raise RuntimeError("Run idf.py -C firmware reconfigure to resolve pinned LittleFS first")
            includes.append(lfs)
            sources[0] = host / "asset_lfs.c"
            for name in ["lfs.c", "lfs_util.c"]:
                source = lfs / name
                if os.name == "nt":
                    wrapper = cls.dir / name
                    wrapper.write_text(
                        '#pragma warning(push)\n'
                        '// LittleFS uses unsigned negation for bit masks and narrows validated type tags.\n'
                        '#pragma warning(disable: 4146 4244)\n'
                        f'#include "{source.as_posix()}"\n'
                        '#pragma warning(pop)\n', newline="\n")
                    source = wrapper
                sources.append(source)
        exports = ["td_asset_upload_begin", "td_asset_upload_write", "td_asset_upload_finish",
                   "td_asset_upload_abort", "td_asset_present", "td_asset_gif_canvas", "td_asset_delete", "td_asset_result_reason", "host_interrupt", "td_asset_pin", "td_asset_unpin", "td_asset_read", "host_delays", "host_decodes"]
        if cls.littlefs:
            exports += ["host_lfs_init", "host_lfs_done", "host_handles", "host_exists", "host_fail_sync", "host_fail_close"]
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

    def test_jpeg_commit_requires_complete_bounded_decode(self):
        from io import BytesIO
        from PIL import Image
        def jpeg(size=(64, 64), mode="RGB", progressive=False):
            out = BytesIO()
            Image.new(mode, size).save(out, format="JPEG", progressive=progressive)
            return out.getvalue()
        data = jpeg()
        key, result = upload(self.lib, data, fmt=3)
        self.assertEqual(result, 0)
        size, fmt = ctypes.c_uint32(), ctypes.c_int()
        self.assertTrue(self.lib.td_asset_present(key, ctypes.byref(size), ctypes.byref(fmt)))
        self.assertEqual((size.value, fmt.value), (len(data), 3))
        for bad, reason in [(data + b"garbage" + b"\xff\xd9", b"format"), (data[:-2], b"format"), (data[:650], b"format"),
                            (jpeg((257, 64)), b"size"), (jpeg((64, 257)), b"size"),
                            (jpeg(mode="L"), b"format"), (jpeg(progressive=True), b"format")]:
            bad_key, result = upload(self.lib, bad, fmt=3)
            self.assertEqual(self.lib.td_asset_result_reason(result), reason)
            self.assertFalse(self.lib.td_asset_present(bad_key, None, None))
            self.assertTrue(self.lib.td_asset_present(key, None, None))
        self.assertEqual(upload(self.lib, jpeg((256, 256)), fmt=3)[1], 0)

    def test_jpeg_validation_has_no_block_sleeps_or_redecode(self):
        from io import BytesIO
        from PIL import Image
        out = BytesIO()
        Image.new("RGB", (256, 256), (12, 34, 56)).save(out, format="JPEG")
        before = self.lib.host_delays()
        key, result = upload(self.lib, out.getvalue(), fmt=3)
        self.assertEqual(result, 0)
        self.assertLess(self.lib.host_delays() - before, 16)
        before = self.lib.host_delays()
        decodes = self.lib.host_decodes()
        self.assertTrue(self.lib.td_asset_present(key, None, None))
        self.assertEqual(self.lib.td_asset_pin(key), 0)
        self.lib.td_asset_unpin(key)
        self.assertEqual(self.lib.host_delays(), before)
        self.assertEqual(self.lib.host_decodes(), decodes)

    def test_live_pins_block_delete_until_last_release(self):
        from io import BytesIO
        from PIL import Image
        out = BytesIO()
        Image.new("RGB", (32, 32)).save(out, format="JPEG")
        key, result = upload(self.lib, out.getvalue(), fmt=3)
        self.assertEqual(result, 0)
        self.assertEqual(self.lib.td_asset_pin(key), 0)
        self.assertEqual(self.lib.td_asset_pin(key), 0)
        deleted = ctypes.c_bool()
        self.assertEqual(self.lib.td_asset_result_reason(self.lib.td_asset_delete(key, ctypes.byref(deleted))), b"in-use")
        self.assertFalse(deleted.value)
        self.lib.td_asset_unpin(key)
        self.assertEqual(self.lib.td_asset_result_reason(self.lib.td_asset_delete(key, ctypes.byref(deleted))), b"in-use")
        self.lib.td_asset_unpin(key)
        self.assertEqual(self.lib.td_asset_delete(key, ctypes.byref(deleted)), 0)
        self.assertTrue(deleted.value)
        self.assertNotEqual(self.lib.td_asset_pin(key), 0)

    def test_read_returns_a_committed_asset_within_its_bound(self):
        from io import BytesIO
        from PIL import Image
        out = BytesIO()
        Image.new("RGB", (48, 48), (200, 10, 10)).save(out, format="JPEG")
        data = out.getvalue()
        key, result = upload(self.lib, data, fmt=3)
        self.assertEqual(result, 0)
        buf = ctypes.create_string_buffer(len(data) + 8)
        got = ctypes.c_size_t()
        self.assertEqual(self.lib.td_asset_read(key, buf, len(data), ctypes.byref(got)), 0)
        self.assertEqual(buf.raw[:got.value], data)
        self.assertEqual(self.lib.td_asset_result_reason(self.lib.td_asset_read(key, buf, len(data) - 1, ctypes.byref(got))), b"size")
        self.assertEqual(got.value, 0)
        self.assertEqual(self.lib.td_asset_result_reason(self.lib.td_asset_read(b"0" * 64, buf, len(buf), ctypes.byref(got))), b"storage")
        self.assertEqual(self.lib.td_asset_result_reason(self.lib.td_asset_read(b"../" + key[3:], buf, len(buf), ctypes.byref(got))), b"storage")

    def test_static_asset_remains_static(self):
        import struct
        data = b"TDI1" + struct.pack("<HHHH", 64, 64, 1, 0) + bytes(12288)
        key, result = upload(self.lib, data, fmt=1)
        self.assertEqual(result, 0)
        size, fmt = ctypes.c_uint32(), ctypes.c_int()
        self.assertTrue(self.lib.td_asset_present(key, ctypes.byref(size), ctypes.byref(fmt)))
        self.assertEqual((size.value, fmt.value), (12300, 1))
        width, height = ctypes.c_uint16(), ctypes.c_uint16()
        self.assertFalse(self.lib.td_asset_gif_canvas(key, ctypes.byref(width), ctypes.byref(height)))

    def test_gif_canvas_for_page_budget(self):
        key, result = upload(self.lib, gif(width=3, codes=[4, 0, 6, 5]))
        self.assertEqual(result, 0)
        width, height = ctypes.c_uint16(), ctypes.c_uint16()
        self.assertTrue(self.lib.td_asset_gif_canvas(key, ctypes.byref(width), ctypes.byref(height)))
        self.assertEqual((width.value, height.value), (3, 1))
        self.assertFalse(self.lib.td_asset_gif_canvas(b"../escape", ctypes.byref(width), ctypes.byref(height)))

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


class LittlefsAssetTest(AssetTest):
    littlefs = True

    def setUp(self):
        self.lib.host_lfs_init()

    def tearDown(self):
        self.lib.td_asset_upload_abort()
        self.assertEqual(self.lib.host_handles(), 0)
        self.lib.host_lfs_done()

    def test_hostile_commit_preserves_previous_assets(self):
        good, result = upload(self.lib, gif())
        self.assertEqual(result, 0)
        for data in [gif()[:-1], gif(width=129), gif(codes=[4, 0] * 1000 + [5])]:
            key, result = upload(self.lib, data)
            self.assertEqual(self.lib.td_asset_result_reason(result),
                             b"size" if data == gif(width=129) else b"format")
            self.assertFalse(self.lib.td_asset_present(key, None, None))
            self.assertFalse(self.lib.host_exists(b"assets/upload.tmp"))
            self.assertTrue(self.lib.td_asset_present(good, None, None))
            self.assertEqual(self.lib.host_handles(), 0)

    test_process_interrupted_during_commit = None

    def test_sync_and_close_failures_preserve_store(self):
        import errno
        good, result = upload(self.lib, gif())
        self.assertEqual(result, 0)
        for operation in [self.lib.host_fail_sync, self.lib.host_fail_close]:
            for error, reason in [(errno.EIO, b"storage"), (errno.ENOSPC, b"full")]:
                operation(error)
                key, result = upload(self.lib, gif(frames=2))
                self.assertNotEqual(result, 0)
                self.assertEqual(self.lib.td_asset_result_reason(result), reason)
                self.assertFalse(self.lib.td_asset_present(key, None, None))
                self.assertTrue(self.lib.td_asset_present(good, None, None))
                self.assertFalse(self.lib.host_exists(b"assets/upload.tmp"))
                self.assertEqual(self.lib.host_handles(), 0)
        self.assertEqual(upload(self.lib, gif(frames=2))[1], 0)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--interrupt":
        lib = load(sys.argv[2])
        lib.host_interrupt(1)
        upload(lib, gif(frames=60))
        sys.exit(1)
    unittest.main()
