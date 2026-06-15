# AUD-2026-06-14-01: Reversed Attenuation (min_distance > max_distance) panics in kira's audio render thread

- **Issue**: #1612
- **Severity**: MEDIUM
- **Labels**: medium, legacy-compat, bug
- **Dimension**: Listener Pose & Attenuation
- **Location**: `crates/audio/src/lib.rs:770` (`drain_pending_oneshots`), `:892` (`dispatch_new_oneshots`), `:535-551` (`Attenuation` struct + `Default`), `:1057-1087` (`spawn_oneshot_at`)
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-06-14.md`

## Description
Both dispatch paths build `SpatialTrackBuilder::distances(att.min_distance..=att.max_distance)` from a caller-supplied `Attenuation` with no `min <= max` check. kira computes attenuation as `distance.clamp(self.min_distance, self.max_distance)` (`kira-0.10.8/src/track/sub/spatial_builder.rs:356`); Rust's `f32::clamp` panics if `min > max`. The panic fires on kira's audio render thread at playback (not dispatch), so it is invisible to the call site.

## Evidence
- `lib.rs:770` & `:892`: `.distances(p.attenuation.min_distance..=p.attenuation.max_distance)` — verbatim.
- kira `spatial_builder.rs:356`: `distance.clamp(self.min_distance, self.max_distance)`.
- Only two live `Attenuation` constructions: footstep `{0.5, 12.0}` (`systems/audio.rs:183`) and `Default {2.0, 30.0}` — both ordered. No `debug_assert!(min <= max)` anywhere.

## Impact
Latent defense-in-depth gap (no current path triggers it). Public API silently accepts a reversed range and converts it into a hard panic deep in the audio thread — hostile for future data-driven FOOT/REGN/scripted producers. Blast radius: whole audio thread (process abort).

## Suggested Fix
`debug_assert!(min_distance <= max_distance)` or clamp-normalize (`let (lo, hi) = (min.min(max), min.max(max))`) at both dispatch sites, or validate once in `Attenuation` construction. Clamp-normalize preferred for data-driven producers.
