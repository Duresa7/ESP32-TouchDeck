import ctypes
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def layout(render="bar", streams=None, **extra):
    tile = dict(id="tile", label="Media", row=0, col=0, rowSpan=1, colSpan=1,
                icon=None, action=dict(type="widget", source="volume", field="level", account=""),
                render=render, streams=streams if streams is not None else dict(value="number"), **extra)
    return dict(schema=2, name="Media", orientation="landscape",
                pages=[dict(id="page", title="Media", kind="grid", rows=1, cols=1, tiles=[tile])])


class MediaParseTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT"))
        out = Path(cls.tmp.name)
        host = ROOT / "tools/host"
        cjson = ROOT / "managed_components/espressif__cjson/cJSON"
        includes = [out, host / "include", cjson]
        includes += [ROOT / "components" / c / "include" for c in ["td_layout", "td_ui_core", "keys", "td_link_core", "td_link", "td_console", "ui"]]
        link = (ROOT / "components/td_link/td_link.c").read_text()
        start = link.index("void td_link_press_part(")
        (out / "press_part.c").write_text(link[start:link.index("\nvoid td_link_input_cancel", start)], newline="\n")
        sources = [ROOT / "components/td_ui_core/ui_live.c", ROOT / "components/td_ui_core/ui_pages.c", host / "media_parse.c", ROOT / "components/td_link/td_link_live.c", ROOT / "components/td_link_core/td_input.c", ROOT / "components/td_layout/td_layout_parse.c", cjson / "cJSON.c"]
        dll = out / ("media.dll" if os.name == "nt" else "media.so")
        if os.name == "nt":
            command = ["cl", "/nologo", "/LD", "/std:clatest", "/W3", "/WX", "/D_CRT_SECURE_NO_WARNINGS"]
            command += ["/I" + str(p) for p in includes] + [str(p) for p in sources]
            command += ["/link", "/OUT:" + str(dll), "/EXPORT:host_layout", "/EXPORT:host_input", "/EXPORT:host_live", "/EXPORT:host_press"]
        else:
            command = [os.environ.get("CC", "cc"), "-shared", "-fPIC", "-std=c11", "-D_POSIX_C_SOURCE=200809L", "-Wall", "-Wextra", "-Werror"]
            command += ["-I" + str(p) for p in includes] + [str(p) for p in sources] + ["-lm", "-o", str(dll)]
        subprocess.run(command, cwd=out, check=True)
        cls.lib = ctypes.CDLL(str(dll))
        cls.lib.host_live.argtypes = [ctypes.c_char_p]
        cls.lib.host_live.restype = ctypes.c_char_p
        cls.lib.host_layout.argtypes = [ctypes.c_char_p]
        cls.lib.host_layout.restype = ctypes.c_char_p

    @classmethod
    def tearDownClass(cls):
        if os.name == "nt":
            import _ctypes
            _ctypes.FreeLibrary(cls.lib._handle)
        cls.tmp.cleanup()

    def parse(self, value):
        return self.lib.host_layout(json.dumps(value, ensure_ascii=False).encode()).decode()

    def test_live_text_image_values_reach_ui(self):
        def send(kind, value):
            request = dict(m="v2.live.set", layout="builtin", page="p", tile="t", stream="s",
                           n=1, ttl_ms=1000, type=kind, value=value)
            return self.lib.host_live(json.dumps(request, ensure_ascii=False).encode()).decode()
        self.assertEqual(send("text", 'Beyoncé "live"'), 'Beyoncé "live"')
        self.assertEqual(send("text", ""), "")
        self.assertEqual(send("text", "é" * 64), "é" * 64)
        self.assertEqual(send("text", "é" * 65), "size")
        self.assertEqual(send("image", "sha256:" + "a" * 64), "sha256:" + "a" * 64)
        self.assertEqual(send("image", None), "clear")
        self.assertEqual(send("text", 1), "args")
        self.assertEqual(send("image", []), "args")
        self.assertEqual(send("text", "refuse-asset"), "asset")

    def test_unknown_tile_precedes_text_size(self):
        request = dict(m="v2.live.set", layout="builtin", page="p", tile="unknown", stream="s",
                       n=1, ttl_ms=1000, type="text", value="x" * 129)
        self.assertEqual(self.lib.host_live(json.dumps(request).encode()), b"tile")

    def test_invalid_parts_do_not_consume_sequence(self):
        self.assertEqual(self.lib.host_press(), 0)

    def test_input_coalesces_paces_ends_and_cancels(self):
        self.assertEqual(self.lib.host_input(), 0)

    def test_bar_required_optional_streams_and_input(self):
        self.assertEqual(self.parse(layout()), "ok")
        self.assertEqual(self.parse(layout(input=True)), "ok")
        self.assertEqual(self.parse(layout(streams=dict(value="number", muted="number"))), "ok")
        for streams in [{}, dict(muted="number"), dict(value="series"), dict(value="number", extra="number")]:
            self.assertEqual(self.parse(layout(streams=streams)), "tile.streams")
        self.assertEqual(self.parse(layout(input=False)), "tile.input")

    def test_now_playing_and_layout_image_budget(self):
        streams = dict(title="text", artist="text", cover="image", state="number")
        value = layout("now-playing", streams)
        self.assertEqual(self.parse(value), "ok")
        for name in streams:
            missing = streams.copy()
            del missing[name]
            self.assertEqual(self.parse(layout("now-playing", missing)), "tile.streams")
            wrong = streams | {name: "series"}
            self.assertEqual(self.parse(layout("now-playing", wrong)), "tile.streams")
        self.assertEqual(self.parse(layout("now-playing", streams, input=True)), "tile.input")
        self.assertEqual(self.parse(layout("now-playing", streams, asset="sha256:" + "a" * 64)), "icon.format")
        pages = value["pages"]
        for i in range(1, 5):
            page = json.loads(json.dumps(pages[0]))
            page["id"] = f"p{i}"
            page["tiles"][0]["id"] = f"t{i}"
            pages.append(page)
            self.assertEqual(self.parse(value), "ok" if i < 4 else "layout.live_image_limit")

        pages[-1]["tiles"][0]["row"] = 1
        self.assertEqual(self.parse(value), "tile.bounds")
        pages[-1]["tiles"][0]["row"] = 0
        duplicate = dict(pages[-1]["tiles"][0], id="overlap")
        pages[-1]["tiles"].append(duplicate)
        self.assertEqual(self.parse(value), "tile.overlap")


if __name__ == "__main__":
    unittest.main()
