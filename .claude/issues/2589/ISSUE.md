# SKY-D7-01: Skyrim's parser arm zeroes two FO4-only BSLSP scalars, and the importer copies them un-gated -- canonical Material.fresnel_power is 0.0 instead of 5.0

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2589
**Finding ID**: SKY-D7-01

**Severity**: MEDIUM
**Dimension**: NIFAL Canonical Material Translation (Skyrim slice)
**Location**: producer `crates/nif/src/blocks/shader.rs:938-939` (`parse_skyrim`); un-gated copy `dedicated_shader.rs:321-322`; neutral defaults `import/material/mod.rs:1033-1034`, `import/types.rs:562,565`, `crates/core/src/ecs/components/material.rs:408`; boundary `byroredux/src/material_translate.rs:200`
**Status**: NEW

## Description
`grayscale_to_palette_scale`/`fresnel_power` are FO4+ wire fields (BSVER ≥ 130); every default site in the pipeline agrees on the neutral fallback (`1.0`/`5.0`) **except** `parse_skyrim`, which constructs the block with literal `0.0`/`0.0` for fields Skyrim never serializes. `apply_bs_lighting_shader` copies both unconditionally with no BSVER gate, so the Skyrim-arm `0.0` survives `into_imported_material` and lands in canonical `Material.fresnel_power = 0.0` for essentially all lit Skyrim geometry — while Oblivion/FO3/FNV (no BSLSP) keep `5.0` and FO4+ get their authored value. The canonical, game-agnostic `Material` diverges by source game on a field no game authors on Skyrim.

## Evidence
Confirmed directly: `shader.rs:938-939` — `grayscale_to_palette_scale: 0.0, fresnel_power: 0.0,` inside `parse_skyrim`; `dedicated_shader.rs:321-322` — `info.grayscale_to_palette_scale = shader.grayscale_to_palette_scale; info.fresnel_power = shader.fresnel_power;` with no gate. The very test meant to guard this (`material_info_default_matches_bslsp_parser_stub_defaults`) asserts only `MaterialInfo::default()`'s own literals against the FO76+ stopcond stub — it never compares against `parse_skyrim`, so it's structurally incapable of catching this exact drift.

## Impact
**Latent today** — `Material.fresnel_power` has no GPU consumer yet (the only `fresnel_power` hits in the renderer belong to an unrelated cell-ambient-cube term). The moment a `triangle.frag` consumer lands (the explicitly stated #2284 follow-up), Skyrim gets a Schlick exponent of `0.0` — `pow(1-cosθ,0)==1.0`, full Fresnel at every view angle, uniformly edge-bright/washed shading across all Skyrim content while FO4 renders correctly. A whole-game shading regression seeded now, detonating later at a site nobody will suspect. Rated MEDIUM (not the HIGH floor `_audit-severity.md` sets for wrong NIFAL output) because present-day live impact is nil; becomes HIGH the day the shading consumer lands.

## Related
#2284 (landed the six BSLSP scalars, promoting this latent parser quirk into canonical-tier state); #1241; SKY-D7-02 (this session)

## Suggested Fix
Make `parse_skyrim` construct both fields with the same neutral literals every other default site uses (`1.0`/`5.0`) — a one-line change per field, no downstream BSVER gate needed. Extend the guard test to assert the invariant against all three parser arms.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The fix keeps the neutral default decision at the parser boundary, not re-derived at the future shading consumer
- [ ] **TESTS**: Extend `material_info_default_matches_bslsp_parser_stub_defaults` to compare against all three parser arms (`parse_skyrim`, `material_reference_stub`, FO76+ stub), not just the FO76+ one
