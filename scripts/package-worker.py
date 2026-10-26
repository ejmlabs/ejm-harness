#!/usr/bin/env python3
"""Archive a built ejm-harness-worker with its licence and print its SHA-256."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXTRAS = ("LICENSE", "THIRD_PARTY_NOTICES.md")
MTIME = 0


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def add(archive: tarfile.TarFile, source: Path, name: str, mode: int) -> None:
    info = tarfile.TarInfo(name)
    data = source.read_bytes()
    info.size = len(data)
    info.mode = mode
    info.mtime = MTIME
    info.uname = info.gname = ""
    archive.addfile(info, io.BytesIO(data))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", required=True, help="for example linux-x86_64")
    parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args()
    if (ROOT / "VERSION").read_text().strip() != args.version:
        print("version does not match VERSION", file=sys.stderr)
        return 1
    args.out_dir.mkdir(parents=True, exist_ok=True)
    stem = f"ejm-harness-worker-{args.version}-{args.target}"
    archive_path = args.out_dir / f"{stem}.tar.gz"
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as archive:
        add(archive, args.binary, f"{stem}/ejm-harness-worker", 0o755)
        for extra in EXTRAS:
            add(archive, ROOT / extra, f"{stem}/{extra}", 0o644)
    with (
        archive_path.open("wb") as raw,
        gzip.GzipFile(fileobj=raw, mode="wb", mtime=MTIME, filename="") as compressed,
    ):
        compressed.write(buffer.getvalue())
    print(f"{sha256(archive_path)}  {archive_path.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
