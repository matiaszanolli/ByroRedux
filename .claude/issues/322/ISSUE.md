# H1: NiPSysData over-reads corrupt particle system block bytes

## Finding: NIF-D3-02 (HIGH)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md`
**Dimension**: Stream Position
**Games Affected**: Fallout NV (primary), Fallout 3, likely Skyrim LE/SE
**Location**: `crates/nif/src/blocks/particle.rs` (NiPSysData parser)

## Description

`NiPSysData` is by far the largest source of per-block byte-count mismatches in the FNV corpus. **559 of 572 total over-reads** come from this single block type, plus 692 parse failures. Over 1,252 broken parses (~7.3% of NiPSysData population).

Observed in `/tmp/audit/nif/fnv_stderr.log`:
- `expected 109 bytes, consumed 267`
- `expected 173 bytes, consumed 1745`
- `expected 301 bytes, consumed 14133`
- `expected 45 bytes, consumed 271`

Because consumed ≫ declared, the parser walks deep into the next block(s) before returning `Ok`. The parse-loop guardrail at `crates/nif/src/lib.rs:179-230` then snaps the cursor back to `start_pos + block_size`, preventing downstream corruption — but the NiPSysData block itself contains garbage. Any consumer (particle emitter, animation) will read junk vertex/key data.

## Impact

Particle effects on FNV/FO3 emit incorrect geometry, colours, sizes, or lifetimes. Some NiPSysData blocks become `NiUnknown` placeholders and produce no particles at all. Visible regression on weapon muzzle flashes, explosions, ambient dust, light bloom sprites.

## Suggested Fix

1. Re-audit the NiPSysData parser against Gamebryo 2.3 source (`CoreLibs/NiParticle/NiPSysData.cpp`) and `docs/legacy/nif.xml`. Watch for nif.xml divergence (precedent: NiTexturingProperty / #149).
2. Focus on conditional subtexture offsets, rotation-angle/rotation-axis arrays, and the `data_size`/`data_size_2` split.
3. Promote `consumed > size` from WARN to ERROR on FO3+ files in `crates/nif/examples/nif_stats` so regressions surface.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
