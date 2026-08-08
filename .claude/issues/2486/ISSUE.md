# D5-01: Half the per-frame scratch cluster is excluded from the peak-shrink policy

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2486
**Finding ID**: D5-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 5 — Memory/Lifecycle
**Location**: `crates/renderer/src/vulkan/context/draw.rs:3169-3183` (`VulkanContext::draw_frame`, end-of-frame scratch restore)
**Status**: NEW

## Description
`draw_frame` restores five per-frame scratch containers to `self`, but only two of them (`gpu_instances_scratch`, `batches_scratch`) get the `shrink_scratch_if_oversized(working_set, floor=512)` treatment. `previous_models_scratch` (a `Vec<GpuPreviousModel>`) is restored on the immediately preceding line and never shrunk, and the two rigid-history `FxHashMap`s (`previous_rigid_models` / `current_rigid_models_scratch`) are `mem::swap`ped without any capacity policy at all. All of them are `clear()`-then-`reserve(draw_commands.len())`, so their capacity is monotonically the session high-water mark, not the working set.

## Evidence
```rust
self.gpu_instances_scratch = gpu_instances;
self.previous_models_scratch = previous_models;   // <- restored, never shrunk
self.batches_scratch = batches;
super::super::acceleration::shrink_scratch_if_oversized(&mut self.gpu_instances_scratch, working_instances, 512);
super::super::acceleration::shrink_scratch_if_oversized(&mut self.batches_scratch, working_batches, 512);
```
and at `draw.rs:3125-3127`:
```rust
std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models);
current_rigid_models.clear();
self.current_rigid_models_scratch = current_rigid_models;   // no shrink
```
The struct doc at `context/mod.rs:1092-1102` describes the whole group as one "amortization pattern" cluster, which is why the omission reads as drift rather than intent — the shrink half of the policy was only wired to two members.

## Impact
Host RAM only — no GPU allocation, no leak, no per-frame growth. Bound is `MAX_INSTANCES = 0x40000` (262144, `scene_buffer/constants.rs:135`): a single large-exterior peak can pin ~16 MB in `previous_models_scratch` plus ~20 MB per rigid-history map, and that residency survives the walk into a small interior for the rest of the session. It is exactly the same pressure `#243`/`#496`/`#504` shrink policy exists to relieve for the other two members. Not a correctness issue.

## Related
`#243` (scratch amortization), `#496`, `#504` (shrink policy), `#2174`/D2-03 (FxHashMap swap, states allocation behaviour is "already correct" — true for churn, but does not address the high-water pin). Telemetry already surfaces all five capacities via the `ctx.scratch` command.

## Suggested Fix
Extend the existing `shrink_scratch_if_oversized` call block to `previous_models_scratch` with the same `(working_instances, 512)` arguments; for the two `FxHashMap`s add an equivalent `if map.capacity() > working * 2 { map.shrink_to(working.max(512)) }` after the swap. Purely additive, no ordering constraints.

## Completeness Checks
- [ ] **TESTS**: `ctx.scratch` telemetry confirms all five capacities track the working set, not just two of five
