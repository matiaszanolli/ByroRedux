# #3565 — REN-2026-08-30-D5-01: three Scene-Buffers rows in `memory-budget.md` contradict test-pinned constants, `MAX_LIGHTS` by 2×

**Labels**: `medium,renderer,memory,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3565 --json state`.

---

- **Severity**: Medium
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md:30,35,37` vs
  `crates/renderer/src/shader_constants_data.rs:41-49` (`MAX_LIGHTS`),
  `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:9-18` (`GpuTerrainTile`),
  `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:67-79` (`GpuCamera`)
- **Status**: Open — the **doc** is wrong on all three; every code value is
  deliberate and compile- or test-pinned.
- **Description**: The ledger's Scene Buffers table is the page other sections
  and the VRAM roll-up derive from. Three of its eight rows no longer match code:
  1. `| Light SSBO | MAX_LIGHTS = 512 | 512 | 64 B | 32 KB | 64 KB |`. Commit
     `8e7582ed` (2026-08-16) redefined `MAX_LIGHTS` as
     `RESERVOIR_LIGHT_MASK as usize` = `(1 << 10) - 1` = **1023**, with
     `const _: () = { assert!(MAX_LIGHTS == 1023); }` beside it. Real footprint
     is 1023 × 64 B ≈ 64 KB/frame, **128 KB** across 2 FIF. The doc's `512` looks
     like it was transcribed from the neighbouring `MAX_LIGHTS_PER_CLUSTER = 512`.
  2. `| Terrain tile SSBO | … | 32 B | — | 32 KB |`. `GpuTerrainTile` is three
     `[u32; 8]` members = **96 B**, pinned by `gpu_terrain_tile_is_96_bytes` with
     the shader's `ArrayStride 96`, and `buffers.rs:480` sizes the buffer from
     `size_of::<GpuTerrainTile>()`. Real total ≈ **96 KB**, 3× the documented row.
  3. `| Camera UBO | — | 1 | 352 B | 352 B | 704 B |`. `GpuCamera` has been
     **368 B** since `#3323` added `exterior_sky_tint`, pinned by
     `gpu_camera_is_368_bytes`.
- **Evidence**:
  - `shader_constants_data.rs:41-47`: `RESERVOIR_LIGHT_BITS: u32 = 10;` …
    `pub const MAX_LIGHTS: usize = RESERVOIR_LIGHT_MASK as usize;` …
    `assert!(MAX_LIGHTS == 1023);`
  - `scene_buffer/constants.rs:15`: `pub(super) const MAX_LIGHTS: usize = crate::shader_constants::MAX_LIGHTS;`
  - `git log -S RESERVOIR_LIGHT_BITS` → `8e7582ed`, 2026-08-16; the doc row was
    last touched by `78540d8e`, 2026-06-02.
  - `gpu_instance_layout_tests.rs:300-307` / `:67-79` for the two struct sizes.
- **Impact**: Anyone budgeting against this page under-counts light-SSBO and
  terrain-tile residency by 2× and 3×. More consequentially, `MAX_LIGHTS` is the
  documented *overflow ceiling* — the number a reader uses to reason about the
  light-clamp path — and the page states half the real value under a constant name
  that resolves to something else in the same file.
- **Suggested Fix**: Update the three rows (1023 / 64 KB / 128 KB; 96 B / 96 KB;
  368 B / 736 B) and the "Total resident scene buffers" line. Prefer wording that
  names `MAX_LIGHTS = RESERVOIR_LIGHT_MASK` so the derivation, not a literal,
  is what the page records.
- **Dedup note**: NOT #3447 — that issue names `shader-pipeline.md` for the
  `GpuCamera` 352 B claim and `memory-budget.md` only for the *Instance SSBO*
  25 % understatement. The three rows above are separate sites and separate
  numbers; the `GpuCamera` row here is `memory-budget.md:37`, not
  `shader-pipeline.md`.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D5-01

## Dedup cross-reference

Adjacent to **#3447** (same defect class, different document rows) and **#3450**. This
issue is `memory-budget.md`'s Scene-Buffers table rows only — `MAX_LIGHTS`, the terrain
tile entry size, and the Camera UBO row. Fold into #3447's suggested automated
size-literal check if that is being worked.


## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
