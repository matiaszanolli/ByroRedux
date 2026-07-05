**Severity**: MEDIUM · **Dimension**: Stream Position · **Source**: `docs/audits/AUDIT_NIF_2026-07-05.md` (NIF-D1-001)
**Game Affected**: Starfield only (`NifVariant::Starfield`, bsver ≥ `bsver::STARFIELD` = 172). FO76/FO4 unaffected.
**Status**: NEW — the missed sibling of the resolved `BSLightingShaderProperty` tail, #1606.
**Location**: `crates/nif/src/blocks/shader.rs` (`BSEffectShaderProperty::parse`, dispatched in `crates/nif/src/blocks/mod.rs`)

## Description
After the FO76+ trailing fields, `BSEffectShaderProperty::parse` returns. Every retail-Starfield instance carries ~32 additional undocumented bytes the parser never consumes; `block_size` reconciliation seeks the stream to `start + block_size` so the file parses "clean". This is exactly the situation #1606 fixed for `BSLightingShaderProperty` — that parser was given `parse_with_size(stream, block_size)` + `read_starfield_tail(...)` capturing `block_size - consumed` opaque bytes. `BSEffectShaderProperty` was never given the treatment: its dispatch arm calls `parse(stream)` with no `block_size` argument, so it structurally cannot capture the tail.

## Evidence
- Dispatch asymmetry in `blocks/mod.rs`: `"BSLightingShaderProperty" => …parse_with_size(…block_size…)` vs `"BSEffectShaderProperty" => …parse(stream)` (no size, no tail hook).
- `read_starfield_tail` + the `starfield_tail: Vec<u8>` field exist ONLY on `BSLightingShaderProperty`.
- Drift histogram (`nif_stats --drift-histogram`): `BSEffectShaderProperty drift=+32` on 48/48 (LODMeshes.ba2) and 118/118 (MeshesPatch.ba2) instances — perfectly systematic, mirroring the byte-identical signature #1606 documented for the sibling.

## Impact
32 undocumented Starfield effect-shader bytes silently discarded; no cascade (reconciliation realigns), but the drop is invisible to `per_block_baselines.rs` (which gates parsed-vs-unknown, not drift). A future consumer of those bytes has nothing to read; the drift is only visible via the opt-in `--drift-histogram`.

## Suggested Fix
Mirror #1606 exactly — add `BSEffectShaderProperty::parse_with_size(stream, block_size)` that runs the existing body then `read_starfield_tail(stream, block_start, block_size, bsver)` into a new `starfield_tail: Vec<u8>` field; switch the `blocks/mod.rs` arm to `parse_with_size`. Add a `parse_bs_effect_starfield_captures_trailing_tail` regression test paralleling `parse_bs_lighting_starfield_captures_trailing_tail`.

## Related
#1606 (BLSP starfield_tail), #746 (BSEffect FO76 `>=` gate), #1510 (BSEffect Starfield empty-name stopcond).

## Completeness Checks
- [ ] **SIBLING**: Confirm no other Starfield shader/property block has the same size-unaware `parse(stream)` dispatch arm
- [ ] **TESTS**: A `parse_bs_effect_starfield_captures_trailing_tail` test pins `consumed == block_size` (the tail captured, not reconciled)
