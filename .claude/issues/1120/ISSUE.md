# Issue #1120 — 10 LOW stragglers batch (Dim 2/3/6/8/10)

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-16.md`
**Severity**: LOW × 10
**Labels**: low, tech-debt

## Items

- TD2-204 — `VF_FULL_PRECISION` dup in `sse_recon.rs:57`
- TD3-201 — `compute_storage_image_barrier()` helper extraction (8+ sites)
- TD3-204 — `impl_ni_object!` macro adoption — 33 hand-rolled impls remain
- TD3-205 — Vec destroy-and-clear pattern in `scene_buffer/descriptors.rs::destroy`
- TD6-202 — `data_dir(env, default)` duplicated across 5 crates → `byroredux-testutil`
- TD6-203 — Golden frame baseline `cube_demo_60f.png` 7+ days stale
- TD8-017 — `watr_to_params(record, _game: GameKind)` — drop `_game` placeholder
- TD8-019 — `legacy/mod.rs:24-32` obituary breadcrumb — delete
- TD10-005 — `audit-tech-debt.md:182` references nonexistent `streaming.rs:286`
- TD10-003/004 — (already in #1117, cross-ref only)

Fix with: `/fix-issue 1120`
