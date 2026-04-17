# FO4-D5-H2: NiParticleSystem parser over-reads by up to 75× — 1,345 occurrences corrupt downstream stream

**Issue**: #407 — https://github.com/matiaszanolli/ByroRedux/issues/407
**Labels**: bug, nif-parser, high

---

## Finding

The FO4 real-data sweep logged 1,345 `block_size` mismatches on `NiParticleSystem`:

```
Block N 'NiParticleSystem' (size 165, offset 2927, consumed 12495)
Block N 'NiParticleSystem' (size 161, consumed 10610)
```

The parser consumed **12,495 bytes for a 165-byte block** — 75× the advertised size — before the `block_size` recovery forced it back to the right offset.

## Why this is dangerous

While the over-consuming parse is running, it reads bytes belonging to the next N blocks. Any interior `stream.read_u32_le()` etc. in those bytes can fabricate arbitrary lengths. The `block_size` gate in `lib.rs` only fires **after** `parse_block` returns, so an interior `Vec::with_capacity(n)` based on random downstream bytes can trigger an allocation before recovery happens.

**This is the exact vector that converts a malformed NIF into the OOM class tracked in #388 and FO4-D5-H3.**

## Impact

- `NiParticleSystem` is replaced with `NiUnknown` in the final scene (all 10,648 `NiUnknown` entries in the main-archive histogram trace to this family). Particle FX silently dropped on every FO4 interior.
- Related: #383 (FNV NiPSys sub-blocks under-read by 4-16 bytes). The FNV under-read and FO4 over-read share a common root: particle-system parsers disagree with the FO4/FNV wire formats.

## Affected parsers (from Dim 5 M-2, top over-readers)

| Block | Count in Meshes.ba2 |
|---|---|
| BSEffectShaderPropertyFloatController | 5,264 |
| NiParticleSystem | 1,345 |
| NiPSysRotationModifier | 836 |
| NiPSysBoxEmitter | 629 |
| NiPSysMeshEmitter | 405 |
| BSClothExtraData | 309 |
| NiPSysSphereEmitter | 177 |
| NiPSysCylinderEmitter | 134 |

Over 8,800 non-BSLSP `block_size` mismatches in the main archive alone.

## Fix

1. Audit each parser in `crates/nif/src/blocks/particle.rs` and `legacy_particle.rs` against nif.xml's FO4 (BSVER 130) definitions. The 4-byte and 4-variant deltas suggest missing trailing fields per BSVER.
2. Route every `read_u32_le` inside these parsers through a remaining-bytes sanity check — don't trust the count.
3. Hard-cap allocations via the helper introduced in FO4-D5-H3 (same as #388 pattern) so that even if the parser over-reads, `Vec::with_capacity(junk_count)` can't OOM the process.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Paired with #383 (FNV under-read). Common root — nif.xml wire-layout vs FO4/FNV.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a fuzz target that feeds random u32 counts to each NiPSys parser; assert none can allocate > 256 MB. Regression test: synthetic NIF with one NiParticleSystem round-trips with `block_size` matching `consumed`.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 5 H-2 + M-2.
