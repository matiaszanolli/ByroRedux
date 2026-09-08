# #3994: REN-2026-09-06-D5-03: #3840's `resident_static_blas_bytes()` cannot change any behaviour — its only consumer is a trigger whose callee re-tests with the paper figure and returns

Labels: bug, renderer, medium, memory

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM (defence-in-depth gap: the machinery is carried, the
  hazard it names is not addressed)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` —
  `AccelerationManager::resident_static_blas_bytes`,
  `pending_destroy_static_bytes`, and the mid-batch trigger inside
  `build_blas_batched`; `AccelerationManager::evict_unused_blas` (same file);
  predicates `should_evict_mid_batch` and `blas_over_budget`
  (`acceleration/predicates.rs`); `BlasEntry.counted_in_static_bytes`
  (`acceleration/types.rs`). Guard: the
  `pending_destroy_static_bytes_stays_balanced_tests` module
  (`acceleration/tests/blas_static_tests.rs`).
- **Status**: **NEW.** Landed yesterday in `fa5c4191`. No matching open issue
  (searched `3840`, `resident`, `pending_destroy`, `eviction`, `budget`).
- **Description**: #3840's stated hazard, in
  `resident_static_blas_bytes`'s own doc comment, is that
  `static_blas_bytes` is "the *paper* figure — eviction credits it the moment
  an entry is queued … letting a batch **allocate against headroom that does
  not exist yet**", and that the resident figure "is the figure admission
  checks must use".

  There is exactly one production call site
  (`grep -rn "resident_static_blas_bytes"` → the definition, one call, one
  doc reference, one source-scan assertion). It is the 90 % **trigger**:

  `should_evict_mid_batch(self.resident_static_blas_bytes(), pending_bytes, budget)`
  → `(static + pending_destroy + pending) * 10 >= budget * 9`

  and the trigger's only action is `self.evict_unused_blas(device, allocator,
  pending_bytes)`, whose first statement is the 100 % **reclaim gate** on the
  *paper* figure:

  `if !blas_over_budget(self.static_blas_bytes, pending_bytes, budget) { return; }`
  → `(static + pending) > budget`

  Because `resident >= paper` always, the resident trigger is a strict
  superset of the paper trigger. In the band it newly covers
  (`static + pending <= budget < 0.9⁻¹ · (static + pending_destroy + pending)`)
  the callee's gate rejects and returns immediately. Outside that band both
  spellings agree. So swapping in the resident figure adds trigger firings
  that do nothing and changes no eviction decision — the batch continues
  allocating against the same headroom #3840 says does not exist.

  This is the shape the sibling comment fifteen lines below already warns
  about, for `pending_bytes`, in the #1792 write-up: *"the trigger above
  fired, but the callee it called was structurally blind to the very bytes
  that triggered it."*

  Note the paper-figure choice inside `evict_unused_blas` is **correct and
  deliberate** — its own #3840 comment explains that a resident-based loop
  condition would never improve, because each eviction moves the same bytes
  from `static_blas_bytes` into `pending_destroy_static_bytes`, so the loop
  would evict every candidate. The gap is that nothing else consumes the
  resident figure: there is no admission check anywhere that can stop or
  throttle a batch, and `tick_deferred_destroy` (the only thing that actually
  reduces residency) cannot run mid-batch — its sole caller is inside
  `draw_frame`.
- **Evidence**:
  - `predicates.rs`: `should_evict_mid_batch(total_live, pending, budget)` =
    `projected.saturating_mul(10) >= budget_bytes.saturating_mul(9)`;
    `blas_over_budget(static, pending, budget)` =
    `static_blas_bytes.saturating_add(pending_bytes) > budget_bytes`.
  - `blas_static.rs`: the only `resident_static_blas_bytes()` call is the
    first argument of the `should_evict_mid_batch` in `build_blas_batched`.
  - `evict_unused_blas`'s early-return and its per-candidate `break` both use
    `self.static_blas_bytes`.
  - The pin is a **source-scan**:
    `BLAS_STATIC_RS.contains("self.resident_static_blas_bytes(),")` — it
    asserts the call is textually present, so it passes whether or not the
    call can affect anything.
- **Impact**: No correctness harm and no leak — the bookkeeping itself is
  balanced and correct (verified: `drop_blas` and `evict_unused_blas` both
  credit `pending_destroy_static_bytes`; `tick_deferred_destroy` debits
  exactly the entries whose `counted_in_static_bytes` is set;
  `drain_pending_destroys` zeroes it; `blas_skinned.rs` pushes
  `counted_in_static_bytes: false` so skinned entries never touch the static
  counter). The cost is that a closed issue's hazard is still live: on a
  cell transition, the outgoing cell's BLAS bytes sit in
  `pending_destroy_static_bytes` until the next `draw_frame`, while
  `build_blas_batched` for the incoming cell allocates as if they were
  already freed. On the 12 GB dev card this is unreachable; on a 6 GB card at
  a large streaming boundary it is the allocator-failure path #3840 named.
  It also means three new struct fields, an accessor, and a guard test are
  carried for no behavioural effect, which is the kind of thing that reads as
  "already handled" to the next reader.
- **Related**: #3840 (the issue this closed), #1792 (the identical
  trigger-fires-callee-blind shape, for `pending_bytes`), #1449 (deferred
  eviction, which created the paper-vs-resident divergence),
  `REN-2026-09-05-D1-02` (a sibling "computed and thrown away" observation in
  the same subsystem, still unfiled).
- **Suggested Fix**: Decide which of the two this is meant to be and make the
  code say so. Either (a) give the resident figure a consumer that can act —
  the natural one is an admission check in `build_blas_batched`'s Phase 1
  loop that stops adding meshes to *this* batch when
  `resident + pending + next_size > budget`, deferring the remainder to the
  next batch after a tick has run; or (b) if throttling is not wanted, revert
  the trigger to `self.static_blas_bytes` and keep
  `resident_static_blas_bytes` purely as telemetry, documented as such. Either
  way, replace the source-scan pin with a behavioural one: a pure-function
  test over `should_evict_mid_batch` + `blas_over_budget` showing that the
  band where they disagree produces a different outcome. Today no test can
  fail if the resident call is deleted outright.

---

### LOW

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


---

# #4011: REN-2026-09-06-D16-02: `screen_scaled_reservation_bytes` claims to cover the caustics pass but reserves only its glass half — the water accumulator has no published per-pixel constant at all

Labels: bug, renderer, low, memory, tech-debt, water

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D16-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (VRAM-accounting accuracy; effect is a BLAS eviction threshold set ~1/3 of the omitted bytes too high, never a correctness or leak issue)
- **Dimension**: Volumetrics / Memory (the #3839 reservation)
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` — `screen_scaled_reservation_bytes` and its doc comment; consumed by `AccelerationManager::recompute_blas_budget` (`acceleration/memory.rs`) and `blas_budget_for_heap`. Constants: `CAUSTIC_BYTES_PER_PIXEL` (`crates/renderer/src/vulkan/caustic.rs`), and the **absent** counterpart in `crates/renderer/src/vulkan/water_caustic.rs`.
- **Status**: **NEW.** Distinct from **#3866** and **#3842**, which are doc-rot findings about the *old* `heap / 3` wording and the orphaned/renamed `compute_blas_budget` doc block — both filed 2026-09-05 and both verified still open and still accurate at HEAD (`memory-budget.md`'s Reserve-floors row and `MIN_BLAS_BUDGET_BYTES`'s docstring both still say `heap / 3`; **not re-filed here**). This finding is about the reservation's *arithmetic input set*, not its documentation.
- **Description**: `fa5c4191` (#3839) replaced the fixed `heap / 3` BLAS budget with `(heap − screen_scaled_reservation_bytes(render_extent, volumetrics)) / 3`, re-derived on every swapchain recreate. Its doc states the design rule: *"Derived from each pass's own published per-unit constant and the shared `froxel_extent` helper — never a hand-copied figure, so a pass that re-sizes itself moves this with it. Covers the three passes that scale with render resolution and dominate the fixed floor."*

  Two of the three constants hold up exactly. The caustics one does not, in a way the naming actively hides: `CAUSTIC_BYTES_PER_PIXEL = 4 * CAUSTIC_COLOR_LAYERS * MAX_FRAMES_IN_FLIGHT` = **24 B/px**, and its own docstring scopes it to *"bytes per pixel **this pipeline** allocates"* — the glass accumulator only. The water-side accumulator (`WaterCausticAccum`, a second full-render-extent `R32_UINT` image per FIF, `.array_layers(1)` → **8 B/px**) is a fourth screen-scaled resident allocation and contributes nothing to the reservation. `memory-budget.md` publishes the pair as **32 B/px combined**, and `caustic.rs`'s own `caustic_bytes_per_pixel_matches_documented_memory_budget` asserts `CAUSTIC_BYTES_PER_PIXEL + WATER_BYTES_PER_PIXEL == 32` — but `WATER_BYTES_PER_PIXEL` is declared **inside that test function**, so the value the reservation would need does not exist outside it.

  TAA (16 B/px), SSAO (2 B/px) and bloom (~5.3 B/px) are likewise absent. Those are defensible under the fn's "deliberately a floor, not a complete VRAM census" caveat; the caustics half is not, because the fn reads a constant that *looks* like it covers the pass and does not.
- **Evidence**:
  ```rust
  // predicates.rs — screen_scaled_reservation_bytes
  froxel_bytes
      .saturating_add(pixels.saturating_mul(u64::from(SVGF_BYTES_PER_PIXEL)))
      .saturating_add(pixels.saturating_mul(u64::from(CAUSTIC_BYTES_PER_PIXEL)))
  ```
  ```
  $ grep -rn "WATER_BYTES_PER_PIXEL" crates/renderer/src
  crates/renderer/src/vulkan/caustic.rs:1431:  const WATER_BYTES_PER_PIXEL: u32 = 4 * super::MAX_FRAMES_IN_FLIGHT as u32;   // test-local
  ```
  Independently recomputed against the live allocators: froxel `FROXEL_BYTES_PER_SLOT = 44` ✓ (`5×RGBA16F + 1×R32F`, and `assert_eq!(FROXEL_BYTES_PER_SLOT, 5 * 8 + 4)` holds); `SVGF_BYTES_PER_PIXEL = 40` ✓ (`4 + 8 + 4 + 4`, ×2 FIF); `CAUSTIC_BYTES_PER_PIXEL = 24` ✓ **for `caustic.rs` alone**.
- **Impact**: The reservation under-counts by 8 B/px (water caustics) — ~16.6 MB at 1080p render extent, ~66 MB at native 4K — plus ~23 B/px of TAA/SSAO/bloom. After the `/3`, the BLAS budget lands ~7 MB too high at 1080p and ~27 MB too high at native 4K from the water half alone (~21 MB / ~85 MB including the other three). Against `MIN_BLAS_BUDGET_BYTES = 256 MB` and a 12 GB dev card this is noise; on the 6 GB minimum-spec card the issue's own worked example targets, eviction fires that much later than intended. No correctness or leak consequence either way.
- **Related**: #3839 (the reservation), #2679 / PERF-D3-03 (which established `CAUSTIC_BYTES_PER_PIXEL` and its doc pin — and made the glass/water split explicit), #3866 / #3842 (the stale `heap / 3` doc sites — already filed, not re-reported).
- **Suggested Fix**: Promote the test-local `WATER_BYTES_PER_PIXEL` into a published `pub(crate) const WATER_CAUSTIC_BYTES_PER_PIXEL` on `water_caustic.rs` (derived from `CAUSTIC_FORMAT`'s 4 B and `MAX_FRAMES_IN_FLIGHT`, mirroring how `caustic.rs` derives its own), have `caustic_bytes_per_pixel_matches_documented_memory_budget` import it instead of redeclaring it, and add it to `screen_scaled_reservation_bytes`. If TAA/SSAO/bloom are to stay out, reword the doc from *"Covers the three passes that scale with render resolution"* to name exactly what is in and what is deliberately out — the current wording is what makes the caustics half look complete.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


---

# #4012: REN-2026-09-06-D17-03: `disneyDiffuseSplit`'s doc block still describes two call sites; the fallback-directional arm it names was deleted

Labels: documentation, renderer, low, tech-debt, shaders, doc-rot

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D17-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` (the `disneyDiffuseSplit` doc block)
- **Status**: NEW
- **Description**: The split-return rationale reads "The two call sites (fallback-directional and per-light loop) need to compose them with different PI scales because the per-light loop carries a `kD * albedo` (no /PI) legacy convention." There is now exactly **one** call site. `lighting.glsl`'s own else-arm comment records the change ("Directional, point and spot sources all arrive through this function; `triangle.frag` no longer carries a duplicate synthetic no-light sun/BRDF arm"), and `disney_sheen_keeps_its_relative_weight_in_canonical_direct_path` actively asserts the second site's *absence* (`!frag.contains("diffuseBrdf = (dd.diffuse + dd.sheen)")`). The struct return is still the right shape — `disneyDiffuseSplit`'s consumer does need diffuse and sheen separable, and the doc is the only place the π-scaling contract is written down — so the fix is to restate the rationale against the surviving single site, not to collapse the struct.
- **Evidence**: `grep -rn "disneyDiffuseSplit(" crates/renderer/shaders/` returns one call (`lighting.glsl`) plus the definition.
- **Impact**: Documentation only. It misleads a reader into thinking a second composition convention exists that must be kept in step — which is how the `* PI` vs no-`* PI` asymmetry #2243 fixed got introduced in the first place. The `/audit-renderer` skill's Dimension 17 text has already absorbed the stale claim verbatim ("the normalized direct-sun path (`triangle.frag`, which keeps `(dd.diffuse + dd.sheen) * (1.0 - metalness)`)"), so it is propagating.
- **Related**: #1252 (the split return), #2243 (the π-scaling fix), #3868 (`triangle.frag`'s own present-tense doc rot). Adjacent instance worth folding into the same edit: `triangle.frag`'s RT-glass block still says "Glass has IOR ≈ 1.5 (soda-lime, window glass, drinking glass)" thirty lines after the correct note that "Canonical glass carries `mat.ior = 1.45` (`GLASS_SURFACE_BEHAVIOR`)".
- **Suggested Fix**: Rewrite the paragraph to name the single surviving call site and state why the struct is still the right return shape (the caller must apply `* PI` to the *sum*, so the two lobes must arrive separable). Update the skill's Dimension 17 bullet in the same pass.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix


---

# #4013: REN-2026-09-06-D17-04: both #2243/#2244 regression guards named in the Dimension 17 checklist point at the wrong test file

Labels: documentation, renderer, low, tech-debt, shaders, doc-rot, test-gap

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D17-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 17 checklist)
- **Status**: NEW
- **Description**: The checklist cites `disney_sheen_keeps_its_relative_weight_in_canonical_direct_path` and `bounded_path_converts_dalc_irradiance_to_environment_radiance` as living in `gpu_instance_layout_tests.rs`. Both are in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`. The path gate cannot catch this: `gpu_instance_layout_tests.rs` **does** exist, so the backtick resolves; only the symbol→file association is wrong. `.claude/issues/2472/ISSUE.md` carries the same wrong anchor with a line number attached (`gpu_instance_layout_tests.rs:1148`).
- **Evidence**: `grep -rn "disney_sheen_keeps_its_relative_weight_in_canonical_direct_path\|bounded_path_converts_dalc_irradiance_to_environment_radiance" .` returns only `shader_contract_tests.rs` (plus the skill, the prior audit report, and the issue file).
- **Impact**: An auditor working the Dimension 17 checklist opens the named file, does not find the guard, and must either conclude the guard was deleted (a false regression report) or re-derive the location. This is the exact TD7-* stale-anchor class `_audit-common.md`'s path-reference convention exists to prevent, in its one form the validate script's advisory cannot flag.
- **Related**: `.claude/commands/_audit-common.md` "Path-Reference Convention (post-#1114)", `.claude/commands/_audit-validate.sh`, #3047 (the same drift in the shader-includes list).
- **Suggested Fix**: Repoint both citations to `shader_contract_tests.rs` in `SKILL.md`, and drop the line number from `.claude/issues/2472/ISSUE.md`. Consider extending `_audit-validate.sh`'s backticked-symbol advisory to also check that a symbol named in the same sentence as a `*_tests.rs` path is actually defined in that file.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


---

