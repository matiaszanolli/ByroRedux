# Safety Audit — 2026-05-19

## Scope

Full audit across the standard ten dimensions per `/audit-safety`: unsafe Rust, Vulkan spec compliance, GPU/CPU memory, threading, cxx FFI, RT pipeline, new compute pipelines, R1 material table, RT IOR-refraction, and NPC / animation spawn.

Contextual trigger: today's M29.6 commit (`5be66790`) promotes `bind_inverses` from a per-frame staging+device pair to a **persistent DEVICE_LOCAL SSBO** written once per skinned-mesh first-sight. New surface to audit: the per-entity slot pool (`SkinSlotPool`, new ECS Resource), the persistent SSBO lifecycle, the staging-buffer pending-upload queue, and the renderer-side per-slot `cmd_copy_buffer` regions. The architectural promise of M29.6 is "write-once GPU state for NIF-static bind_inverses"; this audit verifies it lands without UB.

Three commits also in scope:
- `9df0e8ea` (#1188) — `recreate_in_flight_for_frame` images_in_flight invalidation
- `4ac5ee8f` + `427cdb69` — M29.5 + cleanup
- `5be66790` — M29.6

## Dedup pass

`gh issue list --label safety --state open` returns 0. The 2026-05-16 audit closed all then-pending safety items; today's audit checks only the M29.5-→M29.6 delta against the new risk surface.

`gh issue list --search "bind_inverses_persistent slot 0" --state all` — no matches. The slot-0-init contract is entirely new in M29.6.

## NEW findings

---

### SAFE-D7-NEW-01: `bind_inverses_persistent` slot 0 never initialized → pool-overflow rendering reads UB transforms

- **Severity**: HIGH (Vulkan UB on a reachable-but-rare code path)
- **Dimension**: D7 — new compute pipeline safety (skin_palette)
- **Location**: [`crates/renderer/src/vulkan/scene_buffer/buffers.rs:501-507`](../../crates/renderer/src/vulkan/scene_buffer/buffers.rs#L501-L507) (allocation), [`crates/core/src/ecs/resources.rs:557-575`](../../crates/core/src/ecs/resources.rs#L557-L575) (slot 0 reservation contract), [`crates/renderer/shaders/triangle.vert:135-158`](../../crates/renderer/shaders/triangle.vert#L135-L158) (consumer)
- **Status**: NEW
- **Introduced**: `5be66790` (M29.6, this session)

**Description.** M29.6's `SkinSlotPool` reserves slot 0 as the "global identity slot" and `next_slot` starts at 1 ([`resources.rs:582`](../../crates/core/src/ecs/resources.rs#L582)). The intent: pool-overflowed skinned entities (allocate returns None) get no `skin_offsets` entry → `bone_offset = 0` in the static_meshes draw loop → vertex shader at `triangle.vert:139` reads `bones[base + bIdx]` for slots 0..MBPM. Pre-M29.6 the CPU pushed identity matrices to `bone_palette[0..MBPM]`, so this fallback rendered the entity in bind pose.

Post-M29.6 the flow is:

1. CPU pushes one identity matrix to `bone_world[0]` (`render/mod.rs:269`); `build_skinned_palettes` resize fills `bone_world[1..MBPM]` with identity ([`skinned.rs:131`](../../byroredux/src/render/skinned.rs#L131))
2. The persistent SSBO `bind_inverses_persistent` is allocated `create_device_local_uninit` ([`buffers.rs:501`](../../crates/renderer/src/vulkan/scene_buffer/buffers.rs#L501)). **No code path writes slot 0..MBPM.** The pool's `allocate(entity)` only pushes `(slot_id, entity)` onto `pending_uploads` for slots ≥ 1 (slot 0 is never returned).
3. `skin_palette.comp` computes `palette[i] = bone_world[i] * bind_inverses[i]` for slots `0..max_used_slot * MBPM`, so `palette[0..MBPM] = identity * UNDEFINED = UNDEFINED`.
4. A pool-overflowed skinned entity reads `bones[0 + bIdx]` = UNDEFINED matrices. With wsum > 0.001 (which is true for any skinned vertex), the shader computes `xform = w0*bones[0] + … = UNDEFINED * vec4(inPosition)`.

**Reachability.** Pool capacity is `(MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1 = 226`. Current heaviest cell (Whiterun Bannered Mare) holds ~6 named NPCs each contributing 3–4 skinned meshes (skeleton root + body + hands + head) ≈ 24 SkinnedMesh entities. **226 ceiling is ~10× above the current heaviest workload.** Pool overflow is not reachable on shipped content today, but the contract is silently broken.

A secondary reach: any future code path that constructs `SkinSlotPool::new(N)` with `N > (MAX_TOTAL_BONES / MBPM) - 1` AND issues a `record_pending_bind_inverse_copies` call referencing slot ID > 226 would `cmd_copy_buffer` past the persistent SSBO's end. See SAFE-D7-NEW-03 below.

**Why MISSED in test infrastructure.** The pool tests (`crates/core/src/ecs/resources.rs::skin_slot_pool_tests`) are unit-level — they test allocation/sweep semantics, not the renderer-side persistent SSBO state. The renderer's only numeric pin (`skin_palette_per_slot_math_matches_cpu_compute_palette_into`) verifies the per-slot multiply formula in isolation. No test exercises the "overflow → bone_offset=0 → palette[0..MBPM] read" round trip.

**Pre-M29.6 ground truth.** Read `bone_palette[0..MBPM]` in the pre-M29.6 codebase (per `git show 4ac5ee8f~1:byroredux/src/render/skinned.rs`): CPU pushed identity for slot 0 + filled per-mesh ranges with `world × bind_inv`. Slot 0 was identity. The vertex shader's fallback for skinning produced the bind pose. M29.5's commit body even calls out: "rigid meshes tagged with `bone_offset = 0` that somehow hit the skinning path fall here harmlessly."

**Suggested fix.** Initialize `bind_inverses_persistent[0..MBPM]` with identity at startup. Two viable shapes:

1. **One-time queue submit at `SceneBuffers::new`**: mirror the pre-M29.5 `seed_identity_bones` pattern. Write `MAX_BONES_PER_MESH` identity matrices into a small staging buffer, `cmd_copy_buffer` to the persistent SSBO's slot 0 range, queue-submit and wait. ~30 LOC. Cost: one-shot at engine start.

2. **`vkCmdUpdateBuffer` inline write at first `draw_frame`**: at most 65536 bytes per call. `MBPM × 64 B = 9216 B`, well within. Single inline call, no staging needed. ~15 LOC.

Option 2 is cheaper and avoids reintroducing the `seed_identity_bones` queue-submit shape. Either fix should be paired with a numeric test that constructs a pool, exhausts it, dispatches via a mocked or live pipeline, and asserts the persistent SSBO's slot 0 reads back as identity. (Live-Vulkan test requires harness work; the deeper claim — that the pool's overflow contract is "render in bind pose, no UB" — can be partially pinned by a CPU-side test that asserts `bind_inverses_persistent` is identity-seeded at the relevant slot range without exercising the GPU at all.)

---

### SAFE-D7-NEW-02: `upload_pending_bind_inverses` silently drops excess past `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME` → pool/SSBO state divergence

- **Severity**: MEDIUM (correctness regression; silent under heavy cell-load)
- **Dimension**: D7 — new compute pipeline safety
- **Location**: [`crates/renderer/src/vulkan/scene_buffer/upload.rs:207-231`](../../crates/renderer/src/vulkan/scene_buffer/upload.rs#L207-L231), [`byroredux/src/main.rs:1233-1263`](../../byroredux/src/main.rs#L1233-L1263) (caller drains pool then passes the full list to draw_frame)
- **Status**: NEW
- **Introduced**: `5be66790` (M29.6)

**Description.** `upload_pending_bind_inverses` caps the staging-buffer writes at `pending.len().min(MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME)` = `min(N, 16)` and returns `Ok(capped)`. Entries past index 16 are silently dropped — they were already drained from `pool.pending_uploads` by `main.rs:1233`, and the renderer does not re-queue them.

Consequence: entities whose pending upload was dropped stay in `pool.entity_to_slot` (allocated, will get the same slot on the next `allocate` call), but `pending_uploads` is empty for them in subsequent frames. The persistent SSBO at their slot offset is **never written**. They render with UB transforms forever (or until pool eviction → slot reuse → fresh pending upload).

**Reachability.** Cell streaming spawns multiple NPCs at once. The M41 Phase 2 close-out smoke ran on FO4 MedTekResearch01 with 23 SkinnedMesh entities at cell load. Each NPC is 4-5 SkinnedMesh components (skeleton + body + hands + head + worn armor) → 5 NPCs ≈ 25 fresh skinned-mesh first-sights in one frame. The 16-entry cap is **reachable on a single heavy cell load**.

**Suggested fix.** Three options:

1. **Re-queue the excess** (preferred): `upload_pending_bind_inverses` returns the dropped tail; the caller pushes them back onto `pool.pending_uploads` via a new `SkinSlotPool::requeue_pending(&[(u32, EntityId)])` method. Next frame drains them.
2. **Per-pool maintenance hook**: have `drain_pending` only return up to `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME` entries and keep the tail in `pending_uploads`. Renames break the M29.6 contract but localize the fix.
3. **Bump the cap to MAX_SKINNED = 226**: staging buffer grows from 144 KB to ~2 MB (matches the persistent SSBO). Brute-force, but eliminates the truncation case entirely. Adds one full MAX_TOTAL_BONES × 64 B = 2 MB staging allocation.

Option 1 keeps the per-frame upload bandwidth bounded by the staging size (the original design intent) AND eliminates silent loss. Option 3 is simplest if memory headroom is OK; given the engine is RT-required with a 6 GB VRAM minimum, 2 MB of extra staging is negligible.

**Why MISSED.** Same as SAFE-D7-NEW-01 — no integration test exercises the >16-pending case. The pool unit test `first_allocation_queues_pending_upload` tests the queue mechanism with a single entry; it doesn't model cap saturation.

---

### SAFE-D7-NEW-03: `record_pending_bind_inverse_copies` has no slot-bounds debug_assert → out-of-bounds `cmd_copy_buffer` if pool capacity drifts past `(MAX_TOTAL_BONES / MBPM) - 1`

- **Severity**: LOW (defensive; reachable only via constructor misuse)
- **Dimension**: D2 — Vulkan spec compliance (`VUID-vkCmdCopyBuffer-dstOffset-00114`)
- **Location**: [`crates/renderer/src/vulkan/scene_buffer/upload.rs:260-266`](../../crates/renderer/src/vulkan/scene_buffer/upload.rs#L260-L266)
- **Status**: NEW
- **Introduced**: `5be66790` (M29.6)

**Description.** The loop at `upload.rs:260` computes `dst_offset = (slot_id as DeviceSize) * slot_byte_stride` and `size = slot_byte_stride`, then issues a single `cmd_copy_buffer` with the N regions. If any `slot_id > (MAX_TOTAL_BONES / MBPM) - 1 = 226`, the copy region's end (`(slot_id + 1) × MBPM × 64`) exceeds the persistent SSBO's allocation (`MAX_TOTAL_BONES × 64 = 2097152 B`). Vulkan VUID violation; on drivers without validation, UB / device lost.

**Reachability.** `main.rs:610-619` constructs the pool with `((MAX_TOTAL_BONES / MBPM) - 1) as u32 = 226`. Test files do the same. The contract is enforced by convention only — there's no API-level link between `SkinSlotPool` capacity and the persistent SSBO size.

A future code path that constructs `SkinSlotPool::new(300)` (or `SkinSlotPool::new(MAX_TOTAL_BONES / MBPM)` without the `-1`) would trip this. The `-1` was added during M29.6 implementation precisely because the obvious `MAX_TOTAL_BONES / MBPM = 227` formulation also fails this check (slot 227 × 144 + 144 = 32832 > 32768).

**Suggested fix.** Add a `debug_assert!` at the top of the loop:

```rust
for (i, &slot_id) in pending_slots.iter().take(capped).enumerate() {
    debug_assert!(
        ((slot_id as usize + 1) * MAX_BONES_PER_MESH) <= MAX_TOTAL_BONES,
        "M29.6 contract: slot_id {slot_id} would write past bind_inverses_persistent end \
         ({MAX_TOTAL_BONES} bones). SkinSlotPool capacity must be \
         ≤ (MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1"
    );
    // ... existing copies.push(...) ...
}
```

This catches the constructor-misuse class at the first frame any out-of-range slot is issued, instead of relying on Vulkan validation to fire later.

---

## RE-AFFIRMED clean (no delta since 2026-05-16)

The following dimensions had no touching commits since the 2026-05-16 audit base (verified by `git log --since="2026-05-16" --name-only -- <dimension area>`) and the prior audit's verdicts stand:

- **D1 — unsafe Rust** (existing pattern). The new `SkinPaletteComputePipeline` unsafe blocks mirror `SkinComputePipeline` exactly — same SAFETY comments on push_constant write (`skin_compute.rs:726`), same Drop ordering (`skin_compute.rs:750-756`: pipeline → layout → pool → set_layout). Reflection cross-check (`validate_set_layout`) runs at construction. No new unsafe-block hazards beyond the contract issues filed above.
- **D4 — thread safety**. `SkinSlotPool` is an ECS Resource accessed serially during `build_render_data` → `draw_frame` from the main thread. No new locks, no `Arc<Mutex>` exchanges. Existing `Arc<Mutex<Allocator>>` access points (`upload.rs` and `descriptors.rs::destroy`) are unchanged by M29.6.
- **D5 — FFI safety**. cxx-bridge not touched.
- **D6 — RT pipeline safety**. Skinned-BLAS path (`acceleration/blas_skinned.rs`) and TLAS path unchanged. M29.6 only changes the bone-palette input plumbing; the BLAS refit reads from per-`SkinSlot` output buffers (M29.3, unchanged).
- **D8 — R1 material table**. `GpuMaterial` 260 B pin + per-field offset pin + intern cap all unchanged. No commits since 2026-05-16 touched `material.rs` except `#1190` which added MAT_FLAG_* defines without changing struct layout.
- **D9 — RT IOR refraction**. `triangle.frag`'s glass-loop code (#789), Frisvad basis (#820), GLASS_RAY_BUDGET (8192), and interior cell-ambient fallback (bb53fd5) all unchanged.
- **D10 — NPC / animation spawn**. B-spline pose-fallback (#772) and AnimationClipRegistry case-insensitive dedup (#790) unchanged.

The 2026-05-16 SAFE-D1-NEW-01 doc-comment finding on skinned-BLAS docstring referencing `UPDATABLE_AS_FLAGS` was closed under #1155 (b3096bd3).

## Verification

1. `cargo test --workspace` — 2303 passed, 0 failed.
2. `.claude/commands/_audit-validate.sh` — OK across 287 path refs.
3. `cargo check --workspace` — clean (one persistent `shader_constants.glsl` regen notice; pre-existing `held` warning in `buffer.rs:984`).

## Recommended fix order

1. **SAFE-D7-NEW-01** (HIGH) — slot-0 init. Land before next bench cycle; current content doesn't reach the path but the contract is silently violated.
2. **SAFE-D7-NEW-02** (MEDIUM) — pending-upload re-queue. Reachable on heavy cell load (FO4 MedTek at 23 SkinnedMesh entities is one short of the 16-cap; one more NPC tips it).
3. **SAFE-D7-NEW-03** (LOW) — debug_assert. Hardens the contract without runtime cost in release.

All three are in the M29.6 surface area I shipped this session. The fixes total ~50 LOC + 2 new tests. Recommend bundling into a single follow-on commit `M29.6 hotfix: persistent SSBO init + pending requeue + bounds assert` before resuming forward work.
