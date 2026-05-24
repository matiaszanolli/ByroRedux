# NIF-D1-NEW-01: NiTextureEffect reads NiDynamicEffect base unconditionally on FO4+ (BSVER ≥ 130)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1240

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 1, sole MEDIUM)
**Severity**: MEDIUM
**Dimension**: Block Parsing Correctness
**Game Affected**: FO4 / FO76 / Starfield (BSVER ≥ 130)

## Description

`NiTextureEffect::parse` at `crates/nif/src/blocks/texture.rs:723-737` reads the `NiDynamicEffect` base fields (`Switch State`, `Num Affected Nodes`, `Affected Nodes[]`) gated only on **NIF version** (`>= V10_1_0_106`, `>= V10_1_0_0`). nif.xml lines 3499 + 3504 carry an additional `vercond="#NI_BS_LT_FO4#"` clause: both fields are **absent** at BSVER ≥ 130 (FO4 / FO76 / Starfield), because FO4 reparented `NiTextureEffect`'s parent class `NiDynamicEffect` straight onto `NiAVObject` and dropped the dynamic-effect plumbing.

The sibling parser `NiLightBase::parse` at `crates/nif/src/blocks/light.rs:60-102` already honours this — see the `pre_fo4 = stream.bsver() < bsver::FALLOUT4` guard landed in #721. `NiTextureEffect` was missed in that fix.

The existing wire-layout comment at `texture.rs:635-637` even acknowledges the gate ("< BSVER 130") but the code doesn't honor it.

## Evidence

Current code (texture.rs:725-737):
```rust
let switch_state = if stream.version() >= NifVersion::V10_1_0_106 {
    stream.read_u8()? != 0
} else {
    true
};
let affected_nodes = if stream.version() >= NifVersion::V10_1_0_0 {
    let count = stream.read_u32_le()? as usize;
    stream.read_u32_array(count)?
} else { Vec::new() };
```

vs `light.rs:72-85` (correct):
```rust
let pre_fo4 = stream.bsver() < crate::version::bsver::FALLOUT4;
let switch_state = if pre_fo4 && stream.version() >= NifVersion::V10_1_0_106 { … };
let affected_nodes = if pre_fo4 && stream.version() >= NifVersion::V10_1_0_0 { … };
```

nif.xml line 3499: `<field name="Switch State" type="bool" default="true" since="10.1.0.106" vercond="#NI_BS_LT_FO4#">` — and line 3504 the matching Affected-Nodes gate.

## Impact

On any FO4+ NIF that ships a `NiTextureEffect`, the parser over-reads by 5 bytes (`switch_state` u8 + `num_affected_nodes` u32), then either consumes more bytes as `affected_nodes` body or — far more likely — reads a wildly large `count` and fails the `allocate_vec` budget. FO3+ ships `block_sizes`, so the outer loop recovers via skip and the block lands as a corrupted (but silently-recovered) record — no hard error in the parse-rate gate. The source texture ref + projection matrix never reach the importer; any projected env map / gobo / fog projector authored on FO4+ content silently drops.

Volume on shipping FO4 is unverified — projected env maps are largely replaced by `BSEffectShaderProperty` in the Skyrim+ era — but the asymmetry is a structural defence-in-depth gap and any modded / modder-resource content that uses this block on FO4+ will trip it.

## Suggested Fix

5-LOC change mirroring `NiLightBase`:

```rust
let pre_fo4 = stream.bsver() < crate::version::bsver::FALLOUT4;
let switch_state = if pre_fo4 && stream.version() >= NifVersion::V10_1_0_106 { … };
let affected_nodes = if pre_fo4 && stream.version() >= NifVersion::V10_1_0_0 { … };
```

Add a regression test at `texture.rs` constructing a BSVER=130 fixture with no `Switch State` / `Affected Nodes` bytes between the `NiAVObjectData` tail and the projection matrix; assert the parser reads the matrix without drift.

## Related

- #721 (CLOSED): NiLight parent fix that introduced the same gate. `NiTextureEffect` was a sibling miss in the same pass.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: re-walk the `NiDynamicEffect` inheritance chain in nif.xml and grep `crates/nif/src/blocks/` for every `NiDynamicEffect` subclass parser — any other block that's a `NiDynamicEffect` subclass needs the same gate. Candidates: anything inheriting `NiDynamicEffect` per nif.xml (NiTextureEffect, NiLight subclasses already fixed via #721, possibly others).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: BSVER=130 fixture as above, plus a BSVER=83 (Skyrim LE) fixture verifying the legacy path still reads both fields