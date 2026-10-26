# EXT-D3-2026-10-26-01: Authored-model height scale is correlated with the record pick (both read seed stream 4)

Labels: medium,terrain-exterior,shaders,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: MEDIUM / **Dimension**: 3 / **Tier Violated**: n/a
- **Game Affected**: all games with authored GRAS models on ground that lists more than one placeable record (Skyrim/FO4/FO3/FNV)
- **Location**: `crates/renderer/shaders/groundcover_models.comp:449` (pick) and `:646` (scale); `gcPickLocalRecord` `:312-400`
- **Status**: NEW (code predates the baseline: the same two lines are at `:431` / `:593` in 4a3e89d56; not in `docs/audits/AUDIT_EXTERIOR_*` or open issues)
- **Description**: SCATTER picks the record with `gcPickLocalRecord(..., gcUnit(gcModelHash(h, 4u)))`, which does `target = u * accept` and walks records in a fixed lane/bit order. EMIT then
  draws the plant's height scale from `gcUnit(gcModelHash(h, 4u))` of the same `h` (`point.meta.w`). Same hash, same stream, same value: a record that owns the bucket `[a, b)` of `u` can only ever
  receive scale values from that bucket.
- **Evidence**: `pick = gcPickLocalRecord(cellIdx, ground, gcUnit(gcModelHash(h, 4u)));` ... `float scale = 1.0 + (gcUnit(gcModelHash(h, 4u)) * 2.0 - 1.0) * rec.heightRange;`
  Streams in use in this file: 1, 2, 3, 4 (pick), 4 (scale), 5, 7 (new wind phase); 6 is free.
- **Impact**: with two or more records on the same ground, each record's height variation is truncated to a sub-range: the first-ordered record is systematically shorter, the last taller
  (only a single dominant record gets the full `+-heightRange`). This is exactly the "natural height variance" the field exists for, silently biased per species mix. Deterministic, so no flicker. Static trace only; unconfirmed on screen.
- **Related**: #4906 (per-cell record selection), #6027 (density applied once). Skill "Determinism" bullet (streams are independent hashes) is the rule broken.
- **Suggested Fix**: draw the scale from an unused stream (`gcModelHash(h, 6u)`), and add a source-shape test that no two `gcModelHash(h, Nu)` uses in `groundcover_models.comp` share `N`.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
- [ ] **TESTS**: A regression test pins this specific fix
