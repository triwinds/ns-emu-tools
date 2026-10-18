"""Launch opt-in game XeSS FG with zero motion/fixed depth; no optical flow."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument("--name", required=True)
parser.add_argument("--emulator", type=Path, default=Path("D:/Ryujinx_Nextendo_DLSS/Ryujinx.exe"))
parser.add_argument("--game", type=Path)
parser.add_argument("--max-edge", type=int, default=960)
parser.add_argument("--initial-off", action="store_true")
args = parser.parse_args()
emulator = args.emulator.resolve()
binaries = ROOT / "src-tauri/target/xess-fg-game/release"
if args.game:
    game = str(args.game.resolve())
else:
    previous = ROOT / "build/nr-interpass-20261017/game-1792212353/target-command.json"
    if not previous.is_file():
        raise SystemExit("Pass --game <XCI/NSP path>; no previous local game session exists")
    game = json.loads(previous.read_text(encoding="utf-8"))["arguments"][-1]
if not emulator.is_file() or not Path(game).is_file():
    raise SystemExit("Emulator/game file does not exist")
evidence = ROOT / "build/xess-fg-20261017"
evidence.mkdir(exist_ok=True)
session = evidence / args.name
if session.exists():
    raise SystemExit(f"Session exists: {session}")
env = os.environ.copy()
for key in list(env):
    if key.startswith(("NS_STREAMLINE_", "NS_EMU_", "VK_ADD_")) or key in ["VK_INSTANCE_LAYERS", "VK_LOADER_LAYERS_ENABLE", "VK_LOADER_LAYERS_ALLOW", "RESHADE_BASE_PATH_OVERRIDE"]:
        env.pop(key, None)
env.update({
    "NS_EMU_XESS_FG": "1",
    "NS_EMU_XESS_FG_SDK": str(ROOT / "src-tauri/target/xess-fg-sdk-v3.0.2"),
    "NS_EMU_XESS_FG_AGILITY": str(ROOT / "src-tauri/tools/xess-fg-diagnostics/target/release/D3D12"),
    "NS_EMU_XESS_FG_MAX_EDGE": str(args.max_edge),
    "NS_EMU_XESS_FG_INITIAL_OFF": "1" if args.initial_off else "0",
    "NS_STREAMLINE_TRACE_VERBOSE": "0",
    "NS_STREAMLINE_TRACE_FRAMES": "1",
})
command = [str(binaries / "streamline-layer-probe.exe"), "--target-probe",
    "--target", str(emulator), "--layer", str(binaries / "streamline_probe_layer.dll"),
    "--session", str(session), "--game", game]
config = emulator.parent / "portable/Config.json"
with (evidence / f"{args.name}.stdout.log").open("wb") as stdout, (evidence / f"{args.name}.stderr.log").open("wb") as stderr:
    process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, creationflags=0x08000000)
metadata = {"launcher_pid": process.pid, "session": str(session), "command": command,
    "zero_motion": True, "fixed_depth": 0.5, "optical_flow": False,
    "max_edge": args.max_edge, "initial_off": args.initial_off,
    "layer_sha256": hashlib.sha256((binaries / "streamline_probe_layer.dll").read_bytes()).hexdigest(),
    "launcher_sha256": hashlib.sha256((binaries / "streamline-layer-probe.exe").read_bytes()).hexdigest(),
    "emulator_config_sha256": hashlib.sha256(config.read_bytes()).hexdigest(), "started_unix": time.time()}
(evidence / f"{args.name}.harness.json").write_text(json.dumps(metadata, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(metadata, ensure_ascii=False), flush=True)
