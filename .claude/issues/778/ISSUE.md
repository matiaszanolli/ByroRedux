**Severity**: LOW
**Dimension**: Material Table (R1)
**Source**: AUDIT_RENDERER_2026-05-01.md

## Locations
- [crates/renderer/shaders/triangle.frag:592](../../tree/main/crates/renderer/shaders/triangle.frag#L592) — surrounding comment block describes legacy fields as if they still existed
- [crates/renderer/shaders/triangle.frag:699-704](../../tree/main/crates/renderer/shaders/triangle.frag#L699-L704) — "The per-instance `inst.roughness` slot is still populated by the CPU pipeline (Phase 6 drops it once every reader has migrated)" — Phase 6 already dropped them
- [crates/renderer/shaders/triangle.frag:914](../../tree/main/crates/renderer/shaders/triangle.frag#L914) — "modulation lerps from the upstream (`inst.roughness`)" — `roughness` no longer exists on `GpuInstance`
- [crates/renderer/shaders/triangle.frag:951](../../tree/main/crates/renderer/shaders/triangle.frag#L951) — "`inst.envMapIndex != 0u`" — `envMapIndex` lives on `materials[…]` now

## Description

Several comments in `triangle.frag` were written during Phase 4-5 transition and describe `inst.<field>` as "still populated" or "byte-equal to" `materials[…].<field>`. Phase 6 (commit `22f294a`) dropped those fields entirely; the comments now refer to non-existent struct members.

## Evidence

```glsl
// triangle.frag:699-704 — wrong as of Phase 6
// R1 Phase 4 — first migrated field. `roughness` now reads from the
// deduplicated `MaterialBuffer` SSBO via `inst.materialId`. The
// per-instance `inst.roughness` slot is still populated by the CPU
// pipeline (Phase 6 drops it once every reader has migrated); the
// value at `materials[inst.materialId].roughness` is byte-equal to
// it for now, so the visible output is unchanged.
```

`GpuInstance` no longer carries `roughness` (or any of the other migrated fields). The current `triangle.frag:706` correctly reads `mat.roughness`; only the surrounding comment is stale.

## Impact

Minor reader confusion; no functional issue. Future shader edits could be misled into trying to read `inst.roughness` (which would fail to compile, so harm is bounded).

## Suggested Fix

Sweep the four sites and rewrite the comments to describe the current state. Single PR, ~5 minutes:

- `triangle.frag:699-704`: rewrite to describe the now-final state ("R1 Phase 6 collapsed every per-material field onto the MaterialBuffer SSBO; per-instance copies dropped").
- `triangle.frag:914`: change `inst.roughness` → `mat.roughness`.
- `triangle.frag:951`: change `inst.envMapIndex` → `mat.envMapIndex`.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Sweep all 4 R1-touched shaders (triangle.vert, triangle.frag, ui.vert, caustic_splat.comp) for any other `inst.<field>` comment references to migrated fields
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A — comment-only change
