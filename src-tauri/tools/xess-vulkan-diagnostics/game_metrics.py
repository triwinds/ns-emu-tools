"""Bounded native XeSS on/off sampling; run while the camera stays stationary."""
import argparse
import json
from pathlib import Path
import statistics
import time

parser = argparse.ArgumentParser()
parser.add_argument("session", type=Path)
parser.add_argument("--seconds", type=int, default=15)
args = parser.parse_args()
segments = []
control = args.session / "xess-control.json"
start_us = 0
for line in (args.session / "layer.jsonl").read_text(encoding="utf-8").splitlines():
    try:
        start_us = json.loads(line)["elapsed_us"]
    except (json.JSONDecodeError, KeyError):
        continue
for enabled in (True, False, True, False):
    started = time.time()
    control.write_text(json.dumps({"enabled": enabled}), encoding="ascii")
    print(f"{'XeSS' if enabled else 'Off'}: {args.seconds}s", flush=True)
    time.sleep(args.seconds)
    segments.append({"enabled": enabled, "start_unix": started, "end_unix": time.time()})
# Trace elapsed_us is anchored by the process trace's first wall-clock event.
# Use consecutive runs of requested/active flags, discarding 60 frames at edges.
rows = []
profiles = {}
for line in (args.session / "layer.jsonl").read_text(encoding="utf-8").splitlines():
    try:
        r = json.loads(line)
    except json.JSONDecodeError:
        continue
    d = r["details"]
    if r["elapsed_us"] < start_us:
        continue
    if r["event"] == "xess_native_frame":
        rows.append((r["elapsed_us"], d))
    elif r["event"] == "target_xess_aa_profile":
        profiles[d["frame"]] = d["gpu_us"]
runs = []
for row in rows:
    if not runs or runs[-1][-1][1]["requested"] != row[1]["requested"]:
        runs.append([])
    runs[-1].append(row)
def stats(values):
    values = sorted(values)
    return {"p50": statistics.median(values), "p95": values[int((len(values)-1)*.95)]}
results = []
for run in runs[-4:]:
    safe = run[60:-20]
    if len(safe) < 30:
        continue
    interval = [(b[0]-a[0])/1000 for a,b in zip(safe, safe[1:])]
    profile = [profiles[d["frame"]] for _,d in safe if d["frame"] in profiles]
    results.append({"requested":safe[0][1]["requested"], "samples":len(safe),
        "frames":[safe[0][1]["frame"],safe[-1][1]["frame"]],
        "all_active":all(d["active"] for _,d in safe),
        "present_interval_ms":stats(interval),
        "average_fps":1000/statistics.mean(interval),
        "processing_cpu_ms":stats([d["processing_cpu_us"]/1000 for _,d in safe]),
        "gpu_ms":{k:stats([p[k]/1000 for p in profile]) for k in profile[0]} if profile else None})
report = {"segments":segments,"results":results,
    "limits":"Present intervals in capped gameplay, not GPU rendering FPS; ON retains allocations when OFF. Captures excluded; 60 initial and 20 final frames trimmed from each state run."}
(args.session / "ab-metrics.json").write_text(json.dumps(report,indent=2),encoding="utf-8")
print(json.dumps(results,indent=2),flush=True)
