## Finding NIF-NEW-05 — NIF Audit 2026-06-13

- **Severity**: MEDIUM
- **Dimension**: Block Parsing (Starfield layout; root cause not yet isolated — needs a follow-up byte-audit)
- **Game Affected**: Starfield (bsver 172).
- **Location**: `crates/nif/src/blocks/shader.rs` (`BSEffectShaderProperty::parse`), `crates/nif/src/blocks/extra_data.rs` (`BsClothExtraData::parse`), `crates/nif/src/blocks/light.rs` (`NiPointLight::parse`) — all dispatched (`blocks/mod.rs:269,539,644`), all failing mid-parse on the Starfield field layout.
- **Status**: NEW — validated CONFIRMED at HEAD `8d191d7d` against checked-in golden baselines.

## Description

The full-data-dir baseline records **1,036 `NiUnknown` on the Starfield corpus (0.13%)** — the single largest block of parse failures in the entire corpus. Three dispatched types whose parsers don't match the Starfield (bsver-172) layout fail and fall to `NiUnknown`:

| Block type | parsed / unknown |
|------------|------------------|
| BSEffectShaderProperty | 83 / **665** |
| BSClothExtraData | 0 / **298** |
| NiPointLight | 8 / **18** |

## Evidence (validated — checked-in golden baselines)

- `crates/nif/tests/data/block_coverage_baselines/starfield.tsv`: `total_blocks 770322`, `unknown_blocks 1036`.
- `crates/nif/tests/data/per_block_baselines/starfield.tsv`: `BSClothExtraData 0 298`, `BSEffectShaderProperty 83 665`, `NiPointLight 8 18`.
- `crates/nif/tests/block_coverage_baselines.rs::unknown_ceiling_starfield` pins the ceiling; Starfield game data is installed locally (`/mnt/data/SteamLibrary/steamapps/common/Starfield`).
- All three types ARE dispatched — this is parser-correctness, not a coverage gap.

## Impact

Starfield effect-shader surfaces (the bulk of the 665) render without their authored material; cloth and point-light data dropped. Bounded because Starfield isn't a primary compat target yet, but it's the highest-count correctness gap in the corpus, and the count is a hard floor in the regression baseline.

## Suggested Fix

Byte-audit `BSEffectShaderProperty::parse` for the bsver-172 band first (highest count), then `BSClothExtraData` and the Starfield `NiPointLight` layout, against nif.xml. Likely a missing FO76→Starfield field-band split mirroring the existing `parse_fo76_plus` pattern in `shader.rs` (the BSLightingShaderProperty dispatcher already does this; BSEffectShaderProperty likely needs the same). As each is fixed, tighten the Starfield baseline ceiling in `block_coverage_baselines.rs` downward so it can't silently regrow.

## Completeness Checks
- [ ] **UNSAFE**: N/A expected
- [ ] **SIBLING**: Mirror the bsver-172 band split across the effect/cloth/light parsers; check whether other Starfield-only blocks share the same missing-band pattern
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **CANONICAL-BOUNDARY**: `BSEffectShaderProperty` feeds `translate_material` downstream — keep the bsver-172 field decisions at the parse→`ImportedMesh` layer; do NOT let Starfield-specific material logic leak into `translate_material`/`resolve_pbr` or the renderer. See `/audit-nifal`.
- [ ] **TESTS**: Tighten the Starfield `block_coverage_baselines.rs` ceiling as each type is fixed; add a bsver-172 fixture per type

---
Source: `docs/audits/AUDIT_NIF_2026-06-13.md` · Filed by `/audit-publish` · Surfaced empirically by `block_coverage_baselines.rs`; root cause not yet isolated
