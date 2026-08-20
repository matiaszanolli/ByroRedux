# REN-DOC-03: audit-renderer SKILL.md Dimension 16 and _audit-common.md describe a froxel grid three defaults out of date

**Issue**: #3129 — https://github.com/matiaszanolli/ByroRedux/issues/3129
**Labels**: `low,renderer,documentation`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-20.md`

---

**Severity**: LOW
**Dimension**: Audit-skill drift
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-20.md` (REN-DOC-03)

## Location

- `.claude/commands/audit-renderer/SKILL.md` — Dimension 16 checklist (`:265`), Dimension 3 `CameraUBO` bullet (`:115`)
- `.claude/commands/_audit-common.md` — the `Volumetrics(M55)` project-layout row (`:48`)

## Status

NEW — **distinct from OPEN #3046** (REN-DOC-01), which covers Dimension 2/17/18 items, and from OPEN #3047 (REN-DOC-02), the shader-include roster.

## Description

Three separate drifts, all of which will manufacture false positives on the next run:

**1. Dimension 16 froxel defaults are two generations stale.** The skill states the grid is *"`froxel_xy_divisor` / `froxel_z_slices` (defaults 12 / 64 …), so 160×90×64 at 1080p native"*. The live default is **4 / 64** → 480×270×64. The `12` figure predates even the previous default of `8`. It also still says the per-FIF buffer is a single RGBA16F at 8 B/froxel; there are now six volumes at 44 B/froxel.

**2. `_audit-common.md`'s Volumetrics row says "160×90×128 froxel grid"** — wrong on the XY derivation (it is render-extent-derived, not fixed), wrong on the Z-slice count (64, not 128), and wrong about the volume count now that six volumes exist per FIF.

**3. Dimension 3 over-counts the `CameraUBO` declaration sites.** It says *"all 6 shaders that re-declare `CameraUBO` — `triangle.vert`, `triangle.frag`, `water.vert`, `water.frag`, `cluster_cull.comp`, `caustic_splat.comp`"*. There are **five** declaration sites: `include/bindings.glsl`, `triangle.vert`, `water.vert`, `cluster_cull.comp`, `caustic_splat.comp`. `triangle.frag` and `water.frag` now obtain it by `#include`. The 2026-08-16 report already recorded the correct count of five.

## Evidence

```
$ grep -n "froxel_xy_divisor" crates/renderer/src/vulkan/upscaling.rs
115:            froxel_xy_divisor: 4,
417:        assert_eq!(config.froxel_xy_divisor, 4);      # its own test agrees

$ grep -rn "uniform CameraUBO" crates/renderer/shaders/ | wc -l
5

$ grep -n "160×90×128" .claude/commands/_audit-common.md
48:Volumetrics(M55):crates/renderer/src/vulkan/volumetrics.rs + volumetrics/ (noise.rs)  (160×90×128 froxel grid, …)
```

## Impact

**A checklist that quotes stale numbers is worse than one that quotes none.** The Dimension 16 froxel figures are exactly the numbers this audit had to re-derive from source to produce the volumetrics VRAM finding — an auditor who trusted the skill would have sized the grid **9× low** and missed the finding entirely. (That is not hypothetical: the sibling `/audit-performance` skill carries the same stale `12` and its own report called that out as the reason the ledger error was easy to under-weight.) The `CameraUBO` over-count sends an auditor looking for two declaration sites that no longer exist, which reads as "someone deleted the mirrors" rather than "they were consolidated into an include".

## Suggested Fix

Correct all three. Prefer replacing the quoted defaults with a **pointer** to `VolumetricsConfig::default` and to `grep -c "uniform CameraUBO"`, the way the skill already does for `DBG_BITS` (*"read `DBG_BITS` rather than trusting any figure quoted here"*), so the next tuning change cannot invalidate the skill text again. Then re-run `.claude/commands/_audit-validate.sh`.

## Related

- OPEN #3046 (REN-DOC-01 — Dimension 2/17/18 checklist items describing deleted code)
- OPEN #3047 (REN-DOC-02 — the shader-include roster, **now stale by five**: `caustic_kernel.glsl` and `mesh_id.glsl` joined the 12 it already under-counted, so there are now 14 headers against a listed 9)
- The sibling `/audit-performance` skill carries the same stale `froxel_xy_divisor: 12`, filed separately

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — sweep every audit SKILL.md for transcribed constants that have a live source of truth
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh` passes after the edit
