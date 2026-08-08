# REN-D22-05: Authored period_secs is ignored on the FLICKER path (hardcoded 12 Hz)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2516
**Finding ID**: REN-D22-05 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 22 — Light Animation
**Location**: `byroredux/src/systems/light_anim.rs:164` (`flicker_intensity`, flicker branch)
**Status**: NEW

## Description
The pulse branch honours `flicker.period_secs`; the flicker branch steps its hash buckets at a hardcoded `12.0` Hz and never reads the authored period at all. Skyrim authors a per-light FNAM period (candles ~0.5s, larger fixtures longer), so every flickering light in a scene runs at an identical rate regardless of what the record asked for — the only per-light variation left is `phase_offset_secs`.

## Evidence
```rust
let raw = (total_time + flicker.phase_offset_secs) * 12.0 * speed_scale; // period_secs unused
```

## Impact
Visual-only and subtle; a roomful of mixed fixtures flickers homogeneously. The `12.0` is documented as a *tuning* value (24 Hz → 12 Hz in Phase 19) with no note that it deliberately supersedes the authored period, so this reads as an oversight rather than a decision.

## Related
REN-D22-03 (this report) — on pre-Skyrim games the parsed `period_secs` is garbage anyway, so fixing this one without that one would make FNV worse.

## Suggested Fix
Derive the bucket rate from `period_secs` (e.g. `buckets_per_sec = k / period_secs`, `k` chosen so the current 0.5s Skyrim candle still lands at ~12 Hz), or state explicitly in the comment that the authored period is intentionally not used for the noise path.

## Completeness Checks
- [ ] **SIBLING**: Fix alongside REN-D22-03 (this report) since fixing this one without the ESM layout fix would worsen pre-Skyrim games
- [ ] **TESTS**: A regression test confirms two lights with different `period_secs` flicker at visibly different rates
