"""Create a GitHub release for each published workspace crate."""

import json
from pathlib import Path
import subprocess
import tempfile


def main():
    workspace = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version=1", "--no-deps"],
        text=True,
    ))
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    artifacts = Path(workspace["target_directory"]) / "package"
    for package in workspace["packages"]:
        if package["publish"] == []:
            continue
        name, version = package["name"], package["version"]
        tag = f"{name}-v{version}"
        existing = subprocess.run(
            ["gh", "release", "view", tag, "--json", "tagName"],
            capture_output=True,
            text=True,
        )
        if existing.returncode == 0:
            print(f"Release exists: {tag}", flush=True)
            continue
        artifact = artifacts / f"{name}-{version}.crate"
        if not artifact.is_file():
            raise FileNotFoundError(artifact)
        notes = (
            f"{name} {version} by Extend.\n\n"
            f"{package['description']}\n\n"
            f"[crates.io](https://crates.io/crates/{name}/{version}) · "
            f"[API documentation](https://docs.rs/{name}/{version}) · "
            f"[Source](https://github.com/extend-hq/rust-office/tree/{tag}/crates/{name})\n\n"
            f"```toml\n[dependencies]\n{name} = \"{version}\"\n```\n"
        )
        with tempfile.NamedTemporaryFile(mode="w", suffix=".md") as body:
            body.write(notes)
            body.flush()
            subprocess.run([
                "gh", "release", "create", tag, str(artifact),
                "--target", revision, "--title", f"{name} {version}",
                "--notes-file", body.name, "--latest=false",
            ], check=True)


if __name__ == "__main__":
    main()
