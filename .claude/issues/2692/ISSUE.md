# SAFE-DOC: safety documentation & comment drift (6 sites)

**Issue**: #2692
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

Consolidated documentation / comment drift from the `/audit-safety` half of `/audit-suite renderer-deep` (2026-08-12). Six sites, one root cause: **comments and docs that still certify superseded behaviour.**

Includes two corrections to the `/audit-safety` skill itself:
- the "~676 SAFETY vs ~761 unsafe" gap is a **token-count artefact** (`unsafe fn`/`unsafe impl`/prose), not a real comment gap — the census is **681/681 commented**. This work item should be retired from the skill so future runs stop hunting a haystack that does not exist.
- Dimension 2 names `StorageRef`/`StorageRefMut`, which do not exist; the live types are `QueryRead`/`QueryWrite`. The `DBG_*` catalog is 31 bits, not the 24 the skill claims.

---

## SAFE-D4-02
- **Severity**: LOW
- **Dimension**: 4 — commented block, stated invariant superseded
- **Location**: [blas_static.rs](crates/renderer/src/vulkan/acceleration/blas_static.rs) — `AccelerationManager::build_blas` call site (SAFETY at :193-195, block at :200), `build_blas_batched` pre-batch call site (SAFETY at :603-606, block at :610); callee `evict_unused_blas` at :1233
- **Status**: NEW
- **Description**: The single-shot call site justifies the `unsafe` with
  *"evicted entries are gated to idle >= MAX_FRAMES_IN_FLIGHT + 1, so no
  in-flight command buffer or TLAS build references them"*. `evict_unused_blas`'s
  own body doc (added under #1449 / MEM-01) states the opposite: the idle gate is
  *"now purely an **LRU policy** … NOT the safety mechanism it used to be"*,
  because eviction routes through `pending_destroy_blas` (deferred-destroy) and
  the countdown is what provides cross-frame safety. The function further
  documents its `unsafe` marker as *"likewise vestigial"* (`let _ = (device, allocator);`).
- **Evidence**: `blas_static.rs:1255-1265` — "the idle gate below is now purely
  an LRU policy … the deferred countdown now provides the real cross-frame
  safety"; vs `blas_static.rs:193-195` asserting the gate as the guarantee.
- **Impact**: No unsoundness — the real guard (deferred destroy) is strictly
  stronger than the one the comment names. The hazard is that a future
  performance tune of `MIN_IDLE_FRAMES` (a pure LRU knob) looks, from the call
  site, like it is relaxing a safety invariant, or conversely that someone
  removes the deferred-destroy path believing the idle gate covers it — which
  is exactly the #1449 device-loss regression.
- **Related**: #1449 / MEM-01 (deferred-destroy fix), #1792 / PERF-D3-NEW-01
  (`pending_bytes` threading). Guard itself is intact.
- **Suggested Fix**: Point both call-site SAFETY comments at
  `pending_destroy_blas` + `DEFAULT_COUNTDOWN` as the actual invariant, and
  follow through on the callee's own TODO by dropping the vestigial `unsafe`
  from `evict_unused_blas`'s signature.

---


---

## SAFE-D1-01
- **Severity**: LOW
- **Dimension**: 1 (FFI Lifetime Safety)
- **Location**: [lib.rs](crates/fsr3-sys/src/lib.rs) — `impl Drop for Context` (SAFETY at :447-448, FFI call at :449); referenced contract at `Context::create` `# Safety` (:338-341); the real statement at the `Context` type doc (:326-330)
- **Status**: NEW
- **Description**: The `Drop` SAFETY comment says *"The Vulkan-idle requirement
  is part of `Context::create`'s contract."* `Context::create`'s `# Safety`
  section states only that the handles must be live, mutually compatible, and
  outlive the result — it says nothing about idling before drop. The idle
  requirement is real but lives one level up, on the `Context` **type** doc
  (*"The caller must also ensure no submitted command buffer uses FSR resources
  when this value is dropped"*).
- **Evidence**: `lib.rs:338-341` (`# Safety` on `create`) vs `lib.rs:326-330`
  (type doc) vs `lib.rs:447-448` (the `Drop` claim).
- **Impact**: Documentation only — the obligation is stated somewhere and the
  renderer honours it (`frame_upscaler.rs:118-119` explicitly notes "teardown
  after `device_wait_idle`", and `#2158`'s `Drop`-ordering source-text pins at
  `frame_upscaler.rs:1054-1080` assert the FSR context is retired before
  `vkDestroyDevice`). Risk is a reader following the cross-reference, finding no
  idle clause on `create`, and concluding `Drop` is idle-safe on its own.
- **Related**: #2158 (FSR Drop-ordering pins — verified intact).
- **Suggested Fix**: Move the sentence into `Context::create`'s `# Safety`
  section (or repoint the `Drop` comment at the type-level doc).

---


---

## SAFE-D2-01
- **Severity**: LOW
- **Dimension**: 2 (GPU-struct layout contract — documentation facet)
- **Location**: [gpu_types.rs](crates/renderer/src/vulkan/scene_buffer/gpu_types.rs):84-85; [gpu_instance_layout_tests.rs](crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs):96-99; [descriptors.rs](crates/renderer/src/vulkan/scene_buffer/descriptors.rs):327-328 (and the same figure repeated at [upload.rs](crates/renderer/src/vulkan/scene_buffer/upload.rs):557-558)
- **Status**: NEW — residual of #2308 (CLOSED, which fixed only `docs/engine/renderer.md`)
- **Description**: `GpuInstance` is 128 B and the pins are correct
  (`gpu_instance_is_128_bytes_std430_compatible` asserts 128;
  `gpu_instance_field_offsets_match_shader_contract` pins all 15 offsets
  including `skinned_vertex_address` at 112 and `_reserved` at 120). But four
  in-code doc sites still quote the pre-#2219 size:
  - `gpu_types.rs:84` — *"The `size_of::<GpuInstance>() == 112` test below asserts the invariant"*, sitting **directly under** a layout history whose last line reads `112 → 128 (#2219, …)`.
  - `gpu_instance_layout_tests.rs:97` — *"rely on the size assertion above (112 B)"*, three lines below an assertion of 128.
  - `descriptors.rs:328` and `upload.rs:558` — *"7359 draws at 112 B per `GpuInstance` ≈ 805 KB/frame"* (the real figure is ~942 KB).
- **Evidence**: `gpu_types.rs:82` (`///   - 112 → 128 (#2219, …)`) immediately
  precedes `gpu_types.rs:84` (`/// The size_of::<GpuInstance>() == 112 test …`).
- **Impact**: No runtime effect — the tests are the contract and they are
  correct. This is the exact failure class `_audit-common`'s symbol convention
  calls out ("a wrong number in a GPU layout contract, not a typo"): the
  authoritative *comment* on the struct that must stay in lockstep with five
  GLSL mirrors states the wrong size, which is how a mirror gets updated to the
  wrong figure.
- **Related**: #2308 (CLOSED — same stale 112 B / 300 B pair, fixed in
  `docs/engine/renderer.md` only); `feedback_shader_struct_sync` memory note.
- **Suggested Fix**: Replace 112 with 128 at all four sites and recompute the
  MedTek KB/frame figure.

---


---

## SAFE-D3-01
- **Severity**: LOW
- **Dimension**: 3 (leaks — doc integrity of the drop-ordering contract)
- **Location**: [deferred_destroy.rs](crates/renderer/src/deferred_destroy.rs), `DEFAULT_COUNTDOWN` doc comment
- **Status**: NEW
- **Description**: The doc that states the countdown safety contract points at
  `draw.rs:889` and `acceleration.rs::tick_pending_destroy_blas` as the
  reference implementations. Neither resolves: the tick now lives ~570 lines
  later in `context/draw.rs` (immediately after `wait_for_fences`), and
  `crates/renderer/src/vulkan/acceleration.rs` was split into the
  `acceleration/` directory — the function is `AccelerationManager::tick_deferred_destroy`
  in `acceleration/blas_static.rs`. `grep -rn tick_pending_destroy_blas crates/`
  returns only this comment.
- **Evidence**: `deferred_destroy.rs`: ``/// Mirrors the correct pattern at `draw.rs:889` and`` /
  ``/// `acceleration.rs::tick_pending_destroy_blas`.`` · `ls crates/renderer/src/vulkan/acceleration.rs`
  → No such file. (`context/draw.rs:1599` carries a second stale
  `crates/renderer/src/vulkan/acceleration.rs::build_tlas` path reference.)
- **Impact**: No runtime effect. The #418 "tick after fence wait" invariant is
  the single highest-value ordering rule in this subsystem and its doc anchor
  is what a future refactorer follows; a dead anchor is how that rule gets
  silently relocated.
- **Related**: #418, #732, #1782.
  **Cross-audit**: same site as **REN-D5-04** in the renderer audit.
- **Suggested Fix**: Re-anchor on symbols per the post-`#1114` path convention:
  `VulkanContext::draw_frame` (after `wait_for_fences`) and
  `AccelerationManager::tick_deferred_destroy`.

---


---

## SAFE-D7-02
- **Severity**: LOW
- **Dimension**: 7 — RT IOR-Refraction safety guards
- **Location**: [triangle.frag](crates/renderer/shaders/triangle.frag) — the "Identity check by texture" paragraph inside the `REFRACT_PASSTHRU_BUDGET` block
- **Status**: NEW (sibling of #2546, which fixed the same stale claim in the *skill* text, not in the shader)
- **Description**: The block comment introducing the passthrough loop states
  "Identity check by texture … `tInst.textureIndex == inst.textureIndex` flags
  both self-hits and sibling-part-hits as 'skip past'". That check was replaced
  by `materialKind == MATERIAL_KIND_GLASS` in `a09d2b76` precisely because
  texture-equality misfired when glass shared a texture with opaque geometry.
  The *later* comment on `hitIsGlass` correctly explains the replacement and
  calls the texture keying a mis-fire — so the same file argues both sides, and
  `tInst` does not exist at that point in the function.
- **Evidence**: `hitIsGlass = (materials[hInst.materialId].materialKind == MATERIAL_KIND_GLASS);`
  is the only identity gate; `fallbackTexture` is a separate
  unresolved-placeholder skip, not the described identity check.
- **Impact**: Doc rot in the one comment a future reader consults before
  touching the unbounded-recursion guard. Reintroducing texture-equality on the
  strength of this paragraph re-opens #789's see-through-walls regression.
- **Related**: #789, #2546, `a09d2b76`.
- **Suggested Fix**: Delete or rewrite the paragraph to describe the
  `materialKind` keying; keep the "fixed budget of 2 passthrus" sentence.

---


---

## SAFE-D6-02
- **Severity**: LOW
- **Dimension**: 6 — R1 Material Table Layout Soundness
- **Location**: [static_meshes.rs](byroredux/src/render/static_meshes.rs) — the `#781 / PERF-N4` comment immediately above `cmd.material_id = material_table.intern_by_hash(...)`
- **Status**: NEW (distinct site from #2273, which is `MaterialTable::intern`'s "75 scalar fields"; and from #2415, which is the `gpu_instance_layout_tests` doc comment's "300 B")
- **Description**: The comment reads "`intern_by_hash` skips the
  `to_gpu_material()` 260-byte construction". `GpuMaterial` has been 348 B since
  `1d94eb24` (2026-07-27); the sibling comment on `MaterialTable::intern_by_hash`
  itself already says 348.
- **Evidence**: `grep -n "260-byte" byroredux/src crates/renderer/src` → this one
  site. `std::mem::size_of::<GpuMaterial>()` is pinned at 348 by
  `gpu_material_size_is_348_bytes`.
- **Impact**: Doc rot only, but on the exact hot path whose cost the comment
  justifies — the pattern the validate gate exists to catch (a wrong number in a
  GPU layout contract).
- **Related**: #2273, #2415, `1d94eb24`.
- **Suggested Fix**: s/260-byte/348-byte/. Worth batching with #2273/#2415.

---


---

*Filed from [`docs/audits/AUDIT_SAFETY_2026-08-12.md`](docs/audits/AUDIT_SAFETY_2026-08-12.md).*

## Completeness Checks
- [ ] **SIBLING**: Every listed site corrected — note SAFE-D2-01 and SAFE-D6-02 are residuals of #2308/#2273/#2415, which each fixed only one site of the same stale figure
- [ ] **SKILL**: The two `.claude/commands/audit-safety/SKILL.md` corrections landed
- [ ] **TESTS**: Where a doc pins a numeric contract (112 vs 128 B, 260 vs 348 B), a test asserts the number
