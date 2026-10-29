"""Regenerate the offline catalog from immutable upstream inputs (Python stdlib only).

Use --inputs DIR to reuse downloaded inputs, including for offline reproducibility checks.
"""
import argparse
import gzip
import io
import json
from pathlib import Path
import tarfile
import urllib.request

SIMPLE = "16.36.0"
SELFHST = "02dd039d62a8e7132f4828e69162fca30bd43bb5"
OUTPUT = Path(__file__).resolve().parents[1] / "crates/td-host/src/catalog/index.json"


def strings(value):
    if isinstance(value, str):
        return [value]
    if isinstance(value, dict):
        return [s for v in value.values() for s in strings(v)]
    return [s for v in value for s in strings(v)]


def generate(simple, selfhst):
    entries = []
    for icon in simple:
        license = icon.get("license", {"type": "CC0-1.0"})
        entries.append(dict(source="simple-icons", name=icon["title"], id=icon["slug"],
            aliases=sorted(set(strings(icon.get("aliases", {})))), hex=icon["hex"],
            tags=[], category=None, svg=True, light=False, dark=False,
            license=license["type"], license_url=license.get("url", "https://spdx.org/licenses/" + license["type"] + ".html") if license["type"] != "custom" else license.get("url"),
            guidelines=icon.get("guidelines"), attribution_url=icon["source"]))
    for icon in selfhst:
        entries.append(dict(source="selfhst", name=icon["Name"], id=icon["Reference"],
            aliases=[], hex=None, tags=sorted(t.strip() for t in icon["Tags"].split(",") if t.strip()),
            category=icon["Category"], svg=icon["SVG"] == "Yes", light=icon["Light"] == "Yes", dark=icon["Dark"] == "Yes",
            license="CC-BY-4.0", license_url="https://creativecommons.org/licenses/by/4.0/",
            guidelines=None, attribution_url="https://selfh.st/icons/"))
    entries.sort(key=lambda e: (e["name"].lower(), e["source"], e["id"]))
    # Omit empty optional values; Rust supplies their defaults.
    entries = [{k: v for k, v in e.items() if v not in (None, [], False)} for e in entries]
    return (json.dumps(dict(simple_icons_version=SIMPLE, selfhst_commit=SELFHST, entries=entries), ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    def read(name, url):
        if args.inputs and (args.inputs / name).exists():
            return (args.inputs / name).read_bytes()
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read()
        if args.inputs:
            args.inputs.mkdir(parents=True, exist_ok=True)
            (args.inputs / name).write_bytes(data)
        return data
    archive = read("simple-icons.tgz", f"https://registry.npmjs.org/simple-icons/-/simple-icons-{SIMPLE}.tgz")
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        # The npm package includes canonical slugs, unlike the source repository index.
        simple = json.load(tar.extractfile("package/data/simple-icons.json"))
    selfhst = json.loads(read("selfhst.json", f"https://raw.githubusercontent.com/selfhst/icons/{SELFHST}/index.json"))
    data = generate(simple, selfhst)
    if args.check:
        assert OUTPUT.read_bytes() == data, "catalog differs; regenerate it"
    else:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_bytes(data)
    print(f"Catalog: {len(simple)} Simple Icons, {len(selfhst)} selfh.st; {len(data)} raw bytes, {len(gzip.compress(data, mtime=0))} gzip bytes")

if __name__ == "__main__":
    main()
