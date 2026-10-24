"""Build/stage three graphics packages; optionally retain Release ZIPs with --archives.

Default local delivery is target/release/graphics-components/{common,nvidia,xess}.
No publishing is performed. Existing verified components are kept as a backup.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[3]
TAURI = ROOT / "src-tauri"
CONTRACT = TAURI / "src/services/graphics_components"
DELIVERY = TAURI / "target/release"
CRATE = TAURI / "crates/streamline-fg"


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def rows(directory):
    return [{"name": p.relative_to(directory).as_posix(), "sha256": digest(p)}
            for p in sorted(directory.rglob("*")) if p.is_file()]


def identity(files):
    return hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()[:16]


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def archive(path, directory):
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as output:
        for row in rows(directory):
            info = zipfile.ZipInfo(row["name"], date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            output.writestr(info, (directory / row["name"]).read_bytes())


def source_archive(release):
    output = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "--",
                                      "LICENSE", "src-tauri/crates", "src-tauri/tools/graphics-components",
                                      "src-tauri/tools/xess-fg-diagnostics", "src-tauri/tools/xess-vulkan-diagnostics",
                                      "src-tauri/src/services/graphics_components", "docs"], cwd=ROOT, text=True, encoding="utf-8")
    sources = sorted({name for name in output.splitlines()
                      if not {"target", "evidence", "__pycache__"}.intersection(Path(name).parts) and (ROOT / name).is_file()})
    files = [{"name": name, "sha256": digest(ROOT / name)} for name in sources]
    path = release / ("graphics-sources-" + identity(files) + ".zip")
    with tempfile.NamedTemporaryFile(suffix=".zip", dir=release, delete=False) as temporary:
        temporary_path = Path(temporary.name)
    try:
        with zipfile.ZipFile(temporary_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as output:
            for name in sources:
                info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                output.writestr(info, (ROOT / name).read_bytes())
        if path.exists():
            if digest(path) != digest(temporary_path):
                raise SystemExit(f"Immutable source archive differs: {path}")
        else:
            temporary_path.rename(path)
    finally:
        temporary_path.unlink(missing_ok=True)


def build():
    env = {**os.environ, "RUSTFLAGS": "-C target-feature=-crt-static"}
    for features, target, binary in [
        ("hybrid-xess-fg", CRATE / "target", True),
        ("xess-component", TAURI / "target/xess-component", False),
    ]:
        cmd = ["cargo", "build", "--locked", "--release", "--manifest-path", str(CRATE / "Cargo.toml"),
               "--target-dir", str(target), "--features", features, "--lib"]
        if binary:
            cmd += ["--bin", "streamline-layer-probe"]
        subprocess.run(cmd, cwd=ROOT, env=env, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--archives", action="store_true", help="Retain immutable Release ZIPs and manifests")
    args = parser.parse_args()
    if args.build:
        build()
    DELIVERY.mkdir(parents=True, exist_ok=True)
    old_nvidia = DELIVERY / "streamline-fg-package"
    old_xess = DELIVERY / "xess-package"
    # Existing pinned vendor manifests remain the provenance for staged external binaries.
    # After the first migration, prefer the current split directories.
    current = DELIVERY / "graphics-components"
    split = (current / "distribution.json").is_file()
    source_nvidia = current / "nvidia" if split else old_nvidia
    source_xess = current / "xess" if split else old_xess
    nvidia_manifest = json.loads((CONTRACT / "streamline-package.json").read_text(encoding="utf-8"))
    xess_manifest = json.loads((CONTRACT / "xess-package.json").read_text(encoding="utf-8"))
    own = {"streamline-layer-probe.exe", "graphics-launcher.exe", "streamline_probe_layer.dll", "nvngx.dll",
           "LICENSE-Layer.txt", "README-LAYER.txt", "xess-component.exe", "xess-layer.dll"}
    with tempfile.TemporaryDirectory(prefix=".graphics-stage-", dir=DELIVERY) as temporary:
        stage = Path(temporary)
        for kind in ["common", "nvidia", "xess"]:
            (stage / kind).mkdir()
        sources = {
            "graphics-launcher.exe": CRATE / "target/release/streamline-layer-probe.exe",
            "streamline_probe_layer.dll": CRATE / "target/release/streamline_probe_layer.dll",
            "xess-layer.dll": TAURI / "target/xess-component/release/streamline_probe_layer.dll",
            "nvngx.dll": (current / "common/nvngx.dll") if split else old_nvidia / "nvngx.dll",
            "LICENSE.txt": ROOT / "LICENSE",
        }
        if b"NS_EMU_UNIFIED_LAUNCHER_V2" not in sources["graphics-launcher.exe"].read_bytes():
            raise SystemExit("Rebuild the unified launcher before staging")
        for name, marker in [("streamline_probe_layer.dll", b"NS_EMU_FIXED_RUNTIME_V1"),
                             ("xess-layer.dll", b"NS_EMU_XESS_COMPONENT_V1")]:
            if marker not in sources[name].read_bytes():
                raise SystemExit(f"Incorrect layer build: {name}")
        if b"NS_EMU_FIXED_RUNTIME_V1" in sources["xess-layer.dll"].read_bytes():
            raise SystemExit("Refusing NVIDIA layer in the independent XeSS route")
        bridge_contract = (TAURI / "crates/streamline-nr-contract.rs").read_text(encoding="utf-8")
        if f'pub const BRIDGE: &str = "{digest(sources["nvngx.dll"])}";' not in bridge_contract:
            raise SystemExit("NR bridge changed; revalidate before packaging")
        for name, source in sources.items():
            shutil.copyfile(source, stage / "common" / name)
        for kind, manifest, source in [("nvidia", nvidia_manifest, source_nvidia), ("xess", xess_manifest, source_xess)]:
            for row in manifest["files"]:
                if row["name"] in own:
                    continue
                path = source / row["name"]
                if path.is_symlink() or not path.resolve().is_relative_to(source.resolve()) or digest(path) != row["sha256"]:
                    raise SystemExit(f"Vendor file changed: {path}")
                out = stage / kind / row["name"]
                out.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, out)
                if digest(out) != row["sha256"]:
                    raise SystemExit(f"Vendor file changed during staging: {path}")
        common_files = rows(stage / "common")
        common_version = "graphics-common-" + identity(common_files)
        xess_files = rows(stage / "xess")
        xess_package = {"schemaVersion": 2, "version": "local-xess-" + identity(xess_files), "files": xess_files}
        write_json(stage / "xess/package.json", xess_package)
        nvidia_files = rows(stage / "nvidia")
        nvidia_package = {"schema_version": 2, "launcher_protocol": 2, "native_nr": True,
                          "version": "local-graphics-" + identity(common_files) + "-" + identity(nvidia_files),
                          "common_version": common_version, "files": nvidia_files}
        distribution = {"schemaVersion": 2}
        for kind in ["common", "nvidia", "xess"]:
            files = rows(stage / kind)
            version = f"graphics-{kind}-{identity(files)}"
            # ZIP bytes are deterministic. Only --archives retains distribution archives.
            path = stage / (version + ".zip")
            archive(path, stage / kind)
            distribution[kind] = {"version": version, "files": files, "download": {
                "name": path.name, "sha256": digest(path),
                "url": f"https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/{version}/{path.name}"}}
            if args.archives:
                release = TAURI / "target/graphics-release" / version
                release.mkdir(parents=True, exist_ok=True)
                dest = release / path.name
                if dest.exists() and digest(dest) != digest(path):
                    raise SystemExit(f"Immutable archive already exists with different bytes: {dest}")
                shutil.copyfile(path, dest)
            path.unlink()
        write_json(stage / "distribution.json", distribution)
        backup = None
        if current.exists():
            # Never delete an existing delivery. Reject links; preserve the complete old directory.
            if current.resolve().parent != DELIVERY.resolve() or current.is_symlink() or current.is_junction():
                raise SystemExit("Unsafe component destination")
            for path in current.rglob("*"):
                if path.is_symlink() or path.is_junction():
                    raise SystemExit("Linked component member")
            import uuid
            backup = DELIVERY / ("graphics-components.previous-" + uuid.uuid4().hex)
            current.rename(backup)
        try:
            stage.rename(current)
        except OSError:
            if backup is not None:
                backup.rename(current)
            raise
        stage.mkdir()  # TemporaryDirectory keeps ownership of an empty directory.
        write_json(CONTRACT / "graphics-distribution.json", distribution)
        write_json(CONTRACT / "streamline-package.json", nvidia_package)
        write_json(CONTRACT / "xess-package.json", xess_package)
        if args.archives:
            release = TAURI / "target/graphics-release"
            write_json(release / "distribution.json", distribution)
            shutil.copyfile(CONTRACT / "streamline-package.json", release / "streamline-package.json")
            shutil.copyfile(CONTRACT / "xess-package.json", release / "xess-package.json")
            source_archive(release)
            checksums = [f"{digest(path)}  {path.relative_to(release).as_posix()}\n"
                         for path in sorted(release.rglob("*")) if path.is_file() and path.suffix in {".zip", ".json"}]
            (release / "SHA256SUMS.txt").write_text("".join(checksums), encoding="utf-8")
        print(json.dumps({"directory": str(current), "packages": {k: distribution[k]["version"] for k in ["common", "nvidia", "xess"]}}, indent=2))


if __name__ == "__main__":
    main()
