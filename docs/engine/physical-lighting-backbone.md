# Physical lighting backbone

This document defines the runtime contract behind playable, stable lighting.
Legacy records are inputs to a translator, not renderer semantics.

## Runtime contract

1. **Units.** Runtime distances and medium coefficients use `Meters` and
   `ExtinctionPerMeter`. Bethesda coordinates remain in world units only where
   the existing transform/TLAS ABI requires them; conversion uses the single
   `BETHESDA_UNITS_PER_METER` constant in `byroredux_core::lighting`.
2. **Emitters.** `Emitter` owns geometry, scene-linear radiant intensity,
   physical source radius, range, distance law, and visibility. `LightSource`
   adds animation controls and keeps source flags only for diagnostics.
3. **Legacy boundary.** NIF/LIGH fields are resolved by
   `LightSource::from_legacy_world_units`. Shaders never inspect legacy shadow
   flags or infer an emitter type from source-format data.
4. **Visibility.** CPU emitters and Vulkan TLAS instances share six explicit
   bits: architecture, static props, dynamic actors, foliage, glass, effects.
   A light uploads a union of bits; ray queries consume that union directly.
   All legacy sources use the full material-aware query, irrespective of the
   source game's shadow-map projection flags. Cutouts use alpha coverage,
   glass transmits light, and effect cards do not become opaque blockers.
5. **Transport.** Surface, GI, water, caustic, and froxel passes use the same
   visibility vocabulary. Local surface/froxel attenuation selects either the
   bounded legacy curve or inverse-square metres with finite-source softening.
6. **Ray allocation.** `AdaptiveRayBudget` observes the slower of retired
   main-lighting and volumetric GPU timestamps (the upper-bound brackets are
   not summed). Four hysteretic tiers jointly bound glass/refraction claims,
   direct shadow samples, GI path depth/shaded hits, and froxel local lights.
   Downgrades react to overload; upgrades require 45 stable headroom frames.
7. **Temporal reconstruction.** Volumetrics default to one froxel per 8×8
   render pixels. World reprojection, density rejection, history-neighborhood
   variance clipping, and emission-weighted disagreement rejection prevent
   crawling noise and long fire trails. Local fire turbulence is independent
   of weather coverage and advects at a slower source-relative rate.
8. **Reference laboratory.** Deterministic tests pin unit conversion, layer
   independence, a candle-room legacy visibility case, inverse-square flux,
   quality-controller hysteresis, default froxel resolution, shader layouts,
   and committed SPIR-V reproducibility.

Skinned surfaces use the posed bone blend for both vertex normals and
model-space normal maps, not the actor root's placement matrix. Secondary-hit
parallax frames read that same palette; its compute-write barrier and scene
descriptor expose it to fragment as well as vertex consumers.

Current rigid shadow casters have explicit BLAS working-set protection.
Streaming/recovery batches advance the LRU clock, but cannot age a caster out
of that set before its draw completes. Recovery protects its full requested
set (including missing entries), and TLAS collection refreshes membership from
the actual current draws. Unused entries still follow normal LRU eviction;
resident-memory admission and fence-delayed destruction remain enforced.
Recovery scheduling counts only actual required resident bytes, not the
cache-wide mean mesh size. A large unused mesh must not price small missing
casters out of recovery. Count/deadline limits bound attempts, a short build
ends the frame's recovery, and the builder still enforces full live-plus-
deferred residency. Required entries cannot be evicted by those attempts.

Starfield's metric placements, light ranges, node translations, bind matrices,
and animation translations are normalized at the ESM/NIF import boundaries to
the same 70-world-units-per-meter frame as other games. Packed `.mesh` XYZ,
normal/tangent vectors, and bounds are Z-up source data, not pre-converted
renderer data. The importer rotates them with the nodes and resolves nifly's
69.969 vertex-only tooling scale before exposing canonical geometry. Authored
object scales and rotations stay dimensionless; renderer code has no game-unit
switch.

LIGH cone classification is also resolved once at the game boundary:

| Source layout | Spotlight shape signal |
| --- | --- |
| Oblivion / FO3 / FNV / Skyrim | `0x200` or `0x400` |
| Fallout 4 / Fallout 76 | `0x400` or `0x4000`; **not** `0x200` |
| Starfield | DAT2 Light Type `1` or `2`; not the flags word |

`Shadow Spotlight` implies a cone even without `0x200` (the normal vanilla
Skyrim case). FO4's `NonShadow Spotlight` retains its cone but receives the
same full-scene RT visibility as every other emitter. These mappings follow
xEdit's [TES5](https://raw.githubusercontent.com/TES5Edit/TES5Edit/dev-4.1.6/Core/wbDefinitionsTES5.pas),
[FO4](https://raw.githubusercontent.com/TES5Edit/TES5Edit/dev-4.1.6/Core/wbDefinitionsFO4.pas),
and [FO76](https://raw.githubusercontent.com/TES5Edit/TES5Edit/dev-4.1.6/Core/wbDefinitionsFO76.pas)
LIGH definitions, rather than transferring raw bit positions between games.
`crates/plugin/examples/spot_light_census.rs` reports authored placements,
flags, FOV and rotated axes without constructing a renderer.

## GPU light ABI

`GpuLight.params` has one meaning in every pass:

| Lane | Meaning |
| --- | --- |
| x | Legacy falloff exponent |
| y | Luminous source radius in Bethesda world units |
| z | Explicit visibility-mask bits encoded as an exact `f32` integer |
| w | `AttenuationModel` discriminant encoded as `f32` |

`GpuRayBudget` occupies scene descriptor set 1, binding 11. Its first word is
the atomic glass-ray counter; the remaining words are immutable frame limits.

## Validation gates

```bash
cargo test -p byroredux-core lighting::tests --lib
cargo test -p byroredux-renderer --lib
cargo test -p byroredux --no-run
scripts/check-shader-artifacts.sh
```

The hardware-gated `cornell_rt_oracle` tests include a bone-rotated actor with
a model-space normal map: directional response, rigid/actor point-light parity,
and an actor shadow on a second surface. The spotlight variants additionally
exercise FO4's translation boundary, test illuminated cone-axis and dark
off-cone controls, rigid/actor parity, and an actor-cast spotlight shadow
against its illuminated open control. For installed content,
`scripts/lighting-game-matrix.py` captures membership, light/mask diagnostics,
and paired NPC direct-light images before/after emitter dimming. Run it under
`xvfb-run -a` when headless. It freezes simulation without the benchmark camera
lock; image review is required even if membership passes.
Use `--actor-yaw`, `--actor-rise`, and `--actor-distance` to select an open
viewpoint when nearby furniture or walls block the default camera. For example,
Starfield `CityNeonReliantMedical` exposes the two waiting-room actors with
`--actors 3 --actor-yaw 90 --actor-rise 90 --actor-distance 120`.
The harness rejects geometry-admission-limited scenes even when every *emitted*
instance has a TLAS entry; that counter cannot certify omitted geometry.

The `l2-cache-pressure` oracle warms 64 unused mesh handles before the ordinary
L2 scene. Its hardware test requires admission pressure, actual unused-entry
eviction, missing-BLAS recovery, complete active TLAS membership and the same
blocked/unblocked pixel values under a 4 KiB residency budget. This is distinct
from the unresolved one-byte exhaustion test, which cannot hold the active set.

The `l2-cache-pressure-large` oracle retains a larger unused mesh **below** its
budget, then retires the two tiny required BLAS while keeping their source
geometry. Its budget is 1.5 times the measured unused BLAS size, independent
of driver-specific allocation sizes. Recovery must restore both casters and
the original shadow pixels without evicting the unused entry. This catches
the old cache-wide mean falsely rejecting an affordable required set.

The L3/L4 fog gate uses both right- and left-side sources, each with a matched
open/partition pair. The lit control must stay unchanged, the broad shadow must
remain below 8% of its own open control, and wall-edge leakage below 18% of the
lit side. Equal left/right brightness is not an invariant: the source is
off-centre, scattering is directional, and even the canonical `Homogeneous`
profile includes low-contrast density noise. Moving only the source cannot
mirror that noise field, either. No production fog or leakage limit was changed
to establish the two-sided gate.

The visual reference is a small enclosed room containing a candle, opaque
clutter, an actor proxy, cutout foliage, and a glass pane. Captures should pin
near/far illumination ratios, blocker-category behavior, penumbra width, GPU
time, selected quality tier, and temporal settling after a camera cut.
