# #4011 — REN-2026-09-06-D16-02: `screen_scaled_reservation_bytes` claims to cover the caustics pass but reserves only its glass half — the water accumulator has no published per-pixel constant at all

**Labels**: low, memory, renderer, tech-debt, water, bug

---

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
