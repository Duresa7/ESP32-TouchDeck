import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class GifUiTest(unittest.TestCase):
    def test_worker_and_renderer_lifecycle(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT")) as tmp:
            out = Path(tmp)
            host = ROOT / "tools/host"
            source = (ROOT / "components/ui/ui_gif.c").read_text()
            source = "\n".join(line for line in source.splitlines() if not line.startswith("#include"))
            (out / "ui_gif.c").write_text(
                '#include "gif_ui_host.h"\n#include <stdio.h>\n#include <string.h>\n#include <sys/stat.h>\n'
                + source + "\n", newline="\n")
            includes = [host] + [ROOT / "components" / c / "include"
                                for c in ["td_layout", "td_ui_core", "keys", "td_store"]]
            sources = [host / "gif_ui_test.c", out / "ui_gif.c",
                       ROOT / "components/td_ui_core/ui_gif_player.c", ROOT / "components/td_store/td_gif.c"]
            exe = out / ("gif-ui.exe" if os.name == "nt" else "gif-ui")
            if os.name == "nt":
                cmd = ["cl", "/nologo", "/std:clatest", "/experimental:c11atomics", "/W4", "/WX", "/D_CRT_SECURE_NO_WARNINGS",
                       "/D_CRT_NONSTDC_NO_DEPRECATE", '/DTD_ASSET_DIR="assets"']
                cmd += ["/I" + str(p) for p in includes] + [str(p) for p in sources] + ["/Fe:" + str(exe)]
            else:
                cmd = [os.environ.get("CC", "cc"), "-std=c11", "-D_POSIX_C_SOURCE=200809L", "-pthread",
                       "-Wall", "-Wextra", "-Werror", '-DTD_ASSET_DIR="assets"']
                cmd += ["-I" + str(p) for p in includes] + [str(p) for p in sources] + ["-o", str(exe)]
            subprocess.run(cmd, cwd=out, check=True)
            assets = out / "assets/a"
            assets.mkdir(parents=True)
            (assets / ("a" * 64 + ".bin")).write_bytes((ROOT / "tools/fixtures/gif/normalizer-24.gif").read_bytes())
            subprocess.run([str(exe)], cwd=out, check=True, timeout=30)


if __name__ == "__main__":
    unittest.main()
