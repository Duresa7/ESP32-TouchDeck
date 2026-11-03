import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
VECTORS = ROOT.parent / "schema/live/tile-geometry-vectors.json"


def driver(cases):
    lines = [
        "#include <stdio.h>",
        '#include "ui_geometry.h"',
        "typedef struct { const char *name; int16_t sw, sh; bool full; uint8_t rows, cols, row, col, rs, cs;"
        " int16_t x, y, w, h; } geometry_case_t;",
        "static const geometry_case_t CASES[] = {",
    ]
    for c in cases:
        s, g, t, r = c["screen"], c["grid"], c["tile"], c["rect"]
        lines.append(
            f'    {{ {json.dumps(c["name"])}, {s["w"]}, {s["h"]}, {str(c["fullScreen"]).lower()}, {g["rows"]}, '
            f'{g["cols"]}, {t["row"]}, {t["col"]}, {t["rowSpan"]}, {t["colSpan"]}, {r["x"]}, {r["y"]}, {r["w"]}, '
            f'{r["h"]} }},'
        )
    lines += [
        "};",
        "int main(void)",
        "{",
        "    int failed = 0;",
        "    for (size_t i = 0; i < sizeof CASES / sizeof CASES[0]; i++) {",
        "        const geometry_case_t *c = &CASES[i];",
        "        ui_geometry_t g = ui_geometry((ui_size_t){ c->sw, c->sh }, c->full);",
        "        ui_page_t page = { .id = \"p\", .title = \"p\", .rows = c->rows, .cols = c->cols };",
        "        ui_tile_t tile = { .id = \"t\", .label = \"t\", .row = c->row, .col = c->col,"
        " .row_span = c->rs, .col_span = c->cs };",
        "        ui_rect_t r = ui_tile_rect(&page, &tile, g.grid, g.gap);",
        "        if (r.x != c->x || r.y != c->y || r.w != c->w || r.h != c->h) {",
        '            printf("%s: got %d,%d,%d,%d\\n", c->name, r.x, r.y, r.w, r.h);',
        "            failed++;",
        "        }",
        "    }",
        "    return failed;",
        "}",
    ]
    return "\n".join(lines) + "\n"


class TileGeometryTest(unittest.TestCase):
    def test_firmware_tile_rects_match_the_shared_vectors(self):
        cases = json.loads(VECTORS.read_text(encoding="utf-8"))["cases"]
        with tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT")) as tmp:
            out = Path(tmp)
            (out / "geometry_test.c").write_text(driver(cases), encoding="utf-8", newline="\n")
            core = ROOT / "components/td_ui_core"
            includes = [core / "include", ROOT / "components/keys/include"]
            sources = [core / "ui_geometry.c", core / "ui_pages.c", out / "geometry_test.c"]
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
