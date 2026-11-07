**Source**: `docs/audits/AUDIT_EXTERIOR_2026-11-07.md` (EXT-D3-02).

- **Severity**: LOW
- **Dimension**: Ground-cover pipeline
- **Location**: `crates/renderer/shaders/groundcover_blade.frag` (`groundLab.x > 0.1` near-black ground cutoff; the 6-iteration gamut bisection); `crates/renderer/shaders/groundcover_blade.vert` (`vec2(17.3, 41.9)` height-field noise offset)
- **Tier Violated**: no-fabrication
- **Game Affected**: all
- **Status**: NEW

## Description
`da6d65dc1` named `GROUNDCOVER_NEUTRAL_LUMA`, `GROUNDCOVER_GROUND_COLOUR_FOOTPRINT` and `GROUNDCOVER_HEIGHT_FIELD_*` in `shader_constants_data.rs`, but left these threshold and offset values inline. The exterior audit skill states that a new bare literal in `groundcover_*` GLSL is a finding (the bare `tau` literals were fixed by #6414). None of these is cited or registered in `docs/engine/exal-groundcover.md` §12.12. The OKLab matrices are sourced (Ottosson) and fine; the `.a > 0.5` known-colour tests are a flag convention, not a tunable.

## Evidence
- `groundcover_blade.frag`: `vec2 groundAb = groundLab.x > 0.1 ? ...`
- `groundcover_blade.vert`: `... byroGcFbm(base.xz / GROUNDCOVER_HEIGHT_FIELD_UNITS + vec2(17.3, 41.9)) ...`
- `GROUNDCOVER_HEIGHT_FIELD_UNITS` / `_VARIATION` are named in `shader_constants_data.rs`; the offset and the cutoff are not.

## Impact
Untracked art-direction values. The near-black cutoff silently decides when a texture's colour is replaced by the palette chroma.

## Related
#4378 (blade-shaping constants registered uncited in §12.12), #6414.

## Suggested Fix
Name the cutoff and the noise offset in `shader_constants_data.rs` (for example `GROUNDCOVER_CHROMA_MIN_LIGHTNESS`), emit them through the generated header, and register them as art direction in §12.12. Recompile the `.spv`.

## Completeness Checks
- [ ] **SIBLING**: other `groundcover_*` shaders and `include/groundcover_*.glsl` checked for inline tunables
- [ ] **TESTS**: a source scan (or the existing constants-parity test) fails on a new bare numeric tunable in the blade shaders
