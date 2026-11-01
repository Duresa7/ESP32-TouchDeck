import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class KeySinkTest(unittest.TestCase):
    def test_capacity_and_event_shapes(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT")) as tmp:
            out = Path(tmp)
            (out / "freertos").mkdir()
            (out / "freertos/FreeRTOS.h").write_text(
                "typedef int portMUX_TYPE;\n#define portMUX_INITIALIZER_UNLOCKED 0\n"
                "#define portENTER_CRITICAL(p) ((void)(p))\n"
                "#define portEXIT_CRITICAL(p) ((void)(p))\n", newline="\n")
            (out / "esp_timer.h").write_text(
                "#include <stdint.h>\nstatic inline int64_t esp_timer_get_time(void) "
                "{ return INT64_C(4294967295000); }\n", newline="\n")
            includes = [out, ROOT / "components/td_link/include", ROOT / "components/keys/include"]
            sources = [ROOT / "components/td_link/key_sink_link.c", ROOT / "tools/host/key_sink_test.c"]
            exe = out / ("test.exe" if os.name == "nt" else "test")
            if os.name == "nt":
                command = ["cl", "/nologo", "/std:clatest", "/W3", "/WX", "/D_CRT_SECURE_NO_WARNINGS"]
                command += ["/I" + str(p) for p in includes] + [str(p) for p in sources] + ["/Fe:" + str(exe)]
            else:
                command = [os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror"]
                command += ["-I" + str(p) for p in includes] + [str(p) for p in sources] + ["-o", str(exe)]
            subprocess.run(command, cwd=out, check=True)
            subprocess.run([str(exe)], cwd=out, check=True)


if __name__ == "__main__":
    unittest.main()
