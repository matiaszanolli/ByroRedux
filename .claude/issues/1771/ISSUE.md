# FNV-D6-NEW-01: authored birth-rate of exactly 0.0 silently kills the emitter

**Severity**: MEDIUM · **Source**: `docs/audits/AUDIT_FNV_2026-06-27.md` (FNV-D6-NEW-01)
**Location**: `crates/nif/src/import/walk/mod.rs` (`extract_emitter_rate::sane`); consumed at `byroredux/src/systems/particle.rs` (`apply_emitter_overlays`) + gated at `byroredux/src/systems/particle.rs` (the `em.rate > 0.0` spawn guard)
**Status**: NEW

## Description
`extract_emitter_rate` reads a single representative birth-rate scalar (the first key of the `NiPSysEmitterCtlr` interpolator's `NiFloatData`, or the `NiFloatInterpolator` constant). Its `sane()` filter is `(r.is_finite() && (0.0..3.0e38).contains(&r)).then_some(r)` (`walk/mod.rs:775-778`) — the **inclusive lower bound accepts 0.0**. When the authored rate resolves to exactly 0.0, `extract_emitter_rate` returns `Some(0.0)`; `apply_emitter_overlays` (`particle.rs:75`) then unconditionally writes `preset.rate = 0.0`, overwriting the heuristic preset's non-zero default. The CPU spawn guard `em.rate.is_finite() && em.rate > 0.0` (`particle.rs:385`) is then false forever, so the emitter never spawns a single particle.

## Evidence
`sane(0.0)` → `Some(0.0)` (`0.0..3.0e38` contains `0.0`). `apply_emitter_overlays` has no `rate > 0.0` check before assigning. The FLT_MAX / negative / NaN cases all correctly reject to `None` (→ preset fallback); the finite-but-degenerate `0.0` is the one value that slips through to `Some`.

## Impact
FNV (and FO3) ship many ramp-up emitters whose authored birth-rate **first key is 0.0** (rate climbs over the clip — spell-cast bursts, geyser/steam ramps, ignition FX). Because rate animation is deferred by design (#1402 — single representative scalar), a 0.0 first key now reads as a permanent-zero constant rate. Pre-overlay these fell back to the heuristic preset and emitted *something*; post-overlay they go fully invisible. Visible content loss scoped to emitters whose authored rate sampled at t=0 is exactly 0.0.

## Related
Scope-limitation #1402 (rate animation deferred). Sibling guards #1364 (FLT_MAX), #1382 (NaN rate) — NOT dups: those reject to `None`; this returns `Some(0.0)`.

## Suggested Fix
In `sane()` (or in `apply_emitter_overlays`), treat a resolved rate of `0.0` as "no usable authored rate" and fall back to the preset: make the range exclusive-low (`0.0 < r && r < 3.0e38`) so a zero first-key returns `None` and keeps the preset's spawn rate, matching the no-controller path. (Fuller fix: sample the rate curve over time per #1402.)

## Completeness Checks
- [ ] **SIBLING**: Check the other `sane()`-style finite filters in `extract_emitter_params` (e.g. `start_size`) for the same inclusive-zero-kills-the-emitter shape
- [ ] **CANONICAL-BOUNDARY**: The fix touches `extract_emitter_rate` in `crates/nif/src/import/walk/mod.rs` — keep the authored→preset fallback at the NIFAL parser boundary; do not push rate logic into the renderer/`systems/particle.rs` spawn loop. See `/audit-nifal`.
- [ ] **TESTS**: A regression test feeds an emitter whose authored first-key rate is `0.0` and asserts the preset rate is retained (emitter spawns), not zeroed
