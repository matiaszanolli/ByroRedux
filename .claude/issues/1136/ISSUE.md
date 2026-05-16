# Issue #1136 — PERF-D3-NEW-02: FX-mesh substring scan

**Source**: AUDIT_PERFORMANCE_2026-05-16
**Severity**: MEDIUM (perf, sub-finding of #1132)
**Status**: CLOSED in aee85ef6

## Resolution

Lifted 6-needle substring classification from per-frame draw loop to spawn-time `IsFxMesh` marker. Two spawn sites tagged (cell_loader::spawn + scene::nif_loader) via shared `texture_path_is_fx_mesh` helper. Hot path collapsed from 19 lines to 3.

+2 regression tests. Measurement not done from session (needs flamegraph on hardware).
