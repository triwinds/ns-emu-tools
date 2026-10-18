"""Prepare a verified XeSS Release and pin its download for the toolbox.

Run package.py first. This prepares assets only; publish with gh release create.
"""
import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "src-tauri/target/release/xess-package"
CONTRACT = ROOT / "src-tauri/src/services/graphics_components"
manifest = json.loads((SOURCE / "package.json").read_text(encoding="utf-8"))
assert manifest == json.loads((CONTRACT / "xess-package.json").read_text(encoding="utf-8"))
tag = "xess-" + manifest["version"].removeprefix("local-xess-") + "-v1"
dest = ROOT / "build/xess-release" / tag
dest.mkdir(parents=True, exist_ok=True)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def archive(path, files):
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zip:
        for name, source in sorted(files.items()):
            assert not source.is_symlink()
            entry = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            zip.writestr(entry, source.read_bytes())


files = {row["name"]: SOURCE / row["name"] for row in manifest["files"]}
for row in manifest["files"]:
    assert files[row["name"]].resolve().is_relative_to(SOURCE.resolve())
    assert digest(files[row["name"]]) == row["sha256"], row["name"]
files["package.json"] = SOURCE / "package.json"
assert {p.relative_to(SOURCE).as_posix() for p in SOURCE.rglob("*") if p.is_file()} == set(files)
bundle = dest / (tag + ".zip")
archive(bundle, files)
release = {
    "version": manifest["version"],
    "download": {
        "name": bundle.name,
        "url": f"https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/{tag}/{bundle.name}",
        "sha256": digest(bundle),
    },
}
release_text = json.dumps(release, indent=2) + "\n"
(CONTRACT / "xess-release.json").write_text(release_text, encoding="utf-8")
(dest / (tag + "-manifest.json")).write_text(
    json.dumps({**manifest, "download": release["download"]}, indent=2) + "\n", encoding="utf-8"
)

# Corresponding source, shared Rust modules, shader sources, SDK hashes and licenses.
sources = {"LICENSE": ROOT / "LICENSE"}
for directory in ["src-tauri/crates", "src-tauri/tools/xess-component", "src-tauri/tools/xess-fg-diagnostics", "src-tauri/tools/xess-vulkan-diagnostics"]:
    for path in (ROOT / directory).rglob("*"):
        if path.is_file() and not {"target", "__pycache__"}.intersection(path.relative_to(ROOT / directory).parts):
            sources[path.relative_to(ROOT).as_posix()] = path
for path in (ROOT / "docs").glob("xess-*.md"):
    sources[path.relative_to(ROOT).as_posix()] = path
for path in CONTRACT.glob("xess*"):
    if path.is_file():
        sources[path.relative_to(ROOT).as_posix()] = path
archive(dest / (tag + "-sources.zip"), sources)
assets = sorted(p for p in dest.iterdir() if p.suffix in {".zip", ".json"})
(dest / (tag + "-SHA256SUMS.txt")).write_text(
    "".join(f"{digest(p)}  {p.name}\n" for p in assets), encoding="utf-8"
)
print(json.dumps({"tag": tag, "directory": str(dest), "size": bundle.stat().st_size, "sha256": digest(bundle)}, indent=2))
