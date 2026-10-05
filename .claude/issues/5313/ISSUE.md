# #5313: TD3-2026-10-05-01: The window's file splits left code comments pointing at deleted or moved files (`npc_spawn/resumable.rs` ×6, `draw.rs` ×2, `walkers.rs:158-…` ×3)

Labels: low,tech-debt,documentation,doc-rot
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD3-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - **`npc_spawn/resumable.rs`** (deleted by #5091 / `37db35cca`, now `resumable/{mod,prebaked,runtime}.rs`):
    - `byroredux/src/player_body.rs:14,202`
    - `byroredux/src/scene_import_cache.rs:25`
    - `byroredux/src/material_translate.rs:2486`
    - `byroredux/src/npc_spawn.rs:1604`
    - `byroredux/src/save_io/round_trip_tests.rs:1431`
  - **`draw.rs`**:
    - `byroredux/src/render/camera.rs:115` says `origin_corrected_prev_view_proj` is "in
      `vulkan/context/draw.rs`". It is at `context/frame_params.rs:2102` since #5087.
    - `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:280` says it "mirrors the `GpuInstance` morph
      lookup in `draw.rs`". That lookup is at `context/build_and_upload_instances.rs:449`.
  - **`walkers.rs:158-190` / `158-204`**: `crates/plugin/src/esm/cell/wrld.rs:429,511` and
    `crates/plugin/src/esm/cell/tests/wrld.rs:354`. The interior XCRI arm is at `walkers.rs:344`.
- **Status**: NEW
- **Effort**: trivial
- **Description**:
  - `_audit-validate.sh` resolves backticked paths only in the audit skills and `docs/engine/*.md`. Code comments
    have no gate.
  - On the baseline tree, 130 backticked `.rs` references in comments resolved nowhere. At HEAD there are 141.
  - Most of the 141 are legitimate provenance ("Split from `boot.rs`"). The ones listed above are not: they
    claim the code lives there now.
- **Related**:
  - CONC-D1-2026-10-05-02: a FIF rider still points at `groundcover.rs` after #5089.
  - AUD-2026-10-05-D5-03: footstep doc sites.
  - TD2-2026-10-05-01.
- **Suggested Fix**:
  - Re-point the 11 sites.
  - Optionally extend `_audit-validate.sh`, or a hygiene test, to flag a comment that pairs a missing `.rs` path
    with "in"/"see"/"mirrors". Bare "split from" provenance would stay exempt.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
