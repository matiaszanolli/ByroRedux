=================== ISSUE #3986 ===================
STATE: OPEN
LABELS: bug, renderer, medium, shaders
TITLE: REN-2026-09-06-D2-01: `rayHitHasCoverage` omits the decal-slot alpha composite the raster path applies, so alpha-tested decal meshes cast a different silhouette than they present

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D2-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/include/ray_hit.glsl` —
  `rayHitHasCoverage` / `alphaComparePass`. Raster counterpart:
  `crates/renderer/shaders/triangle.frag`, the `materialDecals` overlay loop
  and the inline `mat.alphaTestFunc` block immediately after it. Data source:
  `crates/renderer/src/vulkan/material.rs` (`decal_map_0_index`, and
  `supplemental_texture_slot::DECAL_0`, imported as `slot` at the call site),
  populated from `byroredux/src/render/static_meshes.rs`
  (`supplemental_texture_indices[slot::DECAL_0] = texture_indices.decals[0]`).
  GLSL mirror: `mat.decalMap0Index` in `crates/renderer/shaders/include/bindings.glsl`.
- **Status**: **NEW.** No open issue matches (`decal`, `silhouette`,
  `coverage`, `alpha-test`, `rayHitHasCoverage`, `alphaComparePass` all return
  nothing across `/tmp/audit/renderer/open_titles.txt` and the JSON cache). No
  prior `docs/audits/` report names `rayHitHasCoverage` in this context.
  Sibling of the already-filed #3902, which covers the *shade* half of the same
  primary↔secondary divergence in `rayHitAlbedo`; this is the *coverage* half,
  in a different function.
- **Description**: `ray_hit.glsl`'s own `getHitVertexAlpha` docstring states the
  invariant: *"secondary rays must reconstruct the same barycentric value or
  alpha-tested leaves/grates cast a different silhouette from the visible
  surface."* The raster path builds its alpha-test input in this order — sample
  diffuse, apply the BC1 punch-through pin, multiply by
  `mat.materialAlpha * fragColor.a`, then run the four `mat.decalMap0..3Index`
  overlays as an alpha-over composite (`texColor.a = decalSample.a +
  texColor.a * (1.0 - decalSample.a)`), and only then evaluate the
  `alphaTestFunc` comparison. `rayHitHasCoverage` reproduces every step of that
  chain **except** the decal composite: it applies the BC1 pin, multiplies by
  `mat.materialAlpha * getHitVertexAlpha(...)`, and calls `alphaComparePass`
  directly.

  Because the composite is alpha-over, the raster alpha is always **≥** the
  un-composited alpha wherever a decal slot is bound. The RT silhouette is
  therefore a strict subset of the raster silhouette: shadow, reflection,
  refraction, GI and water rays punch through texels that the visible surface
  covers.

  Second, smaller divergence in the same pair: the seven-arm comparison table
  is hand-written twice with a **different EQUAL/NOTEQUAL epsilon** —
  `abs(a - aThresh) < 0.004` in `triangle.frag` versus
  `abs(alpha - threshold) < (1.0 / 255.0)` (0.0039216) in `alphaComparePass`.
  `alphaComparePass` already exists as the shared helper; the raster path does
  not call it. Two copies that have already drifted once will drift again.
- **Evidence**:
  - `triangle.frag`: `uint materialDecals[4] = uint[4](mat.decalMap0Index, …);
    for (int decalIndex = 0; decalIndex < 4; ++decalIndex) { … texColor.a =
    decalSample.a + texColor.a * (1.0 - decalSample.a); }` — unconditional, no
    material-kind gate — followed by `if (aThresh > 0.0) { … if (!pass) discard; }`.
  - `ray_hit.glsl::rayHitHasCoverage`: `alpha *= mat.materialAlpha *
    getHitVertexAlpha(instanceIdx, primitiveIdx, barycentrics); if
    (!alphaComparePass(alpha, mat.alphaThreshold, mat.alphaTestFunc)) return false;`
    — `mat.decalMap*Index` appears nowhere in the file.
  - The role is live, not dead: `decal_map_0_index` is written into
    `GpuMaterial` and hashed into the dedup key
    (`h.write_u32(mat.decal_map_0_index)` in `material.rs`), and fed from
    `texture_indices.decals[0]` in `static_meshes.rs`.
- **Impact**: Wrong RT silhouettes — holes in shadows, reflections and GI that
  the visible surface does not have — for any alpha-tested mesh whose
  `NiTexturingProperty` decal slots are populated with a partially-transparent
  overlay. That is Oblivion/FO3/FNV-era authoring, where the decal slots are
  the legacy multi-layer path. **The affected population is uncensused**: no
  archive was mounted this run, so the count of materials with
  (`alphaThreshold > 0` ∧ a bound decal slot ∧ `decalSample.a < 1`) is unknown.
  The mechanism is certain; the blast radius is not. Rated MEDIUM per the
  severity decision tree's "visual artifacts only" row rather than escalated,
  precisely because the population is unmeasured.
- **Related**: #3902 (the shade half of the same primary↔secondary divergence);
  #3911 (supplemental role↔slot correspondence is unpinned, which is how a
  decal slot could also be mis-routed); #1653 / #ae285062 (the BC1 pin that
  *is* mirrored correctly in both paths).
- **Suggested Fix**: Census first, then fix — count decal-slot materials with an
  active alpha test across the FO3/FNV/Oblivion archives before changing the
  hit path, since the composite costs up to four extra bindless fetches per RT
  hit. If the population is non-trivial, fold the four-slot alpha-over composite
  into `rayHitHasCoverage` behind an early-out on
  `mat.decalMap0Index == 0u && …`. Independently and cheaply: replace
  `triangle.frag`'s inline seven-arm block with a call to `alphaComparePass` so
  the table has one definition and the epsilon cannot diverge again.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #3987 ===================
STATE: OPEN
LABELS: bug, renderer, medium, game:starfield
TITLE: REN-2026-09-06-D22-01: the Starfield "no evidenced Flags field" premise that zeroes both light canonicalizers is contradicted by the DAT2 decoder's own verified layout comment

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D22-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Light Animation
- **Location**: `byroredux/src/systems/light_anim.rs` (`canonical_light_animation_flags`, `canonical_light_shadow_flags`, `translate_light`) vs `crates/plugin/src/esm/cell/support.rs` (`build_static_object_from_subs`, `b"DAT2"` arm)
- **Status**: NEW
- **Description**: All three per-game boundary functions in `light_anim.rs` gate
  Starfield to a zero mask on one shared premise, stated verbatim in
  `canonical_light_animation_flags`'s doc: *"SF1Edit's live LIGH definition
  (`wbDefinitionsSF1.pas`) replaced the Skyrim/FO4/FO76 `DATA` subrecord with a
  restructured 76-byte `DAT2` whose only named fields are a handful of floats —
  the bytes a Flags field would occupy are an undifferentiated `wbUnknown`
  block."* `canonical_light_shadow_flags` and `translate_light` each restate it
  and cite the animation sibling as their authority.

  The `DAT2` decoder that actually produces the `flags` word says the opposite,
  citing the *same* reference file. `support.rs`'s arm carries an explicit,
  offset-by-offset layout table introduced as *"Byte layout verified against
  xEdit `wbDefinitionsSF1.pas` (`wbRecord(LIGH … wbStruct(DAT2, 'Data', [...]))`),
  NOT guessed"* — and its third row is `{12} UInt16 Flags (Skyrim DATA stores
  u32)`. The decoder then reads exactly that: `u16::from_le_bytes([sub.data[12],
  sub.data[13]]) as u32`.

  One of the two comments is wrong about what `wbDefinitionsSF1.pas` contains,
  and which one is right decides whether three `match` arms and five decoded
  fields are correct. (The two claims are only reconcilable if the *field* is
  named but its *bit meanings* are not — in which case `light_anim.rs`'s
  "undifferentiated `wbUnknown` block" phrasing is describing the wrong thing
  and should say so, because "no named field at all" is the argument the arms
  currently rest on.)
- **Evidence**:
  - `support.rs` (`b"DAT2" if is_ligh && sub.data.len() >= 11`) reads the flags
    word at offset 12 under a comment naming that offset `Flags`.
  - Downstream consequences of the zero masks, all on real decoded data:
    - `canonical_light_animation_flags` → `0` ⇒ `attach_light_flicker_if_needed`
      hits `if animation_flags == 0 { return; }`, so the three DAT2 flicker
      fields the same arm decodes — `period_secs` (+28), `intensity_amplitude`
      (+32), `movement_amplitude` (+36) — are structurally unreachable on
      Starfield.
    - `translate_light` → `is_spot` is `false` for every Starfield LIGH, so the
      function returns `LightKind::Point` before reading `fov_degrees`; the
      DAT2 FOV at +20 (decoded under `#2439 / NIFAL-D2-01 — same offset as the
      DATA arm above`) is likewise unreachable.
    - `canonical_light_shadow_flags` → `0` ⇒ `LightSource::from_legacy_world_units`
      computes `VisibilityMask::for_legacy_projection(false)` =
      `VisibilityMask::ARCHITECTURE` (`crates/core/src/lighting.rs`), so **every**
      placed Starfield light is invisible to `STATIC_PROP`, `DYNAMIC_ACTOR`,
      `FOLIAGE`, `GLASS` and `EFFECT` shadow rays.
  - That last consequence is the exact failure the shadow canonicalizer's own
    doc argues against: *"Shadow decode is permissive-by-default. Dropping a
    shadow bit that a game does name is the strictly worse error: the light
    silently stops casting RT shadows and the scene just looks flat, with
    nothing to trace it back to."* The Starfield arm applies the strict default
    to an entire game.
- **Impact**: Visual-only, but whole-game on Starfield: no flicker/pulse on any
  LIGH, no spot cones from ESM-placed lights, and props/actors/foliage cast no
  shadows from any placed light. Five decoded DAT2 fields are dead. The
  documentation conflict also means a future reviewer reading either comment
  gets an authoritative-sounding but contradicted answer.
- **Related**: #2251 (the arm's origin), `starfield_has_no_verified_flags_field_for_either_canonicalization`
  (the test that encodes the disputed premise), `crates/core/src/ecs/components/light.rs`
  (`LIGHT_FLAG_SHADOW_MASK`, `VisibilityMask::for_legacy_projection`).
- **Suggested Fix**: Settle the premise against `wbDefinitionsSF1.pas` once and
  make both comments agree. If the field is named but its bits are not, say
  exactly that in `light_anim.rs` and note the shadow-side consequence
  explicitly (all-`ARCHITECTURE` Starfield lights) so it is a recorded decision
  rather than a side effect. If the bit positions can be evidenced, give
  Starfield a real arm in both canonicalizers and let `translate_light` read the
  FOV it already decodes.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #3992 ===================
STATE: OPEN
LABELS: bug, renderer, medium, memory, water, test-gap
TITLE: REN-2026-09-06-D5-01: `screen_scaled_reservation_bytes` counts 3 of the 11 screen-scaled passes memory-budget.md ledgers — ~44 % of the bytes — so #3839's own worked example still holds after the fix

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM (inefficient / under-modelled GPU memory budgeting;
  no leak, no corruption)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` —
  `screen_scaled_reservation_bytes`, consumed by
  `AccelerationManager::recompute_blas_budget`
  (`acceleration/memory.rs`) via `blas_budget_for_heap`. Ground truth:
  `docs/engine/memory-budget.md` §"RT-Denoiser & Post-Process Screen-Sized
  Resources" and §"ReSTIR Reservoirs". Guard that cannot see it:
  `blas_budget_subtracts_the_resolution_scaled_reservation`
  (`acceleration/tests/predicates_tests.rs`).
- **Status**: **NEW.** Landed yesterday in `fa5c4191`; not examined by
  yesterday's scoped run (its Dim 5 was volumetrics-only). No matching open
  issue — searched `reservation`, `3839`, `blas budget`, `screen.scaled`,
  `memory`, `budget` across all 151 open titles. #3866 and #3842 are the
  *rename* / *orphaned-doc-comment* half of the same commit, not this.
- **Description**: The function's own docstring says it "Covers the three
  passes that scale with render resolution and **dominate the fixed floor**
  documented in `docs/engine/memory-budget.md`", and explicitly lists what it
  deliberately excludes: "textures, geometry pools and the swapchain". Every
  omission below is a resolution-scaled *pass*, i.e. inside the stated scope,
  and every one of them has its own row or subsection on the page the
  docstring cites:

  - **ReSTIR reservoirs**, 64 B/px — the single largest omission, and the
    page's own words for it are "the largest single VRAM addition of the
    denoiser overhaul … at 4K it is over 13 % of the ~4 GB engine budget
    target". `RESERVOIR_STRIDE` (`vulkan/restir.rs`) is a `pub` constant the
    function could read directly.
  - **G-buffer**, 44 B/px — `gbuffer.rs`'s seven attachments.
  - **TAA history**, 16 B/px (`taa.rs`).
  - **Water-side caustic accumulator**, 8 B/px. This one is the sharpest:
    the function already imports `CAUSTIC_BYTES_PER_PIXEL` from
    `vulkan/caustic.rs`, which is the **glass half only** (24 B/px,
    `4 * CAUSTIC_COLOR_LAYERS * MAX_FRAMES_IN_FLIGHT`). The doc row it comes
    from is headed "Glass + Water Caustics" and publishes 32 B/px combined.
    The water half has no production constant at all — `WATER_BYTES_PER_PIXEL`
    exists only as a `const` inside `caustic.rs`'s own test module, so the
    reservation cannot reach it even in principle.
  - **Bloom pyramid** (~5.3 B/px effective) and **SSAO** (2 B/px).
  - **FSR upscaler output**, 16 B/px at *output* extent — legitimately
    awkward, since this function is handed the render extent only.

- **Evidence**: The function body is three terms:
  `froxel_bytes + pixels*SVGF_BYTES_PER_PIXEL + pixels*CAUSTIC_BYTES_PER_PIXEL`.
  Re-derived against the doc's own per-row figures (table above): counted
  315.19 MB, omitted ~405 MB at 1080p — the same 43.8 % ratio at 4K, since
  everything including the froxel grid scales with pixel count.

  The arithmetic that *is* there is exactly right: `SVGF_BYTES_PER_PIXEL` = 40
  and `CAUSTIC_BYTES_PER_PIXEL` = 24 are already FIF-inclusive by
  construction, and `FROXEL_BYTES_PER_SLOT` = 44 is per-slot and correctly
  multiplied by `MAX_FRAMES_IN_FLIGHT`. There is no double-count. This is a
  coverage finding, not a math finding.

  The existing pin cannot catch it: it asserts only
  `reserved_hd > 128 MiB`, `reserved_uhd > 2 × reserved_hd`, and monotonicity
  of the resulting budget. All three pass with three terms, and all three
  would still pass with two.
- **Impact**: `blas_budget_for_heap` is `(heap − reserved) / 3`, so a missing
  reservation byte costs one third of a budget byte. The commit's own
  justification —
  *"on a 6 GB card at 1080p the old math handed BLAS 2 GB while ~1.1 GB was
  already committed elsewhere, so nothing evicted until the allocator
  failed"* — is the scenario the fix was written for, and after the fix that
  card gets **1 947.8 MiB** instead of 2 048.0 MiB. The described failure mode
  is essentially unchanged. A complete reservation roughly doubles the
  correction (to −228.9 MiB), which is still modest but is the difference
  between "eviction engages before the allocator does" and "it does not".
  Blast radius is confined to LRU eviction aggressiveness on small-VRAM cards
  and at high resolutions — the 12 GB dev card never reaches the gate.
- **Related**: #3839 (the fix this audits), #3866 / #3842 (sibling doc-rot
  from the same commit, already open), `REN-2026-09-06-D5-02` (the two passes
  the doc itself never ledgered, which is *why* they are also missing here).
- **Suggested Fix**: Add the missing render-extent terms, each from the
  owning pass's own published constant so the number moves when the pass
  does — the discipline the docstring already states. `RESERVOIR_STRIDE`
  (`restir.rs`) and `SVGF_BYTES_PER_PIXEL` are the model. Two of them need a
  constant published first: promote `WATER_BYTES_PER_PIXEL` out of
  `caustic.rs`'s test module, and add a `GBUFFER_BYTES_PER_PIXEL` beside
  `gbuffer.rs`'s attachment table (the doc already publishes 22 B/px × 2 FIF).
  Then strengthen the pin from a magnitude floor to an enumeration — assert
  the reservation equals the sum of the named per-pass constants, so a pass
  added without a term fails the test rather than shrinking the coverage
  ratio silently. If FSR's output-extent term is judged out of scope, say so
  in the docstring rather than leaving it in the "covers the passes that
  dominate" claim.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **FFI**: If the FFI boundary is touched, pointer lifetimes across it are sound
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #3993 ===================
STATE: OPEN
LABELS: documentation, renderer, medium, memory, doc-rot
TITLE: REN-2026-09-06-D5-02: the composite HDR pair and the depth / depth-history attachments — 40 B/px of unconditional render-extent VRAM — have no row anywhere in memory-budget.md

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM (authoritative-ledger gap on the page cited for the
  `< 4 GB` target; same class and severity as `REN-2026-08-30-D5-02`)
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md` — §"RT-Denoiser & Post-Process
  Screen-Sized Resources" (no subsection) and §"VRAM Rough Budget" (no row).
  Ground truth: `CompositePipeline` (`crates/renderer/src/vulkan/composite.rs`
  — the `hdr_images` and `scene_images` families and `HDR_FORMAT`), and
  `VulkanContext::depth_image` / `depth_history_image` allocated by
  `create_depth_resources` (`context/helpers.rs`) from `context/init.rs` and
  `context/resize.rs`.
- **Status**: **NEW.** `grep -in "hdr\|composite\|depth" docs/engine/memory-budget.md`
  returns no ledger row for any of them — the only hit that mentions them is
  the G-buffer roll-up row's own exclusion clause. Not in the 151 open
  issues; not in the 2026-08-30 or 2026-09-05 reports.
- **Description**: The VRAM roll-up's G-buffer row is scoped as
  "…22 B/px, × 2 FIF; **not** counting the separate HDR colour, depth, or
  depth-history attachments". Nothing else on the page counts them either, so
  the exclusion points at a row that does not exist.

  `CompositePipeline` owns **two** independent screen-sized image families,
  each `MAX_FRAMES_IN_FLIGHT` deep and each created at `extents.render`:
  `hdr_images` (the main HDR attachment) and `scene_images` (added by the FSR
  tail — "the single image that either FSR or the native bridge consumes";
  the field's own comment explains why the two must stay distinct). Both are
  `HDR_FORMAT = R16G16B16A16_SFLOAT`, 8 B/px, both `GpuOnly`, both
  unconditional. That is 4 images × 8 B/px = **32 B/px**.

  `depth_image` and `depth_history_image` add 4 B/px each at render extent
  (`find_depth_format` selects `D32_SFLOAT`; `depth_capture_record_copy`
  refuses anything else since #3570) — **8 B/px** more, single-buffered.

  Total **40 B/px** of unconditional render-extent VRAM with no row: 83.0 MB
  at 1080p, 331.8 MB at 4K native. For scale, that is more than the SVGF
  row (~83 MB) which has a whole subsection, and more than the TAA
  (~33 MB), Bloom (~11 MB) and SSAO (~4 MB) rows combined.
- **Evidence**:
  - `composite.rs`: `pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;`
    with the doc comment "RGBA16F = 8 bytes/pixel"; `pub hdr_images: Vec<vk::Image>`
    and `pub scene_images: Vec<vk::Image>`, both filled by
    `for i in 0..MAX_FRAMES_IN_FLIGHT` loops in `new_inner`, both
    `.extent(… width: extents.render.width, height: extents.render.height …)`,
    both `location: gpu_allocator::MemoryLocation::GpuOnly`.
  - `context/resize.rs` calls `create_depth_resources(…, self.frame_extents.render,
    self.depth_format, …)` twice — `"depth_buffer"` and `"depth_history"`.
  - `docs/engine/memory-budget.md`'s G-buffer row carries the exclusion text
    quoted above; no `### HDR`, `### Composite`, or `### Depth` heading exists
    (`grep -n "^### " docs/engine/memory-budget.md`).
- **Impact**: Doc-trust, but on the page every other subsystem's budgeting is
  derived from, and with a concrete downstream consumer: `REN-2026-09-06-D5-01`
  cannot reserve what the ledger does not name, so this omission propagates
  into the live BLAS-eviction threshold. The `Estimated total ~1.81 GB`
  roll-up understates by ~83 MB at 1080p and the 4K peak by ~332 MB.
  `scene_images` in particular is *newer* than the section around it — it
  arrived with the FSR tail, exactly the kind of growth the page's own
  §"Not yet ledgered" preamble ("a grep of this page for the owning subsystem
  name is the cheapest way to find a gap in it") is meant to surface.
- **Related**: `REN-2026-08-30-D5-02` (same class — a real allocation with no
  row; fixed for the staging pools), `REN-2026-09-06-D5-01`,
  `REN-2026-09-06-D5-06`.
- **Suggested Fix**: Add a `### Composite HDR intermediates + depth` subsection
  under "RT-Denoiser & Post-Process Screen-Sized Resources" with the 32 + 8
  B/px split, and a matching roll-up row; then delete the G-buffer row's
  exclusion clause or repoint it at the new section. Follow the
  `SVGF_BYTES_PER_PIXEL` / `CAUSTIC_BYTES_PER_PIXEL` / `FROXEL_BYTES_PER_SLOT`
  precedent — derive a `COMPOSITE_BYTES_PER_PIXEL` from `HDR_FORMAT` and the
  two families' arity and pin the doc against it, so the next image family
  added to `CompositePipeline` fails a test instead of drifting. Also worth
  a row while nearby: `ClusterCullPipeline`'s light-index buffers are
  `TOTAL_CLUSTERS (16×9×24) × MAX_LIGHTS_PER_CLUSTER (512) × 4 B ≈ 7.1 MB`
  per FIF — fixed-size, not resolution-scaled, and likewise unledgered.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

