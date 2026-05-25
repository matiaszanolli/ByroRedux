# Safety Audit — §3 Memory Safety

- **Date**: 2026-05-25
- **Scope**: Dimension 3 only (Memory Safety) per `.claude/commands/audit-safety.md`
- **Baseline**: HEAD `47b11db8` — `cargo test -p byroredux-core` green at session start
- **GitHub issue state**: 0 currently open

## Coverage

Dimension 3 sub-checks from `audit-safety.md`:

1. GPU allocations freed before allocator drop
2. Allocator dropped before device destroy
3. BLAS/TLAS/G-buffer/SVGF/TAA/caustic/SkinSlot/MaterialBuffer tracked + freed
4. M40 streaming: cell-loaded resources freed at unload (incl. async pre-parse worker path)
5. CPU memory: no unbounded Vec / HashMap growth
6. R1 material dedup map: cleared or pooled per frame
7. Stack overflow risk: recursion is bounded

## Result

Two **LOW** findings (recursion hardening). All other checks pass — no critical leak, no Drop-order violation, no unbounded growth beyond intentional process-lifetime interning.

---

## Per-sub-check verification

### §3.1 / §3.2 — Allocator ordering vs device.destroy_device

**PASS.** [crates/renderer/src/vulkan/context/mod.rs:2708-2747](../../crates/renderer/src/vulkan/context/mod.rs#L2708-L2747)

- L2708 `take()` the `Option<SharedAllocator>` out of the context.
- L2709 `Arc::try_unwrap()` unwraps the Arc; inner allocator dropped at L2710 while device still alive.
- L2747 `device.destroy_device()` runs *after* the allocator has been freed.
- Strong-count > 1 takes an early-return at L2743 to avoid use-after-free — defensive against leaked Arc clones (acceptable trade-off: leaked clone leaks the allocator, but does not double-free).

Recent regressions closed and verified:
- **#1163** (allocator MutexGuard held across SSAO `destroy()` re-entry) — hoist confirmed at SSAO init site.
- **#1165** (same pattern in depth-image init) — hoist confirmed.

### §3.3 — Per-subsystem Drop / destroy pairing

**PASS** for every tracked subsystem. Cleanup is uniformly one of: (a) explicit `destroy()` called from [VulkanContext::Drop](../../crates/renderer/src/vulkan/context/mod.rs#L2512-L2756), or (b) safety-net `Drop` impl gated on `if !std::thread::panicking()` (post-`#1128` fix at c636b36a so unwinds don't tip `debug_assert!(false)`).

| Subsystem | Explicit destroy | Drop safety-net | Status |
|---|---|---|---|
| BLAS (static) | [acceleration/blas_static.rs:255](../../crates/renderer/src/vulkan/acceleration/blas_static.rs#L255) | drained in `AccelerationManager::destroy` | ✓ |
| BLAS (skinned) | drained at [acceleration/mod.rs](../../crates/renderer/src/vulkan/acceleration/mod.rs):311–315 | — | ✓ (#1138 fix) |
| TLAS instance+result | drained at [acceleration/mod.rs](../../crates/renderer/src/vulkan/acceleration/mod.rs):280–287 | — | ✓ |
| GBuffer | [gbuffer.rs:436](../../crates/renderer/src/vulkan/gbuffer.rs#L436) | [gbuffer.rs:181](../../crates/renderer/src/vulkan/gbuffer.rs#L181) | ✓ |
| SVGF | [svgf.rs:1058](../../crates/renderer/src/vulkan/svgf.rs#L1058) | [svgf.rs:156](../../crates/renderer/src/vulkan/svgf.rs#L156) | ✓ |
| TAA history | [taa.rs:850](../../crates/renderer/src/vulkan/taa.rs#L850) | — | ✓ |
| Caustic accumulator | [caustic.rs:889](../../crates/renderer/src/vulkan/caustic.rs#L889) | — | ✓ |
| Bloom mips | [bloom.rs:612](../../crates/renderer/src/vulkan/bloom.rs#L612) | — | ✓ |
| Volumetrics volumes | [volumetrics.rs:943](../../crates/renderer/src/vulkan/volumetrics.rs#L943) | — | ✓ |
| Water | [water.rs:494](../../crates/renderer/src/vulkan/water.rs#L494) | — | ✓ |
| Skin compute | [skin_compute.rs:584](../../crates/renderer/src/vulkan/skin_compute.rs#L584) | — | ✓ |
| MaterialBuffer SSBO | held by GpuBuffer in `SceneBuffers` | [buffer.rs:917](../../crates/renderer/src/vulkan/buffer.rs#L917) | ✓ |

`VulkanContext::Drop` reverse-creation order verified end-to-end ([crates/renderer/src/vulkan/context/mod.rs:2524-2752](../../crates/renderer/src/vulkan/context/mod.rs#L2524-L2752)). Pipeline cache + render pass + swapchain destroyed before allocator drop; surface + instance destroyed after device destroy.

### §3.4 — M40 cell-streaming resource lifecycle

**PASS.** Every per-cell resource is released at unload boundary.

- **Entities**: [byroredux/src/cell_loader/unload.rs:247](../../byroredux/src/cell_loader/unload.rs#L247) `world.despawn(eid)` removes all component rows.
- **BLAS refcounts**: [unload.rs:185](../../byroredux/src/cell_loader/unload.rs#L185) `drop_blas(handle)` on refcount=0; scratch shrunk via `shrink_blas_scratch_to_fit` at L200.
- **TextureHandle refcounts**: [unload.rs:229](../../byroredux/src/cell_loader/unload.rs#L229) `drop_texture(handle)`; terrain tiles release 8 layer refs at L152.
- **FormIdPool slots**: [unload.rs:294](../../byroredux/src/cell_loader/unload.rs#L294) `pool.release(id)`.
- **AnimationClipRegistry**: `release(handle)` is wired from [byroredux/src/cell_loader/partial.rs:67](../../byroredux/src/cell_loader/partial.rs#L67) (NIF re-import path) and [partial.rs:158](../../byroredux/src/cell_loader/partial.rs#L158) (cache eviction path). Path-memo `clip_handles_by_path` is erased on release per [crates/core/src/animation/registry.rs:176-189](../../crates/core/src/animation/registry.rs#L176-L189). Regression coverage for #863 at registry.rs:346.

**Async pre-parse worker** ([byroredux/src/streaming.rs:264-304](../../byroredux/src/streaming.rs#L264-L304)):
- `WorldStreamingState::shutdown` closes `request_tx` (L278) and `join_with_timeout` (L279) with a 1 s budget.
- `worker.take()` (L268) makes shutdown idempotent.
- `Drop` impl (L301-304) re-enters `shutdown` so a non-graceful exit still drains the worker (`#1167` fix).
- Timeout fallback drops the JoinHandle (`#856` / `#1169` closed) — worker continues, exits naturally when `request_tx` is closed; no static-state leak (`PartialNifImport` has compile-time `: Send`, no `Rc`).

### §3.5 / §3.6 — Per-frame buffers + dedup map

**PASS.**

- **MaterialTable dedup**: cleared every frame at [byroredux/src/render/mod.rs:251](../../byroredux/src/render/mod.rs#L251) (`material_table.clear()` inside `build_render_data`). `clear()` drains both `materials` Vec and `index` HashMap at [crates/renderer/src/vulkan/material.rs:937-942](../../crates/renderer/src/vulkan/material.rs#L937-L942). Capacity retained (regression test `clear_resets_table_but_keeps_capacity`). Hard cap `MAX_MATERIALS = 4096` enforced at intern site; upload truncation matched in `scene_buffer/upload.rs::upload_materials` (R1 invariant).
- **TLAS instance buffer**: `shrink_tlas_to_fit` runs post-fence at frame end ([acceleration/memory.rs:138-189](../../crates/renderer/src/vulkan/acceleration/memory.rs#L138-L189)); shrinks when capacity ≫ working set.
- **BLAS scratch**: `shrink_blas_scratch_to_fit` runs at cell unload, shrinks to peak survivor.
- **Per-frame Vecs** (`draw_commands`, `water_commands`, `gpu_lights`, `bone_world`, `skin_offsets`): `clear()`-ed at the top of [build_render_data:243-247](../../byroredux/src/render/mod.rs#L243-L247) each frame; allocations are pooled across frames.
- **Bind-inverse pending uploads**: cap enforced at the pool, not at the upload site. `SkinSlotPool::drain_pending(max)` returns at most `max` entries and leaves the rest in `pending_uploads` for next frame ([crates/core/src/ecs/resources.rs:754-757](../../crates/core/src/ecs/resources.rs#L754-L757)). The renderer passes `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME = 227` ([byroredux/src/main.rs:1416-1418](../../byroredux/src/main.rs#L1416-L1418)). Regression coverage `drain_pending_does_not_lose_capped_entities` at resources.rs:969. The `#1192` / SAFE-D7-NEW-02 silent-drop bug is structurally fixed at the pool level.

**Process-lifetime interning** (intentional, called out for transparency, not a finding):
- `StringPool` ([crates/core/src/string/mod.rs:52](../../crates/core/src/string/mod.rs#L52)) — no remove; bounded by unique-string count across session. Reasonable for an interner.
- `FormIdPool` ([crates/core/src/form_id.rs:131-139](../../crates/core/src/form_id.rs#L131-L139)) — `to_pair` Vec + `to_runtime` HashMap grow on first sight. Bounded by unique loaded records. `release()` exists at the unload site (above), so the pool is *not* purely append-only.
- `NifImportRegistry` ([byroredux/src/cell_loader/nif_import_registry.rs:107-115](../../byroredux/src/cell_loader/nif_import_registry.rs#L107-L115)) — process-lifetime with opt-in LRU cap via `BYRO_NIF_CACHE_MAX` env var (default `0` = unlimited). Documented design trade-off per `#381` / `#635`. Bounded in steady state by total unique mesh paths in the loaded plugin set, not by playtime.

### §3.7 — Stack-overflow risk (recursion bounds)

Recursive functions identified:

1. **`walk_node_hierarchical`** ([crates/nif/src/import/walk/mod.rs:165](../../crates/nif/src/import/walk/mod.rs#L165)) — self-call at L224, no depth guard.
2. **`walk_node_flat`** ([crates/nif/src/import/walk/mod.rs:643](../../crates/nif/src/import/walk/mod.rs#L643)) — self-call at L682, no depth guard.
3. **`resolve_shape`** ([crates/nif/src/import/collision.rs:127-180](../../crates/nif/src/import/collision.rs#L127-L180)) — self-call via MOPP wrapper, ListShape children, TransformShape, Phantom inner shape. No depth guard.
4. **`parse_expr_bp`** ([crates/papyrus/src/parser/expr.rs:22-61](../../crates/papyrus/src/parser/expr.rs#L22-L61)) — Pratt parser recurses through RHS of binary ops + nested parenthesised expressions. No depth guard.

Transform propagation is iterative BFS via `VecDeque` (`#464`, `byroredux/src/anim_convert.rs:33-34`) — safe. ESM record walkers are iterative — safe. No other unbounded recursion found.

Two LOW findings filed below for the NIF and Papyrus paths.

---

## Findings

### SAFE-DIM3-NEW-01: NIF scene-graph and collision walkers have no recursion-depth guard

- **Severity**: LOW
- **Dimension**: §3.7 (Stack overflow risk)
- **Location**: [crates/nif/src/import/walk/mod.rs:165-235](../../crates/nif/src/import/walk/mod.rs#L165-L235) (`walk_node_hierarchical`), [walk/mod.rs:643-695](../../crates/nif/src/import/walk/mod.rs#L643-L695) (`walk_node_flat`), [crates/nif/src/import/collision.rs:127-180](../../crates/nif/src/import/collision.rs#L127-L180) (`resolve_shape`)
- **Status**: NEW
- **Description**: Three recursive NIF traversals carry no depth guard. Depth is bounded only by the on-disk NIF block tree. Shipped Bethesda content does not approach a stack-overflow depth, so this is hardening only — but a malformed third-party or adversarial NIF could in principle crash the process via stack overflow rather than being rejected with a parse error.
- **Evidence**:
  - `walk_node_hierarchical` self-call at L224 with no `depth` parameter or `MAX_DEPTH` constant.
  - `walk_node_flat` self-call at L682 same shape.
  - `resolve_shape` recurses through MOPP (L129), ListShape children (L136), TransformShape child (L151), Phantom inner (L179). A `BhkListShape` whose `sub_shape_refs` form a cycle through another `BhkListShape` would stack-overflow rather than return an error.
- **Impact**: Loading a hostile NIF crashes the process (stack overflow → abort) instead of failing the import. Hard to trigger from shipped content, but the parser is otherwise designed to tolerate corrupt data via `Result`-returning paths.
- **Related**: NIF parser graceful-recovery design per Session 8 ("graceful per-block parse recovery").
- **Suggested Fix**: Add an explicit `depth: u32` parameter (or carry it on `inherited_props` length) and bail with a logged warning at, say, depth 128. Cycle-detection via a `visited: HashSet<usize>` for `resolve_shape` would catch the BhkListShape cycle case.

### SAFE-DIM3-NEW-02: Papyrus expression parser has no recursion-depth guard

- **Severity**: LOW
- **Dimension**: §3.7 (Stack overflow risk)
- **Location**: [crates/papyrus/src/parser/expr.rs:22-61](../../crates/papyrus/src/parser/expr.rs#L22-L61) (`parse_expr_bp`)
- **Status**: NEW
- **Description**: The Pratt expression parser recurses through the RHS of every binary operator and through every parenthesised sub-expression with no depth cap. A source file containing `((((((... ))))))` to arbitrary depth would stack-overflow the parser rather than emit a `ParseError`.
- **Evidence**: `parse_expr_bp` self-call at L61; `parse_prefix` → `parse_expr` → `parse_expr_bp` chain at L137 (parenthesised expression). No `MAX_EXPR_DEPTH` constant in the parser.
- **Impact**: A modder-authored `.psc` (or a fuzz-generated one) can crash the parser. Papyrus source input is treated as trusted today but the engine surface includes "compile mod scripts at load time" so the input surface is partially user-controlled.
- **Related**: Papyrus parser is the entry point for M30 Phase 2 (bytecode compile path).
- **Suggested Fix**: Carry `&mut depth: u32` through `parse_expr_bp` and `parse_prefix`; emit `ParseError::ExpressionTooDeep` at, say, depth 256.

---

## Out-of-scope confirmations

The following were checked but produced no findings:

- §1 (unsafe blocks) — out of scope for `/audit-safety 3`.
- §2 (Vulkan spec compliance) — out of scope for `/audit-safety 3`.
- §4 (thread safety) — out of scope; recent closeouts `#1163` / `#1165` / `#1167` / `#1169` confirmed via §3 cleanup paths.
- §5 (FFI) — out of scope.
- §6–§10 (RT, R1, IOR, NPC) — out of scope for `/audit-safety 3`; `#1192` cap verified incidentally under §3.6 above (no regression).

---

## Next step

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-05-25_DIM3.md
```
