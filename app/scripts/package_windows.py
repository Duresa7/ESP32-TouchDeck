#!/usr/bin/env python3
"""Build and verify a Windows x64 production candidate; never publish or touch a port."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[2]
ZIP_TIME = (2020, 1, 1, 0, 0, 0)
TARGET = "x86_64-pc-windows-msvc"


def run(args, *, cwd=ROOT, env=None):
    print("+ " + subprocess.list2cmdline([str(arg) for arg in args]), flush=True)
    subprocess.run([str(arg) for arg in args], cwd=cwd, env=env, check=True)


def capture(args, *, cwd=ROOT, env=None):
    return subprocess.check_output(args, cwd=cwd, env=env, text=True).strip()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8", newline="\n")


def bundle_tool():
    spec = importlib.util.spec_from_file_location("make_bundle", ROOT / "firmware/tools/make_bundle.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def clean_revision():
    if capture(["git", "status", "--porcelain", "--untracked-files=normal"]):
        raise ValueError("Commit source changes before packaging; a clean source revision is required")
    return capture(["git", "rev-parse", "HEAD"])


def audit_radio(build):
    config = (build / "config/sdkconfig.json").read_text(encoding="utf-8")
    settings = json.loads(config)
    for key in ("ESP_WIFI_ENABLED", "BT_ENABLED", "BT_NIMBLE_ENABLED", "BT_BLUEDROID_ENABLED"):
        if settings.get(key):
            raise ValueError(f"Radio enabled: {key}")
    if not settings.get("ESP_CONSOLE_USB_SERIAL_JTAG"):
        raise ValueError("Firmware must keep the USB-Serial/JTAG console")
    description = json.loads((build / "project_description.json").read_text(encoding="utf-8"))
    components = description["build_components"]
    forbidden = {"td_net", "td_ble", "esp_wifi", "bt", "esp_phy"}.intersection(components)
    if forbidden:
        raise ValueError(f"Radio components linked: {sorted(forbidden)}")
    return {"usb_serial_jtag": True, "radio_components": [], "build_components": components}


def firmware(out, revision, lock_path):
    idf = Path(os.environ["IDF_PATH"]) / "tools/idf.py"
    if not idf.is_file():
        raise ValueError("Activate an existing ESP-IDF environment before building")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    # O_EXCL is atomic across processes/worktrees. Never remove another owner's lock.
    owner = f"packaging {revision} pid={os.getpid()}\n"
    fd = os.open(lock_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL)
    try:
        with os.fdopen(fd, "w") as lock:
            lock.write(owner)
        build = out / "build/firmware"
        run([sys.executable, idf, "-C", ROOT / "firmware", "-B", build, "build"])
        audit = audit_radio(build)
        tool = bundle_tool()
        bundle, manifest = tool.make_bundle(build, out / "payload/firmware")
        tool.check_bundle(bundle)
        return {"source_revision": revision, "bundle": bundle.name, "manifest": manifest, "audit": audit}
    finally:
        if lock_path.read_text() == owner:
            lock_path.unlink()


def verify(archive_path):
    with zipfile.ZipFile(archive_path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)) or any(
            name.startswith("/") or ".." in Path(name).parts or "\\" in name for name in names
        ):
            raise ValueError("Unsafe or duplicate archive paths")
        manifest = json.loads(archive.read("manifest.json"))
        if manifest["format"] != 1 or manifest["target"] != TARGET or manifest["cargo_features"] != []:
            raise ValueError("Unexpected package format/target/features")
        if not re.fullmatch(r"[0-9a-f]{40}", manifest["source_revision"]):
            raise ValueError("Missing exact source revision")
        files = manifest["files"]
        expected = {entry["path"] for entry in files} | {"manifest.json", "SHA256SUMS"}
        if set(names) != expected or len(files) != len(expected) - 2:
            raise ValueError("Package contents differ from manifest")
        for entry in files:
            data = archive.read(entry["path"])
            if len(data) != entry["bytes"] or sha(data) != entry["sha256"]:
                raise ValueError(f"Hash/size mismatch: {entry['path']}")
        sums = "".join(f"{sha(archive.read(name))}  {name}\n" for name in sorted(expected - {"SHA256SUMS"}))
        if archive.read("SHA256SUMS").decode("utf-8") != sums:
            raise ValueError("SHA256SUMS mismatch")
        app = archive.read("touchdeck.exe")
        if ("touchdeck-build-sha:" + manifest["source_revision"]).encode() not in app:
            raise ValueError("Executable source marker does not match manifest")
        graph = archive.read("build/production-dependencies.txt").decode("utf-8")
        if "tauri-plugin-wdio-webdriver" in graph or b"tauri_plugin_wdio_webdriver" in app:
            raise ValueError("WebDriver found in production package")
        if not archive.read("td-statusline.exe").startswith(b"MZ") or not app.startswith(b"MZ"):
            raise ValueError("Windows executables missing")
        fw = manifest["firmware"]
        if fw["source_revision"] != manifest["source_revision"]:
            raise ValueError("Firmware source revision differs")
        # Use the existing firmware format reader, not a second implementation.
        import tempfile
        with tempfile.TemporaryDirectory(prefix="touchdeck-bundle-check-") as temp:
            bundle = Path(temp) / fw["bundle"]
            bundle.write_bytes(archive.read("firmware/" + fw["bundle"]))
            if bundle_tool().check_bundle(bundle) != fw["manifest"]:
                raise ValueError("Firmware metadata differs")
        print(json.dumps({"ok": True, "archive": str(archive_path), "source_revision": manifest["source_revision"],
                          "files": len(names), "sha256": sha(archive_path.read_bytes()),
                          "firmware_elf_sha256": fw["manifest"]["elf_sha256"]}))
        return manifest


def build(args):
    if sys.platform != "win32":
        raise ValueError("This portable recipe supports Windows x64 only")
    revision = clean_revision()
    out = args.out.resolve()
    if out.is_relative_to(ROOT) or ROOT.is_relative_to(out):
        raise ValueError("Output must be outside and must not contain the checkout")
    # A new output directory prevents accidental reuse of e2e binaries or old firmware.
    out.mkdir(parents=True, exist_ok=False)
    payload = out / "payload"
    payload.mkdir()
    common = Path(capture(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"]))
    fw = firmware(out, revision, args.build_lock or common.parent / ".verify/m3-build.lock")
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(out / "build/app")
    # Do not inherit feature/target overrides from an e2e developer shell.
    for key in ("CARGO_BUILD_TARGET", "CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS"):
        env.pop(key, None)
    app_dir = ROOT / "app"
    tauri = app_dir / "node_modules/@tauri-apps/cli/tauri.js"
    if not tauri.is_file():
        raise ValueError("Install locked app dependencies first (see docs/packaging.md)")
    run(["node", tauri, "build", "--ci", "--no-bundle", "--target", TARGET,
         "--", "--locked", "--no-default-features"], cwd=app_dir, env=env)
    run(["cargo", "build", "--release", "--locked", "--no-default-features", "--target", TARGET,
         "-p", "td-link", "--bin", "td-statusline"], cwd=app_dir, env=env)
    graph = capture(["cargo", "tree", "--locked", "--target", TARGET, "-p", "touchdeck",
                     "--no-default-features", "-e", "normal,build,features"], cwd=app_dir, env=env)
    if "tauri-plugin-wdio-webdriver" in graph:
        raise ValueError("WebDriver dependency enabled")
    for name in ("touchdeck.exe", "td-statusline.exe"):
        shutil.copyfile(out / "build/app" / TARGET / "release" / name, payload / name)
    (payload / "build").mkdir()
    (payload / "build/production-dependencies.txt").write_text(graph + "\n", encoding="utf-8", newline="\n")
    shutil.copyfile(ROOT / "LICENSE", payload / "LICENSE.txt")
    (payload / "licenses").mkdir()
    shutil.copyfile(ROOT / "app/icons/builtin/LICENSE-lucide.txt", payload / "licenses/LICENSE-lucide.txt")
    shutil.copyfile(ROOT / "docs/packaging.md", payload / "PACKAGING.md")
    entries = [{"path": path.relative_to(payload).as_posix(), "bytes": path.stat().st_size,
                "sha256": sha(path.read_bytes())} for path in sorted(payload.rglob("*")) if path.is_file()]
    write_json(payload / "manifest.json", {
        "format": 1, "product": "TouchDeck", "candidate": True, "source_revision": revision,
        "target": TARGET, "cargo_features": [], "firmware": fw,
        "runtime": {"webview2": "Evergreen runtime required; not included", "settings": "per-user, not portable"},
        "tools": {"rustc": capture(["rustc", "--version"]), "cargo": capture(["cargo", "--version"]),
                  "node": capture(["node", "--version"]), "python": sys.version,
                  "tauri": capture(["node", str(tauri), "--version"], cwd=app_dir)},
        "files": entries,
    })
    paths = sorted(path for path in payload.rglob("*") if path.is_file())
    (payload / "SHA256SUMS").write_text("".join(
        f"{sha(path.read_bytes())}  {path.relative_to(payload).as_posix()}\n" for path in paths
    ), encoding="utf-8", newline="\n")
    if clean_revision() != revision:
        raise ValueError("Source revision changed during build")
    archive_path = out / f"touchdeck-{revision[:12]}-windows-x64-candidate.zip"
    with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(payload.rglob("*")):
            if path.is_file():
                info = zipfile.ZipInfo(path.relative_to(payload).as_posix(), ZIP_TIME)
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o644 << 16
                archive.writestr(info, path.read_bytes())
    verify(archive_path)
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{sha(archive_path.read_bytes())}  {archive_path.name}\n", encoding="utf-8", newline="\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build")
    builder.add_argument("--out", type=Path, required=True, help="new directory outside checkout")
    builder.add_argument("--build-lock", type=Path, help="shared ESP-IDF build lock; defaults to primary checkout .verify")
    checker = commands.add_parser("verify")
    checker.add_argument("archive", type=Path)
    args = parser.parse_args()
    try:
        build(args) if args.command == "build" else verify(args.archive)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        print(f"packaging: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
