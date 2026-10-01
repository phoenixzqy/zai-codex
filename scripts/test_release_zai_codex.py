"""Exercise release installation boundaries without network access or real installs."""

import hashlib
import io
import json
import os
import sys
from pathlib import Path
import tarfile
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
        self.target = "x86_64-unknown-linux-gnu"
        self.tag = "zai-v0.1.0"
        self.package = self.root / "package"
        self.package.mkdir()
        self.notices = self.root / "notices"
        self.notices.mkdir()
        (self.notices / "dependency.txt").write_text("reviewed test notice")
        self.output = self.root / "output"
        self.output.mkdir()
        self.installs = self.root / "installed"
        self.launcher = self.root / "bin/zai-codex"

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
            previous = self.launcher.resolve()
            self.install_asset(asset)
        self.assertNotEqual(previous, self.launcher.resolve())
        self.assertTrue(previous.is_file())
        self.assertEqual(upstream.read_text(), "upstream executable")
        self.assertTrue((self.launcher.resolve().parent.parent / "NOTICE").is_file())
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
        self.launcher.unlink()
        self.launcher.symlink_to(self.root / "foreign")
        with self.assertRaisesRegex(ValueError, "foreign launcher"):
            self.install_asset(asset)
        self.assertEqual(self.launcher.readlink(), self.root / "foreign")

    def test_activation_failure_removes_only_new_package(self):
        asset = self.make_asset()
        with patch.object(installer.subprocess, "run"):
            self.install_asset(asset)
            previous = self.launcher.resolve()
            with patch.object(
                Path, "replace", side_effect=OSError("activation failed")
            ):
                with self.assertRaisesRegex(OSError, "activation failed"):
                    self.install_asset(asset)
        self.assertEqual(self.launcher.resolve(), previous)
        self.assertEqual(list(self.installs.iterdir()), [previous.parent.parent])

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

    def test_tar_traversal_and_symlinks_are_rejected(self):
        for name, kind in (("../escape", tarfile.REGTYPE), ("link", tarfile.SYMTYPE)):
            with self.subTest(name=name):
                archive = self.root / "unsafe.tar.gz"
                with tarfile.open(archive, "w:gz") as tar:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    member.linkname = "../escape"
                    member.size = 1 if kind == tarfile.REGTYPE else 0
                    tar.addfile(member, io.BytesIO(b"x"))
                checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
                with self.assertRaises(ValueError):
                    installer.verified_extract(archive, checksum, self.root / "extract")
                self.assertFalse((self.root / "escape").exists())

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

    def test_upstream_release_is_never_used_as_fallback(self):
        def download(url, destination):
            destination.write_text(json.dumps({"tag_name": "rust-v1.2.3"}))

        with patch.object(installer, "download", side_effect=download) as fetch:
            with patch.object(sys, "argv", ["install_zai_codex.py"]):
                with self.assertRaisesRegex(ValueError, "Not a custom"):
                    installer.main()
        self.assertEqual(fetch.call_count, 1)

    def test_download_pins_one_custom_release_and_installs_verified_asset(self):
        asset = self.make_asset()
        checksum = asset.with_name(asset.name + ".sha256")
        urls = []

        def download(url, destination):
            urls.append(url)
            if url.endswith("/releases/latest"):
                destination.write_text(json.dumps({"tag_name": self.tag}))
            else:
                destination.write_bytes(
                    checksum.read_bytes()
                    if url.endswith(".sha256")
                    else asset.read_bytes()
                )

        with (
            patch.object(installer, "download", side_effect=download),
            patch.object(installer, "host_target", return_value=self.target),
            patch.object(sys, "argv", ["install_zai_codex.py"]),
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
        self.assertEqual(len(urls), 3)
        self.assertTrue(
            all(f"/releases/download/{self.tag}/" in url for url in urls[1:])
        )

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
