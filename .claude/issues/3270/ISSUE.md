# 3270: ESM-2026-08-24-D5-01: FO4 WATR.DNAM offsets 12/16 misread as fog near/far - collapses fog ramp to ~1 world unit

**Severity**: HIGH · **Report**: `docs/audits/AUDIT_ESM_2026-08-24.md` (ESM-2026-08-24-D5-01) · cross-referenced by `AUDIT_FO4_2026-08-24.md` (FO4-2026-08-24-D6-01)

## Description

Commit `7f752c0c` correctly split FO76/Starfield's `decode_dnam_fo76`/`decode_dnam_starfield` so `depth_amount` (offset 0) no longer overwrites the canonical `fog_near`/`fog_far` defaults. For FO4 it instead added two new unverified reads claiming offsets 12/16 are the authored above-water fog near/far distances. No commit message, comment, or real-data test evidence ties offsets 12/16 to fog distances; the only test exercising them is a tautological synthetic fixture.

## Location

`crates/plugin/src/esm/records/misc/water.rs:1068-1088` (`decode_dnam_fo4`); regression surfaced by pre-existing `crates/plugin/tests/parse_real_esm.rs:1414-1435` (`parse_rate_fo4_esm`)

## Evidence

Independent GRUP walk of `Fallout4.esm`, 15+ WATR records: offset 12 is constant ~1.0 across nearly all records; offset 16 ranges 0.56-0.93. Not plausible fog-ramp distances. For `ExtOceanWater`: `fog_far = max(0.9299, 2.0) = 2.0` — a value not present anywhere in the source bytes, fabricated by the clamp.

Reproducible: `cargo test -p byroredux-plugin --test parse_real_esm -- --ignored parse_rate_fo4_esm` fails.

## Impact

Every vanilla FO4 water surface reaches full fog color within ~1 world unit of view depth via `env_translate.rs:527-528` → `water.frag:437-470`, instead of the intended hundreds-to-thousands-unit ramp `depth_amount` was meant to drive. FO4-only; FO76/Starfield unaffected.

## Related

Introduced by `7f752c0c`. Same tautological-fixture defect class as ESM-2026-08-20-D5-03.

## Suggested Fix

Delete the offsets 12/16 reads from `decode_dnam_fo4`; let `fog_near`/`fog_far` stay at `WaterParams::default` (80.0/600.0), matching FO76/Starfield. Restore `parse_rate_fo4_esm`'s original assertions.

## Completeness Checks
- [ ] **TESTS**: `parse_rate_fo4_esm` real-master assertion restored to pass; synthetic fixture assertions on offsets 12/16 replaced or removed
