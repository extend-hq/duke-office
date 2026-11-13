"""Compare DOCX chart images, chart data and figure source references with a Git revision."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parents[2]
INPUTS = ("Cargo.lock", "Cargo.toml", "crates")


def copy_revision(destination, revision):
    destination.mkdir()
    if revision is None:
        for name in INPUTS:
            source = ROOT / name
            if source.is_dir():
                shutil.copytree(source, destination / name)
            else:
                shutil.copy2(source, destination / name)
    else:
        with subprocess.Popen(
            ["git", "archive", revision, *INPUTS], cwd=ROOT, stdout=subprocess.PIPE
        ) as process:
            with tarfile.open(fileobj=process.stdout, mode="r|") as archive:
                archive.extractall(destination, filter="data")
            if process.wait():
                raise RuntimeError(f"Could not export Git revision {revision}")


def snapshot(temp, variant, revision, fixtures):
    source = temp / variant
    copy_revision(source, revision)
    shutil.copy2(SCRIPTS / "snapshot.rs", source / "crates/duke-pages/examples/chart_guard.rs")
    target = temp / "target"
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0")
    subprocess.run(
        ["cargo", "build", "--quiet", "--locked", "-p", "duke-pages", "--example", "chart_guard"],
        cwd=source, env=env, check=True,
    )
    binary = target / "debug/examples/chart_guard"
    digest = hashlib.sha256()
    for crate in ("duke-pages", "duke-office-charts"):
        directory = source / "crates" / crate
        for path in [directory / "Cargo.toml", *sorted((directory / "src").rglob("*.rs"))]:
            digest.update(path.relative_to(source).as_posix().encode() + b"\0")
            digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest(), {
        str(path): json.loads(subprocess.check_output([binary, path]))
        for path in fixtures
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True, help="Git revision representing the previous renderer")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("documents", type=Path, nargs="+")
    args = parser.parse_args()
    revision = subprocess.check_output(
        ["git", "rev-parse", "--verify", "--end-of-options", args.base + "^{commit}"],
        cwd=ROOT, text=True,
    ).strip()
    fixtures = sorted({path.resolve(strict=True) for path in args.documents})
    args.out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="office-docx-charts-") as directory:
        temp = Path(directory)
        baseline_digest, baseline = snapshot(temp, "baseline", revision, fixtures)
        candidate_digest, candidate = snapshot(temp, "candidate", None, fixtures)
    summary = {
        "baselineCommit": revision,
        "baselineSourcesSha256": baseline_digest,
        "candidateSourcesSha256": candidate_digest,
        "documents": len(fixtures),
        "drawings": sum(len(doc["drawings"]) for doc in baseline.values()),
        "nativeSvg": sum(row["renderedSvg"] for doc in baseline.values() for row in doc["drawings"]),
        "figureSourceRefLists": sum(bool(row["sourceRefs"]) for doc in baseline.values() for row in doc["figures"]),
        "differingDocuments": [name for name in baseline if baseline[name] != candidate[name]],
    }
    for name, data in [("baseline", baseline), ("candidate", candidate), ("verification", summary)]:
        (args.out / f"{name}.json").write_text(json.dumps(data, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    if not summary["drawings"]:
        raise SystemExit("No baseline chart drawings were found in the supplied documents.")
    return bool(summary["differingDocuments"])


if __name__ == "__main__":
    raise SystemExit(main())
