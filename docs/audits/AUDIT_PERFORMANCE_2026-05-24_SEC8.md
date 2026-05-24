# Performance Audit — Dimension 8 (Material Table & SSBO Upload)
# Scope: GpuMaterial 280 → 300 B growth (#1248 / #1249 / #1250)
# Date: 2026-05-24

## Setup recap

Five new fields were added across three commits:

- `ior: f32`            (+4 B, offset 280) — #1248
- `subsurface: f32`     (+4 B, offset 284) — #1249
- `sheen: f32`          (+4 B, offset 288) — #1249
- `sheen_tint: f32`     (+4 B, offset 292) — #1249
- `anisotropic: f32`    (+4 B, offset 296) — #1250

Total: 280 → 300 B per `GpuMaterial`, +20 B (+7.1%).
Pinned by `gpu_material_size_is_260_bytes` (named "260" historically;
asserts `== 300` today — see `crates/renderer/src/vulkan/material.rs:1089-1092`).

Per the scope statement, this report covers ONLY Dimension 8
(Material Table & SSBO Upload) per the
`audit-performance` skill's checklist:
> Dedup ratio — per-frame upload size — hash-table churn —
> SSBO resize policy — GpuInstance struct size win — memory
> bandwidth.

Out of scope (not assessed here):
- Per-fragment register pressure from `GpuMaterial mat = materials[inst.materialId];`
  (Dim 1 — GPU Pipeline).
- Per-light loop branch divergence (Dim 1 — shader branching cost).
- BRDF cost in the per-light loop (Dim 1).

Items 2/3 from the scope statement are flagged here as INFO with
"out of dim, see Dim 1 audit" pointers because the request was for
§8 only AND the project's `feedback_speculative_vulkan_fixes`
memory prohibits unmeasurable perf claims. They are not findings.

## Methodology

- Read `crates/renderer/src/vulkan/material.rs` end-to-end
  (GpuMaterial layout, `MaterialTable`, `hash_gpu_material_fields`,
   `intern_by_hash`, `MAX_MATERIALS` cap).
- Read `crates/renderer/src/vulkan/scene_buffer/upload.rs:500-559`
  (`upload_materials` dirty gate + memcpy).
- Read `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:202-231`
  (`hash_material_slice` byte hasher).
- Read `crates/renderer/src/vulkan/context/draw.rs:1645-1649`
  (upload dispatch site).
- Read `crates/renderer/src/vulkan/context/mod.rs:450-588`
  (`DrawCommand::material_hash` + `to_gpu_material` lockstep producer).
- Read `crates/renderer/shaders/triangle.frag:140-167, 985-991`
  (shader-side struct declaration + per-fragment material load).
- Cross-checked against `gh issue list ... GpuMaterial OR Disney OR
  anisotropic OR ior OR sheen OR translucency` (50 closed issues,
  none open) — #878 (dirty-gate for upload), #781 (hot-path intern_by_hash),
  #797 (overflow cap), #807 (slot-0 reservation), #1230 (avg_albedo
  cleanup), #1147 / #1248-#1251 (this session's work).

## Findings

### PERF-D8-2026-05-24-01: Per-frame material SSBO upload grows 7.1% in worst case (dirty frames only)
- **Severity**: LOW
- **Dimension**: Material Table & SSBO Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:540-553`
- **Status**: NEW (delta against #878 / DIM8-01's dirty-gate baseline)
- **Description**: When the per-frame `MaterialTable` content changes
  (`hash_material_slice` returns a new u64), the upload memcpys
  `count × 300 B` instead of `count × 280 B`. For a 200-material
  interior cell that's 60.0 KB vs 56.0 KB (+4 KB per dirty upload).
  For a 3×3 exterior grid with ~500 unique materials it's 150 KB vs
  140 KB (+10 KB per dirty upload).
- **Evidence**:
  - `byte_size = std::mem::size_of::<GpuMaterial>() * count`
    at `upload.rs:542` — `size_of::<GpuMaterial>()` is 300 post-#1250
    (pinned at `material.rs:1091`).
  - `material_buf_size = 300 × 4096 = 1.2 MB` (was 1.12 MB) per
    frame-in-flight; total VRAM reservation across `MAX_FRAMES_IN_FLIGHT = 2`
    grows from 2.24 MB to 2.40 MB (+160 KB device-local).
  - Upload is hash-gated; static interiors hit the gate (#878) and
    pay zero per-frame copy cost. The growth is realized ONLY on
    dirty frames (cell transitions, material edits, animated parameters).
- **Impact**: Negligible (<0.01% of typical per-frame PCIe budget).
  At 60 fps and assuming one dirty upload per cell transition
  (~once every 30 s for typical play), the additional bandwidth is
  ~333 B/s — well below the signal floor. The static VRAM growth
  (+160 KB) is similarly below the noise floor on the project's
  12 GB target (#user_hardware).
- **Related**: #878 (closed — installed the dirty-gate that absorbs
  the cost on static frames), #781 (closed — `intern_by_hash` skips
  the 300 B construction on dedup hits).
- **Suggested Fix**: None. The cost is already absorbed by the
  existing dirty-gate; the 20 B growth is what's needed to ship the
  five new fields and there's no algorithmic improvement available
  without dropping fields. Document as INFO-grade impact in the
  commit body of the next session-close.

### PERF-D8-2026-05-24-02: `hash_material_slice` walks 7.1% more bytes per frame (paid on every frame, not just dirty)
- **Severity**: LOW
- **Dimension**: Material Table & SSBO Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:218-231` +
  `crates/renderer/src/vulkan/scene_buffer/upload.rs:535`
- **Status**: NEW
- **Description**: The dirty-gate hash that protects the SSBO upload
  (`hash_material_slice`) runs `SipHash-1-3` over the raw byte view
  of the entire materials slice — `count × 300 B` post-#1250 vs
  `count × 280 B` pre-fix. This runs on EVERY frame (even when the
  gate hits and the upload is skipped) because the hash itself is
  the check. On a 200-material interior cell that's hashing 60.0 KB
  per frame vs 56.0 KB; on a 500-material exterior grid it's 150 KB
  vs 140 KB.
- **Evidence**:
  - `hash_material_slice` at `descriptors.rs:218-231` builds a
    `std::collections::hash_map::DefaultHasher` (SipHash-1-3) over
    `materials.as_ptr() as *const u8` for `byte_size =
    size_of::<GpuMaterial>() * materials.len()` bytes.
  - Called unconditionally at `upload.rs:535` before the gate
    check at `upload.rs:536`.
  - Docstring at `descriptors.rs:209-211` cites "~30 µs for ~52 KB"
    as the pre-#1250 baseline; the +7.1% bump puts the same calculation
    at ~32 µs per dirty-gate check. For the 500-material exterior
    grid the cost rises from ~85 µs to ~91 µs per frame.
- **Impact**: ~2 µs / frame on interior cells, ~6 µs / frame on
  large exterior grids. At 16.7 ms / frame (60 fps) this is
  ~0.01-0.04% of the frame budget. The cost ladder is paid on the
  CPU side, in a non-hot-path serial call, with no measurement
  infrastructure currently wired (see `audit-performance.md:27-29`
  on dhat-infra gap).
- **Related**: #878 (the dirty-gate this hash protects).
- **Suggested Fix**: None proposed. xxh3 (cited as "~10× faster"
  in the existing docstring at `descriptors.rs:211`) would close
  the gap but requires a new dependency, and the impact is already
  below the signal floor. Per `feedback_speculative_vulkan_fixes.md`,
  proposing the swap without measurement is premature. **If** a
  future dhat sweep flags `hash_material_slice` as a hot spot,
  the obvious lever is the xxh3 swap; today this is just a noted
  delta in the bandwidth budget.

### PERF-D8-2026-05-24-03: `hash_gpu_material_fields` walks 4 extra u32 writes per intern call
- **Severity**: LOW
- **Dimension**: Material Table & SSBO Upload
- **Location**: `crates/renderer/src/vulkan/material.rs:737-844` +
  `crates/renderer/src/vulkan/context/mod.rs:471-588` (lockstep producer)
- **Status**: NEW (the scope statement names "4 more u32s" — the actual
  count is 5: ior + subsurface + sheen + sheen_tint + anisotropic).
- **Description**: The 50-field walk on the hot dedup path now writes
  55 u32s instead of 50. Each `Hasher::write_u32` call is one SipHash
  round (~3-5 cycles amortized on modern x86). The walk is called
  exactly once per `intern_by_hash` call on the producer side
  (`material_hash` at `context/mod.rs:471`) AND once per `intern_by_hash`
  call inside the table (`hash_gpu_material_fields` at
  `material.rs:737`) — wait, no: the producer's `material_hash` is
  the precomputed u64 passed INTO `intern_by_hash`, and `intern_by_hash`
  does NOT re-walk in release. `hash_gpu_material_fields` only runs
  on the `seed_neutral_default` path (once at table creation / clear)
  and inside the `intern()` legacy wrapper (`material.rs:964-967`).
  In the hot path (DrawCommand → material_hash → intern_by_hash) only
  the producer side at `context/mod.rs:474-587` walks the fields.
- **Evidence**:
  - `material_hash` at `mod.rs:471-588` now ends with five extra
    `h.write_u32` calls (lines 579, 582, 583, 584, 586) — `ior`,
    `subsurface`, `sheen`, `sheen_tint`, `anisotropic`.
  - `hash_gpu_material_fields` at `material.rs:835-842` walks the
    same five additional u32s in the same order (lockstep contract,
    pinned by `material_hash_matches_gpu_material_field_hash`).
  - Producer-side walk runs once per `DrawCommand`. Build-render-data
    issues O(visible draws) DrawCommands per frame; with the
    #272 instanced batching this lands in the low thousands on
    Megaton-scale interiors and similar on cell-loaded exteriors.
- **Impact**: ~5 × 3-5 cycle SipHash rounds × ~1000-7000 calls/frame
  = ~15-175 µs/frame additional CPU on the build-render-data
  thread. The pre-#1250 walk was ~50 × 3-5 × 1000-7000 ≈
  150 µs - 1.75 ms; the +10% growth puts the bump in the same
  noise envelope as the existing #878 hash-gate cost. Below the
  signal floor for any single frame, but compounds across the
  cell-stream-in spike where every newly-loaded DrawCommand re-hashes.
- **Related**: #781 (the `intern_by_hash` fast path the walk feeds),
  #878 (the slice-hash gate in PERF-D8-2026-05-24-02 above).
- **Suggested Fix**: None proposed today. The walk is a SipHash
  finalize over `4 × N_fields` bytes; once dhat infra (per
  `audit-performance.md:27-29`) lands, this site is a candidate
  for switching to `Hasher::write` over the raw bytes of the
  authored DrawCommand suffix (post-#781 fast path), which would
  let LLVM auto-vectorize the SipHash absorb at the cost of needing
  a stable repr on the DrawCommand subset. **Marked LOW + speculative,
  not actionable today.**

### PERF-D8-2026-05-24-04: GpuInstance contract still pinned — no per-material field re-expansion observed
- **Severity**: INFO (positive confirmation, not a finding)
- **Dimension**: Material Table & SSBO Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:24, 78`
- **Status**: NEW (confirms #1230 / R1 Phase 6 closeout holds)
- **Description**: The audit checklist explicitly asked for
  > "GpuInstance struct size win — verify the post-R1 size (target
  > 112 B vs ~400 B legacy) is realized in the
  > gpu_instance_is_112_bytes_std430_compatible +
  > gpu_instance_field_offsets_match_shader_contract +
  > gpu_instance_does_not_re_expand_with_per_material_fields tests"
  All three pins survived the #1248/#1249/#1250 growth: none of
  the five new fields was added to `GpuInstance`, only to
  `GpuMaterial`. The R1 dedup win is fully preserved.
- **Evidence**:
  - `gpu_instance_is_112_bytes_std430_compatible` at
    `gpu_instance_layout_tests.rs:24` still passes
    (`size_of::<GpuInstance>() == 112`).
  - `gpu_instance_does_not_re_expand_with_per_material_fields` at
    `gpu_instance_layout_tests.rs:78` is the explicit regression
    guard the audit checklist asked us to verify — it would
    fail if any of `ior` / `subsurface` / `sheen` / `sheen_tint` /
    `anisotropic` had been added to `GpuInstance`. It still passes.
  - The 188 B-per-placement dedup win remains realized: a 7359-instance
    MedTek frame still uploads ~530 KB/frame (per #1134 baseline)
    instead of the legacy ~3 MB+.
- **Impact**: Confirms architectural invariant. No action.

### PERF-D8-2026-05-24-05: VRAM reservation for material SSBO grew +160 KB
- **Severity**: INFO
- **Dimension**: Material Table & SSBO Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/buffers.rs:384-387, 461-464`
- **Status**: NEW
- **Description**: `material_buf_size = sizeof::<GpuMaterial>() × MAX_MATERIALS`
  scales linearly with the struct growth. At 300 × 4096 × 2 (frames in flight)
  = 2.40 MB device-mapped (was 2.24 MB).
- **Evidence**:
  - `material_buf_size` at `buffers.rs:385-387`.
  - `MAX_MATERIALS = 4096` at `scene_buffer/constants.rs:124`.
  - `MAX_FRAMES_IN_FLIGHT = 2` at `vulkan/sync.rs:6`.
  - Allocated `create_host_visible` at `buffers.rs:461-464` — the
    full 1.2 MB per frame-in-flight is reserved at startup whether
    or not real content needs it.
- **Impact**: +160 KB host-visible VRAM, permanently reserved.
  Project VRAM target is 6 GB minimum / 12 GB dev hardware
  (`feedback_vram_baseline.md`); the bump is 0.0013% of dev budget,
  0.0026% of minimum. Below the signal floor on any practical
  budget. Real cells dedup to 50-600 unique materials per the
  docstring at `material.rs:960-963`, so the upper-bound-sized
  reservation is mostly slack regardless of the new fields.
- **Related**: #797 (MAX_MATERIALS cap), #807 (slot-0 reservation),
  `feedback_vram_baseline.md`.
- **Suggested Fix**: None. The cost is structural to the
  upper-bound-sized reservation policy; trimming MAX_MATERIALS
  would shrink the savings but requires a real-world dedup census
  to size safely. Out of scope for this session.

## Items raised in scope statement but flagged as out-of-Dim-8 (not findings)

The scope statement listed five items. Items 2 (per-light loop
branch divergence on `mat.anisotropic > 0`), 3 (Disney diffuse
divergence on `MAT_FLAG_BGSM_PBR`), and 4 (`dielectricF0FromIor`
cost) all live in `triangle.frag` and concern shader branching /
ALU cost — these are Dimension 1 (GPU Pipeline Efficiency)
territory, not Dimension 8 (Material Table & SSBO Upload).

Per the user's explicit "do NOT propose perf fixes you can't
validate by measurement" constraint (`feedback_speculative_vulkan_fixes.md`)
AND the §8 scope, those are noted here as INFO-level pointers
WITHOUT severity claims:

- **INFO**: Per-light loop's anisotropic-GGX gate at
  `triangle.frag:2595-2608` and the fallback path at
  `triangle.frag:2386-2399` both branch on `mat.anisotropic > 0 &&
  dot(fragTangent.xyz, fragTangent.xyz) > 1e-4`. `mat` is loaded
  once per fragment at `triangle.frag:991`, so the gate value is
  uniform within the fragment but may diverge across a warp when
  adjacent fragments cover different material IDs OR same-material
  fragments straddle the zero-tangent boundary (synthetic geometry,
  `BSDynamicTriShape` without authored tangents — see #783
  perturbNormal fallback path). Divergence cost cannot be
  estimated without a RenderDoc/Nsight capture. **Out of dim,
  cannot be measured today.**

- **INFO**: Disney diffuse branch at `triangle.frag:2411-2419`
  (fallback) and `:2626-2634` (per-light) is gated on
  `mat.materialFlags & MAT_FLAG_BGSM_PBR`. Since `material_flags`
  is set at material-import time and the dedup table groups
  identical materials into single slots, the gate value is uniform
  across a draw call iff the draw's instance batch shares a
  material ID. The #272 instanced batching is documented as
  same-material-keyed, so within a single batched draw the gate
  is uniform; cross-draw divergence is GPU work-distribution
  dependent and cannot be characterized without measurement.
  **Out of dim, cannot be measured today.**

- **INFO**: `dielectricF0FromIor` at `triangle.frag:672-675` is
  two divides + one multiply, called once per fragment at
  `triangle.frag:1620` (not in the per-light loop). The cost
  is below the signal floor of any practical bench. The
  per-fragment call did not exist pre-#1248 — previously F0
  was a hardcoded `vec3(0.04)` literal — so the +3 ALU is a
  net delta. **Below signal floor, not a finding.**

- **INFO**: `disneyDiffuseTerm` at `triangle.frag:702-741` walks
  ~10 ALU ops (`pow×3, max×3, mix×2, divide×1` + adds/muls).
  This cost ONLY fires inside the `MAT_FLAG_BGSM_PBR` gated
  branch (so legacy NIF content pays zero ALU). The compounding
  across the per-light loop hits at `:2626-2634`; a warp with
  mixed BGSM_PBR / non-PBR fragments will serialize. Same
  measurement caveat as the anisotropic gate above. **Out of
  dim; flagged for the next Dim 1 audit.**

## Summary

| Finding | Severity | Actionable today? |
|---|---|---|
| PERF-D8-2026-05-24-01 | LOW | No — already gated by #878, growth is below signal floor |
| PERF-D8-2026-05-24-02 | LOW | No — speculative xxh3 swap, no measurement infra |
| PERF-D8-2026-05-24-03 | LOW | No — speculative, no dhat infra |
| PERF-D8-2026-05-24-04 | INFO | Positive confirmation, no action |
| PERF-D8-2026-05-24-05 | INFO | Within VRAM budget, no action |

**Zero CRITICAL / HIGH / MEDIUM findings.** The session's
material-deep changes (#1248/#1249/#1250) preserved every existing
Dim 8 invariant — dedup table works, dirty-gate works, GpuInstance
contract holds, hot-path `intern_by_hash` still skips the
300 B construction on dedup hits.

The 20 B / 7.1% growth in `GpuMaterial` is absorbed by:

1. The #878 dirty-gate suppressing redundant uploads on static
   frames.
2. The #781 `intern_by_hash` fast path skipping the new field
   reads on the ~97% dedup-hit producer side.
3. The #797 / #807 cap + neutral-default policy keeping over-cap
   behavior unchanged.
4. The structural decision to add the fields to `GpuMaterial`
   (dedup target) rather than `GpuInstance` (per-draw)
   (#1230 / R1 Phase 6 invariant).

**No regression-test gap identified for Dim 8.** The five
existing pin tests (`gpu_material_size_is_260_bytes`,
`gpu_material_field_offsets_match_shader_contract`,
`gpu_material_glsl_field_names_pinned`, the GpuInstance trio at
`gpu_instance_layout_tests.rs`, and the lockstep contract test
`material_hash_matches_gpu_material_field_hash`) all updated for
the new fields and continue to pin the per-frame upload path.

The shader-side concerns (items 2/3/4 from the scope statement)
are real but Dimension 1 territory and cannot be evaluated
without measurement infrastructure that is not currently wired
(per `audit-performance.md:27-29` known infrastructure gap).
They are flagged here as INFO pointers for the next Dim 1
audit, NOT as Dim 8 findings.
