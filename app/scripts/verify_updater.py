#!/usr/bin/env python3
"""Verify an installer with the real Tauri updater over disposable local HTTPS, without installing it."""
import argparse
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import ssl
import subprocess
import threading

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path, help="new external fixture directory")
    parser.add_argument("--cargo-target", required=True, type=Path)
    parser.add_argument("--openssl", default="openssl")
    parser.add_argument("--mt", type=Path, help="Windows SDK manifest tool (auto-detected from SDK install)")
    args = parser.parse_args()
    out = args.out.resolve()
    if out.is_relative_to(ROOT):
        raise ValueError("Fixture output must be outside the checkout")
    out.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((args.assets / "release-manifest.json").read_text(encoding="utf-8"))
    shutil.copyfile(args.assets / "release-manifest.json", out / "release-manifest.json")
    installer = (args.assets / manifest["installer"]).read_bytes()
    (out / "installer.exe").write_bytes(installer)
    (out / "corrupt.exe").write_bytes(installer + b"invalid-signature-fixture")
    private_tls = out / "fixture-tls.key"
    subprocess.run([args.openssl, "req", "-x509", "-newkey", "rsa:2048", "-sha256", "-days", "1", "-nodes",
                    "-keyout", str(private_tls), "-out", str(out / "fixture-ca.pem"), "-subj", "/CN=localhost",
                    "-addext", "subjectAltName=DNS:localhost"], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(out))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(out / "fixture-ca.pem", private_tls)
    server.socket = tls.wrap_socket(server.socket, server_side=True)
    url = f"https://localhost:{server.server_port}"
    latest = json.loads((args.assets / "latest.json").read_text(encoding="utf-8"))
    for name, version, asset in [("good", manifest["version"], "installer.exe"),
                                  ("corrupt", manifest["version"], "corrupt.exe"),
                                  ("wrong-version", "999.0.0", "installer.exe"),
                                  ("current", "0.0.0", "installer.exe")]:
        value = {"version": version, "notes": "Disposable local HTTPS fixture", "platforms": {
            "windows-x86_64": {"url": f"{url}/{asset}",
                                  "signature": latest["platforms"]["windows-x86_64"]["signature"]}}}
        (out / f"{name}.json").write_text(json.dumps(value), encoding="utf-8", newline="\n")
    (out / "malformed.json").write_text('{"version": false}', encoding="utf-8")
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(args.cargo_target.resolve())
    env["RUSTFLAGS"] = "-C target-feature=+crt-static"
    env["TOUCHDECK_UPDATER_FIXTURE_DIR"] = str(out)
    env["TOUCHDECK_UPDATER_FIXTURE_URL"] = url
    try:
        result = subprocess.run(["cargo", "test", "--locked", "--release", "--target", "x86_64-pc-windows-msvc",
                                 "-p", "touchdeck", "--features", "updater-fixture", "--lib", "--no-run",
                                 "--message-format=json"], cwd=ROOT / "app", env=env, stdout=subprocess.PIPE, text=True)
        if result.returncode:
            print(result.stdout)
            return result.returncode
        executables = [Path(item["executable"]) for line in result.stdout.splitlines()
                       if (item := json.loads(line)).get("reason") == "compiler-artifact"
                       and item.get("executable") and item["profile"]["test"]]
        if len(executables) != 1:
            raise ValueError("Expected one updater fixture test executable")
        # The Tauri mock runtime links Common Controls v6. Cargo's lib test executable has no app manifest.
        # Embed its activation context into this disposable test executable, never into the signed installer.
        tools = sorted(Path("C:/Program Files (x86)/Windows Kits/10/bin").glob("*/x64/mt.exe"))
        mt = args.mt or (tools[-1] if tools else None)
        if not mt:
            raise ValueError("Windows SDK mt.exe is required for the mock-runtime fixture")
        activation = out / "windows-test.manifest"
        activation.write_text('<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">'
                              '<dependency><dependentAssembly><assemblyIdentity type="win32" '
                              'name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" '
                              'publicKeyToken="6595b64144ccf1df" language="*"/>'
                              '</dependentAssembly></dependency></assembly>', encoding="utf-8")
        subprocess.run([str(mt), "-manifest", str(activation), f"-outputresource:{executables[0]};#1"], check=True)
        return subprocess.run([str(executables[0]), "app_updates::tests::signed_https_fixture", "--ignored", "--exact",
                               "--nocapture"], cwd=ROOT / "app", env=env).returncode
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
        private_tls.unlink()


if __name__ == "__main__":
    raise SystemExit(main())
