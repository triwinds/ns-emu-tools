# Retained validation evidence

The repository keeps only the five recorded fixtures used by `include_str!` regression tests and the compact final game measurements under `ryujinx-fg-2026-09-25`. Bulk per-frame logs, repeated experiment runs, machine snapshots and screenshots are disposable and are excluded from source control.

Historical experiment documents may name session paths that are no longer retained. They record past investigations, not a claim that those full logs ship with the source. The retained `live-control.json` records the observed on/off/on presentation rates and the subsequent window-operation stop. These CPU-side counts do not prove display scanout or visual quality. Experimental block-matching motion estimation remains opt-in and off in toolbox launches.
