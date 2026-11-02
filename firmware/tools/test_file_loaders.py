import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class FileLoaderTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT"))
        out = Path(cls.tmp.name)
        host = ROOT / "tools/host"
        for name, source in [
            ("layout.c", ROOT / "components/td_layout/td_layout.c"),
            ("icon.c", ROOT / "components/ui/ui_icon.c"),
        ]:
            text = source.read_text().replace('#include "ui_priv.h"', '#include "icon_host.h"')
            (out / name).write_text(text, newline="\n")
        includes = [out, host / "loaders", host / "include"]
        includes += [ROOT / "components" / c / "include"
                     for c in ["td_layout", "td_ui_core", "keys", "td_update"]]
        sources = [host / "file_loaders_test.c", ROOT / "components/td_update/td_update_state.c"]
        cls.exe = out / ("loaders.exe" if os.name == "nt" else "loaders")
        if os.name == "nt":
            command = ["cl", "/nologo", "/std:clatest", "/W3", "/WX", "/D_CRT_SECURE_NO_WARNINGS"]
            command += ["/I" + str(p) for p in includes] + [str(p) for p in sources]
            command += ["/Fe:" + str(cls.exe)]
        else:
            command = [os.environ.get("CC", "cc"), "-std=c11", "-D_POSIX_C_SOURCE=200809L",
                       "-Wall", "-Wextra", "-Werror"]
            command += ["-I" + str(p) for p in includes] + [str(p) for p in sources]
            command += ["-o", str(cls.exe)]
        subprocess.run(command, cwd=out, check=True)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_layout_checks_the_opened_file(self):
        subprocess.run([str(self.exe), "layout"], cwd=self.tmp.name, check=True)

    def test_icon_checks_the_opened_file(self):
        subprocess.run([str(self.exe), "icon"], cwd=self.tmp.name, check=True)


if __name__ == "__main__":
    unittest.main()
