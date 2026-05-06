# FO4-D4-NEW-01: TXST DODT decal-data sub-record silently dropped (207/382 vanilla TXSTs)

Labels: bug high legacy-compat 
State: OPEN

**From**: `docs/audits/AUDIT_FO4_2026-05-04_DIM4.md` (FO4 Dim 4)
**Severity**: HIGH
**Location**: `crates/plugin/src/esm/cell/support.rs:230-258`
**Status**: NEW + CONFIRMED (2026-05-04)

## Description

`parse_txst_group` matches TX00..TX07 + MNAM but has no `b"DODT"` arm.
`TextureSet` (`cell/mod.rs:514-530`) has no `decal_data` field at all.

```rust
// support.rs:240-257
match sub.sub_type.as_slice() {
    b"TX00" => set.diffuse = extract(&sub.data),
    // … TX01..TX07
    b"MNAM" => set.material_path = extract(&sub.data),
    _ => {}                  // ← DODT and DNAM land here
}
```

## Why it's wrong

DODT is the texture-set decal-data sub-record (per UESP / xEdit `wbDefinitionsFO4`):
fixed-layout struct with min/max width, depth, shininess, parallax scale + passes,
RGBA color, and flags. **207 of 382 vanilla `Fallout4.esm` TXST records ship a DODT
payload.** Bethesda's CK emits it on every decal-bearing TXST.

Without DODT, every decal mesh whose textures route through a TXST
(blood splatters, scorch marks, posters, graffiti, signs) loses its
width/depth/parallax/color authoring — the renderer's existing decal
pipeline (M28 / `RenderLayer::Decal`) falls back to defaults.

## Repro

Open any FO4 interior with surface decals (Vault 111 wall graffiti,
Diamond City posters). Spawned decals use default depth/width/color.
Counter-check: parse Fallout4.esm and grep TXST sub-record kinds —
DODT is 207 / 382.

## Fix sketch

Add `decal_data: Option<DecalData>` field to `TextureSet` and a `b"DODT"`
arm to `parse_txst_group`. The DODT payload is fixed-size (xEdit shows
~28 bytes: 4×f32 + 4×u8 RGBA + flags + parallax scale/passes). Mirror
the `LightData` / `AddonData` pattern — keep the parse defensive
(length-gate, drop on mismatch). Renderer-side decal RenderLayer
parameter wiring is M28-extension follow-up.

## Sibling

FO4-D4-NEW-02 (DNAM, same parser path) — should land together. Once
both land, FO4-D4-NEW-06 auto-resolves (3 DODT-only TXSTs no longer
default-equal).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: DNAM arm in same parser (FO4-D4-NEW-02). Decal handling on the renderer side reads `decal_data` once it lands.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic round-trip test (TXST with DODT payload → parses to expected `decal_data`). Live-data floor in the FO4 ESM parse-rate harness once FO4-D4-NEW-07 lands.