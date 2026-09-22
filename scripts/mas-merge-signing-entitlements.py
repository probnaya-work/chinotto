#!/usr/bin/env python3
"""
Merge base MAS entitlements with application id + team id from a Mac App Store .provisionprofile.
Fixes TestFlight validation 90886 (signature missing application identifier while profile has one).
"""
from __future__ import annotations

import os
import plistlib
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def decode_provisioning_profile(path: Path) -> dict:
    raw = subprocess.check_output(
        ["security", "cms", "-D", "-i", str(path)],
        stderr=subprocess.PIPE,
    )
    return plistlib.loads(raw)


def main() -> int:
    if len(sys.argv) != 4:
        print(
            "usage: mas-merge-signing-entitlements.py <base.plist> <profile.provisionprofile> <out.plist>",
            file=sys.stderr,
        )
        return 2
    base_path, prof_path, out_path = map(Path, sys.argv[1:4])
    with base_path.open("rb") as f:
        merged: dict = plistlib.load(f)

    prof = decode_provisioning_profile(prof_path)
    expires = prof.get("ExpirationDate")
    if isinstance(expires, datetime):
        comparable = expires if expires.tzinfo else expires.replace(tzinfo=timezone.utc)
        if comparable <= datetime.now(timezone.utc):
            print(f"error: provisioning profile expired at {expires}", file=sys.stderr)
            return 1
    team = prof.get("TeamIdentifier")
    if team is None:
        print("error: provisioning profile has no TeamIdentifier", file=sys.stderr)
        return 1
    if isinstance(team, list):
        if not team:
            print("error: TeamIdentifier array is empty", file=sys.stderr)
            return 1
        team = team[0]
    team = str(team)

    ent = prof.get("Entitlements")
    app_id = None
    if isinstance(ent, dict):
        app_id = ent.get("com.apple.application-identifier") or ent.get(
            "application-identifier"
        )
    if app_id:
        app_id = str(app_id)
    else:
        bundle_id = os.environ.get("BUNDLE_ID", "").strip()
        if not bundle_id:
            print(
                "error: profile Entitlements lack application-identifier; set BUNDLE_ID",
                file=sys.stderr,
            )
            return 1
        app_id = f"{team}.{bundle_id}"

    bundle_id = os.environ.get("BUNDLE_ID", "").strip()
    expected_app_id = f"{team}.{bundle_id}" if bundle_id else ""
    if expected_app_id and app_id != expected_app_id:
        print(
            f"error: profile application-identifier is {app_id!r}, expected {expected_app_id!r}",
            file=sys.stderr,
        )
        return 1

    if not isinstance(ent, dict):
        print("error: provisioning profile has no Entitlements dictionary", file=sys.stderr)
        return 1
    for key in ("com.apple.developer.applesignin",):
        requested = merged.get(key)
        if requested is not None and ent.get(key) != requested:
            print(
                f"error: profile does not authorize requested entitlement {key}",
                file=sys.stderr,
            )
            return 1
    if ent.get("get-task-allow") is True:
        print("error: development provisioning profile is not valid for App Store distribution", file=sys.stderr)
        return 1

    merged["com.apple.application-identifier"] = app_id
    merged["com.apple.developer.team-identifier"] = team
    # Distribution must not enable get-task-allow
    merged.pop("get-task-allow", None)
    merged.pop("com.apple.security.get-task-allow", None)

    out_path.parent.mkdir(parents=True, exist_ok=True)
    # XML plist: codesign reliably consumes this for --entitlements (binary merge caused 90886 for some uploads).
    with out_path.open("wb") as f:
        plistlib.dump(merged, f, fmt=plistlib.FMT_XML)
    print(
        f"merged entitlements: application-identifier={app_id!r} team={team!r} -> {out_path}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
