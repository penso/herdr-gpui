#!/usr/bin/env python3
"""Prints the next build number for an app: one more than the highest
build App Store Connect has for it, across every version, or 1 for an
app with none yet.

Usage: python3 next-build.py BUNDLE_ID
"""

import sys

import app_store_connect as asc

# Builds per page; the highest number is among the newest uploads, and
# one page of them covers far more than a burst of parallel uploads.
PAGE = 200


def highest_build(versions: list) -> int:
    """The highest numeric build among `versions`; others are skipped."""
    numbers = [int(version) for version in versions if version.isdigit()]
    return max(numbers, default=0)


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit("Usage: python3 next-build.py BUNDLE_ID")
    bundle_id = sys.argv[1]
    try:
        bearer = asc.bearer_from_env()
    except KeyError as missing:
        sys.exit(f"next-build: {missing.args[0]} is not set")
    app = asc.app_id(bundle_id, bearer)
    if app is None:
        sys.exit(f"next-build: App Store Connect has no app {bundle_id}")
    builds = asc.get(
        "builds",
        {
            "filter[app]": app,
            "sort": "-uploadedDate",
            "fields[builds]": "version",
            "limit": PAGE,
        },
        bearer,
    )
    print(highest_build([item["attributes"]["version"] for item in builds["data"]]) + 1)


if __name__ == "__main__":
    main()
