# #OPEN PERF-DOC: performance documentation & skill-text drift (3 sites) — stale z_write predicate, structural-vs-empirical split dormancy, contradicted sort-threshold rationale

Consolidated documentation / skill-text drift from the `/audit-performance` half of `/audit-suite renderer-deep` (2026-08-12). Filed as one issue rather than one-per-site — they share a root cause.

Two of these are **corrections to beliefs recorded elsewhere**, not just stale prose:
- **PERF-D2-01** — the Dimension-2 checklist still describes the pre-#2165 `z_write` form of `needs_two_sided_blend_split`. The live predicate is `is_blend && two_sided && order_dependent_glass`. The stale text *inverts* the regression test's meaning. Independently found by REN-D12-01 in the renderer audit.
- **PERF-D2-02** — the split's dormancy is **structural, not empirical**: `collect_static_mesh_draws` force-clears `two_sided` for `MATERIAL_KIND_GLASS`, so only kind-11 MultiLayerParallax can ever reach the predicate. Prior notes recorded this as an empirical observation across tested cells; the real cause is upstream and deterministic, which changes what a fix must touch.
- **PERF-D2-03** — `DRAW_SORT_PARALLEL_THRESHOLD = 3000` is **well-placed** (the in-comment crossover table could not be disproved); only its stated rationale is wrong. The "typical Bethesda cell = 400-1500" band is contradicted by the repo's own runtime baselines (324 / 1839 / 2342 / 2553 / 3440). The same figure also appears in the audit skill text, so it is propagating.

---

## PERF-D2-01

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: `.claude/commands/audit-performance/SKILL.md:91` (Dimension 2 checklist, "Two-sided blend split gate (#1804)")
- **Status**: NEW
- **Description**: The skill text instructs auditors that
  `needs_two_sided_blend_split(&DrawBatch)` "requires `z_write` in addition to
  `is_blend && two_sided`", and frames a split on a non-depth-writing batch as the
  regression to look for. The live predicate has not had a `z_write` limb since
  #2165: it is `is_blend && b.two_sided && b.order_dependent_glass`. The `z_write`
  proxy was removed *deliberately* — FO4 BGEM glass is commonly authored
  `z_write == false`, so the old spelling excluded the population the split exists
  for. An auditor following the skill literally would report the correct current
  code as a regression.
- **Evidence**: [draw.rs](crates/renderer/src/vulkan/context/draw.rs):
  ```rust
  pub(super) fn needs_two_sided_blend_split(b: &DrawBatch) -> bool {
      let is_blend = matches!(b.pipeline_key, PipelineKey::Blended { .. });
      is_blend && b.two_sided && b.order_dependent_glass
  }
  ```
  The doc comment above it states the history explicitly ("Both earlier spellings
  were wrong in opposite directions"), and `DrawBatch::order_dependent_glass`'s own
  doc says "The material kind is the real signal; depth state never was."
- **Impact**: Documentation only — but it is the kind of drift that manufactures a
  false-positive finding in every subsequent Dimension-2 run, which is precisely the
  noise class the audit-hygiene rules exist to suppress.
- **Related**: #1804, #2165, `8e55a714`, #2215. **Cross-audit**: independently found
  as REN-D12-01 in the renderer audit.
- **Suggested Fix**: Update the Dimension 2 checklist bullet to the live predicate
  and re-point the "regression to watch for" at a split reappearing on
  non-`order_dependent_glass` batches (the #2165 particle case), not on
  non-`z_write` ones.

---


---

## PERF-D2-02

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: [static_meshes.rs](byroredux/src/render/static_meshes.rs) — `collect_static_mesh_draws`, the glass single-sided override (~lines 448-452); consumed by `needs_two_sided_blend_split` / `is_refractive_glass` in [draw.rs](crates/renderer/src/vulkan/context/draw.rs)
- **Status**: NEW (mechanism); the dormancy itself is already recorded empirically in `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`'s header
- **Description**: The FNV baseline header and prior audit notes record that
  `blended && two_sided == 0` on every measured cell, and correctly warn that
  changes to `needs_two_sided_blend_split` are runtime no-ops. That is presented as
  an observation. It is in fact a **structural guarantee** for the predicate's
  primary target population, and the guarantee is not documented at either site.
  `is_refractive_glass` accepts two signals: `material_kind == MATERIAL_KIND_GLASS`,
  and `material_kind == 11` (MultiLayerParallax) with a non-zero refraction scale.
  But `collect_static_mesh_draws` — the only producer of glass `DrawCommand`s —
  unconditionally clears `two_sided` for `MATERIAL_KIND_GLASS` *before* the
  `DrawCommand` is constructed. So `b.two_sided` is false for every glass batch by
  construction, and `is_blend && two_sided && order_dependent_glass` can only ever be
  satisfied by an alpha-blended, two-sided, kind-11 MultiLayerParallax draw with
  `multi_layer_refraction_scale > 0` — a vanishingly rare Skyrim+ authoring case.
- **Evidence**:
  ```rust
  // render/static_meshes.rs — the only site that sets two_sided on a glass draw
  let two_sided = if material_kind == byroredux_renderer::MATERIAL_KIND_GLASS {
      false
  } else {
      two_sided
  };
  ```
  The other two `DrawCommand` producers cannot reach the predicate either:
  `render::particles::emit_particles` hardcodes
  `material_kind: MATERIAL_KIND_EFFECT_SHADER` (101, rejected by
  `is_refractive_glass` — this is #2165 working as intended), and
  `render::water::reemit_water_planes` only flips `is_water` on an
  already-emitted command, which `draw.rs` excludes from batch formation via
  `skip_batch`.
- **Impact**: No runtime cost — the dead path costs nothing. The impact is
  interpretive: the split is carried as a live mitigation for the #1804/#2237 glass
  compositing artifact, when for engine-classified glass that artifact is actually
  handled by the single-sided override (which solves it by removing back faces
  entirely, at the documented cost of glass interiors not rendering). Two
  independent mitigations for one artifact, one of them unreachable, with neither
  site cross-referencing the other. This also means Dimension 2's split-related
  checklist items are unfalsifiable on real content and should not be used to
  attribute batch-count movement — consistent with the RT-1 / #2215 conclusion that
  the depth-primary alpha-over sort, not this predicate, drove the
  `bench_draws_batches` rise.
- **Related**: #1804, #2165, #2215, #2237; the `two_sided_blend_split_dormant` note.
- **Suggested Fix**: No code change. Add a cross-reference from
  `needs_two_sided_blend_split`'s doc comment to the `MATERIAL_KIND_GLASS`
  single-sided override in `static_meshes.rs`, stating that the glass arm of
  `is_refractive_glass` is unreachable through `b.two_sided` and that kind-11 is the
  only live population. That converts a repeatedly-rediscovered empirical surprise
  into a stated invariant.

---


---

## PERF-D2-03

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: [mod.rs](byroredux/src/render/mod.rs) — `sort_draw_commands` (`DRAW_SORT_PARALLEL_THRESHOLD`) and the rationale comment in `build_render_data` immediately above the `sort_draw_commands` call
- **Status**: NEW
- **Description**: **The constant itself checks out** — I set out to show 3000 was
  misplaced and could not. The in-comment crossover table (re-measured 2026-07-25 on
  a 7950X after `883f57cd` widened the key to 11 tuples) shows serial ~19% ahead at
  N=2000, still ahead at N=2750, tied at N=3000, and parallel pulling away from
  N=5000. 3000 is the first size where the two are interchangeable, which is the
  right place for the gate. What is stale is the *justification prose* wrapped
  around it: "Typical Bethesda cell counts sit in 400–1500 (Prospector ~811,
  GSDocMitchell ~263, exterior radius-3 grid ~1200), so serial remains the common
  path either way; this only moves the 2000–3000 band."
- **Evidence**: `bench_draws_cmds` from the five checked-in runtime baselines in
  `.claude/audit-baselines/runtime/` (regenerated 2026-06-14 → 2026-08-06):

  | baseline cell | `entities_total` | `bench_draws_cmds` | `bench_draws_batches` | `bench_draws_gpu_calls` |
  |---|---:|---:|---:|---:|
  | `oblivion-ICMarketDistrictTheGildedCarafe` | 701 | 324 | 47 | 4 |
  | `fo3-MegatonPlayerHouse` | 3311 | 1839 | 96 | 9 |
  | `skyrim_se-WhiterunDragonsreach` | 8126 | 2342 | 9 | 2 |
  | `fnv-FreesideAtomicWrangler` | 9271 | 2553 | 89 | 25 |
  | `fo4-InstituteBioScience` | 12448 | 3440 | 753 | 42 |

  Exactly one of five sits inside the quoted 400–1500 band. Three sit in the
  1800–2600 range the comment dismisses as merely "the band this moves", and one is
  *above* the gate — `fo4-InstituteBioScience` at 3440 commands takes the parallel
  path (modulo the in-raster prefix split), which the prose says is uncommon.
- **Impact**: No runtime defect. The risk is that the next person tuning this
  constant reasons from the stale band and lowers the gate to "cover typical cells",
  landing back in the 2000–2750 range where the same comment's measured table shows
  serial winning by ~8-24%. Reported so the rationale and the constant stop
  disagreeing.
- **Related**: #934 / PERF-DC-01, #2173, `883f57cd`; reproduction harness
  `manual_bench_draw_sort_serial_vs_parallel` in
  `byroredux/src/render/draw_sort_key_tests.rs` (`--ignored`).
- **Suggested Fix**: Replace the cited cell counts with the current
  `.claude/audit-baselines/runtime/*.tsv` `bench_draws_cmds` column (or reference the
  directory rather than transcribing numbers, per the audit's own cite-don't-copy
  rule), and restate the conclusion as "one of five baseline cells currently crosses
  the gate" rather than "serial remains the common path either way".

---


---

*Filed from [`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-12.md).*

## Completeness Checks
- [ ] **SIBLING**: Every listed site corrected, incl. the duplicated "400-1500" figure wherever it appears
- [ ] **TESTS**: Where a doc pins a numeric contract, a test or baseline asserts the number


---
# #OPEN SAFE-DOC: safety documentation & comment drift (6 sites) — incl. the unsafe-comment-gap artefact and two stale GPU-struct sizes

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


---
# #OPEN NIFAL-D8-2026-08-12-04: Two independent `BSShaderTextureSet` slot→role tables that already disagree

- **Severity**: MEDIUM
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `single-boundary`
- **Game Affected**: Skyrim SE, FO4, FO76
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:97-238` (shader-type-aware) vs `byroredux/src/cell_loader/refr.rs:139-180` (shader-type-agnostic)
- **Status**: NEW
- **Description**: The importer resolves slots 2/4/7 differently per `shader_type`;
  the REFR overlay resolves the same NIF slot indices through one fixed table
  (`0→diffuse, 1→normal, 2→glow, 3→height, 4→env, 5→env_mask, 6→inner,
  7→specular`) and never sees `shader_type`. The two already disagree on slot 6
  (the overlay is the correct one — see D8-01) and on slots 2/4/7 for shader types
  4/5/11.
- **Evidence**: the two `match` blocks side by side; D8-01 measures which is right
  for slot 6.
- **Impact**: An XTXR swap on a FaceTint / SkinTint / MultiLayerParallax placement
  lands in a different canonical role than the same slot read from the mesh's own
  texture set, so an override changes shading semantics rather than just the
  texture — and any fix to one table silently fails to propagate to the other.
- **Related**: D8-01, D8-03.
- **Suggested Fix**: One `slot_to_role(shader_type, slot)` helper in `crates/nif`,
  called by both sites; the overlay gets `shader_type` from the cached import.

---
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (finding `NIFAL-D8-04`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---
# #OPEN NIFAL-D1-2026-08-12-01: Canonical `Material` doc cites a `grayscale_to_palette_scale` precedent field that does not exist on `Material`

- **Severity**: LOW
- **Dimension**: Material
- **Tier Violated**: none (documentation defect on the canonical type)
- **Game Affected**: all (doc only)
- **Location**: `crates/core/src/ecs/components/material.rs:256-260`
- **Status**: NEW
- **Description**: The #2284 rationale block says the six BSLSP shading scalars
  landed on `Material` "matching the existing `grayscale_to_palette_scale`
  precedent (see that field's doc …)". No such field exists on `Material` — the
  string occurs exactly once in that file, inside this comment. The authored
  scalar lives on the raw `ImportedMaterial` (`crates/nif/src/import/types.rs`,
  written by `byroredux/src/asset_provider/material.rs:1058`) and is
  raw-tier-parked — a *different* tier from the precedent claimed.
- **Evidence**: `grep -c grayscale_to_palette_scale crates/core/src/ecs/components/material.rs` → 1.
- **Impact**: A future audit reading the canonical type's own docs is told a field
  exists that does not, obscuring the genuine parked-at-raw-tier status. No
  runtime effect.
- **Related**: the accurate anchor is the "not yet plumbed to GpuMaterial" comment
  in `crates/renderer/shaders/triangle.frag`.
- **Suggested Fix**: Reword to say the precedent is parked one tier lower on
  `ImportedMaterial`, or land the field for real.

---
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (finding `NIFAL-D1-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---
