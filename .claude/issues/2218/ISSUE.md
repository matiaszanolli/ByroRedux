# #2218 — REN-2026-07-28-BLOCK-01: FO3 Megaton exterior geometry renders pure white (suspected non-finite shading term) — needs RenderDoc

_Filed from `docs/audits/AUDIT_RENDERER_2026-07-28.md` by `/audit-publish` on 2026-07-28. Immutable snapshot of the issue **as filed** — GitHub is authoritative for current state (`gh issue view 2218 --json state`)._

---

**Severity:** HIGH · **Dimension:** 18 (sky/weather/exterior), possible interaction in 2, 8, 17
**Source:** `docs/audits/AUDIT_RENDERER_2026-07-28.md` — REN-2026-07-28-BLOCK-01
**Status when filed:** Recorded in `ROADMAP.md` but never tracked as an issue. **Needs-RenderDoc.**

## Description

FO3 `megatonworld` exterior structures render as pure white (≈95.4% of the frame at
`(255,255,255)`) while the procedural sky renders correctly. This is a live render
blocker for a representative FO3 exterior, and it invalidates any blanket
"FO3 exterior works" claim.

The working hypothesis is non-finite (Inf/NaN) data in the exterior directional
shadow/GI path. **That is not source-proven** — this issue exists to hold the capture
work, not to license a speculative shader edit.

## Evidence

From `ROADMAP.md:970` (surfaced Session 60):

- Exposure bisection: crushing exposure `0.85 → 0.02` (42×) scales the sky exactly as
  expected but **does not move the geometry by a single pixel**. A value that survives a
  42× reduction is non-finite, not merely bright (`ACES(Inf) → 1.0`; `NaN → white`).
- Ruled out by measurement:
  - missing textures — `tex.missing` reports none
  - fog — `fog_clip` / `fog_power` are `None` on exteriors
  - light attenuation — `DBG_LEGACY_LIGHT_ATTEN` is pixel-identical
  - sun contract — `direction_angle` valid, `radius=0` is the documented directional
    convention, `emitterRadius=0` is guarded
- Structural asymmetry is the likely locus: exteriors upload the directional at
  `radius=0` (shadow rays **traced**) while interiors use `radius=-1` (shadow rays
  **skipped**) — and interiors render correctly.

## Impact

A representative FO3 exterior is not meaningfully renderable. Because the symptom is
"geometry saturates regardless of exposure", it also masks any other exterior shading
defect in the same cell.

## Next Step (do NOT patch speculatively)

1. Add `isnan` / `isinf` debug visualization around the direct, indirect, shadow, and GI
   terms in `triangle.frag`.
2. Bisect which term first goes non-finite, then capture the first corrupt pass in
   RenderDoc.
3. Only then design the fix. Do **not** patch exposure, ACES, or the sun path from static
   reasoning — see the project's standing rule against speculative Vulkan/shader fixes
   whose failure modes are invisible to `cargo test`.

## Repro

```bash
cargo run --release -- --game fo3 --grid <megatonworld grid> --radius 3 --bsa … --bench-hold
# then attach: cargo run -p byro-dbg
```

## Completeness Checks
- [ ] **SIBLING**: Once the non-finite source is found, check whether FNV/Oblivion
      exteriors share the same directional `radius=0` shadow-ray path
- [ ] **TESTS**: A regression test pins this specific fix (shader-level finite guard or a
      runtime telemetry baseline for the cell)
- [ ] Capture attached (RenderDoc + per-term NaN/Inf visualization) before any fix lands
