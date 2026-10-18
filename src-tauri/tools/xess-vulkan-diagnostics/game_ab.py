"""Launch one opt-in Xenoblade 3 AA A/B session; never alter emulator settings."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument("backend", choices=["off", "dlaa", "xess"])
parser.add_argument("--name")
args = parser.parse_args()
emulator = Path("D:/Ryujinx_Nextendo_DLSS/Ryujinx.exe")
runtime = ROOT / "src-tauri/target/release/streamline-fg-package"
binaries = runtime if args.backend == "dlaa" else ROOT / "src-tauri/target/xess-native-aa/release"
game = json.loads((ROOT / "build/nr-interpass-20261017/game-1792212353/target-command.json").read_text(encoding="utf-8"))["arguments"][-1]
evidence = ROOT / "build/xess-game-ab-20261017"
evidence.mkdir(exist_ok=True)
name = args.name or f"{args.backend}-{int(time.time())}"
session = evidence / name
if session.exists():
    raise SystemExit(f"Session exists: {session}")
env = os.environ.copy()
for key in list(env):
    if key.startswith(("NS_STREAMLINE_", "NS_EMU_", "VK_ADD_")) or key in ["VK_INSTANCE_LAYERS", "VK_LOADER_LAYERS_ENABLE", "VK_LOADER_LAYERS_ALLOW", "RESHADE_BASE_PATH_OVERRIDE"]:
        env.pop(key, None)
env.update({
    "NS_EMU_XESS_DLL": str(ROOT / "src-tauri/target/xess-sdk-files-v3.0.2/bin/libxess.dll"),
    "NS_STREAMLINE_TRACE_VERBOSE": "0",
    "NS_STREAMLINE_TRACE_FRAMES": "1",
    "NS_STREAMLINE_SR_TIMING": "1",
    "NS_STREAMLINE_CAPTURE_REQUESTS": "1",
    "NS_STREAMLINE_ADVANCED_SETTINGS": json.dumps({"sr": {"autoExposure": True, "exposure": 100}}),
})
command = [str(binaries / "streamline-layer-probe.exe"), "--target-probe",
    "--target", str(emulator), "--layer", str(binaries / "streamline_probe_layer.dll"),
    "--session", str(session), "--game", game]
if args.backend == "dlaa":
    command += ["--graphics-launch", "--fixed-runtime", "--runtime", str(runtime), "--sr-mode", "dlaa", "--sr-scale", "100", "--nvof"]
    env["NS_STREAMLINE_NVOF_TIMING"] = "1"
else:
    env["NS_EMU_XESS_NATIVE_AA"] = "1"
    env["NS_EMU_XESS_AA_INITIAL_OFF"] = "1" if args.backend == "off" else "0"
config = emulator.parent / "portable/Config.json"
with (evidence / f"{name}.stdout.log").open("wb") as stdout, (evidence / f"{name}.stderr.log").open("wb") as stderr:
    process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, creationflags=0x08000000)
metadata = {"backend": args.backend, "launcher_pid": process.pid, "session": str(session), "command": command,
    "emulator_config_sha256": hashlib.sha256(config.read_bytes()).hexdigest(), "started_unix": time.time()}
(evidence / f"{name}.harness.json").write_text(json.dumps(metadata, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(metadata, ensure_ascii=False), flush=True)
