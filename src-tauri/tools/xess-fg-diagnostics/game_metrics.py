"""Summarize completed opt-in game runs; SDK queue rate is not scanout FPS."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("session", type=Path)
args = parser.parse_args()
session = args.session.resolve()
rows = [json.loads(line) for line in (session / "layer.jsonl").read_text(encoding="utf-8").splitlines()]
exit_status = json.loads((session / "target-exit.json").read_text(encoding="utf-8"))
marker = session / "gameplay-observation.json"
minimum = json.loads(marker.read_text())["conservative_min_elapsed_us"] if marker.exists() else 0
segments = []
for row in rows:
    if row["event"] != "xess_fg_frame":
        continue
    value = row["details"]
    if (not segments or segments[-1][-1]["details"]["enabled"] != value["enabled"]
            or segments[-1][-1]["details"]["frame"] >= value["frame"]):
        segments.append([])
    segments[-1].append(row)


def quantile(values, fraction):
    ordered = sorted(values)
    return ordered[round((len(ordered) - 1) * fraction)] if ordered else None


summaries = []
for segment in segments:
    # Discard switch/recreation warmup and all frames before observed gameplay.
    measured = [row for row in segment[32:] if row["elapsed_us"] >= minimum]
    if len(measured) < 2:
        continue
    span = (measured[-1]["elapsed_us"] - measured[0]["elapsed_us"]) / 1e6
    values = [row["details"] for row in measured]
    source_rate = (len(values) - 1) / span
    queued_rate = sum(value["frames_queued"] for value in values[1:]) / span
    summaries.append({
        "enabled": values[0]["enabled"], "measured_frames": len(values), "seconds": span,
        "source_present_fps": source_rate, "sdk_queued_fps": queued_rate,
        "frames_queued_counts": dict(Counter(value["frames_queued"] for value in values)),
        "fg_result_counts": dict(Counter(value["fg_result"] for value in values)),
        "native_result_counts": dict(Counter(value["native_result"] for value in values)),
        "bridge_cpu_wall_ms_p50": quantile([value["processing_cpu_us"] / 1000 for value in values], .5),
        "bridge_cpu_wall_ms_p95": quantile([value["processing_cpu_us"] / 1000 for value in values], .95),
    })
failures = [row for row in rows if row["event"] == "xess_fg_failure" or
            (row["event"] == "xess_fg_runtime" and
             (row["details"].get("event") in ["fatal", "game_runtime_failure"] or row["details"].get("level", 0) >= 3))]
report = {
    "session": str(session), "exit": exit_status,
    "zero_motion": True, "fixed_depth": .5, "optical_flow": False,
    "gameplay_observation": json.loads(marker.read_text()) if marker.exists() else None,
    "bridge_configurations": [row["details"] for row in rows if row["event"] == "xess_fg_bridge_ready"],
    "guide_readbacks": [row["details"] for row in rows if row["event"] == "xess_fg_runtime" and row["details"].get("event") == "game_guides_readback"],
    "segments": summaries, "failures": failures,
    "layer_trace_sha256": hashlib.sha256((session / "layer.jsonl").read_bytes()).hexdigest(),
    "limits": "Sequential bridge CPU wall time includes GPU waits/presentation; SDK queued FPS is not physical scanout. Placeholder guides cannot validate motion quality or input latency.",
}
(session / "game-report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(report, ensure_ascii=False, indent=2))
if (exit_status.get("code") != 0 or failures or not summaries or
        any(row["mismatched_pixels"] for row in report["guide_readbacks"]) or
        any(any(value != 0 for value in segment["fg_result_counts"]) or
            any(value != 0 for value in segment["native_result_counts"]) or
            set(segment["frames_queued_counts"]) != {2 if segment["enabled"] else 1}
            for segment in summaries)):
    raise SystemExit(1)
