# AUD-2026-08-07-D5-01: Reverb-send gate duplicates -60.0 literal instead of reusing SILENCE_DB

**GitHub**: #2405
**Severity**: LOW
**Labels**: low, tech-debt, bug
**Dimension**: Reverb Send & Routing (M44 audio)
**Source**: `docs/audits/AUDIT_AUDIO_2026-08-07.md`

## Location
- `crates/audio/src/lib.rs:138` (`SILENCE_DB` definition)
- `crates/audio/src/lib.rs:806` and `crates/audio/src/lib.rs:924` (duplicated gate)

## Description
`SILENCE_DB: f32 = -60.0` already exists in this file (`lib.rs:138`) as the named "below this = inaudible" threshold, used by `linear_volume_to_db`'s clamp. The reverb-send gate at both dispatch sites (`drain_pending_oneshots` and `dispatch_new_oneshots`) re-expresses the same semantic threshold as a bare `-60.0` literal instead of referencing the constant, and the whole 4-line gate-and-apply block is copy-pasted verbatim across the two dispatch functions rather than factored into one shared helper.

```rust
// lib.rs:805-809 (drain_pending_oneshots) and lib.rs:923-927 (dispatch_new_oneshots) — identical
if let Some(reverb) = audio_world.reverb_send.as_ref() {
    if audio_world.reverb_send_db.is_finite() && audio_world.reverb_send_db > -60.0 {
        track_builder = track_builder.with_send(reverb.id(), audio_world.reverb_send_db);
    }
}
```

## Impact
None today — the two sites are verified byte-identical, and nine consecutive prior audit cycles (2026-05-05 through 2026-08-03) have each manually re-confirmed they stay in sync. That track record is the tell: the invariant is held by repeated manual diffing, not by the compiler making drift impossible. A future edit to one site landing without the other would silently desync reverb wetness between queue-driven one-shots (footsteps) and entity-driven one-shots (emitters).

## Related
Direct precedent for this exact fix pattern already exists in this file — AUD-2026-06-23-01 (closed) extracted three inlined `20*log10(volume)` conversions into the shared `linear_volume_to_db` helper for the identical divergence-risk reason.

Also worth landing alongside `reverb_zone_system`'s per-cell-acoustics extension point (binary interior/exterior detector today) so the per-cell detector value and per-dispatch gate don't drift apart as more callers appear.

## Suggested Fix
Extract a small private helper, e.g. `fn apply_reverb_send(builder: SpatialTrackBuilder, audio_world: &AudioWorld) -> SpatialTrackBuilder`, called from both dispatch sites, referencing `SILENCE_DB` instead of the bare `-60.0` literal.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. reverb-send gate applies identically via both dispatch paths after refactor)
