# #1043 — NIF parsing duplication: generic KeyGroup, impl_ni_object! macro, read_array_of

**Labels**: nif-parser, tech-debt, medium
**Status**: TD3-001 already fixed (KeyGroup<K: KeyParse> exists in interpolator.rs)

## TD3-005: impl_ni_object! macro
175 impl NiObject for X across 29 files. ~130 trivial (only block_type_name + as_any).
Non-trivial files (also override as_object_net/as_av_object/as_shader_refs):
  bs_geometry.rs, legacy_particle.rs, light.rs, mod.rs, node.rs, texture.rs, tri_shape.rs

## TD3-013: read_array_of combinator
Add to NifStream in stream.rs. Convert one block parser as canonical example.
