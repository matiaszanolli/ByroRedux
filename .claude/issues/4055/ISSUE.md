# #4055 — EXAL ground cover Phase 2: blade geometry and wind

**State**: OPEN · **Labels**: enhancement renderer terrain-exterior shaders 

Split from #3807 (item 3). Design:
[`exal-groundcover.md`](../blob/main/docs/engine/exal-groundcover.md) §4, §8.

**Blocked by #4054** (Phase 1 scatter) — there is nothing to draw until
accepted candidate points exist.

## Scope

- **Blade geometry generated in the vertex shader from the seed word.** No blade
  mesh, no per-blade vertex data. A quadratic Bezier ribbon: base point, a
  control point displaced by the bend, a tip. `gl_VertexIndex` selects segment
  and side; height, width, bend stiffness, twist and colour jitter all derive
  from the seed. Segment count comes from the LOD tier, so the same shader emits
  a 3-segment near blade and a 1-segment far blade branching only on a per-chunk
  constant.
- **Wind** (§8) — sample a 2D flow-noise field at the blade base, advected along
  `WindField::direction` at `speed`, bending the Bezier control point. Because
  neighbouring blades sample a *continuous* field at nearby points they bend
  together: travelling gust waves across a meadow rather than per-blade jitter,
  which is most of what sells grass as alive. A per-blade phase offset from the
  seed keeps the response out of lockstep.
- **Near tier only.** The LOD chain is its own phase.

## Already wired

`WindField` is live: `weather_system` rewrites it on every weather change
([`systems/weather.rs:988`](../blob/main/byroredux/src/systems/weather.rs#L988)),
so wind tracks a storm rolling in with nothing further to do here. It is
sanitised at the translate boundary, so the shader can assume finite values and
a unit direction.

## Done when

Blades render over real terrain in the near tier and respond coherently to a
weather change, with no per-blade CPU work and no growth in `GpuInstance` count.

