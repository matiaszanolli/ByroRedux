# Investigation

- Real sort key in render.rs:597-619 orders fields:
  1. alpha_blend (u8: opaque=0, blend=1)
  2. is_decal (u8)
  3. two_sided (u8)
  4+ depth state / depth / mesh / texture
- Debug assert in draw.rs read (alpha_blend, two_sided, is_decal) — slots 2/3 swapped.
- Audit recommended option (a): delete the assert + add a unit test on render.rs.

## Change
- Extracted `pack_depth_state` + `draw_sort_key` as free functions.
- `draw_sort_key` is `pub(crate)` so tests can hit it.
- draw.rs comment now points at the new test by name.
- 3 new unit tests: cluster order, opaque front-to-back, transparent back-to-front.

## Tests
byroredux: 47 → 50.
