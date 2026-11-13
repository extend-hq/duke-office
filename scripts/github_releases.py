"""Create a GitHub release for each published workspace crate."""

import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile

from release import fetch, published_checksum


def main():
    workspace = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version=1", "--no-deps"],
        text=True,
    ))
    artifacts = Path(workspace["target_directory"]) / "package"
    artifacts.mkdir(parents=True, exist_ok=True)
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
        checksum = published_checksum(name, version)
        if checksum is None:
            raise RuntimeError(f"Publish {name} {version} to crates.io first")
        filename = f"{name}-{version}.crate"
        data = fetch(f"https://static.crates.io/crates/{name}/{filename}")
        if hashlib.sha256(data).hexdigest() != checksum:
            raise RuntimeError(f"Registry checksum mismatch for {filename}")
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            metadata = archive.extractfile(f"{name}-{version}/.cargo_vcs_info.json")
            revision = json.load(metadata)["git"]["sha1"]
        artifact = artifacts / filename
        artifact.write_bytes(data)
        notes = (
            f"{name} {version} by Extend.\n\n"
            f"{package['description']}\n\n"
            f"[crates.io](https://crates.io/crates/{name}/{version}) · "
            f"[API documentation](https://docs.rs/{name}/{version}) · "
            f"[Source](https://github.com/extend-hq/duke-office/tree/{tag}/crates/{name})\n\n"
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
