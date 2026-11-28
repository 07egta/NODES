#!/usr/bin/env python3
"""Create a development .app bundle and tar.gz without claiming signing/notarization."""
import argparse
import os
from pathlib import Path
import plistlib
import platform
import shutil
import subprocess
import tarfile
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("Package on native macOS only")
    repo = args.repo.resolve(strict=True)
    binaries = args.bin_dir.resolve(strict=True)
    arch = platform.machine()
    if arch not in ("arm64", "x86_64"):
        raise SystemExit("Unknown macOS architecture")
    for name in ("NODES", "nodes-cli"):
        result = subprocess.check_output(["lipo", "-archs", str(binaries / name)], text=True).strip()
        if result != arch:
            raise SystemExit(f"{name}: expected {arch}, got {result}")
    output = args.output_dir.absolute()
    output.mkdir(parents=True, exist_ok=False)
    package = output / f"NODES-macos-{arch}"
    contents = package / "NODES.app" / "Contents"
    macos = contents / "MacOS"
    resources = contents / "Resources"
    macos.mkdir(parents=True)
    resources.mkdir()
    shutil.copy2(binaries / "NODES", macos / "NODES")
    shutil.copy2(binaries / "nodes-cli", package / "nodes-cli")
    for name in ("examples", "licenses", "docs"):
        shutil.copytree(repo / name, resources / name)
    for name in ("README.md", "LICENSE-MIT", "LICENSE-APACHE", "Cargo.lock"):
        shutil.copy2(repo / name, package / name)
    version = tomllib.loads((repo / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    with (contents / "Info.plist").open("wb") as stream:
        plistlib.dump({"CFBundleExecutable": "NODES", "CFBundleIdentifier": "org.nodes.geometry",
                       "CFBundleName": "NODES", "CFBundleDisplayName": "NODES",
                       "CFBundlePackageType": "APPL", "CFBundleInfoDictionaryVersion": "6.0",
                       "CFBundleShortVersionString": version, "CFBundleVersion": version,
                       "NSHighResolutionCapable": True}, stream)
    (package / "BUILD-INFO.txt").write_text(
        "NODES development artifact\n"
        f"Commit: {os.environ.get('GITHUB_SHA', 'unknown')}\nArchitecture: {arch}\n"
        "No Developer ID signing or Apple notarization performed by this script.\n"
        "Native GUI and real hardware compatibility are separate tests.\n", encoding="utf-8")
    subprocess.run(["plutil", "-lint", str(contents / "Info.plist")], check=True)
    subprocess.run(["otool", "-L", str(macos / "NODES")], check=True)
    with tarfile.open(output / (package.name + ".tar.gz"), "x:gz") as archive:
        archive.add(package, arcname=package.name)
    print(package)


if __name__ == "__main__":
    main()
