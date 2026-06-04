# Investigation — #1441 LC-D5-02 KeyType::Constant collapsed to Linear

**Domain:** animation / nif (clip translation)

## Root cause
Core `KeyType` had only `Linear / Quadratic / Tbc`. The NIF→core converter
(`byroredux/src/anim_convert.rs`) mapped `KeyType::Constant => Linear`, so a
stepped (Gamebryo `KEY_CONST`) TRS channel was LERPed across the segment instead
of held — wrong motion for hard-cut keyframed scenery / IK poses.

## Fix
- `crates/core/src/animation/types.rs`: added `KeyType::Const` (stepped hold).
- `byroredux/src/anim_convert.rs`: `Constant => KeyType::Const` (XyzRotation still
  maps to Linear — its Constant handling is baked by the NIF XYZ-Euler scalar
  sampler, a separate path).
- `crates/core/src/animation/interpolation.rs`: added a `KeyType::Const =>
  Some(k0.value)` (hold start value) arm to all three TRS samplers
  (sample_translation / sample_rotation / sample_scale).

`cargo check` confirms those three were the only `KeyType` matches in core
(float/color/bool channels don't carry a KeyType — they always lerp / always
step — so `Const` is correctly scoped to TRS).

## SIBLING
Both the KF-sequence path (`import_kf`) and the embedded path
(`import_embedded_animations`) produce a nif `AnimationClip` that flows through
`anim_convert::convert_key_type`, so the single converter fix covers both. Other
channel converters (float/color) carry no interpolation type. CANONICAL-BOUNDARY:
the per-game key-type semantics are resolved at the clip-translation boundary
(anim_convert) and honored game-agnostically by the sampler — never at render time.

## Tests
`const_keytype_holds_start_value_across_segment` — asserts a Const channel holds
k0 mid-segment (translation/rotation/scale) and snaps to k1 at the next key time.

## Verification
cargo test 2794 passed; no warnings in touched files.
