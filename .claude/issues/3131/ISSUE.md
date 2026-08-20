# PERF-D5-01: volumetrics_inject.comp runs the full combustion transport stencil on every froxel with no scene-level gate

**Issue**: #3131 — https://github.com/matiaszanolli/ByroRedux/issues/3131
**Labels**: `medium,performance,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`

---

**Severity**: MEDIUM
**Dimension**: GPU Pipeline & Pass Efficiency
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D5-01)

## Location

- `crates/renderer/shaders/volumetrics_inject.comp` — unconditional `transportCombustion` call in `main` (`:2324`), `transportCombustion`'s `hadHistory && dt > 0.0` branch (`:1730-1760`), `incomingDynamicsFromNeighbors` (`:1666-1720`), `samplePreviousTransport` (`:1392-1412`)
- CPU side: `crates/renderer/src/vulkan/volumetrics.rs` — `frame_params.fog_reference[3] = simulation_dt` (`:2036`), `requires_dispatch` (`:2453-2467`), `has_transport_emitter` (`:195`), `combustion_active_until_seconds` (`:809`)

## Description

`main()` calls `transportCombustion` for **every** froxel, unconditionally. Inside, once temporal history is valid (steady state), the RK2 advection block runs, and its first act is:

```glsl
if (combustionActivity(probeChemistry, probeOptical) < 0.08) {
    ... incomingDynamicsFromNeighbors(worldPos, stepX, stepY, stepZ, ...)
}
```

`incomingDynamicsFromNeighbors` loops six neighbours, each calling `samplePreviousTransport` = **three trilinear `texture()` fetches on RGBA16F 3-D images**.

**So the 18-fetch neighbour gather fires precisely on the froxels with *no* combustion activity** — which, in a scene with no fire at all, is 100% of them. The expensive branch is gated on *low* activity, so the quiet majority pays it. Adding the destination probe and the midpoint/source probes, a quiet froxel pays ~21 3-D texture fetches plus 3 `imageStore`s for a field that is uniformly zero.

**There is no CPU-supplied "combustion is active in this scene" signal in `VolumetricsParams` — even though the CPU already computes exactly that predicate.** `requires_dispatch` evaluates `has_transport_emitter(fog_volumes)` and maintains `combustion_active_until_seconds`. The pass itself is correctly gated (`has_global_medium || !fog_volumes.is_empty() || linger`), but `has_global_medium` is true for any cell with authored fog — i.e. the common case — so the dispatch runs and the combustion stencil runs with it.

## Evidence

Confirmed at HEAD:
```
crates/renderer/shaders/volumetrics_inject.comp:2324:    transportCombustion(          # unconditional, in main()
crates/renderer/shaders/volumetrics_inject.comp:1753:        if (combustionActivity(probeChemistry, probeOptical) < 0.08) {
crates/renderer/src/vulkan/volumetrics.rs:195:fn has_transport_emitter(volumes: &[GpuFogVolume]) -> bool {
crates/renderer/src/vulkan/volumetrics.rs:2036:        frame_params.fog_reference[3] = simulation_dt;
crates/renderer/src/vulkan/volumetrics.rs:2460:        if has_transport_emitter(fog_volumes) {
```

At the default `froxel_xy_divisor: 4` / `froxel_z_slices: 64`, a 1920×1080 render extent gives 480×270×64 = **8 294 400** froxels. 21 trilinear RGBA16F 3-D fetches each is ~1.7×10⁸ fetches/frame, against three 8-B `imageStore`s per froxel (~199 MB of writes/frame) — all for `chemistry == 0`.

`carriesCombustion(...)` rejects each neighbour **after** its three samples have already been issued (`:1686-1693`), so the early-out saves the TLAS query but **not the bandwidth**.

## Impact

Wasted 3-D texture bandwidth and L2 pressure on every frame of every fog-bearing cell in every game, scaling linearly with render resolution (4K = 33.2 M froxels, 4× the above). It is not a correctness problem and it does not compound, but it is paid in the frames the project most cares about (dense exteriors), and **the gate that would remove it already exists on the CPU**.

## Confidence / limits

The **structure** above is read directly from the shipped GLSL and is not in doubt. The **magnitude** is arithmetic from the froxel count and fetch count — the `volumetrics` `gpu_timers` bracket was **not** read (runtime-only; no engine instance was spawned, per `feedback_no_parallel_engine_launch.md`). Quantify with `bench-stats --break-down` before and after any fix rather than trusting the estimate.

## Suggested Fix

**One line on the CPU.** In `VolumetricsPipeline::dispatch`, set `frame_params.fog_reference[3]` (the shader's `simulationDt`) to `0.0` when:

```rust
!has_transport_emitter(fog_volumes) && now > self.combustion_active_until_seconds
```

`transportCombustion` already gates its entire RK2 block on `dt > 0.0` (`:1750`), so the neighbour gather, the midpoint probe, the source probe and the differential all fall away **with no shader change**. Pin it with a unit test on the predicate.

## Related

- The VRAM half of the same subsystem observation (the six-volume froxel ledger error) — a lazily-created combustion sub-group would additionally return ~400 MB at 1080p to scenes that never see fire
- #2242 (`REN-D16-04`, CLOSED)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — other per-froxel stencils in `volumetrics_inject.comp` / `volumetrics_integrate.comp` with no scene-level gate
- [ ] **TESTS**: A regression test pins this specific fix (unit-test the CPU predicate: no transport emitter + linger expired ⇒ `simulation_dt == 0.0`)
