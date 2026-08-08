# OBL-D1-04: Two latent TexDesc version gaps, plus a PS2 L/K divergence between the two TexDesc readers

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2565
**Finding ID**: OBL-D1-04

**Severity**: LOW
**Dimension**: NIF Version Handling
**Location**: `crates/nif/src/blocks/properties.rs:349-381,401-462`
**Status**: NEW

## Description
`read_tex_desc`'s `else` branch over-reads 12 bytes for the unexercised `20.1.0.0`–`20.1.0.2` band; `Unknown Short 1 (until=4.1.0.12)` is never read; the shader-map trailer's second `TexDesc` reader omits the PS2 L/K shorts the primary reader correctly reads at `<= 10.4.0.1`. Fully latent on the live 11-BSA vanilla corpus (no file in the affected bands carries a `NiTexturingProperty`). Risk confined to NifSkope-exported Oblivion mod content.

## Evidence
Confirmed directly at `properties.rs:349-365` — version-gated reads with the documented bands.

## Impact
Latent — no vanilla content in the affected version bands. Risk confined to NifSkope-exported mod content.

## Suggested Fix
Make the `TexDesc` version branch explicit rather than `else`, add the missing `Unknown Short 1` read, factor a shared `TexDesc`-body helper so the two readers can't drift again.

## Completeness Checks
- [ ] **TESTS**: A synthetic byte-stream test for the `20.1.0.0`-`20.1.0.2` band and the PS2 L/K shorts pins both readers in lockstep
- [ ] **SIBLING**: Shared helper ensures the primary and shader-map-trailer `TexDesc` readers can't diverge again
