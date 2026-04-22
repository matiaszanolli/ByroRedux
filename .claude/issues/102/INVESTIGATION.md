# #102 Investigation

## Root cause

`crates/renderer/shaders/triangle.frag:1213` — the per-light shadow
ray dispatcher sets `rayDist = 10000.0` for the directional-light
branch. This value flows straight into the `rayQueryInitializeEXT`
call at line 1225 as `tmax`.

FNV / FO3 / Oblivion cells are **4096 × 4096 units** each. At the
default loaded radius of 3 the player sees a 7×7 grid = ~28 672 units
across; at the max radius of 7 (#531) the grid is 15×15 = ~58 000
units across. A directional (sun) shadow ray can need to cross the
diagonal of the loaded footprint to find a distant mountain / cliff
face that would occlude the sun. Anything beyond 10 000 units from
the shaded fragment silently misses the shadow test → visible as
"floating" lighting on distant terrain faces and missing cast
shadows from opposite-cell architecture.

## Fix

Bumped `rayDist` to `100 000.0` on the directional branch. Covers
the ~58K worst-case diagonal with ~40K headroom for distant LOD
terrain (Tamriel-scale skyboxes, Mojave far-field rocks).

BVH traversal cost is logarithmic in scene size — the longer tmax
doesn't meaningfully affect frame time. The point/spot branch was
already distance-bounded via the per-light radius.

## Unchanged

Line 1107's `dist = 10000.0` inside the main light loop is a
placeholder assignment for the directional branch that only feeds
the local attenuation math block (lines 1085-1101). It is never
plumbed into a shadow ray tmax. Left alone to minimise blast radius.

## Also not changed

The issue mentioned making the tmax "configurable via scene UBO" as
an alternative. That would introduce a new scene-UBO field and
Rust-side wiring for no observable benefit — the 100K constant
already covers worst-case exterior grids and scales by the radius
cap enforced at CLI parse time. Deferred unless the cap ever grows.

## Verification

- Shader recompiles clean (`glslangValidator` → `triangle.frag.spv`).
- 78/78 renderer unit tests pass.
- No Rust code touched; downstream reflection validation (#427)
  still sees the same descriptor layout.

## Files changed

- `crates/renderer/shaders/triangle.frag` (+ SPIR-V recompile)
