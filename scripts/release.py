"""Check and publish the workspace's independently versioned crates."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import urllib.error
import urllib.request


def cargo(*args):
    return subprocess.check_output(["cargo", *args], text=True)


def fetch(url):
    request = urllib.request.Request(
        url, headers={"User-Agent": "extend-office-release (github.com/extend-hq/extend-office)"}
    )
    with urllib.request.urlopen(request, timeout=40) as response:
        return response.read()


def published_checksum(name, version):
    prefix = {1: "1", 2: "2", 3: f"3/{name[0]}"}.get(len(name), f"{name[:2]}/{name[2:4]}")
    try:
        index = fetch(f"https://index.crates.io/{prefix}/{name}")
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise
    for line in index.splitlines():
        release = json.loads(line)
        if release["vers"] == version:
            if release["yanked"]:
                sys.exit(f"{name} {version} has been yanked. Choose a new version.")
            return release["cksum"]
    return None


def packaged_contents(data):
    files = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            path = member.name.split("/", 1)[-1]
            if member.isfile() and path not in {".cargo_vcs_info.json", "Cargo.lock", "Cargo.toml.orig"}:
                files[path] = hashlib.sha256(archive.extractfile(member).read()).digest()
    return files


def release_plan():
    workspace = json.loads(cargo("metadata", "--locked", "--format-version=1", "--no-deps"))
    cargo("package", "--workspace", "--locked", "--no-verify")
    artifacts = Path(workspace["target_directory"]) / "package"
    pending = []
    for package in workspace["packages"]:
        if package["publish"] == []:
            continue
        name, version = package["name"], package["version"]
        checksum = published_checksum(name, version)
        if checksum is None:
            pending.append(name)
            print(f"Pending: {name} {version}", flush=True)
            continue
        filename = f"{name}-{version}.crate"
        published = fetch(f"https://static.crates.io/crates/{name}/{filename}")
        if hashlib.sha256(published).hexdigest() != checksum:
            sys.exit(f"Registry checksum mismatch for {filename}.")
        old = packaged_contents(published)
        new = packaged_contents((artifacts / filename).read_bytes())
        changed = sorted(path for path in old.keys() | new.keys() if old.get(path) != new.get(path))
        if changed:
            sys.exit(f"Bump {name} from {version}. Published contents differ: {', '.join(changed[:10])}")
        print(f"Already published: {name} {version}", flush=True)
    return pending


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["plan", "dry-run", "publish"])
    mode = parser.parse_args().mode
    pending = release_plan()
    if output := os.environ.get("GITHUB_OUTPUT"):
        with open(output, "a") as stream:
            print(f"pending={'true' if pending else 'false'}", file=stream)
    if not pending or mode == "plan":
        return
    args = ["cargo", "publish", "--locked", "--registry", "crates-io"]
    if mode == "dry-run":
        args.append("--dry-run")
    for name in pending:
        args.extend(["--package", name])
    subprocess.run(args, check=True)


if __name__ == "__main__":
    main()
