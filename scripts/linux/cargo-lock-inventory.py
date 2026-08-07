#!/usr/bin/env python3
"""Emit a stable security inventory for Cargo.lock.

The inventory intentionally ignores Cargo's serialization details and dependency
reference spelling. It binds the exact package identities that may be fetched:
name, release, source (including an exact Git commit), and registry checksum.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError as exc:  # pragma: no cover - guarded by installer
    raise SystemExit("Python 3.11 or newer is required to validate Cargo.lock") from exc


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("lockfile", type=Path)
    args = parser.parse_args()

    try:
        document = tomllib.loads(args.lockfile.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        print(f"cannot parse {args.lockfile}: {exc}", file=sys.stderr)
        return 2

    packages = document.get("package")
    if not isinstance(packages, list) or not packages:
        print(f"{args.lockfile} has no package inventory", file=sys.stderr)
        return 2

    rows: list[tuple[str, str, str, str]] = []
    for index, package in enumerate(packages):
        if not isinstance(package, dict):
            print(f"package entry {index} is not a table", file=sys.stderr)
            return 2
        name = package.get("name")
        release = package.get("version")
        if not isinstance(name, str) or not isinstance(release, str):
            print(f"package entry {index} lacks a string name or release", file=sys.stderr)
            return 2
        source = package.get("source", "path:workspace")
        checksum = package.get("checksum", "no-registry-checksum")
        if not isinstance(source, str) or not isinstance(checksum, str):
            print(f"package {name} {release} has invalid source/checksum fields", file=sys.stderr)
            return 2
        rows.append((name, release, source, checksum))

    for row in sorted(rows):
        print("\t".join(row))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
