# Safety Audit — 2026-05-03

**Auditor**: Claude Opus 4.7 (1M context)
**Baseline**: `docs/audits/AUDIT_SAFETY_2026-04-23.md` (10 days ago) — SAFE-01..21.
**Scope**: Delta audit since 04-23. Major changes: M29 Phase 1+2 (skin compute pipeline), R1 MaterialTable refactor (272 B GpuMaterial, per-frame dedup), M-NORMALS (Vertex layout 84→100 B), M41.0 NPC spawn pipeline, RenderLayer ECS component, plus ~50 fixes since the prior audit. The 04-23 baseline left 7 open items (SAFE-09 / 11 / 12 / 14 / 15 / 19 / 20 / 21); none are visibly closed in the current tree.
**Open-issue baseline**: `/tmp/audit/safety/issues.json` — 51 OPEN, 2 carry the `safety` label (#91 SAFE-11, #579 SAFE-20).

---

## Summary

| Severity | NEW | Carried Open | Fixed since 2026-04-23 |
|----------|-----|--------------|------------------------|
| CRITICAL | 0   | 0            | — |
| HIGH     | 0   | 0            | — |
| MEDIUM   | 1   | 3 (SAFE-09 / 11 / 20) | 0 |
| LOW      | 0   | 4 (SAFE-12 / 14 / 15 / 19 / 21) | 0 |

**Headline**: One new MEDIUM finding tied to the R1 MaterialTable closeout. Otherwise the safety surface is stable — the 04-23 backlog is unchanged. The new compute pipelines that landed since (skin_compute, R1 MaterialBuffer upload) ship with proper SAFETY comments + layout-invariant tests. The two-day M-NORMALS arc (#783, #786, #787, #795, #796) added zero new unsafe blocks: the parser-side decode is plain Rust, the importer-side wire-up is plain Rust, the shader-side gate flip is GLSL.

### What's confirmed CLOSED since 2026-04-23

| Issue | Closed by | Verification |
|---|---|---|
| `#681` (skin compute output buffer flags) | `b99ae91` | `skin_compute.rs:320-323` — STORAGE_BUFFER + SHADER_DEVICE_ADDRESS + ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR; VERTEX_BUFFER deliberately deferred until M29.3 (raster reads from skinned output) lands. |
| `#732` (LIFE-N1 — flush_pending_destroys on shutdown) | `96d5fbd` | `context/mod.rs:1336-1376` (`drain_deferred_destroy`). |
| `#643` (MEM-2-1 — SkinSlot LRU evict) | `4681784` | `skin_compute.rs:113-120` (`should_evict_skin_slot`). |
| `#662` (PIPE-4 — skin push constants 16→12 B) | `6d4e063` | `skin_compute.rs:514` test pins 12 B. |
| `#674` (DEN-4 — SVGF α knob) | `b7caec6` | `next_svgf_temporal_alpha` state machine. |
| `#653` (SVGF/TAA post-dispatch dst-stage) | `8f004eb` | FRAGMENT \| COMPUTE in barrier dst_stage_mask. |

### What's still open from 2026-04-23

| ID | Severity | Issue | Status (2026-05-03 re-check) |
|---|---|---|---|
| `SAFE-09` | MEDIUM | N>2 multi-query has no TypeId ordering | Still open. `query_2_mut` + `resource_2_mut` enforce; N>=3 adhoc paths in `byroredux/src/systems.rs` still source-ordered. Benign under sequential scheduler. |
| `SAFE-11` (#91) | MEDIUM | Pipeline cache from CWD | Still open. `helpers.rs:419` reads `pipeline_cache.bin` from process cwd. |
| `SAFE-12` | LOW | Swapchain raw pointer to stack-local queue family indices | Still open. |
| `SAFE-14` | LOW | Poisoned lock cascade | Still open — every `.expect("...poisoned")` in `world.rs` / `context/mod.rs` re-verified. |
| `SAFE-15` | LOW | Depth image leak on post-allocate error | Still open — `helpers.rs::create_depth_resources` not wrapped in StagingGuard-style RAII. |
| `SAFE-19` | LOW | `update_rgba` rewrites all per-frame descriptor sets simultaneously | Still open. |
| `SAFE-20` (#579) | MEDIUM | SAFETY comment coverage gap | **Worse**: caustic.rs / taa.rs / ssao.rs / svgf.rs / composite.rs still have **0** `// SAFETY` comments each. Module-level grep counts unchanged from 04-23. |
| `SAFE-21` | LOW | `acceleration.rs:694` lifetime transmute SAFETY comment compressed | Still open — SAFETY comment unchanged since 04-23. |

### What's new since 2026-04-23 (delta surface)

| Code path | Landed in | Safety state |
|---|---|---|
| `crates/renderer/src/vulkan/skin_compute.rs` | M29 (`de1ea1f`, `1ae235b`) | 3 unsafe / 1 SAFETY. Push constants pinned at 12 B (test). VERTEX_STRIDE_FLOATS pinned to 25 against `Vertex` size 100 B (test). Output buffer flags correct per #681. |
| `crates/renderer/src/vulkan/material.rs` (R1) | `aa48d64`+5 phases | 1 unsafe / 1 SAFETY. 272 B size pinned (test). All-scalar layout (no vec3). 9 dedup-correctness tests. |
| `Vertex` 84→100 B (`tangent: [f32; 4]`) | `91e9011` (#783), `5dde345` (#786), `b63ab0c` (#795/#796) | 0 new unsafe blocks; pure parser/importer/shader plumbing. Layout pinned at `vertex.rs:271` (`size_of::<Vertex>() == 100`) and at `skin_compute.rs:484` (matches stride). |
| `RenderLayer` ECS component | `088696e` | 0 unsafe blocks. Pure ECS storage + bias-table function. 6 unit tests pin the ladder. |

---

## Findings

### MEDIUM

#### SAFE-22 — `MaterialTable::intern` returns out-of-bounds `material_id` when unique-material count exceeds `MAX_MATERIALS`

- **Severity**: MEDIUM
- **Dimension**: R1 Material Table Safety / GPU Memory
- **Locations**:
  - `crates/renderer/src/vulkan/material.rs:326-334` — `intern()` returns `self.materials.len() as u32` with no upper bound
  - `crates/renderer/src/vulkan/scene_buffer.rs:63` — `pub const MAX_MATERIALS: usize = 4096;`
  - `crates/renderer/src/vulkan/scene_buffer.rs:404-405` — `MaterialBuffer` SSBO sized for exactly `MAX_MATERIALS` entries (`4096 × 272 = 1,114,112 B`)
  - `crates/renderer/src/vulkan/scene_buffer.rs:975-983` — upload silently caps at `MAX_MATERIALS` and **prints a misleading warning**: *"instances pointing past the cap silently default to material 0"*. There is **no logic** that maps over-cap ids to 0 — the warn message describes a behaviour that isn't implemented.
- **Status**: NEW
- **Description**: When a frame interns more than 4096 unique `GpuMaterial` records, `intern()` returns `material_id`s of 4096 and above. `DrawCommand.material_id` carries those ids unmodified into `GpuInstance.material_id` and into the per-frame instance SSBO. The fragment shader reads `materials[instance.materialId]` from a `MaterialBuffer` SSBO sized for exactly 4096 entries — `material_id == 5000` computes byte offset `5000 × 272 = 1,360,000`, well past the buffer's 1,114,112 B end.

  Per the std430 SSBO spec, out-of-bounds reads are implementation-defined: AMD typically returns zeros, NVIDIA returns last-valid-page contents, Intel may trigger DEVICE_LOST. None of these are "silently default to material 0" as the warn message claims.
- **Evidence**:
  ```rust
  // material.rs:326-334 — current intern
  pub fn intern(&mut self, material: GpuMaterial) -> u32 {
      if let Some(&id) = self.index.get(&material) {
          return id;
      }
      let id = self.materials.len() as u32;     // ← no cap
      self.materials.push(material);
      self.index.insert(material, id);
      id
  }
  ```
  ```rust
  // scene_buffer.rs:975-983 — upload claims a behaviour that isn't implemented
  let count = materials.len().min(MAX_MATERIALS);
  if materials.len() > MAX_MATERIALS {
      log::warn!(
          "Material table overflow: {} materials submitted, capped at {} \
           — instances pointing past the cap silently default to material 0",
          materials.len(),
          MAX_MATERIALS,
      );
  }
  ```
  Search for the supposed "default to 0" remap returns zero hits (`grep -rn 'material_id.*MAX\|material_id.*cap\|material_id.*saturate' crates/`).
- **Impact**: Low reachability today. Per the doc-comment at `scene_buffer.rs:60-62`, real interior cells dedup to 50–200 unique materials and a 3×3 exterior grid lands at 300–600 — well under the 4096 cap. **Reachable on**:
  - Modded content with high material variation (a single cell with 4096+ unique combinations of `texture_index × normal_map × specular × …`)
  - Future Starfield / FO76 large-exterior cells (CK uses per-segment SubIndex materials that can multiply quickly)
  - Synthetic stress tests / benchmarks
  - **Cumulative leak across the frame**: `intern()` is called per-DrawCommand, and `HashMap<GpuMaterial, u32>::get` can return a stale id from a hash collision if the hasher is unstable (`std::collections::HashMap` is `RandomState` by default — irrelevant here because `Hash` is byte-stable, but a future hasher swap could surface a different overflow vector).
  - Per `_audit-severity.md` "SSBO index mismatch | CRITICAL" — deferred to MEDIUM here only because reachability is genuinely low and the GPU response is implementation-defined-not-crash on the current dev hardware (RTX 4070 Ti, NVIDIA driver returns zeros).
- **Suggested Fix** (one-line change at `material.rs:330`):
  ```rust
  pub fn intern(&mut self, material: GpuMaterial) -> u32 {
      if let Some(&id) = self.index.get(&material) {
          return id;
      }
      // Cap at MAX_MATERIALS - 1; over-cap interns reuse the last
      // valid id (or sentinel 0). The upload at scene_buffer.rs:975
      // already truncates the buffer at MAX_MATERIALS — this aligns
      // the id space with the buffer size so no instance reads OOB.
      if self.materials.len() >= MAX_MATERIALS_USIZE {
          static OVERFLOW_LOGGED: std::sync::Once = std::sync::Once::new();
          OVERFLOW_LOGGED.call_once(|| {
              log::warn!(
                  "MaterialTable: unique-material count exceeded MAX_MATERIALS ({}); \
                   over-cap entries will share material 0",
                  MAX_MATERIALS,
              );
          });
          return 0;
      }
      let id = self.materials.len() as u32;
      self.materials.push(material);
      self.index.insert(material, id);
      id
  }
  ```
  This makes the warn message at scene_buffer.rs:978-979 truthful (over-cap *do* default to material 0 once both fixes land). Add a regression test pinning that `intern` returns 0 for the 4097th unique material.
- **Related**:
  - The R1 MaterialBuffer upload's misleading warn message is the secondary fix — once `intern()` caps, the message can stay as-is.
  - `feedback_no_guessing.md` applies in reverse: the `cap` constant (4096) was chosen because real content lands well under it (per `scene_buffer.rs:60-62` doc-comment). That's still the right cap; only the overflow handling needs hardening.
- **Completeness Checks**:
  - [ ] **UNSAFE**: N/A — pure Rust intern logic
  - [ ] **SIBLING**: Verify `MAX_INSTANCES`, `MAX_INDIRECT_DRAWS`, `MAX_TERRAIN_TILES`, `MAX_TOTAL_BONES` upload paths for the same warn-without-impl pattern. `MAX_TOTAL_BONES` already has the proper `Once`-gated overflow guard at `render.rs:240-247`; check the others.
  - [ ] **TESTS**: Add `intern_overflow_returns_material_zero` in `material.rs::tests`.

---

## Verified Working — No New Gaps

### Drop ordering (`VulkanContext::Drop` at `context/mod.rs:1528-1670`)

The reverse-creation chain has been kept in lockstep with every new pipeline that landed since 04-23:

```
device_wait_idle
  → frame_sync.destroy
  → transfer fence + transfer pool + command pool
  → main framebuffers
  → texture_registry / scene_buffers
  → SkinSlots (via skin_compute.destroy_slot)         ← M29 Phase 1
  → accel_manager (skinned BLAS first, then static)   ← acceleration.rs
  → cluster_cull
  → skin_compute (after slots)                         ← M29 Phase 1
  → ssao / composite / caustic / svgf / taa / gbuffer ← in dependency order
  → depth resources (helper-enforced view → image → free order, per #33/R-10)
  → render-pass pipelines
  → mesh_registry (after pipelines, per spec)
  → pipeline_cache save + destroy
  → render_pass + swapchain
  → allocator (Arc::try_unwrap with #665 / LIFE-L1 fallback)
  → device
  → debug_utils / surface / instance / entry
```

Every M29 / R1 / TAA addition is wired correctly. SkinSlots are explicitly destroyed before `SkinComputePipeline` per the descriptor-pool ownership comment at `context/mod.rs:1557-1564`.

### Layout invariants (hard-pinned by tests)

- `Vertex` (`crates/renderer/src/vertex.rs:271`): `size_of::<Vertex>() == 100` — pins post-#783 layout. Field offsets pinned at `:276-285`.
- `VERTEX_STRIDE_FLOATS` (`skin_compute.rs:484`): cross-checks against `Vertex` size at compile time.
- `GpuMaterial` (`material.rs:359`): `size_of::<GpuMaterial>() == 272` — pins R1 std430 layout.
- `SkinPushConstants` (`skin_compute.rs:514`): `size == 12 B` — well under 128 B Vulkan-guaranteed minimum.
- `gpu_instance_size_is_112_bytes` (per session-25 closeout) — pins R1 Phase 6 GpuInstance collapse.
- `ui.vert` build-time grep guard (`scene_buffer.rs:1631-1632`): rejects `materials[inst.…]` reads to prevent #776 / #785 regression.

### SPIR-V reflection coverage

`validate_set_layout` consumes shader SPIR-V at descriptor-set-layout creation time and panics on Rust-vs-shader binding drift. Every compute pipeline + composite pipeline is covered:

| Pipeline | Site |
|---|---|
| `cluster_cull` | `compute.rs:139` |
| `ssao` | `ssao.rs:264` |
| `svgf` | `svgf.rs:329` |
| `caustic` | `caustic.rs:298` |
| `skin_compute` | `skin_compute.rs:177` |
| `taa` | `taa.rs:245` |
| `composite` | `composite.rs:488` |

Coverage is complete for all post-April descriptor-set-bearing pipelines. `gbuffer.rs` has no descriptor set of its own (consumers — composite, svgf — bind gbuffer images via their own sets), so it doesn't need reflection.

### TAA / SVGF history image layout

- TAA history images (`taa.rs:342-363`) created in UNDEFINED, transitioned UNDEFINED → GENERAL once at init (`taa.rs:518-549`), and then stay in GENERAL for the lifetime of the slot. Storage write + sampled read coexist legally.
- `should_force_history_reset` (consumed at `taa.rs:580`) gates first-frame + post-resize handling so no NaN reads / no garbage history accumulation.
- SVGF mirror — same layout discipline.

### Caustic pipeline atomic safety

- Format `R32_UINT` (`caustic.rs:57`) — required for `imageAtomicAdd`.
- CLEAR-before-COMPUTE invariant (`caustic.rs:758-800`):
  1. pre-CLEAR barrier (compute write → transfer write)
  2. `cmd_clear_color_image`
  3. post-CLEAR barrier (transfer write → shader read|write)
  4. dispatch
  Order verified at `caustic.rs:776-800,803`.
- Layout stays in `GENERAL` throughout (line 566, 668, 746-747).

### Bone palette overflow guard (`render.rs:240-247`)

`Once`-gated `log::warn!` fires on first overflow. Per #771 this matches nifly's bone-palette upload contract; per `feedback_no_guessing.md` the formula has been validated against authoritative source. No silent truncation since #771 closed.

### FFI (cxx-bridge) — unchanged

The cxx surface is still trivially small: one struct, two functions, no raw pointer exchange across the boundary. Unchanged since 2026-04-05.

---

## Priority Action Items

1. **SAFE-22** (this audit, MEDIUM) — One-line cap on `MaterialTable::intern` returning 0 past `MAX_MATERIALS`. Cheap; makes the warn message at `scene_buffer.rs:978-979` truthful.
2. **SAFE-20 / #579** (carryover, MEDIUM) — One-line SAFETY comment sweep across `caustic.rs`, `taa.rs`, `ssao.rs`, `svgf.rs`, `composite.rs`, `gbuffer.rs`. Module counts unchanged in 10 days. The forcing function on future edits is the value.
3. **SAFE-09** (carryover, MEDIUM) — Sketch `query_N_mut` API or runtime lock-order validator before any parallel scheduler work.
4. **SAFE-11 / #91** (carryover, MEDIUM) — Move `pipeline_cache.bin` to `dirs::config_dir()`. Cheap.

The remaining LOW items (SAFE-12 / 14 / 15 / 19 / 21) are unchanged in shape and unchanged in priority — no live bug, hardening only.

---

## Methodology Notes

- Issue dedup against the 200-issue snapshot in `/tmp/audit/safety/issues.json`.
- Each SAFE-NN from 2026-04-23 re-verified by direct grep against current tree before classifying it open/closed.
- Module-level unsafe-vs-SAFETY counts driven from `grep -cE '^[[:space:]]*unsafe '` and `grep -c '// SAFETY'`.
- The `MaterialTable::intern` overflow finding (SAFE-22) was caught by reading the warn-message claim against the actual implementation — the message documented behaviour that wasn't implemented.
- Sub-agent dispatches deliberately not used; per the 04-27 / 05-01 audit methodology notes, sub-agents reliably stall on this size of audit. Direct main-context delta audit produces a deterministic deliverable.

---

*Generated by `/audit-safety` on 2026-05-03. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_SAFETY_2026-05-03.md`.*
