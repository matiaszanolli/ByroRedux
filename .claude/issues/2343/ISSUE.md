# FO4-D4-2026-08-03-01: Bare BSVER literal survived the #1242 rename

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2343
**Labels**: low, nif-parser, documentation
**Source audit**: docs/audits/AUDIT_FO4_2026-08-03.md (Dimension 4 — NIF BSVER 130 / FO4 shader-band gating)

**Severity**: LOW
**Dimension**: 4 — NIF BSVER 130 / FO4 shader-band gating
**Location**: `crates/nif/src/blocks/shader.rs:1026`

## Description

`BSLightingShaderProperty::parse_fo4` gates the FO4-DLC subsurface block on the bare literal range `(130..=139).contains(&bsver)` instead of the named constants `(FALLOUT4..FO4_DLC_UPPER).contains(&bsver)` used by every sibling gate in the same file (`:979`, `:1406`, `:1412`, `:1430-1431`). Behaviorally identical today (`130..=139` ≡ `130..140`, since `FALLOUT4 == 130` and `FO4_DLC_UPPER == 140`), so purely a maintainability / rename-hygiene gap — but it is exactly the drift class `#1242` existed to eliminate.

## Evidence

```rust
// shader.rs:1026 — bare literals
let (subsurface_rolloff, rimlight_power, backlight_power) = if (130..=139).contains(&bsver)

// shader.rs:1412 — sibling gate, same band, named constants
if (crate::version::bsver::FALLOUT4..crate::version::bsver::FO4_DLC_UPPER).contains(&bsver)
```

Confirmed via `crates/nif/src/version.rs:381,405`: `FALLOUT4 = 130`, `FO4_DLC_UPPER = 140`.

## Impact

No runtime impact at HEAD. Latent: a future correction to `FO4_DLC_UPPER` would update the SSR/skin-tint/`env_map_scale` gates but not this one, mis-reading 4-12 bytes of `BSLightingShaderProperty` on the newly in/out-of-band BSVERs — the same failure class as `#1223`/`#1552`.

## Related

`#1242` (the rename this site was missed by), `#1223`, `#1552`, `#1901`, `#2281` (same drift class, different sites: `version.rs`/`sequence.rs`)

## Suggested Fix

Replace with `(crate::version::bsver::FALLOUT4..crate::version::bsver::FO4_DLC_UPPER).contains(&bsver)`. One-line, no behavior change; existing `shader_tests/fo4.rs` band coverage already exercises it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other bare-literal BSVER gates across the NIF crate — see also `#2281` for `version.rs`/`sequence.rs` sites)
- [ ] **TESTS**: A regression test pins this specific fix (existing `shader_tests/fo4.rs` band coverage should already exercise the corrected range; confirm it fails if the range regresses)
