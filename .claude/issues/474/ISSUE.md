# Issue #474

FNV-1-M1: NIF parser coverage gaps hidden by block_sizes recovery (bhkSimpleShapePhantom, TallGrassShaderProperty, WaterShaderProperty)

---

## Severity: Medium

**Location**: `crates/nif/src/blocks/mod.rs` (recovery dispatch); specific parsers under `crates/nif/src/blocks/`

## Problem

`nif_stats` on FNV `Fallout - Meshes.bsa` logs per-block size mismatches for three parser stubs:

- `bhkSimpleShapePhantom` — expected 100, consumed 92 (8-byte trailer unread)
- `TallGrassShaderProperty` — expected 75/83, consumed 54 (21/29 bytes short)
- `WaterShaderProperty` — expected 30, consumed 54 (over-reads by 24)

`lib.rs:202-213` `block_sizes` self-heal absorbs the drift — parse-rate stays at 100% — but the underlying parsers are incomplete. Field values land zero-initialized.

## Impact

- **Gameplay-visible**: WaterShaderProperty drives water surface rendering; wrong values affect water tint, refraction, flow UV.
- **Latent risk**: if #393 ever tightens stream recovery (removes or gates the block_sizes fallback), these hard-fail on FNV.

## Related

The pattern matches FO3-5-02 (`TileShaderProperty` aliased to PPLighting — #455). Same class of issue: dispatch exists, trailer unread.

## Fix

Three discrete parser completions:

1. **bhkSimpleShapePhantom**: add Havok trailer (shape ref + transform matrix? — verify against nif.xml)
2. **TallGrassShaderProperty**: extend from PPLighting with grass-specific distance falloff + density fields
3. **WaterShaderProperty**: re-read the nif.xml definition; current over-read suggests we're reading fields that don't exist for this version, or the declared `block_size` is wrong

Reference nif.xml v20.2.0.7 + Gamebryo 2.3 source.

## Completeness Checks

- [ ] **TESTS**: Canonical probe for each shader type parses with zero `warn!` lines and fully-consumed bytes
- [ ] **SIBLING**: Audit the full `blocks/mod.rs` dispatch table for other size-mismatch warnings on FNV sweep
- [ ] **DOCS**: nif.xml reference comment at each fixed parser
- [ ] **RENDER**: `WaterShaderProperty` values need to be plumbed to renderer — separate follow-up if downstream consumer missing

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-1-M1)
