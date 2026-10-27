#!/usr/bin/env python3
"""Check or bump the desktop app version, which must agree everywhere a release reads it.

    python app/scripts/release_version.py check [--tag vX.Y.Z]
    python app/scripts/release_version.py bump X.Y.Z

check exits nonzero when app/src-tauri/tauri.conf.json, app/package.json, app/src-tauri/Cargo.toml and the
touchdeck entry in app/Cargo.lock disagree, or when --tag is not "v" plus that version. bump rewrites only
those version strings and leaves every other byte, line endings included, unchanged.
"""

import argparse
import json
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+")
CRATE = "touchdeck"
# Each pattern captures the text before and after the one version string it owns.
# A top-level JSON "version" follows only scalar members, so nested objects cannot be mistaken for it.
JSON_VERSION = re.compile(r'(\A\{\s*(?:"[^"]*"\s*:\s*(?:"[^"]*"|true|false|null|-?[0-9][0-9.eE+-]*)\s*,\s*)*?'
                          r'"version"\s*:\s*")([^"]*)(")')
PATTERNS = {
    "app/src-tauri/tauri.conf.json": JSON_VERSION,
    "app/package.json": JSON_VERSION,
    "app/src-tauri/Cargo.toml": re.compile(r'(^\[package\]\r?\n(?:[^\[\r\n][^\r\n]*\r?\n|\r?\n)*?version\s*=\s*")([^"]*)(")',
                                           re.MULTILINE),
    "app/Cargo.lock": re.compile(r'(^\[\[package\]\]\r?\nname = "' + CRATE + r'"\r?\nversion = ")([^"]*)(")', re.MULTILINE),
}


def parsed_version(relative, text):
    """The version as each file's own format reports it, so a regex slip cannot go unnoticed."""
    if relative.endswith(".json"):
        return json.loads(text).get("version")
    if relative.endswith("Cargo.toml"):
        return tomllib.loads(text).get("package", {}).get("version")
    versions = [entry.get("version") for entry in tomllib.loads(text).get("package", []) if entry.get("name") == CRATE]
    if len(versions) != 1:
        raise ValueError(f"{relative}: expected one {CRATE} package entry, found {len(versions)}")
    return versions[0]


def read_versions(root):
    versions = {}
    for relative, pattern in PATTERNS.items():
        text = (root / relative).read_bytes().decode("utf-8")
        matches = list(pattern.finditer(text))
        if len(matches) != 1:
            raise ValueError(f"{relative}: could not find exactly one version field")
        if matches[0].group(2) != parsed_version(relative, text):
            raise ValueError(f"{relative}: version field is not where the file format puts it")
        versions[relative] = matches[0].group(2)
    return versions


def check(root, tag=None):
    versions = read_versions(root)
    for relative, version in versions.items():
        print(f"{relative}: {version}")
    if len(set(versions.values())) != 1:
        raise ValueError("App versions disagree")
    version = next(iter(versions.values()))
    if not VERSION.fullmatch(version):
        raise ValueError(f"App version {version!r} is not X.Y.Z")
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"Tag {tag!r} is not v{version}")
    print(f"ok {version}" + (f" tag {tag}" if tag is not None else ""))
    return version


def bump(root, version):
    if not VERSION.fullmatch(version):
        raise ValueError(f"New version {version!r} is not X.Y.Z")
    read_versions(root)
    for relative, pattern in PATTERNS.items():
        path = root / relative
        text = path.read_bytes().decode("utf-8")
        updated = pattern.sub(lambda match: match.group(1) + version + match.group(3), text, count=1)
        # Bytes, not text mode: Windows checkouts may hold CRLF files and those must stay as they are.
        path.write_bytes(updated.encode("utf-8"))
        print(f"{relative}: {version}")
    check(root)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", type=Path, default=ROOT, help=argparse.SUPPRESS)
    commands = parser.add_subparsers(dest="command", required=True)
    checker = commands.add_parser("check", help="fail unless every version agrees (and matches --tag)")
    checker.add_argument("--tag")
    bumper = commands.add_parser("bump", help="set every version to X.Y.Z")
    bumper.add_argument("version")
    args = parser.parse_args(argv)
    try:
        check(args.root, args.tag) if args.command == "check" else bump(args.root, args.version)
    except (OSError, ValueError, UnicodeDecodeError) as error:
        print(f"release_version: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
