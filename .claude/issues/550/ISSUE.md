# NIF-05: SkyShaderProperty alias under-consumes 13-28 bytes per block (FNV+FO3)

**Severity**: HIGH
**Dimension**: Stream Position × Version Handling
**Game Affected**: FO3, FNV (Oblivion sky meshes too)
**Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-05

## Summary

`SkyShaderProperty` inherits `BSShaderLightingProperty` and adds 4 scroll vectors (per nif.xml line 6284 — `Sky Scroll X`, `Sky Scroll Y`, `Sky Distance`, `Sky Z Distance`, plus an optional `Flag`). The current dispatch at `blocks/mod.rs:268-276` aliases to `BSShaderPPLightingProperty::parse`, which consumes only texture_set_ref + refraction + parallax, missing the sky-specific tail. `block_sizes` keeps the outer stream aligned so the bug is silent at parse time. Sky NIFs render with default scroll and distance — static clouds, wrong horizon fade.

## Evidence

Recurring warning bucket in `/tmp/audit/nif/fnv_err.log:21-49` and `/tmp/audit/nif/fo3_err.log:16-36` — 6+ distinct block offsets per run, all `SkyShaderProperty` with `consumed 54, expected 42-82`.

## Location

`crates/nif/src/blocks/mod.rs:268-276` — alias block for `"BSShaderPPLightingProperty" | "SkyShaderProperty" | ...`.

## Suggested fix

Split `SkyShaderProperty` out of the alias block — same structural move as `TileShaderProperty` got in #455 and `WaterShaderProperty` / `TallGrassShaderProperty` in #474. Parser reads PPLighting base + 4 f32 + optional Flag. ~60 LOC with unit test.

## Completeness Checks
- [ ] **SIBLING**: Verify `HairShaderProperty`, `BSDistantTreeShaderProperty` at the same alias site — same under-consume pattern likely
- [ ] **TESTS**: Synthetic SkyShaderProperty block round-trip at FO3/FNV bsver
- [ ] **REAL-DATA**: FNV + FO3 stderr sweeps drop the SkyShaderProperty warning bucket to 0

Fix with: /fix-issue <number>
