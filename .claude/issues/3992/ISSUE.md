# #3992 — REN-2026-09-06-D5-01: `screen_scaled_reservation_bytes` counts 3 of the 11 screen-scaled passes memory-budget.md ledgers — ~44 % of the bytes — so #3839's own worked example still holds after the fix

**Labels**: medium, memory, renderer, water, test-gap, bug

---

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
