"""Launch the packaged XeSS component without the GUI for bounded manual smoke checks."""
import argparse
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument("--name", required=True)
parser.add_argument("--mode", choices=["aa", "fg", "both"], default="both")
parser.add_argument("--game", type=Path)
parser.add_argument("--emulator", type=Path, default=Path("D:/Ryujinx_Nextendo_DLSS/Ryujinx.exe"))
parser.add_argument("--scale", type=int, default=100)
parser.add_argument("--max-edge", type=int, default=0)
args = parser.parse_args()
bundle = ROOT / "src-tauri/target/release/xess-package"
session = ROOT / "build/xess-component-20261017" / args.name
if session.exists():
    raise SystemExit("Session already exists")
session.parent.mkdir(parents=True, exist_ok=True)
game = str(args.game.resolve()) if args.game else json.loads((ROOT / "build/nr-interpass-20261017/game-1792212353/target-command.json").read_text(encoding="utf-8"))["arguments"][-1]
env = {key: value for key, value in os.environ.items() if not key.startswith(("NS_EMU_", "NS_STREAMLINE_", "VK_"))}
env.update({
    "NS_EMU_XESS_COMPONENT_ROOT": str(bundle),
    "NS_EMU_XESS_NATIVE_AA": "0" if args.mode == "fg" else "1",
    "NS_EMU_XESS_DLL": str(bundle / "libxess.dll"),
    "NS_EMU_XESS_FG": "0" if args.mode == "aa" else "1",
    "NS_EMU_XESS_FG_SDK": str(bundle),
    "NS_EMU_XESS_FG_AGILITY": str(bundle / "D3D12"),
    "NS_EMU_XESS_INPUT_SIZING": json.dumps({"scalePercent":args.scale,"maxEdge":args.max_edge}),
    "NS_STREAMLINE_TRACE_VERBOSE": "0",
    "NS_STREAMLINE_TRACE_FRAMES": "1",
})
command = [str(bundle / "xess-component.exe"), "--target-probe", "--graphics-launch", "--target", str(args.emulator.resolve()), "--layer", str(bundle / "xess-layer.dll"), "--session", str(session), "--game", game]
with (session.parent / f"{args.name}.launcher.log").open("wb") as stdout, (session.parent / f"{args.name}.launcher-error.log").open("wb") as stderr:
    process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, creationflags=0x08000000)
print(json.dumps({"launcherPid": process.pid, "session": str(session), "mode": args.mode}))
