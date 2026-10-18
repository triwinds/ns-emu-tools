"""Check both routes and hybrid routing using a short-lived fake emulator (no GPU/game).

This validates launcher arguments, DLL contracts, environment isolation and exit handling.
It does not replace hardware or image-quality validation.
"""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
COMPONENTS = ROOT / "src-tauri/target/release/graphics-components"


def main():
    ctypes.windll.kernel32.SetErrorMode(0x8007)
    scratch = ROOT / "src-tauri/target/graphics-smoke"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=scratch) as directory:
        base = Path(directory)
        source = base / "target.rs"
        source.write_text('''fn main() {
            let session = std::env::var("NS_STREAMLINE_LIVE_DIR").unwrap();
            let keys = ["NS_STREAMLINE_TARGET_SDK", "NS_EMU_XESS_NATIVE_AA", "NS_EMU_XESS_FG", "NS_EMU_XESS_HYBRID"];
            let output = keys.iter().map(|key| format!("{}={}\\n", key, std::env::var(key).unwrap_or_default())).collect::<String>();
            std::fs::write(std::path::Path::new(&session).join("observed.txt"), output).unwrap();
            if std::env::var("SMOKE_FAIL").as_deref() == Ok("1") { std::process::exit(7); }
        }''', encoding="utf-8")
        target = base / "Ryujinx.exe"
        subprocess.run(["rustc", "--crate-name", "smoke_target", "-C", "target-feature=+crt-static", str(source), "-o", str(target)], check=True)
        sha = hashlib.sha256(target.read_bytes()).hexdigest()
        common = COMPONENTS / "common"
        results = []
        for mode in ["nvidia", "xess", "hybrid", "xess-failure"]:
            xess = mode.startswith("xess")
            session = base / mode
            env = {key: value for key, value in os.environ.items()
                   if not key.upper().startswith(("NS_EMU_", "NS_STREAMLINE_", "VK_"))}
            env.update({"NS_EMU_XESS_COMPONENT_ROOT": str(COMPONENTS / "xess"),
                        "NS_EMU_XESS_NATIVE_AA": "1", "NS_EMU_XESS_FG": "1",
                        "NS_EMU_XESS_DLL": str(COMPONENTS / "xess/libxess.dll"),
                        "NS_EMU_XESS_FG_SDK": str(COMPONENTS / "xess"),
                        "NS_EMU_XESS_FG_AGILITY": str(COMPONENTS / "xess/D3D12")})
            args = [str(common / "graphics-launcher.exe"), "--target-probe", "--backend", "xess" if xess else "nvidia",
                    "--graphics-launch", "--target", str(target), "--layer", str(common / ("xess-layer.dll" if xess else "streamline_probe_layer.dll")),
                    "--session", str(session), "--expected-target-sha256", sha, "--allow-unverified-target"]
            if not xess:
                args += ["--fixed-runtime", "--runtime", str(COMPONENTS / "nvidia")]
            if mode == "hybrid":
                env["NS_STREAMLINE_ADVANCED_SETTINGS"] = json.dumps({"fg": {"backend": "xess"}})
                args += ["--xess-runtime", str(COMPONENTS / "xess")]
            if mode == "xess-failure":
                env["SMOKE_FAIL"] = "1"
            result = subprocess.run(args, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace")
            assert result.returncode == (1 if mode == "xess-failure" else 0), (mode, result.stdout, result.stderr)
            observed = dict(line.split("=", 1) for line in (session / "observed.txt").read_text().splitlines())
            assert observed["NS_STREAMLINE_TARGET_SDK"] == ("0" if xess else "1"), observed
            assert observed["NS_EMU_XESS_NATIVE_AA"] == ("1" if xess else "0"), observed
            assert observed["NS_EMU_XESS_FG"] == ("1" if xess or mode == "hybrid" else "0"), observed
            status = json.loads((session / "launch-result.json").read_text())
            assert not status["strict_acceptance_run"]
            assert status["target_exit_code"] == (7 if mode == "xess-failure" else 0)
            results.append({"mode": mode, "passed": True, "targetExitCode": status["target_exit_code"], "environment": observed})
        print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
