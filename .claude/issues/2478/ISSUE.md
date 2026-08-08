# REN-D22-03: Flicker/pulse parameters bypass the per-game boundary the flags respect -- pre-Skyrim lights can never animate

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2478
**Finding ID**: REN-D22-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 22 — Light Animation
**Location**: `crates/plugin/src/esm/cell/support.rs:75` (`build_static_object_from_subs`, `b"DATA" if is_ligh` arm) → `byroredux/src/cell_loader/references/attach.rs:417` (`attach_light_flicker_if_needed`)
**Status**: NEW

## Description
`canonical_light_animation_flags` canonicalizes the *flags* per game, but the *animation parameters* they drive (`period_secs`, `intensity_amplitude`, `movement_amplitude`) are read at fixed **Skyrim** `DATA` offsets 28/32/36 for every `DATA`-layout game, gated only on subrecord *length*. Skyrim's LIGH `DATA` is 48 bytes (..., 28 Flicker Period, 32 Intensity Amplitude, 36 Movement Amplitude, 40 Value, 44 Weight). Oblivion/FO3/FNV's `DATA` is 32 bytes and ends "..., 16 Falloff, 20 FOV, 24 Value(u32), 28 Weight(f32)". Consequences on FO3/FNV (and 32-byte Oblivion): (1) `len >= 32` is true → `period_secs = read_f32(28)` reads the record's **Weight**, not a flicker period — the `> 0.0` fallback doesn't fire (weight is positive), so garbage is kept instead of the 0.5s default. (2) `len >= 36` is false → `intensity_amplitude = 0.0`, with no fallback anywhere — `flicker_intensity` returns `1.0` always. Every FNV/FO3/Oblivion torch, candle and campfire that authored the Flicker bit gets a `LightFlicker` attached and produces exactly zero visible animation. The doc on `LightData.period_secs` ("Zero when the LIGH record's DATA subrecord is truncated ... only the 16-byte header") encodes the wrong premise: pre-Skyrim `DATA` is 32 bytes, not 16.

## Evidence
```rust
// support.rs — one Skyrim-layout decode for every DATA-layout game
let period_secs         = if sub.data.len() >= 32 { read_f32(28) } else { 0.0 }; // FNV: Weight
let intensity_amplitude = if sub.data.len() >= 36 { read_f32(32) } else { 0.0 }; // FNV: absent → 0
// attach.rs — only period has a fallback; amplitude has none
let period_secs = if ld.period_secs > 0.0 { ld.period_secs } else { 0.5 };
// light_anim.rs:179
1.0 + modulation * flicker.intensity_amplitude * FLICKER_INTENSITY_DAMPING  // == 1.0 when amp == 0
```

## Impact
Visual-only, but wide: flicker/pulse is silently dead on the project's most-exercised game (FNV) and on FO3/Oblivion — every interior torch is a constant light. Also wasted per-frame work (a `LightFlicker` slot + query hit per torch that provably cannot change anything), and a latent trap: anyone who later adds an `intensity_amplitude` default would immediately start driving the animation at a period sourced from the record's Weight field.

## Related
#2250 / #2251 (the flag half of the same boundary); REN-D22-04 (this report — shares the `flicker_intensity` call path).

## Suggested Fix
Discriminate the `DATA` arm on game/length like the `DAT2` arm already does — only read 28/32/36 when the layout actually has them (Skyrim+/48-byte), and treat pre-Skyrim as "no authored flicker parameters". Then give `intensity_amplitude` an explicit default at the boundary (same shape as the existing `period_secs` 0.5 fallback) so pre-Skyrim Flicker/Pulse bits still animate with engine-chosen amplitude, or skip the `LightFlicker` attach entirely when no parameters exist.

## Completeness Checks
- [ ] **TESTS**: A regression test decodes a real pre-Skyrim LIGH `DATA` and asserts `period_secs` falls back to a sane default (not the record's Weight)
- [ ] **CANONICAL-BOUNDARY**: Per-game DATA layout discrimination stays at the ESM parser boundary, not pushed into the light-animation system
