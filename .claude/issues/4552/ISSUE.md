# NIFAL-D1-2026-09-21-01: #4444's named-default sweep missed terrain_lod_btr.rs — .btr distant terrain still restates the parallax defaults as literals

**Labels**: low, nifal, tech-debt, bug

**Severity**: LOW · **Dimension**: Material (canonical-default single source) · **Tier Violated**: single-boundary · **Game Affected**: Skyrim / FO4 (`.btr`) vs FNV (`terrain_lod.rs`) — diverge on retune
**Location**: `byroredux/src/cell_loader/terrain_lod_btr.rs:394-395`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
f5cddc7c5 (#4444) replaced bare `0.04`/`4.0` parallax literals with `DEFAULT_PARALLAX_HEIGHT_SCALE`/`DEFAULT_PARALLAX_MAX_PASSES` at `terrain.rs`, `terrain_lod.rs`, `render/particles.rs` and the `GpuMaterial` neutral — but left the `.btr` spawner on the literals (the only remaining production site restating either literal, repo-wide grep). A future retune of the named constants now splits distant terrain by game: FNV `terrain_lod` moves, Skyrim/FO4 `.btr` stays. Exactly the per-game divergence the #3073/#4444 doctrine exists to prevent.

### Evidence
`terrain_lod_btr.rs:394-395`: `parallax_height_scale: 0.04,` / `parallax_max_passes: 4.0,` vs `terrain_lod.rs:864-868` / `terrain.rs:1094-1098` on the named constants.

### Impact
None today (literals equal the constants). On retune, `.btr` distant terrain keeps the old POM parameters while every other synthetic path moves — silent, untestable (both files build `MaterialTextureHandles` inline; no guard compares them).

### Related
#4444, #3073

### Suggested Fix
Swap the two literals for the named constants (2-line change); optionally extend #4444's regression test to grep for bare parallax literals in production code.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
