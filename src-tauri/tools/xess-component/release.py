"""Prepare the three independent graphics Release packages; never publish them."""
from pathlib import Path
import subprocess
import sys

script = Path(__file__).resolve().parents[1] / "graphics-components/package.py"
subprocess.run([sys.executable, str(script), "--archives", *sys.argv[1:]], check=True)
