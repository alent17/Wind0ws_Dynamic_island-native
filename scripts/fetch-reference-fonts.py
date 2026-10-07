#!/usr/bin/env python3
"""Fetch the exact reference fonts into the existing private font build directory."""
from hashlib import sha256
from pathlib import Path
from urllib.request import urlopen

REFERENCE = "a9a855ef62d2596775115c70b82f63af8a67229e"
FONT_HASHES = {'MiSans-Bold.ttf': '250fb5c8d4657f103e332b0b948a995e45f2c159beb8d0528424e2d32ae421be', 'MiSans-Medium.ttf': '93528275ee67039c1daeb2ef700625bcf72abab2d3f97e1b6afae6273b579fc1', 'MiSans-Regular.ttf': '7172aa1b5c703780ee12df3fe4c50481dde01f3b125a045a398000a88d032ae5'}


def main():
    destination = Path(__file__).resolve().parents[1] / "local-only/assets/fonts"
    destination.mkdir(parents=True, exist_ok=True)
    for name, digest in FONT_HASHES.items():
        target = destination / name
        if target.is_file() and sha256(target.read_bytes()).hexdigest() == digest:
            print(f"{name}: verified")
            continue
        url = f"https://raw.githubusercontent.com/alent17/Wind0ws_Dynamic_island/{REFERENCE}/public/fonts/{name}"
        with urlopen(url, timeout=60) as response:
            data = response.read(16 * 1024 * 1024)
        if sha256(data).hexdigest() != digest:
            raise RuntimeError(f"Reference font checksum mismatch: {name}")
        temporary = target.with_suffix(".download")
        temporary.write_bytes(data)
        temporary.replace(target)
        print(f"{name}: downloaded and verified")


if __name__ == "__main__":
    main()
