# NIF-D5-2026-09-04-02: Starfield BSLightingShaderProperty runs the FO76 skin/hair-tint tail after the parser's own comment says the block has ended

URL: https://github.com/matiaszanolli/ByroRedux/issues/4149
Labels: bug, nif-parser, high, nif, game:starfield

---

**Severity**: HIGH
**Dimension**: 5 — Collision/Shader Parsing
**Game Affected**: Starfield
**Location**: `crates/nif/src/blocks/shader.rs:1249-1254` (the "ends after the luminance quad" comment), `:1300` (unconditional call), `:1595-1630` (`parse_shader_type_data_fo76`)
**Status**: NEW (previously identified in the unpublished `AUDIT_NIF_2026-09-04.md`, re-verified against current code this session, still present; no matching GitHub issue)

**Description**: `parse_fo76_plus` gates its FO76-only translucency/texture-array tail on `bsver < STARFIELD` with an explicit comment that Starfield's block "ends after the luminance quad... reading these over-ran the block into the NIF footer." Three lines later, `parse_shader_type_data_fo76` is called unconditionally on `bsver`, and for `shader_type ∈ {4 (Skin Tint), 5 (Hair Tint)}` reads an additional 12-16 bytes that the adjacent comment says Starfield doesn't have. No test fixture exercises Starfield with `shader_type == 4` or `5` — every Starfield fixture resolves to `shader_type == 0`.

**Evidence**: Confirmed by direct code reading — the translucency/texture-array tail is correctly gated `if bsver < crate::version::bsver::STARFIELD { ... }`, but `let shader_type_data = parse_shader_type_data_fo76(stream, shader_type)?;` immediately after is unconditional; `parse_shader_type_data_fo76`'s `4 =>`/`5 =>` arms read 16/12 extra bytes respectively with no `bsver` gate.

**Impact**: If real Starfield content authors a skin/hair-material `BSLightingShaderProperty` (common on character NIFs) without this tail on the wire, the parser over-reads into the next block's data, desyncing the entire remaining block stream — a cascading misparse, not a locally-contained one.

**Related**: `#2616`/`#2622` (the precedent that nif.xml's Starfield scoping needs corpus verification per-field).

**Suggested Fix**: Corpus-verify against real Starfield character/hair NIFs with `shader_type ∈ {4,5}`. If the tail is genuinely absent on Starfield, gate the call on `bsver < STARFIELD` like the adjacent translucency/texture-array fields.

## Completeness Checks
- [ ] **SIBLING**: Check whether any other `parse_shader_type_data_*` variant has the same unconditional-call-after-gated-comment shape
- [ ] **TESTS**: A Starfield fixture with `shader_type ∈ {4,5}` pins the corrected gate (or confirms the tail is genuinely present, in which case the parser is already correct and this closes as a false alarm)

