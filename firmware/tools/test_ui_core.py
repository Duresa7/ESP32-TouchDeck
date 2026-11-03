import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
CORE = ROOT / "components/td_ui_core"
TEST_CASE = re.compile(r'TEST_CASE\(\s*("(?:[^"\\]|\\.)*")\s*,\s*("(?:[^"\\]|\\.)*")\s*\)', re.S)
EMBED = re.compile(r'\s*asm\("_binary_(\w+)_start"\)')


def c_bytes(data):
    """A char array initializer that holds where char is signed, as it is for gcc on x86."""
    body = ",".join(str(b - 256 if b > 127 else b) for b in data + b"\0")
    return "{" + body + "}"


def embedded(token):
    for path in [*(REPO / "schema/live").glob("*.json"), REPO / "schema/examples/builtin.json"]:
        if re.sub(r"[^A-Za-z0-9]", "_", path.name) == token:
            return path
    raise LookupError(f"no file embeds as {token}")


def rewrite_tests(out):
    tests, embeds, sources = [], {}, []
    for index, path in enumerate(sorted((CORE / "test").glob("test_*.c"))):
        text = path.read_text(encoding="utf-8")
        for match in re.finditer(r"extern const char (\w+)\[\] asm\(\"_binary_(\w+)_start\"\)", text):
            embeds[match.group(1)] = embedded(match.group(2))
        text = EMBED.sub("", text)
        count = 0

        def name(match):
            nonlocal count
            fn = f"host_test_{index}_{count}"
            count += 1
            line = text.count("\n", 0, match.start()) + 1
            tests.append((fn, match.group(1), match.group(2), path.name, line))
            return f"void {fn}(void)"

        target = out / path.name
        target.write_text(TEST_CASE.sub(name, text), encoding="utf-8", newline="\n")
        sources.append(target)
    return tests, embeds, sources


def write_runner(out, tests, embeds):
    lines = ['#include <stdio.h>', '#include <string.h>', '#include "unity.h"', "",
             "void setUp(void) {}", "void tearDown(void) {}", ""]
    lines += [f"void {fn}(void);" for fn, *_ in tests]
    lines += [f"const char {symbol}[] = {c_bytes(path.read_bytes())};" for symbol, path in sorted(embeds.items())]
    lines += ["", "static const struct { void (*fn)(void); const char *name; const char *tags; const char *file; int line; }"
              " k_tests[] = {"]
    lines += [f"    {{ {fn}, {name}, {tags}, \"{file}\", {line} }}," for fn, name, tags, file, line in tests]
    lines += ["};", "",
              "int main(int argc, char **argv)",
              "{",
              "    UnityBegin(\"td_ui_core\");",
              "    for (size_t i = 0; i < sizeof k_tests / sizeof k_tests[0]; i++) {",
              "        if (argc > 1 && strstr(k_tests[i].tags, argv[1]) == NULL && strstr(k_tests[i].name, argv[1]) == NULL) {",
              "            continue;",
              "        }",
              "        UnitySetTestFile(k_tests[i].file);",
              "        UnityDefaultTestRun(k_tests[i].fn, k_tests[i].name, k_tests[i].line);",
              "    }",
              "    return UnityEnd();",
              "}"]
    (out / "host_tests.c").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def write_builtin(out):
    source = (ROOT / "components/td_layout/td_layout.c").read_text(encoding="utf-8")
    start = source.index("void td_layout_free(")
    end = source.index("bool td_layout_load_file(", start)
    data = c_bytes((REPO / "schema/examples/builtin.json").read_bytes())
    head = ['#include <stdlib.h>', '#include "td_layout.h"', f"static const char k_builtin_start[] = {data};",
            "#define k_builtin_end (k_builtin_start + sizeof k_builtin_start)", "static td_layout_t *s_builtin;", ""]
    (out / "builtin_layout.c").write_text("\n".join(head) + source[start:end], encoding="utf-8", newline="\n")


class UiCoreTest(unittest.TestCase):
    def test_ui_core_unity_suite(self):
        idf = os.environ.get("IDF_PATH")
        if not idf:
            self.fail("IDF_PATH must name the ESP-IDF tree, which holds Unity")
        unity = Path(idf) / "components/unity/unity/src"
        cjson = ROOT / "managed_components/espressif__cjson/cJSON"
        with tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT")) as tmp:
            out = Path(tmp)
            tests, embeds, test_sources = rewrite_tests(out)
            write_runner(out, tests, embeds)
            write_builtin(out)
            includes = [unity, cjson] + [ROOT / "components" / c / "include" for c in ["td_ui_core", "keys", "td_layout"]]
            sources = sorted(CORE.glob("ui_*.c")) + test_sources
            sources += [out / "host_tests.c", out / "builtin_layout.c", unity / "unity.c", cjson / "cJSON.c",
                        ROOT / "components/td_layout/td_layout_parse.c", ROOT / "components/keys/key_engine.c"]
            sources = [s for s in sources if s.name != "ui_gif_player.c"]
            defines = ["UNITY_INCLUDE_DOUBLE", "UNITY_SUPPORT_64"]
            exe = out / ("ui-core.exe" if os.name == "nt" else "ui-core")
            if os.name == "nt":
                command = ["cl", "/nologo", "/std:clatest", "/W3", "/WX", "/D_CRT_SECURE_NO_WARNINGS", "/utf-8"]
                command += ["/D" + d for d in defines] + ["/I" + str(p) for p in includes]
                command += [str(p) for p in sources] + ["/Fe:" + str(exe)]
            else:
                command = [os.environ.get("CC", "cc"), "-std=c11", "-D_POSIX_C_SOURCE=200809L", "-Wall", "-Wextra",
                           "-Werror"]
                command += ["-D" + d for d in defines] + ["-I" + str(p) for p in includes]
                command += [str(p) for p in sources] + ["-lm", "-o", str(exe)]
            subprocess.run(command, cwd=out, check=True)
            run = subprocess.run([str(exe), *sys.argv[1:2]], cwd=out, timeout=120, capture_output=True, text=True)
            print(run.stdout)
            self.assertEqual(run.returncode, 0, run.stdout[-4000:] + run.stderr)
            ran = f"{len(tests)} Tests 0 Failures" if len(sys.argv) < 2 else " Tests 0 Failures"
            self.assertIn(ran, run.stdout)


if __name__ == "__main__":
    unittest.main(argv=sys.argv[:1])
