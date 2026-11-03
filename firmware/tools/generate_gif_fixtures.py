"""Regenerate synthetic Companion outputs with the repository's normalizer."""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
out = ROOT / "firmware/tools/fixtures/gif"
with tempfile.TemporaryDirectory(dir=os.environ.get("TD_TEST_OUT")) as tmp:
    project = Path(tmp)
    (project / "Cargo.lock").write_bytes((ROOT / "app/Cargo.lock").read_bytes())
    (project / "Cargo.toml").write_text(
        '[package]\nname = "gif-firmware-fixtures"\nversion = "0.0.0"\nedition = "2024"\n'
        '[dependencies]\ngif = "0.14"\n'
        f'td-host = {{ path = "{(ROOT / "app/crates/td-host").as_posix()}" }}\n'
        '[[bin]]\nname = "generate"\npath = "main.rs"\n', newline="\n")
    (project / "main.rs").write_bytes((ROOT / "firmware/tools/gif_fixture_generator.rs").read_bytes())
    subprocess.run(["cargo", "run", "--manifest-path", str(project / "Cargo.toml"),
                    "--target-dir", str(ROOT / "app/target"), "--", str(out)], check=True)
