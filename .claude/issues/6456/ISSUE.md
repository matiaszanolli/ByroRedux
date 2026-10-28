# EXT-D2-2026-10-27-02: exal.md records none of the 2026-10-26/27 full-detail <-> LOD join work

Labels: low, terrain-exterior, documentation, doc-rot
Source: docs/audits/AUDIT_EXTERIOR_2026-10-27.md

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-27.md` finding `EXT-D2-2026-10-27-02` (Dimension 2 / 6, spec doc) · Severity LOW (doc-rot) · Status NEW (same class as open #6416, which covers `skyal.md`).

**Location**: `docs/engine/exal.md`

## Description
`exal.md` has no entry for the 2026-10-26/27 full-detail <-> LOD join work:

- `terrain_lod_albedo_gain`, a per-game EXAL table whose measured values (Skyrim 0.36 from one Whiterun scene; FNV and FO4 1.0) live only in rustdoc.
- The synth boundary block wearing its quad's baked `.btr` diffuse and normal (`74e1b0788`, `btr_quad_texture`).
- The skirts and rim mask (#6409).
- Distance softening (`e71451d08`).

## Evidence
`git log --since=2026-10-25 -- docs/engine/exal.md` lists only `d32e2ade8`. A grep of `exal.md` for `albedo_gain`, `skirt` and `soften` finds nothing. The only prose mention of softening is in `shader-pipeline.md` (lane 220) and `debug-cli.md`.

## Impact
The spec doc for the exterior layer omits four shipped behaviours, including a measurement-derived constant whose caveat (one scene) is recorded only in rustdoc.

## Related
#6416 (skyal.md silent on the drift cap), `EXT-D2-2026-10-27-01`, #6409.

## Suggested Fix
Add one `exal.md` section "terrain-LOD join" citing the four items, with the gain's measurement caveat.

## Completeness Checks
- [ ] **SIBLING**: `docs/engine/exal-groundcover.md` and `shader-pipeline.md` cross-reference the new section
- [ ] **TESTS**: Any doc-scan guard that enumerates exal.md sections is updated

