# OBL-D3-NEW-05: Oblivion MGEF lookup keyed by FormID; engine uses 4-char EDID codes via EFID

**Labels**: bug, medium, legacy-compat

**Audit**: `docs/audits/AUDIT_OBLIVION_2026-05-11_DIM3.md`
**Severity**: MEDIUM (deferred — no consumer today)
**Domain**: ESM / TES4 magic system

## Premise

`parse_mgef` reads `EDID`, `FULL`, `DESC`, and `DATA` byte 0..4 as `effect_flags`. The consumer indexes `EsmIndex.magic_effects: HashMap<u32, MgefRecord>` keyed by FormID.

[crates/plugin/src/esm/records/misc.rs:709-726](../../crates/plugin/src/esm/records/misc.rs#L709-L726)

## Gap

In Oblivion, MGEF EDIDs are fixed 4-character codes (\"FIDG\", \"DGFA\", \"REDG\", \"DRSP\", …) and **the engine looks up effects by these literal 4-byte codes, not by FormID**. SPEL/ENCH/ALCH records cross-reference effects via `EFID` whose first 4 bytes ARE the magic-effect 4-byte code, not a u32 FormID.

A FormID-keyed map cannot resolve EFID lookups on Oblivion content.

## Impact

Zero today (no spell-casting / enchant / alchemy runtime consumes the map yet). The moment such a runtime lands and reads SPEL→EFID→MGEF, every Oblivion spell will silently no-op.

## Suggested Fix

Build a secondary map populated when `game == GameKind::Oblivion`:

```rust
pub magic_effects_by_code: HashMap<[u8; 4], u32>,  // 4-char code → MGEF FormID
```

Populate during `parse_mgef` by treating the EDID's first 4 ASCII bytes as the key (if the EDID is exactly 4 chars + null terminator). Alternative: key the MGEF map directly on `[u8; 4]` for Oblivion and let the consumer handle the variant lookup.

Defer until ENCH/SPEL parsing reads `EFID` and a consumer needs resolution.

## Completeness Checks

- [ ] **SIBLING**: Verify FO3/FNV/Skyrim MGEF FormID-keyed lookup still works unchanged.
- [ ] **TESTS**: Once landed, regression test asserts `magic_effects_by_code[*b\"FIDG\"]` resolves to the \"Feather\" MGEF FormID in `Oblivion.esm`.
- [ ] **GATE**: Hold this open until SPEL/ENCH/ALCH consumers materialize; revisit then.
