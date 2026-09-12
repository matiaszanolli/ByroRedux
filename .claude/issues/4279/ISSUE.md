# SF-2026-09-11-D6-01: Own_Emit additive-blend promotion is typed-word-only, so it can never fire on any CRC-era BSEffectShaderProperty

**Issue**: #4279 — https://github.com/matiaszanolli/ByroRedux/issues/4279
**Labels**: medium,nif-parser,nif,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 6 — NIF Shader Blocks, BSVER 155+
**Location**: `crates/nif/src/import/material/dedicated_shader.rs:696-719 (Own_Emit additive-blend promotion)`
**Status**: NEW

## Description
The `Own_Emit` (SLSF1 bit 22) additive-blend promotion for `BSEffectShaderProperty` reads only the typed `shader.shader_flags_1` word (via `slsf1_bit` selecting the Skyrim or FO4 constant by `TextureSlotLayout::from_bsver`). CRC-era shader blocks (BSVER ≥ 132, which includes Starfield/FO76) carry their flags in a separate CRC-hashed flag array (see `crates/nif/src/shader_flags.rs::bs_shader_crc32`), not in the typed word this code reads — so `Own_Emit`'s additive-blend promotion (`src=ONE, dst=ONE` instead of default `SRC_ALPHA`/`INV_SRC_ALPHA`) can never fire on a CRC-era block, even when the CRC-hashed `EMIT_ENABLED` flag (the CRC-era equivalent) is set. This is the fifth flag on this block whose other four #890 already fixed to read correctly on this path; `Own_Emit` is still one-sided.

## Evidence
Verified during this audit against the newly-established (D6-02) complete 32-flag CRC table: `EMIT_ENABLED` (CRC `0x86DBD392`) is confirmed parsed but its "Read by import?" column is "no — see D6-01" — i.e. it is preserved on parse but never consulted by the additive-blend promotion logic, which only ever reads the typed-word bit.

## Impact
Zero live blast radius on vanilla Starfield (never observed in the 108,816-NIF census this audit cites — Starfield's own effect-shader authoring apparently doesn't rely on this bit), but reachable on FO76 and mod content that does author CRC-era effect shaders with `EMIT_ENABLED` set, where the additive-blend promotion (needed to correctly bloom high-emissive VFX like glows/energy effects) would silently not apply.

## Related
Adjacent to #890 (fixed the other four flags on this same block for the typed-word path).

## Suggested Fix
Add a CRC-flag read (via `bs_shader_crc32`'s `EMIT_ENABLED` entry) alongside the existing typed-word check, gated the same way the block already selects typed-word-vs-CRC-array per `bsver`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
