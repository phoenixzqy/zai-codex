"""Exercise release installation boundaries without network access or real installs."""

import hashlib
import json
import os
import sys
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import install_zai_codex as installer
import prepare_zai_codex_release as release


class CustomReleaseTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zai-release-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.windows = os.name == "nt"
        self.target = (
            "x86_64-pc-windows-msvc" if self.windows else "x86_64-unknown-linux-gnu"
        )
        self.tag = "zai-codex-v0.1.0"
        self.package = self.root / "package"
        self.package.mkdir()
        self.notices = self.root / "notices"
        self.notices.mkdir()
        (self.notices / "dependency.txt").write_text("reviewed test notice")
        self.output = self.root / "output"
        self.output.mkdir()
        self.installs = self.root / "installed"
        self.launcher = self.root / (
            "bin/zai-codex.cmd" if self.windows else "bin/zai-codex"
        )

    def make_asset(self, target=None):
        target = target or self.target
        windows = target.endswith("windows-msvc")
        suffix = ".exe" if windows else ""
        for name in (
            f"bin/codex{suffix}",
            f"bin/codex-code-mode-host{suffix}",
            f"codex-path/rg{suffix}",
            *(
                (
                    "codex-resources/codex-command-runner.exe",
                    "codex-resources/codex-windows-sandbox-setup.exe",
                )
                if windows
                else ("codex-resources/bwrap",)
            ),
        ):
            path = self.package / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("test executable")
            path.chmod(0o755)
        (self.package / "codex-package.json").write_text(
            json.dumps(
                {
                    "layoutVersion": 1,
                    "target": target,
                    "variant": "codex",
                    "version": "0.1.0",
                    "entrypoint": f"bin/codex{suffix}",
                }
            )
        )
        return release.package_release(
            self.package, self.output, target, "0.1.0", "a" * 40, self.notices
        )

    def install_asset(self, asset, target=None, launcher=None):
        installer.install(
            asset,
            asset.with_name(asset.name + ".sha256").read_text(),
            self.tag,
            target or self.target,
            self.installs,
            launcher or self.launcher,
        )

    def test_install_and_upgrade_preserve_upstream_and_previous_package(self):
        asset = self.make_asset()
        upstream = self.launcher.with_name("codex")
        upstream.parent.mkdir()
        upstream.write_text("upstream executable")
        with patch.object(installer.subprocess, "run"):
            self.install_asset(asset)
            previous = next(self.installs.iterdir())
            self.install_asset(asset)
        self.assertEqual(len(list(self.installs.iterdir())), 2)
        self.assertTrue(previous.is_dir())
        self.assertEqual(upstream.read_text(), "upstream executable")
        self.assertTrue((previous / "NOTICE").is_file())
        self.assertFalse(list(self.installs.glob(".zai-stage-*")))

    def test_windows_uses_launcher_without_symlink_privileges(self):
        target = "aarch64-pc-windows-msvc"
        asset = self.make_asset(target)
        launcher = self.launcher.with_suffix(".cmd")
        with patch.object(installer.subprocess, "run"):
            self.install_asset(asset, target, launcher)
            self.install_asset(asset, target, launcher)
        self.assertFalse(launcher.is_symlink())
        self.assertTrue(launcher.read_text().startswith(installer.WINDOWS_MARKER))
        self.assertIn('codex.exe" %*', launcher.read_text())

    def test_bad_checksum_never_executes_or_activates(self):
        asset = self.make_asset()
        with patch.object(installer.subprocess, "run") as execute:
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                installer.install(
                    asset, "0" * 64, self.tag, self.target, self.installs, self.launcher
                )
        execute.assert_not_called()
        self.assertFalse(self.launcher.exists())
        self.assertEqual(list(self.installs.iterdir()), [])

    def test_foreign_executable_and_symlink_are_preserved(self):
        asset = self.make_asset()
        self.launcher.parent.mkdir()
        self.launcher.write_text("foreign executable")
        with self.assertRaisesRegex(ValueError, "existing executable"):
            self.install_asset(asset)
        self.assertEqual(self.launcher.read_text(), "foreign executable")
        if self.windows:
            return  # Windows uses regular .cmd launchers, never Unix symlinks.
        self.launcher.unlink()
        self.launcher.symlink_to(self.root / "foreign")
        with self.assertRaisesRegex(ValueError, "foreign launcher"):
            self.install_asset(asset)
        self.assertEqual(self.launcher.readlink(), self.root / "foreign")

    def test_activation_failure_removes_only_new_package(self):
        asset = self.make_asset()
        with patch.object(installer.subprocess, "run"):
            self.install_asset(asset)
            previous = next(self.installs.iterdir())
            launcher_state = installer.launcher_state(
                self.launcher, self.installs, self.windows
            )
            with patch.object(
                Path, "replace", side_effect=OSError("activation failed")
            ):
                with self.assertRaisesRegex(OSError, "activation failed"):
                    self.install_asset(asset)
        self.assertEqual(
            installer.launcher_state(self.launcher, self.installs, self.windows),
            launcher_state,
        )
        self.assertEqual(list(self.installs.iterdir()), [previous])

    def test_target_mismatch_and_failed_smoke_leave_no_install(self):
        asset = self.make_asset()
        for target, failure in (
            ("aarch64-unknown-linux-gnu", "metadata mismatch"),
            (self.target, "smoke failed"),
        ):
            with patch.object(
                installer.subprocess, "run", side_effect=OSError("smoke failed")
            ):
                with self.assertRaisesRegex((ValueError, OSError), failure):
                    self.install_asset(asset, target)
            self.assertEqual(list(self.installs.iterdir()), [])

    def test_zip_traversal_and_symlinks_are_rejected(self):
        for name, mode in (("../escape", 0o100644), ("link", 0o120777)):
            archive = self.root / "unsafe.zip"
            with zipfile.ZipFile(archive, "w") as zip:
                member = zipfile.ZipInfo(name)
                member.external_attr = mode << 16
                zip.writestr(member, "../escape")
            with self.assertRaises(ValueError):
                installer.verified_extract(
                    archive,
                    hashlib.sha256(archive.read_bytes()).hexdigest(),
                    self.root / "extract",
                )

    def test_unpublished_and_foreign_manifests_do_not_download_binaries(self):
        for manifest in (
            {"schemaVersion": 1, "appId": "zai-codex", "release": None},
            {"schemaVersion": 1, "appId": "codex", "release": None},
        ):
            with patch.object(
                installer,
                "download",
                side_effect=lambda url, path: path.write_text(json.dumps(manifest)),
            ) as fetch:
                with patch.object(sys, "argv", ["installer"]):
                    with self.assertRaises(ValueError):
                        installer.main()
            self.assertEqual(fetch.call_count, 1)

    def test_download_uses_one_manifest_and_installs_verified_asset(self):
        asset = self.make_asset()
        manifest = {
            "schemaVersion": 1,
            "appId": "zai-codex",
            "release": {
                "version": "0.1.0",
                "assets": [
                    {
                        "platform": "windows" if self.windows else "linux",
                        "architecture": "x64",
                        "file": asset.name,
                        "bytes": asset.stat().st_size,
                        "sha256": asset.with_name(asset.name + ".sha256")
                        .read_text()
                        .strip(),
                    }
                ],
            },
        }
        urls = []

        def download(url, path):
            urls.append(url)
            path.write_bytes(
                json.dumps(manifest).encode()
                if url.endswith("manifest.json")
                else asset.read_bytes()
            )

        with (
            patch.object(installer, "download", side_effect=download),
            patch.object(installer, "host_target", return_value=self.target),
            patch.object(
                installer.platform,
                "system",
                return_value="Windows" if self.windows else "Linux",
            ),
            patch.object(sys, "argv", ["installer"]),
            patch.object(installer.subprocess, "run"),
            patch.dict(
                os.environ,
                {
                    "ZAI_CODEX_INSTALL_ROOT": str(self.installs),
                    "ZAI_CODEX_BIN_LINK": str(self.launcher),
                },
            ),
        ):
            self.assertEqual(installer.main(), 0)
        self.assertTrue(self.launcher.resolve().is_file())
        self.assertEqual(len(urls), 2)
        self.assertTrue(urls[1].endswith(asset.name))

    def test_packaging_requires_notices_and_refuses_overwrite(self):
        self.make_asset()
        with self.assertRaisesRegex(ValueError, "already exists"):
            release.package_release(
                self.package, self.output, self.target, "0.1.0", "a" * 40, self.notices
            )
        with self.assertRaisesRegex(ValueError, "nonempty reviewed"):
            release.package_release(
                self.package,
                self.output,
                self.target,
                "0.1.0",
                "a" * 40,
                self.root / "missing",
            )

    def test_release_commit_rejects_dirty_and_upstream_checkout(self):
        for results in ((" M README.md",), ("", "a" * 40, "b" * 40)):
            with patch.object(release.subprocess, "check_output", side_effect=results):
                with self.assertRaises(ValueError):
                    release.release_commit()


if __name__ == "__main__":
    unittest.main()
