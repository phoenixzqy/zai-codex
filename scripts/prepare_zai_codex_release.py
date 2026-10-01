#!/usr/bin/env python3
"""Build one native customized release asset with notices and a checksum."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parent.parent
os.environ["CODEX_REPO_ROOT"] = str(ROOT)
sys.path.insert(0, str(ROOT / "scripts"))

from codex_package.archive import write_archive
from codex_package.cli import parse_package_version
from codex_package.targets import TARGET_SPECS
from install_zai_codex import host_target, TAG_PATTERN


def release_commit():
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()

    if git("status", "--porcelain"):
        raise ValueError("Release builds require a clean committed checkout")
    commit = git("rev-parse", "HEAD")
    if commit != git("rev-parse", "origin/zai-codex"):
        raise ValueError("Fetch origin first; releases must use origin/zai-codex HEAD")
    return commit


def package_release(package, output, target, version, commit, notices):
    if not notices.is_dir() or not any(path.is_file() for path in notices.rglob("*")):
        raise ValueError("A nonempty reviewed third-party notice directory is required")
    extension = "zip" if TARGET_SPECS[target].is_windows else "tar.gz"
    asset = output / f"zai-codex-{target}.{extension}"
    checksum = asset.with_name(asset.name + ".sha256")
    if asset.exists() or checksum.exists():
        raise ValueError("Release output already exists; use a new output directory")
    for name in ("LICENSE", "NOTICE"):
        shutil.copy2(ROOT / name, package / name)
    shutil.copytree(notices, package / "third-party-notices")
    (package / "MODIFICATIONS.txt").write_text(
        "zai-codex is a modified OpenAI Codex distribution.\n"
        "Fork modifications: GitHub Copilot subscription support; telemetry,\n"
        "remote feedback, and error-report uploads disabled. Local logs remain.\n"
        f"Source: https://github.com/phoenixzqy/zai-codex/tree/{commit}\n"
    )
    (package / "zai-release.json").write_text(
        json.dumps(
            {
                "repository": "phoenixzqy/zai-codex",
                "branch": "zai-codex",
                "commit": commit,
                "tag": f"zai-v{version}",
            },
            indent=2,
        )
        + "\n"
    )
    try:
        write_archive(package, asset, force=False)
        digest = hashlib.sha256()
        with asset.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
        checksum.write_text(digest.hexdigest() + "\n")
    except BaseException:
        asset.unlink(missing_ok=True)
        checksum.unlink(missing_ok=True)
        raise
    return asset


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, type=parse_package_version)
    parser.add_argument("--target", choices=sorted(TARGET_SPECS), default=host_target())
    parser.add_argument("--third-party-notices", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not TAG_PATTERN.fullmatch(f"zai-v{args.version}"):
        raise ValueError("Release version must not contain build metadata")
    if args.target != host_target():
        raise ValueError("Build and smoke-test each release on its native target host")
    commit = release_commit()
    notices = args.third_party_notices.resolve()
    if not notices.is_dir() or not any(path.is_file() for path in notices.rglob("*")):
        raise ValueError("A nonempty reviewed third-party notice directory is required")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="zai-codex-release-") as temporary:
        package = Path(temporary) / "package"
        subprocess.run(
            [
                sys.executable,
                "-B",
                str(ROOT / "scripts/build_codex_package.py"),
                "--target",
                args.target,
                "--cargo-profile",
                "release",
                "--package-version",
                args.version,
                "--package-dir",
                str(package),
            ],
            cwd=ROOT,
            check=True,
        )
        if release_commit() != commit:
            raise ValueError("Source commit changed during the release build")
        from install_zai_codex import validate_package

        asset = package_release(
            package, output, args.target, args.version, commit, notices
        )
        try:
            validate_package(package, args.target, f"zai-v{args.version}")
        except BaseException:
            asset.unlink()
            asset.with_name(asset.name + ".sha256").unlink()
            raise
    print(f"Prepared {asset} from {commit}; not uploaded.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error))
