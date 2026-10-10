#!/usr/bin/env python3
"""Prepare pinned notices and verify a complete native release before upload."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "x64"),
    "aarch64-unknown-linux-gnu": ("linux", "arm64"),
    "x86_64-apple-darwin": ("macos", "x64"),
    "aarch64-apple-darwin": ("macos", "arm64"),
    "x86_64-pc-windows-msvc": ("windows", "x64"),
    "aarch64-pc-windows-msvc": ("windows", "arm64"),
}


def prepare_notices(output):
    pin = json.loads((ROOT / "scripts/zai-codex-notices.json").read_text())
    fingerprint = hashlib.sha256()
    for name in pin["dependencyFiles"]:
        fingerprint.update(name.encode() + b"\0" + (ROOT / name).read_bytes() + b"\0")
    if fingerprint.hexdigest() != pin["dependencySha256"]:
        raise ValueError(
            "Release dependencies changed; review and refresh the notice pin"
        )
    with urllib.request.urlopen(pin["url"], timeout=120) as response:
        body = response.read(10 * 1024 * 1024 + 1)
    if hashlib.sha256(body).hexdigest() != pin["sha256"]:
        raise ValueError("Reviewed notice checksum mismatch")
    sections = re.findall(
        r"(?ms)^===== third-party-notices/([^/\r\n]+) =====\nPackages: [^\n]+\n\n(.*?)(?=\n===== |\Z)",
        body.decode("utf-8"),
    )
    if not sections:
        raise ValueError("Reviewed third-party notices missing")
    output.mkdir(parents=True, exist_ok=False)
    for name, text in sections:
        if name in (".", "..") or "\\" in name or ":" in name:
            raise ValueError("Unsafe notice filename")
        (output / name).write_text(text, encoding="utf-8")


def release_manifest(directory, version, commit):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version) or not re.fullmatch(
        r"[0-9a-f]{40}", commit
    ):
        raise ValueError("Invalid release version or source commit")
    expected = {f"zai-codex-{version}-{target}.zip" for target in TARGETS}
    if {p.name for p in directory.glob("*.zip")} != expected:
        raise ValueError("Release must contain exactly all six native packages")
    assets = []
    for target, (platform, architecture) in TARGETS.items():
        path = directory / f"zai-codex-{version}-{target}.zip"
        checksum = hashlib.sha256()
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                checksum.update(block)
        digest = checksum.hexdigest()
        if path.with_name(path.name + ".sha256").read_text().strip() != digest:
            raise ValueError("Package checksum mismatch")
        with zipfile.ZipFile(path) as archive:
            if archive.testzip():
                raise ValueError("Corrupt package")
            provenance = json.loads(archive.read("zai-release.json"))
            if provenance != {
                "repository": "phoenixzqy/zai-codex",
                "branch": "zai-codex",
                "commit": commit,
                "tag": f"zai-codex-v{version}",
                "version": version,
            }:
                raise ValueError("Package provenance mismatch")
            for name in archive.namelist():
                parts = Path(name).parts
                if (
                    name.startswith("/")
                    or ".." in parts
                    or "\\" in name
                    or ":" in name
                    or any(
                        part.lower().endswith(
                            (".pdb", ".map", ".debug", ".dsym", ".dwo", ".dwp")
                        )
                        or part.lower()
                        in (".git", ".env", "auth.json", "credentials.json")
                        for part in parts
                    )
                ):
                    raise ValueError(f"Private/debug or unsafe package member: {name}")
        assets.append(
            {
                "name": f"zai-codex {platform} {architecture}",
                "platform": platform,
                "architecture": architecture,
                "file": path.name,
                "bytes": path.stat().st_size,
                "sha256": digest,
                "signing": "unsigned",
                "url": f"https://github.com/phoenixzqy/zai-codex/releases/download/zai-codex-v{version}/{path.name}",
                "installNotes": "Unsigned native preview. Linux requires compatible glibc; macOS is not notarized.",
            }
        )
    return {
        "schemaVersion": 1,
        "appId": "zai-codex",
        "sourceCommit": commit,
        "release": {
            "version": version,
            "channel": "preview",
            "notes": [
                "All six packages built and smoke-tested on native GitHub runners. Packages are unsigned; macOS is not notarized.",
                "Package verification covers version, Copilot login help and isolated installation; the full source test suite and interactive login were not run.",
            ],
            "assets": assets,
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("notices", "smoke", "publish"))
    parser.add_argument("--directory", required=True, type=Path)
    parser.add_argument("--version")
    parser.add_argument("--commit")
    args = parser.parse_args()
    if args.mode == "notices":
        prepare_notices(args.directory)
        return
    if args.mode == "smoke":
        import tempfile
        from install_zai_codex import host_target, install

        target = host_target()
        archive = args.directory / f"zai-codex-{args.version}-{target}.zip"
        with tempfile.TemporaryDirectory(prefix="zai-codex-smoke-") as temporary:
            root = Path(temporary)
            launcher = root / (
                "bin/codex.cmd" if target.endswith("windows-msvc") else "bin/codex"
            )
            install(
                archive,
                archive.with_name(archive.name + ".sha256").read_text().strip(),
                f"zai-codex-v{args.version}",
                target,
                root / "bundles",
                launcher,
            )
        return
    from datetime import datetime, timezone

    data = release_manifest(args.directory, args.version, args.commit)
    data["release"]["publishedAt"] = datetime.now(timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )
    manifest = args.directory / "manifest.json"
    manifest.write_text(json.dumps(data, indent=2) + "\n")
    record = {
        "repository": "phoenixzqy/zai-codex",
        "branch": "zai-codex",
        "commit": args.commit,
        "dependencies": {},
    }
    notes = args.directory / "release-notes.md"
    notes.write_text(
        "\n\n".join(data["release"]["notes"])
        + "\n\n<!-- app-release-provenance "
        + json.dumps(record)
        + " -->\n"
    )
    tag = f"zai-codex-v{args.version}"
    subprocess.run(
        [
            "gh",
            "release",
            "create",
            tag,
            "--repo",
            os.environ["GITHUB_REPOSITORY"],
            "--target",
            args.commit,
            "--draft",
            "--prerelease",
            "--title",
            f"zai-codex {args.version}",
            "--notes-file",
            str(notes),
            str(manifest),
            *[str(args.directory / a["file"]) for a in data["release"]["assets"]],
            *[
                str(args.directory / (a["file"] + ".sha256"))
                for a in data["release"]["assets"]
            ],
        ],
        check=True,
    )
    subprocess.run(
        [
            "gh",
            "release",
            "edit",
            tag,
            "--repo",
            os.environ["GITHUB_REPOSITORY"],
            "--draft=false",
        ],
        check=True,
    )


if __name__ == "__main__":
    main()
