# NIF-D2-2026-09-11-01: carries_typed_shader_flags/carries_crc_shader_flags mis-gate BSSkyShaderProperty/BSWaterShaderProperty at the unattested bsver == 131 stream

URL: https://github.com/matiaszanolli/ByroRedux/issues/4151
Labels: bug, nif-parser, medium, nif

---

**Severity**: MEDIUM
**Dimension**: 2 — Version Gating
**Game Affected**: None in shipped content today (BSVER 131 is a documented unattested FO4-era dev stream) — but the affected block types are Skyrim+ `BSSkyShaderProperty`/`BSWaterShaderProperty`
**Location**: `crates/nif/src/version.rs:524-552`; `crates/nif/src/blocks/shader.rs:414-459,497-565`
**Status**: NEW (deepens the prior report's one-line NIF-D2-2026-09-04-04; confirmed against nif.xml directly this round)

**Description**: `parse_skyrim_shader_base` is shared by four block types (`BSLightingShaderProperty`, `BSEffectShaderProperty`, `BSSkyShaderProperty`, `BSWaterShaderProperty`), but the `bsver == 131` "neither typed nor CRC flags" gap band (`carries_typed_shader_flags`/`carries_crc_shader_flags`) is only correct for the first two — nif.xml splits their flags at `#NI_BS_LT_FO4#`/`#BS_FO4#` (union `bsver <= 130`). `BSSkyShaderProperty`/`BSWaterShaderProperty` instead use nif.xml's un-split `vercond="!#BS_GTE_132#"` (`bsver < 132`) — meaning both genuinely carry the typed flags at 131, which the shared reader skips.

**Evidence**: Confirmed `parse_skyrim_shader_base` (`shader.rs:414-459`) is the single shared head reader called by all four block types' parse functions, gated only via the two named predicates in `version.rs:524-552`.

**Impact**: An 8-byte under-read on any `BSSkyShaderProperty`/`BSWaterShaderProperty` at bsver 131, misaligning every field after it in the block. No observed exposure today (131 is unattested in any shipped corpus).

**Suggested Fix**: Give Sky/Water their own gate (`bsver < 132`, no gap band) instead of routing through the BLSP-derived predicate; add fixtures at bsver 131 for both types.

## Completeness Checks
- [ ] **SIBLING**: Verify no other shared-reader call site assumes the BLSP-derived gap band applies uniformly
- [ ] **TESTS**: Fixtures at bsver 131 for `BSSkyShaderProperty` and `BSWaterShaderProperty` pin the corrected gate

