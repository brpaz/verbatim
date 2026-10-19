#!/usr/bin/env python3
"""Record a release in the AppStream metainfo before building a release."""

from __future__ import annotations

import argparse
import re
import sys
from datetime import date
from pathlib import Path


def add_release(content: str, version: str, day: str) -> str:
    """Returns `content` with `version` released on `day`, newest first. A
    version that is already listed gets the new date instead."""
    existing = re.compile(rf'(<release version="{re.escape(version)}" date=")[^"]*(")')
    if existing.search(content):
        return existing.sub(rf"\g<1>{day}\g<2>", content, count=1)
    entry = f'<release version="{version}" date="{day}" />'
    if "<releases>" in content:
        return re.sub(r"(\n(\s*)<releases>\n)", rf"\g<1>\g<2>  {entry}\n", content, count=1)
    return content.replace(
        '<content_rating type="oars-1.1" />',
        f'<content_rating type="oars-1.1" />\n  <releases>\n    {entry}\n  </releases>',
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", help="Release version or tag (e.g. 1.0.0 or v1.0.0)")
    parser.add_argument(
        "--metainfo",
        default="data/dev.brunopaz.Reefgraph.metainfo.xml.in.in",
        help="Path to metainfo template",
    )
    args = parser.parse_args()

    metainfo_path = Path(args.metainfo)
    if not metainfo_path.exists():
        sys.exit(f"Error: {metainfo_path} not found")

    # Tags are written v1.0.0; AppStream versions have no prefix.
    version = re.sub(r"^v(?=\d)", "", args.version)
    day = date.today().isoformat()
    metainfo_path.write_text(add_release(metainfo_path.read_text(), version, day))
    print(f"Recorded release {version} ({day}) in {metainfo_path}")


if __name__ == "__main__":
    main()
