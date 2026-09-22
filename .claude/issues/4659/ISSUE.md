# PAR-D4-2026-09-21-01: No CI lane runs any parser crate's real-data suite: the nightly lane is NIF-only

Labels: medium,bug,import-pipeline,test-gap

## Description
`.github/workflows/real-data-gates.yml:97-101` runs only:
```
cargo test --release -p byroredux-nif \
  --test parse_real_nifs --test per_block_baselines --test block_coverage_baselines \
  -- --ignored ...
```
`ci.yml` runs `cargo test --workspace`, which skips every `#[ignore]`d test. None of the bsa, bgsm, sfmaterial, hkx, facegen or menuxml real-data suites is scheduled anywhere. This re-verifies the skill's 2026-09-19 note; it is unchanged.

Verified unchanged at HEAD `ee6d3fb39`: `real-data-gates.yml`'s `corpus` job still filters `-p byroredux-nif` exclusively; no sibling job exists for the other parser crates.

## Evidence
This audit ran the missing suites by hand: 37 tests plus 3 `archive_precedence` tests, all green (see the report's Executive Summary real-data-suite breakdown). A regression in `Ba2Archive::open`, the BGSM decoder, the HKX decoder or the CDB walker would stay invisible until someone does the same by hand.

## Impact
No automated coverage of vanilla archive, material, packfile or FaceGen decoding. #3918-style silent regressions (a change that drops or corrupts vanilla parse output with CI staying green) are possible here exactly as they were for NIF before #3919 added its own nightly lane.

## Related
#3919 (the NIF-side fix this generalises), #3850, #1558

## Suggested Fix
Add per-title steps (or one parsers job) to `real-data-gates.yml` running `cargo test --release -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml} -- --ignored`, together with the lane's existing no-test-matched guard.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D4-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix