# REN-2026-07-02-L01: DBG_BITS test catalog covers only 13 of 17 DBG_* constants

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (shader-constant lockstep)
- **Location**: `crates/renderer/src/shader_constants.rs` :: `DBG_BITS` array (`shader_constants.rs:31-45`); constants declared in `crates/renderer/src/shader_constants_data.rs`; hand-written emits in `crates/renderer/build.rs`
- **Source report**: `docs/audits/AUDIT_RENDERER_2026-07-02.md` (originally raised as REN-2026-07-01-L01 in `docs/audits/AUDIT_RENDERER_2026-07-01.md`, confirmed unchanged in both sessions; this is the first publish run to file it)

## Description

`shader_constants_data.rs` declares **17** `pub const DBG_*` constants, but the
`DBG_BITS` catalog array in `shader_constants.rs` — the shared iteration source
for both the header value-pin test (`generated_header_contains_all_defines`)
and the shader no-redeclare guard (`triangle_frag_dbg_bits_not_redeclared`) —
still enumerates only **13** entries, `DBG_BYPASS_POM` (0x1) through
`DBG_LEGACY_LIGHT_ATTEN` (0x1000).

The four newest bits — `DBG_DISABLE_MULTISCATTER` (0x2000),
`DBG_DISABLE_ATROUS` (0x4000), `DBG_DISABLE_RESTIR` (0x8000),
`DBG_DISABLE_SPATIAL` (0x10000) — are emitted into the generated GLSL header
via separate hand-written `writeln!` calls in `build.rs`, bypassing the
catalog, and carry neither a value-pin test nor a no-redeclare guard.

## Evidence

```
$ grep -c "^pub const DBG_" crates/renderer/src/shader_constants_data.rs
17
```

`shader_constants.rs:31-45` — the `DBG_BITS` array body has 13 entries
(`DBG_BYPASS_POM` through `DBG_LEGACY_LIGHT_ATTEN`); the four newest bits are
absent from this list.

## Impact

Latent, not live. No shader currently shadow-redeclares the four uncovered
bits, so the generated header values are currently correct. Risk is a future
shader or `build.rs` edit on those four bits shipping undetected past
`cargo test`.

## Suggested Fix

Add the four missing entries to `DBG_BITS` and route their header emit
through the catalog loop instead of the hand-written `writeln!`s in
`build.rs`. Add a `dbg_bits_catalog_covers_every_dbg_constant` test asserting
`DBG_BITS.len()` equals the `^pub const DBG_` count in
`shader_constants_data.rs`.

## Related

- #1482 (original catalog fix)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix (`dbg_bits_catalog_covers_every_dbg_constant`)
