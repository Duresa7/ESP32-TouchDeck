import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class PlaybackTest(unittest.TestCase):
    def test_layout_commit_budget(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT")) as tmp:
            out = Path(tmp)
            source = (ROOT / "components/td_link/td_link_files.c").read_text()
            start = source.index("static void finish_put(")
            end = source.index("\nvoid td_link_xfer_end(", start)
            (out / "finish_put.c").write_text(source[start:end], newline="\n")
            includes = [out] + [ROOT / "components" / c / "include"
                               for c in ["td_layout", "td_store", "td_ui_core", "keys"]]
            sources = [ROOT / "tools/host/layout_budget_test.c",
                       ROOT / "components/td_ui_core/ui_gif_player.c", ROOT / "components/td_store/td_gif.c"]
            exe = out / ("budget.exe" if os.name == "nt" else "budget")
            if os.name == "nt":
                cmd = ["cl", "/nologo", "/std:clatest", "/W4", "/WX"]
                cmd += ["/I" + str(p) for p in includes] + [str(p) for p in sources] + ["/Fe:" + str(exe)]
            else:
                cmd = [os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror"]
                cmd += ["-I" + str(p) for p in includes] + [str(p) for p in sources] + ["-o", str(exe)]
            subprocess.run(cmd, cwd=out, check=True)
            subprocess.run([str(exe)], cwd=out, check=True)

    def test_playback(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT")) as tmp:
            out = Path(tmp)
            includes = [ROOT / "components" / component / "include"
                        for component in ["td_store", "td_ui_core", "keys"]]
            sources = [ROOT / "tools/host/playback_test.c",
                       ROOT / "components/td_ui_core/ui_gif_player.c",
                       ROOT / "components/td_store/td_gif.c"]
            exe = out / ("playback.exe" if os.name == "nt" else "playback")
            if os.name == "nt":
                cmd = ["cl", "/nologo", "/std:clatest", "/W4", "/WX"]
                cmd += ["/I" + str(p) for p in includes] + [str(p) for p in sources]
                cmd += ["/Fe:" + str(exe)]
            else:
                cmd = [os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror"]
                cmd += ["-I" + str(p) for p in includes] + [str(p) for p in sources]
                cmd += ["-o", str(exe)]
            subprocess.run(cmd, cwd=out, check=True)
            subprocess.run([str(exe), str(ROOT / "tools/fixtures/gif/normalizer-24.gif")], cwd=out, check=True)


if __name__ == "__main__":
    unittest.main()
