"""Compatibility entry point for the shared three-package graphics delivery."""
from pathlib import Path
import subprocess
import sys

script = Path(__file__).resolve().parents[1] / "graphics-components/package.py"
subprocess.run([sys.executable, str(script), *sys.argv[1:]], check=True)
