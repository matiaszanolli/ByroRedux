# Safety Audit — 2026-07-16

**Scope:** `/audit-safety` full sweep — unsafe-block invariants, memory-corruption / UB,
per-frame / per-cell leaks, Vulkan spec compliance, FFI lifetimes across the cxx bridge,
R1 material layout, RT IOR/glass guards, NPC/animation spawn safety, NIFAL NaN boundary,
debug-ui teardown ordering.

**Method:** Grepped all `unsafe` in `crates/` + `byroredux/` (636 in renderer — 70 `unsafe
fn` decls + ~566 blocks, 581 `SAFETY` comments; 11 in nif, 6 in core, 1 each in plugin*/
pex/cxx-bridge, 2 in byroredux; 0 in save/audio/bgsm/spt/physics/scripting/papyrus/
sfmaterial/debug-protocol/debug-server/debug-ui/platform/ui — *plugin's hit is an English
sentence, not a code block, see below). Re-ran every regression-guard test named in the
SKILL (`rapier_release_tests`, `bone_palette_overflow_tests`, `gpu_material_*`,
`scene_descriptor_reflection_tests`, `intern_overflow_*`) plus full `cargo test` for
`byroredux-core`, `byroredux-nif`, `byroredux-renderer` (1787 tests, 0 failures). Diffed
`git log --since=2026-07-06` against the prior safety audit
(`docs/audits/AUDIT_SAFETY_2026-07-06.md`) to scope the delta: five new commits landed the
M42.3–M42.8 AI-package procedure runtimes (Wander/Travel/Follow/Escort/Guard/Patrol) —
audited those files directly since they're unreviewed by any prior safety pass. Deduped
against `gh issue list` (`/tmp/audit/issues.json`) and `docs/audits/`.

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |
| **Total NEW** | **0** |

Zero new findings. Both MEDIUM findings from the 2026-07-06 audit are now closed
(`#1904` documented every renderer FFI unsafe block; the header-string allocation gap
now has `check_header_alloc` gating `read_sized_string`/`read_short_string`). The new
M42.3–M42.8 AI-package code (`byroredux/src/systems/{wander,travel,follow,escort,guard,
patrol}.rs` + matching `crates/core/src/ecs/components/*.rs`) introduces no `unsafe`,
no unbounded allocation, and no production-path `.unwrap()`/`.expect()`/`panic!` — every
such call in those six files is confined to `#[cfg(test)]` modules. Every regression
guard the SKILL enumerates re-verified intact; one (Dimension 3's `AllocatorResource`
ordering) is now structurally *stronger* than at the last audit, and one (Dimension 10)
turned out to be checking a stale premise against the current architecture — both
detailed below.

---

## Regression Guards Verified (PASS — not findings)

- **Dimension 1 — cxx bridge is still a no-pointer placeholder.**
  `crates/cxx-bridge/src/lib.rs` exposes only `fn native_hello() -> String` (owned
  `cxx::String` return, no `*const`/`&[u8]`/`Box<…>`/Rust-reference-taking `extern
  "C++"`). Unchanged since 2026-07-06. PASS.
- **Dimension 2 — ECS cached-pointer contract (#35/#1367).**
  `StorageRef`/`StorageRefMut` (`crates/core/src/ecs/query.rs:58-144`) and
  `ComponentRef::deref` (`query.rs:282-291`) still cache a `*const`/`*mut` resolved once
  in `new()`, each deref carrying a SAFETY comment tying validity to the pinned lock
  guard; `&mut *self.storage` (`query.rs:143`) still gated behind `&mut self`. No guard
  drops before its pointer. PASS.
- **Dimension 2 — repr(C) GPU-struct layout.** `gpu_material_size_is_300_bytes`,
  `gpu_material_field_offsets_match_shader_contract`,
  `gpu_material_glsl_field_order_matches_rust_struct`, `material_hash_matches_
  gpu_material_field_hash`, `camera_ubo_size_matches_gpu_camera_in_every_shader` — all
  green (`cargo test -p byroredux-renderer gpu_material` / `reflect`). PASS.
- **Dimension 2 — NIF POD reads.** `read_pod_vec` (`crates/nif/src/stream.rs:350`) and
  the header mirror `read_pod_vec_from_cursor` (`header.rs:360`) still keep the
  `count.checked_mul(size_of)` overflow guard and route through `check_alloc`; the
  sealed `AnyBitPattern` trait still blocks `read_pod_vec::<bool>`. PASS.
- **Dimension 2 — pex `OpCode::from_u8` transmute.** `crates/pex/src/opcode.rs:130-136`:
  `#[repr(u8)]`, contiguous discriminants `0..MAX_OPCODE` (=51), range check precedes the
  transmute. Both preconditions hold. PASS.
- **Dimension 2 — header string allocation (closes #388-class gap flagged as SAFE-D2-01
  in the 2026-07-06 report).** `crates/nif/src/header.rs:394` now defines
  `check_header_alloc(len, cursor)`, called from `read_sized_string` (`header.rs:422`)
  before the `vec![0u8; len]` allocation, mirroring the stream-body guard. Regression
  test `check_header_alloc_rejects_oversized_len` present and passing. PASS — prior
  finding closed.
- **Dimension 3 — `AllocatorResource` drop ordering (#1406/#1477/#1640), now
  structurally hardened.** Since the 2026-07-06 audit, `#1858` split `main.rs` into
  `boot.rs`/`app_step.rs`; the invariant moved with it and got *stronger*: `impl Drop
  for App` (`byroredux/src/main.rs:218-244`) now unconditionally removes
  `AllocatorResource` and takes `renderer` on **every** teardown path (not just
  `WindowEvent::CloseRequested`), closing the exact panic-unwind gap the SKILL calls
  out. The `CloseRequested` handler (`main.rs:904-912`) does the same sequence
  explicitly and is idempotent with the `Drop` impl. PASS, improved.
- **Dimension 3 — deferred-destroy tick after fence wait (#418/#732/#1782).**
  `tick_deferred_destroy` calls in `context/draw.rs:2379-2383` still run after
  `wait_for_fences`; retired BLAS scratch still routes through
  `pending_destroy_scratch` (`cell_loader/unload.rs:134-143`, SAFETY comment intact,
  citing #1782's race-closure rationale). PASS.
- **Dimension 3 — Rapier bodies released on cell unload (#1520).** All 7
  `rapier_release_tests` pass (`cargo test -p byroredux --bin byroredux
  rapier_release`), including ragdoll-specific coverage
  (`release_removes_ragdoll_bodies_colliders_and_joints`,
  `release_sweeps_both_ragdoll_and_rapier_handles`). PASS.
- **Dimension 4 — unsafe-block discipline (closes SAFE-D4-01 from the 2026-07-06
  report).** `#1904` swept every `unsafe {}` block in `crates/renderer/src/vulkan/` with
  a `SAFETY:` comment and landed `#![deny(clippy::undocumented_unsafe_blocks)]` at the
  crate root (`crates/renderer/src/lib.rs:21`). Re-ran `cargo clippy -p
  byroredux-renderer --lib -- -W clippy::undocumented_unsafe_blocks`: zero
  `undocumented_unsafe_blocks` warnings (only unrelated style lints). PASS — prior
  finding closed.
- **Dimension 5 — TLAS resize wait (#1390).** `device.device_wait_idle()`
  (`crates/renderer/src/vulkan/acceleration/tlas.rs:347`) still precedes the old-scratch
  free in the resize branch. PASS.
- **Dimension 5 — SPIR-V reflection ↔ descriptor layout.** All 5
  `scene_descriptor_reflection_tests` pass (RT-enabled/disabled × triangle/water
  shaders, plus the missing-binding diagnostic test). PASS.
- **Dimension 6 — GpuMaterial 300 B pin + intern cap (#797/#1249/#1250).** Size/offset/
  field-order/hash pins all green; `intern_overflow_returns_material_zero` and
  `intern_overflow_persists_across_clear` (`crates/renderer/src/vulkan/material.rs`
  tests) confirm the `MAX_MATERIALS = 16384` cap returns slot 0 rather than
  over-indexing. PASS.
- **Dimension 7 — RT IOR-refraction guards.** `GLASS_RAY_BUDGET = 1_048_576`
  (`shader_constants_data.rs:107`) unchanged; the `#789` texture-equality passthrough
  loop (`triangle.frag:1313-1418`, bounded by `REFRACT_PASSTHRU_BUDGET`) and the
  Frisvad orthonormal basis (`triangle.frag:1292`, `math_common.glsl:103-121`) are both
  the active path. `DBG_VIZ_GLASS_PASSTHRU = 0x80` has no collision in the `DBG_BITS`
  catalog (`shader_constants_data.rs:445+`, 17 entries, all unique). PASS.
- **Dimension 8 — B-spline FLT_MAX sentinel (#772) + AnimationClipRegistry dedup
  (#790).** `crates/nif/src/anim/bspline.rs` still gates translation/rotation/scale on
  the FLT_MAX-sentinel-means-inactive convention (comments at lines 178/328/359-360/394/
  427-428). `AnimationClipRegistry` still interns via `key.to_ascii_lowercase()`
  (`crates/core/src/animation/registry.rs:212`) — case-insensitive dedup intact. PASS.
- **Dimension 8 — `MAX_TOTAL_BONES` overflow guard.** Both
  `bone_palette_overflow_tests` (`over_capacity_breaks_loop_and_truncates_offsets`,
  `at_capacity_fills_palette_completely`) pass. PASS.
- **Dimension 9 — NIFAL NaN boundary.** `Material::resolve_pbr`
  (`crates/core/src/ecs/components/material.rs:686-706`) still gates on
  `metalness.is_nan() || roughness.is_nan()` before clamping; grepped every `Material {`
  construction site in `byroredux/src` + `crates/` — the only ECS-`Material` producer
  outside `material_translate.rs`/`resolve_pbr`'s own tests is a test helper
  (`byroredux/src/helpers.rs:102`, `#[cfg(test)]`, uses `Material::default()`, which is
  finite). `render/static_meshes.rs` still reads the pre-resolved
  `material.{metalness,roughness}` scalars rather than re-deriving them. PASS.
- **Dimension 9 — particle emitter finite-value guard (#1434/#1771).**
  `extract_emitter_rate`'s `sane()` helper (`crates/nif/src/import/walk/mod.rs:790-799`)
  still rejects non-finite, negative, the FLT_MAX sentinel, and exact-zero rates before
  they reach `systems/particle.rs`. PASS.

## Premises Checked and Disproved (no finding)

- **Dimension 10 — "`DebugUiState` holds an `ash::Device`, a `vk::RenderPass`, and the
  renderer's shared allocator; must be freed before device-destroy."** Read
  `crates/debug-ui/src/lib.rs` in full: `DebugUiState` holds only `egui::Context`,
  `egui_winit::State`, `Option<egui::FullOutput>`, and `PanelState` — no Vulkan handles,
  no `Drop` impl at all. The actual Vulkan-side egui resources (render pass,
  descriptor pool, per-frame buffers) live in `EguiPass`
  (`crates/renderer/src/vulkan/egui_pass.rs`), which is a field on `VulkanContext`
  itself (`context/mod.rs:1436`) and is explicitly destroyed inside
  `VulkanContext::Drop` (`context/mod.rs:2990-2992`) *before* `device_wait_idle`'s
  sibling teardown and well before the `VkDevice` destroy call further down. The
  hazard the SKILL describes — a `DebugUiState` ECS resource outliving the renderer
  and calling into a destroyed device — does not exist in the current architecture; the
  design already avoids it by keeping GPU resources inside `VulkanContext`'s own
  reverse-order teardown chain rather than a separately-owned overlay struct. This
  looks like SKILL doc-rot (the description may predate a refactor that moved the
  Vulkan-side egui state out of `debug-ui` and into the renderer crate) rather than a
  live code issue — flagging for the SKILL maintainer, not filing a code finding.
- **"plugin/esm/reader.rs has an unsafe block."** The grep hit is English prose ("Exact
  float equality is unsafe — match on small bands", `reader.rs:130`), not a code
  `unsafe` block. No finding.
- **Vulkan barrier/layout spec claims.** Per the repo's No-Speculative-Vulkan-Fixes
  guardrail, no barrier/layout/sync finding is asserted without validation-layer or
  RenderDoc evidence; none was gathered this pass (no live engine instance was
  launched, per the No-Parallel-Engine-Launch policy) and no render-pass/barrier/
  pipeline-state code changed since the 2026-07-06 audit (confirmed via `git log
  --since=2026-07-06` — the only renderer touches were comment/doc-rot fixes: #1904's
  SAFETY-comment sweep, #1913's shadow-mask bit-width pin, #1916/#1922-25/#1927/#1929
  doc corrections). Carrying the prior audit's clean validation-layer posture forward
  rather than re-asserting it from static reasoning alone.

---

*Report generated by `/audit-safety`. Zero findings this pass — nothing to publish.*
