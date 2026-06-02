# Issues 1402 + 1403

## #1402 ANIM-08: Multi-emitter first-match limitation undocumented
**File:** `crates/nif/src/import/walk/mod.rs:616`
**Fix:** Both `extract_first_color_curve` and `extract_emitter_rate` already
carry partial scope notes; add #1402 to their cross-reference lists so the
deferred work is traceable.

## #1403 SAFE-U2: slice::from_raw_parts on WaterPush missing SAFETY comment
**File:** `crates/renderer/src/vulkan/water.rs:466`
**Fix:** Add SAFETY comment before `from_raw_parts` explaining why the cast
is valid: WaterPush is #[repr(C)] + Copy with only [f32;4] fields.
Sibling check: no other from_raw_parts sites in water.rs.
