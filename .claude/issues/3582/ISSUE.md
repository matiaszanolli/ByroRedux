# CONC-D2-2026-08-30-01: `caustic_splat.comp` reads the skinned-vertex SSBO from COMPUTE; the skin chain publishes only to AS_BUILD | FRAGMENT

**Issue**: #3582
**Labels**: bug, renderer, high, sync, shaders, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D2-2026-08-30-01 (HIGH, D2 · Compute -> AS -> Fragment Chains).

**Regression of the #2403 class**, introduced by `9bf7d024` (2026-08-15) — *one day after* `docs/audits/AUDIT_CONCURRENCY_2026-08-14.md` verified this exact barrier clean.

## Description

`record_skinned_blas_refit` publishes the `skin_vertices.comp` output with a single global `memory_barrier` whose destination scope is `ACCELERATION_STRUCTURE_BUILD_KHR | FRAGMENT_SHADER` / `SHADER_READ`. #2403 widened that mask to `FRAGMENT_SHADER` precisely because `include/ray_hit.glsl` (reached from `triangle.frag` / `water.frag`) dereferences `GpuInstance.skinnedVertexAddress`.

The 2026-08-14 audit re-traced the include graph and explicitly cleared the missing `COMPUTE_SHADER` bit:

> "`caustic_splat.comp` and `volumetrics_inject.comp` include `include/shadow_common.glsl`, which touches no geometry buffer, so the missing `COMPUTE_SHADER` dst bit on that barrier is **not** a gap today."
> — `docs/audits/AUDIT_CONCURRENCY_2026-08-14.md:496-501`

**That is no longer true.** `caustic_splat.comp` does **not** go through `ray_hit.glsl` — `9bf7d024` gave it its own inline `SkinnedVertexRef` block and its own `getCausticHitTriWorldPositions()` that dereferences `hit.skinnedVertexAddress` directly. So an include-graph trace still comes back clean while the deref exists.

## Every barrier between the two dispatches was walked in command order; none makes the write visible to `COMPUTE_SHADER` / `SHADER_READ`

| Site | src | dst | verdict |
|---|---|---|---|
| `skinned_blas_refit.rs:480` | `COMPUTE`/`SHADER_WRITE` | `AS_BUILD\|FRAGMENT`/`SHADER_READ` | correct availability, **wrong visibility scope** |
| `skinned_blas_refit.rs:672`, `blas_skinned.rs:695` | `AS_WRITE` | `AS_WRITE\|AS_READ` | src does not cover the compute `SHADER_WRITE` |
| `draw.rs:2688` | `AS_WRITE` | `FRAGMENT\|COMPUTE`/`AS_READ` | has the `COMPUTE` dst bit, but src is `AS_WRITE` |
| `draw.rs:2757`, `draw.rs:3612` | `HOST` | — | n/a |
| `draw.rs:2775` (cluster-cull trailing) | `COMPUTE`/`SHADER_WRITE` | `FRAGMENT` only | the same accidental-cover barrier #2403 called out; still no `COMPUTE` dst bit |
| `context/geometry_pass.rs` | — | — | **zero barriers**; the render pass's outgoing subpass dependency (`context/helpers.rs:274-312`) is `COLOR_ATTACHMENT_OUTPUT\|EARLY/LATE_FRAGMENT_TESTS`, which cannot cover a compute SSBO write |
| `caustic.rs:889` (inside `CausticPipeline::dispatch`) | `HOST`/`HOST_WRITE` | — | all other barriers there are image barriers on the accumulator |
| `post_passes.rs:593` | `COMPUTE`/`SHADER_WRITE` | `COMPUTE`/`SHADER_READ` | **would** cover it — but it lives in `record_volumetrics_pass`, which `record_post_passes` calls **after** `record_caustic_splat_pass` (`post_passes.rs:242-243`), and it is gated behind the volumetrics TLAS/cluster/geometry triple |

Execution ordering *is* established by chaining (skin compute -> AS_BUILD via `:480`, AS_BUILD -> COMPUTE via `draw.rs:2688`), so this is a pure **memory-visibility** gap, not an execution-order gap — the same shape as #2403.

## Evidence

`crates/renderer/shaders/caustic_splat.comp` (the new COMPUTE consumer):
```glsl
158  layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer SkinnedVertexRef {
159      float data[];
160  };
...
205      if (hit.boneOffset != 0u && hit.skinnedVertexAddress != 0ul) {
206          SkinnedVertexRef skinned = SkinnedVertexRef(hit.skinnedVertexAddress);
211          w0 = vec3(skinned.data[p0], skinned.data[p0 + 1u], skinned.data[p0 + 2u]);
```

`crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (the publish barrier, unchanged since #2403):
```rust
480    memory_barrier(
481        &self.device, cmd,
483        vk::PipelineStageFlags::COMPUTE_SHADER,
484        vk::AccessFlags::SHADER_WRITE,
485        vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR
486            | vk::PipelineStageFlags::FRAGMENT_SHADER,
487        vk::AccessFlags::SHADER_READ,
488    );
```

Provenance:
```
$ git log -S "SkinnedVertexRef" --oneline -- crates/renderer/shaders/caustic_splat.comp
9bf7d024 refactor: enhance glass IOR handling and ray budget telemetry   (2026-08-15)
```

## Trigger Conditions

Any frame in which (a) at least one skinned entity dispatched `skin_vertices.comp`, (b) `accel.tlas_handle(frame)` is `Some` so `record_caustic_splat_pass` actually dispatches, and (c) a caustic ray-query committed hit lands on that skinned actor's geometry. Concretely: an NPC standing behind glass or near water in a cell where the caustic pass is live. No CPU-side timing window — a pure device-side RAW inside one command buffer.

## Verification Path — REQUIRED BEFORE ANY FIX LANDS

Per the project's speculative-Vulkan-fix rule, **no GPU run was performed in this sweep**; this is a source-level conclusion. Confirm first:

`BYRO_VALIDATION=1` **release** build with `VK_LAYER_KHRONOS_validation` synchronization validation, on a cell with a skinned actor + a caustic source (e.g. FO4 `DmndDugoutInn01`, or any Skyrim interior with water).

Expected concrete signal: **`SYNC-HAZARD-READ-AFTER-WRITE`** naming the `SkinSlot::output_buffer` `VkBuffer` at the `vkCmdDispatch` inside `CausticPipeline::dispatch` (`caustic.rs:1058`), with `prior_access = SYNC_COMPUTE_SHADER_SHADER_STORAGE_WRITE` from the `skin_vertices.comp` dispatch. RenderDoc alternative: the same pixel's caustic contribution computed from a previous pose's triangle positions (caustic ghosting trailing a moving NPC). `cargo test` cannot see this.

## Impact

The caustic splat can compute refracted-light deposits from a skinned actor's **previous-frame** triangle positions (or partially-written positions) on drivers with incoherent compute L1. Visible class: caustic pools that lag/ghost behind a moving NPC near water or glass. Blast radius is bounded — the accumulator is additive and screen-space; it cannot corrupt the AS or cause device loss.

What makes this HIGH rather than MEDIUM is the upgrade path: **the same barrier is the only publish for a buffer that already has three consumers**, and each new consumer added without re-auditing this mask silently re-opens the hole. The include-graph trace demonstrably cannot catch an inline deref.

## Suggested Fix

Add `vk::PipelineStageFlags::COMPUTE_SHADER` to the dst stage mask at `skinned_blas_refit.rs:485-486`, exactly as #2403 added `FRAGMENT_SHADER`. Widening a dst stage mask is purely **additive** — it can only add execution/memory dependencies, never remove one — so it is the lowest-risk class of change, but confirm with the sync-val signal above **before and after**.

Also add a source-assert test (mirroring `skin_dispatch_ran_ordering_tests`) that fails if any `.comp` under `shaders/` mentions `skinnedVertexAddress` while the barrier's dst mask lacks `COMPUTE_SHADER`.

## Completeness Checks
- [ ] **VALIDATION**: The `SYNC-HAZARD-READ-AFTER-WRITE` signal is observed before the change and absent after it — do not land on reasoning alone
- [ ] **SIBLING**: Every `.comp` under `crates/renderer/shaders/` grepped for `skinnedVertexAddress` / inline geometry-buffer derefs, not just `caustic_splat.comp`
- [ ] **TESTS**: A source-assert regression test pins the dst-mask <-> consumer-set relationship, since the include-graph trace cannot
