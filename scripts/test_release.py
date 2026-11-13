import json
import unittest
from unittest.mock import patch

import github_releases
import release


def workspace(name="duke-parser", dependencies=(), publish=None):
    return {
        "workspace_members": [name],
        "packages": [{
            "id": name,
            "name": name,
            "publish": publish,
            "dependencies": [{"name": dependency} for dependency in dependencies],
        }],
    }


class ReleasePolicyTests(unittest.TestCase):
    def test_accepts_duke_workspace_and_external_dependencies(self):
        metadata = workspace(dependencies=("serde", "rust-fontconfig"))
        with patch.object(release, "cargo", return_value=json.dumps(metadata)):
            self.assertEqual(release.checked_workspace(), metadata)

    def test_checks_only_workspace_members(self):
        metadata = workspace()
        metadata["packages"].append({"id": "external", "name": "serde"})
        with patch.object(release, "cargo", return_value=json.dumps(metadata)):
            self.assertEqual(len(release.checked_workspace()["packages"]), 1)

    def test_rejects_retired_and_non_duke_workspace_members(self):
        for name in ("rust-docx", "rust-docx-wasm", "rust-pptx", "rust-pptx-wasm",
                     "rust-odf", "rust-office-art", "rust-office-charts", "rust-office-charts-wasm",
                     "other-parser"):
            for publish in (None, []):
                with self.subTest(name=name, publish=publish):
                    with patch.object(release, "cargo", return_value=json.dumps(workspace(name, publish=publish))):
                        with self.assertRaisesRegex(SystemExit, "Only duke-\\*"):
                            release.checked_workspace()

    def test_rejects_retired_dependencies(self):
        metadata = workspace(dependencies=("rust-office-art",))
        metadata["packages"][0]["dependencies"][0]["rename"] = "art"
        with patch.object(release, "cargo", return_value=json.dumps(metadata)):
            with self.assertRaisesRegex(SystemExit, "retired Office dependencies"):
                release.checked_workspace()

    def test_rejects_before_packaging_or_registry_access(self):
        with patch.object(release, "cargo", return_value=json.dumps(workspace("rust-odf"))) as cargo:
            with patch.object(release, "fetch") as fetch:
                with self.assertRaises(SystemExit):
                    release.release_plan()
                cargo.assert_called_once_with("metadata", "--locked", "--format-version=1", "--no-deps")
                fetch.assert_not_called()

    def test_github_release_rejects_before_running_gh(self):
        with patch.object(release, "cargo", return_value=json.dumps(workspace("rust-odf"))):
            with patch.object(github_releases.subprocess, "run") as run:
                with self.assertRaises(SystemExit):
                    github_releases.main()
                run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
