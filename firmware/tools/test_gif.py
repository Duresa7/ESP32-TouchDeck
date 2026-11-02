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


class Info(ctypes.Structure):
    _fields_ = [("width", ctypes.c_uint16), ("height", ctypes.c_uint16), ("frames", ctypes.c_uint16),
                ("duration_ms", ctypes.c_uint32), ("delays_ms", ctypes.c_uint32 * 60)]


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
                       "/EXPORT:td_gif_validate", "/EXPORT:td_gif_open", "/EXPORT:td_gif_close",
                       "/EXPORT:td_gif_render", "/EXPORT:td_gif_info"]
        else:
            command = [os.environ.get("CC", "cc"), "-shared", "-fPIC", "-O2", "-Wall", "-Wextra",
                       "-Werror", "-I" + include, source, "-o", str(dll)]
        subprocess.run(command, check=True, cwd=cls.tmp.name)
        cls.lib = ctypes.CDLL(str(dll))
        cls.lib.td_gif_validate.argtypes = [READ, YIELD, ctypes.c_void_p, ctypes.c_uint32]
        cls.lib.td_gif_open.argtypes = [ctypes.c_void_p, ctypes.c_uint32, YIELD, ctypes.c_void_p,
                                      ctypes.POINTER(ctypes.c_void_p)]
        cls.lib.td_gif_close.argtypes = [ctypes.c_void_p]
        cls.lib.td_gif_info.argtypes = [ctypes.c_void_p]
        cls.lib.td_gif_info.restype = ctypes.POINTER(Info)
        cls.lib.td_gif_render.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p,
                                        ctypes.c_size_t, YIELD, ctypes.c_void_p]

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
        self.bytes_read = at
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
        self.assertLess(self.bytes_read, 100)
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

    def test_decoder_pixels_bounds_and_cancel(self):
        data = gif(width=3, codes=[4, 0, 1, 0, 5])
        decoder = ctypes.c_void_p()
        keep = YIELD(lambda _: True)
        self.assertEqual(self.lib.td_gif_open(data, len(data), keep, None, ctypes.byref(decoder)), 0)
        try:
            out = (ctypes.c_uint8 * 9)()
            self.assertEqual(self.lib.td_gif_render(decoder, 0, out, 8, keep, None), 2)
            self.assertEqual(self.lib.td_gif_render(decoder, 1, out, 9, keep, None), 1)
            self.assertEqual(self.lib.td_gif_render(decoder, 0, out, 9, keep, None), 0)
            self.assertEqual(bytes(out), bytes([0, 0, 255, 255, 0, 0, 255, 255, 255]))
        finally:
            self.lib.td_gif_close(decoder)

    def test_decoder_cancels_expansion_not_only_input(self):
        from PIL import Image
        import io
        image = Image.new("P", (128, 128))
        image.putpalette(bytes(range(256)) * 3)
        encoded = io.BytesIO()
        image.save(encoded, format="GIF", duration=100, loop=0, disposal=1, interlace=False)
        data = encoded.getvalue()
        decoder = ctypes.c_void_p()
        keep = YIELD(lambda _: True)
        self.assertEqual(self.lib.td_gif_open(data, len(data), keep, None, ctypes.byref(decoder)), 0)
        try:
            out = (ctypes.c_uint8 * (128 * 128 * 3))()
            self.assertEqual(self.lib.td_gif_render(decoder, 0, out, len(out), YIELD(lambda _: False), None), 4)
        finally:
            self.lib.td_gif_close(decoder)

    def test_companion_outputs_validate_and_decode(self):
        from PIL import Image
        import io
        folder = ROOT / "tools/fixtures/gif"
        manifest = (folder / "manifest.tsv").read_text().splitlines()
        self.assertEqual(len(manifest), 32)
        for line in manifest:
            name, width, height, delays = line.split("\t")
            data = (folder / name).read_bytes()
            self.assertEqual(self.validate(data)[0], 0, name)
            decoder = ctypes.c_void_p()
            keep = YIELD(lambda _: True)
            self.assertEqual(self.lib.td_gif_open(data, len(data), keep, None, ctypes.byref(decoder)), 0, name)
            try:
                info = self.lib.td_gif_info(decoder).contents
                self.assertEqual((info.width, info.height), (int(width), int(height)))
                self.assertEqual(list(info.delays_ms)[:info.frames], list(map(int, delays.split(","))))
                image = Image.open(io.BytesIO(data))
                for frame in reversed(range(info.frames)):
                    image.seek(frame)
                    raw = image.convert("RGBA").tobytes()
                    rgba = list(zip(raw[0::4], raw[1::4], raw[2::4], raw[3::4]))
                    rgb = b"".join(struct.pack("<H", ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3))
                                   if a else b"\0\0" for r, g, b, a in rgba)
                    expected = rgb + bytes(a for _, _, _, a in rgba)
                    pixels = (ctypes.c_uint8 * len(expected))()
                    self.assertEqual(self.lib.td_gif_render(decoder, frame, pixels, len(pixels), keep, None), 0)
                    self.assertEqual(bytes(pixels), expected, (name, frame))
            finally:
                self.lib.td_gif_close(decoder)

    def test_decoder_composes_dependent_transparency(self):
        white = gif(codes=[4, 1, 5])
        transparent = bytearray(gif())
        control = transparent.index(b"\x21\xf9")
        transparent[control + 3] = 5
        data = white[:-1] + bytes(transparent[control:])
        decoder = ctypes.c_void_p()
        keep = YIELD(lambda _: True)
        self.assertEqual(self.lib.td_gif_open(data, len(data), keep, None, ctypes.byref(decoder)), 0)
        try:
            out = (ctypes.c_uint8 * 3)()
            for frame in [1, 0, 1]:
                self.assertEqual(self.lib.td_gif_render(decoder, frame, out, 3, keep, None), 0)
                self.assertEqual(bytes(out), b"\xff\xff\xff")
        finally:
            self.lib.td_gif_close(decoder)

    def test_decoder_refuses_truncation_and_invalid_expansion(self):
        keep = YIELD(lambda _: True)
        data = gif()
        bad = [data[:n] for n in range(len(data))]
        bad += [gif(width=129), gif(frames=61), gif(codes=[4, 7, 5]), gif(codes=[4, 0, 6, 5])]
        for data in bad:
            decoder = ctypes.c_void_p()
            self.assertNotEqual(self.lib.td_gif_open(data, len(data), keep, None, ctypes.byref(decoder)), 0)
            self.assertFalse(decoder.value)


if __name__ == "__main__":
    unittest.main()
