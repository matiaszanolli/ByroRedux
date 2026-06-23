# AUD-2026-06-23-01: vol→dB conversion duplicated verbatim across three dispatch sites

**Issue**: #1709
**Severity**: LOW
**Labels**: low, tech-debt, bug
**Source audit**: `docs/audits/AUDIT_AUDIO_2026-06-23.md`
**Dimension**: Spatial Sub-Track Lifecycle (M44 audio)
**Location**: `crates/audio/src/lib.rs:438–442` (`play_music`), `:805–809` (`drain_pending_oneshots`), `:937–941` (`dispatch_new_oneshots`)

## Description
The linear-amplitude → decibels conversion (`if vol > 0.0001 { 20.0 * vol.log10() } else { -60.0 }`) is copy-pasted identically into all three sound-dispatch paths. There is **no drift** — all three copies are byte-for-byte identical, so this is NOT a correctness bug. It is a maintainability hazard only: a future tweak to one site could silently diverge the others.

## Evidence
```rust
// identical at lib.rs:438, 805, 937
let db = if /* vol */ > 0.0001 {
    20.0 * /* vol */.log10()
} else {
    -60.0
};
```

## Impact
None today (all copies agree). Future risk of inconsistent per-path loudness if one site is edited without the others. No AS/SSBO/GPU correctness exposure.

## Related
The reverb-send gate (`is_finite() && > -60.0`) is also duplicated verbatim at `lib.rs:794` and `:916`.

## Suggested Fix
Extract a private `fn linear_volume_to_db(v: f32) -> f32` (and optionally a `fn reverb_send_for(&self) -> Option<(SendTrackId, f32)>`) and call it from the three/two sites.

## Completeness Checks
- [ ] **SIBLING**: The reverb-send gate dup at `lib.rs:794`/`:916` is collapsed in the same pass
- [ ] **TESTS**: A unit test pins `linear_volume_to_db` at the epsilon boundary and silence clamp

## Validation
CONFIRMED against current code (HEAD 2d4c350d): three identical copies at lib.rs:438, 805, 937.
