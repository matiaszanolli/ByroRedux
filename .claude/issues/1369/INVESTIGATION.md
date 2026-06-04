# Investigation — #1369 PERF-D1-NEW-01 WRS reservoir occupancy

**Domain:** renderer (shader) · **Status: NOT fixed in code — see below**

## Premise check (2026-06-04): largely stale
- **Fix #2 (headline, 320B→128B) ALREADY DONE.** `triangle.frag:2816-2819`
  carries a `#1369` marker; `resRadiance[16]` (192 B vec3 array) is retired and
  pass 2 recomputes via `shadowableLightRadiance()` (`:833`). Current RIS-block
  storage is `uint resLight[16] + float resWSel[16]` = 128 B, not the issue's
  320 B. The occupancy hit in the premise is already >halved.
- **Fix #3 (noise hoist) DECLINED — counterproductive.** The streaming-pass
  `interleavedGradientNoise` (`:3049-3053`) depends on BOTH `s` (first-arg
  offset) AND `ci` (second-arg seed `resFrameSeed + float(ci)*0.37`) — a genuine
  per-candidate-per-slot WRS draw; hoisting the full eval breaks sampling. Only
  the `vec2(s*13.1, s*27.7)` offset is ci-invariant, but hoisting it needs a
  `vec2[16]` array (128 B) — adds the very storage the issue wants to cut, to
  save ~2 mults/iter. Wrong direction for an occupancy-bound kernel.
- **Fix #1 (NUM_RESERVOIRS spec-constant clamp) — genuinely open.** `:2810` still
  hardcodes `const uint NUM_RESERVOIRS = 16`. Per the issue's own "do NOT ship
  blind" note + `feedback_speculative_vulkan_fixes.md`, this needs a
  RenderDoc/Nsight occupancy capture before/after on the 4070 Ti — not
  performable on this headless host.

## Decision
No code shipped. User (during /fix-issue) chose the "safe noise hoist", but
investigation showed that hoist is counterproductive and fix #2 is already done.
Posted these findings as a GitHub comment and left #1369 OPEN, scoped to fix #1
pending a GPU occupancy capture. Recommended relabel: `needs-occupancy-capture`.
