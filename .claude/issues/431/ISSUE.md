# NIF-COV-01: NiColorInterpolator missing from dispatch — every animated emissive-color controller lands NiUnknown

**Issue**: #431 — https://github.com/matiaszanolli/ByroRedux/issues/431
**Labels**: bug, animation, nif-parser, high

---

## Finding

`NiColorInterpolator` (nif.xml:3236, `inherit="NiKeyBasedInterpolator"`) is not in the `parse_block` dispatch table at `crates/nif/src/blocks/mod.rs`.

It is the interpolator paired with the `BSEffectShaderPropertyColorController` / `BSLightingShaderPropertyColorController` wrappers that ARE dispatched (mod.rs:380-388). The controller parses correctly, but its `interpolator_ref` points at a NiColorInterpolator → lands on `NiUnknown` → ref graph dead end.

## Impact — extremely high frequency

Every NIF with an animated emissive / plasma / glow / muzzle-flash color uses this pair. Across the FNV/Skyrim/FO4 corpus:
- Every magic spell FX with color-over-lifetime animation
- Plasma weapon glow pulses
- Muzzle flashes
- Fade-in UI overlays
- Glow meshes (enchanted weapons, daedric sigils)

The `block_size` recovery keeps the parse loop alive (block is skipped), but the animation system has no way to sample the color keys. Currently the controller points to `None` and the animation silently runs with a default color.

## Games affected

FO3, FNV, Skyrim LE/SE, FO4, FO76, Starfield.

## Fix

Add to `crates/nif/src/blocks/interpolator.rs`:

```rust
/// NiColorInterpolator — RGBA key-based color animation.
/// Inherits NiKeyBasedInterpolator → NiInterpolator → NiObject.
/// Data ref points at a NiPosData-style block with RGBA keys.
#[derive(Debug)]
pub struct NiColorInterpolator {
    pub base: NiObjectNETData,
    pub value: [f32; 4],      // default RGBA when no keys
    pub data_ref: BlockRef,   // → NiColorData
}

impl NiColorInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiObjectNETData::parse(stream)?;
        let value = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let data_ref = BlockRef::parse(stream)?;
        Ok(Self { base, value, data_ref })
    }
}
```

Plus a dispatch arm in `crates/nif/src/blocks/mod.rs`:

```rust
"NiColorInterpolator" => {
    let block = NiColorInterpolator::parse(stream)?;
    Ok(Box::new(block))
}
```

~30 LOC total.

**Note**: `NiColorData` itself is part of #394's missing-parsers list (Oblivion). Both need to land together to fully unblock the color-key chain.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Also add `NiColorData` (tracked in #394 for Oblivion; this extends it cross-game). Cross-check with the `NiPoint3Interpolator` / `NiFloatInterpolator` parsers for struct layout consistency.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic NIF with a color-controller + NiColorInterpolator chain. Parse the whole chain, assert the interpolator's data_ref resolves and the ECS animation player reads RGB keys.

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 5 COV-01.
