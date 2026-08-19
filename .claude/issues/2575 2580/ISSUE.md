# #2575: OBL-D7-02: Doc drift: ROADMAP.md's Oblivion exterior compat-matrix entity/FPS figure is stale against the newer, more thorough readiness-plan bench

**Severity**: LOW
**Dimension**: Exterior Blocker Chain & Game-Specific Quirks
**Location**: `ROADMAP.md:430` vs `docs/engine/exterior-readiness-plan.md`
**Status**: NEW
**Labels**: documentation, low, legacy-compat

## Description
`ROADMAP.md` still cites "4,886 entities / 150.6 FPS" for Tamriel `(0,0)` radius-1; the 2026-08-04 EX-01 sweep re-ran the identical profile and recorded 5,709 entities / 2,355 draws with an explicit image-health pass — a denser, more validated measurement of the same scenario, landed in the same commit window that touched `ROADMAP.md` for an adjacent edit but left this figure untouched.

## Evidence
Confirmed directly: `ROADMAP.md:430` still reads "Tamriel `(0,0)` radius 1 recorded 4,886 entities / 150.6 FPS."

## Impact
Documentation-only; risk is a future contributor misreading the delta as a regression.

## Suggested Fix
Update `ROADMAP.md:430` to cite the 2026-08-04 figures and/or point at `docs/engine/exterior-readiness-plan.md` as the live source.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)

---

# #2580: SKY-D2-02: shader_flags.rs module doc asserts Skyrim has an Alpha_Test SLSF1 bit -- nif.xml has none, and the file contradicts itself 37 lines later

**Severity**: LOW
**Dimension**: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/shader_flags.rs:203` (vs `:240-241`)
**Status**: NEW
**Labels**: documentation, nif-parser, low

## Description
The `fo4_slsf2` module doc's parenthetical ("Skyrim has Alpha_Test on SLSF1!") is unsupported by nif.xml — no `Alpha_Test` option exists anywhere in `SkyrimShaderPropertyFlags1`/`2` (bit 25 is `Remappable_Textures`). Skyrim routes alpha-test exclusively via `NiAlphaProperty`, which the same file's own doc states correctly 37 lines below.

## Evidence
Confirmed directly: `shader_flags.rs:203` reads "Bit 25 is `Alpha_Test` on FO4 (Skyrim has Alpha_Test on SLSF1!)" while `:240-241` correctly reads "Bit 25 — `Alpha_Test` on FO4. Skyrim routes alpha-test via `NiAlphaProperty` on a sibling block, not a shader flag bit." nif.xml:6396 confirms bit 25 is `Remappable_Textures`.

## Impact
No runtime effect (no code reads Skyrim SLSF1 bit 25), but this file's stated purpose is documenting per-game bit semantics for future contributors — exactly the error class behind #414/#1879.

## Related
#414, #1879

## Suggested Fix
Fix the parenthetical to match `fo4_slsf2::ALPHA_TEST`'s own correct doc.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
