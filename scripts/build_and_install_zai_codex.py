#!/usr/bin/env python3
"""Build a release package and install it as the codex command."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone


REPO_ROOT = Path(__file__).resolve().parent.parent


def install_root_and_link(
    install_root: Path, bin_link: Path
) -> tuple[Path, Path, Path | None]:
    install_root = install_root.expanduser().resolve()
    bin_link = bin_link.expanduser()
    bin_link.parent.mkdir(parents=True, exist_ok=True)
    install_root.mkdir(parents=True, exist_ok=True)
    bin_link = bin_link.parent.resolve() / bin_link.name
    previous_package = None
    if bin_link.is_symlink():
        previous_binary = bin_link.resolve()
        if not previous_binary.is_relative_to(install_root):
            raise ValueError(
                f"Refusing to replace a symlink outside {install_root}: {bin_link}"
            )
        previous_package = previous_binary.parent.parent
    elif bin_link.exists():
        raise ValueError(f"Refusing to replace an existing executable: {bin_link}")
    return install_root, bin_link, previous_package


def activate_install(
    install_root: Path,
    bin_link: Path,
    install_dir: Path,
    previous_package: Path | None,
    executable_name: str = "codex",
) -> None:
    _, _, current_package = install_root_and_link(install_root, bin_link)
    if current_package != previous_package:
        raise ValueError("Install destination changed during the build")
    temporary_link = bin_link.with_name(f"{bin_link.name}.tmp.{os.getpid()}")
    try:
        temporary_link.symlink_to(install_dir / "bin" / executable_name)
        temporary_link.replace(bin_link)
    finally:
        temporary_link.unlink(missing_ok=True)


def main() -> int:
    os.environ["CODEX_REPO_ROOT"] = str(REPO_ROOT)
    sys.path.insert(0, str(REPO_ROOT / "scripts"))
    from codex_package.targets import PACKAGE_VARIANTS, TARGET_SPECS, default_target
    from codex_package.version import read_workspace_version

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--target",
        choices=sorted(TARGET_SPECS),
        default=os.environ.get("CODEX_TARGET", default_target()),
    )
    args = parser.parse_args()
    spec = TARGET_SPECS[args.target]
    executable_name = PACKAGE_VARIANTS["codex"].entrypoint_name(spec)
    install_root, bin_link, previous_package = install_root_and_link(
        Path(
            os.environ.get(
                "ZAI_CODEX_INSTALL_ROOT", Path.home() / ".local/lib/zai-codex"
            )
        ),
        Path(
            os.environ.get(
                "ZAI_CODEX_BIN_LINK",
                Path.home() / ".local/bin" / f"codex{spec.exe_suffix}",
            )
        ),
    )
    version = read_workspace_version()
    commit = subprocess.check_output(
        ["git", "rev-parse", "--short=12", "HEAD"], cwd=REPO_ROOT, text=True
    ).strip()
    if subprocess.check_output(
        ["git", "status", "--porcelain"], cwd=REPO_ROOT, text=True
    ).strip():
        commit += "-dirty"
    install_id = f"{commit}-{datetime.now(timezone.utc):%Y%m%dT%H%M%SZ}-{os.getpid()}"
    install_dir = install_root / install_id
    if install_dir.exists():
        raise ValueError(f"Install destination already exists: {install_dir}")

    with tempfile.TemporaryDirectory(prefix="zai-codex-package-") as build_temp:
        package_dir = Path(build_temp) / "package"
        build_args = [
            sys.executable,
            str(REPO_ROOT / "scripts/build_codex_package.py"),
            "--target",
            args.target,
            "--variant",
            "codex",
            "--cargo-profile",
            "release",
            "--package-version",
            version,
            "--package-dir",
            str(package_dir),
        ]
        for flag, override, relative_path in (
            ("--bwrap-bin", "CODEX_BWRAP_BIN", "codex-resources/bwrap"),
            ("--rg-bin", "CODEX_RG_BIN", f"codex-path/{spec.rg_name}"),
        ):
            resource = os.environ.get(override)
            if (
                resource is None
                and previous_package
                and args.target == default_target()
            ):
                existing = previous_package / relative_path
                if existing.is_file():
                    resource = str(existing)
            if resource is not None:
                build_args.extend((flag, resource))
        subprocess.run(build_args, cwd=REPO_ROOT, check=True)

        with tempfile.TemporaryDirectory(
            prefix=".zai-codex-stage-", dir=install_root
        ) as stage_temp:
            stage = Path(stage_temp) / "package"
            shutil.copytree(package_dir, stage)
            actual_version = subprocess.check_output(
                [str(stage / "bin" / executable_name), "--version"], text=True
            ).strip()
            if actual_version != f"codex-cli {version}":
                raise ValueError(f"Unexpected Codex version: {actual_version}")
            subprocess.run(
                [
                    str(stage / "bin" / executable_name),
                    "login",
                    "github-copilot",
                    "--help",
                ],
                check=True,
                stdout=subprocess.DEVNULL,
            )
            metadata = json.loads((stage / "codex-package.json").read_text())
            if any(
                metadata.get(key) != value
                for key, value in {
                    "version": version,
                    "target": args.target,
                    "variant": "codex",
                }.items()
            ):
                raise ValueError("Package metadata does not match the requested build")
            stage.rename(install_dir)

    activate_install(
        install_root, bin_link, install_dir, previous_package, executable_name
    )
    print(f"Installed {actual_version} at {install_dir}")
    print(f"Active command: {bin_link}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
