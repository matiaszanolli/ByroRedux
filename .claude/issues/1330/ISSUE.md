**Severity:** MEDIUM · **Dimension:** Block Parsing / Stream Position · **Game Affected:** Fallout 3, Fallout New Vegas (NoLighting content at bsver ≤ 26; 22 instances each in `Fallout - Meshes.bsa`)

From audit `docs/audits/AUDIT_NIF_2026-05-29.md` (finding NIF-2026-05-29-02; merges Dim 1 + Dim 3, same lines).

## Description
The four falloff fields (`falloff_start_angle/stop_angle/start_opacity/stop_opacity` = 4×f32 = 16 bytes) are read when `stream.variant().avobject_flags_u32()` is true. That predicate is **game-enum keyed** (`version.rs:480-491`: true for FO3/FNV/Skyrim+/FO4/FO76/Starfield) and is semantically the *NiAVObject flag-width* test, not the falloff-presence test. nif.xml (6236-6239) gates these on `vercond="#BSVER# #GT# 26"`. FO3/FNV ship NoLighting content down to bsver 11/14/24/26; for those files the parser reads 16 bytes that aren't there, consuming the first 16 bytes of the **following** block.

## Location
`crates/nif/src/blocks/shader.rs:165-175` (`BSShaderNoLightingProperty::parse`)

## Evidence
Corpus drift sweep (`nif_stats --drift-histogram`): `22× BSShaderNoLightingProperty drift=-16` on **both** FO3 and FNV mesh archives (`-16` = consumed 16 over the declared block_size; `16 = 4×sizeof(f32)`). The sibling `BSShaderPPLightingProperty::parse` (same file, shader.rs:82-101) does this correctly via per-file `bsver > FO3_PARALLAX(24)` gates. The matching constant `bsver::FLAGS_U32_THRESHOLD = 26` exists (`crates/nif/src/version.rs:224`) but is **unused** here (used correctly in `properties.rs:44`).

## Impact
(1) The four falloff floats are populated with garbage bytes read from the next block → wrong cone angles / opacities on affected FO3/FNV decals & glow-map surfaces (per #451's importer wiring, the falloff reaches the GPU). (2) block_size realigns the stream so no downstream desync — a data-correctness bug, not a crash. (Dim 1 initially rated this LOW assuming no shipping content was affected; Dim 3's corpus sweep empirically disproved that — 22 real instances per game — so it is MEDIUM.)

## Suggested Fix
Replace the `shader.rs:166` predicate `if stream.variant().avobject_flags_u32() {` with `if stream.bsver() > crate::version::bsver::FLAGS_U32_THRESHOLD {`. Add an FO3/FNV bsver≤26 fixture asserting `consumed == declared` and falloff defaults (0,0,1,0). `Sky`/`TileShaderProperty` share the `parse_fo3` base but don't read falloff — unaffected.

## Related
#451 (CLOSED, importer-side capture), #429 (CLOSED, analogous NiTexturingProperty over-read), #939 (CLOSED, drift histogram that surfaced this). **Same anti-pattern, different site:** the NiAVObject flag-width finding (NIF-2026-05-29-03) — fix both in one pass.

## Completeness Checks
- [ ] **SIBLING**: Fix the twin `avobject_flags_u32()`-as-BSVER-gate misuse at `base.rs:78` (companion finding) in the same change
- [ ] **SIBLING**: Grep for any other `avobject_flags_u32()` call site standing in for a per-file `BSVER > 26` gate
- [ ] **TESTS**: Regression test added — FO3/FNV bsver≤26 NoLighting fixture asserts `consumed == declared`
