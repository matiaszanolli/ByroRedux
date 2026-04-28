# #747: SF-D1-DISPATCH: BSShaderType155 dispatch gated on `bsver == 155` — Starfield falls through to legacy FO4 dispatch

URL: https://github.com/matiaszanolli/ByroRedux/issues/747
Labels: bug, nif-parser, high, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 1, SF-D1-03)
**Severity**: HIGH
**Status**: NEW (related to closed #109 but logically separate from the value-gates regression in #746)

## Description

`fo76_shader_type` is read at `shader.rs:799` only when `bsver == 155`, and `shader_type` selects between `legacy_shader_type` and `fo76_shader_type` on the same equality at `:827`. Then `shader_type_data` resolves through `parse_shader_type_data_fo4` at `:995` instead of `parse_shader_type_data_fo76` because `bsver == 155` is false on Starfield (bsver = 172).

Starfield uses the same BSShaderType155 numeric mapping as FO76 (type 4 = skin tint Color4, type 5 = hair tint Color3) per nif.xml. Result: skin-tint dispatch mis-routed to eye-tint enum + 12-byte under-read on tinted blocks.

## Evidence

```rust
// shader.rs:799
let fo76_shader_type = if bsver == 155 { stream.read_u32_le()? } else { 0 };

// shader.rs:827
let shader_type = if bsver == 155 {
    fo76_shader_type
} else {
    legacy_shader_type
};

// shader.rs:990
let shader_type_data = if bsver == 155 {
    parse_shader_type_data_fo76(stream, shader_type)?
} else if bsver < 130 {
    parse_shader_type_data(stream, shader_type)?
} else {
    parse_shader_type_data_fo4(stream, shader_type, bsver)?
};
```

## Impact

Starfield character/face meshes (which carry skin-tint and hair-tint shader-type variants) silently route through the wrong dispatch. Any block where `parse_shader_type_data_fo4` reads fewer bytes than `parse_shader_type_data_fo76` would have read leaves the stream offset short — `block_size` skip masks the under-read, but the captured material data is wrong (or zeroed).

## Suggested Fix

All three sites become `if bsver >= 155`. Then verify `parse_shader_type_data_fo76` is BSVER-agnostic for the values it reads — if it has its own internal `== 155` gates, those need the same widening. Before/after parse counts on `Starfield - Meshes01.ba2` should show a drop in shader-type-dispatched truncation.

## Completeness Checks

- [ ] **SIBLING**: After widening these 3 sites, grep `parse_shader_type_data_fo76` for any internal `bsver == 155` gates.
- [ ] **TESTS**: Add regression test on a Starfield character-skin or hair shader block; assert `shader_type_data` resolves to the BSShaderType155 enum variant.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a (parser-only change).

## Related

- #746 (SF-D1 value gates)
- Closed #109 (FO76/Starfield shader property mis-parse, original fix)
