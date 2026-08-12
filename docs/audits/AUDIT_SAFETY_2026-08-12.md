# AUDIT — SAFETY · 2026-08-12

**Repo**: `/mnt/data/src/gamebyro-redux` @ `efc089ba` (main)
**Skill**: `/audit-safety` — all 10 dimensions
**Dedup baseline**: `/tmp/audit/issues.json` (400 open + closed issues); every
finding below was grepped against it before reporting.
**Merged from**: three parallel dimension agents —
Dims 1/2/4 (FFI, UB, unsafe discipline), Dims 3/5/10 (leaks, Vulkan spec,
debug-ui/egui), Dims 6/7/8/9 (material layout, IOR/glass, NPC/anim, NIFAL NaN).

---

## 1. Executive Summary

**13 findings** — 0 CRITICAL · 0 HIGH · **4 MEDIUM** · **9 LOW** — plus
**2 hypotheses** that are explicitly *not* confirmed bugs.

| Severity | Count | IDs |
|---|---:|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 4 | SAFE-D4-01, SAFE-D4-03, SAFE-D10-01, SAFE-D7-01 |
| LOW | 9 | SAFE-D1-01, SAFE-D2-01, SAFE-D3-01, SAFE-D4-02, SAFE-D6-01, SAFE-D6-02, SAFE-D7-02, SAFE-D8-01, SAFE-D9-01 |
| Hypotheses (unconfirmed) | 2 | SAFE-D5-H1, SAFE-D5-H2 |

**No unsoundness was found.** Not one finding is a live memory-corruption,
use-after-free, or leak-per-frame bug. The MEDIUM band is dominated by
*contract-integrity* defects: SAFETY comments that assert things which are
false, `unsafe fn` without a `# Safety` section, a safety constant that is
decoupled from the value actually enforced, and one bounded leak on a rare
error path.

### Scope line

All **10 dimensions** were covered. This was a **static-analysis-only** audit:
**no Vulkan device and no on-disk game data were in scope**, and no engine
instance was launched (per the no-parallel-engine-launch rule). Consequently
**every Vulkan-spec claim about per-frame barriers, image layouts, or
render-pass compatibility is a hypothesis, not a confirmed bug** — those are
quarantined in §4 and must not be "fixed" without a validation-layer run or a
RenderDoc capture first. The known **pre-existing exterior `CopyBufferToImage`
VUID** is acknowledged and is not attributed to any finding here.

**Un-owned subsystems** (per `_audit-common.md`'s coverage-gap table — the
shared protocol requires saying which were skipped rather than claiming a full
sweep):

| Subsystem | Reached? | How |
|---|---|---|
| **`crates/fsr3-sys`** (FSR3 upscaler) | **COVERED** | Dim 1 — full read of `src/lib.rs` (669 lines, all 11 `unsafe` tokens), plus the renderer-side consumer `frame_upscaler.rs`. Produced SAFE-D1-01 and 4 of the 6 sites in SAFE-D4-03. |
| **`crates/physics`** | **PARTIALLY REACHED** | Dim 3 only, drop-ordering/leak facet: `PhysicsWorld::remove_body`, both Rapier insertion sites (`sync.rs`, `ragdoll.rs`), and the #1520/#1531 release guard. Solver correctness and constraint decode were **not** examined. |
| **`crates/ui`** (Scaleform/SWF, R4 + M48) | **CENSUS ONLY — effectively SKIPPED** | Swept by the workspace `unsafe`/`SAFETY` recount (result: **zero real `unsafe`**, one prose match). The Ruffle host bridge, AVM2 adapter, and archive navigator were **not** audited. Note that Dimension 10 is the **debug-ui/egui** overlay, which is a different crate (`crates/debug-ui` + `EguiPass`) — it does **not** give `crates/ui` coverage. |
| **`crates/plugin`** (ESM records) | **CENSUS ONLY — effectively SKIPPED** | Swept by the recount (**zero real `unsafe`**, one prose match). Record parsers not audited. |
| **`crates/mod-runtime`** (sandboxed mods, new trust boundary 2026-08-07) | **CENSUS ONLY — effectively SKIPPED** | Swept by the recount (0 `unsafe`, 0 `SAFETY:`). The host/guest trust boundary — the surface `/audit-safety` most exists for — **has still not been examined by anything**. Recommend a dedicated in-scope pass. |
| **`crates/core/src/character`** (CHARAL) | **NOT REACHED** | Skipped entirely. No dimension touched it. |

### Two findings that deserve attention above their severity band

**SAFE-D9-01 (LOW severity, structurally important).** Save-restore is a
**renderer-bound `Material` producer that runs neither `resolve_pbr()` nor any
finiteness gate**. `restore_world` inserts the decoded `Material` straight into
the ECS; `grep -rn "is_finite\|is_nan" crates/save/src byroredux/src/save_io.rs`
returns nothing. The renderer then reads `m.roughness` / `m.metalness` / `m.ior`
directly and interns them into the material SSBO. **NaN on the GPU is UB.** This
is the **persistence half of OPEN #2489**, and it is the leg that **#2489's fix
would not close**: #2489's scope is the `mat.set` write site only, so an
already-poisoned save still restores raw on every subsequent load. Severity is
LOW only because injection today requires deliberate console input or file
tampering — the structural gap is real regardless.

**SAFE-D4-01 (MEDIUM).** The three `vkFlushMappedMemoryRanges` SAFETY comments
in [buffer.rs](crates/renderer/src/vulkan/buffer.rs) assert **three** things,
and **all three are false as written**: (1) the flushed range is *not* contained
in the allocation — `aligned_flush_range` rounds the offset **down** and the
size **up**, making it a strict superset past **both** ends; (2) the size is
**never capped** at the written length, contrary to the `write_mapped` comment;
(3) the claim that gpu-allocator pads sub-allocations to `nonCoherentAtomSize`
is **false about the dependency** — the vendored gpu-allocator **0.28** source
contains **zero** occurrences of `non_coherent` / `atom_size`. It is **not
unsound today** purely because all **20/20** `AllocationScheme` sites use
`GpuAllocatorManaged`, so the widened range stays inside a multi-MB parent
block. The hazard is that the comment tells a future reader the range is bounded
by a dependency guarantee that does not exist.

---

## 2. The `unsafe`-Block Census — KEY NEGATIVE RESULT

> ### **681 / 681 unsafe blocks carry a SAFETY comment.**
>
> The `/audit-safety` SKILL's standing "**~676 SAFETY vs ~761 unsafe**" gap —
> a recurring work item on every run of this audit — **is a token-count
> artefact, not a real comment gap.** The 761 figure counts `unsafe fn`
> declarations, `unsafe trait` / `unsafe impl` declarations, and plain prose
> occurrences of the word "unsafe", none of which are deref sites requiring a
> SAFETY comment. **There is no batched "unsafe without a safety comment"
> finding to report, and this work item can be retired.**

Method: `grep -ro '\bunsafe\b' <dir> --include='*.rs' | wc -l` and
`grep -rio 'SAFETY:' <dir> --include='*.rs' | wc -l` (raw token counts), then a
mechanical sweep of every block with manual confirmation of every flagged site.
A naive ±5-line comment window flagged **15** blocks; **all 15 were manually
opened and are false positives** (long multi-paragraph SAFETY comments, or the
SAFETY line sitting inside the block body).

### Per-crate recount

| Crate / dir | `unsafe` tokens | `SAFETY:` comments | Real unsafe surface |
|---|---:|---:|---|
| `crates/renderer/src` | 761 | 673 | 681 blocks + 77 `unsafe fn` (whole workspace) — the FFI mass |
| `crates/fsr3-sys/src` | 11 | 9 | 9 blocks + 2 `pub unsafe fn` (`Context::create` / `::dispatch`) |
| `crates/nif/src` | 11 | 3 | 2 blocks (`read_pod_vec`, `read_pod_vec_from_cursor`) + 1 `unsafe trait` + 4 `unsafe impl` + 4 prose refs |
| `crates/core/src` | 6 | 6 | 6 blocks (4 cached-pointer derefs, 2 UTF-8) |
| `byroredux/src` | 2 | 2 | 1 block (`shrink_blas_scratch_to_fit` call) + 1 prose ref |
| `crates/pex/src` | 1 | 1 | 1 block (`OpCode::from_u8` transmute) |
| `crates/cxx-bridge/src` | 1 | 0 | 1 `unsafe extern "C++"` **declaration** block (cxx idiom, not a deref site) |
| `crates/plugin/src` | 1 | 0 | **0** — prose only (`// Exact float equality is unsafe`, `esm/reader.rs:130`) |
| `crates/facegen/src` | 1 | 0 | **0** — prose only (`` //! No `unsafe`. ``, `lib.rs:21`) |
| `crates/ui/src` | 1 | 0 | **0** — prose only (log string, `navigator.rs:344`) |
| `crates/bsa/src` | 0 | 5 | 0 (the SAFETY comments explain *why there is no unsafe*) |
| `crates/save`, `crates/hkx`, `crates/mod-runtime`, `crates/sfmaterial` | 0 | 0 | 0 |

Workspace-wide (`crates/` + `byroredux/` + `tools/`, excluding the vendored
`tools/nifskope`): **681 `unsafe` blocks** and **77 `unsafe fn`** declarations.
Of the 77 `unsafe fn`, **71 carry a `# Safety` doc section** — the 6 that do not
are SAFE-D4-03.

**Recorded for the skill**: `crates/plugin`, `crates/facegen` and `crates/ui`
each have **ZERO real `unsafe`**. All three are listed in `audit-safety/SKILL.md`
(lines 21-22) as carrying "one `unsafe`"; all three are **prose matches, not
code**.

---

## 3. Findings

### MEDIUM

---

#### SAFE-D4-01: `GpuBuffer` flush SAFETY comments assert two facts that are false — the range is *not* contained in the allocation, and gpu-allocator does *not* pad to `nonCoherentAtomSize`
- **Severity**: MEDIUM
- **Dimension**: 4 (Unsafe-Block Discipline) — commented block, stated invariant false
- **Location**: [buffer.rs](crates/renderer/src/vulkan/buffer.rs) — `GpuBuffer::flush_if_needed` (SAFETY at :768-771, block at :772), `GpuBuffer::write_mapped`'s flush (SAFETY at :826-832, block at :833), `GpuBuffer::flush_range` (SAFETY at :870-872, block at :873); helper `aligned_flush_range` at :506-516
- **Status**: NEW
- **Description**: Three `vkFlushMappedMemoryRanges` call sites justify their
  `unsafe` with claims about `aligned_flush_range`'s output that the function
  demonstrably does not produce.
  1. `flush_if_needed`: *"The range is contained within this allocation's slice
     of the parent VkDeviceMemory: offset is rounded down and size rounded up to
     nonCoherentAtomSize, which gpu-allocator already pads sub-allocations to."*
     Rounding the offset **down** by up to 255 B moves the range's start *before*
     the allocation, and rounding the size **up** moves its end *after* it. By
     construction the flushed range is a strict superset of the allocation, not a
     subset — the sentence contradicts itself.
  2. The `gpu-allocator already pads sub-allocations to [nonCoherentAtomSize]`
     claim is false about the dependency. The workspace pins
     `gpu-allocator = "0.28"` (`Cargo.toml:74`); the vendored source at
     `~/.cargo/registry/src/index.crates.io-*/gpu-allocator-0.28.0/src` contains
     **zero** occurrences of `non_coherent` / `atom_size` — it sub-allocates on
     `VkMemoryRequirements.alignment` only and has no `nonCoherentAtomSize`
     awareness at all (unlike VMA, which does clamp `vmaFlushAllocation`).
  3. `write_mapped`: *"aligned_size is rounded up to atom size but capped at the
     written length"*. `aligned_flush_range` never caps — `aligned_size >= extra + size`
     unconditionally. There is no clamp anywhere on the path.
- **Evidence**:
  ```rust
  // buffer.rs:506
  fn aligned_flush_range(offset, size) -> (DeviceSize, DeviceSize) {
      let aligned_offset = offset & !(NON_COHERENT_ATOM_SIZE - 1);   // rounds DOWN, out of the alloc
      let extra = offset - aligned_offset;
      let aligned_size = (extra + size + N - 1) & !(N - 1);          // rounds UP, no clamp
      (aligned_offset, aligned_size)
  }
  // buffer.rs:767 — the whole allocation, then rounded outward past both ends
  let (aligned_offset, aligned_size) = aligned_flush_range(alloc.offset(), alloc.size());
  ```
  Existing tests confirm the widening is intentional: `aligned_flush_range_unaligned_offset`
  asserts `off == 0` for input offset 100, and `aligned_flush_range_small_allocation`
  asserts a 48 B allocation flushes 256 B.
- **Impact**: Not unsound today: every call site uses
  `AllocationScheme::GpuAllocatorManaged` (verified — 20/20 `AllocationScheme`
  occurrences in `crates/renderer/src` are `GpuAllocatorManaged`), so each
  allocation is a sub-range of a multi-MB gpu-allocator block whose size is a
  multiple of 256, and the widened range stays inside the parent
  `VkDeviceMemory`. Flushing a neighbouring sub-allocation's bytes only
  publishes host writes that were already made; it does not corrupt.
  The risk is the *comment*: it tells a future reader the range is bounded and
  that the dependency guarantees the padding, so a refactor that (a) switches a
  host-visible buffer to `AllocationScheme::DedicatedBuffer`, or (b) grows one
  past gpu-allocator's 64 MB host-visible block size — which forces a dedicated
  block sized exactly `VkMemoryRequirements.size` — would put `offset + size`
  past the end of the memory object and violate
  VUID-VkMappedMemoryRange-size-01389 with no guard and no test to catch it.
  Per the No-Speculative-Vulkan-Fixes rule, that escalation is stated as a
  hazard, **not** as a confirmed bug: **needs validation-layer verification**
  before any claim that a VUID is currently being violated.
- **Related**: #1759 / TD7-002 (`NON_COHERENT_ATOM_SIZE` power-of-two pin +
  the `device.rs` `debug_assert` on the reported atom size — both intact and
  verified); the closed doc-accuracy siblings #2545 / #2546.
- **Suggested Fix**: Rewrite the three SAFETY comments to state what is
  actually true — "the range is widened outward past the sub-allocation and is
  bounded only by the parent gpu-allocator block, which is a multiple of
  `NON_COHERENT_ATOM_SIZE`" — and add a `debug_assert!` that
  `aligned_offset + aligned_size <= <parent block size>` (or clamp the tail to
  the allocation end when the allocation is the block, i.e. the dedicated case)
  so the dedicated-allocation refactor fails loudly instead of silently.

---

#### SAFE-D4-03: six `unsafe fn` carry no `# Safety` doc section, four of them on the FSR3 barrier boundary
- **Severity**: MEDIUM
- **Dimension**: 4 — batched (per the skill's batching rule)
- **Location**:
  - [frame_upscaler.rs](crates/renderer/src/vulkan/frame_upscaler.rs) — `FrameUpscaler::record_native_blit` (:592), `::record_fsr_barriers_before` (:705), `::record_fsr_depth_restore` (:764), `::record_fsr_barriers_after` (:822)
  - [gbuffer.rs](crates/renderer/src/vulkan/gbuffer.rs) — `GBufferAttachment::destroy` (:180)
  - [screenshot.rs](crates/renderer/src/vulkan/context/screenshot.rs) — `screenshot_record_copy` (:101)
- **Status**: NEW (same class as CLOSED #2544 / #2349 / #2131, different sites)
- **Description**: Of 77 `unsafe fn` in the workspace, 71 carry a `# Safety`
  doc section stating the caller contract; these six do not. All are
  private or `pub(super)`, so blast radius is crate-internal, but the four
  `frame_upscaler` ones are the FSR3 boundary barriers — the contract they
  depend on (`cmd` in the recording state, and each image in the specific
  layout the FSR boundary assumes) is *discussed at length* in their prose docs
  yet never written as a caller obligation. `record_fsr_barriers_after`'s doc
  even records a 900-frame validation run against that contract without ever
  stating the contract. `GBufferAttachment::destroy` relies on the standard
  "no in-flight command buffer references these views" obligation — the inner
  blocks say *"caller of `destroy` (an `unsafe fn`) guarantees …"*, forwarding
  to a contract that does not exist at the signature.
- **Evidence**: `record_fsr_barriers_before` (:705) has **no** doc comment at
  all; the preceding lines are the tail of the previous function's body.
  `screenshot_record_copy` (:96-100) documents *when* it is called and the
  expected swapchain layout in prose, but has no `# Safety` heading.
- **Impact**: MEDIUM per `_audit-severity` Special Rules (`unsafe` without a
  safety comment). Practical risk is a caller added later that does not know
  the layout precondition, producing a `VUID-VkImageMemoryBarrier-oldLayout-01197`
  class error that only shows up under validation layers — precisely the
  failure mode `record_fsr_barriers_after`'s own doc warns will reappear on an
  SDK upgrade.
- **Related**: #2544 (CLOSED — fsr3-sys smoke example; verified fixed, 20/23 →
  0 uncommented), #2349 (CLOSED — `post_passes.rs` split regression), #2131.
  **Cross-audit**: overlaps **REN-D23-05** in the renderer audit, which adds the
  mechanism — clippy misses these because they are **PRIVATE** fns.
- **Suggested Fix**: Add a `# Safety` section to each of the six stating the
  caller obligation (recording-state `cmd`, live device-owned images, and for
  the four FSR fns the specific entry layouts); consider enabling
  `#![warn(clippy::missing_safety_doc)]` on `crates/renderer` so the class stops
  recurring per-refactor.

---

#### SAFE-D10-01: `EguiPass` VkRenderPass leaks on the `recreate_framebuffers` error path
- **Severity**: MEDIUM
- **Dimension**: 10 (egui overlay teardown)
- **Location**: [resize.rs](crates/renderer/src/vulkan/context/resize.rs) —
  `VulkanContext::recreate_swapchain_core`, the `if let Some(mut pass) = self.egui_pass.take()`
  block; [egui_pass.rs](crates/renderer/src/vulkan/egui_pass.rs) — `EguiPass` (no `Drop` impl),
  `EguiPass::destroy`, `EguiPass::recreate_framebuffers`
- **Status**: NEW
- **Description**: The resize path `take()`s `EguiPass` out of `self.egui_pass`,
  then calls `pass.recreate_framebuffers(...)?`. `EguiPass` has **no `Drop`
  impl** (grep `impl Drop for EguiPass` → no match); all of its device-owned
  state (`render_pass`, `framebuffers`) is freed only by the explicit
  `EguiPass::destroy`. On the `?` the taken `pass` is dropped without
  `destroy()`, so its `vk::RenderPass` — and any framebuffers
  `create_framebuffers` had already created before erroring — are never
  destroyed. The old framebuffers *are* safe (drained + destroyed at the top of
  `recreate_framebuffers`), so the leak is exactly one `VkRenderPass` plus the
  partial framebuffer set per failed resize, held until process exit; the
  validation layer will report live objects at `vkDestroyDevice`. As a
  secondary effect `self.egui_pass` stays `None`, silently disabling the
  overlay for the rest of the session.
- **Evidence**: `resize.rs` (format-stable arm):
  ```rust
  if let Some(mut pass) = self.egui_pass.take() {
      if pass.format() == self.swapchain_state.format.format {
          pass.recreate_framebuffers(              // <-- `?` here drops `pass`
              &self.device,                        //     without destroy()
              &self.swapchain_state.image_views,
              self.swapchain_state.extent,
          )?;
          self.egui_pass = Some(pass);
  ```
  Every sibling `take()` in the same function (`self.water`, `self.volumetrics`,
  `self.presentation`, `self.taa`) destroys the taken value **immediately**,
  with no fallible call in between — egui is the only asymmetric site.
  The format-CHANGE arm of the same block is correct (`pass.destroy(&self.device)`
  before rebuild), which is why #2475's fix did not cover this.
- **Impact**: Leak is per failed swapchain recreate (framebuffer creation OOM),
  not per frame — bounded and rare, hence MEDIUM rather than the HIGH that
  `_audit-severity`'s "missing cleanup on swapchain recreate" row would imply
  for the happy path. Blast radius: one render pass + N framebuffers, plus a
  permanently disabled debug overlay.
- **Related**: #2475 (CLOSED, format-change rebuild — the arm that *is* correct);
  Dim 3's allocator-before-device class.
- **Suggested Fix**: Capture the result instead of `?`-ing it —
  `let r = pass.recreate_framebuffers(...); if r.is_err() { pass.destroy(&self.device); } self.egui_pass = r.map(|_| pass).ok(); r?;`
  — or give `EguiPass` a `Drop` that calls `destroy` idempotently (null the
  handles in `destroy` so double-destroy is a no-op).

---

#### SAFE-D7-01: `GLASS_RAY_BUDGET` is a dead constant — the enforced runaway-recursion cap is four unlinked literals in `AdaptiveRayBudget::settings`
- **Severity**: MEDIUM
- **Dimension**: 7 — RT IOR-Refraction safety guards
- **Location**: [shader_constants_data.rs](crates/renderer/src/shader_constants_data.rs) (`GLASS_RAY_BUDGET`) · [ray_budget.rs](crates/renderer/src/vulkan/scene_buffer/ray_budget.rs) (`AdaptiveRayBudget::settings`) · [triangle.frag](crates/renderer/shaders/triangle.frag) (the `atomicAdd` gate)
- **Status**: NEW
- **Description**: The skill treats `GLASS_RAY_BUDGET` as the canonical
  runaway-recursion cap, mirrored into the generated
  `crates/renderer/shaders/include/shader_constants.glsl` and read by the
  shader. The Rust↔GLSL mirror *is* in lockstep (`2_097_152` ↔ `2097152u`), but
  **no shader references the define**: the live gate reads
  `rayBudget.glassRayLimit`, a per-frame CPU-uploaded word whose value comes
  from `AdaptiveRayBudget::settings()`'s four hard-coded tier literals
  (`262_144` / `524_288` / `1_048_576` / `2_097_152`), none of which reference
  `GLASS_RAY_BUDGET`. The canonical constant is therefore decorative: editing
  it changes nothing at runtime, and only the tier-3 literal happens to equal it
  today.
- **Evidence**:
  - `grep -rn GLASS_RAY_BUDGET crates/renderer/shaders/` → three hits, all
    comments or the `#define` itself; zero uses in shader code.
  - `triangle.frag`: `uint old = atomicAdd(rayBudget.rayBudgetCount, GLASS_RAY_COST);`
    then `glassIORAllowed = (old + GLASS_RAY_COST <= rayBudget.glassRayLimit);`
    — `GLASS_RAY_COST` *is* the generated define; `glassRayLimit` is not.
  - `ray_budget.rs`: four `glass_ray_limit:` literals, no import of
    `shader_constants_data`.
- **Impact**: The safety cap and its documented source of truth are decoupled.
  A future "lower the glass budget" change made in the obvious place
  (`shader_constants_data.rs`) silently does nothing, leaving the real cap at
  the tier table — the exact class of drift the generated-header mechanism was
  built to prevent. Also makes the shader comment at `triangle.frag`
  ("`GLASS_RAY_BUDGET` … from shader_constants.glsl") false for half its
  subject.
- **Related**: #1438 (the atomicAdd overshoot is by-design and is *not* reported
  here); `feedback_shader_struct_sync.md`.
  **Cross-audit**: overlaps **REN-D2-02** in the renderer audit.
- **Suggested Fix**: Derive the tier-3 `glass_ray_limit` from
  `shader_constants_data::GLASS_RAY_BUDGET` (and the lower tiers as fractions of
  it), or delete the const + generated define and document the tier table as the
  single source of truth. Either way add a test pinning the two together.

---

### LOW

---

#### SAFE-D9-01: Save-restore is a renderer-bound `Material` producer that runs neither `resolve_pbr()` nor any finiteness validation
- **Severity**: LOW
- **Dimension**: 9 — NIFAL boundary, NaN/Inf on the GPU (safety facet)
- **Location**: [save_io.rs](byroredux/src/save_io.rs) (`build_save_registry`, `.register_component::<Material>("Material")`) · `crates/save/src/{validate,snapshot,driver}.rs` · consumed by [static_meshes.rs](byroredux/src/render/static_meshes.rs)
- **Status**: NEW (persistence half of the OPEN #2489; #2489's scope is the `mat.set` write site only)
- **Description**: `Material` became a save-registered component under #2378 so
  live `mat.set` edits survive a round trip. `restore_world` inserts the decoded
  `Material` straight into the ECS; nothing on that path calls
  `Material::resolve_pbr()` (the *only* NaN detector for `metalness`/`roughness`,
  both plain `f32`) and there is no finiteness gate anywhere in the save crate —
  `grep -rn "is_finite\|is_nan" crates/save/src byroredux/src/save_io.rs` returns
  nothing. The renderer then reads `m.roughness` / `m.metalness` /
  `m.ior` directly in `static_meshes.rs` and interns them into the SSBO.
- **Evidence**: `translate_material` is the only production `resolve_pbr()`
  caller ([material_translate.rs](byroredux/src/material_translate.rs)); the restore path bypasses
  translation entirely (M45.1 applies FormId-keyed deltas *after* cell reload,
  so a restored `Material` overwrites the translated one).
- **Impact**: A non-finite scalar that reaches a save — today only reachable via
  `mat.set nan` (#2489), or via a hand-edited/corrupt save file — is replayed
  into the material SSBO on every subsequent load, with no re-sanitisation.
  NaN on the GPU is UB. Low severity because injection requires deliberate
  console input or file tampering, but note that fixing #2489 alone does **not**
  close this leg: an already-poisoned save still restores raw.
- **Related**: #2489 (OPEN — `mat.set` has no clamp/finite guard), #2378 (CLOSED
  — registered `Material`), #1409/#1411/#1434/#1443 (the finite-guard family).
- **Suggested Fix**: Call `resolve_pbr()` (or a dedicated `sanitize_finite()`)
  on each `Material` after `restore_world`, or add a finiteness sweep to the
  save-side validate gate so a poisoned snapshot is rejected before it is
  written.

---

#### SAFE-D4-02: `evict_unused_blas` call-site SAFETY comments name an idle gate the callee documents as no longer the safety mechanism
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

#### SAFE-D1-01: `Drop for Context` attributes the Vulkan-idle requirement to `Context::create`'s `# Safety` section, which does not state it
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

#### SAFE-D2-01: `GpuInstance` is documented as 112 B at three in-code sites after #2219 grew it to 128 B
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

#### SAFE-D3-01: Deferred-destroy safety contract documented against two dead anchors
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

#### SAFE-D7-02: `triangle.frag`'s glass-passthrough comment still documents the removed texture-equality identity check, contradicting the live `materialKind` gate 50 lines below
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

#### SAFE-D6-01: GLSL↔Rust `GpuMaterial` lockstep pins names, order and Rust offsets — but never the GLSL scalar *type*
- **Severity**: LOW
- **Dimension**: 6 — R1 Material Table Layout Soundness
- **Location**: [gpu_instance_layout_tests.rs](crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs) (`parse_glsl_struct_fields`, `gpu_material_glsl_field_order_matches_rust_struct`) · [material.rs](crates/renderer/src/vulkan/material.rs) (`gpu_material_glsl_field_names_pinned`)
- **Status**: NEW
- **Description**: Three guards cover the contract:
  `gpu_material_size_is_348_bytes` (size),
  `gpu_material_field_offsets_match_shader_contract` (Rust offsets — verified
  complete, see PASS list), and
  `gpu_material_glsl_field_order_matches_rust_struct` (GLSL declaration order).
  `parse_glsl_struct_fields` reads the type token only to decide whether a line
  is a field declaration and then **discards it**, and the name-needle test
  matches bare identifiers (`"materialKind;"`), so a `uint ↔ float` flip inside
  `bindings.glsl` is invisible to `cargo test` while being byte-lethal for any
  field consumed via an implicit widening read.
- **Evidence**: `parse_glsl_struct_fields` pushes `id.to_string()` only; the
  needle list in `material.rs` is names-with-punctuation, no types. I verified
  by hand that all 87 GLSL/Rust type pairs match **today** (`float`↔`f32`,
  `uint`↔`u32`) — this is a missing guard, not a live drift.
- **Impact**: Bounded — most type flips (bindless index used as an array
  subscript, `materialKind` compared against a `uint` constant) fail glslang
  compilation, so the realistic residual is fields read only through an implicit
  `uint→float` widening. Defense-in-depth gap in a HIGH-severity-class contract,
  cheap to close.
- **Related**: #1657 / SF-D8-01 (added the order guard), #806.
- **Suggested Fix**: Have `parse_glsl_struct_fields` return `(type, name)` pairs
  and assert `float↔f32` / `uint↔u32` alongside the existing order comparison.

---

#### SAFE-D6-02: Stale "260-byte" `GpuMaterial` size in the `static_meshes.rs` intern-hot-path comment
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

#### SAFE-D8-01: `AnimationClipRegistry` slot vector grows monotonically — `release()` never returns a slot to reuse, so each evict→reload cycle strands one empty clip header
- **Severity**: LOW
- **Dimension**: 8 — NPC / Animation spawn safety
- **Location**: [registry.rs](crates/core/src/animation/registry.rs) — `AnimationClipRegistry::add`, `::release`, `::get_or_insert_by_path`
- **Status**: NEW
- **Description**: The #790 path-memo dedup is intact and case-insensitive (see
  PASS list), so a *repeated* load of the same `.kf` costs nothing. But
  `release()` (called from the `NifImportRegistry` LRU eviction path, #863)
  clears a slot's contents while keeping the slot occupied, **and** removes the
  reverse-map entry so a later `get_or_insert_by_path` for that key rebuilds via
  `add()` — which unconditionally `self.clips.push(...)` at a fresh index. There
  is no free list. An evict→reload cycle therefore leaks one empty
  `AnimationClip` header per cycle, permanently.
- **Evidence**:
  ```rust
  pub fn add(&mut self, clip: AnimationClip) -> u32 {
      let handle = self.clips.len() as u32;
      self.clips.push(clip);
      handle
  }
  ```
  and in `release`: `self.clip_handles_by_path.retain(|_, h| *h != handle);`
  with the slot left in place (documented as the no-stale-handle invariant).
- **Impact**: Slow, unbounded CPU-side growth under exterior streaming churn —
  a few hundred bytes per evict→reload cycle, so single-digit MB over a long
  session, not a per-frame leak. The deliberate no-stale-handle design is sound;
  what is missing is any bound or telemetry on the stub count.
- **Related**: #790, #863, #2524 (LRU eviction dropping freed handles).
- **Suggested Fix**: Either push released handles onto a free list and have
  `add` prefer it (safe: the slot is empty and its path binding is already gone,
  so reuse cannot alias a live consumer to *different* content — only to the
  same empty stub semantics it already reads), or expose a `stub_slot_count()`
  for the debug stats so the growth is at least observable.

---

## 4. Hypotheses — NOT confirmed bugs

> Per the No-Speculative-Vulkan-Fixes rule: **no engine was run in this audit.**
> No Vulkan device and no on-disk game data were in scope, so there is **zero
> validation-layer evidence** behind anything in this section. These are
> static-reading questions. **Do not change render-pass layouts, barriers, or
> clear behaviour on the strength of these paragraphs** — each names the signal
> that would confirm it.

### SAFE-D5-H1: egui render pass assumes the swapchain image is already in `PRESENT_SRC_KHR`
`create_render_pass` ([egui_pass.rs](crates/renderer/src/vulkan/egui_pass.rs)) declares the color attachment with
`load_op = LOAD`, `initial_layout = PRESENT_SRC_KHR`, `final_layout = PRESENT_SRC_KHR`.
`draw_frame` records `pass.dispatch(...)` after `record_post_passes`, and the
in-source justification is "the presentation pass (FSR 3.1 tail) already wrote
the swapchain image and left it in `PRESENT_SRC_KHR`". If any frame reaches the
egui block without the presentation pass having run (upscaler init failure
promoting to the native-blit fallback mid-frame, presentation rebuild failure
during a resize — `resize.rs` logs and continues in at least one such arm), the
image would be in a different layout and the begin would trip
VUID-vkCmdBeginRenderPass-initialLayout-00897.

**Confirming signal**: a debug build with validation layers enabled, overlay
toggled on, with `BYRO_FSR_FORCE_DISPATCH_FAIL=1` or an `--upscaler` switch
mid-session — report any emitted `VUID-vkCmdBeginRenderPass-initialLayout-00897`
verbatim.

### SAFE-D5-H2: caustic parked-camera EMA has no CLEAR before the atomic accumulation
`CausticPipeline::dispatch` clears the R32_UINT accumulator only on the
`!camera_static` branch; the parked branch runs a decay pass
(`decay = N/(N+1)`, capped `CAUSTIC_DECAY_MAX = 0.995`) and then
`imageAtomicAdd`s on top. This is deliberate (documented progressive
convergence, mirrors the SVGF GI path) and the barriers around it are
well-formed, so the CLEAR-before-COMPUTE invariant is intentionally *relaxed*
here rather than violated. Whether the fixed-point accumulator can drift or
saturate over a long parked session is a numeric question only a capture can
answer.

**Confirming signal**: a RenderDoc capture of the caustic accumulator image
after a multi-minute parked-camera session near water, inspecting the R32_UINT
values for saturation toward `0xFFFFFFFF`. Filed as a hypothesis so it is not
mistaken for a missing clear on a future read.

---

## 5. Guards Verified Intact (PASS — not findings)

Consolidated and deduplicated across all three dimension agents.

### Dimension 1 — FFI

1. **cxx-bridge scope guard.** [lib.rs](crates/cxx-bridge/src/lib.rs) still exposes
   exactly one bridge fn, `native_hello() -> String`, inside a single
   `unsafe extern "C++"` block. **No `*const`, no `*mut`, no `&[u8]`, no `Box<…>`,
   no fn taking a Rust reference.** The C++ side (`cpp/native_utils.{h,cpp}`)
   returns an owned `rust::String` by value — no borrowed-lifetime handoff.
   Dimension 1 remains a scope guard, not a live CRITICAL surface.
2. **fsr3-sys `# Safety` coverage.** Both `pub unsafe fn` carry `# Safety`
   sections with real lifetime contracts: `Context::create` (live, mutually
   compatible handles + matching `vkGetDeviceProcAddr`, all outliving the
   result) and `Context::dispatch` (every handle belongs to the creating
   device, lives through command-buffer execution, is in the renderer-side
   boundary layouts). All 9 `unsafe` blocks in the crate carry SAFETY comments.
3. **fsr3-sys `Context` is `!Send`/`!Sync`** by virtue of the
   `NonNull<RawContext>` field — no `unsafe impl Send`/`Sync` anywhere in the
   crate, so the context cannot escape to another thread.
4. **#2544 fixed and holding** — `crates/fsr3-sys/examples/vulkan_context_smoke.rs`
   now has 20 SAFETY comments across 23 `unsafe` tokens; every block manually
   confirmed commented.
5. **#2158 FSR drop-ordering pins alive** — `frame_upscaler.rs:1054-1080`
   parses `impl Drop for VulkanContext`'s source text and asserts the FSR SDK
   context is retired and the output images freed before `vkDestroyDevice`.

### Dimension 2 — Memory corruption / UB

6. **ECS cached-pointer contract (#35 + #1367) INTACT.** `World::get`
   ([world.rs](crates/core/src/ecs/world.rs):333) returns `Option<ComponentRef<'_, T>>`,
   never a raw pointer past a dropped guard. `ComponentRef` / `QueryRead` /
   `QueryWrite` each declare the guard field **first** and the cached
   `*const T` / `*mut T` second; the guard is `#[allow(dead_code)]`-held for the
   wrapper's whole lifetime. `QueryWrite::storage()` takes `&self` and
   `storage_mut()` takes `&mut self`, so the borrow checker forbids the
   shared/exclusive overlap. All four SAFETY comments accurately describe the
   field layout. No `unsafe impl Send`/`Sync` in `crates/core`, so the raw
   pointers keep the wrappers thread-local.
7. **`#[repr(C)]` GPU-struct soundness.** [gpu_types.rs](crates/renderer/src/vulkan/scene_buffer/gpu_types.rs) contains **no**
   `[f32; 3]` field — the only occurrences of that token are the prohibition
   text itself (:46-48). `GpuInstance` = 128 B with all 15 offsets pinned;
   `GpuCamera` = 336 B pinned and cross-checked against the shipped `.spv` by
   `reflect.rs`'s `uniform_block_size_by_name`; `GpuMaterial` = 348 B pinned
   (`material.rs:1272`, test name matches asserted size). `Vertex`'s `[f32; 3]`
   fields are **not** a std430 hazard: shaders read the global vertex SSBO as a
   flat `float` array at `VERTEX_STRIDE_FLOATS = 26` (104 B), never as a
   `struct` with `vec3` members (`include/bindings.glsl:298`, `ray_hit.glsl`).
8. **NIF bulk POD reads sealed and bounded.** `AnyBitPattern`
   ([stream.rs](crates/nif/src/stream.rs):47) is a `pub(crate) unsafe trait` with an
   explicit `unsafe impl` list — `read_pod_vec::<bool>` **does not compile**. The
   `count.checked_mul(size_of::<T>())` overflow guard is present in **both**
   `NifStream::read_pod_vec` (:439) and `read_pod_vec_from_cursor`
   (`header.rs:364`), and `read_pod_vec` additionally runs `check_alloc`
   (256 MB cap). The header mirror's "caller is responsible for the byte
   budget" contract is honoured at both of its two call sites
   (`header.rs:196-206` and `:225-235`). The three widened element types
   (`BoneWeight`, `Meshlet`, `CullData`, `blocks/bs_geometry.rs:350-352`) are
   all `#[repr(C)]` aggregates of `u16`/`u32`/`[f32; 3]` with no padding —
   4 / 16 / 24 B respectively, as documented. `NiPoint3` is `#[repr(C)]` with
   three scalar `f32`.
9. **sfmaterial `BuiltinType::from_u32` is still a checked match.**
   [types.rs](crates/sfmaterial/src/types.rs):37-57 — sixteen explicit `0xFFFFFF##` arms
   plus `_ => return Err(crate::Error::UnsupportedBuiltin { raw })`. **No
   `transmute` anywhere in the crate** (grep confirms zero occurrences). The
   module doc's "transmute into this enum" wording remains aspirational prose.
10. **pex `OpCode::from_u8` transmute is sound.** [opcode.rs](crates/pex/src/opcode.rs):130-137
    — `if byte >= MAX_OPCODE { return None; }` precedes
    `transmute::<u8, OpCode>`. `OpCode` is `#[repr(u8)]` with `Nop = 0` and
    **50 implicit successors, no skipped discriminants** (manually enumerated:
    `Nop`=0 … `TryLockGuards`=50), and `MAX_OPCODE = 51`. Both halves are pinned
    by `discriminants_match_on_disk_order` (asserts `TryLockGuards as u8 == 50`
    **and** `MAX_OPCODE == 51`) and `from_u8_round_trips_and_rejects_oob`.
    A `MAX_OPCODE` raise without a matching table entry also fails to compile
    (`OPCODES: [_; MAX_OPCODE as usize]`).
11. **No unbounded recursion in the traversal paths.**
    `walk_node_hierarchical` / `walk_node_flat` both bail on
    `depth > MAX_NIF_NODE_DEPTH` at entry (`import/walk/mod.rs:228`, :940);
    `resolve_shape` (`import/collision/shape.rs:42`) carries **both** a
    `visited`-set cycle break (#1269) and a depth cap (MEM-06 / #1385), with
    regression tests for each including bookkeeping balance after a depth bail.
12. **SSBO count clamps present at every mapped-write site.**
    `upload_lights` clamps to `MAX_LIGHTS` (:28), `upload_instances` and
    `upload_previous_models` to `MAX_INSTANCES` (:596), each with a one-shot
    overflow warn and a `debug_assert` proving the write range fits the mapped
    slice. Hashes are taken over the *clamped prefix*, so a dropped tail still
    re-uploads correctly. No `GpuInstance.material_id` over-index path found.

### Dimension 3 — Leaks & drop ordering

13. **Rapier release on cell unload (#1520 / #1531, `34c7a218`) — INTACT.**
    `PhysicsWorld::remove_body` ([world.rs](crates/physics/src/world.rs)) removes through
    `bodies.remove(handle, &mut islands, &mut colliders, &mut impulse_joints,
    &mut multibody_joints, /* remove_attached_colliders = */ true)` — every set
    plus island state. `release_victim_rapier_bodies`
    ([unload.rs](byroredux/src/cell_loader/unload.rs)) sweeps **both** `RapierHandles` and
    `Ragdoll` victims and is called from `unload_cell`. Insertion sites were
    enumerated (`crates/physics/src/sync.rs::physics_sync_system`,
    `crates/physics/src/ragdoll.rs`) — both are covered by one of the two
    branches, so no body class escapes the sweep. Guard test
    `byroredux/src/cell_loader/rapier_release_tests.rs` still asserts
    `body_count() == 0`, `colliders.len() == 0`, and ragdoll joint removal
    post-unload.
14. **Deferred-destroy tick + shutdown drain (#418 / #732 / #1782) — INTACT.**
    `DeferredDestroyQueue<T>` `tick`/`drain` semantics unchanged;
    `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT` (2 frames — matches
    `memory-budget.md` "Deferred-Destroy Queue"). In `context/draw.rs` the
    `texture_registry.begin_frame` + `mesh_registry` / `texture_registry` /
    `accel_manager` ticks all run **after** the both-slot `wait_for_fences`,
    under a live in-source comment naming #418. Shutdown: `flush_pending_destroys`
    (`context/mod.rs`) does `device_wait_idle` → `accel.drain_pending_destroys` →
    `mesh_registry.drain_deferred_destroy` → `texture_registry.drain_pending_destroys`,
    called from both `byroredux/src/main.rs` (window close) and
    `byroredux/src/streaming_helpers.rs`; `AccelerationManager::destroy` also
    drains internally. The #1782 BLAS-scratch queue (`pending_destroy_scratch`)
    is pushed from both `shrink_blas_scratch_to_fit` arms and is ticked/drained
    alongside the BLAS queue.
15. **`AllocatorResource` removal before `VulkanContext::drop` (#1406 / #1477,
    `299e6a84`) — INTACT, including the panic-unwind path.** `impl Drop for App`
    (`byroredux/src/main.rs`) removes the resource and *then* `self.renderer.take()`,
    so the ordering is structural on **every** teardown path, not just the
    `CloseRequested` arm (which repeats it idempotently). `Cargo.toml` pins
    **`panic = "unwind"`** with an explicit FFI-03/#1383 comment forbidding
    `panic = "abort"` — so the unwind actually runs this `Drop`.
16. **GPU allocation inventory** — every item traced from allocation site to
    `VulkanContext::drop`: BLAS result + scratch and TLAS accel/instance/
    instance-device/scratch (`AccelerationManager::destroy` drains
    `pending_destroy_*`, `blas_entries`, `tlas[]`, `skinned_blas`,
    `scratch_buffers[]`, `blas_scratch_buffer`); G-buffer, SVGF history, TAA
    per-FIF history, glass caustic + water-caustic R32_UINT accumulators, SSAO,
    bloom pyramid, volumetrics froxels, ReSTIR reservoirs, composite, upscaler
    (SDK context in the allocator-**independent** block per #2158, output images
    in the guard), exposure, placeholders, depth + depth history, scene buffers
    (MaterialBuffer SSBO), texture + mesh registries — all present before
    `destroy_device`. Per-skinned-entity `SkinSlot` output buffers: released via
    `pending_skin_unload_victims` + the LRU sweep in
    `context/skinned_blas_refit.rs` (drain sits outside the input-buffer/
    bone-buffer guard per #2494) and wholesale in `Drop`; the only `create_slot`
    call site sits under the same `skin_pipeline` + `accel` guard as the eviction
    pass, so creation and release are symmetric.
17. **CPU-side unbounded growth** — `MaterialTable` (`vulkan/material.rs`) is a
    per-frame table with an explicit `clear()`; `AnimationClipRegistry` still
    interns ASCII-lowercased paths (#790 case-insensitive dedup intact);
    `MeshRegistry::pending_vertices` / `pending_indices` have a live compaction
    path that rebuilds from live meshes; `blend_pipeline_cache` is drained by
    `destroy_render_pass_pipelines` on both resize and Drop; `failed_skin_slots`
    is pruned per unload. The known `TextureRegistry` grow-only slot-index
    behaviour is already documented in `memory-budget.md` (#2030 / MEM-D3-01)
    and is **not** re-reported. (The `AnimationClipRegistry` slot-vector growth
    *is* SAFE-D8-01.)

### Dimension 4 — Unsafe-block discipline

18. **681 / 681 unsafe blocks carry a SAFETY comment** (all 15 window-misses
    manually verified as false positives). This includes every `from_raw_parts` /
    `copy_nonoverlapping` / mapped-memory cast in the renderer:
    `scene_buffer/upload.rs` (7 sites), `scene_buffer/descriptors.rs` (5
    hash-slice casts), `buffer.rs`, `material.rs`, `skin_compute.rs`, `water.rs`,
    `svgf.rs`, `presentation.rs`. The two BLAS device-address reads in
    `blas_static.rs` (:209, :221) both carry the `SHADER_DEVICE_ADDRESS`
    justification, and the scratch address goes through `align_scratch_address`
    in **release** as well as debug (#1386 / #659) — VUID-…-pInfos-03715 is
    enforced unconditionally.
19. **#2545 fixed and holding** — the BLAS-scratch-shrink call site in
    `byroredux/src/cell_loader/unload.rs:275-283` has its explicit `SAFETY:` tag
    back, correctly naming the #1782 deferred-retire as the cross-frame guard.
20. **`GpuBuffer` / `Texture` Drop safety nets (#656 / #927 / #1128) intact** —
    `Drop` short-circuits when `allocation` is `None`, skips the
    `debug_assert` during unwind, and drops the `Option<SharedAllocator>` Arc
    on `destroy()`.
21. **gpu-allocator `Arc<Mutex<…>>` interactions** — every `AllocationScheme` in
    `crates/renderer/src` is `GpuAllocatorManaged` (20/20 sites — no dedicated
    allocations), and allocator locks are taken with `.lock().expect(...)` scoped
    to the single `free`/`allocate` call, never held across an FFI dispatch.

### Dimension 5 — Vulkan spec (static half)

22. **TLAS resize `device_wait_idle` (#1390, `a7e1502b`) — INTACT.**
    `ensure_tlas_state` ([tlas.rs](crates/renderer/src/vulkan/acceleration/tlas.rs)) calls
    `device.device_wait_idle()` inside the `need_new_tlas` branch before
    `destroy_acceleration_structure` + `buffer`/`instance_buffer`/
    `instance_buffer_device` destroys.
23. **TLAS UPDATE primitive-count contract** double-guarded: `decide_use_update`'s
    length compare plus the explicit `if use_update && instance_count !=
    tlas.built_primitive_count { use_update = false }` (VUID-…-pInfos-03708), with
    a `debug_assert_eq!` on the UPDATE arm and a second one pinning
    `last_blas_addresses.len() == instance_count` (#914 / #1121). Source/dst flags
    both come from the shared `UPDATABLE_AS_FLAGS`.
24. **Skinned-BLAS refit** validates against the BUILD via `validate_refit_counts`
    (vertex + index, #907) **and** `validate_refit_flags` (#1145) before the
    UPDATE; a mismatch drops the entry for a fresh BUILD instead of proceeding. A
    mesh remap that changes `vertex_count` also forces slot recreate +
    `drop_skinned_blas` (#1297 / #1298), and `SKINNED_BLAS_REFIT_THRESHOLD` forces
    a periodic full rebuild — matching `memory-budget.md`'s 600-frame figure.
25. **AS input buffers carry `SHADER_DEVICE_ADDRESS`** (+
    `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`) at `mesh.rs`,
    `skin_compute.rs::create_slot`, and every `blas_static.rs` / `tlas.rs`
    allocation; `GpuMesh::rt_capable` mirrors it as the BLAS-eligibility gate.
26. **`VK_KHR_ray_query`** is extension-probed (`ray_query_supported`) and the
    `PhysicalDeviceRayQueryFeaturesKHR` / `acceleration_structure` /
    `buffer_device_address` features are all gated on that same flag.
27. **`initialize_layouts` FIF/mip coverage across 8 subsystems** — gbuffer,
    SVGF, TAA, caustic, water-caustic (`for slot in &self.slots`), bloom
    (`MAX_FRAMES_IN_FLIGHT * (BLOOM_MIP_COUNT + BLOOM_MIP_COUNT-1)`, every down +
    up mip of every frame), volumetrics (lighting + integrated volumes, both FIF,
    plus noise volumes), SSAO (`for &img in &self.ao_images`) — **no slot or mip
    omitted**, and each is re-run on the corresponding resize path.
28. **CLEAR-before-COMPUTE** — water-caustic `clear_pre_render_pass` (per-frame,
    called from `draw.rs` before the main RP) and caustic's moving-camera branch
    both clear with correct pre/post barriers; volumetrics' `initialize_layouts`
    seeds the no-fog sentinel `[0,0,0,1]` (deliberately not a plain zero-fill —
    that is the safety net for the dispatch-skip path). The parked-camera caustic
    branch is SAFE-D5-H2 (intentional relaxation, hypothesis only).
29. **`VOLUMETRIC_OUTPUT_CONSUMED` — READ, not assumed: it is `true`**
    (`volumetrics.rs`). Both callers gate on the const either way —
    `context/post_passes.rs::record_volumetrics_pass` wraps the dispatch in
    `if VOLUMETRIC_OUTPUT_CONSUMED`, and `context/draw.rs` pins the UBO mirror
    slot to `1.0`/`0.0` off the same const.

### Dimension 6 — R1 material table layout

30. **`GpuMaterial` is 348 B**: 87 fields × 4 B, all flat scalar `f32`/`u32`,
    zero `[f32; 3]`, zero implicit padding (verified by programmatic field
    extraction, not by trusting the comment). Pin `gpu_material_size_is_348_bytes`
    asserts 348 and **the test name matches the asserted size**.
31. **All 87 fields are offset-asserted.**
    `gpu_material_field_offsets_match_shader_contract` has an `offset_of!`
    assertion for **every one of the 87 fields**, with no assertion for an absent
    field — including all twelve newest role indices (`tint_map_index` @300 …
    `decal_map_3_index` @344). Verified by set-differencing the struct against
    the test body.
32. **Prose is current**: `material.rs` header (348 + full 272→260→296→300→348
    history), `docs/engine/shader-pipeline.md` ("348 bytes", offset table ending
    "→ total **348**"), `docs/engine/memory-budget.md` (`348 B`, `MAX_MATERIALS`
    16 384 → 5.7 MB). `docs/engine/renderer.md` reads "300 bytes at R1 (348 bytes
    today)" — historically framed, correct. (`static_meshes.rs`'s "260-byte" is
    SAFE-D6-02.)
33. **`GpuMaterial::default()` initialises all 87 fields** (none left implicit),
    so the raw-348-B byte hash has no uninit hole; the twelve new role indices
    default to `0` and dedup to slot 0. **`hash_gpu_material_fields` walks all 87
    fields** — no field excluded from dedup (a missed field would silently alias
    distinct materials).
34. **GLSL mirror**: `include/bindings.glsl`'s `struct GpuMaterial` has exactly
    87 fields, in the same order, with matching scalar types (the *type pin* gap
    is SAFE-D6-01). All four standalone `GpuInstance` mirrors (`triangle.vert`,
    `ui.vert`, `water.vert`, `caustic_splat.comp`) match `bindings.glsl` exactly
    (13 fields each, verified by parse). **`ui.vert`** declares no
    `struct GpuMaterial`, never binds/reads `MaterialBuffer` — the #785
    stale-hunk regression has not returned.
35. **Intern cap ↔ upload truncation in lockstep**: `MaterialTable::intern_by_hash`
    returns `0` with a `Once`-gated warn at `len >= MAX_MATERIALS (16384)`;
    `upload_materials` `debug_assert`s `len <= MAX_MATERIALS` then
    `.min(MAX_MATERIALS)`; `MaterialTable::new()`/`clear()` re-seed slot 0 with
    the neutral default so `material_id == 0` is always safe to read.
    `GpuInstance.material_id` is in-range by construction: only **two**
    production sites assign it (`render/static_meshes.rs`, `render/particles.rs`),
    both from `intern_by_hash`.

### Dimension 7 — RT IOR refraction

36. **`REFRACT_PASSTHRU_BUDGET = 2` enforced two ways** in `triangle.frag`: the
    loop header (`passthru = 0; passthru <= BUDGET`, max 3 traces) *and* the
    continuation guard (`… && passthru < REFRACT_PASSTHRU_BUDGET`), so the
    terminating iteration always commits. **Unbounded recursion is structurally
    impossible.**
37. **Identity gate is `materials[hInst.materialId].materialKind ==
    MATERIAL_KIND_GLASS`** (`a09d2b76`), not texture equality. Present and live
    (the stale comment beside it is SAFE-D7-02).
38. **Frisvad basis is the ACTIVE path** for IOR refraction roughness spread:
    `buildOrthoBasis(refractDir, rRight, rUp)` (`include/math_common.glsl`,
    Frisvad 2012) guarded by `dot(refractDir, refractDir) > 0.0001`. The naive
    `cross(N, up)` degenerate is gone (#820).
39. **`DBG_VIZ_GLASS_PASSTHRU = 0x80` is collision-free** — extracted all **31**
    `DBG_*` constants from `shader_constants_data.rs`; every value is unique
    across `0x1 … 0x40000000`; `0x80` is held only by `DBG_VIZ_GLASS_PASSTHRU`,
    and the generated header agrees (`#define DBG_VIZ_GLASS_PASSTHRU 128u`).
40. **`GLASS_RAY_COST = 4`** is in Rust↔generated-GLSL lockstep and *is* consumed
    by the shader. #1438's unconditional atomicAdd overshoot is present **by
    design** and documented in-place — not reported as new. (The `GLASS_RAY_BUDGET`
    half is SAFE-D7-01.)
41. **IOR miss fallback honours the interior/exterior split**: `refrColor =
    isExteriorGlass ? (skyTint.xyz*0.5 + sceneFlags.yzw*0.5) : sceneFlags.yzw` —
    interiors get cell ambient only, no global sky-tint leakage (#1125).

### Dimension 8 — NPC / animation spawn

42. **#772 `FLT_MAX` pose-fallback sentinel fully wired**: `FLT_MAX_SENTINEL = 3.0e38`
    + `is_flt_max` gate applied on **all three** consumers — `anim/transform.rs`
    (static-pose T/R/S), `anim/bspline.rs` (B-spline fallback pose, 6 sites incl.
    translation/rotation/scale), `anim/channel.rs` (float/colour channels). The
    #1443 `is_key_value_sane` mainline sanitizer is a superset and is also live.
    **No game-era gating anywhere** — the FNV/FO3 B-spline reachability
    correction holds.
43. **`AnimationClipRegistry` interns by lowercased path** on both
    `get_or_insert_by_path` and `get_by_path` (in-place ASCII fold,
    allocation-free when already canonical);
    `get_or_insert_by_path_is_case_insensitive` pins it (#790 / #866). No
    per-cell keyframe-set leak.
44. **`SkinSlotPool` overflow warn** — `MAX_TOTAL_BONES = 196608` guard fires as
    specified: `SkinSlotPool::allocate` returns `None` past `max_slot`, bumps
    `overflow_attempt_count` (saturating), and logs **once** via
    `overflow_warned`. The consumer (`byroredux/src/render/skinned.rs`)
    `continue`s on `None` so the entity's `skin_offsets` entry stays unset and
    the draw loop falls through to `bone_offset = 0` = identity/bind pose — **no
    over-index**. Guard tests `at_capacity_fills_palette_completely` and
    `over_capacity_breaks_loop_and_truncates_offsets`
    (`byroredux/src/render/bone_palette_overflow_tests.rs`) still assert this;
    telemetry surfaces via `DebugStats::skin_pool_overflow_attempts`.

### Dimension 9 — NaN/Inf on the GPU (safety facet)

45. **Producer enumeration (the core check).** All `Material` construction sites
    reachable by the renderer:
    1. `material_translate.rs::translate_material` — the NIFAL boundary. Single
       linear path, **no early return** between construction and
       `material.resolve_pbr()`; the deliberate `unwrap_or(f32::NAN)` sentinels
       for `metalness`/`roughness` are always resolved and clamped
       (`[0,1]` / `[0.04,1]`). **PASS.**
    2. `render/static_meshes.rs` no-`Material` fallback — constructs finite
       literals directly (`0.5`, `0.0`, `DEFAULT_DIELECTRIC_IOR`, `[1.0;3]`, …).
       **PASS.**
    3. `Material::default()` — `metalness: 0.0`, `roughness: 0.5`,
       `ior: DEFAULT_DIELECTRIC_IOR`, Disney lobe zeros. **PASS.**
    4. `byroredux/src/cornell.rs` (`matte`/`pbr`/`pbr_bsdf`/`pbr_bsdf_lobes`/
       `glass`/`emissive`/`fire_refraction`) — all `..Default::default()` plus
       finite literals or caller-supplied scalars. **PASS as producers.**
    5. `commands/scene.rs::MatSetCommand` — raw `parse::<f32>()` with no clamp
       and no `resolve_pbr`. **Already OPEN as #2489** — not re-reported.
    6. Save-restore (`save_io.rs` registry) — **SAFE-D9-01**.
46. **`translate_material`'s `ior` seed is finite-guarded**:
    `material_optical_scalar` does `refraction_strength.is_finite() ?
    clamp(0,1) : 0.0` for `MATERIAL_KIND_FIRE_REFRACTION` and returns
    `DEFAULT_DIELECTRIC_IOR` otherwise.
47. **Collision translate** (`crates/nif/src/import/collision/{mod,shape}.rs`):
    `finite()` / `finite_vec()` reject NaN/±Inf on `BhkSphereShape` radius,
    `BhkBoxShape` half-extents (`.abs()`, so never negative),
    `BhkCapsuleShape`/`BhkCylinderShape` radii, and convex-hull vertex sets.
    `BhkMultiSphereShape` explicitly tests `!center.is_finite() || !radius.is_finite()`
    per sphere; `BhkConvexListShape` inherits the guard through its recursive
    children and returns `None` (not an empty `Compound`) when all children fail.
    Cycle guard + `MAX_COLLISION_SHAPE_DEPTH = 64` bound the recursion. The scale
    multiplier is the per-game `scene.havok_scale` constant, **not** the
    unclamped REFR `XSCL` that produced #2543 (whose clamp in
    `synthesize_packed_havok_proxy` is separately still present, with its
    `packed_proxy_clamps_extreme_finite_scale` regression test).
48. **Particle emitter extract boundary**: `extract_emitter_params`
    (`import/walk/mod.rs`) rejects the whole emitter unless *all* of
    speed/speed_variation/declination/declination_variation/planar_angle/
    planar_angle_variation/initial_radius/radius_variation/life_span/
    life_span_variation are finite **and** `life_span > 0.0` **and**
    `initial_radius >= 0.0`; `NiPSysGrowFadeModifier.base_scale` is separately
    filtered on `is_finite() && > 0.0`. `extract_emitter_rate`'s `sane()` requires
    `is_finite() && 0.0 < r < 3.0e38` (also rejecting the FLT_MAX sentinel).
    Downstream, `particle_system` re-checks `rate`/`start_size` finiteness before
    touching `spawn_accumulator` and caps the population at `em.max_particles` —
    no unbounded allocation from a large-but-finite rate.

### Dimension 10 — debug-ui / egui overlay

49. **`DebugUiState`** ([lib.rs](crates/debug-ui/src/lib.rs)) confirmed **CPU-only** — no `vk::`
    handle, no allocator; the Vulkan half is entirely `EguiPass`.
50. **Teardown before device-destroy — INTACT.** `VulkanContext::drop` does
    `device_wait_idle` → `self.egui_pass.take()` → `pass.destroy(&self.device)`,
    and the taken local drops (running `egui_ash_renderer::Renderer`'s own `Drop`,
    which frees its descriptor pool + per-texture images) at the end of that
    `if let`, hundreds of lines before `destroy_device`. `destroy()` also flushes
    the final `pending_free` first (#1427) so the renderer's descriptor-pool
    accounting balances. (The *error*-path leak is SAFE-D10-01.)
51. **One-frame texture-free defer — INTACT.** `dispatch` step 1 frees the
    *previous* frame's `TexturesDelta.free`, step 5 stashes the arriving one;
    `draw_frame` waits **both** `in_flight[frame]` and `in_flight[prev]` before
    reaching it — strictly stronger than the one-frame defer needs.
52. **Graphics-queue Mutex scope (CONC-D1-01 / #1713) — INTACT, not widened.**
    `EguiDispatchCtx::queue` is a `&Mutex<vk::Queue>`; the guard is bound inside
    the `if !output.textures_delta.set.is_empty()` block around `set_textures`
    only. Tessellate + `cmd_begin_render_pass`/`cmd_draw`/`cmd_end_render_pass`
    run with the queue released. The `cmd_draw` failure path still balances the
    render pass before propagating (REG-05 / #1637).
53. **Shared allocator hold duration — minimal.** Verified in the vendored
    dependency (`egui-ash-renderer` 0.11.0, `src/renderer/allocator/gpu.rs`):
    exactly one `lock()` site, `get_allocator()`, whose `MutexGuard` is
    function-scoped to each `create_buffer` / `create_image` / free. The lock is
    never held across the `set_textures` queue submit + wait.
54. **Command-pool sharing**: `EguiPass` uploads through `self.transfer_pool`,
    shared with `with_transfer_commands`. `grep thread::spawn` over
    `byroredux/src` + `crates/renderer/src` returns **no** matches, so there is
    no second thread contending for the pool — the external-synchronization
    requirement is satisfied by single-threaded ownership.

---

## 6. Documentation & Skill-Text Drift

### Corrections TO the `/audit-safety` skill itself

These are defects in `audit-safety/SKILL.md`, not in the engine. They caused
wasted effort on this run and will recur on every future run until fixed.

| # | Skill claim | Ground truth |
|---|---|---|
| **S-1** | Dimension 4 standing work item: "**~676 SAFETY vs ~761 unsafe**" implies ~85 uncommented blocks. | **Artefact.** 681/681 unsafe blocks are commented; 71/77 `unsafe fn` have `# Safety`. The 761 counts `unsafe fn` / `unsafe trait` / `unsafe impl` declarations and plain prose. **Retire the work item**; replace with "audit the 6 `unsafe fn` missing `# Safety`" (SAFE-D4-03). |
| **S-2** | Dimension 2 names **`StorageRef` / `StorageRefMut`** in `crates/core/src/ecs/query.rs` as the cached-pointer types to inspect. | **These symbols do not exist anywhere in the tree.** The live types are **`QueryRead` / `QueryWrite` / `ComponentRef`**. Update the dimension text. |
| **S-3** | SKILL.md lines 21-22 list `crates/plugin`, `crates/facegen` and `crates/ui` as each carrying "one `unsafe`". | All three are **prose matches, not code** — each has **zero real `unsafe`**. (`plugin`: `// Exact float equality is unsafe`, `esm/reader.rs:130`; `facegen`: `` //! No `unsafe`. ``, `lib.rs:21`; `ui`: a log string, `navigator.rs:344`.) |
| **S-4** | Dimension 7 describes the `DBG_*` debug-visualisation catalog as **24 bits**. | It is **31 bits** (all extracted from `shader_constants_data.rs`, values unique across `0x1 … 0x40000000`). |
| **S-5** | Dimension 7 treats **`GLASS_RAY_BUDGET`** as the canonical enforced runaway-recursion cap. | It is a **dead constant** — see SAFE-D7-01. The live cap is `rayBudget.glassRayLimit`, sourced from four unlinked tier literals in `AdaptiveRayBudget::settings`. |

### In-code / doc drift found in the engine

Every one of these is filed as a finding above; consolidated here as the drift
picture, since five of thirteen findings are stale numbers or dead anchors in
safety-critical contracts:

- **Stale GPU-layout figures**: `GpuInstance` documented as 112 B at four sites
  after #2219 grew it to 128 B (**SAFE-D2-01**); `GpuMaterial` documented as
  260 B at the `static_meshes.rs` intern hot path after `1d94eb24` made it 348 B
  (**SAFE-D6-02**). Both are the *exact* failure class the shader-struct-sync
  rule exists to catch — the authoritative comment on a struct with five GLSL
  mirrors stating the wrong size.
- **Dead doc anchors**: `deferred_destroy.rs`'s `DEFAULT_COUNTDOWN` contract
  points at `draw.rs:889` and a file
  (`crates/renderer/src/vulkan/acceleration.rs`) that no longer exists
  (**SAFE-D3-01**); `context/draw.rs:1599` carries a second stale reference to
  the same removed file.
- **SAFETY comments asserting false invariants**: the three `buffer.rs` flush
  sites (**SAFE-D4-01**) and the two `evict_unused_blas` call sites
  (**SAFE-D4-02**), the latter naming an idle gate the callee itself documents
  as *"NOT the safety mechanism it used to be"*.
- **Cross-referenced contract that isn't there**: `Drop for Context` attributes
  the Vulkan-idle requirement to `Context::create`'s `# Safety`, which does not
  state it (**SAFE-D1-01**).
- **Shader comment arguing both sides**: `triangle.frag` documents the removed
  texture-equality identity check ~50 lines above the live `materialKind` gate
  that replaced it (**SAFE-D7-02**).

Doc-rot note: prose the audit checked and found **current** —
`docs/engine/shader-pipeline.md` (348 B + offset table), `docs/engine/memory-budget.md`
(348 B, `MAX_MATERIALS` 16 384 → 5.7 MB, deferred-destroy depth, 600-frame
skinned-refit figure), `docs/engine/renderer.md` ("300 bytes at R1 (348 bytes
today)" — historically framed, correct). `material.rs`'s header carries the full
272→260→296→300→348 history. Older `docs/audits/*` are historical records and
were not counted as drift.

---

## 7. Prioritized Fix Order

| # | ID | Sev | Why this position | Cost |
|---|---|---|---|---|
| 1 | **SAFE-D4-01** | MEDIUM | Three SAFETY comments assert three false facts on the mapped-memory flush path — the single most consequential place in the tree for a reader to be misled. Not unsound today only because 20/20 sites are `GpuAllocatorManaged`; the comment actively invites the refactor that breaks it. Fix = rewrite prose + add the `debug_assert`. | Small |
| 2 | **SAFE-D9-01** | LOW | Structurally more serious than its band: a renderer-bound `Material` producer with **no** finiteness gate, and the leg that **#2489's fix will not close**. NaN on the GPU is UB. Fix it *with* #2489 or the #2489 close will be misleading. | Small |
| 3 | **SAFE-D4-03** | MEDIUM | Six `unsafe fn` with no `# Safety`, four on the FSR3 barrier boundary. **Cross-audit**: pairs with REN-D23-05, which supplies the mechanism (clippy misses them because they are PRIVATE). Fix the six *and* enable `#![warn(clippy::missing_safety_doc)]` on `crates/renderer` so the class stops recurring per-refactor — that is what makes this worth doing once. | Small–Medium |
| 4 | **SAFE-D10-01** | MEDIUM | The only real resource leak found. Bounded (per failed swapchain recreate) but also silently kills the debug overlay for the session. `Drop`-for-`EguiPass` with an idempotent `destroy` is the durable fix. | Small |
| 5 | **SAFE-D7-01** | MEDIUM | The documented safety cap is decoupled from the enforced one. Nothing is broken today, but a future "lower the glass budget" edit in the obvious file is a silent no-op — the exact drift the generated-header mechanism exists to prevent. Needs a pinning test either way. | Small |
| 6 | **Skill corrections S-1 … S-5** | — | Zero code risk, and S-1 alone retires a recurring per-run work item while S-2's phantom symbols send every future Dimension-2 pass on a dead search. Highest effort-to-value ratio in the list. | Trivial |
| 7 | **SAFE-D2-01 + SAFE-D6-02** | LOW ×2 | Batch together with the already-open #2273 / #2415 stale-size siblings — one sweep of the GPU-layout figures (112→128, 260→348, recompute the KB/frame numbers). | Trivial |
| 8 | **SAFE-D3-01 + SAFE-D4-02 + SAFE-D1-01 + SAFE-D7-02** | LOW ×4 | Doc/anchor corrections on safety-critical contracts. **SAFE-D3-01 shares a site with REN-D5-04** — coordinate. SAFE-D4-02 can also drop the vestigial `unsafe` from `evict_unused_blas`. | Trivial each |
| 9 | **SAFE-D6-01** | LOW | Defense-in-depth: extend `parse_glsl_struct_fields` to return `(type, name)` and assert `float↔f32` / `uint↔u32`. No live drift — all 87 pairs verified matching today. | Small |
| 10 | **SAFE-D8-01** | LOW | Free list or `stub_slot_count()` telemetry on `AnimationClipRegistry`. Single-digit MB over a long session; make it observable at minimum. | Small |
| — | **SAFE-D5-H1, SAFE-D5-H2** | — | **DO NOT FIX.** Run the named confirming signal first (validation-layer run with `BYRO_FSR_FORCE_DISPATCH_FAIL=1` / overlay toggle for H1; RenderDoc capture of the parked caustic accumulator for H2). Only then decide whether there is anything to fix. | — |
| — | **`crates/mod-runtime`** | — | Not a finding — a **coverage gap**. The 2026-08-07 host/guest trust boundary has still never been examined by any audit. Recommend a dedicated `/audit-safety` pass with that crate explicitly in scope. | — |

---

## Tests Run

`cargo test -p byroredux-renderer` → **566 passed, 0 failed, 0 ignored** (0.24 s)
+ doc-tests 0 passed / 1 ignored.

Includes all five **`scene_descriptor_reflection_tests`** (rt-enabled/disabled ×
triangle/water), `camera_ubo_size_matches_gpu_camera_in_every_shader`,
`every_committed_spv_is_spirv_1_0`, the bindless-descriptor reflection tests,
and the material intern-overflow tests. This is the only SPIR-V binding-drift
channel visible to `cargo test`, and it is **green**.

The Dims 6/7/8/9 slice did not run `cargo test` — its layout claims are
arithmetic (87 × 4 B scalars under `#[repr(C)]` ⇒ no padding ⇒ 348) and were
verified programmatically (struct/test/GLSL parsing) rather than by eye.

---

## Coverage Statement

**Dimensions 1, 2, 4** — recounted `unsafe`/`SAFETY` across all 24 `crates/` +
`byroredux/` + `tools/` (excluding the vendored `tools/nifskope`). Dim 1: full
read of `crates/cxx-bridge` (5 files) + `crates/fsr3-sys/src/lib.rs` (669 lines,
all 11 `unsafe` tokens) + `frame_upscaler.rs`. Dim 2: `crates/core/src/ecs/{query,world}.rs`,
`crates/core/src/string/mod.rs`, `crates/nif/src/{stream,header,blocks/bs_geometry,types}.rs`,
`crates/nif/src/import/{walk,collision}`, `crates/sfmaterial/src/types.rs`,
`crates/pex/src/opcode.rs`, `crates/renderer/src/vulkan/scene_buffer/*`,
`crates/renderer/src/{vertex.rs,vulkan/material.rs}`, shader-side
`include/{bindings,ray_hit,shader_constants}.glsl`. Dim 4: mechanical sweep of
all 681 unsafe blocks + 77 `unsafe fn` with manual confirmation of every flagged
site; deep spot-checks on the ash dispatch wrappers, the gpu-allocator
interactions, and every `from_raw_parts`/`cast` on mapped memory.

**Dimensions 3, 5, 10** — all three drop-ordering regression guards verified
against current code; full GPU allocation inventory traced from allocation site
to `VulkanContext::drop`; CPU-side growth checked on MaterialTable,
AnimationClipRegistry, MeshRegistry pending pools, blend-pipeline cache,
failed-skin-slot set. Dim 5 static half: AS geometry flags / device addresses /
usage flags, TLAS BUILD-vs-UPDATE count + flag contract, skinned refit
validation, #1390 resize wait, `initialize_layouts` across 8 subsystems,
clear-before-compute, `VOLUMETRIC_OUTPUT_CONSUMED` read live, ray-query feature
gate. Dim 10: `DebugUiState` / `EguiPass` split, teardown ordering, one-frame
texture defer, queue-mutex scope, allocator-mutex hold (verified in the vendored
crate), command-pool thread ownership.

**Dimensions 6, 7, 8, 9** — `crates/renderer/src/vulkan/material.rs` (full
struct + Default + hash + intern + both layout pins + the GLSL needle list),
`scene_buffer/{constants,upload,ray_budget,gpu_types,gpu_instance_layout_tests}.rs`,
`shader_constants_data.rs` (all 31 `DBG_*` bits extracted and de-duplicated by
value), `shaders/include/{bindings,shader_constants,math_common}.glsl`,
`shaders/{triangle.frag,triangle.vert,ui.vert,water.vert,caustic_splat.comp}`,
`crates/core/src/animation/registry.rs`,
`crates/core/src/ecs/resources/skin_slot_pool.rs`,
`crates/nif/src/anim/{channel,keys,transform,bspline}.rs`,
`crates/nif/src/import/collision/{mod,shape}.rs`, `crates/nif/src/import/walk/mod.rs`,
`crates/core/src/ecs/components/material.rs`,
`byroredux/src/{material_translate,cornell,save_io}.rs`,
`byroredux/src/render/{static_meshes,particles,skinned,bone_palette_overflow_tests}.rs`,
`byroredux/src/systems/particle.rs`, `byroredux/src/commands/scene.rs`,
`docs/engine/{shader-pipeline,memory-budget,renderer}.md`.
Dimension 9 covers the **safety facet only** (non-finite scalars reaching the
GPU, unbounded allocation); correctness-of-mapping is `/audit-nifal`'s and was
deliberately not touched.

**Not covered**: no engine run, hence no validation-layer or RenderDoc evidence
for any per-frame barrier / layout / render-pass claim — everything of that
class is confined to §4. NIFAL correctness-of-mapping. `crates/core/src/character`
(CHARAL) was not reached at all; `crates/mod-runtime`, `crates/ui` and
`crates/plugin` were reached only by the mechanical `unsafe` census; and
`crates/physics` only on the drop-ordering/leak facet.
