# NIF-D5-06: FO4 / FO76 / Starfield NiPointLight parser errors — 681 light blocks demoted

URL: https://github.com/matiaszanolli/ByroRedux/issues/721
Labels: bug, nif-parser, medium

---

## Severity: MEDIUM

## Game Affected
FO4, FO76, Starfield

## Location
- `crates/nif/src/blocks/light.rs` `NiPointLight::parse()` (called from `crates/nif/src/blocks/mod.rs:128`)
- `crates/nif/src/blocks/light.rs` `NiSpotLight::parse()` (called from `mod.rs:129`)

## Description
`NiPointLight` and `NiSpotLight` ARE dispatched, but parse fails on Bethesda 20.2.0.7+ NIFs (bsver ≥ 130) and demotes via `block_size`-driven recovery. This is a **parser-error** (not a missing dispatch arm) — the NiUnknown bucket conflates dispatch-fallback with err-recovery.

`NiPointLight` was first audited in #156; the FO3/FNV/SE corpus parses cleanly but FO4+ does not.

## Evidence
2026-04-26 corpus sweep:
- `Fallout4 - Meshes.ba2` — 100 `NiPointLight`
- `SeventySix - Meshes.ba2` — 562 `NiPointLight` + 1 `NiSpotLight`
- `Starfield - Meshes01.ba2` — 18 `NiPointLight`
- Total: 681

## Impact
All embedded NIF lights on FO4+ go dark — these are typically per-mesh accent lights (Pip-Boy glow, robot head LEDs, vendor sign illumination) co-shipped with the geometry NIF. Cell-record lights still work; mesh-embedded lights don't.

## Suggested Fix
Trace one failing block with `crates/nif/examples/trace_block.rs`. Likely a bsver-gated trailing field — post-FO3 added a `Texture: Ref<NiSourceTexture>` for projected gobos per nif.xml line 1896 — that the parser unconditionally reads or unconditionally skips.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-06)
- Adjacent: #156 (initial NiPointLight audit)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: `NiSpotLight` extends `NiPointLight` — fix should resolve both
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Capture failing block via `trace_block.rs`; add byte-exact regression
- [ ] **CORPUS**: Reproduce zero NiUnknown for `NiPointLight` / `NiSpotLight` on FO4 + FO76 + SF Meshes archives
