# FNV-D6-NEW-02: EmitterBaseParams.radius_variation dropped at the ImportedEmitterParams handoff

**Severity**: LOW · **Source**: `docs/audits/AUDIT_FNV_2026-06-27.md` (FNV-D6-NEW-02)
**Location**: parsed in `crates/nif/src/blocks/particle.rs` (`read_emitter_base`, `radius_variation`) but dropped at the handoff to `ImportedEmitterParams` in `crates/nif/src/import/types.rs`; handoff fn `extract_emitter_params` in `crates/nif/src/import/walk/mod.rs`
**Status**: NEW

## Description
`read_emitter_base` correctly parses `radius_variation` (`particle.rs:64,112,128`; version-gated `>= 10.4.0.1`, satisfied by every retail title incl. all FNV) into `EmitterBaseParams.radius_variation`. But `ImportedEmitterParams` (`types.rs:1236`) has **no** `radius_variation` field, so `extract_emitter_params` never copies it forward and `apply_emitter_params` cannot consume it. The canonical `ParticleEmitter` (only `start_size`/`end_size`, no per-particle size-variance field) spawns every particle at exactly `initial_radius × base_scale` with zero radius jitter.

## Evidence
`grep radius_variation` finds consumers only inside the parser + its tests (`particle.rs:64,112,128,1624,1645,1764-1778`); nothing in `import/`, `systems/`, or `components/` reads it. The `is_finite()` sweep in `extract_emitter_params` deliberately covers `planar_angle` / `planar_angle_variation` (proactively, #1445) but not `radius_variation` — because the field is never forwarded, only the parser holds it.

## Impact
Low — FNV particle FX that authored a per-particle radius spread render with uniform-size particles instead of the intended jittered spray. No crash, no stream desync (the byte is consumed). Visual-fidelity gap only. Same drop-at-handoff family as #1610 (tint_map), #1445 (planar_angle), #1580 (BGEM bool).

## Related
#1445 (same struct, planar_angle handled), #1610 (drop-at-handoff precedent).

## Suggested Fix
Add `radius_variation: f32` to `ImportedEmitterParams`, forward it in `extract_emitter_params` (include it in the finite sweep), and add a `start_size_variation` to `ParticleEmitter` consumed at spawn (size jitter) in `systems/particle.rs`. Lowest-risk first step: forward the value so it isn't lost, even before the renderer consumes it.

## Completeness Checks
- [ ] **SIBLING**: Audit `EmitterBaseParams` for any other parsed-but-not-forwarded field beyond `radius_variation` (same drop-at-`ImportedEmitterParams`-handoff class as #1445/#1610)
- [ ] **CANONICAL-BOUNDARY**: The forward happens in `extract_emitter_params` (`crates/nif/src/import/walk/mod.rs`) — keep the authored value flowing through the NIFAL parser→`ImportedEmitterParams` boundary; do not re-derive size jitter in the renderer. See `/audit-nifal`.
- [ ] **TESTS**: A regression test asserts `radius_variation` survives the `ImportedEmitterParams` handoff (and, once consumed, drives non-zero size variance)
