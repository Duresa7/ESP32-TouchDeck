import ctypes
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
READ = ctypes.CFUNCTYPE(ctypes.c_size_t, ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t)
YIELD = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p)


def gif(frames=1, width=1, height=1, codes=None):
    header = b"GIF89a" + struct.pack("<HHBBB", width, height, 0x80, 0, 0)
    header += bytes([0, 0, 0, 255, 255, 255]) + b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00"
    # Repeated clears keep code width at three bits, including for hostile output.
    codes = codes or [4, 0, 5]
    bits = sum(c << (3 * i) for i, c in enumerate(codes))
    data = bits.to_bytes((len(codes) * 3 + 7) // 8, "little")
    blocks = b"".join(bytes([len(data[i:i+255])]) + data[i:i+255] for i in range(0, len(data), 255)) + b"\0"
    frame = b"\x21\xf9\x04\x04\x0a\x00\x00\x00\x2c" + struct.pack("<HHHHB", 0, 0, width, height, 0) + b"\x02" + blocks
    return header + frame * frames + b"\x3b"


class GifTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT"))
        dll = Path(cls.tmp.name) / ("gif.dll" if os.name == "nt" else "gif.so")
        source = str(ROOT / "components/td_store/td_gif.c")
        include = str(ROOT / "components/td_store/include")
        if os.name == "nt":
            command = ["cl", "/nologo", "/LD", "/W4", "/WX", "/I" + include, source,
                       "/Fo" + str(Path(cls.tmp.name) / "gif.obj"), "/link", "/OUT:" + str(dll),
                       "/EXPORT:td_gif_validate"]
        else:
            command = [os.environ.get("CC", "cc"), "-shared", "-fPIC", "-O2", "-Wall", "-Wextra",
                       "-Werror", "-I" + include, source, "-o", str(dll)]
        subprocess.run(command, check=True, cwd=cls.tmp.name)
        cls.lib = ctypes.CDLL(str(dll))
        cls.lib.td_gif_validate.argtypes = [READ, YIELD, ctypes.c_void_p, ctypes.c_uint32]

    @classmethod
    def tearDownClass(cls):
        if os.name == "nt":
            import _ctypes
            _ctypes.FreeLibrary(cls.lib._handle)
        cls.tmp.cleanup()

    def validate(self, data, cancel=False):
        data = bytes(data)
        at = 0
        errors = []
        since = 0
        yields = 0
        @READ
        def read(ctx, out, count):
            nonlocal at, since
            if count > 256: errors.append("unbounded read")
            block = data[at:at+count]
            ctypes.memmove(out, block, len(block))
            at += len(block)
            since += len(block)
            if since > 256: errors.append("unbounded slice")
            return len(block)
        @YIELD
        def pause(ctx):
            nonlocal yields, since
            yields += 1
            since = 0
            return not cancel
        result = self.lib.td_gif_validate(read, pause, None, len(data))
        self.assertEqual(errors, [])
        return result, yields

    def test_valid(self):
        self.assertEqual(self.validate(gif())[0], 0)
        self.assertEqual(self.validate(gif(frames=60))[0], 0)
        self.assertEqual(self.validate(gif(width=3, codes=[4, 0, 6, 5]))[0], 0)

    def test_encoder_dictionary_growth(self):
        from PIL import Image
        import io
        import random
        rng = random.Random(94)
        for pixels in [bytes(128 * 128), rng.randbytes(128 * 128)]:
            image = Image.frombytes("P", (128, 128), pixels)
            image.putpalette(bytes(range(256)) * 3)
            out = io.BytesIO()
            image.save(out, format="GIF", duration=100, loop=0, disposal=1, optimize=False, interlace=False)
            self.assertEqual(self.validate(out.getvalue())[0], 0)

    def test_truncated(self):
        data = gif()
        for n in range(len(data)):
            self.assertNotEqual(self.validate(data[:n])[0], 0, n)

    def test_limits(self):
        for data in [gif(width=129), gif(height=65535), gif(frames=61), bytes(262145)]:
            self.assertEqual(self.validate(data)[0], 2)

    def test_boundary_canvas_and_truncated_compressed_stream(self):
        from PIL import Image
        import io
        import random
        image = Image.frombytes("P", (128, 128), random.Random(2).randbytes(16384))
        image.putpalette(bytes(range(256)) * 3)
        out = io.BytesIO()
        image.save(out, format="GIF", duration=100, loop=0, disposal=1, optimize=False, interlace=False)
        data = out.getvalue()
        self.assertEqual(self.validate(data)[0], 0)
        for n in range(13, len(data), 97):
            self.assertNotEqual(self.validate(data[:n])[0], 0)

    def test_lzw_bomb(self):
        self.assertEqual(self.validate(gif(codes=[4, 0] * 1000 + [5]))[0], 1)
        self.assertEqual(self.validate(gif(codes=[4, 0, 6, 5]))[0], 1)

    def test_invalid_lzw(self):
        for codes in [[4, 7, 5], [4, 5], [4, 0], [0, 5], [4, 2, 5]]:
            self.assertEqual(self.validate(gif(codes=codes))[0], 1, codes)

    def test_normalization(self):
        for offset, value in [(0, 0), (10, 0), (35, 1), (41, 8), (42, 1), (47, 1), (51, 2), (55, 64)]:
            data = bytearray(gif())
            data[offset] = value
            self.assertEqual(self.validate(data)[0], 1, offset)
        self.assertEqual(self.validate(gif() + b"extra")[0], 1)

    def test_cancellation_between_slices(self):
        result, count = self.validate(gif(frames=60), cancel=True)
        self.assertEqual(result, 4)
        self.assertEqual(count, 1)


if __name__ == "__main__":
    unittest.main()
