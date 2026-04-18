# NIF-COV-02: Four Ni*LightController variants missing from dispatch (animated point/spot lights)

**Issue**: #433 — https://github.com/matiaszanolli/ByroRedux/issues/433
**Labels**: bug, animation, nif-parser, high

---

## Finding

Four `Ni*LightController` subclasses are not in the `parse_block` dispatch table at `crates/nif/src/blocks/mod.rs`:

| Type | nif.xml line | Inherits | Versions |
|---|---|---|---|
| `NiLightColorController` | 3750 | NiPoint3InterpController | All games (Oblivion+) |
| `NiLightDimmerController` | 3776 | NiFloatInterpController | All games (Oblivion+) |
| `NiLightIntensityController` | 5025 | NiFloatInterpController | FO3+ |
| `NiLightRadiusController` | 8444 | NiFloatInterpController | FO4+ |

## Impact

Every animated point/spot light in Bethesda content uses one of these:
- Lantern flicker
- Campfire pulse
- Torch flicker
- Magic spell glow (sustained effects)
- Terminal screen bloom emitters (FO4)
- Plasma weapon effects

Currently they parse as NiUnknown → controller ref graph dead end → light stays at static authored values instead of animating.

## Games affected

Color + Dimmer: all games (Oblivion onward). Intensity: FO3+. Radius: FO4+.

## Fix

All four have zero extra fields beyond their `NiSingleInterpController` base. Add their type names to the dispatch group at `crates/nif/src/blocks/mod.rs:361-371` (same pattern `NiVisController` / `NiAlphaController` already use):

```rust
"NiVisController"
| "NiAlphaController"
| "NiLightColorController"       // NEW
| "NiLightDimmerController"      // NEW
| "NiLightIntensityController"   // NEW
| "NiLightRadiusController"      // NEW
=> {
    let block = NiSingleInterpController::parse(stream)?;
    Ok(Box::new(block))
}
```

~4 LOC total. Block-size recovery catches any variant-specific tail bytes that don't match the base layout.

**Note**: These also depend on NIF-COV-01 (NiColorInterpolator) for NiLightColorController's interpolator chain to resolve.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check the `NiSingleInterpController::parse` base reads only the shared fields (NiObjectNET + target + flags + frequency + phase + start/stop time + interpolator_ref). If any of these four controllers adds a tail field, block-size recovery will elide it silently — not ideal but not broken.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic NIF with a NiLightColorController pointing at a NiColorInterpolator (once NIF-COV-01 lands). Assert controller parses and interpolator ref resolves.

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 5 COV-02.
