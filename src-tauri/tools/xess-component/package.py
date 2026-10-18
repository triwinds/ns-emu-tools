"""Stage the independent XeSS component beside the local Tauri executable."""
import hashlib
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import tempfile

ROOT = Path(__file__).resolve().parents[3]
BUILD = ROOT / "src-tauri/target/xess-component/release"
DELIVERY = ROOT / "src-tauri/target/release"
DEST = DELIVERY / "xess-package"
INTEL = ROOT / "src-tauri/target/xess-fg-sdk-v3.0.2"
AGILITY = ROOT / "src-tauri/target/d3d12-agility-1.619.6"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


sources = {
    "xess-component.exe": BUILD / "streamline-layer-probe.exe",
    "xess-layer.dll": BUILD / "streamline_probe_layer.dll",
    "libxess.dll": ROOT / "src-tauri/target/xess-sdk-files-v3.0.2/bin/libxess.dll",
    "bin/libxess_fg.dll": INTEL / "bin/libxess_fg.dll",
    "bin/libxell.dll": INTEL / "bin/libxell.dll",
    "licenses/Intel-LICENSE.txt": INTEL / "LICENSE.txt",
    "licenses/Intel-third-party-programs.txt": INTEL / "third-party-programs.txt",
    "licenses/Microsoft-LICENSE.txt": AGILITY / "LICENSE.txt",
    "licenses/Microsoft-LICENSE-CODE.txt": AGILITY / "LICENSE-CODE.txt",
    "licenses/NS-Emu-Tools-LICENSE.txt": ROOT / "LICENSE",
}
intel = json.loads((ROOT / "src-tauri/tools/xess-fg-diagnostics/sdk.json").read_text())
for file in intel["files"]:
    if file["path"] in ["bin/libxess_fg.dll", "bin/libxell.dll", "LICENSE.txt", "third-party-programs.txt"]:
        if digest(INTEL / file["path"]) != file["sha256"]:
            raise SystemExit(f"Intel file hash mismatch: {file['path']}")
if digest(sources["libxess.dll"]) != "251659dd84a3e84de67c886a4186e01f3eca49b00641906fe38bb6b807e5d5b7":
    raise SystemExit("Native AA runtime hash mismatch")
agility = json.loads((ROOT / "src-tauri/tools/xess-fg-diagnostics/agility.json").read_text())
for file in agility["files"]:
    source = AGILITY / file["path"]
    if digest(source) != file["sha256"]:
        raise SystemExit(f"Agility file hash mismatch: {file['path']}")
    sources[f"D3D12/{file['path']}"] = source
layer = sources["xess-layer.dll"].read_bytes()
for marker in [b"NS_EMU_XESS_COMPONENT_V1", b"NS_EMU_XESS_NATIVE_AA_V1", b"NS_EMU_XESS_FG_GAME_V1"]:
    if marker not in layer:
        raise SystemExit(f"Missing component marker: {marker!r}")
if b"NS_EMU_FIXED_RUNTIME_V1" in layer:
    raise SystemExit("Refusing Streamline build in XeSS package")
files = [{"name": name, "sha256": digest(source)} for name, source in sorted(sources.items())]
version = "local-xess-" + hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()[:16]
manifest = {"schemaVersion": 1, "version": version, "files": files}
DELIVERY.mkdir(exist_ok=True, parents=True)


def verify_existing(directory):
    if directory.is_symlink() or directory.resolve().parent != DELIVERY.resolve():
        raise SystemExit("Refusing unexpected package destination")
    old = json.loads((directory / "package.json").read_text())
    allowed = {row["name"] for row in old["files"]} | {"package.json"}
    for path in directory.rglob("*"):
        if path.is_symlink() or (hasattr(path, "is_junction") and path.is_junction()):
            raise SystemExit("Refusing linked component member")
        if path.is_file() and path.relative_to(directory).as_posix() not in allowed:
            raise SystemExit("Refusing package with extra files")
    for row in old["files"]:
        path = directory / row["name"]
        if path.resolve().is_relative_to(directory.resolve()) is False or digest(path) != row["sha256"]:
            raise SystemExit("Refusing changed existing component")


@contextmanager
def removal_guard(paths):
    # Acquire DELETE access for every file before changing the old package.
    # This fails if any launcher/layer has pinned a member without delete sharing.
    handles = []
    if os.name == "nt":
        import ctypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateFileW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p]
        kernel.CreateFileW.restype = ctypes.c_void_p
        kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    try:
        if os.name == "nt":
            for path in paths:
                handle = kernel.CreateFileW(str(path), 0x10000, 7, None, 3, 0x80, None)
                if handle == ctypes.c_void_p(-1).value:
                    raise SystemExit(f"Component in use (Windows {ctypes.get_last_error()}); old package retained: {path}")
                handles.append(handle)
        yield
    finally:
        for handle in handles:
            kernel.CloseHandle(handle)


with tempfile.TemporaryDirectory(prefix=".xess-stage-", dir=DELIVERY) as staging:
    stage = Path(staging)
    for name, source in sources.items():
        target = stage / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        if digest(target) != digest(source):
            raise SystemExit(f"Staging mismatch: {name}")
    (stage / "package.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    if DEST.exists():
        verify_existing(DEST)
        # Every removal target was resolved/verified inside the named component.
        # Active component files are held without delete sharing and will refuse.
        paths = sorted(DEST.rglob("*"), key=lambda p: len(p.parts), reverse=True)
        with removal_guard([path for path in paths if path.is_file()]):
            for path in paths:
                if path.is_file():
                    path.unlink()
        for path in paths:
            if path.is_dir():
                path.rmdir()
        DEST.rmdir()
    os.rename(stage, DEST)
    # TemporaryDirectory must retain an empty owned directory for its cleanup.
    stage.mkdir()
contract = ROOT / "src-tauri/src/services/graphics_components/xess-package.json"
contract.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"version": version, "directory": str(DEST), "files": len(files)}, indent=2))
