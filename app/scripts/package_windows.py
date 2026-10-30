#!/usr/bin/env python3
"""Build a signed Open TouchDeck Windows x64 installer with the Companion and Firmware; never publish or touch a port."""

import argparse
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "app"
ELECTRON = APP / "electron"
TARGET = "x86_64-pc-windows-msvc"
FEED = "https://github.com/Duresa7/ESP32-TouchDeck/releases/download"
TEST_HOOK_MARKERS = (b"/test/pairing-delivery", b"/test/devices/", b"--emit-ts")


def run(args, *, cwd=ROOT, env=None, quiet=False):
    print("+ " + subprocess.list2cmdline([str(arg) for arg in args]), flush=True)
    # The signer prints key material on generate: quiet runs capture and drop all output.
    output = subprocess.PIPE if quiet else None
    subprocess.run([str(arg) for arg in args], cwd=cwd, env=env, check=True, stdout=output, stderr=output)


def capture(args, *, cwd=ROOT, env=None):
    return subprocess.check_output(args, cwd=cwd, env=env, text=True).strip()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8", newline="\n")


def bundle_tool():
    # Importing the existing tool must not dirty the source checkout with __pycache__.
    sys.dont_write_bytecode = True
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


def verify_production_companion(binary):
    if not binary.startswith(b"MZ"):
        raise ValueError("Windows executable missing")
    for marker in TEST_HOOK_MARKERS:
        if marker in binary:
            raise ValueError(f"Test hooks found in production Companion: {marker.decode()}")


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
        installed_bundle = bundle.with_name("touchdeck.tdfw")
        bundle.rename(installed_bundle)
        return {"source_revision": revision, "bundle": installed_bundle.name,
                "sha256": sha(installed_bundle.read_bytes()), "manifest": manifest, "audit": audit}
    finally:
        if lock_path.read_text() == owner:
            lock_path.unlink()


def prebuilt_firmware(out, revision, source):
    folder = out / "payload/firmware"
    folder.mkdir()
    bundle = folder / "touchdeck.tdfw"
    shutil.copyfile(source, bundle)
    manifest = bundle_tool().check_bundle(bundle)
    return {"source_revision": None, "requested_revision": revision, "bundle": bundle.name,
            "sha256": sha(bundle.read_bytes()), "manifest": manifest, "audit": None}


def next_patch(version):
    major, minor, patch = (int(part) for part in version.split("."))
    return f"{major}.{minor}.{patch + 1}"


def package(out, version, companion, payload, name, env):
    config = json.loads((ELECTRON / "electron-builder.json").read_text(encoding="utf-8"))
    output = out / "build" / f"installer-{version}"
    config["directories"]["output"] = str(output)
    config["extraMetadata"] = {"version": version}
    config["extraFiles"] = [{"from": str(companion), "to": "td-companion.exe"}]
    config["extraResources"] = [{"from": str(payload), "to": "."}]
    config["nsis"]["artifactName"] = name
    path = out / f"electron-builder-{version}.json"
    write_json(path, config)
    run(["node", ELECTRON / "node_modules/electron-builder/cli.js", "--win", "nsis", "--x64",
         "--projectDir", ELECTRON, "--config", path, "--publish", "never"], cwd=ELECTRON, env=env)
    installer = output / name
    if not installer.is_file():
        raise ValueError(f"electron-builder did not write {name}")
    return installer


def sign(installer, version, env, key_path=None):
    args = ["node", APP / "node_modules/@tauri-apps/cli/tauri.js", "signer", "sign", "--app-version", version]
    if key_path:
        args += ["--private-key-path", key_path]
    run(args + [installer], cwd=APP, env=env, quiet=True)
    return installer.with_name(installer.name + ".sig").read_text(encoding="utf-8").strip()


def manifest_entry(installer, signature, version, notes):
    return {"version": version, "notes": notes,
            "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "platforms": {"windows-x86_64": {"signature": signature, "size": installer.stat().st_size,
                                             "url": f"{FEED}/v{version}/{installer.name}"}}}


def build(args):
    if sys.platform != "win32":
        raise ValueError("This recipe supports Windows x64 only")
    signing_present = os.environ.get("TAURI_SIGNING_PRIVATE_KEY") or os.environ.get("TAURI_SIGNING_PRIVATE_KEY_PATH")
    if args.test_signing and (args.public_key_file or signing_present):
        raise ValueError("Disposable signing cannot be combined with an existing signing configuration")
    if not args.test_signing and not (args.public_key_file and signing_present):
        raise ValueError("Configure a durable public key and signing environment, or use --test-signing for an unpublished candidate")
    if args.firmware_bundle and not args.test_signing:
        raise ValueError("--firmware-bundle is for test-signed candidates only; releases build the Firmware")
    revision = clean_revision()
    out = args.out.resolve()
    if out.is_relative_to(ROOT) or ROOT.is_relative_to(out):
        raise ValueError("Output must be outside and must not contain the checkout")
    out.mkdir(parents=True, exist_ok=False)
    payload = out / "payload"
    payload.mkdir()
    if args.firmware_bundle:
        fw = prebuilt_firmware(out, revision, args.firmware_bundle)
    else:
        common = Path(capture(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"]))
        fw = firmware(out, revision, args.build_lock or common.parent / ".verify/m3-build.lock")
    version = json.loads((ELECTRON / "package.json").read_text(encoding="utf-8"))["version"]
    shutil.copyfile(ROOT / "LICENSE", payload / "LICENSE.txt")
    (payload / "licenses").mkdir()
    shutil.copyfile(ROOT / "app/icons/builtin/LICENSE-lucide.txt", payload / "licenses/LICENSE-lucide.txt")
    write_json(payload / "install-manifest.json", {"source_revision": revision, "version": version, "firmware": fw})

    env = os.environ.copy()
    for key in ("CARGO_BUILD_TARGET", "CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS"):
        env.pop(key, None)
    env["RUSTFLAGS"] = "-C target-feature=+crt-static"
    env["CARGO_TARGET_DIR"] = str(out / "build/app")
    env["TOUCHDECK_FIRMWARE_BUNDLE"] = str(payload / "firmware" / fw["bundle"])
    env["CSC_IDENTITY_AUTO_DISCOVERY"] = "false"
    key_path = None
    try:
        if args.test_signing:
            key_path = out / "disposable-test-signing.key"
            run(["node", APP / "node_modules/@tauri-apps/cli/tauri.js", "signer", "generate", "--ci", "--password", "",
                 "--write-keys", key_path], cwd=APP, quiet=True)
            public_key = key_path.with_suffix(".key.pub").read_text(encoding="utf-8").strip()
            env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = ""
        else:
            public_key = args.public_key_file.read_text(encoding="utf-8").strip()
        if not public_key:
            raise ValueError("Updater public key is empty")
        env["TOUCHDECK_UPDATER_PUBKEY"] = public_key
        pnpm = shutil.which("pnpm")
        if not pnpm:
            raise ValueError("Install pnpm and the locked app dependencies first (see docs/packaging.md)")
        run([pnpm, "--filter", "touchdeck-ui", "build"], cwd=APP, env=env)
        run(["cargo", "build", "--release", "--locked", "--target", TARGET, "-p", "td-companion"], cwd=APP, env=env)
        companion = out / "build/app" / TARGET / "release/td-companion.exe"
        verify_production_companion(companion.read_bytes())
        run([pnpm, "--filter", "@touchdeck/electron", "build"], cwd=APP, env=env)

        suffix = "-TEST-ONLY" if args.test_signing else ""
        installer = package(out, version, companion, payload, f"OpenTouchDeck-windows-x64{suffix}-setup.exe", env)
        signature = sign(installer, version, env, key_path)
        if args.test_signing:
            upcoming = next_patch(version)
            next_installer = package(out, upcoming, companion, payload, "OpenTouchDeck-windows-x64-TEST-ONLY-next-setup.exe", env)
            next_signature = sign(next_installer, upcoming, env, key_path)
    finally:
        if key_path and key_path.exists():
            key_path.unlink()

    assets = out / "release-assets"
    assets.mkdir()
    shutil.copyfile(installer, assets / installer.name)
    (assets / (installer.name + ".sig")).write_text(signature + "\n", encoding="utf-8", newline="\n")
    notes = args.notes_file.read_text(encoding="utf-8") if args.notes_file else "Open TouchDeck desktop app update."
    write_json(assets / "latest.json", manifest_entry(assets / installer.name, signature, version, notes))
    write_json(assets / "release-manifest.json", {"source_revision": revision, "version": version,
        "test_signing": args.test_signing, "updater_public_key": public_key,
        "installer": installer.name, "installer_sha256": sha(installer.read_bytes()), "firmware": fw,
        "companion_sha256": sha(companion.read_bytes()),
        "tools": {"rustc": capture(["rustc", "--version"]), "node": capture(["node", "--version"]),
                  "electron-builder": json.loads((ELECTRON / "node_modules/electron-builder/package.json").read_text())["version"]}})
    shutil.copyfile(payload / "firmware" / fw["bundle"], assets / fw["bundle"])
    paths = sorted(path for path in assets.iterdir() if path.is_file())
    (assets / "SHA256SUMS").write_text("".join(f"{sha(path.read_bytes())}  {path.name}\n" for path in paths),
                                     encoding="utf-8", newline="\n")
    if args.test_signing:
        lifecycle = out / "lifecycle"
        lifecycle.mkdir()
        shutil.copyfile(next_installer, lifecycle / next_installer.name)
        write_json(lifecycle / "next.json", {"version": upcoming, "installer": next_installer.name,
                                             "signature": next_signature, "updater_public_key": public_key})
    if clean_revision() != revision:
        raise ValueError("Source revision changed during build")
    print(json.dumps({"ok": True, "version": version, "installer": installer.name,
                      "installer_sha256": sha(installer.read_bytes()), "test_signing": args.test_signing}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build")
    builder.add_argument("--out", type=Path, required=True, help="new directory outside checkout")
    builder.add_argument("--build-lock", type=Path, help="shared ESP-IDF build lock; defaults to primary checkout .verify")
    builder.add_argument("--test-signing", action="store_true", help="generate and delete a disposable candidate-only signing key")
    builder.add_argument("--public-key-file", type=Path, help="durable updater public key; private key comes from existing signing environment")
    builder.add_argument("--notes-file", type=Path, help="release notes for latest.json")
    builder.add_argument("--firmware-bundle", type=Path, help="test-signed candidates only: reuse this .tdfw instead of building Firmware")
    checker = commands.add_parser("verify-companion", help="check a Companion binary has no test hooks")
    checker.add_argument("binary", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "build":
            build(args)
        else:
            verify_production_companion(args.binary.read_bytes())
            print(json.dumps({"ok": True, "test_hooks": False}))
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"packaging: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
