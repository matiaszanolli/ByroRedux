# #4310: REN-2026-09-14-D14-01: `CausticPipeline::dispatch` still documents a fixed 0.15 parked new-sample weight, and `advance_parked_visits` points at `context::draw` for a flag that lives in `build_and_upload_instances.rs`

- **Labels**: low,renderer,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4310
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Caustics
- **Location**: `crates/renderer/src/vulkan/caustic.rs` (`CausticPipeline::dispatch` splat-dispatch comment; `advance_parked_visits` doc)
- **Status**: NEW
- **Description**:
  1. The comment above the splat `cmd_push_constants` in `dispatch` reads: "decay_factor drives the EMA new-sample weight (1 - decay_factor) in the shader: 0.15 of this frame while parked, full energy while moving". The live decay is not a fixed 0.85:
     - `advance_parked_visits` returns `(n / (n + 1.0)).min(CAUSTIC_DECAY_MAX)`, with `CAUSTIC_DECAY_MAX = 0.995`.
     - So the new-sample weight is `1/(n+1)`: 0.5 on the first parked visit, falling to a 0.005 floor.
     - A few lines earlier the same function explains why a constant decay was replaced ("a constant decay (e.g. 0.96) plateaus…").
  2. `advance_parked_visits`'s doc says "see `context::draw`'s `caustic_history_valid`". `caustic_history_valid` is computed in `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`; `grep -n caustic_history_valid crates/renderer/src/vulkan/context/draw.rs` has no definition.
- **Evidence**:
  - `grep -n "0\.15" crates/renderer/src/vulkan/caustic.rs` → only the splat comment.
  - `const CAUSTIC_DECAY_MAX: f32 = 0.995;` and `advance_parked_visits`'s body.
- **Impact**: Documentation only. Anyone tuning the parked caustic convergence (or the #2468 scene-dirty gate) from the comment would expect a ~6-frame EMA. The actual path is a ~200-visit running average, a very different stale-pool window.
- **Related**: #2468, #4009 (closed, earlier caustic/water `file:NN` doc-rot sweep), REN-2026-09-14-D8-01 (the same `caustic_history_valid` now also feeds SVGF).
- **Suggested Fix**: Replace "0.15 of this frame while parked" with "`1/(N+1)` of this frame while parked (floored at `1 - CAUSTIC_DECAY_MAX`)", and point the `advance_parked_visits` cross-reference at `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
