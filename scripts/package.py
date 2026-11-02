#!/usr/bin/env python3
"""Package both desktop applications using only Python's standard library.

Native packaging is performed on each OS. --bundle-only also permits inspection
of the AppDir/.app structure without appimagetool or macOS packaging tools.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PRODUCTS = {"acbrinstall": "ACBrInstall", "pascalinstall": "PascalInstall"}
# Versioned upstream releases: continuous assets can be replaced in place.
APPIMAGE_TOOLS = {
    "appimagetool": (
        "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage",
        "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0",
    ),
    "runtime": (
        "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64",
        "2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d",
    ),
}


def run(*args, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def verified_tool(name):
    url, digest = APPIMAGE_TOOLS[name]
    destination = ROOT / "target" / "packaging-tools" / digest / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    if not destination.is_file() or hashlib.sha256(destination.read_bytes()).hexdigest() != digest:
        temporary = destination.with_suffix(".download")
        request = urllib.request.Request(url, headers={"User-Agent": "ACBrInstall-build"})
        with urllib.request.urlopen(request, timeout=120) as response, temporary.open("wb") as output:
            shutil.copyfileobj(response, output)
        if hashlib.sha256(temporary.read_bytes()).hexdigest() != digest:
            temporary.unlink()
            raise RuntimeError(f"SHA256 inválido: {url}")
        temporary.replace(destination)
    destination.chmod(0o755)
    return destination


def appdir(app, title, binary, directory):
    (directory / "usr/bin").mkdir(parents=True)
    shutil.copy2(binary, directory / "usr/bin" / app)
    (directory / "usr/bin" / app).chmod(0o755)
    app_id = f"dev.rftd.{app}"
    shutil.copy2(ROOT / "assets/linux" / f"{app_id}.desktop", directory)
    shutil.copy2(ROOT / "assets" / f"{app}{'-256' if app == 'pascalinstall' else ''}.png",
                 directory / f"{app_id}.png")
    applications = directory / "usr/share/applications"
    icons = directory / "usr/share/icons/hicolor/256x256/apps"
    applications.mkdir(parents=True)
    icons.mkdir(parents=True)
    shutil.copy2(directory / f"{app_id}.desktop", applications)
    shutil.copy2(directory / f"{app_id}.png", icons)
    (directory / ".DirIcon").symlink_to(f"{app_id}.png")
    (directory / "AppRun").write_text(
        '#!/bin/sh\nAPPDIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\n'
        f'exec "$APPDIR/usr/bin/{app}" "$@"\n', encoding="utf-8")
    (directory / "AppRun").chmod(0o755)


def mac_bundle(app, title, binary, directory, version, arch):
    contents = directory / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    executable = contents / "MacOS" / app
    shutil.copy2(binary, executable)
    executable.chmod(0o755)
    shutil.copy2(ROOT / "assets" / f"{app}.icns", contents / "Resources")
    info = {
        "CFBundleName": title,
        "CFBundleDisplayName": title,
        "CFBundleIdentifier": f"dev.rftd.{app}",
        "CFBundleExecutable": app,
        "CFBundleIconFile": f"{app}.icns",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "LSMinimumSystemVersion": "11.0",
        "LSArchitecturePriority": ["arm64" if arch == "aarch64" else "x86_64"],
        "NSHighResolutionCapable": True,
        "NSHumanReadableCopyright": "ACBrInstall / PascalInstall contributors",
    }
    with (contents / "Info.plist").open("wb") as output:
        plistlib.dump(info, output)
    (contents / "PkgInfo").write_bytes(b"APPL????")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", required=True,
                        choices=["linux-x86_64", "macos-x86_64", "macos-aarch64"])
    parser.add_argument("--binary-dir", type=Path, default=ROOT / "target/release")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "dist")
    parser.add_argument("--bundle-only", action="store_true")
    args = parser.parse_args()
    if not args.bundle_only:
        expected = "darwin" if args.platform.startswith("macos") else "linux"
        if not sys.platform.startswith(expected):
            parser.error("Execute o empacotamento nativo no sistema de destino ou use --bundle-only.")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version=1", "--offline"], cwd=ROOT))
    version = next(p["version"] for p in metadata["packages"] if p["name"] == "acbrinstall")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for app, title in PRODUCTS.items():
        binary = args.binary_dir / app
        if not binary.is_file():
            parser.error(f"Executável não encontrado: {binary}")
        directory = ROOT / "target/packages" / args.platform / (
            f"{title}.app" if args.platform.startswith("macos") else f"{title}.AppDir")
        if directory.exists():
            shutil.rmtree(directory)
        if args.platform.startswith("linux"):
            appdir(app, title, binary, directory)
            if not args.bundle_only:
                artifact = args.output_dir / f"{title}-{args.platform}.AppImage"
                # Extract-and-run works on CI runners without a FUSE mount.
                run(verified_tool("appimagetool"), "--appimage-extract-and-run", "--no-appstream",
                    "--runtime-file", verified_tool("runtime"), directory, artifact,
                    env={**os.environ, "ARCH": "x86_64", "VERSION": version})
                artifact.chmod(0o755)
                run(artifact, "--appimage-extract-and-run", "--help")
        else:
            mac_bundle(app, title, binary, directory, version, args.platform.split("-")[1])
            if not args.bundle_only:
                run("plutil", "-lint", directory / "Contents/Info.plist")
                # Ad-hoc signing supports native execution, including Apple Silicon.
                # Developer ID signing/notarization requires the publisher's credentials.
                run("codesign", "--force", "--sign", "-", directory)
                run("codesign", "--verify", "--strict", directory)
                run(directory / "Contents/MacOS" / app, "--help")
                run("ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", directory,
                    args.output_dir / f"{title}-{args.platform}.app.zip")
                run("pkgbuild", "--component", directory, "--install-location", "/Applications",
                    "--identifier", f"dev.rftd.{app}", "--version", version,
                    args.output_dir / f"{title}-{args.platform}.pkg")
        print(f"Preparado: {directory}", flush=True)


if __name__ == "__main__":
    main()
