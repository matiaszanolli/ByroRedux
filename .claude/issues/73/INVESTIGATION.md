# Investigation: Issue #73

## Root Cause
NiBlendTransformInterpolator (and siblings) not registered in parse_block.
Used by NiControllerManager for animation blending between sequences.

## Binary Layout (version >= 10.1.0.112 = all Bethesda games)
NiBlendInterpolator base:
  flags: u8
  array_size: u8
  weight_threshold: f32
  if !(flags & 1):  // not manager-controlled
    interp_count: u8
    single_index: u8
    high_priority: i8
    next_high_priority: i8
    single_time: f32
    high_weights_sum: f32
    next_high_weights_sum: f32
    high_ease_spinner: f32
    interp_array_items: [InterpBlendItem; array_size]
      each: interp_ref (BlockRef), weight (f32), normalized_weight (f32), priority (u8), ease_spinner (f32)

Concrete subtypes add:
  NiBlendTransformInterpolator: nothing extra (>= 10.1.0.110)
  NiBlendFloatInterpolator: value: f32
  NiBlendPoint3Interpolator: value: [f32; 3]
  NiBlendBoolInterpolator: value: u8

## Fix
Add NiBlendInterpolator base parser + 4 concrete subtype registrations.
Manager-controlled (flag bit 0) is the common case for Bethesda games —
only reads flags + array_size + weight_threshold.

## Scope
2 files: interpolator.rs (parser), mod.rs (dispatch registration).
