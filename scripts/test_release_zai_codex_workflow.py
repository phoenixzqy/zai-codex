"""Test complete-matrix publication and pinned notice extraction without publishing."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import zai_codex_workflow as workflow


class WorkflowReleaseTest(unittest.TestCase):
    def test_matrix_checks_all_provenance_before_publication(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            version, commit = "0.1.2", "a" * 40
            for target in workflow.TARGETS:
                path = directory / f"zai-codex-{version}-{target}.zip"
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr(
                        "zai-release.json",
                        json.dumps(
                            {
                                "repository": "phoenixzqy/zai-codex",
                                "branch": "zai-codex",
                                "commit": commit,
                                "tag": f"zai-codex-v{version}",
                                "version": version,
                            }
                        ),
                    )
                path.with_name(path.name + ".sha256").write_text(
                    hashlib.sha256(path.read_bytes()).hexdigest()
                )
            data = workflow.release_manifest(directory, version, commit)
            self.assertEqual(
                {(a["platform"], a["architecture"]) for a in data["release"]["assets"]},
                set(workflow.TARGETS.values()),
            )
            with self.assertRaisesRegex(ValueError, "provenance mismatch"):
                workflow.release_manifest(directory, version, "b" * 40)
            path.unlink()
            with self.assertRaisesRegex(ValueError, "all six"):
                workflow.release_manifest(directory, version, commit)

    def test_notice_pin_requires_unchanged_dependencies_and_exact_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "dependency").write_bytes(b"locked")
            body = b"===== third-party-notices/LICENSE.txt =====\nPackages: bundle.zip\n\nReviewed license\n"
            pin = {
                "url": "https://example.com/notices",
                "sha256": hashlib.sha256(body).hexdigest(),
                "dependencyFiles": ["dependency"],
                "dependencySha256": hashlib.sha256(b"dependency\0locked\0").hexdigest(),
            }
            (root / "scripts/zai-codex-notices.json").write_text(json.dumps(pin))
            with (
                patch.object(workflow, "ROOT", root),
                patch.object(workflow.urllib.request, "urlopen") as fetch,
            ):
                fetch.return_value.__enter__.return_value.read.return_value = body
                workflow.prepare_notices(root / "notices")
                self.assertEqual(
                    (root / "notices/LICENSE.txt").read_text(), "Reviewed license\n"
                )
                (root / "dependency").write_bytes(b"changed")
                with self.assertRaisesRegex(ValueError, "dependencies changed"):
                    workflow.prepare_notices(root / "other")
                self.assertEqual(fetch.call_count, 1)


if __name__ == "__main__":
    unittest.main()
