# #4257 — OB-D1-01: inline block-type-name inference keys off block_types.is_empty() rather than the version threshold

**Severity**: LOW
**Dimension**: Dimension 1 — NIF Version Handling
**Location**: `crates/nif/src/lib.rs:404`
**Status**: NEW

## Description
Inline block-type-name inference (pre-Gamebryo NetImmerse files, NIF v < 5.0.0.1) decides whether to read inline sized-string type names by checking `header.block_types.is_empty() && header.num_blocks > 0`, rather than checking the version threshold directly (the constant exists at `crates/nif/src/header.rs:231`).

## Evidence
`crates/nif/src/lib.rs:404`: `let inline_type_names = header.block_types.is_empty() && header.num_blocks > 0;` — a diagnostic/dispatch decision derived from an incidental table-emptiness signal instead of the version gate that actually determines this format difference.

## Impact
Unreachable on any shipping content today (every version-appropriate header either has a populated block-type table or is legitimately pre-5.0.0.1). Purely a diagnostic-quality nit: on hand-corrupted or fuzzed input where the table could be spuriously empty for an unrelated reason, this would misclassify the block layout.

## Suggested Fix
Key `inline_type_names` off the same version constant `header.rs:231` uses, rather than off `block_types.is_empty()`, so the decision is not incidentally correct.

## Completeness Checks
- [x] **SIBLING**: Same pattern checked in related files
- [x] **TESTS**: A regression test pins this specific fix

**Disposition**: Extracted the decision into a standalone `uses_inline_block_type_names(version, num_blocks)` pure function keyed directly on `NifVersion::V5_0_0_1`, so it's both correct and directly unit-testable. 3 new tests: pre-Gamebryo + blocks → true; >= 5.0.0.1 + an (adversarial) empty table → false (the exact case the old check got wrong); zero blocks → false regardless of version.

---

# #4262 — OB-D4-03: gamebryo_to_vk_blend_factor's out-of-range fallback applies SRC_ALPHA to the destination factor too, where the engine default is INV_SRC_ALPHA

**Severity**: LOW
**Dimension**: Dimension 4 — Rendering Path for Oblivion Shaders
**Location**: `crates/renderer/src/vulkan/pipeline.rs:214-227 (gamebryo_to_vk_blend_factor), call sites at lines 795-796`
**Status**: NEW

## Description
`gamebryo_to_vk_blend_factor` is a single shared function used for both the source and destination blend-factor lookups. Its documented out-of-range fallback is `SRC_ALPHA`, correct for the source factor but not for the destination factor, whose engine default is `INV_SRC_ALPHA`.

## Evidence
`gamebryo_to_vk_blend_factor(v: u8)` had a single `_ => vk::BlendFactor::SRC_ALPHA` fallback arm shared by both call sites; `NiAlphaProperty` storage is a nibble (max parsed value 15), so an out-of-range value (11-15) is reachable only on corrupted/fuzzed input.

## Impact
Defensive-path only. If ever hit on the destination side, the resulting blend would use `SRC_ALPHA` instead of the engine's actual default `INV_SRC_ALPHA`.

## Suggested Fix
Split the fallback per call site (or add a parameter) so the destination-factor call falls back to `INV_SRC_ALPHA`.

## Completeness Checks
- [x] **SIBLING**: Same pattern checked in related files
- [x] **TESTS**: A regression test pins this specific fix

**Disposition**: Added a `default: vk::BlendFactor` parameter to `gamebryo_to_vk_blend_factor`; the production call site now passes `SRC_ALPHA` for `src` and `ONE_MINUS_SRC_ALPHA` for `dst`. Updated all call sites (production + the existing `gamebryo_to_vk_blend_factor_covers_all_11_values` / `coverage_alpha_lane_tracks_the_authored_destination_factor` tests in `pipeline.rs`, plus `byroredux/src/asset_provider/tests/bgsm_merge.rs`). Extended the regression test to assert the out-of-range fallback now differs per slot. Verified by temporarily reverting the fallback arm to hardcode `SRC_ALPHA` (ignoring `default`) — confirmed the new assertion fails — then restored.

---

# #4263 — OB-D5-01: Oblivion's APPLY_HILIGHT2 arm fabricates detached literal copies of the canonical parallax defaults instead of leaving them unset

**Severity**: LOW
**Dimension**: Dimension 5 — NIFAL Canonical Material Translation for Oblivion
**Location**: `crates/nif/src/import/material/legacy_properties.rs:303-308`
**Status**: NEW

## Description
Oblivion's `APPLY_HILIGHT2` arm sets `info.parallax_max_passes = Some(4.0)` and `info.parallax_height_scale = Some(0.04)` as detached literal copies of the canonical `DEFAULT_PARALLAX_MAX_PASSES`/`DEFAULT_PARALLAX_HEIGHT_SCALE` constants, converting "unauthored" into "authored" at the raw import tier — precisely the pattern #3073 was introduced to prevent.

## Evidence
`crates/nif/src/import/material/legacy_properties.rs:303-308`: literal duplicates of the canonical defaults, not a reference to the constants.

## Impact
Numerically inert today (the values match the canonical defaults exactly). Becomes a live per-game divergence the moment either canonical default is retuned, since this Oblivion-specific literal would silently stop matching the (now-changed) canonical default.

## Related
Adjacent to #3073 (introduced the canonical constants this duplicates) and #3596 (made this arm reachable).

## Suggested Fix
Reference `DEFAULT_PARALLAX_MAX_PASSES`/`DEFAULT_PARALLAX_HEIGHT_SCALE` directly here instead of duplicating the literal values.

## Completeness Checks
- [x] **CANONICAL-BOUNDARY**: N/A — this is a NIF-import-tier literal, not a `translate_material`/`resolve_pbr` change.
- [x] **SIBLING**: Same pattern checked in related files
- [x] **TESTS**: A regression test pins this specific fix

**Disposition**: Replaced both literal duplicates with direct references to `byroredux_core::ecs::components::material::DEFAULT_PARALLAX_MAX_PASSES`/`DEFAULT_PARALLAX_HEIGHT_SCALE`. Found and fixed the identical SIBLING pattern one branch above (the `NiTexturingProperty`-only slot-7 parallax path, `legacy_properties.rs:236-249`) that the `APPLY_HILIGHT2` comment itself cites as sharing "the same pair." Updated the existing assertions in `crates/nif/src/import/tests/material_texture.rs` (both `APPLY_HILIGHT2` cases) and `crates/nif/src/import/material/texture_slot_3_4_5_tests.rs` (the slot-7 sibling case) to reference the canonical constants instead of the raw literals, so a future retuning of either default is caught here rather than silently diverging. Numerically inert (as the issue itself notes), so no behavior-revert verification applies — this is an anti-drift/hygiene fix, not a behavior fix.

---

# #4265 — OB-D6-01: nif_stats.rs --tsv histogram does not implement the byte-for-byte parity with tests/common::PerBlockHistogram its own module doc claims

**Severity**: LOW
**Dimension**: Dimension 6 — Real-Data Validation
**Location**: `crates/nif/examples/nif_stats.rs (module doc + block_histogram keying); crates/nif/tests/common/mod.rs:826-874 (PerBlockHistogram, wire-name keying)`
**Status**: NEW

## Description
`crates/nif/examples/nif_stats.rs`'s module doc claims its `--tsv` histogram "mirrors byte-for-byte" `tests/common::PerBlockHistogram` (#1883 / NIF-D3-001). In fact `nif_stats.rs`'s `block_histogram` keys on `block.block_type_name()` (the parser's resolved/dispatched type, which collapses alias families), while `PerBlockHistogram` keys on the header-advertised wire name (the #3326 fix) — the two are not the same keying scheme.

## Evidence
`crates/nif/tests/common/mod.rs:865-874` reads `header.wire_name` per block; `crates/nif/examples/nif_stats.rs:211-212` keys on `block.block_type_name().to_string()` instead.

## Impact
The actual regression-gate tests are unaffected. Impact is on manual/ad hoc use of `nif_stats --tsv` for cross-checking corpus histograms — a misleading result that could waste time chasing a phantom regression or dismiss a real one.

## Suggested Fix
Either implement the same wire-name keying in `nif_stats.rs`, or correct the module doc to state the actual keying and stop claiming byte-for-byte parity.

## Completeness Checks
- [x] **SIBLING**: Same pattern checked in related files
- [x] **TESTS**: A regression test pins this specific fix

**Disposition**: Took the doc-correction option: `nif_stats.rs`'s module doc itself already explains (two paragraphs earlier) that `NifScene` doesn't carry per-block header-advertised type names, which is exactly why byte-for-byte wire-name keying isn't implementable here without a larger structural change. Corrected the doc to explicitly retract the false parity claim and describe the actual divergence (dispatch-target census vs. wire-name census), with a warning not to diff the two tools' TSVs directly. No behavior change; example-only, `N/A` regression test per its own completeness checklist reasoning (there is no runtime behavior to pin, only doc accuracy).
