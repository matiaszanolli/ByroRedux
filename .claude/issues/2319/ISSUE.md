# FO3-D1-03: fo3nv_f1::OWN_EMIT is mislabeled — nif.xml bit 22 is Tree_Billboard, not Own_Emit; a test cements the wrong fact

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2319

**Severity**: MEDIUM
**Location**: `crates/nif/src/shader_flags.rs:34-37,331-339`; use site `crates/nif/src/import/material/dedicated_shader.rs:526-532`
**Status**: NEW

### Description
FO3/FNV `BSShaderFlags` bit 22 is `Tree_Billboard` per nif.xml; there is no `Own_Emit` bit in the FO3/FNV enum. The module's own purpose is pinning per-game bit semantics, yet it declares this constant as a "cross-game constant" and a passing test asserts the false claim.

Confirmed against current code: `crates/nif/src/shader_flags.rs:34-44` declares `pub mod fo3nv_f1 { pub const OWN_EMIT: u32 = 0x0040_0000; ... }` with a doc comment claiming "Same bit as `skyrim_slsf1` and `fo4_slsf1` — cross-game constant." The test at `:331-339` (`own_emit_bit_is_cross_game_constant`) asserts `fo3nv_f1::OWN_EMIT == skyrim_slsf1::OWN_EMIT` and `== fo4_slsf1::OWN_EMIT`, cementing the claim green.

### Impact
Latent — the only production read of this constant is inside a genuinely Skyrim+ code path (`apply_bs_effect_shader`), so no live miscompute today. But the wrong fact is protected by a green test — exactly the failure mode `shader_flags.rs` exists to prevent — and would silently additive-composite every FO3 tree/foliage billboard if ever reached from the FO3 module.

### Suggested Fix
Rename to `fo3nv_f1::TREE_BILLBOARD`, move the `Own_Emit` alias to `skyrim_slsf1::OWN_EMIT`, rewrite the test.

### Related
#414, #592

## Completeness Checks
- [ ] **SIBLING**: Check other `fo3nv_f1` constants for the same "cross-game constant" mislabeling pattern
- [ ] **TESTS**: Existing `own_emit_bit_is_cross_game_constant` test rewritten to assert the correct per-game bit semantics instead of the false claim
