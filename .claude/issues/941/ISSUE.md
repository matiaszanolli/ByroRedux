# NIF-D5-NEW-02: BSTreadTransfInterpolator not dispatched (Liberty Prime, Power Armor wheels)

**Severity**: MEDIUM
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 5)

## Game Affected

FO3, FNV, Skyrim LE/SE, FO4 (per nif.xml `versions="#FO3_AND_LATER#"`)

## Location

`crates/nif/src/blocks/mod.rs` — no entry. Originally flagged in 2026-04-18 audit as `NIF-COV-11` with no closing issue.

## Why it's a bug

Used on tread/wheel rolling animation:
- FNV: Ranger Vertibird treads
- FO3: Liberty Prime treads
- Skyrim: mammoth/horse rolling-stone
- FO4: Power Armor wheels and Vertibird

Inherits `NiInterpolator` and carries `Num Tread Transforms` + array of `BSTreadTransform` + `Ref<NiFloatData>`. ~20 LOC parser.

## Impact

Silent `NiUnknown` skip via block_size (FO3+ all have block_sizes table → no drift). Affected vehicle/creature tread animation imports as static — wheel/tread textures don't roll. Cosmetic but obvious on Liberty Prime / Power Armor.

## Fix

Add `BsTreadTransfInterpolator` block in `interpolator.rs` reading `num` + `array<BSTreadTransform>` + `NiFloatData` ref. Animation importer needs a new "tread-uv" channel — defer if pipeline isn't ready, parser-only is still worth landing.

## Completeness Checks

- [ ] **SIBLING**: Check whether `BSTreadTransform` compound also needs a parser
- [ ] **TESTS**: Fixture parse test; corpus assertion that FO3 Liberty Prime KF references this type
